impl ProjectActivityCoordinator {
    pub fn seed_projects(&self, projects: Vec<ProjectRecord>) {
        if let Ok(mut guard) = self.projects.lock() {
            for project in projects {
                upsert_project(&mut guard, project.id, project.name, project.path);
            }
            for project in guard.values_mut() {
                project.last_git_refresh = Some(Instant::now());
                project.last_ai_refresh = Some(Instant::now());
            }
        }
        runtime_trace(
            "startup",
            "project activity seeded with deferred background refresh",
        );
    }

    pub fn mark_project_summary(&self, project: &ProjectSummary) -> bool {
        self.projects
            .lock()
            .map(|mut guard| {
                upsert_project(
                    &mut guard,
                    project.id.clone(),
                    project.name.clone(),
                    project.path.clone(),
                )
            })
            .unwrap_or(false)
    }

    pub fn mark_project_active(&self, project: ProjectSummary) {
        self.mark_project_summary(&project);
        let mut should_refresh_sidecars = true;
        if let Ok(mut active) = self.active_project_id.lock() {
            let is_same_active = active.as_deref() == Some(project.id.as_str());
            *active = Some(project.id.clone());
            if is_same_active
                && self
                    .activated_git_projects
                    .lock()
                    .map(|activated| activated.contains(&project.id))
                    .unwrap_or(false)
            {
                return;
            }
            should_refresh_sidecars = is_same_active;
        }
        let is_first_git_activation = self.mark_git_activation(&project.id);
        if should_refresh_sidecars || is_first_git_activation {
            self.refresh_git_sidecars_by_path(project.clone());
        }
        if is_first_git_activation {
            self.refresh_git_once(&project);
        }
    }

    pub fn mark_main_window_visible(&self, visible: bool) {
        self.main_window_visible.store(visible, Ordering::Relaxed);
    }

    pub fn mark_main_window_focused(&self, focused: bool) {
        self.main_window_focused.store(focused, Ordering::Relaxed);
    }

    pub fn drain_events(&self) -> Vec<ProjectActivityEvent> {
        self.events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn snapshot(&self) -> ProjectActivitySnapshot {
        let tracked_projects = self
            .projects
            .lock()
            .map(|projects| {
                projects
                    .values()
                    .map(|project| ProjectActivityTrackedProject {
                        id: project.id.clone(),
                        name: project.name.clone(),
                        path: project.path.clone(),
                        has_git_refresh: project.last_git_refresh.is_some(),
                        has_ai_refresh: project.last_ai_refresh.is_some(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        ProjectActivitySnapshot {
            tracked_count: tracked_projects.len(),
            active_project_id: self.active_project_id.lock().ok().and_then(|id| id.clone()),
            visible: self.main_window_visible.load(Ordering::Relaxed),
            focused: self.main_window_focused.load(Ordering::Relaxed),
            activated_git_count: self
                .activated_git_projects
                .lock()
                .map(|ids| ids.len())
                .unwrap_or_default(),
            activated_ai_count: self
                .activated_ai_projects
                .lock()
                .map(|ids| ids.len())
                .unwrap_or_default(),
            queued_activation_count: 0,
            tracked_projects,
        }
    }

    pub fn remove_project(&self, project_id: &str) {
        if let Ok(mut guard) = self.projects.lock() {
            guard.remove(project_id);
        }
        if let Ok(mut activated) = self.activated_git_projects.lock() {
            activated.remove(project_id);
        }
        if let Ok(mut activated) = self.activated_ai_projects.lock() {
            activated.remove(project_id);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.projects.lock() {
            guard.clear();
        }
        if let Ok(mut active) = self.active_project_id.lock() {
            *active = None;
        }
        if let Ok(mut activated) = self.activated_git_projects.lock() {
            activated.clear();
        }
        if let Ok(mut activated) = self.activated_ai_projects.lock() {
            activated.clear();
        }
    }

    fn mark_git_activation(&self, project_id: &str) -> bool {
        self.activated_git_projects
            .lock()
            .map(|mut activated| activated.insert(project_id.to_string()))
            .unwrap_or(false)
    }
}
