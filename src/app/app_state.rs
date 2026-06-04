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
    pub(in crate::app) next_terminal_index: usize,
    pub(in crate::app) runtime: RuntimeInventory,
    pub(in crate::app) state: RuntimeState,
    pub(in crate::app) runtime_service: RuntimeService,
    pub(in crate::app) is_exiting: bool,
    pub(in crate::app) main_window_close_handler_registered: bool,
    pub(in crate::app) status_message: String,
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
    pub(in crate::app) worktree_creator_window: Option<AnyWindowHandle>,
    pub(in crate::app) parent_main_window: Option<gpui::WeakEntity<CoduxApp>>,
    pub(in crate::app) desktop_pet_line: String,
    pub(in crate::app) desktop_pet_tone: DesktopPetActivityTone,
    pub(in crate::app) desktop_pet_active_llm_key: String,
    pub(in crate::app) desktop_pet_requested_llm_key: String,
    pub(in crate::app) desktop_pet_last_llm_requested_at: f64,
    pub(in crate::app) pet_sprite_frame: usize,
    pub(in crate::app) pet_sprite_animation_active: bool,
    pub(in crate::app) file_preview: String,
    pub(in crate::app) file_editable: bool,
    pub(in crate::app) file_dirty: bool,
    pub(in crate::app) file_editor_tabs: Vec<FileEditorTab>,
    pub(in crate::app) active_file_editor_tab: Option<String>,
    pub(in crate::app) file_editor_states: HashMap<String, gpui::Entity<InputState>>,
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
    pub(in crate::app) file_preview_scroll_handle: UniformListScrollHandle,
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
    pub(in crate::app) ssh_seen_revision: u64,
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
    pub(in crate::app) pending_runtime_refresh: Option<RuntimeScheduledRefresh>,
    pub(in crate::app) ai_runtime_state_save_tick: u64,
    pub(in crate::app) dismissed_worktree_ai_completion_at: HashMap<String, f64>,
    pub(in crate::app) ai_index_progress_visible_until: f64,
    pub(in crate::app) ai_index_progress_generation: u64,
    pub(in crate::app) ai_history_active_index_count: usize,
    pub(in crate::app) ai_history_refresh_keys: HashSet<String>,
    pub(in crate::app) project_switch_generation: u64,
    pub(in crate::app) scheduled_work_in_flight: HashSet<String>,
    pub(in crate::app) scheduled_work_last_started_at: HashMap<String, f64>,
    pub(in crate::app) scheduled_work_last_finished_at: HashMap<String, f64>,
    pub(in crate::app) task_column_refreshing: bool,
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
    pub(in crate::app) selected_runtime_terminal_id: Option<String>,
    pub(in crate::app) selected_ssh_profile_id: Option<String>,
    pub(in crate::app) ssh_draft_open: bool,
    pub(in crate::app) ssh_testing: bool,
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
    pub(in crate::app) remote_pairing_poll_generation: u64,
    pub(in crate::app) recording_shortcut_id: Option<String>,
    pub(in crate::app) agent_split_enabled: bool,
    pub(in crate::app) workspace_view: WorkspaceView,
    pub(in crate::app) assistant_panel: Option<AssistantPanel>,
    pub(in crate::app) project_column_collapsed: bool,
    pub(in crate::app) task_column_collapsed: bool,
    pub(in crate::app) project_list_store: Option<gpui::Entity<ProjectListStore>>,
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
    pub(in crate::app) project_view_store: HashMap<String, ProjectViewState>,
    pub(in crate::app) worktree_view_store: HashMap<WorktreeViewStoreKey, WorktreeViewState>,
    pub(in crate::app) project_open_applications: Vec<ProjectOpenApplicationSummary>,
    pub(in crate::app) project_editor_project_id: Option<String>,
    pub(in crate::app) project_editor_name: String,
    pub(in crate::app) project_editor_path: String,
    pub(in crate::app) project_editor_badge_symbol: Option<String>,
    pub(in crate::app) project_editor_badge_color_hex: String,
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

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct RuntimeActivityTickResult {
    pub(in crate::app) project_events: usize,
    pub(in crate::app) file_events: usize,
    pub(in crate::app) ai_history_events: usize,
    pub(in crate::app) pet_events: usize,
    pub(in crate::app) pet_update_events: usize,
    pub(in crate::app) ai_events: usize,
    pub(in crate::app) ai_activity_changed: bool,
    pub(in crate::app) memory_events: usize,
    pub(in crate::app) dock_badge_count: Option<i64>,
    pub(in crate::app) changed: bool,
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
    pub(in crate::app) store_key: WorktreeViewStoreKey,
    pub(in crate::app) terminal_layout: TerminalLayoutSummary,
    pub(in crate::app) terminal_runtime: TerminalRuntimeSummary,
}

