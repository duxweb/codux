use super::{
    AgentWorktreeCreatedWorktree, AgentWorktreeHost, AgentWorktreeTerminalPlan,
    store::{ClaimResult, OperationStore},
    task_monitor,
};
use crate::ai_runtime::{
    AIRuntimeBridge,
    tool_driver::{canonical_tool_name, initial_prompt_launch},
};
use codux_runtime_core::agent_worktree::{
    AgentWorktreeCreateRequest, AgentWorktreeCreateResult, AgentWorktreeDeliveryResult,
    AgentWorktreeDeliveryState, AgentWorktreeError, AgentWorktreeErrorCode,
    AgentWorktreeOperationState, AgentWorktreeTaskState, AgentWorktreeTerminalScope,
};
use std::{
    fs,
    io::Write,
    sync::{Arc, Mutex},
};

pub(super) struct AgentWorktreeService {
    store: Arc<OperationStore>,
    ai_runtime: Arc<AIRuntimeBridge>,
    delivery_lock: Mutex<()>,
}

impl AgentWorktreeService {
    pub fn open(root: &std::path::Path, ai_runtime: Arc<AIRuntimeBridge>) -> Result<Self, String> {
        Ok(Self {
            store: Arc::new(OperationStore::open(root)?),
            ai_runtime,
            delivery_lock: Mutex::new(()),
        })
    }

    pub fn create(
        &self,
        host: Arc<dyn AgentWorktreeHost>,
        scope: AgentWorktreeTerminalScope,
        request: AgentWorktreeCreateRequest,
        wait_for_completion: bool,
    ) -> AgentWorktreeCreateResult {
        if let Err(error) = request.validate() {
            return failed_result(&request, String::new(), error);
        }
        let Some(tool) = canonical_tool_name(&request.agent) else {
            return failed_result(
                &request,
                String::new(),
                AgentWorktreeError::new(
                    AgentWorktreeErrorCode::UnsupportedAgent,
                    format!("Unsupported AI agent: {}", request.agent.trim()),
                ),
            );
        };
        let claimed = match self.store.claim(&scope, &request) {
            Ok(ClaimResult::Existing(result)) => {
                let request_id = result.request_id.clone();
                return if wait_for_completion {
                    self.wait_for_completion(&scope, &request_id, result)
                } else {
                    self.wait_for_ready(&scope, &request_id, result)
                };
            }
            Ok(ClaimResult::Claimed(result)) => result,
            Err(message) => {
                return failed_result(
                    &request,
                    String::new(),
                    AgentWorktreeError::new(AgentWorktreeErrorCode::InvalidRequest, message),
                );
            }
        };
        let operation_request_id = claimed.request_id.clone();

        let worktree = match host.create_worktree(&scope, &request) {
            Ok(worktree) => worktree,
            Err(message) => {
                return self.finish_failed(
                    &scope,
                    claimed,
                    AgentWorktreeErrorCode::WorktreeCreateFailed,
                    message,
                );
            }
        };
        let terminal_id = format!("gpui-term-{}-{}", worktree.id, uuid::Uuid::new_v4());
        let mut created = claimed.clone();
        created.worktree_id = Some(worktree.id.clone());
        created.worktree_path = Some(worktree.path.clone());
        created.base_branch = worktree.base_branch.clone();
        created.source_branch = Some(worktree.source_branch.clone());
        created.terminal_id = Some(terminal_id.clone());
        created.task_state = Some(AgentWorktreeTaskState::Pending);
        let starting = match self.store.mark_starting(
            &scope,
            &operation_request_id,
            worktree.id.clone(),
            worktree.path.clone(),
            worktree.base_branch.clone(),
            worktree.source_branch.clone(),
            terminal_id.clone(),
        ) {
            Ok(result) => result,
            Err(message) => {
                return self.finish_failed(
                    &scope,
                    created,
                    AgentWorktreeErrorCode::Internal,
                    message,
                );
            }
        };
        let prompt_path = self.store.prompt_path(&starting.operation_id);
        if let Err(error) = write_private_prompt(&prompt_path, &request.prompt) {
            return self.finish_failed(&scope, starting, AgentWorktreeErrorCode::Internal, error);
        }
        let Some(launch) = initial_prompt_launch(tool, &prompt_path) else {
            let _ = fs::remove_file(&prompt_path);
            return self.finish_failed(
                &scope,
                starting,
                AgentWorktreeErrorCode::UnsupportedAgent,
                format!("Unsupported AI agent: {tool}"),
            );
        };
        let plan = AgentWorktreeTerminalPlan {
            terminal_id: terminal_id.clone(),
            operation_id: starting.operation_id.clone(),
            tool: tool.to_string(),
            title: format!("{} · {}", worktree.name, tool),
            command: launch.command,
            env: launch.env,
            prompt_path: prompt_path.clone(),
        };
        let activity = self.ai_runtime.subscribe_terminal_activity(&terminal_id);
        if let Err(message) = host.create_terminal(&scope, &worktree, &plan) {
            let _ = fs::remove_file(prompt_path);
            return self.finish_failed(
                &scope,
                starting,
                AgentWorktreeErrorCode::TerminalCreateFailed,
                message,
            );
        }
        let ready = match self.store.mark_ready(&scope, &operation_request_id) {
            Ok(result) => result,
            Err(message) => {
                crate::ai_runtime::runtime_log_line(
                    "agent-worktree",
                    &format!(
                        "failed to persist ready state request={operation_request_id} error={message}"
                    ),
                );
                let mut ready = starting;
                ready.state = AgentWorktreeOperationState::Ready;
                ready.error = None;
                ready
            }
        };
        task_monitor::spawn(
            Arc::clone(&self.store),
            scope.clone(),
            operation_request_id.clone(),
            activity,
        );
        if wait_for_completion {
            self.wait_for_completion(&scope, &operation_request_id, ready)
        } else {
            ready
        }
    }

