/// Entries injected into the AI CLI launch context. Only `active` entries are
/// injected — `archived`/`merged` rows are explicitly superseded/stale and must
/// not be fed to the live agent. Ranking prefers important and frequently-used
/// memory over merely-recent: tier priority (core, then working), then
/// access_count, then recency.
pub(super) fn load_recent_entries(
    conn: &Connection,
    project_id: Option<&str>,
    include_user_recall: bool,
) -> Result<Vec<MemoryEntrySummary>, String> {
    const SELECT: &str = r#"
            SELECT id, scope, project_id, tool_id, tier, kind, COALESCE(module_key, 'general'),
                   status, content, rationale, source_tool, source_session_id, merged_summary_id,
                   archived_at, access_count, created_at, updated_at
            FROM memory_entries
            WHERE status = 'active' AND "#;
    const ORDER: &str = r#"
            ORDER BY CASE tier WHEN 'core' THEN 0 WHEN 'working' THEN 1 ELSE 2 END,
                     access_count DESC, updated_at DESC
            LIMIT 16
            "#;
    let (where_clause, values): (&str, Vec<rusqlite::types::Value>) = match project_id {
        Some(project_id) if include_user_recall => (
            "(project_id = ?1 OR scope = 'user')",
            vec![rusqlite::types::Value::Text(project_id.to_string())],
        ),
        Some(project_id) => (
            "project_id = ?1",
            vec![rusqlite::types::Value::Text(project_id.to_string())],
        ),
        None => ("1 = 1", Vec::new()),
    };
    let sql = format!("{SELECT}{where_clause}{ORDER}");

    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(values), |row| {
            memory_entry_summary_from_row(row, true)
        })
        .map_err(|error| error.to_string())?;

    let mut entries = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    attach_latest_memory_entry_decisions(conn, &mut entries)?;
    Ok(entries)
}

pub(super) fn list_entries_for_management(
    conn: &Connection,
    scope: &str,
    project_id: Option<&str>,
    tier: Option<&str>,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<MemoryEntrySummary>, String> {
    let mut clauses = vec![
        "scope = ?".to_string(),
        "COALESCE(project_id, '') = COALESCE(?, '')".to_string(),
    ];
    let mut values = vec![
        rusqlite::types::Value::Text(normalize_scope(scope).to_string()),
        optional_sql_text(if normalize_scope(scope) == "project" {
            project_id
        } else {
            None
        }),
    ];
    if let Some(tier) = tier {
        clauses.push("tier = ?".to_string());
        values.push(rusqlite::types::Value::Text(tier.to_string()));
    }
    if let Some(status) = status {
        if status == "archived" {
            clauses.push("status IN ('archived', 'merged')".to_string());
        } else {
            clauses.push("status = ?".to_string());
            values.push(rusqlite::types::Value::Text(status.to_string()));
        }
    }
    values.push(rusqlite::types::Value::Integer(limit));
    let sql = format!(
        r#"
        SELECT {}
        FROM memory_entries
        WHERE {}
        ORDER BY updated_at DESC, created_at DESC
        LIMIT ?
        "#,
        entry_select_columns(),
        clauses.join(" AND ")
    );
    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(values), |row| {
            memory_entry_summary_from_row(row, false)
        })
        .map_err(|error| error.to_string())?;
    let mut entries = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    attach_latest_memory_entry_decisions(conn, &mut entries)?;
    Ok(entries)
}

fn memory_entry_summary_from_row(
    row: &rusqlite::Row<'_>,
    truncate_content: bool,
) -> rusqlite::Result<MemoryEntrySummary> {
    let content = row.get::<_, String>(8)?;
    Ok(MemoryEntrySummary {
        id: row.get(0)?,
        scope: row.get(1)?,
        project_id: row.get(2)?,
        tool_id: row.get(3)?,
        tier: row.get(4)?,
        kind: row.get(5)?,
        module_key: row.get(6)?,
        status: row.get(7)?,
        content: if truncate_content {
            truncate(content, 96)
        } else {
            content
        },
        rationale: row.get(9)?,
        source_tool: row.get(10)?,
        source_session_id: row.get(11)?,
        merged_summary_id: row.get(12)?,
        archived_at: row.get(13)?,
        access_count: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        last_decision: None,
    })
}

fn entry_select_columns() -> &'static str {
    "id, scope, project_id, tool_id, tier, kind, COALESCE(module_key, 'general'), status, content, rationale, source_tool, source_session_id, merged_summary_id, archived_at, access_count, created_at, updated_at"
}

fn attach_latest_memory_entry_decisions(
    conn: &Connection,
    entries: &mut [MemoryEntrySummary],
) -> Result<(), String> {
    const ENTRY_IDS_PER_QUERY: usize = 300;
    let entry_indices = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.id.clone(), index))
        .collect::<HashMap<_, _>>();
    let entry_ids = entry_indices.keys().cloned().collect::<Vec<_>>();

    for chunk in entry_ids.chunks(ENTRY_IDS_PER_QUERY) {
        let placeholders = std::iter::repeat_n("?", chunk.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            r#"
            SELECT decision, entry_id, target_entry_id, reason, created_at
            FROM memory_decision_logs
            WHERE entry_id IN ({placeholders}) OR target_entry_id IN ({placeholders})
            ORDER BY created_at DESC;
            "#
        );
        let values = chunk
            .iter()
            .chain(chunk.iter())
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect::<Vec<_>>();
        let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(values), |row| {
                Ok(MemoryEntryDecisionSummary {
                    kind: row.get(0)?,
                    entry_id: row.get(1)?,
                    target_entry_id: row.get(2)?,
                    reason: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|error| error.to_string())?;

        for decision in rows {
            let decision = decision.map_err(|error| error.to_string())?;
            for entry_id in [
                decision.entry_id.as_deref(),
                decision.target_entry_id.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if let Some(index) = entry_indices.get(entry_id)
                    && entries[*index].last_decision.is_none()
                {
                    entries[*index].last_decision = Some(decision.clone());
                }
            }
        }
    }
    Ok(())
}
