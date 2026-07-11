use crate::ai_runtime::{
    constants::COMPLETION_TIMESTAMP_SKEW_SECONDS,
    payload::{AIHookEventMetadata, AIHookEventPayload},
    probe::{claude::ClaudeProbeCache, probe_runtime, probe_runtime_with_claude_cache},
    runtime_log_line,
    snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest, AISessionSnapshot},
    state::{canonical_tool_name, normalized_string},
};

use super::helpers::number_or;

pub(super) fn resolve_hook_event(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    resolve_hook_event_with_claude_cache(event, current_session, None)
}

pub(super) fn resolve_hook_event_with_claude_cache(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
    claude_cache: Option<&mut ClaudeProbeCache>,
) -> AIHookEventPayload {
    match canonical_tool_name(&event.tool).as_deref() {
        Some("codex") => resolve_codex_hook_event(event, current_session),
        Some("claude") => resolve_claude_hook_event(event, current_session, claude_cache),
        Some("kiro") => resolve_project_probe_hook_event(event, current_session, "kiro"),
        Some("codewhale") => resolve_project_probe_hook_event(event, current_session, "codewhale"),
        _ => {
            let fallback = matching_fallback_session(&event, current_session);
            with_fallback(event, fallback)
        }
    }
}

fn resolve_codex_hook_event(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    let fallback = matching_fallback_session(&event, current_session);
    let resolved = with_fallback(event, fallback);
    if resolved.kind != "turnCompleted"
        || resolved
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.transcript_path.as_deref()))
            .is_none()
    {
        return resolved;
    }
    let request = AIRuntimeProbeRequest {
        terminal_id: resolved.terminal_id.clone(),
        terminal_instance_id: resolved.terminal_instance_id.clone(),
        project_id: resolved.project_id.clone(),
        project_path: resolved.project_path.clone(),
        tool: "codex".to_string(),
        external_session_id: normalized_string(resolved.ai_session_id.as_deref())
            .or_else(|| fallback.and_then(|session| session.ai_session_id.clone())),
        transcript_path: resolved
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.transcript_path.as_deref())),
        started_at: fallback
            .and_then(|session| session.started_at)
            .or(Some(resolved.updated_at)),
        updated_at: resolved.updated_at,
        occupied_external_session_ids: Default::default(),
    };
    probe_runtime(&request)
        .map(|snapshot| merge_snapshot_into_hook(resolved.clone(), snapshot, fallback))
        .unwrap_or(resolved)
}

fn resolve_claude_hook_event(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
    claude_cache: Option<&mut ClaudeProbeCache>,
) -> AIHookEventPayload {
    let fallback = matching_fallback_session(&event, current_session);
    let resolved = with_fallback(event, fallback);
    if resolved.kind != "turnCompleted" {
        return resolved;
    }
    let external_session_id = normalized_string(resolved.ai_session_id.as_deref())
        .or_else(|| fallback.and_then(|session| session.ai_session_id.clone()));
    if normalized_string(resolved.project_path.as_deref()).is_none()
        || external_session_id.is_none()
    {
        return resolved;
    }
    let request = AIRuntimeProbeRequest {
        terminal_id: resolved.terminal_id.clone(),
        terminal_instance_id: resolved.terminal_instance_id.clone(),
        project_id: resolved.project_id.clone(),
        project_path: resolved.project_path.clone(),
        tool: "claude".to_string(),
        external_session_id: external_session_id.clone(),
        transcript_path: None,
        started_at: fallback
            .and_then(|session| session.started_at)
            .or(Some(resolved.updated_at)),
        updated_at: resolved.updated_at,
        occupied_external_session_ids: Default::default(),
    };
    probe_claude_hook_runtime(&request, claude_cache)
        .map(|snapshot| {
            merge_snapshot_into_hook(
                AIHookEventPayload {
                    ai_session_id: normalized_string(resolved.ai_session_id.as_deref())
                        .or(external_session_id),
                    ..resolved.clone()
                },
                snapshot,
                fallback,
            )
        })
        .unwrap_or(resolved)
}

