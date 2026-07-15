use super::{MemoryService, now_seconds};
use crate::{MemorySettings, normalized_string};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

#[derive(Debug, Clone)]
pub(crate) struct MemorySourceSnapshot {
    pub(crate) source_key: String,
    pub(crate) line_count: i64,
    pub(crate) byte_len: i64,
    pub(crate) modified_at: f64,
    pub(crate) prefix_hash: String,
    pub(crate) prefix_lines: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemorySourceGate {
    Allow,
    LowSignal,
    InsufficientGrowth,
}

impl MemorySourceGate {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Allow => "enqueued",
            Self::LowSignal => "low-signal",
            Self::InsufficientGrowth => "insufficient-growth",
        }
    }
}

impl MemoryService {
    pub(crate) fn recent_completed_extraction_within(
        &self,
        project_id: &str,
        tool: &str,
        session_id: &str,
        cooldown_seconds: i64,
    ) -> Result<bool, String> {
        if cooldown_seconds <= 0 {
            return Ok(false);
        }
        let conn = self.open_or_create_connection()?;
        let latest: Option<f64> = conn
            .query_row(
                r#"
                SELECT MAX(enqueued_at)
                FROM memory_extraction_queue
                WHERE project_id = ?1
                  AND tool = ?2
                  AND session_id = ?3
                  AND status = 'done';
                "#,
                params![project_id, tool, session_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let Some(latest) = latest else {
            return Ok(false);
        };
        Ok(now_seconds() - latest < cooldown_seconds as f64)
    }

    pub(crate) fn source_snapshot(
        &self,
        tool: &str,
        session_id: &str,
        transcript_path: &str,
        transcript: &str,
    ) -> MemorySourceSnapshot {
        let (byte_len, modified_at) = fs::metadata(transcript_path)
            .ok()
            .map(|metadata| {
                let modified_at = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|time| time.as_secs_f64())
                    .unwrap_or(0.0);
                (metadata.len() as i64, modified_at)
            })
            .unwrap_or_else(|| (transcript.len() as i64, 0.0));
        let canonical_path =
            codux_runtime_core::path::normalize_local_path(Path::new(transcript_path));
        let (line_count, prefix_hash, prefix_lines) =
            transcript_file_stats(transcript_path).unwrap_or_else(|| {
                (
                    transcript.lines().count() as i64,
                    prefix_hash(transcript),
                    transcript.lines().take(PREFIX_HASH_LINES).count() as i64,
                )
            });
        MemorySourceSnapshot {
            source_key: sha256_hex(&format!(
                "{}|{}|{}",
                tool.trim().to_lowercase(),
                session_id.trim(),
                canonical_path
            )),
            line_count,
            byte_len,
            modified_at,
            prefix_hash,
            prefix_lines,
        }
    }

    pub(crate) fn update_source_state(
        &self,
        snapshot: &MemorySourceSnapshot,
    ) -> Result<(), String> {
        let conn = self.open_or_create_connection()?;
        conn.execute(
            r#"
            INSERT INTO memory_extraction_source_state (
                source_key, last_seen_lines, last_seen_len, last_seen_mtime, prefix_hash, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(source_key) DO UPDATE SET
                last_seen_lines = excluded.last_seen_lines,
                last_seen_len = excluded.last_seen_len,
                last_seen_mtime = excluded.last_seen_mtime,
                prefix_hash = excluded.prefix_hash,
                updated_at = excluded.updated_at;
            "#,
            params![
                snapshot.source_key,
                snapshot.line_count,
                snapshot.byte_len,
                snapshot.modified_at,
                snapshot.prefix_hash,
                now_seconds()
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn source_gate(
        &self,
        settings: &MemorySettings,
        snapshot: &MemorySourceSnapshot,
        transcript: &str,
    ) -> Result<MemorySourceGate, String> {
        let has_signal = memory_text_has_signal(transcript);
        let has_auto_trigger = memory_text_has_auto_trigger_marker(transcript);
        if settings.extraction_heuristic_gate_enabled && !has_signal {
            return Ok(MemorySourceGate::LowSignal);
        }
        let threshold = settings.extraction_growth_threshold_lines.max(0) as i64;
        if threshold <= 0 || has_auto_trigger {
            return Ok(MemorySourceGate::Allow);
        }
        let conn = self.open_or_create_connection()?;
        let previous: Option<(i64, i64, f64, String)> = conn
            .query_row(
                r#"
                SELECT last_seen_lines, last_seen_len, last_seen_mtime, prefix_hash
                FROM memory_extraction_source_state
                WHERE source_key = ?1;
                "#,
                params![snapshot.source_key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((last_lines, last_len, _last_mtime, prefix_hash)) = previous else {
            return Ok(MemorySourceGate::Allow);
        };
        let prefix_is_stable = snapshot.prefix_lines as usize >= PREFIX_HASH_LINES
            && last_lines as usize >= PREFIX_HASH_LINES;
        if snapshot.line_count < last_lines
            || snapshot.byte_len < last_len
            || (prefix_is_stable && !prefix_hash.is_empty() && snapshot.prefix_hash != prefix_hash)
        {
            return Ok(MemorySourceGate::Allow);
        }
        if snapshot.line_count.saturating_sub(last_lines) < threshold {
            return Ok(MemorySourceGate::InsufficientGrowth);
        }
        Ok(MemorySourceGate::Allow)
    }
}

pub(crate) fn memory_text_has_signal(text: &str) -> bool {
    let Some(text) = normalized_string(Some(text)) else {
        return false;
    };
    if memory_text_has_auto_trigger_marker(&text) {
        return true;
    }
    let lower = text.to_lowercase();
    let durable_terms = [
        "always",
        "never",
        "prefer",
        "preferred",
        "decided",
        "decision",
        "convention",
        "rule",
        "standard",
        "remember",
        "fix",
        "fixed",
        "bug",
        "regression",
        "error",
        "failed",
        "warning",
        "config",
        "setting",
        "command",
        "release",
        "deploy",
        "约定",
        "决定",
        "以后",
        "总是",
        "不要",
        "必须",
        "偏好",
        "修复",
        "报错",
        "错误",
        "问题",
        "配置",
        "命令",
        "发布",
    ];
    durable_terms.iter().any(|term| lower.contains(term))
        || looks_like_path_or_identifier(&text)
        || looks_like_command_or_diff(&text)
}

pub(crate) fn memory_text_has_auto_trigger_marker(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start().to_lowercase();
        line.starts_with("记住:")
            || line.starts_with("记住：")
            || line.starts_with("#记住")
            || line.starts_with("/记住")
            || line.starts_with("remember:")
            || line.starts_with("remember ")
            || line.starts_with("#remember")
            || line.starts_with("/remember")
            || line.starts_with("user memory:")
            || line.starts_with("project memory:")
    })
}

fn looks_like_path_or_identifier(text: &str) -> bool {
    text.split_whitespace().any(|token| {
        let token = token.trim_matches(|ch: char| {
            matches!(ch, ',' | ';' | ':' | '"' | '\'' | ')' | ']' | '}')
        });
        token.contains('/')
            || token.contains('\\')
            || token.contains("::")
            || token.contains("=>")
            || token.ends_with(".rs")
            || token.ends_with(".ts")
            || token.ends_with(".tsx")
            || token.ends_with(".js")
            || token.ends_with(".json")
            || token.ends_with(".toml")
            || token.ends_with(".md")
            || (token.contains('_') && token.len() >= 5)
            || (token.contains('-') && token.len() >= 5)
    })
}

fn looks_like_command_or_diff(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with('$')
            || line.starts_with("cargo ")
            || line.starts_with("git ")
            || line.starts_with("npm ")
            || line.starts_with("pnpm ")
            || line.starts_with("just ")
            || line.starts_with("flutter ")
            || line.starts_with("diff --git")
            || line.starts_with("+++ ")
            || line.starts_with("--- ")
            || line.starts_with("+ ")
            || line.starts_with("- ")
            || line.contains(" = ")
            || line.contains(": ")
    })
}

const PREFIX_HASH_LINES: usize = 16;

fn transcript_file_stats(path: &str) -> Option<(i64, String, i64)> {
    let file = fs::File::open(path).ok()?;
    let mut line_count = 0_i64;
    let mut prefix = Vec::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        line_count += 1;
        if prefix.len() < PREFIX_HASH_LINES {
            prefix.push(line);
        }
    }
    let prefix_lines = prefix.len() as i64;
    Some((line_count, sha256_hex(&prefix.join("\n")), prefix_lines))
}

fn prefix_hash(text: &str) -> String {
    let prefix = text
        .lines()
        .take(PREFIX_HASH_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    sha256_hex(&prefix)
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
