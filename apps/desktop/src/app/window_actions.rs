use super::*;
use crate::app::app_events::{current_child_window_update_event, current_memory_update_event};
use crate::app::app_state::CoduxTooltipState;

pub(in crate::app) struct AuxiliaryWindowSpec {
    pub(in crate::app) slot: AuxiliaryWindowSlot,
    pub(in crate::app) title: SharedString,
    pub(in crate::app) size: gpui::Size<Pixels>,
    pub(in crate::app) min_size: gpui::Size<Pixels>,
    pub(in crate::app) already_open_message: &'static str,
    pub(in crate::app) opened_message: &'static str,
    pub(in crate::app) failed_prefix: &'static str,
}

#[derive(Clone, Copy)]
pub(in crate::app) enum AuxiliaryWindowSlot {
    Settings,
    About,
    UpdateDialog,
    GitClone,
    GitCredentials,
    MemoryManager,
    ProjectEditor,
    WorktreeCreator,
    SshProfileEditor,
    DbProfileEditor,
    FilePicker,
}

impl CoduxApp {
    pub(super) fn new_settings_window_from_state(
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut state = state;
        state.settings = settings_with_active_restart_locked_values(&state.settings);
        Self::new_auxiliary_window_from_state(state, runtime, runtime_service)
    }