fn probe_claude_hook_runtime(
    request: &AIRuntimeProbeRequest,
    claude_cache: Option<&mut ClaudeProbeCache>,
) -> Option<AIRuntimeContextSnapshot> {
    if let Some(cache) = claude_cache {
        return probe_runtime_with_claude_cache(request, cache);
    }
    probe_runtime(request)
}

fn resolve_project_probe_hook_event(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
    tool: &str,
) -> AIHookEventPayload {
    let fallback = matching_fallback_session(&event, current_session);
    let resolved = with_fallback(event, fallback);
    if normalized_string(resolved.project_path.as_deref()).is_none() {
        return resolved;
    }
    let request = AIRuntimeProbeRequest {
        terminal_id: resolved.terminal_id.clone(),
        terminal_instance_id: resolved.terminal_instance_id.clone(),
        project_id: resolved.project_id.clone(),
        project_path: resolved.project_path.clone(),
        tool: tool.to_string(),
        external_session_id: normalized_string(resolved.ai_session_id.as_deref())
            .or_else(|| fallback.and_then(|session| session.ai_session_id.clone())),
        transcript_path: resolved
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.transcript_path.as_deref())),
        started_at: fallback
            .and_then(|session| session.started_at)
            .or(Some(resolved.updated_at)),
        updated_at: resolved.updated_at,
        occupied_external_session_ids: Default::default(),
    };
    probe_runtime(&request)
        .map(|snapshot| merge_snapshot_into_hook(resolved.clone(), snapshot, fallback))
        .unwrap_or(resolved)
}

fn matching_fallback_session<'a>(
    event: &AIHookEventPayload,
    current_session: Option<&'a AISessionSnapshot>,
) -> Option<&'a AISessionSnapshot> {
    let session = current_session?;
    if canonical_tool_name(&session.tool) != canonical_tool_name(&event.tool) {
        return None;
    }
    let incoming_session_id = normalized_string(event.ai_session_id.as_deref());
    if incoming_session_id.is_some() && session.ai_session_id != incoming_session_id {
        return None;
    }
    if event.kind == "sessionStarted" && incoming_session_id.is_none() {
        return None;
    }
    Some(session)
}

fn with_fallback(
    mut event: AIHookEventPayload,
    fallback: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    let Some(fallback) = fallback else {
        event.tool = canonical_tool_name(&event.tool).unwrap_or(event.tool);
        return event;
    };
    event.tool = canonical_tool_name(&event.tool).unwrap_or(event.tool);
    event.ai_session_id =
        normalized_string(event.ai_session_id.as_deref()).or(fallback.ai_session_id.clone());
    event.model = normalized_string(event.model.as_deref()).or(fallback.model.clone());
    event.total_tokens = event.total_tokens.or(Some(fallback.total_tokens));
    event
}

