use crate::ai_runtime::{
    payload::AIHookEventPayload,
    snapshot::AISessionSnapshot,
    state::{
        canonical_tool_name, normalized_string, runtime_state_for_hook_kind,
        status_for_runtime_state,
    },
    store::{
        AIRuntimeStateCore,
        helpers::{
            is_tool_activity_without_loading, note_latest_active_started_at, now_seconds, number_or,
        },
    },
};

pub(in crate::ai_runtime::store) fn apply_hook_unlocked(
    core: &mut AIRuntimeStateCore,
    event: AIHookEventPayload,
) -> bool {
    let Some(terminal_id) = normalized_string(Some(event.terminal_id.as_str())) else {
        return false;
    };
    let Some(tool) = canonical_tool_name(&event.tool) else {
        return false;
    };

    let previous = core.sessions.get(&terminal_id).cloned();
    let terminal_instance_id = normalized_string(event.terminal_instance_id.as_deref());
    if event.kind == "sessionStarted"
        && previous
            .as_ref()
            .map(|session| event.updated_at < session.updated_at)
            .unwrap_or(false)
    {
        return false;
    }
    if previous
        .as_ref()
        .and_then(|session| session.terminal_instance_id.as_deref())
        .is_some()
        && terminal_instance_id.is_some()
        && previous
            .as_ref()
            .and_then(|session| session.terminal_instance_id.as_deref())
            != terminal_instance_id.as_deref()
        && event.updated_at
            < previous
                .as_ref()
                .map(|session| session.updated_at)
                .unwrap_or(0.0)
    {
        return false;
    }
    if is_tool_activity_without_loading(&event, previous.as_ref()) {
        return false;
    }

    let now = if event.updated_at > 0.0 {
        event.updated_at
    } else {
        now_seconds()
    };
    let should_reset = previous.as_ref().is_some_and(|session| {
        session.tool != tool
            || (session.terminal_instance_id.is_some()
                && terminal_instance_id.is_some()
                && session.terminal_instance_id != terminal_instance_id)
            || (session.ai_session_id.is_some()
                && normalized_string(event.ai_session_id.as_deref()).is_some()
                && session.ai_session_id != normalized_string(event.ai_session_id.as_deref()))
    });
    let base = if should_reset {
        None
    } else {
        previous.as_ref()
    };
    let ai_session_id = normalized_string(event.ai_session_id.as_deref())
        .or_else(|| base.and_then(|session| session.ai_session_id.clone()));
    let logical_key = ai_session_id
        .as_ref()
        .map(|session_id| format!("{tool}:{session_id}"));
    let total_tokens = number_or(base.map(|session| session.total_tokens), event.total_tokens);
    let cached_input_tokens = number_or(
        base.map(|session| session.cached_input_tokens),
        event.cached_input_tokens,
    );
    let baseline_total_tokens = base
        .map(|session| session.baseline_total_tokens)
        .or_else(|| {
            logical_key
                .as_ref()
                .and_then(|key| core.logical_baselines.get(key).copied())
        })
        .unwrap_or(total_tokens);
    let baseline_cached_input_tokens = base
        .map(|session| session.baseline_cached_input_tokens)
        .or_else(|| {
            logical_key
                .as_ref()
                .and_then(|key| core.logical_cached_baselines.get(key).copied())
        })
        .unwrap_or(cached_input_tokens);
    if let Some(key) = logical_key {
        core.logical_baselines
            .entry(key.clone())
            .or_insert(baseline_total_tokens);
        core.logical_cached_baselines
            .entry(key)
            .or_insert(baseline_cached_input_tokens);
    }

    let state = runtime_state_for_hook_kind(&event.kind, event.metadata.as_ref());
    let was_interrupted = if event.kind == "turnCompleted" || event.kind == "sessionEnded" {
        event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.was_interrupted)
            .unwrap_or(false)
    } else if state == "responding" || state == "needsInput" {
        false
    } else {
        base.map(|session| session.was_interrupted).unwrap_or(false)
    };
    let has_completed_turn = if event.kind == "turnCompleted" {
        event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.has_completed_turn)
            .unwrap_or(true)
    } else if event.kind == "sessionEnded" {
        base.map(|session| session.has_completed_turn)
            .unwrap_or(false)
    } else if event.kind == "sessionStarted" {
        false
    } else {
        base.map(|session| session.has_completed_turn)
            .unwrap_or(false)
    };

    if event.kind == "sessionEnded"
        && base
            .map(|session| !session.has_completed_turn)
            .unwrap_or(false)
    {
        core.sessions.remove(&terminal_id);
        return true;
    }

    let active_turn_started_at = if state == "responding" || state == "needsInput" {
        base.and_then(|session| session.active_turn_started_at)
            .or(Some(now))
    } else {
        None
    };
    let completed_turn_started_at =
        if state == "responding" || state == "needsInput" || event.kind == "sessionStarted" {
            None
        } else if event.kind == "turnCompleted" || event.kind == "sessionEnded" {
            base.and_then(|session| session.active_turn_started_at)
                .or_else(|| base.and_then(|session| session.runtime_turn_started_at))
                .or_else(|| base.and_then(|session| session.started_at))
                .or(Some(now))
        } else {
            base.and_then(|session| session.completed_turn_started_at)
        };
    if let Some(started_at) = active_turn_started_at {
        note_latest_active_started_at(core, &event.project_id, started_at);
    }

    let next = AISessionSnapshot {
        terminal_id: terminal_id.clone(),
        terminal_instance_id: terminal_instance_id
            .or_else(|| base.and_then(|session| session.terminal_instance_id.clone())),
        project_id: event.project_id.clone(),
        project_name: if event.project_name.trim().is_empty() {
            base.map(|session| session.project_name.clone())
                .unwrap_or_else(|| "Workspace".to_string())
        } else {
            event.project_name.clone()
        },
        project_path: normalized_string(event.project_path.as_deref())
            .or_else(|| base.and_then(|session| session.project_path.clone())),
        session_title: if event.session_title.trim().is_empty() {
            base.map(|session| session.session_title.clone())
                .unwrap_or_else(|| "Terminal".to_string())
        } else {
            event.session_title.clone()
        },
        tool,
        ai_session_id,
        model: normalized_string(event.model.as_deref())
            .or_else(|| base.and_then(|session| session.model.clone())),
        state: state.to_string(),
        status: status_for_runtime_state(state).to_string(),
        is_running: state == "responding",
        input_tokens: number_or(base.map(|session| session.input_tokens), event.input_tokens),
        output_tokens: number_or(
            base.map(|session| session.output_tokens),
            event.output_tokens,
        ),
        cached_input_tokens,
        total_tokens,
        baseline_total_tokens,
        baseline_cached_input_tokens,
        baseline_resolved: base
            .map(|session| session.baseline_resolved)
            .unwrap_or(false),
        started_at: base.and_then(|session| session.started_at).or(Some(now)),
        updated_at: base
            .map(|session| session.updated_at)
            .unwrap_or(0.0)
            .max(now),
        active_turn_started_at,
        runtime_turn_started_at: if state == "responding" {
            base.and_then(|session| session.runtime_turn_started_at)
        } else {
            None
        },
        completed_turn_started_at,
        has_completed_turn,
        was_interrupted,
        transcript_path: event
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.transcript_path.as_deref()))
            .or_else(|| base.and_then(|session| session.transcript_path.clone())),
        notification_type: event
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.notification_type.as_deref())),
        target_tool_name: event
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.target_tool_name.as_deref())),
        message: event
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.message.as_deref())),
        latest_assistant_preview: if state == "idle" {
            None
        } else {
            base.and_then(|session| session.latest_assistant_preview.clone())
        },
        plan: if state == "responding" || state == "needsInput" {
            base.and_then(|session| session.plan.clone())
        } else {
            None
        },
    };

    if previous.as_ref() == Some(&next) {
        return false;
    }
    core.sessions.insert(terminal_id, next);
    true
}