    pub fn merge(
        &self,
        host: Arc<dyn AgentWorktreeHost>,
        scope: AgentWorktreeTerminalScope,
        operation_id: &str,
    ) -> Result<AgentWorktreeDeliveryResult, AgentWorktreeError> {
        let _delivery = self
            .delivery_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (result, branch) = self.delivery_operation(&scope, operation_id)?;
        match result.delivery_state {
            Some(AgentWorktreeDeliveryState::Merged) => return delivery_result(&result),
            Some(AgentWorktreeDeliveryState::Removed) => {
                return Err(AgentWorktreeError::new(
                    AgentWorktreeErrorCode::InvalidRequest,
                    "The agent worktree has already been removed.",
                ));
            }
            None => {}
        }
        if result.state != AgentWorktreeOperationState::Ready
            || result.task_state != Some(AgentWorktreeTaskState::Completed)
        {
            return Err(AgentWorktreeError::new(
                AgentWorktreeErrorCode::InvalidRequest,
                "The agent task must complete before its worktree can be merged.",
            ));
        }
        let worktree = delivery_worktree(&result, &branch)?;
        host.merge_worktree(&scope, &worktree).map_err(|message| {
            AgentWorktreeError::new(AgentWorktreeErrorCode::WorktreeMergeFailed, message)
        })?;
        let result = self
            .store
            .mark_delivery(&scope, operation_id, AgentWorktreeDeliveryState::Merged)
            .map_err(internal_error)?;
        delivery_result(&result)
    }

    pub fn remove(
        &self,
        host: Arc<dyn AgentWorktreeHost>,
        scope: AgentWorktreeTerminalScope,
        operation_id: &str,
    ) -> Result<AgentWorktreeDeliveryResult, AgentWorktreeError> {
        let _delivery = self
            .delivery_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (result, branch) = self.delivery_operation(&scope, operation_id)?;
        if result.delivery_state == Some(AgentWorktreeDeliveryState::Removed) {
            return delivery_result(&result);
        }
        if result.delivery_state != Some(AgentWorktreeDeliveryState::Merged) {
            return Err(AgentWorktreeError::new(
                AgentWorktreeErrorCode::InvalidRequest,
                "Merge the reviewed agent worktree before removing it.",
            ));
        }
        let worktree = delivery_worktree(&result, &branch)?;
        host.remove_worktree(&scope, &worktree).map_err(|message| {
            AgentWorktreeError::new(AgentWorktreeErrorCode::WorktreeRemoveFailed, message)
        })?;
        let result = self
            .store
            .mark_delivery(&scope, operation_id, AgentWorktreeDeliveryState::Removed)
            .map_err(internal_error)?;
        delivery_result(&result)
    }

