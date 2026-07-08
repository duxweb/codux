use super::AIRuntimeStateCore;
use crate::ai_runtime::{
    binding::{AIRuntimeBinding, normalized_runtime_project_name},
    constants::{CODEX_INTERVAL_POLL_MINIMUM_SECONDS, RUNNING_STALE_SECONDS},
    payload::AIHookEventPayload,
    registry::AIRuntimeTerminalState,
    snapshot::{AIRuntimeProbeRequest, AISessionSnapshot},
    state::{canonical_tool_name, normalized_string},
    tool_driver::{is_supported_runtime_tool, runtime_resource_paths},
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
        occupied_external_session_ids: Default::default(),
    }
}

/// Silently retire an in-flight turn that the runtime simply lost track of
/// (no authoritative end signal within the backstop window). This is NOT an
/// interruption: `was_interrupted`/`has_completed_turn` stay false so it emits
/// no "已中断"/"completed" notification, is not enqueued for memory extraction,
/// and does not trigger a pet failure reaction — it just stops the loading
/// state. Genuine interruptions/completions still flow through hook metadata.
pub(super) fn mark_timed_out(session: AISessionSnapshot, updated_at: f64) -> AISessionSnapshot {
    let completed_turn_started_at = session
        .active_turn_started_at
        .or(session.runtime_turn_started_at)
        .or(session.started_at);
    AISessionSnapshot {
        state: "idle".to_string(),
        status: "idle".to_string(),
        is_running: false,
        was_interrupted: false,
        has_completed_turn: false,
        active_turn_started_at: None,
        runtime_turn_started_at: None,
        completed_turn_started_at,
        updated_at,
        ..session
    }
}

/// Idle session for a process-detected AI tool (the sole creation path); no session id — the probe resolves the transcript by cwd and refines state.
pub(super) fn detected_terminal_session(
    terminal: &AIRuntimeTerminalState,
    tool: &str,
    now: f64,
) -> Option<AISessionSnapshot> {
    let tool = canonical_tool_name(tool)?;
    if !is_supported_runtime_tool(&tool) {
        return None;
    }
    let project_id = normalized_string(Some(terminal.project_id.as_str()))?;
    let terminal_id = normalized_string(Some(terminal.terminal_id.as_str()))?;
    let project_path = normalized_string(Some(terminal.cwd.as_str()));
    Some(AISessionSnapshot {
        terminal_id,
        terminal_instance_id: normalized_string(terminal.terminal_instance_id.as_deref()),
        project_name: normalized_runtime_project_name(
            None,
            project_path.as_deref(),
            Some(&project_id),
        ),
        project_id,
        project_path,
        session_title: normalized_string(Some(terminal.title.as_str()))
            .unwrap_or_else(|| "Terminal".to_string()),
        tool,
        ai_session_id: None,
        model: None,
        state: "idle".to_string(),
        status: "idle".to_string(),
        is_running: false,
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        total_tokens: 0,
        baseline_total_tokens: 0,
        baseline_cached_input_tokens: 0,
        usage_amounts: Vec::new(),
        baseline_usage_amounts: Vec::new(),
        baseline_resolved: false,
        started_at: Some(now),
        updated_at: now,
        active_turn_started_at: None,
        runtime_turn_started_at: None,
        completed_turn_started_at: None,
        session_origin: None,
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

pub(super) fn binding_terminal_session(binding: &AIRuntimeBinding) -> AISessionSnapshot {
    AISessionSnapshot {
        terminal_id: binding.terminal_id.clone(),
        terminal_instance_id: binding.terminal_instance_id.clone(),
        project_id: binding.project_id.clone(),
        project_name: binding.project_name.clone(),
        project_path: binding.project_path.clone(),
        session_title: binding.session_title.clone(),
        tool: binding.tool.clone(),
        ai_session_id: binding.external_session_id.clone(),
        model: binding.model.clone(),
        state: "idle".to_string(),
        status: "idle".to_string(),
        is_running: false,
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        total_tokens: 0,
        baseline_total_tokens: 0,
        baseline_cached_input_tokens: 0,
        usage_amounts: Vec::new(),
        baseline_usage_amounts: Vec::new(),
        baseline_resolved: false,
        started_at: Some(binding.launch_started_at),
        updated_at: binding.updated_at.max(binding.launch_started_at),
        active_turn_started_at: None,
        runtime_turn_started_at: None,
        completed_turn_started_at: None,
        session_origin: binding.session_origin.clone(),
        has_completed_turn: false,
        was_interrupted: false,
        transcript_path: binding.transcript_path.clone(),
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
        plan: None,
    }
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
    if canonical_tool_name(&session.tool).as_deref() == Some("codewhale")
        && session.state == "idle"
        && session.was_interrupted
    {
        return false;
    }
    if reason == "transcript-tail" && is_transcript_monitored_session(session) {
        return true;
    }
    if canonical_tool_name(&session.tool).as_deref() == Some("kiro") {
        return session.state == "responding"
            || session.state == "needsInput"
            || now - session.updated_at <= RUNNING_STALE_SECONDS * 3.0;
    }
    // Quiet codex sessions skip the interval re-parse (the transcript monitor
    // covers active ones), EXCEPT while a turn is live: a `responding` turn can
    // be silently blocked on an approval the rollout never records, and a
    // `needsInput` wait must be re-checked often to catch the resume. Keeping
    // those on the 5s interval is what lets the pure-file `needsInput` surface
    // and clear promptly instead of waiting out the 60s quiet-session gate.
    if is_codex_transcript_session(session)
        && reason == "interval"
        && session.state != "responding"
        && session.state != "needsInput"
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

/// Sessions whose live state can be refreshed from driver-registered resources
/// (transcript files or DB files). The monitor only watches `mtime + size`; the
/// driver probe owns format parsing.
pub(super) fn is_transcript_monitored_session(session: &AISessionSnapshot) -> bool {
    !runtime_resource_paths(session).is_empty()
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
