use super::AIRuntimeStateCore;
use crate::ai_runtime::{
    constants::CODEX_INTERVAL_POLL_MINIMUM_SECONDS,
    payload::AIHookEventPayload,
    registry::AIRuntimeTerminalState,
    snapshot::{AIRuntimeProbeRequest, AISessionSnapshot},
    state::{canonical_tool_name, normalized_string},
    tool_driver::is_supported_runtime_tool,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn probe_request_for_session(session: &AISessionSnapshot) -> AIRuntimeProbeRequest {
    AIRuntimeProbeRequest {
        terminal_id: session.terminal_id.clone(),
        terminal_instance_id: session.terminal_instance_id.clone(),
        project_id: session.project_id.clone(),
        project_path: session.project_path.clone(),
        tool: session.tool.clone(),
        external_session_id: session.ai_session_id.clone(),
        transcript_path: session.transcript_path.clone(),
        started_at: session.started_at,
        updated_at: session.updated_at,
    }
}

pub(super) fn mark_interrupted(session: AISessionSnapshot, updated_at: f64) -> AISessionSnapshot {
    let completed_turn_started_at = session
        .active_turn_started_at
        .or(session.runtime_turn_started_at)
        .or(session.started_at);
    AISessionSnapshot {
        state: "idle".to_string(),
        status: "idle".to_string(),
        is_running: false,
        was_interrupted: true,
        has_completed_turn: false,
        active_turn_started_at: None,
        runtime_turn_started_at: None,
        completed_turn_started_at,
        updated_at,
        ..session
    }
}

pub(super) fn bridge_terminal_session(
    terminal: &AIRuntimeTerminalState,
    now: f64,
) -> Option<AISessionSnapshot> {
    if !terminal.is_active {
        return None;
    }
    let tool = canonical_tool_name(terminal.tool.as_deref()?)?;
    if !is_supported_runtime_tool(&tool) {
        return None;
    }
    let session_key = normalized_string(terminal.session_key.as_deref())?;
    let project_id = normalized_string(Some(terminal.project_id.as_str()))?;
    let terminal_id = normalized_string(Some(terminal.terminal_id.as_str()))?;
    Some(AISessionSnapshot {
        terminal_id,
        terminal_instance_id: normalized_string(terminal.terminal_instance_id.as_deref()),
        project_id,
        project_name: "Workspace".to_string(),
        project_path: normalized_string(Some(terminal.cwd.as_str())),
        session_title: normalized_string(Some(terminal.title.as_str()))
            .unwrap_or_else(|| "Terminal".to_string()),
        tool,
        ai_session_id: Some(session_key),
        model: None,
        state: "responding".to_string(),
        status: "running".to_string(),
        is_running: true,
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        total_tokens: 0,
        baseline_total_tokens: 0,
        baseline_cached_input_tokens: 0,
        baseline_resolved: false,
        started_at: Some(now),
        updated_at: now,
        active_turn_started_at: Some(now),
        runtime_turn_started_at: None,
        completed_turn_started_at: None,
        has_completed_turn: false,
        was_interrupted: false,
        transcript_path: None,
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
        plan: None,
    })
}

pub(super) fn is_tool_activity_without_loading(
    event: &AIHookEventPayload,
    previous: Option<&AISessionSnapshot>,
) -> bool {
    if event.kind != "promptSubmitted"
        || event
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.source.as_deref()))
            .as_deref()
            != Some("tool-use")
    {
        return false;
    }
    previous
        .map(|session| session.has_completed_turn || session.was_interrupted)
        .unwrap_or(true)
}

pub(super) fn note_latest_active_started_at(
    core: &mut AIRuntimeStateCore,
    project_id: &str,
    started_at: f64,
) {
    let previous = core
        .latest_active_started_at_by_project
        .get(project_id)
        .copied()
        .unwrap_or(0.0);
    if started_at > previous {
        core.latest_active_started_at_by_project
            .insert(project_id.to_string(), started_at);
    }
}

pub fn should_poll_runtime_session(session: &AISessionSnapshot, reason: &str, now: f64) -> bool {
    if reason == "transcript-tail" && is_codex_transcript_session(session) {
        return true;
    }
    if canonical_tool_name(&session.tool).as_deref() == Some("codex")
        && normalized_string(session.transcript_path.as_deref()).is_some()
        && reason == "interval"
        && now - session.updated_at < CODEX_INTERVAL_POLL_MINIMUM_SECONDS
    {
        return false;
    }
    session.state == "responding" || session.state == "needsInput" || !session.has_completed_turn
}

pub(super) fn is_codex_transcript_session(session: &AISessionSnapshot) -> bool {
    canonical_tool_name(&session.tool).as_deref() == Some("codex")
        && normalized_string(session.transcript_path.as_deref()).is_some()
}

pub(super) fn number_or(previous: Option<i64>, value: Option<i64>) -> i64 {
    value
        .map(|value| value.max(0))
        .unwrap_or(previous.unwrap_or(0))
}

pub(super) fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}
