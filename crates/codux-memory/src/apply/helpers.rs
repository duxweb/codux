use super::{MemoryCandidate, MemoryEntryStatus, StoredMemoryEntry, StoredMemorySummary};
use crate::extraction::{MemoryKind, MemoryScope, MemoryTier, parse_uuid_string};
use rusqlite::types::Value as SqlValue;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub(super) fn stored_entry_select_columns() -> &'static str {
    "id, scope, project_id, tool_id, module_key, tier, kind, content, rationale, source_tool, source_session_id, source_fingerprint, normalized_hash, superseded_by, status, merged_summary_id, merged_at, archived_at, access_count, last_accessed_at, created_at, updated_at"
}

pub(super) fn stored_memory_entry_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredMemoryEntry> {
    Ok(StoredMemoryEntry {
        id: row.get(0)?,
        scope: MemoryScope::from_str(row.get::<_, String>(1)?.as_str()),
        project_id: row.get(2)?,
        tool_id: row.get(3)?,
        module_key: row.get(4)?,
        tier: MemoryTier::from_str(row.get::<_, String>(5)?.as_str()),
        kind: MemoryKind::from_str(row.get::<_, String>(6)?.as_str()),
        content: row.get(7)?,
        rationale: row.get(8)?,
        source_tool: row.get(9)?,
        source_session_id: row.get(10)?,
        source_fingerprint: row.get(11)?,
        normalized_hash: row.get(12)?,
        superseded_by: row.get(13)?,
        status: MemoryEntryStatus::from_str(row.get::<_, String>(14)?.as_str()),
        merged_summary_id: row.get(15)?,
        merged_at: row.get(16)?,
        archived_at: row.get(17)?,
        access_count: row.get(18)?,
        last_accessed_at: row.get(19)?,
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
        last_decision: None,
    })
}

pub(super) fn stored_memory_summary_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredMemorySummary> {
    let source_ids: Option<String> = row.get(6)?;
    Ok(StoredMemorySummary {
        id: row.get(0)?,
        scope: MemoryScope::from_str(row.get::<_, String>(1)?.as_str()),
        project_id: row.get(2)?,
        tool_id: row.get(3)?,
        content: row.get(4)?,
        version: row.get(5)?,
        source_entry_ids: decode_string_array(source_ids.as_deref()),
        token_estimate: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub(super) fn optional_text_value(value: Option<&str>) -> SqlValue {
    value
        .map(|value| SqlValue::Text(value.to_string()))
        .unwrap_or(SqlValue::Null)
}

pub(super) fn sorted_unique(values: &[String]) -> Vec<String> {
    let mut values = values
        .iter()
        .filter_map(|value| parse_uuid_string(value))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn decode_string_array(value: Option<&str>) -> Vec<String> {
    value
        .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

pub(super) fn estimate_tokens(value: &str) -> i64 {
    (value.chars().count() as i64 + 3) / 4
}

pub(super) fn preferred_tier(existing: &MemoryTier, candidate: &MemoryTier) -> MemoryTier {
    match (existing, candidate) {
        (MemoryTier::Core, _) | (_, MemoryTier::Core) => MemoryTier::Core,
        (MemoryTier::Working, _) | (_, MemoryTier::Working) => MemoryTier::Working,
        _ => MemoryTier::Archive,
    }
}

pub(super) fn should_skip_memory_candidate(candidate: &MemoryCandidate) -> bool {
    let normalized = normalized_memory_content(&candidate.content);
    if normalized.chars().count() < 12 {
        return true;
    }
    let terms = memory_similarity_terms(&normalized);
    terms.len() < 2 && normalized.chars().count() < 28
}

pub(super) fn memory_candidate_conflicts(
    candidate: &MemoryCandidate,
    existing: &StoredMemoryEntry,
) -> bool {
    candidate.scope == existing.scope
        && candidate.project_id == existing.project_id
        && candidate.module_key == existing.module_key
        && candidate.kind == existing.kind
        && candidate.content != existing.content
}

pub(super) fn merge_memory_content(existing: &str, candidate: &str) -> String {
    if existing.contains(candidate) {
        existing.to_string()
    } else if candidate.contains(existing) {
        candidate.to_string()
    } else {
        format!("{existing}; {candidate}")
    }
}

pub(super) fn merge_optional_memory_text(
    existing: Option<&str>,
    candidate: Option<&str>,
) -> Option<String> {
    match (
        existing.and_then(normalized_non_empty),
        candidate.and_then(normalized_non_empty),
    ) {
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(left), Some(right)) => Some(format!("{left}; {right}")),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

pub(super) fn memory_similarity(left: &str, right: &str) -> f64 {
    let left_norm = normalized_memory_content(left);
    let right_norm = normalized_memory_content(right);
    if left_norm.is_empty() || right_norm.is_empty() {
        return 0.0;
    }
    if left_norm == right_norm {
        return 1.0;
    }
    let left_terms = memory_similarity_terms(&left_norm);
    let right_terms = memory_similarity_terms(&right_norm);
    let term_score = jaccard_score(&left_terms, &right_terms);
    let left_grams = memory_char_ngrams(&left_norm);
    let right_grams = memory_char_ngrams(&right_norm);
    let char_score = jaccard_score(&left_grams, &right_grams);
    term_score.max(char_score)
}

fn memory_similarity_terms(value: &str) -> HashSet<String> {
    memory_query_terms(value)
        .into_iter()
        .filter(|term| term.chars().count() >= 2)
        .collect()
}

fn memory_char_ngrams(value: &str) -> HashSet<String> {
    let chars = value
        .chars()
        .filter(|ch| !ch.is_whitespace() && !ch.is_ascii_punctuation())
        .collect::<Vec<_>>();
    if chars.len() < 2 {
        return HashSet::new();
    }
    chars
        .windows(2)
        .map(|window| window.iter().collect::<String>())
        .collect()
}

fn jaccard_score(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(right).count() as f64;
    let union = left.union(right).count() as f64;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn memory_query_terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    query
        .split(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    ',' | '.'
                        | ';'
                        | ':'
                        | '/'
                        | '\\'
                        | '|'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '<'
                        | '>'
                        | '"'
                        | '\''
                        | '`'
                )
        })
        .filter_map(|term| {
            let normalized = term.trim().to_lowercase();
            if normalized.chars().count() < 2 || !seen.insert(normalized.clone()) {
                return None;
            }
            Some(normalized)
        })
        .take(120)
        .collect()
}

pub(super) fn normalized_memory_content(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(super) fn normalized_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
