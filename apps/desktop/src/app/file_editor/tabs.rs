use super::*;

impl CoduxApp {
    pub(in crate::app) fn add_file_editor_window_tab(&mut self, relative_path: String) {
        if self.selected_worktree_path().is_none() {
            self.status_message = "no selected project to open file".to_string();
            return;
        }
        self.file_editor_tabs.push(FileEditorTab {
            label: file_editor_label(&relative_path),
            relative_path: relative_path.clone(),
            editable: file_preview_kind_for_path(&relative_path) == FilePreviewKind::Text,
            dirty: false,
            language: file_language_for_path(&relative_path).to_string(),
        });
        self.active_file_editor_tab = Some(relative_path.clone());
        self.set_single_file_selection(relative_path);
    }

    pub(in crate::app) fn open_file_editor_tab(
        &mut self,
        relative_path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_worktree_path().is_none() {
            self.status_message = "no selected project to open file".to_string();
            self.invalidate_status_bar(cx);
            return;
        }
        let key = self.file_editor_state_key(&relative_path);

        let tab_exists = self
            .file_editor_tabs
            .iter()
            .any(|tab| tab.relative_path == relative_path);

        if !tab_exists {
            self.file_editor_tabs.push(FileEditorTab {
                label: file_editor_label(&relative_path),
                relative_path: relative_path.clone(),
                editable: true,
                dirty: false,
                language: file_language_for_path(&relative_path).to_string(),
            });
            self.ensure_file_editor_state_for_path(relative_path.clone(), window, cx);
        } else {
            self.ensure_file_editor_state_for_path(relative_path.clone(), window, cx);
        }

        self.workspace_view = WorkspaceView::Files;
        self.assistant_panel = Some(AssistantPanel::FileManager);
        self.active_file_editor_tab = Some(relative_path.clone());
        self.set_single_file_selection(relative_path.clone());
        if let Some(editor) = self.file_editor_states.get(&key) {
            editor.update(cx, |state, cx| state.focus(window, cx));
        }
        self.persist_file_editor_layout_async(cx);
        self.status_message = format!("file opened: {relative_path}");
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceAssistant,
                UiRegion::WorkspaceBody,
                UiRegion::FileSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    /// Open a file as the split-mode panel: add (or focus) a file-editor tab and
    /// show the editor next to the terminal, without leaving the terminal view.
    /// Reuses the same tab pool as the full Files view, so a second open just
    /// adds another tab to the existing split.
    pub(in crate::app) fn open_file_editor_split(
        &mut self,
        relative_path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_worktree_path().is_none() {
            self.status_message = "no selected project to open file".to_string();
            self.invalidate_status_bar(cx);
            return;
        }
        let key = self.file_editor_state_key(&relative_path);
        let tab_exists = self
            .file_editor_tabs
            .iter()
            .any(|tab| tab.relative_path == relative_path);
        if !tab_exists {
            self.file_editor_tabs.push(FileEditorTab {
                label: file_editor_label(&relative_path),
                relative_path: relative_path.clone(),
                editable: true,
                dirty: false,
                language: file_language_for_path(&relative_path).to_string(),
            });
        }
        self.ensure_file_editor_state_for_path(relative_path.clone(), window, cx);

        self.workspace_split = Some(WorkspaceSplitKind::FileEditor);
        self.active_file_editor_tab = Some(relative_path.clone());
        self.set_single_file_selection(relative_path.clone());
        if let Some(editor) = self.file_editor_states.get(&key) {
            editor.update(cx, |state, cx| state.focus(window, cx));
        }
        self.persist_file_editor_layout_async(cx);
        self.status_message = format!("file opened in split: {relative_path}");
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceBody,
                UiRegion::FileSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(in crate::app) fn select_file_editor_tab(
        &mut self,
        relative_path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_file_editor_tab = Some(relative_path.clone());
        self.set_single_file_selection(relative_path.clone());
        self.ensure_file_editor_state_for_path(relative_path, window, cx);
        if let Some(editor) = self.active_file_editor_state() {
            editor.update(cx, |state, cx| state.focus(window, cx));
        }
        self.persist_file_editor_layout_async(cx);
        if !self.update_file_editor_workspace_view(cx) {
            self.invalidate_ui_region(cx, UiRegion::WorkspaceBody);
        }
        self.invalidate_ui(cx, [UiRegion::FileSidebar, UiRegion::StatusBar]);
    }

    pub(in crate::app) fn close_file_editor_tab(
        &mut self,
        relative_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .file_editor_tabs
            .iter()
            .position(|tab| tab.relative_path == relative_path)
        else {
            return;
        };
        self.file_editor_tabs.remove(index);
        let key = self.file_editor_state_key(&relative_path);
        self.file_editor_states.remove(&key);
        self.file_editor_state_lru
            .retain(|existing| existing != &key);

        if self.active_file_editor_tab.as_deref() == Some(relative_path.as_str()) {
            self.active_file_editor_tab = self
                .file_editor_tabs
                .get(index)
                .or_else(|| {
                    index
                        .checked_sub(1)
                        .and_then(|prev| self.file_editor_tabs.get(prev))
                })
                .map(|tab| tab.relative_path.clone());
        }
        // Closing the last tab collapses the split panel back to a plain
        // terminal workspace.
        if self.file_editor_tabs.is_empty() {
            self.workspace_split = None;
        }
        self.persist_file_editor_layout_async(cx);
        if !self.update_file_editor_workspace_view(cx) {
            self.invalidate_ui_region(cx, UiRegion::WorkspaceBody);
        }
        self.invalidate_ui(cx, [UiRegion::FileSidebar, UiRegion::StatusBar]);
    }

    /// Hide the split file-editor panel and return to a plain terminal
    /// workspace. The open file tabs are kept (still reachable from the Files
    /// view), so opening another file re-shows the split with them.
    pub(in crate::app) fn close_file_editor_split(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_split.is_none() {
            return;
        }
        self.workspace_split = None;
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceBody,
                UiRegion::StatusBar,
            ],
        );
    }

