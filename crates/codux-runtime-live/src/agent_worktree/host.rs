use codux_runtime_core::agent_worktree::{AgentWorktreeCreateRequest, AgentWorktreeTerminalScope};
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentWorktreeCreatedWorktree {
    pub id: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub base_branch: Option<String>,
    pub source_branch: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentWorktreeTerminalPlan {
    pub terminal_id: String,
    pub operation_id: String,
    pub tool: String,
    pub title: String,
    pub command: String,
    pub env: HashMap<String, String>,
    pub prompt_path: PathBuf,
}

pub trait AgentWorktreeHost: Send + Sync {
    fn create_worktree(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request: &AgentWorktreeCreateRequest,
    ) -> Result<AgentWorktreeCreatedWorktree, String>;

    fn create_terminal(
        &self,
        scope: &AgentWorktreeTerminalScope,
        worktree: &AgentWorktreeCreatedWorktree,
        plan: &AgentWorktreeTerminalPlan,
    ) -> Result<(), String>;

    fn merge_worktree(
        &self,
        scope: &AgentWorktreeTerminalScope,
        worktree: &AgentWorktreeCreatedWorktree,
    ) -> Result<(), String>;

    fn remove_worktree(
        &self,
        scope: &AgentWorktreeTerminalScope,
        worktree: &AgentWorktreeCreatedWorktree,
    ) -> Result<(), String>;
}
