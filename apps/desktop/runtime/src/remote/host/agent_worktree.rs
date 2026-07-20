use super::*;
use codux_runtime_core::agent_worktree::{AgentWorktreeCreateRequest, AgentWorktreeTerminalScope};
use codux_runtime_live::agent_worktree::{
    AgentWorktreeCreatedWorktree, AgentWorktreeHost, AgentWorktreeTerminalPlan,
};
use std::sync::Weak;

pub(super) struct DesktopAgentWorktreeHost {
    pub(super) runtime: Weak<RemoteHostRuntime>,
}

impl DesktopAgentWorktreeHost {
    fn runtime(&self) -> Result<Arc<RemoteHostRuntime>, String> {
        self.runtime
            .upgrade()
            .ok_or_else(|| "The desktop runtime is unavailable.".to_string())
    }
}

impl AgentWorktreeHost for DesktopAgentWorktreeHost {
    fn create_worktree(
        &self,
        scope: &AgentWorktreeTerminalScope,
        request: &AgentWorktreeCreateRequest,
    ) -> Result<AgentWorktreeCreatedWorktree, String> {
        let runtime = self.runtime()?;
        if !scope.runtime_target.is_local() {
            return Err("The desktop worktree host only accepts local terminals.".to_string());
        }
        let source_branch = codux_git::worktree::current_branch(&scope.source_worktree_path)
            .ok_or_else(|| "Source worktree branch cannot be resolved.".to_string())?;
        let created = WorktreeService::new(runtime.support_dir.clone()).create_in_background(
            WorktreeCreateRequest {
                project_id: scope.root_project_id.clone(),
                project_path: scope.source_worktree_path.clone(),
                base_branch: request.base_branch.clone(),
                branch_name: request.name.clone(),
                task_title: Some(request.name.clone()),
            },
        )?;
        crate::runtime_state::note_pet_project_membership_change(&runtime.support_dir);
        runtime.broadcast_worktree_list_change(&scope.root_project_id, &scope.root_project_path);
        runtime.push_event(RemoteHostEvent::WorktreesChanged {
            project_id: scope.root_project_id.clone(),
            project_path: scope.root_project_path.clone(),
        });
        let base_branch = created
            .snapshot
            .tasks
            .iter()
            .find(|task| task.worktree_id == created.worktree.id)
            .map(|task| task.base_branch.clone())
            .filter(|branch| !branch.trim().is_empty());
        Ok(AgentWorktreeCreatedWorktree {
            id: created.worktree.id,
            name: created.worktree.name,
            branch: created.worktree.branch,
            path: created.worktree.path,
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
        let runtime = self.runtime()?;
        let layout_key = terminal_layout_storage_key(&scope.root_project_id, &worktree.id);
        let session_key = format!("gpui:{}:{}", worktree.id, plan.terminal_id);
        let launch_artifacts =
            crate::runtime_state::RuntimeService::prepare_memory_launch_artifacts_at(
                &runtime.support_dir,
                &scope.root_project_id,
                &worktree.id,
                &scope.project_name,
                &worktree.path,
            )
            .ok_or_else(|| "Unable to prepare the agent launch context.".to_string())?;
        let config = TerminalPtyConfig {
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
            support_dir: Some(runtime.support_dir.clone()),
            runtime_root: Some(codux_runtime_live::runtime_paths::runtime_root_dir()),
            session_instance_id: Some(
                uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, session_key.as_bytes()).to_string(),
            ),
            memory_workspace_root: Some(launch_artifacts.workspace_root),
            memory_prompt_file: Some(launch_artifacts.prompt_file),
            memory_index_file: Some(launch_artifacts.index_file),
            ..Default::default()
        };
        let event_runtime = Arc::clone(&runtime);
        let session_id = runtime
            .terminals
            .create_with_event_key(
                config,
                format!("remote-terminal:{}", plan.terminal_id),
                Arc::new(move |event| {
                    event_runtime.handle_terminal_event(event);
                    true
                }),
            )
            .map_err(|error| error.to_string())?;
        if session_id != plan.terminal_id {
            return Err("The desktop runtime returned an unexpected terminal id.".to_string());
        }
        TerminalLayoutService::new(runtime.support_dir.clone()).ensure_terminal(
            &layout_key,
            &session_id,
            &plan.title,
        )?;
        runtime.mark_terminal_event_subscription(&session_id);
        runtime.publish_remote_terminal_layout_changed();
        runtime.broadcast_terminal_list(None);
        Ok(())
    }

    fn merge_worktree(
        &self,
        scope: &AgentWorktreeTerminalScope,
        worktree: &AgentWorktreeCreatedWorktree,
    ) -> Result<(), String> {
        let runtime = self.runtime()?;
        codux_git::worktree::merge_worktree_into_source(
            &scope.source_worktree_path,
            &worktree.path,
            Some(&worktree.source_branch),
        )?;
        WorktreeService::new(runtime.support_dir.clone())
            .sync_from_git(&scope.root_project_id, &scope.root_project_path)?;
        runtime.broadcast_worktree_list_change(&scope.root_project_id, &scope.root_project_path);
        runtime.push_event(RemoteHostEvent::WorktreesChanged {
            project_id: scope.root_project_id.clone(),
            project_path: scope.root_project_path.clone(),
        });
        Ok(())
    }

    fn remove_worktree(
        &self,
        scope: &AgentWorktreeTerminalScope,
        worktree: &AgentWorktreeCreatedWorktree,
    ) -> Result<(), String> {
        let runtime = self.runtime()?;
        codux_git::worktree::ensure_merged_worktree_removable(
            &scope.source_worktree_path,
            &worktree.path,
            &worktree.branch,
        )?;
        let layout_key = terminal_layout_storage_key(&scope.root_project_id, &worktree.id);
        let layout =
            TerminalLayoutService::new(runtime.support_dir.clone()).load(Some(&layout_key));
        for terminal_id in layout
            .top_panes
            .iter()
            .map(|pane| pane.terminal_id.as_str())
            .chain(layout.tabs.iter().map(|tab| tab.terminal_id.as_str()))
        {
            runtime
                .terminals
                .kill_and_wait_if_present(terminal_id, std::time::Duration::from_secs(10))
                .map_err(|error| error.to_string())?;
        }
        codux_git::worktree::remove_merged_worktree(
            &scope.source_worktree_path,
            &worktree.path,
            &worktree.branch,
        )?;
        WorktreeService::new(runtime.support_dir.clone())
            .sync_from_git(&scope.root_project_id, &scope.root_project_path)?;
        TerminalLayoutService::new(runtime.support_dir.clone()).delete(&layout_key)?;
        crate::runtime_state::note_pet_project_membership_change(&runtime.support_dir);
        runtime.broadcast_worktree_list_change(&scope.root_project_id, &scope.root_project_path);
        runtime.push_event(RemoteHostEvent::WorktreesChanged {
            project_id: scope.root_project_id.clone(),
            project_path: scope.root_project_path.clone(),
        });
        runtime.publish_remote_terminal_layout_changed();
        runtime.broadcast_terminal_list(None);
        Ok(())
    }
}
