pub(super) fn current_project_profile(
    conn: &Connection,
    project_id: &str,
) -> Result<Option<MemoryProjectProfileSummary>, String> {
    conn.query_row(
        r#"
        SELECT project_id, content, source_fingerprint, created_at, updated_at
        FROM memory_project_profiles
        WHERE project_id = ?1
        LIMIT 1
        "#,
        params![project_id],
        |row| {
            Ok(MemoryProjectProfileSummary {
                project_id: row.get(0)?,
                content: row.get(1)?,
                source_fingerprint: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}
