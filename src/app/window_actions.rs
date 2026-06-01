use super::*;

impl CoduxApp {
    pub(super) fn new_settings_window() -> Self {
        let mut state = RuntimeState::load();
        state.settings = settings_with_active_restart_locked_values(&state.settings);
        let runtime = RuntimeInventory::load();
        let runtime_service = RuntimeService::new(state.support_dir.clone());
        state.remote = runtime_service.reload_remote();
        let power_sync_error = runtime_service.start_power_settings_sync().err();
        state.power = runtime_service.power_summary(&state.settings.sleep_mode);
        if let Some(error) = power_sync_error {
            state.power.error = Some(error);
        }
        let selected_ai_provider_id = state
            .settings
            .ai_providers
            .first()
            .map(|provider| provider.id.clone());
        let selected_memory_entry_id = state
            .memory
            .recent_entries
            .first()
            .map(|entry| entry.id.clone());
        let selected_memory_summary_id = state
            .memory_manager
            .summaries
            .first()
            .map(|summary| summary.id.clone());
        let memory_manager_project_id = state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone());
        let selected_notification_channel_id = state
            .notifications
            .channels
            .first()
            .map(|channel| channel.id.clone());
        let selected_runtime_terminal_id = state
            .runtime_events
            .sessions
            .first()
            .map(|session| session.terminal_id.clone());
        let selected_remote_device_id = state
            .remote
            .device_list
            .first()
            .map(|device| device.id.clone());
        let selected_git_branch = state
            .git
            .branches
            .iter()
            .find(|branch| branch.is_current)
            .or_else(|| state.git.branches.first())
            .map(|branch| branch.name.clone());
        let git_review = app_git_review(&state);
        let project_open_applications = runtime_service.project_open_applications();
        let pet_catalog = runtime_service.pet_catalog();
        let pet_snapshot = runtime_service.pet_snapshot().unwrap_or_default();
        let pet_custom_pets = pet_catalog.custom_pets.clone();
        let pet_sprite_paths =
            pet_sprite_path_cache(&runtime.source_root, &state.support_dir, &pet_catalog);
        let project_view_store = initial_project_view_store(&state);
        let worktree_view_store = initial_worktree_view_store(&state);
        let terminal_view_store = initial_terminal_view_store(&state);

