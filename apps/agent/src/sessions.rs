//! AI-session ops for the headless host. The controller routes a remote-hosted
//! project's session list / detail / rename / remove / fork here; the agent runs
//! the shared `codux-ai-sessions` op dispatch against its own `ai-usage.sqlite3`
//! — the same table the desktop host uses, so the two never drift.

use codux_ai_sessions::AIHistoryService;
use serde_json::Value;

use crate::projects::agent_data_dir;

/// Serve an `ai.session` query. Returns `{op, result}` where `result` is the
/// operation's JSON value.
pub fn ai_session_payload(payload: &Value) -> Result<Value, String> {
    let project_path = payload
        .get("projectPath")
        .and_then(Value::as_str)
        .unwrap_or("");
    let service = AIHistoryService::new(agent_data_dir());
    codux_ai_sessions::session_op_payload(&service, project_path, payload)
}
