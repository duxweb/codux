use super::*;
use crate::app::app_events::{ChildWindowUpdateKind, publish_child_window_update};
use crate::app::window_actions::{AuxiliaryWindowSlot, AuxiliaryWindowSpec};

const PROJECT_TASK_LOAD_RECENT_SECONDS: f64 = 3.0;
const PROJECT_TASK_LOAD_START_DEBOUNCE_SECONDS: f64 = 1.0;
const WORKTREE_SIDEBAR_LOAD_RECENT_SECONDS: f64 = 3.0;
const WORKTREE_SIDEBAR_LOAD_START_DEBOUNCE_SECONDS: f64 = 1.0;

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
        let Some(store_key) = worktree_view_store_key(&self.state) else {
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
                    (store_key, generation, git, git_review)
                },
            )
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                let Some((store_key, generation, git, git_review)) = result else {
                    if app.project_switch_generation == generation {
                        app.git_review_refreshing = false;
                    }
                    app.invalidate_git_panel(cx);
                    return;
                };
                if app.project_switch_generation != generation
                    || worktree_view_store_key(&app.state).as_ref() != Some(&store_key)
                {
                    if let Some(view_state) = app.worktree_view_store.get_mut(&store_key) {
                        view_state.git.git = git;
                        view_state.git.git_review = git_review;
                    }
                    if app.project_switch_generation == generation {
                        app.git_review_refreshing = false;
                    }
                    app.invalidate_git_panel(cx);
                    return;
                }
                app.state.git = git;
                app.git_review = git_review;
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
                app.save_current_worktree_view_state();
                app.invalidate_git_panel(cx);
                app.invalidate_status_bar(cx);
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
            self.save_current_project_view_state_for_switch();
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
        self.apply_selected_project_shell(&project_id, window, cx);
        let terminal_view_state = worktree_view_store_key(&self.state).and_then(|key| {
            self.worktree_view_store
                .get(&key)
                .map(|state| state.terminal.clone())
        });
        self.memory_manager_scope = "project".to_string();
        self.memory_manager_project_id = Some(project_id.clone());
        if let Some(terminal_view_state) = terminal_view_state {
            self.apply_terminal_layout_from_summary(
                terminal_view_state.terminal_layout,
                terminal_view_state.terminal_runtime,
                cx,
            );
            self.runtime_trace(
                "project-switch",
                &format!(
                    "terminal_restore memory_apply project={} generation={}",
                    project_id, switch_generation
                ),
            );
        } else {
            self.terminal_layout_loading = true;
            self.terminals.clear();
            self.active_terminal_id = 1;
            self.next_terminal_index = 1;
        }
        self.spawn_project_switch_load(project_id, switch_generation, cx);
        self.sync_project_list_store(cx);
        self.invalidate_project_context(cx);
    }

    pub(super) fn apply_selected_project_shell(
        &mut self,
        project_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let started_at = Instant::now();
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
        self.state.git = GitSummary::default();
        self.git_review = GitReviewSummary::default();
        self.state.files.clear();
        if let Some(view_state) = self.project_view_store.get(project_id).cloned() {
            self.state.ai_history = view_state.ai_history;
            self.state.ai_session_detail = None;
            self.state.memory = view_state.memory;
            self.state.memory_manager = view_state.memory_manager;
            self.state.worktrees = view_state.worktrees;
            self.apply_saved_worktree_view_state(cx);
            self.ensure_active_file_editor_state(window, cx);
            self.apply_saved_terminal_view_state();
            self.selected_ai_session_id = None;
            self.runtime_trace(
                "project-switch",
                &format!(
                    "shell memory_hit project={} elapsed_ms={} worktrees={} tasks={} sessions={}",
                    project_id,
                    started_at.elapsed().as_millis(),
                    self.state.worktrees.worktrees.len(),
                    self.state.worktrees.tasks.len(),
                    self.state.ai_history.sessions.len()
                ),
            );
        } else {
            self.selected_ai_session_id = None;
            self.state.ai_history = AIHistorySummary {
                is_loading: true,
                detail: "loading".to_string(),
                ..AIHistorySummary::default()
            };
            self.state.ai_session_detail = None;
            self.state.memory = MemorySummary::default();
            self.state.memory_manager = MemoryManagerSnapshot::default();
            self.state.worktrees = WorktreeSummary::default();
            self.state.terminal_layout = TerminalLayoutSummary::default();
            self.state.terminal_runtime = TerminalRuntimeSummary::default();
            self.state.git = GitSummary::default();
            self.git_review = GitReviewSummary::default();
            self.state.files.clear();
            self.file_editor_tabs.clear();
            self.active_file_editor_tab = None;
            self.runtime_trace(
                "project-switch",
                &format!(
                    "shell memory_miss project={} elapsed_ms={} worktrees=0 tasks=0 sessions=0",
                    project_id,
                    started_at.elapsed().as_millis()
                ),
            );
        }
    }

    pub(super) fn save_current_project_view_state(&mut self) {
        self.save_current_project_view_state_in_memory();
        self.persist_current_worktree_view_state();
    }

    pub(super) fn save_project_view_shell(&mut self) {
        self.save_project_view_shell_in_memory();
        self.persist_current_worktree_view_state();
    }

    pub(super) fn save_current_project_view_state_in_memory(&mut self) {
        self.save_project_view_shell_in_memory();
        self.save_current_terminal_view_state();
    }

    pub(super) fn save_project_view_shell_in_memory(&mut self) {
        let Some(project_id) = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            return;
        };
        self.project_view_store.insert(
            project_id.clone(),
            ProjectViewState {
                ai_history: self.state.ai_history.clone(),
                ai_global_history: self.state.ai_global_history.clone(),
                memory: self.state.memory.clone(),
                memory_manager: self.state.memory_manager.clone(),
                worktrees: self.state.worktrees.clone(),
            },
        );
        self.save_current_worktree_view_state_in_memory();
        self.trace_workspace_state(
            "save_project_shell",
            self.state
                .worktrees
                .selected_worktree_id
                .as_deref()
                .unwrap_or(""),
            &format!(
                "project={} worktrees={} tasks={}",
                project_id,
                self.state.worktrees.worktrees.len(),
                self.state.worktrees.tasks.len()
            ),
        );
    }

    pub(super) fn save_current_project_view_state_for_switch(&mut self) {
        let Some(project_id) = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            return;
        };
        self.project_view_store.insert(
            project_id,
            ProjectViewState {
                ai_history: self.state.ai_history.clone(),
                ai_global_history: self.state.ai_global_history.clone(),
                memory: self.state.memory.clone(),
                memory_manager: self.state.memory_manager.clone(),
                worktrees: self.state.worktrees.clone(),
            },
        );
        self.save_worktree_view_for_switch();
        self.save_current_terminal_view_state();
    }

    pub(super) fn file_worktree_view_state(&self) -> super::app_state::FileWorktreeViewState {
        super::app_state::FileWorktreeViewState {
            files: self.state.files.clone(),
            file_directory: self.file_directory.clone(),
            selected_file_entry: self.selected_file_entry.clone(),
            selected_file_entries: self.selected_file_entries.clone(),
            file_selection_anchor: self.file_selection_anchor.clone(),
            file_tree_expanded_dirs: self.file_tree_expanded_dirs.clone(),
            file_tree_children: self.file_tree_children.clone(),
            file_editor_tabs: self.file_editor_tabs.clone(),
            active_file_editor_tab: self.active_file_editor_tab.clone(),
        }
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

    pub(super) fn apply_file_worktree_view_state(
        &mut self,
        state: super::app_state::FileWorktreeViewState,
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

    pub(super) fn git_worktree_view_state(&self) -> super::app_state::GitWorktreeViewState {
        super::app_state::GitWorktreeViewState {
            git: self.state.git.clone(),
            git_review: self.git_review.clone(),
            selected_git_file: self.selected_git_file.clone(),
            selected_git_files: self.selected_git_files.clone(),
            selected_git_branch: self.selected_git_branch.clone(),
            git_expanded_sections: self.git_expanded_sections.clone(),
            git_expanded_dirs: self.git_expanded_dirs.clone(),
            git_tree_children: self.git_tree_children.clone(),
            git_diff_preview: self.git_diff_preview.clone(),
            git_review_content: self.git_review_content.clone(),
        }
    }

    pub(super) fn apply_git_worktree_view_state(
        &mut self,
        state: super::app_state::GitWorktreeViewState,
    ) {
        self.state.git = state.git;
        self.git_review = state.git_review;
        super::git_actions::merge_git_review_status_files(&mut self.git_review, &self.state.git);
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
    }

    pub(super) fn clear_worktree_view_state(&mut self) {
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
        self.state.git = GitSummary::default();
        self.git_review = GitReviewSummary::default();
        self.normalize_selected_git_branch();
    }

    pub(super) fn save_current_worktree_view_state(&mut self) {
        self.save_current_worktree_view_state_in_memory();
        self.persist_current_worktree_view_state();
    }

    pub(super) fn save_current_worktree_view_state_in_memory(&mut self) {
        let Some(key) = worktree_view_store_key(&self.state) else {
            return;
        };
        self.worktree_view_store.insert(
            key,
            super::app_state::WorktreeViewState {
                files: self.file_worktree_view_state(),
                git: self.git_worktree_view_state(),
                terminal: self.terminal_view_state(),
            },
        );
    }

    pub(super) fn save_worktree_view_for_switch(&mut self) {
        let Some(key) = worktree_view_store_key(&self.state) else {
            return;
        };
        let (files, file_tree_children, git_tree_children, git_review_content, terminal) =
            match self.worktree_view_store.remove(&key) {
                Some(state) => (
                    state.files.files,
                    state.files.file_tree_children,
                    state.git.git_tree_children,
                    state.git.git_review_content,
                    state.terminal,
                ),
                None => (
                    self.state.files.clone(),
                    HashMap::new(),
                    HashMap::new(),
                    None,
                    self.terminal_view_state(),
                ),
            };
        self.worktree_view_store.insert(
            key.clone(),
            super::app_state::WorktreeViewState {
                files: super::app_state::FileWorktreeViewState {
                    files,
                    file_directory: self.file_directory.clone(),
                    selected_file_entry: self.selected_file_entry.clone(),
                    selected_file_entries: self.selected_file_entries.clone(),
                    file_selection_anchor: self.file_selection_anchor.clone(),
                    file_tree_expanded_dirs: self.file_tree_expanded_dirs.clone(),
                    file_tree_children,
                    file_editor_tabs: self.file_editor_tabs.clone(),
                    active_file_editor_tab: self.active_file_editor_tab.clone(),
                },
                git: super::app_state::GitWorktreeViewState {
                    git: self.state.git.clone(),
                    git_review: self.git_review.clone(),
                    selected_git_file: self.selected_git_file.clone(),
                    selected_git_files: self.selected_git_files.clone(),
                    selected_git_branch: self.selected_git_branch.clone(),
                    git_expanded_sections: self.git_expanded_sections.clone(),
                    git_expanded_dirs: self.git_expanded_dirs.clone(),
                    git_tree_children,
                    git_diff_preview: self.git_diff_preview.clone(),
                    git_review_content: git_review_content
                        .or_else(|| self.git_review_content.clone()),
                },
                terminal,
            },
        );
        self.trace_workspace_state(
            "save_worktree_for_switch",
            &key.worktree_id,
            &format!(
                "files={} tabs={} top_panes={} bottom_tabs={}",
                self.state.files.len(),
                self.file_editor_tabs.len(),
                self.state.terminal_layout.top_panes.len(),
                self.state.terminal_layout.tabs.len()
            ),
        );
    }

    pub(super) fn persist_current_worktree_view_state(&self) {
        self.persist_current_file_tree_state();
        self.persist_current_git_ui_state();
    }

    pub(super) fn persist_current_file_tree_state(&self) {
        let Some(key) = worktree_view_store_key(&self.state) else {
            return;
        };
        let summary = codux_runtime::file_tree_state::FileTreeStateSummary {
            files: self.state.files.clone(),
            file_directory: self.file_directory.clone(),
            selected_file_entry: self.selected_file_entry.clone(),
            selected_file_entries: self.selected_file_entries.iter().cloned().collect(),
            file_selection_anchor: self.file_selection_anchor.clone(),
            file_tree_expanded_dirs: self.file_tree_expanded_dirs.iter().cloned().collect(),
            file_tree_children: self.file_tree_children.clone(),
            error: None,
        };
        if let Err(error) = self
            .runtime_service
            .save_file_tree_state(&key.worktree_id, summary)
        {
            self.runtime_trace(
                "config",
                &format!(
                    "failed to persist file tree state {}: {error}",
                    key.worktree_id
                ),
            );
        }
    }

    pub(super) fn persist_current_git_ui_state(&self) {
        let Some(key) = worktree_view_store_key(&self.state) else {
            return;
        };
        let summary =
            super::app_state::git_ui_state_summary_from_worktree(&self.git_worktree_view_state());
        if let Err(error) = self
            .runtime_service
            .save_git_ui_state(&key.worktree_id, summary)
        {
            self.runtime_trace(
                "config",
                &format!(
                    "failed to persist git ui state {}: {error}",
                    key.worktree_id
                ),
            );
        }
    }

    pub(super) fn apply_saved_worktree_view_state(&mut self, cx: &mut Context<Self>) {
        let Some(key) = worktree_view_store_key(&self.state) else {
            self.clear_worktree_view_state();
            return;
        };
        if let Some(view_state) = self.worktree_view_store.get(&key).cloned() {
            self.apply_file_worktree_view_state(view_state.files);
            self.apply_git_worktree_view_state(view_state.git);
            self.apply_terminal_view_state(view_state.terminal);
            if self.file_editor_tabs.is_empty() {
                self.load_current_file_editor_layout_async(cx);
            }
        } else {
            self.clear_worktree_view_state();
            self.load_current_file_editor_layout_async(cx);
        }
    }

    pub(super) fn spawn_worktree_sidebar_load(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some(store_key) = worktree_view_store_key(&self.state) else {
            return;
        };
        if self.worktree_view_store.contains_key(&store_key) {
            return;
        }
        let Some(worktree_path) = self.selected_worktree_path() else {
            return;
        };
        if self.worktree_sidebar_load_busy_or_recent(&store_key) {
            self.record_ui_scheduler_event(
                "skip_busy",
                &worktree_sidebar_load_scheduler_key(&store_key),
            );
            return;
        }
        if !self.begin_scheduled_work(
            worktree_sidebar_load_scheduler_key(&store_key),
            ScheduledWorkPolicy::new(
                WORKTREE_SIDEBAR_LOAD_RECENT_SECONDS,
                WORKTREE_SIDEBAR_LOAD_START_DEBOUNCE_SECONDS,
            ),
        ) {
            return;
        }
        let runtime_service = self.runtime_service.clone();
        let cleanup_store_key = store_key.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let load = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    let files = runtime_service.reload_project_files(&worktree_path, None);
                    let file_editor_layout =
                        runtime_service.reload_file_editor_layout(Some(&store_key.worktree_id));
                    let git = runtime_service.reload_project_git(&worktree_path);
                    let mut git_review =
                        runtime_service.reload_project_git_review(&worktree_path, None);
                    super::git_actions::merge_git_review_status_files(&mut git_review, &git);
                    WorktreeSidebarLoad {
                        generation,
                        store_key,
                        files,
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
                } else {
                    app.finish_worktree_sidebar_load(&cleanup_store_key);
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
        self.finish_worktree_sidebar_load(&load.store_key);
        let mut file_state = self
            .worktree_view_store
            .get(&load.store_key)
            .map(|state| state.files.clone())
            .unwrap_or_else(|| super::app_state::FileWorktreeViewState {
                files: Vec::new(),
                file_directory: String::new(),
                selected_file_entry: None,
                selected_file_entries: HashSet::new(),
                file_selection_anchor: None,
                file_tree_expanded_dirs: HashSet::new(),
                file_tree_children: HashMap::new(),
                file_editor_tabs: Vec::new(),
                active_file_editor_tab: None,
            });
        file_state.files = load.files.clone();
        if file_state.file_directory.trim().is_empty() {
            file_state.file_directory.clear();
        }
        if file_state.file_editor_tabs.is_empty() {
            let (tabs, active_path) =
                super::app_state::file_editor_tabs_from_layout(load.file_editor_layout);
            file_state.file_editor_tabs = tabs;
            file_state.active_file_editor_tab = active_path;
        }
        let git_state = super::app_state::GitWorktreeViewState {
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
        self.worktree_view_store.insert(
            load.store_key.clone(),
            super::app_state::WorktreeViewState {
                files: file_state.clone(),
                git: git_state.clone(),
                terminal: self
                    .worktree_view_store
                    .get(&load.store_key)
                    .map(|state| state.terminal.clone())
                    .unwrap_or_else(|| self.terminal_view_state()),
            },
        );
        let current_key = worktree_view_store_key(&self.state);
        if self.project_switch_generation != load.generation
            || current_key.as_ref() != Some(&load.store_key)
        {
            return;
        }
        self.apply_file_worktree_view_state(file_state);
        self.ensure_active_file_editor_state(window, cx);
        self.apply_git_worktree_view_state(git_state);
        if self.workspace_view == WorkspaceView::Review {
            self.ensure_selected_git_review_file_loaded_async(cx);
        }
        self.invalidate_file_panel(cx);
        self.invalidate_git_panel(cx);
    }

    pub(super) fn apply_saved_terminal_view_state(&mut self) {
        let Some(key) = worktree_view_store_key(&self.state) else {
            self.state.terminal_layout = TerminalLayoutSummary::default();
            self.state.terminal_runtime = TerminalRuntimeSummary::default();
            return;
        };
        if let Some(view_state) = self.worktree_view_store.get(&key).cloned() {
            self.apply_terminal_view_state(view_state.terminal);
        } else {
            self.state.terminal_layout = TerminalLayoutSummary::default();
            self.state.terminal_runtime = TerminalRuntimeSummary::default();
        }
    }

    pub(super) fn save_current_terminal_view_state(&mut self) {
        let Some(key) = worktree_view_store_key(&self.state) else {
            return;
        };
        let terminal = self.terminal_view_state();
        if let Some(view_state) = self.worktree_view_store.get_mut(&key) {
            view_state.terminal = terminal;
        } else {
            self.worktree_view_store.insert(
                key,
                super::app_state::WorktreeViewState {
                    files: self.file_worktree_view_state(),
                    git: self.git_worktree_view_state(),
                    terminal,
                },
            );
        }
    }

    pub(super) fn terminal_view_state(&self) -> TerminalViewState {
        TerminalViewState {
            terminal_layout: self.state.terminal_layout.clone(),
            terminal_runtime: self.state.terminal_runtime.clone(),
        }
    }

    pub(super) fn apply_terminal_view_state(&mut self, state: TerminalViewState) {
        self.state.terminal_layout = state.terminal_layout;
        self.state.terminal_runtime = state.terminal_runtime;
    }

    pub(super) fn upsert_worktree_terminal_view_state(
        &mut self,
        key: super::app_state::WorktreeViewStoreKey,
        terminal: TerminalViewState,
    ) {
        let top_panes = terminal.terminal_layout.top_panes.len();
        let bottom_tabs = terminal.terminal_layout.tabs.len();
        let sessions = terminal.terminal_runtime.sessions.len();
        if let Some(view_state) = self.worktree_view_store.get_mut(&key) {
            view_state.terminal = terminal;
            self.trace_workspace_state(
                "upsert_terminal_memory_hit",
                &key.worktree_id,
                &format!("top_panes={top_panes} bottom_tabs={bottom_tabs} sessions={sessions}"),
            );
            return;
        }
        self.worktree_view_store.insert(
            key.clone(),
            super::app_state::WorktreeViewState {
                files: self.file_worktree_view_state(),
                git: self.git_worktree_view_state(),
                terminal,
            },
        );
        self.trace_workspace_state(
            "upsert_terminal_created",
            &key.worktree_id,
            &format!("top_panes={top_panes} bottom_tabs={bottom_tabs} sessions={sessions}"),
        );
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

    pub(super) fn should_skip_scheduled_project_activity_tick(&self) -> bool {
        let project_busy = self
            .state
            .selected_project
            .as_ref()
            .is_some_and(|project| self.project_task_load_busy_or_recent(&project.id));
        let worktree_busy = worktree_view_store_key(&self.state)
            .as_ref()
            .is_some_and(|key| self.worktree_sidebar_load_busy_or_recent(key));
        project_busy || worktree_busy
    }

    fn project_task_load_busy_or_recent(&self, project_id: &str) -> bool {
        let scheduler_key = project_task_load_scheduler_key(project_id);
        self.scheduled_work_busy_or_recent(
            &scheduler_key,
            ScheduledWorkPolicy::new(
                PROJECT_TASK_LOAD_RECENT_SECONDS,
                PROJECT_TASK_LOAD_START_DEBOUNCE_SECONDS,
            ),
        )
    }

    fn begin_project_task_load(&mut self, project_id: &str) -> bool {
        if self.project_task_load_busy_or_recent(project_id) {
            self.record_ui_scheduler_event(
                "skip_busy",
                &project_task_load_scheduler_key(project_id),
            );
            return false;
        }
        if !self.begin_scheduled_work(
            project_task_load_scheduler_key(project_id),
            ScheduledWorkPolicy::new(
                PROJECT_TASK_LOAD_RECENT_SECONDS,
                PROJECT_TASK_LOAD_START_DEBOUNCE_SECONDS,
            ),
        ) {
            return false;
        }
        true
    }

    fn finish_project_task_load(&mut self, project_id: &str) {
        self.finish_scheduled_work(&project_task_load_scheduler_key(project_id));
    }

    fn worktree_sidebar_load_busy_or_recent(
        &self,
        key: &super::app_state::WorktreeViewStoreKey,
    ) -> bool {
        let scheduler_key = worktree_sidebar_load_scheduler_key(key);
        self.scheduled_work_busy_or_recent(
            &scheduler_key,
            ScheduledWorkPolicy::new(
                WORKTREE_SIDEBAR_LOAD_RECENT_SECONDS,
                WORKTREE_SIDEBAR_LOAD_START_DEBOUNCE_SECONDS,
            ),
        )
    }

    fn finish_worktree_sidebar_load(&mut self, key: &super::app_state::WorktreeViewStoreKey) {
        self.finish_scheduled_work(&worktree_sidebar_load_scheduler_key(key));
    }

    pub(super) fn spawn_persist_terminal_layout(&mut self, _cx: &mut Context<Self>) {
        let Some(owner_id) =
            super::ai_runtime_status::current_terminal_layout_storage_key(&self.state)
        else {
            return;
        };
        self.refresh_terminal_slot_snapshots();
        let layout_snapshot = self.terminal_layout_snapshot();
        self.spawn_persist_terminal_layout_snapshot(Some(owner_id), layout_snapshot);
    }

    pub(super) fn spawn_persist_terminal_layout_snapshot(
        &self,
        owner_id: Option<String>,
        layout_snapshot: (
            Vec<TerminalTabSummary>,
            String,
            Vec<TerminalPaneSummary>,
            String,
        ),
    ) {
        let Some(owner_id) = owner_id else {
            return;
        };
        let (tabs, active_tab_id, top_panes, active_slot_id) = layout_snapshot;
        let runtime_service = self.runtime_service.clone();
        codux_runtime::async_runtime::spawn_blocking(move || {
            if let Err(error) = runtime_service.save_terminal_layout(
                &owner_id,
                tabs,
                active_tab_id,
                top_panes,
                active_slot_id,
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
        let stats_runtime_service = runtime_service.clone();
        let stats_project = project.clone();
        let should_load_tasks = self.begin_project_task_load(&project_id);
        self.runtime_trace(
            "project-switch",
            &format!(
                "spawn_loads project={} generation={} should_load_tasks={}",
                project_id, generation, should_load_tasks
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
                    let store_key = WorktreeViewStoreKey {
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
                        store_key,
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

        if should_load_tasks {
            let task_project_id = task_project.id.clone();
            let task_project_path = task_project.path.clone();
            let cleanup_project_id = task_project_id.clone();
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
                        let worktrees = task_runtime_service
                            .reload_worktrees(Some(&task_project_id), Some(&task_project_path));
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
                    } else {
                        app.finish_project_task_load(&cleanup_project_id);
                    }
                });
            })
            .detach();
        }

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
        let existing_view_state = self
            .worktree_view_store
            .get(&load.store_key)
            .map(|state| state.terminal.clone());
        let view_state = existing_view_state
            .clone()
            .unwrap_or_else(|| TerminalViewState {
                terminal_layout: load.terminal_layout.clone(),
                terminal_runtime: load.terminal_runtime.clone(),
            });
        self.upsert_worktree_terminal_view_state(load.store_key.clone(), view_state.clone());
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
        let selected_terminal_key = worktree_view_store_key(&self.state);
        let is_selected_terminal_owner = selected_terminal_key
            .as_ref()
            .is_some_and(|key| key == &load.store_key);
        let has_selected_terminal_view = selected_terminal_key
            .as_ref()
            .is_some_and(|key| self.worktree_view_store.contains_key(key));
        self.runtime_trace(
            "project-switch",
            &format!(
                "terminal_load apply project={} worktree={} generation={} selected={} saved={}",
                load.project_id,
                load.store_key.worktree_id,
                load.generation,
                is_selected_terminal_owner,
                has_selected_terminal_view
            ),
        );
        if !is_selected_terminal_owner {
            self.runtime_trace(
                "project-switch",
                &format!(
                    "terminal_load memory_keep project={} generation={}",
                    load.project_id, load.generation
                ),
            );
        } else if existing_view_state.is_some() && !self.terminals.is_empty() {
            self.runtime_trace(
                "project-switch",
                &format!(
                    "terminal_load memory_skip_restore project={} generation={}",
                    load.project_id, load.generation
                ),
            );
        } else {
            self.schedule_terminal_layout_restore(
                view_state.terminal_layout,
                view_state.terminal_runtime,
                load.generation,
                window,
                cx,
            );
        }
        self.save_current_project_view_state_in_memory();
        self.invalidate_terminal_workspace(cx);
    }

    pub(super) fn apply_project_switch_task_load(
        &mut self,
        load: ProjectSwitchTaskLoad,
        cx: &mut Context<Self>,
    ) {
        self.finish_project_task_load(&load.project_id);
        let existing = self.project_view_store.get(&load.project_id).cloned();
        self.project_view_store.insert(
            load.project_id.clone(),
            ProjectViewState {
                ai_history: existing
                    .as_ref()
                    .map(|state| state.ai_history.clone())
                    .unwrap_or_else(|| self.state.ai_history.clone()),
                ai_global_history: existing
                    .as_ref()
                    .map(|state| state.ai_global_history.clone())
                    .unwrap_or_else(|| self.state.ai_global_history.clone()),
                memory: existing
                    .as_ref()
                    .map(|state| state.memory.clone())
                    .unwrap_or_else(|| self.state.memory.clone()),
                memory_manager: existing
                    .as_ref()
                    .map(|state| state.memory_manager.clone())
                    .unwrap_or_else(|| self.state.memory_manager.clone()),
                worktrees: load.worktrees.clone(),
            },
        );
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
        self.apply_saved_worktree_view_state(cx);
        self.spawn_worktree_sidebar_load(load.generation, cx);
        self.apply_saved_terminal_view_state();
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
        self.save_current_project_view_state_in_memory();
        self.invalidate_worktree_context(cx);
    }

    pub(super) fn apply_project_switch_primary_load(
        &mut self,
        load: ProjectSwitchPrimaryLoad,
        cx: &mut Context<Self>,
    ) {
        let existing = self.project_view_store.get(&load.project_id).cloned();
        let store_ai_history = existing
            .as_ref()
            .map(|state| {
                if ai_history_should_replace(&state.ai_history, &load.ai_history) {
                    load.ai_history.clone()
                } else {
                    state.ai_history.clone()
                }
            })
            .unwrap_or_else(|| load.ai_history.clone());
        self.project_view_store.insert(
            load.project_id.clone(),
            ProjectViewState {
                ai_history: store_ai_history,
                ai_global_history: existing
                    .as_ref()
                    .map(|state| state.ai_global_history.clone())
                    .unwrap_or_else(|| self.state.ai_global_history.clone()),
                memory: existing
                    .as_ref()
                    .map(|state| state.memory.clone())
                    .unwrap_or_else(|| self.state.memory.clone()),
                memory_manager: existing
                    .as_ref()
                    .map(|state| state.memory_manager.clone())
                    .unwrap_or_else(|| self.state.memory_manager.clone()),
                worktrees: existing
                    .as_ref()
                    .map(|state| state.worktrees.clone())
                    .unwrap_or_else(|| self.state.worktrees.clone()),
            },
        );
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
        let replaced = ai_history_should_replace(&self.state.ai_history, &load.ai_history);
        if replaced {
            self.state.ai_history = load.ai_history;
            self.selected_ai_session_id = None;
            self.state.ai_session_detail = None;
        } else if !load.ai_history.indexed {
            self.state.ai_history.is_loading = load.ai_history.is_loading;
            self.state.ai_history.queued = load.ai_history.queued;
            self.state.ai_history.progress = load.ai_history.progress;
            self.state.ai_history.detail = load.ai_history.detail;
            self.state.ai_history.error = load.ai_history.error;
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
        self.refresh_ai_history_after_project_switch(cx);
        self.save_current_project_view_state_in_memory();
        self.invalidate_ui(
            cx,
            [
                UiRegion::TaskColumn,
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceAssistant,
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
        let entry = self
            .project_view_store
            .entry(load.project_id.clone())
            .or_insert_with(|| ProjectViewState {
                ai_history: self.state.ai_history.clone(),
                ai_global_history: self.state.ai_global_history.clone(),
                memory: self.state.memory.clone(),
                memory_manager: self.state.memory_manager.clone(),
                worktrees: self.state.worktrees.clone(),
            });
        entry.ai_global_history = load.ai_global_history.clone();
        entry.memory = load.memory.clone();
        entry.memory_manager = load.memory_manager.clone();
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
        self.save_current_project_view_state_in_memory();
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceChrome,
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
        terminal_layout: TerminalLayoutSummary,
        terminal_runtime: TerminalRuntimeSummary,
        generation: u64,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.terminal_layout = terminal_layout.clone();
        self.state.terminal_runtime = terminal_runtime.clone();
        self.terminal_layout_loading = true;
        let scheduled_at = Instant::now();
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_millis(16)).await;
            let _ = this.update(cx, |app, cx| {
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
                app.apply_terminal_layout_from_summary(terminal_layout, terminal_runtime, cx);
            });
        })
        .detach();
    }

    pub(super) fn reload_runtime_state(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state = self.runtime_service.reload_state();
        self.project_open_applications = self.runtime_service.project_open_applications();
        self.file_directory.clear();
        self.reset_file_tree_state();
        self.file_editable = false;
        self.file_dirty = false;
        self.clear_file_selection();
        self.selected_git_file = None;
        self.normalize_selected_git_branch();
        self.git_diff_preview = "select a changed file to preview its diff".to_string();
        self.clear_git_review_derived_content();
        self.normalize_selected_ai_session();
        self.normalize_selected_runtime_session();
        self.normalize_selected_ssh_profile();
        self.status_message = "state reloaded from Codux support files".to_string();
        self.sync_project_list_store(cx);
        self.invalidate_project_management(cx);
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
        self.sync_project_list_store(cx);
    }

    pub(super) fn reload_project_open_applications(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_open_applications = self.runtime_service.project_open_applications();
        self.status_message = "project application list refreshed".to_string();
        self.invalidate_project_management(cx);
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
                self.project_open_applications = self.runtime_service.project_open_applications();
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
                    self.sync_project_list_store(cx);
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
        match self.runtime_service.close_project(&project.id) {
            Ok(next_project_id) => {
                self.project_view_store.remove(&project.id);
                self.worktree_view_store
                    .retain(|key, _| key.project_id != project.id);
                self.state = self.runtime_service.reload_state();
                self.normalize_selected_ai_session();
                self.normalize_selected_runtime_session();
                self.normalize_selected_ssh_profile();
                self.sync_project_list_store(cx);
                self.status_message = match next_project_id {
                    Some(next_project_id) => {
                        format!("closed {}, selected {next_project_id}", project.name)
                    }
                    None => format!("closed {}, no projects left", project.name),
                };
            }
            Err(error) => self.status_message = format!("failed to close project: {error}"),
        }
        self.invalidate_project_management(cx);
        self.invalidate_status_bar(cx);
    }

    pub(super) fn close_all_projects(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.projects.is_empty() {
            self.status_message = "no projects to close".to_string();
            self.invalidate_project_management(cx);
            return;
        }
        let closed = self.state.projects.len();
        match self.runtime_service.project_close_all() {
            Ok(_snapshot) => {
                self.project_view_store.clear();
                self.worktree_view_store.clear();
                self.state = self.runtime_service.reload_state();
                self.clear_file_selection();
                self.file_tree_expanded_dirs.clear();
                self.file_tree_children.clear();
                self.record_ui_state_clear("file_tree");
                self.file_preview = "select a file to preview it".to_string();
                self.file_editable = false;
                self.file_dirty = false;
                self.selected_git_file = None;
                self.git_tree_children.clear();
                self.git_expanded_dirs.clear();
                self.record_ui_state_clear("git_tree");
                self.git_diff_preview = "select a changed file to preview its diff".to_string();
                self.clear_git_review_derived_content();
                self.normalize_selected_ai_session();
                self.normalize_selected_runtime_session();
                self.normalize_selected_ssh_profile();
                self.sync_project_list_store(cx);
                self.status_message = format!(
                    "closed {closed} project{}",
                    if closed == 1 { "" } else { "s" }
                );
            }
            Err(error) => self.status_message = format!("failed to close projects: {error}"),
        }
        self.invalidate_project_management(cx);
    }

    pub(super) fn rename_selected_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_selected_project_editor_window(_window, cx);
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
        let name = self.project_editor_name.trim().to_string();
        let path = clean_dialog_path(&self.project_editor_path);
        if name.is_empty() || path.is_empty() {
            self.status_message = "project name and path are required".to_string();
            self.invalidate_project_management(cx);
            return;
        }

        if let Some(project_id) = self.project_editor_project_id.clone() {
            match self.runtime_service.project_update(ProjectUpdateRequest {
                project_id,
                name: name.clone(),
                path,
                badge_text: project_badge_text_from_name(&name),
                badge_symbol: self.project_editor_badge_symbol.clone(),
                badge_color_hex: Some(self.project_editor_badge_color_hex.clone()),
            }) {
                Ok(_snapshot) => {
                    self.state = self.runtime_service.reload_state();
                    self.sync_project_list_store(cx);
                    self.status_message = format!("project saved: {name}");
                    publish_child_window_update(ChildWindowUpdateKind::Project);
                    window.remove_window();
                }
                Err(error) => self.status_message = format!("failed to save project: {error}"),
            }
        } else {
            match self.runtime_service.project_create(ProjectCreateRequest {
                name: name.clone(),
                path,
                badge_text: project_badge_text_from_name(&name),
                badge_symbol: self.project_editor_badge_symbol.clone(),
                badge_color_hex: Some(self.project_editor_badge_color_hex.clone()),
            }) {
                Ok(_snapshot) => {
                    self.state = self.runtime_service.reload_state();
                    self.sync_project_list_store(cx);
                    self.status_message = format!("project created: {name}");
                    publish_child_window_update(ChildWindowUpdateKind::Project);
                    window.remove_window();
                }
                Err(error) => self.status_message = format!("failed to create project: {error}"),
            }
        }
        self.invalidate_project_management(cx);
    }

    pub(super) fn move_selected_project_up(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to move".to_string();
            self.invalidate_project_management(cx);
            return;
        };
        match self.runtime_service.move_project_up(&project.id) {
            Ok(()) => {
                self.state = self.runtime_service.reload_state();
                self.normalize_selected_ai_session();
                self.normalize_selected_runtime_session();
                self.normalize_selected_ssh_profile();
                self.sync_project_list_store(cx);
                self.status_message = format!("moved project up: {}", project.name);
            }
            Err(error) => self.status_message = format!("failed to move project: {error}"),
        }
        self.invalidate_project_management(cx);
    }

    pub(super) fn move_selected_project_down(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to move".to_string();
            self.invalidate_project_management(cx);
            return;
        };
        match self.runtime_service.move_project_down(&project.id) {
            Ok(()) => {
                self.state = self.runtime_service.reload_state();
                self.normalize_selected_ai_session();
                self.normalize_selected_runtime_session();
                self.normalize_selected_ssh_profile();
                self.sync_project_list_store(cx);
                self.status_message = format!("moved project down: {}", project.name);
            }
            Err(error) => self.status_message = format!("failed to move project: {error}"),
        }
        self.invalidate_project_management(cx);
    }
}

fn project_task_load_scheduler_key(project_id: &str) -> String {
    format!("project_task:{project_id}")
}

fn worktree_sidebar_load_scheduler_key(key: &super::app_state::WorktreeViewStoreKey) -> String {
    format!("worktree_sidebar:{}:{}", key.project_id, key.worktree_id)
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
