use super::*;

static ACTIVE_SETTINGS_SNAPSHOT: OnceLock<SettingsSummary> = OnceLock::new();

pub(crate) fn set_active_settings_snapshot(settings: SettingsSummary) {
    let _ = ACTIVE_SETTINGS_SNAPSHOT.set(settings);
}

pub(in crate::app) fn active_settings_snapshot() -> Option<SettingsSummary> {
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
    pub(in crate::app) terminal_manager: Arc<TerminalManager>,
    pub(in crate::app) terminal_layout_loading: bool,
    pub(in crate::app) active_terminal_id: usize,
    pub(in crate::app) next_terminal_index: usize,
    pub(in crate::app) runtime: RuntimeInventory,
    pub(in crate::app) runtime_ingress: RuntimeIngressStatus,
    pub(in crate::app) state: RuntimeState,
    pub(in crate::app) runtime_service: RuntimeService,
    pub(in crate::app) is_exiting: bool,
    pub(in crate::app) status_message: String,
    pub(in crate::app) desktop_pet_window: Option<AnyWindowHandle>,
    pub(in crate::app) settings_window: Option<AnyWindowHandle>,
    pub(in crate::app) about_window: Option<AnyWindowHandle>,
    pub(in crate::app) memory_manager_window: Option<AnyWindowHandle>,
    pub(in crate::app) pet_claim_window: Option<AnyWindowHandle>,
    pub(in crate::app) pet_custom_install_window: Option<AnyWindowHandle>,
    pub(in crate::app) pet_dex_window: Option<AnyWindowHandle>,
    pub(in crate::app) ssh_profile_editor_window: Option<AnyWindowHandle>,
    pub(in crate::app) project_editor_window: Option<AnyWindowHandle>,
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
    pub(in crate::app) selected_git_file: Option<String>,
    pub(in crate::app) selected_git_branch: Option<String>,
    pub(in crate::app) git_review: GitReviewSummary,
    pub(in crate::app) git_expanded_sections: HashSet<String>,
    pub(in crate::app) git_expanded_dirs: HashSet<String>,
    pub(in crate::app) git_tree_children: HashMap<String, Vec<GitFileStatus>>,
    pub(in crate::app) git_files_scroll_handle: VirtualListScrollHandle,
    pub(in crate::app) selected_git_files: HashSet<String>,
    pub(in crate::app) git_diff_preview: String,
    pub(in crate::app) git_diff_window_path: Option<String>,
    pub(in crate::app) git_diff_window_content: String,
    pub(in crate::app) git_diff_window_error: Option<String>,
    pub(in crate::app) git_review_content: Option<GitReviewContentSummary>,
    pub(in crate::app) git_clone_remote_url: String,
    pub(in crate::app) git_remote_editor_open: bool,
    pub(in crate::app) git_remote_name: String,
    pub(in crate::app) git_remote_url: String,
    pub(in crate::app) git_running_operation: Option<GitRunningOperation>,
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
    pub(in crate::app) pet_sprite_paths: HashMap<String, PathBuf>,
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
    pub(in crate::app) pet_claim_species: String,
    pub(in crate::app) pet_name_editing: bool,
    pub(in crate::app) pet_dex_spotlight: Option<PetDexSpotlight>,
    pub(in crate::app) selected_ai_session_id: Option<String>,
    pub(in crate::app) ai_session_delete_confirm_id: Option<String>,
    pub(in crate::app) selected_ai_provider_id: Option<String>,
    pub(in crate::app) ai_provider_testing_id: Option<String>,
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
    pub(in crate::app) ai_history_refresh_project_ids: HashSet<String>,
    pub(in crate::app) project_switch_generation: u64,
    pub(in crate::app) project_task_load_in_flight: HashSet<String>,
    pub(in crate::app) project_task_load_last_started_at: HashMap<String, f64>,
    pub(in crate::app) project_task_load_last_finished_at: HashMap<String, f64>,
    pub(in crate::app) worktree_sidebar_load_in_flight: HashSet<WorktreeViewStoreKey>,
    pub(in crate::app) worktree_sidebar_load_last_started_at: HashMap<WorktreeViewStoreKey, f64>,
    pub(in crate::app) worktree_sidebar_load_last_finished_at: HashMap<WorktreeViewStoreKey, f64>,
    pub(in crate::app) memory_progress_visible_until: f64,
    pub(in crate::app) memory_progress_generation: u64,
    pub(in crate::app) performance_refresh_in_flight: bool,
    pub(in crate::app) pending_performance_refresh: Option<PerformanceSummary>,
    pub(in crate::app) today_level_day_start: f64,
    pub(in crate::app) active_settings_pane: SettingsPane,
    pub(in crate::app) memory_manager_tab: MemoryManagerTab,
    pub(in crate::app) memory_manager_scope: String,
    pub(in crate::app) memory_manager_project_id: Option<String>,
    pub(in crate::app) memory_processing: bool,
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
    pub(in crate::app) workspace_column_view: Option<gpui::Entity<WorkspaceColumnView>>,
    pub(in crate::app) workspace_toolbar_view:
        Option<gpui::Entity<workspace_views::WorkspaceToolbarView>>,
    pub(in crate::app) workspace_body_view:
        Option<gpui::Entity<workspace_views::WorkspaceBodyView>>,
    pub(in crate::app) workspace_assistant_view:
        Option<gpui::Entity<workspace_views::WorkspaceAssistantView>>,
    pub(in crate::app) status_bar_view: Option<gpui::Entity<StatusBarView>>,
    pub(in crate::app) file_sidebar_view: Option<gpui::Entity<FileSidebarView>>,
    pub(in crate::app) project_view_store: HashMap<String, ProjectViewState>,
    pub(in crate::app) worktree_view_store: HashMap<WorktreeViewStoreKey, WorktreeViewState>,
    pub(in crate::app) terminal_view_store: HashMap<TerminalViewStoreKey, TerminalViewState>,
    pub(in crate::app) project_open_applications: Vec<ProjectOpenApplicationSummary>,
    pub(in crate::app) project_editor_project_id: Option<String>,
    pub(in crate::app) project_editor_name: String,
    pub(in crate::app) project_editor_path: String,
    pub(in crate::app) project_editor_badge_symbol: Option<String>,
    pub(in crate::app) project_editor_badge_color_hex: String,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct GitOperationCompletion {
    pub(in crate::app) success_message: String,
    pub(in crate::app) failure_prefix: String,
    pub(in crate::app) clear_commit_message: bool,
    pub(in crate::app) clear_git_diff_preview: bool,
    pub(in crate::app) clear_git_tree_cache: bool,
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
    pub(in crate::app) memory_events: usize,
    pub(in crate::app) dock_badge_count: Option<i64>,
    pub(in crate::app) changed: bool,
    pub(in crate::app) ai_state_error: Option<String>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct RuntimeScheduledRefresh {
    pub(in crate::app) runtime_activity: RuntimeActivitySummary,
    pub(in crate::app) remote: RemoteSummary,
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
    pub(in crate::app) store_key: TerminalViewStoreKey,
    pub(in crate::app) terminal_layout: TerminalLayoutSummary,
    pub(in crate::app) terminal_runtime: TerminalRuntimeSummary,
}

pub(in crate::app) struct ProjectSwitchPrimaryLoad {
    pub(in crate::app) project_id: String,
    pub(in crate::app) generation: u64,
    pub(in crate::app) ai_history: AIHistorySummary,
    pub(in crate::app) ai_session_detail: Option<AISessionDetail>,
}

pub(in crate::app) struct WorktreeSwitchTerminalLoad {
    pub(in crate::app) project_id: String,
    pub(in crate::app) generation: u64,
    pub(in crate::app) store_key: TerminalViewStoreKey,
    pub(in crate::app) terminal_layout: TerminalLayoutSummary,
    pub(in crate::app) terminal_runtime: TerminalRuntimeSummary,
}

pub(in crate::app) struct WorktreeSidebarLoad {
    pub(in crate::app) generation: u64,
    pub(in crate::app) store_key: WorktreeViewStoreKey,
    pub(in crate::app) files: Vec<FileEntry>,
    pub(in crate::app) git: GitSummary,
    pub(in crate::app) git_review: GitReviewSummary,
}

#[derive(Clone)]
pub(in crate::app) struct ProjectViewState {
    pub(in crate::app) ai_history: AIHistorySummary,
    pub(in crate::app) ai_global_history: AIGlobalHistorySummary,
    pub(in crate::app) ai_session_detail: Option<AISessionDetail>,
    pub(in crate::app) memory: MemorySummary,
    pub(in crate::app) memory_manager: MemoryManagerSnapshot,
    pub(in crate::app) worktrees: WorktreeSummary,
}

#[derive(Clone)]
pub(in crate::app) struct WorktreeViewState {
    pub(in crate::app) files: FileWorktreeViewState,
    pub(in crate::app) git: GitWorktreeViewState,
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
pub(in crate::app) struct TerminalViewStoreKey {
    pub(in crate::app) project_id: String,
    pub(in crate::app) task_id: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(in crate::app) struct WorktreeViewStoreKey {
    pub(in crate::app) project_id: String,
    pub(in crate::app) worktree_id: String,
}

pub(in crate::app) fn initial_project_view_store(
    state: &RuntimeState,
) -> HashMap<String, ProjectViewState> {
    state
        .selected_project
        .as_ref()
        .map(|project| {
            HashMap::from([(
                project.id.clone(),
                ProjectViewState {
                    ai_history: state.ai_history.clone(),
                    ai_global_history: state.ai_global_history.clone(),
                    ai_session_detail: state.ai_session_detail.clone(),
                    memory: state.memory.clone(),
                    memory_manager: state.memory_manager.clone(),
                    worktrees: state.worktrees.clone(),
                },
            )])
        })
        .unwrap_or_default()
}

pub(in crate::app) fn initial_terminal_view_store(
    state: &RuntimeState,
) -> HashMap<TerminalViewStoreKey, TerminalViewState> {
    terminal_view_store_key(state)
        .map(|key| {
            HashMap::from([(
                key,
                TerminalViewState {
                    terminal_layout: state.terminal_layout.clone(),
                    terminal_runtime: state.terminal_runtime.clone(),
                },
            )])
        })
        .unwrap_or_default()
}

pub(in crate::app) fn initial_worktree_view_store(
    state: &RuntimeState,
) -> HashMap<WorktreeViewStoreKey, WorktreeViewState> {
    worktree_view_store_key(state)
        .map(|key| {
            HashMap::from([(
                key,
                WorktreeViewState {
                    files: FileWorktreeViewState {
                        files: state.files.clone(),
                        file_directory: String::new(),
                        selected_file_entry: None,
                        selected_file_entries: HashSet::new(),
                        file_selection_anchor: None,
                        file_tree_expanded_dirs: HashSet::new(),
                        file_tree_children: HashMap::new(),
                    },
                    git: GitWorktreeViewState {
                        git: state.git.clone(),
                        git_review: state.git_review.clone(),
                        selected_git_file: None,
                        selected_git_files: HashSet::new(),
                        selected_git_branch: state
                            .git
                            .branches
                            .iter()
                            .find(|branch| branch.is_current)
                            .or_else(|| state.git.branches.first())
                            .map(|branch| branch.name.clone()),
                        git_expanded_sections: HashSet::from([
                            "changed".to_string(),
                            "untracked".to_string(),
                        ]),
                        git_expanded_dirs: HashSet::new(),
                        git_tree_children: HashMap::new(),
                        git_diff_preview: "select a changed file to preview its diff".to_string(),
                        git_review_content: None,
                    },
                },
            )])
        })
        .unwrap_or_default()
}

pub(in crate::app) fn terminal_view_store_key(
    state: &RuntimeState,
) -> Option<TerminalViewStoreKey> {
    let project_id = state.selected_project.as_ref()?.id.clone();
    let task_id = super::ai_runtime_status::terminal_layout_owner_id(state)?;
    Some(TerminalViewStoreKey {
        project_id,
        task_id,
    })
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

pub(in crate::app) fn prewarm_terminal_restore(state: &RuntimeState, runtime: &RuntimeInventory) {
    prepare_memory_launch_artifacts(state);
    let (terminal_layout, terminal_runtime) = normalize_terminal_restore_state(
        super::ai_runtime_status::terminal_layout_owner_id(state).as_deref(),
        state.terminal_layout.clone(),
        state.terminal_runtime.clone(),
    );
    let restore_plan = terminal_restore_plan_for_language(
        &terminal_layout,
        &terminal_runtime,
        &state.settings.language,
    );
    let base_context = terminal_launch_context(state, runtime, &state.tool_permissions);

    for (tab_index, tab) in restore_plan.tabs.iter().enumerate() {
        let mount_tab = tab_index == restore_plan.active_index
            || (tab.source_id.is_none() && tab.terminal_id.is_none());
        if !mount_tab {
            continue;
        }
        let tab_id = tab_index + 1;
        for (pane_index, pane) in tab.panes.iter().enumerate() {
            let pane_context =
                terminal_pane_launch_context(base_context.as_ref(), tab_id, pane_index, pane);
            let config = pane_context
                .as_ref()
                .map(TerminalLaunchContext::to_config)
                .unwrap_or_else(TerminalPtyConfig::default);
            let shell = config.shell.clone().unwrap_or_else(default_shell);
            let cwd = config.cwd.clone().or_else(|| {
                pane_context
                    .as_ref()
                    .map(|context| context.project_path.display().to_string())
            });
            let session_id = config
                .terminal_id
                .as_deref()
                .or_else(|| {
                    pane_context
                        .as_ref()
                        .and_then(|context| context.terminal_id.as_deref())
                })
                .unwrap_or("terminal-prewarm");
            let _ = terminal_environment(
                &shell,
                cwd.as_deref(),
                session_id,
                &config,
                pane_context.as_ref(),
            );
        }
    }
}

impl Default for GitOperationCompletion {
    fn default() -> Self {
        Self {
            success_message: String::new(),
            failure_prefix: "Git operation failed".to_string(),
            clear_commit_message: false,
            clear_git_diff_preview: false,
            clear_git_tree_cache: false,
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
    state.git_review.clone()
}

pub(in crate::app) fn git_status_tree_key(section_id: &str, path: &str) -> String {
    format!("{section_id}:{}", path.trim_matches('/'))
}

pub(in crate::app) const PET_DEX_FRAME_INTERVAL: Duration = Duration::from_millis(280);
pub(in crate::app) const PET_CUSTOM_INSTALL_WINDOW_WIDTH: f32 = 680.0;
pub(in crate::app) const PET_CUSTOM_INSTALL_INPUT_HEIGHT: f32 = 230.0;
pub(in crate::app) const PET_CUSTOM_INSTALL_READY_HEIGHT: f32 = 530.0;
pub(in crate::app) const PET_CUSTOM_INSTALL_ERROR_HEIGHT: f32 = 280.0;
pub(in crate::app) const TASK_COLUMN_FIXED_WIDTH: f32 = 240.0;

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

impl Drop for CoduxApp {
    fn drop(&mut self) {
        if self.window_mode == AppWindowMode::Main {
            self.shutdown_runtime_state_from_drop();
        }
    }
}
