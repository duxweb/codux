use super::*;
use crate::app::app_events::{current_child_window_update_event, current_memory_update_event};
use crate::app::app_state::CoduxTooltipState;

impl CoduxApp {
    pub(super) fn text(&self, key: &str, fallback: &str) -> String {
        let locale = locale_from_language_setting(&self.state.settings.language);
        translate(&locale, key, fallback)
    }

    pub(super) fn runtime_trace(&self, category: &str, message: &str) {
        self.runtime_service
            .runtime_trace_frontend(category, message);
    }

    pub fn new(window: &mut Window, cx: &mut App) -> Result<Self> {
        let mut state = RuntimeState::load();
        set_active_settings_snapshot(state.settings.clone());
        theme::apply_component_theme(
            &state.settings.theme,
            &state.settings.theme_color,
            Some(window),
            cx,
        );
        let runtime = RuntimeInventory::load();
        let runtime_service = RuntimeService::new(state.support_dir.clone());
        let runtime_ingress = RuntimeIngressService::new()
            .start_background_with_ai_runtime(runtime_service.ai_runtime_bridge());
        let _ = runtime_service.recover_interrupted_memory_extraction_queue();
        let _ = runtime_service.clear_memory_extraction_failures();
        let power_sync_error = runtime_service.start_power_settings_sync().err();
        state.power = runtime_service.power_summary(&state.settings.sleep_mode);
        if let Some(error) = power_sync_error {
            state.power.error = Some(error);
        }
        let tool_permissions = runtime_service.sync_tool_permissions();
        state.tool_permissions = tool_permissions.clone();
        let ai_runtime_status = match runtime_service.start_ai_runtime_event_processing() {
            Ok(_) => {
                let snapshot = runtime_service.ai_runtime_state_snapshot();
                state.ai_runtime_state =
                    runtime_service.summarize_ai_runtime_state_snapshot(&snapshot);
                "AI runtime supervisor started".to_string()
            }
            Err(error) => format!("AI runtime supervisor failed: {error}"),
        };
        let ready_snapshot = runtime_service.app_runtime_ready(true, window.is_window_active());
        state.remote = ready_snapshot.remote.clone();
        let (terminal_layout, terminal_runtime) = normalize_terminal_restore_state(
            super::ai_runtime_status::terminal_layout_owner_id(&state).as_deref(),
            state.terminal_layout.clone(),
            state.terminal_runtime.clone(),
        );
        state.terminal_layout = terminal_layout;
        state.terminal_runtime = terminal_runtime;
        let restore_plan = terminal_restore_plan_for_language(
            &state.terminal_layout,
            &state.terminal_runtime,
            &state.settings.language,
        );
        prepare_memory_launch_artifacts(&state);
        let launch_context = terminal_launch_context(&state, &runtime, &tool_permissions);
        let terminal_config = terminal_config_for_settings(&state.settings);
        let terminal_manager = Arc::new(TerminalManager::with_ai_runtime_registry(
            runtime_service.ai_runtime_bridge().registry(),
        ));
        let (terminals, active_terminal_id, next_terminal_index) = spawn_terminal_tabs(
            &restore_plan,
            terminal_manager.clone(),
            launch_context.as_ref(),
            terminal_config,
            cx,
        )?;
        if let Some(view) = terminals
            .get(restore_plan.active_index)
            .or_else(|| terminals.first())
            .and_then(|tab| tab.panes.last())
            .and_then(|slot| slot.pane.as_ref())
            .map(|pane| pane.view.clone())
        {
            view.read(cx).focus_handle().focus(window, cx);
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
        state.memory_manager = runtime_service.reload_memory_manager(
            &state.projects,
            "project",
            memory_manager_project_id.as_deref(),
            "summary",
        );
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
        let selected_ssh_profile_id = None;
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
        let ai_history_refresh_project_ids = state
            .selected_project
            .as_ref()
            .map(|project| HashSet::from([project.id.clone()]))
            .unwrap_or_default();
        let ai_history_active_index_count = runtime_service.active_ai_history_index_count();
        let project_view_store = initial_project_view_store(&state);
        let worktree_view_store = initial_worktree_view_store(&state, &project_view_store);
        let terminal_view_store = initial_terminal_view_store(&state);

        let app = Self {
            window_mode: AppWindowMode::Main,
            root_focus_handle: None,
            terminals,
            terminal_manager,
            terminal_layout_loading: false,
            active_terminal_id,
            next_terminal_index,
            runtime,
            runtime_ingress,
            state,
            runtime_service,
            is_exiting: false,
            status_message: format!(
                "runtime ready · {} project{} · restored {} terminal tab{} · {}",
                ready_snapshot.projects.projects.len(),
                if ready_snapshot.projects.projects.len() == 1 {
                    ""
                } else {
                    "s"
                },
                restore_plan.tabs.len(),
                if restore_plan.tabs.len() == 1 {
                    ""
                } else {
                    "s"
                },
                ai_runtime_status
            ),
            desktop_pet_window: None,
            settings_window: None,
            about_window: None,
            git_clone_window: None,
            git_credentials_window: None,
            memory_manager_window: None,
            pet_claim_window: None,
            pet_custom_install_window: None,
            pet_dex_window: None,
            ssh_profile_editor_window: None,
            project_editor_window: None,
            worktree_creator_window: None,
            parent_main_window: None,
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
            file_editor_tabs: Vec::new(),
            active_file_editor_tab: None,
            file_editor_states: HashMap::new(),
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
            file_name_draft_value: String::new(),
            file_name_draft_select_all: false,
            file_tree_expanded_dirs: HashSet::new(),
            file_tree_children: HashMap::new(),
            file_tree_scroll_handle: UniformListScrollHandle::new(),
            file_preview_scroll_handle: UniformListScrollHandle::new(),
            file_panel_refreshing: false,
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
            git_review_aligned_rows: None,
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
            ssh_seen_revision: current_ssh_update_event().revision,
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
            pending_runtime_refresh: None,
            ai_runtime_state_save_tick: 0,
            dismissed_worktree_ai_completion_at: HashMap::new(),
            ai_index_progress_visible_until: 0.0,
            ai_index_progress_generation: 0,
            ai_history_active_index_count,
            ai_history_refresh_project_ids,
            project_switch_generation: 0,
            scheduled_work_in_flight: HashSet::new(),
            scheduled_work_last_started_at: HashMap::new(),
            scheduled_work_last_finished_at: HashMap::new(),
            task_column_refreshing: false,
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
            selected_runtime_terminal_id,
            selected_ssh_profile_id,
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
            project_column_collapsed: true,
            task_column_collapsed: false,
            project_list_store: None,
            project_column_view: None,
            task_column_view: None,
            task_column_header_view: None,
            task_worktree_list_view: None,
            task_session_list_view: None,
            workspace_column_view: None,
            workspace_toolbar_view: None,
            workspace_body_view: None,
            workspace_assistant_view: None,
            ai_stats_sidebar_view: None,
            ssh_sidebar_view: None,
            git_sidebar_view: None,
            git_files_panel_view: None,
            git_history_panel_view: None,
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
            worktree_creator_project_id: None,
            worktree_creator_project_name: String::new(),
            worktree_creator_project_path: String::new(),
            worktree_creator_base_branch: String::new(),
            worktree_creator_name: String::new(),
            worktree_creator_error: None,
            worktree_creator_submitting: false,
            tooltip_state: CoduxTooltipState::default(),
            ui_performance_counts: HashMap::new(),
            ui_performance_last_report_at: 0.0,
        };
        let support_dir = app.state.support_dir.clone();
        let (active_terminal_id, active_slot_id, sessions) = app.terminal_runtime_snapshot();
        codux_runtime::async_runtime::spawn_blocking(move || {
            if let Err(error) = TerminalRuntimeService::new(support_dir).save_from_gpui(
                active_terminal_id,
                active_slot_id,
                sessions,
            ) {
                codux_runtime::runtime_trace::runtime_trace(
                    "terminal-runtime",
                    &format!("failed to persist startup terminal runtime: {error}"),
                );
            }
        });
        Ok(app)
    }

    pub(super) fn spawn_runtime_scheduled_refresh(&mut self, cx: &mut Context<Self>) {
        let scheduler_key = "runtime_refresh";
        let policy = ScheduledWorkPolicy::new(1.0, 1.0);
        if self.runtime_refresh_in_flight {
            self.record_ui_scheduler_event("skip_busy", scheduler_key);
            return;
        }
        if !self.begin_scheduled_work(scheduler_key, policy) {
            return;
        }
        self.runtime_refresh_in_flight = true;
        let runtime_service = self.runtime_service.clone();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let refresh = codux_runtime::async_runtime::spawn_blocking(move || {
                let runtime_activity = runtime_service.reload_runtime_activity();
                let remote = runtime_service.reload_remote();
                RuntimeScheduledRefresh {
                    runtime_activity,
                    remote,
                }
            })
            .await
            .ok();

            let _ = this.update(cx, |app, _cx| {
                app.runtime_refresh_in_flight = false;
                app.finish_scheduled_work(scheduler_key);
                if let Some(refresh) = refresh {
                    app.pending_runtime_refresh = Some(refresh);
                }
            });
        })
        .detach();
    }

    pub(super) fn apply_pending_performance_refresh(&mut self) -> bool {
        let Some(performance) = self.pending_performance_refresh.take() else {
            return false;
        };
        let changed = self.state.performance.cpu_label != performance.cpu_label
            || self.state.performance.memory_label != performance.memory_label;
        if changed {
            self.state.performance = performance;
        }
        changed
    }

    pub(super) fn spawn_performance_refresh(&mut self, cx: &mut Context<Self>) {
        if !self.state.settings.developer_hud {
            return;
        }
        let scheduler_key = "performance_refresh";
        let policy = ScheduledWorkPolicy::new(0.5, 0.5);
        if self.performance_refresh_in_flight {
            self.record_ui_scheduler_event("skip_busy", scheduler_key);
            return;
        }
        if !self.begin_scheduled_work(scheduler_key, policy) {
            return;
        }
        self.performance_refresh_in_flight = true;

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let performance =
                codux_runtime::async_runtime::spawn_blocking(PerformanceService::summary)
                    .await
                    .ok();

            let _ = this.update(cx, |app, _cx| {
                app.performance_refresh_in_flight = false;
                app.finish_scheduled_work(scheduler_key);
                if let Some(performance) = performance {
                    app.pending_performance_refresh = Some(performance);
                }
            });
        })
        .detach();
    }

    pub(super) fn performance_refresh_interval_seconds(&self) -> u64 {
        self.state
            .settings
            .developer_refresh
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|seconds| *seconds > 0)
            .unwrap_or(3)
    }

    pub(super) fn shutdown_runtime_state_from_drop(&mut self) {
        if self.is_exiting {
            return;
        }
        self.is_exiting = true;

        codux_runtime::config::flush_all_config_writes();

        let support_dir = self.state.support_dir.clone();
        let terminal_snapshot = self.terminal_runtime_snapshot();
        let terminal_manager = self.terminal_manager.clone();
        let runtime_service = self.runtime_service.clone();
        let _ = std::thread::Builder::new()
            .name("codux-runtime-shutdown".to_string())
            .spawn(move || {
                let (active_terminal_id, active_slot_id, sessions) = terminal_snapshot;
                let _ = TerminalRuntimeService::new(support_dir).save_from_gpui(
                    active_terminal_id,
                    active_slot_id,
                    sessions,
                );
                for terminal in terminal_manager.list() {
                    let _ = terminal_manager.kill(&terminal.id);
                }
                runtime_service.shutdown_runtime_state();
            });
    }
}
