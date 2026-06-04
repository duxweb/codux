use super::types::SessionLink;
use rusqlite::Connection;
use uuid::Uuid;

pub(super) fn table_has_column(conn: &Connection, table: &str, column: &str) -> bool {
    let Ok(mut statement) = conn.prepare(&format!("PRAGMA table_info({table})")) else {
        return false;
    };
    let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(1)) else {
        return false;
    };
    rows.flatten().any(|name| name == column)
}

pub(super) fn min_option(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

pub(super) fn max_option(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

pub(super) fn matching_session_keys(
    links: &[SessionLink],
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
        if session_id == link.session_key || session_id == raw_id || session_id == grouped_id {
            let key = (link.source.clone(), link.session_key.clone());
            if !matched.contains(&key) {
                matched.push(key);
            }
        }
    }
    matched
}

fn history_key(source: &str, session_id: &str) -> String {
    format!("{source}:{session_id}")
}

pub(super) fn history_group_key(
    source: &str,
    session_key: &str,
    external_session_id: Option<&str>,
) -> String {
    history_key(source, external_session_id.unwrap_or(session_key))
}

pub(super) fn deterministic_uuid(value: &str) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, value.as_bytes()).to_string()
}

pub(super) fn local_today_start_seconds() -> f64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0);
    crate::ai_history_normalized::local_day_start_seconds(now)
}
