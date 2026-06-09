use super::types::CodexPayloadFields;
use crate::ai_runtime::probe::preview::sanitized_preview_from_raw_values;

pub(super) fn codex_assistant_preview(
    row_type: Option<&str>,
    payload: &CodexPayloadFields<'_>,
) -> Option<String> {
    let payload_type = payload.payload_type.as_deref();
    match row_type {
        Some("event_msg") if payload_type == Some("agent_message") => {
            sanitized_preview_from_raw_values(&[payload.message, payload.text, payload.content])
        }
        Some("response_item") if payload_type == Some("reasoning") => {
            sanitized_preview_from_raw_values(&[
                payload.summary,
                payload.summary_text,
                payload.text,
            ])
        }
        Some("response_item") if payload_type == Some("agentMessage") => {
            sanitized_preview_from_raw_values(&[payload.text, payload.content, payload.message])
        }
        Some("response_item") if payload_type == Some("message") => {
            sanitized_preview_from_raw_values(&[payload.content, payload.message, payload.text])
        }
        _ => None,
    }
}
