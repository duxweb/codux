use super::{RuntimeStdioWriter, terminal::RuntimeStdioTerminals};
use codux_runtime_core::{
    agent_worktree::{AgentWorktreeCreateRequest, AgentWorktreeTerminalScope},
    runtime_stdio::RuntimeStdioFrame,
    worktree::worktree_uuid,
};
use codux_runtime_live::{
    agent_worktree::{AgentWorktreeCreatedWorktree, AgentWorktreeHost, AgentWorktreeTerminalPlan},
    terminal_pty::TerminalPtyConfig,
};
use serde_json::json;

pub(super) struct RuntimeStdioAgentWorktreeHost {
    terminals: RuntimeStdioTerminals,
    writer: RuntimeStdioWriter,
}

impl RuntimeStdioAgentWorktreeHost {
    pub(super) fn new(terminals: RuntimeStdioTerminals, writer: RuntimeStdioWriter) -> Self {
        Self { terminals, writer }
    }
}

impl AgentWorktreeHost for RuntimeStdioAgentWorktreeHost {
    fn create_worktree(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request: &AgentWorktreeCreateRequest,
    ) -> Result<AgentWorktreeCreatedWorktree, String> {
        let source_branch = codux_git::worktree::current_branch(&scope.source_worktree_path)
            .ok_or_else(|| "Source worktree branch cannot be resolved.".to_string())?;
        let base_branch = request
            .base_branch
            .clone()
            .or_else(|| Some(source_branch.clone()));
        let path = codux_git::worktree::create_worktree(
            &scope.source_worktree_path,
            &request.name,
            request.base_branch.as_deref(),
        )?;
        let path = codux_git::normalize_repository_path(&path.to_string_lossy());
        Ok(AgentWorktreeCreatedWorktree {
            id: worktree_uuid(&scope.root_project_id, &path),
            name: request.name.clone(),
            branch: request.name.clone(),
            path,
            base_branch,
            source_branch,
        })
    }

    fn create_terminal(
        &self,
        scope: &AgentWorktreeTerminalScope,
        worktree: &AgentWorktreeCreatedWorktree,
        plan: &AgentWorktreeTerminalPlan,
    ) -> Result<(), String> {
        let session_key = format!("gpui:{}:{}", worktree.id, plan.terminal_id);
        let terminal = self.terminals.create_config(TerminalPtyConfig {
            cwd: Some(worktree.path.clone()),
            command: Some(plan.command.clone()),
            env: Some(plan.env.clone()),
            root_project_id: Some(scope.root_project_id.clone()),
            root_project_path: Some(scope.root_project_path.clone()),
            project_id: Some(worktree.id.clone()),
            project_name: Some(scope.project_name.clone()),
            terminal_id: Some(plan.terminal_id.clone()),
            session_key: Some(session_key.clone()),
            worktree_id: Some(worktree.id.clone()),
            title: Some(plan.title.clone()),
            tool: Some(plan.tool.clone()),
            runtime_target: scope.runtime_target.clone(),
            session_instance_id: Some(
                uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, session_key.as_bytes()).to_string(),
            ),
            ..Default::default()
        })?;
        if terminal.id != plan.terminal_id {
            return Err("The WSL runtime returned an unexpected terminal id.".to_string());
        }
        self.writer.write(&RuntimeStdioFrame::Event {
            method: "agentWorktree.created".to_string(),
            params: json!({
                "projectId": scope.root_project_id,
                "projectPath": scope.root_project_path,
                "worktreeId": worktree.id,
                "worktreePath": worktree.path,
                "terminalId": terminal.id,
                "title": plan.title,
            }),
        })
    }

    fn merge_worktree(
        &self,
        scope: &AgentWorktreeTerminalScope,
        worktree: &AgentWorktreeCreatedWorktree,
    ) -> Result<(), String> {
        codux_git::worktree::merge_worktree_into_source(
            &scope.source_worktree_path,
            &worktree.path,
            Some(&worktree.source_branch),
        )?;
        self.write_changed(scope, worktree, false)
    }

    fn remove_worktree(
        &self,
        scope: &AgentWorktreeTerminalScope,
        worktree: &AgentWorktreeCreatedWorktree,
    ) -> Result<(), String> {
        codux_git::worktree::ensure_merged_worktree_removable(
            &scope.source_worktree_path,
            &worktree.path,
            &worktree.branch,
        )?;
        for terminal in self
            .terminals
            .manager()
            .list()
            .into_iter()
            .filter(|terminal| terminal.worktree_id.as_deref() == Some(worktree.id.as_str()))
        {
            self.terminals
                .manager()
                .kill_and_wait_if_present(&terminal.id, std::time::Duration::from_secs(10))
                .map_err(|error| error.to_string())?;
        }
        codux_git::worktree::remove_merged_worktree(
            &scope.source_worktree_path,
            &worktree.path,
            &worktree.branch,
        )?;
        self.write_changed(scope, worktree, true)
    }
}

impl RuntimeStdioAgentWorktreeHost {
    fn write_changed(
        &self,
        scope: &AgentWorktreeTerminalScope,
        worktree: &AgentWorktreeCreatedWorktree,
        removed: bool,
    ) -> Result<(), String> {
        self.writer.write(&RuntimeStdioFrame::Event {
            method: "agentWorktree.changed".to_string(),
            params: json!({
                "projectId": scope.root_project_id,
                "projectPath": scope.root_project_path,
                "worktreeId": worktree.id,
                "removed": removed,
            }),
        })
    }
}
