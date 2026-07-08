pub(super) use super::super::*;
pub(super) use crate::ai_runtime::{
    AIHookEventMetadata, AIHookEventPayload, binding::AIRuntimeBinding,
    constants::CODEX_STALE_PRELAUNCH_OPEN_TURN_SOURCE,
};
pub(super) use std::fs;
pub(super) use uuid::Uuid;

pub(super) fn needs_input_probe_snapshot(updated_at: f64) -> AIRuntimeContextSnapshot {
    AIRuntimeContextSnapshot {
        tool: "claude".to_string(),
        external_session_id: Some("claude-external-1".to_string()),
        transcript_path: None,
        model: Some("sonnet".to_string()),
        assistant_preview: None,
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        total_tokens: 0,
        usage_amounts: Vec::new(),
        baseline_usage_amounts: Vec::new(),
        updated_at,
        started_at: Some(1001.0),
        completed_at: None,
        response_state: Some("needsInput".to_string()),
        was_interrupted: false,
        has_completed_turn: false,
        session_origin: "live".to_string(),
        source: "probe".to_string(),
        plan: None,
    }
}
pub(super) fn responding_probe_snapshot(updated_at: f64) -> AIRuntimeContextSnapshot {
    AIRuntimeContextSnapshot {
        tool: "codex".to_string(),
        external_session_id: Some("session-1".to_string()),
        transcript_path: None,
        model: Some("gpt-5.5".to_string()),
        assistant_preview: None,
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        total_tokens: 0,
        usage_amounts: Vec::new(),
        baseline_usage_amounts: Vec::new(),
        updated_at,
        started_at: None,
        completed_at: None,
        response_state: Some("responding".to_string()),
        was_interrupted: false,
        has_completed_turn: false,
        session_origin: "live".to_string(),
        source: "probe".to_string(),
        plan: None,
    }
}
pub(super) fn codex_bridge_terminal() -> crate::ai_runtime::registry::AIRuntimeTerminalState {
    crate::ai_runtime::registry::AIRuntimeTerminalState {
        terminal_id: "terminal-1".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Codex".to_string(),
        cwd: "/tmp/codex-project".to_string(),
        tool: Some("codex".to_string()),
        is_active: true,
        session_key: Some("session-1".to_string()),
    }
}
pub(super) fn test_hook(kind: &str, updated_at: f64) -> AIHookEventPayload {
    AIHookEventPayload {
        kind: kind.to_string(),
        terminal_id: "terminal-1".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        project_name: "Project".to_string(),
        project_path: Some("/tmp/codex-project".to_string()),
        session_title: "Codex".to_string(),
        tool: "codex".to_string(),
        ai_session_id: Some("session-1".to_string()),
        model: Some("gpt-5.4".to_string()),
        input_tokens: None,
        output_tokens: None,
        cached_input_tokens: None,
        total_tokens: None,
        updated_at,
        metadata: None,
    }
}
pub(super) fn test_hook_for(
    tool: &str,
    terminal_id: &str,
    ai_session_id: &str,
    updated_at: f64,
) -> AIHookEventPayload {
    AIHookEventPayload {
        tool: tool.to_string(),
        terminal_id: terminal_id.to_string(),
        terminal_instance_id: Some(format!("{terminal_id}-instance")),
        ai_session_id: Some(ai_session_id.to_string()),
        session_title: format!("{tool} session"),
        updated_at,
        ..test_hook("promptSubmitted", updated_at)
    }
}
pub(super) fn empty_metadata() -> AIHookEventMetadata {
    AIHookEventMetadata {
        transcript_path: None,
        notification_type: None,
        source: None,
        reason: None,
        cwd: None,
        target_tool_name: None,
        message: None,
        was_interrupted: None,
        has_completed_turn: None,
    }
}
pub(super) fn test_binding(
    binding_id: &str,
    terminal_id: &str,
    terminal_instance_id: &str,
    started_at: f64,
) -> AIRuntimeBinding {
    AIRuntimeBinding {
        runtime_binding_id: binding_id.to_string(),
        terminal_id: terminal_id.to_string(),
        terminal_instance_id: Some(terminal_instance_id.to_string()),
        tool: "codex".to_string(),
        project_id: "project-1".to_string(),
        project_name: "Project".to_string(),
        project_path: Some("/tmp/codex-project".to_string()),
        session_title: "Codex".to_string(),
        launch_started_at: started_at,
        external_session_id: None,
        transcript_path: None,
        model: None,
        session_origin: None,
        updated_at: started_at,
    }
}
