use super::*;

impl CoduxApp {
    pub(in crate::app) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;

        if self.should_close_window_for_keystroke(keystroke) {
            window.remove_window();
            cx.stop_propagation();
            return;
        }

        if let Some(shortcut_id) = self.recording_shortcut_id.clone() {
            if keystroke.key.eq_ignore_ascii_case("escape") {
                self.recording_shortcut_id = None;
                self.status_message = "shortcut recording cancelled".to_string();
                cx.stop_propagation();
                self.invalidate_ui_region(cx, UiRegion::Root);
                return;
            }
            if matches!(
                keystroke.key.as_str(),
                "shift" | "control" | "alt" | "meta" | "cmd" | "command"
            ) {
                cx.stop_propagation();
                return;
            }
            let value = shortcut_display_from_keystroke(keystroke);
            match self.runtime_service.set_shortcut(&shortcut_id, &value) {
                Ok(settings) => {
                    self.apply_settings_summary(settings);
                    self.recording_shortcut_id = None;
                    self.status_message = format!("shortcut saved: {value}");
                }
                Err(error) => {
                    self.recording_shortcut_id = None;
                    self.status_message = format!("failed to save shortcut: {error}");
                }
            }
            cx.stop_propagation();
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }

        if self.handle_terminal_close_shortcut(event, window, cx) {
            cx.stop_propagation();
            return;
        }

