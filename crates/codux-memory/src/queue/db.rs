use super::types::MemoryExtractionTask;
use rusqlite::{OptionalExtension, params};

pub(super) fn queue_count(conn: &rusqlite::Connection, status: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM memory_extraction_queue WHERE status = ?1;",
        params![status],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

/// Pending and running counts in a single scan -- the 300ms status poller
/// previously issued these as two separate `COUNT(*)` queries.
pub(super) fn queue_pending_running_counts(
    conn: &rusqlite::Connection,
) -> Result<(i64, i64), String> {
    conn.query_row(
        r#"
        SELECT
            COALESCE(SUM(status = 'pending'), 0),
            COALESCE(SUM(status = 'running'), 0)
        FROM memory_extraction_queue;
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|error| error.to_string())
}

pub(super) fn latest_failed_error(conn: &rusqlite::Connection) -> Result<Option<String>, String> {
    conn.query_row(
        r#"
        SELECT error
        FROM memory_extraction_queue
        WHERE status = 'failed' AND error IS NOT NULL AND error != ''
        ORDER BY enqueued_at DESC
        LIMIT 1;
        "#,
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub(super) fn failed_extraction_tasks(
    conn: &rusqlite::Connection,
    project_id: Option<&str>,
    limit: i64,
) -> Result<Vec<MemoryExtractionTask>, String> {
    let limit = limit.clamp(1, 1000);
    let mut statement = conn
        .prepare(
            r#"
            SELECT id, project_id, tool, session_id, transcript_path, workspace_path, source_fingerprint, status, attempts, error, enqueued_at
            FROM memory_extraction_queue
            WHERE status = 'failed'
              AND (?1 IS NULL OR project_id = ?1)
            ORDER BY enqueued_at DESC
            LIMIT ?2;
            "#,
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![project_id, limit], memory_task_from_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(super) fn active_extraction_tasks(
    conn: &rusqlite::Connection,
    project_id: Option<&str>,
    limit: i64,
) -> Result<Vec<MemoryExtractionTask>, String> {
    let limit = limit.clamp(1, 1000);
    let mut statement = conn
        .prepare(
            r#"
            SELECT id, project_id, tool, session_id, transcript_path, workspace_path, source_fingerprint, status, attempts, error, enqueued_at
            FROM memory_extraction_queue
            WHERE status IN ('queued', 'pending', 'running')
              AND (?1 IS NULL OR project_id = ?1)
            ORDER BY
              CASE status WHEN 'running' THEN 0 ELSE 1 END,
              enqueued_at ASC
            LIMIT ?2;
            "#,
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![project_id, limit], memory_task_from_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(super) fn memory_task_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MemoryExtractionTask> {
    Ok(MemoryExtractionTask {
        id: row.get(0)?,
        project_id: row.get(1)?,
        tool: row.get(2)?,
        session_id: row.get(3)?,
        transcript_path: row.get(4)?,
        workspace_path: row.get(5)?,
        source_fingerprint: row.get(6)?,
        status: row.get(7)?,
        attempts: row.get(8)?,
        error: row.get(9)?,
        enqueued_at: row.get(10)?,
    })
}
