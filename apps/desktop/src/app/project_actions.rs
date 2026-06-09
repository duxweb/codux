use super::*;
use crate::app::app_events::{ChildWindowUpdateKind, publish_child_window_update};
use crate::app::terminal_worktree_actions::TerminalLayoutSnapshot;
use crate::app::window_actions::{AuxiliaryWindowSlot, AuxiliaryWindowSpec};

impl CoduxApp {
    pub(super) fn refresh_task_column_async(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn refresh_files_panel_state_async(&mut self, cx: &mut Context<Self>) {
        self.reload_project_files_async(cx);
    }

    pub(super) fn refresh_git_panel_state_async(&mut self, cx: &mut Context<Self>) {
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
                    let git = runtime_service.reload_project_git(&project_path);
                    let mut git_review = runtime_service
                        .reload_project_git_review(&project_path, base_branch.as_deref());
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
                app.status_message = format!(
                    "git status reloaded: {} changed, {} staged, {} unstaged, {} untracked",
                    app.state.git.changed_files.len(),
                    app.state.git.staged,
                    app.state.git.unstaged,
                    app.state.git.untracked
                );
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

    pub(super) fn select_project(
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

    fn select_project_after_state_reload(
        &mut self,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_switch_generation = self.project_switch_generation.wrapping_add(1);
        let switch_generation = self.project_switch_generation;
        self.remember_focused_terminal_for_current_scope(window, cx);
        self.remember_active_bottom_terminal_for_current_scope();
        self.apply_selected_project_shell(&project_id, window, cx);
        self.memory_manager_scope = "project".to_string();
        self.memory_manager_project_id = Some(project_id.clone());
        self.terminal_layout_loading = true;
        self.spawn_project_switch_load(project_id, switch_generation, cx);
        self.sync_project_list_state(cx);
        self.invalidate_project_context(cx);
    }

    pub(super) fn apply_selected_project_shell(
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

    pub(super) fn selected_worktree_path(&self) -> Option<String> {
        super::ai_runtime_status::selected_worktree_info(&self.state)
            .map(|worktree| worktree.path)
            .or_else(|| {
                self.state
                    .selected_project
                    .as_ref()
                    .map(|project| project.path.clone())
            })
    }

    pub(super) fn apply_current_file_panel_state(
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
        self.file_name_draft_kind = None;
        self.file_name_draft_target = None;
        self.file_name_draft_value.clear();
        self.file_name_draft_select_all = false;
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

    pub(super) fn apply_current_git_panel_state(
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

    pub(super) fn sync_current_worktree_git_summary_from_current_git(&mut self) -> bool {
        let Some(scope_key) = current_worktree_scope_key(&self.state) else {
            return false;
        };
        let summary = ProjectWorktreeGitSummary {
            changes: self.state.git.staged + self.state.git.unstaged + self.state.git.untracked,
            incoming: self.state.git.behind,
            outgoing: self.state.git.ahead,
            additions: self
                .git_review
                .files
                .iter()
                .map(|file| file.additions)
                .sum(),
            deletions: self
                .git_review
                .files
                .iter()
                .map(|file| file.deletions)
                .sum(),
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

    pub(super) fn clear_current_worktree_ui_state(&mut self) {
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

    pub(super) fn reset_current_worktree_ui_state(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn spawn_worktree_sidebar_load(&mut self, generation: u64, cx: &mut Context<Self>) {
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

    pub(super) fn apply_worktree_sidebar_load(
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

    pub(super) fn persist_current_terminal_layout(&mut self) {
        self.spawn_persist_terminal_layout_snapshot(
            super::ai_runtime_status::current_terminal_layout_storage_key(&self.state),
            self.terminal_layout_snapshot(),
        );
    }

    pub(super) fn merge_worktree_ai_history_if_current(
        &mut self,
        key: super::app_state::WorktreeScopeKey,
        ai_history: AIHistorySummary,
    ) -> bool {
        if current_worktree_scope_key(&self.state).as_ref() == Some(&key) {
            return merge_ai_history_summary(&mut self.state.ai_history, ai_history);
        }
        false
    }

    pub(super) fn trace_workspace_state(&self, event: &str, worktree_id: &str, detail: &str) {
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

    pub(super) fn spawn_persist_terminal_layout_snapshot(
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
            if let Err(error) = runtime_service.save_terminal_layout(
                &owner_id,
                layout_snapshot.tabs,
                String::new(),
                layout_snapshot.top_panes,
                layout_snapshot.top_ratios,
                layout_snapshot.bottom_ratio,
            ) {
                codux_runtime::runtime_trace::runtime_trace(
                    "terminal-layout",
                    &format!("failed to persist terminal layout {owner_id}: {error}"),
                );
            }
        });
    }

    pub(super) fn spawn_project_switch_load(
        &mut self,
        project_id: String,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self
            .state
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
        else {
            return;
        };
        let projects = self.state.projects.clone();
        let runtime_service = self.runtime_service.clone();
        let terminal_layout_service = runtime_service.clone();
        let terminal_project = project.clone();
        let task_runtime_service = runtime_service.clone();
        let task_project = project.clone();
        let primary_runtime_service = runtime_service.clone();
        let primary_project = project.clone();
        let primary_worktree = super::ai_runtime_status::selected_worktree_info(&self.state);
        let primary_scope_key = WorktreeScopeKey {
            project_id: project.id.clone(),
            worktree_id: primary_worktree
                .as_ref()
                .map(|worktree| worktree.id.clone())
                .unwrap_or_else(|| project.id.clone()),
        };
        let stats_runtime_service = runtime_service.clone();
        let stats_project = project.clone();
        self.runtime_trace(
            "project-switch",
            &format!(
                "spawn_loads project={} generation={}",
                project_id, generation
            ),
        );

        let terminal_queued_at = Instant::now();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let terminal = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    let worker_started_at = Instant::now();
                    codux_runtime::runtime_trace::runtime_trace(
                        "project-switch",
                        &format!(
                            "terminal_load worker_start project={} generation={} queue_wait_ms={}",
                            terminal_project.id,
                            generation,
                            terminal_queued_at.elapsed().as_millis()
                        ),
                    );
                    let worktrees = terminal_layout_service.reload_worktrees_from_state(
                        Some(&terminal_project.id),
                        Some(&terminal_project.path),
                    );
                    let terminal_owner_id = worktrees
                        .selected_worktree_id
                        .as_deref()
                        .unwrap_or(terminal_project.id.as_str())
                        .to_string();
                    let terminal_storage_key =
                        super::ai_runtime_status::terminal_layout_storage_key(
                            &terminal_project.id,
                            &terminal_owner_id,
                        );
                    let scope_key = WorktreeScopeKey {
                        project_id: terminal_project.id.clone(),
                        worktree_id: terminal_owner_id.clone(),
                    };
                    let terminal_layout =
                        terminal_layout_service.reload_terminal_layout(Some(&terminal_storage_key));
                    let terminal_runtime = TerminalRuntimeSummary::default();
                    codux_runtime::runtime_trace::runtime_trace(
                        "project-switch",
                        &format!(
                            "terminal_load worker_done project={} generation={} elapsed_ms={}",
                            terminal_project.id,
                            generation,
                            worker_started_at.elapsed().as_millis()
                        ),
                    );
                    ProjectSwitchTerminalLoad {
                        project_id: terminal_project.id,
                        generation,
                        scope_key,
                        terminal_layout,
                        terminal_runtime,
                    }
                },
            )
            .await
            .ok();
            let _ = this.update_in(cx, |app, window, cx| {
                if let Some(terminal) = terminal {
                    app.apply_project_switch_terminal_load(terminal, window, cx);
                }
            });
        })
        .detach();

        let task_project_id = task_project.id.clone();
        let task_project_path = task_project.path.clone();
        let task_queued_at = Instant::now();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let task_load = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    let worker_started_at = Instant::now();
                    codux_runtime::runtime_trace::runtime_trace(
                        "project-switch",
                        &format!(
                            "task_load worker_start project={} generation={} queue_wait_ms={}",
                            task_project_id,
                            generation,
                            task_queued_at.elapsed().as_millis()
                        ),
                    );
                    let worktrees = task_runtime_service.reload_worktrees_from_state(
                        Some(&task_project_id),
                        Some(&task_project_path),
                    );
                    codux_runtime::runtime_trace::runtime_trace(
                        "project-switch",
                        &format!(
                            "task_load worker_done project={} generation={} elapsed_ms={}",
                            task_project_id,
                            generation,
                            worker_started_at.elapsed().as_millis()
                        ),
                    );
                    ProjectSwitchTaskLoad {
                        project_id: task_project_id,
                        generation,
                        worktrees,
                    }
                },
            )
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                if let Some(task_load) = task_load {
                    app.apply_project_switch_task_load(task_load, cx);
                }
            });
        })
        .detach();

        let primary_queued_at = Instant::now();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let primary = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    let worker_started_at = Instant::now();
                    codux_runtime::runtime_trace::runtime_trace(
                        "project-switch",
                        &format!(
                            "primary_load worker_start project={} generation={} queue_wait_ms={}",
                            primary_project.id,
                            generation,
                            primary_queued_at.elapsed().as_millis()
                        ),
                    );
                    let request =
                        ai_history_worktree_request(&primary_project, primary_worktree.as_ref());
                    let ai_history = primary_runtime_service
                        .indexed_project_ai_history_summary(request)
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
                    codux_runtime::runtime_trace::runtime_trace(
                        "project-switch",
                        &format!(
                            "primary_load worker_done project={} generation={} elapsed_ms={}",
                            primary_project.id,
                            generation,
                            worker_started_at.elapsed().as_millis()
                        ),
                    );
                    ProjectSwitchPrimaryLoad {
                        project_id: primary_project.id,
                        generation,
                        scope_key: primary_scope_key,
                        ai_history,
                    }
                },
            )
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                if let Some(primary) = primary {
                    app.apply_project_switch_primary_load(primary, cx);
                }
            });
        })
        .detach();

        let stats_queued_at = Instant::now();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let load = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    let worker_started_at = Instant::now();
                    codux_runtime::runtime_trace::runtime_trace(
                        "project-switch",
                        &format!(
                            "full_load worker_start project={} generation={} queue_wait_ms={}",
                            stats_project.id,
                            generation,
                            stats_queued_at.elapsed().as_millis()
                        ),
                    );
                    let ai_global_history = stats_runtime_service.reload_global_ai_history();
                    let memory = stats_runtime_service.reload_memory(Some(&stats_project.id));
                    let memory_manager = stats_runtime_service.reload_memory_manager(
                        &projects,
                        "project",
                        Some(&stats_project.id),
                        "active",
                    );
                    codux_runtime::runtime_trace::runtime_trace(
                        "project-switch",
                        &format!(
                            "full_load worker_done project={} generation={} elapsed_ms={}",
                            stats_project.id,
                            generation,
                            worker_started_at.elapsed().as_millis()
                        ),
                    );
                    ProjectSwitchLoad {
                        project_id: stats_project.id,
                        generation,
                        ai_global_history,
                        memory,
                        memory_manager,
                    }
                },
            )
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                if let Some(load) = load {
                    app.apply_project_switch_load(load, cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn apply_project_switch_terminal_load(
        &mut self,
        load: ProjectSwitchTerminalLoad,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str())
            != Some(load.project_id.as_str())
            || self.project_switch_generation != load.generation
        {
            return;
        }
        let selected_terminal_key = current_worktree_scope_key(&self.state);
        let is_selected_terminal_owner = selected_terminal_key
            .as_ref()
            .is_some_and(|key| key == &load.scope_key);
        self.runtime_trace(
            "project-switch",
            &format!(
                "terminal_load apply project={} worktree={} generation={} selected={}",
                load.project_id,
                load.scope_key.worktree_id,
                load.generation,
                is_selected_terminal_owner
            ),
        );
        if !is_selected_terminal_owner {
            self.runtime_trace(
                "project-switch",
                &format!(
                    "terminal_load stale_scope project={} generation={}",
                    load.project_id, load.generation
                ),
            );
        } else {
            self.schedule_terminal_layout_restore(
                load.terminal_layout,
                load.terminal_runtime,
                load.generation,
                window,
                cx,
            );
            self.invalidate_terminal_workspace(cx);
        }
    }

    pub(super) fn apply_project_switch_task_load(
        &mut self,
        load: ProjectSwitchTaskLoad,
        cx: &mut Context<Self>,
    ) {
        if self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str())
            != Some(load.project_id.as_str())
            || self.project_switch_generation != load.generation
        {
            return;
        }
        self.state.worktrees = load.worktrees;
        self.reset_current_worktree_ui_state(cx);
        self.spawn_worktree_sidebar_load(load.generation, cx);
        self.runtime_trace(
            "project-switch",
            &format!(
                "task_load apply project={} generation={} worktrees={} tasks={}",
                load.project_id,
                load.generation,
                self.state.worktrees.worktrees.len(),
                self.state.worktrees.tasks.len()
            ),
        );
        self.invalidate_worktree_context(cx);
    }

    pub(super) fn apply_project_switch_primary_load(
        &mut self,
        load: ProjectSwitchPrimaryLoad,
        cx: &mut Context<Self>,
    ) {
        if self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str())
            != Some(load.project_id.as_str())
            || self.project_switch_generation != load.generation
            || current_worktree_scope_key(&self.state).as_ref() != Some(&load.scope_key)
        {
            return;
        }
        if merge_ai_history_summary(&mut self.state.ai_history, load.ai_history) {
            self.selected_ai_session_id = None;
            self.state.ai_session_detail = None;
            self.state.refresh_ai_history_stats();
        }
        self.runtime_trace(
            "project-switch",
            &format!(
                "primary_load apply project={} generation={} worktrees={} tasks={} sessions={}",
                load.project_id,
                load.generation,
                self.state.worktrees.worktrees.len(),
                self.state.worktrees.tasks.len(),
                self.state.ai_history.sessions.len()
            ),
        );
        self.normalize_selected_ai_session();
        self.invalidate_ui(
            cx,
            [
                UiRegion::TaskColumn,
                UiRegion::AIStatsSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(super) fn apply_project_switch_load(
        &mut self,
        load: ProjectSwitchLoad,
        cx: &mut Context<Self>,
    ) {
        if self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str())
            != Some(load.project_id.as_str())
            || self.project_switch_generation != load.generation
        {
            return;
        }
        self.state.ai_global_history = load.ai_global_history;
        self.state.memory = load.memory;
        self.state.memory_manager = load.memory_manager;
        self.runtime_trace(
            "project-switch",
            &format!(
                "full_load apply project={} generation={} worktrees={} tasks={}",
                load.project_id,
                load.generation,
                self.state.worktrees.worktrees.len(),
                self.state.worktrees.tasks.len()
            ),
        );
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceAssistant,
                UiRegion::AIStatsSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(super) fn merge_selected_project_worktrees(&mut self, worktrees: WorktreeSummary) {
        if worktree_summary_has_git_counts(&self.state.worktrees)
            && !worktree_summary_has_git_counts(&worktrees)
        {
            return;
        }
        if worktree_summary_has_rows(&worktrees)
            || !worktree_summary_has_rows(&self.state.worktrees)
        {
            self.state.worktrees = worktrees;
        }
    }

    pub(super) fn schedule_terminal_layout_restore(
        &mut self,
        mut terminal_layout: TerminalLayoutSummary,
        mut terminal_runtime: TerminalRuntimeSummary,
        generation: u64,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(key) = current_worktree_scope_key(&self.state)
            && let Some((cached_layout, cached_runtime)) = self.cached_terminal_layout_state(&key)
        {
            self.runtime_trace(
                "terminal-restore",
                &format!(
                    "use runtime layout cache project={} worktree={} tabs={} top={}",
                    key.project_id,
                    key.worktree_id,
                    cached_layout.tabs.len(),
                    cached_layout.top_panes.len()
                ),
            );
            terminal_layout = cached_layout;
            terminal_runtime = cached_runtime;
        }
        self.state.terminal_layout = terminal_layout.clone();
        self.state.terminal_runtime = terminal_runtime.clone();
        self.terminal_layout_loading = true;
        let scheduled_at = Instant::now();
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_millis(16)).await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.runtime_trace(
                    "terminal-restore",
                    &format!(
                        "after_frame_start generation={} delay_ms={}",
                        generation,
                        scheduled_at.elapsed().as_millis()
                    ),
                );
                if app.project_switch_generation != generation {
                    return;
                }
                app.apply_terminal_layout_skeleton(
                    terminal_layout,
                    terminal_runtime,
                    generation,
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    fn apply_terminal_layout_skeleton(
        &mut self,
        terminal_layout: TerminalLayoutSummary,
        terminal_runtime: TerminalRuntimeSummary,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let restore_started_at = Instant::now();
        self.terminal_layout_loading = true;
        let owner_id = super::ai_runtime_status::terminal_layout_owner_id(&self.state);
        let (terminal_layout, terminal_runtime) = normalize_terminal_restore_state(
            owner_id.as_deref(),
            terminal_layout,
            terminal_runtime,
            &self.state.settings.language,
        );
        self.state.terminal_layout = terminal_layout;
        self.state.terminal_runtime = terminal_runtime;
        let plan_started_at = Instant::now();
        let restore_plan = terminal_restore_plan_for_language(
            &self.state.terminal_layout,
            &self.state.terminal_runtime,
            &self.state.settings.language,
            self.remembered_active_terminal_runtime_id(),
            self.remembered_active_bottom_terminal_id(),
        );
        self.state.terminal_layout.active_terminal_id =
            restore_plan.active_terminal_id.clone().unwrap_or_default();
        self.runtime_trace(
            "terminal-restore",
            &format!(
                "plan elapsed_ms={} owner={} tabs={} active_index={} active_runtime={} active_bottom={}",
                plan_started_at.elapsed().as_millis(),
                owner_id.as_deref().unwrap_or("none"),
                restore_plan.tabs.len(),
                restore_plan.active_index,
                restore_plan.active_terminal_id.as_deref().unwrap_or("none"),
                restore_plan
                    .active_bottom_terminal_id
                    .as_deref()
                    .unwrap_or("none")
            ),
        );
        let artifacts_started_at = Instant::now();
        prepare_memory_launch_artifacts(&self.runtime_service, &self.state);
        self.runtime_trace(
            "terminal-restore",
            &format!(
                "artifacts elapsed_ms={} owner={}",
                artifacts_started_at.elapsed().as_millis(),
                owner_id.as_deref().unwrap_or("none")
            ),
        );
        let launch_context = self.current_terminal_launch_context();
        let base_pty_config = launch_context
            .as_ref()
            .map(TerminalLaunchContext::to_config)
            .unwrap_or_default();
        let (terminals, active_terminal_id, next_terminal_index) =
            restore_terminal_tabs_skeleton(&restore_plan, launch_context.as_ref());
        let tab_count = terminals.len();
        self.terminals = terminals;
        self.active_terminal_id = active_terminal_id;
        self.next_terminal_index = next_terminal_index;
        let pending_terminals =
            self.mount_visible_terminal_views_for_restore(&restore_plan, &base_pty_config, cx);
        self.status_message = format!(
            "terminal layout reloading · {} tab{}",
            self.terminals.len(),
            if self.terminals.len() == 1 { "" } else { "s" }
        );
        self.runtime_trace(
            "terminal-restore",
            &format!(
                "skeleton elapsed_ms={} owner={} tabs={}",
                restore_started_at.elapsed().as_millis(),
                owner_id.as_deref().unwrap_or("none"),
                tab_count
            ),
        );
        self.invalidate_terminal_workspace(cx);
        if self.workspace_view == WorkspaceView::Terminal {
            let focused = self.focus_active_terminal(window, cx);
            self.runtime_trace(
                "terminal-restore",
                &format!("focus_after_skeleton focused={focused} generation={generation}"),
            );
        }
        self.spawn_attach_pending_terminals(generation, pending_terminals, cx);
    }

    fn mount_visible_terminal_views_for_restore(
        &mut self,
        restore_plan: &TerminalRestorePlan,
        base_pty_config: &TerminalPtyConfig,
        cx: &mut Context<Self>,
    ) -> Vec<(TerminalPtyConfig, crate::terminal::PendingTerminalAttach)> {
        let terminal_config = self.terminal_config_from_settings();
        let terminal_pane_registry = self.terminal_pane_registry.clone();
        let mut pending = Vec::new();
        let mut registrations = Vec::new();
        for (tab_index, tab) in self.terminals.iter_mut().enumerate() {
            let Some(tab_plan) = restore_plan.tabs.get(tab_index) else {
                continue;
            };
            let mount_tab = tab_index == restore_plan.active_index
                || tab.id == self.active_terminal_id
                || tab_plan.placement == TerminalTabPlacement::Top;
            if !mount_tab {
                continue;
            }
            for slot in &mut tab.panes {
                if slot.pane.is_some() {
                    continue;
                }
                let pty_config = terminal_pty_config_for_terminal_id(
                    base_pty_config,
                    slot.terminal_id.as_deref(),
                    &slot.title,
                );
                if let Some(pane) = slot
                    .terminal_id
                    .as_deref()
                    .and_then(|terminal_id| terminal_pane_registry.get(terminal_id))
                    .cloned()
                {
                    refresh_terminal_pane_config(&pane, &terminal_config, cx);
                    slot.pane = Some(pane);
                    continue;
                }
                let (pane, attach) = TerminalPane::pending_with_pty_config(
                    cx,
                    pty_config.clone(),
                    terminal_config.clone(),
                );
                if let Some(terminal_id) = slot.terminal_id.clone() {
                    registrations.push((terminal_id, pane.clone()));
                }
                slot.pane = Some(pane);
                pending.push((pty_config, attach));
            }
        }
        for (terminal_id, pane) in registrations {
            self.register_terminal_pane(Some(&terminal_id), &pane, cx);
        }
        pending
    }

    fn spawn_attach_pending_terminals(
        &mut self,
        generation: u64,
        pending_terminals: Vec<(TerminalPtyConfig, crate::terminal::PendingTerminalAttach)>,
        cx: &mut Context<Self>,
    ) {
        if pending_terminals.is_empty() {
            self.terminal_layout_loading = false;
            self.sync_terminal_state_for_project_switch();
            self.invalidate_terminal_workspace(cx);
            return;
        }
        let terminal_manager = self.terminal_manager.clone();
        let terminal_config = self.terminal_config_from_settings();
        let attach_started_at = Instant::now();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let results = codux_runtime::async_runtime::spawn_blocking({
                let terminal_manager = terminal_manager.clone();
                let terminal_config = terminal_config.clone();
                move || {
                    pending_terminals
                        .into_iter()
                        .map(|(pty_config, pending)| {
                            let terminal_id = pending
                                .terminal_id()
                                .map(str::to_string)
                                .unwrap_or_else(|| "none".to_string());
                            let result = TerminalPane::attach_pending_session(
                                terminal_manager.clone(),
                                pty_config,
                                terminal_config.clone(),
                                pending,
                            )
                            .map(|_| ())
                            .map_err(|error| error.to_string());
                            (terminal_id, result)
                        })
                        .collect::<Vec<_>>()
                }
            })
            .await
            .unwrap_or_else(|error| vec![("none".to_string(), Err(error.to_string()))]);
            let _ = this.update(cx, |app, cx| {
                let ok_count = results.iter().filter(|(_, result)| result.is_ok()).count();
                let error = results.iter().find_map(|(terminal_id, result)| {
                    result
                        .as_ref()
                        .err()
                        .map(|error| format!("{terminal_id}: {error}"))
                });
                app.runtime_trace(
                    "terminal-restore",
                    &format!(
                        "attach_pending elapsed_ms={} ok={} total={} error={}",
                        attach_started_at.elapsed().as_millis(),
                        ok_count,
                        results.len(),
                        error.as_deref().unwrap_or("none")
                    ),
                );
                if app.project_switch_generation != generation {
                    return;
                }
                app.terminal_layout_loading = false;
                if let Some(error) = error {
                    app.status_message = format!("failed to prepare terminal: {error}");
                } else {
                    app.status_message = format!(
                        "terminal layout reloaded · {} tab{}",
                        app.terminals.len(),
                        if app.terminals.len() == 1 { "" } else { "s" }
                    );
                    app.sync_terminal_state_for_project_switch();
                }
                app.invalidate_terminal_workspace(cx);
            });
        })
        .detach();
    }

    pub(super) fn apply_project_list_state(&mut self, next: RuntimeState, cx: &mut Context<Self>) {
        let previous_selected_id = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone());
        self.state.projects = next.projects;
        self.state.selected_project = previous_selected_id
            .as_deref()
            .and_then(|id| {
                self.state
                    .projects
                    .iter()
                    .find(|project| project.id == id)
                    .cloned()
            })
            .or(next.selected_project);
        self.sync_project_list_state(cx);
    }

    pub(super) fn reload_project_open_applications_async(&mut self, cx: &mut Context<Self>) {
        let service = self.runtime_service.clone();
        self.runtime_trace("project-open", "applications_refresh queued");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                service.runtime_trace_frontend("project-open", "applications_refresh start");
                let applications = service.project_open_applications();
                service.runtime_trace_frontend("project-open", "applications_refresh ok");
                applications
            })
            .await;

            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(applications) => {
                        app.project_open_applications = applications;
                    }
                    Err(error) => {
                        app.runtime_trace(
                            "project-open",
                            &format!("applications_refresh failed join_error={error}"),
                        );
                    }
                }
                app.invalidate_project_management(cx);
            });
        })
        .detach();
    }

    pub(super) fn reveal_selected_project_in_file_manager(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to reveal".to_string();
            self.invalidate_project_management(cx);
            return;
        };

        match self
            .runtime_service
            .project_reveal_in_file_manager(&project.path)
        {
            Ok(()) => {
                self.status_message = format!("revealed project: {}", project.name);
            }
            Err(error) => self.status_message = format!("failed to reveal project: {error}"),
        }
        self.invalidate_project_management(cx);
    }

    pub(super) fn reveal_project_in_file_manager(
        &mut self,
        project_name: String,
        project_path: String,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .project_reveal_in_file_manager(&project_path)
        {
            Ok(()) => {
                self.status_message = format!("revealed project: {project_name}");
            }
            Err(error) => {
                let title = self.text("sidebar.project.open_folder", "Open Folder");
                self.status_message = format!("failed to reveal project: {error}");
                self.show_system_error_alert(title, error, cx);
            }
        }
        self.invalidate_project_management(cx);
    }

    pub(super) fn open_selected_project_in_application(
        &mut self,
        application_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to open".to_string();
            self.invalidate_project_management(cx);
            return;
        };

        let application_label = self
            .project_open_applications
            .iter()
            .find(|application| application.id == application_id)
            .map(|application| application.label.clone())
            .unwrap_or_else(|| application_id.clone());

        match self
            .runtime_service
            .project_open_in_application(project.path, application_id)
        {
            Ok(()) => {
                self.status_message = format!("opened {} in {application_label}", project.name);
            }
            Err(error) => {
                self.status_message = format!(
                    "failed to open {} in {application_label}: {error}",
                    project.name
                );
                self.reload_project_open_applications_async(cx);
            }
        }
        self.invalidate_project_management(cx);
    }

    pub(super) fn open_project_folder_from_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = locale_from_language_setting(&self.state.settings.language);
        let request = LocalizedOpenDialogRequest {
            title: translate(&locale, "project.open_folder.title", "Open Folder"),
            message: translate(
                &locale,
                "project.open_folder.message",
                "Choose a project folder to import.",
            ),
            prompt: translate(&locale, "project.open_folder.prompt", "Open"),
            default_path: None,
            filters: Vec::new(),
            directory: true,
            multiple: false,
            can_create_directories: Some(false),
        };
        let runtime_service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.status_message = "opening project folder dialog".to_string();
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                let paths = runtime_service.localized_open_dialog(request)?;
                let Some(paths) = paths else {
                    return Ok(None);
                };
                let Some(path) = paths.first().cloned() else {
                    return Ok(None);
                };
                let name = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or("Project")
                    .to_string();
                let project_id = runtime_service.create_or_select_project(&name, &path)?;
                let state = runtime_service.reload_state();
                Ok(Some((project_id, state)))
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join open project dialog: {error}")));

            let _ = window_handle.update(cx, |_root, window, cx| {
                let _ = this.update(cx, |app, cx| {
                    app.apply_open_project_folder_result(result, window, cx);
                });
            });
        })
        .detach();
    }

    fn apply_open_project_folder_result(
        &mut self,
        result: Result<Option<(String, RuntimeState)>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Some((project_id, state))) => {
                self.state = state;
                let selected_project_id = self
                    .state
                    .selected_project
                    .as_ref()
                    .map(|project| project.id.clone());
                if selected_project_id.as_deref() == Some(project_id.as_str()) {
                    self.select_project_after_state_reload(project_id.clone(), window, cx);
                } else {
                    self.normalize_selected_ai_session();
                    self.normalize_selected_runtime_session();
                    self.normalize_selected_ssh_profile();
                    self.sync_project_list_state(cx);
                }
                self.status_message = format!("project added/selected: {project_id}");
            }
            Ok(None) => {
                self.status_message = "project import canceled".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to choose project folder: {error}");
            }
        }
        self.invalidate_project_management(cx);
    }

    pub(super) fn close_selected_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to close".to_string();
            self.invalidate_project_management(cx);
            return;
        };
        self.remove_project(project, cx);
    }

    pub(super) fn request_remove_project_by_id(
        &mut self,
        project_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self
            .state
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
        else {
            self.status_message = "project not found".to_string();
            self.invalidate_project_management(cx);
            return;
        };

        let title = self.text("project.remove.title", "Remove Project");
        let message = self
            .text(
                "project.remove.confirm_format",
                "Are you sure you want to remove project %@? Files on disk will not be deleted.",
            )
            .replace("%@", &project.name);
        let confirm_label = self.text("common.remove", "Remove");
        let cancel_label = self.text("common.cancel", "Cancel");
        let service = self.runtime_service.clone();
        self.status_message = "waiting for project removal confirmation".to_string();
        self.invalidate_project_management(cx);
        self.invalidate_status_bar(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.localized_confirm_dialog(LocalizedConfirmDialogRequest {
                    title,
                    message,
                    confirm_label,
                    cancel_label,
                })
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| match result {
                Ok(true) => app.remove_project(project, cx),
                Ok(false) => {
                    app.status_message = "project removal canceled".to_string();
                    app.invalidate_project_management(cx);
                    app.invalidate_status_bar(cx);
                }
                Err(error) => {
                    let title = app.text("project.remove.title", "Remove Project");
                    app.status_message = title.clone();
                    app.show_system_error_alert(title, error, cx);
                    app.invalidate_project_management(cx);
                    app.invalidate_status_bar(cx);
                }
            });
        })
        .detach();
    }

    fn remove_project(&mut self, project: ProjectInfo, cx: &mut Context<Self>) {
        let runtime_service = self.runtime_service.clone();
        let project_id = project.id.clone();
        self.runtime_trace(
            "project",
            &format!("remove_project queued project_id={project_id}"),
        );
        self.status_message = format!("closing project: {}", project.name);
        self.invalidate_project_management(cx);
        self.invalidate_status_bar(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                runtime_service.runtime_trace_frontend(
                    "project",
                    &format!("remove_project start project_id={project_id}"),
                );
                let result = runtime_service
                    .close_project(&project_id)
                    .map(|next_project_id| (runtime_service.reload_state(), next_project_id));
                match &result {
                    Ok(_) => runtime_service.runtime_trace_frontend(
                        "project",
                        &format!("remove_project ok project_id={project_id}"),
                    ),
                    Err(error) => runtime_service.runtime_trace_frontend(
                        "project",
                        &format!("remove_project failed project_id={project_id} error={error}"),
                    ),
                }
                result
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join project removal: {error}")));

            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok((state, next_project_id)) => {
                        app.state = state;
                        app.normalize_selected_ai_session();
                        app.normalize_selected_runtime_session();
                        app.normalize_selected_ssh_profile();
                        app.sync_project_list_state(cx);
                        app.status_message = match next_project_id {
                            Some(next_project_id) => {
                                format!("closed {}, selected {next_project_id}", project.name)
                            }
                            None => format!("closed {}, no projects left", project.name),
                        };
                    }
                    Err(error) => {
                        app.status_message = format!("failed to close project: {error}");
                    }
                }
                app.invalidate_project_management(cx);
                app.invalidate_status_bar(cx);
            });
        })
        .detach();
    }

    pub(super) fn reorder_projects_by_ids(
        &mut self,
        project_ids: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        if project_ids.len() != self.state.projects.len()
            || self
                .state
                .projects
                .iter()
                .zip(project_ids.iter())
                .all(|(project, project_id)| project.id == *project_id)
        {
            return;
        }

        let runtime_service = self.runtime_service.clone();
        self.runtime_trace(
            "project",
            &format!("reorder_projects queued count={}", project_ids.len()),
        );
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                runtime_service.runtime_trace_frontend(
                    "project",
                    &format!("reorder_projects start count={}", project_ids.len()),
                );
                let result = runtime_service.project_reorder(ProjectReorderRequest { project_ids });
                match &result {
                    Ok(snapshot) => runtime_service.runtime_trace_frontend(
                        "project",
                        &format!("reorder_projects ok count={}", snapshot.projects.len()),
                    ),
                    Err(error) => runtime_service.runtime_trace_frontend(
                        "project",
                        &format!("reorder_projects failed error={error}"),
                    ),
                }
                result.map(|_| runtime_service.reload_state())
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join project reorder: {error}")));

            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(state) => {
                        app.state = state;
                        app.sync_project_list_state(cx);
                    }
                    Err(error) => {
                        app.status_message = format!("failed to reorder projects: {error}");
                    }
                }
                app.invalidate_project_management(cx);
                app.invalidate_status_bar(cx);
            });
        })
        .detach();
    }

    pub(super) fn edit_project_by_id(
        &mut self,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str())
            != Some(project_id.as_str())
        {
            self.select_project(project_id, window, cx);
        }
        self.open_selected_project_editor_window(window, cx);
    }

    pub(super) fn open_project_create_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = locale_from_language_setting(&self.state.settings.language);
        if Self::activate_child_window(&mut self.project_editor_window, cx) {
            self.status_message = "project creator already opened".to_string();
            self.invalidate_project_management(cx);
            return;
        }

        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::ProjectEditor,
                title: SharedString::from(translate(
                    &locale,
                    "project.create.title",
                    "Create Project",
                )),
                size: size(px(620.0), px(446.0)),
                min_size: size(px(520.0), px(390.0)),
                already_open_message: "project creator already opened",
                opened_message: "project creator opened",
                failed_prefix: "failed to open project creator",
            },
            cx,
            |state, runtime, runtime_service, _window, _cx| {
                CoduxApp::new_project_creator_window_from_state(state, runtime, runtime_service)
            },
            |_view, _window, _cx| {},
        );
        self.invalidate_project_management(cx);
    }

    pub(super) fn open_selected_project_editor_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to edit".to_string();
            self.invalidate_project_management(cx);
            return;
        };
        let locale = locale_from_language_setting(&self.state.settings.language);

        if Self::activate_child_window(&mut self.project_editor_window, cx) {
            self.status_message = "project editor already opened".to_string();
            self.invalidate_project_management(cx);
            return;
        }

        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::ProjectEditor,
                title: SharedString::from(translate(&locale, "project.edit.title", "Edit Project")),
                size: size(px(620.0), px(446.0)),
                min_size: size(px(520.0), px(390.0)),
                already_open_message: "project editor already opened",
                opened_message: "project editor opened",
                failed_prefix: "failed to open project editor",
            },
            cx,
            move |state, runtime, runtime_service, _window, _cx| {
                CoduxApp::new_project_editor_window_from_state(
                    project,
                    state,
                    runtime,
                    runtime_service,
                )
            },
            |_view, _window, _cx| {},
        );
        self.invalidate_project_management(cx);
    }

    pub(super) fn set_project_editor_name(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_name = value;
        self.invalidate_project_management(cx);
    }

    pub(super) fn set_project_editor_path(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_path = clean_dialog_path(&value);
        self.invalidate_project_management(cx);
    }

    pub(super) fn set_project_editor_badge_symbol(
        &mut self,
        value: Option<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_badge_symbol = value;
        self.invalidate_project_management(cx);
    }

    pub(super) fn set_project_editor_badge_color(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_badge_color_hex = value;
        self.invalidate_project_management(cx);
    }

    pub(super) fn choose_project_editor_directory(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = locale_from_language_setting(&self.state.settings.language);
        let default_path = clean_dialog_path(&self.project_editor_path);
        let request = LocalizedOpenDialogRequest {
            title: translate(
                &locale,
                "project.editor.choose_directory.title",
                "Choose Project Directory",
            ),
            message: translate(
                &locale,
                "project.editor.choose_directory.message",
                "Select a folder for this project.",
            ),
            prompt: translate(&locale, "project.editor.choose_directory.prompt", "Choose"),
            default_path: (!default_path.trim().is_empty()).then_some(default_path),
            filters: Vec::new(),
            directory: true,
            multiple: false,
            can_create_directories: Some(false),
        };
        let runtime_service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.status_message = "opening project directory dialog".to_string();
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                runtime_service.localized_open_dialog(request)
            })
            .await
            .unwrap_or_else(|error| {
                Err(format!("failed to join project directory dialog: {error}"))
            });

            let _ = window_handle.update(cx, |_root, _window, cx| {
                let _ = this.update(cx, |app, cx| {
                    app.apply_project_editor_directory_result(result, cx);
                });
            });
        })
        .detach();
    }

    fn apply_project_editor_directory_result(
        &mut self,
        result: Result<Option<Vec<String>>, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Some(paths)) => {
                if let Some(path) = paths.first() {
                    self.project_editor_path = clean_dialog_path(path);
                    self.status_message = "project directory selected".to_string();
                } else {
                    self.status_message = "project directory selection canceled".to_string();
                }
            }
            Ok(None) => self.status_message = "project directory selection canceled".to_string(),
            Err(error) => {
                self.status_message = format!("failed to choose project directory: {error}")
            }
        }
        self.invalidate_project_management(cx);
    }

    pub(super) fn save_project_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.project_editor_saving {
            return;
        }
        let name = self.project_editor_name.trim().to_string();
        let path = clean_dialog_path(&self.project_editor_path);
        if name.is_empty() || path.is_empty() {
            self.status_message = "project name and path are required".to_string();
            self.invalidate_project_management(cx);
            return;
        }

        let project_id = self.project_editor_project_id.clone();
        let badge_symbol = self.project_editor_badge_symbol.clone();
        let badge_color_hex = self.project_editor_badge_color_hex.clone();
        let runtime_service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.project_editor_saving = true;
        self.status_message = if project_id.is_some() {
            format!("saving project: {name}")
        } else {
            format!("creating project: {name}")
        };
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                let save_result = if let Some(project_id) = project_id {
                    runtime_service.project_update(ProjectUpdateRequest {
                        project_id,
                        name: name.clone(),
                        path,
                        badge_text: project_badge_text_from_name(&name),
                        badge_symbol,
                        badge_color_hex: Some(badge_color_hex),
                    })
                } else {
                    runtime_service.project_create(ProjectCreateRequest {
                        name: name.clone(),
                        path,
                        badge_text: project_badge_text_from_name(&name),
                        badge_symbol,
                        badge_color_hex: Some(badge_color_hex),
                    })
                };
                save_result.map(|_| (runtime_service.reload_state(), name))
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join project save: {error}")));

            let _ = window_handle.update(cx, |_root, window, cx| {
                let _ = this.update(cx, |app, cx| {
                    app.project_editor_saving = false;
                    match result {
                        Ok((state, name)) => {
                            let was_editing = app.project_editor_project_id.is_some();
                            app.state = state;
                            app.sync_project_list_state(cx);
                            app.status_message = if was_editing {
                                format!("project saved: {name}")
                            } else {
                                format!("project created: {name}")
                            };
                            publish_child_window_update(ChildWindowUpdateKind::Project);
                            window.remove_window();
                        }
                        Err(error) => {
                            app.status_message = if app.project_editor_project_id.is_some() {
                                format!("failed to save project: {error}")
                            } else {
                                format!("failed to create project: {error}")
                            };
                        }
                    }
                    app.invalidate_project_management(cx);
                });
            });
        })
        .detach();
    }
}

fn clean_dialog_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }
    if let Ok(url) = url::Url::parse(path) {
        if url.scheme() == "file" {
            if let Ok(file_path) = url.to_file_path() {
                return file_path.to_string_lossy().into_owned();
            }
        }
    }
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

pub(super) fn merge_ai_history_summary(
    current: &mut AIHistorySummary,
    incoming: AIHistorySummary,
) -> bool {
    if ai_history_should_replace(current, &incoming) {
        *current = incoming;
        return true;
    }
    if !incoming.indexed {
        current.is_loading = incoming.is_loading;
        current.queued = incoming.queued;
        current.progress = incoming.progress;
        current.detail = incoming.detail;
        current.error = incoming.error;
        return true;
    }
    false
}
