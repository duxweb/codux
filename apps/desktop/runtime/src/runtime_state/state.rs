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
        let git_workspace = selected_path
            .map(|path| load_git_workspace(&support_dir, path))
            .unwrap_or_default();
        let git = git_workspace.status;
        let git_review = git_workspace.review;
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
        let runtime_root = RuntimeInventory::load().root;
        let ssh = load_ssh(&support_dir, runtime_root.clone());
        let db = load_db(
            &support_dir,
            runtime_root,
            selected_project.as_ref().map(|project| project.id.as_str()),
        );
        let worktrees = load_worktrees_from_state(
            &support_dir,
            selected_project.as_ref().map(|project| project.id.as_str()),
            selected_project
                .as_ref()
                .map(|project| project.path.as_str()),
            selected_project
                .as_ref()
                .is_some_and(|project| project.runtime_target.is_hosted()),
        );
        let terminal_layout_owner = runtime_model_scope_key(selected_project.as_ref(), &worktrees);
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
            ai_history_stats_fingerprint: 0,
            ai_session_detail,
            memory,
            memory_manager,
            notifications,
            ssh,
            db,
            worktrees,
            terminal_layout,
            terminal_runtime,
            update,
            runtime_activity,
            runtime_events,
            ai_runtime_state,
            remote_ai_current_sessions: Vec::new(),
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
        let git_workspace = load_git_workspace(&self.support_dir, &project.path);
        self.git = git_workspace.status;
        self.git_review = git_workspace.review;
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
        self.db = load_db(
            &self.support_dir,
            RuntimeInventory::load().root,
            Some(&project.id),
        );
        self.worktrees = load_worktrees_from_state(
            &self.support_dir,
            Some(&project.id),
            Some(&project.path),
            project.runtime_target.is_hosted(),
        );
        let terminal_layout_owner =
            runtime_model_scope_key(self.selected_project.as_ref(), &self.worktrees);
        self.terminal_layout =
            load_terminal_layout(&self.support_dir, terminal_layout_owner.as_deref());
        self.terminal_runtime = TerminalRuntimeSummary::default();
        self.runtime_activity = load_runtime_activity(&self.support_dir);
        self.runtime_events = load_runtime_events();
        self.ai_runtime_state = load_ai_runtime_state(&self.support_dir, &self.runtime_events);
        self.remote_ai_current_sessions.clear();
        self.refresh_ai_history_stats();
        self.pet = load_pet(&self.support_dir);
        self.power = PowerService::new().summary(&self.settings.sleep_mode);
        self.performance = load_performance();
        self.tool_permissions = load_tool_permissions(&self.support_dir);
    }

    pub fn refresh_ai_history_stats(&mut self) {
        let ai_runtime_session_scope_id =
            selected_ai_runtime_session_scope_id(self.selected_project.as_ref(), &self.worktrees);
        // The history-derived geometry (today buckets, heatmap, tool/model rows,
        // totals) only changes when the indexed history, the cache mode, or the
        // local day changes — none of which move during an active turn. Only the
        // live current-session rows change per tick, so when the geometry
        // fingerprint matches we recompute just those and leave the rest in place
        // instead of rebuilding 48 buckets + 140 heatmap cells + two sorts.
        let fingerprint =
            ai_history_geometry_fingerprint(&self.ai_history, &self.settings.statistics_mode);
        if fingerprint == self.ai_history_stats_fingerprint {
            self.ai_history_stats.current_sessions = crate::ai_history::current_sessions_view(
                &self.ai_runtime_state,
                ai_runtime_session_scope_id.as_deref(),
                &self.settings.statistics_mode,
            );
        } else {
            self.ai_history_stats = build_ai_history_stats(
                &self.ai_history,
                &self.ai_runtime_state,
                ai_runtime_session_scope_id.as_deref(),
                &self.settings.statistics_mode,
            );
            self.ai_history_stats_fingerprint = fingerprint;
        }
        if self
            .selected_project
            .as_ref()
            .and_then(ProjectInfo::remote_device_id)
            .is_some()
        {
            self.ai_history_stats.current_sessions = self.remote_ai_current_sessions.clone();
        }
    }

    pub fn refresh_daily_level(&mut self) {
        self.daily_level = build_daily_level(&self.ai_global_history);
    }
}

fn build_daily_level(global_history: &AIGlobalHistorySummary) -> AIHistoryDailyLevelView {
    crate::ai_history::daily_level_view(global_history.today_total_tokens)
}

