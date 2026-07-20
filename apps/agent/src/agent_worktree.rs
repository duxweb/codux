use crate::terminals::{TerminalFanout, TransportSlot, broadcast_terminal_list, create_terminal};
use codux_protocol::REMOTE_WORKTREE_UPDATED;
use codux_runtime_core::{
    agent_worktree::{AgentWorktreeCreateRequest, AgentWorktreeTerminalScope},
    worktree::worktree_uuid,
};
use codux_runtime_live::{
    agent_worktree::{AgentWorktreeCreatedWorktree, AgentWorktreeHost, AgentWorktreeTerminalPlan},
    terminal_pty::{TerminalManager, TerminalPtyConfig},
};
use serde_json::json;
use std::sync::Arc;

pub(crate) struct HeadlessAgentWorktreeHost {
    terminals: Arc<TerminalManager>,
    transport: TransportSlot,
    fanout: TerminalFanout,
}

impl HeadlessAgentWorktreeHost {
    pub(crate) fn new(
        terminals: Arc<TerminalManager>,
        transport: TransportSlot,
        fanout: TerminalFanout,
    ) -> Self {
        Self {
            terminals,
            transport,
            fanout,
        }
    }

    fn broadcast_worktrees(&self, project_id: &str, project_path: &str) {
        let envelope = json!({
            "type": REMOTE_WORKTREE_UPDATED,
            "payload": crate::worktree::worktree_list_payload(project_id, project_path),
        });
        let Ok(bytes) = serde_json::to_vec(&envelope) else {
            return;
        };
        if let Ok(transport) = self.transport.lock()
            && let Some(transport) = transport.as_ref()
        {
            transport.send(bytes, None);
        }
    }
}

impl AgentWorktreeHost for HeadlessAgentWorktreeHost {
    fn create_worktree(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request: &AgentWorktreeCreateRequest,
    ) -> Result<AgentWorktreeCreatedWorktree, String> {
        if !scope.runtime_target.is_local() {
            return Err("The headless worktree host only accepts local terminals.".to_string());
        }
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
        let worktree = AgentWorktreeCreatedWorktree {
            id: worktree_uuid(&scope.root_project_id, &path),
            name: request.name.clone(),
            branch: request.name.clone(),
            path,
            base_branch,
            source_branch,
        };
        self.broadcast_worktrees(&scope.root_project_id, &scope.root_project_path);
        Ok(worktree)
    }

    fn create_terminal(
        &self,
        scope: &AgentWorktreeTerminalScope,
        worktree: &AgentWorktreeCreatedWorktree,
        plan: &AgentWorktreeTerminalPlan,
    ) -> Result<(), String> {
        let session_key = format!("gpui:{}:{}", worktree.id, plan.terminal_id);
        let terminal = create_terminal(
            &self.terminals,
            &self.transport,
            &self.fanout,
            TerminalPtyConfig {
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
                    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, session_key.as_bytes())
                        .to_string(),
                ),
                ..Default::default()
            },
            Some(&scope.root_project_id),
        )?;
        if terminal.id != plan.terminal_id {
            return Err("The headless runtime returned an unexpected terminal id.".to_string());
        }
        broadcast_terminal_list(&self.terminals, &self.transport, &self.fanout);
        Ok(())
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
        self.broadcast_worktrees(&scope.root_project_id, &scope.root_project_path);
        Ok(())
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
            .list()
            .into_iter()
            .filter(|terminal| terminal.worktree_id.as_deref() == Some(worktree.id.as_str()))
        {
            self.terminals
                .kill_and_wait_if_present(&terminal.id, std::time::Duration::from_secs(10))
                .map_err(|error| error.to_string())?;
        }
        codux_git::worktree::remove_merged_worktree(
            &scope.source_worktree_path,
            &worktree.path,
            &worktree.branch,
        )?;
        self.broadcast_worktrees(&scope.root_project_id, &scope.root_project_path);
        broadcast_terminal_list(&self.terminals, &self.transport, &self.fanout);
        Ok(())
    }
}