    fn delivery_operation(
        &self,
        scope: &AgentWorktreeTerminalScope,
        operation_id: &str,
    ) -> Result<(AgentWorktreeCreateResult, String), AgentWorktreeError> {
        if operation_id.trim().is_empty() {
            return Err(AgentWorktreeError::new(
                AgentWorktreeErrorCode::InvalidRequest,
                "operationId is required",
            ));
        }
        self.store
            .operation_for_delivery(scope, operation_id)
            .map_err(|message| {
                AgentWorktreeError::new(AgentWorktreeErrorCode::InvalidRequest, message)
            })
    }

    fn wait_for_completion(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request_id: &str,
        result: AgentWorktreeCreateResult,
    ) -> AgentWorktreeCreateResult {
        self.store
            .wait_for_completion(scope, request_id)
            .unwrap_or_else(|message| wait_failed_result(result, message))
    }

    fn wait_for_ready(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request_id: &str,
        result: AgentWorktreeCreateResult,
    ) -> AgentWorktreeCreateResult {
        self.store
            .wait_for_ready(scope, request_id)
            .unwrap_or_else(|message| wait_failed_result(result, message))
    }

    fn finish_failed(
        &self,
        scope: &AgentWorktreeTerminalScope,
        mut result: AgentWorktreeCreateResult,
        code: AgentWorktreeErrorCode,
        message: String,
    ) -> AgentWorktreeCreateResult {
        result.state = AgentWorktreeOperationState::Failed;
        result.error = Some(AgentWorktreeError::new(code, message));
        let _ = self.store.finish_operation(scope, result.clone());
        result
    }
}

fn wait_failed_result(
    mut result: AgentWorktreeCreateResult,
    message: String,
) -> AgentWorktreeCreateResult {
    result.state = AgentWorktreeOperationState::Failed;
    result.error = Some(AgentWorktreeError::new(
        AgentWorktreeErrorCode::Internal,
        message,
    ));
    result
}

fn failed_result(
    request: &AgentWorktreeCreateRequest,
    operation_id: String,
    error: AgentWorktreeError,
) -> AgentWorktreeCreateResult {
    AgentWorktreeCreateResult {
        request_id: request.request_id.clone(),
        operation_id,
        state: AgentWorktreeOperationState::Failed,
        worktree_id: None,
        worktree_path: None,
        base_branch: request.base_branch.clone(),
        source_branch: None,
        terminal_id: None,
        task_state: None,
        task_error: None,
        terminal_exit_code: None,
        delivery_state: None,
        error: Some(error),
    }
}

fn delivery_worktree(
    result: &AgentWorktreeCreateResult,
    branch: &str,
) -> Result<AgentWorktreeCreatedWorktree, AgentWorktreeError> {
    Ok(AgentWorktreeCreatedWorktree {
        id: required_delivery_field(result.worktree_id.as_deref(), "worktreeId")?.to_string(),
        name: branch.to_string(),
        branch: branch.to_string(),
        path: required_delivery_field(result.worktree_path.as_deref(), "worktreePath")?.to_string(),
        base_branch: result.base_branch.clone(),
        source_branch: required_delivery_field(result.source_branch.as_deref(), "sourceBranch")?
            .to_string(),
    })
}