        Self {
            window_mode: AppWindowMode::Settings,
            root_focus_handle: None,
            terminals: Vec::new(),
            terminal_manager: Arc::new(TerminalManager::with_ai_runtime_registry(
                runtime_service.ai_runtime_bridge().registry(),
            )),
            terminal_layout_loading: false,
            active_terminal_id: 0,
            next_terminal_index: 1,
            runtime,
            runtime_ingress: RuntimeIngressStatus::default(),
            state,
            runtime_service,
            is_exiting: false,
            status_message: "settings window ready".to_string(),
            desktop_pet_window: None,
            settings_window: None,
            about_window: None,
            memory_manager_window: None,
            pet_claim_window: None,
            pet_custom_install_window: None,
            pet_dex_window: None,
            ssh_profile_editor_window: None,
            project_editor_window: None,
            desktop_pet_line: desktop_pet_fallback_line().to_string(),
            desktop_pet_tone: DesktopPetActivityTone::Normal,
            desktop_pet_active_llm_key: String::new(),
            desktop_pet_requested_llm_key: String::new(),
            desktop_pet_last_llm_requested_at: 0.0,
            pet_sprite_frame: 0,
            pet_sprite_animation_active: false,
            file_preview: "select a file to preview it".to_string(),
            file_editable: false,
            file_dirty: false,
            file_search_open: false,
            file_search_query: String::new(),
            file_search_match_index: 0,
            file_directory: String::new(),
            selected_file_entry: None,
            selected_file_entries: HashSet::new(),
            file_selection_anchor: None,
            file_name_draft_kind: None,
            file_name_draft_target: None,
            file_name_draft_value: String::new(),
            file_name_draft_select_all: false,
            file_tree_expanded_dirs: HashSet::new(),
            file_tree_children: HashMap::new(),
            file_tree_scroll_handle: UniformListScrollHandle::new(),
            file_preview_scroll_handle: UniformListScrollHandle::new(),
            selected_git_file: None,
            selected_git_branch,
            git_review,
            git_expanded_sections: HashSet::from(["changed".to_string(), "untracked".to_string()]),
            git_expanded_dirs: HashSet::new(),
            git_tree_children: HashMap::new(),
            git_files_scroll_handle: VirtualListScrollHandle::new(),
            selected_git_files: HashSet::new(),
            git_diff_preview: "select a changed file to preview its diff".to_string(),
            git_diff_window_path: None,
            git_diff_window_content: String::new(),
            git_diff_window_error: None,
            git_review_content: None,
            git_clone_remote_url: String::new(),
            git_remote_editor_open: false,
            git_remote_name: "origin".to_string(),
            git_remote_url: String::new(),
            git_running_operation: None,
            git_commit_message: String::new(),
            git_commit_message_revision: 0,
            pet_install_url: String::new(),
            pet_install_display_name: String::new(),
            pet_install_preview: None,
            pet_install_error: None,
            pet_install_previewing: false,
            pet_installing: false,
            pet_catalog,
            pet_snapshot,
            pet_custom_pets,
            pet_sprite_paths,
            project_scroll_handle: UniformListScrollHandle::new(),
            task_scroll_handle: UniformListScrollHandle::new(),
            session_scroll_handle: UniformListScrollHandle::new(),
            ssh_scroll_handle: UniformListScrollHandle::new(),
            git_history_scroll_handle: VirtualListScrollHandle::new(),
            pet_dex_scroll_handle: VirtualListScrollHandle::new(),
            pet_custom_install_seen_revision: current_pet_custom_install_event().revision,
            pet_update_seen_revision: current_pet_update_event().revision,
            settings_seen_revision: current_settings_update_event().revision,
            ssh_seen_revision: current_ssh_update_event().revision,
            pet_claim_species: String::new(),
            pet_name_editing: false,
            pet_dex_spotlight: None,
            selected_ai_session_id: None,
            ai_session_delete_confirm_id: None,
            selected_ai_provider_id,
            ai_provider_testing_id: None,
            selected_memory_entry_id,
            selected_memory_summary_id,
            selected_notification_channel_id,
            notification_testing_channel_id: None,
            runtime_refresh_in_flight: false,
            pending_runtime_refresh: None,
            ai_runtime_state_save_tick: 0,
            dismissed_worktree_ai_completion_at: HashMap::new(),
            ai_index_progress_visible_until: 0.0,
            ai_index_progress_generation: 0,
            ai_history_active_index_count: 0,
            ai_history_refresh_project_ids: HashSet::new(),
            project_switch_generation: 0,
            project_task_load_in_flight: HashSet::new(),
            project_task_load_last_started_at: HashMap::new(),
            project_task_load_last_finished_at: HashMap::new(),
            worktree_sidebar_load_in_flight: HashSet::new(),
            worktree_sidebar_load_last_started_at: HashMap::new(),
            worktree_sidebar_load_last_finished_at: HashMap::new(),
            memory_progress_visible_until: 0.0,
            memory_progress_generation: 0,
            performance_refresh_in_flight: false,
            pending_performance_refresh: None,
            today_level_day_start: codux_runtime::ai_history_normalized::local_day_start_seconds(
                app_now_seconds(),
            ),
            active_settings_pane: SettingsPane::General,
            memory_manager_tab: MemoryManagerTab::Summary,
            memory_manager_scope: "project".to_string(),
            memory_manager_project_id,
            memory_processing: false,
            selected_runtime_terminal_id,
            selected_ssh_profile_id: None,
            ssh_draft_open: false,
            ssh_testing: false,
            ssh_draft_id: None,
            ssh_draft_name: String::new(),
            ssh_draft_host: String::new(),
            ssh_draft_port: "22".to_string(),
            ssh_draft_username: String::new(),
            ssh_draft_credential_kind: "none".to_string(),
            ssh_draft_private_key_path: String::new(),
            ssh_draft_password: String::new(),
            ssh_draft_key_passphrase: String::new(),
            selected_remote_device_id,
            remote_reconnecting: false,
            remote_pairing_sheet_open: false,
            remote_pairing_creating: false,
            remote_pairing_poll_generation: 0,
            recording_shortcut_id: None,
            agent_split_enabled: false,
            workspace_view: WorkspaceView::Terminal,
            assistant_panel: None,
            project_column_collapsed: false,
            task_column_collapsed: false,
            project_list_store: None,
            project_column_view: None,
            task_column_view: None,
            workspace_column_view: None,
            workspace_toolbar_view: None,
            workspace_body_view: None,
            workspace_assistant_view: None,
            status_bar_view: None,
            file_sidebar_view: None,
            project_view_store,
            worktree_view_store,
            terminal_view_store,
            project_open_applications,
            project_editor_project_id: None,
            project_editor_name: String::new(),
            project_editor_path: String::new(),
            project_editor_badge_symbol: None,
            project_editor_badge_color_hex: PROJECT_BADGE_COLORS[0].to_string(),
        }
    }

    pub(super) fn new_project_editor_window(project: ProjectInfo) -> Self {
        let mut app = Self::new_settings_window();
        app.window_mode = AppWindowMode::ProjectEditor;
        app.status_message = format!("editing project: {}", project.name);
        app.project_editor_project_id = Some(project.id);
        app.project_editor_name = project.name;
        app.project_editor_path = project.path;
        app.project_editor_badge_symbol = project.badge_symbol;
        app.project_editor_badge_color_hex = project
            .badge_color_hex
            .unwrap_or_else(|| PROJECT_BADGE_COLORS[0].to_string());
        app
    }

    pub(super) fn new_project_creator_window() -> Self {
        let mut app = Self::new_settings_window();
        app.window_mode = AppWindowMode::ProjectEditor;
        app.status_message = "creating project".to_string();
        app.project_editor_project_id = None;
        app.project_editor_name = String::new();
        app.project_editor_path = String::new();
        app.project_editor_badge_symbol = None;
        app.project_editor_badge_color_hex = PROJECT_BADGE_COLORS[0].to_string();
        app
    }

    pub(super) fn new_ssh_profile_editor_window(profile: Option<SSHConnectionProfile>) -> Self {
        let mut app = Self::new_settings_window();
        app.window_mode = AppWindowMode::SshProfileEditor;
        app.ssh_draft_open = true;
        app.ssh_draft_id = None;
        app.ssh_draft_name.clear();
        app.ssh_draft_host.clear();
        app.ssh_draft_port = "22".to_string();
        app.ssh_draft_username.clear();
        app.ssh_draft_credential_kind = "none".to_string();
        app.ssh_draft_private_key_path.clear();
        app.ssh_draft_password.clear();
        app.ssh_draft_key_passphrase.clear();
        if let Some(profile) = profile {
            app.apply_ssh_draft(profile);
            app.status_message = "editing SSH profile".to_string();
        } else {
            app.status_message = "new SSH profile".to_string();
        }
        app
    }

    pub(super) fn activate_child_window(
        handle: &mut Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(window_handle) = *handle else {
            return false;
        };
        if window_handle
            .update(cx, |_view, window, _cx| window.activate_window())
            .is_ok()
        {
            return true;
        }
        *handle = None;
        false
    }

    pub(super) fn on_key_down(
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
                cx.notify();
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
            cx.notify();
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

    pub(super) fn should_close_window_for_keystroke(&self, keystroke: &gpui::Keystroke) -> bool {
        !matches!(
            self.window_mode,
            AppWindowMode::Main | AppWindowMode::DesktopPet
        ) && keystroke.key.eq_ignore_ascii_case("w")
            && keystroke.modifiers.platform
            && !keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.shift
    }

    pub(super) fn handle_configured_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.handle_project_number_shortcut(event, window, cx) {
            return true;
        }

        let actual = shortcut_display_from_keystroke(&event.keystroke);
        let shortcuts = &self.state.settings.shortcuts;

        if shortcut_matches(shortcuts, "view.terminal", &actual) {
            self.workspace_view = WorkspaceView::Terminal;
            cx.notify();
            return true;
        }
        if shortcut_matches(shortcuts, "view.files", &actual) {
            self.workspace_view = WorkspaceView::Files;
            cx.notify();
            return true;
        }
        if shortcut_matches(shortcuts, "view.review", &actual) {
            self.workspace_view = WorkspaceView::Review;
            cx.notify();
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
        if shortcut_matches(shortcuts, "terminal.tab.create", &actual)
            || shortcut_matches(shortcuts, "terminal.tab", &actual)
        {
            self.add_terminal_tab(window, cx);
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
                self.create_worktree(window, cx);
            }
            return true;
        }

        false
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

    pub(super) fn handle_file_clipboard_key(
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
            let paths = clipboard_external_paths(cx);
            return self.paste_clipboard_file_entries(paths, window, cx);
        }
        false
    }

    pub(super) fn open_settings_window(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_window_with_pane(SettingsPane::General, cx);
    }

    pub(super) fn open_settings_window_with_pane(
        &mut self,
        pane: SettingsPane,
        cx: &mut Context<Self>,
    ) {
        let pane_label = pane.label(&self.state.settings.language);
        if Self::activate_child_window(&mut self.settings_window, cx) {
            self.status_message = format!("settings window already opened: {pane_label}");
            cx.notify();
            return;
        }

        let bounds = Bounds::centered(None, size(px(980.0), px(720.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_titlebar("Codux Settings")),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(760.0), px(560.0))),
                ..Default::default()
            },
            |window, cx| {
                let mut app = CoduxApp::new_settings_window();
                app.active_settings_pane = pane;
                theme::apply_component_theme(
                    &app.state.settings.theme,
                    &app.state.settings.theme_color,
                    Some(window),
                    cx,
                );
                let view = cx.new(|_| app);
                view.update(cx, |app, cx| app.start_settings_remote_snapshot_loop(cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        match result {
            Ok(handle) => {
                self.settings_window = Some(handle.into());
                self.status_message = format!("settings window opened: {pane_label}");
            }
            Err(error) => {
                self.status_message = format!("failed to open settings window: {error}");
            }
        }
        cx.notify();
    }

    pub(super) fn open_remote_settings_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_window_with_pane(SettingsPane::Remote, cx);
    }

    pub(super) fn open_ssh_profile_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_ssh_profile_editor(None, cx);
    }

    pub(super) fn open_selected_ssh_profile_editor(
        &mut self,
        profile_id: String,
        cx: &mut Context<Self>,
    ) {
        self.open_ssh_profile_editor(Some(profile_id), cx);
    }

    pub(super) fn open_ssh_profile_editor(
        &mut self,
        profile_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
        self.normalize_selected_ssh_profile();
        if Self::activate_child_window(&mut self.ssh_profile_editor_window, cx) {
            self.status_message = "SSH profile editor already opened".to_string();
            cx.notify();
            return;
        }

        let profile = if let Some(profile_id) = profile_id {
            let snapshot = self.runtime_service.ssh_profiles();
            let Some(profile) = snapshot
                .profiles
                .into_iter()
                .find(|profile| profile.id == profile_id)
            else {
                self.status_message = "SSH profile is no longer available".to_string();
                cx.notify();
                return;
            };
            Some(profile)
        } else {
            None
        };
        let title = if profile.is_some() {
            "Edit SSH Profile"
        } else {
            "Add SSH Profile"
        };
        let bounds = Bounds::centered(None, size(px(520.0), px(430.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_titlebar(title)),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(460.0), px(390.0))),
                ..Default::default()
            },
            move |window, cx| {
                let app = CoduxApp::new_ssh_profile_editor_window(profile);
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
                self.ssh_profile_editor_window = Some(handle.into());
                "SSH profile editor opened".to_string()
            }
            Err(error) => format!("failed to open SSH profile editor: {error}"),
        };
        cx.notify();
    }

    pub(super) fn close_ssh_profile_dialog(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_open = false;
        self.ssh_testing = false;
        cx.notify();
    }

    pub(super) fn toggle_project_column(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.project_column_collapsed = !self.project_column_collapsed;
        cx.notify();
    }

    pub(super) fn toggle_task_column(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.task_column_collapsed = !self.task_column_collapsed;
        cx.notify();
    }

    pub(super) fn close_active_workspace_item(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.workspace_view {
            WorkspaceView::Terminal => {
                let Some(tab_index) = self
                    .terminals
                    .iter()
                    .position(|tab| tab.id == self.active_terminal_id)
                else {
                    self.status_message = "no active terminal".to_string();
                    cx.notify();
                    return;
                };
                let pane_count = self.terminals[tab_index].panes.len();
                if pane_count > 1 {
                    self.close_terminal_pane(pane_count - 1, window, cx);
                } else {
                    self.status_message = "keep at least one terminal split".to_string();
                    cx.notify();
                }
            }
            WorkspaceView::Files => {
                if self.selected_file_entry.take().is_some() {
                    self.file_preview = "select a file to preview it".to_string();
                    self.file_editable = false;
                    self.file_dirty = false;
                    self.status_message = "file preview closed".to_string();
                } else {
                    self.status_message = "no active file preview".to_string();
                }
                cx.notify();
            }
            WorkspaceView::Review => {
                self.status_message = "no active review item to close".to_string();
                cx.notify();
            }
        }
    }

    pub(super) fn set_workspace_view(
        &mut self,
        view: WorkspaceView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_view = view;
        match view {
            WorkspaceView::Files => self.refresh_files_panel_state(),
            WorkspaceView::Review => self.refresh_git_panel_state(),
            WorkspaceView::Terminal => {}
        }
        cx.notify();
    }

    pub(super) fn set_settings_pane(&mut self, pane: SettingsPane, cx: &mut Context<Self>) {
        self.active_settings_pane = pane;
        cx.notify();
    }

    pub(super) fn toggle_assistant_panel(
        &mut self,
        panel: AssistantPanel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.assistant_panel = if self.assistant_panel == Some(panel) {
            None
        } else {
            Some(panel)
        };
        if self.assistant_panel == Some(panel) {
            self.refresh_assistant_panel_state(panel);
        }
        self.notify_workspace_chrome(cx);
        cx.notify();
    }

    pub(super) fn notify_workspace_chrome(&self, cx: &mut Context<Self>) {
        if let Some(view) = &self.workspace_toolbar_view {
            view.update(cx, |_view, cx| cx.notify());
        }
        if let Some(view) = &self.workspace_assistant_view {
            view.update(cx, |_view, cx| cx.notify());
        }
    }

    pub(super) fn refresh_assistant_panel_state(&mut self, panel: AssistantPanel) {
        match panel {
            AssistantPanel::AIStats => {
                self.state.memory = self.runtime_service.reload_memory(
                    self.state
                        .selected_project
                        .as_ref()
                        .map(|project| project.id.as_str()),
                );
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_entry();
                self.normalize_selected_memory_summary();
            }
            AssistantPanel::SSH => {
                self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
                self.normalize_selected_ssh_profile();
            }
            AssistantPanel::FileManager => {
                self.refresh_files_panel_state();
            }
            AssistantPanel::Git => {
                self.refresh_git_panel_state();
            }
        }
    }

    pub(super) fn reload_update(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.update = self
            .runtime_service
            .reload_update(std::env::current_dir().unwrap_or_default());
        self.status_message = if let Some(error) = &self.state.update.error {
            format!("update check failed: {error}")
        } else if let Some(version) = &self.state.update.latest_version {
            format!("update checked: latest {version}")
        } else {
            "update checked: no latest version in manifest".to_string()
        };
        cx.notify();
    }

    pub(super) fn install_update(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.install_update(
            std::env::current_dir().unwrap_or_default(),
            env!("CARGO_PKG_VERSION"),
        ) {
            Ok(result) => {
                self.state.update = self
                    .runtime_service
                    .reload_update(std::env::current_dir().unwrap_or_default());
                self.status_message = result.message;
            }
            Err(error) => self.status_message = format!("failed to install update: {error}"),
        }
        cx.notify();
    }

    pub(super) fn register_native_menu_actions(
        &mut self,
        mut root: gpui::Div,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        macro_rules! register {
            ($action:ty, $handler:expr) => {
                root = root.on_action(cx.listener(|app, _action: &$action, window, cx| {
                    ($handler)(app, window, cx);
                }));
            };
        }

        register!(
            native_menu::ShowAbout,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_about_window(window, cx)
            }
        );
        register!(
            native_menu::OpenSettings,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_settings_window(window, cx)
            }
        );
        register!(
            native_menu::CheckUpdates,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.reload_update(window, cx)
            }
        );
        register!(native_menu::ExportDiagnostics, |app: &mut CoduxApp,
                                                   _window: &mut Window,
                                                   cx: &mut Context<
            CoduxApp,
        >| {
            app.export_diagnostics(cx)
        });
        register!(
            native_menu::OpenRuntimeLog,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_runtime_log(cx)
            }
        );
        register!(
            native_menu::OpenLiveLog,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_live_log(cx)
            }
        );
        register!(
            native_menu::OpenWebsite,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_codux_website(cx)
            }
        );
        register!(
            native_menu::OpenGithub,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_codux_github(cx)
            }
        );
        register!(
            native_menu::HideCodux,
            |_app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| { cx.hide() }
        );
        register!(
            native_menu::HideOthers,
            |_app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                cx.hide_other_apps()
            }
        );
        register!(
            native_menu::ShowAll,
            |_app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                cx.unhide_other_apps()
            }
        );
        register!(
            native_menu::NewProject,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_project_create_window(window, cx)
            }
        );
        register!(native_menu::OpenProjectFolder, |app: &mut CoduxApp,
                                                   window: &mut Window,
                                                   cx: &mut Context<
            CoduxApp,
        >| {
            app.open_project_folder_from_dialog(window, cx)
        });
        register!(native_menu::CloseCurrentProject, |app: &mut CoduxApp,
                                                     window: &mut Window,
                                                     cx: &mut Context<
            CoduxApp,
        >| {
            app.close_selected_project(window, cx)
        });
        register!(native_menu::CloseAllProjects, |app: &mut CoduxApp,
                                                  window: &mut Window,
                                                  cx: &mut Context<
            CoduxApp,
        >| {
            app.close_all_projects(window, cx)
        });
        register!(
            native_menu::CloseActive,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.close_active_workspace_item(window, cx)
            }
        );
        register!(
            native_menu::CloseWindow,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                if app.window_mode == AppWindowMode::Main {
                    app.close_active_workspace_item(window, cx);
                } else {
                    window.remove_window();
                }
            }
        );
        register!(
            native_menu::ViewTerminal,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.set_workspace_view(WorkspaceView::Terminal, window, cx)
            }
        );
        register!(
            native_menu::ViewFiles,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.set_workspace_view(WorkspaceView::Files, window, cx)
            }
        );
        register!(
            native_menu::ViewReview,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.set_workspace_view(WorkspaceView::Review, window, cx)
            }
        );
        register!(
            native_menu::ToggleProjects,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_project_column(window, cx)
            }
        );
        register!(
            native_menu::ToggleTasks,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_task_column(window, cx)
            }
        );
        register!(
            native_menu::OpenGitPanel,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_assistant_panel(AssistantPanel::Git, window, cx)
            }
        );
        register!(
            native_menu::OpenFilesPanel,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_assistant_panel(AssistantPanel::FileManager, window, cx)
            }
        );
        register!(
            native_menu::OpenAiPanel,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_assistant_panel(AssistantPanel::AIStats, window, cx)
            }
        );
        register!(
            native_menu::OpenSshPanel,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_assistant_panel(AssistantPanel::SSH, window, cx)
            }
        );
        register!(
            native_menu::CreateSplit,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.split_terminal(window, cx)
            }
        );
        register!(
            native_menu::CreateTab,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.add_terminal_tab(window, cx)
            }
        );
        register!(
            native_menu::CreateTask,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.create_worktree(window, cx)
            }
        );
        register!(
            native_menu::EditorSave,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.save_selected_file_preview(window, cx)
            }
        );
        register!(
            native_menu::EditorSearch,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_file_search(cx)
            }
        );
        register!(native_menu::MinimizeWindow, |_app: &mut CoduxApp,
                                                window: &mut Window,
                                                _cx: &mut Context<
            CoduxApp,
        >| {
            window.minimize_window()
        });
        register!(
            native_menu::ZoomWindow,
            |_app: &mut CoduxApp, window: &mut Window, _cx: &mut Context<CoduxApp>| {
                window.zoom_window()
            }
        );
        register!(native_menu::ToggleFullscreen, |_app: &mut CoduxApp,
                                                  window: &mut Window,
                                                  _cx: &mut Context<
            CoduxApp,
        >| {
            window.toggle_fullscreen()
        });
        root
    }

    pub(super) fn register_child_window_actions(
        &mut self,
        root: gpui::Div,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let focus_handle = self.root_focus_handle(cx);
        self.register_native_menu_actions(root.track_focus(&focus_handle), cx)
    }

    pub(super) fn register_close_window_action(
        &mut self,
        root: gpui::Div,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let focus_handle = self.root_focus_handle(cx);
        root.track_focus(&focus_handle).on_action(cx.listener(
            |app, _action: &native_menu::CloseWindow, window, cx| {
                if app.window_mode == AppWindowMode::Main {
                    app.close_active_workspace_item(window, cx);
                } else {
                    window.remove_window();
                }
            },
        ))
    }

    pub(super) fn root_focus_handle(&mut self, cx: &mut Context<Self>) -> FocusHandle {
        if let Some(handle) = &self.root_focus_handle {
            return handle.clone();
        }
        let handle = cx.focus_handle();
        self.root_focus_handle = Some(handle.clone());
        handle
    }

    pub(super) fn focus_root_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.window_mode == AppWindowMode::Main {
            return;
        }

        let focus_handle = self.root_focus_handle(cx);
        if !focus_handle.contains_focused(window, cx) {
            focus_handle.focus(window, cx);
        }
    }
}