pub(in crate::app) struct ProjectSwitchPrimaryLoad {
    pub(in crate::app) project_id: String,
    pub(in crate::app) generation: u64,
    pub(in crate::app) store_key: WorktreeViewStoreKey,
    pub(in crate::app) ai_history: AIHistorySummary,
}

pub(in crate::app) struct WorktreeSwitchLoad {
    pub(in crate::app) project_id: String,
    pub(in crate::app) generation: u64,
    pub(in crate::app) store_key: WorktreeViewStoreKey,
    pub(in crate::app) worktrees: WorktreeSummary,
    pub(in crate::app) ai_history: AIHistorySummary,
    pub(in crate::app) terminal_layout: TerminalLayoutSummary,
    pub(in crate::app) terminal_runtime: TerminalRuntimeSummary,
}

pub(in crate::app) struct WorktreeSidebarLoad {
    pub(in crate::app) generation: u64,
    pub(in crate::app) store_key: WorktreeViewStoreKey,
    pub(in crate::app) files: Vec<FileEntry>,
    pub(in crate::app) file_editor_layout: FileEditorLayoutSummary,
    pub(in crate::app) git: GitSummary,
    pub(in crate::app) git_review: GitReviewSummary,
}

pub(in crate::app) struct WorktreeFilePanelLoad {
    pub(in crate::app) generation: u64,
    pub(in crate::app) store_key: WorktreeViewStoreKey,
    pub(in crate::app) files: Vec<FileEntry>,
    pub(in crate::app) file_tree_children: HashMap<String, Vec<FileEntry>>,
}

#[derive(Clone)]
pub(in crate::app) struct ProjectViewState {
    pub(in crate::app) ai_history: AIHistorySummary,
    pub(in crate::app) ai_global_history: AIGlobalHistorySummary,
    pub(in crate::app) memory: MemorySummary,
    pub(in crate::app) memory_manager: MemoryManagerSnapshot,
    pub(in crate::app) worktrees: WorktreeSummary,
}

#[derive(Clone)]
pub(in crate::app) struct WorktreeViewState {
    pub(in crate::app) ai_history: AIHistorySummary,
    pub(in crate::app) files: FileWorktreeViewState,
    pub(in crate::app) git: GitWorktreeViewState,
    pub(in crate::app) terminal: TerminalViewState,
}

