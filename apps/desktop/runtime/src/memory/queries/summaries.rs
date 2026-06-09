pub(super) fn list_summaries_for_management(
    conn: &Connection,
    scope: &str,
    project_id: Option<&str>,
) -> Result<Vec<MemorySummaryRow>, String> {
    let mut statement = conn
        .prepare(
            r#"
            SELECT id, scope, project_id, tool_id, content, version, source_entry_ids,
                   token_estimate, created_at, updated_at
            FROM memory_summaries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '')
            ORDER BY updated_at DESC
            "#,
        )
        .map_err(|error| error.to_string())?;
    let project_id = if normalize_scope(scope) == "project" {
        project_id
    } else {
        None
    };
    let rows = statement
        .query_map(params![normalize_scope(scope), project_id], |row| {
            let source_ids: Option<String> = row.get(6)?;
            Ok(MemorySummaryRow {
                id: row.get(0)?,
                scope: row.get(1)?,
                project_id: row.get(2)?,
                tool_id: row.get(3)?,
                content: row.get(4)?,
                version: row.get(5)?,
                source_entry_ids: decode_string_array(source_ids.as_deref()),
                token_estimate: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