        if self.handle_file_picker_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }

        if self.handle_focused_terminal_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }

        if self.handle_configured_shortcut(event, window, cx) {
            cx.stop_propagation();
            return;
        }

        if self.handle_file_name_draft_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }

        if self.handle_file_clipboard_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }

        if keystroke.modifiers.control && (keystroke.key == "+" || keystroke.key == "=") {
            let Some(view) = self.active_terminal_view() else {
                return;
            };
            view.update(cx, |terminal, cx| {
                let mut config = terminal.config().clone();
                config.font_size += px(1.0);
                terminal.update_config(config, cx);
            });
            cx.stop_propagation();
            return;
        }

        if keystroke.modifiers.control && keystroke.key == "-" {
            let Some(view) = self.active_terminal_view() else {
                return;
            };
            view.update(cx, |terminal, cx| {
                let mut config = terminal.config().clone();
                if config.font_size > px(7.0) {
                    config.font_size -= px(1.0);
                    terminal.update_config(config, cx);
                }
            });
            cx.stop_propagation();
            return;
        }

        if self.handle_file_editor_key(event, cx) {
            cx.stop_propagation();
        }
    }

    fn handle_focused_terminal_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.window_mode != AppWindowMode::Main {
            return false;
        }
        let Some(view) = self.focused_or_active_terminal_view(window, cx) else {
            return false;
        };
        view.update(cx, |terminal, cx| {
            terminal.handle_app_terminal_keystroke(&event.keystroke, window, cx)
        })
    }

    pub(in crate::app) fn focused_or_active_terminal_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Entity<TerminalView>> {
        self.focused_terminal_view(window, cx).or_else(|| {
            (self.workspace_view == WorkspaceView::Terminal
                && !self.file_sidebar_contains_focused(window, cx)
                && !self.terminal_search_contains_focused(window, cx)
                && !self.chat_contains_focused(window, cx))
            .then(|| self.active_terminal_view())
            .flatten()
        })
    }

    // Chat panes live inside the Terminal view mode; while one owns focus the
    // active-terminal fallback must not double-deliver keystrokes.
    fn chat_contains_focused(&self, window: &Window, cx: &mut Context<Self>) -> bool {
        self.chat_views
            .values()
            .any(|view| view.read(cx).contains_focused(window, cx))
    }

    pub(super) fn terminal_search_contains_focused(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.terminal_pane_registry
            .values()
            .any(|pane| pane.view.read(cx).search_contains_focused(window, cx))
    }

    fn file_sidebar_contains_focused(&self, window: &Window, cx: &mut Context<Self>) -> bool {
        self.file_sidebar_view
            .as_ref()
            .map(|view| view.read(cx).focus_handle())
            .is_some_and(|focus_handle| focus_handle.contains_focused(window, cx))
    }

    pub(in crate::app) fn should_close_window_for_keystroke(
        &self,
        keystroke: &gpui::Keystroke,
    ) -> bool {
        !matches!(
            self.window_mode,
            AppWindowMode::Main | AppWindowMode::DesktopPet
        ) && keystroke.key.eq_ignore_ascii_case("w")
            && keystroke.modifiers.platform
            && !keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.shift
    }

    pub(in crate::app) fn handle_configured_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.handle_project_number_shortcut(event, window, cx) {
            return true;
        }

        // Debug: ⌃⌥L replays the pet level-up celebration for visual tuning.
        if event.keystroke.modifiers.control
            && event.keystroke.modifiers.alt
            && event.keystroke.key.eq_ignore_ascii_case("l")
        {
            self.preview_pet_level_up(cx);
            return true;
        }

        let actual = shortcut_display_from_keystroke(&event.keystroke);
        let shortcuts = &self.state.settings.shortcuts;

        if shortcut_matches(shortcuts, "view.terminal", &actual) {
            self.set_workspace_view(WorkspaceView::Terminal, window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "view.files", &actual) {
            self.set_workspace_view(WorkspaceView::Files, window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "view.review", &actual) {
            self.set_workspace_view(WorkspaceView::Review, window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "settings.open", &actual) {
            self.open_settings_window(window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "project.open_folder", &actual) {
            self.open_project_folder_from_dialog(window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "sidebar.projects.toggle", &actual) {
            self.toggle_project_column(window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "sidebar.tasks.toggle", &actual) {
            self.toggle_task_column(window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "assistant.git.open", &actual)
            || shortcut_matches(shortcuts, "panel.git", &actual)
        {
            self.toggle_assistant_panel(AssistantPanel::Git, window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "assistant.files.open", &actual) {
            self.toggle_assistant_panel(AssistantPanel::FileManager, window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "assistant.ai.open", &actual)
            || shortcut_matches(shortcuts, "panel.ai", &actual)
        {
            self.toggle_assistant_panel(AssistantPanel::AIStats, window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "assistant.server.open", &actual) {
            self.toggle_assistant_panel(AssistantPanel::ServerInfo, window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "assistant.ssh.open", &actual) {
            self.toggle_assistant_panel(AssistantPanel::SSH, window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "terminal.split.create", &actual)
            || shortcut_matches(shortcuts, "terminal.split", &actual)
        {
            self.split_terminal(window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "editor.save", &actual) {
            self.save_selected_file_preview(window, cx);
            return true;
        }
        if shortcut_matches(shortcuts, "editor.search", &actual) {
            self.open_file_search(cx);
            return true;
        }
        if shortcut_matches(shortcuts, "close.active", &actual) {
            self.close_active_workspace_item(window, cx);
            return true;
        }

        let project_create = shortcut_matches(shortcuts, "project.create", &actual);
        let task_create = shortcut_matches(shortcuts, "task.create", &actual);
        if project_create || task_create {
            if self.task_column_collapsed || !task_create {
                self.open_project_create_window(window, cx);
            } else {
                self.open_worktree_creator_window(window, cx);
            }
            return true;
        }

        false
    }

    fn handle_terminal_close_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.window_mode != AppWindowMode::Main || self.workspace_view != WorkspaceView::Terminal
        {
            return false;
        }
        let actual = shortcut_display_from_keystroke(&event.keystroke);
        if !shortcut_matches(&self.state.settings.shortcuts, "close.active", &actual) {
            return false;
        }
        self.close_active_workspace_item(window, cx);
        true
    }

    fn handle_project_number_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let keystroke = &event.keystroke;
        if !keystroke.modifiers.platform
            || keystroke.modifiers.control
            || keystroke.modifiers.alt
            || keystroke.modifiers.shift
        {
            return false;
        }

        let Ok(index) = keystroke.key.parse::<usize>() else {
            return false;
        };
        if !(1..=9).contains(&index) {
            return false;
        }

        let Some(project) = self.state.projects.get(index - 1) else {
            return true;
        };
        self.select_project(project.id.clone(), window, cx);
        true
    }

    pub(in crate::app) fn handle_file_clipboard_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.workspace_view != WorkspaceView::Files
            && self.assistant_panel != Some(AssistantPanel::FileManager)
        {
            return false;
        }
        let keystroke = &event.keystroke;
        if !keystroke.modifiers.platform
            || keystroke.modifiers.control
            || keystroke.modifiers.alt
            || keystroke.modifiers.shift
        {
            return false;
        }
        if keystroke.key.eq_ignore_ascii_case("c") {
            return self.copy_selected_file_paths_to_clipboard(cx);
        }
        if keystroke.key.eq_ignore_ascii_case("v") {
            let app_entity = cx.entity();
            window.defer(cx, move |window, cx| {
                let payload = clipboard_file_payload(cx);
                cx.update_entity(&app_entity, |app, cx| {
                    app.paste_clipboard_file_entries(payload, window, cx);
                });
            });
            return true;
        }
        false
    }
}
