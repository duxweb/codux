use super::*;

const PROJECT_TASK_LOAD_RECENT_SECONDS: f64 = 3.0;
const PROJECT_TASK_LOAD_START_DEBOUNCE_SECONDS: f64 = 1.0;
const WORKTREE_SIDEBAR_LOAD_RECENT_SECONDS: f64 = 3.0;
const WORKTREE_SIDEBAR_LOAD_START_DEBOUNCE_SECONDS: f64 = 1.0;

impl CoduxApp {
    pub(super) fn refresh_task_column_async(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to refresh".to_string();
            cx.notify();
            return;
        };
        if self.task_column_refreshing {
            return;
        }

        self.task_column_refreshing = true;
        self.status_message = format!("refreshing tasks for {}", project.name);
        self.notify_task_column(cx);
        cx.notify();

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
                let ai_history = runtime_service.reload_project_ai_history(&request.path);
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
                app.notify_task_column(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn refresh_files_panel_state(&mut self) {
        let Some(project_path) = self.selected_worktree_path() else {
            return;
        };
        self.state.files = self
            .runtime_service
            .reload_project_files(&project_path, file_directory_option(&self.file_directory));
        self.refresh_file_tree_cache();
        self.normalize_selected_file_entry();
        self.save_current_worktree_view_state();
    }

    pub(super) fn refresh_git_panel_state(&mut self) {
        let Some(project_path) = self.selected_worktree_path() else {
            return;
        };
        self.state.git = self.runtime_service.reload_project_git(&project_path);
        self.refresh_git_review_for_project(&project_path);
        self.normalize_selected_git_file();
        self.normalize_selected_git_branch();
        self.save_current_worktree_view_state();
    }

    pub(super) fn select_project(
        &mut self,
        project_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            self.save_current_project_view_state();
            self.spawn_persist_terminal_state(cx);
        }
        match self.runtime_service.select_project(&project_id) {
            Ok(()) => self.status_message = "selected project saved to state.json".to_string(),
            Err(error) => self.status_message = format!("selected in memory only: {error}"),
        }
        self.project_switch_generation = self.project_switch_generation.wrapping_add(1);
        let switch_generation = self.project_switch_generation;
        self.apply_selected_project_shell(&project_id);
        let terminal_view_state = terminal_view_store_key(&self.state)
            .and_then(|key| self.terminal_view_store.get(&key).cloned());
        self.memory_manager_scope = "project".to_string();
        self.memory_manager_project_id = Some(project_id.clone());
        if let Some(terminal_view_state) = terminal_view_state {
            self.apply_terminal_layout_from_summary(
                terminal_view_state.terminal_layout,
                terminal_view_state.terminal_runtime,
                cx,
            );
        } else {
            self.terminal_layout_loading = true;
            self.terminals.clear();
            self.active_terminal_id = 1;
            self.next_terminal_index = 1;
        }
        self.notify_task_column(cx);
        self.spawn_project_switch_load(project_id, switch_generation, cx);
        self.sync_project_list_store(cx);
        cx.notify();
    }

