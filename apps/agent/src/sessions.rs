//! AI-session ops for the headless host. The controller routes a remote-hosted
//! project's session detail / rename / remove / fork here; the agent runs the
//! shared `codux-ai-sessions` engine against its own `ai-usage.sqlite3`.

use codux_ai_sessions::{AIHistoryService, AISessionForkRequest, AISessionForkTarget};
use serde_json::{Value, json};

use crate::projects::agent_data_dir;

fn service() -> AIHistoryService {
    AIHistoryService::new(agent_data_dir())
}

/// Serve an `ai.session` query. Returns `{op, result}` where `result` is the
/// op's JSON (or null on error, mirroring the engine's own fallbacks).
pub fn ai_session_payload(payload: &Value) -> Value {
    let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
    let project_path = payload
        .get("projectPath")
        .and_then(Value::as_str)
        .unwrap_or("");
    let session_id = payload.get("sessionId").and_then(Value::as_str).unwrap_or("");
    let result = match op {
        "detail" => service()
            .project_session_detail(project_path, session_id)
            .ok()
            .and_then(|detail| serde_json::to_value(detail).ok())
            .unwrap_or(Value::Null),
        "rename" => {
            let title = payload.get("title").and_then(Value::as_str).unwrap_or("");
            service()
                .rename_project_session(project_path, session_id, title)
                .ok()
                .and_then(|summary| serde_json::to_value(summary).ok())
                .unwrap_or(Value::Null)
        }
        "remove" => service()
            .remove_project_session(project_path, session_id)
            .ok()
            .and_then(|summary| serde_json::to_value(summary).ok())
            .unwrap_or(Value::Null),
        "fork" => {
            let target_tool = payload
                .get("targetTool")
                .cloned()
                .and_then(|value| serde_json::from_value::<AISessionForkTarget>(value).ok())
                .unwrap_or(AISessionForkTarget::Codex);
            let request = AISessionForkRequest {
                project_id: string_field(payload, "projectId"),
                project_name: string_field(payload, "projectName"),
                project_path: project_path.to_string(),
                session_id: session_id.to_string(),
                target_tool,
            };
            service()
                .fork_project_session(request)
                .ok()
                .and_then(|result| serde_json::to_value(result).ok())
                .unwrap_or(Value::Null)
        }
        _ => Value::Null,
    };
    json!({ "op": op, "result": result })
}

fn string_field(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}
