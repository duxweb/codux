use crate::ai_runtime::{
    constants::{
        CLAUDE_STALE_PRELAUNCH_OPEN_TURN_SOURCE, CODEX_STALE_PRELAUNCH_OPEN_TURN_SOURCE,
        COMPLETION_TIMESTAMP_SKEW_SECONDS, RESPONDING_RENEWAL_MAX_SECONDS,
        RUNNING_STATE_RENEWAL_SECONDS,
    },
    screen_signal::ScreenSignal,
    snapshot::{AIRuntimeContextSnapshot, AISessionSnapshot, AIUsageAmountSnapshot},
    state::{canonical_tool_name, normalized_string, status_for_runtime_state},
    store::{AIRuntimeStateCore, helpers::note_latest_active_started_at, helpers::now_seconds},
};

/// Apply the universal screen-scrape signal. For most tools this only refines an
/// already-active turn between `responding` and `needsInput`. Kiro is the
/// exception: current Kiro CLI writes its json/jsonl only after the turn
/// completes, so the rendered "Thinking... (esc to cancel)" footer is the only
/// authoritative live-start signal.
pub(in crate::ai_runtime::store) fn apply_screen_signal_unlocked(
    core: &mut AIRuntimeStateCore,
    terminal_id: &str,
    signal: ScreenSignal,
    allow_idle_start: bool,
) -> bool {
    let Some(session) = core.sessions.get(terminal_id).cloned() else {
        return false;
    };
    let next_state = match signal {
        ScreenSignal::Waiting if session.state == "responding" => "needsInput",
        ScreenSignal::Running if session.state == "needsInput" => "responding",
        ScreenSignal::Running if allow_idle_start && session.state == "idle" => "responding",
        _ => return false,
    };
    let mut next = session;
    next.state = next_state.to_string();
    next.status = status_for_runtime_state(next_state).to_string();
    next.is_running = next_state == "responding";
    if next_state == "responding" {
        let now = now_seconds();
        next.active_turn_started_at = next.active_turn_started_at.or(Some(now));
        next.runtime_turn_started_at = next.runtime_turn_started_at.or(Some(now));
        next.updated_at = next.updated_at.max(now);
        next.was_interrupted = false;
        next.has_completed_turn = false;
        next.completed_turn_started_at = None;
        note_latest_active_started_at(core, &next.project_id, now);
    }
    if core.sessions.get(terminal_id) == Some(&next) {
        return false;
    }
    core.sessions.insert(terminal_id.to_string(), next);
    true
}

