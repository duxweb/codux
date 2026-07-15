impl RuntimeService {
    pub fn reload_state(&self) -> RuntimeState {
        RuntimeState::load_from_support_dir(self.support_dir.clone())
    }

    pub fn reload_settings(&self) -> SettingsSummary {
        SettingsService::new(self.support_dir.clone()).summary()
    }

    pub fn runtime_trace_frontend(&self, category: &str, message: &str) {
        crate::runtime_trace::runtime_trace(category, message);
    }

    pub fn ai_runtime_probe(
        &self,
        request: AIRuntimeProbeRequest,
    ) -> Option<AIRuntimeContextSnapshot> {
        crate::ai_runtime::probe_runtime(&request)
    }

    pub fn app_runtime_ready(&self, visible: bool, focused: bool) -> AppRuntimeReadySnapshot {
        let started_at = std::time::Instant::now();
        let project_store = ProjectStore::new(self.support_dir.clone());
        let projects = project_store.list_snapshot();
        let selected_project_id = projects
            .selected_project_id
            .as_deref()
            .unwrap_or("none")
            .to_string();

        crate::runtime_trace::runtime_trace(
            "startup",
            &format!(
                "app_runtime_ready start projects={} selected={selected_project_id}",
                projects.projects.len()
            ),
        );

        self.project_activity
            .seed_projects(project_store.projects_snapshot());
        self.project_activity.mark_main_window_visible(visible);
        self.project_activity.mark_main_window_focused(focused);

        if let Some(project) = projects
            .selected_project_id
            .as_ref()
            .and_then(|id| projects.projects.iter().find(|project| &project.id == id))
            .cloned()
        {
            let active_workspace_path = project_store
                .active_workspace_path_for_project(&project.id)
                .unwrap_or_else(|| project.path.clone());
            self.project_activity.mark_project_active(project.clone());
            self.watch_project_background(
                active_workspace_path,
                project.runtime_target,
            );
            self.refresh_active_ai_history_background();
        }

        let ai_runtime_state = self.ai_runtime.runtime_state_snapshot();
        let project_activity = self.project_activity.snapshot();
        let settings = SettingsService::new(self.support_dir.clone()).summary();
        let window_state = RuntimeWindowStateSnapshot {
            project_activity: project_activity.clone(),
            shows_dock_badge: settings.shows_dock_badge,
            attention_count: runtime_attention_count(&ai_runtime_state),
            dock_badge_count: runtime_dock_badge_count(
                settings.shows_dock_badge,
                &ai_runtime_state,
            ),
        };

        let snapshot = AppRuntimeReadySnapshot {
            projects,
            terminal_layouts: project_store.terminal_layouts_snapshot(),
            remote: self.remote_host.start(),
            ai_runtime_state,
            project_activity,
            window_state,
        };

        crate::runtime_trace::runtime_trace_elapsed(
            "startup",
            "app_runtime_ready finish",
            started_at,
            &format!(
                "projects={} selected={selected_project_id}",
                snapshot.projects.projects.len()
            ),
        );

        snapshot
    }
    pub fn mark_main_window_state(&self, visible: bool, focused: bool) -> ProjectActivitySnapshot {
        self.project_activity.mark_main_window_visible(visible);
        self.project_activity.mark_main_window_focused(focused);
        self.project_activity.snapshot()
    }

    pub fn app_window_state(&self, visible: bool, focused: bool) -> RuntimeWindowStateSnapshot {
        self.project_activity.mark_main_window_visible(visible);
        self.project_activity.mark_main_window_focused(focused);
        let settings = SettingsService::new(self.support_dir.clone()).summary();
        let ai_runtime_state = self.ai_runtime.runtime_state_snapshot();
        RuntimeWindowStateSnapshot {
            project_activity: self.project_activity.snapshot(),
            shows_dock_badge: settings.shows_dock_badge,
            attention_count: runtime_attention_count(&ai_runtime_state),
            dock_badge_count: runtime_dock_badge_count(
                settings.shows_dock_badge,
                &ai_runtime_state,
            ),
        }
    }

    pub fn ai_runtime_dock_badge_count(&self) -> Option<i64> {
        let settings = SettingsService::new(self.support_dir.clone()).summary();
        let ai_runtime_state = self.ai_runtime.runtime_state_snapshot();
        runtime_dock_badge_count(settings.shows_dock_badge, &ai_runtime_state)
    }
    pub fn app_milestones(&self) -> crate::app_milestones::AppMilestones {
        crate::app_milestones::load_or_seed(&self.support_dir)
    }

    pub fn mark_star_prompt_shown(&self) -> crate::app_milestones::AppMilestones {
        crate::app_milestones::mark_star_prompt_shown(&self.support_dir)
    }
}

fn runtime_attention_count(snapshot: &AIRuntimeStateSnapshot) -> usize {
    snapshot.needs_input_count + snapshot.completion_count
}

fn runtime_dock_badge_count(
    shows_dock_badge: bool,
    snapshot: &AIRuntimeStateSnapshot,
) -> Option<i64> {
    let attention_count = runtime_attention_count(snapshot);
    if shows_dock_badge && attention_count > 0 {
        Some(attention_count as i64)
    } else {
        None
    }
}
