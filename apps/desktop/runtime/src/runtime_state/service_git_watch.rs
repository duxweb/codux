impl RuntimeService {
    pub fn watch_project_git(
        &self,
        project_path: String,
        on_change: impl Fn(git::GitRepositoryChangeEvent) + Send + Sync + 'static,
    ) -> Result<git::GitWatchRegistration, String> {
        self.git_watch_manager.watch(project_path, on_change)
    }

    pub fn unwatch_project_git(&self, project_path: String) -> Result<(), String> {
        self.git_watch_manager.unwatch(project_path)
    }

    pub fn git_watch(&self, project_path: String) -> Result<git::GitWatchRegistration, String> {
        let activity = Arc::clone(&self.project_activity);
        let support_dir = self.support_dir.clone();
        self.watch_project_git(project_path, move |event| {
            let project_store = ProjectStore::new(support_dir.clone());
            activity.refresh_git_changed(
                &project_store,
                event.project_path,
                event.repository_path,
                event.changed_paths,
            );
        })
    }

    pub fn git_unwatch(&self, project_path: String) -> Result<(), String> {
        self.unwatch_project_git(project_path)
    }

    fn watch_active_project_git(
        &self,
        project_path: String,
        generation: u64,
    ) -> Result<Option<git::GitWatchRegistration>, String> {
        let registration = self.git_watch(project_path)?;
        let previous = self
            .active_project_watches
            .lock()
            .map_err(|_| "Active project watcher lock is poisoned.".to_string())?
            .then_install_git(generation, registration.project_path.clone());
        let Some(previous) = previous else {
            let _ = self.git_unwatch(registration.project_path.clone());
            return Ok(None);
        };
        if let Some(previous) = previous.filter(|path| path != &registration.project_path) {
            let _ = self.git_unwatch(previous);
        }
        Ok(Some(registration))
    }
    pub fn reload_project_git(&self, project_path: &str) -> git::GitSummary {
        if let Some(runtime) = self.hosted_runtime_for_project_path(project_path) {
            let project_id = self
                .project_id_for_workspace_path(project_path)
                .unwrap_or_default();
            return match runtime
                .and_then(|runtime| runtime.git_status(&project_id, project_path))
                .and_then(|value| git_summary_from_payload(&value))
            {
                Ok(summary) => summary,
                Err(error) => git::GitSummary {
                    error: Some(error),
                    ..Default::default()
                },
            };
        }
        refresh_git_summary(&self.support_dir, project_path)
    }

    pub fn stored_project_git_state(
        &self,
        project_path: &str,
        base_branch: Option<&str>,
    ) -> (git::GitSummary, git::GitReviewSummary) {
        // Remote projects have no local cache; fetch live status from the host
        // (review diff isn't cached remotely — the panel refreshes it on demand).
        if self
            .hosted_runtime_for_project_path(project_path)
            .is_some()
        {
            return (
                self.reload_project_git(project_path),
                git::GitReviewSummary::default(),
            );
        }
        (
            crate::runtime_cache::cached_git_summary(&self.support_dir, project_path)
                .unwrap_or_default(),
            crate::runtime_cache::cached_git_review(&self.support_dir, project_path, base_branch)
                .unwrap_or_default(),
        )
    }

    pub fn reload_project_git_review(
        &self,
        project_path: &str,
        base_branch: Option<&str>,
    ) -> git::GitReviewSummary {
        if let Some(result) = self.hosted_git_read(
            project_path,
            "review",
            serde_json::json!({ "baseBranch": base_branch }),
        ) {
            return result
                .and_then(|value| serde_json::from_value(value).map_err(|error| error.to_string()))
                .unwrap_or_else(|error| git::GitReviewSummary {
                    mode: "workingTreeAudit".to_string(),
                    title: "Uncommitted Audit".to_string(),
                    base_branch: base_branch.map(str::to_string),
                    error: Some(error),
                    ..Default::default()
                });
        }
        refresh_git_review(&self.support_dir, project_path, base_branch)
    }
}
