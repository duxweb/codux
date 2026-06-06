impl RuntimeState {
    pub fn load() -> Self {
        Self::load_from_support_dir(app_support_dir())
    }

    pub fn load_from_support_dir(support_dir: PathBuf) -> Self {
        let (projects, selected_project) = load_projects(&support_dir);
        let settings = load_settings(&support_dir);
        let selected_path = selected_project
            .as_ref()
            .map(|project| project.path.as_str());
        let git = selected_path
            .map(|path| load_git_summary(&support_dir, path))
            .unwrap_or_default();
        let git_review = selected_path
            .map(|path| load_git_review(&support_dir, path, None))
            .unwrap_or_default();
        let files = selected_path
            .map(|path| load_file_entries(path, None))
            .unwrap_or_default();
        let ai_global_history = load_global_ai_history(&support_dir);
        let daily_level = build_daily_level(&ai_global_history);
        let ai_history = selected_path
            .map(|path| load_ai_history(&support_dir, path))
            .unwrap_or_default();
        let ai_session_detail = ai_history.sessions.first().and_then(|session| {
            selected_path.map(|path| load_ai_session_detail(&support_dir, path, &session.id))
        });
        let memory = load_memory(
            &support_dir,
            selected_project.as_ref().map(|project| project.id.as_str()),
        );
        let memory_manager = load_memory_manager(
            &support_dir,
            &projects,
            "project",
            selected_project.as_ref().map(|project| project.id.as_str()),
            "active",
        );
        let notifications = load_notifications(&support_dir);
        let ssh = load_ssh(&support_dir, RuntimeInventory::load().root);
        let worktrees = load_worktrees_from_state(
            &support_dir,
            selected_project.as_ref().map(|project| project.id.as_str()),
            selected_project
                .as_ref()
                .map(|project| project.path.as_str()),
        );
        let terminal_layout_owner = selected_project.as_ref().map(|project| {
            crate::terminal_layout::terminal_layout_storage_key(
                &project.id,
                worktrees
                    .selected_worktree_id
                    .as_deref()
                    .unwrap_or(project.id.as_str()),
            )
        });
        let terminal_layout = load_terminal_layout(&support_dir, terminal_layout_owner.as_deref());
        let terminal_runtime = TerminalRuntimeSummary::default();
        let update = load_update(&support_dir, std::env::current_dir().unwrap_or_default());
        let runtime_activity = load_runtime_activity(&support_dir);
        let runtime_events = load_runtime_events();
        let ai_runtime_state = load_ai_runtime_state(&support_dir, &runtime_events);
        let ai_runtime_session_scope_id =
            selected_ai_runtime_session_scope_id(selected_project.as_ref(), &worktrees);
        let ai_history_stats = build_ai_history_stats(
            &ai_history,
            &ai_runtime_state,
            ai_runtime_session_scope_id.as_deref(),
            &settings.statistics_mode,
        );
        let remote = load_remote(&support_dir);
        let pet = load_pet(&support_dir);
        let power = PowerService::new().summary(&settings.sleep_mode);
        let performance = load_performance();
        let tool_permissions = load_tool_permissions(&support_dir);

        Self {
            support_dir,
            settings,
            projects,
            selected_project,
            git,
            git_review,
            files,
            ai_global_history,
            daily_level,
            ai_history,
            ai_history_stats,
            ai_session_detail,
            memory,
            memory_manager,
            notifications,
            ssh,
            worktrees,
            terminal_layout,
            terminal_runtime,
            update,
            runtime_activity,
            runtime_events,
            ai_runtime_state,
            remote,
            pet,
            power,
            performance,
            tool_permissions,
        }
    }

    pub fn select_project(&mut self, project_id: &str) {
        let Some(project) = self
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
        else {
            return;
        };

        self.selected_project = Some(project.clone());
        self.git = load_git_summary(&self.support_dir, &project.path);
        self.git_review = load_git_review(&self.support_dir, &project.path, None);
        self.files = load_file_entries(&project.path, None);
        self.ai_global_history = load_global_ai_history(&self.support_dir);
        self.refresh_daily_level();
        self.ai_history = load_ai_history(&self.support_dir, &project.path);
        self.ai_session_detail =
            self.ai_history.sessions.first().map(|session| {
                load_ai_session_detail(&self.support_dir, &project.path, &session.id)
            });
        self.memory = load_memory(&self.support_dir, Some(&project.id));
        self.memory_manager = load_memory_manager(
            &self.support_dir,
            &self.projects,
            "project",
            Some(&project.id),
            "active",
        );
        self.notifications = load_notifications(&self.support_dir);
        self.worktrees =
            load_worktrees_from_state(&self.support_dir, Some(&project.id), Some(&project.path));
        let terminal_layout_owner = crate::terminal_layout::terminal_layout_storage_key(
            &project.id,
            self.worktrees
                .selected_worktree_id
                .as_deref()
                .unwrap_or(project.id.as_str()),
        );
        self.terminal_layout = load_terminal_layout(&self.support_dir, Some(&terminal_layout_owner));
        self.terminal_runtime = TerminalRuntimeSummary::default();
        self.runtime_activity = load_runtime_activity(&self.support_dir);
        self.runtime_events = load_runtime_events();
        self.ai_runtime_state = load_ai_runtime_state(&self.support_dir, &self.runtime_events);
        self.refresh_ai_history_stats();
        self.pet = load_pet(&self.support_dir);
        self.power = PowerService::new().summary(&self.settings.sleep_mode);
        self.performance = load_performance();
        self.tool_permissions = load_tool_permissions(&self.support_dir);
    }

    pub fn refresh_ai_history_stats(&mut self) {
        let ai_runtime_session_scope_id =
            selected_ai_runtime_session_scope_id(self.selected_project.as_ref(), &self.worktrees);
        self.ai_history_stats = build_ai_history_stats(
            &self.ai_history,
            &self.ai_runtime_state,
            ai_runtime_session_scope_id.as_deref(),
            &self.settings.statistics_mode,
        );
    }

    pub fn refresh_daily_level(&mut self) {
        self.daily_level = build_daily_level(&self.ai_global_history);
    }
}

