use super::{
    capability::CapabilityRegistry, host::AgentWorktreeHost, service::AgentWorktreeService,
};
use crate::ai_runtime::AIRuntimeBridge;
use codux_runtime_core::agent_worktree::{
    AgentWorktreeCommand, AgentWorktreeCommandResult, AgentWorktreeControlRequest,
    AgentWorktreeControlResponse, AgentWorktreeCreateResult, AgentWorktreeError,
    AgentWorktreeErrorCode, AgentWorktreeOperationState, AgentWorktreeTerminalScope,
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{Arc, Mutex, RwLock, Weak},
    time::Duration,
};

const MAX_REQUEST_BYTES: u64 = 1024 * 1024;

pub struct AgentWorktreeControl {
    address: String,
    capabilities: Mutex<CapabilityRegistry>,
    host: RwLock<Option<Weak<dyn AgentWorktreeHost>>>,
    service: AgentWorktreeService,
}

impl AgentWorktreeControl {
    pub fn start(root: &Path, ai_runtime: Arc<AIRuntimeBridge>) -> Result<Arc<Self>, String> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        let control = Arc::new(Self {
            address: address.to_string(),
            capabilities: Mutex::new(CapabilityRegistry::default()),
            host: RwLock::new(None),
            service: AgentWorktreeService::open(root, ai_runtime)?,
        });
        spawn_listener(listener, Arc::downgrade(&control));
        Ok(control)
    }

    pub fn set_host(&self, host: Arc<dyn AgentWorktreeHost>) {
        *self.host.write().unwrap_or_else(|error| error.into_inner()) = Some(Arc::downgrade(&host));
    }

    pub fn grant_terminal(
        &self,
        terminal_id: String,
        scope: AgentWorktreeTerminalScope,
    ) -> (String, String) {
        let grant = self
            .capabilities
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .grant(terminal_id, scope);
        (self.address.clone(), grant.capability)
    }

    pub fn revoke_terminal(&self, terminal_id: &str) {
        self.capabilities
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .revoke_terminal(terminal_id);
    }

    pub fn revoke_capability(&self, capability: &str) {
        self.capabilities
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .revoke_capability(capability);
    }

    #[cfg(test)]
    pub(crate) fn has_terminal_capability(&self, terminal_id: &str) -> bool {
        self.capabilities
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains_terminal(terminal_id)
    }

    #[cfg(test)]
    pub(crate) fn has_capability(&self, capability: &str) -> bool {
        self.capabilities
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains_capability(capability)
    }

    #[cfg(test)]
    pub(crate) fn terminal_capability(&self, terminal_id: &str) -> Option<String> {
        self.capabilities
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .terminal_capability(terminal_id)
    }

    #[cfg(test)]
    pub(crate) fn address(&self) -> &str {
        &self.address
    }

    fn dispatch(&self, request: AgentWorktreeControlRequest) -> AgentWorktreeCommandResult {
        let grant = self
            .capabilities
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .resolve(&request.capability);
        let Some(grant) = grant else {
            return control_failure(
                AgentWorktreeErrorCode::Unauthorized,
                "The terminal capability is invalid or expired.",
            );
        };
        let host = self
            .host
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .and_then(Weak::upgrade);
        let Some(host) = host else {
            return control_failure(
                AgentWorktreeErrorCode::Internal,
                "The agent worktree service is unavailable.",
            );
        };
        if let Err(error) = request.command.validate() {
            return AgentWorktreeCommandResult::Error { error };
        }
        match request.command {
            AgentWorktreeCommand::Create {
                request,
                wait_for_completion,
            } => create_command_result(self.service.create(
                host,
                grant.scope,
                request,
                wait_for_completion,
            )),
            AgentWorktreeCommand::Merge { operation_id } => self
                .service
                .merge(host, grant.scope, &operation_id)
                .map(|result| AgentWorktreeCommandResult::Delivery { result })
                .unwrap_or_else(|error| AgentWorktreeCommandResult::Error { error }),
            AgentWorktreeCommand::Remove { operation_id } => self
                .service
                .remove(host, grant.scope, &operation_id)
                .map(|result| AgentWorktreeCommandResult::Delivery { result })
                .unwrap_or_else(|error| AgentWorktreeCommandResult::Error { error }),
        }
    }
}

fn create_command_result(result: AgentWorktreeCreateResult) -> AgentWorktreeCommandResult {
    if result.state == AgentWorktreeOperationState::Failed && result.worktree_id.is_none() {
        return AgentWorktreeCommandResult::Error {
            error: result.error.unwrap_or_else(|| {
                AgentWorktreeError::new(
                    AgentWorktreeErrorCode::Internal,
                    "The agent worktree operation failed before creation.",
                )
            }),
        };
    }
    AgentWorktreeCommandResult::Create { result }
}

fn spawn_listener(listener: TcpListener, control: Weak<AgentWorktreeControl>) {
    std::thread::Builder::new()
        .name("codux-agent-worktree-control".to_string())
        .spawn(move || {
            loop {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let Some(control) = control.upgrade() else {
                            return;
                        };
                        let _ = std::thread::Builder::new()
                            .name("codux-agent-worktree-request".to_string())
                            .spawn(move || handle_connection(stream, control));
                    }
                    Err(_) => return,
                }
            }
        })
        .expect("spawn agent worktree control listener");
}

impl Drop for AgentWorktreeControl {
    fn drop(&mut self) {
        let _ = TcpStream::connect(&self.address);
    }
}

fn handle_connection(mut stream: TcpStream, control: Arc<AgentWorktreeControl>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let request = {
        let mut line = String::new();
        let mut reader = BufReader::new((&stream).take(MAX_REQUEST_BYTES));
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => serde_json::from_str::<AgentWorktreeControlRequest>(&line),
        }
    };
    let result = match request {
        Ok(request) => control.dispatch(request),
        Err(error) => control_failure(AgentWorktreeErrorCode::InvalidRequest, error.to_string()),
    };
    let response = AgentWorktreeControlResponse { result };
    if let Ok(mut bytes) = serde_json::to_vec(&response) {
        bytes.push(b'\n');
        let _ = stream.write_all(&bytes);
        let _ = stream.flush();
    }
}

fn control_failure(
    code: AgentWorktreeErrorCode,
    message: impl Into<String>,
) -> AgentWorktreeCommandResult {
    AgentWorktreeCommandResult::Error {
        error: AgentWorktreeError::new(code, message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_before_worktree_creation_is_an_error_without_operation_id() {
        let result = create_command_result(AgentWorktreeCreateResult {
            request_id: "request-1".to_string(),
            operation_id: "operation-1".to_string(),
            state: AgentWorktreeOperationState::Failed,
            worktree_id: None,
            worktree_path: None,
            base_branch: None,
            source_branch: None,
            terminal_id: None,
            task_state: None,
            task_error: None,
            terminal_exit_code: None,
            delivery_state: None,
            error: Some(AgentWorktreeError::new(
                AgentWorktreeErrorCode::WorktreeCreateFailed,
                "worktree already exists",
            )),
        });

        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["kind"], "error");
        assert_eq!(value["error"]["code"], "worktreeCreateFailed");
        assert!(value.get("operationId").is_none());
        assert!(value.get("result").is_none());
    }
}