fn ai_history_geometry_fingerprint(history: &AIHistorySummary, statistics_mode: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (statistics_mode.trim() == "includingCache").hash(&mut hasher);
    // Geometry pivots on the local day boundary (buckets/heatmap), not the exact
    // second, so a day-granular clock keeps the fingerprint stable within a day.
    let now = crate::ai_history_normalized::now_seconds();
    (crate::ai_history_normalized::local_day_start_seconds(now) as i64).hash(&mut hasher);
    history.project_total_tokens.hash(&mut hasher);
    history.project_cached_input_tokens.hash(&mut hasher);
    history.today_total_tokens.hash(&mut hasher);
    history.today_cached_input_tokens.hash(&mut hasher);
    history.today_time_buckets.len().hash(&mut hasher);
    history.heatmap.len().hash(&mut hasher);
    history.tool_breakdown.len().hash(&mut hasher);
    history.model_breakdown.len().hash(&mut hasher);
    hasher.finish()
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

fn runtime_model_scope_key(
    selected_project: Option<&ProjectInfo>,
    worktrees: &crate::worktree::WorktreeSummary,
) -> Option<String> {
    let project = selected_project?;
    let mut runtime = RuntimeModel::new();
    runtime.apply_project_list(
        vec![RuntimeProject {
            id: project.id.clone(),
            name: project.name.clone(),
            path: Some(project.path.clone()),
        }],
        Some(project.id.clone()),
        worktrees.selected_worktree_id.clone(),
        false,
        true,
    );
    runtime.apply_worktree_state(
        RuntimeWorktreeState {
            project_id: Some(project.id.clone()),
            selected_worktree_id: worktrees.selected_worktree_id.clone(),
            worktrees: worktrees
                .worktrees
                .iter()
                .map(runtime_worktree_from_desktop)
                .collect(),
            base_branches: Vec::new(),
            default_base_branch: None,
        },
        false,
        false,
        true,
    );
    runtime
        .selected_scope_key()
        .or_else(|| Some(runtime_scope_key(&project.id, Some(&project.id))))
}

fn runtime_worktree_from_desktop(worktree: &WorktreeInfo) -> RuntimeWorktree {
    RuntimeWorktree {
        id: worktree.id.clone(),
        project_id: worktree.project_id.clone(),
        name: worktree.name.clone(),
        branch: worktree.branch.clone(),
        path: worktree.path.clone(),
        status: worktree.status.clone(),
        is_default: worktree.is_default,
        exists: worktree.exists,
        base_branch: None,
        changes: worktree.git_summary.changes as i64,
        incoming: worktree.git_summary.incoming,
        outgoing: worktree.git_summary.outgoing,
        additions: worktree.git_summary.additions,
        deletions: worktree.git_summary.deletions,
    }
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
            runtime_target: ProjectRuntimeTarget::Local,
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
            runtime_target: ProjectRuntimeTarget::Local,
        };
        let worktrees = crate::worktree::WorktreeSummary::default();

        assert_eq!(
            selected_ai_runtime_session_scope_id(Some(&project), &worktrees).as_deref(),
            Some("project-a")
        );
    }

    #[test]
    fn runtime_model_scope_key_matches_desktop_terminal_layout_key() {
        let project = ProjectInfo {
            id: "project-a".to_string(),
            name: "Project A".to_string(),
            path: "/tmp/project-a".to_string(),
            exists: true,
            badge: "PA".to_string(),
            badge_symbol: None,
            badge_color_hex: None,
            git_default_push_remote_name: None,
            runtime_target: ProjectRuntimeTarget::Local,
        };
        let worktrees = crate::worktree::WorktreeSummary {
            selected_worktree_id: Some("worktree-b".to_string()),
            worktrees: vec![
                crate::worktree::WorktreeInfo {
                    id: "project-a".to_string(),
                    project_id: "project-a".to_string(),
                    name: "main".to_string(),
                    branch: "main".to_string(),
                    path: "/tmp/project-a".to_string(),
                    status: "todo".to_string(),
                    is_default: true,
                    exists: true,
                    git_summary: Default::default(),
                },
                crate::worktree::WorktreeInfo {
                    id: "worktree-b".to_string(),
                    project_id: "project-a".to_string(),
                    name: "Task B".to_string(),
                    branch: "task-b".to_string(),
                    path: "/tmp/worktree-b".to_string(),
                    status: "active".to_string(),
                    is_default: false,
                    exists: true,
                    git_summary: Default::default(),
                },
            ],
            ..Default::default()
        };

        assert_eq!(
            runtime_model_scope_key(Some(&project), &worktrees).as_deref(),
            Some("project-a::worktree-b")
        );
    }
}
