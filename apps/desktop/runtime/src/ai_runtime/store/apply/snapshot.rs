use crate::ai_runtime::{
    constants::{COMPLETION_TIMESTAMP_SKEW_SECONDS, RUNNING_STATE_RENEWAL_SECONDS},
    snapshot::{AIRuntimeContextSnapshot, AISessionSnapshot},
    state::{canonical_tool_name, normalized_string, status_for_runtime_state},
    store::{AIRuntimeStateCore, helpers::note_latest_active_started_at, helpers::now_seconds},
};

pub(in crate::ai_runtime::store) fn apply_runtime_snapshot_unlocked(
    core: &mut AIRuntimeStateCore,
    terminal_id: &str,
    snapshot: AIRuntimeContextSnapshot,
) -> bool {
    let Some(session) = core.sessions.get(terminal_id).cloned() else {
        return false;
    };
    let mut snapshot_updated_at = snapshot.updated_at.max(session.updated_at);
    let now = now_seconds();
    if snapshot.response_state.as_deref() == Some("responding")
        && now - session.updated_at >= RUNNING_STATE_RENEWAL_SECONDS
    {
        snapshot_updated_at = snapshot_updated_at.max(now);
    }

    let mut state = session.state.clone();
    let mut was_interrupted = session.was_interrupted;
    let mut has_completed_turn = session.has_completed_turn;
    let mut active_turn_started_at = session.active_turn_started_at;
    let mut runtime_turn_started_at = session.runtime_turn_started_at;
    let mut completed_turn_started_at = session.completed_turn_started_at;
    let snapshot_is_newer = snapshot.updated_at > session.updated_at;
    let prompt_turn_started_at = session
        .active_turn_started_at
        .or(session.started_at)
        .unwrap_or(session.updated_at);

    if snapshot.response_state.as_deref() == Some("responding") {
        if session.state == "responding"
            || (!session.was_interrupted
                && !session.has_completed_turn
                && (snapshot_is_newer || session.state == "idle"))
            || snapshot_started_after_prompt_turn(&snapshot, prompt_turn_started_at)
        {
            state = "responding".to_string();
            was_interrupted = false;
            has_completed_turn = false;
            completed_turn_started_at = None;
            let started = runtime_turn_started_at_for_responding_snapshot(
                &snapshot,
                prompt_turn_started_at,
                snapshot_updated_at,
            );
            active_turn_started_at = Some(started);
            runtime_turn_started_at = Some(started);
        }
    } else if snapshot.response_state.as_deref() == Some("idle")
        && (session.state == "responding"
            || session.state == "needsInput"
            || snapshot.was_interrupted
            || snapshot.has_completed_turn)
    {
        let turn_completed_at = snapshot.completed_at.or_else(|| {
            (snapshot.was_interrupted || snapshot.has_completed_turn).then_some(snapshot.updated_at)
        });
        let can_resolve_idle = if snapshot.was_interrupted || snapshot.has_completed_turn {
            turn_completed_at
                .map(|completed_at| {
                    completed_at + COMPLETION_TIMESTAMP_SKEW_SECONDS >= prompt_turn_started_at
                })
                .unwrap_or(false)
        } else if session.state == "needsInput" {
            true
        } else if let Some(observed_started_at) = session.runtime_turn_started_at {
            observed_started_at >= prompt_turn_started_at
                && snapshot.updated_at >= observed_started_at
        } else {
            false
        };
        if can_resolve_idle {
            state = "idle".to_string();
            active_turn_started_at = None;
            runtime_turn_started_at = None;
            was_interrupted = snapshot.was_interrupted;
            has_completed_turn = snapshot.has_completed_turn || !was_interrupted;
            completed_turn_started_at = session
                .completed_turn_started_at
                .or(session.active_turn_started_at)
                .or(session.runtime_turn_started_at)
                .or(session.started_at)
                .or(turn_completed_at);
        } else if session.has_completed_turn || session.was_interrupted {
            completed_turn_started_at = session
                .completed_turn_started_at
                .or(session.active_turn_started_at)
                .or(session.runtime_turn_started_at)
                .or(session.started_at)
                .or(turn_completed_at);
        }
    }

    if let Some(started_at) = active_turn_started_at {
        note_latest_active_started_at(core, &session.project_id, started_at);
    }

    let (baseline_total_tokens, baseline_cached_input_tokens, baseline_resolved) =
        if session.baseline_resolved {
            (
                session.baseline_total_tokens,
                session.baseline_cached_input_tokens,
                true,
            )
        } else if snapshot.session_origin == "restored" {
            (
                snapshot.total_tokens.max(0),
                snapshot.cached_input_tokens.max(0),
                true,
            )
        } else {
            (
                session.baseline_total_tokens,
                session.baseline_cached_input_tokens,
                true,
            )
        };

    let next = AISessionSnapshot {
        tool: canonical_tool_name(&snapshot.tool).unwrap_or(session.tool.clone()),
        ai_session_id: normalized_string(snapshot.external_session_id.as_deref())
            .or(session.ai_session_id.clone()),
        transcript_path: normalized_string(snapshot.transcript_path.as_deref())
            .or(session.transcript_path.clone()),
        model: normalized_string(snapshot.model.as_deref()).or(session.model.clone()),
        state: state.clone(),
        status: status_for_runtime_state(&state).to_string(),
        is_running: state == "responding",
        input_tokens: session.input_tokens.max(snapshot.input_tokens.max(0)),
        output_tokens: session.output_tokens.max(snapshot.output_tokens.max(0)),
        cached_input_tokens: session
            .cached_input_tokens
            .max(snapshot.cached_input_tokens.max(0)),
        total_tokens: session.total_tokens.max(snapshot.total_tokens.max(0)),
        baseline_total_tokens,
        baseline_cached_input_tokens,
        baseline_resolved,
        updated_at: snapshot_updated_at,
        active_turn_started_at,
        runtime_turn_started_at,
        completed_turn_started_at,
        was_interrupted,
        has_completed_turn,
        latest_assistant_preview: normalized_string(snapshot.assistant_preview.as_deref())
            .or(session.latest_assistant_preview.clone()),
        plan: snapshot.plan.or(session.plan.clone()),
        ..session
    };

    if let Some(ai_session_id) = next.ai_session_id.as_ref() {
        let key = format!("{}:{ai_session_id}", next.tool);
        core.logical_baselines
            .entry(key.clone())
            .or_insert(next.baseline_total_tokens);
        core.logical_cached_baselines
            .entry(key)
            .or_insert(next.baseline_cached_input_tokens);
    }

    if core.sessions.get(terminal_id) == Some(&next) {
        return false;
    }
    core.sessions.insert(terminal_id.to_string(), next);
    true
}

fn snapshot_started_after_prompt_turn(
    snapshot: &AIRuntimeContextSnapshot,
    prompt_turn_started_at: f64,
) -> bool {
    snapshot
        .started_at
        .map(|started_at| started_at >= prompt_turn_started_at)
        .unwrap_or(false)
}

fn runtime_turn_started_at_for_responding_snapshot(
    snapshot: &AIRuntimeContextSnapshot,
    prompt_turn_started_at: f64,
    fallback: f64,
) -> f64 {
    if let Some(started_at) = snapshot.started_at {
        if started_at >= prompt_turn_started_at {
            return started_at;
        }
    }
    snapshot.updated_at.max(fallback)
}