    pub(super) fn apply_selected_project_shell(&mut self, project_id: &str) {
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
            self.apply_saved_worktree_view_state();
            self.apply_saved_terminal_view_state();
            self.selected_ai_session_id = None;
            self.runtime_trace(
                "project-switch",
                &format!(
                    "shell memory_hit project={} worktrees={} tasks={} sessions={}",
                    project_id,
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
            self.runtime_trace(
                "project-switch",
                &format!(
                    "shell memory_miss project={} worktrees=0 tasks=0 sessions=0",
                    project_id
                ),
            );
        }
    }

    pub(super) fn save_current_project_view_state(&mut self) {
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
        self.save_current_worktree_view_state();
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
        self.selected_git_file = state.selected_git_file;
        self.selected_git_files = state.selected_git_files;
        self.selected_git_branch = state.selected_git_branch;
        self.git_expanded_sections = state.git_expanded_sections;
        self.git_expanded_dirs = state.git_expanded_dirs;
        self.git_tree_children = state.git_tree_children;
        self.git_diff_preview = state.git_diff_preview;
        self.git_review_content = state.git_review_content;
        self.normalize_selected_git_file();
        self.normalize_selected_git_branch();
    }

    pub(super) fn clear_worktree_view_state(&mut self) {
        self.file_directory.clear();
        self.reset_file_tree_cache();
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
        self.git_diff_preview = "select a changed file to preview its diff".to_string();
        self.git_review_content = None;
        self.state.git = GitSummary::default();
        self.git_review = GitReviewSummary::default();
        self.normalize_selected_git_branch();
    }

    pub(super) fn save_current_worktree_view_state(&mut self) {
        let Some(key) = worktree_view_store_key(&self.state) else {
            return;
        };
        self.worktree_view_store.insert(
            key,
            super::app_state::WorktreeViewState {
                files: self.file_worktree_view_state(),
                git: self.git_worktree_view_state(),
            },
        );
    }

    pub(super) fn apply_saved_worktree_view_state(&mut self) {
        let Some(key) = worktree_view_store_key(&self.state) else {
            self.clear_worktree_view_state();
            return;
        };
        if let Some(view_state) = self.worktree_view_store.get(&key).cloned() {
            self.apply_file_worktree_view_state(view_state.files);
            self.apply_git_worktree_view_state(view_state.git);
            if self.file_editor_tabs.is_empty() {
                self.load_current_file_editor_layout();
            }
        } else {
            self.clear_worktree_view_state();
            self.load_current_file_editor_layout();
        }
    }

    pub(super) fn spawn_worktree_sidebar_load(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some(store_key) = worktree_view_store_key(&self.state) else {
            return;
        };
        if self.worktree_view_store.contains_key(&store_key) {
            return;
        }
        let now = app_now_seconds();
        if self.worktree_sidebar_load_busy_or_recent(&store_key, now) {
            return;
        }
        let Some(worktree_path) = self.selected_worktree_path() else {
            return;
        };
        self.worktree_sidebar_load_in_flight
            .insert(store_key.clone());
        self.worktree_sidebar_load_last_started_at
            .insert(store_key.clone(), now);
        let runtime_service = self.runtime_service.clone();
        let cleanup_store_key = store_key.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let load = codux_runtime::async_runtime::run_limited_blocking(move || {
                let files = runtime_service.reload_project_files(&worktree_path, None);
                let file_editor_layout =
                    runtime_service.reload_file_editor_layout(Some(&store_key.worktree_id));
                let git = runtime_service.reload_project_git(&worktree_path);
                let git_review = runtime_service.reload_project_git_review(&worktree_path, None);
                WorktreeSidebarLoad {
                    generation,
                    store_key,
                    files,
                    file_editor_layout,
                    git,
                    git_review,
                }
            })
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                if let Some(load) = load {
                    app.apply_worktree_sidebar_load(load, cx);
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
        cx: &mut Context<Self>,
    ) {
        self.worktree_sidebar_load_in_flight.remove(&load.store_key);
        self.worktree_sidebar_load_last_finished_at
            .insert(load.store_key.clone(), app_now_seconds());
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
        if file_state.file_editor_tabs.is_empty() {
            file_state.file_editor_tabs = load
                .file_editor_layout
                .tabs
                .into_iter()
                .map(|tab| super::app_state::FileEditorTab {
                    label: tab.label,
                    relative_path: tab.path,
                    editable: true,
                    dirty: false,
                    language: if tab.language.trim().is_empty() {
                        "text".to_string()
                    } else {
                        tab.language
                    },
                })
                .collect();
            file_state.active_file_editor_tab = load
                .file_editor_layout
                .active_path
                .filter(|active| {
                    file_state
                        .file_editor_tabs
                        .iter()
                        .any(|tab| tab.relative_path == *active)
                })
                .or_else(|| {
                    file_state
                        .file_editor_tabs
                        .first()
                        .map(|tab| tab.relative_path.clone())
                });
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
            },
        );
        let current_key = worktree_view_store_key(&self.state);
        if self.project_switch_generation != load.generation
            || current_key.as_ref() != Some(&load.store_key)
        {
            return;
        }
        self.apply_file_worktree_view_state(file_state);
        self.apply_git_worktree_view_state(git_state);
        cx.notify();
    }

    pub(super) fn apply_saved_terminal_view_state(&mut self) {
        let Some(key) = terminal_view_store_key(&self.state) else {
            self.state.terminal_layout = TerminalLayoutSummary::default();
            self.state.terminal_runtime = TerminalRuntimeSummary::default();
            return;
        };
        if let Some(view_state) = self.terminal_view_store.get(&key).cloned() {
            self.state.terminal_layout = view_state.terminal_layout;
            self.state.terminal_runtime = view_state.terminal_runtime;
        } else {
            self.state.terminal_layout = TerminalLayoutSummary::default();
            self.state.terminal_runtime = TerminalRuntimeSummary::default();
        }
    }

    pub(super) fn save_current_terminal_view_state(&mut self) {
        let Some(key) = terminal_view_store_key(&self.state) else {
            return;
        };
        self.terminal_view_store.insert(
            key,
            TerminalViewState {
                terminal_layout: self.state.terminal_layout.clone(),
                terminal_runtime: self.state.terminal_runtime.clone(),
            },
        );
    }

    pub(super) fn notify_task_column(&mut self, cx: &mut Context<Self>) {
        self.invalidate_task_column(cx);
    }

    pub(super) fn should_skip_scheduled_project_activity_tick(&self) -> bool {
        let now = app_now_seconds();
        let project_busy = self
            .state
            .selected_project
            .as_ref()
            .is_some_and(|project| self.project_task_load_busy_or_recent(&project.id, now));
        let worktree_busy = worktree_view_store_key(&self.state)
            .as_ref()
            .is_some_and(|key| self.worktree_sidebar_load_busy_or_recent(key, now));
        project_busy || worktree_busy
    }

    fn project_task_load_busy_or_recent(&self, project_id: &str, now: f64) -> bool {
        self.project_task_load_in_flight.contains(project_id)
            || self
                .project_task_load_last_finished_at
                .get(project_id)
                .is_some_and(|finished| now - finished < PROJECT_TASK_LOAD_RECENT_SECONDS)
            || self
                .project_task_load_last_started_at
                .get(project_id)
                .is_some_and(|started| now - started < PROJECT_TASK_LOAD_START_DEBOUNCE_SECONDS)
    }

    fn begin_project_task_load(&mut self, project_id: &str) -> bool {
        let now = app_now_seconds();
        if self.project_task_load_busy_or_recent(project_id, now) {
            return false;
        }
        self.project_task_load_in_flight
            .insert(project_id.to_string());
        self.project_task_load_last_started_at
            .insert(project_id.to_string(), now);
        true
    }

    fn finish_project_task_load(&mut self, project_id: &str) {
        self.project_task_load_in_flight.remove(project_id);
        self.project_task_load_last_finished_at
            .insert(project_id.to_string(), app_now_seconds());
    }

    fn worktree_sidebar_load_busy_or_recent(
        &self,
        key: &super::app_state::WorktreeViewStoreKey,
        now: f64,
    ) -> bool {
        self.worktree_sidebar_load_in_flight.contains(key)
            || self
                .worktree_sidebar_load_last_finished_at
                .get(key)
                .is_some_and(|finished| now - finished < WORKTREE_SIDEBAR_LOAD_RECENT_SECONDS)
            || self
                .worktree_sidebar_load_last_started_at
                .get(key)
                .is_some_and(|started| now - started < WORKTREE_SIDEBAR_LOAD_START_DEBOUNCE_SECONDS)
    }

    fn finish_worktree_sidebar_load(&mut self, key: &super::app_state::WorktreeViewStoreKey) {
        self.worktree_sidebar_load_in_flight.remove(key);
        self.worktree_sidebar_load_last_finished_at
            .insert(key.clone(), app_now_seconds());
    }

    pub(super) fn spawn_persist_terminal_state(&mut self, _cx: &mut Context<Self>) {
        let Some(owner_id) = super::ai_runtime_status::terminal_layout_owner_id(&self.state) else {
            return;
        };
        self.refresh_terminal_slot_snapshots();
        let (tabs, active_tab_id, top_panes, active_slot_id) = self.terminal_layout_snapshot();
        let (active_terminal_id, active_runtime_slot_id, sessions) =
            self.terminal_runtime_snapshot();
        let runtime_service = self.runtime_service.clone();
        let support_dir = self.state.support_dir.clone();
        codux_runtime::async_runtime::spawn_blocking(move || {
            let _ = runtime_service.save_terminal_layout(
                &owner_id,
                tabs,
                active_tab_id,
                top_panes,
                active_slot_id,
            );
            let _ = TerminalRuntimeService::new(support_dir).save_from_gpui(
                active_terminal_id,
                active_runtime_slot_id,
                sessions,
            );
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
        let runtime_inventory = self.runtime.clone();
        let terminal_state = self.state.clone();
        let terminal_runtime_service = runtime_service.clone();
        let terminal_project = project.clone();
        let terminal_runtime_inventory = runtime_inventory.clone();
        let task_runtime_service = runtime_service.clone();
        let task_project = project.clone();
        let primary_runtime_service = runtime_service.clone();
        let primary_project = project.clone();
        let primary_worktree = super::ai_runtime_status::selected_worktree_info(&self.state);
        let stats_runtime_service = runtime_service.clone();
        let stats_project = project.clone();
        let should_load_tasks = self.begin_project_task_load(&project_id);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let terminal = codux_runtime::async_runtime::run_limited_blocking(move || {
                let worktrees = terminal_runtime_service.reload_worktrees_from_state(
                    Some(&terminal_project.id),
                    Some(&terminal_project.path),
                );
                let terminal_owner_id = worktrees
                    .selected_worktree_id
                    .as_deref()
                    .unwrap_or(terminal_project.id.as_str())
                    .to_string();
                let store_key = TerminalViewStoreKey {
                    project_id: terminal_project.id.clone(),
                    task_id: terminal_owner_id.clone(),
                };
                let terminal_layout =
                    terminal_runtime_service.reload_terminal_layout(Some(&terminal_owner_id));
                let terminal_runtime = terminal_runtime_service.reload_terminal_runtime();
                let mut terminal_state = terminal_state;
                terminal_state.selected_project = Some(terminal_project.clone());
                terminal_state.worktrees = worktrees.clone();
                terminal_state.terminal_layout = terminal_layout.clone();
                terminal_state.terminal_runtime = terminal_runtime.clone();
                prewarm_terminal_restore(&terminal_state, &terminal_runtime_inventory);
                ProjectSwitchTerminalLoad {
                    project_id: terminal_project.id,
                    generation,
                    store_key,
                    terminal_layout,
                    terminal_runtime,
                }
            })
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                if let Some(terminal) = terminal {
                    app.apply_project_switch_terminal_load(terminal, cx);
                }
            });
        })
        .detach();

