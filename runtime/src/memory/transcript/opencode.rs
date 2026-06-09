fn fetch_opencode_transcript(
    project_path: &str,
    external_session_id: &str,
    database_path: &str,
    line_limit: i32,
    token_limit: i32,
) -> Option<String> {
    let conn = Connection::open(database_path).ok()?;
    let mut statement = conn
        .prepare(
            r#"
            SELECT json_extract(m.data, '$.role') AS role,
                   COALESCE(json_extract(m.data, '$.time.created'), '') AS created_at,
                   COALESCE(json_extract(m.data, '$.content'), json_extract(p.data, '$.text'), json_extract(p.data, '$.state.output'), '') AS content,
                   COALESCE(json_extract(m.data, '$.path.root'), s.directory, '') AS root_path,
                   COALESCE(json_extract(p.data, '$.type'), '') AS part_type,
                   COALESCE(json_extract(p.data, '$.tool'), '') AS tool_name
            FROM session s
            JOIN message m ON m.session_id = s.id
            LEFT JOIN part p ON p.message_id = m.id
            WHERE s.id = ?1
              AND s.time_archived IS NULL
            ORDER BY m.time_created ASC, p.time_created ASC;
            "#,
        )
        .ok()?;
    let rows = statement
        .query_map(params![external_session_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .ok()?;
    let mut lines = Vec::new();
    for row in rows.flatten() {
        let (role, created_at, content, root_path, part_type, tool_name) = row;
        if !paths_equivalent(root_path.as_deref(), project_path) {
            continue;
        }
        let Some(content) = content.and_then(|value| normalized_string(Some(&value))) else {
            continue;
        };
        let role = role.unwrap_or_else(|| "assistant".to_string());
        let prefix = if part_type.as_deref() == Some("tool") {
            format!("{}.tool[{}]", role, tool_name.unwrap_or_default())
        } else {
            role
        };
        lines.push(format!(
            "[{}] {}: {}",
            created_at.unwrap_or_default(),
            prefix,
            content
        ));
    }
    let text = lines
        .into_iter()
        .rev()
        .take(line_limit.max(1) as usize)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    compact_transcript_for_memory(&text, token_limit)
}

fn opencode_database_path() -> PathBuf {
    home_dir()
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db")
}
