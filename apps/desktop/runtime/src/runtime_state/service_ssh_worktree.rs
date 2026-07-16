impl RuntimeService {
    pub fn reload_ssh(&self, runtime_assets: PathBuf) -> SSHSummary {
        load_ssh(&self.support_dir, runtime_assets)
    }

    pub fn ssh_profiles(&self) -> SSHProfilesSnapshot {
        SSHStore::from_support_dir(self.support_dir.clone()).snapshot()
    }

    pub fn upsert_ssh_profile(
        &self,
        request: SSHProfileUpsertRequest,
    ) -> Result<SSHProfilesSnapshot, String> {
        SSHStore::from_support_dir(self.support_dir.clone()).upsert(request)
    }

    pub fn delete_ssh_profile(&self, profile_id: String) -> Result<SSHProfilesSnapshot, String> {
        SSHStore::from_support_dir(self.support_dir.clone()).delete(profile_id)
    }

    pub fn test_ssh_profile(
        &self,
        request: SSHProfileUpsertRequest,
        runtime_assets: PathBuf,
    ) -> Result<SSHProfileTestResult, String> {
        SSHStore::from_support_dir(self.support_dir.clone()).test_profile(request, &runtime_assets)
    }

    pub fn ssh_launch_command(&self, profile_id: String) -> Result<SSHLaunchCommand, String> {
        SSHStore::from_support_dir(self.support_dir.clone()).launch_command(profile_id)
    }

    pub fn ssh_launch_context(&self, codux_ssh_command: Option<String>) -> Option<String> {
        render_ssh_launch_context_from_support_dir(self.support_dir.clone(), codux_ssh_command)
    }

    pub fn reload_db(&self, runtime_assets: PathBuf, project_id: Option<&str>) -> DBSummary {
        load_db(&self.support_dir, runtime_assets, project_id)
    }

    pub fn db_profiles(&self, project_id: Option<&str>) -> DBProfilesSnapshot {
        DBStore::from_support_dir(self.support_dir.clone()).snapshot(project_id)
    }

    pub fn upsert_db_profile(
        &self,
        request: DBProfileUpsertRequest,
    ) -> Result<DBProfilesSnapshot, String> {
        DBStore::from_support_dir(self.support_dir.clone()).upsert(request)
    }

    pub fn delete_db_profile(
        &self,
        project_id: &str,
        profile_id: String,
    ) -> Result<DBProfilesSnapshot, String> {
        DBStore::from_support_dir(self.support_dir.clone()).delete(project_id, profile_id)
    }

    pub fn test_db_profile(
        &self,
        request: DBProfileUpsertRequest,
        runtime_assets: PathBuf,
    ) -> Result<DBQueryResult, String> {
        DBStore::from_support_dir(self.support_dir.clone()).test_profile(request, &runtime_assets)
    }

    pub fn reload_terminal_layout(&self, project_id: Option<&str>) -> TerminalLayoutSummary {
        load_terminal_layout(&self.support_dir, project_id)
    }

    pub fn reload_terminal_layouts<'a, I>(
        &self,
        project_ids: I,
    ) -> std::collections::HashMap<String, TerminalLayoutSummary>
    where
        I: IntoIterator<Item = &'a str>,
    {
        TerminalLayoutService::new(self.support_dir.clone()).load_many(project_ids)
    }

    pub fn delete_terminal_layout(&self, project_id: &str) -> Result<bool, String> {
        TerminalLayoutService::new(self.support_dir.clone()).delete(project_id)
    }

    pub fn reload_file_editor_layout(&self, owner_id: Option<&str>) -> FileEditorLayoutSummary {
        FileEditorLayoutService::new(self.support_dir.clone()).load(owner_id)
    }

    pub fn reload_worktrees(
        &self,
        project_id: Option<&str>,
        project_path: Option<&str>,
    ) -> WorktreeSummary {
        if let Some(path) = project_path
            && let Some(runtime) = self.hosted_runtime_for_project_path(path)
        {
            return match runtime {
                Ok(runtime) => {
                    self.hosted_worktree_summary(&runtime, project_id.unwrap_or_default(), path)
                }
                Err(error) => WorktreeSummary {
                    error: Some(error),
                    ..Default::default()
                },
            };
        }
        load_worktrees(&self.support_dir, project_id, project_path)
    }

    pub fn reload_worktrees_from_state(
        &self,
        project_id: Option<&str>,
        project_path: Option<&str>,
    ) -> WorktreeSummary {
        if let Some(path) = project_path
            && let Some(runtime) = self.hosted_runtime_for_project_path(path)
        {
            return match runtime {
                Ok(runtime) => {
                    self.hosted_worktree_summary(&runtime, project_id.unwrap_or_default(), path)
                }
                Err(error) => WorktreeSummary {
                    error: Some(error),
                    ..Default::default()
                },
            };
        }
        WorktreeService::new(self.support_dir.clone()).state_summary(project_id, project_path)
    }

    pub fn cached_worktrees_from_state(
        &self,
        project_id: Option<&str>,
        project_path: Option<&str>,
    ) -> WorktreeSummary {
        let hosted_paths = project_path
            .and_then(|path| {
                ProjectStore::new(self.support_dir.clone())
                    .runtime_target_for_workspace_path(path)
                    .ok()
            })
            .is_some_and(|target| target.is_hosted());
        let service = WorktreeService::new(self.support_dir.clone());
        if hosted_paths {
            service.hosted_state_summary(project_id, project_path)
        } else {
            service.state_summary(project_id, project_path)
        }
    }

    pub fn worktree_snapshot(&self, project_id: String, project_path: String) -> WorktreeSnapshot {
        WorktreeService::new(self.support_dir.clone()).snapshot(project_id, project_path)
    }

    pub fn create_worktree_from_request(
        &self,
        request: WorktreeCreateRequest,
    ) -> Result<WorktreeSnapshot, String> {
        let project_id = request.project_id.clone();
        let project_path = request.project_path.clone();
        if let Some(runtime) = self.hosted_runtime_for_project_path(&project_path) {
            return runtime.and_then(|runtime| {
                runtime
                    .worktree_create(
                        &project_id,
                        &project_path,
                        &request.branch_name,
                        request.base_branch.as_deref(),
                        request.task_title.as_deref(),
                    )
                    .and_then(|value| {
                        self.sync_hosted_created_worktree_snapshot(
                            &project_id,
                            &value,
                            request.task_title.as_deref(),
                            request.base_branch.as_deref(),
                        )
                    })
            });
        }
        let result = WorktreeService::new(self.support_dir.clone()).create_from_request(request);
        if result.is_ok() {
            self.remote_host
                .broadcast_worktree_list_change(&project_id, &project_path);
            self.sync_pet_project_memberships();
        }
        result
    }

    pub fn remove_worktree_from_request(
        &self,
        request: WorktreeRemoveRequest,
    ) -> Result<WorktreeSnapshot, String> {
        if let Some(runtime) = self.hosted_runtime_for_project_path(&request.project_path) {
            return runtime.and_then(|runtime| {
                runtime
                    .worktree_remove(
                        &request.project_id,
                        &request.project_path,
                        &request.worktree_path,
                        request.remove_branch,
                    )
                    .and_then(|value| {
                        self.sync_hosted_worktree_snapshot(&request.project_id, &value, false)
                    })
            });
        }
        let project_id = request.project_id.clone();
        let project_path = request.project_path.clone();
        let result = WorktreeService::new(self.support_dir.clone()).remove_from_request(request);
        if result.is_ok() {
            self.remote_host
                .broadcast_worktree_list_change(&project_id, &project_path);
            self.sync_pet_project_memberships();
        }
        result
    }

    pub fn merge_worktree_from_request(
        &self,
        request: WorktreeMergeRequest,
    ) -> Result<WorktreeSnapshot, String> {
        if let Some(runtime) = self.hosted_runtime_for_project_path(&request.project_path) {
            return runtime.and_then(|runtime| {
                runtime
                    .worktree_merge(
                        &request.project_id,
                        &request.project_path,
                        &request.worktree_path,
                        request.base_branch.as_deref(),
                        request.remove_branch.unwrap_or(false),
                    )
                    .and_then(|value| {
                        self.sync_hosted_worktree_snapshot(&request.project_id, &value, false)
                    })
            });
        }
        let project_id = request.project_id.clone();
        let project_path = request.project_path.clone();
        let removes_worktree = request.remove_branch.unwrap_or(false);
        let result = WorktreeService::new(self.support_dir.clone()).merge_from_request(request);
        if result.is_ok() {
            self.remote_host
                .broadcast_worktree_list_change(&project_id, &project_path);
            if removes_worktree {
                self.sync_pet_project_memberships();
            }
        }
        result
    }

    pub fn select_worktree(&self, project_id: &str, worktree_id: &str) -> Result<(), String> {
        self.project_select_worktree(crate::project_store::ProjectSelectWorktreeRequest {
            project_id: project_id.to_string(),
            worktree_id: worktree_id.to_string(),
        })
    }

    pub fn sync_worktrees_from_git(
        &self,
        project_id: &str,
        project_path: &str,
    ) -> Result<WorktreeSummary, String> {
        let summary =
            WorktreeService::new(self.support_dir.clone()).sync_from_git(project_id, project_path)?;
        self.sync_pet_project_memberships();
        Ok(summary)
    }

    pub fn remove_worktree(
        &self,
        project_id: &str,
        project_path: &str,
        worktree_id: &str,
        remove_branch: bool,
    ) -> Result<WorktreeSummary, String> {
        let summary = self.reload_worktrees(Some(project_id), Some(project_path));
        let worktree = mutation_worktree(&summary, project_id, worktree_id, "removed")?;
        self.remove_worktree_from_request(WorktreeRemoveRequest {
            project_id: project_id.to_string(),
            project_path: project_path.to_string(),
            worktree_path: worktree.path,
            remove_branch,
        })?;
        Ok(self.reload_worktrees(Some(project_id), Some(project_path)))
    }

    pub fn merge_worktree(
        &self,
        project_id: &str,
        project_path: &str,
        worktree_id: &str,
    ) -> Result<WorktreeSummary, String> {
        let summary = self.reload_worktrees(Some(project_id), Some(project_path));
        let worktree = mutation_worktree(&summary, project_id, worktree_id, "merged")?;
        let base_branch = summary
            .tasks
            .iter()
            .find(|task| task.worktree_id == worktree_id)
            .map(|task| task.base_branch.trim().to_string())
            .filter(|branch| !branch.is_empty());
        self.merge_worktree_from_request(WorktreeMergeRequest {
            project_id: project_id.to_string(),
            project_path: project_path.to_string(),
            worktree_path: worktree.path,
            base_branch,
            remove_branch: Some(false),
        })?;
        Ok(self.reload_worktrees(Some(project_id), Some(project_path)))
    }

    pub fn save_terminal_layout(
        &self,
        project_id: &str,
        tabs: Vec<crate::terminal_layout::TerminalTabSummary>,
        _active_terminal_id: String,
        top_panes: Vec<crate::terminal_layout::TerminalPaneSummary>,
        top_ratios: Vec<f64>,
        bottom_ratio: f64,
    ) -> Result<TerminalLayoutSummary, String> {
        TerminalLayoutService::new(self.support_dir.clone()).save_from_gpui(
            project_id,
            tabs,
            top_panes,
            top_ratios,
            bottom_ratio,
        )
    }

    pub fn save_terminal_layout_with_grid(
        &self,
        project_id: &str,
        layout: TerminalLayoutSummary,
    ) -> Result<TerminalLayoutSummary, String> {
        TerminalLayoutService::new(self.support_dir.clone()).save_from_gpui_with_grid(
            project_id, layout,
        )
    }

    pub fn save_file_editor_layout(
        &self,
        owner_id: &str,
        tabs: Vec<FileEditorTabSummary>,
        active_path: Option<String>,
    ) -> Result<FileEditorLayoutSummary, String> {
        FileEditorLayoutService::new(self.support_dir.clone()).save_from_gpui(
            owner_id,
            tabs,
            active_path,
        )
    }

    fn sync_hosted_worktree_snapshot(
        &self,
        project_id: &str,
        value: &serde_json::Value,
        prefer_payload_selection: bool,
    ) -> Result<WorktreeSnapshot, String> {
        let mut snapshot = worktree_snapshot_from_payload(value)?;
        snapshot.tasks = self.sync_hosted_project_worktree_snapshot(
            project_id,
            &snapshot,
            value.get("defaultBaseBranch").and_then(Value::as_str),
            prefer_payload_selection,
        )?;
        Ok(snapshot)
    }

    fn sync_hosted_created_worktree_snapshot(
        &self,
        project_id: &str,
        value: &serde_json::Value,
        task_title: Option<&str>,
        base_branch: Option<&str>,
    ) -> Result<WorktreeSnapshot, String> {
        let mut snapshot = worktree_snapshot_from_payload(value)?;
        add_created_worktree_task(&mut snapshot, value, task_title, base_branch);
        snapshot.tasks = self.sync_hosted_project_worktree_snapshot(
            project_id,
            &snapshot,
            value.get("defaultBaseBranch").and_then(Value::as_str),
            true,
        )?;
        Ok(snapshot)
    }
}

fn mutation_worktree(
    summary: &WorktreeSummary,
    project_id: &str,
    worktree_id: &str,
    operation: &str,
) -> Result<crate::worktree::WorktreeInfo, String> {
    if let Some(error) = summary.error.as_deref() {
        return Err(error.to_string());
    }
    let worktree = summary
        .worktrees
        .iter()
        .find(|worktree| worktree.id == worktree_id)
        .cloned()
        .ok_or_else(|| "Worktree not found.".to_string())?;
    if worktree.is_default || worktree.id == project_id {
        return Err(format!("Default worktree cannot be {operation}."));
    }
    Ok(worktree)
}
