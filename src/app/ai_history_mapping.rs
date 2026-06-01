use codux_runtime::{
    ai_history::{AIGlobalHistorySummary, AIHistorySummary, AISessionSummary},
    ai_history_indexer::AIHistoryProjectState,
    ai_history_normalized::{AIGlobalHistorySnapshot, AIHistoryProjectRequest, AIHistorySnapshot},
    runtime_state::ProjectInfo,
};

use super::shell_quote;

pub(in crate::app) fn ai_session_restore_command(session: &AISessionSummary) -> String {
    let tool = session.source.to_lowercase();
    let id = session
        .external_session_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or(&session.session_key);
    let quoted_id = shell_quote(id);
    if tool.contains("codex") {
        format!("codex resume {quoted_id}")
    } else if tool.contains("claude") {
        format!("claude --resume {quoted_id}")
    } else if tool.contains("agy") || tool.contains("antigravity") {
        format!("agy resume {quoted_id}")
    } else if tool.contains("gemini") {
        format!("gemini resume {quoted_id}")
    } else if tool.contains("opencode") {
        format!("opencode run --session {quoted_id}")
    } else {
        format!("codex resume {quoted_id}")
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
    apply_ai_history_project_state(&mut summary, &state);
    Some(summary)
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
        project_totals: Vec::new(),
        recent_sessions: snapshot
            .sessions
            .into_iter()
            .take(10)
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
        last_model: session.last_model,
        last_seen_at: session.last_seen_at,
        total_tokens: session.total_tokens,
        cached_input_tokens: session.cached_input_tokens,
        request_count: session.request_count,
    }
}