    /// Whether the active file-editor tab's input currently holds focus. Used to
    /// route Cmd+W to the file tab (instead of the terminal) while the split is
    /// open and the user is editing the file.
    pub(in crate::app) fn active_file_editor_split_focused(
        &self,
        window: &Window,
        cx: &gpui::App,
    ) -> bool {
        self.active_file_editor_state()
            .map(|state| gpui::Focusable::focus_handle(state.read(cx), cx).is_focused(window))
            .unwrap_or(false)
    }

    pub(in crate::app) fn reorder_file_editor_tabs(
        &mut self,
        next_paths: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        if next_paths.len() != self.file_editor_tabs.len() {
            return;
        }

        let current_paths = self
            .file_editor_tabs
            .iter()
            .map(|tab| tab.relative_path.clone())
            .collect::<Vec<_>>();
        if current_paths == next_paths {
            return;
        }

        let mut remaining = std::mem::take(&mut self.file_editor_tabs);
        let mut reordered = Vec::with_capacity(remaining.len());
        for path in next_paths {
            let Some(index) = remaining.iter().position(|tab| tab.relative_path == path) else {
                self.file_editor_tabs = remaining;
                return;
            };
            reordered.push(remaining.remove(index));
        }
        if !remaining.is_empty() {
            self.file_editor_tabs = remaining;
            return;
        }

        self.file_editor_tabs = reordered;
        self.persist_file_editor_layout_async(cx);
        if !self.update_file_editor_workspace_view(cx) {
            self.invalidate_ui_region(cx, UiRegion::WorkspaceBody);
        }
        self.invalidate_ui_region(cx, UiRegion::WorkspaceChrome);
    }

    pub(in crate::app) fn mark_file_editor_dirty(
        &mut self,
        relative_path: &str,
        dirty: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        if let Some(tab) = self
            .file_editor_tabs
            .iter_mut()
            .find(|tab| tab.relative_path == relative_path)
            && tab.dirty != dirty
        {
            tab.dirty = dirty;
            changed = true;
        }
        if self.active_file_editor_tab.as_deref() == Some(relative_path) && self.file_dirty != dirty
        {
            self.file_dirty = dirty;
            changed = true;
        }
        if !changed {
            return;
        }
        if self.workspace_view == WorkspaceView::Files
            && !self.update_file_editor_workspace_view(cx)
        {
            self.invalidate_ui_region(cx, UiRegion::WorkspaceBody);
        }
    }

    pub(in crate::app) fn active_file_editor_state(&self) -> Option<gpui::Entity<InputState>> {
        let relative_path = self.active_file_editor_tab.as_deref()?;
        self.file_editor_states
            .get(&self.file_editor_state_key(relative_path))
            .cloned()
    }

    pub(in crate::app) fn ensure_active_file_editor_state(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(relative_path) = self.active_file_editor_tab.clone() else {
            self.file_dirty = false;
            return;
        };
        self.ensure_file_editor_state_for_path(relative_path, window, cx);
    }

    pub(in crate::app) fn ensure_file_editor_state_for_path(
        &mut self,
        relative_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Entity<InputState>> {
        let key = self.file_editor_state_key(&relative_path);
        if let Some(state) = self.file_editor_states.get(&key) {
            return Some(state.clone());
        }
        if file_preview_kind_for_path(&relative_path) == FilePreviewKind::Image {
            return None;
        }
        self.spawn_file_editor_state_load(key, relative_path, cx);
        None
    }
}