pub(in crate::ai_runtime::store) fn apply_runtime_snapshot_unlocked(
    core: &mut AIRuntimeStateCore,
    terminal_id: &str,
    snapshot: AIRuntimeContextSnapshot,
) -> bool {
    let Some(session) = core.sessions.get(terminal_id).cloned() else {
        return false;
    };
    let snapshot_tool = canonical_tool_name(&snapshot.tool).unwrap_or_else(|| session.tool.clone());
    let codewhale_finished_turn_is_authoritative = snapshot_tool == "codewhale"
        && session.state == "idle"
        && (session.was_interrupted || session.has_completed_turn);
    if codewhale_finished_turn_is_authoritative
        && snapshot.response_state.as_deref() == Some("responding")
    {
        return false;
    }

    let mut snapshot_updated_at = snapshot.updated_at.max(session.updated_at);
    let now = now_seconds();
    if snapshot.response_state.as_deref() == Some("responding")
        && now - session.updated_at >= RUNNING_STATE_RENEWAL_SECONDS
    {
        // Renew the heartbeat across quiet gaps within a genuinely long turn,
        // but only while the turn is younger than the renewal ceiling. Past it,
        // stop synthesizing fresh activity so a session whose transcript merely
        // *looks* like it is still responding (e.g. the CLI was killed mid-turn
        // while the terminal tab stayed open) finally goes stale and gets aged
        // out by reconcile_bridge_snapshot instead of pinning the pet bubble.
        let turn_started_at = session
            .runtime_turn_started_at
            .or(session.active_turn_started_at)
            .or(session.started_at)
            .unwrap_or(session.updated_at);
        if now - turn_started_at <= RESPONDING_RENEWAL_MAX_SECONDS {
            snapshot_updated_at = snapshot_updated_at.max(now);
        }
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
            // Recover a permission-wait (needsInput) once the transcript shows
            // fresh activity after the wait: the user approved and Claude
            // resumed, but Claude emits no "granted" hook. `snapshot_is_newer`
            // (the log advanced past when the wait was recorded; never inflated
            // to `now` — the probe request carries the session's own updated_at)
            // distinguishes a genuine resume from a still-pending prompt, and is
            // robust to the started_at/hook-clock skew the clause below is not.
            || (session.state == "needsInput" && snapshot_is_newer)
            || (!codewhale_finished_turn_is_authoritative
                && snapshot_started_after_prompt_turn(&snapshot, prompt_turn_started_at))
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
    } else if snapshot.response_state.as_deref() == Some("needsInput")
        && (session.state == "responding"
            || session.state == "needsInput"
            || (!session.was_interrupted
                && !session.has_completed_turn
                && (snapshot_is_newer || session.state == "idle"))
            || snapshot_started_after_prompt_turn(&snapshot, prompt_turn_started_at))
    {
        // Pure-file permission/elicitation wait surfaced by the probe: a tool
        // call is written with no result, the mode can still prompt, and the
        // transcript has been idle past the threshold. The turn is still open --
        // just blocked on the user -- so keep its timers and stay out of the
        // completed/interrupted states. Recovery back to `responding` (user
        // approved, log advanced) and resolution to `idle` (turn ended) are both
        // already handled by the branches above/below.
        state = "needsInput".to_string();
        was_interrupted = false;
        has_completed_turn = false;
        completed_turn_started_at = None;
        let started = session
            .active_turn_started_at
            .or(session.runtime_turn_started_at)
            .or(session.started_at)
            .unwrap_or(prompt_turn_started_at);
        active_turn_started_at = Some(started);
        runtime_turn_started_at = Some(started);
    } else if snapshot.response_state.as_deref() == Some("idle")
        && (session.state == "responding"
            || session.state == "needsInput"
            || snapshot.was_interrupted
            || snapshot.has_completed_turn)
    {
        let turn_completed_at = snapshot.completed_at.or_else(|| {
            (snapshot.was_interrupted || snapshot.has_completed_turn).then_some(snapshot.updated_at)
        });
        let silent_stale_prelaunch_open_turn = is_silent_stale_prelaunch_open_turn(&snapshot);
        let can_resolve_idle = if silent_stale_prelaunch_open_turn {
            true
        } else if snapshot_tool == "kiro" && snapshot.has_completed_turn {
            true
        } else if snapshot.was_interrupted || snapshot.has_completed_turn {
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
            was_interrupted = !silent_stale_prelaunch_open_turn && snapshot.was_interrupted;
            has_completed_turn = !silent_stale_prelaunch_open_turn
                && (snapshot.has_completed_turn || !was_interrupted);
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
    } else if snapshot.response_state.as_deref() == Some("idle")
        && is_silent_stale_prelaunch_open_turn(&snapshot)
        && session.state == "idle"
    {
        was_interrupted = false;
        has_completed_turn = false;
    }

    if let Some(started_at) = active_turn_started_at {
        note_latest_active_started_at(core, &session.project_id, started_at);
    }

    let (
        baseline_total_tokens,
        baseline_cached_input_tokens,
        baseline_usage_amounts,
        baseline_resolved,
    ) = if !snapshot.baseline_usage_amounts.is_empty() {
        (
            session.baseline_total_tokens,
            session.baseline_cached_input_tokens,
            max_usage_amounts(
                &session.baseline_usage_amounts,
                &snapshot.baseline_usage_amounts,
            ),
            true,
        )
    } else if session.baseline_resolved {
        (
            session.baseline_total_tokens,
            session.baseline_cached_input_tokens,
            session.baseline_usage_amounts.clone(),
            true,
        )
    } else if snapshot.session_origin == "restored"
        || binding_marks_restored_session(&session, &snapshot)
        || first_snapshot_is_prelaunch_history(&session, &snapshot)
    {
        (
            snapshot.total_tokens.max(0),
            snapshot.cached_input_tokens.max(0),
            snapshot.usage_amounts.clone(),
            true,
        )
    } else {
        (
            session.baseline_total_tokens,
            session.baseline_cached_input_tokens,
            session.baseline_usage_amounts.clone(),
            true,
        )
    };

    let next = AISessionSnapshot {
        tool: snapshot_tool,
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
        usage_amounts: max_usage_amounts(&session.usage_amounts, &snapshot.usage_amounts),
        baseline_usage_amounts,
        baseline_resolved,
        session_origin: None,
        updated_at: snapshot_updated_at,
        active_turn_started_at,
        runtime_turn_started_at,
        completed_turn_started_at,
        was_interrupted,
        has_completed_turn,
        latest_assistant_preview: normalized_string(snapshot.assistant_preview.as_deref())
            .or(session.latest_assistant_preview.clone()),
        // Only keep a task plan while the turn is still active; once it resolves
        // to idle, drop it so the pet's task bubble doesn't linger on a finished
        // (all-✓) list. Mirrors the hook path's plan handling.
        plan: if state == "responding" || state == "needsInput" {
            snapshot.plan.or(session.plan.clone())
        } else {
            None
        },
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

fn is_silent_stale_prelaunch_open_turn(snapshot: &AIRuntimeContextSnapshot) -> bool {
    matches!(
        snapshot.source.as_str(),
        CODEX_STALE_PRELAUNCH_OPEN_TURN_SOURCE | CLAUDE_STALE_PRELAUNCH_OPEN_TURN_SOURCE
    )
}

fn max_usage_amounts(
    current: &[AIUsageAmountSnapshot],
    snapshot: &[AIUsageAmountSnapshot],
) -> Vec<AIUsageAmountSnapshot> {
    let mut amounts = current.to_vec();
    for next in snapshot {
        let unit = next.unit.trim();
        if unit.is_empty() || next.value <= 0.0 {
            continue;
        }
        if let Some(existing) = amounts.iter_mut().find(|item| item.unit == next.unit) {
            existing.value = existing.value.max(next.value);
        } else {
            amounts.push(next.clone());
        }
    }
    amounts
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

fn first_snapshot_is_prelaunch_history(
    session: &AISessionSnapshot,
    snapshot: &AIRuntimeContextSnapshot,
) -> bool {
    if session.state != "idle"
        || session.has_completed_turn
        || session.was_interrupted
        || session.active_turn_started_at.is_some()
        || session.runtime_turn_started_at.is_some()
        || session.completed_turn_started_at.is_some()
    {
        return false;
    }
    if matches!(
        snapshot.response_state.as_deref(),
        Some("responding" | "needsInput")
    ) {
        return false;
    }
    let Some(session_started_at) = session.started_at else {
        return false;
    };
    let snapshot_last_activity_at = snapshot.completed_at.or_else(|| {
        (snapshot.has_completed_turn || snapshot.was_interrupted).then_some(snapshot.updated_at)
    });
    snapshot_last_activity_at
        .or(snapshot.started_at)
        .is_some_and(|activity_at| activity_at + 1.0 < session_started_at)
}

fn binding_marks_restored_session(
    session: &AISessionSnapshot,
    snapshot: &AIRuntimeContextSnapshot,
) -> bool {
    if session.session_origin.as_deref() != Some("restored") {
        return false;
    }
    let Some(session_id) = normalized_string(session.ai_session_id.as_deref()) else {
        return false;
    };
    normalized_string(snapshot.external_session_id.as_deref())
        .as_deref()
        .is_some_and(|snapshot_id| snapshot_id == session_id)
}
