//! AI usage stats for the headless host. The shared `codux-ai-history` engine
//! parses each CLI's session history, caches it in SQLite under the agent data
//! dir, and serves per-project usage snapshots — the same engine the desktop
//! runs, so the controller's AI stats panel renders with full parity.
//!
//! Single-reply, mirroring the desktop remote host: `project_state` returns the
//! cached snapshot (and queues a background refresh on a cold cache), and we
//! build the `ai.stats` payload from its `baseline`. The controller re-requests
//! to pick up freshly indexed data.

use codux_ai_history::indexer::AIHistoryIndexer;
use codux_ai_history::normalized::AIHistoryProjectRequest;
use serde_json::{json, Value};

/// Open the indexer against the agent data dir's usage cache.
pub fn open_indexer() -> AIHistoryIndexer {
    AIHistoryIndexer::with_database_path(crate::projects::agent_data_dir().join("ai-usage.sqlite3"))
}

/// Build the `ai.stats` payload for a project.
pub fn ai_stats_payload(indexer: &AIHistoryIndexer, id: &str, name: &str, path: &str) -> Value {
    let request = AIHistoryProjectRequest {
        id: id.to_string(),
        name: name.to_string(),
        path: path.to_string(),
    };
    match indexer.project_state(request) {
        Ok(state) => stats_payload_from_state(id, name, state),
        Err(_) => fallback_payload(id, name),
    }
}

fn stats_payload_from_state(
    id: &str,
    name: &str,
    state: codux_ai_history::indexer::AIHistoryProjectState,
) -> Value {
    let mut value = serde_json::to_value(state).unwrap_or(Value::Null);
    let baseline = value
        .get_mut("baseline")
        .map(Value::take)
        .filter(|value| !value.is_null());
    let mut payload = baseline.unwrap_or_else(|| fallback_payload(id, name));
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "updatedAt".to_string(),
            json!(chrono::Utc::now().to_rfc3339()),
        );
    }
    payload
}

fn fallback_payload(id: &str, name: &str) -> Value {
    json!({
        "projectId": id,
        "projectName": name,
        "projectSummary": {},
        "sessions": [],
        "heatmap": [],
        "todayTimeBuckets": [],
        "toolBreakdown": [],
        "modelBreakdown": [],
    })
}
