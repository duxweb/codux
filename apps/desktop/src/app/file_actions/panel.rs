use super::*;
use crate::app::project_actions::FilePickerOpenRequest;

impl CoduxApp {
    pub(in crate::app) fn current_file_panel_state_snapshot(
        &self,
    ) -> super::app_state::FilePanelState {
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

    pub(in crate::app) fn remember_current_file_panel_state(&mut self) {
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

    pub(in crate::app) fn restore_cached_file_panel_state(&mut self) -> bool {
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
    pub(in crate::app) fn detect_project_drive_recovery(&mut self, cx: &mut Context<Self>) {
        // Only local projects sit on this machine's (disconnect-prone) volumes;
        // a remote project's files/git are served by its host, unaffected here.
        let is_local = self
            .state
            .selected_project
            .as_ref()
            .is_some_and(|project| project.runtime_target.is_local());
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
        let (git_path, runtime_target) = self
            .state
            .selected_project
            .as_ref()
            .map(|project| (project.path.clone(), project.runtime_target.clone()))
            .unwrap_or_else(|| (worktree_path.clone(), ProjectRuntimeTarget::Local));
        self.status_message = "project drive reconnected — reloading files and git".to_string();
        // Re-arm the file + git watchers: the originals attached to the now-dead
        // inode and won't recover on their own.
        self.runtime_service
            .watch_project_background(worktree_path, git_path, runtime_target);
        self.reload_project_files_async(cx);
        self.refresh_git_panel_state_async(cx);
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn reload_project_files_async(&mut self, cx: &mut Context<Self>) {
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

    pub(in crate::app) fn apply_worktree_file_panel_load(
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

    pub(in crate::app) fn reset_file_tree_state(&mut self) {
        self.file_tree_expanded_dirs.clear();
        self.file_tree_children.clear();
        self.record_ui_state_clear("file_tree");
    }

    pub(in crate::app) fn file_tree_entry(&self, path: &str) -> Option<FileEntry> {
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

    pub(in crate::app) fn selected_file_entry(&self) -> Option<FileEntry> {
        self.selected_file_entry
            .as_deref()
            .and_then(|path| self.file_tree_entry(path))
    }

    /// "Save as…": copy the selected file to a destination chosen in the file
    /// picker (Save mode), on the project's device (local or its host).
    pub(in crate::app) fn save_as_selected_file_entry(
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
        let project_path = project.path.clone();
        let source_abs = if entry.relative_path.is_empty() {
            project_path.clone()
        } else {
            codux_runtime::path::join_path(&project_path, &entry.relative_path)
        };
        let start_dir = codux_runtime::path::parent_path(&source_abs);
        let runtime_target = project.runtime_target.clone();
        self.open_file_picker_window(
            FilePickerOpenRequest {
                mode: super::types::FilePickerMode::Save,
                target: super::types::FilePickerTarget::SaveFileAs {
                    source_path: source_abs,
                    runtime_target: runtime_target.clone(),
                },
                runtime_target,
                start_path: start_dir,
                default_filename: Some(entry.name.clone()),
            },
            window,
            cx,
        );
    }

    pub(in crate::app) fn clear_file_selection(&mut self) {
        self.selected_file_entry = None;
        self.selected_file_entries.clear();
        self.file_selection_anchor = None;
    }

    pub(in crate::app) fn set_single_file_selection(&mut self, relative_path: String) {
        self.selected_file_entry = Some(relative_path.clone());
        self.selected_file_entries.clear();
        self.file_selection_anchor = Some(relative_path);
    }

    pub(in crate::app) fn prepare_file_context_menu_selection(
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

    pub(in crate::app) fn reload_file_tree_directory(&mut self, directory_path: &str) {
        let Some(project_path) = self.selected_worktree_path() else {
            return;
        };
        let children = self
            .runtime_service
            .reload_project_files(&project_path, Some(directory_path));
        self.file_tree_children
            .insert(directory_path.to_string(), children);
    }

    pub(in crate::app) fn refresh_file_tree_state(&mut self) {
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

    pub(in crate::app) fn prune_missing_file_tree_directories(&mut self) {
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

    pub(in crate::app) fn toggle_file_tree_directory(
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
}
