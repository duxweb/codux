//! AI usage stats for the headless host. The shared `codux-ai-history` engine
//! parses each CLI's session history, caches it in SQLite under the agent data
//! dir, and serves per-project usage snapshots — the same engine the desktop
//! runs, so the controller's AI stats panel renders with full parity.
//!
//! Single-reply, mirroring the desktop remote host: `project_state` returns the
//! cached snapshot (and queues a background refresh on a cold cache), and we
//! build the `ai.stats` payload from the same shared snapshot/current-session
//! wire builder the desktop host uses. The controller re-requests to pick up
//! freshly indexed data.

use codux_ai_history::indexer::AIHistoryIndexer;
use codux_ai_history::normalized::AIHistoryProjectRequest;
use codux_runtime_core::ai_stats::RemoteAICurrentSessionProvider;
use serde_json::{Value, json};
use std::path::Path;

/// Open the indexer against the agent data dir's usage cache.
pub fn open_indexer() -> AIHistoryIndexer {
    open_indexer_at(&crate::projects::agent_data_dir())
}

pub fn open_indexer_at(data_dir: &Path) -> AIHistoryIndexer {
    AIHistoryIndexer::with_database_path(data_dir.join("ai-usage.sqlite3"))
}

/// Build the `ai.stats` payload for a project.
pub fn ai_stats_payload(
    indexer: &AIHistoryIndexer,
    current_sessions: &dyn RemoteAICurrentSessionProvider,
    id: &str,
    name: &str,
    path: &str,
) -> Value {
    let request = AIHistoryProjectRequest {
        id: id.to_string(),
        name: name.to_string(),
        path: path.to_string(),
    };
    let live_sessions = current_sessions.current_sessions(id);
    match indexer.project_state(request) {
        Ok(state) => stats_payload_from_state(id, name, state, live_sessions),
        Err(_) => {
            let mut payload = codux_runtime_core::ai_stats::empty_ai_stats_payload(id, name);
            if let Some(object) = payload.as_object_mut() {
                object.insert("currentSessions".to_string(), json!(live_sessions));
            }
            payload
        }
    }
}

fn stats_payload_from_state(
    id: &str,
    name: &str,
    state: codux_ai_history::indexer::AIHistoryProjectState,
    current_sessions: Vec<codux_protocol::RemoteAICurrentSession>,
) -> Value {
    codux_runtime_core::ai_stats::ai_stats_payload_from_state(id, name, state, current_sessions)
        .unwrap_or_else(|_| codux_runtime_core::ai_stats::empty_ai_stats_payload(id, name))
}

/// The full `AIHistoryProjectState` (incl. snapshot) for a desktop controller,
/// indexed from the payload's project path directly (the controller owns the
/// project record; the agent just indexes the host's history for that path).
pub fn ai_state_payload(
    indexer: &AIHistoryIndexer,
    id: &str,
    name: &str,
    path: &str,
    refresh: bool,
) -> Result<Value, String> {
    let request = AIHistoryProjectRequest {
        id: id.to_string(),
        name: name.to_string(),
        path: path.to_string(),
    };
    if refresh {
        indexer.refresh_project(request.clone())?;
    }
    serde_json::to_value(indexer.project_state(request)?).map_err(|error| error.to_string())
}
