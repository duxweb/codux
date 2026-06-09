pub(super) fn manager_target_rows(
    conn: &Connection,
    projects: &[ProjectInfo],
) -> Result<Vec<MemoryManagerTargetRow>, String> {
    let project_by_id = projects
        .iter()
        .map(|project| (project.id.as_str(), project))
        .collect::<HashMap<_, _>>();
    let mut rows = Vec::new();
    let user_overview = memory_scope_overview(conn, "user", None)?;
    rows.push(MemoryManagerTargetRow {
        id: "user".to_string(),
        scope: "user".to_string(),
        project_id: None,
        title: "User Memory".to_string(),
        subtitle: "Cross-project preferences".to_string(),
        count: user_overview.total_count(),
        updated_at: user_overview.updated_at,
        is_open_project: false,
    });
    for (project_id, overview) in project_overviews_for_management(conn)? {
        let project = project_by_id.get(project_id.as_str()).copied();
        rows.push(MemoryManagerTargetRow {
            id: format!("project-{project_id}"),
            scope: "project".to_string(),
            project_id: Some(project_id.clone()),
            title: project
                .map(|project| project.name.clone())
                .unwrap_or_else(|| {
                    format!("Project {}", project_id.chars().take(8).collect::<String>())
                }),
            subtitle: project
                .map(|project| project.path.clone())
                .unwrap_or_else(|| project_id.clone()),
            count: overview.total_count(),
            updated_at: overview.updated_at,
            is_open_project: project.is_some(),
        });
    }
    for project in projects {
        if rows.iter().any(|row| {
            row.scope == "project" && row.project_id.as_deref() == Some(project.id.as_str())
        }) {
            continue;
        }
        rows.push(MemoryManagerTargetRow {
            id: format!("project-{}", project.id),
            scope: "project".to_string(),
            project_id: Some(project.id.clone()),
            title: project.name.clone(),
            subtitle: project.path.clone(),
            count: 0,
            updated_at: None,
            is_open_project: true,
        });
    }
    Ok(rows)
}

fn project_overviews_for_management(
    conn: &Connection,
) -> Result<Vec<(String, MemoryScopeOverview)>, String> {
    let mut ids = HashSet::new();
    let mut statement = conn
        .prepare(
            r#"
            SELECT DISTINCT project_id
            FROM memory_entries
            WHERE scope = 'project' AND project_id IS NOT NULL
            UNION
            SELECT DISTINCT project_id
            FROM memory_summaries
            WHERE scope = 'project' AND project_id IS NOT NULL
            UNION
            SELECT DISTINCT project_id
            FROM memory_project_profiles
            WHERE project_id IS NOT NULL
            "#,
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    for row in rows.flatten() {
        ids.insert(row);
    }
    let mut overviews = ids
        .into_iter()
        .filter_map(|project_id| {
            let overview =
                memory_scope_overview(conn, "project", Some(project_id.as_str())).ok()?;
            (overview.total_count() > 0).then_some((project_id, overview))
        })
        .collect::<Vec<_>>();
    overviews.sort_by(|left, right| {
        right
            .1
            .updated_at
            .partial_cmp(&left.1.updated_at)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    Ok(overviews)
}

pub(super) fn memory_scope_overview(
    conn: &Connection,
    scope: &str,
    project_id: Option<&str>,
) -> Result<MemoryScopeOverview, String> {
    let scope = normalize_scope(scope);
    let project_id = if scope == "project" { project_id } else { None };
    let (active, archived, merged, entry_tokens, entry_updated): (i64, i64, i64, i64, Option<f64>) =
        conn.query_row(
            r#"
            SELECT
                COALESCE(SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'archived' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'merged' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM((length(content) + length(COALESCE(rationale, '')) + 3) / 4), 0),
                MAX(updated_at)
            FROM memory_entries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '')
            "#,
            params![scope, project_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    let (summary_count, summary_tokens, summary_updated): (i64, i64, Option<f64>) = conn
        .query_row(
            r#"
            SELECT COUNT(*), COALESCE(SUM(token_estimate), 0), MAX(updated_at)
            FROM memory_summaries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '')
            "#,
            params![scope, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| error.to_string())?;
    let (profile_count, profile_tokens, profile_updated): (i64, i64, Option<f64>) =
        if scope == "project" {
            conn.query_row(
                r#"
                SELECT COUNT(*), COALESCE(SUM((length(content) + 3) / 4), 0), MAX(updated_at)
                FROM memory_project_profiles
                WHERE project_id = ?1
                "#,
                params![project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| error.to_string())?
        } else {
            (0, 0, None)
        };
    Ok(MemoryScopeOverview {
        active_entry_count: active,
        archived_entry_count: archived,
        merged_entry_count: merged,
        profile_count,
        summary_count,
        total_token_estimate: entry_tokens + summary_tokens + profile_tokens,
        updated_at: max_optional_f64(
            max_optional_f64(entry_updated, summary_updated),
            profile_updated,
        ),
    })
}

pub(super) fn selected_memory_target_title(
    rows: &[MemoryManagerTargetRow],
    scope: &str,
    project_id: Option<&str>,
) -> String {
    rows.iter()
        .find(|row| {
            row.scope == scope
                && if scope == "project" {
                    row.project_id.as_deref() == project_id
                } else {
                    true
                }
        })
        .map(|row| row.title.clone())
        .unwrap_or_else(|| {
            if scope == "project" {
                "Project Memory".to_string()
            } else {
                "User Memory".to_string()
            }
        })
}
