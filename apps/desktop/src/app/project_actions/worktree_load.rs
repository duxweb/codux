use super::*;

impl CoduxApp {
    pub(in crate::app) fn spawn_worktree_sidebar_load(
        &mut self,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(scope_key) = current_worktree_scope_key(&self.state) else {
            return;
        };
        let Some(worktree_path) = self.selected_worktree_path() else {
            return;
        };
        let expanded_dirs = self
            .file_panel_cache
            .get(&scope_key)
            .map(|state| {
                state
                    .file_tree_expanded_dirs
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let runtime_service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let load = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    let files = runtime_service.reload_project_files(&worktree_path, None);
                    let file_tree_children = expanded_dirs
                        .into_iter()
                        .map(|directory_path| {
                            let children = runtime_service.reload_project_files(
                                &worktree_path,
                                Some(directory_path.as_str()),
                            );
                            (directory_path, children)
                        })
                        .collect::<HashMap<_, _>>();
                    let file_editor_layout =
                        runtime_service.reload_file_editor_layout(Some(&scope_key.worktree_id));
                    let (git, mut git_review) =
                        runtime_service.stored_project_git_state(&worktree_path, None);
                    super::git_actions::merge_git_review_status_files(&mut git_review, &git);
                    WorktreeSidebarLoad {
                        generation,
                        scope_key,
                        files,
                        file_tree_children,
                        file_editor_layout,
                        git,
                        git_review,
                    }
                },
            )
            .await
            .ok();
            let _ = this.update_in(cx, |app, window, cx| {
                if let Some(load) = load {
                    app.apply_worktree_sidebar_load(load, window, cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::app) fn apply_worktree_sidebar_load(
        &mut self,
        load: WorktreeSidebarLoad,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_key = current_worktree_scope_key(&self.state);
        if self.project_switch_generation != load.generation
            || current_key.as_ref() != Some(&load.scope_key)
        {
            return;
        }
        let (file_editor_tabs, active_file_editor_tab) =
            super::app_state::file_editor_tabs_from_layout(load.file_editor_layout);
        let file_state = if let Some(cached) = self.file_panel_cache.get(&load.scope_key) {
            super::app_state::FilePanelState {
                files: load.files.clone(),
                file_directory: cached.file_directory.clone(),
                selected_file_entry: cached.selected_file_entry.clone(),
                selected_file_entries: cached.selected_file_entries.clone(),
                file_selection_anchor: cached.file_selection_anchor.clone(),
                file_tree_expanded_dirs: cached.file_tree_expanded_dirs.clone(),
                file_tree_children: load.file_tree_children,
                file_editor_tabs: cached.file_editor_tabs.clone(),
                active_file_editor_tab: cached.active_file_editor_tab.clone(),
            }
        } else {
            super::app_state::FilePanelState {
                files: load.files.clone(),
                file_directory: String::new(),
                selected_file_entry: None,
                selected_file_entries: HashSet::new(),
                file_selection_anchor: None,
                file_tree_expanded_dirs: HashSet::new(),
                file_tree_children: HashMap::new(),
                file_editor_tabs,
                active_file_editor_tab,
            }
        };
        let git_state = super::app_state::GitPanelState {
            git: load.git.clone(),
            git_review: load.git_review.clone(),
            selected_git_file: None,
            selected_git_files: HashSet::new(),
            selected_git_branch: load
                .git
                .branches
                .iter()
                .find(|branch| branch.is_current)
                .or_else(|| load.git.branches.first())
                .map(|branch| branch.name.clone()),
            git_expanded_sections: HashSet::from(["changed".to_string(), "untracked".to_string()]),
            git_expanded_dirs: HashSet::new(),
            git_tree_children: HashMap::new(),
            git_diff_preview: "select a changed file to preview its diff".to_string(),
            git_review_content: None,
        };
        self.clear_current_worktree_ui_state();
        self.apply_current_file_panel_state(file_state);
        self.ensure_active_file_editor_state(window, cx);
        let worktree_git_changed = self.apply_current_git_panel_state(git_state);
        if self.workspace_view == WorkspaceView::Review {
            self.ensure_selected_git_review_file_loaded_async(cx);
        }
        self.invalidate_file_panel(cx);
        self.invalidate_git_panel(cx);
        self.invalidate_status_bar(cx);
        if worktree_git_changed {
            self.invalidate_task_column(cx);
        }
    }

    pub(in crate::app) fn persist_current_terminal_layout(&mut self) {
        self.spawn_persist_terminal_layout_snapshot(
            super::ai_runtime_status::current_terminal_layout_storage_key(&self.state),
            self.terminal_layout_snapshot(),
        );
    }

    pub(in crate::app) fn merge_worktree_ai_history_if_current(
        &mut self,
        key: super::app_state::WorktreeScopeKey,
        ai_history: AIHistorySummary,
    ) -> bool {
        if current_worktree_scope_key(&self.state).as_ref() == Some(&key) {
            return merge_ai_history_summary(&mut self.state.ai_history, ai_history);
        }
        false
    }

    pub(in crate::app) fn trace_workspace_state(
        &self,
        event: &str,
        worktree_id: &str,
        detail: &str,
    ) {
        let project_id = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str())
            .unwrap_or("");
        self.runtime_trace(
            "workspace-switch",
            &format!(
                "{event} project={} worktree={} generation={} {detail}",
                project_id, worktree_id, self.project_switch_generation
            ),
        );
    }

    pub(in crate::app) fn spawn_persist_terminal_layout_snapshot(
        &self,
        owner_id: Option<String>,
        layout_snapshot: TerminalLayoutSnapshot,
    ) {
        let Some(owner_id) = owner_id else {
            return;
        };
        if layout_snapshot.tabs.is_empty() && layout_snapshot.top_panes.is_empty() {
            self.runtime_trace(
                "terminal-layout",
                &format!("skip empty layout persist owner={owner_id}"),
            );
            return;
        }
        let runtime_service = self.runtime_service.clone();
        codux_runtime::async_runtime::spawn_blocking(move || {
            if let Err(error) = runtime_service.save_terminal_layout_with_grid(
                &owner_id,
                codux_runtime::terminal_layout::TerminalLayoutSummary {
                    tabs: layout_snapshot.tabs,
                    top_panes: layout_snapshot.top_panes,
                    top_ratios: layout_snapshot.top_ratios,
                    top_grid: layout_snapshot.top_grid,
                    split_tree: layout_snapshot.split_tree,
                    bottom_ratio: layout_snapshot.bottom_ratio,
                    collapsed_panes: layout_snapshot.collapsed_panes,
                    ..Default::default()
                },
            ) {
                codux_runtime::runtime_trace::runtime_trace(
                    "terminal-layout",
                    &format!("failed to persist terminal layout {owner_id}: {error}"),
                );
            }
        });
    }
}
