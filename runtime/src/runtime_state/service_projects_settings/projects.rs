impl RuntimeService {
    pub fn project_open_applications(
        &self,
    ) -> Vec<crate::project_open::ProjectOpenApplicationSummary> {
        crate::project_open::project_open_applications()
    }

    pub fn project_open_in_application(
        &self,
        project_path: String,
        application_id: String,
    ) -> Result<(), String> {
        crate::project_open::project_open_in_application(
            crate::project_open::ProjectOpenApplicationRequest {
                project_path,
                application_id,
            },
        )
    }

    pub fn project_reveal_in_file_manager(&self, project_path: &str) -> Result<(), String> {
        crate::project_open::project_reveal_in_file_manager(project_path)
    }

    pub fn project_list(&self) -> ProjectListSnapshot {
        ProjectStore::new(self.support_dir.clone()).list_snapshot()
    }

    pub fn project_create(
        &self,
        request: ProjectCreateRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let snapshot = ProjectStore::new(self.support_dir.clone()).create_project(request)?;
        if let Some(project_id) = snapshot.selected_project_id.as_deref() {
            let _ = self.mark_project_active_with_watch(project_id);
        }
        Ok(snapshot)
    }

    pub fn project_update(
        &self,
        request: ProjectUpdateRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let snapshot =
            ProjectStore::new(self.support_dir.clone()).update_project_from_request(request)?;
        if let Some(project_id) = snapshot.selected_project_id.as_deref() {
            let _ = self.mark_project_active_with_watch(project_id);
        }
        Ok(snapshot)
    }

    pub fn project_reorder(
        &self,
        request: ProjectReorderRequest,
    ) -> Result<ProjectListSnapshot, String> {
        ProjectStore::new(self.support_dir.clone()).reorder_projects(request)
    }

    pub fn project_close(
        &self,
        request: ProjectCloseRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let project_id = request.project_id.clone();
        let workspace_ids = self.project_workspace_ids_for_root(&project_id);
        let snapshot =
            ProjectStore::new(self.support_dir.clone()).close_project_snapshot(project_id.clone())?;
        self.cleanup_project_workspace_data(&workspace_ids);
        self.project_activity.remove_project(&project_id);
        if let Some(next_project_id) = snapshot.selected_project_id.as_deref() {
            let _ = self.mark_project_active_with_watch(next_project_id);
        } else {
            self.stop_active_project_files();
        }
        Ok(snapshot)
    }

    pub fn project_close_all(&self) -> Result<ProjectListSnapshot, String> {
        let workspace_ids = self.all_project_workspace_ids();
        let snapshot = ProjectStore::new(self.support_dir.clone()).close_all_projects()?;
        self.cleanup_project_workspace_data(&workspace_ids);
        let _ = self.forget_all_pet_project_baselines();
        self.project_activity.clear();
        self.stop_active_project_files();
        Ok(snapshot)
    }

    pub fn project_select_worktree(
        &self,
        request: ProjectSelectWorktreeRequest,
    ) -> Result<(), String> {
        let store = ProjectStore::new(self.support_dir.clone());
        let worktree_id = request.worktree_id.clone();
        store.select_worktree(request)?;
        let project_id = store
            .root_project_summary_for_workspace_id(&worktree_id)
            .map(|project| project.id)
            .or_else(|| store.list_snapshot().selected_project_id);
        if let Some(project_id) = project_id {
            let _ = self.mark_project_active_with_watch(&project_id);
        }
        Ok(())
    }

    pub fn project_set_default_push_remote(
        &self,
        request: ProjectDefaultPushRemoteRequest,
    ) -> Result<ProjectListSnapshot, String> {
        ProjectStore::new(self.support_dir.clone()).set_default_push_remote(request)
    }

    pub fn terminal_layout_record(&self, project_id: &str) -> Option<TerminalLayoutRecord> {
        ProjectStore::new(self.support_dir.clone()).terminal_layout(project_id)
    }

    pub fn terminal_layout_records(&self) -> TerminalLayoutsSnapshot {
        ProjectStore::new(self.support_dir.clone()).terminal_layouts_snapshot()
    }

    pub fn save_terminal_layout_record(
        &self,
        project_id: String,
        layout: TerminalLayoutRecord,
    ) -> Result<TerminalLayoutRecord, String> {
        ProjectStore::new(self.support_dir.clone()).save_terminal_layout(project_id, layout)
    }

    pub fn select_project(&self, project_id: &str) -> Result<(), String> {
        ProjectStore::new(self.support_dir.clone()).select_project(project_id)?;
        let project = ProjectStore::new(self.support_dir.clone())
            .project_summaries()
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        self.project_activity.mark_project_active(project.clone());
        let service = self.clone();
        let _ = std::thread::Builder::new()
            .name("codux-project-watch-switch".to_string())
            .spawn(move || {
                let _ = service.git_watch(project.path.clone());
                let _ = service.watch_active_project_files(project.path);
            });
        Ok(())
    }

    pub fn create_or_select_project(&self, name: &str, path: &str) -> Result<String, String> {
        let project_id = ProjectStore::new(self.support_dir.clone()).create_or_select_project(name, path)?;
        self.mark_project_active_with_watch(&project_id)?;
        Ok(project_id)
    }

    pub fn update_project(&self, project_id: &str, name: &str, path: &str) -> Result<(), String> {
        ProjectStore::new(self.support_dir.clone()).update_project(project_id, name, path)?;
        self.mark_project_active_with_watch(project_id)?;
        Ok(())
    }

    pub fn move_project_up(&self, project_id: &str) -> Result<(), String> {
        ProjectStore::new(self.support_dir.clone())
            .move_project(project_id, ProjectMoveDirection::Up)
    }

    pub fn move_project_down(&self, project_id: &str) -> Result<(), String> {
        ProjectStore::new(self.support_dir.clone())
            .move_project(project_id, ProjectMoveDirection::Down)
    }

    pub fn close_project(&self, project_id: &str) -> Result<Option<String>, String> {
        let workspace_ids = self.project_workspace_ids_for_root(project_id);
        let next_project_id = ProjectStore::new(self.support_dir.clone()).close_project(project_id)?;
        self.cleanup_project_workspace_data(&workspace_ids);
        self.project_activity.remove_project(project_id);
        if let Some(next_project_id) = next_project_id.as_deref() {
            let _ = self.mark_project_active_with_watch(next_project_id);
        } else {
            self.stop_active_project_files();
        }
        Ok(next_project_id)
    }

    pub fn read_project_file_edit_buffer(
        &self,
        project_path: &str,
        relative_path: &str,
    ) -> Result<(String, bool), String> {
        let result = FilesService::read_text(project_path, relative_path)?;
        if let Some(message) = result.message {
            return Ok((message, false));
        }
        Ok((
            result.content,
            !result.is_binary && !result.is_large && !result.is_truncated,
        ))
    }

    fn project_workspace_ids_for_root(&self, project_id: &str) -> Vec<String> {
        ProjectStore::new(self.support_dir.clone())
            .project_workspaces_snapshot()
            .into_iter()
            .filter(|workspace| workspace.root_project_id == project_id)
            .map(|workspace| workspace.id)
            .collect()
    }

    fn all_project_workspace_ids(&self) -> Vec<String> {
        ProjectStore::new(self.support_dir.clone())
            .project_workspaces_snapshot()
            .into_iter()
            .map(|workspace| workspace.id)
            .collect()
    }

    fn cleanup_project_workspace_data(&self, workspace_ids: &[String]) {
        if workspace_ids.is_empty() {
            return;
        }

        let terminal_layout = TerminalLayoutService::new(self.support_dir.clone());
        let file_editor_layout = FileEditorLayoutService::new(self.support_dir.clone());
        let file_tree_state = FileTreeStateService::new(self.support_dir.clone());
        let git_ui_state = GitUiStateService::new(self.support_dir.clone());

        for workspace_id in workspace_ids {
            let _ = self.forget_pet_project_baseline(workspace_id);
            let _ = terminal_layout.delete(workspace_id);
            let _ = file_editor_layout.delete(workspace_id);
            let _ = file_tree_state.delete(workspace_id);
            let _ = git_ui_state.delete(workspace_id);
        }
    }
}