    pub(super) fn new_memory_manager_window(
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
            desktop_pet_line: desktop_pet_fallback_line().to_string(),
            desktop_pet_tone: DesktopPetActivityTone::Normal,
            desktop_pet_plan_items: Vec::new(),
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
            file_editor_scroll: HashMap::new(),
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
            git_remote_editor_open: false,
            git_remote_name: "origin".to_string(),
            git_remote_url: String::new(),
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
            pending_runtime_refresh: None,
            ai_runtime_state_save_tick: 0,
            pane_agent_lifecycle: HashMap::new(),
            agent_git_refresh_after: None,
            dismissed_worktree_ai_completion_at: HashMap::new(),
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
            project_editor_host_device_id: None,
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

    pub(super) fn open_auxiliary_window(
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

    pub(super) fn new_project_editor_window_from_state(
        project: ProjectInfo,
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut app = Self::new_settings_window_from_state(state, runtime, runtime_service);
        app.window_mode = AppWindowMode::ProjectEditor;
        app.status_message = format!("editing project: {}", project.name);
        app.project_editor_project_id = Some(project.id);
        app.project_editor_name = project.name;
        app.project_editor_path = project.path;
        app.project_editor_badge_symbol = project.badge_symbol;
        app.project_editor_badge_color_hex = project
            .badge_color_hex
            .unwrap_or_else(|| PROJECT_BADGE_COLORS[0].to_string());
        app.project_editor_host_device_id = project.host_device_id;
        app
    }

    pub(super) fn new_project_creator_window_from_state(
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

    pub(super) fn new_ssh_profile_editor_window_from_state(
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

    pub(super) fn new_db_profile_editor_window_from_state(
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

    pub(super) fn new_file_editor_window_from_state(
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
        app.file_editor_scroll.clear();
        app.file_editor_loading_states.clear();
        app.active_file_editor_tab = None;
        app.add_file_editor_window_tab(relative_path);
        app
    }

    pub(super) fn new_file_preview_window_from_state(
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

    pub(super) fn open_file_editor_window(
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

    pub(super) fn open_file_preview_window(
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

    pub(super) fn refresh_window_runtime_data(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn register_child_window_handle(&mut self, handle: AnyWindowHandle) {
        self.child_windows
            .retain(|existing| existing.window_id() != handle.window_id());
        self.child_windows.push(handle);
    }

    pub(super) fn has_active_child_window(&mut self, cx: &mut Context<Self>) -> bool {
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

    pub(super) fn prune_child_window_handles(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn close_auxiliary_windows(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn focused_or_active_terminal_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Entity<TerminalView>> {
        self.focused_terminal_view(window, cx).or_else(|| {
            (self.workspace_view == WorkspaceView::Terminal
                && !self.file_sidebar_contains_focused(window, cx)
                && !self.terminal_search_contains_focused(window, cx))
            .then(|| self.active_terminal_view())
            .flatten()
        })
    }

    fn terminal_search_contains_focused(&self, window: &Window, cx: &mut Context<Self>) -> bool {
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

    pub(super) fn open_settings_window(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_window_with_pane(SettingsPane::General, cx);
    }

    pub(super) fn open_settings_window_with_pane(
        &mut self,
        pane: SettingsPane,
        cx: &mut Context<Self>,
    ) {
        let pane_label = pane.label(&self.state.settings.language);
        let opened = self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::Settings,
                title: SharedString::from("Codux Settings"),
                size: size(px(980.0), px(720.0)),
                min_size: size(px(760.0), px(560.0)),
                already_open_message: "settings window already opened",
                opened_message: "settings window opened",
                failed_prefix: "failed to open settings window",
            },
            cx,
            move |state, runtime, runtime_service, window, cx| {
                let mut app =
                    CoduxApp::new_settings_window_from_state(state, runtime, runtime_service);
                app.active_settings_pane = pane;
                let _ = (window, cx);
                app
            },
            |view, _window, cx| {
                view.update(cx, |app, cx| app.start_settings_remote_snapshot_loop(cx));
            },
        );

        if opened {
            self.status_message = format!("{}: {pane_label}", self.status_message);
        }
        self.invalidate_status_bar(cx);
    }

    pub(super) fn open_remote_settings_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_window_with_pane(SettingsPane::Remote, cx);
    }

    pub(super) fn open_git_clone_window(&mut self, cx: &mut Context<Self>) {
        let labels = GitSidebarLabels::load(&self.state.settings.language);
        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::GitClone,
                title: SharedString::from(labels.clone_repository.clone()),
                size: size(px(420.0), px(204.0)),
                min_size: size(px(360.0), px(190.0)),
                already_open_message: "Git clone window already opened",
                opened_message: "Git clone window opened",
                failed_prefix: "failed to open Git clone window",
            },
            cx,
            |state, runtime, runtime_service, _window, _cx| {
                let mut app =
                    CoduxApp::new_settings_window_from_state(state, runtime, runtime_service);
                app.window_mode = AppWindowMode::GitClone;
                app.status_message = "Git clone window ready".to_string();
                app
            },
            |_view, _window, _cx| {},
        );
        self.invalidate_status_bar(cx);
    }

    pub(super) fn open_git_credentials_window(&mut self, cx: &mut Context<Self>) {
        let labels = GitSidebarLabels::load(&self.state.settings.language);
        let project_id = self.git_credential_project_id.clone();
        let project_name = self.git_credential_project_name.clone();
        let project_path = self.git_credential_project_path.clone();
        let remote_url = self.git_credential_remote_url.clone();
        let username = self.git_credential_username.clone();
        let error = self.git_credential_error.clone();
        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::GitCredentials,
                title: SharedString::from(labels.credentials_title.clone()),
                size: size(
                    px(GIT_CREDENTIALS_WINDOW_WIDTH),
                    px(GIT_CREDENTIALS_COMPACT_HEIGHT),
                ),
                min_size: size(px(380.0), px(GIT_CREDENTIALS_COMPACT_HEIGHT)),
                already_open_message: "Git credentials window already opened",
                opened_message: "Git credentials window opened",
                failed_prefix: "failed to open Git credentials window",
            },
            cx,
            move |state, runtime, runtime_service, _window, _cx| {
                let mut app =
                    CoduxApp::new_settings_window_from_state(state, runtime, runtime_service);
                app.window_mode = AppWindowMode::GitCredentials;
                app.status_message = "Git credentials window ready".to_string();
                app.git_credential_project_id = project_id;
                app.git_credential_project_name = project_name;
                app.git_credential_project_path = project_path;
                app.git_credential_remote_url = remote_url;
                app.git_credential_username = username;
                app.git_credential_error = error;
                app
            },
            |view, window, cx| {
                let expanded = view.read(cx).git_credential_error.is_some();
                resize_git_credentials_window(window, expanded);
            },
        );
        self.invalidate_status_bar(cx);
    }

    pub(super) fn open_ssh_profile_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_ssh_profile_editor(None, cx);
    }

    pub(super) fn open_db_profile_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_db_profile_editor(None, cx);
    }

    pub(super) fn open_selected_ssh_profile_editor(
        &mut self,
        profile_id: String,
        cx: &mut Context<Self>,
    ) {
        self.open_ssh_profile_editor(Some(profile_id), cx);
    }

    pub(super) fn open_selected_db_profile_editor(
        &mut self,
        profile_id: String,
        cx: &mut Context<Self>,
    ) {
        self.open_db_profile_editor(Some(profile_id), cx);
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
            self.invalidate_remote_panel(cx);
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
                self.invalidate_remote_panel(cx);
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
        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::SshProfileEditor,
                title: SharedString::from(title),
                size: size(px(520.0), px(430.0)),
                min_size: size(px(460.0), px(390.0)),
                already_open_message: "SSH profile editor already opened",
                opened_message: "SSH profile editor opened",
                failed_prefix: "failed to open SSH profile editor",
            },
            cx,
            move |state, runtime, runtime_service, _window, _cx| {
                CoduxApp::new_ssh_profile_editor_window_from_state(
                    profile,
                    state,
                    runtime,
                    runtime_service,
                )
            },
            |_view, _window, _cx| {},
        );
        self.invalidate_remote_panel(cx);
    }

    pub(super) fn open_db_profile_editor(
        &mut self,
        profile_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(project_id) = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            self.status_message = self.db_text(
                "db.profile.no_project",
                "Select a project before adding a database profile",
            );
            self.invalidate_db_panel(cx);
            return;
        };
        self.reload_selected_project_db();
        self.normalize_selected_db_profile();
        if Self::activate_child_window(&mut self.db_profile_editor_window, cx) {
            self.status_message = self.db_text(
                "db.profile.editor.already_open",
                "Database profile editor already opened",
            );
            self.invalidate_db_panel(cx);
            return;
        }

        let profile = if let Some(profile_id) = profile_id {
            let snapshot = self.runtime_service.db_profiles(Some(&project_id));
            let Some(profile) = snapshot
                .profiles
                .into_iter()
                .find(|profile| profile.id == profile_id)
            else {
                self.status_message = self.db_text(
                    "db.profile.unavailable",
                    "Database profile is no longer available",
                );
                self.invalidate_db_panel(cx);
                return;
            };
            Some(profile)
        } else {
            None
        };
        let locale = locale_from_language_setting(&self.state.settings.language);
        let title = if profile.is_some() {
            translate(&locale, "db.profile.edit_window", "Edit Database Profile")
        } else {
            translate(&locale, "db.profile.add_window", "Add Database Profile")
        };
        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::DbProfileEditor,
                title: SharedString::from(title),
                size: size(px(520.0), px(520.0)),
                min_size: size(px(460.0), px(430.0)),
                already_open_message: "Database profile editor already opened",
                opened_message: "Database profile editor opened",
                failed_prefix: "failed to open database profile editor",
            },
            cx,
            move |state, runtime, runtime_service, _window, _cx| {
                CoduxApp::new_db_profile_editor_window_from_state(
                    profile,
                    state,
                    runtime,
                    runtime_service,
                )
            },
            |_view, _window, _cx| {},
        );
        self.invalidate_db_panel(cx);
    }

    pub(super) fn toggle_project_column(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.project_column_collapsed = !self.project_column_collapsed;
        self.invalidate_project_shell(cx);
    }

    pub(super) fn toggle_task_column(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.task_column_collapsed = !self.task_column_collapsed;
        self.invalidate_project_shell(cx);
    }

    pub(super) fn close_active_workspace_item(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.workspace_view {
            WorkspaceView::Terminal => {
                // In split mode, if the file editor holds focus, Cmd+W closes the
                // active file tab (and the split collapses once the last one is
                // gone) instead of closing the terminal.
                let close_split_file = self.workspace_split == Some(WorkspaceSplitKind::FileEditor)
                    && self.active_file_editor_tab.is_some()
                    && self.active_file_editor_split_focused(window, cx);
                if close_split_file {
                    if let Some(relative_path) = self.active_file_editor_tab.clone() {
                        self.close_file_editor_tab(relative_path, window, cx);
                        self.status_message = "file tab closed".to_string();
                    }
                } else {
                    self.confirm_or_close_active_terminal_target(window, cx);
                }
            }
            WorkspaceView::Files => {
                if let Some(relative_path) = self.active_file_editor_tab.clone() {
                    self.close_file_editor_tab(relative_path, window, cx);
                    self.status_message = "file tab closed".to_string();
                } else if self.selected_file_entry.take().is_some() {
                    self.file_preview = "select a file to preview it".to_string();
                    self.file_editable = false;
                    self.file_dirty = false;
                    self.status_message = "file preview closed".to_string();
                } else {
                    self.status_message = "no active file preview".to_string();
                }
                self.invalidate_file_panel(cx);
            }
            WorkspaceView::Review => {
                self.status_message = "no active review item to close".to_string();
                self.invalidate_git_panel(cx);
            }
            WorkspaceView::Stats => {
                self.set_workspace_view(WorkspaceView::Terminal, window, cx);
            }
        }
    }

    pub(super) fn set_workspace_view(
        &mut self,
        view: WorkspaceView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_view = view;
        match view {
            WorkspaceView::Files => {
                self.assistant_panel = Some(AssistantPanel::FileManager);
                self.refresh_files_panel_state_async(cx);
            }
            WorkspaceView::Review => {
                self.assistant_panel = None;
                self.refresh_git_panel_state_async(cx);
                self.ensure_selected_git_review_file_loaded_async(cx);
            }
            WorkspaceView::Stats => {
                self.assistant_panel = None;
                self.refresh_stats_workspace_data(false, cx);
            }
            WorkspaceView::Terminal => {
                self.activate_first_terminal();
                if let Err(error) = self.ensure_active_terminal_mounted(cx) {
                    self.status_message = format!("failed to focus terminal: {error}");
                } else {
                    self.focus_active_terminal(window, cx);
                }
            }
        }
        self.invalidate_workspace(cx);
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn show_stats_workspace_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view == WorkspaceView::Stats {
            self.set_workspace_view(WorkspaceView::Terminal, window, cx);
        } else {
            self.set_workspace_view(WorkspaceView::Stats, window, cx);
        }
    }

    pub(in crate::app) fn refresh_stats_workspace_data(
        &mut self,
        show_progress: bool,
        cx: &mut Context<Self>,
    ) {
        self.refresh_ai_global_history_summary(cx);
        if self.state.selected_project.is_some() {
            self.start_ai_history_refresh(show_progress, cx);
        } else {
            self.invalidate_ui(cx, [UiRegion::WorkspaceBody, UiRegion::StatusBar]);
        }
    }

    pub(in crate::app) fn set_stats_time_range(
        &mut self,
        range: StatsTimeRange,
        cx: &mut Context<Self>,
    ) {
        if self.stats_time_range == range {
            return;
        }
        self.stats_time_range = range;
        self.invalidate_ui(cx, [UiRegion::WorkspaceBody]);
    }

    pub(super) fn set_settings_pane(&mut self, pane: SettingsPane, cx: &mut Context<Self>) {
        self.active_settings_pane = pane;
        self.invalidate_ui_region(cx, UiRegion::Root);
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
            self.refresh_assistant_panel_state(panel, cx);
        }
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceAssistant,
                UiRegion::AIStatsSidebar,
                UiRegion::ServerInfoSidebar,
                UiRegion::SshSidebar,
                UiRegion::DbSidebar,
                UiRegion::FileSidebar,
                UiRegion::GitSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(super) fn refresh_assistant_panel_state(
        &mut self,
        panel: AssistantPanel,
        cx: &mut Context<Self>,
    ) {
        match panel {
            AssistantPanel::AIStats => {
                self.refresh_ai_stats_panel_async(cx);
            }
            AssistantPanel::ServerInfo => {
                self.refresh_server_info_panel(cx);
            }
            AssistantPanel::SSH => {
                self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
                self.normalize_selected_ssh_profile();
            }
            AssistantPanel::DB => {
                self.reload_selected_project_db();
                self.normalize_selected_db_profile();
            }
            AssistantPanel::FileManager => {
                self.refresh_files_panel_state_async(cx);
            }
            AssistantPanel::Git => {
                self.refresh_git_panel_state_async(cx);
            }
        }
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
                app.open_update_dialog_window(window, cx)
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
            native_menu::QuitCodux,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                if app.window_mode == AppWindowMode::Main {
                    app.request_quit(cx);
                }
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
            native_menu::ViewStats,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.set_workspace_view(WorkspaceView::Stats, window, cx)
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
            native_menu::CreateTask,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_worktree_creator_window(window, cx)
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
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                // The global cmd-f binding consumes the keystroke before the
                // terminal's key handler sees it — route to terminal search here.
                if app.terminal_search_contains_focused(window, cx) {
                    return;
                }
                if let Some(terminal) = app.focused_or_active_terminal_view(window, cx) {
                    terminal.update(cx, |terminal, cx| terminal.open_search(window, cx));
                    return;
                }
                if let Some(editor) = app.active_file_editor_state() {
                    editor.update(cx, |state, cx| state.focus(window, cx));
                    window.dispatch_action(Box::new(gpui_component::input::Search), cx);
                }
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
        register!(native_menu::ToggleFullscreen, |app: &mut CoduxApp,
                                                  window: &mut Window,
                                                  _cx: &mut Context<
            CoduxApp,
        >| {
            window.toggle_fullscreen();
            app.main_window_fullscreen = window.is_fullscreen();
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
