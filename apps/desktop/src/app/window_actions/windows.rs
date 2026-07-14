use super::*;
use crate::app::app_events::{current_child_window_update_event, current_memory_update_event};
use crate::app::app_state::CoduxTooltipState;

impl CoduxApp {
    pub(in crate::app) fn new_settings_window_from_state(
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut state = state;
        state.settings = settings_with_active_restart_locked_values(&state.settings);
        Self::new_auxiliary_window_from_state(state, runtime, runtime_service)
    }

    pub(in crate::app) fn new_memory_manager_window(
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut app = Self::new_auxiliary_window_from_state(state, runtime, runtime_service);
        app.window_mode = AppWindowMode::MemoryManager;
        app.memory_manager_tab = MemoryManagerTab::Summary;
        app.memory_manager_scope = "project".to_string();
        app.memory_manager_project_id = app
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone());
        app.memory_manager_refreshing = true;
        app
    }

    fn new_auxiliary_window_from_state(
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
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
        let project_open_applications = Vec::new();
        let pet_catalog = runtime_service.pet_catalog();
        let pet_snapshot = runtime_service.pet_snapshot().unwrap_or_default();
        let pet_custom_pets = pet_catalog.custom_pets.clone();
        let pet_sprite_paths =
            pet_sprite_path_cache(&runtime.source_root, &state.support_dir, &pet_catalog);
        Self {
            window_mode: AppWindowMode::Settings,
            root_focus_handle: None,
            terminals: Vec::new(),
            terminal_pane_registry: HashMap::new(),
            terminal_osc_titles: HashMap::new(),
            terminal_search_open: std::collections::HashSet::new(),
            terminal_attach_in_flight: std::collections::HashSet::new(),
            hosted_terminal_rebind_in_flight: HashSet::new(),
            terminal_manager: Arc::new(TerminalManager::with_ai_runtime(
                runtime_service.ai_runtime_bridge(),
            )),
            boot_pending_terminals: Vec::new(),
            terminal_layout_loading: false,
            active_terminal_id: 0,
            active_terminal_runtime_ids: HashMap::new(),
            terminal_layout_cache: HashMap::new(),
            file_panel_cache: HashMap::new(),
            next_terminal_index: 1,
            runtime,
            state,
            runtime_service,
            window_appearance: WindowAppearance::Dark,
            main_window_fullscreen: false,
            main_window_lost_to_external_app: false,
            _observe_window_appearance: None,
            _observe_window_activation: None,
            is_exiting: false,
            main_window_close_handler_registered: false,
            last_quit_request_at: None,
            pending_terminal_close: None,
            status_message: "settings window ready".to_string(),
            toast_message: None,
            toast_revision: 0,
            pending_restart_language: None,
            desktop_pet_window: None,
            settings_window: None,
            about_window: None,
            update_dialog_window: None,
            git_clone_window: None,
            git_credentials_window: None,
            memory_manager_window: None,
            pet_claim_window: None,
            pet_custom_install_window: None,
            pet_dex_window: None,
            ssh_profile_editor_window: None,
            db_profile_editor_window: None,
            file_picker_window: None,
            file_picker_mode: FilePickerMode::OpenFolder,
            file_picker_target: FilePickerTarget::ProjectEditorPath,
            file_picker_filename: String::new(),
            file_picker_selected: None,
            file_picker_active_path: None,
            project_editor_window: None,
            worktree_creator_window: None,
            child_windows: Vec::new(),
            parent_main_window: None,
            parent_main_window_handle: None,
            desktop_pet_line: desktop_pet_fallback_line().to_string(),
            desktop_pet_tone: DesktopPetActivityTone::Normal,
            desktop_pet_plan_items: Vec::new(),
            desktop_pet_terminal_statuses: Vec::new(),
            desktop_pet_runtime_activity_active: false,
            desktop_pet_main_window_fullscreen: false,
            desktop_pet_active_llm_key: String::new(),
            desktop_pet_requested_llm_key: String::new(),
            desktop_pet_last_llm_requested_at: 0.0,
            desktop_pet_llm_generation: 0,
            desktop_pet_llm_in_flight: false,
            desktop_pet_next_hydration_reminder_at: 0.0,
            desktop_pet_next_sedentary_reminder_at: 0.0,
            desktop_pet_next_late_night_reminder_at: 0.0,
            desktop_pet_next_idle_llm_at: 0.0,
            desktop_pet_line_visible_until: 0.0,
            desktop_pet_line_hold_until: 0.0,
            pet_sprite_frame: 0,
            pet_sprite_animation_active: false,
            pet_level_up: None,
            pet_level_up_ticking: false,
            file_preview: "select a file to preview it".to_string(),
            file_preview_window_path: None,
            file_preview_window_content: String::new(),
            file_preview_window_error: None,
            file_preview_window_view: None,
            file_editable: false,
            file_dirty: false,
            file_editor_tabs: Vec::new(),
            active_file_editor_tab: None,
            file_editor_states: HashMap::new(),
            file_editor_state_lru: Vec::new(),
            file_editor_loading_states: HashSet::new(),
            file_search_open: false,
            file_search_query: String::new(),
            file_search_match_index: 0,
            file_directory: String::new(),
            selected_file_entry: None,
            selected_file_entries: HashSet::new(),
            file_selection_anchor: None,
            file_name_draft_kind: None,
            file_name_draft_target: None,
            file_name_draft_parent: None,
            file_name_draft_value: String::new(),
            file_name_draft_select_all: false,
            file_tree_expanded_dirs: HashSet::new(),
            file_tree_children: HashMap::new(),
            file_tree_scroll_handle: UniformListScrollHandle::new(),
            file_panel_refreshing: false,
            file_mutation_generation: 0,
            selected_project_path_available: true,
            selected_git_file: None,
            selected_git_branch,
            git_review,
            git_expanded_sections: HashSet::from(["changed".to_string(), "untracked".to_string()]),
            git_expanded_dirs: HashSet::new(),
            git_tree_children: HashMap::new(),
            git_files_scroll_handle: VirtualListScrollHandle::new(),
            git_review_code_scroll_handle: ScrollHandle::new(),
            selected_git_files: HashSet::new(),
            git_diff_preview: "select a changed file to preview its diff".to_string(),
            git_diff_window_path: None,
            git_diff_window_content: String::new(),
            git_diff_window_error: None,
            git_review_content: None,
            git_review_derived_rows: None,
            git_review_refreshing: false,
            git_clone_remote_url: String::new(),
            git_running_operation: None,
            git_credential_project_id: None,
            git_credential_project_name: String::new(),
            git_credential_project_path: String::new(),
            git_credential_remote_url: String::new(),
            git_credential_username: String::new(),
            git_credential_password_or_token: String::new(),
            git_credential_error: None,
            git_credential_retrying: false,
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
            memory_seen_revision: current_memory_update_event().revision,
            child_window_update_seen_revision: current_child_window_update_event().revision,
            child_window_settings_seen_revision: current_child_window_update_event()
                .settings_revision,
            child_window_ssh_seen_revision: current_child_window_update_event().ssh_revision,
            child_window_memory_seen_revision: current_child_window_update_event().memory_revision,
            child_window_project_seen_revision: current_child_window_update_event()
                .project_revision,
            child_window_worktree_seen_revision: current_child_window_update_event()
                .worktree_revision,
            child_window_git_seen_revision: current_child_window_update_event().git_revision,
            pet_claim_species: String::new(),
            pet_name_editing: false,
            pet_dex_spotlight: None,
            selected_ai_session_id: None,
            ai_session_delete_confirm_id: None,
            selected_ai_provider_id,
            ai_provider_testing_id: None,
            ai_provider_test_result: None,
            selected_memory_entry_id,
            selected_memory_summary_id,
            selected_notification_channel_id,
            notification_testing_channel_id: None,
            runtime_refresh_in_flight: false,
            runtime_ready: true,
            runtime_queue_busy: false,
            pending_runtime_refresh: None,
            ai_runtime_state_save_tick: 0,
            pane_agent_lifecycle: HashMap::new(),
            agent_pulse_active: false,
            agent_git_refresh_after: None,
            ai_index_progress_visible_until: 0.0,
            ai_index_progress_generation: 0,
            ai_history_active_index_count: 0,
            ai_history_refreshing: false,
            ai_global_history_refreshing: false,
            ai_global_history_refresh_pending: false,
            project_switch_generation: 0,
            terminal_restore_epoch: 0,
            terminal_restored_generation: None,
            scheduled_work_in_flight: HashSet::new(),
            scheduled_work_last_started_at: HashMap::new(),
            scheduled_work_last_finished_at: HashMap::new(),
            task_column_refreshing: false,
            terminal_font_families: Vec::new(),
            terminal_font_families_loaded: false,
            terminal_font_families_loading: false,
            memory_progress_visible_until: 0.0,
            memory_progress_generation: 0,
            memory_manager_refreshing: false,
            memory_manager_refresh_generation: 0,
            memory_project_profile_refreshing: false,
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
            memory_extraction_status_refreshing: false,
            memory_status_seen_failed_count: 0,
            selected_runtime_terminal_id,
            selected_ssh_profile_id: None,
            selected_db_profile_id: None,
            db_testing: false,
            db_test_result: None,
            db_draft_id: None,
            db_draft_project_id: String::new(),
            db_draft_name: String::new(),
            db_draft_engine: "postgres".to_string(),
            db_draft_host: "localhost".to_string(),
            db_draft_port: "5432".to_string(),
            db_draft_database: String::new(),
            db_draft_username: String::new(),
            db_draft_password: String::new(),
            db_draft_ssl_mode: "prefer".to_string(),
            db_draft_read_only: true,
            ssh_draft_open: false,
            ssh_testing: false,
            ssh_test_result: None,
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
            remote_pairing_error: None,
            remote_pairing_poll_generation: 0,
            remote_connect_open: false,
            remote_connect_ticket: String::new(),
            remote_connect_name: String::new(),
            remote_connect_error: None,
            remote_connect_busy: false,
            recording_shortcut_id: None,
            workspace_view: WorkspaceView::Terminal,
            stats_time_range: StatsTimeRange::ThirtyDays,
            workspace_split: None,
            assistant_panel: None,
            project_column_collapsed: true,
            task_column_collapsed: false,
            task_section_terminals_collapsed: false,
            task_section_sessions_collapsed: false,
            project_list_state: None,
            remote_link_states: std::collections::HashMap::new(),
            remote_saved_host_ids: Vec::new(),
            project_column_view: None,
            task_column_view: None,
            task_column_header_view: None,
            task_worktree_list_view: None,
            task_session_list_view: None,
            task_terminal_list_view: None,
            collapsed_terminal_panes: Vec::new(),
            workspace_column_view: None,
            workspace_toolbar_view: None,
            workspace_body_view: None,
            workspace_assistant_view: None,
            ai_stats_sidebar_view: None,
            server_info_sidebar_view: None,
            ssh_sidebar_view: None,
            db_sidebar_view: None,
            git_sidebar_view: None,
            git_files_panel_view: None,
            git_history_panel_view: None,
            status_bar_view: None,
            appearance_vibrancy_slider: None,
            _appearance_slider_subscriptions: Vec::new(),
            file_sidebar_view: None,
            project_open_applications,
            project_editor_project_id: None,
            project_editor_name: String::new(),
            project_editor_path: String::new(),
            project_editor_badge_symbol: None,
            project_editor_badge_color_hex: PROJECT_BADGE_COLORS[0].to_string(),
            project_editor_saving: false,
            project_editor_runtime_target: ProjectRuntimeTarget::Local,
            wsl_distribution_catalog: None,
            wsl_distribution_catalog_loading: false,
            wsl_selected_distribution: String::new(),
            wsl_install_progress: None,
            wsl_runtime_error: None,
            project_editor_browse_busy: false,
            project_editor_browse_path: String::new(),
            project_editor_browse_parent: None,
            project_editor_browse_entries: Vec::new(),
            project_editor_browse_error: None,
            project_editor_browse_new_folder: String::new(),
            project_editor_browse_generation: 0,
            file_picker_rename_draft: None,
            file_picker_new_folder_active: false,
            worktree_creator_project_id: None,
            worktree_creator_project_name: String::new(),
            worktree_creator_project_path: String::new(),
            worktree_creator_base_branch: String::new(),
            worktree_creator_name: String::new(),
            worktree_creator_error: None,
            worktree_creator_loading: false,
            worktree_creator_submitting: false,
            update_dialog_phase: UpdateDialogPhase::Checking,
            update_dialog_status: None,
            update_dialog_progress: None,
            update_dialog_result: None,
            update_dialog_error: None,
            tooltip_state: CoduxTooltipState::default(),
            ui_performance_counts: HashMap::new(),
            ui_performance_last_report_at: 0.0,
        }
    }

    pub(in crate::app) fn open_auxiliary_window(
        &mut self,
        spec: AuxiliaryWindowSpec,
        cx: &mut Context<Self>,
        build: impl FnOnce(
            RuntimeState,
            RuntimeInventory,
            RuntimeService,
            &mut Window,
            &mut App,
        ) -> CoduxApp
        + 'static,
        after_view: impl FnOnce(gpui::Entity<CoduxApp>, &mut Window, &mut App) + 'static,
    ) -> bool {
        if self.activate_auxiliary_window_slot(spec.slot, cx) {
            self.status_message = spec.already_open_message.to_string();
            self.invalidate_status_bar(cx);
            return true;
        }

        let state = self.state.clone();
        let runtime = self.runtime.clone();
        let runtime_service = self.runtime_service.clone();
        let window_appearance = self.window_appearance;
        let bounds = Bounds::centered(None, spec.size, cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_child_titlebar(spec.title)),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(spec.min_size),
                is_minimizable: false,
                is_resizable: false,
                ..Default::default()
            },
            move |window, cx| {
                macos_window::configure_child_window_controls(window);
                let mut app = build(state, runtime, runtime_service, window, cx);
                app.window_appearance = window_appearance;
                theme::apply_component_theme_for_appearance(
                    &app.state.settings.theme,
                    &app.state.settings.theme_color,
                    window_appearance,
                    Some(window),
                    cx,
                );
                let view = cx.new(|_| app);
                after_view(view.clone(), window, cx);
                view.update(cx, |app, cx| {
                    app.refresh_window_runtime_data(cx);
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        match result {
            Ok(handle) => {
                let handle: AnyWindowHandle = handle.into();
                *self.auxiliary_window_slot_mut(spec.slot) = Some(handle);
                self.register_child_window_handle(handle);
                self.status_message = spec.opened_message.to_string();
                true
            }
            Err(error) => {
                self.status_message = format!("{}: {error}", spec.failed_prefix);
                false
            }
        }
    }

    fn activate_auxiliary_window_slot(
        &mut self,
        slot: AuxiliaryWindowSlot,
        cx: &mut Context<Self>,
    ) -> bool {
        let activated = {
            let handle_slot = self.auxiliary_window_slot_mut(slot);
            Self::activate_child_window(handle_slot, cx)
        };
        if activated {
            let handle = *self.auxiliary_window_slot_mut(slot);
            if let Some(handle) = handle {
                self.register_child_window_handle(handle);
            }
        }
        activated
    }

    fn auxiliary_window_slot_mut(
        &mut self,
        slot: AuxiliaryWindowSlot,
    ) -> &mut Option<AnyWindowHandle> {
        match slot {
            AuxiliaryWindowSlot::Settings => &mut self.settings_window,
            AuxiliaryWindowSlot::About => &mut self.about_window,
            AuxiliaryWindowSlot::UpdateDialog => &mut self.update_dialog_window,
            AuxiliaryWindowSlot::GitClone => &mut self.git_clone_window,
            AuxiliaryWindowSlot::GitCredentials => &mut self.git_credentials_window,
            AuxiliaryWindowSlot::MemoryManager => &mut self.memory_manager_window,
            AuxiliaryWindowSlot::ProjectEditor => &mut self.project_editor_window,
            AuxiliaryWindowSlot::WorktreeCreator => &mut self.worktree_creator_window,
            AuxiliaryWindowSlot::SshProfileEditor => &mut self.ssh_profile_editor_window,
            AuxiliaryWindowSlot::DbProfileEditor => &mut self.db_profile_editor_window,
            AuxiliaryWindowSlot::FilePicker => &mut self.file_picker_window,
        }
    }

    pub(in crate::app) fn new_project_editor_window_from_state(
        project: ProjectInfo,
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut app = Self::new_settings_window_from_state(state, runtime, runtime_service);
        let runtime_target = project.runtime_target.clone();
        app.window_mode = AppWindowMode::ProjectEditor;
        app.status_message = format!("editing project: {}", project.name);
        app.project_editor_project_id = Some(project.id);
        app.project_editor_name = project.name;
        app.project_editor_path = project.path;
        app.project_editor_badge_symbol = project.badge_symbol;
        app.project_editor_badge_color_hex = project
            .badge_color_hex
            .unwrap_or_else(|| PROJECT_BADGE_COLORS[0].to_string());
        app.project_editor_runtime_target = runtime_target;
        app
    }

    pub(in crate::app) fn new_project_creator_window_from_state(
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut app = Self::new_settings_window_from_state(state, runtime, runtime_service);
        app.window_mode = AppWindowMode::ProjectEditor;
        app.status_message = "creating project".to_string();
        app.project_editor_project_id = None;
        app.project_editor_name = String::new();
        app.project_editor_path = String::new();
        app.project_editor_badge_symbol = None;
        app.project_editor_badge_color_hex = PROJECT_BADGE_COLORS[0].to_string();
        app
    }

    pub(in crate::app) fn new_ssh_profile_editor_window_from_state(
        profile: Option<SSHConnectionProfile>,
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut app = Self::new_settings_window_from_state(state, runtime, runtime_service);
        app.window_mode = AppWindowMode::SshProfileEditor;
        app.ssh_draft_open = true;
        app.ssh_test_result = None;
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

    pub(in crate::app) fn new_db_profile_editor_window_from_state(
        profile: Option<DBConnectionProfile>,
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut app = Self::new_settings_window_from_state(state, runtime, runtime_service);
        app.window_mode = AppWindowMode::DbProfileEditor;
        let locale = locale_from_language_setting(&app.state.settings.language);
        if let Some(profile) = profile {
            app.apply_db_draft(profile);
            app.status_message = translate(
                &locale,
                "db.profile.editing_status",
                "editing database profile",
            );
        } else {
            app.reset_db_draft_for_selected_project();
            app.status_message =
                translate(&locale, "db.profile.new_status", "new database profile");
        }
        app
    }

    pub(in crate::app) fn new_file_editor_window_from_state(
        relative_path: String,
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut app = Self::new_settings_window_from_state(state, runtime, runtime_service);
        app.window_mode = AppWindowMode::FileEditor;
        app.status_message = format!("file editor opened: {relative_path}");
        app.file_editor_tabs.clear();
        app.file_editor_states.clear();
        app.file_editor_state_lru.clear();
        app.file_editor_loading_states.clear();
        app.active_file_editor_tab = None;
        app.add_file_editor_window_tab(relative_path);
        app
    }

    pub(in crate::app) fn new_file_preview_window_from_state(
        relative_path: String,
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut app = Self::new_settings_window_from_state(state, runtime, runtime_service);
        app.window_mode = AppWindowMode::FilePreview;
        app.status_message = format!("file preview opened: {relative_path}");
        app.file_preview_window_path = Some(relative_path);
        app.file_preview_window_content.clear();
        app.file_preview_window_error = None;
        app
    }

    pub(in crate::app) fn open_file_editor_window(
        &mut self,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to open file editor".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let title = format!(
            "{} - {}",
            file_editor::file_editor_window_title(&relative_path),
            project.name
        );
        let state = self.state.clone();
        let runtime = self.runtime.clone();
        let runtime_service = self.runtime_service.clone();
        let window_appearance = self.window_appearance;
        let bounds = Bounds::centered(None, size(px(900.0), px(640.0)), cx);
        let opened_path = relative_path.clone();
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_child_titlebar(SharedString::from(title))),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(520.0), px(360.0))),
                ..Default::default()
            },
            move |window, cx| {
                macos_window::configure_document_child_window_controls(window);
                let mut app = CoduxApp::new_file_editor_window_from_state(
                    relative_path,
                    state,
                    runtime,
                    runtime_service,
                );
                app.window_appearance = window_appearance;
                theme::apply_component_theme_for_appearance(
                    &app.state.settings.theme,
                    &app.state.settings.theme_color,
                    window_appearance,
                    Some(window),
                    cx,
                );
                let view = cx.new(|_| app);
                view.update(cx, |app, cx| {
                    app.ensure_active_file_editor_state(window, cx);
                    app.refresh_window_runtime_data(cx);
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        );
        self.status_message = match result {
            Ok(handle) => {
                self.register_child_window_handle(handle.into());
                format!("file editor window opened: {opened_path}")
            }
            Err(error) => format!("failed to open file editor window: {error}"),
        };
        self.invalidate_file_panel(cx);
    }

    pub(in crate::app) fn open_file_preview_window(
        &mut self,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to open file preview".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let preview_kind = file_editor::file_preview_kind_for_path(&relative_path);
        if preview_kind == file_editor::FilePreviewKind::External {
            self.open_file_entry_external(relative_path, cx);
            return;
        }
        let title = format!(
            "{} - {}",
            file_editor::file_editor_window_title(&relative_path),
            project.name
        );
        let state = self.state.clone();
        let runtime = self.runtime.clone();
        let runtime_service = self.runtime_service.clone();
        let window_appearance = self.window_appearance;
        let bounds = Bounds::centered(None, size(px(880.0), px(640.0)), cx);
        let opened_path = relative_path.clone();
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_child_titlebar(SharedString::from(title))),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(520.0), px(360.0))),
                ..Default::default()
            },
            move |window, cx| {
                macos_window::configure_document_child_window_controls(window);
                let mut app = CoduxApp::new_file_preview_window_from_state(
                    relative_path,
                    state,
                    runtime,
                    runtime_service,
                );
                app.window_appearance = window_appearance;
                theme::apply_component_theme_for_appearance(
                    &app.state.settings.theme,
                    &app.state.settings.theme_color,
                    window_appearance,
                    Some(window),
                    cx,
                );
                let view = cx.new(|_| app);
                view.update(cx, |app, cx| {
                    app.load_file_preview_window_content_async(cx);
                    app.refresh_window_runtime_data(cx);
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        );
        self.status_message = match result {
            Ok(handle) => {
                self.register_child_window_handle(handle.into());
                format!("file preview window opened: {opened_path}")
            }
            Err(error) => format!("failed to open file preview window: {error}"),
        };
        self.invalidate_file_panel(cx);
    }

    pub(in crate::app) fn refresh_window_runtime_data(&mut self, cx: &mut Context<Self>) {
        let refresh_pet = matches!(
            self.window_mode,
            AppWindowMode::Main
                | AppWindowMode::Settings
                | AppWindowMode::PetClaim
                | AppWindowMode::PetCustomInstall
                | AppWindowMode::PetDex
                | AppWindowMode::DesktopPet
        );
        let refresh_project_open = matches!(
            self.window_mode,
            AppWindowMode::Main | AppWindowMode::Settings | AppWindowMode::ProjectEditor
        );
        if !refresh_pet && !refresh_project_open {
            return;
        }
        self.runtime_trace(
            "window",
            &format!("runtime_data_refresh queued mode={:?}", self.window_mode),
        );
        if refresh_pet {
            self.refresh_pet_cache_async(cx);
        }
        if !refresh_project_open {
            return;
        }
        let service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                service.runtime_trace_frontend("window", "auxiliary_project_open_refresh start");
                let applications = service.project_open_applications();
                service.runtime_trace_frontend("window", "auxiliary_project_open_refresh ok");
                applications
            })
            .await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(applications) => {
                        app.project_open_applications = applications;
                    }
                    Err(error) => app.runtime_trace(
                        "window",
                        &format!("auxiliary_project_open_refresh failed join_error={error}"),
                    ),
                }
                app.invalidate_ui_region(cx, UiRegion::Root);
            });
        })
        .detach();
    }

    pub(in crate::app) fn activate_child_window(
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

    pub(in crate::app) fn register_child_window_handle(&mut self, handle: AnyWindowHandle) {
        self.child_windows
            .retain(|existing| existing.window_id() != handle.window_id());
        self.child_windows.push(handle);
    }

    pub(in crate::app) fn has_active_child_window(&mut self, cx: &mut Context<Self>) -> bool {
        if self.child_windows.is_empty() {
            return false;
        }

        let mut has_active = false;
        self.child_windows.retain(|handle| {
            match handle.update(cx, |_view, window, _cx| window.is_window_active()) {
                Ok(active) => {
                    has_active |= active;
                    true
                }
                Err(_) => false,
            }
        });
        self.clear_dead_child_window_slots();
        has_active
    }

    pub(in crate::app) fn prune_child_window_handles(&mut self, cx: &mut Context<Self>) {
        if self.child_windows.is_empty() {
            return;
        }

        let mut live_windows = Vec::with_capacity(self.child_windows.len());
        for handle in self.child_windows.iter().copied() {
            if handle.update(cx, |_view, _window, _cx| ()).is_ok() {
                live_windows.push(handle);
            }
        }
        let removed = self.child_windows.len().saturating_sub(live_windows.len());
        self.child_windows = live_windows;
        self.clear_dead_child_window_slots();
        self.runtime_trace("window", &format!("child_window_prune removed={removed}"));
    }

    fn clear_dead_child_window_slots(&mut self) {
        let live = self.child_windows.clone();
        for handle in [
            &mut self.settings_window,
            &mut self.about_window,
            &mut self.update_dialog_window,
            &mut self.git_clone_window,
            &mut self.git_credentials_window,
            &mut self.memory_manager_window,
            &mut self.pet_claim_window,
            &mut self.pet_custom_install_window,
            &mut self.pet_dex_window,
            &mut self.ssh_profile_editor_window,
            &mut self.project_editor_window,
            &mut self.worktree_creator_window,
        ] {
            if let Some(handle_value) = *handle {
                let still_live = live
                    .iter()
                    .any(|live_handle| live_handle.window_id() == handle_value.window_id());
                if !still_live {
                    *handle = None;
                }
            }
        }
    }

    pub(in crate::app) fn close_auxiliary_windows(&mut self, cx: &mut Context<Self>) {
        let handles = [
            &mut self.settings_window,
            &mut self.about_window,
            &mut self.update_dialog_window,
            &mut self.git_clone_window,
            &mut self.git_credentials_window,
            &mut self.memory_manager_window,
            &mut self.pet_claim_window,
            &mut self.pet_custom_install_window,
            &mut self.pet_dex_window,
            &mut self.ssh_profile_editor_window,
            &mut self.project_editor_window,
            &mut self.worktree_creator_window,
        ];

        for handle in handles {
            if let Some(window_handle) = handle.take() {
                let _ = window_handle.update(cx, |_view, window, _cx| window.remove_window());
            }
        }
        for handle in self.child_windows.drain(..) {
            let _ = handle.update(cx, |_view, window, _cx| window.remove_window());
        }
    }
}
