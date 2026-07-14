use super::*;

impl CoduxApp {
    pub(in crate::app) fn refresh_task_column_async(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to refresh".to_string();
            self.invalidate_task_column(cx);
            self.invalidate_status_bar(cx);
            return;
        };
        if self.task_column_refreshing {
            return;
        }

        self.task_column_refreshing = true;
        self.status_message = format!("refreshing tasks for {}", project.name);
        self.invalidate_task_column(cx);
        self.invalidate_status_bar(cx);

        let runtime_service = self.runtime_service.clone();
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worktree = super::ai_runtime_status::selected_worktree_info(&self.state);
        let scope_key = WorktreeScopeKey {
            project_id: project_id.clone(),
            worktree_id: worktree
                .as_ref()
                .map(|worktree| worktree.id.clone())
                .unwrap_or_else(|| project_id.clone()),
        };
        let generation = self.project_switch_generation;
        let include_cached = self.state.settings.statistics_mode == "includingCache";
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                let worktrees =
                    runtime_service.reload_worktrees(Some(&project_id), Some(&project_path));
                let request = ai_history_worktree_request(&project, worktree.as_ref());
                let ai_history = runtime_service
                    .indexed_project_ai_history_state(request)
                    .ok()
                    .map(|state| {
                        ai_history_summary_from_state_or_status(
                            &AIHistorySummary::default(),
                            &state,
                        )
                    })
                    .unwrap_or_else(|| AIHistorySummary {
                        is_loading: true,
                        detail: "loading".to_string(),
                        ..AIHistorySummary::default()
                    });
                let remote_ai_current_sessions = runtime_service
                    .remote_ai_current_sessions(
                        &project.path,
                        &scope_key.worktree_id,
                        include_cached,
                    )
                    .and_then(Result::ok)
                    .unwrap_or_default();
                (
                    ProjectSwitchTaskLoad {
                        project_id: project_id.clone(),
                        generation,
                        worktrees,
                    },
                    ProjectSwitchPrimaryLoad {
                        project_id: project_id.clone(),
                        generation,
                        scope_key,
                        ai_history,
                        remote_ai_current_sessions,
                    },
                )
            })
            .await
            .ok();

            let _ = this.update(cx, |app, cx| {
                app.task_column_refreshing = false;
                if let Some((task_load, primary_load)) = result {
                    app.apply_project_switch_task_load(task_load, cx);
                    app.apply_project_switch_primary_load(primary_load, cx);
                    app.status_message = "task list refreshed".to_string();
                } else {
                    app.status_message = "failed to refresh task list".to_string();
                }
                app.invalidate_task_column(cx);
                app.invalidate_status_bar(cx);
            });
        })
        .detach();
    }

    pub(in crate::app) fn refresh_files_panel_state_async(&mut self, cx: &mut Context<Self>) {
        self.reload_project_files_async(cx);
    }

    pub(in crate::app) fn refresh_git_panel_state_async(&mut self, cx: &mut Context<Self>) {
        self.refresh_git_panel_state_async_impl(false, cx);
    }

    pub(in crate::app) fn refresh_git_panel_state_async_quiet(&mut self, cx: &mut Context<Self>) {
        self.refresh_git_panel_state_async_impl(true, cx);
    }

    fn refresh_git_panel_state_async_impl(&mut self, quiet: bool, cx: &mut Context<Self>) {
        if self.git_review_refreshing {
            return;
        }
        let Some(project_path) = self.selected_worktree_path() else {
            return;
        };
        let Some(scope_key) = current_worktree_scope_key(&self.state) else {
            return;
        };
        let base_branch = self.git_review.base_branch.clone();
        let runtime_service = self.runtime_service.clone();
        let generation = self.project_switch_generation;
        self.git_review_refreshing = true;
        self.invalidate_git_panel(cx);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    let (git, mut git_review) = runtime_service
                        .reload_project_git_state(&project_path, base_branch.as_deref());
                    super::git_actions::merge_git_review_status_files(&mut git_review, &git);
                    (scope_key, generation, git, git_review)
                },
            )
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                let Some((scope_key, generation, git, git_review)) = result else {
                    if app.project_switch_generation == generation {
                        app.git_review_refreshing = false;
                    }
                    app.invalidate_git_panel(cx);
                    return;
                };
                if app.project_switch_generation != generation
                    || current_worktree_scope_key(&app.state).as_ref() != Some(&scope_key)
                {
                    if app.project_switch_generation == generation {
                        app.git_review_refreshing = false;
                    }
                    app.invalidate_git_panel(cx);
                    return;
                }
                app.state.git = git;
                app.git_review = git_review;
                let worktree_git_changed = app.sync_current_worktree_git_summary_from_current_git();
                app.git_review_refreshing = false;
                app.normalize_selected_git_file();
                app.normalize_selected_git_branch();
                if !quiet {
                    app.status_message = format!(
                        "git status reloaded: {} changed, {} staged, {} unstaged, {} untracked",
                        app.state.git.changed_files.len(),
                        app.state.git.staged,
                        app.state.git.unstaged,
                        app.state.git.untracked
                    );
                }
                app.runtime_trace(
                    "git",
                    &format!(
                        "manual_reload done changed={} staged={} unstaged={} untracked={}",
                        app.state.git.changed_files.len(),
                        app.state.git.staged,
                        app.state.git.unstaged,
                        app.state.git.untracked
                    ),
                );
                if app.workspace_view == WorkspaceView::Review {
                    app.ensure_selected_git_review_file_loaded_async(cx);
                }
                app.invalidate_git_panel(cx);
                app.invalidate_status_bar(cx);
                if worktree_git_changed {
                    app.invalidate_task_column(cx);
                }
            });
        })
        .detach();
    }
}
