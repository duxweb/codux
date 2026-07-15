//! AI-session ops for the headless host. The controller routes a remote-hosted
//! project's session list / detail / rename / remove / fork here; the agent runs
//! the shared `codux-ai-sessions` op dispatch against its own `ai-usage.sqlite3`
//! — the same table the desktop host uses, so the two never drift.

use codux_ai_history::{indexer::AIHistoryIndexer, normalized::AIHistoryProjectRequest};
use codux_ai_sessions::AIHistoryService;
use serde_json::Value;

use crate::projects::agent_data_dir;

/// Serve an `ai.session` query. Returns `{op, result}` where `result` is the
/// operation's JSON value.
pub fn ai_session_payload(indexer: &AIHistoryIndexer, payload: &Value) -> Result<Value, String> {
    let project_path = payload
        .get("projectPath")
        .and_then(Value::as_str)
        .unwrap_or("");
    let service = AIHistoryService::new(agent_data_dir());
    let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
    let result = codux_ai_sessions::session_op_result_with_indexer(
        &service,
        indexer,
        AIHistoryProjectRequest {
            id: string_field(payload, "projectId"),
            name: string_field(payload, "projectName"),
            path: project_path.to_string(),
        },
        payload,
    )?;
    Ok(serde_json::json!({ "op": op, "result": result }))
}

fn string_field(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}
