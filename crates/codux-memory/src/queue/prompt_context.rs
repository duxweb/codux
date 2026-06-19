use crate::extraction::PromptMemoryEntry;
use rusqlite::{Connection, params};

pub(super) fn prompt_entries(
    conn: &Connection,
    scope: &str,
    project_id: Option<&str>,
    limit: i64,
    query: &str,
) -> Result<Vec<PromptMemoryEntry>, String> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let mut statement = conn
        .prepare(
            r#"
            SELECT id, COALESCE(module_key, 'general'), kind, content, rationale, access_count, updated_at
            FROM memory_entries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '')
              AND tier IN ('core', 'working')
              AND status = 'active'
              AND superseded_by IS NULL
            ORDER BY access_count DESC, updated_at DESC
            LIMIT 64;
            "#,
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![scope, project_id], |row| {
            Ok((
                PromptMemoryEntry {
                    id: row.get(0)?,
                    module_key: row.get(1)?,
                    kind: row.get(2)?,
                    content: row.get(3)?,
                    rationale: row.get(4)?,
                },
                row.get::<_, i64>(5)?,
                row.get::<_, f64>(6)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let query_terms = prompt_query_terms(query);
    let mut entries = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by(|left, right| {
        let left_score = prompt_entry_score(&left.0, left.1, left.2, &query_terms);
        let right_score = prompt_entry_score(&right.0, right.1, right.2, &query_terms);
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let selected: Vec<PromptMemoryEntry> = entries
        .into_iter()
        .take(limit as usize)
        .map(|(entry, _, _)| entry)
        .collect();
    // Record usage: these entries were recalled as relevant to a real session.
    // This is the signal the scorer's access_count weighting and the launch
    // injection ranking rely on (previously never written, so it stayed 0).
    bump_access_counts(conn, selected.iter().map(|entry| entry.id.as_str()));
    Ok(selected)
}

fn bump_access_counts<'a>(conn: &Connection, ids: impl Iterator<Item = &'a str>) {
    let ids = ids.collect::<Vec<_>>();
    if ids.is_empty() {
        return;
    }
    let placeholders = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "UPDATE memory_entries SET access_count = access_count + 1, \
         last_accessed_at = unixepoch('now') WHERE id IN ({placeholders})"
    );
    // Best-effort: a failed usage bump must not fail extraction recall.
    let _ = conn.execute(&sql, rusqlite::params_from_iter(ids));
}

fn prompt_entry_score(
    entry: &PromptMemoryEntry,
    access_count: i64,
    updated_at: f64,
    query_terms: &[String],
) -> f64 {
    let haystack = format!(
        "{} {} {} {}",
        entry.content,
        entry.rationale.as_deref().unwrap_or(""),
        entry.kind,
        entry.module_key.as_deref().unwrap_or("")
    )
    .to_lowercase();
    let mut score = access_count.min(20) as f64 * 1.5 + updated_at / 86_400.0 / 10_000.0;
    for term in query_terms {
        if haystack.contains(term) {
            score += 20.0;
        }
    }
    score
}

fn prompt_query_terms(query: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
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
