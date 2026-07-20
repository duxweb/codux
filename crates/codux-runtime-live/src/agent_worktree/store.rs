use codux_runtime_core::agent_worktree::{
    AgentWorktreeCreateRequest, AgentWorktreeCreateResult, AgentWorktreeDeliveryState,
    AgentWorktreeError, AgentWorktreeErrorCode, AgentWorktreeOperationState,
    AgentWorktreeTaskState, AgentWorktreeTerminalScope,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Condvar, Mutex},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredOperation {
    scope: AgentWorktreeTerminalScope,
    request: AgentWorktreeCreateRequest,
    result: AgentWorktreeCreateResult,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct OperationKey {
    runtime_target: codux_runtime_core::runtime_target::RuntimeTarget,
    root_project_id: String,
    source_worktree_id: String,
    request_id: String,
}

impl OperationKey {
    fn new(scope: &AgentWorktreeTerminalScope, request_id: &str) -> Self {
        Self {
            runtime_target: scope.runtime_target.clone(),
            root_project_id: scope.root_project_id.clone(),
            source_worktree_id: scope.source_worktree_id.clone(),
            request_id: request_id.to_string(),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredOperationsFile {
    operations: Vec<StoredOperation>,
}

#[derive(Default)]
struct OperationState {
    operations: HashMap<OperationKey, StoredOperation>,
}

pub(super) enum ClaimResult {
    Existing(AgentWorktreeCreateResult),
    Claimed(AgentWorktreeCreateResult),
}

pub(super) struct OperationStore {
    path: PathBuf,
    prompt_dir: PathBuf,
    state: Mutex<OperationState>,
    changed: Condvar,
}

impl OperationStore {
    pub fn open(root: &Path) -> Result<Self, String> {
        let operation_dir = root.join("agent-worktree");
        let path = operation_dir.join("operations.json");
        let prompt_dir = operation_dir.join("prompts");
        fs::create_dir_all(&prompt_dir).map_err(|error| error.to_string())?;
        let (mut state, recovered_from_backup) = load_state(&path)?;
        let mut recovered = recovered_from_backup;
        for operation in state.operations.values_mut() {
            if matches!(
                operation.result.state,
                AgentWorktreeOperationState::Creating | AgentWorktreeOperationState::Starting
            ) {
                operation.result.state = AgentWorktreeOperationState::Failed;
                operation.result.error = Some(AgentWorktreeError::new(
                    AgentWorktreeErrorCode::Interrupted,
                    "The previous runtime stopped before this operation completed.",
                ));
                recovered = true;
            } else if operation.result.state == AgentWorktreeOperationState::Ready
                && matches!(
                    operation.result.task_state,
                    None | Some(
                        AgentWorktreeTaskState::Pending
                            | AgentWorktreeTaskState::Running
                            | AgentWorktreeTaskState::Waiting
                    )
                )
            {
                operation.result.task_state = Some(AgentWorktreeTaskState::Failed);
                operation.result.task_error = Some(AgentWorktreeError::new(
                    AgentWorktreeErrorCode::TaskInterrupted,
                    "The previous runtime stopped before the agent task completed.",
                ));
                recovered = true;
            }
        }
        let store = Self {
            path,
            prompt_dir,
            state: Mutex::new(state),
            changed: Condvar::new(),
        };
        if recovered {
            let state = store
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            store.persist(&state)?;
        }
        Ok(store)
    }

    pub fn claim(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request: &AgentWorktreeCreateRequest,
    ) -> Result<ClaimResult, String> {
        let key = OperationKey::new(scope, &request.request_id);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(operation) = state.operations.get(&key) {
            if operation.request != *request {
                return Err("requestId is already bound to a different request".to_string());
            }
            if operation_is_reusable(operation) {
                return Ok(ClaimResult::Existing(operation.result.clone()));
            }
        }
        if let Some(operation) = state.operations.values().find(|operation| {
            operation_scope_matches(&operation.scope, scope)
                && requests_match_semantically(&operation.request, request)
                && operation_is_reusable(operation)
        }) {
            return Ok(ClaimResult::Existing(operation.result.clone()));
        }

        let result = AgentWorktreeCreateResult {
            request_id: request.request_id.clone(),
            operation_id: uuid::Uuid::new_v4().to_string(),
            state: AgentWorktreeOperationState::Creating,
            worktree_id: None,
            worktree_path: None,
            base_branch: request.base_branch.clone(),
            source_branch: None,
            terminal_id: None,
            task_state: None,
            task_error: None,
            terminal_exit_code: None,
            delivery_state: None,
            error: None,
        };
        let replaced = state.operations.insert(
            key.clone(),
            StoredOperation {
                scope: scope.clone(),
                request: request.clone(),
                result: result.clone(),
            },
        );
        if let Err(error) = self.persist(&state) {
            if let Some(operation) = replaced {
                state.operations.insert(key, operation);
            } else {
                state.operations.remove(&key);
            }
            return Err(error);
        }
        self.changed.notify_all();
        Ok(ClaimResult::Claimed(result))
    }

    pub fn mark_starting(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request_id: &str,
        worktree_id: String,
        worktree_path: String,
        base_branch: Option<String>,
        source_branch: String,
        terminal_id: String,
    ) -> Result<AgentWorktreeCreateResult, String> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let key = OperationKey::new(scope, request_id);
        let result = {
            let operation = state
                .operations
                .get_mut(&key)
                .ok_or_else(|| "agent worktree operation is missing".to_string())?;
            operation.result.state = AgentWorktreeOperationState::Starting;
            operation.result.worktree_id = Some(worktree_id);
            operation.result.worktree_path = Some(worktree_path);
            operation.result.base_branch = base_branch;
            operation.result.source_branch = Some(source_branch);
            operation.result.terminal_id = Some(terminal_id);
            operation.result.task_state = Some(AgentWorktreeTaskState::Pending);
            operation.result.clone()
        };
        let persist_result = self.persist(&state);
        self.changed.notify_all();
        persist_result.map(|()| result)
    }

    pub fn mark_ready(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request_id: &str,
    ) -> Result<AgentWorktreeCreateResult, String> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let key = OperationKey::new(scope, request_id);
        let result = {
            let operation = state
                .operations
                .get_mut(&key)
                .ok_or_else(|| "agent worktree operation is missing".to_string())?;
            operation.result.state = AgentWorktreeOperationState::Ready;
            operation.result.error = None;
            operation.result.clone()
        };
        let persist_result = self.persist(&state);
        self.changed.notify_all();
        persist_result.map(|()| result)
    }

    pub fn update_task(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request_id: &str,
        task_state: AgentWorktreeTaskState,
        task_error: Option<AgentWorktreeError>,
        terminal_exit_code: Option<i32>,
    ) -> Result<AgentWorktreeCreateResult, String> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let key = OperationKey::new(scope, request_id);
        let result = {
            let operation = state
                .operations
                .get_mut(&key)
                .ok_or_else(|| "agent worktree operation is missing".to_string())?;
            operation.result.task_state = Some(task_state);
            operation.result.task_error = task_error;
            operation.result.terminal_exit_code = terminal_exit_code;
            operation.result.clone()
        };
        let persist_result = self.persist(&state);
        self.changed.notify_all();
        persist_result.map(|()| result)
    }

    pub fn finish_operation(
        &self,
        scope: &AgentWorktreeTerminalScope,
        result: AgentWorktreeCreateResult,
    ) -> Result<(), String> {
        let key = OperationKey::new(scope, &result.request_id);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let operation = state
            .operations
            .get_mut(&key)
            .ok_or_else(|| "agent worktree operation is missing".to_string())?;
        operation.result = result;
        let persist_result = self.persist(&state);
        self.changed.notify_all();
        persist_result
    }

    pub fn wait_for_completion(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request_id: &str,
    ) -> Result<AgentWorktreeCreateResult, String> {
        let key = OperationKey::new(scope, request_id);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            let result = state
                .operations
                .get(&key)
                .map(|operation| operation.result.clone())
                .ok_or_else(|| "agent worktree operation is missing".to_string())?;
            if result.state == AgentWorktreeOperationState::Failed
                || matches!(
                    result.task_state,
                    Some(AgentWorktreeTaskState::Completed | AgentWorktreeTaskState::Failed)
                )
            {
                return Ok(result);
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    pub fn wait_for_ready(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request_id: &str,
    ) -> Result<AgentWorktreeCreateResult, String> {
        let key = OperationKey::new(scope, request_id);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            let result = state
                .operations
                .get(&key)
                .map(|operation| operation.result.clone())
                .ok_or_else(|| "agent worktree operation is missing".to_string())?;
            if matches!(
                result.state,
                AgentWorktreeOperationState::Ready | AgentWorktreeOperationState::Failed
            ) {
                return Ok(result);
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    pub fn prompt_path(&self, operation_id: &str) -> PathBuf {
        self.prompt_dir.join(format!("{operation_id}.txt"))
    }

    pub fn operation_for_delivery(
        &self,
        scope: &AgentWorktreeTerminalScope,
        operation_id: &str,
    ) -> Result<(AgentWorktreeCreateResult, String), String> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state
            .operations
            .values()
            .find(|operation| {
                delivery_scope_matches(&operation.scope, scope)
                    && operation.result.operation_id == operation_id
            })
            .map(|operation| (operation.result.clone(), operation.request.name.clone()))
            .ok_or_else(|| "agent worktree operation is not available in this project".to_string())
    }

    pub fn mark_delivery(
        &self,
        scope: &AgentWorktreeTerminalScope,
        operation_id: &str,
        delivery_state: codux_runtime_core::agent_worktree::AgentWorktreeDeliveryState,
    ) -> Result<AgentWorktreeCreateResult, String> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let operation = state
            .operations
            .values_mut()
            .find(|operation| {
                delivery_scope_matches(&operation.scope, scope)
                    && operation.result.operation_id == operation_id
            })
            .ok_or_else(|| {
                "agent worktree operation is not available in this project".to_string()
            })?;
        operation.result.delivery_state = Some(delivery_state);
        let result = operation.result.clone();
        self.persist(&state)?;
        self.changed.notify_all();
        Ok(result)
    }

    fn persist(&self, state: &OperationState) -> Result<(), String> {
        let mut operations = state.operations.values().cloned().collect::<Vec<_>>();
        operations.sort_by(|left, right| {
            serde_json::to_string(&left.scope)
                .unwrap_or_default()
                .cmp(&serde_json::to_string(&right.scope).unwrap_or_default())
                .then_with(|| left.request.request_id.cmp(&right.request.request_id))
        });
        let bytes = serde_json::to_vec_pretty(&StoredOperationsFile { operations })
            .map_err(|error| error.to_string())?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let temp = self.path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|error| error.to_string())?;
        #[cfg(windows)]
        {
            let backup = backup_path(&self.path);
            if self.path.exists() {
                let backup_temp = self.path.with_extension("json.bak.tmp");
                let _ = fs::remove_file(&backup_temp);
                fs::copy(&self.path, &backup_temp).map_err(|error| error.to_string())?;
                if backup.exists() {
                    fs::remove_file(&backup).map_err(|error| error.to_string())?;
                }
                fs::rename(&backup_temp, &backup).map_err(|error| error.to_string())?;
                fs::remove_file(&self.path).map_err(|error| error.to_string())?;
                if let Err(error) = fs::rename(&temp, &self.path) {
                    let _ = fs::copy(&backup, &self.path);
                    return Err(error.to_string());
                }
                return Ok(());
            }
        }
        fs::rename(temp, &self.path).map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn operation_scope_matches(
    stored: &AgentWorktreeTerminalScope,
    caller: &AgentWorktreeTerminalScope,
) -> bool {
    stored == caller
}

fn requests_match_semantically(
    stored: &AgentWorktreeCreateRequest,
    caller: &AgentWorktreeCreateRequest,
) -> bool {
    stored.name == caller.name
        && stored.agent == caller.agent
        && stored.prompt == caller.prompt
        && stored.base_branch == caller.base_branch
}

fn operation_is_reusable(operation: &StoredOperation) -> bool {
    operation.result.state != AgentWorktreeOperationState::Failed
        && operation.result.task_state != Some(AgentWorktreeTaskState::Failed)
        && operation.result.delivery_state != Some(AgentWorktreeDeliveryState::Removed)
}

fn delivery_scope_matches(
    stored: &AgentWorktreeTerminalScope,
    caller: &AgentWorktreeTerminalScope,
) -> bool {
    stored.runtime_target == caller.runtime_target
        && stored.root_project_id == caller.root_project_id
        && stored.root_project_path == caller.root_project_path
        && stored.source_worktree_id == caller.source_worktree_id
        && stored.source_worktree_path == caller.source_worktree_path
}

fn load_state(path: &Path) -> Result<(OperationState, bool), String> {
    match read_operations_file(path) {
        Ok(Some(file)) => Ok((operation_state(file), false)),
        Ok(None) => match read_operations_file(&backup_path(path))? {
            Some(file) => Ok((operation_state(file), true)),
            None => Ok((OperationState::default(), false)),
        },
        Err(primary_error) => match read_operations_file(&backup_path(path)) {
            Ok(Some(file)) => {
                fs::remove_file(path)
                    .map_err(|error| format!("failed to discard {}: {error}", path.display()))?;
                Ok((operation_state(file), true))
            }
            _ => Err(primary_error),
        },
    }
}

fn read_operations_file(path: &Path) -> Result<Option<StoredOperationsFile>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn operation_state(file: StoredOperationsFile) -> OperationState {
    OperationState {
        operations: file
            .operations
            .into_iter()
            .map(|operation| {
                (
                    OperationKey::new(&operation.scope, &operation.request.request_id),
                    operation,
                )
            })
            .collect(),
    }
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

#[cfg(test)]
mod tests {
    use super::*;
    use codux_runtime_core::runtime_target::RuntimeTarget;

    fn scope(project_id: &str) -> AgentWorktreeTerminalScope {
        AgentWorktreeTerminalScope {
            root_project_id: project_id.to_string(),
            root_project_path: format!("/{project_id}"),
            project_name: project_id.to_string(),
            source_worktree_id: project_id.to_string(),
            source_worktree_path: format!("/{project_id}"),
            runtime_target: RuntimeTarget::Local,
        }
    }

    fn request(id: &str) -> AgentWorktreeCreateRequest {
        AgentWorktreeCreateRequest {
            request_id: id.to_string(),
            name: "fix-login".to_string(),
            agent: "codex".to_string(),
            prompt: "Fix login".to_string(),
            base_branch: None,
        }
    }

    #[test]
    fn completed_result_survives_reopen() {
        let root =
            std::env::temp_dir().join(format!("codux-operation-store-{}", uuid::Uuid::new_v4()));
        let store = OperationStore::open(&root).unwrap();
        let scope = scope("project-1");
        let ClaimResult::Claimed(mut result) = store.claim(&scope, &request("request-1")).unwrap()
        else {
            panic!("first claim must execute");
        };
        result.state = AgentWorktreeOperationState::Ready;
        result.worktree_id = Some("worktree-1".to_string());
        result.terminal_id = Some("terminal-1".to_string());
        result.task_state = Some(AgentWorktreeTaskState::Completed);
        store.finish_operation(&scope, result.clone()).unwrap();
        drop(store);

        let reopened = OperationStore::open(&root).unwrap();
        let ClaimResult::Existing(existing) =
            reopened.claim(&scope, &request("request-1")).unwrap()
        else {
            panic!("completed operation must be reused");
        };
        assert_eq!(existing, result);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn semantic_retry_reuses_the_existing_operation() {
        let root = std::env::temp_dir().join(format!(
            "codux-operation-semantic-retry-{}",
            uuid::Uuid::new_v4()
        ));
        let store = OperationStore::open(&root).unwrap();
        let scope = scope("project-1");
        let ClaimResult::Claimed(first) = store.claim(&scope, &request("request-1")).unwrap()
        else {
            panic!("first request must claim the operation");
        };

        let ClaimResult::Existing(retried) = store.claim(&scope, &request("request-2")).unwrap()
        else {
            panic!("semantic retry must reuse the operation");
        };

        assert_eq!(retried.operation_id, first.operation_id);
        assert_eq!(retried.request_id, "request-1");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn failed_and_removed_operations_can_be_recreated() {
        let root = std::env::temp_dir().join(format!(
            "codux-operation-semantic-recreate-{}",
            uuid::Uuid::new_v4()
        ));
        let store = OperationStore::open(&root).unwrap();
        let scope = scope("project-1");
        let ClaimResult::Claimed(mut failed) =
            store.claim(&scope, &request("request-failed")).unwrap()
        else {
            panic!("first request must claim the operation");
        };
        failed.state = AgentWorktreeOperationState::Failed;
        failed.error = Some(AgentWorktreeError::new(
            AgentWorktreeErrorCode::WorktreeCreateFailed,
            "failed",
        ));
        store.finish_operation(&scope, failed.clone()).unwrap();

        let ClaimResult::Claimed(after_failure) =
            store.claim(&scope, &request("request-retry")).unwrap()
        else {
            panic!("failed operation must not block a retry");
        };
        assert_ne!(after_failure.operation_id, failed.operation_id);

        let mut removed = after_failure.clone();
        removed.state = AgentWorktreeOperationState::Ready;
        removed.task_state = Some(AgentWorktreeTaskState::Completed);
        removed.delivery_state = Some(AgentWorktreeDeliveryState::Removed);
        store.finish_operation(&scope, removed.clone()).unwrap();

        let ClaimResult::Claimed(after_removal) = store
            .claim(&scope, &request("request-after-removal"))
            .unwrap()
        else {
            panic!("removed operation must not block a new task");
        };
        assert_ne!(after_removal.operation_id, removed.operation_id);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn interrupted_operation_can_be_retried() {
        let root =
            std::env::temp_dir().join(format!("codux-operation-recovery-{}", uuid::Uuid::new_v4()));
        let store = OperationStore::open(&root).unwrap();
        let scope = scope("project-1");
        let ClaimResult::Claimed(result) = store.claim(&scope, &request("request-1")).unwrap()
        else {
            panic!("first request must claim the operation");
        };
        drop(store);

        let reopened = OperationStore::open(&root).unwrap();
        let interrupted = reopened
            .operation_for_delivery(&scope, &result.operation_id)
            .unwrap()
            .0;
        assert_eq!(interrupted.state, AgentWorktreeOperationState::Failed);
        assert_eq!(
            interrupted.error.unwrap().code,
            AgentWorktreeErrorCode::Interrupted
        );
        let ClaimResult::Claimed(retried) = reopened.claim(&scope, &request("request-1")).unwrap()
        else {
            panic!("interrupted operation must allow a retry");
        };
        assert_ne!(retried.operation_id, result.operation_id);
        assert_eq!(retried.state, AgentWorktreeOperationState::Creating);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn backup_recovers_a_missing_primary_store() {
        let root =
            std::env::temp_dir().join(format!("codux-operation-backup-{}", uuid::Uuid::new_v4()));
        let store = OperationStore::open(&root).unwrap();
        let scope = scope("project-1");
        let ClaimResult::Claimed(result) = store.claim(&scope, &request("request-1")).unwrap()
        else {
            panic!("first operation must be newly claimed");
        };
        fs::copy(&store.path, backup_path(&store.path)).unwrap();
        fs::remove_file(&store.path).unwrap();
        drop(store);

        let recovered = OperationStore::open(&root).unwrap();
        let operation = recovered
            .operation_for_delivery(&scope, &result.operation_id)
            .unwrap()
            .0;

        assert_eq!(operation.state, AgentWorktreeOperationState::Failed);
        assert_eq!(
            operation.error.as_ref().map(|error| error.code),
            Some(AgentWorktreeErrorCode::Interrupted)
        );
        assert!(recovered.path.is_file());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn backup_recovers_a_corrupted_primary_store() {
        let root = std::env::temp_dir().join(format!(
            "codux-operation-corrupt-backup-{}",
            uuid::Uuid::new_v4()
        ));
        let store = OperationStore::open(&root).unwrap();
        let scope = scope("project-1");
        let ClaimResult::Claimed(result) = store.claim(&scope, &request("request-1")).unwrap()
        else {
            panic!("first operation must be newly claimed");
        };
        fs::copy(&store.path, backup_path(&store.path)).unwrap();
        fs::write(&store.path, b"not json").unwrap();
        drop(store);

        let recovered = OperationStore::open(&root).unwrap();
        assert!(
            recovered
                .operation_for_delivery(&scope, &result.operation_id)
                .is_ok()
        );
        assert!(
            serde_json::from_slice::<StoredOperationsFile>(&fs::read(&recovered.path).unwrap())
                .is_ok()
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn failed_claim_persistence_does_not_reserve_request_id() {
        let root = std::env::temp_dir().join(format!(
            "codux-operation-claim-rollback-{}",
            uuid::Uuid::new_v4()
        ));
        let store = OperationStore::open(&root).unwrap();
        let scope = scope("project-1");
        fs::create_dir_all(&store.path).unwrap();

        assert!(store.claim(&scope, &request("request-1")).is_err());

        fs::remove_dir(&store.path).unwrap();
        assert!(matches!(
            store.claim(&scope, &request("request-1")).unwrap(),
            ClaimResult::Claimed(_)
        ));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn request_ids_are_scoped_to_the_calling_worktree() {
        let root =
            std::env::temp_dir().join(format!("codux-operation-scope-{}", uuid::Uuid::new_v4()));
        let store = OperationStore::open(&root).unwrap();
        let first_scope = scope("project-1");
        let second_scope = scope("project-2");

        assert!(matches!(
            store.claim(&first_scope, &request("request-1")).unwrap(),
            ClaimResult::Claimed(_)
        ));
        assert!(matches!(
            store.claim(&second_scope, &request("request-1")).unwrap(),
            ClaimResult::Claimed(_)
        ));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn delivery_operations_are_scoped_to_the_calling_worktree() {
        let root = std::env::temp_dir().join(format!(
            "codux-operation-delivery-scope-{}",
            uuid::Uuid::new_v4()
        ));
        let store = OperationStore::open(&root).unwrap();
        let mut first_scope = scope("project-1");
        first_scope.source_worktree_id = "source-1".to_string();
        let mut second_scope = first_scope.clone();
        second_scope.source_worktree_id = "source-2".to_string();
        let ClaimResult::Claimed(result) =
            store.claim(&first_scope, &request("request-1")).unwrap()
        else {
            panic!("first operation must be newly claimed");
        };

        assert!(
            store
                .operation_for_delivery(&second_scope, &result.operation_id)
                .is_err()
        );
        assert!(
            store
                .mark_delivery(
                    &second_scope,
                    &result.operation_id,
                    codux_runtime_core::agent_worktree::AgentWorktreeDeliveryState::Merged,
                )
                .is_err()
        );

        fs::remove_dir_all(root).ok();
    }
}
