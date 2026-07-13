impl RuntimeService {
    pub fn project_activity_snapshot(&self) -> ProjectActivitySnapshot {
        self.project_activity.snapshot()
    }
    pub fn mark_project_active(&self, project_id: &str) -> Result<ProjectActivitySnapshot, String> {
        let project = ProjectStore::new(self.support_dir.clone())
            .project_summaries()
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        self.project_activity.mark_project_active(project);
        Ok(self.project_activity.snapshot())
    }

    pub fn mark_project_active_with_watch(
        &self,
        project_id: &str,
    ) -> Result<ProjectActivitySnapshot, String> {
        let store = ProjectStore::new(self.support_dir.clone());
        let project = store
            .project_summaries()
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        let active_workspace_path = store
            .active_workspace_path_for_project(project_id)
            .unwrap_or_else(|| project.path.clone());
        self.project_activity.mark_project_active(project.clone());
        self.watch_project_background(
            active_workspace_path,
            project.path,
            project.runtime_target,
        );
        self.refresh_active_ai_history_background();

        Ok(self.project_activity.snapshot())
    }

    fn refresh_active_ai_history_background(&self) {
        let Some(request) = self.active_ai_history_project_request() else {
            return;
        };
        let activation_key = format!("{}:{}", request.id, request.path);
        let should_refresh = self
            .ai_history_activation_keys
            .lock()
            .map(|mut keys| keys.insert(activation_key))
            .unwrap_or(false);
        if !should_refresh {
            return;
        }
        let ai_history = self.ai_history_indexer.clone();
        let _ = std::thread::Builder::new()
            .name("codux-ai-history-activation".to_string())
            .spawn(move || {
                let _ = ai_history.refresh_project(request);
            });
    }

    fn active_ai_history_project_request(&self) -> Option<AIHistoryProjectRequest> {
        let store = ProjectStore::new(self.support_dir.clone());
        let snapshot = store.snapshot();
        let project = snapshot
            .selected_project_id
            .as_ref()
            .and_then(|id| snapshot.projects.iter().find(|project| &project.id == id))
            .or_else(|| snapshot.projects.first())?;
        let selected_worktree_id = snapshot
            .selected_worktree_id_by_project
            .get(&project.id)
            .map(String::as_str)
            .unwrap_or(project.id.as_str());
        if selected_worktree_id != project.id
            && let Some(worktree) = snapshot
                .worktrees
                .iter()
                .find(|worktree| worktree.id == selected_worktree_id)
        {
            return Some(AIHistoryProjectRequest {
                id: worktree.id.clone(),
                name: worktree.name.clone(),
                path: worktree.path.clone(),
            });
        }
        Some(AIHistoryProjectRequest {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
        })
    }

    pub fn watch_project_background(
        &self,
        file_watch_path: String,
        git_watch_path: String,
        runtime_target: ProjectRuntimeTarget,
    ) {
        let Ok(generation) = self.begin_project_watch_switch() else {
            return;
        };
        let hosted = runtime_target.is_hosted();
        let service = self.clone();
        drop(crate::async_runtime::spawn_blocking(move || {
            let Ok(_registration) = service.project_watch_registration.lock() else {
                return;
            };
            service.drain_pending_project_watch_cleanup();
            if hosted || !service.project_watch_generation_is_current(generation) {
                return;
            }
            if let Err(error) = service.watch_active_project_files(file_watch_path, generation) {
                crate::runtime_trace::runtime_trace(
                    "files",
                    &format!("failed to watch active project: {error}"),
                );
                return;
            }
            if !service.project_watch_generation_is_current(generation) {
                return;
            }
            if let Err(error) = service.watch_active_project_git(git_watch_path, generation) {
                crate::runtime_trace::runtime_trace(
                    "git",
                    &format!("failed to watch active project: {error}"),
                );
            }
        }));
    }

    pub fn tick_project_activity(&self) -> ProjectActivitySnapshot {
        let settings = SettingsService::new(self.support_dir.clone()).summary();
        self.project_activity.run_tick(&settings);
        self.project_activity.snapshot()
    }

    pub fn drain_project_activity_events(&self) -> Vec<ProjectActivityEvent> {
        self.project_activity.drain_events()
    }
}