fn delivery_result(
    result: &AgentWorktreeCreateResult,
) -> Result<AgentWorktreeDeliveryResult, AgentWorktreeError> {
    Ok(AgentWorktreeDeliveryResult {
        operation_id: result.operation_id.clone(),
        worktree_id: required_delivery_field(result.worktree_id.as_deref(), "worktreeId")?
            .to_string(),
        worktree_path: required_delivery_field(result.worktree_path.as_deref(), "worktreePath")?
            .to_string(),
        terminal_id: required_delivery_field(result.terminal_id.as_deref(), "terminalId")?
            .to_string(),
        state: result.delivery_state.ok_or_else(|| {
            AgentWorktreeError::new(
                AgentWorktreeErrorCode::Internal,
                "The agent worktree delivery state is missing.",
            )
        })?,
    })
}

fn required_delivery_field<'a>(
    value: Option<&'a str>,
    field: &str,
) -> Result<&'a str, AgentWorktreeError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AgentWorktreeError::new(
                AgentWorktreeErrorCode::Internal,
                format!("The agent worktree {field} is missing."),
            )
        })
}

fn internal_error(message: String) -> AgentWorktreeError {
    AgentWorktreeError::new(AgentWorktreeErrorCode::Internal, message)
}

fn write_private_prompt(path: &std::path::Path, prompt: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = options
        .open(path)
        .and_then(|mut file| file.write_all(prompt.as_bytes()));
    if let Err(error) = result {
        let _ = fs::remove_file(path);
        return Err(error.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_worktree::AgentWorktreeCreatedWorktree;
    use crate::ai_runtime::{
        TerminalStatusEvent, TerminalStatusState, terminal_activity::TerminalActivityEvent,
    };
    use codux_runtime_core::agent_worktree::AgentWorktreeTaskState;
    use codux_runtime_core::runtime_target::RuntimeTarget;
    use std::{
        sync::{Barrier, Mutex, mpsc},
        time::Duration,
    };

    #[derive(Default)]
    struct FakeHost {
        worktree_calls: Mutex<usize>,
        terminal_calls: Mutex<usize>,
        merge_calls: Mutex<usize>,
        remove_calls: Mutex<usize>,
        fail_merge: bool,
        fail_remove: bool,
        merge_delay: Option<Duration>,
    }

    impl AgentWorktreeHost for FakeHost {
        fn create_worktree(
            &self,
            _scope: &AgentWorktreeTerminalScope,
            request: &AgentWorktreeCreateRequest,
        ) -> Result<AgentWorktreeCreatedWorktree, String> {
            *self.worktree_calls.lock().unwrap() += 1;
            Ok(AgentWorktreeCreatedWorktree {
                id: "worktree-1".to_string(),
                name: request.name.clone(),
                branch: request.name.clone(),
                path: "/repo/.codux/worktrees/fix-login".to_string(),
                base_branch: Some("main".to_string()),
                source_branch: "main".to_string(),
            })
        }

        fn create_terminal(
            &self,
            _scope: &AgentWorktreeTerminalScope,
            _worktree: &AgentWorktreeCreatedWorktree,
            _plan: &AgentWorktreeTerminalPlan,
        ) -> Result<(), String> {
            *self.terminal_calls.lock().unwrap() += 1;
            Ok(())
        }

        fn merge_worktree(
            &self,
            _scope: &AgentWorktreeTerminalScope,
            _worktree: &AgentWorktreeCreatedWorktree,
        ) -> Result<(), String> {
            *self.merge_calls.lock().unwrap() += 1;
            if let Some(delay) = self.merge_delay {
                std::thread::sleep(delay);
            }
            if self.fail_merge {
                Err("merge failed".to_string())
            } else {
                Ok(())
            }
        }

        fn remove_worktree(
            &self,
            _scope: &AgentWorktreeTerminalScope,
            _worktree: &AgentWorktreeCreatedWorktree,
        ) -> Result<(), String> {
            *self.remove_calls.lock().unwrap() += 1;
            if self.fail_remove {
                Err("remove failed".to_string())
            } else {
                Ok(())
            }
        }
    }

    struct EventHost {
        ai_runtime: Arc<AIRuntimeBridge>,
        terminal_events: Vec<TerminalActivityEvent>,
        terminal_id_sender: Option<mpsc::Sender<String>>,
    }

    impl AgentWorktreeHost for EventHost {
        fn create_worktree(
            &self,
            _scope: &AgentWorktreeTerminalScope,
            request: &AgentWorktreeCreateRequest,
        ) -> Result<AgentWorktreeCreatedWorktree, String> {
            Ok(AgentWorktreeCreatedWorktree {
                id: "worktree-1".to_string(),
                name: request.name.clone(),
                branch: request.name.clone(),
                path: "/repo/.codux/worktrees/fix-login".to_string(),
                base_branch: Some("main".to_string()),
                source_branch: "main".to_string(),
            })
        }

        fn create_terminal(
            &self,
            _scope: &AgentWorktreeTerminalScope,
            _worktree: &AgentWorktreeCreatedWorktree,
            plan: &AgentWorktreeTerminalPlan,
        ) -> Result<(), String> {
            if let Some(sender) = &self.terminal_id_sender {
                sender.send(plan.terminal_id.clone()).unwrap();
            }
            for event in &self.terminal_events {
                match event {
                    TerminalActivityEvent::Status(status) => self
                        .ai_runtime
                        .submit_terminal_status(TerminalStatusEvent {
                            terminal_id: plan.terminal_id.clone(),
                            ..status.clone()
                        })?,
                    TerminalActivityEvent::Exit { exit_code, .. } => self
                        .ai_runtime
                        .submit_terminal_exit(&plan.terminal_id, *exit_code),
                    TerminalActivityEvent::Error { message, .. } => self
                        .ai_runtime
                        .submit_terminal_error(&plan.terminal_id, message),
                }
            }
            Ok(())
        }

        fn merge_worktree(
            &self,
            _scope: &AgentWorktreeTerminalScope,
            _worktree: &AgentWorktreeCreatedWorktree,
        ) -> Result<(), String> {
            Ok(())
        }

        fn remove_worktree(
            &self,
            _scope: &AgentWorktreeTerminalScope,
            _worktree: &AgentWorktreeCreatedWorktree,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    fn scope() -> AgentWorktreeTerminalScope {
        AgentWorktreeTerminalScope {
            root_project_id: "project-1".to_string(),
            root_project_path: "/repo".to_string(),
            project_name: "Repo".to_string(),
            source_worktree_id: "project-1".to_string(),
            source_worktree_path: "/repo".to_string(),
            runtime_target: RuntimeTarget::Local,
        }
    }

    fn request() -> AgentWorktreeCreateRequest {
        AgentWorktreeCreateRequest {
            request_id: "request-1".to_string(),
            name: "fix-login".to_string(),
            agent: "codex".to_string(),
            prompt: "Fix login".to_string(),
            base_branch: None,
        }
    }

    fn test_runtime(root: &std::path::Path) -> Arc<AIRuntimeBridge> {
        Arc::new(AIRuntimeBridge::with_runtime_paths(
            root.join("runtime-root"),
            root.join("runtime-temp"),
            root.join("home"),
        ))
    }

    fn status(state: TerminalStatusState) -> TerminalActivityEvent {
        TerminalActivityEvent::Status(TerminalStatusEvent {
            terminal_id: String::new(),
            terminal_instance_id: None,
            project_id: Some("project-1".to_string()),
            worktree_id: Some("worktree-1".to_string()),
            state,
            updated_at: 1.0,
            source: "terminal-progress-osc".to_string(),
        })
    }

    fn completed_operation(service: &AgentWorktreeService) -> AgentWorktreeCreateResult {
        let request = request();
        let ClaimResult::Claimed(_) = service.store.claim(&scope(), &request).unwrap() else {
            panic!("test operation must be newly claimed");
        };
        service
            .store
            .mark_starting(
                &scope(),
                &request.request_id,
                "worktree-1".to_string(),
                "/repo/.codux/worktrees/fix-login".to_string(),
                Some("main".to_string()),
                "main".to_string(),
                "terminal-1".to_string(),
            )
            .unwrap();
        service
            .store
            .mark_ready(&scope(), &request.request_id)
            .unwrap();
        service
            .store
            .update_task(
                &scope(),
                &request.request_id,
                AgentWorktreeTaskState::Completed,
                None,
                None,
            )
            .unwrap()
    }

    #[test]
    fn wait_failure_never_returns_a_stale_ready_result() {
        let mut result = failed_result(
            &request(),
            "operation-1".to_string(),
            AgentWorktreeError::new(AgentWorktreeErrorCode::Internal, "placeholder"),
        );
        result.state = AgentWorktreeOperationState::Ready;
        result.error = None;

        let result = wait_failed_result(result, "operation is missing".to_string());

        assert_eq!(result.state, AgentWorktreeOperationState::Failed);
        assert_eq!(
            result.error.as_ref().map(|error| error.code),
            Some(AgentWorktreeErrorCode::Internal)
        );
        assert_eq!(
            result.error.as_ref().map(|error| error.message.as_str()),
            Some("operation is missing")
        );
    }

    #[test]
    fn concurrent_duplicate_request_executes_host_once() {
        let root =
            std::env::temp_dir().join(format!("codux-agent-service-{}", uuid::Uuid::new_v4()));
        let service = Arc::new(AgentWorktreeService::open(&root, test_runtime(&root)).unwrap());
        let host = Arc::new(FakeHost::default());
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for _ in 0..2 {
            let service = Arc::clone(&service);
            let host = Arc::clone(&host);
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                service.create(host, scope(), request(), false)
            }));
        }
        barrier.wait();
        let first = threads.remove(0).join().unwrap();
        let second = threads.remove(0).join().unwrap();
        assert_eq!(first, second);
        assert_eq!(*host.worktree_calls.lock().unwrap(), 1);
        assert_eq!(*host.terminal_calls.lock().unwrap(), 1);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn semantic_retry_waits_for_the_original_operation() {
        let root = std::env::temp_dir().join(format!(
            "codux-agent-semantic-retry-{}",
            uuid::Uuid::new_v4()
        ));
        let ai_runtime = test_runtime(&root);
        let service = Arc::new(AgentWorktreeService::open(&root, Arc::clone(&ai_runtime)).unwrap());
        let (terminal_id_sender, terminal_id_receiver) = mpsc::channel();
        let host = Arc::new(EventHost {
            ai_runtime: Arc::clone(&ai_runtime),
            terminal_events: Vec::new(),
            terminal_id_sender: Some(terminal_id_sender),
        });
        let (result_sender, result_receiver) = mpsc::channel();
        let first_service = Arc::clone(&service);
        let first_host = Arc::clone(&host);
        let first_sender = result_sender.clone();
        let first = std::thread::spawn(move || {
            first_sender
                .send(first_service.create(first_host, scope(), request(), true))
                .unwrap();
        });
        let terminal_id = terminal_id_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        let second_service = Arc::clone(&service);
        let second_host = Arc::clone(&host);
        let second = std::thread::spawn(move || {
            let mut retry = request();
            retry.request_id = "request-2".to_string();
            result_sender
                .send(second_service.create(second_host, scope(), retry, true))
                .unwrap();
        });

        assert!(
            result_receiver
                .recv_timeout(Duration::from_millis(100))
                .is_err()
        );
        ai_runtime
            .submit_terminal_status(TerminalStatusEvent {
                terminal_id: terminal_id.clone(),
                terminal_instance_id: None,
                project_id: Some("project-1".to_string()),
                worktree_id: Some("worktree-1".to_string()),
                state: TerminalStatusState::Working,
                updated_at: 2.0,
                source: "terminal-progress-osc".to_string(),
            })
            .unwrap();
        ai_runtime
            .submit_terminal_status(TerminalStatusEvent {
                terminal_id,
                terminal_instance_id: None,
                project_id: Some("project-1".to_string()),
                worktree_id: Some("worktree-1".to_string()),
                state: TerminalStatusState::Completed,
                updated_at: 3.0,
                source: "terminal-progress-osc".to_string(),
            })
            .unwrap();

        let first_result = result_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        let second_result = result_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        first.join().unwrap();
        second.join().unwrap();
        assert_eq!(first_result, second_result);
        assert_eq!(first_result.request_id, "request-1");
        assert_eq!(
            first_result.task_state,
            Some(AgentWorktreeTaskState::Completed)
        );
        assert!(
            terminal_id_receiver
                .recv_timeout(Duration::from_millis(100))
                .is_err()
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn default_request_waits_for_fast_working_then_completed_events() {
        let root =
            std::env::temp_dir().join(format!("codux-agent-complete-{}", uuid::Uuid::new_v4()));
        let ai_runtime = test_runtime(&root);
        let service = AgentWorktreeService::open(&root, Arc::clone(&ai_runtime)).unwrap();
        let host = Arc::new(EventHost {
            ai_runtime,
            terminal_events: vec![
                status(TerminalStatusState::Working),
                status(TerminalStatusState::Waiting),
                status(TerminalStatusState::Completed),
            ],
            terminal_id_sender: None,
        });

        let result = service.create(host, scope(), request(), true);

        assert_eq!(result.state, AgentWorktreeOperationState::Ready);
        assert_eq!(result.task_state, Some(AgentWorktreeTaskState::Completed));
        assert_eq!(
            result.worktree_path.as_deref(),
            Some("/repo/.codux/worktrees/fix-login")
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn completed_without_working_does_not_finish_waiter() {
        let root = std::env::temp_dir().join(format!(
            "codux-agent-requires-working-{}",
            uuid::Uuid::new_v4()
        ));
        let ai_runtime = test_runtime(&root);
        let service = AgentWorktreeService::open(&root, Arc::clone(&ai_runtime)).unwrap();
        let (terminal_id_sender, terminal_id_receiver) = mpsc::channel();
        let host = Arc::new(EventHost {
            ai_runtime: Arc::clone(&ai_runtime),
            terminal_events: vec![status(TerminalStatusState::Completed)],
            terminal_id_sender: Some(terminal_id_sender),
        });
        let (sender, receiver) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            sender
                .send(service.create(host, scope(), request(), true))
                .unwrap();
        });

        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
        let terminal_id = terminal_id_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        ai_runtime
            .submit_terminal_status(TerminalStatusEvent {
                terminal_id: terminal_id.clone(),
                terminal_instance_id: None,
                project_id: Some("project-1".to_string()),
                worktree_id: Some("worktree-1".to_string()),
                state: TerminalStatusState::Working,
                updated_at: 2.0,
                source: "terminal-progress-osc".to_string(),
            })
            .unwrap();
        ai_runtime
            .submit_terminal_status(TerminalStatusEvent {
                terminal_id,
                terminal_instance_id: None,
                project_id: Some("project-1".to_string()),
                worktree_id: Some("worktree-1".to_string()),
                state: TerminalStatusState::Completed,
                updated_at: 3.0,
                source: "terminal-progress-osc".to_string(),
            })
            .unwrap();

        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .task_state,
            Some(AgentWorktreeTaskState::Completed)
        );
        waiter.join().unwrap();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn terminal_exit_fails_waiting_task() {
        let root = std::env::temp_dir().join(format!("codux-agent-exit-{}", uuid::Uuid::new_v4()));
        let ai_runtime = test_runtime(&root);
        let service = AgentWorktreeService::open(&root, Arc::clone(&ai_runtime)).unwrap();
        let host = Arc::new(EventHost {
            ai_runtime,
            terminal_events: vec![
                status(TerminalStatusState::Working),
                TerminalActivityEvent::Exit {
                    terminal_id: String::new(),
                    exit_code: Some(9),
                },
            ],
            terminal_id_sender: None,
        });

        let result = service.create(host, scope(), request(), true);

        assert_eq!(result.task_state, Some(AgentWorktreeTaskState::Failed));
        assert_eq!(result.terminal_exit_code, Some(9));
        assert_eq!(
            result.task_error.unwrap().code,
            AgentWorktreeErrorCode::TaskInterrupted
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn delivery_requires_completed_then_merged_state() {
        let root = std::env::temp_dir().join(format!(
            "codux-agent-delivery-order-{}",
            uuid::Uuid::new_v4()
        ));
        let service = AgentWorktreeService::open(&root, test_runtime(&root)).unwrap();
        let host = Arc::new(FakeHost::default());
        let result = service.create(host.clone(), scope(), request(), false);

        let merge_error = service
            .merge(host.clone(), scope(), &result.operation_id)
            .unwrap_err();
        let remove_error = service
            .remove(host.clone(), scope(), &result.operation_id)
            .unwrap_err();

        assert_eq!(merge_error.code, AgentWorktreeErrorCode::InvalidRequest);
        assert_eq!(remove_error.code, AgentWorktreeErrorCode::InvalidRequest);
        assert_eq!(*host.merge_calls.lock().unwrap(), 0);
        assert_eq!(*host.remove_calls.lock().unwrap(), 0);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn merge_and_remove_are_ordered_and_idempotent() {
        let root =
            std::env::temp_dir().join(format!("codux-agent-delivery-{}", uuid::Uuid::new_v4()));
        let service = AgentWorktreeService::open(&root, test_runtime(&root)).unwrap();
        let host = Arc::new(FakeHost::default());
        let result = completed_operation(&service);

        let merged = service
            .merge(host.clone(), scope(), &result.operation_id)
            .unwrap();
        let merged_again = service
            .merge(host.clone(), scope(), &result.operation_id)
            .unwrap();
        let removed = service
            .remove(host.clone(), scope(), &result.operation_id)
            .unwrap();
        let removed_again = service
            .remove(host.clone(), scope(), &result.operation_id)
            .unwrap();

        assert_eq!(merged.state, AgentWorktreeDeliveryState::Merged);
        assert_eq!(merged_again, merged);
        assert_eq!(removed.state, AgentWorktreeDeliveryState::Removed);
        assert_eq!(removed_again, removed);
        assert_eq!(*host.merge_calls.lock().unwrap(), 1);
        assert_eq!(*host.remove_calls.lock().unwrap(), 1);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn failed_delivery_preserves_the_last_successful_state() {
        let root = std::env::temp_dir().join(format!(
            "codux-agent-delivery-failure-{}",
            uuid::Uuid::new_v4()
        ));
        let service = AgentWorktreeService::open(&root, test_runtime(&root)).unwrap();
        let host = Arc::new(FakeHost {
            fail_merge: true,
            ..Default::default()
        });
        let result = completed_operation(&service);

        let error = service
            .merge(host.clone(), scope(), &result.operation_id)
            .unwrap_err();
        let stored = service
            .store
            .operation_for_delivery(&scope(), &result.operation_id)
            .unwrap()
            .0;

        assert_eq!(error.code, AgentWorktreeErrorCode::WorktreeMergeFailed);
        assert_eq!(stored.delivery_state, None);
        assert_eq!(stored.task_state, Some(AgentWorktreeTaskState::Completed));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn concurrent_merge_executes_host_once() {
        let root = std::env::temp_dir().join(format!(
            "codux-agent-concurrent-merge-{}",
            uuid::Uuid::new_v4()
        ));
        let service = Arc::new(AgentWorktreeService::open(&root, test_runtime(&root)).unwrap());
        let host = Arc::new(FakeHost {
            merge_delay: Some(Duration::from_millis(50)),
            ..Default::default()
        });
        let result = completed_operation(&service);
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for _ in 0..2 {
            let service = Arc::clone(&service);
            let host = Arc::clone(&host);
            let barrier = Arc::clone(&barrier);
            let operation_id = result.operation_id.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                service.merge(host, scope(), &operation_id).unwrap()
            }));
        }
        barrier.wait();
        let first = threads.remove(0).join().unwrap();
        let second = threads.remove(0).join().unwrap();

        assert_eq!(first, second);
        assert_eq!(*host.merge_calls.lock().unwrap(), 1);
        fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn prompt_file_is_private_from_creation() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "codux-agent-prompt-permissions-{}",
            uuid::Uuid::new_v4()
        ));
        let path = root.join("prompt.txt");
        write_private_prompt(&path, "Fix login").unwrap();

        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(root).ok();
    }
}
