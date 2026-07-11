use codux_runtime::{
    ai_history::{
        AIGlobalHistoryRangeSummary, AIGlobalHistorySummary, AIHistorySummary,
        AIProjectUsageSummary, AISessionForkTarget, AISessionSummary,
    },
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

pub(in crate::app) const AI_SESSION_FORK_TARGETS: [AISessionForkTarget; 8] = [
    AISessionForkTarget::Codex,
    AISessionForkTarget::Claude,
    AISessionForkTarget::Agy,
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
    AIHistorySummary {
        indexed: true,
        indexed_at: Some(snapshot.indexed_at),
        is_loading: false,
        queued: false,
        progress: Some(1.0),
        detail: "completed".to_string(),
        project_total_tokens: snapshot.project_summary.project_total_tokens,
        project_cached_input_tokens: snapshot.project_summary.project_cached_input_tokens,
        today_total_tokens: snapshot.project_summary.today_total_tokens,
        today_cached_input_tokens: snapshot.project_summary.today_cached_input_tokens,
        session_count: snapshot.sessions.len(),
        sessions: snapshot
            .sessions
            .into_iter()
            .map(normalized_ai_session_to_summary)
            .collect(),
        heatmap: snapshot.heatmap,
        today_time_buckets: snapshot.today_time_buckets,
        tool_breakdown: snapshot.tool_breakdown,
        model_breakdown: snapshot.model_breakdown,
        error: None,
    }
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
    AIGlobalHistorySummary {
        indexed_project_count: snapshot.project_count,
        session_count: snapshot.sessions.len(),
        total_tokens: snapshot.total_tokens,
        cached_input_tokens: snapshot.cached_input_tokens,
        today_total_tokens: snapshot.today_total_tokens,
        today_cached_input_tokens: snapshot.today_cached_input_tokens,
        project_totals: snapshot
            .project_totals
            .into_iter()
            .map(normalized_project_total_to_summary)
            .collect(),
        heatmap: snapshot.heatmap,
        today_time_buckets: snapshot.today_time_buckets,
        recent_time_buckets: snapshot.recent_time_buckets,
        tool_breakdown: snapshot.tool_breakdown,
        model_breakdown: snapshot.model_breakdown,
        range_summaries: snapshot
            .range_summaries
            .into_iter()
            .map(normalized_range_summary_to_summary)
            .collect(),
        recent_sessions: snapshot
            .sessions
            .into_iter()
            .take(80)
            .map(normalized_ai_session_to_summary)
            .collect(),
        error: None,
    }
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

pub(in crate::app) fn ai_history_project_requests(
    projects: &[ProjectInfo],
) -> Vec<AIHistoryProjectRequest> {
    projects.iter().map(ai_history_project_request).collect()
}

fn normalized_ai_session_to_summary(
    session: codux_runtime::ai_history_normalized::AISessionSummary,
) -> AISessionSummary {
    let session_id = session.session_id;
    AISessionSummary {
        id: session_id.clone(),
        session_key: session
            .external_session_id
            .clone()
            .unwrap_or_else(|| session_id.clone()),
        external_session_id: session.external_session_id,
        title: session.session_title,
        source: session.last_tool.unwrap_or_else(|| "ai".to_string()),
        project_name: Some(session.project_name),
        project_path: Some(session.project_path),
        last_model: session.last_model,
        last_seen_at: session.last_seen_at,
        input_tokens: session.total_input_tokens,
        output_tokens: session.total_output_tokens,
        total_tokens: session.total_tokens,
        cached_input_tokens: session.cached_input_tokens,
        request_count: session.request_count,
        active_duration_seconds: session.active_duration_seconds,
        usage_amounts: session
            .usage_amounts
            .into_iter()
            .map(|amount| codux_runtime::ai_history::AIUsageAmount {
                unit: amount.unit,
                value: amount.value,
            })
            .collect(),
    }
}

fn normalized_project_total_to_summary(
    project: codux_runtime::ai_history_normalized::AIProjectUsageTotal,
) -> AIProjectUsageSummary {
    AIProjectUsageSummary {
        project_id: project.project_id,
        project_path: project.project_path,
        project_name: project.project_name,
        session_count: project.session_count,
        input_tokens: project.input_tokens,
        output_tokens: project.output_tokens,
        total_tokens: project.total_tokens,
        cached_input_tokens: project.cached_input_tokens,
        request_count: project.request_count,
        active_duration_seconds: project.active_duration_seconds,
        today_total_tokens: project.today_total_tokens,
        today_cached_input_tokens: project.today_cached_input_tokens,
    }
}

fn normalized_range_summary_to_summary(
    summary: codux_runtime::ai_history_normalized::AIGlobalHistoryRangeSummary,
) -> AIGlobalHistoryRangeSummary {
    AIGlobalHistoryRangeSummary {
        key: summary.key,
        input_tokens: summary.input_tokens,
        output_tokens: summary.output_tokens,
        total_tokens: summary.total_tokens,
        cached_input_tokens: summary.cached_input_tokens,
        request_count: summary.request_count,
        session_count: summary.session_count,
        active_duration_seconds: summary.active_duration_seconds,
        sessions: summary
            .sessions
            .into_iter()
            .map(normalized_ai_session_to_summary)
            .collect(),
        project_totals: summary
            .project_totals
            .into_iter()
            .map(normalized_project_total_to_summary)
            .collect(),
        tool_breakdown: summary.tool_breakdown,
        model_breakdown: summary.model_breakdown,
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
