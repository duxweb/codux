use super::*;

type CleanFileEditorReloadResult = Vec<(String, std::result::Result<(String, bool), String>)>;

impl CoduxApp {
    pub(super) fn spawn_file_editor_state_load(
        &mut self,
        key: String,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        if self.file_editor_states.contains_key(&key)
            || !self.file_editor_loading_states.insert(key.clone())
        {
            return;
        }
        let Some(worktree_path) = self.selected_worktree_path() else {
            self.file_editor_loading_states.remove(&key);
            return;
        };
        let runtime_service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND,
                {
                    let worktree_path = worktree_path.clone();
                    let relative_path = relative_path.clone();
                    move || {
                        runtime_service
                            .read_project_file_edit_buffer(&worktree_path, &relative_path)
                    }
                },
            )
            .await
            .ok();
            let _ = this.update_in(cx, |app, window, cx| {
                app.apply_file_editor_state_load(key, relative_path, result, window, cx);
            });
        })
        .detach();
    }

    fn apply_file_editor_state_load(
        &mut self,
        key: String,
        relative_path: String,
        result: Option<std::result::Result<(String, bool), String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_editor_loading_states.remove(&key);
        let is_current_file_context = self.file_editor_state_key(&relative_path) == key;
        match result {
            Some(Ok((content, editable))) => {
                let language = file_language_for_path(&relative_path).to_string();
                if is_current_file_context
                    && let Some(tab) = self
                        .file_editor_tabs
                        .iter_mut()
                        .find(|tab| tab.relative_path == relative_path)
                {
                    tab.editable = editable;
                    tab.language = language.clone();
                }
                self.ensure_file_editor_state(
                    key,
                    relative_path.clone(),
                    language,
                    content,
                    window,
                    cx,
                );
                if is_current_file_context
                    && self.active_file_editor_tab.as_deref() == Some(relative_path.as_str())
                    && let Some(editor) = self.active_file_editor_state()
                {
                    editor.update(cx, |state, cx| state.focus(window, cx));
                }
            }
            Some(Err(error)) => {
                if is_current_file_context {
                    self.status_message = format!("failed to load file editor: {error}");
                    self.invalidate_status_bar(cx);
                }
            }
            None => {
                if is_current_file_context {
                    self.status_message = "failed to load file editor".to_string();
                    self.invalidate_status_bar(cx);
                }
            }
        }
        if is_current_file_context
            && self.workspace_view == WorkspaceView::Files
            && !self.update_file_editor_workspace_view(cx)
        {
            self.invalidate_ui_region(cx, UiRegion::WorkspaceBody);
        }
        if is_current_file_context {
            self.invalidate_ui_region(cx, UiRegion::FileSidebar);
        }
    }

    pub(in crate::app) fn reload_clean_file_editor_tabs_for_file_events(
        &mut self,
        events: &[FileChangeEvent],
        cx: &mut Context<Self>,
    ) -> usize {
        let Some(worktree_path) = self.selected_worktree_path() else {
            return 0;
        };
        let changed_paths = changed_file_event_relative_paths(events, &worktree_path);
        if changed_paths.is_empty() {
            return 0;
        }
        let reload_paths = self
            .file_editor_tabs
            .iter()
            .filter(|tab| {
                !tab.dirty
                    && changed_paths.contains(tab.relative_path.as_str())
                    && file_preview_kind_for_path(&tab.relative_path) != FilePreviewKind::Image
            })
            .map(|tab| tab.relative_path.clone())
            .collect::<Vec<_>>();
        if reload_paths.is_empty() {
            return 0;
        }

        let runtime_service = self.runtime_service.clone();
        let scope_key = super::app_state::current_worktree_scope_key(&self.state);
        let generation = self.project_switch_generation;
        self.runtime_trace(
            "files",
            &format!("external_reload queued count={}", reload_paths.len()),
        );
        let reload_count = reload_paths.len();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                {
                    let worktree_path = worktree_path.clone();
                    let reload_paths = reload_paths.clone();
                    move || {
                        reload_paths
                            .into_iter()
                            .map(|relative_path| {
                                let result = runtime_service
                                    .read_project_file_edit_buffer(&worktree_path, &relative_path);
                                (relative_path, result)
                            })
                            .collect::<Vec<_>>()
                    }
                },
            )
            .await
            .ok();
            let _ = this.update_in(cx, |app, window, cx| {
                if app.project_switch_generation != generation
                    || super::app_state::current_worktree_scope_key(&app.state) != scope_key
                {
                    return;
                }
                app.apply_clean_file_editor_tab_reload(result, window, cx);
            });
        })
        .detach();

        reload_count
    }

    fn apply_clean_file_editor_tab_reload(
        &mut self,
        result: Option<CleanFileEditorReloadResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(result) = result else {
            self.runtime_trace("files", "external_reload failed result=missing");
            return;
        };
        let mut changed = false;
        let mut applied = 0usize;
        for (relative_path, result) in result {
            let Some(tab) = self
                .file_editor_tabs
                .iter()
                .find(|tab| tab.relative_path == relative_path)
            else {
                continue;
            };
            if tab.dirty {
                continue;
            }
            let Ok((content, editable)) = result else {
                self.runtime_trace(
                    "files",
                    &format!("external_reload skipped path={relative_path} reason=read_failed"),
                );
                continue;
            };
            let language = file_language_for_path(&relative_path).to_string();

            let key = self.file_editor_state_key(&relative_path);
            if let Some(editor) = self.file_editor_states.get(&key) {
                editor.update(cx, |state, cx| state.set_value(content.clone(), window, cx));
            } else {
                self.ensure_file_editor_state(
                    key,
                    relative_path.clone(),
                    language.clone(),
                    content.clone(),
                    window,
                    cx,
                );
            }
            if let Some(tab) = self
                .file_editor_tabs
                .iter_mut()
                .find(|tab| tab.relative_path == relative_path)
            {
                tab.editable = editable;
                tab.language = language;
                tab.dirty = false;
            }
            if self.active_file_editor_tab.as_deref() == Some(relative_path.as_str()) {
                self.file_preview = content;
                self.file_editable = editable;
                self.file_dirty = false;
            }
            changed = true;
            applied += 1;
        }
        if !changed {
            return;
        }
        self.runtime_trace("files", &format!("external_reload applied count={applied}"));
        if self.workspace_view == WorkspaceView::Files {
            if !self.update_file_editor_workspace_view(cx) {
                self.invalidate_ui_region(cx, UiRegion::WorkspaceBody);
            }
            self.invalidate_ui_region(cx, UiRegion::WorkspaceChrome);
        }
        self.invalidate_ui(cx, [UiRegion::FileSidebar, UiRegion::StatusBar]);
    }

    pub(in crate::app) fn ensure_file_editor_state(
        &mut self,
        key: String,
        relative_path: String,
        language: String,
        content: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<InputState> {
        if let Some(state) = self.file_editor_states.get(&key) {
            let state = state.clone();
            self.touch_file_editor_state(&key);
            return state;
        }

        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor(language)
                .folding(false)
                .multi_line(true)
                .tab_size(TabSize {
                    tab_size: 4,
                    ..Default::default()
                })
                .default_value(content)
        });
        cx.subscribe_in(&state, window, move |app, _state, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                app.mark_file_editor_dirty(&relative_path, true, window, cx);
            }
        })
        .detach();
        self.file_editor_states.insert(key.clone(), state.clone());
        self.touch_file_editor_state(&key);
        self.prune_file_editor_states();
        state
    }

    /// Mark an editor-state key as most-recently-used.
    fn touch_file_editor_state(&mut self, key: &str) {
        self.file_editor_state_lru
            .retain(|existing| existing != key);
        self.file_editor_state_lru.push(key.to_string());
    }

    /// Bound the editor-state cache so opening files across many projects does
    /// not retain every file's rope + syntax tree forever. Evicts least-recently
    /// used states beyond `MAX_FILE_EDITOR_STATES`, but never a state that is
    /// still referenced by an open tab or has unsaved (dirty) changes.
    fn prune_file_editor_states(&mut self) {
        const MAX_FILE_EDITOR_STATES: usize = 12;
        if self.file_editor_states.len() <= MAX_FILE_EDITOR_STATES {
            return;
        }
        let dirty_paths: Vec<String> = self
            .file_editor_tabs
            .iter()
            .filter(|tab| tab.dirty)
            .map(|tab| tab.relative_path.clone())
            .collect();
        let protected: std::collections::HashSet<String> = dirty_paths
            .iter()
            .map(|path| self.file_editor_state_key(path))
            .collect();
        let mut lru = std::mem::take(&mut self.file_editor_state_lru);
        let mut index = 0;
        while self.file_editor_states.len() > MAX_FILE_EDITOR_STATES && index < lru.len() {
            let key = lru[index].clone();
            if protected.contains(&key) {
                index += 1;
                continue;
            }
            self.file_editor_states.remove(&key);
            self.file_editor_loading_states.remove(&key);
            lru.remove(index);
        }
        // Drop LRU entries for states that no longer exist.
        lru.retain(|key| self.file_editor_states.contains_key(key));
        self.file_editor_state_lru = lru;
    }

    pub(in crate::app) fn apply_file_editor_layout(&mut self, layout: FileEditorLayoutSummary) {
        if layout.tabs.is_empty() {
            return;
        }
        let (tabs, active_path) = super::app_state::file_editor_tabs_from_layout(layout);
        self.file_editor_tabs = tabs;
        self.active_file_editor_tab = active_path;
        if let Some(active) = self.active_file_editor_tab.clone() {
            self.set_single_file_selection(active);
        }
    }

    pub(in crate::app) fn load_current_file_editor_layout_async(&mut self, cx: &mut Context<Self>) {
        let Some(owner_id) = super::ai_runtime_status::terminal_layout_owner_id(&self.state) else {
            return;
        };
        let runtime_service = self.runtime_service.clone();
        let scope_key = super::app_state::current_worktree_scope_key(&self.state);
        let generation = self.project_switch_generation;
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || runtime_service.reload_file_editor_layout(Some(&owner_id)),
            )
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                let Some(layout) = result else {
                    return;
                };
                if app.project_switch_generation != generation
                    || super::app_state::current_worktree_scope_key(&app.state) != scope_key
                {
                    return;
                }
                app.apply_file_editor_layout(layout);
                app.invalidate_file_panel(cx);
                if app.workspace_view == WorkspaceView::Files {
                    app.invalidate_ui_region(cx, UiRegion::WorkspaceBody);
                }
            });
        })
        .detach();
    }

    pub(in crate::app) fn persist_file_editor_layout_async(&self, cx: &mut Context<Self>) {
        let Some(owner_id) = super::ai_runtime_status::terminal_layout_owner_id(&self.state) else {
            return;
        };
        let tabs = self
            .file_editor_tabs
            .iter()
            .map(|tab| FileEditorTabSummary {
                path: tab.relative_path.clone(),
                label: tab.label.clone(),
                language: tab.language.clone(),
            })
            .collect::<Vec<_>>();
        let active_path = self.active_file_editor_tab.clone();
        let runtime_service = self.runtime_service.clone();
        cx.spawn(async move |_: gpui::WeakEntity<Self>, _cx| {
            let owner_id_for_log = owner_id.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                runtime_service.save_file_editor_layout(&owner_id, tabs, active_path)
            })
            .await;
            match result {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => codux_runtime::runtime_trace::runtime_trace(
                    "config",
                    &format!(
                        "failed to persist file editor layout {}: {error}",
                        owner_id_for_log
                    ),
                ),
                Err(error) => codux_runtime::runtime_trace::runtime_trace(
                    "config",
                    &format!(
                        "file editor layout writer failed {}: {error}",
                        owner_id_for_log
                    ),
                ),
            }
        })
        .detach();
    }

    pub(in crate::app) fn file_editor_state_key(&self, relative_path: &str) -> String {
        if let Some(key) = current_worktree_scope_key(&self.state) {
            format!("{}:{}:{}", key.project_id, key.worktree_id, relative_path)
        } else {
            relative_path.to_string()
        }
    }

    fn file_editor_preview_path(&self, relative_path: &str) -> Option<String> {
        let worktree_path = self.selected_worktree_path()?;
        Some(codux_runtime::path::join_path(
            &worktree_path,
            relative_path,
        ))
    }

    pub(in crate::app) fn file_editor_workspace_snapshot(&self) -> FileEditorWorkspaceSnapshot {
        let tabs = self.file_editor_tabs.clone();
        let active_path = self.active_file_editor_tab.clone();
        let active_tab = self
            .file_editor_tabs
            .iter()
            .find(|tab| Some(tab.relative_path.as_str()) == active_path.as_deref())
            .cloned();
        let active_editor = self.active_file_editor_state();
        let active_loading = active_editor.is_none()
            && active_path
                .as_deref()
                .map(|path| {
                    self.file_editor_loading_states
                        .contains(&self.file_editor_state_key(path))
                })
                .unwrap_or(false);
        FileEditorWorkspaceSnapshot {
            tabs,
            active_path,
            active_preview_path: self.active_file_editor_tab.as_deref().and_then(|path| {
                matches!(
                    file_preview_kind_for_path(path),
                    FilePreviewKind::Image | FilePreviewKind::Markdown
                )
                .then(|| self.file_editor_preview_path(path))
                .flatten()
            }),
            single_window: self.window_mode == AppWindowMode::FileEditor,
            active_tab,
            active_editor,
            active_loading,
            split_active: self.workspace_view == WorkspaceView::Terminal
                && self.workspace_split == Some(WorkspaceSplitKind::FileEditor),
        }
    }

    pub(in crate::app) fn file_preview_window_snapshot(&self) -> FilePreviewWindowSnapshot {
        let relative_path = self.file_preview_window_path.clone();
        let full_path = relative_path
            .as_deref()
            .and_then(|path| self.file_editor_preview_path(path));
        let kind = relative_path
            .as_deref()
            .map(file_preview_kind_for_path)
            .unwrap_or(FilePreviewKind::Text);
        FilePreviewWindowSnapshot {
            relative_path,
            full_path,
            kind,
            content: self.file_preview_window_content.clone(),
            error: self.file_preview_window_error.clone(),
            language: self.state.settings.language.clone(),
        }
    }

    pub(in crate::app) fn load_file_preview_window_content_async(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(relative_path) = self.file_preview_window_path.clone() else {
            self.file_preview_window_error = Some("No file selected.".to_string());
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        };
        match file_preview_kind_for_path(&relative_path) {
            FilePreviewKind::Image => {
                self.file_preview_window_content.clear();
                self.file_preview_window_error = None;
                self.invalidate_ui_region(cx, UiRegion::Root);
            }
            FilePreviewKind::External => {
                self.file_preview_window_error = Some("Unsupported preview format.".to_string());
                self.invalidate_ui_region(cx, UiRegion::Root);
            }
            FilePreviewKind::Markdown | FilePreviewKind::Text => {
                let Some(worktree_path) = self.selected_worktree_path() else {
                    self.file_preview_window_error = Some("No selected project.".to_string());
                    self.invalidate_ui_region(cx, UiRegion::Root);
                    return;
                };
                self.file_preview_window_content.clear();
                self.file_preview_window_error = None;
                let runtime_service = self.runtime_service.clone();
                cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                    let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                        codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND,
                        {
                            let worktree_path = worktree_path.clone();
                            let relative_path = relative_path.clone();
                            move || {
                                runtime_service
                                    .read_project_file_edit_buffer(&worktree_path, &relative_path)
                            }
                        },
                    )
                    .await
                    .unwrap_or_else(|error| {
                        Err(format!("failed to join file preview load: {error}"))
                    });
                    let _ = this.update(cx, |app, cx| {
                        match result {
                            Ok((content, _editable)) => {
                                app.file_preview_window_content = content;
                                app.file_preview_window_error = None;
                            }
                            Err(error) => {
                                app.file_preview_window_content.clear();
                                app.file_preview_window_error = Some(error);
                            }
                        }
                        app.invalidate_ui_region(cx, UiRegion::Root);
                    });
                })
                .detach();
            }
        }
    }
}
