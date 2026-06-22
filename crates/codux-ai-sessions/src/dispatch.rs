//! Shared `ai.session` op dispatch. Both the desktop and headless remote hosts
//! route the wire op (`list` / `detail` / `rename` / `remove` / `restore` /
//! `fork`) through this single table so neither host serves a partial set or
//! drifts on shape.
//! The host resolves `project_path` (its own projectId fallback) and supplies
//! the service; this returns the inner `result` value for `{op, result}`.

use serde_json::{Value, json};

use crate::{AIHistoryService, AISessionForkRequest, AISessionForkTarget};

/// Run one `ai.session` op and return its JSON result (null on error, mirroring
/// the engine's own fallbacks).
pub fn session_op_result(service: &AIHistoryService, project_path: &str, payload: &Value) -> Value {
    let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
    let session_id = payload
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or("");
    match op {
        "list" => serde_json::to_value(
            service
                .project_summary(project_path)
                .sessions
                .into_iter()
                .map(codux_protocol::RemoteAISessionSummary::from)
                .collect::<Vec<_>>(),
        )
        .unwrap_or(Value::Null),
        "detail" => service
            .project_session_detail(project_path, session_id)
            .ok()
            .and_then(|detail| serde_json::to_value(detail).ok())
            .unwrap_or(Value::Null),
        "rename" => {
            let title = payload.get("title").and_then(Value::as_str).unwrap_or("");
            service
                .rename_project_session(project_path, session_id, title)
                .ok()
                .and_then(|summary| serde_json::to_value(summary).ok())
                .unwrap_or(Value::Null)
        }
        "remove" => service
            .remove_project_session(project_path, session_id)
            .ok()
            .and_then(|summary| serde_json::to_value(summary).ok())
            .unwrap_or(Value::Null),
        "restore" => service
            .project_summary(project_path)
            .sessions
            .into_iter()
            .find(|session| session.id == session_id)
            .map(|session| {
                json!({
                    "command": crate::session_restore_command(&session),
                    "title": session.title,
                })
            })
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
            service
                .fork_project_session(request)
                .ok()
                .and_then(|result| serde_json::to_value(result).ok())
                .unwrap_or(Value::Null)
        }
        _ => Value::Null,
    }
}

/// Convenience wrapper returning the full `{op, result}` envelope both hosts send.
pub fn session_op_payload(service: &AIHistoryService, project_path: &str, payload: &Value) -> Value {
    let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
    json!({ "op": op, "result": session_op_result(service, project_path, payload) })
}

fn string_field(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}
