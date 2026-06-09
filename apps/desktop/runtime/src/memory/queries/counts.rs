pub(super) fn count_entries(
    conn: &Connection,
    tier: Option<&str>,
    project_id: Option<&str>,
    status: Option<&str>,
) -> Result<i64, String> {
    let mut sql = "SELECT COUNT(*) FROM memory_entries WHERE 1=1".to_string();
    let mut values = Vec::new();
    if let Some(tier) = tier {
        sql.push_str(" AND tier = ?");
        values.push(tier.to_string());
    }
    if let Some(project_id) = project_id {
        sql.push_str(" AND (project_id = ? OR scope = 'user')");
        values.push(project_id.to_string());
    }
    if let Some(status) = status {
        sql.push_str(" AND status = ?");
        values.push(status.to_string());
    }

    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    statement
        .query_row(rusqlite::params_from_iter(values), |row| row.get(0))
        .map_err(|error| error.to_string())
}

pub(super) fn count_summaries(conn: &Connection, project_id: Option<&str>) -> Result<i64, String> {
    if let Some(project_id) = project_id {
        conn.query_row(
            "SELECT COUNT(*) FROM memory_summaries WHERE project_id = ?1 OR scope = 'user'",
            [project_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
    } else {
        conn.query_row("SELECT COUNT(*) FROM memory_summaries", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())
    }
}

pub(super) fn count_queue(conn: &Connection, statuses: &[&str]) -> Result<i64, String> {
    let placeholders = std::iter::repeat_n("?", statuses.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql =
        format!("SELECT COUNT(*) FROM memory_extraction_queue WHERE status IN ({placeholders})");
    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    statement
        .query_row(
            rusqlite::params_from_iter(statuses.iter().copied()),
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

pub(super) fn latest_failed_queue_error(conn: &Connection) -> Result<Option<String>, String> {
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
