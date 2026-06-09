use super::{AIRuntimeStateCore, helpers::now_seconds};
use crate::ai_runtime::log::runtime_log_line;
use crate::ai_runtime::snapshot::{
    AILatestCompletion, AIProjectPhase, AIProjectStateSnapshot, AIProjectTotals,
    AIRuntimeCompletionEvent, AIRuntimeStateSnapshot, AISessionSnapshot,
};
use std::collections::HashSet;

const NEEDS_INPUT_VISIBLE_SECONDS: f64 = 30.0;

pub(super) fn state_snapshot_unlocked(core: &AIRuntimeStateCore) -> AIRuntimeStateSnapshot {
    let now = now_seconds();
    let mut sessions = core
        .sessions
        .values()
        .cloned()
        .map(|session| visible_session_snapshot(session, now))
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.updated_at.total_cmp(&left.updated_at));
    let mut project_ids = sessions
        .iter()
        .map(|session| session.project_id.clone())
        .collect::<Vec<_>>();
    project_ids.sort();
    project_ids.dedup();

    let projects = project_ids
        .iter()
        .map(|project_id| AIProjectStateSnapshot {
            project_id: project_id.clone(),
            project_phase: project_phase_unlocked(core, project_id, now),
            completed_phase: completed_phase_unlocked(core, project_id, now),
            totals: project_totals_unlocked(core, Some(project_id), now),
        })
        .collect::<Vec<_>>();
    let needs_input_count = projects
        .iter()
        .filter(|project| matches!(project.project_phase, AIProjectPhase::NeedsInput { .. }))
        .count();
    let running_count = projects
        .iter()
        .filter(|project| matches!(project.project_phase, AIProjectPhase::Running { .. }))
        .count();
    let completion_count = projects
        .iter()
        .filter(|project| matches!(project.completed_phase, AIProjectPhase::Completed { .. }))
        .count();
    AIRuntimeStateSnapshot {
        sessions,
        projects,
        global_totals: project_totals_unlocked(core, None, now),
        needs_input_count,
        running_count,
        completion_count,
        latest_completion: latest_completion_unlocked(core, now),
        updated_at: now,
    }
}

fn project_phase_unlocked(core: &AIRuntimeStateCore, project_id: &str, now: f64) -> AIProjectPhase {
    let sessions = sorted_project_sessions(core, project_id);
    if let Some(session) = sessions
        .iter()
        .find(|session| session_has_active_needs_input(session, now))
    {
        return AIProjectPhase::NeedsInput {
            tool: session.tool.clone(),
        };
    }
    if let Some(session) = sessions
        .iter()
        .find(|session| session.state == "responding")
    {
        return AIProjectPhase::Running {
            tool: session.tool.clone(),
        };
    }
    AIProjectPhase::Idle
}

pub(super) fn completed_phase_unlocked(
    core: &AIRuntimeStateCore,
    project_id: &str,
    now: f64,
) -> AIProjectPhase {
    let sessions = sorted_project_sessions(core, project_id);
    if sessions
        .iter()
        .any(|session| session_has_active_needs_input(session, now))
    {
        return AIProjectPhase::Idle;
    }
    let latest_active_started_at = core
        .latest_active_started_at_by_project
        .get(project_id)
        .copied()
        .unwrap_or(0.0);
    let completed = sessions.iter().find(|session| {
        session.state == "idle"
            && (session.has_completed_turn || session.was_interrupted)
            && session.updated_at >= latest_active_started_at
    });
    let Some(completed) = completed else {
        return AIProjectPhase::Idle;
    };
    let dismissed_at = core
        .dismissed_completed_at
        .get(project_id)
        .copied()
        .unwrap_or(0.0);
    if completed.updated_at <= dismissed_at {
        return AIProjectPhase::Idle;
    }
    AIProjectPhase::Completed {
        tool: completed.tool.clone(),
        was_interrupted: completed.was_interrupted,
        updated_at: completed.updated_at,
    }
}

pub(super) fn project_totals_unlocked(
    core: &AIRuntimeStateCore,
    project_id: Option<&str>,
    now: f64,
) -> AIProjectTotals {
    core.sessions
        .values()
        .filter(|session| {
            project_id
                .map(|project_id| session.project_id == project_id)
                .unwrap_or(true)
        })
        .fold(AIProjectTotals::default(), |mut total, session| {
            total.total_tokens += (session.total_tokens - session.baseline_total_tokens).max(0);
            total.cached_input_tokens +=
                (session.cached_input_tokens - session.baseline_cached_input_tokens).max(0);
            total.running += usize::from(session.state == "responding");
            total.needs_input += usize::from(session_has_active_needs_input(session, now));
            total.completed += usize::from(session.has_completed_turn);
            total
        })
}

