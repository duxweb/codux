use super::store::OperationStore;
use crate::ai_runtime::{
    TerminalStatusState,
    terminal_activity::{TerminalActivityEvent, TerminalActivitySubscription},
};
use codux_runtime_core::agent_worktree::{
    AgentWorktreeError, AgentWorktreeErrorCode, AgentWorktreeTaskState, AgentWorktreeTerminalScope,
};
use std::sync::Arc;

pub(super) fn spawn(
    store: Arc<OperationStore>,
    scope: AgentWorktreeTerminalScope,
    request_id: String,
    activity: TerminalActivitySubscription,
) {
    let failure_store = Arc::clone(&store);
    let failure_scope = scope.clone();
    let failure_request_id = request_id.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("codux-agent-worktree-monitor".to_string())
        .spawn(move || monitor(store, scope, request_id, activity))
    {
        let _ = failure_store.update_task(
            &failure_scope,
            &failure_request_id,
            AgentWorktreeTaskState::Failed,
            Some(AgentWorktreeError::new(
                AgentWorktreeErrorCode::TaskInterrupted,
                format!("Failed to monitor the agent task: {error}"),
            )),
            None,
        );
    }
}

fn monitor(
    store: Arc<OperationStore>,
    scope: AgentWorktreeTerminalScope,
    request_id: String,
    activity: TerminalActivitySubscription,
) {
    let mut started = false;
    loop {
        let event = match activity.recv() {
            Ok(event) => event,
            Err(_) => {
                let _ = store.update_task(
                    &scope,
                    &request_id,
                    AgentWorktreeTaskState::Failed,
                    Some(AgentWorktreeError::new(
                        AgentWorktreeErrorCode::TaskInterrupted,
                        "The terminal activity stream ended before task completion.",
                    )),
                    None,
                );
                return;
            }
        };
        let update = match event {
            TerminalActivityEvent::Status(status) => match status.state {
                TerminalStatusState::Working => {
                    started = true;
                    Some((AgentWorktreeTaskState::Running, None, None))
                }
                TerminalStatusState::Waiting if started => {
                    Some((AgentWorktreeTaskState::Waiting, None, None))
                }
                TerminalStatusState::Completed if started => {
                    Some((AgentWorktreeTaskState::Completed, None, None))
                }
                TerminalStatusState::Error => Some((
                    AgentWorktreeTaskState::Failed,
                    Some(AgentWorktreeError::new(
                        AgentWorktreeErrorCode::TaskFailed,
                        "The agent reported a terminal error.",
                    )),
                    None,
                )),
                TerminalStatusState::Idle
                | TerminalStatusState::Waiting
                | TerminalStatusState::Completed
                | TerminalStatusState::Warning => None,
            },
            TerminalActivityEvent::Exit { exit_code, .. } => Some((
                AgentWorktreeTaskState::Failed,
                Some(AgentWorktreeError::new(
                    AgentWorktreeErrorCode::TaskInterrupted,
                    match exit_code {
                        Some(code) => format!("The agent terminal exited with code {code}."),
                        None => "The agent terminal exited before completion.".to_string(),
                    },
                )),
                exit_code,
            )),
            TerminalActivityEvent::Error { message, .. } => Some((
                AgentWorktreeTaskState::Failed,
                Some(AgentWorktreeError::new(
                    AgentWorktreeErrorCode::TaskFailed,
                    message,
                )),
                None,
            )),
        };
        let Some((task_state, task_error, terminal_exit_code)) = update else {
            continue;
        };
        let terminal = matches!(
            task_state,
            AgentWorktreeTaskState::Completed | AgentWorktreeTaskState::Failed
        );
        if let Err(error) = store.update_task(
            &scope,
            &request_id,
            task_state,
            task_error,
            terminal_exit_code,
        ) {
            crate::ai_runtime::runtime_log_line(
                "agent-worktree",
                &format!("failed to persist task state request={request_id} error={error}"),
            );
        }
        if terminal {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::terminal_activity::TerminalActivityHub;
    use codux_runtime_core::{
        agent_worktree::{
            AgentWorktreeCreateRequest, AgentWorktreeOperationState, AgentWorktreeTaskState,
        },
        runtime_target::RuntimeTarget,
    };

    #[test]
    fn closed_activity_stream_fails_the_waiting_task() {
        let root = std::env::temp_dir().join(format!(
            "codux-agent-monitor-closed-{}",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(OperationStore::open(&root).unwrap());
        let scope = AgentWorktreeTerminalScope {
            root_project_id: "project-1".to_string(),
            root_project_path: "/repo".to_string(),
            project_name: "Repo".to_string(),
            source_worktree_id: "project-1".to_string(),
            source_worktree_path: "/repo".to_string(),
            runtime_target: RuntimeTarget::Local,
        };
        let request = AgentWorktreeCreateRequest {
            request_id: "request-1".to_string(),
            name: "fix-login".to_string(),
            agent: "codex".to_string(),
            prompt: "Fix login".to_string(),
            base_branch: None,
        };
        store.claim(&scope, &request).unwrap();
        store
            .mark_starting(
                &scope,
                &request.request_id,
                "worktree-1".to_string(),
                "/repo/worktree-1".to_string(),
                Some("main".to_string()),
                "main".to_string(),
                "terminal-1".to_string(),
            )
            .unwrap();
        store.mark_ready(&scope, &request.request_id).unwrap();
        let activity = {
            let hub = TerminalActivityHub::default();
            hub.subscribe("terminal-1")
        };

        monitor(
            Arc::clone(&store),
            scope.clone(),
            request.request_id.clone(),
            activity,
        );

        let result = store
            .wait_for_completion(&scope, &request.request_id)
            .unwrap();
        assert_eq!(result.state, AgentWorktreeOperationState::Ready);
        assert_eq!(result.task_state, Some(AgentWorktreeTaskState::Failed));
        assert_eq!(
            result.task_error.as_ref().map(|error| error.code),
            Some(AgentWorktreeErrorCode::TaskInterrupted)
        );
        std::fs::remove_dir_all(root).ok();
    }
}
