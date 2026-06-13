use super::*;

static ACTIVE_SETTINGS_SNAPSHOT: OnceLock<SettingsSummary> = OnceLock::new();

pub(crate) fn set_active_settings_snapshot(settings: SettingsSummary) {
    let _ = ACTIVE_SETTINGS_SNAPSHOT.set(settings);
}

pub(crate) fn active_settings_snapshot() -> Option<SettingsSummary> {
    ACTIVE_SETTINGS_SNAPSHOT.get().cloned()
}

pub(in crate::app) fn settings_with_active_restart_locked_values(
    settings: &SettingsSummary,
) -> SettingsSummary {
    let Some(active) = active_settings_snapshot() else {
        return settings.clone();
    };
    let mut next = settings.clone();
    next.language = active.language;
    next
}

pub struct CoduxApp {
    pub(in crate::app) window_mode: AppWindowMode,
    pub(in crate::app) root_focus_handle: Option<FocusHandle>,
    pub(in crate::app) terminals: Vec<TerminalTab>,
    pub(in crate::app) terminal_pane_registry: HashMap<String, TerminalPane>,
    pub(in crate::app) terminal_manager: Arc<TerminalManager>,
    pub(in crate::app) terminal_layout_loading: bool,
    pub(in crate::app) active_terminal_id: usize,
    pub(in crate::app) active_terminal_runtime_ids: HashMap<WorktreeScopeKey, String>,
    pub(in crate::app) active_bottom_terminal_ids: HashMap<WorktreeScopeKey, String>,
    pub(in crate::app) terminal_layout_cache: HashMap<WorktreeScopeKey, TerminalLayoutCacheEntry>,
    pub(in crate::app) file_panel_cache: HashMap<WorktreeScopeKey, FilePanelState>,
    pub(in crate::app) next_terminal_index: usize,
    pub(in crate::app) runtime: RuntimeInventory,
    pub(in crate::app) state: RuntimeState,
    pub(in crate::app) runtime_service: RuntimeService,
    pub(in crate::app) window_appearance: WindowAppearance,
    pub(in crate::app) main_window_fullscreen: bool,
    pub(in crate::app) main_window_lost_to_external_app: bool,
    pub(in crate::app) _observe_window_appearance: Option<Subscription>,
    pub(in crate::app) _observe_window_activation: Option<Subscription>,
    pub(in crate::app) is_exiting: bool,
    pub(in crate::app) main_window_close_handler_registered: bool,
    pub(in crate::app) last_quit_request_at: Option<Instant>,
    pub(in crate::app) pending_terminal_close: Option<PendingTerminalClose>,
    pub(in crate::app) status_message: String,
    pub(in crate::app) toast_message: Option<String>,
    pub(in crate::app) toast_revision: u64,
    pub(in crate::app) pending_restart_language: Option<String>,
    pub(in crate::app) desktop_pet_window: Option<AnyWindowHandle>,
    pub(in crate::app) settings_window: Option<AnyWindowHandle>,
    pub(in crate::app) about_window: Option<AnyWindowHandle>,
    pub(in crate::app) update_dialog_window: Option<AnyWindowHandle>,
    pub(in crate::app) git_clone_window: Option<AnyWindowHandle>,
    pub(in crate::app) git_credentials_window: Option<AnyWindowHandle>,
    pub(in crate::app) memory_manager_window: Option<AnyWindowHandle>,
    pub(in crate::app) pet_claim_window: Option<AnyWindowHandle>,
    pub(in crate::app) pet_custom_install_window: Option<AnyWindowHandle>,
    pub(in crate::app) pet_dex_window: Option<AnyWindowHandle>,
    pub(in crate::app) ssh_profile_editor_window: Option<AnyWindowHandle>,
    pub(in crate::app) project_editor_window: Option<AnyWindowHandle>,
    pub(in crate::app) terminal_tab_editor_window: Option<AnyWindowHandle>,
    pub(in crate::app) worktree_creator_window: Option<AnyWindowHandle>,
    pub(in crate::app) child_windows: Vec<AnyWindowHandle>,
    pub(in crate::app) parent_main_window: Option<gpui::WeakEntity<CoduxApp>>,
    pub(in crate::app) desktop_pet_line: String,
    pub(in crate::app) desktop_pet_tone: DesktopPetActivityTone,
    pub(in crate::app) desktop_pet_plan_items: Vec<DesktopPetPlanItem>,
    pub(in crate::app) desktop_pet_main_window_fullscreen: bool,
    pub(in crate::app) desktop_pet_active_llm_key: String,
    pub(in crate::app) desktop_pet_requested_llm_key: String,
    pub(in crate::app) desktop_pet_last_llm_requested_at: f64,
    pub(in crate::app) desktop_pet_next_hydration_reminder_at: f64,
    pub(in crate::app) desktop_pet_next_sedentary_reminder_at: f64,
    pub(in crate::app) desktop_pet_next_late_night_reminder_at: f64,
    pub(in crate::app) desktop_pet_next_idle_llm_at: f64,
    pub(in crate::app) desktop_pet_line_visible_until: f64,
    pub(in crate::app) pet_sprite_frame: usize,
    pub(in crate::app) pet_sprite_animation_active: bool,
    pub(in crate::app) file_preview: String,
    pub(in crate::app) file_preview_window_path: Option<String>,
    pub(in crate::app) file_preview_window_content: String,
    pub(in crate::app) file_preview_window_error: Option<String>,
    pub(in crate::app) file_preview_window_view:
        Option<gpui::Entity<super::file_editor::FilePreviewWindowView>>,
    pub(in crate::app) file_editable: bool,
    pub(in crate::app) file_dirty: bool,
    pub(in crate::app) file_editor_tabs: Vec<FileEditorTab>,
    pub(in crate::app) active_file_editor_tab: Option<String>,
    pub(in crate::app) file_editor_states: HashMap<String, gpui::Entity<InputState>>,
    // Most-recently-accessed editor-state keys, oldest first. Bounds the
    // editor-state cache: opening files across many projects/worktrees would
    // otherwise retain every file's rope + syntax tree forever (only cleared on
    // worktree switch / window close), growing process memory unboundedly.
    pub(in crate::app) file_editor_state_lru: Vec<String>,
    // Last scroll offset per editor-state key, captured when a state is evicted
    // by the LRU and restored when the file is reopened, so a tab evicted from
    // the cache still returns to its previous scroll line (small Point, unlike
    // the heavy InputState it survives).
    pub(in crate::app) file_editor_scroll: HashMap<String, gpui::Point<gpui::Pixels>>,
    pub(in crate::app) file_editor_loading_states: HashSet<String>,
    pub(in crate::app) file_search_open: bool,
    pub(in crate::app) file_search_query: String,
    pub(in crate::app) file_search_match_index: usize,
    pub(in crate::app) file_directory: String,
    pub(in crate::app) selected_file_entry: Option<String>,
    pub(in crate::app) selected_file_entries: HashSet<String>,
    pub(in crate::app) file_selection_anchor: Option<String>,
    pub(in crate::app) file_name_draft_kind: Option<FileNameDraftKind>,
    pub(in crate::app) file_name_draft_target: Option<String>,
    pub(in crate::app) file_name_draft_value: String,
    pub(in crate::app) file_name_draft_select_all: bool,
    pub(in crate::app) file_tree_expanded_dirs: HashSet<String>,
    pub(in crate::app) file_tree_children: HashMap<String, Vec<FileEntry>>,
    pub(in crate::app) file_tree_scroll_handle: UniformListScrollHandle,
    pub(in crate::app) file_panel_refreshing: bool,
    pub(in crate::app) selected_git_file: Option<String>,
    pub(in crate::app) selected_git_branch: Option<String>,
    pub(in crate::app) git_review: GitReviewSummary,
    pub(in crate::app) git_expanded_sections: HashSet<String>,
    pub(in crate::app) git_expanded_dirs: HashSet<String>,
    pub(in crate::app) git_tree_children: HashMap<String, Vec<GitFileStatus>>,
    pub(in crate::app) git_files_scroll_handle: VirtualListScrollHandle,
    pub(in crate::app) git_review_code_scroll_handle: ScrollHandle,
    pub(in crate::app) selected_git_files: HashSet<String>,
    pub(in crate::app) git_diff_preview: String,
    pub(in crate::app) git_diff_window_path: Option<String>,
    pub(in crate::app) git_diff_window_content: String,
    pub(in crate::app) git_diff_window_error: Option<String>,
    pub(in crate::app) git_review_content: Option<GitReviewContentSummary>,
    pub(in crate::app) git_review_derived_rows: Option<super::sidebars::GitReviewDerivedRows>,
    pub(in crate::app) git_review_refreshing: bool,
    pub(in crate::app) git_clone_remote_url: String,
    pub(in crate::app) git_remote_editor_open: bool,
    pub(in crate::app) git_remote_name: String,
    pub(in crate::app) git_remote_url: String,
    pub(in crate::app) git_running_operation: Option<GitRunningOperation>,
    pub(in crate::app) git_credential_project_id: Option<String>,
    pub(in crate::app) git_credential_project_name: String,
    pub(in crate::app) git_credential_project_path: String,
    pub(in crate::app) git_credential_remote_url: String,
    pub(in crate::app) git_credential_username: String,
    pub(in crate::app) git_credential_password_or_token: String,
    pub(in crate::app) git_credential_error: Option<String>,
    pub(in crate::app) git_credential_retrying: bool,
    pub(in crate::app) git_commit_message: String,
    pub(in crate::app) git_commit_message_revision: u64,
    pub(in crate::app) pet_install_url: String,
    pub(in crate::app) pet_install_display_name: String,
    pub(in crate::app) pet_install_preview: Option<PetCustomPetInstallPreview>,
    pub(in crate::app) pet_install_error: Option<String>,
    pub(in crate::app) pet_install_previewing: bool,
    pub(in crate::app) pet_installing: bool,
    pub(in crate::app) pet_catalog: PetCatalog,
    pub(in crate::app) pet_snapshot: PetSnapshot,
    pub(in crate::app) pet_custom_pets: Vec<PetCustomPet>,
    pub(in crate::app) pet_sprite_paths: HashMap<String, ImageSource>,
    pub(in crate::app) project_scroll_handle: UniformListScrollHandle,
    pub(in crate::app) task_scroll_handle: UniformListScrollHandle,
    pub(in crate::app) session_scroll_handle: UniformListScrollHandle,
    pub(in crate::app) ssh_scroll_handle: UniformListScrollHandle,
    pub(in crate::app) git_history_scroll_handle: VirtualListScrollHandle,
    pub(in crate::app) pet_dex_scroll_handle: VirtualListScrollHandle,
    pub(in crate::app) pet_custom_install_seen_revision: u64,
    pub(in crate::app) pet_update_seen_revision: u64,
    pub(in crate::app) settings_seen_revision: u64,
    pub(in crate::app) memory_seen_revision: u64,
    pub(in crate::app) child_window_update_seen_revision: u64,
    pub(in crate::app) child_window_settings_seen_revision: u64,
    pub(in crate::app) child_window_ssh_seen_revision: u64,
    pub(in crate::app) child_window_memory_seen_revision: u64,
    pub(in crate::app) child_window_project_seen_revision: u64,
    pub(in crate::app) child_window_worktree_seen_revision: u64,
    pub(in crate::app) child_window_git_seen_revision: u64,
    pub(in crate::app) pet_claim_species: String,
    pub(in crate::app) pet_name_editing: bool,
    pub(in crate::app) pet_dex_spotlight: Option<PetDexSpotlight>,
    pub(in crate::app) selected_ai_session_id: Option<String>,
    pub(in crate::app) ai_session_delete_confirm_id: Option<String>,
    pub(in crate::app) selected_ai_provider_id: Option<String>,
    pub(in crate::app) ai_provider_testing_id: Option<String>,
    pub(in crate::app) ai_provider_test_result: Option<AIProviderTestResult>,
    pub(in crate::app) selected_memory_entry_id: Option<String>,
    pub(in crate::app) selected_memory_summary_id: Option<String>,
    pub(in crate::app) selected_notification_channel_id: Option<String>,
    pub(in crate::app) notification_testing_channel_id: Option<String>,
    pub(in crate::app) runtime_refresh_in_flight: bool,
    pub(in crate::app) runtime_ready: bool,
    pub(in crate::app) pending_runtime_refresh: Option<RuntimeScheduledRefresh>,
    pub(in crate::app) ai_runtime_state_save_tick: u64,
    pub(in crate::app) dismissed_worktree_ai_completion_at: HashMap<String, f64>,
    pub(in crate::app) ai_index_progress_visible_until: f64,
    pub(in crate::app) ai_index_progress_generation: u64,
    pub(in crate::app) ai_history_active_index_count: usize,
    pub(in crate::app) ai_history_refreshing: bool,
    pub(in crate::app) project_switch_generation: u64,
    pub(in crate::app) scheduled_work_in_flight: HashSet<String>,
    pub(in crate::app) scheduled_work_last_started_at: HashMap<String, f64>,
    pub(in crate::app) scheduled_work_last_finished_at: HashMap<String, f64>,
    pub(in crate::app) task_column_refreshing: bool,
    pub(in crate::app) terminal_font_families: Vec<String>,
    pub(in crate::app) terminal_font_families_loaded: bool,
    pub(in crate::app) terminal_font_families_loading: bool,
    pub(in crate::app) memory_progress_visible_until: f64,
    pub(in crate::app) memory_progress_generation: u64,
    pub(in crate::app) memory_manager_refreshing: bool,
    pub(in crate::app) memory_manager_refresh_generation: u64,
    pub(in crate::app) memory_project_profile_refreshing: bool,
    pub(in crate::app) performance_refresh_in_flight: bool,
    pub(in crate::app) pending_performance_refresh: Option<PerformanceSummary>,
    pub(in crate::app) today_level_day_start: f64,
    pub(in crate::app) active_settings_pane: SettingsPane,
    pub(in crate::app) memory_manager_tab: MemoryManagerTab,
    pub(in crate::app) memory_manager_scope: String,
    pub(in crate::app) memory_manager_project_id: Option<String>,
    pub(in crate::app) memory_processing: bool,
    pub(in crate::app) memory_extraction_status_refreshing: bool,
    pub(in crate::app) memory_status_seen_failed_count: i64,
    pub(in crate::app) selected_runtime_terminal_id: Option<String>,
    pub(in crate::app) selected_ssh_profile_id: Option<String>,
    pub(in crate::app) ssh_draft_open: bool,
    pub(in crate::app) ssh_testing: bool,
    pub(in crate::app) ssh_test_result: Option<SSHProfileTestDisplay>,
    pub(in crate::app) ssh_draft_id: Option<String>,
    pub(in crate::app) ssh_draft_name: String,
    pub(in crate::app) ssh_draft_host: String,
    pub(in crate::app) ssh_draft_port: String,
    pub(in crate::app) ssh_draft_username: String,
    pub(in crate::app) ssh_draft_credential_kind: String,
    pub(in crate::app) ssh_draft_private_key_path: String,
    pub(in crate::app) ssh_draft_password: String,
    pub(in crate::app) ssh_draft_key_passphrase: String,
    pub(in crate::app) selected_remote_device_id: Option<String>,
    pub(in crate::app) remote_reconnecting: bool,
    pub(in crate::app) remote_pairing_sheet_open: bool,
    pub(in crate::app) remote_pairing_creating: bool,
    pub(in crate::app) remote_pairing_error: Option<String>,
    pub(in crate::app) remote_pairing_poll_generation: u64,
    pub(in crate::app) recording_shortcut_id: Option<String>,
    pub(in crate::app) workspace_view: WorkspaceView,
    /// Secondary body panel shown next to the terminal workspace (split mode).
    /// `None` = single full-body view (the default). Session-only; not persisted.
    pub(in crate::app) workspace_split: Option<WorkspaceSplitKind>,
    pub(in crate::app) assistant_panel: Option<AssistantPanel>,
    pub(in crate::app) project_column_collapsed: bool,
    pub(in crate::app) task_column_collapsed: bool,
    pub(in crate::app) project_list_state: Option<gpui::Entity<ProjectListState>>,
    pub(in crate::app) project_column_view: Option<gpui::Entity<ProjectColumnView>>,
    pub(in crate::app) task_column_view: Option<gpui::Entity<TaskColumnView>>,
    pub(in crate::app) task_column_header_view: Option<gpui::Entity<TaskColumnHeaderView>>,
    pub(in crate::app) task_worktree_list_view: Option<gpui::Entity<TaskWorktreeListView>>,
    pub(in crate::app) task_session_list_view: Option<gpui::Entity<TaskSessionListView>>,
    pub(in crate::app) workspace_column_view: Option<gpui::Entity<WorkspaceColumnView>>,
    pub(in crate::app) workspace_toolbar_view:
        Option<gpui::Entity<workspace_views::WorkspaceToolbarView>>,
    pub(in crate::app) workspace_body_view:
        Option<gpui::Entity<workspace_views::WorkspaceBodyView>>,
    pub(in crate::app) workspace_assistant_view:
        Option<gpui::Entity<workspace_views::WorkspaceAssistantView>>,
    pub(in crate::app) ai_stats_sidebar_view: Option<gpui::Entity<sidebars::AIStatsSidebarView>>,
    pub(in crate::app) ssh_sidebar_view: Option<gpui::Entity<sidebars::SshSidebarView>>,
    pub(in crate::app) git_sidebar_view: Option<gpui::Entity<sidebars::GitSidebarView>>,
    pub(in crate::app) git_files_panel_view: Option<gpui::Entity<sidebars::GitFilesPanelView>>,
    pub(in crate::app) git_history_panel_view: Option<gpui::Entity<sidebars::GitHistoryPanelView>>,
    pub(in crate::app) status_bar_view: Option<gpui::Entity<StatusBarView>>,
    pub(in crate::app) file_sidebar_view: Option<gpui::Entity<FileSidebarView>>,
    pub(in crate::app) project_open_applications: Vec<ProjectOpenApplicationSummary>,
    pub(in crate::app) project_editor_project_id: Option<String>,
    pub(in crate::app) project_editor_name: String,
    pub(in crate::app) project_editor_path: String,
    pub(in crate::app) project_editor_badge_symbol: Option<String>,
    pub(in crate::app) project_editor_badge_color_hex: String,
    pub(in crate::app) project_editor_saving: bool,
    pub(in crate::app) terminal_tab_editor_id: Option<usize>,
    pub(in crate::app) terminal_tab_editor_label: String,
    pub(in crate::app) worktree_creator_project_id: Option<String>,
    pub(in crate::app) worktree_creator_project_name: String,
    pub(in crate::app) worktree_creator_project_path: String,
    pub(in crate::app) worktree_creator_base_branch: String,
    pub(in crate::app) worktree_creator_name: String,
    pub(in crate::app) worktree_creator_error: Option<String>,
    pub(in crate::app) worktree_creator_submitting: bool,
    pub(in crate::app) update_dialog_phase: UpdateDialogPhase,
    pub(in crate::app) update_dialog_status: Option<codux_runtime::update::UpdateStatus>,
    pub(in crate::app) update_dialog_progress:
        Option<codux_runtime::app_info::UpdateInstallProgressEvent>,
    pub(in crate::app) update_dialog_result: Option<codux_runtime::app_info::UpdateInstallResult>,
    pub(in crate::app) update_dialog_error: Option<String>,
    pub(in crate::app) tooltip_state: CoduxTooltipState,
    pub(in crate::app) ui_performance_counts: HashMap<String, u64>,
    pub(in crate::app) ui_performance_last_report_at: f64,
}