fn build_daily_level(global_history: &AIGlobalHistorySummary) -> AIHistoryDailyLevelView {
    crate::ai_history::daily_level_view(global_history.today_total_tokens)
}

fn build_ai_history_stats(
    history: &AIHistorySummary,
    ai_runtime_state: &AIRuntimeStateSummary,
    selected_scope_id: Option<&str>,
    statistics_mode: &str,
) -> AIHistoryStatsView {
    crate::ai_history::stats_view(
        history,
        ai_runtime_state,
        selected_scope_id,
        statistics_mode,
        crate::ai_history_normalized::now_seconds(),
    )
}

fn selected_ai_runtime_session_scope_id(
    selected_project: Option<&ProjectInfo>,
    worktrees: &crate::worktree::WorktreeSummary,
) -> Option<String> {
    worktrees
        .selected_worktree_id
        .clone()
        .or_else(|| selected_project.map(|project| project.id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_runtime_session_scope_prefers_selected_worktree() {
        let project = ProjectInfo {
            id: "project-a".to_string(),
            name: "Project A".to_string(),
            path: "/tmp/project-a".to_string(),
            exists: true,
            badge: "PA".to_string(),
            badge_symbol: None,
            badge_color_hex: None,
            git_default_push_remote_name: None,
        };
        let worktrees = crate::worktree::WorktreeSummary {
            selected_worktree_id: Some("worktree-b".to_string()),
            ..Default::default()
        };

        assert_eq!(
            selected_ai_runtime_session_scope_id(Some(&project), &worktrees).as_deref(),
            Some("worktree-b")
        );
    }

    #[test]
    fn ai_runtime_session_scope_falls_back_to_project() {
        let project = ProjectInfo {
            id: "project-a".to_string(),
            name: "Project A".to_string(),
            path: "/tmp/project-a".to_string(),
            exists: true,
            badge: "PA".to_string(),
            badge_symbol: None,
            badge_color_hex: None,
            git_default_push_remote_name: None,
        };
        let worktrees = crate::worktree::WorktreeSummary::default();

        assert_eq!(
            selected_ai_runtime_session_scope_id(Some(&project), &worktrees).as_deref(),
            Some("project-a")
        );
    }
}
