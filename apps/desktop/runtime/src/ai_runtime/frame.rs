use super::payload::{
    AIHookEventMetadata, AIHookEventPayload, AIToolUsageEnvelope, RuntimeEnvelope,
};

pub fn runtime_frame_to_hook(buffer: &[u8]) -> Option<AIHookEventPayload> {
    let buffer = buffer.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(buffer);
    let envelope = serde_json::from_slice::<RuntimeEnvelope>(buffer).ok()?;
    match envelope.kind.as_str() {
        "ai-hook" => serde_json::from_value::<AIHookEventPayload>(envelope.payload).ok(),
        "opencode-runtime" => serde_json::from_value::<AIToolUsageEnvelope>(envelope.payload)
            .ok()
            .and_then(opencode_runtime_to_hook),
        _ => None,
    }
}

pub fn opencode_runtime_to_hook(envelope: AIToolUsageEnvelope) -> Option<AIHookEventPayload> {
    if envelope.session_id.trim().is_empty() || envelope.project_id.trim().is_empty() {
        return None;
    }

    let response_state = envelope.response_state.as_deref();
    let (kind, metadata) = match response_state {
        Some("responding") => ("promptSubmitted", None),
        Some("idle") if envelope.status == "completed" => (
            "turnCompleted",
            Some(opencode_runtime_metadata(&envelope.status, false, true)),
        ),
        Some("idle") => (
            "turnCompleted",
            Some(opencode_runtime_metadata(&envelope.status, true, false)),
        ),
        _ if envelope.status == "running" => ("promptSubmitted", None),
        _ => ("turnCompleted", None),
    };

    Some(AIHookEventPayload {
        kind: kind.to_string(),
        terminal_id: envelope.session_id,
        terminal_instance_id: envelope.session_instance_id,
        project_id: envelope.project_id,
        project_name: envelope.project_name,
        project_path: envelope.project_path,
        session_title: envelope.session_title,
        tool: envelope.tool,
        ai_session_id: envelope.external_session_id,
        model: envelope.model,
        input_tokens: envelope.input_tokens,
        output_tokens: envelope.output_tokens,
        cached_input_tokens: envelope.cached_input_tokens,
        total_tokens: envelope.total_tokens,
        updated_at: envelope.updated_at,
        metadata,
    })
}

fn opencode_runtime_metadata(
    status: &str,
    was_interrupted: bool,
    has_completed_turn: bool,
) -> AIHookEventMetadata {
    AIHookEventMetadata {
        transcript_path: None,
        notification_type: None,
        source: Some("opencode-runtime".to_string()),
        reason: Some(status.to_string()),
        cwd: None,
        target_tool_name: None,
        message: None,
        was_interrupted: Some(was_interrupted),
        has_completed_turn: Some(has_completed_turn),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_opencode_runtime_to_ai_hook_payload() {
        let payload = runtime_frame_to_hook(
            br#"{
              "kind": "opencode-runtime",
              "payload": {
                "sessionId": "term-2",
                "sessionInstanceId": "inst-1",
                "externalSessionID": "external-1",
                "projectId": "project-1",
                "projectName": "Codux",
                "projectPath": "/Volumes/Web/codux-gpui",
                "sessionTitle": "Review",
                "tool": "opencode",
                "model": "model-a",
                "status": "completed",
                "responseState": "idle",
                "updatedAt": 20,
                "inputTokens": 10,
                "outputTokens": 5,
                "cachedInputTokens": 2,
                "totalTokens": 15
              }
            }"#,
        )
        .expect("payload");

        assert_eq!(payload.kind, "turnCompleted");
        assert_eq!(payload.terminal_id, "term-2");
        assert_eq!(payload.terminal_instance_id.as_deref(), Some("inst-1"));
        assert_eq!(payload.ai_session_id.as_deref(), Some("external-1"));
        assert_eq!(
            payload.project_path.as_deref(),
            Some("/Volumes/Web/codux-gpui")
        );
        assert_eq!(payload.total_tokens, Some(15));
        assert_eq!(
            payload
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.has_completed_turn),
            Some(true)
        );
    }
}