        if should_load_tasks {
            let task_project_id = task_project.id.clone();
            let task_project_path = task_project.path.clone();
            let cleanup_project_id = task_project_id.clone();
            cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                let task_load = codux_runtime::async_runtime::run_limited_blocking(move || {
                    let worktrees = task_runtime_service
                        .reload_worktrees(Some(&task_project_id), Some(&task_project_path));
                    ProjectSwitchTaskLoad {
                        project_id: task_project_id,
                        generation,
                        worktrees,
                    }
                })
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

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let primary = codux_runtime::async_runtime::run_limited_blocking(move || {
                let request =
                    ai_history_worktree_request(&primary_project, primary_worktree.as_ref());
                let ai_history = primary_runtime_service.reload_project_ai_history(&request.path);
                ProjectSwitchPrimaryLoad {
                    project_id: primary_project.id,
                    generation,
                    ai_history,
                }
            })
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                if let Some(primary) = primary {
                    app.apply_project_switch_primary_load(primary, cx);
                }
            });
        })
        .detach();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let load = codux_runtime::async_runtime::run_limited_blocking(move || {
                let ai_global_history = stats_runtime_service.reload_global_ai_history();
                let memory = stats_runtime_service.reload_memory(Some(&stats_project.id));
                let memory_manager = stats_runtime_service.reload_memory_manager(
                    &projects,
                    "project",
                    Some(&stats_project.id),
                    "active",
                );
                ProjectSwitchLoad {
                    project_id: stats_project.id,
                    generation,
                    ai_global_history,
                    memory,
                    memory_manager,
                }
            })
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
        cx: &mut Context<Self>,
    ) {
        self.terminal_view_store.insert(
            load.store_key.clone(),
            TerminalViewState {
                terminal_layout: load.terminal_layout.clone(),
                terminal_runtime: load.terminal_runtime.clone(),
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
        let selected_terminal_key = terminal_view_store_key(&self.state);
        let is_selected_terminal_owner = selected_terminal_key
            .as_ref()
            .is_some_and(|key| key == &load.store_key);
        let has_selected_terminal_view = selected_terminal_key
            .as_ref()
            .is_some_and(|key| self.terminal_view_store.contains_key(key));
        self.runtime_trace(
            "project-switch",
            &format!(
                "terminal_load apply project={} task={} generation={} selected={} saved={}",
                load.project_id,
                load.store_key.task_id,
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
        } else {
            self.schedule_terminal_layout_restore(
                load.terminal_layout,
                load.terminal_runtime,
                load.generation,
                cx,
            );
        }
        self.save_current_project_view_state();
        cx.notify();
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
        self.apply_saved_worktree_view_state();
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
        self.save_current_project_view_state();
        self.notify_task_column(cx);
        cx.notify();
    }

    pub(super) fn apply_project_switch_primary_load(
        &mut self,
        load: ProjectSwitchPrimaryLoad,
        cx: &mut Context<Self>,
    ) {
        let existing = self.project_view_store.get(&load.project_id).cloned();
        self.project_view_store.insert(
            load.project_id.clone(),
            ProjectViewState {
                ai_history: load.ai_history.clone(),
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
        self.state.ai_history = load.ai_history;
        self.selected_ai_session_id = None;
        self.state.ai_session_detail = None;
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
        self.save_current_project_view_state();
        self.notify_task_column(cx);
        cx.notify();
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
        self.save_current_project_view_state();
        self.notify_task_column(cx);
        cx.notify();
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
        cx: &mut Context<Self>,
    ) {
        self.state.terminal_layout = terminal_layout.clone();
        self.state.terminal_runtime = terminal_runtime.clone();
        self.terminal_layout_loading = true;
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let _ = this.update(cx, |app, cx| {
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
        self.reset_file_tree_cache();
        self.file_editable = false;
        self.file_dirty = false;
        self.clear_file_selection();
        self.selected_git_file = None;
        self.normalize_selected_git_branch();
        self.git_diff_preview = "select a changed file to preview its diff".to_string();
        self.git_review_content = None;
        self.normalize_selected_ai_session();
        self.normalize_selected_runtime_session();
        self.normalize_selected_ssh_profile();
        self.status_message = "state reloaded from Codux support files".to_string();
        self.sync_project_list_store(cx);
        cx.notify();
    }

    pub(super) fn reload_project_open_applications(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_open_applications = self.runtime_service.project_open_applications();
        self.status_message = "project application list refreshed".to_string();
        cx.notify();
    }

    pub(super) fn reveal_selected_project_in_file_manager(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to reveal".to_string();
            cx.notify();
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
        cx.notify();
    }

    pub(super) fn open_selected_project_in_application(
        &mut self,
        application_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to open".to_string();
            cx.notify();
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
        cx.notify();
    }

    pub(super) fn open_project_folder_from_dialog(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = locale_from_language_setting(&self.state.settings.language);
        match self
            .runtime_service
            .localized_open_dialog(LocalizedOpenDialogRequest {
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
            }) {
            Ok(Some(paths)) => {
                let Some(path) = paths.first().cloned() else {
                    self.status_message = "project import canceled".to_string();
                    cx.notify();
                    return;
                };
                let name = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or("Project")
                    .to_string();
                match self.runtime_service.create_or_select_project(&name, &path) {
                    Ok(project_id) => {
                        self.state = self.runtime_service.reload_state();
                        self.normalize_selected_ai_session();
                        self.normalize_selected_runtime_session();
                        self.normalize_selected_ssh_profile();
                        self.sync_project_list_store(cx);
                        self.status_message = format!("project added/selected: {project_id}");
                    }
                    Err(error) => {
                        self.status_message = format!("failed to add project: {error}");
                    }
                }
            }
            Ok(None) => {
                self.status_message = "project import canceled".to_string();
            }
            Err(error) => self.status_message = format!("failed to choose project folder: {error}"),
        }
        cx.notify();
    }

    pub(super) fn close_selected_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to close".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.close_project(&project.id) {
            Ok(next_project_id) => {
                self.project_view_store.remove(&project.id);
                self.worktree_view_store
                    .retain(|key, _| key.project_id != project.id);
                self.terminal_view_store
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
        cx.notify();
    }

    pub(super) fn close_all_projects(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.projects.is_empty() {
            self.status_message = "no projects to close".to_string();
            cx.notify();
            return;
        }
        let closed = self.state.projects.len();
        match self.runtime_service.project_close_all() {
            Ok(_snapshot) => {
                self.project_view_store.clear();
                self.worktree_view_store.clear();
                self.terminal_view_store.clear();
                self.state = self.runtime_service.reload_state();
                self.clear_file_selection();
                self.file_tree_expanded_dirs.clear();
                self.file_tree_children.clear();
                self.file_preview = "select a file to preview it".to_string();
                self.file_editable = false;
                self.file_dirty = false;
                self.selected_git_file = None;
                self.git_tree_children.clear();
                self.git_expanded_dirs.clear();
                self.git_diff_preview = "select a changed file to preview its diff".to_string();
                self.git_review_content = None;
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
        cx.notify();
    }

    pub(super) fn rename_selected_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_selected_project_editor_window(_window, cx);
    }

    pub(super) fn open_project_create_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = locale_from_language_setting(&self.state.settings.language);
        if Self::activate_child_window(&mut self.project_editor_window, cx) {
            self.status_message = "project creator already opened".to_string();
            cx.notify();
            return;
        }

        let state = self.state.clone();
        let runtime = self.runtime.clone();
        let runtime_service = self.runtime_service.clone();
        let bounds = Bounds::centered(None, size(px(620.0), px(430.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_titlebar(translate(
                    &locale,
                    "project.create.title",
                    "Create Project",
                ))),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(520.0), px(390.0))),
                ..Default::default()
            },
            move |window, cx| {
                let app = CoduxApp::new_project_creator_window_from_state(
                    state.clone(),
                    runtime.clone(),
                    runtime_service.clone(),
                );
                theme::apply_component_theme(
                    &app.state.settings.theme,
                    &app.state.settings.theme_color,
                    Some(window),
                    cx,
                );
                let view = cx.new(|_| app);
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(handle) => {
                self.project_editor_window = Some(handle.into());
                "project creator opened".to_string()
            }
            Err(error) => format!("failed to open project creator: {error}"),
        };
        cx.notify();
    }

    pub(super) fn open_selected_project_editor_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to edit".to_string();
            cx.notify();
            return;
        };
        let locale = locale_from_language_setting(&self.state.settings.language);

        if Self::activate_child_window(&mut self.project_editor_window, cx) {
            self.status_message = "project editor already opened".to_string();
            cx.notify();
            return;
        }

        let state = self.state.clone();
        let runtime = self.runtime.clone();
        let runtime_service = self.runtime_service.clone();
        let bounds = Bounds::centered(None, size(px(620.0), px(430.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_titlebar(translate(
                    &locale,
                    "project.edit.title",
                    "Edit Project",
                ))),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(520.0), px(390.0))),
                ..Default::default()
            },
            move |window, cx| {
                let app = CoduxApp::new_project_editor_window_from_state(
                    project,
                    state.clone(),
                    runtime.clone(),
                    runtime_service.clone(),
                );
                theme::apply_component_theme(
                    &app.state.settings.theme,
                    &app.state.settings.theme_color,
                    Some(window),
                    cx,
                );
                let view = cx.new(|_| app);
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(handle) => {
                self.project_editor_window = Some(handle.into());
                "project editor opened".to_string()
            }
            Err(error) => format!("failed to open project editor: {error}"),
        };
        cx.notify();
    }

    pub(super) fn set_project_editor_name(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_name = value;
        cx.notify();
    }

    pub(super) fn set_project_editor_path(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_path = value;
        cx.notify();
    }

    pub(super) fn set_project_editor_badge_symbol(
        &mut self,
        value: Option<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_badge_symbol = value;
        cx.notify();
    }

    pub(super) fn set_project_editor_badge_color(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_badge_color_hex = value;
        cx.notify();
    }

    pub(super) fn choose_project_editor_directory(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = locale_from_language_setting(&self.state.settings.language);
        match self
            .runtime_service
            .localized_open_dialog(LocalizedOpenDialogRequest {
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
                default_path: Some(self.project_editor_path.clone()),
                filters: Vec::new(),
                directory: true,
                multiple: false,
                can_create_directories: Some(false),
            }) {
            Ok(Some(paths)) => {
                if let Some(path) = paths.first() {
                    self.project_editor_path = path.clone();
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
        cx.notify();
    }

    pub(super) fn save_project_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.project_editor_name.trim().to_string();
        let path = self.project_editor_path.trim().to_string();
        if name.is_empty() || path.is_empty() {
            self.status_message = "project name and path are required".to_string();
            cx.notify();
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
                    window.remove_window();
                }
                Err(error) => self.status_message = format!("failed to create project: {error}"),
            }
        }
        cx.notify();
    }

    pub(super) fn move_selected_project_up(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to move".to_string();
            cx.notify();
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
        cx.notify();
    }

    pub(super) fn move_selected_project_down(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to move".to_string();
            cx.notify();
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
        cx.notify();
    }
}
