use super::*;

impl CoduxApp {
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
        self.file_name_draft_kind = None;
        self.file_name_draft_target = None;
        self.file_name_draft_value.clear();
        self.file_name_draft_select_all = false;
        self.runtime_trace(
            "files",
            &format!(
                "cache_restore hit project={} worktree={} expanded_dirs={expanded_count}",
                scope_key.project_id, scope_key.worktree_id
            ),
        );
        true
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
                } else if self.state.settings.file_open_default == "preview" {
                    self.open_file_preview_window(entry.relative_path, cx);
                } else {
                    self.open_file_editor_window(entry.relative_path, cx);
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

        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file move".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        self.status_message = format!(
            "moving {} file item{} to {target_directory_path}",
            paths.len(),
            plural(paths.len())
        );
        let project_path = project.path.clone();
        let conflicts = paths
            .iter()
            .filter_map(|path| {
                let name = path.rsplit('/').next()?;
                let target = join_relative_child_path(&target_directory_path, name);
                Path::new(&project_path)
                    .join(&target)
                    .exists()
                    .then_some(target)
            })
            .collect::<Vec<_>>();
        if conflicts.is_empty() {
            self.move_file_entries_to_directory_confirmed(paths, target_directory_path, false, cx);
            return;
        }

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

            let _ = this.update(cx, |app, cx| match result {
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
            });
        })
        .detach();
        self.invalidate_file_panel(cx);
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
        let project_path = project.path.clone();
        let mut moved = Vec::new();
        let mut latest_files = None;
        for path in paths {
            let result = if overwrite {
                self.runtime_service.move_project_file_entry_overwrite(
                    &project_path,
                    &path,
                    &target_directory_path,
                    file_directory_option(&self.file_directory),
                )
            } else {
                self.runtime_service.move_project_file_entry(
                    &project_path,
                    &path,
                    &target_directory_path,
                    file_directory_option(&self.file_directory),
                )
            };
            match result {
                Ok((files, moved_path)) => {
                    latest_files = Some(files);
                    moved.push(moved_path);
                    self.file_tree_expanded_dirs.retain(|expanded| {
                        expanded != &path && !expanded.starts_with(&format!("{path}/"))
                    });
                }
                Err(error) => {
                    self.status_message = format!("failed to move {path}: {error}");
                    self.invalidate_file_panel(cx);
                    return;
                }
            }
        }

        if let Some(files) = latest_files {
            self.state.files = files;
        }
        self.refresh_file_tree_state();
        if moved.len() == 1 {
            self.set_single_file_selection(moved[0].clone());
        } else {
            self.selected_file_entries = moved.iter().cloned().collect();
            self.selected_file_entry = moved.last().cloned();
            self.file_selection_anchor = self.selected_file_entry.clone();
        }
        self.state.git = self.runtime_service.reload_project_git(&project_path);
        self.normalize_selected_git_file();
        self.normalize_selected_git_branch();
        self.status_message = format!("moved {} file item{}", moved.len(), plural(moved.len()));
        self.invalidate_file_panel(cx);
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
        self.start_file_name_draft(
            FileNameDraftKind::CreateFile,
            Some("undefined".to_string()),
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
            Some("undefined".to_string()),
            cx,
        );
    }

    pub(super) fn start_file_name_draft(
        &mut self,
        kind: FileNameDraftKind,
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
        self.file_name_draft_select_all = true;
        self.file_name_draft_value = value;
        self.workspace_view = WorkspaceView::Files;
        self.assistant_panel = Some(AssistantPanel::FileManager);
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
            self.file_name_draft_kind = None;
            self.file_name_draft_target = None;
            self.file_name_draft_value.clear();
            self.file_name_draft_select_all = false;
            self.status_message = "file name edit canceled".to_string();
            self.invalidate_file_panel(cx);
        } else {
            self.confirm_file_name_draft(window, cx);
        }
    }

    pub(super) fn cancel_file_name_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.file_name_draft_kind = None;
        self.file_name_draft_target = None;
        self.file_name_draft_value.clear();
        self.file_name_draft_select_all = false;
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
            FileNameDraftKind::CreateFile => self.create_project_file_entry(false, name, cx),
            FileNameDraftKind::CreateDirectory => self.create_project_file_entry(true, name, cx),
            FileNameDraftKind::Rename => self.rename_selected_file_entry_to(name, cx),
        }
    }

    pub(super) fn create_project_file_entry(
        &mut self,
        directory: bool,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file creation".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let project_path = project.path.clone();
        let parent = file_directory_option(&self.file_directory).map(str::to_string);
        let result = if directory {
            self.runtime_service
                .create_project_directory(&project_path, parent.as_deref(), &name)
        } else {
            self.runtime_service
                .create_project_file(&project_path, parent.as_deref(), &name)
        };
        match result {
            Ok(files) => {
                let relative_path = join_relative_child_path(&self.file_directory, &name);
                self.state.files = files;
                self.refresh_file_tree_state();
                self.set_single_file_selection(relative_path.clone());
                self.file_preview = if directory {
                    "directory created".to_string()
                } else {
                    String::new()
                };
                self.file_editable = !directory;
                self.file_dirty = false;
                self.state.git = self.runtime_service.reload_project_git(&project_path);
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.file_name_draft_kind = None;
                self.file_name_draft_target = None;
                self.file_name_draft_value.clear();
                self.status_message = format!(
                    "{} created: {relative_path}",
                    if directory { "directory" } else { "file" }
                );
            }
            Err(error) => {
                self.status_message = format!(
                    "failed to create {}: {error}",
                    if directory { "directory" } else { "file" }
                );
            }
        }
        self.invalidate_file_panel(cx);
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

                let _ = this.update(cx, |app, cx| match result {
                    Ok(true) => app.delete_file_entries(paths, cx),
                    Ok(false) => {
                        app.status_message = "file deletion canceled".to_string();
                        app.invalidate_file_panel(cx);
                    }
                    Err(error) => {
                        app.status_message = format!("failed to show delete confirmation: {error}");
                        app.invalidate_file_panel(cx);
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
        let directory = file_directory_option(&self.file_directory).map(str::to_string);
        let count = paths.len();
        let mut latest_files = None;
        for entry_path in &paths {
            match self.runtime_service.delete_project_file_entry(
                &project_path,
                entry_path,
                directory.as_deref(),
            ) {
                Ok(files) => {
                    latest_files = Some(files);
                    self.file_tree_expanded_dirs.retain(|path| {
                        path != entry_path && !path.starts_with(&format!("{entry_path}/"))
                    });
                }
                Err(error) => {
                    self.status_message = format!("failed to delete file entry: {error}");
                    self.invalidate_file_panel(cx);
                    return;
                }
            }
        }
        if let Some(files) = latest_files {
            self.state.files = files;
        }
        self.refresh_file_tree_state();
        self.clear_file_selection();
        self.file_preview = "select a file to preview it".to_string();
        self.file_editable = false;
        self.file_dirty = false;
        self.state.git = self.runtime_service.reload_project_git(&project_path);
        self.normalize_selected_git_file();
        self.normalize_selected_git_branch();
        self.status_message = format!("moved {count} file item{} to trash", plural(count));
        self.invalidate_file_panel(cx);
    }

    pub(super) fn save_selected_file_preview(
        &mut self,
        window: &mut Window,
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
        match self
            .runtime_service
            .write_project_file(&project_path, &entry_path, &content)
        {
            Ok(preview) => {
                self.file_preview = preview;
                self.file_editable = true;
                self.file_dirty = false;
                self.mark_file_editor_dirty(&entry_path, false, window, cx);
                self.normalize_file_search_index();
                self.state.files = self.runtime_service.reload_project_files(
                    &project_path,
                    file_directory_option(&self.file_directory),
                );
                self.refresh_file_tree_state();
                self.set_single_file_selection(entry_path.clone());
                self.state.git = self.runtime_service.reload_project_git(&project_path);
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.status_message = format!("file saved: {entry_path}");
                self.persist_file_editor_layout_async(cx);
            }
            Err(error) => self.status_message = format!("failed to save file: {error}"),
        }
        self.invalidate_file_panel(cx);
    }

    pub(super) fn reload_active_file_editor_tab(
        &mut self,
        window: &mut Window,
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

        match self
            .runtime_service
            .read_project_file_edit_buffer(&project_path, &entry_path)
        {
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
        self.start_file_name_draft(FileNameDraftKind::Rename, None, cx);
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
        match self.runtime_service.rename_project_file_entry(
            &project_path,
            &entry_path,
            &new_name,
            file_directory_option(&self.file_directory),
        ) {
            Ok((files, renamed_path)) => {
                self.state.files = files;
                let was_expanded = self.file_tree_expanded_dirs.remove(&entry_path);
                self.file_tree_expanded_dirs
                    .retain(|path| !path.starts_with(&format!("{entry_path}/")));
                if was_expanded {
                    self.file_tree_expanded_dirs.insert(renamed_path.clone());
                }
                self.refresh_file_tree_state();
                self.set_single_file_selection(renamed_path.clone());
                if matches!(entry.kind, FileKind::File) {
                    match self
                        .runtime_service
                        .read_project_file_edit_buffer(&project_path, &renamed_path)
                    {
                        Ok((content, editable)) => {
                            self.file_preview = content;
                            self.file_editable = editable;
                            self.file_dirty = false;
                        }
                        Err(error) => {
                            self.file_preview = format!("failed to reload renamed file: {error}");
                            self.file_editable = false;
                            self.file_dirty = false;
                        }
                    }
                } else {
                    self.file_preview = "directory renamed".to_string();
                    self.file_editable = false;
                    self.file_dirty = false;
                }
                self.state.git = self.runtime_service.reload_project_git(&project_path);
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.file_name_draft_kind = None;
                self.file_name_draft_target = None;
                self.file_name_draft_value.clear();
                self.status_message = format!("renamed file entry: {renamed_path}");
            }
            Err(error) => self.status_message = format!("failed to rename file entry: {error}"),
        }
        self.invalidate_file_panel(cx);
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
        match self.runtime_service.copy_project_file_entry(
            &project_path,
            &entry_path,
            file_directory_option(&self.file_directory),
        ) {
            Ok((files, copied_path)) => {
                self.state.files = files;
                self.refresh_file_tree_state();
                self.set_single_file_selection(copied_path.clone());
                if matches!(entry.kind, FileKind::File) {
                    match self
                        .runtime_service
                        .read_project_file_edit_buffer(&project_path, &copied_path)
                    {
                        Ok((content, editable)) => {
                            self.file_preview = content;
                            self.file_editable = editable;
                            self.file_dirty = false;
                        }
                        Err(error) => {
                            self.file_preview = format!("failed to load copied file: {error}");
                            self.file_editable = false;
                            self.file_dirty = false;
                        }
                    }
                } else {
                    self.file_preview = "directory copied".to_string();
                    self.file_editable = false;
                    self.file_dirty = false;
                }
                self.state.git = self.runtime_service.reload_project_git(&project_path);
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.status_message = format!("copied file entry: {copied_path}");
            }
            Err(error) => self.status_message = format!("failed to copy file entry: {error}"),
        }
        self.invalidate_file_panel(cx);
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
        let paths = payload
            .paths
            .into_iter()
            .filter(|path| Path::new(path).exists())
            .collect::<Vec<_>>();
        if paths.is_empty() && payload.images.is_empty() {
            self.status_message = "clipboard has no files or images to paste".to_string();
            self.invalidate_file_panel(cx);
            return;
        }
        let directory = file_directory_option(&target_directory).map(str::to_string);

        let mut selected = None;
        let mut latest_files = None;
        if !paths.is_empty() {
            match self.runtime_service.import_external_project_files(
                &project_path,
                paths,
                directory.as_deref(),
            ) {
                Ok((files, pasted)) => {
                    latest_files = Some(files);
                    selected = pasted;
                }
                Err(error) => {
                    self.status_message = format!("failed to paste clipboard file: {error}");
                    self.invalidate_file_panel(cx);
                    return;
                }
            }
        }

        let mut pasted_image_count = 0usize;
        for image in payload.images {
            match self.runtime_service.write_project_file_bytes(
                &project_path,
                directory.as_deref(),
                &image.file_name,
                image.bytes,
            ) {
                Ok((files, path)) => {
                    latest_files = Some(files);
                    selected = Some(path);
                    pasted_image_count += 1;
                }
                Err(error) => {
                    self.status_message = format!("failed to paste clipboard image: {error}");
                    self.invalidate_file_panel(cx);
                    return;
                }
            }
        }

        if let Some(files) = latest_files {
            self.state.files = files;
        }
        self.refresh_file_tree_state();
        if let Some(path) = selected.clone() {
            self.set_single_file_selection(path.clone());
            self.load_file_preview_after_file_paste(&project_path, &path);
        }
        self.file_dirty = false;
        self.state.git = self.runtime_service.reload_project_git(&project_path);
        self.normalize_selected_git_file();
        self.normalize_selected_git_branch();
        self.status_message = if pasted_image_count > 0 {
            format!("clipboard image{} pasted", plural(pasted_image_count))
        } else {
            "clipboard file pasted".to_string()
        };
        self.invalidate_file_panel(cx);
    }

    fn load_file_preview_after_file_paste(&mut self, project_path: &str, path: &str) {
        if matches!(
            crate::app::file_editor::file_preview_kind_for_path(path),
            crate::app::file_editor::FilePreviewKind::Image
                | crate::app::file_editor::FilePreviewKind::External
        ) {
            self.file_preview = "clipboard file pasted".to_string();
            self.file_editable = false;
            return;
        }
        match self
            .runtime_service
            .read_project_file_edit_buffer(project_path, path)
        {
            Ok((content, editable)) => {
                self.file_preview = content;
                self.file_editable = editable;
            }
            Err(_) => {
                self.file_preview = "clipboard file pasted".to_string();
                self.file_editable = false;
            }
        }
    }

    pub(super) fn reveal_selected_file_entry(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_selected_file_system_action("reveal", cx);
    }

    pub(super) fn open_selected_file_entry(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_selected_file_system_action("open", cx);
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