pub(in crate::ai_runtime::store) fn merge_snapshot_into_hook(
    event: AIHookEventPayload,
    snapshot: AIRuntimeContextSnapshot,
    fallback: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    let authoritative_codewhale_completion = event.kind == "turnCompleted"
        && canonical_tool_name(&event.tool).as_deref() == Some("codewhale")
        && event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.source.as_deref())
            .is_some_and(|source| source == "codewhale-lifecycle");
    let event_interrupted_completion = event.kind == "turnCompleted"
        && event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.was_interrupted)
            .unwrap_or(false);
    let prompt_turn_started_at = fallback
        .and_then(|session| session.active_turn_started_at.or(session.started_at))
        .unwrap_or(event.updated_at);
    let snapshot_completed_at = snapshot.completed_at.or_else(|| {
        (snapshot.was_interrupted || snapshot.has_completed_turn).then_some(snapshot.updated_at)
    });
    let stale_completion = !event_interrupted_completion
        && !authoritative_codewhale_completion
        && event.kind == "turnCompleted"
        && snapshot.response_state.as_deref() != Some("responding")
        && fallback
            .map(|session| session.state == "responding")
            .unwrap_or(false)
        && snapshot_completed_at
            .map(|completed_at| {
                completed_at + COMPLETION_TIMESTAMP_SKEW_SECONDS < prompt_turn_started_at
            })
            .unwrap_or(false);
    if event.kind == "turnCompleted" {
        runtime_log_line(
            "runtime-probe",
            &format!(
                "turnCompleted probe terminal={} response_state={} completed_at={} updated_at={} prompt_started_at={} has_completed={} stale_completion={} transcript={}",
                event.terminal_id,
                snapshot.response_state.as_deref().unwrap_or("none"),
                snapshot
                    .completed_at
                    .map(|value| format!("{value:.3}"))
                    .unwrap_or_else(|| "none".to_string()),
                snapshot.updated_at,
                prompt_turn_started_at,
                snapshot.has_completed_turn,
                stale_completion,
                snapshot.transcript_path.as_deref().unwrap_or("none")
            ),
        );
    }
    let was_interrupted = if event_interrupted_completion {
        true
    } else {
        snapshot.was_interrupted
            || event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.was_interrupted)
                .unwrap_or(false)
    };
    let has_completed_turn = if event_interrupted_completion {
        event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.has_completed_turn)
            .unwrap_or(false)
    } else {
        snapshot.has_completed_turn
            || event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.has_completed_turn)
                .unwrap_or(!was_interrupted)
    };
    let mut metadata = event.metadata.clone().unwrap_or(AIHookEventMetadata {
        transcript_path: None,
        notification_type: None,
        source: None,
        reason: None,
        cwd: None,
        target_tool_name: None,
        message: None,
        was_interrupted: None,
        has_completed_turn: None,
    });
    metadata.was_interrupted = Some(if stale_completion {
        false
    } else {
        was_interrupted
    });
    metadata.has_completed_turn = Some(if stale_completion {
        false
    } else {
        has_completed_turn
    });
    let total_tokens = if event.kind == "promptSubmitted"
        && canonical_tool_name(&event.tool).as_deref() == Some("codewhale")
    {
        event
            .total_tokens
            .or_else(|| fallback.map(|session| session.total_tokens))
    } else {
        Some(
            event
                .total_tokens
                .unwrap_or(0)
                .max(fallback.map(|session| session.total_tokens).unwrap_or(0))
                .max(snapshot.total_tokens),
        )
    };

    AIHookEventPayload {
        kind: if !event_interrupted_completion
            && !authoritative_codewhale_completion
            && event.kind != "sessionEnded"
            && (snapshot.response_state.as_deref() == Some("responding") || stale_completion)
        {
            "promptSubmitted".to_string()
        } else {
            event.kind
        },
        ai_session_id: normalized_string(event.ai_session_id.as_deref())
            .or_else(|| normalized_string(snapshot.external_session_id.as_deref()))
            .or_else(|| fallback.and_then(|session| session.ai_session_id.clone())),
        model: normalized_string(event.model.as_deref())
            .or_else(|| normalized_string(snapshot.model.as_deref()))
            .or_else(|| fallback.and_then(|session| session.model.clone())),
        input_tokens: Some(number_or(
            event
                .input_tokens
                .or_else(|| fallback.map(|session| session.input_tokens)),
            Some(snapshot.input_tokens),
        )),
        output_tokens: Some(number_or(
            event
                .output_tokens
                .or_else(|| fallback.map(|session| session.output_tokens)),
            Some(snapshot.output_tokens),
        )),
        cached_input_tokens: Some(number_or(
            event
                .cached_input_tokens
                .or_else(|| fallback.map(|session| session.cached_input_tokens)),
            Some(snapshot.cached_input_tokens),
        )),
        total_tokens,
        updated_at: event
            .updated_at
            .max(snapshot.completed_at.unwrap_or(0.0))
            .max(snapshot.updated_at),
        metadata: Some(metadata),
        ..event
    }
}
