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

    pub fn reload_file_editor_layout(&self, owner_id: Option<&str>) -> FileEditorLayoutSummary {
        FileEditorLayoutService::new(self.support_dir.clone()).load(owner_id)
    }

    pub fn reload_worktrees(
        &self,
        project_id: Option<&str>,
        project_path: Option<&str>,
    ) -> WorktreeSummary {
        if let Some(path) = project_path {
            if let Some(device_id) = self.host_device_for_project_path(path) {
                return self.remote_worktree_summary(
                    &device_id,
                    project_id.unwrap_or_default(),
                    path,
                );
            }
        }
        load_worktrees(&self.support_dir, project_id, project_path)
    }

    pub fn reload_worktrees_from_state(
        &self,
        project_id: Option<&str>,
        project_path: Option<&str>,
    ) -> WorktreeSummary {
        if let Some(path) = project_path {
            if let Some(device_id) = self.host_device_for_project_path(path) {
                return self.remote_worktree_summary(
                    &device_id,
                    project_id.unwrap_or_default(),
                    path,
                );
            }
        }
        WorktreeService::new(self.support_dir.clone()).state_summary(project_id, project_path)
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
        if let Some(result) = self.remote_worktree_mutation(&project_path, |controller| {
            controller.worktree_create(
                &project_id,
                &project_path,
                &request.branch_name,
                request.base_branch.as_deref(),
            )
        }) {
            return result;
        }
        let result = WorktreeService::new(self.support_dir.clone()).create_from_request(request);
        if result.is_ok() {
            self.remote_host
                .broadcast_worktree_list_change(&project_id, &project_path);
        }
        result
    }

    pub fn remove_worktree_from_request(
        &self,
        request: WorktreeRemoveRequest,
    ) -> Result<WorktreeSnapshot, String> {
        if let Some(result) = self.remote_worktree_mutation(&request.project_path, |controller| {
            controller.worktree_remove(
                &request.project_id,
                &request.project_path,
                &request.worktree_path,
                request.remove_branch,
            )
        }) {
            return result;
        }
        WorktreeService::new(self.support_dir.clone()).remove_from_request(request)
    }

    pub fn merge_worktree_from_request(
        &self,
        request: WorktreeMergeRequest,
    ) -> Result<WorktreeSnapshot, String> {
        if let Some(result) = self.remote_worktree_mutation(&request.project_path, |controller| {
            controller.worktree_merge(
                &request.project_id,
                &request.project_path,
                &request.worktree_path,
                request.base_branch.as_deref(),
                request.remove_branch.unwrap_or(false),
            )
        }) {
            return result;
        }
        WorktreeService::new(self.support_dir.clone()).merge_from_request(request)
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
        WorktreeService::new(self.support_dir.clone()).sync_from_git(project_id, project_path)
    }

    pub fn create_worktree(
        &self,
        project_id: &str,
        project_path: &str,
    ) -> Result<WorktreeSummary, String> {
        WorktreeService::new(self.support_dir.clone()).create_worktree(project_id, project_path)
    }

    pub fn remove_worktree(
        &self,
        project_id: &str,
        project_path: &str,
        worktree_id: &str,
        remove_branch: bool,
    ) -> Result<WorktreeSummary, String> {
        let result = WorktreeService::new(self.support_dir.clone()).remove_worktree(
            project_id,
            project_path,
            worktree_id,
            remove_branch,
        );
        if result.is_ok() {
            self.remote_host
                .broadcast_worktree_list_change(project_id, project_path);
        }
        result
    }

    pub fn merge_worktree(
        &self,
        project_id: &str,
        project_path: &str,
        worktree_id: &str,
    ) -> Result<WorktreeSummary, String> {
        let result = WorktreeService::new(self.support_dir.clone()).merge_worktree(
            project_id,
            project_path,
            worktree_id,
        );
        if result.is_ok() {
            self.remote_host
                .broadcast_worktree_list_change(project_id, project_path);
        }
        result
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
        tabs: Vec<crate::terminal_layout::TerminalTabSummary>,
        top_panes: Vec<crate::terminal_layout::TerminalPaneSummary>,
        top_ratios: Vec<f64>,
        top_grid: crate::terminal_layout::TerminalTopGrid,
        split_tree: Option<crate::terminal_layout::TerminalSplitNode>,
        bottom_ratio: f64,
        collapsed_panes: Vec<crate::terminal_layout::TerminalPaneSummary>,
    ) -> Result<TerminalLayoutSummary, String> {
        TerminalLayoutService::new(self.support_dir.clone()).save_from_gpui_with_grid(
            project_id,
            tabs,
            top_panes,
            top_ratios,
            top_grid,
            split_tree,
            bottom_ratio,
            collapsed_panes,
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
}
