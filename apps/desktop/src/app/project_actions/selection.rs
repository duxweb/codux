use super::*;

impl CoduxApp {
    pub(in crate::app) fn select_project(
        &mut self,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let select_started_at = Instant::now();
        let previous_project_id = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone());
        if previous_project_id.as_deref() == Some(project_id.as_str()) {
            return;
        }
        self.runtime_trace(
            "project-switch",
            &format!(
                "select start from={} to={}",
                previous_project_id.as_deref().unwrap_or("none"),
                project_id
            ),
        );
        if previous_project_id.is_some() {
            let save_started_at = Instant::now();
            self.sync_terminal_state_for_project_switch();
            self.runtime_trace(
                "project-switch",
                &format!(
                    "save_for_switch elapsed_ms={} to={project_id}",
                    save_started_at.elapsed().as_millis()
                ),
            );
        }
        self.status_message = "selected project in memory".to_string();
        self.persist_selected_project_async(project_id.clone(), cx);
        self.select_project_after_state_reload(project_id, window, cx);
        self.restore_selected_project_terminal_layout_now(
            self.project_switch_generation,
            window,
            cx,
        );
        self.runtime_trace(
            "project-switch",
            &format!(
                "select sync_done elapsed_ms={}",
                select_started_at.elapsed().as_millis()
            ),
        );
    }

    fn persist_selected_project_async(&mut self, project_id: String, cx: &mut Context<Self>) {
        let runtime_service = self.runtime_service.clone();
        let queued_at = Instant::now();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking({
                let project_id = project_id.clone();
                move || {
                    codux_runtime::runtime_trace::runtime_trace(
                        "project-switch",
                        &format!(
                            "select_persist worker_start project={} queue_wait_ms={}",
                            project_id,
                            queued_at.elapsed().as_millis()
                        ),
                    );
                    runtime_service.select_project(&project_id)
                }
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            let _ = this.update(cx, |app, cx| {
                if app
                    .state
                    .selected_project
                    .as_ref()
                    .map(|project| project.id.as_str())
                    != Some(project_id.as_str())
                {
                    return;
                }
                match result {
                    Ok(()) => {
                        app.runtime_trace(
                            "project-switch",
                            &format!(
                                "select persist done project={} elapsed_ms={}",
                                project_id,
                                queued_at.elapsed().as_millis()
                            ),
                        );
                    }
                    Err(error) => {
                        app.status_message = format!("selected in memory only: {error}");
                        app.invalidate_status_bar(cx);
                    }
                }
            });
        })
        .detach();
    }

    pub(in crate::app) fn restore_selected_project_terminal_layout_now(
        &mut self,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(_scope_key) = current_worktree_scope_key(&self.state) else {
            return;
        };
        self.runtime_trace(
            "project-switch",
            &format!("terminal_restore_immediate generation={generation}"),
        );
        self.schedule_terminal_layout_restore(
            self.state.terminal_layout.clone(),
            self.state.terminal_runtime.clone(),
            generation,
            window,
            cx,
        );
    }

    pub(super) fn select_project_after_state_reload(
        &mut self,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_switch_generation = self.project_switch_generation.wrapping_add(1);
        let switch_generation = self.project_switch_generation;
        self.remember_focused_terminal_for_current_scope(window, cx);
        self.apply_selected_project_shell(&project_id, window, cx);
        self.memory_manager_scope = "project".to_string();
        self.memory_manager_project_id = Some(project_id.clone());
        self.spawn_project_switch_load(project_id, switch_generation, cx);
        self.sync_project_list_state(cx);
        self.invalidate_project_context(cx);
    }

    pub(in crate::app) fn apply_selected_project_shell(
        &mut self,
        project_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let started_at = Instant::now();
        self.remember_current_file_panel_state();
        let Some(project) = self
            .state
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
        else {
            return;
        };

        self.state.selected_project = Some(project.clone());
        self.state.files.clear();
        self.selected_ai_session_id = None;
        self.state.ai_history = AIHistorySummary {
            is_loading: true,
            detail: "loading".to_string(),
            ..AIHistorySummary::default()
        };
        self.state.refresh_ai_history_stats();
        self.state.ai_session_detail = None;
        self.state.memory = MemorySummary::default();
        self.state.memory_manager = MemoryManagerSnapshot::default();
        self.reload_selected_project_db();
        self.normalize_selected_db_profile();
        self.state.worktrees = self
            .runtime_service
            .reload_worktrees_from_state(Some(&project.id), Some(&project.path));
        let terminal_owner_id = self
            .state
            .worktrees
            .selected_worktree_id
            .as_deref()
            .unwrap_or(project.id.as_str());
        let terminal_storage_key =
            super::ai_runtime_status::terminal_layout_storage_key(&project.id, terminal_owner_id);
        self.state.terminal_layout = self
            .runtime_service
            .reload_terminal_layout(Some(&terminal_storage_key));
        self.state.terminal_runtime = TerminalRuntimeSummary::default();
        self.reset_current_worktree_ui_state(cx);
        self.ensure_active_file_editor_state(window, cx);
        self.runtime_trace(
            "project-switch",
            &format!(
                "shell reset project={} elapsed_ms={} worktrees={} tasks={} selected_worktree={}",
                project_id,
                started_at.elapsed().as_millis(),
                self.state.worktrees.worktrees.len(),
                self.state.worktrees.tasks.len(),
                self.state
                    .worktrees
                    .selected_worktree_id
                    .as_deref()
                    .unwrap_or("none")
            ),
        );
    }

    pub(in crate::app) fn selected_worktree_path(&self) -> Option<String> {
        super::ai_runtime_status::selected_worktree_info(&self.state)
            .map(|worktree| worktree.path)
            .or_else(|| {
                self.state
                    .selected_project
                    .as_ref()
                    .map(|project| project.path.clone())
            })
    }

    pub(in crate::app) fn apply_current_file_panel_state(
        &mut self,
        state: super::app_state::FilePanelState,
    ) {
        self.state.files = state.files;
        self.file_directory = state.file_directory;
        self.selected_file_entry = state.selected_file_entry;
        self.selected_file_entries = state.selected_file_entries;
        self.file_selection_anchor = state.file_selection_anchor;
        self.file_tree_expanded_dirs = state.file_tree_expanded_dirs;
        self.file_tree_children = state.file_tree_children;
        self.file_editor_tabs = state.file_editor_tabs;
        self.active_file_editor_tab = state.active_file_editor_tab;
        self.clear_file_name_draft();
        self.prune_missing_file_tree_directories();
        self.normalize_selected_file_entry();
        self.file_dirty = self
            .active_file_editor_tab
            .as_deref()
            .and_then(|active| {
                self.file_editor_tabs
                    .iter()
                    .find(|tab| tab.relative_path == active)
            })
            .is_some_and(|tab| tab.dirty);
    }

    pub(in crate::app) fn apply_current_git_panel_state(
        &mut self,
        state: super::app_state::GitPanelState,
    ) -> bool {
        self.state.git = state.git;
        self.git_review = state.git_review;
        super::git_actions::merge_git_review_status_files(&mut self.git_review, &self.state.git);
        let worktree_git_changed = self.sync_current_worktree_git_summary_from_current_git();
        self.selected_git_file = state.selected_git_file;
        self.selected_git_files = state.selected_git_files;
        self.selected_git_branch = state.selected_git_branch;
        self.git_expanded_sections = state.git_expanded_sections;
        self.git_expanded_dirs = state.git_expanded_dirs;
        self.git_tree_children = state.git_tree_children;
        self.git_diff_preview = state.git_diff_preview;
        if let Some(content) = state.git_review_content {
            self.restore_git_review_derived_content(content);
        } else {
            self.clear_git_review_derived_content();
        }
        self.normalize_selected_git_file();
        self.normalize_selected_git_branch();
        worktree_git_changed
    }

    pub(in crate::app) fn sync_current_worktree_git_summary_from_current_git(&mut self) -> bool {
        let Some(scope_key) = current_worktree_scope_key(&self.state) else {
            return false;
        };
        let summary = ProjectWorktreeGitSummary {
            changes: self.state.git.staged + self.state.git.unstaged + self.state.git.untracked,
            incoming: self.state.git.behind,
            outgoing: self.state.git.ahead,
            additions: self.state.git.additions,
            deletions: self.state.git.deletions,
        };
        let Some(worktree) = self.state.worktrees.worktrees.iter_mut().find(|worktree| {
            worktree.project_id == scope_key.project_id && worktree.id == scope_key.worktree_id
        }) else {
            return false;
        };
        if worktree.git_summary == summary {
            return false;
        }
        worktree.git_summary = summary;
        true
    }

    pub(in crate::app) fn clear_current_worktree_ui_state(&mut self) {
        self.file_directory.clear();
        self.reset_file_tree_state();
        self.file_preview = "select a file to preview it".to_string();
        self.file_editable = false;
        self.file_dirty = false;
        self.file_editor_tabs.clear();
        self.active_file_editor_tab = None;
        self.clear_file_selection();
        self.state.files.clear();
        self.selected_git_file = None;
        self.selected_git_files.clear();
        self.git_expanded_sections =
            HashSet::from(["changed".to_string(), "untracked".to_string()]);
        self.git_expanded_dirs.clear();
        self.git_tree_children.clear();
        self.record_ui_state_clear("git_tree");
        self.git_diff_preview = "select a changed file to preview its diff".to_string();
        self.clear_git_review_derived_content();
        self.normalize_selected_git_branch();
    }

    pub(in crate::app) fn reset_current_worktree_ui_state(&mut self, cx: &mut Context<Self>) {
        self.state.git = self.current_worktree_initial_git();
        self.git_review = GitReviewSummary::default();
        self.clear_current_worktree_ui_state();
        self.load_current_file_editor_layout_async(cx);
    }

    fn current_worktree_initial_git(&self) -> GitSummary {
        if self.state.worktrees.active_git.is_repository
            || self.state.worktrees.active_git.error.is_some()
        {
            return self.state.worktrees.active_git.clone();
        }
        GitSummary::default()
    }
}