#[derive(Clone)]
pub(in crate::app) struct TerminalLayoutCacheEntry {
    pub(in crate::app) layout: TerminalLayoutSummary,
    pub(in crate::app) runtime: TerminalRuntimeSummary,
}

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct CoduxTooltipState {
    pub(in crate::app) id: Option<ElementId>,
    pub(in crate::app) text: SharedString,
    pub(in crate::app) bounds: Bounds<Pixels>,
    pub(in crate::app) placement: CoduxTooltipPlacement,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::app) enum CoduxTooltipPlacement {
    #[default]
    Auto,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::app) struct FileEditorTab {
    pub(in crate::app) relative_path: String,
    pub(in crate::app) label: String,
    pub(in crate::app) editable: bool,
    pub(in crate::app) dirty: bool,
    pub(in crate::app) language: String,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct GitOperationCompletion {
    pub(in crate::app) success_message: String,
    pub(in crate::app) failure_prefix: String,
    pub(in crate::app) clear_commit_message: bool,
    pub(in crate::app) clear_git_diff_preview: bool,
    pub(in crate::app) clear_git_tree_state: bool,
    pub(in crate::app) clear_remote_url: bool,
    pub(in crate::app) reload_state: bool,
    pub(in crate::app) refresh_review: bool,
    pub(in crate::app) diff_file_to_reload: Option<String>,
    pub(in crate::app) clear_selected_branch: bool,
    pub(in crate::app) selected_branch: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) struct PendingTerminalClose {
    pub(in crate::app) target: TerminalCloseTarget,
    pub(in crate::app) requested_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum TerminalCloseTarget {
    Split { pane_index: usize },
    Tab { terminal_id: usize },
}

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct RuntimeActivityTickResult {
    pub(in crate::app) project_events: usize,
    pub(in crate::app) file_events: usize,
    pub(in crate::app) ai_history_events: usize,
    pub(in crate::app) pet_events: usize,
    pub(in crate::app) pet_update_events: usize,
    pub(in crate::app) ai_activity_changed: bool,
    pub(in crate::app) memory_events: usize,
    pub(in crate::app) dock_badge_count: Option<i64>,
    pub(in crate::app) changed: bool,
    #[allow(dead_code)]
    pub(in crate::app) ai_state_error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::app) enum UpdateDialogPhase {
    #[default]
    Checking,
    Available,
    Latest,
    NotConfigured,
    Downloading,
    Finished,
    Error,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct RuntimeScheduledRefresh {
    pub(in crate::app) runtime_activity: RuntimeActivitySummary,
    pub(in crate::app) remote: RemoteSummary,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct AIProviderTestResult {
    pub(in crate::app) provider_id: String,
    pub(in crate::app) message: String,
    pub(in crate::app) ok: bool,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct SSHProfileTestDisplay {
    pub(in crate::app) message: String,
    pub(in crate::app) ok: bool,
}

pub(in crate::app) struct ProjectSwitchLoad {
    pub(in crate::app) project_id: String,
    pub(in crate::app) generation: u64,
    pub(in crate::app) ai_global_history: AIGlobalHistorySummary,
    pub(in crate::app) memory: MemorySummary,
    pub(in crate::app) memory_manager: MemoryManagerSnapshot,
}

pub(in crate::app) struct ProjectSwitchTaskLoad {
    pub(in crate::app) project_id: String,
    pub(in crate::app) generation: u64,
    pub(in crate::app) worktrees: WorktreeSummary,
}

pub(in crate::app) struct ProjectSwitchTerminalLoad {
    pub(in crate::app) project_id: String,
    pub(in crate::app) generation: u64,
    pub(in crate::app) scope_key: WorktreeScopeKey,
    pub(in crate::app) terminal_layout: TerminalLayoutSummary,
    pub(in crate::app) terminal_runtime: TerminalRuntimeSummary,
}

pub(in crate::app) struct ProjectSwitchPrimaryLoad {
    pub(in crate::app) project_id: String,
    pub(in crate::app) generation: u64,
    pub(in crate::app) scope_key: WorktreeScopeKey,
    pub(in crate::app) ai_history: AIHistorySummary,
}

pub(in crate::app) struct WorktreeSwitchLoad {
    pub(in crate::app) project_id: String,
    pub(in crate::app) generation: u64,
    pub(in crate::app) scope_key: WorktreeScopeKey,
    pub(in crate::app) ai_history: AIHistorySummary,
    pub(in crate::app) terminal_layout: TerminalLayoutSummary,
    pub(in crate::app) terminal_runtime: TerminalRuntimeSummary,
}

pub(in crate::app) struct WorktreeSidebarLoad {
    pub(in crate::app) generation: u64,
    pub(in crate::app) scope_key: WorktreeScopeKey,
    pub(in crate::app) files: Vec<FileEntry>,
    pub(in crate::app) file_tree_children: HashMap<String, Vec<FileEntry>>,
    pub(in crate::app) file_editor_layout: FileEditorLayoutSummary,
    pub(in crate::app) git: GitSummary,
    pub(in crate::app) git_review: GitReviewSummary,
}

pub(in crate::app) struct WorktreeFilePanelLoad {
    pub(in crate::app) generation: u64,
    pub(in crate::app) scope_key: WorktreeScopeKey,
    pub(in crate::app) files: Vec<FileEntry>,
    pub(in crate::app) file_tree_children: HashMap<String, Vec<FileEntry>>,
}

#[derive(Clone)]
pub(in crate::app) struct FilePanelState {
    pub(in crate::app) files: Vec<FileEntry>,
    pub(in crate::app) file_directory: String,
    pub(in crate::app) selected_file_entry: Option<String>,
    pub(in crate::app) selected_file_entries: HashSet<String>,
    pub(in crate::app) file_selection_anchor: Option<String>,
    pub(in crate::app) file_tree_expanded_dirs: HashSet<String>,
    pub(in crate::app) file_tree_children: HashMap<String, Vec<FileEntry>>,
    pub(in crate::app) file_editor_tabs: Vec<FileEditorTab>,
    pub(in crate::app) active_file_editor_tab: Option<String>,
}

#[derive(Clone)]
pub(in crate::app) struct GitPanelState {
    pub(in crate::app) git: GitSummary,
    pub(in crate::app) git_review: GitReviewSummary,
    pub(in crate::app) selected_git_file: Option<String>,
    pub(in crate::app) selected_git_files: HashSet<String>,
    pub(in crate::app) selected_git_branch: Option<String>,
    pub(in crate::app) git_expanded_sections: HashSet<String>,
    pub(in crate::app) git_expanded_dirs: HashSet<String>,
    pub(in crate::app) git_tree_children: HashMap<String, Vec<GitFileStatus>>,
    pub(in crate::app) git_diff_preview: String,
    pub(in crate::app) git_review_content: Option<GitReviewContentSummary>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(in crate::app) struct WorktreeScopeKey {
    pub(in crate::app) project_id: String,
    pub(in crate::app) worktree_id: String,
}

pub(in crate::app) fn file_editor_tabs_from_layout(
    layout: FileEditorLayoutSummary,
) -> (Vec<FileEditorTab>, Option<String>) {
    let tabs = layout
        .tabs
        .into_iter()
        .map(|tab| FileEditorTab {
            label: tab.label,
            relative_path: tab.path,
            editable: true,
            dirty: false,
            language: if tab.language.trim().is_empty() {
                "text".to_string()
            } else {
                tab.language
            },
        })
        .collect::<Vec<_>>();
    let active_path = layout
        .active_path
        .filter(|active| tabs.iter().any(|tab| tab.relative_path == *active))
        .or_else(|| tabs.first().map(|tab| tab.relative_path.clone()));
    (tabs, active_path)
}

pub(in crate::app) fn current_worktree_scope_key(state: &RuntimeState) -> Option<WorktreeScopeKey> {
    let project_id = state.selected_project.as_ref()?.id.clone();
    let worktree_id = super::ai_runtime_status::terminal_layout_owner_id(state)?;
    Some(WorktreeScopeKey {
        project_id,
        worktree_id,
    })
}

pub(in crate::app) fn worktree_terminal_storage_key(key: &WorktreeScopeKey) -> String {
    super::ai_runtime_status::terminal_layout_storage_key(&key.project_id, &key.worktree_id)
}

pub(in crate::app) fn worktree_summary_has_rows(summary: &WorktreeSummary) -> bool {
    summary.available && !summary.worktrees.is_empty()
}

pub(in crate::app) fn worktree_summary_has_git_counts(summary: &WorktreeSummary) -> bool {
    summary.worktrees.iter().any(|worktree| {
        let git = &worktree.git_summary;
        git.changes > 0
            || git.incoming != 0
            || git.outgoing != 0
            || git.additions != 0
            || git.deletions != 0
    })
}

impl Default for GitOperationCompletion {
    fn default() -> Self {
        Self {
            success_message: String::new(),
            failure_prefix: "Git operation failed".to_string(),
            clear_commit_message: false,
            clear_git_diff_preview: false,
            clear_git_tree_state: false,
            clear_remote_url: false,
            reload_state: false,
            refresh_review: false,
            diff_file_to_reload: None,
            clear_selected_branch: false,
            selected_branch: None,
        }
    }
}

pub(in crate::app) fn app_now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

pub(in crate::app) fn app_git_review(state: &RuntimeState) -> GitReviewSummary {
    let mut review = state.git_review.clone();
    super::git_actions::merge_git_review_status_files(&mut review, &state.git);
    review
}

pub(in crate::app) fn git_status_tree_key(section_id: &str, path: &str) -> String {
    format!("{section_id}:{}", path.trim_matches('/'))
}

pub(in crate::app) const PET_DEX_FRAME_INTERVAL: Duration = Duration::from_millis(280);
pub(in crate::app) const PET_CUSTOM_INSTALL_WINDOW_WIDTH: f32 = 680.0;
pub(in crate::app) const PET_CUSTOM_INSTALL_INPUT_HEIGHT: f32 = 230.0;
pub(in crate::app) const PET_CUSTOM_INSTALL_READY_HEIGHT: f32 = 530.0;
pub(in crate::app) const PET_CUSTOM_INSTALL_ERROR_HEIGHT: f32 = 280.0;
pub(in crate::app) const GIT_CREDENTIALS_WINDOW_WIDTH: f32 = 440.0;
pub(in crate::app) const GIT_CREDENTIALS_COMPACT_HEIGHT: f32 = 310.0;
pub(in crate::app) const GIT_CREDENTIALS_EXPANDED_HEIGHT: f32 = 350.0;

pub(in crate::app) fn resize_pet_custom_install_window(window: &mut Window, height: f32) {
    window.resize(size(
        px(PET_CUSTOM_INSTALL_WINDOW_WIDTH),
        px(height.clamp(
            PET_CUSTOM_INSTALL_INPUT_HEIGHT,
            PET_CUSTOM_INSTALL_READY_HEIGHT,
        )),
    ));
}

pub(in crate::app) fn resize_pet_custom_install_window_handle(
    handle: AnyWindowHandle,
    height: f32,
    cx: &mut Context<CoduxApp>,
) {
    let _ = handle.update(cx, |_view, window, _cx| {
        resize_pet_custom_install_window(window, height);
    });
}

pub(in crate::app) fn resize_git_credentials_window(window: &mut Window, expanded: bool) {
    let height = if expanded {
        GIT_CREDENTIALS_EXPANDED_HEIGHT
    } else {
        GIT_CREDENTIALS_COMPACT_HEIGHT
    };
    window.resize(size(
        px(GIT_CREDENTIALS_WINDOW_WIDTH),
        px(height.clamp(
            GIT_CREDENTIALS_COMPACT_HEIGHT,
            GIT_CREDENTIALS_EXPANDED_HEIGHT,
        )),
    ));
}

impl Drop for CoduxApp {
    fn drop(&mut self) {
        if self.window_mode == AppWindowMode::Main {
            self.shutdown_runtime_state_from_drop();
        }
    }
}
