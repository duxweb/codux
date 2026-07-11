use super::*;

impl CoduxApp {
    pub(in crate::app) fn open_file_entry(
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

    pub(in crate::app) fn select_file_entry(
        &mut self,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        self.set_single_file_selection(relative_path.clone());
        self.status_message = format!("selected file item: {relative_path}");
        self.invalidate_file_panel(cx);
    }

    pub(in crate::app) fn toggle_file_entry_selection(
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

    pub(in crate::app) fn select_file_entry_from_click(
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

    pub(in crate::app) fn select_file_entry_range(
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

    pub(in crate::app) fn visible_file_tree_paths(&self) -> Vec<String> {
        fn push_visible(
            rows: &mut Vec<String>,
            files: &[FileEntry],
            children: &HashMap<String, Vec<FileEntry>>,
            expanded: &HashSet<String>,
        ) {
            for file in files {
                rows.push(file.relative_path.clone());
                if expanded.contains(&file.relative_path)
                    && let Some(child_files) = children.get(&file.relative_path)
                {
                    push_visible(rows, child_files, children, expanded);
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

    pub(in crate::app) fn handle_file_sidebar_key_action(
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

    pub(in crate::app) fn move_file_sidebar_selection(
        &mut self,
        delta: isize,
        cx: &mut Context<Self>,
    ) {
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
    pub(in crate::app) fn normalize_selected_file_entry(&mut self) {
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

    pub(in crate::app) fn selected_file_is_text_file(&self) -> bool {
        let Some(entry_path) = self.selected_file_entry.as_deref() else {
            return false;
        };
        self.file_tree_entry(entry_path)
            .map(|entry| matches!(entry.kind, FileKind::File))
            .unwrap_or(false)
    }

    pub(in crate::app) fn file_search_match_lines(&self) -> Vec<usize> {
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

    pub(in crate::app) fn normalize_file_search_index(&mut self) {
        let count = self.file_search_match_lines().len();
        if count == 0 {
            self.file_search_match_index = 0;
        } else if self.file_search_match_index >= count {
            self.file_search_match_index = count - 1;
        }
    }

    pub(in crate::app) fn open_file_search(&mut self, cx: &mut Context<Self>) {
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

    pub(in crate::app) fn handle_file_editor_key(
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
}
