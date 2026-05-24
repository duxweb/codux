use crate::ai_history::{
    deterministic_uuid, half_hour_bucket_start, history_key, local_day_start_seconds, now_seconds,
    AIHeatmapDay, AIHistoryProjectRequest, AIHistorySnapshot, AIProjectUsageSummary,
    AISessionSummary, AITimeBucket, AIUsageBreakdownItem, HistoryEntry, HistoryEvent, HistoryRole,
    JSONLParseSnapshot, ParsedHistory,
};
use crate::paths::app_support_dir;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const NORMALIZED_HISTORY_SCHEMA_VERSION: &str = "6";
const RECENT_HISTORY_SESSION_LIMIT: usize = 80;

const SCHEMA_STATEMENTS: &[&str] = &[
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_file_state (
        source TEXT NOT NULL,
        file_path TEXT NOT NULL,
        project_path TEXT NOT NULL,
        file_modified_at REAL NOT NULL,
        PRIMARY KEY (source, file_path, project_path)
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_file_session_link (
        source TEXT NOT NULL,
        file_path TEXT NOT NULL,
        project_path TEXT NOT NULL,
        session_key TEXT NOT NULL,
        external_session_id TEXT,
        project_id TEXT NOT NULL,
        project_name TEXT NOT NULL,
        session_title TEXT NOT NULL,
        first_seen_at REAL NOT NULL,
        last_seen_at REAL NOT NULL,
        last_model TEXT,
        active_duration_seconds INTEGER NOT NULL,
        PRIMARY KEY (source, file_path, project_path, session_key)
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_file_usage_bucket (
        source TEXT NOT NULL,
        file_path TEXT NOT NULL,
        project_path TEXT NOT NULL,
        session_key TEXT NOT NULL,
        model TEXT NOT NULL,
        bucket_start REAL NOT NULL,
        bucket_end REAL NOT NULL,
        input_tokens INTEGER NOT NULL,
        output_tokens INTEGER NOT NULL,
        total_tokens INTEGER NOT NULL,
        cached_input_tokens INTEGER NOT NULL,
        request_count INTEGER NOT NULL,
        PRIMARY KEY (source, file_path, project_path, session_key, model, bucket_start)
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_project_index_state (
        project_path TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        project_name TEXT NOT NULL,
        indexed_at REAL NOT NULL
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_file_checkpoint (
        source TEXT NOT NULL,
        file_path TEXT NOT NULL,
        project_path TEXT NOT NULL,
        file_modified_at REAL NOT NULL,
        file_size INTEGER NOT NULL,
        last_offset INTEGER NOT NULL,
        last_indexed_at REAL NOT NULL,
        payload_json TEXT,
        PRIMARY KEY (source, file_path, project_path)
    );
    "#,
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_state_project_path ON ai_history_file_state(project_path);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_checkpoint_project_path ON ai_history_file_checkpoint(project_path);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_session_link_project_path ON ai_history_file_session_link(project_path);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_usage_bucket_project_path ON ai_history_file_usage_bucket(project_path, bucket_start);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_usage_bucket_bucket_start ON ai_history_file_usage_bucket(bucket_start);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_project_index_state_indexed_at ON ai_history_project_index_state(indexed_at DESC);",
];

#[derive(Debug, Clone)]
pub(crate) struct AIUsageStore {
    database_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct AIExternalFileSummary {
    pub(crate) source: String,
    pub(crate) file_path: String,
    pub(crate) file_modified_at: f64,
    pub(crate) file_size: i64,
    pub(crate) project_path: String,
    pub(crate) usage_buckets: Vec<AIUsageBucket>,
}

#[derive(Debug, Clone)]
pub(crate) struct AIUsageBucket {
    pub(crate) source: String,
    pub(crate) session_key: String,
    pub(crate) external_session_id: Option<String>,
    pub(crate) session_title: String,
    pub(crate) model: Option<String>,
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) bucket_start: f64,
    pub(crate) bucket_end: f64,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) total_tokens: i64,
    pub(crate) cached_input_tokens: i64,
    pub(crate) request_count: i64,
    pub(crate) active_duration_seconds: i64,
    pub(crate) first_seen_at: f64,
    pub(crate) last_seen_at: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AIUsageProjectTotal {
    pub(crate) project_id: String,
    pub(crate) total_tokens: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct AIExternalFileCheckpoint {
    pub(crate) source: String,
    pub(crate) file_path: String,
    pub(crate) project_path: String,
    pub(crate) file_modified_at: f64,
    pub(crate) file_size: i64,
    pub(crate) last_offset: i64,
    pub(crate) last_indexed_at: f64,
    pub(crate) payload_json: Option<String>,
}

#[derive(Debug, Clone)]
struct NormalizedSessionLinkRow {
    source: String,
    session_key: String,
    external_session_id: Option<String>,
    project_id: String,
    project_name: String,
    session_title: String,
    first_seen_at: f64,
    last_seen_at: f64,
    last_model: Option<String>,
    active_duration_seconds: i64,
}

#[derive(Debug, Clone)]
struct StoredUsageBucketRow {
    source: String,
    session_key: String,
    model: Option<String>,
    bucket_start: f64,
    bucket_end: f64,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cached_input_tokens: i64,
    request_count: i64,
}

#[derive(Debug, Default, Clone)]
struct PersistedSessionAccumulator {
    source: String,
    session_key: String,
    external_session_id: Option<String>,
    title: Option<String>,
    first_seen_at: f64,
    last_seen_at: f64,
    last_model: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cached_input_tokens: i64,
    request_count: i64,
    today_tokens: i64,
    today_cached_input_tokens: i64,
    active_duration_seconds: i64,
}

#[derive(Debug, Default, Clone)]
struct ParsedSessionAccumulator {
    session_key: String,
    external_session_id: Option<String>,
    title: Option<String>,
    first_seen_at: f64,
    last_seen_at: f64,
    last_model: Option<String>,
    active_duration_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JSONLIndexMode {
    Unchanged,
    Append,
    Rebuild,
}

impl AIUsageStore {
    pub(crate) fn default() -> Self {
        Self {
            database_path: default_database_path(),
        }
    }

    #[cfg(test)]
    pub(crate) fn at_path(database_path: PathBuf) -> Self {
        Self { database_path }
    }

    pub(crate) fn connect(&self) -> Result<Connection> {
        if let Some(parent) = self.database_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create AI usage database directory {}",
                    parent.display()
                )
            })?;
        }
        let conn = Connection::open(&self.database_path).with_context(|| {
            format!(
                "failed to open AI usage database {}",
                self.database_path.display()
            )
        })?;
        conn.busy_timeout(std::time::Duration::from_millis(3_000))?;
        initialize_connection(&conn)?;
        Ok(conn)
    }

    pub(crate) fn global_today_normalized_tokens(&self, conn: &Connection) -> Result<i64> {
        let start = local_day_start_seconds(now_seconds());
        let end = start + 86_400.0;
        conn.query_row(
            r#"
            SELECT COALESCE(SUM(total_tokens), 0)
            FROM ai_history_file_usage_bucket
            WHERE bucket_end > ?1 AND bucket_start < ?2;
            "#,
            params![start, end],
            |row| row.get(0),
        )
        .map_err(Into::into)
    }

    pub(crate) fn global_all_time_normalized_tokens(&self, conn: &Connection) -> Result<i64> {
        conn.query_row(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM ai_history_file_usage_bucket;",
            [],
            |row| row.get(0),
        )
        .map_err(Into::into)
    }

    pub(crate) fn normalized_project_totals_since(
        &self,
        conn: &Connection,
        cutoff: Option<f64>,
    ) -> Result<Vec<AIUsageProjectTotal>> {
        let mut statement = conn.prepare(
            r#"
            SELECT
                session.project_id,
                COALESCE(SUM(CASE WHEN ?1 IS NULL OR bucket.bucket_start >= ?2 THEN bucket.total_tokens ELSE 0 END), 0) AS total_tokens
            FROM ai_history_file_session_link AS session
            LEFT JOIN ai_history_file_usage_bucket AS bucket
              ON bucket.source = session.source
             AND bucket.file_path = session.file_path
             AND bucket.project_path = session.project_path
             AND bucket.session_key = session.session_key
            WHERE (?3 IS NULL OR session.last_seen_at >= ?4)
            GROUP BY session.project_id
            HAVING total_tokens > 0
            ORDER BY session.project_id ASC;
            "#,
        )?;
        let rows = statement
            .query_map(params![cutoff, cutoff, cutoff, cutoff], |row| {
                Ok(AIUsageProjectTotal {
                    project_id: row.get(0)?,
                    total_tokens: row.get::<_, i64>(1)?.max(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub(crate) fn indexed_sessions_since(
        &self,
        conn: &Connection,
        cutoff: Option<f64>,
    ) -> Result<Vec<AISessionSummary>> {
        let today_start = local_day_start_seconds(now_seconds());
        let today_end = today_start + 86_400.0;
        let mut statement = conn.prepare(
            r#"
            SELECT
                session.source,
                session.session_key,
                session.external_session_id,
                session.project_id,
                session.project_name,
                session.project_path,
                session.session_title,
                session.first_seen_at,
                session.last_seen_at,
                session.last_model,
                session.active_duration_seconds,
                COALESCE(SUM(CASE WHEN ?1 IS NULL OR bucket.bucket_start >= ?2 THEN bucket.request_count ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ?3 IS NULL OR bucket.bucket_start >= ?4 THEN bucket.input_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ?5 IS NULL OR bucket.bucket_start >= ?6 THEN bucket.output_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ?7 IS NULL OR bucket.bucket_start >= ?8 THEN bucket.total_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ?9 IS NULL OR bucket.bucket_start >= ?10 THEN bucket.cached_input_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN bucket.bucket_end > ?11 AND bucket.bucket_start < ?12 THEN bucket.total_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN bucket.bucket_end > ?13 AND bucket.bucket_start < ?14 THEN bucket.cached_input_tokens ELSE 0 END), 0)
            FROM ai_history_file_session_link AS session
            LEFT JOIN ai_history_file_usage_bucket AS bucket
              ON bucket.source = session.source
             AND bucket.file_path = session.file_path
             AND bucket.project_path = session.project_path
             AND bucket.session_key = session.session_key
            WHERE (?15 IS NULL OR session.last_seen_at >= ?16)
            GROUP BY
                session.source,
                session.file_path,
                session.project_path,
                session.session_key,
                session.external_session_id,
                session.project_id,
                session.project_name,
                session.session_title,
                session.first_seen_at,
                session.last_seen_at,
                session.last_model,
                session.active_duration_seconds
            HAVING total_tokens > 0 OR cached_input_tokens > 0 OR request_count > 0
            ORDER BY session.last_seen_at DESC;
            "#,
        )?;
        let rows = statement
            .query_map(
                params![
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    today_start,
                    today_end,
                    today_start,
                    today_end,
                    cutoff,
                    cutoff,
                ],
                |row| {
                    let source: String = row.get(0)?;
                    let session_key: String = row.get(1)?;
                    let external_session_id: Option<String> = row.get(2)?;
                    let first_seen_at: f64 = row.get(7)?;
                    let last_seen_at: f64 = row.get(8)?;
                    let stored_active_duration: i64 = row.get(10)?;
                    let active_duration_seconds = cutoff
                        .map(|cutoff| {
                            if last_seen_at <= cutoff {
                                0
                            } else {
                                let clipped = (last_seen_at - first_seen_at.max(cutoff))
                                    .max(0.0)
                                    .round() as i64;
                                stored_active_duration.max(0).min(clipped)
                            }
                        })
                        .unwrap_or(stored_active_duration.max(0));
                    Ok(AISessionSummary {
                        session_id: deterministic_uuid(&history_group_key(
                            &source,
                            &session_key,
                            external_session_id.as_deref(),
                        )),
                        external_session_id,
                        project_id: row.get(3)?,
                        project_name: row.get(4)?,
                        project_path: row.get(5)?,
                        session_title: row.get(6)?,
                        first_seen_at,
                        last_seen_at,
                        last_tool: Some(source),
                        last_model: row.get(9)?,
                        active_duration_seconds,
                        request_count: row.get(11)?,
                        total_input_tokens: row.get(12)?,
                        total_output_tokens: row.get(13)?,
                        total_tokens: row.get(14)?,
                        cached_input_tokens: row.get(15)?,
                        today_tokens: row.get(16)?,
                        today_cached_input_tokens: row.get(17)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub(crate) fn load_or_index_file<F>(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &Path,
        project: &AIHistoryProjectRequest,
        parser: F,
    ) -> Result<AIExternalFileSummary>
    where
        F: FnOnce() -> ParsedHistory,
    {
        let metadata = fs::metadata(file_path)
            .with_context(|| format!("failed to read AI history file {}", file_path.display()))?;
        let normalized_file_path = normalized_path(file_path);
        let modified_at = modified_seconds(&metadata);
        let file_size = metadata.len().min(i64::MAX as u64) as i64;

        if let Some(summary) = self.stored_external_summary(
            conn,
            source,
            &normalized_file_path,
            &project.path,
            Some(modified_at),
        )? {
            return Ok(summary);
        }

        let parsed = parser();
        let summary = external_file_summary_from_parsed(
            source,
            normalized_file_path,
            modified_at,
            file_size,
            project,
            parsed,
        );
        let checkpoint = AIExternalFileCheckpoint {
            source: summary.source.clone(),
            file_path: summary.file_path.clone(),
            project_path: summary.project_path.clone(),
            file_modified_at: summary.file_modified_at,
            file_size: summary.file_size,
            last_offset: summary.file_size,
            last_indexed_at: now_seconds(),
            payload_json: None,
        };
        self.replace_external_summary(conn, &summary, Some(&checkpoint))?;
        Ok(summary)
    }

    pub(crate) fn load_or_index_jsonl_file<AppendParser, RebuildParser>(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &Path,
        project: &AIHistoryProjectRequest,
        append_parser: AppendParser,
        rebuild_parser: RebuildParser,
    ) -> Result<AIExternalFileSummary>
    where
        AppendParser: FnOnce(Option<&AIExternalFileCheckpoint>) -> JSONLParseSnapshot,
        RebuildParser: FnOnce() -> JSONLParseSnapshot,
    {
        let metadata = fs::metadata(file_path)
            .with_context(|| format!("failed to read AI history file {}", file_path.display()))?;
        let normalized_file_path = normalized_path(file_path);
        let modified_at = modified_seconds(&metadata);
        let file_size = metadata.len().min(i64::MAX as u64) as i64;
        let stored_summary =
            self.stored_external_summary(conn, source, &normalized_file_path, &project.path, None)?;
        let checkpoint =
            self.external_file_checkpoint(conn, source, &normalized_file_path, &project.path)?;

        match jsonl_index_mode(
            file_size,
            modified_at,
            stored_summary.as_ref(),
            checkpoint.as_ref(),
        ) {
            JSONLIndexMode::Unchanged => {
                if let Some(summary) = stored_summary {
                    return Ok(summary);
                }
            }
            JSONLIndexMode::Append => {
                if let (Some(stored_summary), Some(checkpoint)) =
                    (stored_summary.as_ref(), checkpoint.as_ref())
                {
                    let snapshot = append_parser(Some(checkpoint));
                    let delta = external_file_summary_from_parsed(
                        source,
                        normalized_file_path.clone(),
                        modified_at,
                        file_size,
                        project,
                        snapshot.result,
                    );
                    let summary = AIExternalFileSummary {
                        source: source.to_string(),
                        file_path: normalized_file_path.clone(),
                        file_modified_at: modified_at,
                        file_size,
                        project_path: project.path.clone(),
                        usage_buckets: merge_usage_buckets(
                            &stored_summary.usage_buckets,
                            &delta.usage_buckets,
                        ),
                    };
                    let checkpoint = AIExternalFileCheckpoint {
                        source: summary.source.clone(),
                        file_path: summary.file_path.clone(),
                        project_path: summary.project_path.clone(),
                        file_modified_at: summary.file_modified_at,
                        file_size: summary.file_size,
                        last_offset: snapshot.last_processed_offset.clamp(0, file_size),
                        last_indexed_at: now_seconds(),
                        payload_json: snapshot
                            .payload_json
                            .or_else(|| checkpoint.payload_json.clone()),
                    };
                    self.replace_external_summary(conn, &summary, Some(&checkpoint))?;
                    return Ok(summary);
                }
            }
            JSONLIndexMode::Rebuild => {}
        }

        let snapshot = rebuild_parser();
        let summary = external_file_summary_from_parsed(
            source,
            normalized_file_path,
            modified_at,
            file_size,
            project,
            snapshot.result,
        );
        let checkpoint = AIExternalFileCheckpoint {
            source: summary.source.clone(),
            file_path: summary.file_path.clone(),
            project_path: summary.project_path.clone(),
            file_modified_at: summary.file_modified_at,
            file_size: summary.file_size,
            last_offset: snapshot.last_processed_offset.clamp(0, file_size),
            last_indexed_at: now_seconds(),
            payload_json: snapshot.payload_json,
        };
        self.replace_external_summary(conn, &summary, Some(&checkpoint))?;
        Ok(summary)
    }

    pub(crate) fn stored_external_summary(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &str,
        project_path: &str,
        modified_at: Option<f64>,
    ) -> Result<Option<AIExternalFileSummary>> {
        let state_modified_at = conn
            .query_row(
                r#"
                SELECT file_modified_at
                FROM ai_history_file_state
                WHERE source = ?1 AND file_path = ?2 AND project_path = ?3
                LIMIT 1;
                "#,
                params![source, file_path, project_path],
                |row| row.get::<_, f64>(0),
            )
            .optional()?;
        let Some(state_modified_at) = state_modified_at else {
            return Ok(None);
        };
        if let Some(modified_at) = modified_at {
            if !same_timestamp(state_modified_at, modified_at) {
                return Ok(None);
            }
        }

        let usage_buckets = self.load_usage_buckets(conn, source, file_path, project_path)?;
        let checkpoint = self.external_file_checkpoint(conn, source, file_path, project_path)?;
        Ok(Some(AIExternalFileSummary {
            source: source.to_string(),
            file_path: file_path.to_string(),
            file_modified_at: state_modified_at,
            file_size: checkpoint.map(|item| item.file_size).unwrap_or(0),
            project_path: project_path.to_string(),
            usage_buckets,
        }))
    }

    pub(crate) fn replace_external_summary(
        &self,
        conn: &Connection,
        summary: &AIExternalFileSummary,
        checkpoint: Option<&AIExternalFileCheckpoint>,
    ) -> Result<()> {
        conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
        let result = (|| -> Result<()> {
            conn.execute(
                "DELETE FROM ai_history_file_session_link WHERE source = ?1 AND file_path = ?2 AND project_path = ?3;",
                params![summary.source, summary.file_path, summary.project_path],
            )?;
            conn.execute(
                "DELETE FROM ai_history_file_usage_bucket WHERE source = ?1 AND file_path = ?2 AND project_path = ?3;",
                params![summary.source, summary.file_path, summary.project_path],
            )?;
            conn.execute(
                r#"
                INSERT INTO ai_history_file_state (source, file_path, project_path, file_modified_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(source, file_path, project_path) DO UPDATE SET
                    file_modified_at = excluded.file_modified_at;
                "#,
                params![
                    summary.source,
                    summary.file_path,
                    summary.project_path,
                    summary.file_modified_at
                ],
            )?;

            if let Some(checkpoint) = checkpoint {
                conn.execute(
                    r#"
                    INSERT INTO ai_history_file_checkpoint (
                        source, file_path, project_path, file_modified_at,
                        file_size, last_offset, last_indexed_at, payload_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                    ON CONFLICT(source, file_path, project_path) DO UPDATE SET
                        file_modified_at = excluded.file_modified_at,
                        file_size = excluded.file_size,
                        last_offset = excluded.last_offset,
                        last_indexed_at = excluded.last_indexed_at,
                        payload_json = excluded.payload_json;
                    "#,
                    params![
                        checkpoint.source,
                        checkpoint.file_path,
                        checkpoint.project_path,
                        checkpoint.file_modified_at,
                        checkpoint.file_size,
                        checkpoint.last_offset,
                        checkpoint.last_indexed_at,
                        checkpoint.payload_json,
                    ],
                )?;
            } else {
                conn.execute(
                    "DELETE FROM ai_history_file_checkpoint WHERE source = ?1 AND file_path = ?2 AND project_path = ?3;",
                    params![summary.source, summary.file_path, summary.project_path],
                )?;
            }

            for session in build_session_links(&summary.usage_buckets) {
                conn.execute(
                    r#"
                    INSERT INTO ai_history_file_session_link (
                        source, file_path, project_path, session_key, external_session_id,
                        project_id, project_name, session_title, first_seen_at, last_seen_at,
                        last_model, active_duration_seconds
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);
                    "#,
                    params![
                        summary.source,
                        summary.file_path,
                        summary.project_path,
                        session.session_key,
                        session.external_session_id,
                        session.project_id,
                        session.project_name,
                        session.session_title,
                        session.first_seen_at,
                        session.last_seen_at,
                        session.last_model,
                        session.active_duration_seconds,
                    ],
                )?;
            }

            for bucket in &summary.usage_buckets {
                conn.execute(
                    r#"
                    INSERT INTO ai_history_file_usage_bucket (
                        source, file_path, project_path, session_key, model, bucket_start, bucket_end,
                        input_tokens, output_tokens, total_tokens, cached_input_tokens, request_count
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);
                    "#,
                    params![
                        summary.source,
                        summary.file_path,
                        summary.project_path,
                        bucket.session_key,
                        bucket.model.clone().unwrap_or_default(),
                        bucket.bucket_start,
                        bucket.bucket_end,
                        bucket.input_tokens,
                        bucket.output_tokens,
                        bucket.total_tokens,
                        bucket.cached_input_tokens,
                        bucket.request_count,
                    ],
                )?;
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT;")?;
                Ok(())
            }
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK;");
                Err(error)
            }
        }
    }

    pub(crate) fn project_snapshot(
        &self,
        conn: &Connection,
        project: AIHistoryProjectRequest,
    ) -> Result<AIHistorySnapshot> {
        let links = self.project_session_links(conn, &project.path)?;
        let buckets = self.project_usage_buckets(conn, &project.path)?;
        Ok(build_snapshot_from_rows(project, links, buckets))
    }

    pub(crate) fn indexed_project_snapshot(
        &self,
        conn: &Connection,
        project: AIHistoryProjectRequest,
    ) -> Result<Option<AIHistorySnapshot>> {
        let indexed_at = conn
            .query_row(
                r#"
                SELECT indexed_at
                FROM ai_history_project_index_state
                WHERE project_path = ?1
                LIMIT 1;
                "#,
                params![project.path],
                |row| row.get::<_, f64>(0),
            )
            .optional()?;
        let Some(indexed_at) = indexed_at else {
            return Ok(None);
        };
        let mut snapshot = self.project_snapshot(conn, project)?;
        snapshot.indexed_at = indexed_at;
        Ok(Some(snapshot))
    }

    pub(crate) fn rename_project_session(
        &self,
        conn: &Connection,
        project_path: &str,
        session_id: &str,
        title: &str,
    ) -> Result<bool> {
        let title = title.trim();
        if title.is_empty() {
            return Ok(false);
        }
        let links = self.project_session_links(conn, project_path)?;
        let matched = matching_session_keys(&links, session_id);
        if matched.is_empty() {
            return Ok(false);
        }
        let tx = conn.unchecked_transaction()?;
        for (source, session_key) in &matched {
            tx.execute(
                r#"
                UPDATE ai_history_file_session_link
                SET session_title = ?1
                WHERE project_path = ?2 AND source = ?3 AND session_key = ?4;
                "#,
                params![title, project_path, source, session_key],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    pub(crate) fn remove_project_session(
        &self,
        conn: &Connection,
        project_path: &str,
        session_id: &str,
    ) -> Result<bool> {
        let links = self.project_session_links(conn, project_path)?;
        let matched = matching_session_keys(&links, session_id);
        if matched.is_empty() {
            return Ok(false);
        }
        let tx = conn.unchecked_transaction()?;
        for (source, session_key) in &matched {
            tx.execute(
                r#"
                DELETE FROM ai_history_file_usage_bucket
                WHERE project_path = ?1 AND source = ?2 AND session_key = ?3;
                "#,
                params![project_path, source, session_key],
            )?;
            tx.execute(
                r#"
                DELETE FROM ai_history_file_session_link
                WHERE project_path = ?1 AND source = ?2 AND session_key = ?3;
                "#,
                params![project_path, source, session_key],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    pub(crate) fn save_project_index_state(
        &self,
        conn: &Connection,
        snapshot: &AIHistorySnapshot,
        project_path: &str,
    ) -> Result<()> {
        conn.execute(
            r#"
            INSERT INTO ai_history_project_index_state (project_path, project_id, project_name, indexed_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(project_path) DO UPDATE SET
                project_id = excluded.project_id,
                project_name = excluded.project_name,
                indexed_at = excluded.indexed_at;
            "#,
            params![
                project_path,
                snapshot.project_id,
                snapshot.project_name,
                snapshot.indexed_at
            ],
        )?;
        Ok(())
    }

    fn external_file_checkpoint(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &str,
        project_path: &str,
    ) -> Result<Option<AIExternalFileCheckpoint>> {
        conn.query_row(
            r#"
            SELECT file_modified_at, file_size, last_offset, last_indexed_at, payload_json
            FROM ai_history_file_checkpoint
            WHERE source = ?1 AND file_path = ?2 AND project_path = ?3
            LIMIT 1;
            "#,
            params![source, file_path, project_path],
            |row| {
                Ok(AIExternalFileCheckpoint {
                    source: source.to_string(),
                    file_path: file_path.to_string(),
                    project_path: project_path.to_string(),
                    file_modified_at: row.get(0)?,
                    file_size: row.get(1)?,
                    last_offset: row.get(2)?,
                    last_indexed_at: row.get(3)?,
                    payload_json: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    fn load_usage_buckets(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &str,
        project_path: &str,
    ) -> Result<Vec<AIUsageBucket>> {
        let session_links = self.load_session_links(conn, source, file_path, project_path)?;
        let mut statement = conn.prepare(
            r#"
            SELECT session_key, model, bucket_start, bucket_end, input_tokens, output_tokens,
                   total_tokens, cached_input_tokens, request_count
            FROM ai_history_file_usage_bucket
            WHERE source = ?1 AND file_path = ?2 AND project_path = ?3
            ORDER BY bucket_start ASC, session_key ASC, model ASC;
            "#,
        )?;
        let rows = statement
            .query_map(params![source, file_path, project_path], |row| {
                Ok(StoredUsageBucketRow {
                    source: source.to_string(),
                    session_key: row.get(0)?,
                    model: normalized_optional_string(row.get::<_, String>(1)?.as_str()),
                    bucket_start: row.get(2)?,
                    bucket_end: row.get(3)?,
                    input_tokens: row.get(4)?,
                    output_tokens: row.get(5)?,
                    total_tokens: row.get(6)?,
                    cached_input_tokens: row.get(7)?,
                    request_count: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let usage_buckets = rows
            .into_iter()
            .filter_map(|row| {
                let session = session_links.get(&row.session_key)?;
                Some(AIUsageBucket {
                    source: row.source,
                    session_key: row.session_key,
                    external_session_id: session.external_session_id.clone(),
                    session_title: session.session_title.clone(),
                    model: row.model,
                    project_id: session.project_id.clone(),
                    project_name: session.project_name.clone(),
                    bucket_start: row.bucket_start,
                    bucket_end: row.bucket_end,
                    input_tokens: row.input_tokens,
                    output_tokens: row.output_tokens,
                    total_tokens: row.total_tokens,
                    cached_input_tokens: row.cached_input_tokens,
                    request_count: row.request_count,
                    active_duration_seconds: session.active_duration_seconds,
                    first_seen_at: session.first_seen_at,
                    last_seen_at: session.last_seen_at,
                })
            })
            .collect();
        Ok(usage_buckets)
    }

    fn load_session_links(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &str,
        project_path: &str,
    ) -> Result<HashMap<String, NormalizedSessionLinkRow>> {
        let mut statement = conn.prepare(
            r#"
            SELECT session_key, external_session_id, project_id, project_name, session_title,
                   first_seen_at, last_seen_at, last_model, active_duration_seconds
            FROM ai_history_file_session_link
            WHERE source = ?1 AND file_path = ?2 AND project_path = ?3
            ORDER BY last_seen_at DESC;
            "#,
        )?;
        let rows = statement.query_map(params![source, file_path, project_path], |row| {
            Ok(NormalizedSessionLinkRow {
                source: source.to_string(),
                session_key: row.get(0)?,
                external_session_id: row.get(1)?,
                project_id: row.get(2)?,
                project_name: row.get(3)?,
                session_title: row.get(4)?,
                first_seen_at: row.get(5)?,
                last_seen_at: row.get(6)?,
                last_model: row.get(7)?,
                active_duration_seconds: row.get(8)?,
            })
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let row = row?;
            map.insert(row.session_key.clone(), row);
        }
        Ok(map)
    }

    fn project_session_links(
        &self,
        conn: &Connection,
        project_path: &str,
    ) -> Result<Vec<NormalizedSessionLinkRow>> {
        let mut statement = conn.prepare(
            r#"
            SELECT source, file_path, project_path, session_key, external_session_id,
                   project_id, project_name, session_title, first_seen_at, last_seen_at,
                   last_model, active_duration_seconds
            FROM ai_history_file_session_link
            WHERE project_path = ?1
            ORDER BY last_seen_at DESC;
            "#,
        )?;
        let rows = statement
            .query_map(params![project_path], |row| {
                Ok(NormalizedSessionLinkRow {
                    source: row.get(0)?,
                    session_key: row.get(3)?,
                    external_session_id: row.get(4)?,
                    project_id: row.get(5)?,
                    project_name: row.get(6)?,
                    session_title: row.get(7)?,
                    first_seen_at: row.get(8)?,
                    last_seen_at: row.get(9)?,
                    last_model: row.get(10)?,
                    active_duration_seconds: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into);
        rows
    }

    fn project_usage_buckets(
        &self,
        conn: &Connection,
        project_path: &str,
    ) -> Result<Vec<StoredUsageBucketRow>> {
        let mut statement = conn.prepare(
            r#"
            SELECT source, session_key, model, bucket_start, bucket_end, input_tokens, output_tokens,
                   total_tokens, cached_input_tokens, request_count
            FROM ai_history_file_usage_bucket
            WHERE project_path = ?1
            ORDER BY bucket_start ASC, source ASC, session_key ASC, model ASC;
            "#,
        )?;
        let rows = statement
            .query_map(params![project_path], |row| {
                Ok(StoredUsageBucketRow {
                    source: row.get(0)?,
                    session_key: row.get(1)?,
                    model: normalized_optional_string(row.get::<_, String>(2)?.as_str()),
                    bucket_start: row.get(3)?,
                    bucket_end: row.get(4)?,
                    input_tokens: row.get(5)?,
                    output_tokens: row.get(6)?,
                    total_tokens: row.get(7)?,
                    cached_input_tokens: row.get(8)?,
                    request_count: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into);
        rows
    }
}

fn initialize_connection(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;

    for statement in SCHEMA_STATEMENTS {
        conn.execute_batch(statement)?;
    }

    let stored_version: Option<String> = conn
        .query_row(
            "SELECT value FROM ai_history_meta WHERE key = 'normalized_history_schema_version' LIMIT 1;",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if stored_version.as_deref() != Some(NORMALIZED_HISTORY_SCHEMA_VERSION) {
        migrate_schema(conn)?;
    }
    Ok(())
}

fn jsonl_index_mode(
    current_file_size: i64,
    current_modified_at: f64,
    stored_summary: Option<&AIExternalFileSummary>,
    checkpoint: Option<&AIExternalFileCheckpoint>,
) -> JSONLIndexMode {
    let (Some(stored_summary), Some(checkpoint)) = (stored_summary, checkpoint) else {
        return JSONLIndexMode::Rebuild;
    };
    if current_file_size < checkpoint.file_size {
        return JSONLIndexMode::Rebuild;
    }
    if checkpoint.last_offset < current_file_size {
        return JSONLIndexMode::Append;
    }
    if same_timestamp(stored_summary.file_modified_at, current_modified_at)
        && same_timestamp(checkpoint.file_modified_at, current_modified_at)
        && checkpoint.last_offset >= current_file_size
    {
        return JSONLIndexMode::Unchanged;
    }
    if current_file_size >= checkpoint.file_size && checkpoint.last_offset <= current_file_size {
        return JSONLIndexMode::Append;
    }
    JSONLIndexMode::Rebuild
}

fn merge_usage_buckets(existing: &[AIUsageBucket], delta: &[AIUsageBucket]) -> Vec<AIUsageBucket> {
    let mut map = HashMap::<(String, String, String, i64), AIUsageBucket>::new();
    for bucket in existing.iter().chain(delta.iter()) {
        let key = (
            bucket.source.clone(),
            bucket.session_key.clone(),
            bucket.model.clone().unwrap_or_default(),
            bucket.bucket_start as i64,
        );
        map.entry(key)
            .and_modify(|current| {
                current.input_tokens += bucket.input_tokens;
                current.output_tokens += bucket.output_tokens;
                current.total_tokens += bucket.total_tokens;
                current.cached_input_tokens += bucket.cached_input_tokens;
                current.request_count += bucket.request_count;
                current.active_duration_seconds += bucket.active_duration_seconds;
                current.first_seen_at = min_nonzero(current.first_seen_at, bucket.first_seen_at);
                current.last_seen_at = current.last_seen_at.max(bucket.last_seen_at);
                current.external_session_id = current
                    .external_session_id
                    .clone()
                    .or(bucket.external_session_id.clone());
                current.session_title =
                    preferred_string(Some(&current.session_title), Some(&bucket.session_title))
                        .unwrap_or_else(|| bucket.project_name.clone());
                current.model = current.model.clone().or(bucket.model.clone());
            })
            .or_insert_with(|| bucket.clone());
    }
    let mut values = map.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left.bucket_start
            .total_cmp(&right.bucket_start)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.session_key.cmp(&right.session_key))
            .then_with(|| left.model.cmp(&right.model))
    });
    values
}

fn migrate_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        DROP TABLE IF EXISTS ai_history_file_usage_bucket;
        DROP TABLE IF EXISTS ai_history_file_session_link;
        DROP TABLE IF EXISTS ai_history_file_time_bucket;
        DROP TABLE IF EXISTS ai_history_file_day_usage;
        DROP TABLE IF EXISTS ai_history_file_session;
        DROP TABLE IF EXISTS ai_history_file_checkpoint;
        DROP TABLE IF EXISTS ai_history_file_state;
        DROP TABLE IF EXISTS ai_history_project_index_state;
        "#,
    )?;
    for statement in SCHEMA_STATEMENTS {
        if statement.contains("ai_history_meta") {
            continue;
        }
        conn.execute_batch(statement)?;
    }
    conn.execute(
        r#"
        INSERT INTO ai_history_meta (key, value)
        VALUES ('normalized_history_schema_version', ?1)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value;
        "#,
        params![NORMALIZED_HISTORY_SCHEMA_VERSION],
    )?;
    Ok(())
}

fn external_file_summary_from_parsed(
    source: &str,
    file_path: String,
    file_modified_at: f64,
    file_size: i64,
    project: &AIHistoryProjectRequest,
    parsed: ParsedHistory,
) -> AIExternalFileSummary {
    let mut sessions = HashMap::<String, ParsedSessionAccumulator>::new();
    let active_duration_by_session = active_duration_by_session_id(&parsed.events);

    for event in &parsed.events {
        let session =
            sessions
                .entry(event.session_id.clone())
                .or_insert_with(|| ParsedSessionAccumulator {
                    session_key: event.session_id.clone(),
                    first_seen_at: event.timestamp,
                    last_seen_at: event.timestamp,
                    ..Default::default()
                });
        session.first_seen_at = min_nonzero(session.first_seen_at, event.timestamp);
        session.last_seen_at = session.last_seen_at.max(event.timestamp);
        session.active_duration_seconds = session.active_duration_seconds.max(
            *active_duration_by_session
                .get(&event.session_id)
                .unwrap_or(&0),
        );
    }

    for entry in &parsed.entries {
        let session =
            sessions
                .entry(entry.session_id.clone())
                .or_insert_with(|| ParsedSessionAccumulator {
                    session_key: entry.session_id.clone(),
                    first_seen_at: entry.timestamp,
                    last_seen_at: entry.timestamp,
                    ..Default::default()
                });
        session.external_session_id = entry
            .external_session_id
            .clone()
            .or(session.external_session_id.clone());
        session.title = entry.session_title.clone().or(session.title.clone());
        session.last_model = entry.model.clone().or(session.last_model.clone());
        session.first_seen_at = min_nonzero(session.first_seen_at, entry.timestamp);
        session.last_seen_at = session.last_seen_at.max(entry.timestamp);
        session.active_duration_seconds = session.active_duration_seconds.max(
            *active_duration_by_session
                .get(&entry.session_id)
                .unwrap_or(&0),
        );
    }

    let mut buckets = HashMap::<(String, String, i64), AIUsageBucket>::new();
    for entry in &parsed.entries {
        let model = entry.model.clone().unwrap_or_else(|| "unknown".to_string());
        let bucket_start = half_hour_bucket_start(entry.timestamp);
        let session = sessions
            .entry(entry.session_id.clone())
            .or_insert_with(|| parsed_session_from_entry(entry));
        let bucket = buckets
            .entry((entry.session_id.clone(), model.clone(), bucket_start as i64))
            .or_insert_with(|| {
                usage_bucket_from_session(source, session, project, &model, bucket_start)
            });
        bucket.input_tokens += entry.input_tokens;
        bucket.output_tokens += entry.output_tokens;
        bucket.total_tokens += entry.total_tokens();
        bucket.cached_input_tokens += entry.cached_input_tokens;
    }

    for event in &parsed.events {
        if event.role != HistoryRole::User {
            continue;
        }
        let bucket_start = half_hour_bucket_start(event.timestamp);
        let session = sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| parsed_session_from_event(event));
        let model = session
            .last_model
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let bucket = buckets
            .entry((event.session_id.clone(), model.clone(), bucket_start as i64))
            .or_insert_with(|| {
                usage_bucket_from_session(source, session, project, &model, bucket_start)
            });
        bucket.request_count += 1;
    }

    let mut usage_buckets = buckets.into_values().collect::<Vec<_>>();
    usage_buckets.sort_by(|left, right| {
        left.bucket_start
            .total_cmp(&right.bucket_start)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.session_key.cmp(&right.session_key))
            .then_with(|| left.model.cmp(&right.model))
    });

    AIExternalFileSummary {
        source: source.to_string(),
        file_path,
        file_modified_at,
        file_size,
        project_path: project.path.clone(),
        usage_buckets,
    }
}

fn parsed_session_from_entry(entry: &HistoryEntry) -> ParsedSessionAccumulator {
    ParsedSessionAccumulator {
        session_key: entry.session_id.clone(),
        external_session_id: entry.external_session_id.clone(),
        title: entry.session_title.clone(),
        first_seen_at: entry.timestamp,
        last_seen_at: entry.timestamp,
        last_model: entry.model.clone(),
        active_duration_seconds: 0,
    }
}

fn parsed_session_from_event(event: &HistoryEvent) -> ParsedSessionAccumulator {
    ParsedSessionAccumulator {
        session_key: event.session_id.clone(),
        first_seen_at: event.timestamp,
        last_seen_at: event.timestamp,
        ..Default::default()
    }
}

fn usage_bucket_from_session(
    source: &str,
    session: &ParsedSessionAccumulator,
    project: &AIHistoryProjectRequest,
    model: &str,
    bucket_start: f64,
) -> AIUsageBucket {
    AIUsageBucket {
        source: source.to_string(),
        session_key: session.session_key.clone(),
        external_session_id: session.external_session_id.clone(),
        session_title: session
            .title
            .clone()
            .unwrap_or_else(|| project.name.clone()),
        model: Some(model.to_string()),
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        bucket_start,
        bucket_end: bucket_start + 30.0 * 60.0,
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        cached_input_tokens: 0,
        request_count: 0,
        active_duration_seconds: session.active_duration_seconds,
        first_seen_at: session.first_seen_at,
        last_seen_at: session.last_seen_at,
    }
}

fn build_session_links(usage_buckets: &[AIUsageBucket]) -> Vec<NormalizedSessionLinkRow> {
    let mut map = HashMap::<String, NormalizedSessionLinkRow>::new();
    for bucket in usage_buckets {
        map.entry(bucket.session_key.clone())
            .and_modify(|session| {
                session.external_session_id = session
                    .external_session_id
                    .clone()
                    .or(bucket.external_session_id.clone());
                session.session_title =
                    preferred_string(Some(&session.session_title), Some(&bucket.session_title))
                        .unwrap_or_else(|| bucket.project_name.clone());
                session.first_seen_at = min_nonzero(session.first_seen_at, bucket.first_seen_at);
                session.last_seen_at = session.last_seen_at.max(bucket.last_seen_at);
                session.last_model = bucket.model.clone().or(session.last_model.clone());
                session.active_duration_seconds = session
                    .active_duration_seconds
                    .max(bucket.active_duration_seconds);
            })
            .or_insert_with(|| NormalizedSessionLinkRow {
                source: bucket.source.clone(),
                session_key: bucket.session_key.clone(),
                external_session_id: bucket.external_session_id.clone(),
                project_id: bucket.project_id.clone(),
                project_name: bucket.project_name.clone(),
                session_title: preferred_string(Some(&bucket.session_title), None)
                    .unwrap_or_else(|| bucket.project_name.clone()),
                first_seen_at: bucket.first_seen_at,
                last_seen_at: bucket.last_seen_at,
                last_model: bucket.model.clone(),
                active_duration_seconds: bucket.active_duration_seconds,
            });
    }
    let mut values = map.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .last_seen_at
            .total_cmp(&left.last_seen_at)
            .then_with(|| left.session_key.cmp(&right.session_key))
    });
    values
}

fn build_snapshot_from_rows(
    project: AIHistoryProjectRequest,
    links: Vec<NormalizedSessionLinkRow>,
    buckets: Vec<StoredUsageBucketRow>,
) -> AIHistorySnapshot {
    let today_start = local_day_start_seconds(now_seconds());
    let mut sessions_by_key = HashMap::<String, PersistedSessionAccumulator>::new();
    let mut tool_breakdown = HashMap::<String, AIUsageBreakdownItem>::new();
    let mut model_breakdown = HashMap::<String, AIUsageBreakdownItem>::new();
    let mut heatmap = HashMap::<i64, AIHeatmapDay>::new();
    let mut time_buckets = HashMap::<i64, AITimeBucket>::new();
    let mut project_total_tokens = 0;
    let mut project_cached_input_tokens = 0;
    let mut today_total_tokens = 0;
    let mut today_cached_input_tokens = 0;
    let link_group_keys = links
        .iter()
        .map(|link| {
            (
                history_key(&link.source, &link.session_key),
                history_group_key(
                    &link.source,
                    &link.session_key,
                    link.external_session_id.as_deref(),
                ),
            )
        })
        .collect::<HashMap<_, _>>();

    for link in &links {
        let key = history_group_key(
            &link.source,
            &link.session_key,
            link.external_session_id.as_deref(),
        );
        let session = sessions_by_key
            .entry(key)
            .or_insert_with(|| PersistedSessionAccumulator {
                source: link.source.clone(),
                session_key: link.session_key.clone(),
                external_session_id: link.external_session_id.clone(),
                title: Some(link.session_title.clone()),
                first_seen_at: link.first_seen_at,
                last_seen_at: link.last_seen_at,
                last_model: link.last_model.clone(),
                active_duration_seconds: link.active_duration_seconds,
                ..Default::default()
            });
        session.external_session_id = session
            .external_session_id
            .clone()
            .or(link.external_session_id.clone());
        session.title = preferred_string(session.title.as_deref(), Some(&link.session_title));
        session.first_seen_at = min_nonzero(session.first_seen_at, link.first_seen_at);
        session.last_seen_at = session.last_seen_at.max(link.last_seen_at);
        if link.last_seen_at >= session.last_seen_at {
            session.last_model = link.last_model.clone().or(session.last_model.clone());
        }
        session.active_duration_seconds = session
            .active_duration_seconds
            .max(link.active_duration_seconds);
    }

    for bucket in buckets {
        let raw_key = history_key(&bucket.source, &bucket.session_key);
        let key = link_group_keys
            .get(&raw_key)
            .cloned()
            .unwrap_or_else(|| raw_key.clone());
        let session = sessions_by_key
            .entry(key)
            .or_insert_with(|| PersistedSessionAccumulator {
                source: bucket.source.clone(),
                session_key: bucket.session_key.clone(),
                first_seen_at: bucket.bucket_start,
                last_seen_at: bucket.bucket_end,
                ..Default::default()
            });
        session.input_tokens += bucket.input_tokens;
        session.output_tokens += bucket.output_tokens;
        session.total_tokens += bucket.total_tokens;
        session.cached_input_tokens += bucket.cached_input_tokens;
        session.request_count += bucket.request_count;
        session.first_seen_at = min_nonzero(session.first_seen_at, bucket.bucket_start);
        session.last_seen_at = session.last_seen_at.max(bucket.bucket_end);
        session.last_model = bucket.model.clone().or(session.last_model.clone());
        if bucket.bucket_start >= today_start {
            session.today_tokens += bucket.total_tokens;
            session.today_cached_input_tokens += bucket.cached_input_tokens;
        }

        project_total_tokens += bucket.total_tokens;
        project_cached_input_tokens += bucket.cached_input_tokens;
        if bucket.bucket_start >= today_start {
            today_total_tokens += bucket.total_tokens;
            today_cached_input_tokens += bucket.cached_input_tokens;
        }

        accumulate_breakdown(
            &mut tool_breakdown,
            &bucket.source,
            bucket.total_tokens,
            bucket.cached_input_tokens,
            bucket.request_count,
        );
        if let Some(model) = displayable_model_name(bucket.model.as_deref()) {
            accumulate_breakdown(
                &mut model_breakdown,
                model,
                bucket.total_tokens,
                bucket.cached_input_tokens,
                bucket.request_count,
            );
        }

        let day = local_day_start_seconds(bucket.bucket_start);
        let heatmap_day = heatmap.entry(day as i64).or_insert(AIHeatmapDay {
            day,
            total_tokens: 0,
            cached_input_tokens: 0,
            request_count: 0,
        });
        heatmap_day.total_tokens += bucket.total_tokens;
        heatmap_day.cached_input_tokens += bucket.cached_input_tokens;
        heatmap_day.request_count += bucket.request_count;

        if bucket.bucket_start >= today_start {
            let item = time_buckets
                .entry(bucket.bucket_start as i64)
                .or_insert(AITimeBucket {
                    start: bucket.bucket_start,
                    end: bucket.bucket_end,
                    total_tokens: 0,
                    cached_input_tokens: 0,
                    request_count: 0,
                });
            item.total_tokens += bucket.total_tokens;
            item.cached_input_tokens += bucket.cached_input_tokens;
            item.request_count += bucket.request_count;
        }
    }

    let mut sessions = sessions_by_key
        .into_values()
        .filter(|session| {
            session.total_tokens + session.cached_input_tokens + session.request_count > 0
        })
        .map(|session| AISessionSummary {
            session_id: deterministic_uuid(&history_key(&session.source, &session.session_key)),
            external_session_id: session.external_session_id,
            project_id: project.id.clone(),
            project_name: project.name.clone(),
            project_path: project.path.clone(),
            session_title: session.title.unwrap_or_else(|| project.name.clone()),
            first_seen_at: session.first_seen_at,
            last_seen_at: session.last_seen_at,
            last_tool: Some(session.source),
            last_model: session.last_model,
            request_count: session.request_count,
            total_input_tokens: session.input_tokens,
            total_output_tokens: session.output_tokens,
            total_tokens: session.total_tokens,
            cached_input_tokens: session.cached_input_tokens,
            active_duration_seconds: session.active_duration_seconds.max(
                (session.last_seen_at - session.first_seen_at)
                    .max(0.0)
                    .round() as i64,
            ),
            today_tokens: session.today_tokens,
            today_cached_input_tokens: session.today_cached_input_tokens,
        })
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.last_seen_at.total_cmp(&left.last_seen_at));

    let latest_session = sessions.first().cloned();
    sessions.truncate(RECENT_HISTORY_SESSION_LIMIT);
    AIHistorySnapshot {
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        project_summary: AIProjectUsageSummary {
            project_id: project.id,
            project_name: project.name,
            current_session_tokens: latest_session
                .as_ref()
                .map(|session| session.total_tokens)
                .unwrap_or(0),
            current_session_cached_input_tokens: latest_session
                .as_ref()
                .map(|session| session.cached_input_tokens)
                .unwrap_or(0),
            project_total_tokens,
            project_cached_input_tokens,
            today_total_tokens,
            today_cached_input_tokens,
            current_tool: latest_session
                .as_ref()
                .and_then(|session| session.last_tool.clone()),
            current_model: latest_session
                .as_ref()
                .and_then(|session| session.last_model.clone()),
            current_session_updated_at: latest_session.as_ref().map(|session| session.last_seen_at),
        },
        sessions,
        heatmap: sorted_values(heatmap),
        today_time_buckets: fixed_today_time_buckets(time_buckets),
        tool_breakdown: sorted_breakdown(tool_breakdown),
        model_breakdown: sorted_breakdown(model_breakdown),
        indexed_at: now_seconds(),
    }
}

fn accumulate_breakdown(
    map: &mut HashMap<String, AIUsageBreakdownItem>,
    key: &str,
    total_tokens: i64,
    cached_input_tokens: i64,
    request_count: i64,
) {
    let item = map.entry(key.to_string()).or_insert(AIUsageBreakdownItem {
        key: key.to_string(),
        total_tokens: 0,
        cached_input_tokens: 0,
        request_count: 0,
    });
    item.total_tokens += total_tokens;
    item.cached_input_tokens += cached_input_tokens;
    item.request_count += request_count;
}

fn sorted_breakdown(mut map: HashMap<String, AIUsageBreakdownItem>) -> Vec<AIUsageBreakdownItem> {
    let mut values = map.drain().map(|(_, value)| value).collect::<Vec<_>>();
    values.sort_by(|left, right| right.total_tokens.cmp(&left.total_tokens));
    values
}

fn sorted_values<T>(map: HashMap<i64, T>) -> Vec<T> {
    let mut entries = map.into_iter().collect::<Vec<_>>();
    entries.sort_by_key(|(key, _)| *key);
    entries.into_iter().map(|(_, value)| value).collect()
}

fn fixed_today_time_buckets(mut map: HashMap<i64, AITimeBucket>) -> Vec<AITimeBucket> {
    let today_start = local_day_start_seconds(now_seconds());
    (0..48)
        .map(|index| {
            let start = today_start + f64::from(index) * 30.0 * 60.0;
            map.remove(&(start as i64)).unwrap_or(AITimeBucket {
                start,
                end: start + 30.0 * 60.0,
                total_tokens: 0,
                cached_input_tokens: 0,
                request_count: 0,
            })
        })
        .collect()
}

fn matching_session_keys(
    links: &[NormalizedSessionLinkRow],
    session_id: &str,
) -> Vec<(String, String)> {
    let mut matched = Vec::new();
    for link in links {
        let raw_id = deterministic_uuid(&history_key(&link.source, &link.session_key));
        let grouped_id = deterministic_uuid(&history_group_key(
            &link.source,
            &link.session_key,
            link.external_session_id.as_deref(),
        ));
        if session_id == raw_id || session_id == grouped_id {
            let key = (link.source.clone(), link.session_key.clone());
            if !matched.contains(&key) {
                matched.push(key);
            }
        }
    }
    matched
}

fn history_group_key(source: &str, session_key: &str, external_session_id: Option<&str>) -> String {
    history_key(source, external_session_id.unwrap_or(session_key))
}

fn active_duration_by_session_id(events: &[HistoryEvent]) -> HashMap<String, i64> {
    let mut grouped = HashMap::<String, Vec<&HistoryEvent>>::new();
    for event in events {
        grouped
            .entry(event.session_id.clone())
            .or_default()
            .push(event);
    }

    let mut result = HashMap::new();
    for (session_id, mut events) in grouped {
        events.sort_by(|left, right| left.timestamp.total_cmp(&right.timestamp));
        let (Some(first), Some(last)) = (events.first(), events.last()) else {
            continue;
        };
        let wall_clock_seconds = (last.timestamp - first.timestamp).max(0.0).round() as i64;
        let mut active_seconds = 0i64;
        let mut waiting_for_first_response = false;
        let mut turn_start: Option<f64> = None;
        let mut turn_end: Option<f64> = None;

        for event in events {
            match event.role {
                HistoryRole::User => {
                    if let (Some(start), Some(end)) = (turn_start, turn_end) {
                        if end > start {
                            active_seconds = active_seconds
                                .saturating_add((end - start).max(0.0).round() as i64)
                                .min(wall_clock_seconds);
                        }
                    }
                    turn_start = None;
                    turn_end = None;
                    waiting_for_first_response = true;
                }
                HistoryRole::Assistant => {
                    if waiting_for_first_response {
                        turn_start = Some(event.timestamp);
                        turn_end = Some(event.timestamp);
                        waiting_for_first_response = false;
                    } else if turn_start.is_some() {
                        turn_end = Some(event.timestamp);
                    }
                }
            }
        }
        if let (Some(start), Some(end)) = (turn_start, turn_end) {
            if end > start {
                active_seconds = active_seconds
                    .saturating_add((end - start).max(0.0).round() as i64)
                    .min(wall_clock_seconds);
            }
        }
        result.insert(session_id, active_seconds.min(wall_clock_seconds));
    }
    result
}

fn min_nonzero(left: f64, right: f64) -> f64 {
    if left <= 0.0 {
        right
    } else {
        left.min(right)
    }
}

fn preferred_string(left: Option<&str>, right: Option<&str>) -> Option<String> {
    normalized_optional_string(left.unwrap_or(""))
        .or_else(|| normalized_optional_string(right.unwrap_or("")))
}

fn normalized_optional_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn displayable_model_name(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") {
        return None;
    }
    Some(value)
}

fn same_timestamp(left: f64, right: f64) -> bool {
    (left - right).abs() < 0.000_001
}

fn normalized_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn modified_seconds(metadata: &fs::Metadata) -> f64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(duration_seconds)
        .unwrap_or(0.0)
}

fn duration_seconds(duration: std::time::Duration) -> f64 {
    duration.as_secs() as f64 + f64::from(duration.subsec_micros()) / 1_000_000.0
}

fn default_database_path() -> PathBuf {
    app_support_dir().join("ai-usage.sqlite3")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_history::JSONLParseSnapshot;
    use chrono::TimeZone;
    use uuid::Uuid;

    #[test]
    fn initializes_normalized_schema() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();

        let version: String = conn
            .query_row(
                "SELECT value FROM ai_history_meta WHERE key = 'normalized_history_schema_version';",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, NORMALIZED_HISTORY_SCHEMA_VERSION);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unchanged_file_reuses_persisted_summary() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "{}\n").unwrap();
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        let mut parse_count = 0;

        let first = store
            .load_or_index_file(&conn, "claude", &file_path, &project, || {
                parse_count += 1;
                parsed_history("s1", 100.0, 100, 50)
            })
            .unwrap();
        let second = store
            .load_or_index_file(&conn, "claude", &file_path, &project, || {
                parse_count += 1;
                ParsedHistory::default()
            })
            .unwrap();

        assert_eq!(parse_count, 1);
        assert_eq!(first.usage_buckets.len(), 1);
        assert_eq!(second.usage_buckets.len(), 1);
        assert_eq!(second.usage_buckets[0].total_tokens, 150);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn persists_half_hour_bucket_and_project_snapshot() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "{}\n").unwrap();
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        let timestamp = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 10, 42, 0)
            .single()
            .unwrap()
            .timestamp() as f64;

        store
            .load_or_index_file(&conn, "codex", &file_path, &project, || {
                parsed_history("s2", timestamp, 80, 20)
            })
            .unwrap();
        let stored = store
            .stored_external_summary(
                &conn,
                "codex",
                &normalized_path(&file_path),
                &project.path,
                None,
            )
            .unwrap()
            .unwrap();
        let project_path = project.path.clone();
        let snapshot = store.project_snapshot(&conn, project.clone()).unwrap();

        assert_eq!(
            stored.usage_buckets[0].bucket_start,
            half_hour_bucket_start(timestamp)
        );
        assert_eq!(snapshot.project_summary.project_total_tokens, 100);
        assert_eq!(snapshot.today_time_buckets.len(), 48);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert_eq!(snapshot.tool_breakdown[0].key, "codex");
        store
            .save_project_index_state(&conn, &snapshot, &project_path)
            .unwrap();
        assert!(store
            .indexed_project_snapshot(&conn, project)
            .unwrap()
            .is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn jsonl_append_indexes_only_new_bytes() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "one\n").unwrap();
        let initial_size = fs::metadata(&file_path).unwrap().len() as i64;
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        let mut rebuild_count = 0;
        let mut append_count = 0;

        store
            .load_or_index_jsonl_file(
                &conn,
                "codex",
                &file_path,
                &project,
                |_| {
                    append_count += 1;
                    JSONLParseSnapshot::default()
                },
                || {
                    rebuild_count += 1;
                    JSONLParseSnapshot {
                        result: parsed_history("s1", 100.0, 10, 10),
                        last_processed_offset: initial_size,
                        payload_json: None,
                    }
                },
            )
            .unwrap();
        fs::write(&file_path, "one\ntwo\n").unwrap();
        let updated_size = fs::metadata(&file_path).unwrap().len() as i64;
        let summary = store
            .load_or_index_jsonl_file(
                &conn,
                "codex",
                &file_path,
                &project,
                |checkpoint| {
                    append_count += 1;
                    assert_eq!(checkpoint.unwrap().last_offset, initial_size);
                    JSONLParseSnapshot {
                        result: parsed_history("s1", 200.0, 20, 20),
                        last_processed_offset: updated_size,
                        payload_json: None,
                    }
                },
                || {
                    rebuild_count += 1;
                    JSONLParseSnapshot::default()
                },
            )
            .unwrap();

        assert_eq!(rebuild_count, 1);
        assert_eq!(append_count, 1);
        assert_eq!(
            summary
                .usage_buckets
                .iter()
                .map(|bucket| bucket.total_tokens)
                .sum::<i64>(),
            60
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn jsonl_truncation_promotes_to_rebuild() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "one\ntwo\n").unwrap();
        let initial_size = fs::metadata(&file_path).unwrap().len() as i64;
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        let mut rebuild_count = 0;
        let mut append_count = 0;

        store
            .load_or_index_jsonl_file(
                &conn,
                "claude",
                &file_path,
                &project,
                |_| {
                    append_count += 1;
                    JSONLParseSnapshot::default()
                },
                || {
                    rebuild_count += 1;
                    JSONLParseSnapshot {
                        result: parsed_history("s1", 100.0, 10, 10),
                        last_processed_offset: initial_size,
                        payload_json: None,
                    }
                },
            )
            .unwrap();
        fs::write(&file_path, "one\n").unwrap();
        let truncated_size = fs::metadata(&file_path).unwrap().len() as i64;
        let summary = store
            .load_or_index_jsonl_file(
                &conn,
                "claude",
                &file_path,
                &project,
                |_| {
                    append_count += 1;
                    JSONLParseSnapshot::default()
                },
                || {
                    rebuild_count += 1;
                    JSONLParseSnapshot {
                        result: parsed_history("s1", 300.0, 30, 30),
                        last_processed_offset: truncated_size,
                        payload_json: None,
                    }
                },
            )
            .unwrap();

        assert_eq!(rebuild_count, 2);
        assert_eq!(append_count, 0);
        assert_eq!(summary.usage_buckets[0].total_tokens, 60);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_snapshot_groups_sessions_by_external_session_id() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let first_path = root.join("first.jsonl");
        let second_path = root.join("second.jsonl");
        fs::write(&first_path, "{}\n").unwrap();
        fs::write(&second_path, "{}\n").unwrap();
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);

        store
            .load_or_index_file(&conn, "opencode", &first_path, &project, || {
                parsed_history_with_external("file-session-1", "external-1", 100.0, 10, 10)
            })
            .unwrap();
        store
            .load_or_index_file(&conn, "opencode", &second_path, &project, || {
                parsed_history_with_external("file-session-2", "external-1", 200.0, 20, 20)
            })
            .unwrap();
        let snapshot = store.project_snapshot(&conn, project).unwrap();

        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(
            snapshot.sessions[0].external_session_id.as_deref(),
            Some("external-1")
        );
        assert_eq!(snapshot.sessions[0].total_tokens, 60);
        let _ = fs::remove_dir_all(root);
    }

    fn test_project(root: &Path) -> AIHistoryProjectRequest {
        AIHistoryProjectRequest {
            id: "project-1".to_string(),
            name: "Project".to_string(),
            path: root.to_string_lossy().to_string(),
        }
    }

    fn parsed_history(
        session_id: &str,
        timestamp: f64,
        input_tokens: i64,
        output_tokens: i64,
    ) -> ParsedHistory {
        parsed_history_with_external(
            session_id,
            session_id,
            timestamp,
            input_tokens,
            output_tokens,
        )
    }

    fn parsed_history_with_external(
        session_id: &str,
        external_session_id: &str,
        timestamp: f64,
        input_tokens: i64,
        output_tokens: i64,
    ) -> ParsedHistory {
        ParsedHistory {
            events: vec![HistoryEvent {
                source: "claude".to_string(),
                session_id: session_id.to_string(),
                timestamp,
                role: HistoryRole::User,
            }],
            entries: vec![HistoryEntry {
                source: "claude".to_string(),
                session_id: session_id.to_string(),
                external_session_id: Some(external_session_id.to_string()),
                session_title: Some("Session".to_string()),
                timestamp: timestamp + 60.0,
                model: Some("model-a".to_string()),
                input_tokens,
                output_tokens,
                cached_input_tokens: 5,
                reasoning_output_tokens: 0,
            }],
        }
    }
}
