use super::*;

struct FileMutationResult {
    files: Vec<FileEntry>,
    file_tree_children: HashMap<String, Vec<FileEntry>>,
    expanded_dirs: HashSet<String>,
    selection: FileMutationSelection,
    preview: Option<(String, bool, bool)>,
    git: GitSummary,
    status: String,
    clear_draft: bool,
    saved_editor_path: Option<String>,
    saved_editor_content: Option<String>,
}

#[derive(Default)]
enum FileMutationSelection {
    #[default]
    Keep,
    Clear,
    Single(String),
    Multiple(Vec<String>),
}

enum FileMoveConflictCheckResult {
    Clear,
    Conflicts(Vec<String>),
}

#[derive(Clone, Default, PartialEq, Eq)]
struct FileMutationSelectionState {
    selected_entry: Option<String>,
    selected_entries: HashSet<String>,
    selection_anchor: Option<String>,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct FileMutationDraftState {
    kind: Option<FileNameDraftKind>,
    target: Option<String>,
    parent: Option<String>,
    value: String,
}

fn load_file_mutation_tree(
    runtime_service: &RuntimeService,
    project_path: &str,
    directory: Option<&str>,
    expanded_dirs: &[String],
) -> Result<(Vec<FileEntry>, HashMap<String, Vec<FileEntry>>), String> {
    let files = runtime_service.try_reload_project_files(project_path, directory)?;
    let file_tree_children =
        load_file_mutation_children(runtime_service, project_path, expanded_dirs)?;
    Ok((files, file_tree_children))
}

fn load_file_mutation_children(
    runtime_service: &RuntimeService,
    project_path: &str,
    expanded_dirs: &[String],
) -> Result<HashMap<String, Vec<FileEntry>>, String> {
    expanded_dirs
        .iter()
        .cloned()
        .map(|directory_path| {
            let children =
                runtime_service.try_reload_project_files(project_path, Some(&directory_path))?;
            Ok((directory_path, children))
        })
        .collect::<Result<HashMap<_, _>, String>>()
}

fn file_mutation_text_preview(
    runtime_service: &RuntimeService,
    project_path: &str,
    path: &str,
    fallback: &str,
) -> (String, bool, bool) {
    if matches!(
        crate::app::file_editor::file_preview_kind_for_path(path),
        crate::app::file_editor::FilePreviewKind::Image
            | crate::app::file_editor::FilePreviewKind::External
    ) {
        return (fallback.to_string(), false, false);
    }
    runtime_service
        .read_project_file_edit_buffer(project_path, path)
        .map(|(content, editable)| (content, editable, false))
        .unwrap_or_else(|error| (format!("failed to load file: {error}"), false, false))
}

fn file_mutation_prune_expanded(expanded_dirs: &[String], removed_paths: &[String]) -> Vec<String> {
    expanded_dirs
        .iter()
        .filter(|expanded| {
            !removed_paths
                .iter()
                .any(|path| *expanded == path || expanded.starts_with(&format!("{path}/")))
        })
        .cloned()
        .collect()
}

fn file_mutation_refresh_error(action: &str, error: String) -> String {
    format!("{action}, but the first file tree refresh failed: {error}")
}

impl CoduxApp {
    fn spawn_file_mutation<F>(
        &mut self,
        pending_status: String,
        task_failed_status: &'static str,
        cx: &mut Context<Self>,
        task: F,
    ) where
        F: FnOnce() -> Result<FileMutationResult, String> + Send + 'static,
    {
        let generation = self.project_switch_generation;
        let scope_key = super::app_state::current_worktree_scope_key(&self.state);
        let started_selection = self.file_mutation_selection_state();
        let started_draft = self.file_mutation_draft_state();
        self.file_mutation_generation = self.file_mutation_generation.wrapping_add(1);
        let mutation_generation = self.file_mutation_generation;
        self.status_message = pending_status;
        self.invalidate_file_panel(cx);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                task,
            )
            .await
            .ok()
            .unwrap_or_else(|| Err(task_failed_status.to_string()));

