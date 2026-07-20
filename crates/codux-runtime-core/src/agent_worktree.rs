use crate::runtime_target::RuntimeTarget;
use serde::{Deserialize, Serialize};

pub const AGENT_WORKTREE_CONTROL_ADDRESS_ENV: &str = "CODUX_WORKTREE_CONTROL_ADDRESS";
pub const AGENT_WORKTREE_CONTROL_CAPABILITY_ENV: &str = "CODUX_WORKTREE_CONTROL_CAPABILITY";

pub fn agent_worktree_ai_directive() -> &'static str {
    "## Agent Worktrees\n\
- To delegate independent work, run `codux-worktree create --name <branch> --agent <tool> --prompt <task> --json`.\n\
- You own the delegated task end to end. Give the child explicit acceptance criteria and require it to test and commit its intended changes before finishing. The command creates a managed worktree and terminal, waits for the child agent to complete, then returns its worktree path and stable IDs.\n\
- After completion, inspect the returned worktree's commits and diff against `baseBranch`, confirm `sourceBranch` is still the intended integration target, and run the relevant tests yourself. If review fails, keep the worktree and continue the child or fix it there; do not merge or remove it.\n\
- Only after review passes, run `codux-worktree merge --operation <operationId> --json`, followed by `codux-worktree remove --operation <operationId> --json`. Merge requires a completed task, a clean child worktree, and no tracked changes in the source worktree; unrelated untracked source files are preserved. Never stash, delete, or modify the user's source-worktree files to make delivery pass. If tracked changes or a path conflict block merging, preserve both worktrees and report the blocker. Remove requires the reviewed commits to be merged and closes the child terminal before safe cleanup. Perform routine delivery steps yourself instead of asking the user.\n\
- For normal delegated delivery, never pass `--detach`: keep the same `create` invocation blocked until OSC reports completion, then review, merge, and remove. If that invocation is still running, continue waiting for it and never issue a second `create` for the same task. Use `--detach` only when the user explicitly asks to start background work; never poll the interactive child process, because its TUI remains open after a turn completes.\n\
- Do not use raw `git worktree add`, `git worktree remove`, or manual branch deletion for delegated work; they bypass Codux orchestration and status tracking."
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorktreeCreateRequest {
    pub request_id: String,
    pub name: String,
    pub agent: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
}