#[derive(Clone)]
pub(in crate::app) struct FileWorktreeViewState {
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
pub(in crate::app) struct GitWorktreeViewState {
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

#[derive(Clone)]
pub(in crate::app) struct TerminalViewState {
    pub(in crate::app) terminal_layout: TerminalLayoutSummary,
    pub(in crate::app) terminal_runtime: TerminalRuntimeSummary,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(in crate::app) struct WorktreeViewStoreKey {
    pub(in crate::app) project_id: String,
    pub(in crate::app) worktree_id: String,
}

pub(in crate::app) fn empty_project_view_state() -> ProjectViewState {
    ProjectViewState {
        ai_history: AIHistorySummary::default(),
        ai_global_history: AIGlobalHistorySummary::default(),
        memory: MemorySummary::default(),
        memory_manager: MemoryManagerSnapshot::default(),
        worktrees: WorktreeSummary::default(),
    }
}

pub(in crate::app) fn empty_worktree_view_state() -> WorktreeViewState {
    WorktreeViewState {
        ai_history: AIHistorySummary::default(),
        files: empty_file_worktree_view_state(),
        git: empty_git_worktree_view_state(),
        terminal: empty_terminal_view_state(),
    }
}

pub(in crate::app) fn empty_file_worktree_view_state() -> FileWorktreeViewState {
    FileWorktreeViewState {
        files: Vec::new(),
        file_directory: String::new(),
        selected_file_entry: None,
        selected_file_entries: HashSet::new(),
        file_selection_anchor: None,
        file_tree_expanded_dirs: HashSet::new(),
        file_tree_children: HashMap::new(),
        file_editor_tabs: Vec::new(),
        active_file_editor_tab: None,
    }
}

pub(in crate::app) fn empty_git_worktree_view_state() -> GitWorktreeViewState {
    GitWorktreeViewState {
        git: GitSummary::default(),
        git_review: GitReviewSummary::default(),
        selected_git_file: None,
        selected_git_files: HashSet::new(),
        selected_git_branch: None,
        git_expanded_sections: HashSet::new(),
        git_expanded_dirs: HashSet::new(),
        git_tree_children: HashMap::new(),
        git_diff_preview: "select a changed file to preview its diff".to_string(),
        git_review_content: None,
    }
}

pub(in crate::app) fn empty_terminal_view_state() -> TerminalViewState {
    TerminalViewState {
        terminal_layout: TerminalLayoutSummary::default(),
        terminal_runtime: TerminalRuntimeSummary::default(),
    }
}

pub(in crate::app) fn initial_project_view_store(
    state: &RuntimeState,
) -> HashMap<String, ProjectViewState> {
    let worktree_service = WorktreeService::new(state.support_dir.clone());
    let persisted_worktrees = worktree_service.state_summaries(
        state
            .projects
            .iter()
            .map(|project| (project.id.as_str(), project.path.as_str())),
    );
    let selected_project_id = state
        .selected_project
        .as_ref()
        .map(|project| project.id.as_str());
    state
        .projects
        .iter()
        .map(|project| {
            let worktrees = if Some(project.id.as_str()) == selected_project_id {
                state.worktrees.clone()
            } else {
                persisted_worktrees
                    .get(&project.id)
                    .cloned()
                    .unwrap_or_default()
            };
            (
                project.id.clone(),
                ProjectViewState {
                    ai_history: if Some(project.id.as_str()) == selected_project_id {
                        state.ai_history.clone()
                    } else {
                        AIHistorySummary::default()
                    },
                    ai_global_history: state.ai_global_history.clone(),
                    memory: if Some(project.id.as_str()) == selected_project_id {
                        state.memory.clone()
                    } else {
                        MemorySummary::default()
                    },
                    memory_manager: if Some(project.id.as_str()) == selected_project_id {
                        state.memory_manager.clone()
                    } else {
                        MemoryManagerSnapshot::default()
                    },
                    worktrees,
                },
            )
        })
        .collect()
}

pub(in crate::app) fn initial_worktree_view_store(
    state: &RuntimeState,
    project_view_store: &HashMap<String, ProjectViewState>,
) -> HashMap<WorktreeViewStoreKey, WorktreeViewState> {
    let file_editor_layout_service =
        codux_runtime::file_editor_layout::FileEditorLayoutService::new(state.support_dir.clone());
    let file_tree_state_service =
        codux_runtime::file_tree_state::FileTreeStateService::new(state.support_dir.clone());
    let git_ui_state_service =
        codux_runtime::git_ui_state::GitUiStateService::new(state.support_dir.clone());
    let worktree_keys = project_view_store
        .iter()
        .flat_map(|(project_id, project_state)| {
            project_state
                .worktrees
                .worktrees
                .iter()
                .map(move |worktree| WorktreeViewStoreKey {
                    project_id: project_id.clone(),
                    worktree_id: worktree.id.clone(),
                })
        })
        .collect::<Vec<_>>();
    let file_editor_layouts = file_editor_layout_service
        .load_many(worktree_keys.iter().map(|key| key.worktree_id.as_str()));
    let file_tree_states =
        file_tree_state_service.load_many(worktree_keys.iter().map(|key| key.worktree_id.as_str()));
    let git_ui_states =
        git_ui_state_service.load_many(worktree_keys.iter().map(|key| key.worktree_id.as_str()));
    let terminal_layout_service =
        codux_runtime::terminal_layout::TerminalLayoutService::new(state.support_dir.clone());
    let terminal_layout_keys = worktree_keys
        .iter()
        .map(worktree_terminal_storage_key)
        .collect::<Vec<_>>();
    let terminal_layouts =
        terminal_layout_service.load_many(terminal_layout_keys.iter().map(|key| key.as_str()));
    let current_key = worktree_view_store_key(state);

    worktree_keys
        .into_iter()
        .map(|key| {
            let is_current = current_key.as_ref() == Some(&key);
            let terminal_storage_key = worktree_terminal_storage_key(&key);
            let file_editor_layout = file_editor_layouts
                .get(&key.worktree_id)
                .cloned()
                .unwrap_or_default();
            let (file_editor_tabs, active_file_editor_tab) =
                file_editor_tabs_from_layout(file_editor_layout);
            let file_tree_state = file_tree_states
                .get(&key.worktree_id)
                .cloned()
                .unwrap_or_default();
            let git_ui_state = git_ui_states
                .get(&key.worktree_id)
                .cloned()
                .unwrap_or_default();
            (
                key,
                WorktreeViewState {
                    ai_history: if is_current {
                        state.ai_history.clone()
                    } else {
                        AIHistorySummary::default()
                    },
                    files: FileWorktreeViewState {
                        files: if is_current {
                            state.files.clone()
                        } else {
                            file_tree_state.files
                        },
                        file_directory: file_tree_state.file_directory,
                        selected_file_entry: file_tree_state.selected_file_entry,
                        selected_file_entries: file_tree_state
                            .selected_file_entries
                            .into_iter()
                            .collect(),
                        file_selection_anchor: file_tree_state.file_selection_anchor,
                        file_tree_expanded_dirs: file_tree_state
                            .file_tree_expanded_dirs
                            .into_iter()
                            .collect(),
                        file_tree_children: file_tree_state.file_tree_children,
                        file_editor_tabs,
                        active_file_editor_tab,
                    },
                    git: git_worktree_view_state_from_summary(git_ui_state, state, is_current),
                    terminal: TerminalViewState {
                        terminal_layout: if is_current {
                            state.terminal_layout.clone()
                        } else {
                            terminal_layouts
                                .get(&terminal_storage_key)
                                .cloned()
                                .unwrap_or_default()
                        },
                        terminal_runtime: if is_current {
                            state.terminal_runtime.clone()
                        } else {
                            TerminalRuntimeSummary::default()
                        },
                    },
                },
            )
        })
        .collect()
}

pub(in crate::app) fn git_ui_state_summary_from_worktree(
    state: &GitWorktreeViewState,
) -> codux_runtime::git_ui_state::GitUiStateSummary {
    codux_runtime::git_ui_state::GitUiStateSummary {
        git: state.git.clone(),
        git_review: state.git_review.clone(),
        selected_git_file: state.selected_git_file.clone(),
        selected_git_files: state.selected_git_files.iter().cloned().collect(),
        selected_git_branch: state.selected_git_branch.clone(),
        git_expanded_sections: state.git_expanded_sections.iter().cloned().collect(),
        git_expanded_dirs: state.git_expanded_dirs.iter().cloned().collect(),
        git_tree_children: state.git_tree_children.clone(),
        git_diff_preview: state.git_diff_preview.clone(),
        git_review_content: state.git_review_content.clone(),
        error: None,
    }
}

fn git_worktree_view_state_from_summary(
    summary: codux_runtime::git_ui_state::GitUiStateSummary,
    state: &RuntimeState,
    is_current: bool,
) -> GitWorktreeViewState {
    let git = if is_current {
        state.git.clone()
    } else {
        summary.git
    };
    let git_review = if is_current {
        state.git_review.clone()
    } else {
        summary.git_review
    };
    let selected_git_branch = summary.selected_git_branch.or_else(|| {
        is_current.then(|| {
            git.branches
                .iter()
                .find(|branch| branch.is_current)
                .or_else(|| git.branches.first())
                .map(|branch| branch.name.clone())
        })?
    });
    let mut git_expanded_sections = summary
        .git_expanded_sections
        .into_iter()
        .collect::<HashSet<_>>();
    if git_expanded_sections.is_empty() {
        git_expanded_sections = HashSet::from(["changed".to_string(), "untracked".to_string()]);
    }
    let git_diff_preview = if summary.git_diff_preview.trim().is_empty() {
        "select a changed file to preview its diff".to_string()
    } else {
        summary.git_diff_preview
    };
    GitWorktreeViewState {
        git,
        git_review,
        selected_git_file: summary.selected_git_file,
        selected_git_files: summary.selected_git_files.into_iter().collect(),
        selected_git_branch,
        git_expanded_sections,
        git_expanded_dirs: summary.git_expanded_dirs.into_iter().collect(),
        git_tree_children: summary.git_tree_children,
        git_diff_preview,
        git_review_content: summary.git_review_content,
    }
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

pub(in crate::app) fn worktree_view_store_key(
    state: &RuntimeState,
) -> Option<WorktreeViewStoreKey> {
    let project_id = state.selected_project.as_ref()?.id.clone();
    let worktree_id = super::ai_runtime_status::terminal_layout_owner_id(state)?;
    Some(WorktreeViewStoreKey {
        project_id,
        worktree_id,
    })
}

pub(in crate::app) fn worktree_terminal_storage_key(key: &WorktreeViewStoreKey) -> String {
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