fn latest_completion_unlocked(core: &AIRuntimeStateCore, now: f64) -> Option<AILatestCompletion> {
    let mut latest = None;
    for project_id in core
        .sessions
        .values()
        .map(|session| session.project_id.clone())
        .collect::<HashSet<_>>()
    {
        let AIProjectPhase::Completed {
            tool,
            was_interrupted,
            updated_at,
        } = completed_phase_unlocked(core, &project_id, now)
        else {
            continue;
        };
        let project_name = core
            .sessions
            .values()
            .find(|session| session.project_id == project_id)
            .map(|session| session.project_name.clone())
            .unwrap_or_else(|| project_id.clone());
        let candidate = AILatestCompletion {
            id: format!("{project_id}:{updated_at}"),
            project_id,
            project_name,
            tool,
            was_interrupted,
            updated_at,
        };
        if latest
            .as_ref()
            .map(|current: &AILatestCompletion| candidate.updated_at > current.updated_at)
            .unwrap_or(true)
        {
            latest = Some(candidate);
        }
    }
    latest
}

pub(super) fn next_completion_event_unlocked(
    core: &mut AIRuntimeStateCore,
) -> Option<AIRuntimeCompletionEvent> {
    let latest = latest_completion_unlocked(core, now_seconds())?;
    let session = core
        .sessions
        .values()
        .filter(|session| session.project_id == latest.project_id)
        .filter(|session| session.state == "idle")
        .filter(|session| session.has_completed_turn || session.was_interrupted)
        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
        .cloned();
    let Some(session) = session else {
        runtime_log_line(
            "runtime-state",
            &format!(
                "completion skipped reason=session-missing project={} updated_at={:.3}",
                latest.project_id, latest.updated_at
            ),
        );
        return None;
    };
    let completion_key = completion_event_key(&session);
    if !core.notified_completion_keys.insert(completion_key.clone()) {
        runtime_log_line(
            "runtime-state",
            &format!(
                "completion dedupe key={} project={} updated_at={:.3}",
                completion_key, latest.project_id, latest.updated_at
            ),
        );
        return None;
    }
    runtime_log_line(
        "runtime-state",
        &format!(
            "completion emit key={} project={} terminal={} session={} updated_at={:.3}",
            completion_key,
            latest.project_id,
            session.terminal_id,
            session.ai_session_id.as_deref().unwrap_or("none"),
            latest.updated_at
        ),
    );
    Some(AIRuntimeCompletionEvent {
        id: completion_key,
        project_name: latest.project_name,
        tool: latest.tool,
        was_interrupted: latest.was_interrupted,
        session: Some(session),
    })
}

fn session_has_active_needs_input(session: &AISessionSnapshot, now: f64) -> bool {
    session.state == "needsInput" && now - session.updated_at <= NEEDS_INPUT_VISIBLE_SECONDS
}

fn visible_session_snapshot(mut session: AISessionSnapshot, now: f64) -> AISessionSnapshot {
    if session.state == "needsInput" && !session_has_active_needs_input(&session, now) {
        session.state = "idle".to_string();
        session.status = "idle".to_string();
        session.notification_type = None;
        session.target_tool_name = None;
        session.message = None;
    }
    session
}

fn completion_event_key(session: &AISessionSnapshot) -> String {
    let session_identity = session
        .ai_session_id
        .as_deref()
        .unwrap_or(session.terminal_id.as_str());
    let turn_identity = session
        .completed_turn_started_at
        .or(session.active_turn_started_at)
        .or(session.runtime_turn_started_at)
        .or(session.started_at)
        .unwrap_or(session.updated_at);
    format!(
        "{}:{}:{}:{:.3}",
        session.project_id, session.tool, session_identity, turn_identity
    )
}

fn sorted_project_sessions<'a>(
    core: &'a AIRuntimeStateCore,
    project_id: &str,
) -> Vec<&'a AISessionSnapshot> {
    let mut sessions = core
        .sessions
        .values()
        .filter(|session| session.project_id == project_id)
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.updated_at.total_cmp(&left.updated_at));
    sessions
}