            let _ = this.update_in(cx, |app, window, cx| {
                let current_key = super::app_state::current_worktree_scope_key(&app.state);
                if app.project_switch_generation != generation || current_key != scope_key {
                    app.invalidate_file_panel(cx);
                    return;
                }
                let mutation_is_current = app.file_mutation_generation == mutation_generation;
                app.apply_file_mutation_result(
                    result,
                    mutation_is_current,
                    started_selection,
                    started_draft,
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    fn file_mutation_selection_state(&self) -> FileMutationSelectionState {
        FileMutationSelectionState {
            selected_entry: self.selected_file_entry.clone(),
            selected_entries: self.selected_file_entries.clone(),
            selection_anchor: self.file_selection_anchor.clone(),
        }
    }

    fn file_mutation_draft_state(&self) -> FileMutationDraftState {
        FileMutationDraftState {
            kind: self.file_name_draft_kind,
            target: self.file_name_draft_target.clone(),
            parent: self.file_name_draft_parent.clone(),
            value: self.file_name_draft_value.clone(),
        }
    }

    pub(super) fn clear_file_name_draft(&mut self) {
        self.file_name_draft_kind = None;
        self.file_name_draft_target = None;
        self.file_name_draft_parent = None;
        self.file_name_draft_value.clear();
        self.file_name_draft_select_all = false;
    }

    fn spawn_file_mutation_failure_sync(&mut self, error: String, cx: &mut Context<Self>) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = error;
            self.invalidate_file_panel(cx);
            return;
        };
        let file_directory = self.file_directory.clone();
        let expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        let generation = self.project_switch_generation;
        let scope_key = super::app_state::current_worktree_scope_key(&self.state);
        let mutation_generation = self.file_mutation_generation;
        self.status_message = format!("{error}. Retrying file tree refresh…");
        self.invalidate_file_panel(cx);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                {
                    let project_path = project_path.clone();
                    let file_directory = file_directory.clone();
                    let expanded_dirs = expanded_dirs.clone();
                    move || {
                        let (files, file_tree_children) = load_file_mutation_tree(
                            &runtime_service,
                            &project_path,
                            file_directory_option(&file_directory),
                            &expanded_dirs,
                        )?;
                        let git = runtime_service.reload_project_git(&project_path);
                        Ok::<_, String>((
                            files,
                            file_tree_children,
                            expanded_dirs.into_iter().collect::<HashSet<_>>(),
                            git,
                        ))
                    }
                },
            )
            .await
            .ok()
            .unwrap_or_else(|| Err("file refresh retry task failed".to_string()));

            let _ = this.update(cx, |app, cx| {
                if app.project_switch_generation != generation
                    || super::app_state::current_worktree_scope_key(&app.state) != scope_key
                    || app.file_mutation_generation != mutation_generation
                {
                    app.invalidate_file_panel(cx);
                    return;
                }
                match result {
                    Ok((files, file_tree_children, expanded_dirs, git)) => {
                        app.state.files = files;
                        app.file_tree_children = file_tree_children;
                        app.file_tree_expanded_dirs = expanded_dirs;
                        app.prune_missing_file_tree_directories();
                        app.normalize_selected_file_entry();
                        app.state.git = git;
                        app.normalize_selected_git_file();
                        app.normalize_selected_git_branch();
                        app.status_message = format!("{error}. File tree refreshed.");
                        app.remember_current_file_panel_state();
                    }
                    Err(refresh_error) => {
                        app.status_message = format!(
                            "{error}. Refresh retry failed: {refresh_error}. Please refresh files manually."
                        );
                    }
                }
                app.invalidate_file_panel(cx);
            });
        })
        .detach();
    }

    fn apply_file_mutation_result(
        &mut self,
        result: Result<FileMutationResult, String>,
        mutation_is_current: bool,
        started_selection: FileMutationSelectionState,
        started_draft: FileMutationDraftState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !mutation_is_current {
            self.invalidate_file_panel(cx);
            return;
        }
        match result {
            Ok(result) => {
                self.state.files = result.files;
                self.file_tree_expanded_dirs = result.expanded_dirs;
                self.file_tree_children = result.file_tree_children;
                self.prune_missing_file_tree_directories();
                let selection_unchanged = self.file_mutation_selection_state() == started_selection;
                if selection_unchanged {
                    match result.selection {
                        FileMutationSelection::Keep => self.normalize_selected_file_entry(),
                        FileMutationSelection::Clear => self.clear_file_selection(),
                        FileMutationSelection::Single(path) => self.set_single_file_selection(path),
                        FileMutationSelection::Multiple(paths) => {
                            self.selected_file_entries = paths.iter().cloned().collect();
                            self.selected_file_entry = paths.last().cloned();
                            self.file_selection_anchor = self.selected_file_entry.clone();
                        }
                    }
                } else {
                    self.normalize_selected_file_entry();
                }
                let saved_editor_unchanged =
                    result.saved_editor_path.as_deref().is_none_or(|path| {
                        result.saved_editor_content.as_ref().is_none_or(|content| {
                            self.file_editor_states
                                .get(&self.file_editor_state_key(path))
                                .map(|state| state.read(cx).value().as_ref() == content)
                                .unwrap_or(self.file_preview == *content)
                        })
                    });
                if selection_unchanged && saved_editor_unchanged {
                    if let Some((preview, editable, dirty)) = result.preview {
                        self.file_preview = preview;
                        self.file_editable = editable;
                        self.file_dirty = dirty;
                    }
                }
                self.state.git = result.git;
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                if result.clear_draft && self.file_mutation_draft_state() == started_draft {
                    self.clear_file_name_draft();
                }
                if saved_editor_unchanged {
                    if let Some(path) = result.saved_editor_path.as_deref() {
                        self.mark_file_editor_dirty(path, false, window, cx);
                        self.normalize_file_search_index();
                        self.persist_file_editor_layout_async(cx);
                    }
                }
                self.status_message = result.status;
                self.remember_current_file_panel_state();
            }
            Err(error) => {
                self.spawn_file_mutation_failure_sync(error, cx);
                return;
            }
        }
        self.invalidate_file_panel(cx);
    }

    pub(super) fn current_file_panel_state_snapshot(&self) -> super::app_state::FilePanelState {
        super::app_state::FilePanelState {
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

    pub(super) fn remember_current_file_panel_state(&mut self) {
        let Some(scope_key) = super::app_state::current_worktree_scope_key(&self.state) else {
            return;
        };
        if self.state.files.is_empty()
            && self.file_tree_expanded_dirs.is_empty()
            && self.file_tree_children.is_empty()
        {
            self.runtime_trace(
                "files",
                &format!(
                    "cache_save skipped_empty project={} worktree={}",
                    scope_key.project_id, scope_key.worktree_id
                ),
            );
            return;
        }
        let expanded_count = self.file_tree_expanded_dirs.len();
        self.file_panel_cache
            .insert(scope_key.clone(), self.current_file_panel_state_snapshot());
        self.runtime_trace(
            "files",
            &format!(
                "cache_save project={} worktree={} expanded_dirs={expanded_count}",
                scope_key.project_id, scope_key.worktree_id
            ),
        );
    }

    pub(super) fn restore_cached_file_panel_state(&mut self) -> bool {
        let Some(scope_key) = super::app_state::current_worktree_scope_key(&self.state) else {
            return false;
        };
        let Some(cached) = self.file_panel_cache.get(&scope_key).cloned() else {
            self.runtime_trace(
                "files",
                &format!(
                    "cache_restore miss project={} worktree={}",
                    scope_key.project_id, scope_key.worktree_id
                ),
            );
            return false;
        };
        let expanded_count = cached.file_tree_expanded_dirs.len();
        self.file_directory = cached.file_directory;
        self.selected_file_entry = cached.selected_file_entry;
        self.selected_file_entries = cached.selected_file_entries;
        self.file_selection_anchor = cached.file_selection_anchor;
        self.file_tree_expanded_dirs = cached.file_tree_expanded_dirs;
        self.file_tree_children = cached.file_tree_children;
        self.file_editor_tabs = cached.file_editor_tabs;
        self.active_file_editor_tab = cached.active_file_editor_tab;
        self.clear_file_name_draft();
        self.runtime_trace(
            "files",
            &format!(
                "cache_restore hit project={} worktree={} expanded_dirs={expanded_count}",
                scope_key.project_id, scope_key.worktree_id
            ),
        );
        true
    }

    /// Auto-recover a local project whose drive disconnected then remounted at
    /// the same path (a new inode). The file + git watchers die silently on
    /// unmount and never re-attach, and the tree reads back empty, so on a
    /// false→true availability flip we reload the tree, re-arm both watchers, and
    /// refresh git. Driven by the ~1s slow tick.
    pub(super) fn detect_project_drive_recovery(&mut self, cx: &mut Context<Self>) {
        // Only local projects sit on this machine's (disconnect-prone) volumes;
        // a remote project's files/git are served by its host, unaffected here.
        let is_local = self
            .state
            .selected_project
            .as_ref()
            .is_some_and(|project| project.host_device_id.is_none());
        let Some(path) = self.selected_worktree_path().filter(|_| is_local) else {
            // Nothing local selected: keep the baseline "available" so a later
            // local selection doesn't spuriously fire recovery on its first tick.
            self.selected_project_path_available = true;
            return;
        };
        let available = std::path::Path::new(&path).is_dir();
        if available && !self.selected_project_path_available {
            self.recover_project_drive(path, cx);
        }
        self.selected_project_path_available = available;
    }

    fn recover_project_drive(&mut self, worktree_path: String, cx: &mut Context<Self>) {
        let git_path = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.path.clone())
            .unwrap_or_else(|| worktree_path.clone());
        self.status_message = "project drive reconnected — reloading files and git".to_string();
        // Re-arm the file + git watchers: the originals attached to the now-dead
        // inode and won't recover on their own.
        self.runtime_service
            .watch_project_background(worktree_path, git_path);
        self.reload_project_files_async(cx);
        self.refresh_git_panel_state_async(cx);
        self.invalidate_status_bar(cx);
    }

    pub(super) fn reload_project_files_async(&mut self, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to refresh".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let project_name = project.name.clone();
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected worktree to refresh".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let Some(scope_key) = super::app_state::current_worktree_scope_key(&self.state) else {
            self.status_message = "no selected worktree to refresh".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        if self.file_panel_refreshing {
            return;
        }
        let file_directory = self.file_directory.clone();
        let expanded = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        let generation = self.project_switch_generation;
        self.file_panel_refreshing = true;
        self.status_message = format!(
            "refreshing files for {}{}",
            project_name,
            current_directory_suffix(&self.file_directory)
        );
        self.invalidate_file_panel(cx);
        self.invalidate_status_bar(cx);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    let files = runtime_service.reload_project_files(
                        &project_path,
                        file_directory_option(&file_directory),
                    );
                    let file_tree_children = expanded
                        .into_iter()
                        .map(|directory_path| {
                            let children = runtime_service
                                .reload_project_files(&project_path, Some(directory_path.as_str()));
                            (directory_path, children)
                        })
                        .collect::<HashMap<_, _>>();
                    super::app_state::WorktreeFilePanelLoad {
                        generation,
                        scope_key,
                        files,
                        file_tree_children,
                    }
                },
            )
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                app.file_panel_refreshing = false;
                let Some(load) = result else {
                    app.status_message = "failed to refresh file list".to_string();
                    app.invalidate_file_panel(cx);
                    app.invalidate_status_bar(cx);
                    return;
                };
                app.apply_worktree_file_panel_load(load, &project_name, cx);
            });
        })
        .detach();
    }

    pub(super) fn apply_worktree_file_panel_load(
        &mut self,
        load: super::app_state::WorktreeFilePanelLoad,
        project_name: &str,
        cx: &mut Context<Self>,
    ) {
        let current_key = super::app_state::current_worktree_scope_key(&self.state);
        if self.project_switch_generation != load.generation
            || current_key.as_ref() != Some(&load.scope_key)
        {
            self.invalidate_status_bar(cx);
            return;
        }
        self.state.files = load.files;
        self.file_tree_children = load.file_tree_children;
        self.prune_missing_file_tree_directories();
        self.remember_current_file_panel_state();
        self.normalize_selected_file_entry();
        self.status_message = format!(
            "file list reloaded for {}{}",
            project_name,
            current_directory_suffix(&self.file_directory)
        );
        self.runtime_trace(
            "files",
            &format!(
                "manual_reload project={} directory={} entries={}",
                project_name,
                file_directory_option(&self.file_directory).unwrap_or("root"),
                self.state.files.len()
            ),
        );
        self.invalidate_file_panel(cx);
        self.invalidate_status_bar(cx);
    }

    pub(super) fn reset_file_tree_state(&mut self) {
        self.file_tree_expanded_dirs.clear();
        self.file_tree_children.clear();
        self.record_ui_state_clear("file_tree");
    }

    pub(super) fn file_tree_entry(&self, path: &str) -> Option<FileEntry> {
        self.state
            .files
            .iter()
            .chain(
                self.file_tree_children
                    .values()
                    .flat_map(|children| children.iter()),
            )
            .find(|entry| entry.relative_path == path)
            .cloned()
    }

    pub(super) fn selected_file_entry(&self) -> Option<FileEntry> {
        self.selected_file_entry
            .as_deref()
            .and_then(|path| self.file_tree_entry(path))
    }

    /// "Save as…": copy the selected file to a destination chosen in the file
    /// picker (Save mode), on the project's device (local or its host).
    pub(super) fn save_as_selected_file_entry(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.selected_file_entry() else {
            return;
        };
        if matches!(entry.kind, FileKind::Directory) {
            return;
        }
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project for save-as".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let project_path = project.path.trim_end_matches('/').to_string();
        let source_abs = if entry.relative_path.is_empty() {
            project_path.clone()
        } else {
            format!("{project_path}/{}", entry.relative_path)
        };
        let start_dir = std::path::Path::new(&source_abs)
            .parent()
            .map(|parent| parent.to_string_lossy().to_string());
        let device_id = project.host_device_id.clone();
        self.open_file_picker_window(
            super::types::FilePickerMode::Save,
            super::types::FilePickerTarget::SaveFileAs {
                source_path: source_abs,
                device_id: device_id.clone(),
            },
            device_id,
            start_dir,
            Some(entry.name.clone()),
            window,
            cx,
        );
    }

    pub(super) fn clear_file_selection(&mut self) {
        self.selected_file_entry = None;
        self.selected_file_entries.clear();
        self.file_selection_anchor = None;
    }

    pub(super) fn set_single_file_selection(&mut self, relative_path: String) {
        self.selected_file_entry = Some(relative_path.clone());
        self.selected_file_entries.clear();
        self.file_selection_anchor = Some(relative_path);
    }

    pub(super) fn prepare_file_context_menu_selection(
        &mut self,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        if self.selected_file_entries.contains(&relative_path) {
            self.selected_file_entry = Some(relative_path);
        } else {
            self.set_single_file_selection(relative_path);
        }
        self.invalidate_file_panel(cx);
    }

    pub(super) fn reload_file_tree_directory(&mut self, directory_path: &str) {
        let Some(project_path) = self.selected_worktree_path() else {
            return;
        };
        let children = self
            .runtime_service
            .reload_project_files(&project_path, Some(directory_path));
        self.file_tree_children
            .insert(directory_path.to_string(), children);
    }

    pub(super) fn refresh_file_tree_state(&mut self) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.reset_file_tree_state();
            return;
        };
        self.state.files = self
            .runtime_service
            .reload_project_files(&project_path, file_directory_option(&self.file_directory));
        let expanded = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        self.file_tree_children.clear();
        self.record_ui_state_clear("file_tree_children");
        for directory_path in expanded {
            let children = self
                .runtime_service
                .reload_project_files(&project_path, Some(directory_path.as_str()));
            self.file_tree_children.insert(directory_path, children);
        }
        self.prune_missing_file_tree_directories();
    }

    pub(super) fn prune_missing_file_tree_directories(&mut self) {
        let existing_dirs = self
            .state
            .files
            .iter()
            .chain(
                self.file_tree_children
                    .values()
                    .flat_map(|children| children.iter()),
            )
            .filter(|entry| matches!(entry.kind, FileKind::Directory))
            .map(|entry| entry.relative_path.clone())
            .collect::<HashSet<_>>();

        self.file_tree_expanded_dirs
            .retain(|path| existing_dirs.contains(path));
        self.file_tree_children
            .retain(|path, _| existing_dirs.contains(path));
    }

    pub(super) fn toggle_file_tree_directory(
        &mut self,
        relative_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_tree_expanded_dirs.contains(&relative_path) {
            self.file_tree_expanded_dirs.remove(&relative_path);
            self.status_message = format!("directory collapsed: {relative_path}");
        } else {
            self.file_tree_expanded_dirs.insert(relative_path.clone());
            self.reload_file_tree_directory(&relative_path);
            self.status_message = format!("directory expanded: {relative_path}");
        }
        self.remember_current_file_panel_state();
        self.invalidate_file_panel(cx);
    }

    pub(super) fn open_file_entry(
        &mut self,
        entry: FileEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_single_file_selection(entry.relative_path.clone());
        match entry.kind {
            FileKind::Directory => self.toggle_file_tree_directory(entry.relative_path, window, cx),
            FileKind::File => {
                if self.workspace_view == WorkspaceView::Files {
                    self.open_file_editor_tab(entry.relative_path, window, cx);
                } else {
                    match self.state.settings.file_open_default.as_str() {
                        "preview" => self.open_file_preview_window(entry.relative_path, cx),
                        "split" => self.open_file_editor_split(entry.relative_path, window, cx),
                        _ => self.open_file_editor_window(entry.relative_path, cx),
                    }
                }
            }
        }
    }

    pub(super) fn select_file_entry(&mut self, relative_path: String, cx: &mut Context<Self>) {
        self.set_single_file_selection(relative_path.clone());
        self.status_message = format!("selected file item: {relative_path}");
        self.invalidate_file_panel(cx);
    }

    pub(super) fn toggle_file_entry_selection(
        &mut self,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        if !self.selected_file_entries.remove(&relative_path) {
            self.selected_file_entries.insert(relative_path.clone());
            self.selected_file_entry = Some(relative_path);
        } else if self.selected_file_entry.as_deref() == Some(relative_path.as_str()) {
            self.selected_file_entry = self.selected_file_entries.iter().next().cloned();
        }
        if self.file_selection_anchor.is_none() {
            self.file_selection_anchor = self.selected_file_entry.clone();
        }
        self.status_message = format!(
            "selected {} file item{}",
            self.selected_file_entries.len(),
            plural(self.selected_file_entries.len())
        );
        self.invalidate_file_panel(cx);
    }

    pub(super) fn select_file_entry_from_click(
        &mut self,
        entry: FileEntry,
        extend: bool,
        toggle: bool,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if open || matches!(entry.kind, FileKind::Directory) && !extend && !toggle {
            self.open_file_entry(entry, window, cx);
        } else if extend {
            self.select_file_entry_range(entry.relative_path, cx);
        } else if toggle {
            self.toggle_file_entry_selection(entry.relative_path, cx);
        } else {
            self.select_file_entry(entry.relative_path, cx);
        }
    }

    pub(super) fn select_file_entry_range(
        &mut self,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        let anchor = self
            .file_selection_anchor
            .clone()
            .unwrap_or_else(|| relative_path.clone());
        let visible_paths = self.visible_file_tree_paths();
        let Some(anchor_index) = visible_paths.iter().position(|path| path == &anchor) else {
            self.select_file_entry(relative_path, cx);
            return;
        };
        let Some(target_index) = visible_paths.iter().position(|path| path == &relative_path)
        else {
            self.select_file_entry(relative_path, cx);
            return;
        };
        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        self.selected_file_entries = visible_paths[start..=end].iter().cloned().collect();
        self.selected_file_entry = Some(relative_path);
        self.file_selection_anchor = Some(anchor);
        self.status_message = format!(
            "selected {} file item{}",
            self.selected_file_entries.len(),
            plural(self.selected_file_entries.len())
        );
        self.invalidate_file_panel(cx);
    }

    pub(super) fn visible_file_tree_paths(&self) -> Vec<String> {
        fn push_visible(
            rows: &mut Vec<String>,
            files: &[FileEntry],
            children: &HashMap<String, Vec<FileEntry>>,
            expanded: &HashSet<String>,
        ) {
            for file in files {
                rows.push(file.relative_path.clone());
                if expanded.contains(&file.relative_path) {
                    if let Some(child_files) = children.get(&file.relative_path) {
                        push_visible(rows, child_files, children, expanded);
                    }
                }
            }
        }

        let mut rows = Vec::new();
        push_visible(
            &mut rows,
            &self.state.files,
            &self.file_tree_children,
            &self.file_tree_expanded_dirs,
        );
        rows
    }

    pub(super) fn handle_file_sidebar_key_action(
        &mut self,
        action: super::sidebars::FileSidebarKeyAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_name_draft_kind.is_some() {
            return;
        }
        match action {
            super::sidebars::FileSidebarKeyAction::Rename => {
                if self.selected_file_entries.len() <= 1 {
                    self.rename_selected_file_entry(window, cx);
                }
            }
            super::sidebars::FileSidebarKeyAction::MoveSelection(delta) => {
                self.move_file_sidebar_selection(delta, cx);
            }
            super::sidebars::FileSidebarKeyAction::Delete => {
                self.request_delete_selected_file_entries(window, cx);
            }
        }
    }

    pub(super) fn move_file_sidebar_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let paths = self.visible_file_tree_paths();
        if paths.is_empty() {
            self.status_message = "no file items to select".to_string();
            self.invalidate_file_panel(cx);
            return;
        }
        let current_index = self
            .selected_file_entry
            .as_ref()
            .and_then(|selected| paths.iter().position(|path| path == selected));
        let next_index = current_index
            .map(|index| {
                index
                    .saturating_add_signed(delta)
                    .min(paths.len().saturating_sub(1))
            })
            .unwrap_or(0);
        self.file_tree_scroll_handle
            .scroll_to_item(next_index, gpui::ScrollStrategy::Nearest);
        self.select_file_entry(paths[next_index].clone(), cx);
    }

    fn spawn_file_move_conflict_check(
        &mut self,
        paths: Vec<String>,
        target_directory_path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file move".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let project_path = project.path.clone();
        let runtime_service = self.runtime_service.clone();
        let generation = self.project_switch_generation;
        let scope_key = super::app_state::current_worktree_scope_key(&self.state);
        self.file_mutation_generation = self.file_mutation_generation.wrapping_add(1);
        let mutation_generation = self.file_mutation_generation;
        let target_directory_for_read = target_directory_path.clone();
        let worker_paths = paths.clone();
        self.status_message = format!(
            "checking move conflicts for {} file item{}",
            paths.len(),
            plural(paths.len())
        );
        self.invalidate_file_panel(cx);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    let target_entries = runtime_service.try_reload_project_files(
                        &project_path,
                        file_directory_option(&target_directory_for_read),
                    )?;
                    let target_names = target_entries
                        .iter()
                        .map(|entry| entry.name.as_str())
                        .collect::<HashSet<_>>();
                    let conflicts = worker_paths
                        .iter()
                        .filter_map(|path| {
                            let name = path.rsplit('/').next()?;
                            target_names
                                .contains(name)
                                .then(|| join_relative_child_path(&target_directory_for_read, name))
                        })
                        .collect::<Vec<_>>();
                    Ok::<_, String>(if conflicts.is_empty() {
                        FileMoveConflictCheckResult::Clear
                    } else {
                        FileMoveConflictCheckResult::Conflicts(conflicts)
                    })
                },
            )
            .await
            .ok()
            .unwrap_or_else(|| Err("file move conflict check failed".to_string()));

            let _ = this.update(cx, |app, cx| {
                let current_key = super::app_state::current_worktree_scope_key(&app.state);
                if app.project_switch_generation != generation
                    || current_key != scope_key
                    || app.file_mutation_generation != mutation_generation
                {
                    app.invalidate_file_panel(cx);
                    return;
                }
                app.apply_file_move_conflict_check_result(paths, target_directory_path, result, cx);
            });
        })
        .detach();
    }

    fn apply_file_move_conflict_check_result(
        &mut self,
        paths: Vec<String>,
        target_directory_path: String,
        result: Result<FileMoveConflictCheckResult, String>,
        cx: &mut Context<Self>,
    ) {
        let conflicts = match result {
            Ok(FileMoveConflictCheckResult::Clear) => {
                self.move_file_entries_to_directory_confirmed(
                    paths,
                    target_directory_path,
                    false,
                    cx,
                );
                return;
            }
            Ok(FileMoveConflictCheckResult::Conflicts(conflicts)) => conflicts,
            Err(error) => {
                self.status_message = format!("failed to check file move conflicts: {error}");
                self.invalidate_file_panel(cx);
                return;
            }
        };

        let title = if conflicts.len() == 1 {
            self.text("files.move.conflict_one_format", "Overwrite \"%@\"?")
                .replace("%@", &conflicts[0])
        } else {
            self.text(
                "files.move.conflict_many_format",
                "Overwrite %d file items?",
            )
            .replace("%d", &conflicts.len().to_string())
        };
        let message = self.text(
            "files.move.conflict.message",
            "The destination already contains file items with the same name. Overwriting will replace the destination items.",
        );
        let confirm_label = self.text("files.move.conflict.confirm", "Overwrite");
        let cancel_label = self.text("common.cancel", "Cancel");
        let service = self.runtime_service.clone();
        let generation = self.project_switch_generation;
        let scope_key = super::app_state::current_worktree_scope_key(&self.state);
        let mutation_generation = self.file_mutation_generation;
        self.status_message = "waiting for file move confirmation".to_string();
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_millis(120)).await;
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

            let _ = this.update(cx, |app, cx| {
                if app.project_switch_generation != generation
                    || super::app_state::current_worktree_scope_key(&app.state) != scope_key
                    || app.file_mutation_generation != mutation_generation
                {
                    app.invalidate_file_panel(cx);
                    return;
                }
                match result {
                    Ok(true) => app.move_file_entries_to_directory_confirmed(
                        paths,
                        target_directory_path,
                        true,
                        cx,
                    ),
                    Ok(false) => {
                        app.status_message = "file move canceled".to_string();
                        app.invalidate_file_panel(cx);
                    }
                    Err(error) => {
                        app.status_message = format!("failed to show move confirmation: {error}");
                        app.invalidate_file_panel(cx);
                    }
                }
            });
        })
        .detach();
        self.invalidate_file_panel(cx);
    }

    pub(super) fn move_file_entries_to_directory(
        &mut self,
        paths: Vec<String>,
        target_directory_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut paths = paths
            .into_iter()
            .filter(|path| path != &target_directory_path)
            .filter(|path| !target_directory_path.starts_with(&format!("{path}/")))
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        let original_len = paths.len();
        paths.retain(|path| {
            parent_relative_directory(path) != target_directory_path.trim_matches('/')
        });
        if paths.is_empty() {
            self.status_message = if original_len == 0 {
                "no movable file item selected".to_string()
            } else {
                "file item is already in that directory".to_string()
            };
            self.invalidate_file_panel(cx);
            return;
        }

        self.spawn_file_move_conflict_check(paths, target_directory_path, cx);
    }

    pub(super) fn move_file_entries_to_directory_confirmed(
        &mut self,
        paths: Vec<String>,
        target_directory_path: String,
        overwrite: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file move".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        if paths.is_empty() {
            self.status_message = "no movable file item selected".to_string();
            self.invalidate_file_panel(cx);
            return;
        }
        let project_path = project.path.clone();
        let file_directory = self.file_directory.clone();
        let directory = file_directory_option(&file_directory).map(str::to_string);
        let expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        let count = paths.len();
        self.spawn_file_mutation(
            format!("moving {count} file item{}", plural(count)),
            "file move task failed",
            cx,
            move || {
                let mut moved = Vec::new();
                let mut files = None;
                for path in &paths {
                    let (next_files, moved_path) = if overwrite {
                        runtime_service.move_project_file_entry_overwrite(
                            &project_path,
                            path,
                            &target_directory_path,
                            directory.as_deref(),
                        )
                    } else {
                        runtime_service.move_project_file_entry(
                            &project_path,
                            path,
                            &target_directory_path,
                            directory.as_deref(),
                        )
                    }
                    .map_err(|error| format!("failed to move {path}: {error}"))?;
                    files = Some(next_files);
                    moved.push(moved_path);
                }
                let files = files.unwrap_or_default();
                let next_expanded = file_mutation_prune_expanded(&expanded_dirs, &paths);
                let file_tree_children =
                    load_file_mutation_children(&runtime_service, &project_path, &next_expanded)
                        .map_err(|error| file_mutation_refresh_error("file items moved", error))?;
                let git = runtime_service.reload_project_git(&project_path);
                let selection = if moved.len() == 1 {
                    FileMutationSelection::Single(moved[0].clone())
                } else {
                    FileMutationSelection::Multiple(moved.clone())
                };
                Ok(FileMutationResult {
                    files,
                    file_tree_children,
                    expanded_dirs: next_expanded.into_iter().collect(),
                    selection,
                    preview: None,
                    git,
                    status: format!("moved {} file item{}", moved.len(), plural(moved.len())),
                    clear_draft: false,
                    saved_editor_path: None,
                    saved_editor_content: None,
                })
            },
        );
    }

    pub(super) fn normalize_selected_file_entry(&mut self) {
        let selected_still_exists = self
            .selected_file_entry
            .as_deref()
            .map(|path| self.file_tree_entry(path).is_some())
            .unwrap_or(false);
        if !selected_still_exists {
            self.clear_file_selection();
            self.file_editable = false;
            self.file_dirty = false;
        }
    }

    pub(super) fn selected_file_is_text_file(&self) -> bool {
        let Some(entry_path) = self.selected_file_entry.as_deref() else {
            return false;
        };
        self.file_tree_entry(entry_path)
            .map(|entry| matches!(entry.kind, FileKind::File))
            .unwrap_or(false)
    }

    pub(super) fn file_search_match_lines(&self) -> Vec<usize> {
        let query = self.file_search_query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }

        self.file_preview
            .lines()
            .enumerate()
            .filter_map(|(index, line)| line.to_lowercase().contains(&query).then_some(index))
            .collect()
    }

    pub(super) fn normalize_file_search_index(&mut self) {
        let count = self.file_search_match_lines().len();
        if count == 0 {
            self.file_search_match_index = 0;
        } else if self.file_search_match_index >= count {
            self.file_search_match_index = count - 1;
        }
    }

    pub(super) fn open_file_search(&mut self, cx: &mut Context<Self>) {
        self.workspace_view = WorkspaceView::Files;
        self.file_search_open = true;
        self.normalize_file_search_index();
        let count = self.file_search_match_lines().len();
        self.status_message = if self.file_search_query.trim().is_empty() {
            "file search opened".to_string()
        } else {
            format!("file search matches: {count}")
        };
        self.invalidate_file_panel(cx);
    }

    pub(super) fn handle_file_editor_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.workspace_view == WorkspaceView::Files && self.active_file_editor_tab.is_some() {
            return false;
        }
        if self.workspace_view != WorkspaceView::Files
            || !self.file_editable
            || !self.selected_file_is_text_file()
        {
            return false;
        }
        let keystroke = &event.keystroke;
        if keystroke.modifiers.control
            || keystroke.modifiers.alt
            || keystroke.modifiers.platform
            || keystroke.modifiers.function
        {
            return false;
        }
        let changed = match keystroke.key.as_str() {
            "backspace" | "Backspace" => self.file_preview.pop().is_some(),
            "enter" | "Enter" | "return" | "Return" => {
                self.file_preview.push('\n');
                true
            }
            "tab" | "Tab" => {
                self.file_preview.push_str("  ");
                true
            }
            _ => {
                let Some(text) = keystroke.key_char.as_deref() else {
                    return false;
                };
                if text.chars().all(|ch| !ch.is_control()) {
                    self.file_preview.push_str(text);
                    true
                } else {
                    false
                }
            }
        };
        if changed {
            self.file_dirty = true;
            self.status_message = "file edit buffer changed".to_string();
            self.invalidate_file_panel(cx);
        }
        changed
    }

    pub(super) fn create_project_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Start with an empty field (the input shows its own placeholder) rather
        // than a pre-filled value the user has to clear first.
        self.start_file_name_draft(FileNameDraftKind::CreateFile, None, Some(String::new()), cx);
    }

    pub(super) fn create_project_file_in_directory(
        &mut self,
        parent: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_file_name_draft(
            FileNameDraftKind::CreateFile,
            Some(parent),
            Some(String::new()),
            cx,
        );
    }

    pub(super) fn create_project_directory(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_file_name_draft(
            FileNameDraftKind::CreateDirectory,
            None,
            Some(String::new()),
            cx,
        );
    }

    pub(super) fn create_project_directory_in_directory(
        &mut self,
        parent: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_file_name_draft(
            FileNameDraftKind::CreateDirectory,
            Some(parent),
            Some(String::new()),
            cx,
        );
    }

    pub(super) fn start_file_name_draft(
        &mut self,
        kind: FileNameDraftKind,
        parent: Option<String>,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let value = value.unwrap_or_else(|| {
            if kind == FileNameDraftKind::Rename {
                self.selected_file_entry()
                    .map(|entry| entry.name)
                    .unwrap_or_default()
            } else {
                generated_project_child_name(
                    &self.state.files,
                    kind == FileNameDraftKind::CreateDirectory,
                )
            }
        });
        self.file_name_draft_kind = Some(kind);
        self.file_name_draft_target = if kind == FileNameDraftKind::Rename {
            self.selected_file_entry.clone()
        } else {
            None
        };
        self.file_name_draft_parent = if kind == FileNameDraftKind::Rename {
            None
        } else {
            parent.filter(|path| !path.trim().is_empty())
        };
        if let Some(parent) = self.file_name_draft_parent.clone() {
            self.file_tree_expanded_dirs.insert(parent.clone());
            self.reload_file_tree_directory(&parent);
        }
        self.file_name_draft_select_all = true;
        self.file_name_draft_value = value;
        self.status_message = match kind {
            FileNameDraftKind::CreateFile => "enter file name".to_string(),
            FileNameDraftKind::CreateDirectory => "enter folder name".to_string(),
            FileNameDraftKind::Rename => "enter new file name".to_string(),
        };
        self.invalidate_file_panel(cx);
    }

    pub(super) fn set_file_name_draft_value(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_name_draft_value = value;
        self.invalidate_file_panel(cx);
    }

    pub(super) fn handle_file_name_draft_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.file_name_draft_kind.is_none() {
            return false;
        }
        let keystroke = &event.keystroke;
        if keystroke.modifiers.control
            || keystroke.modifiers.alt
            || keystroke.modifiers.platform
            || keystroke.modifiers.function
        {
            return false;
        }

        if matches!(keystroke.key.as_str(), "escape" | "Escape") {
            self.cancel_file_name_draft(window, cx);
            true
        } else if matches!(
            keystroke.key.as_str(),
            "enter" | "Enter" | "return" | "Return"
        ) {
            self.confirm_file_name_draft(window, cx);
            true
        } else {
            false
        }
    }

    pub(super) fn finish_file_name_draft_on_blur(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_name_draft_kind.is_none() {
            return;
        }
        let value = self.file_name_draft_value.trim();
        let unchanged_rename = self.file_name_draft_kind == Some(FileNameDraftKind::Rename)
            && self
                .selected_file_entry()
                .map(|entry| entry.name == value)
                .unwrap_or(false);
        if value.is_empty() || value.eq_ignore_ascii_case("undefined") || unchanged_rename {
            self.clear_file_name_draft();
            self.status_message = "file name edit canceled".to_string();
            self.invalidate_file_panel(cx);
        } else {
            self.confirm_file_name_draft(window, cx);
        }
    }

    pub(super) fn cancel_file_name_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.clear_file_name_draft();
        self.status_message = "file name edit canceled".to_string();
        self.invalidate_file_panel(cx);
    }

    pub(super) fn confirm_file_name_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.file_name_draft_kind else {
            self.status_message = "no file name edit in progress".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let name = self.file_name_draft_value.trim().to_string();
        let parent = self.file_name_draft_parent.clone();
        self.clear_file_name_draft();
        if name.is_empty()
            || name.eq_ignore_ascii_case("undefined")
            || name.contains('/')
            || name.contains('\\')
        {
            self.status_message =
                "file name is required and cannot be undefined or contain path separators"
                    .to_string();
            self.invalidate_file_panel(cx);
            return;
        }

        match kind {
            FileNameDraftKind::CreateFile => {
                self.create_project_file_entry(false, parent, name, cx)
            }
            FileNameDraftKind::CreateDirectory => {
                self.create_project_file_entry(true, parent, name, cx)
            }
            FileNameDraftKind::Rename => self.rename_selected_file_entry_to(name, cx),
        }
    }

    pub(super) fn create_project_file_entry(
        &mut self,
        directory: bool,
        parent: Option<String>,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file creation".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let project_path = project.path.clone();
        let parent =
            parent.or_else(|| file_directory_option(&self.file_directory).map(str::to_string));
        let file_directory = self.file_directory.clone();
        let mut expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        if let Some(parent) = &parent {
            expanded_dirs.push(parent.clone());
            expanded_dirs.sort();
            expanded_dirs.dedup();
        }
        let runtime_service = self.runtime_service.clone();
        let item_label = if directory { "directory" } else { "file" };
        self.spawn_file_mutation(
            format!("creating {item_label}: {name}"),
            "file creation task failed",
            cx,
            move || {
                if directory {
                    runtime_service.create_project_directory(
                        &project_path,
                        parent.as_deref(),
                        &name,
                    )?
                } else {
                    runtime_service.create_project_file(&project_path, parent.as_deref(), &name)?
                };
                let (files, file_tree_children) = load_file_mutation_tree(
                    &runtime_service,
                    &project_path,
                    file_directory_option(&file_directory),
                    &expanded_dirs,
                )
                .map_err(|error| file_mutation_refresh_error("file created", error))?;
                let relative_path =
                    join_relative_child_path(parent.as_deref().unwrap_or_default(), &name);
                let git = runtime_service.reload_project_git(&project_path);
                Ok(FileMutationResult {
                    files,
                    file_tree_children,
                    expanded_dirs: expanded_dirs.into_iter().collect(),
                    selection: FileMutationSelection::Single(relative_path.clone()),
                    preview: Some(if directory {
                        ("directory created".to_string(), false, false)
                    } else {
                        (String::new(), true, false)
                    }),
                    git,
                    status: format!("{item_label} created: {relative_path}"),
                    clear_draft: true,
                    saved_editor_path: None,
                    saved_editor_content: None,
                })
            },
        );
    }

    pub(super) fn request_delete_selected_file_entries(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut paths = if self.selected_file_entries.is_empty() {
            self.selected_file_entry
                .clone()
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            self.selected_file_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>()
        };
        paths.sort();
        paths.dedup();
        if paths.is_empty() {
            self.status_message = "no selected file entry to delete".to_string();
        } else {
            let title = if paths.len() == 1 {
                self.text("files.delete.confirm_one_format", "Delete \"%@\"?")
                    .replace("%@", &paths[0])
            } else {
                self.text("files.delete.confirm_many_format", "Delete %d file items?")
                    .replace("%d", &paths.len().to_string())
            };
            let message = self.text(
                "files.delete.confirm.message",
                "Deleted file items will be moved to Trash.",
            );
            let confirm_label = self.text("common.delete", "Delete");
            let cancel_label = self.text("common.cancel", "Cancel");
            let service = self.runtime_service.clone();
            self.file_mutation_generation = self.file_mutation_generation.wrapping_add(1);
            let mutation_generation = self.file_mutation_generation;
            let generation = self.project_switch_generation;
            let scope_key = super::app_state::current_worktree_scope_key(&self.state);
            self.status_message = "waiting for file deletion confirmation".to_string();
            let timer = cx.background_executor().clone();
            cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                timer.timer(Duration::from_millis(120)).await;
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

                let _ = this.update(cx, |app, cx| {
                    if app.project_switch_generation != generation
                        || super::app_state::current_worktree_scope_key(&app.state) != scope_key
                        || app.file_mutation_generation != mutation_generation
                    {
                        app.invalidate_file_panel(cx);
                        return;
                    }
                    match result {
                        Ok(true) => app.delete_file_entries(paths, cx),
                        Ok(false) => {
                            app.status_message = "file deletion canceled".to_string();
                            app.invalidate_file_panel(cx);
                        }
                        Err(error) => {
                            app.status_message =
                                format!("failed to show delete confirmation: {error}");
                            app.invalidate_file_panel(cx);
                        }
                    }
                });
            })
            .detach();
            self.invalidate_file_panel(cx);
            return;
        }
        self.invalidate_file_panel(cx);
    }

    pub(super) fn delete_file_entries(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file deletion".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        if paths.is_empty() {
            self.status_message = "no selected file entry to delete".to_string();
            self.invalidate_file_panel(cx);
            return;
        }
        let project_path = project.path.clone();
        let file_directory = self.file_directory.clone();
        let directory = file_directory_option(&file_directory).map(str::to_string);
        let expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        let count = paths.len();
        self.spawn_file_mutation(
            format!("moving {count} file item{} to trash", plural(count)),
            "file deletion task failed",
            cx,
            move || {
                let mut files = None;
                for entry_path in &paths {
                    files = Some(runtime_service.delete_project_file_entry(
                        &project_path,
                        entry_path,
                        directory.as_deref(),
                    )?);
                }
                let files = files.unwrap_or_default();
                let next_expanded = file_mutation_prune_expanded(&expanded_dirs, &paths);
                let file_tree_children =
                    load_file_mutation_children(&runtime_service, &project_path, &next_expanded)
                        .map_err(|error| {
                            file_mutation_refresh_error("file items deleted", error)
                        })?;
                let git = runtime_service.reload_project_git(&project_path);
                Ok(FileMutationResult {
                    files,
                    file_tree_children,
                    expanded_dirs: next_expanded.into_iter().collect(),
                    selection: FileMutationSelection::Clear,
                    preview: Some(("select a file to preview it".to_string(), false, false)),
                    git,
                    status: format!("moved {count} file item{} to trash", plural(count)),
                    clear_draft: false,
                    saved_editor_path: None,
                    saved_editor_content: None,
                })
            },
        );
    }

    pub(super) fn save_selected_file_preview(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for file save".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let entry_path = self
            .active_file_editor_tab
            .clone()
            .or_else(|| self.selected_file_entry.clone());
        let Some(entry_path) = entry_path else {
            self.status_message = "no selected file to save".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let tab_editable = self
            .file_editor_tabs
            .iter()
            .find(|tab| tab.relative_path == entry_path)
            .map(|tab| tab.editable)
            .unwrap_or(self.file_editable);
        if !tab_editable {
            self.status_message = "selected file preview is read-only".to_string();
            self.invalidate_file_panel(cx);
            return;
        }
        let content = self
            .active_file_editor_state()
            .map(|state| state.read(cx).value().to_string())
            .unwrap_or_else(|| self.file_preview.clone());
        let file_directory = self.file_directory.clone();
        let expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        self.spawn_file_mutation(
            format!("saving file: {entry_path}"),
            "file save task failed",
            cx,
            move || {
                let preview =
                    runtime_service.write_project_file(&project_path, &entry_path, &content)?;
                let (files, file_tree_children) = load_file_mutation_tree(
                    &runtime_service,
                    &project_path,
                    file_directory_option(&file_directory),
                    &expanded_dirs,
                )
                .map_err(|error| file_mutation_refresh_error("file saved", error))?;
                let git = runtime_service.reload_project_git(&project_path);
                Ok(FileMutationResult {
                    files,
                    file_tree_children,
                    expanded_dirs: expanded_dirs.into_iter().collect(),
                    selection: FileMutationSelection::Single(entry_path.clone()),
                    preview: Some((preview, true, false)),
                    git,
                    status: format!("file saved: {entry_path}"),
                    clear_draft: false,
                    saved_editor_path: Some(entry_path),
                    saved_editor_content: Some(content.clone()),
                })
            },
        );
    }

    pub(super) fn reload_active_file_editor_tab(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for file reload".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let Some(entry_path) = self.active_file_editor_tab.clone() else {
            self.status_message = "no active file to reload".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let runtime_service = self.runtime_service.clone();
        let generation = self.project_switch_generation;
        let scope_key = super::app_state::current_worktree_scope_key(&self.state);
        self.status_message = format!("reloading file: {entry_path}");
        self.invalidate_file_panel(cx);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_entry_path = entry_path.clone();
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    runtime_service.read_project_file_edit_buffer(&project_path, &worker_entry_path)
                },
            )
            .await
            .ok()
            .unwrap_or_else(|| Err("file reload task failed".to_string()));

            let _ = this.update_in(cx, |app, window, cx| {
                if app.project_switch_generation != generation
                    || super::app_state::current_worktree_scope_key(&app.state) != scope_key
                {
                    app.invalidate_file_panel(cx);
                    return;
                }
                app.apply_active_file_editor_tab_reload(entry_path, result, window, cx);
            });
        })
        .detach();
    }

    fn apply_active_file_editor_tab_reload(
        &mut self,
        entry_path: String,
        result: Result<(String, bool), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((content, editable)) => {
                let key = self.file_editor_state_key(&entry_path);
                let language = self
                    .file_editor_tabs
                    .iter()
                    .find(|tab| tab.relative_path == entry_path)
                    .map(|tab| tab.language.clone())
                    .unwrap_or_else(|| "text".to_string());
                if let Some(editor) = self.file_editor_states.get(&key) {
                    editor.update(cx, |state, cx| {
                        state.set_value(content.clone(), window, cx);
                        state.focus(window, cx);
                    });
                } else {
                    self.ensure_file_editor_state(
                        key,
                        entry_path.clone(),
                        language,
                        content.clone(),
                        window,
                        cx,
                    );
                }
                if let Some(tab) = self
                    .file_editor_tabs
                    .iter_mut()
                    .find(|tab| tab.relative_path == entry_path)
                {
                    tab.editable = editable;
                    tab.dirty = false;
                }
                self.file_preview = content;
                self.file_editable = editable;
                self.file_dirty = false;
                self.status_message = format!("file reloaded: {entry_path}");
            }
            Err(error) => self.status_message = format!("failed to reload file: {error}"),
        }
        self.invalidate_file_panel(cx);
    }

    pub(super) fn rename_selected_file_entry(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_file_entry().is_none() {
            self.status_message = "no selected file entry to rename".to_string();
            self.invalidate_file_panel(cx);
            return;
        }
        self.start_file_name_draft(FileNameDraftKind::Rename, None, None, cx);
    }

    pub(super) fn rename_selected_file_entry_to(
        &mut self,
        new_name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file rename".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let Some(entry_path) = self.selected_file_entry.clone() else {
            self.status_message = "no selected file entry to rename".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let Some(entry) = self.selected_file_entry() else {
            self.status_message = "selected file entry is no longer available".to_string();
            self.normalize_selected_file_entry();
            self.invalidate_file_panel(cx);
            return;
        };
        let project_path = project.path.clone();
        let file_directory = self.file_directory.clone();
        let directory = file_directory_option(&file_directory).map(str::to_string);
        let expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        let was_file = matches!(entry.kind, FileKind::File);
        self.spawn_file_mutation(
            format!("renaming file entry: {entry_path}"),
            "file rename task failed",
            cx,
            move || {
                let (files, renamed_path) = runtime_service.rename_project_file_entry(
                    &project_path,
                    &entry_path,
                    &new_name,
                    directory.as_deref(),
                )?;
                let was_expanded = expanded_dirs.iter().any(|path| path == &entry_path);
                let mut next_expanded =
                    file_mutation_prune_expanded(&expanded_dirs, std::slice::from_ref(&entry_path));
                if was_expanded {
                    next_expanded.push(renamed_path.clone());
                }
                let file_tree_children =
                    load_file_mutation_children(&runtime_service, &project_path, &next_expanded)
                        .map_err(|error| file_mutation_refresh_error("file renamed", error))?;
                let preview = if was_file {
                    file_mutation_text_preview(
                        &runtime_service,
                        &project_path,
                        &renamed_path,
                        "file entry renamed",
                    )
                } else {
                    ("directory renamed".to_string(), false, false)
                };
                let git = runtime_service.reload_project_git(&project_path);
                Ok(FileMutationResult {
                    files,
                    file_tree_children,
                    expanded_dirs: next_expanded.into_iter().collect(),
                    selection: FileMutationSelection::Single(renamed_path.clone()),
                    preview: Some(preview),
                    git,
                    status: format!("renamed file entry: {renamed_path}"),
                    clear_draft: true,
                    saved_editor_path: None,
                    saved_editor_content: None,
                })
            },
        );
    }

    pub(super) fn selected_file_entry_paths(&self) -> Vec<String> {
        let mut paths = if self.selected_file_entries.is_empty() {
            self.selected_file_entry
                .clone()
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            self.selected_file_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>()
        };
        paths.sort();
        paths.dedup();
        paths
    }

    pub(super) fn copy_selected_file_paths_to_clipboard(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for file copy".to_string();
            self.invalidate_file_panel(cx);
            return true;
        };
        let paths = self.selected_file_entry_paths();
        if paths.is_empty() {
            self.status_message = "no selected file entry to copy".to_string();
            self.invalidate_file_panel(cx);
            return true;
        }
        let full_paths = paths
            .iter()
            .map(|path| {
                Path::new(&project_path)
                    .join(path)
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>();
        let external_paths = full_paths.iter().map(PathBuf::from).collect::<Vec<_>>();
        cx.write_to_clipboard(ClipboardItem {
            entries: vec![
                gpui::ClipboardEntry::ExternalPaths(gpui::ExternalPaths(external_paths.into())),
                gpui::ClipboardEntry::String(gpui::ClipboardString::new(full_paths.join("\n"))),
            ],
        });
        self.status_message = format!("copied {} file path{}", paths.len(), plural(paths.len()));
        self.invalidate_file_panel(cx);
        true
    }

    pub(super) fn copy_active_file_editor_path_to_clipboard(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(path) = self.active_file_editor_tab.clone() else {
            self.status_message = "no active file to copy".to_string();
            self.invalidate_file_panel(cx);
            return true;
        };
        self.copy_file_path_to_clipboard(path, cx)
    }

    pub(super) fn copy_file_path_to_clipboard(
        &mut self,
        path: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for file copy".to_string();
            self.invalidate_file_panel(cx);
            return true;
        };
        let full_path = Path::new(&project_path)
            .join(&path)
            .to_string_lossy()
            .to_string();
        cx.write_to_clipboard(ClipboardItem {
            entries: vec![
                gpui::ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
                    vec![PathBuf::from(&full_path)].into(),
                )),
                gpui::ClipboardEntry::String(gpui::ClipboardString::new(full_path)),
            ],
        });
        self.status_message = format!("copied file path: {path}");
        self.invalidate_file_panel(cx);
        true
    }

    pub(super) fn paste_clipboard_file_entries(
        &mut self,
        payload: ClipboardFilePayload,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let target_directory = self
            .selected_file_entry()
            .map(|entry| {
                if matches!(entry.kind, FileKind::Directory) {
                    entry.relative_path
                } else {
                    parent_relative_directory(&entry.relative_path)
                }
            })
            .unwrap_or_else(|| self.file_directory.clone());
        self.paste_file_payload_into_directory(payload, target_directory, cx);
        true
    }

    pub(super) fn copy_selected_file_entry(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file copy".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let Some(entry_path) = self.selected_file_entry.clone() else {
            self.status_message = "no selected file entry to copy".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let Some(entry) = self.selected_file_entry() else {
            self.status_message = "selected file entry is no longer available".to_string();
            self.normalize_selected_file_entry();
            self.invalidate_file_panel(cx);
            return;
        };
        let project_path = project.path.clone();
        let file_directory = self.file_directory.clone();
        let directory = file_directory_option(&file_directory).map(str::to_string);
        let expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        let was_file = matches!(entry.kind, FileKind::File);
        self.spawn_file_mutation(
            format!("copying file entry: {entry_path}"),
            "file copy task failed",
            cx,
            move || {
                let (files, copied_path) = runtime_service.copy_project_file_entry(
                    &project_path,
                    &entry_path,
                    directory.as_deref(),
                )?;
                let file_tree_children =
                    load_file_mutation_children(&runtime_service, &project_path, &expanded_dirs)
                        .map_err(|error| file_mutation_refresh_error("file copied", error))?;
                let preview = if was_file {
                    file_mutation_text_preview(
                        &runtime_service,
                        &project_path,
                        &copied_path,
                        "file entry copied",
                    )
                } else {
                    ("directory copied".to_string(), false, false)
                };
                let git = runtime_service.reload_project_git(&project_path);
                Ok(FileMutationResult {
                    files,
                    file_tree_children,
                    expanded_dirs: expanded_dirs.into_iter().collect(),
                    selection: FileMutationSelection::Single(copied_path.clone()),
                    preview: Some(preview),
                    git,
                    status: format!("copied file entry: {copied_path}"),
                    clear_draft: false,
                    saved_editor_path: None,
                    saved_editor_content: None,
                })
            },
        );
    }

    pub(super) fn paste_external_file_entries(
        &mut self,
        payload: ClipboardFilePayload,
        target_entry: FileEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target_directory = if matches!(target_entry.kind, FileKind::Directory) {
            target_entry.relative_path.clone()
        } else {
            parent_relative_directory(&target_entry.relative_path)
        };
        self.paste_file_payload_into_directory(payload, target_directory, cx);
    }

    fn paste_file_payload_into_directory(
        &mut self,
        payload: ClipboardFilePayload,
        target_directory: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for file paste".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        if payload.paths.is_empty() && payload.images.is_empty() {
            self.status_message = "clipboard has no files or images to paste".to_string();
            self.invalidate_file_panel(cx);
            return;
        }
        let directory = file_directory_option(&target_directory).map(str::to_string);
        let expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        self.spawn_file_mutation(
            "pasting clipboard files".to_string(),
            "file paste task failed",
            cx,
            move || {
                let paths = payload
                    .paths
                    .into_iter()
                    .filter(|path| Path::new(path).exists())
                    .collect::<Vec<_>>();
                if paths.is_empty() && payload.images.is_empty() {
                    return Err("clipboard has no files or images to paste".to_string());
                }
                let mut selected = None;
                let mut files = None;
                if !paths.is_empty() {
                    let (next_files, pasted) = runtime_service.import_external_project_files(
                        &project_path,
                        paths,
                        directory.as_deref(),
                    )?;
                    files = Some(next_files);
                    selected = pasted;
                }
                let mut pasted_image_count = 0usize;
                for image in payload.images {
                    let (next_files, path) = runtime_service.write_project_file_bytes(
                        &project_path,
                        directory.as_deref(),
                        &image.file_name,
                        image.bytes,
                    )?;
                    files = Some(next_files);
                    selected = Some(path);
                    pasted_image_count += 1;
                }
                let files = files.unwrap_or_default();
                let file_tree_children =
                    load_file_mutation_children(&runtime_service, &project_path, &expanded_dirs)
                        .map_err(|error| file_mutation_refresh_error("clipboard pasted", error))?;
                let preview = selected.as_ref().map(|path| {
                    file_mutation_text_preview(
                        &runtime_service,
                        &project_path,
                        path,
                        "clipboard file pasted",
                    )
                });
                let git = runtime_service.reload_project_git(&project_path);
                Ok(FileMutationResult {
                    files,
                    file_tree_children,
                    expanded_dirs: expanded_dirs.into_iter().collect(),
                    selection: selected
                        .clone()
                        .map(FileMutationSelection::Single)
                        .unwrap_or_default(),
                    preview,
                    git,
                    status: if pasted_image_count > 0 {
                        format!("clipboard image{} pasted", plural(pasted_image_count))
                    } else {
                        "clipboard file pasted".to_string()
                    },
                    clear_draft: false,
                    saved_editor_path: None,
                    saved_editor_content: None,
                })
            },
        );
    }

    pub(super) fn reveal_selected_file_entry(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_selected_file_system_action("reveal", cx);
    }

    pub(super) fn open_selected_file_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_file_entry() else {
            self.status_message = "no selected file entry to open".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        self.open_file_entry(entry, window, cx);
    }

    pub(super) fn open_selected_file_preview(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.selected_file_entry() else {
            self.status_message = "no selected file entry to preview".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        if matches!(entry.kind, FileKind::Directory) {
            self.status_message = "directories are previewed in the file sidebar".to_string();
            self.invalidate_file_panel(cx);
            return;
        }
        self.open_file_preview_window(entry.relative_path, cx);
    }

    pub(super) fn send_file_path_to_active_terminal(
        &mut self,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for terminal path".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let full_path = Path::new(&project.path).join(&relative_path);
        self.send_to_active_terminal(&shell_quote(&full_path.to_string_lossy()), cx);
        self.status_message = format!("file path sent to terminal: {relative_path}");
        self.invalidate_file_panel(cx);
    }

    pub(super) fn run_selected_file_system_action(&mut self, action: &str, cx: &mut Context<Self>) {
        let Some(entry_path) = self.selected_file_entry.clone() else {
            self.status_message = format!("no selected file entry to {action}");
            self.invalidate_file_panel(cx);
            return;
        };
        self.run_file_system_action(action, entry_path, cx);
    }

    pub(super) fn run_active_file_editor_file_system_action(
        &mut self,
        action: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(entry_path) = self.active_file_editor_tab.clone() else {
            self.status_message = format!("no active file to {action}");
            self.invalidate_file_panel(cx);
            return;
        };
        self.run_file_system_action(action, entry_path, cx);
    }

    pub(super) fn run_file_entry_system_action(
        &mut self,
        action: &str,
        entry_path: String,
        cx: &mut Context<Self>,
    ) {
        self.run_file_system_action(action, entry_path, cx);
    }

    pub(super) fn open_file_entry_external(&mut self, entry_path: String, cx: &mut Context<Self>) {
        self.run_file_system_action("open", entry_path, cx);
    }

    fn run_file_system_action(&mut self, action: &str, entry_path: String, cx: &mut Context<Self>) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = format!("no selected project for file {action}");
            self.invalidate_file_panel(cx);
            return;
        };
        let result = match action {
            "reveal" => self
                .runtime_service
                .reveal_project_file_entry(&project_path, &entry_path),
            "open" => self
                .runtime_service
                .open_project_file_entry(&project_path, &entry_path),
            _ => Err(format!("unknown file action: {action}")),
        };
        match result {
            Ok(()) => self.status_message = format!("file {action} requested: {entry_path}"),
            Err(error) => self.status_message = format!("failed to {action} file entry: {error}"),
        }
        self.invalidate_file_panel(cx);
    }
}