impl AgentWorktreeCreateRequest {
    pub fn validate(&self) -> Result<(), AgentWorktreeError> {
        for (field, value) in [
            ("requestId", self.request_id.as_str()),
            ("name", self.name.as_str()),
            ("agent", self.agent.as_str()),
            ("prompt", self.prompt.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(AgentWorktreeError::new(
                    AgentWorktreeErrorCode::InvalidRequest,
                    format!("{field} is required"),
                ));
            }
        }
        if self.name.trim() != self.name {
            return Err(AgentWorktreeError::new(
                AgentWorktreeErrorCode::InvalidRequest,
                "name cannot have leading or trailing whitespace",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentWorktreeOperationState {
    Creating,
    Starting,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentWorktreeTaskState {
    Pending,
    Running,
    Waiting,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentWorktreeErrorCode {
    InvalidRequest,
    Unauthorized,
    UnsupportedAgent,
    WorktreeCreateFailed,
    TerminalCreateFailed,
    WorktreeMergeFailed,
    WorktreeRemoveFailed,
    TaskFailed,
    TaskInterrupted,
    Interrupted,
    Internal,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorktreeError {
    pub code: AgentWorktreeErrorCode,
    pub message: String,
}

impl AgentWorktreeError {
    pub fn new(code: AgentWorktreeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorktreeCreateResult {
    pub request_id: String,
    pub operation_id: String,
    pub state: AgentWorktreeOperationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_state: Option<AgentWorktreeTaskState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_error: Option<AgentWorktreeError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_state: Option<AgentWorktreeDeliveryState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentWorktreeError>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "command",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AgentWorktreeCommand {
    Create {
        request: AgentWorktreeCreateRequest,
        #[serde(default)]
        wait_for_completion: bool,
    },
    Merge {
        operation_id: String,
    },
    Remove {
        operation_id: String,
    },
}

impl AgentWorktreeCommand {
    pub fn validate(&self) -> Result<(), AgentWorktreeError> {
        match self {
            Self::Create { request, .. } => request.validate(),
            Self::Merge { operation_id } | Self::Remove { operation_id } => {
                if operation_id.trim().is_empty() {
                    return Err(AgentWorktreeError::new(
                        AgentWorktreeErrorCode::InvalidRequest,
                        "operationId is required",
                    ));
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentWorktreeDeliveryState {
    Merged,
    Removed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorktreeDeliveryResult {
    pub operation_id: String,
    pub worktree_id: String,
    pub worktree_path: String,
    pub terminal_id: String,
    pub state: AgentWorktreeDeliveryState,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgentWorktreeCommandResult {
    Create { result: AgentWorktreeCreateResult },
    Delivery { result: AgentWorktreeDeliveryResult },
    Error { error: AgentWorktreeError },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorktreeTerminalScope {
    pub root_project_id: String,
    pub root_project_path: String,
    pub project_name: String,
    pub source_worktree_id: String,
    pub source_worktree_path: String,
    #[serde(default)]
    pub runtime_target: RuntimeTarget,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorktreeControlRequest {
    pub capability: String,
    #[serde(flatten)]
    pub command: AgentWorktreeCommand,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorktreeControlResponse {
    pub result: AgentWorktreeCommandResult,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_result_keep_camel_case_contract() {
        let request = AgentWorktreeCreateRequest {
            request_id: "request-1".to_string(),
            name: "fix-login".to_string(),
            agent: "codex".to_string(),
            prompt: "Fix login".to_string(),
            base_branch: Some("main".to_string()),
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["requestId"], "request-1");
        assert_eq!(value["baseBranch"], "main");
        assert!(value.get("sourceBranch").is_none());

        let result = AgentWorktreeCreateResult {
            request_id: request.request_id,
            operation_id: "operation-1".to_string(),
            state: AgentWorktreeOperationState::Ready,
            worktree_id: Some("worktree-1".to_string()),
            worktree_path: Some("/repo/worktree-1".to_string()),
            base_branch: Some("main".to_string()),
            source_branch: Some("feature/parent".to_string()),
            terminal_id: Some("terminal-1".to_string()),
            task_state: Some(AgentWorktreeTaskState::Completed),
            task_error: None,
            terminal_exit_code: None,
            delivery_state: None,
            error: None,
        };
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["state"], "ready");
        assert_eq!(value["taskState"], "completed");
        assert_eq!(value["worktreeId"], "worktree-1");
        assert_eq!(value["worktreePath"], "/repo/worktree-1");
        assert_eq!(value["baseBranch"], "main");
        assert_eq!(value["sourceBranch"], "feature/parent");
        assert!(value.get("error").is_none());

        let control = AgentWorktreeControlRequest {
            capability: "capability-1".to_string(),
            command: AgentWorktreeCommand::Create {
                request: AgentWorktreeCreateRequest {
                    request_id: "request-2".to_string(),
                    name: "fix-settings".to_string(),
                    agent: "codex".to_string(),
                    prompt: "Fix settings".to_string(),
                    base_branch: None,
                },
                wait_for_completion: true,
            },
        };
        let value = serde_json::to_value(control).unwrap();
        assert_eq!(value["command"], "create");
        assert_eq!(value["waitForCompletion"], true);
    }

    #[test]
    fn request_requires_all_caller_fields() {
        let request = AgentWorktreeCreateRequest {
            request_id: "request-1".to_string(),
            name: "fix-login".to_string(),
            agent: "codex".to_string(),
            prompt: " ".to_string(),
            base_branch: None,
        };
        assert_eq!(
            request.validate().unwrap_err().code,
            AgentWorktreeErrorCode::InvalidRequest
        );

        let request = AgentWorktreeCreateRequest {
            prompt: "Fix login".to_string(),
            name: " fix-login ".to_string(),
            ..request
        };
        assert_eq!(
            request.validate().unwrap_err().message,
            "name cannot have leading or trailing whitespace"
        );
    }

    #[test]
    fn ai_directive_describes_the_closed_loop_contract() {
        let directive = agent_worktree_ai_directive();
        assert!(directive.contains("codux-worktree create"));
        assert!(directive.contains("waits for the child agent to complete"));
        assert!(directive.contains("commits and diff against `baseBranch`"));
        assert!(directive.contains("`sourceBranch`"));
        assert!(directive.contains("codux-worktree merge"));
        assert!(directive.contains("codux-worktree remove"));
        assert!(directive.contains("Never stash"));
        assert!(directive.contains("unrelated untracked source files are preserved"));
        assert!(directive.contains("instead of asking the user"));
        assert!(directive.contains("never pass `--detach`"));
        assert!(directive.contains("never issue a second `create`"));
        assert!(directive.contains("never poll the interactive child process"));
    }

    #[test]
    fn delivery_commands_require_an_operation_id() {
        let command = AgentWorktreeCommand::Merge {
            operation_id: " ".to_string(),
        };
        assert_eq!(
            command.validate().unwrap_err().code,
            AgentWorktreeErrorCode::InvalidRequest
        );
    }
}
