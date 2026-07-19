use codux_runtime::{
    ai_history::{AIGlobalHistorySummary, AIHistorySummary, AISessionForkTarget, AISessionSummary},
    ai_history_indexer::AIHistoryProjectState,
    ai_history_normalized::{AIGlobalHistorySnapshot, AIHistoryProjectRequest, AIHistorySnapshot},
    runtime_state::ProjectInfo,
};

use super::shell_utils::shell_read_file_arg;

pub(in crate::app) fn ai_session_restore_command(session: &AISessionSummary) -> String {
    // Single source of truth lives in the shared sessions crate so the desktop
    // "open" action and the remote `ai.session` `restore` op never drift.
    codux_runtime::ai_history::session_restore_command(session)
}

pub(in crate::app) const AI_SESSION_FORK_TARGETS: [AISessionForkTarget; 9] = [
    AISessionForkTarget::Codex,
    AISessionForkTarget::Claude,
    AISessionForkTarget::Agy,
    AISessionForkTarget::Omp,
    AISessionForkTarget::OpenCode,
    AISessionForkTarget::Kiro,
    AISessionForkTarget::CodeWhale,
    AISessionForkTarget::Kimi,
    AISessionForkTarget::MiMo,
];

pub(in crate::app) fn ai_session_fork_command(
    target: AISessionForkTarget,
    prompt_path: &str,
) -> String {
    let prompt = shell_read_file_arg(prompt_path);
    match target {
        AISessionForkTarget::Codex => format!("codex {prompt}"),
        AISessionForkTarget::Claude => format!("claude {prompt}"),
        AISessionForkTarget::Agy => format!("agy {prompt}"),
        AISessionForkTarget::Omp => format!("omp {prompt}"),
        AISessionForkTarget::OpenCode => format!("opencode run {prompt}"),
        AISessionForkTarget::Kiro => format!("kiro-cli {prompt}"),
        AISessionForkTarget::CodeWhale => format!("codewhale {prompt}"),
        AISessionForkTarget::Kimi => format!("kimi {prompt}"),
        AISessionForkTarget::MiMo => format!("mimo run {prompt}"),
    }
}

pub(in crate::app) fn normalized_ai_history_snapshot_to_summary(
    snapshot: AIHistorySnapshot,
) -> AIHistorySummary {
    codux_runtime::ai_history::project_summary_from_normalized_snapshot(snapshot)
}

pub(in crate::app) fn ai_history_summary_from_project_state(
    state: &AIHistoryProjectState,
) -> Option<AIHistorySummary> {
    let mut summary = state
        .snapshot
        .clone()
        .map(normalized_ai_history_snapshot_to_summary)?;
    apply_ai_history_project_state(&mut summary, state);
    Some(summary)
}

pub(in crate::app) fn ai_history_summary_from_state_or_status(
    current: &AIHistorySummary,
    state: &AIHistoryProjectState,
) -> AIHistorySummary {
    ai_history_summary_from_project_state(state).unwrap_or_else(|| {
        let mut summary = current.clone();
        apply_ai_history_project_state(&mut summary, state);
        summary
    })
}

pub(in crate::app) fn ai_history_should_replace(
    current: &AIHistorySummary,
    next: &AIHistorySummary,
) -> bool {
    if !next.indexed {
        return !current.indexed || current.sessions.is_empty();
    }
    if !current.indexed || current.sessions.is_empty() {
        return true;
    }
    let current_indexed_at = current.indexed_at.unwrap_or(0.0);
    let next_indexed_at = next.indexed_at.unwrap_or(0.0);
    if next_indexed_at + f64::EPSILON < current_indexed_at {
        return false;
    }
    let current_latest_session = latest_session_seen_at(current);
    let next_latest_session = latest_session_seen_at(next);
    next_latest_session + f64::EPSILON >= current_latest_session
}

pub(in crate::app) fn apply_ai_history_project_state(
    summary: &mut AIHistorySummary,
    state: &AIHistoryProjectState,
) {
    summary.is_loading = state.is_loading;
    summary.queued = state.queued;
    summary.progress = state.progress;
    summary.detail = state.detail.clone();
    summary.error = state.error.clone();
}

pub(in crate::app) fn normalized_global_ai_history_snapshot_to_summary(
    snapshot: AIGlobalHistorySnapshot,
) -> AIGlobalHistorySummary {
    codux_runtime::ai_history::global_summary_from_normalized_snapshot(snapshot)
}

pub(in crate::app) fn ai_history_project_request(project: &ProjectInfo) -> AIHistoryProjectRequest {
    AIHistoryProjectRequest {
        id: project.id.clone(),
        name: project.name.clone(),
        path: project.path.clone(),
    }
}

pub(in crate::app) fn ai_history_worktree_request(
    project: &ProjectInfo,
    worktree: Option<&crate::app::WorktreeInfo>,
) -> AIHistoryProjectRequest {
    let Some(worktree) = worktree else {
        return ai_history_project_request(project);
    };
    AIHistoryProjectRequest {
        id: worktree.id.clone(),
        name: worktree.name.clone(),
        path: worktree.path.clone(),
    }
}

fn latest_session_seen_at(summary: &AIHistorySummary) -> f64 {
    summary
        .sessions
        .iter()
        .map(|session| session.last_seen_at)
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(indexed_at: f64, last_seen_at: f64) -> AIHistorySummary {
        AIHistorySummary {
            indexed: true,
            indexed_at: Some(indexed_at),
            sessions: vec![AISessionSummary {
                id: "session".to_string(),
                session_key: "session".to_string(),
                external_session_id: Some("session".to_string()),
                title: "Session".to_string(),
                source: "codex".to_string(),
                project_name: None,
                project_path: None,
                last_model: None,
                last_seen_at,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 1,
                cached_input_tokens: 0,
                request_count: 1,
                active_duration_seconds: 0,
                usage_amounts: Vec::new(),
            }],
            ..AIHistorySummary::default()
        }
    }

    #[test]
    fn ai_history_replace_rejects_older_snapshot() {
        let current = history(20.0, 200.0);
        let older = history(10.0, 100.0);
        assert!(!ai_history_should_replace(&current, &older));
    }

    #[test]
    fn ai_history_replace_accepts_newer_snapshot() {
        let current = history(10.0, 100.0);
        let newer = history(20.0, 200.0);
        assert!(ai_history_should_replace(&current, &newer));
    }

    #[test]
    fn ai_history_replace_keeps_existing_sessions_for_status_only_update() {
        let current = history(10.0, 100.0);
        let status_only = AIHistorySummary {
            indexed: false,
            is_loading: true,
            detail: "queued".to_string(),
            ..AIHistorySummary::default()
        };
        assert!(!ai_history_should_replace(&current, &status_only));
    }
}
