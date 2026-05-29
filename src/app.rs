use crate::{
    terminal::{TerminalConfig, TerminalLaunchContext, TerminalPane, TerminalView},
    theme::{self, color},
};
use anyhow::Result;
use codux_runtime::{
    ai_history::{AIGlobalHistorySummary, AIHistorySummary, AISessionDetail, AISessionSummary},
    ai_history_normalized::{AIGlobalHistorySnapshot, AIHistoryProjectRequest, AIHistorySnapshot},
    desktop_pet::{
        DESKTOP_PET_BASE_HEIGHT, DESKTOP_PET_BASE_WIDTH, DESKTOP_PET_HIDE, DESKTOP_PET_MUTE_1_HOUR,
        DESKTOP_PET_MUTE_30_MINUTES, DESKTOP_PET_MUTE_TODAY, DESKTOP_PET_SKIP_LINE,
        DESKTOP_PET_SPEAK_LESS, DESKTOP_PET_SPEAK_MORE, DesktopPetSavedOrigin, DesktopPetWorkArea,
    },
    dialog::LocalizedOpenDialogRequest,
    files::FileChangeEvent,
    git::{
        GitBranchSummary, GitCommitSummary, GitFileStatus, GitRemoteSummary,
        GitReviewContentSummary, GitReviewSummary, GitSummary,
    },
    memory::{
        MemoryEntrySummary, MemoryExtractionStatusSnapshot, MemoryManagerSnapshot,
        MemoryProjectMigrationRequest, MemoryProjectProfileRefreshResult, MemorySummary,
        MemorySummaryUpdateRequest,
    },
    pet::{
        PetClaimRequest, PetCustomPet, PetCustomPetInstallPreview, PetCustomPetInstallRequest,
        PetRenameRequest, PetRestoreRequest, PetSnapshot, PetSummary,
    },
    project_activity::ProjectActivityEvent,
    project_open::ProjectOpenApplicationSummary,
    project_store::{ProjectCreateRequest, ProjectDefaultPushRemoteRequest, ProjectUpdateRequest},
    remote::{RemoteDeviceSummary, RemotePairingInfo, RemotePairingPollResult, RemoteSummary},
    runtime_activity::RuntimeActivitySummary,
    runtime_bridge::RuntimeInventory,
    runtime_event::{RuntimeEventSummary, RuntimeSessionSummary},
    runtime_ingress::{RuntimeIngressService, RuntimeIngressStatus},
    runtime_state::{FileEntry, FileKind, ProjectInfo, RuntimeService, RuntimeState},
    settings::SettingsSummary,
    ssh::{SSHConnectionProfile, SSHProfileSummary, SSHProfileUpsertRequest, SSHSummary},
    terminal_layout::{TerminalPaneSummary, TerminalTabSummary},
    terminal_pty::TerminalManager,
    terminal_runtime::{TerminalInputSummary, TerminalRuntimeService, TerminalRuntimeSessionInput},
    tool_permissions::ToolPermissionsSummary,
    worktree::{ProjectWorktreeGitSummary, WorktreeInfo, WorktreeTaskInfo},
};
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext, Bounds, ClipboardItem, Context, DispatchPhase,
    FontWeight, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ObjectFit,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, StyledImage, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, img,
    linear_color_stop, linear_gradient, point, prelude::FluentBuilder as _, px, size,
};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Root, Sizable,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenu, PopupMenuItem},
    resizable::{resizable_panel, v_resizable},
    scroll::ScrollableElement,
    tag::Tag,
    tooltip::Tooltip,
};
use std::{
    any::TypeId,
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod about;
mod formatting;
pub(crate) mod native_menu;
mod pet;
mod project_column;
mod project_editor;
mod settings;
mod sidebars;
mod status_bar;
mod task_column;
mod terminal_state;
mod types;
mod workspace;
use self::{
    formatting::{compact_number, relative_time_label},
    settings::SettingsPane,
    sidebars::{
        AssistantPanel, current_directory_suffix, file_directory_option, file_preview_workspace,
        file_section, git_diff_window_workspace, git_review_workspace, git_workspace_section,
        memory_manager_window_workspace, parent_relative_directory,
    },
    terminal_state::{
        prepare_memory_launch_artifacts, spawn_terminal_tabs, terminal_config_for_settings,
        terminal_launch_context, terminal_pane_launch_context, terminal_pane_summary,
        terminal_restore_plan, terminal_tab_summary,
    },
    types::*,
};

pub struct CoduxApp {
    window_mode: AppWindowMode,
    terminals: Vec<TerminalTab>,
    terminal_manager: Arc<TerminalManager>,
    active_terminal_id: usize,
    next_terminal_index: usize,
    runtime: RuntimeInventory,
    runtime_ingress: RuntimeIngressStatus,
    state: RuntimeState,
    runtime_service: RuntimeService,
    is_exiting: bool,
    status_message: String,
    desktop_pet_window: Option<AnyWindowHandle>,
    desktop_pet_line_skipped: bool,
    desktop_pet_line: String,
    file_preview: String,
    file_editable: bool,
    file_dirty: bool,
    file_search_open: bool,
    file_search_query: String,
    file_search_match_index: usize,
    file_directory: String,
    selected_file_entry: Option<String>,
    file_name_draft_kind: Option<FileNameDraftKind>,
    file_name_draft_value: String,
    file_tree_expanded_dirs: HashSet<String>,
    file_tree_children: HashMap<String, Vec<FileEntry>>,
    selected_git_file: Option<String>,
    selected_git_branch: Option<String>,
    git_review: GitReviewSummary,
    git_expanded_sections: HashSet<String>,
    git_expanded_dirs: HashSet<String>,
    git_tree_children: HashMap<String, Vec<GitFileStatus>>,
    git_diff_preview: String,
    git_diff_window_path: Option<String>,
    git_diff_window_content: String,
    git_diff_window_error: Option<String>,
    git_review_content: Option<GitReviewContentSummary>,
    git_clone_remote_url: String,
    git_remote_name: String,
    git_remote_url: String,
    git_running_operation: Option<GitRunningOperation>,
    git_commit_message: String,
    pet_install_url: String,
    pet_install_display_name: String,
    pet_install_preview: Option<PetCustomPetInstallPreview>,
    pet_install_previewing: bool,
    pet_installing: bool,
    pet_custom_pets: Vec<PetCustomPet>,
    pet_claim_species: String,
    selected_ai_session_id: Option<String>,
    selected_ai_provider_id: Option<String>,
    ai_provider_testing_id: Option<String>,
    selected_memory_entry_id: Option<String>,
    selected_memory_summary_id: Option<String>,
    selected_notification_channel_id: Option<String>,
    notification_testing_channel_id: Option<String>,
    active_settings_pane: SettingsPane,
    memory_manager_tab: MemoryManagerTab,
    memory_processing: bool,
    selected_runtime_terminal_id: Option<String>,
    selected_ssh_profile_id: Option<String>,
    ssh_testing: bool,
    ssh_draft_id: Option<String>,
    ssh_draft_name: String,
    ssh_draft_host: String,
    ssh_draft_port: String,
    ssh_draft_username: String,
    ssh_draft_credential_kind: String,
    ssh_draft_private_key_path: String,
    ssh_draft_password: String,
    ssh_draft_key_passphrase: String,
    selected_remote_device_id: Option<String>,
    remote_pairing_poll_generation: u64,
    recording_shortcut_id: Option<String>,
    agent_split_enabled: bool,
    workspace_view: WorkspaceView,
    assistant_panel: Option<AssistantPanel>,
    project_column_collapsed: bool,
    task_column_collapsed: bool,
    project_open_applications: Vec<ProjectOpenApplicationSummary>,
    project_editor_project_id: Option<String>,
    project_editor_name: String,
    project_editor_path: String,
    project_editor_badge_symbol: Option<String>,
    project_editor_badge_color_hex: String,
}

fn shortcut_display_from_keystroke(keystroke: &gpui::Keystroke) -> String {
    let mut parts = Vec::new();
    if keystroke.modifiers.platform {
        parts.push(
            if cfg!(target_os = "macos") {
                "⌘"
            } else {
                "Ctrl+"
            }
            .to_string(),
        );
    }
    if keystroke.modifiers.control {
        parts.push(
            if cfg!(target_os = "macos") {
                "⌃"
            } else {
                "Ctrl+"
            }
            .to_string(),
        );
    }
    if keystroke.modifiers.alt {
        parts.push(
            if cfg!(target_os = "macos") {
                "⌥"
            } else {
                "Alt+"
            }
            .to_string(),
        );
    }
    if keystroke.modifiers.shift {
        parts.push(
            if cfg!(target_os = "macos") {
                "⇧"
            } else {
                "Shift+"
            }
            .to_string(),
        );
    }
    let key = if keystroke.key.chars().count() == 1 {
        keystroke.key.to_uppercase()
    } else {
        keystroke.key.clone()
    };
    parts.push(key);
    parts.join("")
}

fn default_shortcut_display(shortcut_id: &str) -> Option<&'static str> {
    let primary = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    match (shortcut_id, primary) {
        ("view.terminal", "⌘") => Some("⌘1"),
        ("view.files", "⌘") => Some("⌘2"),
        ("view.review", "⌘") => Some("⌘3"),
        ("project.create", "⌘") => Some("⌘N"),
        ("settings.open", "⌘") => Some("⌘,"),
        ("task.create", "⌘") => Some("⌘N"),
        ("editor.save", "⌘") => Some("⌘S"),
        ("editor.search", "⌘") => Some("⌘F"),
        ("close.active", "⌘") => Some("⌘W"),
        ("view.terminal", _) => Some("Ctrl+1"),
        ("view.files", _) => Some("Ctrl+2"),
        ("view.review", _) => Some("Ctrl+3"),
        ("project.create", _) => Some("Ctrl+N"),
        ("settings.open", _) => Some("Ctrl+,"),
        ("task.create", _) => Some("Ctrl+N"),
        ("editor.save", _) => Some("Ctrl+S"),
        ("editor.search", _) => Some("Ctrl+F"),
        ("close.active", _) => Some("Ctrl+W"),
        _ => None,
    }
}

fn normalized_shortcut_text(value: &str) -> Option<String> {
    let mut rest = value.trim().to_lowercase();
    if rest.is_empty() {
        return None;
    }

    let platform = rest.contains("command") || rest.contains("cmd") || rest.contains('⌘');
    let control = rest.contains("control") || rest.contains("ctrl") || rest.contains('⌃');
    let alt = rest.contains("option") || rest.contains("alt") || rest.contains('⌥');
    let shift = rest.contains("shift") || rest.contains('⇧');

    for token in [
        "command", "cmd", "control", "ctrl", "option", "alt", "shift", "⌘", "⌃", "⌥", "⇧", "+",
    ] {
        rest = rest.replace(token, "");
    }
    rest.retain(|character| !character.is_whitespace());
    if rest.is_empty() {
        return None;
    }

    let key = if rest.chars().count() == 1 {
        rest.to_uppercase()
    } else {
        rest
    };
    Some(format!(
        "{}{}{}{}{}",
        if platform { "Meta+" } else { "" },
        if control { "Ctrl+" } else { "" },
        if alt { "Alt+" } else { "" },
        if shift { "Shift+" } else { "" },
        key
    ))
}

fn shortcut_value_matches(configured: &str, actual: &str) -> bool {
    let Some(actual) = normalized_shortcut_text(actual) else {
        return false;
    };
    configured
        .split('/')
        .filter_map(normalized_shortcut_text)
        .any(|candidate| candidate == actual)
}

fn shortcut_matches(shortcuts: &HashMap<String, String>, shortcut_id: &str, actual: &str) -> bool {
    shortcuts
        .get(shortcut_id)
        .filter(|value| !value.trim().is_empty())
        .map(|value| shortcut_value_matches(value, actual))
        .unwrap_or_else(|| {
            default_shortcut_display(shortcut_id)
                .map(|value| shortcut_value_matches(value, actual))
                .unwrap_or(false)
        })
}

fn git_remote_action_label(action: &str) -> String {
    if let Some(remote) = action.strip_prefix("push:") {
        return format!("push to {remote}");
    }
    if let Some(remote_branch) = action.strip_prefix("push-branch:") {
        return format!("push to {remote_branch}");
    }

    match action {
        "force-push" => "force push".to_string(),
        _ => action.to_string(),
    }
}

fn normalized_git_action_paths(paths: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for path in paths {
        let path = path.trim().trim_start_matches('/').to_string();
        if path.is_empty() || !seen.insert(path.clone()) {
            continue;
        }
        normalized.push(path);
    }
    normalized
}

fn file_search_status_message(index: usize, count: usize) -> String {
    if count == 0 {
        "file search has no matches".to_string()
    } else {
        format!("file search match {} of {count}", index + 1)
    }
}

#[derive(Clone, Debug)]
struct GitOperationCompletion {
    success_message: String,
    failure_prefix: String,
    clear_commit_message: bool,
    clear_git_diff_preview: bool,
    clear_git_tree_cache: bool,
    clear_remote_url: bool,
    reload_state: bool,
    refresh_review: bool,
    diff_file_to_reload: Option<String>,
    clear_selected_branch: bool,
    selected_branch: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct RuntimeActivityTickResult {
    project_events: usize,
    file_events: usize,
    ai_events: usize,
    memory_events: usize,
    dock_badge_count: Option<i64>,
    changed: bool,
    ai_state_error: Option<String>,
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

fn ai_tool_launch_command(tool: AIToolLauncher, permissions: &ToolPermissionsSummary) -> String {
    match tool {
        AIToolLauncher::Codex => {
            let mut parts = vec!["codex".to_string()];
            push_flag_value(&mut parts, "--model", &permissions.codex_model);
            if !permissions.codex_effort.trim().is_empty() && permissions.codex_effort != "medium" {
                push_flag_value(&mut parts, "--reasoning-effort", &permissions.codex_effort);
            }
            shell_join(parts)
        }
        AIToolLauncher::Claude => {
            let mut parts = vec!["claude".to_string()];
            push_flag_value(&mut parts, "--model", &permissions.claude_code_model);
            shell_join(parts)
        }
        AIToolLauncher::Gemini => {
            let mut parts = vec!["gemini".to_string()];
            push_flag_value(&mut parts, "--model", &permissions.gemini_model);
            shell_join(parts)
        }
        AIToolLauncher::OpenCode => {
            let mut parts = vec!["opencode".to_string()];
            push_flag_value(&mut parts, "--model", &permissions.opencode_model);
            shell_join(parts)
        }
        AIToolLauncher::Kiro => {
            let mut parts = vec!["kiro".to_string()];
            push_flag_value(&mut parts, "--model", &permissions.kiro_model);
            shell_join(parts)
        }
    }
}

fn ai_session_restore_command(session: &AISessionSummary) -> String {
    let tool = session.source.to_lowercase();
    let id = session
        .external_session_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or(&session.session_key);
    let quoted_id = shell_quote(id);
    if tool.contains("codex") {
        format!("codex resume {quoted_id}")
    } else if tool.contains("claude") {
        format!("claude --resume {quoted_id}")
    } else if tool.contains("agy") || tool.contains("antigravity") {
        format!("agy resume {quoted_id}")
    } else if tool.contains("gemini") {
        format!("gemini resume {quoted_id}")
    } else if tool.contains("opencode") {
        format!("opencode run --session {quoted_id}")
    } else {
        format!("codex resume {quoted_id}")
    }
}

fn normalized_ai_history_snapshot_to_summary(snapshot: AIHistorySnapshot) -> AIHistorySummary {
    AIHistorySummary {
        indexed: true,
        indexed_at: Some(snapshot.indexed_at),
        project_total_tokens: snapshot.project_summary.project_total_tokens,
        project_cached_input_tokens: snapshot.project_summary.project_cached_input_tokens,
        today_total_tokens: snapshot.project_summary.today_total_tokens,
        today_cached_input_tokens: snapshot.project_summary.today_cached_input_tokens,
        session_count: snapshot.sessions.len(),
        sessions: snapshot
            .sessions
            .into_iter()
            .map(normalized_ai_session_to_summary)
            .collect(),
        heatmap: snapshot.heatmap,
        today_time_buckets: snapshot.today_time_buckets,
        tool_breakdown: snapshot.tool_breakdown,
        model_breakdown: snapshot.model_breakdown,
        error: None,
    }
}

fn normalized_global_ai_history_snapshot_to_summary(
    snapshot: AIGlobalHistorySnapshot,
) -> AIGlobalHistorySummary {
    AIGlobalHistorySummary {
        indexed_project_count: snapshot.project_count,
        session_count: snapshot.sessions.len(),
        total_tokens: snapshot.total_tokens,
        cached_input_tokens: snapshot.cached_input_tokens,
        today_total_tokens: snapshot.today_total_tokens,
        today_cached_input_tokens: snapshot.today_cached_input_tokens,
        project_totals: Vec::new(),
        recent_sessions: snapshot
            .sessions
            .into_iter()
            .take(10)
            .map(normalized_ai_session_to_summary)
            .collect(),
        error: None,
    }
}

fn ai_history_project_request(project: &ProjectInfo) -> AIHistoryProjectRequest {
    AIHistoryProjectRequest {
        id: project.id.clone(),
        name: project.name.clone(),
        path: project.path.clone(),
    }
}

fn ai_history_project_requests(projects: &[ProjectInfo]) -> Vec<AIHistoryProjectRequest> {
    projects.iter().map(ai_history_project_request).collect()
}

fn normalized_ai_session_to_summary(
    session: codux_runtime::ai_history_normalized::AISessionSummary,
) -> AISessionSummary {
    let session_id = session.session_id;
    AISessionSummary {
        id: session_id.clone(),
        session_key: session
            .external_session_id
            .clone()
            .unwrap_or_else(|| session_id.clone()),
        external_session_id: session.external_session_id,
        title: session.session_title,
        source: session.last_tool.unwrap_or_else(|| "ai".to_string()),
        last_model: session.last_model,
        last_seen_at: session.last_seen_at,
        total_tokens: session.total_tokens,
        cached_input_tokens: session.cached_input_tokens,
        request_count: session.request_count,
    }
}

#[cfg(test)]
fn ssh_connect_command(profile: &SSHProfileSummary) -> String {
    shell_join(vec!["codux-ssh".to_string(), profile.id.clone()])
}

fn generated_git_commit_message(git: &GitSummary) -> String {
    let changed = git.staged + git.unstaged + git.untracked;
    if git.staged > 0 {
        format!("Update {} staged file{}", git.staged, plural(git.staged))
    } else if changed > 0 {
        format!("Update {} changed file{}", changed, plural(changed))
    } else {
        "Update project files".to_string()
    }
}

fn generated_git_branch_name() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("codux-gpui-{timestamp}")
}

fn generated_project_child_name(files: &[FileEntry], directory: bool) -> String {
    let prefix = if directory {
        "codux-folder"
    } else {
        "codux-file"
    };
    let suffix = if directory { "" } else { ".txt" };
    for index in 1..1000 {
        let name = format!("{prefix}-{index}{suffix}");
        if !files.iter().any(|file| file.name == name) {
            return name;
        }
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{prefix}-{timestamp}{suffix}")
}

const PROJECT_BADGE_COLORS: &[&str] = &[
    "#0A84FF", "#8C52FF", "#4C8BF5", "#15B8A6", "#32C766", "#FFB020", "#FF7A59", "#FF5C8A",
    "#7B61FF", "#00A3FF", "#6D9F71",
];

fn project_badge_text_from_name(name: &str) -> Option<String> {
    let badge = name
        .chars()
        .filter(|character| !character.is_whitespace())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    (!badge.is_empty()).then_some(badge)
}

fn join_relative_child_path(parent: &str, name: &str) -> String {
    let parent = parent.trim().trim_matches('/');
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn push_flag_value(parts: &mut Vec<String>, flag: &str, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    parts.push(flag.to_string());
    parts.push(value.to_string());
}

fn shell_join(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .map(|part| shell_quote(&part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '='))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn app_git_review(state: &RuntimeState) -> GitReviewSummary {
    state.git_review.clone()
}

fn desktop_pet_fallback_line() -> &'static str {
    "休息一下，我会在这里陪你盯住进度。"
}

const PET_ATLAS_COLUMNS: f32 = 8.0;
const PET_ATLAS_ROWS: f32 = 9.0;
const PET_ATLAS_CELL_WIDTH: f32 = 192.0;
const PET_ATLAS_CELL_HEIGHT: f32 = 208.0;
const DESKTOP_PET_SPRITE_SIZE: f32 = 112.0;

fn pet_sprite_visible_width(size: f32) -> f32 {
    PET_ATLAS_CELL_WIDTH * (size / PET_ATLAS_CELL_HEIGHT)
}

fn pet_sprite_path(
    runtime_asset_root: &Path,
    support_dir: &Path,
    pet: &PetSummary,
    custom_pets: &[PetCustomPet],
) -> PathBuf {
    let fallback = runtime_asset_root
        .join("pets")
        .join("voidcat")
        .join("spritesheet.png");
    if let Some(custom_id) = pet.species.strip_prefix("custom:") {
        if let Some(custom_pet) = custom_pets.iter().find(|item| item.id == custom_id) {
            let path = support_dir
                .join("custom-pets")
                .join(&custom_pet.directory_name)
                .join(&custom_pet.spritesheet_path);
            if path.is_file() {
                return path;
            }
        }
        return fallback;
    }

    let species = pet.species.trim();
    let path = runtime_asset_root
        .join("pets")
        .join(if species.is_empty() {
            "voidcat"
        } else {
            species
        })
        .join("spritesheet.png");
    if path.is_file() { path } else { fallback }
}

fn custom_pet_sprite_path(support_dir: &Path, custom_pet: &PetCustomPet) -> PathBuf {
    support_dir
        .join("custom-pets")
        .join(&custom_pet.directory_name)
        .join(&custom_pet.spritesheet_path)
}

impl CoduxApp {
    pub fn new(window: &mut Window, cx: &mut App) -> Result<Self> {
        let mut state = RuntimeState::load();
        theme::apply_component_theme_for_name(&state.settings.theme, Some(window), cx);
        let runtime = RuntimeInventory::load();
        let runtime_ingress = RuntimeIngressService::new().start_background();
        let runtime_service = RuntimeService::new(state.support_dir.clone());
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
                match runtime_service.save_ai_runtime_state_snapshot(&snapshot) {
                    Ok(summary) => {
                        state.ai_runtime_state = summary;
                        "AI runtime supervisor started".to_string()
                    }
                    Err(error) => {
                        format!("AI runtime supervisor started, state save failed: {error}")
                    }
                }
            }
            Err(error) => format!("AI runtime supervisor failed: {error}"),
        };
        let ready_snapshot = runtime_service.app_runtime_ready(true, window.is_window_active());
        state.remote = ready_snapshot.remote.clone();
        let restore_plan = terminal_restore_plan(&state.terminal_layout, &state.terminal_runtime);
        prepare_memory_launch_artifacts(&state);
        let launch_context = terminal_launch_context(&state, &runtime, &tool_permissions);
        let terminal_config = terminal_config_for_settings(&state.settings);
        let terminal_manager = Arc::new(TerminalManager::new());
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
            .map(|slot| slot.pane.view.clone())
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
        let pet_custom_pets = runtime_service.pet_catalog().custom_pets;

        let mut app = Self {
            window_mode: AppWindowMode::Main,
            terminals,
            terminal_manager,
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
            desktop_pet_line_skipped: false,
            desktop_pet_line: desktop_pet_fallback_line().to_string(),
            file_preview: "select a file to preview it".to_string(),
            file_editable: false,
            file_dirty: false,
            file_search_open: false,
            file_search_query: String::new(),
            file_search_match_index: 0,
            file_directory: String::new(),
            selected_file_entry: None,
            file_name_draft_kind: None,
            file_name_draft_value: String::new(),
            file_tree_expanded_dirs: HashSet::new(),
            file_tree_children: HashMap::new(),
            selected_git_file: None,
            selected_git_branch,
            git_review,
            git_expanded_sections: HashSet::from(["changed".to_string(), "untracked".to_string()]),
            git_expanded_dirs: HashSet::new(),
            git_tree_children: HashMap::new(),
            git_diff_preview: "select a changed file to preview its diff".to_string(),
            git_diff_window_path: None,
            git_diff_window_content: String::new(),
            git_diff_window_error: None,
            git_review_content: None,
            git_clone_remote_url: String::new(),
            git_remote_name: "origin".to_string(),
            git_remote_url: String::new(),
            git_running_operation: None,
            git_commit_message: String::new(),
            pet_install_url: String::new(),
            pet_install_display_name: String::new(),
            pet_install_preview: None,
            pet_install_previewing: false,
            pet_installing: false,
            pet_custom_pets,
            pet_claim_species: String::new(),
            selected_ai_session_id: None,
            selected_ai_provider_id,
            ai_provider_testing_id: None,
            selected_memory_entry_id,
            selected_memory_summary_id,
            selected_notification_channel_id,
            notification_testing_channel_id: None,
            active_settings_pane: SettingsPane::General,
            memory_manager_tab: MemoryManagerTab::Active,
            memory_processing: false,
            selected_runtime_terminal_id,
            selected_ssh_profile_id,
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
            remote_pairing_poll_generation: 0,
            recording_shortcut_id: None,
            agent_split_enabled: false,
            workspace_view: WorkspaceView::Terminal,
            assistant_panel: None,
            project_column_collapsed: false,
            task_column_collapsed: false,
            project_open_applications,
            project_editor_project_id: None,
            project_editor_name: String::new(),
            project_editor_path: String::new(),
            project_editor_badge_symbol: None,
            project_editor_badge_color_hex: PROJECT_BADGE_COLORS[0].to_string(),
        };
        let _ = app.persist_terminal_runtime();
        Ok(app)
    }

    fn new_settings_window() -> Self {
        let mut state = RuntimeState::load();
        let runtime = RuntimeInventory::load();
        let runtime_service = RuntimeService::new(state.support_dir.clone());
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
        let pet_custom_pets = runtime_service.pet_catalog().custom_pets;

        Self {
            window_mode: AppWindowMode::Settings,
            terminals: Vec::new(),
            terminal_manager: Arc::new(TerminalManager::new()),
            active_terminal_id: 0,
            next_terminal_index: 1,
            runtime,
            runtime_ingress: RuntimeIngressStatus::default(),
            state,
            runtime_service,
            is_exiting: false,
            status_message: "settings window ready".to_string(),
            desktop_pet_window: None,
            desktop_pet_line_skipped: false,
            desktop_pet_line: desktop_pet_fallback_line().to_string(),
            file_preview: "select a file to preview it".to_string(),
            file_editable: false,
            file_dirty: false,
            file_search_open: false,
            file_search_query: String::new(),
            file_search_match_index: 0,
            file_directory: String::new(),
            selected_file_entry: None,
            file_name_draft_kind: None,
            file_name_draft_value: String::new(),
            file_tree_expanded_dirs: HashSet::new(),
            file_tree_children: HashMap::new(),
            selected_git_file: None,
            selected_git_branch,
            git_review,
            git_expanded_sections: HashSet::from(["changed".to_string(), "untracked".to_string()]),
            git_expanded_dirs: HashSet::new(),
            git_tree_children: HashMap::new(),
            git_diff_preview: "select a changed file to preview its diff".to_string(),
            git_diff_window_path: None,
            git_diff_window_content: String::new(),
            git_diff_window_error: None,
            git_review_content: None,
            git_clone_remote_url: String::new(),
            git_remote_name: "origin".to_string(),
            git_remote_url: String::new(),
            git_running_operation: None,
            git_commit_message: String::new(),
            pet_install_url: String::new(),
            pet_install_display_name: String::new(),
            pet_install_preview: None,
            pet_install_previewing: false,
            pet_installing: false,
            pet_custom_pets,
            pet_claim_species: String::new(),
            selected_ai_session_id: None,
            selected_ai_provider_id,
            ai_provider_testing_id: None,
            selected_memory_entry_id,
            selected_memory_summary_id,
            selected_notification_channel_id,
            notification_testing_channel_id: None,
            active_settings_pane: SettingsPane::General,
            memory_manager_tab: MemoryManagerTab::Active,
            memory_processing: false,
            selected_runtime_terminal_id,
            selected_ssh_profile_id: None,
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
            remote_pairing_poll_generation: 0,
            recording_shortcut_id: None,
            agent_split_enabled: false,
            workspace_view: WorkspaceView::Terminal,
            assistant_panel: None,
            project_column_collapsed: false,
            task_column_collapsed: false,
            project_open_applications,
            project_editor_project_id: None,
            project_editor_name: String::new(),
            project_editor_path: String::new(),
            project_editor_badge_symbol: None,
            project_editor_badge_color_hex: PROJECT_BADGE_COLORS[0].to_string(),
        }
    }

    fn new_project_editor_window(project: ProjectInfo) -> Self {
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

    fn new_project_creator_window() -> Self {
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

    fn new_desktop_pet_window() -> Self {
        let mut app = Self::new_settings_window();
        app.window_mode = AppWindowMode::DesktopPet;
        app.status_message = "desktop pet window ready".to_string();
        app
    }

    fn new_pet_window(mode: AppWindowMode) -> Self {
        let mut app = Self::new_settings_window();
        app.window_mode = mode;
        app.status_message = match mode {
            AppWindowMode::PetClaim => "pet claim window ready".to_string(),
            AppWindowMode::PetCustomInstall => "custom pet install window ready".to_string(),
            AppWindowMode::PetDex => "pet dex window ready".to_string(),
            _ => "pet window ready".to_string(),
        };
        app
    }

    fn start_desktop_pet_speech_loop(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::DesktopPet {
            return;
        }

        self.request_desktop_pet_speech("idle", desktop_pet_fallback_line(), cx);
    }

    pub(crate) fn start_runtime_event_loop(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            return;
        }
        self.sync_desktop_pet_window(false, cx);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let mut ticks = 0_u64;
            loop {
                let _ = codux_runtime::async_runtime::spawn_blocking(|| {
                    std::thread::sleep(Duration::from_millis(500));
                })
                .await;
                ticks = ticks.wrapping_add(1);
                let include_scheduled_tick = ticks % 4 == 0;

                if this
                    .update(cx, |app, cx| {
                        if app.window_mode != AppWindowMode::Main {
                            return;
                        }
                        if app
                            .apply_runtime_activity_tick(true, true, include_scheduled_tick)
                            .changed
                        {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    return;
                }
            }
        })
        .detach();
    }

    fn shutdown_runtime_state(&mut self) {
        if self.is_exiting {
            return;
        }
        self.is_exiting = true;
        let _ = self.persist_terminal_runtime();
        for terminal in self.terminal_manager.list() {
            let _ = self.terminal_manager.kill(&terminal.id);
        }
        self.runtime_service.shutdown_runtime_state();
    }

    fn open_desktop_pet_window(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.desktop_pet_window {
            if handle.update(cx, |_view, _window, _cx| {}).is_ok() {
                self.status_message = "desktop pet window already opened".to_string();
                cx.notify();
                return;
            }
            self.desktop_pet_window = None;
        }

        match self.runtime_service.desktop_pet_should_show() {
            Ok(true) => {}
            Ok(false) => {
                self.status_message =
                    "desktop pet needs pet enabled, desktop widget enabled, and a claimed pet"
                        .to_string();
                cx.notify();
                return;
            }
            Err(error) => {
                self.status_message = format!("failed to check desktop pet: {error}");
                cx.notify();
                return;
            }
        }
        self.runtime_service.desktop_pet_set_bubble_visible(true);

        let display = cx.primary_display();
        let display_id = display.as_ref().map(|display| display.id());
        let visible_bounds = display
            .as_ref()
            .map(|display| display.visible_bounds())
            .unwrap_or_else(|| Bounds::centered(None, size(px(1280.0), px(820.0)), cx));
        let work_area = DesktopPetWorkArea {
            x: visible_bounds.origin.x.to_f64(),
            y: visible_bounds.origin.y.to_f64(),
            width: visible_bounds.size.width.to_f64(),
            height: visible_bounds.size.height.to_f64(),
            scale_factor: 1.0,
        };
        let origin = self.runtime_service.desktop_pet_initial_position(work_area);
        let bounds = Bounds::new(
            point(px(origin.x as f32), px(origin.y as f32)),
            size(
                px(DESKTOP_PET_BASE_WIDTH as f32),
                px(DESKTOP_PET_BASE_HEIGHT as f32),
            ),
        );

        let result = cx.open_window(
            WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(
                    px(DESKTOP_PET_BASE_WIDTH as f32),
                    px(DESKTOP_PET_BASE_HEIGHT as f32),
                )),
                display_id,
                focus: false,
                show: true,
                kind: WindowKind::PopUp,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| {
                let app = CoduxApp::new_desktop_pet_window();
                theme::apply_component_theme_for_name(&app.state.settings.theme, Some(window), cx);
                let view = cx.new(|_| app);
                view.update(cx, |app, cx| app.start_desktop_pet_speech_loop(cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(handle) => {
                self.desktop_pet_window = Some(handle.into());
                "desktop pet window opened".to_string()
            }
            Err(error) => format!("failed to open desktop pet window: {error}"),
        };
        cx.notify();
    }

    fn close_desktop_pet_window(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.desktop_pet_window.take() {
            let _ = handle.update(cx, |_view, window, _cx| window.remove_window());
        }
        self.runtime_service.desktop_pet_set_bubble_visible(false);
    }

    fn sync_desktop_pet_window(&mut self, report_unavailable: bool, cx: &mut Context<Self>) {
        match self.runtime_service.desktop_pet_should_show() {
            Ok(true) => self.open_desktop_pet_window(cx),
            Ok(false) => {
                self.close_desktop_pet_window(cx);
                if report_unavailable {
                    self.status_message =
                        "desktop pet needs pet enabled, desktop widget enabled, and a claimed pet"
                            .to_string();
                    cx.notify();
                }
            }
            Err(error) => {
                self.close_desktop_pet_window(cx);
                if report_unavailable {
                    self.status_message = format!("failed to check desktop pet: {error}");
                    cx.notify();
                }
            }
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;

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

        if self.handle_configured_shortcut(event, _window, cx) {
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

    fn handle_configured_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
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
        if task_create && !project_create {
            self.create_worktree(window, cx);
            return true;
        }
        if project_create && !task_create {
            self.open_project_create_window(window, cx);
            return true;
        }

        false
    }

    fn open_settings_window(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_window_with_pane(SettingsPane::General, cx);
    }

    fn open_settings_window_with_pane(&mut self, pane: SettingsPane, cx: &mut Context<Self>) {
        let bounds = Bounds::centered(None, size(px(980.0), px(720.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Codux Settings".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(760.0), px(560.0))),
                ..Default::default()
            },
            |window, cx| {
                let mut app = CoduxApp::new_settings_window();
                app.active_settings_pane = pane;
                theme::apply_component_theme_for_name(&app.state.settings.theme, Some(window), cx);
                let view = cx.new(|_| app);
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        match result {
            Ok(_) => {
                self.status_message = format!("settings window opened: {}", pane.label());
            }
            Err(error) => {
                self.status_message = format!("failed to open settings window: {error}");
            }
        }
        cx.notify();
    }

    fn open_remote_settings_window(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_window_with_pane(SettingsPane::Remote, cx);
    }

    fn open_ssh_settings_window(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_window_with_pane(SettingsPane::SSH, cx);
    }

    fn toggle_project_column(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.project_column_collapsed = !self.project_column_collapsed;
        cx.notify();
    }

    fn toggle_task_column(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.task_column_collapsed = !self.task_column_collapsed;
        cx.notify();
    }

    fn close_active_workspace_item(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn set_workspace_view(
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

    fn set_settings_pane(&mut self, pane: SettingsPane, cx: &mut Context<Self>) {
        self.active_settings_pane = pane;
        cx.notify();
    }

    fn toggle_assistant_panel(
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
        cx.notify();
    }

    fn refresh_assistant_panel_state(&mut self, panel: AssistantPanel) {
        match panel {
            AssistantPanel::AIStats => {
                let _ = self.refresh_ai_history_summaries_for_selected_project();
                self.refresh_runtime_activity_state(false);
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

    fn refresh_files_panel_state(&mut self) {
        let Some(project) = &self.state.selected_project else {
            return;
        };
        self.state.files = self
            .runtime_service
            .reload_project_files(&project.path, file_directory_option(&self.file_directory));
        self.refresh_file_tree_cache();
        self.normalize_selected_file_entry();
    }

    fn refresh_git_panel_state(&mut self) {
        let Some(project) = self.state.selected_project.clone() else {
            return;
        };
        self.state.git = self.runtime_service.reload_project_git(&project.path);
        self.refresh_git_review_for_project(&project.path);
        self.normalize_selected_git_file();
        self.normalize_selected_git_branch();
    }

    fn refresh_runtime_activity_state(&mut self, poll_live_ai: bool) {
        self.state.runtime_activity = self.runtime_service.reload_runtime_activity();
        self.state.runtime_events = self.runtime_service.reload_runtime_events();
        let ai_snapshot = if poll_live_ai {
            self.runtime_service
                .poll_ai_runtime_state()
                .unwrap_or_else(|_| self.runtime_service.ai_runtime_state_snapshot())
        } else {
            self.runtime_service.ai_runtime_state_snapshot()
        };
        self.state.ai_runtime_state = self
            .runtime_service
            .save_ai_runtime_state_snapshot(&ai_snapshot)
            .unwrap_or_else(|error| {
                let mut summary = self
                    .runtime_service
                    .reload_ai_runtime_state(&self.state.runtime_events);
                summary.error = Some(error);
                summary
            });
        self.normalize_selected_runtime_session();
    }

    fn select_project(&mut self, project_id: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.select_project(&project_id) {
            Ok(()) => self.status_message = "selected project saved to state.json".to_string(),
            Err(error) => self.status_message = format!("selected in memory only: {error}"),
        }
        self.state.select_project(&project_id);
        self.file_directory.clear();
        self.reset_file_tree_cache();
        self.file_preview = "select a file to preview it".to_string();
        self.file_editable = false;
        self.file_dirty = false;
        self.selected_file_entry = None;
        self.selected_git_file = None;
        self.normalize_selected_git_branch();
        self.git_diff_preview = "select a changed file to preview its diff".to_string();
        self.git_review_content = None;
        self.selected_ai_session_id = self
            .state
            .ai_history
            .sessions
            .first()
            .map(|session| session.id.clone());
        cx.notify();
    }

    fn reload_runtime_state(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state = self.runtime_service.reload_state();
        self.project_open_applications = self.runtime_service.project_open_applications();
        self.file_directory.clear();
        self.reset_file_tree_cache();
        self.file_editable = false;
        self.file_dirty = false;
        self.selected_file_entry = None;
        self.selected_git_file = None;
        self.normalize_selected_git_branch();
        self.git_diff_preview = "select a changed file to preview its diff".to_string();
        self.git_review_content = None;
        self.normalize_selected_ai_session();
        self.normalize_selected_runtime_session();
        self.normalize_selected_ssh_profile();
        self.status_message = "state reloaded from Codux support files".to_string();
        cx.notify();
    }

    fn reload_project_open_applications(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.project_open_applications = self.runtime_service.project_open_applications();
        self.status_message = "project application list refreshed".to_string();
        cx.notify();
    }

    fn reveal_selected_project_in_file_manager(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to reveal".to_string();
            cx.notify();
            return;
        };

        match self
            .runtime_service
            .project_reveal_in_file_manager(&project.path)
        {
            Ok(()) => {
                self.status_message = format!("revealed project: {}", project.name);
            }
            Err(error) => self.status_message = format!("failed to reveal project: {error}"),
        }
        cx.notify();
    }

    fn open_selected_project_in_application(
        &mut self,
        application_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to open".to_string();
            cx.notify();
            return;
        };

        let application_label = self
            .project_open_applications
            .iter()
            .find(|application| application.id == application_id)
            .map(|application| application.label.clone())
            .unwrap_or_else(|| application_id.clone());

        match self
            .runtime_service
            .project_open_in_application(project.path, application_id)
        {
            Ok(()) => {
                self.status_message = format!("opened {} in {application_label}", project.name);
            }
            Err(error) => {
                self.status_message = format!(
                    "failed to open {} in {application_label}: {error}",
                    project.name
                );
                self.project_open_applications = self.runtime_service.project_open_applications();
            }
        }
        cx.notify();
    }

    fn open_project_folder_from_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self
            .runtime_service
            .localized_open_dialog(LocalizedOpenDialogRequest {
                title: "Open Folder".to_string(),
                message: "Choose a project folder to import.".to_string(),
                prompt: "Open".to_string(),
                default_path: None,
                filters: Vec::new(),
                directory: true,
                multiple: false,
                can_create_directories: Some(false),
            }) {
            Ok(Some(paths)) => {
                let Some(path) = paths.first().cloned() else {
                    self.status_message = "project import canceled".to_string();
                    cx.notify();
                    return;
                };
                let name = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or("Project")
                    .to_string();
                match self.runtime_service.create_or_select_project(&name, &path) {
                    Ok(project_id) => {
                        self.state = self.runtime_service.reload_state();
                        self.normalize_selected_ai_session();
                        self.normalize_selected_runtime_session();
                        self.normalize_selected_ssh_profile();
                        self.status_message = format!("project added/selected: {project_id}");
                    }
                    Err(error) => {
                        self.status_message = format!("failed to add project: {error}");
                    }
                }
            }
            Ok(None) => {
                self.status_message = "project import canceled".to_string();
            }
            Err(error) => self.status_message = format!("failed to choose project folder: {error}"),
        }
        cx.notify();
    }

    fn close_selected_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to close".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.close_project(&project.id) {
            Ok(next_project_id) => {
                self.state = self.runtime_service.reload_state();
                self.normalize_selected_ai_session();
                self.normalize_selected_runtime_session();
                self.normalize_selected_ssh_profile();
                self.status_message = match next_project_id {
                    Some(next_project_id) => {
                        format!("closed {}, selected {next_project_id}", project.name)
                    }
                    None => format!("closed {}, no projects left", project.name),
                };
            }
            Err(error) => self.status_message = format!("failed to close project: {error}"),
        }
        cx.notify();
    }

    fn close_all_projects(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.projects.is_empty() {
            self.status_message = "no projects to close".to_string();
            cx.notify();
            return;
        }
        let closed = self.state.projects.len();
        match self.runtime_service.project_close_all() {
            Ok(_snapshot) => {
                self.state = self.runtime_service.reload_state();
                self.selected_file_entry = None;
                self.file_tree_expanded_dirs.clear();
                self.file_tree_children.clear();
                self.file_preview = "select a file to preview it".to_string();
                self.file_editable = false;
                self.file_dirty = false;
                self.selected_git_file = None;
                self.git_tree_children.clear();
                self.git_expanded_dirs.clear();
                self.git_diff_preview = "select a changed file to preview its diff".to_string();
                self.git_review_content = None;
                self.normalize_selected_ai_session();
                self.normalize_selected_runtime_session();
                self.normalize_selected_ssh_profile();
                self.status_message = format!(
                    "closed {closed} project{}",
                    if closed == 1 { "" } else { "s" }
                );
            }
            Err(error) => self.status_message = format!("failed to close projects: {error}"),
        }
        cx.notify();
    }

    fn rename_selected_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_selected_project_editor_window(_window, cx);
    }

    fn open_project_create_window(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let bounds = Bounds::centered(None, size(px(620.0), px(360.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Create Project".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(520.0), px(300.0))),
                ..Default::default()
            },
            move |window, cx| {
                let app = CoduxApp::new_project_creator_window();
                theme::apply_component_theme_for_name(&app.state.settings.theme, Some(window), cx);
                let view = cx.new(|_| app);
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(_) => "project creator opened".to_string(),
            Err(error) => format!("failed to open project creator: {error}"),
        };
        cx.notify();
    }

    fn open_selected_project_editor_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to edit".to_string();
            cx.notify();
            return;
        };

        let bounds = Bounds::centered(None, size(px(620.0), px(360.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Edit Project".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(520.0), px(300.0))),
                ..Default::default()
            },
            move |window, cx| {
                let app = CoduxApp::new_project_editor_window(project);
                theme::apply_component_theme_for_name(&app.state.settings.theme, Some(window), cx);
                let view = cx.new(|_| app);
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(_) => "project editor opened".to_string(),
            Err(error) => format!("failed to open project editor: {error}"),
        };
        cx.notify();
    }

    fn set_project_editor_name(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_name = value;
        cx.notify();
    }

    fn set_project_editor_path(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_path = value;
        cx.notify();
    }

    fn set_project_editor_badge_symbol(
        &mut self,
        value: Option<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_badge_symbol = value;
        cx.notify();
    }

    fn set_project_editor_badge_color(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_badge_color_hex = value;
        cx.notify();
    }

    fn choose_project_editor_directory(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self
            .runtime_service
            .localized_open_dialog(LocalizedOpenDialogRequest {
                title: "Choose Project Directory".to_string(),
                message: "Select a folder for this project.".to_string(),
                prompt: "Choose".to_string(),
                default_path: Some(self.project_editor_path.clone()),
                filters: Vec::new(),
                directory: true,
                multiple: false,
                can_create_directories: Some(false),
            }) {
            Ok(Some(paths)) => {
                if let Some(path) = paths.first() {
                    self.project_editor_path = path.clone();
                    self.status_message = "project directory selected".to_string();
                } else {
                    self.status_message = "project directory selection canceled".to_string();
                }
            }
            Ok(None) => self.status_message = "project directory selection canceled".to_string(),
            Err(error) => {
                self.status_message = format!("failed to choose project directory: {error}")
            }
        }
        cx.notify();
    }

    fn save_project_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.project_editor_name.trim().to_string();
        let path = self.project_editor_path.trim().to_string();
        if name.is_empty() || path.is_empty() {
            self.status_message = "project name and path are required".to_string();
            cx.notify();
            return;
        }

        if let Some(project_id) = self.project_editor_project_id.clone() {
            match self.runtime_service.project_update(ProjectUpdateRequest {
                project_id,
                name: name.clone(),
                path,
                badge_text: project_badge_text_from_name(&name),
                badge_symbol: self.project_editor_badge_symbol.clone(),
                badge_color_hex: Some(self.project_editor_badge_color_hex.clone()),
            }) {
                Ok(_snapshot) => {
                    self.state = self.runtime_service.reload_state();
                    self.status_message = format!("project saved: {name}");
                    window.remove_window();
                }
                Err(error) => self.status_message = format!("failed to save project: {error}"),
            }
        } else {
            match self.runtime_service.project_create(ProjectCreateRequest {
                name: name.clone(),
                path,
                badge_text: project_badge_text_from_name(&name),
                badge_symbol: self.project_editor_badge_symbol.clone(),
                badge_color_hex: Some(self.project_editor_badge_color_hex.clone()),
            }) {
                Ok(_snapshot) => {
                    self.state = self.runtime_service.reload_state();
                    self.status_message = format!("project created: {name}");
                    window.remove_window();
                }
                Err(error) => self.status_message = format!("failed to create project: {error}"),
            }
        }
        cx.notify();
    }

    fn move_selected_project_up(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to move".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.move_project_up(&project.id) {
            Ok(()) => {
                self.state = self.runtime_service.reload_state();
                self.normalize_selected_ai_session();
                self.normalize_selected_runtime_session();
                self.normalize_selected_ssh_profile();
                self.status_message = format!("moved project up: {}", project.name);
            }
            Err(error) => self.status_message = format!("failed to move project: {error}"),
        }
        cx.notify();
    }

    fn move_selected_project_down(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to move".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.move_project_down(&project.id) {
            Ok(()) => {
                self.state = self.runtime_service.reload_state();
                self.normalize_selected_ai_session();
                self.normalize_selected_runtime_session();
                self.normalize_selected_ssh_profile();
                self.status_message = format!("moved project down: {}", project.name);
            }
            Err(error) => self.status_message = format!("failed to move project: {error}"),
        }
        cx.notify();
    }

    fn set_terminal_font_size(
        &mut self,
        size: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_terminal_font_size(&size) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.apply_terminal_text_settings(cx);
                self.status_message = format!(
                    "terminal font size saved: {}",
                    self.state.settings.terminal_font_size
                );
            }
            Err(error) => self.status_message = format!("failed to save font size: {error}"),
        }
        cx.notify();
    }

    fn set_terminal_scrollback_lines(
        &mut self,
        lines: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_terminal_scrollback_value(&lines) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "terminal scrollback saved: {}",
                    self.state.settings.terminal_scrollback_lines
                );
            }
            Err(error) => self.status_message = format!("failed to save scrollback: {error}"),
        }
        cx.notify();
    }

    fn terminal_config_from_settings(&self) -> TerminalConfig {
        terminal_config_for_settings(&self.state.settings)
    }

    fn apply_terminal_text_settings(&self, cx: &mut Context<Self>) {
        let config = self.terminal_config_from_settings();
        for tab in &self.terminals {
            for slot in &tab.panes {
                let config = config.clone();
                slot.pane.view.update(cx, |terminal, cx| {
                    terminal.update_config(config, cx);
                });
            }
        }
    }

    fn apply_settings_summary(&mut self, settings: SettingsSummary) {
        self.state.settings = settings;
        self.state.remote = self.runtime_service.reload_remote();
        self.state.notifications = self.runtime_service.reload_notifications();
        self.state.power = self
            .runtime_service
            .power_summary(&self.state.settings.sleep_mode);
    }

    fn set_theme(&mut self, theme: String, window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_theme(&theme) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                theme::apply_component_theme_for_name(&self.state.settings.theme, Some(window), cx);
                self.status_message = "theme saved to settings.json".to_string();
            }
            Err(error) => self.status_message = format!("failed to save theme: {error}"),
        }
        cx.notify();
    }

    fn set_theme_color(
        &mut self,
        theme_color: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_theme_color(&theme_color) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message =
                    format!("theme color saved: {}", self.state.settings.theme_color);
            }
            Err(error) => self.status_message = format!("failed to save theme color: {error}"),
        }
        cx.notify();
    }

    fn set_icon_style(&mut self, icon_style: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_icon_style(&icon_style) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message =
                    format!("icon style saved: {}", self.state.settings.icon_style);
            }
            Err(error) => self.status_message = format!("failed to save icon style: {error}"),
        }
        cx.notify();
    }

    fn toggle_dock_badge(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_dock_badge() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "dock badge saved: {}",
                    if self.state.settings.shows_dock_badge {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save dock badge: {error}"),
        }
        cx.notify();
    }

    fn set_language(&mut self, language: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_language(&language) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!("language saved: {}", self.state.settings.language);
            }
            Err(error) => self.status_message = format!("failed to save language: {error}"),
        }
        cx.notify();
    }

    fn set_shell(&mut self, shell: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_shell(&shell) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!("shell saved: {}", self.state.settings.shell);
            }
            Err(error) => self.status_message = format!("failed to save shell: {error}"),
        }
        cx.notify();
    }

    fn toggle_developer_hud(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_developer_hud() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.normalize_selected_ai_provider();
                self.status_message = "developer HUD setting saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save developer HUD: {error}"),
        }
        cx.notify();
    }

    fn set_developer_refresh(
        &mut self,
        seconds: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_developer_refresh(&seconds) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "developer refresh saved: {}",
                    self.state.settings.developer_refresh
                );
            }
            Err(error) => {
                self.status_message = format!("failed to save developer refresh: {error}")
            }
        }
        cx.notify();
    }

    fn toggle_update_enabled(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_update_enabled() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.state.update = self
                    .runtime_service
                    .reload_update(std::env::current_dir().unwrap_or_default());
                self.status_message = format!(
                    "update setting saved: {}",
                    if self.state.settings.update_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save update setting: {error}"),
        }
        cx.notify();
    }

    fn set_statistics_mode(&mut self, mode: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_statistics_mode(&mode) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "AI statistics mode saved: {}",
                    self.state.settings.statistics_mode
                );
            }
            Err(error) => {
                self.status_message = format!("failed to save AI statistics mode: {error}")
            }
        }
        cx.notify();
    }

    fn set_git_refresh(&mut self, seconds: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_git_refresh(&seconds) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message =
                    format!("Git refresh saved: {}", self.state.settings.git_refresh);
            }
            Err(error) => self.status_message = format!("failed to save Git refresh: {error}"),
        }
        cx.notify();
    }

    fn set_ai_refresh(&mut self, seconds: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_ai_refresh(&seconds) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message =
                    format!("AI refresh saved: {}", self.state.settings.ai_refresh);
            }
            Err(error) => self.status_message = format!("failed to save AI refresh: {error}"),
        }
        cx.notify();
    }

    fn set_ai_background_refresh(
        &mut self,
        seconds: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_ai_background_refresh(&seconds) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "AI background refresh saved: {}",
                    self.state.settings.ai_background_refresh
                );
            }
            Err(error) => {
                self.status_message = format!("failed to save AI background refresh: {error}")
            }
        }
        cx.notify();
    }

    fn toggle_pet_enabled(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_pet_enabled() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet setting saved: {}",
                    if self.state.settings.pet_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save pet setting: {error}"),
        }
        cx.notify();
    }

    fn toggle_pet_desktop_widget(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_pet_desktop_widget() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                let enabled = self.state.settings.pet_desktop_widget;
                self.status_message = format!(
                    "desktop pet setting saved: {}",
                    if enabled { "on" } else { "off" }
                );
                if enabled {
                    self.open_desktop_pet_window(cx);
                    return;
                } else {
                    self.close_desktop_pet_window(cx);
                }
            }
            Err(error) => {
                self.status_message = format!("failed to save desktop pet setting: {error}")
            }
        }
        cx.notify();
    }

    fn toggle_pet_static_mode(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_pet_static_mode() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet static mode saved: {}",
                    if self.state.settings.pet_static_mode {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save pet static mode: {error}"),
        }
        cx.notify();
    }

    fn toggle_pet_reminders(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_pet_reminders() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet reminders saved: {}",
                    if self.state.settings.pet_reminders {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save pet reminders: {error}"),
        }
        cx.notify();
    }

    fn set_pet_speech_mode(&mut self, mode: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_pet_speech_mode(&mode) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet speech mode saved: {}",
                    self.state.settings.pet_speech_mode
                );
            }
            Err(error) => self.status_message = format!("failed to save pet speech mode: {error}"),
        }
        cx.notify();
    }

    fn set_pet_speech_frequency(
        &mut self,
        frequency: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_pet_speech_frequency(&frequency) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet speech frequency saved: {}",
                    self.state.settings.pet_speech_frequency
                );
            }
            Err(error) => {
                self.status_message = format!("failed to save pet speech frequency: {error}")
            }
        }
        cx.notify();
    }

    fn toggle_pet_speech_llm_enabled(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_pet_speech_llm_enabled() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet speech LLM saved: {}",
                    if self.state.settings.pet_speech_llm_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save pet speech LLM: {error}"),
        }
        cx.notify();
    }

    fn toggle_pet_speech_quiet_during_work(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.toggle_pet_speech_quiet_during_work() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet speech work-hours setting saved".to_string();
            }
            Err(error) => {
                self.status_message =
                    format!("failed to save pet speech work-hours setting: {error}")
            }
        }
        cx.notify();
    }

    fn toggle_pet_speech_louder_at_night(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_pet_speech_louder_at_night() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet speech night setting saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save pet speech night setting: {error}")
            }
        }
        cx.notify();
    }

    fn toggle_pet_speech_mute_on_fullscreen(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.toggle_pet_speech_mute_on_fullscreen() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet speech fullscreen setting saved".to_string();
            }
            Err(error) => {
                self.status_message =
                    format!("failed to save pet speech fullscreen setting: {error}")
            }
        }
        cx.notify();
    }

    fn toggle_pet_speech_quiet_hours(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_pet_speech_quiet_hours() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet speech quiet-hours setting saved".to_string();
            }
            Err(error) => {
                self.status_message =
                    format!("failed to save pet speech quiet-hours setting: {error}")
            }
        }
        cx.notify();
    }

    fn set_pet_speech_temporary_mute(&mut self, muted: bool, cx: &mut Context<Self>) {
        match self.runtime_service.set_pet_speech_temporary_mute(muted) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet speech temporary mute setting saved".to_string();
            }
            Err(error) => {
                self.status_message =
                    format!("failed to save pet speech temporary mute setting: {error}")
            }
        }
        cx.notify();
    }

    fn normalize_selected_notification_channel(&mut self) {
        let selected_still_exists = self
            .selected_notification_channel_id
            .as_deref()
            .map(|id| {
                self.state
                    .notifications
                    .channels
                    .iter()
                    .any(|channel| channel.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_notification_channel_id = self
                .state
                .notifications
                .channels
                .first()
                .map(|channel| channel.id.clone());
        }
    }

    fn set_notification_channel_enabled(
        &mut self,
        channel_id: String,
        enabled: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_notification_channel_enabled(&channel_id, enabled)
        {
            Ok(notifications) => {
                self.state.notifications = notifications;
                self.selected_notification_channel_id = Some(channel_id);
                self.normalize_selected_notification_channel();
                self.status_message = "notification channel setting saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save notification channel: {error}")
            }
        }
        cx.notify();
    }

    fn update_notification_channel_string(
        &mut self,
        channel_id: String,
        key: &'static str,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .update_notification_channel_string(&channel_id, key, &value)
        {
            Ok(notifications) => {
                self.state.notifications = notifications;
                self.selected_notification_channel_id = Some(channel_id);
                self.normalize_selected_notification_channel();
                self.status_message = "notification channel setting saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save notification channel: {error}")
            }
        }
        cx.notify();
    }

    fn test_notification_channel(
        &mut self,
        channel_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.notification_testing_channel_id.is_some() {
            self.status_message = "notification test is already running".to_string();
            cx.notify();
            return;
        }
        let service = self.runtime_service.clone();
        self.notification_testing_channel_id = Some(channel_id.clone());
        self.selected_notification_channel_id = Some(channel_id.clone());
        self.status_message = "notification test started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_channel_id = channel_id.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.test_notification_channel(&worker_channel_id)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_notification_test_result(channel_id, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_notification_test_result(
        &mut self,
        channel_id: String,
        result: Result<codux_runtime::notification::NotificationDispatchResult, String>,
        cx: &mut Context<Self>,
    ) {
        if self.notification_testing_channel_id.as_deref() == Some(channel_id.as_str()) {
            self.notification_testing_channel_id = None;
        }
        match result {
            Ok(result) => {
                if result.failed.is_empty() {
                    self.status_message = format!("notification test sent: {}", result.sent);
                } else {
                    let failures = result
                        .failed
                        .iter()
                        .map(|failure| format!("{}: {}", failure.id, failure.message))
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.status_message =
                        format!("notification test sent {}, failed: {failures}", result.sent);
                }
            }
            Err(error) => {
                self.status_message = format!("notification test failed: {error}");
            }
        }
        cx.notify();
    }

    fn set_update_channel(
        &mut self,
        channel: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_update_channel(&channel) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.state.update = self
                    .runtime_service
                    .reload_update(std::env::current_dir().unwrap_or_default());
                self.status_message = format!(
                    "update channel saved: {}",
                    self.state.settings.update_channel
                );
            }
            Err(error) => self.status_message = format!("failed to save update channel: {error}"),
        }
        cx.notify();
    }

    fn set_sleep_mode(&mut self, mode: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_sleep_mode(&mode) {
            Ok((settings, power)) => {
                self.apply_settings_summary(settings);
                self.state.power = power;
                self.status_message = format!(
                    "sleep prevention mode saved: {}",
                    self.state.settings.sleep_mode
                );
            }
            Err(error) => self.status_message = format!("failed to save sleep mode: {error}"),
        }
        cx.notify();
    }

    fn select_ai_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = self
            .state
            .settings
            .ai_providers
            .iter()
            .find(|provider| provider.id == provider_id)
        else {
            self.status_message = "AI provider is no longer available".to_string();
            self.normalize_selected_ai_provider();
            cx.notify();
            return;
        };
        self.selected_ai_provider_id = Some(provider.id.clone());
        self.status_message = format!("selected AI provider: {}", provider.display_name);
        cx.notify();
    }

    fn set_git_commit_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_git_commit_provider(&provider_id) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.normalize_selected_ai_provider();
                self.status_message = format!(
                    "Git commit provider saved: {}",
                    self.state.settings.git_commit_provider_id
                );
            }
            Err(error) => {
                self.status_message = format!("failed to set Git commit provider: {error}")
            }
        }
        cx.notify();
    }

    fn set_pet_speech_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_pet_speech_provider(&provider_id) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet LLM provider saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save pet LLM provider: {error}"),
        }
        cx.notify();
    }

    fn set_ai_global_prompt(
        &mut self,
        prompt: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_ai_global_prompt(&prompt) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "AI global prompt saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save AI global prompt: {error}"),
        }
        cx.notify();
    }

    fn set_git_commit_style_rules(
        &mut self,
        rules: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_git_commit_style_rules(&rules) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "Git commit style rules saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save Git commit style rules: {error}")
            }
        }
        cx.notify();
    }

    fn add_ai_provider(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.add_ai_provider("openAICompatible") {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.selected_ai_provider_id = self
                    .state
                    .settings
                    .ai_providers
                    .last()
                    .map(|provider| provider.id.clone());
                self.normalize_selected_ai_provider();
                self.status_message = "AI provider added".to_string();
            }
            Err(error) => self.status_message = format!("failed to add AI provider: {error}"),
        }
        cx.notify();
    }

    fn remove_ai_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.remove_ai_provider(&provider_id) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                if self.ai_provider_testing_id.as_deref() == Some(provider_id.as_str()) {
                    self.ai_provider_testing_id = None;
                }
                self.normalize_selected_ai_provider();
                self.status_message = "AI provider removed".to_string();
            }
            Err(error) => self.status_message = format!("failed to remove AI provider: {error}"),
        }
        cx.notify();
    }

    fn update_ai_provider_string(
        &mut self,
        provider_id: String,
        key: &'static str,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .update_ai_provider_string(&provider_id, key, &value)
        {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.selected_ai_provider_id = Some(provider_id);
                self.normalize_selected_ai_provider();
                self.status_message = "AI provider saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save AI provider: {error}"),
        }
        cx.notify();
    }

    fn set_ai_provider_bool(
        &mut self,
        provider_id: String,
        key: &'static str,
        value: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_ai_provider_bool(&provider_id, key, value)
        {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.selected_ai_provider_id = Some(provider_id);
                self.normalize_selected_ai_provider();
                self.status_message = "AI provider saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save AI provider: {error}"),
        }
        cx.notify();
    }

    fn test_ai_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai_provider_testing_id.is_some() {
            self.status_message = "AI provider test is already running".to_string();
            cx.notify();
            return;
        }
        if !self
            .state
            .settings
            .ai_providers
            .iter()
            .any(|provider| provider.id == provider_id)
        {
            self.status_message = "AI provider not found".to_string();
            cx.notify();
            return;
        }

        let service = self.runtime_service.clone();
        self.ai_provider_testing_id = Some(provider_id.clone());
        self.selected_ai_provider_id = Some(provider_id.clone());
        self.status_message = "AI provider test started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_provider_id = provider_id.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.test_ai_provider(&worker_provider_id)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_ai_provider_test_result(provider_id, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_ai_provider_test_result(
        &mut self,
        provider_id: String,
        result: Result<codux_runtime::llm::LLMProviderTestResult, String>,
        cx: &mut Context<Self>,
    ) {
        if self.ai_provider_testing_id.as_deref() == Some(provider_id.as_str()) {
            self.ai_provider_testing_id = None;
        }
        match result {
            Ok(result) => {
                self.status_message = format!(
                    "AI provider test ok: {} · {}",
                    result.provider_name, result.text
                );
            }
            Err(error) => {
                self.status_message = format!("AI provider test failed: {error}");
            }
        }
        cx.notify();
    }

    fn set_ai_memory_bool(
        &mut self,
        key: &'static str,
        value: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_ai_memory_bool(key, value) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "memory setting saved".to_string();
                prepare_memory_launch_artifacts(&self.state);
            }
            Err(error) => self.status_message = format!("failed to save memory setting: {error}"),
        }
        cx.notify();
    }

    fn set_ai_memory_number(
        &mut self,
        key: &'static str,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_ai_memory_number(key, &value) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "memory setting saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save memory setting: {error}"),
        }
        cx.notify();
    }

    fn set_ai_memory_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_ai_memory_provider(&provider_id) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "memory extraction provider saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save memory provider: {error}"),
        }
        cx.notify();
    }

    fn set_agent_split_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.agent_split_enabled = enabled;
        self.status_message = format!(
            "agent split setting saved: {}",
            if enabled { "on" } else { "off" }
        );
        cx.notify();
    }

    fn record_shortcut(
        &mut self,
        shortcut_id: &'static str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.recording_shortcut_id = Some(shortcut_id.to_string());
        self.status_message = "record shortcut, press Esc to cancel".to_string();
        cx.notify();
    }

    fn reset_shortcut(
        &mut self,
        shortcut_id: &'static str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.reset_shortcut(shortcut_id) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                if self.recording_shortcut_id.as_deref() == Some(shortcut_id) {
                    self.recording_shortcut_id = None;
                }
                self.status_message = "shortcut reset".to_string();
            }
            Err(error) => self.status_message = format!("failed to reset shortcut: {error}"),
        }
        cx.notify();
    }

    fn set_git_commit_tone(&mut self, tone: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_git_commit_tone(&tone) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "Git commit style saved: {}",
                    self.state.settings.git_commit_tone
                );
            }
            Err(error) => self.status_message = format!("failed to save Git commit style: {error}"),
        }
        cx.notify();
    }

    fn set_git_commit_language(
        &mut self,
        language: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_git_commit_language(&language) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "Git commit language saved: {}",
                    self.state.settings.git_commit_language
                );
            }
            Err(error) => {
                self.status_message = format!("failed to save Git commit language: {error}")
            }
        }
        cx.notify();
    }

    fn set_runtime_tool_permission(
        &mut self,
        tool_key: &'static str,
        permission: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_runtime_tool_permission(tool_key, &permission)
        {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
                self.status_message = format!("{tool_key} permission saved");
            }
            Err(error) => {
                self.status_message = format!("failed to save {tool_key} permission: {error}")
            }
        }
        cx.notify();
    }

    fn set_runtime_tool_model(
        &mut self,
        model_key: &'static str,
        model: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_runtime_tool_model(model_key, &model)
        {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
                self.status_message = format!("{model_key} saved");
            }
            Err(error) => self.status_message = format!("failed to save {model_key}: {error}"),
        }
        cx.notify();
    }

    fn set_codex_effort(&mut self, effort: String, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_codex_effort(&effort) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
                self.status_message = format!(
                    "Codex effort saved: {}",
                    self.state.tool_permissions.codex_effort
                );
            }
            Err(error) => self.status_message = format!("failed to save Codex effort: {error}"),
        }
        cx.notify();
    }

    fn normalize_selected_ai_provider(&mut self) {
        let selected_still_exists = self
            .selected_ai_provider_id
            .as_deref()
            .map(|id| {
                self.state
                    .settings
                    .ai_providers
                    .iter()
                    .any(|provider| provider.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_ai_provider_id = self
                .state
                .settings
                .ai_providers
                .first()
                .map(|provider| provider.id.clone());
        }
    }

    fn reload_project_files(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to refresh".to_string();
            cx.notify();
            return;
        };
        let project_name = project.name.clone();
        let project_path = project.path.clone();
        self.state.files = self
            .runtime_service
            .reload_project_files(&project_path, file_directory_option(&self.file_directory));
        self.reset_file_tree_cache();
        self.normalize_selected_file_entry();
        self.status_message = format!(
            "file list reloaded for {}{}",
            project_name,
            current_directory_suffix(&self.file_directory)
        );
        cx.notify();
    }

    fn reset_file_tree_cache(&mut self) {
        self.file_tree_expanded_dirs.clear();
        self.file_tree_children.clear();
    }

    fn file_tree_entry(&self, path: &str) -> Option<FileEntry> {
        self.state
            .files
            .iter()
            .chain(
                self.file_tree_children
                    .values()
                    .flat_map(|children| children.iter()),
            )
            .find(|entry| entry.relative_path == path)
            .cloned()
    }

    fn selected_file_entry(&self) -> Option<FileEntry> {
        self.selected_file_entry
            .as_deref()
            .and_then(|path| self.file_tree_entry(path))
    }

    fn reload_file_tree_directory(&mut self, directory_path: &str) {
        let Some(project) = &self.state.selected_project else {
            return;
        };
        let children = self
            .runtime_service
            .reload_project_files(&project.path, Some(directory_path));
        self.file_tree_children
            .insert(directory_path.to_string(), children);
    }

    fn refresh_file_tree_cache(&mut self) {
        let Some(project) = &self.state.selected_project else {
            self.reset_file_tree_cache();
            return;
        };
        let project_path = project.path.clone();
        self.state.files = self
            .runtime_service
            .reload_project_files(&project_path, file_directory_option(&self.file_directory));
        let expanded = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        self.file_tree_children.clear();
        for directory_path in expanded {
            let children = self
                .runtime_service
                .reload_project_files(&project_path, Some(directory_path.as_str()));
            self.file_tree_children.insert(directory_path, children);
        }
    }

    fn toggle_file_tree_directory(
        &mut self,
        relative_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_tree_expanded_dirs.contains(&relative_path) {
            self.file_tree_expanded_dirs.remove(&relative_path);
            self.status_message = format!("directory collapsed: {relative_path}");
        } else {
            self.file_tree_expanded_dirs.insert(relative_path.clone());
            self.reload_file_tree_directory(&relative_path);
            self.status_message = format!("directory expanded: {relative_path}");
        }
        cx.notify();
    }

    fn open_file_entry(&mut self, entry: FileEntry, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_file_entry = Some(entry.relative_path.clone());
        match entry.kind {
            FileKind::Directory => self.toggle_file_tree_directory(entry.relative_path, window, cx),
            FileKind::File => self.preview_file(entry.relative_path, window, cx),
        }
    }

    fn open_parent_file_directory(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.file_directory.trim().is_empty() {
            self.status_message = "already at project root".to_string();
            cx.notify();
            return;
        }
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to open parent directory".to_string();
            cx.notify();
            return;
        };
        let parent = parent_relative_directory(&self.file_directory);
        self.state.files = self
            .runtime_service
            .reload_project_files(&project.path, file_directory_option(&parent));
        self.file_directory = parent;
        self.selected_file_entry = None;
        self.file_preview = "select a file to preview it".to_string();
        self.file_editable = false;
        self.file_dirty = false;
        self.status_message = format!(
            "directory opened{}",
            current_directory_suffix(&self.file_directory)
        );
        cx.notify();
    }

    fn normalize_selected_file_entry(&mut self) {
        let selected_still_exists = self
            .selected_file_entry
            .as_deref()
            .map(|path| self.file_tree_entry(path).is_some())
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_file_entry = None;
            self.file_editable = false;
            self.file_dirty = false;
        }
    }

    fn selected_file_is_text_file(&self) -> bool {
        let Some(entry_path) = self.selected_file_entry.as_deref() else {
            return false;
        };
        self.file_tree_entry(entry_path)
            .map(|entry| matches!(entry.kind, FileKind::File))
            .unwrap_or(false)
    }

    fn file_search_match_lines(&self) -> Vec<usize> {
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

    fn normalize_file_search_index(&mut self) {
        let count = self.file_search_match_lines().len();
        if count == 0 {
            self.file_search_match_index = 0;
        } else if self.file_search_match_index >= count {
            self.file_search_match_index = count - 1;
        }
    }

    fn open_file_search(&mut self, cx: &mut Context<Self>) {
        self.workspace_view = WorkspaceView::Files;
        self.file_search_open = true;
        self.normalize_file_search_index();
        let count = self.file_search_match_lines().len();
        self.status_message = if self.file_search_query.trim().is_empty() {
            "file search opened".to_string()
        } else {
            format!("file search matches: {count}")
        };
        cx.notify();
    }

    fn close_file_search(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.file_search_open = false;
        self.status_message = "file search closed".to_string();
        cx.notify();
    }

    fn set_file_search_query(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_search_query = value;
        self.file_search_match_index = 0;
        let count = self.file_search_match_lines().len();
        self.status_message = if self.file_search_query.trim().is_empty() {
            "file search query cleared".to_string()
        } else {
            format!("file search matches: {count}")
        };
        cx.notify();
    }

    fn select_next_file_search_match(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let count = self.file_search_match_lines().len();
        if count > 0 {
            self.file_search_match_index = (self.file_search_match_index + 1) % count;
        }
        self.status_message = file_search_status_message(self.file_search_match_index, count);
        cx.notify();
    }

    fn select_previous_file_search_match(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let count = self.file_search_match_lines().len();
        if count > 0 {
            self.file_search_match_index = if self.file_search_match_index == 0 {
                count - 1
            } else {
                self.file_search_match_index - 1
            };
        }
        self.status_message = file_search_status_message(self.file_search_match_index, count);
        cx.notify();
    }

    fn handle_file_editor_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
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
            cx.notify();
        }
        changed
    }

    fn create_project_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.start_file_name_draft(FileNameDraftKind::CreateFile, None, cx);
    }

    fn create_project_directory(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.start_file_name_draft(FileNameDraftKind::CreateDirectory, None, cx);
    }

    fn start_file_name_draft(
        &mut self,
        kind: FileNameDraftKind,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let value = value.unwrap_or_else(|| {
            if kind == FileNameDraftKind::Rename {
                self.selected_file_entry()
                    .map(|entry| entry.name)
                    .unwrap_or_default()
            } else {
                generated_project_child_name(
                    &self.state.files,
                    kind == FileNameDraftKind::CreateDirectory,
                )
            }
        });
        self.file_name_draft_kind = Some(kind);
        self.file_name_draft_value = value;
        self.workspace_view = WorkspaceView::Files;
        self.assistant_panel = Some(AssistantPanel::FileManager);
        self.status_message = match kind {
            FileNameDraftKind::CreateFile => "enter file name".to_string(),
            FileNameDraftKind::CreateDirectory => "enter folder name".to_string(),
            FileNameDraftKind::Rename => "enter new file name".to_string(),
        };
        cx.notify();
    }

    fn set_file_name_draft_value(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_name_draft_value = value;
        cx.notify();
    }

    fn cancel_file_name_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.file_name_draft_kind = None;
        self.file_name_draft_value.clear();
        self.status_message = "file name edit canceled".to_string();
        cx.notify();
    }

    fn confirm_file_name_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.file_name_draft_kind else {
            self.status_message = "no file name edit in progress".to_string();
            cx.notify();
            return;
        };
        let name = self.file_name_draft_value.trim().to_string();
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            self.status_message =
                "file name is required and cannot contain path separators".to_string();
            cx.notify();
            return;
        }

        match kind {
            FileNameDraftKind::CreateFile => self.create_project_file_entry(false, name, cx),
            FileNameDraftKind::CreateDirectory => self.create_project_file_entry(true, name, cx),
            FileNameDraftKind::Rename => self.rename_selected_file_entry_to(name, cx),
        }
    }

    fn create_project_file_entry(&mut self, directory: bool, name: String, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file creation".to_string();
            cx.notify();
            return;
        };
        let project_path = project.path.clone();
        let parent = file_directory_option(&self.file_directory).map(str::to_string);
        let result = if directory {
            self.runtime_service
                .create_project_directory(&project_path, parent.as_deref(), &name)
        } else {
            self.runtime_service
                .create_project_file(&project_path, parent.as_deref(), &name)
        };
        match result {
            Ok(files) => {
                let relative_path = join_relative_child_path(&self.file_directory, &name);
                self.state.files = files;
                self.refresh_file_tree_cache();
                self.selected_file_entry = Some(relative_path.clone());
                self.file_preview = if directory {
                    "directory created".to_string()
                } else {
                    String::new()
                };
                self.file_editable = !directory;
                self.file_dirty = false;
                self.state.git = self.runtime_service.reload_project_git(&project_path);
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.file_name_draft_kind = None;
                self.file_name_draft_value.clear();
                self.status_message = format!(
                    "{} created: {relative_path}",
                    if directory { "directory" } else { "file" }
                );
            }
            Err(error) => {
                self.status_message = format!(
                    "failed to create {}: {error}",
                    if directory { "directory" } else { "file" }
                );
            }
        }
        cx.notify();
    }

    fn delete_selected_file_entry(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file deletion".to_string();
            cx.notify();
            return;
        };
        let Some(entry_path) = self.selected_file_entry.clone() else {
            self.status_message = "no selected file entry to delete".to_string();
            cx.notify();
            return;
        };
        let project_path = project.path.clone();
        let directory = file_directory_option(&self.file_directory).map(str::to_string);
        match self.runtime_service.delete_project_file_entry(
            &project_path,
            &entry_path,
            directory.as_deref(),
        ) {
            Ok(files) => {
                self.state.files = files;
                self.file_tree_expanded_dirs.retain(|path| {
                    path != &entry_path && !path.starts_with(&format!("{entry_path}/"))
                });
                self.refresh_file_tree_cache();
                self.selected_file_entry = None;
                self.file_preview = "select a file to preview it".to_string();
                self.file_editable = false;
                self.file_dirty = false;
                self.state.git = self.runtime_service.reload_project_git(&project_path);
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.status_message = format!("moved file entry to trash: {entry_path}");
            }
            Err(error) => self.status_message = format!("failed to delete file entry: {error}"),
        }
        cx.notify();
    }

    fn save_selected_file_preview(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file save".to_string();
            cx.notify();
            return;
        };
        let Some(entry_path) = self.selected_file_entry.clone() else {
            self.status_message = "no selected file to save".to_string();
            cx.notify();
            return;
        };
        let Some(entry) = self.selected_file_entry() else {
            self.status_message = "selected file is no longer available".to_string();
            self.normalize_selected_file_entry();
            cx.notify();
            return;
        };
        if !matches!(entry.kind, FileKind::File) {
            self.status_message = "directories cannot be saved as text files".to_string();
            cx.notify();
            return;
        }
        let project_path = project.path.clone();
        if !self.file_editable {
            self.status_message = "selected file preview is read-only".to_string();
            cx.notify();
            return;
        }
        let content = self.file_preview.clone();
        match self
            .runtime_service
            .write_project_file(&project_path, &entry_path, &content)
        {
            Ok(preview) => {
                self.file_preview = preview;
                self.file_editable = true;
                self.file_dirty = false;
                self.normalize_file_search_index();
                self.state.files = self.runtime_service.reload_project_files(
                    &project_path,
                    file_directory_option(&self.file_directory),
                );
                self.refresh_file_tree_cache();
                self.selected_file_entry = Some(entry_path.clone());
                self.state.git = self.runtime_service.reload_project_git(&project_path);
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.status_message = format!("file saved: {entry_path}");
            }
            Err(error) => self.status_message = format!("failed to save file: {error}"),
        }
        cx.notify();
    }

    fn rename_selected_file_entry(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.start_file_name_draft(FileNameDraftKind::Rename, None, cx);
    }

    fn rename_selected_file_entry_to(&mut self, new_name: String, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file rename".to_string();
            cx.notify();
            return;
        };
        let Some(entry_path) = self.selected_file_entry.clone() else {
            self.status_message = "no selected file entry to rename".to_string();
            cx.notify();
            return;
        };
        let Some(entry) = self.selected_file_entry() else {
            self.status_message = "selected file entry is no longer available".to_string();
            self.normalize_selected_file_entry();
            cx.notify();
            return;
        };
        let project_path = project.path.clone();
        match self.runtime_service.rename_project_file_entry(
            &project_path,
            &entry_path,
            &new_name,
            file_directory_option(&self.file_directory),
        ) {
            Ok((files, renamed_path)) => {
                self.state.files = files;
                let was_expanded = self.file_tree_expanded_dirs.remove(&entry_path);
                self.file_tree_expanded_dirs
                    .retain(|path| !path.starts_with(&format!("{entry_path}/")));
                if was_expanded {
                    self.file_tree_expanded_dirs.insert(renamed_path.clone());
                }
                self.refresh_file_tree_cache();
                self.selected_file_entry = Some(renamed_path.clone());
                if matches!(entry.kind, FileKind::File) {
                    match self
                        .runtime_service
                        .read_project_file_edit_buffer(&project_path, &renamed_path)
                    {
                        Ok((content, editable)) => {
                            self.file_preview = content;
                            self.file_editable = editable;
                            self.file_dirty = false;
                        }
                        Err(error) => {
                            self.file_preview = format!("failed to reload renamed file: {error}");
                            self.file_editable = false;
                            self.file_dirty = false;
                        }
                    }
                } else {
                    self.file_preview = "directory renamed".to_string();
                    self.file_editable = false;
                    self.file_dirty = false;
                }
                self.state.git = self.runtime_service.reload_project_git(&project_path);
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.file_name_draft_kind = None;
                self.file_name_draft_value.clear();
                self.status_message = format!("renamed file entry: {renamed_path}");
            }
            Err(error) => self.status_message = format!("failed to rename file entry: {error}"),
        }
        cx.notify();
    }

    fn copy_selected_file_entry(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file copy".to_string();
            cx.notify();
            return;
        };
        let Some(entry_path) = self.selected_file_entry.clone() else {
            self.status_message = "no selected file entry to copy".to_string();
            cx.notify();
            return;
        };
        let Some(entry) = self.selected_file_entry() else {
            self.status_message = "selected file entry is no longer available".to_string();
            self.normalize_selected_file_entry();
            cx.notify();
            return;
        };
        let project_path = project.path.clone();
        match self.runtime_service.copy_project_file_entry(
            &project_path,
            &entry_path,
            file_directory_option(&self.file_directory),
        ) {
            Ok((files, copied_path)) => {
                self.state.files = files;
                self.refresh_file_tree_cache();
                self.selected_file_entry = Some(copied_path.clone());
                if matches!(entry.kind, FileKind::File) {
                    match self
                        .runtime_service
                        .read_project_file_edit_buffer(&project_path, &copied_path)
                    {
                        Ok((content, editable)) => {
                            self.file_preview = content;
                            self.file_editable = editable;
                            self.file_dirty = false;
                        }
                        Err(error) => {
                            self.file_preview = format!("failed to load copied file: {error}");
                            self.file_editable = false;
                            self.file_dirty = false;
                        }
                    }
                } else {
                    self.file_preview = "directory copied".to_string();
                    self.file_editable = false;
                    self.file_dirty = false;
                }
                self.state.git = self.runtime_service.reload_project_git(&project_path);
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.status_message = format!("copied file entry: {copied_path}");
            }
            Err(error) => self.status_message = format!("failed to copy file entry: {error}"),
        }
        cx.notify();
    }

    fn import_external_file_entries(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file import".to_string();
            cx.notify();
            return;
        };
        let project_path = project.path.clone();
        let directory = file_directory_option(&self.file_directory).map(str::to_string);
        let selection =
            match self
                .runtime_service
                .localized_open_dialog(LocalizedOpenDialogRequest {
                    title: "导入文件".to_string(),
                    message: "选择要复制到当前项目目录的文件。".to_string(),
                    prompt: "导入".to_string(),
                    default_path: None,
                    filters: Vec::new(),
                    directory: false,
                    multiple: true,
                    can_create_directories: Some(false),
                }) {
                Ok(Some(paths)) if !paths.is_empty() => paths,
                Ok(_) => {
                    self.status_message = "file import canceled".to_string();
                    cx.notify();
                    return;
                }
                Err(error) => {
                    self.status_message = format!("failed to choose files: {error}");
                    cx.notify();
                    return;
                }
            };

        match self.runtime_service.import_external_project_files(
            &project_path,
            selection,
            directory.as_deref(),
        ) {
            Ok((files, selected)) => {
                self.state.files = files;
                self.refresh_file_tree_cache();
                self.selected_file_entry = selected.clone();
                if let Some(path) = selected {
                    match self
                        .runtime_service
                        .read_project_file_edit_buffer(&project_path, &path)
                    {
                        Ok((content, editable)) => {
                            self.file_preview = content;
                            self.file_editable = editable;
                        }
                        Err(_) => {
                            self.file_preview = "external file imported".to_string();
                            self.file_editable = false;
                        }
                    }
                }
                self.file_dirty = false;
                self.state.git = self.runtime_service.reload_project_git(&project_path);
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.status_message = "external file imported".to_string();
            }
            Err(error) => self.status_message = format!("failed to import external file: {error}"),
        }
        cx.notify();
    }

    fn reveal_selected_file_entry(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_selected_file_system_action("reveal", cx);
    }

    fn open_selected_file_entry(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_selected_file_system_action("open", cx);
    }

    fn run_selected_file_system_action(&mut self, action: &str, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = format!("no selected project for file {action}");
            cx.notify();
            return;
        };
        let Some(entry_path) = self.selected_file_entry.clone() else {
            self.status_message = format!("no selected file entry to {action}");
            cx.notify();
            return;
        };
        let result = match action {
            "reveal" => self
                .runtime_service
                .reveal_project_file_entry(&project.path, &entry_path),
            "open" => self
                .runtime_service
                .open_project_file_entry(&project.path, &entry_path),
            _ => Err(format!("unknown file action: {action}")),
        };
        match result {
            Ok(()) => self.status_message = format!("file {action} requested: {entry_path}"),
            Err(error) => self.status_message = format!("failed to {action} file entry: {error}"),
        }
        cx.notify();
    }

    fn reload_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to refresh".to_string();
            cx.notify();
            return;
        };
        let project_name = project.name.clone();
        let project_path = project.path.clone();
        self.state.git = self.runtime_service.reload_project_git(&project_path);
        self.refresh_git_review_for_project(&project_path);
        self.git_expanded_dirs.clear();
        self.git_tree_children.clear();
        self.normalize_selected_git_file();
        self.normalize_selected_git_branch();
        self.status_message = format!("git status reloaded for {project_name}");
        cx.notify();
    }

    fn init_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git init".to_string();
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            cx.notify();
            return;
        }
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let action = "init".to_string();
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: false,
        });
        self.status_message = "Git init started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.init_project_git(&worker_project_path)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_repository_result(
                    project_id,
                    project_path,
                    action,
                    result,
                    false,
                    "Git repository initialized with git2".to_string(),
                    cx,
                );
            });
        })
        .detach();
        cx.notify();
    }

    fn set_git_clone_remote_url(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_clone_remote_url = value;
        cx.notify();
    }

    fn set_git_remote_name(&mut self, value: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.git_remote_name = value;
        cx.notify();
    }

    fn set_git_remote_url(&mut self, value: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.git_remote_url = value;
        cx.notify();
    }

    fn set_git_commit_message(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.git_commit_message == value {
            return;
        }
        self.git_commit_message = value;
        cx.notify();
    }

    fn generate_git_commit_message_with_ai(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message =
                "no selected project for Git commit message generation".to_string();
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            cx.notify();
            return;
        }
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: "aiCommitMessage".to_string(),
            cancellable: false,
        });
        self.status_message = "AI commit message generation started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.generate_project_git_commit_message(&worker_project_path)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_generated_git_commit_message(project_id, project_path, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_generated_git_commit_message(
        &mut self,
        project_id: String,
        project_path: String,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        if self
            .git_running_operation
            .as_ref()
            .is_some_and(|operation| operation.label == "aiCommitMessage")
        {
            self.git_running_operation = None;
        }
        match result {
            Ok(message) => {
                let selected_matches =
                    self.state.selected_project.as_ref().is_some_and(|project| {
                        project.id == project_id && project.path == project_path
                    });
                if selected_matches {
                    self.git_commit_message = message.clone();
                    self.status_message = format!("AI commit message generated: {message}");
                } else {
                    self.status_message =
                        "AI commit message ignored because selected project changed".to_string();
                }
            }
            Err(error) => {
                self.status_message = format!("failed to generate commit message: {error}");
            }
        }
        cx.notify();
    }

    fn clone_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git clone".to_string();
            cx.notify();
            return;
        };
        let remote_url = self.git_clone_remote_url.trim().to_string();
        if remote_url.is_empty() {
            self.status_message = "Git clone failed: remote URL is empty".to_string();
            cx.notify();
            return;
        }
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            cx.notify();
            return;
        }

        let project_id = project.id.clone();
        let project_name = project.name.clone();
        let project_path = project.path.clone();
        let action = "clone".to_string();
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: false,
        });
        self.status_message = format!("Git clone started for {project_name}");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.clone_project_git(&worker_project_path, &remote_url)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_repository_result(
                    project_id,
                    project_path,
                    action,
                    result,
                    true,
                    format!("Git repository cloned for {project_name}"),
                    cx,
                );
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_project_git_repository_result(
        &mut self,
        project_id: String,
        project_path: String,
        action: String,
        result: Result<GitSummary, String>,
        refresh_files: bool,
        success_message: String,
        cx: &mut Context<Self>,
    ) {
        if self
            .git_running_operation
            .as_ref()
            .is_some_and(|operation| operation.label == action)
        {
            self.git_running_operation = None;
        }
        match result {
            Ok(summary) => {
                let selected_matches = self
                    .state
                    .selected_project
                    .as_ref()
                    .is_some_and(|project| project.path == project_path);
                if selected_matches {
                    self.state.git = summary;
                    self.refresh_git_review_for_project(&project_path);
                    self.git_expanded_sections.insert("changed".to_string());
                    self.git_expanded_sections.insert("untracked".to_string());
                    self.git_expanded_dirs.clear();
                    self.git_tree_children.clear();
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    if refresh_files {
                        self.state.files = self.runtime_service.reload_project_files(
                            &project_path,
                            file_directory_option(&self.file_directory),
                        );
                        self.reset_file_tree_cache();
                        self.normalize_selected_file_entry();
                        self.git_clone_remote_url.clear();
                    }
                    self.state.worktrees = self
                        .runtime_service
                        .reload_worktrees(Some(&project_id), Some(&project_path));
                }
                self.status_message = success_message;
            }
            Err(error) => {
                self.status_message = format!("Git {action} failed: {error}");
            }
        }
        cx.notify();
    }

    fn toggle_git_status_section(&mut self, section: &'static str, cx: &mut Context<Self>) {
        if self.git_expanded_sections.contains(section) {
            self.git_expanded_sections.remove(section);
        } else {
            self.git_expanded_sections.insert(section.to_string());
        }
        cx.notify();
    }

    fn toggle_git_status_dir(&mut self, directory_path: String, cx: &mut Context<Self>) {
        if self.git_expanded_dirs.contains(&directory_path) {
            self.git_expanded_dirs.remove(&directory_path);
            cx.notify();
            return;
        }

        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git tree".to_string();
            cx.notify();
            return;
        };

        if !self.git_tree_children.contains_key(&directory_path) {
            match self
                .runtime_service
                .read_project_git_path_status(&project.path, &directory_path)
            {
                Ok(files) => {
                    self.git_tree_children.insert(directory_path.clone(), files);
                }
                Err(error) => {
                    self.status_message = format!("failed to load Git tree: {error}");
                    cx.notify();
                    return;
                }
            }
        }

        self.git_expanded_dirs.insert(directory_path.clone());
        self.status_message = format!("git tree expanded: {directory_path}");
        cx.notify();
    }

    fn select_git_file(&mut self, file_path: String, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git diff".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.read_project_git_review_diff(
            &project.path,
            &file_path,
            self.git_review.base_branch.as_deref(),
        ) {
            Ok(diff) => {
                let content = self.runtime_service.read_project_git_review_file_content(
                    &project.path,
                    &file_path,
                    self.git_review.base_branch.as_deref(),
                );
                self.selected_git_file = Some(file_path.clone());
                self.git_diff_preview = diff;
                self.git_review_content = Some(content);
                self.status_message = format!("diff loaded: {file_path}");
            }
            Err(error) => self.status_message = format!("failed to load diff: {error}"),
        }
        cx.notify();
    }

    fn open_git_diff_window(
        &mut self,
        file_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git diff".to_string();
            cx.notify();
            return;
        };
        if file_path.trim().is_empty() || file_path.ends_with('/') {
            self.status_message = "no Git file selected for diff window".to_string();
            cx.notify();
            return;
        }

        let project_path = project.path.clone();
        let selected_project_id = project.id.clone();
        let selected_project_name = project.name.clone();
        let selected_file = file_path.clone();
        let bounds = Bounds::centered(None, size(px(920.0), px(680.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(format!("Diff - {selected_file}").into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(720.0), px(520.0))),
                ..Default::default()
            },
            move |window, cx| {
                let mut app = CoduxApp::new_settings_window();
                app.window_mode = AppWindowMode::GitDiff;
                app.git_diff_window_path = Some(selected_file.clone());
                match app.runtime_service.read_project_git_review_diff(
                    &project_path,
                    &selected_file,
                    app.git_review.base_branch.as_deref(),
                ) {
                    Ok(diff) => {
                        app.git_diff_window_content = diff;
                        app.git_diff_window_error = None;
                    }
                    Err(error) => {
                        app.git_diff_window_content.clear();
                        app.git_diff_window_error = Some(error);
                    }
                }
                app.state.selected_project = Some(ProjectInfo {
                    id: selected_project_id.clone(),
                    name: selected_project_name.clone(),
                    path: project_path.clone(),
                    exists: true,
                    badge: String::new(),
                    badge_symbol: None,
                    badge_color_hex: None,
                    git_default_push_remote_name: None,
                });
                theme::apply_component_theme_for_name(&app.state.settings.theme, Some(window), cx);
                let view = cx.new(|_| app);
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(_) => format!("Git diff window opened: {file_path}"),
            Err(error) => format!("failed to open Git diff window: {error}"),
        };
        cx.notify();
    }

    fn open_git_diff_window_file(&mut self, file_path: String, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for opening diff file".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .open_project_file_entry(&project.path, &file_path)
        {
            Ok(()) => self.status_message = format!("file open requested: {file_path}"),
            Err(error) => self.status_message = format!("failed to open diff file: {error}"),
        }
        cx.notify();
    }

    fn normalize_selected_git_file(&mut self) {
        let selected_still_exists = self
            .selected_git_file
            .as_deref()
            .map(|path| {
                self.git_review.files.iter().any(|file| file.path == path)
                    || self
                        .state
                        .git
                        .changed_files
                        .iter()
                        .any(|file| file.path == path)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_git_file = None;
            self.git_diff_preview = "select a changed file to preview its diff".to_string();
            self.git_review_content = None;
        }
    }

    fn refresh_git_review_for_project(&mut self, project_path: &str) {
        self.git_review = self
            .runtime_service
            .reload_project_git_review(project_path, self.git_review.base_branch.as_deref());
    }

    fn normalize_selected_git_branch(&mut self) {
        let selected_still_exists = self
            .selected_git_branch
            .as_deref()
            .map(|name| {
                self.state
                    .git
                    .branches
                    .iter()
                    .any(|branch| branch.name == name)
            })
            .unwrap_or(false);
        if selected_still_exists {
            return;
        }
        self.selected_git_branch = self
            .state
            .git
            .branches
            .iter()
            .find(|branch| branch.is_current)
            .or_else(|| self.state.git.branches.first())
            .map(|branch| branch.name.clone());
    }

    fn select_git_branch(
        &mut self,
        branch_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .state
            .git
            .branches
            .iter()
            .any(|branch| branch.name == branch_name)
        {
            self.selected_git_branch = Some(branch_name.clone());
            self.status_message = format!("selected Git branch: {branch_name}");
        } else {
            self.normalize_selected_git_branch();
            self.status_message = "Git branch is no longer available".to_string();
        }
        cx.notify();
    }

    fn stage_selected_git_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.update_selected_git_file_stage(true, cx);
    }

    fn unstage_selected_git_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.update_selected_git_file_stage(false, cx);
    }

    fn stage_git_paths(
        &mut self,
        paths: Vec<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_git_paths_stage(paths, true, cx);
    }

    fn unstage_git_paths(
        &mut self,
        paths: Vec<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_git_paths_stage(paths, false, cx);
    }

    fn discard_selected_git_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git discard".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let Some(file_path) = self.selected_git_file.clone() else {
            self.status_message = "no selected Git file to discard".to_string();
            cx.notify();
            return;
        };
        let worker_file = file_path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("discard:{file_path}"),
                cancellable: false,
            },
            move |service, path| service.discard_project_git_file(&path, &worker_file),
            GitOperationCompletion {
                success_message: format!("discarded Git file: {file_path}"),
                failure_prefix: "failed to discard Git file".to_string(),
                clear_git_diff_preview: true,
                refresh_review: true,
                ..Default::default()
            },
            cx,
        );
    }

    fn discard_git_paths(
        &mut self,
        paths: Vec<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git discard".to_string();
            cx.notify();
            return;
        };
        let paths = normalized_git_action_paths(paths);
        if paths.is_empty() {
            self.status_message = "no Git files to discard".to_string();
            cx.notify();
            return;
        };
        let count = paths.len();
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("discard-batch:{count}"),
                cancellable: false,
            },
            move |service, path| service.discard_project_git_paths(&path, &paths),
            GitOperationCompletion {
                success_message: format!("discarded {count} Git file paths"),
                failure_prefix: "failed to discard Git file paths".to_string(),
                clear_git_diff_preview: true,
                clear_git_tree_cache: true,
                refresh_review: true,
                ..Default::default()
            },
            cx,
        );
    }

    fn append_project_gitignore_path(
        &mut self,
        file_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for .gitignore".to_string();
            cx.notify();
            return;
        };
        let normalized_path = file_path.trim().to_string();
        if normalized_path.is_empty() {
            self.status_message = "no Git path to ignore".to_string();
            cx.notify();
            return;
        }

        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_path = normalized_path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("ignore:{normalized_path}"),
                cancellable: false,
            },
            move |service, path| service.append_project_gitignore(&path, &[worker_path]),
            GitOperationCompletion {
                success_message: format!("added to .gitignore: {normalized_path}"),
                failure_prefix: "failed to update .gitignore".to_string(),
                clear_git_diff_preview: true,
                clear_git_tree_cache: true,
                refresh_review: true,
                ..Default::default()
            },
            cx,
        );
    }

    fn append_project_gitignore_paths(
        &mut self,
        paths: Vec<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for .gitignore".to_string();
            cx.notify();
            return;
        };
        let paths = normalized_git_action_paths(paths);
        if paths.is_empty() {
            self.status_message = "no Git paths to ignore".to_string();
            cx.notify();
            return;
        }

        let count = paths.len();
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("ignore-batch:{count}"),
                cancellable: false,
            },
            move |service, path| service.append_project_gitignore(&path, &paths),
            GitOperationCompletion {
                success_message: format!("added {count} Git paths to .gitignore"),
                failure_prefix: "failed to update .gitignore".to_string(),
                clear_git_diff_preview: true,
                clear_git_tree_cache: true,
                refresh_review: true,
                ..Default::default()
            },
            cx,
        );
    }

    fn update_selected_git_file_stage(&mut self, stage: bool, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git file operation".to_string();
            cx.notify();
            return;
        };
        let project_path = project.path.clone();
        let Some(file_path) = self.selected_git_file.clone() else {
            self.status_message = "no selected Git file".to_string();
            cx.notify();
            return;
        };

        let worker_file = file_path.clone();
        self.start_project_git_operation(
            project.id.clone(),
            project_path,
            GitRunningOperation {
                label: format!("{}:{file_path}", if stage { "stage" } else { "unstage" }),
                cancellable: false,
            },
            move |service, path| {
                if stage {
                    service.stage_project_git_file(&path, &worker_file)
                } else {
                    service.unstage_project_git_file(&path, &worker_file)
                }
            },
            GitOperationCompletion {
                success_message: format!(
                    "{} Git file: {file_path}",
                    if stage { "staged" } else { "unstaged" }
                ),
                failure_prefix: format!(
                    "failed to {} Git file",
                    if stage { "stage" } else { "unstage" }
                ),
                diff_file_to_reload: Some(file_path),
                ..Default::default()
            },
            cx,
        );
    }

    fn update_git_paths_stage(&mut self, paths: Vec<String>, stage: bool, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git file operation".to_string();
            cx.notify();
            return;
        };
        let paths = normalized_git_action_paths(paths);
        if paths.is_empty() {
            self.status_message = "no Git files selected".to_string();
            cx.notify();
            return;
        }

        let count = paths.len();
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let label = if stage { "stage" } else { "unstage" };
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("{label}-batch:{count}"),
                cancellable: false,
            },
            move |service, path| {
                if stage {
                    service.stage_project_git_paths(&path, &paths)
                } else {
                    service.unstage_project_git_paths(&path, &paths)
                }
            },
            GitOperationCompletion {
                success_message: format!(
                    "{} {count} Git file paths",
                    if stage { "staged" } else { "unstaged" }
                ),
                failure_prefix: format!(
                    "failed to {} Git file paths",
                    if stage { "stage" } else { "unstage" }
                ),
                clear_git_tree_cache: true,
                refresh_review: true,
                ..Default::default()
            },
            cx,
        );
    }

    fn commit_staged_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_git_with_action("commit", cx);
    }

    fn commit_and_push_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_git_with_action("commitAndPush", cx);
    }

    fn commit_and_sync_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_git_with_action("commitAndSync", cx);
    }

    fn commit_git_with_action(&mut self, action: &str, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git commit".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let message = self
            .git_commit_message
            .trim()
            .to_string()
            .chars()
            .take(500)
            .collect::<String>();
        let message = if message.is_empty() {
            generated_git_commit_message(&self.state.git)
        } else {
            message
        };
        let action = action.to_string();
        let worker_action = action.clone();
        let worker_message = message.clone();
        let success_message = match action.as_str() {
            "commitAndPush" => format!("committed and pushed staged changes: {message}"),
            "commitAndSync" => format!("committed and synced staged changes: {message}"),
            _ => format!("committed staged changes: {message}"),
        };
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: action.clone(),
                cancellable: false,
            },
            move |service, path| match worker_action.as_str() {
                "commit" => service.commit_project_git(&path, &worker_message),
                "commitAndPush" | "commitAndSync" => {
                    service.commit_project_git_action(&path, &worker_message, &worker_action)
                }
                _ => Err(format!("unknown Git commit action: {worker_action}")),
            },
            GitOperationCompletion {
                success_message,
                failure_prefix: "failed to commit staged changes".to_string(),
                clear_commit_message: true,
                refresh_review: true,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    fn load_last_git_commit_message(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git commit message".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .read_project_git_last_commit_message(&project.path)
        {
            Ok(message) if !message.trim().is_empty() => {
                self.git_commit_message = message;
                self.status_message = "loaded last Git commit message".to_string();
            }
            Ok(_) => {
                self.status_message = "last Git commit has no summary".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to load last Git commit message: {error}");
            }
        }
        cx.notify();
    }

    fn amend_last_git_commit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git amend".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let draft_message = self
            .git_commit_message
            .trim()
            .to_string()
            .chars()
            .take(500)
            .collect::<String>();
        let message = if draft_message.is_empty() {
            match self
                .runtime_service
                .read_project_git_last_commit_message(&project_path)
            {
                Ok(message) if !message.trim().is_empty() => message,
                Ok(_) => {
                    self.status_message = "last Git commit has no summary".to_string();
                    cx.notify();
                    return;
                }
                Err(error) => {
                    self.status_message =
                        format!("failed to load last Git commit message: {error}");
                    cx.notify();
                    return;
                }
            }
        } else {
            draft_message
        };

        let worker_message = message.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: "amend".to_string(),
                cancellable: false,
            },
            move |service, path| service.amend_project_git_last_commit(&path, &worker_message),
            GitOperationCompletion {
                success_message: format!("amended last Git commit: {message}"),
                failure_prefix: "failed to amend last Git commit".to_string(),
                clear_commit_message: true,
                refresh_review: true,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    fn undo_last_git_commit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git undo".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: "undo".to_string(),
                cancellable: false,
            },
            |service, path| service.undo_project_git_last_commit(&path),
            GitOperationCompletion {
                success_message: "undid last Git commit".to_string(),
                failure_prefix: "failed to undo last Git commit".to_string(),
                clear_commit_message: false,
                refresh_review: true,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    fn fetch_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_project_git_remote_action("fetch", cx);
    }

    fn pull_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_project_git_remote_action("pull", cx);
    }

    fn push_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(remote_name) = self
            .state
            .selected_project
            .as_ref()
            .and_then(|project| project.git_default_push_remote_name.clone())
        {
            self.run_project_git_push_remote(&remote_name, cx);
            return;
        }
        self.run_project_git_remote_action("push", cx);
    }

    fn sync_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_project_git_remote_action("sync", cx);
    }

    fn force_push_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_project_git_remote_action("force-push", cx);
    }

    fn run_project_git_remote_action(&mut self, action: &str, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = format!("no selected project for Git {action}");
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git remote operation is already running".to_string();
            cx.notify();
            return;
        }
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let action = action.to_string();
        let worker_action = action.clone();
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: true,
        });
        self.status_message = format!("Git {action} started");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || match worker_action
                .as_str()
            {
                "fetch" => service.fetch_project_git(&worker_project_path),
                "pull" => service.pull_project_git(&worker_project_path),
                "push" => service.push_project_git(&worker_project_path),
                "sync" => service.sync_project_git(&worker_project_path),
                "force-push" => service.force_push_project_git(&worker_project_path),
                _ => Err(format!("unknown Git action: {worker_action}")),
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_remote_result(project_id, project_path, action, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn push_project_git_remote(
        &mut self,
        remote_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_project_git_push_remote(&remote_name, cx);
    }

    fn run_project_git_push_remote(&mut self, remote_name: &str, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git push".to_string();
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git remote operation is already running".to_string();
            cx.notify();
            return;
        }
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let remote_name = remote_name.to_string();
        let action = format!("push:{remote_name}");
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: true,
        });
        self.status_message = format!("Git push to {remote_name} started");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let worker_remote_name = remote_name.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.push_project_git_remote(&worker_project_path, &worker_remote_name)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_remote_result(project_id, project_path, action, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn cancel_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git cancel".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.cancel_project_git(&project.path) {
            Ok(()) => {
                self.status_message = "Git cancel requested".to_string();
            }
            Err(error) => {
                self.status_message = format!("Git cancel failed: {error}");
            }
        }
        cx.notify();
    }

    fn apply_project_git_remote_result(
        &mut self,
        project_id: String,
        project_path: String,
        action: String,
        result: Result<GitSummary, String>,
        cx: &mut Context<Self>,
    ) {
        if self
            .git_running_operation
            .as_ref()
            .is_some_and(|operation| operation.label == action)
        {
            self.git_running_operation = None;
        }
        match result {
            Ok(summary) => {
                let selected_matches = self
                    .state
                    .selected_project
                    .as_ref()
                    .is_some_and(|project| project.path == project_path);
                if selected_matches {
                    self.state.git = summary;
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    self.state.worktrees = self
                        .runtime_service
                        .reload_worktrees(Some(&project_id), Some(&project_path));
                }
                self.status_message = format!("Git {} completed", git_remote_action_label(&action));
            }
            Err(error) => {
                self.status_message =
                    format!("Git {} failed: {error}", git_remote_action_label(&action));
            }
        }
        cx.notify();
    }

    fn start_project_git_operation(
        &mut self,
        project_id: String,
        project_path: String,
        operation: GitRunningOperation,
        action: impl FnOnce(RuntimeService, String) -> Result<GitSummary, String> + Send + 'static,
        completion: GitOperationCompletion,
        cx: &mut Context<Self>,
    ) {
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            cx.notify();
            return;
        }

        let operation_label = operation.label.clone();
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(operation);
        self.status_message = format!("Git {operation_label} started");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                action(service, worker_project_path)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_operation_result(
                    project_id,
                    project_path,
                    operation_label,
                    result,
                    completion,
                    cx,
                );
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_project_git_operation_result(
        &mut self,
        project_id: String,
        project_path: String,
        operation_label: String,
        result: Result<GitSummary, String>,
        completion: GitOperationCompletion,
        cx: &mut Context<Self>,
    ) {
        if self
            .git_running_operation
            .as_ref()
            .is_some_and(|operation| operation.label == operation_label)
        {
            self.git_running_operation = None;
        }

        match result {
            Ok(summary) => {
                let selected_matches = self
                    .state
                    .selected_project
                    .as_ref()
                    .is_some_and(|project| project.path == project_path);
                if selected_matches {
                    if completion.reload_state {
                        self.state = self.runtime_service.reload_state();
                    }
                    self.state.git = summary;
                    if completion.clear_commit_message {
                        self.git_commit_message.clear();
                    }
                    if completion.clear_remote_url {
                        self.git_remote_url.clear();
                    }
                    if completion.clear_selected_branch {
                        self.selected_git_branch = None;
                    }
                    if let Some(branch) = completion.selected_branch.clone() {
                        self.selected_git_branch = Some(branch);
                    }
                    if completion.refresh_review {
                        self.refresh_git_review_for_project(&project_path);
                    }
                    if completion.clear_git_tree_cache {
                        self.git_expanded_dirs.clear();
                        self.git_tree_children.clear();
                    }
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    if completion.clear_git_diff_preview {
                        self.git_diff_preview =
                            "select a changed file to preview its diff".to_string();
                        self.git_review_content = None;
                    } else if let Some(file_path) = completion.diff_file_to_reload.as_deref()
                        && self.selected_git_file.is_some()
                    {
                        self.git_diff_preview = self
                            .runtime_service
                            .read_project_git_review_diff(
                                &project_path,
                                file_path,
                                self.git_review.base_branch.as_deref(),
                            )
                            .unwrap_or_else(|error| format!("failed to reload diff: {error}"));
                        self.git_review_content =
                            Some(self.runtime_service.read_project_git_review_file_content(
                                &project_path,
                                file_path,
                                self.git_review.base_branch.as_deref(),
                            ));
                    }
                    self.state.worktrees = self
                        .runtime_service
                        .reload_worktrees(Some(&project_id), Some(&project_path));
                }
                self.status_message = completion.success_message;
            }
            Err(error) => {
                self.status_message = format!("{}: {error}", completion.failure_prefix);
            }
        }
        cx.notify();
    }

    fn set_project_default_push_remote(
        &mut self,
        remote_name: Option<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for default Git remote".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        match self.runtime_service.project_set_default_push_remote(
            ProjectDefaultPushRemoteRequest {
                project_id,
                remote_name: remote_name.clone(),
            },
        ) {
            Ok(_) => {
                self.state = self.runtime_service.reload_state();
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.normalize_selected_ai_session();
                self.normalize_selected_runtime_session();
                self.normalize_selected_ssh_profile();
                self.status_message = match remote_name {
                    Some(remote_name) => format!("default Git push remote saved: {remote_name}"),
                    None => "default Git push remote cleared".to_string(),
                };
            }
            Err(error) => {
                self.status_message = format!("failed to save default Git push remote: {error}");
            }
        }
        cx.notify();
    }

    fn add_project_git_remote(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git remote".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let remote_name = self.git_remote_name.trim().to_string();
        let remote_url = self.git_remote_url.trim().to_string();
        let worker_remote_name = remote_name.clone();
        let worker_remote_url = remote_url.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("add-remote:{remote_name}"),
                cancellable: false,
            },
            move |service, path| {
                service.add_project_git_remote(&path, &worker_remote_name, &worker_remote_url)
            },
            GitOperationCompletion {
                success_message: format!("Git remote added: {remote_name}"),
                failure_prefix: "failed to add Git remote".to_string(),
                clear_remote_url: true,
                ..Default::default()
            },
            cx,
        );
    }

    fn remove_project_git_remote(
        &mut self,
        remote_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git remote".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let default_remote = project.git_default_push_remote_name.clone();
        let clears_default_remote = default_remote.as_deref() == Some(remote_name.as_str());
        let worker_project_id = project_id.clone();
        let worker_remote_name = remote_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("remove-remote:{remote_name}"),
                cancellable: false,
            },
            move |service, path| {
                let summary = service.remove_project_git_remote(&path, &worker_remote_name)?;
                if clears_default_remote {
                    let _ =
                        service.project_set_default_push_remote(ProjectDefaultPushRemoteRequest {
                            project_id: worker_project_id,
                            remote_name: None,
                        });
                }
                Ok(summary)
            },
            GitOperationCompletion {
                success_message: format!("Git remote removed: {remote_name}"),
                failure_prefix: "failed to remove Git remote".to_string(),
                reload_state: clears_default_remote,
                ..Default::default()
            },
            cx,
        );
    }

    fn checkout_selected_git_branch(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git checkout".to_string();
            cx.notify();
            return;
        };
        let Some(branch_name) = self.selected_git_branch.clone() else {
            self.status_message = "no selected Git branch".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_branch = branch_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("checkout:{branch_name}"),
                cancellable: false,
            },
            move |service, path| service.checkout_project_git_branch(&path, &worker_branch),
            GitOperationCompletion {
                success_message: format!("checked out Git branch: {branch_name}"),
                failure_prefix: "Git checkout failed".to_string(),
                clear_commit_message: false,
                refresh_review: false,
                clear_selected_branch: false,
                selected_branch: Some(branch_name),
                ..Default::default()
            },
            cx,
        );
    }

    fn checkout_git_remote_branch(
        &mut self,
        remote_branch: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git remote checkout".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_branch = remote_branch.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("checkout-remote:{remote_branch}"),
                cancellable: false,
            },
            move |service, path| service.checkout_project_git_remote_branch(&path, &worker_branch),
            GitOperationCompletion {
                success_message: format!("checked out remote Git branch: {remote_branch}"),
                failure_prefix: "Git remote checkout failed".to_string(),
                clear_commit_message: false,
                refresh_review: false,
                clear_selected_branch: true,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    fn push_project_git_remote_branch(
        &mut self,
        remote_branch: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git remote branch push".to_string();
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git remote operation is already running".to_string();
            cx.notify();
            return;
        }

        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let local_branch = self.state.git.branch.trim().to_string();
        let local_branch = if local_branch.is_empty() || local_branch == "HEAD" {
            None
        } else {
            Some(local_branch)
        };
        let action = format!("push-branch:{remote_branch}");
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: true,
        });
        self.status_message = format!("Git push to {remote_branch} started");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let worker_remote_branch = remote_branch.clone();
            let worker_local_branch = local_branch.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.push_project_git_remote_branch(
                    &worker_project_path,
                    &worker_remote_branch,
                    worker_local_branch.as_deref(),
                )
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_remote_result(project_id, project_path, action, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn checkout_git_commit(
        &mut self,
        commit: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_git_commit_history_action("checkout", &commit, cx);
    }

    fn revert_git_commit(&mut self, commit: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_git_commit_history_action("revert", &commit, cx);
    }

    fn restore_git_commit(&mut self, commit: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_git_commit_history_action("restore", &commit, cx);
    }

    fn run_git_commit_history_action(
        &mut self,
        action: &str,
        commit: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = format!("no selected project for Git {action}");
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let action = action.to_string();
        let commit = commit.to_string();
        let worker_action = action.clone();
        let worker_commit = commit.clone();
        let success_message = match action.as_str() {
            "checkout" => format!("checked out Git commit: {commit}"),
            "revert" => format!("reverted Git commit: {commit}"),
            "restore" => format!("restored Git branch to commit: {commit}"),
            _ => format!("Git history action completed: {commit}"),
        };
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("{action}:{commit}"),
                cancellable: false,
            },
            move |service, path| match worker_action.as_str() {
                "checkout" => service.checkout_project_git_commit(&path, &worker_commit),
                "revert" => service.revert_project_git_commit(&path, &worker_commit),
                "restore" => service.restore_project_git_commit(&path, &worker_commit, false),
                _ => Err(format!("unknown Git history action: {worker_action}")),
            },
            GitOperationCompletion {
                success_message,
                failure_prefix: format!("Git {action} commit failed"),
                clear_commit_message: false,
                refresh_review: true,
                clear_selected_branch: action == "checkout",
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    fn create_git_branch(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git branch creation".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let branch_name = generated_git_branch_name();
        let worker_branch = branch_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("create-branch:{branch_name}"),
                cancellable: false,
            },
            move |service, path| service.create_project_git_branch(&path, &worker_branch, true),
            GitOperationCompletion {
                success_message: format!("created and checked out Git branch: {branch_name}"),
                failure_prefix: "Git branch creation failed".to_string(),
                clear_commit_message: false,
                refresh_review: false,
                clear_selected_branch: false,
                selected_branch: Some(branch_name),
                ..Default::default()
            },
            cx,
        );
    }

    fn create_git_branch_from(
        &mut self,
        from_branch: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git branch creation".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let branch_name = generated_git_branch_name();
        let worker_branch = branch_name.clone();
        let worker_from_branch = from_branch.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("create-branch:{branch_name}"),
                cancellable: false,
            },
            move |service, path| {
                service.create_project_git_branch_from(
                    &path,
                    &worker_branch,
                    Some(&worker_from_branch),
                    true,
                )
            },
            GitOperationCompletion {
                success_message: format!("created Git branch {branch_name} from {from_branch}"),
                failure_prefix: format!("Git branch creation from {from_branch} failed"),
                clear_commit_message: false,
                refresh_review: false,
                clear_selected_branch: false,
                selected_branch: Some(branch_name),
                ..Default::default()
            },
            cx,
        );
    }

    fn merge_git_branch(
        &mut self,
        branch_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git merge".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_branch = branch_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("merge:{branch_name}"),
                cancellable: false,
            },
            move |service, path| service.merge_project_git_branch(&path, &worker_branch, false),
            GitOperationCompletion {
                success_message: format!("merged Git branch: {branch_name}"),
                failure_prefix: format!("Git merge {branch_name} failed"),
                clear_commit_message: false,
                refresh_review: true,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    fn squash_merge_git_branch(
        &mut self,
        branch_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git squash merge".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_branch = branch_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("squash-merge:{branch_name}"),
                cancellable: false,
            },
            move |service, path| service.merge_project_git_branch(&path, &worker_branch, true),
            GitOperationCompletion {
                success_message: format!("squash merged Git branch: {branch_name}"),
                failure_prefix: format!("Git squash merge {branch_name} failed"),
                clear_commit_message: false,
                refresh_review: true,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    fn delete_selected_git_branch(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git branch deletion".to_string();
            cx.notify();
            return;
        };
        let Some(branch_name) = self.selected_git_branch.clone() else {
            self.status_message = "no selected Git branch to delete".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_branch = branch_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("delete-branch:{branch_name}"),
                cancellable: false,
            },
            move |service, path| service.delete_project_git_branch(&path, &worker_branch, false),
            GitOperationCompletion {
                success_message: format!("deleted Git branch: {branch_name}"),
                failure_prefix: "Git branch deletion failed".to_string(),
                clear_commit_message: false,
                refresh_review: false,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    fn reload_ai_history(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.refresh_ai_history_summaries_for_selected_project() {
            Ok(project_name) => {
                self.status_message = format!("AI history reloaded for {project_name}");
            }
            Err(error) => {
                self.status_message = error;
            }
        }
        cx.notify();
    }

    fn refresh_ai_history_summaries_for_selected_project(&mut self) -> Result<String, String> {
        let Some(project) = &self.state.selected_project else {
            return Err("no selected project to refresh".to_string());
        };
        let selected_project = ai_history_project_request(project);
        let project_name = selected_project.name.clone();
        let projects = ai_history_project_requests(&self.state.projects);

        let mut errors = Vec::new();
        match self
            .runtime_service
            .indexed_project_ai_history_summary(selected_project.clone())
        {
            Ok(state) => {
                if let Some(snapshot) = state.snapshot {
                    self.state.ai_history = normalized_ai_history_snapshot_to_summary(snapshot);
                }
            }
            Err(error) => {
                errors.push(format!("indexed project AI history failed: {error}"));
            }
        }
        match self
            .runtime_service
            .indexed_global_ai_history_summary(projects)
        {
            Ok(snapshot) => {
                self.state.ai_global_history =
                    normalized_global_ai_history_snapshot_to_summary(snapshot);
            }
            Err(error) => {
                self.state.ai_global_history = self.runtime_service.reload_global_ai_history();
                self.state.ai_global_history.error = Some(error);
                errors.push("indexed global AI history failed".to_string());
            }
        }
        if self.state.ai_history.sessions.is_empty() {
            self.state.ai_history = self
                .runtime_service
                .reload_project_ai_history(&selected_project.path);
        }
        self.normalize_selected_ai_session();
        self.reload_selected_ai_session_detail();
        if errors.is_empty() {
            Ok(project_name)
        } else {
            Ok(format!("{project_name} ({})", errors.join("; ")))
        }
    }

    fn select_ai_session(
        &mut self,
        session_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((session_id, session_title)) = self
            .state
            .ai_history
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .map(|session| (session.id.clone(), session.title.clone()))
        else {
            self.status_message = "AI session is no longer available".to_string();
            self.normalize_selected_ai_session();
            cx.notify();
            return;
        };
        self.selected_ai_session_id = Some(session_id);
        self.reload_selected_ai_session_detail();
        self.status_message = format!("selected AI session: {session_title}");
        cx.notify();
    }

    fn selected_ai_session(&self) -> Option<&AISessionSummary> {
        self.selected_ai_session_id
            .as_deref()
            .and_then(|id| {
                self.state
                    .ai_history
                    .sessions
                    .iter()
                    .find(|session| session.id == id)
            })
            .or_else(|| self.state.ai_history.sessions.first())
    }

    fn normalize_selected_ai_session(&mut self) {
        let selected_still_exists = self
            .selected_ai_session_id
            .as_deref()
            .map(|id| {
                self.state
                    .ai_history
                    .sessions
                    .iter()
                    .any(|session| session.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_ai_session_id = self
                .state
                .ai_history
                .sessions
                .first()
                .map(|session| session.id.clone());
        }
        self.reload_selected_ai_session_detail();
    }

    fn reload_selected_ai_session_detail(&mut self) {
        let Some(project) = self.state.selected_project.as_ref() else {
            self.state.ai_session_detail = None;
            return;
        };
        let Some(session_id) = self.selected_ai_session_id.as_deref() else {
            self.state.ai_session_detail = None;
            return;
        };
        self.state.ai_session_detail = Some(
            self.runtime_service
                .reload_project_ai_session_detail(&project.path, session_id),
        );
    }

    fn remove_selected_ai_session(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for AI session removal".to_string();
            cx.notify();
            return;
        };
        let project_request = ai_history_project_request(project);
        let Some(session) = self.selected_ai_session().cloned() else {
            self.status_message = "no AI session to remove".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .remove_indexed_ai_session(project_request, session.id.clone())
        {
            Ok(state) => {
                if let Some(snapshot) = state.snapshot {
                    self.state.ai_history = normalized_ai_history_snapshot_to_summary(snapshot);
                }
                self.refresh_ai_global_history_summary();
                self.selected_ai_session_id = None;
                self.normalize_selected_ai_session();
                self.reload_selected_ai_session_detail();
                self.status_message = "selected AI session removed from index".to_string();
            }
            Err(error) => self.status_message = format!("failed to remove AI session: {error}"),
        }
        cx.notify();
    }

    fn rename_selected_ai_session_to(
        &mut self,
        title: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = title.trim().to_string();
        if title.is_empty() {
            self.status_message = "AI session title cannot be empty".to_string();
            cx.notify();
            return;
        }
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for AI session rename".to_string();
            cx.notify();
            return;
        };
        let project_request = ai_history_project_request(project);
        let Some(session) = self.selected_ai_session().cloned() else {
            self.status_message = "no AI session to rename".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.rename_indexed_ai_session(
            project_request,
            session.id.clone(),
            title.clone(),
        ) {
            Ok(state) => {
                if let Some(snapshot) = state.snapshot {
                    self.state.ai_history = normalized_ai_history_snapshot_to_summary(snapshot);
                }
                self.refresh_ai_global_history_summary();
                self.selected_ai_session_id = Some(session.id);
                self.reload_selected_ai_session_detail();
                self.status_message = format!("AI session renamed: {title}");
            }
            Err(error) => self.status_message = format!("failed to rename AI session: {error}"),
        }
        cx.notify();
    }

    fn refresh_ai_global_history_summary(&mut self) {
        let projects = ai_history_project_requests(&self.state.projects);
        match self
            .runtime_service
            .indexed_global_ai_history_summary(projects)
        {
            Ok(snapshot) => {
                self.state.ai_global_history =
                    normalized_global_ai_history_snapshot_to_summary(snapshot);
            }
            Err(error) => {
                self.state.ai_global_history = self.runtime_service.reload_global_ai_history();
                self.state.ai_global_history.error = Some(error);
            }
        }
    }

    fn restore_selected_ai_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for AI session restore".to_string();
            cx.notify();
            return;
        };
        let project_name = project.name.clone();
        let Some(session) = self.selected_ai_session().cloned() else {
            self.status_message = "no AI session to restore".to_string();
            cx.notify();
            return;
        };
        prepare_memory_launch_artifacts(&self.state);
        self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
        let command = ai_session_restore_command(&session);
        self.send_to_active_terminal(&format!("{command}\n"), cx);
        if let Some(view) = self.active_terminal_view() {
            view.read(cx).focus_handle().focus(window, cx);
        }
        self.status_message = format!("restore sent for {} in {}", session.title, project_name);
    }

    fn reload_memory(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.memory = self.runtime_service.reload_memory(
            self.state
                .selected_project
                .as_ref()
                .map(|project| project.id.as_str()),
        );
        self.reload_memory_manager_snapshot();
        self.normalize_selected_memory_entry();
        self.normalize_selected_memory_summary();
        self.status_message = "memory summary reloaded".to_string();
        cx.notify();
    }

    fn process_memory_sessions_now(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.memory_processing {
            self.status_message = "memory processing is already running".to_string();
            cx.notify();
            return;
        }

        let service = self.runtime_service.clone();
        self.memory_processing = true;
        self.status_message = "memory processing started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn(async move {
                service.process_memory_sessions_now().await
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            let _ = this.update(cx, |app, cx| {
                app.apply_memory_processing_result(result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_memory_processing_result(
        &mut self,
        result: Result<MemoryExtractionStatusSnapshot, String>,
        cx: &mut Context<Self>,
    ) {
        self.memory_processing = false;
        match result {
            Ok(status) => {
                let history_refresh = self.refresh_ai_history_summaries_for_selected_project();
                self.state.memory = self.runtime_service.reload_memory(
                    self.state
                        .selected_project
                        .as_ref()
                        .map(|project| project.id.as_str()),
                );
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_entry();
                self.normalize_selected_memory_summary();
                self.status_message = match history_refresh {
                    Ok(project_name) => format!(
                        "memory indexed for {project_name} · checked {} · enqueued {} · pending {}",
                        status.checked_count, status.enqueued_count, status.pending_count
                    ),
                    Err(error) => format!(
                        "memory indexed · checked {} · enqueued {} · pending {} · {error}",
                        status.checked_count, status.enqueued_count, status.pending_count
                    ),
                };
            }
            Err(error) => self.status_message = format!("failed to process memory: {error}"),
        }
        cx.notify();
    }

    fn cancel_memory_extraction_queue(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.cancel_memory_extraction_queue() {
            Ok(status) => {
                self.state.memory = self.runtime_service.reload_memory(
                    self.state
                        .selected_project
                        .as_ref()
                        .map(|project| project.id.as_str()),
                );
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_entry();
                self.normalize_selected_memory_summary();
                self.status_message = format!(
                    "memory queue cancelled · pending {} · running {}",
                    status.pending_count, status.running_count
                );
            }
            Err(error) => self.status_message = format!("failed to cancel memory queue: {error}"),
        }
        cx.notify();
    }

    fn select_memory_entry(
        &mut self,
        entry_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .state
            .memory
            .recent_entries
            .iter()
            .chain(self.state.memory_manager.entries.iter())
            .find(|entry| entry.id == entry_id)
        else {
            self.status_message = "memory entry is no longer available".to_string();
            self.normalize_selected_memory_entry();
            cx.notify();
            return;
        };
        self.selected_memory_entry_id = Some(entry.id.clone());
        self.status_message = format!("selected memory: {}", entry.content);
        cx.notify();
    }

    fn archive_selected_memory_entry(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.update_selected_memory_status("archived", cx);
    }

    fn restore_selected_memory_entry(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.update_selected_memory_status("active", cx);
    }

    fn delete_selected_memory_entry(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry_id) = self.selected_memory_entry_id.clone().or_else(|| {
            self.state
                .memory_manager
                .entries
                .first()
                .map(|entry| entry.id.clone())
        }) else {
            self.status_message = "no memory entry selected".to_string();
            cx.notify();
            return;
        };
        let project_id = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str());
        match self
            .runtime_service
            .delete_memory_entry(project_id, &entry_id)
        {
            Ok(memory) => {
                self.state.memory = memory;
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_entry();
                self.status_message = "memory entry deleted".to_string();
            }
            Err(error) => self.status_message = format!("failed to delete memory: {error}"),
        }
        cx.notify();
    }

    fn delete_selected_memory_summary(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(summary_id) = self.selected_memory_summary_id.clone().or_else(|| {
            self.state
                .memory_manager
                .summaries
                .first()
                .map(|summary| summary.id.clone())
        }) else {
            self.status_message = "no memory summary selected".to_string();
            cx.notify();
            return;
        };
        let project_id = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str());
        match self
            .runtime_service
            .delete_memory_summary(project_id, &summary_id)
        {
            Ok(memory) => {
                self.state.memory = memory;
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_summary();
                self.status_message = "memory summary deleted".to_string();
            }
            Err(error) => self.status_message = format!("failed to delete memory summary: {error}"),
        }
        cx.notify();
    }

    fn update_memory_summary_content(
        &mut self,
        summary_id: String,
        content: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let content = content.trim().to_string();
        if content.is_empty() {
            self.status_message = "memory summary content cannot be empty".to_string();
            cx.notify();
            return;
        }
        match self
            .runtime_service
            .update_memory_summary(MemorySummaryUpdateRequest {
                summary_id: summary_id.clone(),
                content,
                max_versions: Some(20),
            }) {
            Ok(_) => {
                self.selected_memory_summary_id = Some(summary_id);
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_summary();
                self.status_message = "memory summary updated".to_string();
            }
            Err(error) => self.status_message = format!("failed to update memory summary: {error}"),
        }
        cx.notify();
    }

    fn delete_selected_memory_project_profile(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_id) = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            self.status_message = "no selected project profile".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .delete_memory_project_profile(&project_id)
        {
            Ok(memory) => {
                self.state.memory = memory;
                self.reload_memory_manager_snapshot();
                self.status_message = "memory project profile deleted".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to delete project profile: {error}")
            }
        }
        cx.notify();
    }

    fn delete_selected_memory_project(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project_id) = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            self.status_message = "no selected project memory".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.delete_memory_project(&project_id) {
            Ok(memory) => {
                self.state.memory = memory;
                self.selected_memory_entry_id = None;
                self.selected_memory_summary_id = None;
                self.reload_memory_manager_snapshot();
                self.status_message = "project memory deleted".to_string();
            }
            Err(error) => self.status_message = format!("failed to delete project memory: {error}"),
        }
        cx.notify();
    }

    fn migrate_selected_memory_project_to(
        &mut self,
        to_project_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(from_project_id) = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            self.status_message = "no selected project memory to migrate".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .migrate_memory_project(MemoryProjectMigrationRequest {
                from_project_id: from_project_id.clone(),
                to_project_id: to_project_id.clone(),
                overwrite: false,
            }) {
            Ok(memory) => {
                self.state.memory = memory;
                self.selected_memory_entry_id = None;
                self.selected_memory_summary_id = None;
                self.reload_memory_manager_snapshot();
                self.status_message =
                    format!("project memory migrated from {from_project_id} to {to_project_id}");
            }
            Err(error) => {
                self.status_message = format!("failed to migrate project memory: {error}")
            }
        }
        cx.notify();
    }

    fn refresh_selected_memory_project_profile(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_id) = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            self.status_message = "no selected project profile".to_string();
            cx.notify();
            return;
        };
        let service = self.runtime_service.clone();
        self.status_message = "memory project profile refresh started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn(async move {
                service
                    .force_refresh_memory_project_profile_with_llm(&project_id)
                    .await
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_memory_project_profile_refresh_result(result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_memory_project_profile_refresh_result(
        &mut self,
        result: Result<MemoryProjectProfileRefreshResult, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(result) => {
                self.state.memory = self.runtime_service.reload_memory(
                    self.state
                        .selected_project
                        .as_ref()
                        .map(|project| project.id.as_str()),
                );
                self.reload_memory_manager_snapshot();
                self.status_message = if result.used_llm {
                    format!(
                        "memory project profile refreshed with LLM · {} chars",
                        result.profile.content.chars().count()
                    )
                } else {
                    format!(
                        "memory project profile refreshed locally · {} chars{}",
                        result.profile.content.chars().count(),
                        result
                            .fallback_reason
                            .as_ref()
                            .map(|reason| format!(" · {reason}"))
                            .unwrap_or_default()
                    )
                };
            }
            Err(error) => {
                self.status_message = format!("failed to refresh project profile: {error}")
            }
        }
        cx.notify();
    }

    fn set_memory_manager_tab(&mut self, tab: MemoryManagerTab, cx: &mut Context<Self>) {
        self.memory_manager_tab = tab;
        self.reload_memory_manager_snapshot();
        self.normalize_selected_memory_entry();
        self.normalize_selected_memory_summary();
        self.status_message = format!("memory manager tab: {}", tab.as_str());
        cx.notify();
    }

    fn update_selected_memory_status(&mut self, status: &str, cx: &mut Context<Self>) {
        let Some(entry_id) = self.selected_memory_entry_id.clone().or_else(|| {
            self.state
                .memory
                .recent_entries
                .first()
                .map(|entry| entry.id.clone())
        }) else {
            self.status_message = "no memory entry selected".to_string();
            cx.notify();
            return;
        };
        let project_id = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str());
        let result = if status == "archived" {
            self.runtime_service
                .archive_memory_entry(project_id, &entry_id)
        } else {
            self.runtime_service
                .restore_memory_entry(project_id, &entry_id)
        };
        match result {
            Ok(memory) => {
                self.state.memory = memory;
                self.selected_memory_entry_id = Some(entry_id);
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_entry();
                self.status_message = format!("memory entry set to {status}");
            }
            Err(error) => self.status_message = format!("failed to update memory: {error}"),
        }
        cx.notify();
    }

    fn normalize_selected_memory_entry(&mut self) {
        let selected_still_exists = self
            .selected_memory_entry_id
            .as_deref()
            .map(|id| {
                self.state
                    .memory
                    .recent_entries
                    .iter()
                    .any(|entry| entry.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_memory_entry_id = self
                .state
                .memory_manager
                .entries
                .first()
                .map(|entry| entry.id.clone());
        }
    }

    fn normalize_selected_memory_summary(&mut self) {
        let selected_still_exists = self
            .selected_memory_summary_id
            .as_deref()
            .map(|id| {
                self.state
                    .memory_manager
                    .summaries
                    .iter()
                    .any(|summary| summary.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_memory_summary_id = self
                .state
                .memory_manager
                .summaries
                .first()
                .map(|summary| summary.id.clone());
        }
    }

    fn reload_memory_manager_snapshot(&mut self) {
        let project_id = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str());
        self.state.memory_manager = self.runtime_service.reload_memory_manager(
            &self.state.projects,
            "project",
            project_id,
            self.memory_manager_tab.as_str(),
        );
    }

    fn selected_runtime_session(&self) -> Option<&RuntimeSessionSummary> {
        self.selected_runtime_terminal_id
            .as_deref()
            .and_then(|id| {
                self.state
                    .runtime_events
                    .sessions
                    .iter()
                    .find(|session| session.terminal_id == id)
            })
            .or_else(|| self.state.runtime_events.sessions.first())
    }

    fn normalize_selected_runtime_session(&mut self) {
        let selected_still_exists = self
            .selected_runtime_terminal_id
            .as_deref()
            .map(|id| {
                self.state
                    .runtime_events
                    .sessions
                    .iter()
                    .any(|session| session.terminal_id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_runtime_terminal_id = self
                .state
                .runtime_events
                .sessions
                .first()
                .map(|session| session.terminal_id.clone());
        }
    }

    fn select_runtime_session(
        &mut self,
        terminal_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self
            .state
            .runtime_events
            .sessions
            .iter()
            .find(|session| session.terminal_id == terminal_id)
        else {
            self.status_message = "runtime session is no longer available".to_string();
            self.normalize_selected_runtime_session();
            cx.notify();
            return;
        };
        self.selected_runtime_terminal_id = Some(session.terminal_id.clone());
        let matched_terminal_id = self
            .terminals
            .iter()
            .find(|tab| {
                tab.panes.iter().any(|slot| {
                    slot.launch_context
                        .as_ref()
                        .and_then(|context| context.terminal_id.as_deref())
                        .map(|id| id == session.terminal_id)
                        .unwrap_or(false)
                        || slot
                            .launch_context
                            .as_ref()
                            .and_then(|context| context.slot_id.as_deref())
                            .map(|id| id == session.terminal_id)
                            .unwrap_or(false)
                })
            })
            .map(|tab| tab.id);
        if let Some(tab_id) = matched_terminal_id {
            self.active_terminal_id = tab_id;
            if let Some(view) = self.active_terminal_view() {
                view.read(cx).focus_handle().focus(window, cx);
            }
            self.status_message = format!(
                "selected runtime session {} and focused terminal {}",
                session.session_title, tab_id
            );
        } else {
            self.status_message = format!(
                "selected runtime session {} ({})",
                session.session_title, session.terminal_id
            );
        }
        cx.notify();
    }

    fn reload_ssh(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
        self.normalize_selected_ssh_profile();
        self.status_message = "SSH profiles reloaded".to_string();
        cx.notify();
    }

    fn selected_ssh_profile(&self) -> Option<&SSHProfileSummary> {
        self.selected_ssh_profile_id
            .as_deref()
            .and_then(|id| {
                self.state
                    .ssh
                    .profiles
                    .iter()
                    .find(|profile| profile.id == id)
            })
            .or_else(|| self.state.ssh.profiles.first())
    }

    fn normalize_selected_ssh_profile(&mut self) {
        let selected_still_exists = self
            .selected_ssh_profile_id
            .as_deref()
            .map(|id| {
                self.state
                    .ssh
                    .profiles
                    .iter()
                    .any(|profile| profile.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_ssh_profile_id = None;
        }
    }

    fn select_ssh_profile(
        &mut self,
        profile_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .state
            .ssh
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
        else {
            self.status_message = "SSH profile is no longer available".to_string();
            self.normalize_selected_ssh_profile();
            cx.notify();
            return;
        };
        self.selected_ssh_profile_id = Some(profile.id.clone());
        self.status_message = format!("selected SSH profile: {}", profile.name);
        cx.notify();
    }

    fn connect_selected_ssh_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.ssh.wrapper_available {
            self.status_message = "codux-ssh wrapper is not available".to_string();
            cx.notify();
            return;
        }
        let Some(profile) = self.selected_ssh_profile().cloned() else {
            self.status_message = "no SSH profile selected".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.ssh_launch_command(profile.id.clone()) {
            Ok(command) => {
                self.send_to_active_terminal(&format!("{}\n", command.command), cx);
                if let Some(view) = self.active_terminal_view() {
                    view.read(cx).focus_handle().focus(window, cx);
                }
                self.status_message = format!("SSH connect sent: {}", profile.name);
            }
            Err(error) => {
                self.status_message = format!("failed to build SSH launch command: {error}");
            }
        }
        cx.notify();
    }

    fn new_ssh_profile_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.ssh_draft_id = None;
        self.ssh_draft_name.clear();
        self.ssh_draft_host.clear();
        self.ssh_draft_port = "22".to_string();
        self.ssh_draft_username.clear();
        self.ssh_draft_credential_kind = "none".to_string();
        self.ssh_draft_private_key_path.clear();
        self.ssh_draft_password.clear();
        self.ssh_draft_key_passphrase.clear();
        self.status_message = "new SSH profile draft".to_string();
        cx.notify();
    }

    fn load_selected_ssh_profile_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let selected_id = self.selected_ssh_profile_id.clone().or_else(|| {
            self.state
                .ssh
                .profiles
                .first()
                .map(|profile| profile.id.clone())
        });
        let Some(profile_id) = selected_id else {
            self.status_message = "no SSH profile selected".to_string();
            cx.notify();
            return;
        };
        let snapshot = self.runtime_service.ssh_profiles();
        let Some(profile) = snapshot
            .profiles
            .into_iter()
            .find(|profile| profile.id == profile_id)
        else {
            self.status_message = "SSH profile is no longer available".to_string();
            self.normalize_selected_ssh_profile();
            cx.notify();
            return;
        };
        self.apply_ssh_draft(profile);
        self.status_message = "SSH profile loaded into editor".to_string();
        cx.notify();
    }

    fn apply_ssh_draft(&mut self, profile: SSHConnectionProfile) {
        self.ssh_draft_id = Some(profile.id);
        self.ssh_draft_name = profile.name;
        self.ssh_draft_host = profile.host;
        self.ssh_draft_port = profile.port.to_string();
        self.ssh_draft_username = profile.username;
        self.ssh_draft_credential_kind = profile.credential_kind;
        self.ssh_draft_private_key_path = profile.private_key_path;
        self.ssh_draft_password = profile.password.unwrap_or_default();
        self.ssh_draft_key_passphrase = profile.key_passphrase.unwrap_or_default();
    }

    fn set_ssh_draft_name(&mut self, value: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.ssh_draft_name = value;
        cx.notify();
    }

    fn set_ssh_draft_host(&mut self, value: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.ssh_draft_host = value;
        cx.notify();
    }

    fn set_ssh_draft_port(&mut self, value: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.ssh_draft_port = value;
        cx.notify();
    }

    fn set_ssh_draft_username(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_username = value;
        cx.notify();
    }

    fn set_ssh_draft_credential_kind(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_credential_kind = value;
        cx.notify();
    }

    fn set_ssh_draft_private_key_path(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_private_key_path = value;
        cx.notify();
    }

    fn set_ssh_draft_password(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_password = value;
        cx.notify();
    }

    fn set_ssh_draft_key_passphrase(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_draft_key_passphrase = value;
        cx.notify();
    }

    fn ssh_draft_request(&self) -> Result<SSHProfileUpsertRequest, String> {
        let port = self
            .ssh_draft_port
            .trim()
            .parse::<u16>()
            .map_err(|_| "SSH port must be a number from 1 to 65535.".to_string())?;
        Ok(SSHProfileUpsertRequest {
            id: self.ssh_draft_id.clone(),
            name: self.ssh_draft_name.clone(),
            host: self.ssh_draft_host.clone(),
            port,
            username: self.ssh_draft_username.clone(),
            credential_kind: self.ssh_draft_credential_kind.clone(),
            private_key_path: Some(self.ssh_draft_private_key_path.clone()),
            password: Some(self.ssh_draft_password.clone()),
            key_passphrase: Some(self.ssh_draft_key_passphrase.clone()),
        })
    }

    fn save_ssh_profile_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let request = match self.ssh_draft_request() {
            Ok(request) => request,
            Err(error) => {
                self.status_message = format!("failed to save SSH profile: {error}");
                cx.notify();
                return;
            }
        };
        let requested_id = request.id.clone();
        match self.runtime_service.upsert_ssh_profile(request) {
            Ok(snapshot) => {
                self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
                self.selected_ssh_profile_id = requested_id.or_else(|| {
                    snapshot
                        .profiles
                        .iter()
                        .max_by_key(|profile| profile.updated_at)
                        .map(|profile| profile.id.clone())
                });
                self.normalize_selected_ssh_profile();
                self.status_message = "SSH profile saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save SSH profile: {error}"),
        }
        cx.notify();
    }

    fn delete_selected_ssh_profile(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(profile_id) = self
            .ssh_draft_id
            .clone()
            .or_else(|| self.selected_ssh_profile_id.clone())
        else {
            self.status_message = "no SSH profile selected".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.delete_ssh_profile(profile_id) {
            Ok(_) => {
                self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
                self.normalize_selected_ssh_profile();
                self.new_ssh_profile_draft(_window, cx);
                self.status_message = "SSH profile deleted".to_string();
            }
            Err(error) => self.status_message = format!("failed to delete SSH profile: {error}"),
        }
        cx.notify();
    }

    fn test_ssh_profile_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.ssh_testing {
            self.status_message = "SSH test is already running".to_string();
            cx.notify();
            return;
        }
        let request = match self.ssh_draft_request() {
            Ok(request) => request,
            Err(error) => {
                self.status_message = format!("SSH test failed: {error}");
                cx.notify();
                return;
            }
        };
        let service = self.runtime_service.clone();
        let runtime_root = self.runtime.root.clone();
        self.ssh_testing = true;
        self.status_message = "SSH test started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.test_ssh_profile(request, runtime_root)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_ssh_test_result(result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_ssh_test_result(
        &mut self,
        result: Result<codux_runtime::ssh::SSHProfileTestResult, String>,
        cx: &mut Context<Self>,
    ) {
        self.ssh_testing = false;
        match result {
            Ok(result) => self.status_message = result.message,
            Err(error) => self.status_message = format!("SSH test failed: {error}"),
        }
        cx.notify();
    }

    fn reload_update(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
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

    fn install_update(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
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

    fn register_native_menu_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        macro_rules! register {
            ($action:ty, $handler:expr) => {
                cx.on_action(
                    TypeId::of::<$action>(),
                    window,
                    |app, _action, phase, window, cx| {
                        if phase == DispatchPhase::Bubble {
                            ($handler)(app, window, cx);
                            cx.stop_propagation();
                        }
                    },
                );
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
            |_app: &mut CoduxApp, window: &mut Window, _cx: &mut Context<CoduxApp>| {
                window.remove_window()
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
    }

    fn toggle_remote_host(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.state.remote.enabled;
        match self.runtime_service.set_remote_enabled(next) {
            Ok(remote) => {
                let settings = self.runtime_service.reload_state().settings;
                self.apply_settings_summary(settings);
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = format!(
                    "remote host setting saved: {}",
                    if self.state.remote.enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save remote setting: {error}"),
        }
        cx.notify();
    }

    fn set_remote_server_url(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_remote_server_url(&value) {
            Ok(remote) => {
                let settings = self.runtime_service.reload_state().settings;
                self.apply_settings_summary(settings);
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = "remote server saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save remote server: {error}");
            }
        }
        cx.notify();
    }

    fn reconnect_remote(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.reconnect_remote() {
            Ok(remote) => {
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = "remote reconnect requested".to_string();
            }
            Err(error) => self.status_message = format!("failed to reconnect remote: {error}"),
        }
        cx.notify();
    }

    fn refresh_remote_devices(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.refresh_remote_devices() {
            Ok(remote) => {
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = "remote devices refreshed".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to refresh remote devices: {error}")
            }
        }
        cx.notify();
    }

    fn create_remote_pairing(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.create_remote_pairing() {
            Ok(remote) => {
                let code = remote
                    .pairing
                    .as_ref()
                    .map(|pairing| pairing.code.clone())
                    .unwrap_or_default();
                let pairing = remote.pairing.clone();
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = if code.is_empty() {
                    "remote pairing created".to_string()
                } else {
                    format!("remote pairing code: {code}")
                };
                if let Some(pairing) = pairing {
                    self.start_remote_pairing_poll(pairing, cx);
                }
            }
            Err(error) => self.status_message = format!("failed to create remote pairing: {error}"),
        }
        cx.notify();
    }

    fn start_remote_pairing_poll(&mut self, pairing: RemotePairingInfo, cx: &mut Context<Self>) {
        self.remote_pairing_poll_generation = self.remote_pairing_poll_generation.wrapping_add(1);
        let generation = self.remote_pairing_poll_generation;
        let service = self.runtime_service.clone();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                let _ = codux_runtime::async_runtime::spawn_blocking(|| {
                    std::thread::sleep(Duration::from_secs(1));
                })
                .await;

                let should_poll = this
                    .update(cx, |app, _| {
                        app.remote_pairing_poll_generation == generation
                            && app
                                .state
                                .remote
                                .pairing
                                .as_ref()
                                .map(|current| current.pairing_id == pairing.pairing_id)
                                .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !should_poll {
                    return;
                }

                let worker_pairing = pairing.clone();
                let worker_service = service.clone();
                let result = codux_runtime::async_runtime::spawn_blocking(move || {
                    worker_service.poll_remote_pairing_status(&worker_pairing)
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);

                let finished = this
                    .update(cx, |app, cx| {
                        app.apply_remote_pairing_poll_result(generation, &pairing, result, cx)
                    })
                    .unwrap_or(true);
                if finished {
                    return;
                }
            }
        })
        .detach();
    }

    fn apply_remote_pairing_poll_result(
        &mut self,
        generation: u64,
        pairing: &RemotePairingInfo,
        result: Result<RemotePairingPollResult, String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.remote_pairing_poll_generation != generation
            || self
                .state
                .remote
                .pairing
                .as_ref()
                .map(|current| current.pairing_id.as_str())
                != Some(pairing.pairing_id.as_str())
        {
            return true;
        }

        match result {
            Ok(result) => {
                let finished = result.finished;
                self.state.remote = result.summary;
                self.normalize_selected_remote_device();
                self.status_message = self.state.remote.message.clone();
                cx.notify();
                finished
            }
            Err(error) => {
                self.state.remote.pairing = None;
                self.status_message = format!("remote pairing poll failed: {error}");
                cx.notify();
                true
            }
        }
    }

    fn cancel_remote_pairing(
        &mut self,
        pairing_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_pairing_poll_generation = self.remote_pairing_poll_generation.wrapping_add(1);
        match self.runtime_service.cancel_remote_pairing(&pairing_id) {
            Ok(remote) => {
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = "remote pairing cancelled".to_string();
            }
            Err(error) => self.status_message = format!("failed to cancel remote pairing: {error}"),
        }
        cx.notify();
    }

    fn confirm_remote_pairing(
        &mut self,
        pairing_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_pairing_poll_generation = self.remote_pairing_poll_generation.wrapping_add(1);
        match self.runtime_service.confirm_remote_pairing(&pairing_id) {
            Ok(remote) => {
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = "remote pairing confirmed".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to confirm remote pairing: {error}");
            }
        }
        cx.notify();
    }

    fn reject_remote_pairing(
        &mut self,
        pairing_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_pairing_poll_generation = self.remote_pairing_poll_generation.wrapping_add(1);
        match self.runtime_service.reject_remote_pairing(&pairing_id) {
            Ok(remote) => {
                self.state.remote = remote;
                self.normalize_selected_remote_device();
                self.status_message = "remote pairing rejected".to_string();
            }
            Err(error) => self.status_message = format!("failed to reject remote pairing: {error}"),
        }
        cx.notify();
    }

    fn selected_remote_device(&self) -> Option<&RemoteDeviceSummary> {
        self.selected_remote_device_id
            .as_deref()
            .and_then(|id| {
                self.state
                    .remote
                    .device_list
                    .iter()
                    .find(|device| device.id == id)
            })
            .or_else(|| self.state.remote.device_list.first())
    }

    fn normalize_selected_remote_device(&mut self) {
        let selected_still_exists = self
            .selected_remote_device_id
            .as_deref()
            .map(|id| {
                self.state
                    .remote
                    .device_list
                    .iter()
                    .any(|device| device.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_remote_device_id = self
                .state
                .remote
                .device_list
                .first()
                .map(|device| device.id.clone());
        }
    }

    fn select_remote_device(
        &mut self,
        device_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(device) = self
            .state
            .remote
            .device_list
            .iter()
            .find(|device| device.id == device_id)
        else {
            self.status_message = "remote device is no longer available".to_string();
            self.normalize_selected_remote_device();
            cx.notify();
            return;
        };
        self.selected_remote_device_id = Some(device.id.clone());
        self.status_message = format!("selected remote device: {}", empty_label(&device.name));
        cx.notify();
    }

    fn revoke_selected_remote_device(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(device) = self.selected_remote_device().cloned() else {
            self.status_message = "no remote device selected".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.revoke_remote_device(&device.id) {
            Ok(remote) => {
                self.state.remote = remote;
                self.state.settings = self.runtime_service.reload_state().settings;
                self.selected_remote_device_id = None;
                self.normalize_selected_remote_device();
                self.status_message =
                    format!("remote device revoked: {}", empty_label(&device.name));
            }
            Err(error) => self.status_message = format!("failed to revoke remote device: {error}"),
        }
        cx.notify();
    }

    fn reload_runtime_activity(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let result = self.apply_runtime_activity_tick(true, _window.is_window_active(), true);
        self.status_message = format!(
            "runtime activity reloaded · project events {} · file events {} · AI events {} · memory queued {} · badge {}",
            result.project_events,
            result.file_events,
            result.ai_events,
            result.memory_events,
            result
                .dock_badge_count
                .map(|count| count.to_string())
                .unwrap_or_else(|| "off".to_string())
        );
        if let Some(error) = result.ai_state_error {
            self.status_message
                .push_str(&format!(" · AI state save failed: {error}"));
        }
        cx.notify();
    }

    fn apply_runtime_activity_tick(
        &mut self,
        visible: bool,
        focused: bool,
        include_scheduled_tick: bool,
    ) -> RuntimeActivityTickResult {
        let window_state = self.runtime_service.app_window_state(visible, focused);
        if include_scheduled_tick {
            self.runtime_service.tick_project_activity();
        }
        let project_events = self.runtime_service.drain_project_activity_events();
        let applied_project_events = self.apply_project_activity_events(project_events);
        let file_events = self.runtime_service.drain_file_change_events();
        let applied_file_events = self.apply_file_change_events(file_events);
        let remote_events = self.runtime_service.drain_remote_events();
        if let Some(remote) = remote_events.last().cloned() {
            self.state.remote = remote;
            self.normalize_selected_remote_device();
        } else if include_scheduled_tick {
            self.state.remote = self.runtime_service.reload_remote();
            self.normalize_selected_remote_device();
        }
        let drained = self
            .runtime_service
            .drain_ai_runtime_events_and_enqueue_memory();
        self.state.runtime_activity = self.runtime_service.reload_runtime_activity();
        self.state.runtime_events = self.runtime_service.reload_runtime_events();
        let live_ai_snapshot = self.runtime_service.ai_runtime_state_snapshot();
        let mut ai_state_error = None;
        self.state.ai_runtime_state = match self
            .runtime_service
            .save_ai_runtime_state_snapshot(&live_ai_snapshot)
        {
            Ok(summary) => summary,
            Err(error) => {
                ai_state_error = Some(error.clone());
                let mut summary = self
                    .runtime_service
                    .reload_ai_runtime_state(&self.state.runtime_events);
                summary.error = Some(error);
                summary
            }
        };
        if include_scheduled_tick {
            self.state.terminal_runtime = self.runtime_service.reload_terminal_runtime();
            self.state.notifications = self.runtime_service.reload_notifications();
            self.normalize_selected_notification_channel();
            self.state.performance = self.runtime_service.reload_performance();
            self.normalize_selected_runtime_session();
        }
        if !drained.memory.is_empty() {
            self.state.memory = self.runtime_service.reload_memory(
                self.state
                    .selected_project
                    .as_ref()
                    .map(|project| project.id.as_str()),
            );
            self.reload_memory_manager_snapshot();
        }
        RuntimeActivityTickResult {
            project_events: applied_project_events,
            file_events: applied_file_events,
            ai_events: drained.events.len(),
            memory_events: drained.memory.len(),
            dock_badge_count: window_state.dock_badge_count,
            changed: applied_project_events > 0
                || applied_file_events > 0
                || !remote_events.is_empty()
                || !drained.events.is_empty()
                || !drained.memory.is_empty()
                || include_scheduled_tick
                || ai_state_error.is_some(),
            ai_state_error,
        }
    }

    fn apply_project_activity_events(&mut self, events: Vec<ProjectActivityEvent>) -> usize {
        let selected_project = self.state.selected_project.clone();
        let selected_path = selected_project
            .as_ref()
            .map(|project| project.path.as_str());
        let selected_id = selected_project.as_ref().map(|project| project.id.as_str());
        let mut applied = 0;

        for event in events {
            match event {
                ProjectActivityEvent::GitStatus {
                    project_path,
                    snapshot,
                    ..
                } if selected_path == Some(project_path.as_str()) => {
                    self.state.git = snapshot;
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    applied += 1;
                }
                ProjectActivityEvent::GitReview {
                    project_path,
                    snapshot,
                    ..
                } if selected_path == Some(project_path.as_str()) => {
                    self.git_review = snapshot;
                    self.normalize_selected_git_file();
                    applied += 1;
                }
                ProjectActivityEvent::WorktreeSnapshot {
                    project_id,
                    project_path,
                    snapshot,
                } if selected_id == Some(project_id.as_str())
                    || selected_path == Some(project_path.as_str()) =>
                {
                    self.state.worktrees = snapshot;
                    applied += 1;
                }
                ProjectActivityEvent::GitChanged { project_path, .. }
                    if selected_path == Some(project_path.as_str()) =>
                {
                    applied += 1;
                }
                ProjectActivityEvent::AIHistory {
                    project_id,
                    project_path,
                    snapshot,
                    ..
                } if selected_id == Some(project_id.as_str())
                    || selected_path == Some(project_path.as_str()) =>
                {
                    self.state.ai_history = normalized_ai_history_snapshot_to_summary(snapshot);
                    self.normalize_selected_ai_session();
                    applied += 1;
                }
                _ => {}
            }
        }

        applied
    }

    fn apply_file_change_events(&mut self, events: Vec<FileChangeEvent>) -> usize {
        let selected_path = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.path.clone());
        let mut applied = 0;

        for event in events {
            if selected_path.as_deref() == Some(event.project_path.as_str()) {
                self.refresh_file_tree_cache();
                self.normalize_selected_file_entry();
                applied += 1;
            }
        }

        applied
    }

    fn poll_ai_runtime_state(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.poll_ai_runtime_state() {
            Ok(snapshot) => {
                match self
                    .runtime_service
                    .save_ai_runtime_state_snapshot(&snapshot)
                {
                    Ok(summary) => {
                        self.state.ai_runtime_state = summary;
                        self.status_message = format!(
                            "AI runtime polled · running {} · waiting {} · completed {}",
                            self.state.ai_runtime_state.running_count,
                            self.state.ai_runtime_state.needs_input_count,
                            self.state.ai_runtime_state.completed_count
                        );
                    }
                    Err(error) => {
                        let mut summary = self
                            .runtime_service
                            .reload_ai_runtime_state(&self.state.runtime_events);
                        summary.error = Some(error);
                        self.state.ai_runtime_state = summary;
                        self.status_message = "AI runtime polled; state save failed".to_string();
                    }
                }
            }
            Err(error) => self.status_message = format!("failed to poll AI runtime: {error}"),
        }
        cx.notify();
    }

    fn dismiss_selected_project_ai_completion(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.as_ref() else {
            self.status_message = "no selected project for AI completion dismiss".to_string();
            cx.notify();
            return;
        };
        let snapshot = self
            .runtime_service
            .dismiss_ai_runtime_completion(&project.id);
        match self
            .runtime_service
            .save_ai_runtime_state_snapshot(&snapshot)
        {
            Ok(summary) => {
                self.state.ai_runtime_state = summary;
                self.status_message = format!("AI completion dismissed for {}", project.name);
            }
            Err(error) => {
                let mut summary = self
                    .runtime_service
                    .reload_ai_runtime_state(&self.state.runtime_events);
                summary.error = Some(error.clone());
                self.state.ai_runtime_state = summary;
                self.status_message =
                    format!("AI completion dismissed; state save failed: {error}");
            }
        }
        cx.notify();
    }

    fn refresh_pet(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.refresh_pet_from_indexed_history() {
            Ok(summary) => {
                self.state.pet = summary;
                self.status_message = "pet progress refreshed".to_string();
            }
            Err(error) => self.status_message = format!("failed to refresh pet: {error}"),
        }
        cx.notify();
    }

    fn save_desktop_pet_window_origin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let bounds = window.bounds();
        let origin = DesktopPetSavedOrigin {
            x: bounds.origin.x.to_f64(),
            y: bounds.origin.y.to_f64(),
        };
        if let Err(error) = self.runtime_service.save_desktop_pet_origin(origin) {
            self.status_message = format!("failed to save desktop pet position: {error}");
            cx.notify();
        }
    }

    fn apply_desktop_pet_action(
        &mut self,
        action_id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .apply_desktop_pet_menu_action(action_id)
        {
            Ok(_) => {
                let state = self.runtime_service.reload_state();
                self.state.settings = state.settings;
                self.state.pet = state.pet;
                self.desktop_pet_line_skipped = action_id == DESKTOP_PET_SKIP_LINE;
                if self.desktop_pet_line_skipped {
                    self.desktop_pet_line.clear();
                } else if matches!(action_id, DESKTOP_PET_SPEAK_MORE | DESKTOP_PET_SPEAK_LESS) {
                    self.request_desktop_pet_speech("idle", desktop_pet_fallback_line(), cx);
                }
                self.runtime_service
                    .desktop_pet_set_bubble_visible(!matches!(
                        action_id,
                        DESKTOP_PET_SKIP_LINE | DESKTOP_PET_HIDE
                    ));
                self.status_message = match action_id {
                    DESKTOP_PET_MUTE_30_MINUTES => "desktop pet muted for 30 minutes".to_string(),
                    DESKTOP_PET_MUTE_1_HOUR => "desktop pet muted for 1 hour".to_string(),
                    DESKTOP_PET_MUTE_TODAY => "desktop pet muted until tomorrow".to_string(),
                    DESKTOP_PET_SKIP_LINE => "desktop pet line skipped".to_string(),
                    DESKTOP_PET_SPEAK_MORE => "desktop pet speech frequency increased".to_string(),
                    DESKTOP_PET_SPEAK_LESS => "desktop pet speech frequency lowered".to_string(),
                    DESKTOP_PET_HIDE => {
                        window.remove_window();
                        "desktop pet hidden".to_string()
                    }
                    _ => "desktop pet action applied".to_string(),
                };
            }
            Err(error) => {
                self.status_message = format!("failed to apply desktop pet action: {error}");
            }
        }
        cx.notify();
    }

    fn request_desktop_pet_speech(
        &mut self,
        event: &'static str,
        fallback_text: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.desktop_pet_line_skipped = false;
        self.desktop_pet_line = fallback_text.to_string();

        let service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let request = codux_runtime::llm::PetIdleSpeechRequest {
                event: event.to_string(),
                fallback_text: fallback_text.to_string(),
            };
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.pet_idle_speech(request)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_desktop_pet_speech_result(result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_desktop_pet_speech_result(
        &mut self,
        result: Result<codux_runtime::llm::PetIdleSpeechResponse, String>,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_pet_line_skipped {
            return;
        }
        if let Ok(response) = result {
            let text = response.text.trim();
            if !text.is_empty() {
                self.desktop_pet_line = text.to_string();
                self.runtime_service.desktop_pet_set_bubble_visible(true);
                cx.notify();
            }
        }
    }

    fn set_pet_install_url(&mut self, value: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.pet_install_url = value;
        self.pet_install_preview = None;
        cx.notify();
    }

    fn set_pet_install_display_name(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pet_install_display_name = value;
        self.pet_install_preview = None;
        cx.notify();
    }

    fn preview_custom_pet_install(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.pet_install_previewing || self.pet_installing {
            self.status_message = "custom pet install task is already running".to_string();
            cx.notify();
            return;
        }
        let page_url = self.pet_install_url.trim().to_string();
        if page_url.is_empty() {
            self.status_message = "enter a Petdex URL first".to_string();
            cx.notify();
            return;
        }
        let display_name = self.pet_install_display_name.trim().to_string();
        let request = PetCustomPetInstallRequest {
            page_url: page_url.clone(),
            display_name: display_name.clone(),
        };

        let service = self.runtime_service.clone();
        self.pet_install_previewing = true;
        self.status_message = "custom pet preview loading".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn(async move {
                service.resolve_custom_pet_install(request).await
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_custom_pet_preview_result(page_url, display_name, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_custom_pet_preview_result(
        &mut self,
        page_url: String,
        display_name: String,
        result: Result<PetCustomPetInstallPreview, String>,
        cx: &mut Context<Self>,
    ) {
        self.pet_install_previewing = false;
        if !self.pet_install_input_matches(&page_url, &display_name) {
            self.status_message = "stale custom pet preview ignored".to_string();
            cx.notify();
            return;
        }
        match result {
            Ok(preview) => {
                self.status_message =
                    format!("custom pet preview loaded: {}", preview.display_name);
                self.pet_install_preview = Some(preview);
            }
            Err(error) => {
                self.pet_install_preview = None;
                self.status_message = format!("failed to preview custom pet: {error}");
            }
        }
        cx.notify();
    }

    fn install_custom_pet(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.pet_install_previewing || self.pet_installing {
            self.status_message = "custom pet install task is already running".to_string();
            cx.notify();
            return;
        }
        let page_url = self.pet_install_url.trim().to_string();
        if page_url.is_empty() {
            self.status_message = "enter a Petdex URL first".to_string();
            cx.notify();
            return;
        }
        let display_name = self.pet_install_display_name.trim().to_string();
        let request = PetCustomPetInstallRequest {
            page_url: page_url.clone(),
            display_name: display_name.clone(),
        };

        let service = self.runtime_service.clone();
        let current_pet_claimed = self.state.pet.claimed;
        self.pet_installing = true;
        self.status_message = "custom pet install started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn(async move {
                let custom_pet = service.install_custom_pet(request).await?;
                let archived_current = if current_pet_claimed {
                    match service.archive_current_pet() {
                        Ok(_) => true,
                        Err(error) => {
                            let pet = service.reload_pet();
                            return Ok((
                                pet,
                                format!(
                                    "custom pet installed, but current pet archive failed: {error}"
                                ),
                            ));
                        }
                    }
                } else {
                    false
                };

                let claim = PetClaimRequest {
                    species: custom_pet.id.clone(),
                    custom_name: String::new(),
                    custom_pet: Some(custom_pet.clone()),
                    _projects: Vec::new(),
                };
                let status_message = match service.claim_pet_from_indexed_history(claim) {
                    Ok(_) => {
                        if archived_current {
                            format!(
                                "custom pet installed, current pet archived, claimed: {}",
                                custom_pet.display_name
                            )
                        } else {
                            format!(
                                "custom pet installed and claimed: {}",
                                custom_pet.display_name
                            )
                        }
                    }
                    Err(error) => format!("custom pet installed, but claim failed: {error}"),
                };
                Ok((service.reload_pet(), status_message))
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_custom_pet_install_result(page_url, display_name, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_custom_pet_install_result(
        &mut self,
        page_url: String,
        display_name: String,
        result: Result<(PetSummary, String), String>,
        cx: &mut Context<Self>,
    ) {
        self.pet_installing = false;
        match result {
            Ok((pet, status_message)) => {
                let matches_input = self.pet_install_input_matches(&page_url, &display_name);
                self.state.pet = pet;
                self.pet_custom_pets = self.runtime_service.pet_catalog().custom_pets;
                if matches_input {
                    self.pet_install_url.clear();
                    self.pet_install_display_name.clear();
                    self.pet_install_preview = None;
                }
                self.status_message = status_message;
            }
            Err(error) => {
                self.status_message = format!("failed to install custom pet: {error}");
            }
        }
        cx.notify();
    }

    fn pet_install_input_matches(&self, page_url: &str, display_name: &str) -> bool {
        self.pet_install_url.trim() == page_url
            && self.pet_install_display_name.trim() == display_name
    }

    fn claim_custom_pet(
        &mut self,
        custom_pet: PetCustomPet,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let custom_pet = self.runtime_service.hydrate_custom_pet_data_url(custom_pet);
        let request = PetClaimRequest {
            species: format!("custom:{}", custom_pet.id),
            custom_name: String::new(),
            custom_pet: Some(custom_pet.clone()),
            _projects: Vec::new(),
        };

        match self.runtime_service.claim_pet_from_indexed_history(request) {
            Ok(_) => {
                self.state.pet = self.runtime_service.reload_pet();
                self.pet_custom_pets = self.runtime_service.pet_catalog().custom_pets;
                self.status_message = format!("custom pet claimed: {}", custom_pet.display_name);
            }
            Err(error) => self.status_message = format!("failed to claim custom pet: {error}"),
        }
        cx.notify();
    }

    fn claim_pet_species(
        &mut self,
        species: String,
        custom_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trimmed_species = species.trim();
        if let Some(custom_id) = trimmed_species.strip_prefix("custom:") {
            if let Some(custom_pet) = self
                .pet_custom_pets
                .iter()
                .find(|pet| pet.id == custom_id)
                .cloned()
            {
                let custom_pet = self.runtime_service.hydrate_custom_pet_data_url(custom_pet);
                let request = PetClaimRequest {
                    species: format!("custom:{}", custom_pet.id),
                    custom_name: custom_name.trim().to_string(),
                    custom_pet: Some(custom_pet.clone()),
                    _projects: Vec::new(),
                };
                match self.runtime_service.claim_pet_from_indexed_history(request) {
                    Ok(_) => {
                        self.state.pet = self.runtime_service.reload_pet();
                        self.pet_custom_pets = self.runtime_service.pet_catalog().custom_pets;
                        self.status_message =
                            format!("custom pet claimed: {}", custom_pet.display_name);
                    }
                    Err(error) => {
                        self.status_message = format!("failed to claim custom pet: {error}");
                    }
                }
                cx.notify();
                return;
            }
        }

        let species = if trimmed_species.is_empty() {
            self.runtime_service
                .pet_catalog()
                .species
                .first()
                .map(|item| item.species.clone())
                .unwrap_or_else(|| "voidcat".to_string())
        } else {
            trimmed_species.to_string()
        };
        let request = PetClaimRequest {
            species,
            custom_name: custom_name.trim().to_string(),
            custom_pet: None,
            _projects: Vec::new(),
        };

        match self.runtime_service.claim_pet_from_indexed_history(request) {
            Ok(_) => {
                self.state.pet = self.runtime_service.reload_pet();
                self.status_message = "pet claimed".to_string();
            }
            Err(error) => self.status_message = format!("failed to claim pet: {error}"),
        }
        cx.notify();
    }

    fn rename_current_pet_to(
        &mut self,
        custom_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.pet.claimed {
            self.status_message = "no pet to rename".to_string();
            cx.notify();
            return;
        }

        match self.runtime_service.rename_pet(PetRenameRequest {
            custom_name: custom_name.trim().to_string(),
        }) {
            Ok(_) => {
                self.state.pet = self.runtime_service.reload_pet();
                self.status_message = "pet renamed".to_string();
            }
            Err(error) => self.status_message = format!("failed to rename pet: {error}"),
        }
        cx.notify();
    }

    fn archive_current_pet(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.archive_current_pet() {
            Ok(_) => {
                self.state.pet = self.runtime_service.reload_pet();
                self.status_message = "pet archived".to_string();
            }
            Err(error) => self.status_message = format!("failed to archive pet: {error}"),
        }
        cx.notify();
    }

    fn restore_latest_archived_pet(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let legacy_id = match self
            .runtime_service
            .pet_snapshot()
            .ok()
            .and_then(|snapshot| snapshot.legacy.last().map(|record| record.id.clone()))
        {
            Some(legacy_id) => legacy_id,
            None => {
                self.status_message = "no archived pet to restore".to_string();
                cx.notify();
                return;
            }
        };

        match self
            .runtime_service
            .restore_archived_pet(PetRestoreRequest { legacy_id })
        {
            Ok(_) => {
                self.state.pet = self.runtime_service.reload_pet();
                self.status_message = "pet restored".to_string();
            }
            Err(error) => self.status_message = format!("failed to restore pet: {error}"),
        }
        cx.notify();
    }

    fn sync_tool_permissions(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
        self.status_message = if let Some(error) = &self.state.tool_permissions.error {
            format!("failed to sync tool permissions: {error}")
        } else {
            "tool permissions synced for runtime wrappers".to_string()
        };
        cx.notify();
    }

    fn save_terminal_layout(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.persist_terminal_layout() {
            Ok(()) => self.status_message = "terminal layout saved to state.json".to_string(),
            Err(error) => self.status_message = error,
        }
        cx.notify();
    }

    fn persist_terminal_layout(&mut self) -> Result<(), String> {
        let Some(project) = &self.state.selected_project else {
            return Err("no selected project to save terminal layout".to_string());
        };
        let (tabs, active_tab_id, top_panes, active_slot_id) = self.terminal_layout_snapshot();
        let layout = self.runtime_service.save_terminal_layout(
            &project.id,
            tabs,
            active_tab_id,
            top_panes,
            active_slot_id,
        )?;
        self.state.terminal_layout = layout;
        Ok(())
    }

    fn persist_terminal_runtime(&mut self) -> Result<(), String> {
        let (active_terminal_id, active_slot_id, sessions) = self.terminal_runtime_snapshot();
        self.state.terminal_runtime = TerminalRuntimeService::new(self.state.support_dir.clone())
            .save_from_gpui(
            active_terminal_id,
            active_slot_id,
            sessions,
        )?;
        Ok(())
    }

    fn terminal_runtime_snapshot(&self) -> (String, String, Vec<TerminalRuntimeSessionInput>) {
        let active = self.active_terminal();
        let active_terminal_id = active
            .and_then(|tab| {
                tab.panes
                    .last()
                    .and_then(|slot| slot.launch_context.as_ref())
                    .and_then(|context| context.terminal_id.clone())
                    .or_else(|| tab.terminal_id.clone())
            })
            .unwrap_or_else(|| format!("gpui-term-{}", self.active_terminal_id));
        let active_slot_id = active
            .and_then(|tab| {
                tab.panes
                    .last()
                    .and_then(|slot| slot.launch_context.as_ref())
                    .and_then(|context| context.slot_id.clone())
                    .or_else(|| tab.source_id.clone())
            })
            .unwrap_or_else(|| format!("bottom-{}", self.active_terminal_id));
        let sessions = self
            .terminals
            .iter()
            .flat_map(|tab| {
                tab.panes.iter().enumerate().map(|(pane_index, slot)| {
                    let context = slot.launch_context.as_ref();
                    let project = self.state.selected_project.as_ref();
                    let terminal_id = context
                        .and_then(|context| context.terminal_id.clone())
                        .or_else(|| tab.terminal_id.clone())
                        .unwrap_or_else(|| format!("gpui-term-{}", tab.id));
                    let slot_id = context
                        .and_then(|context| context.slot_id.clone())
                        .or_else(|| tab.source_id.clone())
                        .unwrap_or_else(|| format!("gpui-pane-{}-{}", tab.id, pane_index + 1));
                    let project_id = context
                        .map(|context| context.project_id.clone())
                        .or_else(|| project.map(|project| project.id.clone()))
                        .unwrap_or_default();
                    let project_name = context
                        .map(|context| context.project_name.clone())
                        .or_else(|| project.map(|project| project.name.clone()))
                        .unwrap_or_default();
                    let project_path = context
                        .map(|context| context.project_path.display().to_string())
                        .or_else(|| project.map(|project| project.path.clone()))
                        .unwrap_or_default();
                    let cwd = context
                        .and_then(|context| context.session_cwd.as_ref())
                        .map(|cwd| cwd.display().to_string())
                        .unwrap_or_else(|| project_path.clone());
                    let input = slot.pane.input_snapshot();
                    let output = slot.pane.output_snapshot();
                    let (output_bytes, output_tail) = if output.tail.is_empty() {
                        (
                            slot.restored_output_bytes,
                            slot.restored_output_tail.clone(),
                        )
                    } else {
                        (output.bytes, output.tail)
                    };
                    TerminalRuntimeSessionInput {
                        terminal_id,
                        slot_id,
                        tab_id: tab
                            .source_id
                            .clone()
                            .unwrap_or_else(|| format!("bottom-{}", tab.id)),
                        pane_index,
                        title: slot.title.clone(),
                        project_id,
                        project_name,
                        project_path,
                        cwd,
                        input_bytes: input.bytes,
                        input_history: input
                            .history
                            .into_iter()
                            .map(|entry| TerminalInputSummary {
                                text: entry.text,
                                bytes: entry.bytes,
                                timestamp: entry.timestamp,
                            })
                            .collect(),
                        output_bytes,
                        output_tail,
                    }
                })
            })
            .collect();
        (active_terminal_id, active_slot_id, sessions)
    }

    fn terminal_layout_snapshot(
        &self,
    ) -> (
        Vec<TerminalTabSummary>,
        String,
        Vec<TerminalPaneSummary>,
        String,
    ) {
        let tabs = self
            .terminals
            .iter()
            .map(terminal_tab_summary)
            .collect::<Vec<_>>();
        let top_panes = self
            .active_terminal()
            .map(|tab| {
                tab.panes
                    .iter()
                    .enumerate()
                    .map(|(index, slot)| terminal_pane_summary(tab.id, index, slot))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let active_tab_id = self
            .active_terminal()
            .and_then(|tab| tab.source_id.clone())
            .or_else(|| {
                tabs.iter()
                    .find(|tab| tab.id == format!("bottom-{}", self.active_terminal_id))
                    .map(|tab| tab.id.clone())
            })
            .unwrap_or_else(|| format!("bottom-{}", self.active_terminal_id));
        let active_slot_id = top_panes
            .last()
            .map(|pane| pane.id.clone())
            .unwrap_or_else(|| active_tab_id.clone());
        (tabs, active_tab_id, top_panes, active_slot_id)
    }

    fn reload_terminal_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.terminal_layout = self.runtime_service.reload_terminal_layout(
            self.state
                .selected_project
                .as_ref()
                .map(|project| project.id.as_str()),
        );
        self.state.terminal_runtime = self.runtime_service.reload_terminal_runtime();
        let restore_plan =
            terminal_restore_plan(&self.state.terminal_layout, &self.state.terminal_runtime);
        prepare_memory_launch_artifacts(&self.state);
        let launch_context = self.current_terminal_launch_context();
        let terminal_config = self.terminal_config_from_settings();
        match spawn_terminal_tabs(
            &restore_plan,
            self.terminal_manager.clone(),
            launch_context.as_ref(),
            terminal_config,
            cx,
        ) {
            Ok((terminals, active_terminal_id, next_terminal_index)) => {
                self.terminals = terminals;
                self.active_terminal_id = active_terminal_id;
                self.next_terminal_index = next_terminal_index;
                let runtime_result = self.persist_terminal_runtime();
                if let Some(view) = self.active_terminal_view() {
                    view.read(cx).focus_handle().focus(window, cx);
                }
                self.status_message = if let Err(error) = runtime_result {
                    format!("terminal layout reloaded; runtime save failed: {error}")
                } else {
                    format!(
                        "terminal layout reloaded · {} tab{}",
                        self.terminals.len(),
                        if self.terminals.len() == 1 { "" } else { "s" }
                    )
                };
            }
            Err(error) => {
                self.status_message = format!("failed to rebuild terminal layout: {error}");
            }
        }
        cx.notify();
    }

    fn reload_worktrees(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.worktrees = self.runtime_service.reload_worktrees(
            self.state
                .selected_project
                .as_ref()
                .map(|project| project.id.as_str()),
            self.state
                .selected_project
                .as_ref()
                .map(|project| project.path.as_str()),
        );
        self.status_message = "worktrees reloaded".to_string();
        cx.notify();
    }

    fn sync_worktrees_from_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to sync worktrees".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .sync_worktrees_from_git(&project.id, &project.path)
        {
            Ok(summary) => {
                self.state.worktrees = summary;
                self.status_message = "worktrees synced from Git".to_string();
            }
            Err(error) => self.status_message = format!("failed to sync worktrees: {error}"),
        }
        cx.notify();
    }

    fn create_worktree(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to create worktree".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .create_worktree(&project.id, &project.path)
        {
            Ok(summary) => {
                self.state.worktrees = summary;
                self.status_message = "worktree created".to_string();
            }
            Err(error) => self.status_message = format!("failed to create worktree: {error}"),
        }
        cx.notify();
    }

    fn remove_selected_worktree(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.remove_selected_worktree_with_options(false, cx);
    }

    fn remove_selected_worktree_and_branch(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove_selected_worktree_with_options(true, cx);
    }

    fn remove_selected_worktree_with_options(
        &mut self,
        remove_branch: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to remove worktree".to_string();
            cx.notify();
            return;
        };
        let Some(worktree_id) = self.state.worktrees.selected_worktree_id.clone() else {
            self.status_message = "no selected worktree to remove".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.remove_worktree(
            &project.id,
            &project.path,
            &worktree_id,
            remove_branch,
        ) {
            Ok(summary) => {
                self.state.worktrees = summary;
                self.status_message = if remove_branch {
                    format!("worktree and branch removed: {worktree_id}")
                } else {
                    format!("worktree removed: {worktree_id}")
                };
            }
            Err(error) => self.status_message = format!("failed to remove worktree: {error}"),
        }
        cx.notify();
    }

    fn merge_selected_worktree(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to merge worktree".to_string();
            cx.notify();
            return;
        };
        let Some(worktree_id) = self.state.worktrees.selected_worktree_id.clone() else {
            self.status_message = "no selected worktree to merge".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .merge_worktree(&project.id, &project.path, &worktree_id)
        {
            Ok(summary) => {
                self.state.worktrees = summary;
                self.status_message = format!("worktree merged: {worktree_id}");
            }
            Err(error) => self.status_message = format!("failed to merge worktree: {error}"),
        }
        cx.notify();
    }

    fn select_worktree(
        &mut self,
        worktree_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for worktree selection".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .select_worktree(&project.id, &worktree_id)
        {
            Ok(()) => {
                self.state.worktrees = self
                    .runtime_service
                    .reload_worktrees(Some(&project.id), Some(&project.path));
                self.status_message = format!("selected worktree: {worktree_id}");
            }
            Err(error) => self.status_message = format!("failed to select worktree: {error}"),
        }
        cx.notify();
    }

    fn preview_file(
        &mut self,
        relative_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to preview".to_string();
            cx.notify();
            return;
        };

        match self
            .runtime_service
            .read_project_file_edit_buffer(&project.path, &relative_path)
        {
            Ok((content, editable)) => {
                self.file_preview = if content.trim().is_empty() && !editable {
                    "(empty file)".to_string()
                } else {
                    content
                };
                self.file_editable = editable;
                self.file_dirty = false;
                self.normalize_file_search_index();
                self.status_message = format!(
                    "{} loaded: {relative_path}",
                    if editable { "editor buffer" } else { "preview" }
                );
            }
            Err(error) => self.status_message = format!("failed to preview file: {error}"),
        }
        cx.notify();
    }

    fn add_terminal_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        prepare_memory_launch_artifacts(&self.state);
        let launch_context = self.current_terminal_launch_context();
        let id = self.next_terminal_index;
        let title = format!("终端 {id}");
        let pane_plan = TerminalPanePlan {
            source_id: Some(format!("bottom-{id}")),
            terminal_id: Some(format!("gpui-term-{id}")),
            title: title.clone(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };
        let pane_context = terminal_pane_launch_context(launch_context.as_ref(), id, 0, &pane_plan);
        match TerminalPane::spawn_with_context_and_config(
            cx,
            self.terminal_manager.clone(),
            pane_context.as_ref(),
            self.terminal_config_from_settings(),
        ) {
            Ok(terminal) => {
                terminal.view.read(cx).focus_handle().focus(window, cx);
                self.next_terminal_index += 1;
                self.terminals.push(TerminalTab {
                    id,
                    label: title.clone(),
                    source_id: pane_plan.source_id.clone(),
                    terminal_id: pane_plan.terminal_id.clone(),
                    panes: vec![TerminalPaneSlot {
                        title: title.clone(),
                        launch_context: pane_context,
                        pane: terminal,
                        restored_output_bytes: 0,
                        restored_output_tail: String::new(),
                    }],
                });
                self.active_terminal_id = id;
                if let Err(error) = self.persist_terminal_layout() {
                    self.status_message =
                        format!("terminal tab added; layout save failed: {error}");
                } else if let Err(error) = self.persist_terminal_runtime() {
                    self.status_message =
                        format!("terminal tab added; runtime save failed: {error}");
                } else {
                    self.status_message = format!("terminal tab added: {title}");
                }
                cx.notify();
            }
            Err(error) => eprintln!("failed to create terminal tab: {error}"),
        }
    }

    fn split_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        prepare_memory_launch_artifacts(&self.state);
        let launch_context = self.current_terminal_launch_context();
        let Some(active_tab) = self.active_terminal() else {
            return;
        };
        if active_tab.panes.len() >= 6 {
            self.status_message = "main split limit reached: 6 panes".to_string();
            cx.notify();
            return;
        }
        let tab_id = active_tab.id;
        let pane_index = active_tab.panes.len();
        let title = format!("分屏 {}", pane_index + 1);
        let pane_plan = TerminalPanePlan {
            source_id: Some(format!("top-{}", pane_index + 1)),
            terminal_id: Some(format!("gpui-pane-{tab_id}-{}", pane_index + 1)),
            title: title.clone(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };
        let pane_context =
            terminal_pane_launch_context(launch_context.as_ref(), tab_id, pane_index, &pane_plan);
        match TerminalPane::spawn_with_context_and_config(
            cx,
            self.terminal_manager.clone(),
            pane_context.as_ref(),
            self.terminal_config_from_settings(),
        ) {
            Ok(terminal) => {
                terminal.view.read(cx).focus_handle().focus(window, cx);
                if let Some(tab) = self.active_terminal_mut() {
                    tab.panes.push(TerminalPaneSlot {
                        title,
                        launch_context: pane_context,
                        pane: terminal,
                        restored_output_bytes: 0,
                        restored_output_tail: String::new(),
                    });
                }
                if let Err(error) = self.persist_terminal_layout() {
                    self.status_message =
                        format!("terminal split added; layout save failed: {error}");
                } else if let Err(error) = self.persist_terminal_runtime() {
                    self.status_message =
                        format!("terminal split added; runtime save failed: {error}");
                } else {
                    self.status_message = "terminal split added and layout saved".to_string();
                }
                cx.notify();
            }
            Err(error) => eprintln!("failed to split terminal: {error}"),
        }
    }

    fn close_terminal_pane(
        &mut self,
        pane_index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self
            .terminals
            .iter()
            .position(|tab| tab.id == self.active_terminal_id)
        else {
            return;
        };
        if self.terminals[tab_index].panes.len() <= 1 {
            self.status_message = "keep at least one main split pane".to_string();
            cx.notify();
            return;
        }
        if pane_index >= self.terminals[tab_index].panes.len() {
            return;
        }
        self.terminals[tab_index].panes.remove(pane_index);
        if let Err(error) = self.persist_terminal_layout() {
            self.status_message = format!("terminal split closed; layout save failed: {error}");
        } else if let Err(error) = self.persist_terminal_runtime() {
            self.status_message = format!("terminal split closed; runtime save failed: {error}");
        } else {
            self.status_message = "terminal split closed and layout saved".to_string();
        }
        cx.notify();
    }

    fn current_terminal_launch_context(&self) -> Option<TerminalLaunchContext> {
        terminal_launch_context(&self.state, &self.runtime, &self.state.tool_permissions)
    }

    fn send_to_active_terminal(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(tab_index) = self
            .terminals
            .iter()
            .position(|tab| tab.id == self.active_terminal_id)
        else {
            self.status_message = "no active terminal".to_string();
            cx.notify();
            return;
        };
        let Some(slot_index) = self.terminals[tab_index].panes.len().checked_sub(1) else {
            self.status_message = "active terminal has no pane".to_string();
            cx.notify();
            return;
        };
        let result = self.terminals[tab_index].panes[slot_index]
            .pane
            .send_text(text);
        match result {
            Ok(()) => {
                let tab_label = self.terminals[tab_index].label.clone();
                if let Err(error) = self.persist_terminal_runtime() {
                    self.status_message =
                        format!("sent command to {tab_label}; runtime save failed: {error}");
                } else {
                    self.status_message = format!("sent command to {tab_label}");
                }
            }
            Err(error) => {
                self.status_message = format!("failed to send terminal command: {error}");
            }
        }
        cx.notify();
    }

    fn launch_ai_tool(
        &mut self,
        tool: AIToolLauncher,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        prepare_memory_launch_artifacts(&self.state);
        self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
        let command = ai_tool_launch_command(tool, &self.state.tool_permissions);
        self.send_to_active_terminal(&format!("{command}\n"), cx);
        if let Some(view) = self.active_terminal_view() {
            view.read(cx).focus_handle().focus(window, cx);
        }
    }

    fn close_terminal_tab(
        &mut self,
        terminal_id: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.terminals.iter().position(|tab| tab.id == terminal_id) else {
            return;
        };
        self.terminals.remove(index);
        self.active_terminal_id = self
            .terminals
            .get(index.saturating_sub(1))
            .or_else(|| self.terminals.first())
            .map(|tab| tab.id)
            .unwrap_or(0);
        if let Err(error) = self.persist_terminal_layout() {
            self.status_message = format!("terminal tab closed; layout save failed: {error}");
        } else if let Err(error) = self.persist_terminal_runtime() {
            self.status_message = format!("terminal tab closed; runtime save failed: {error}");
        } else {
            self.status_message = "terminal tab closed and layout saved".to_string();
        }
        cx.notify();
    }

    fn select_terminal_tab(
        &mut self,
        terminal_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_terminal_id = terminal_id;
        if let Some(view) = self.active_terminal_view() {
            view.read(cx).focus_handle().focus(window, cx);
        }
        if let Err(error) = self.persist_terminal_layout() {
            self.status_message = format!("terminal selected; layout save failed: {error}");
        } else if let Err(error) = self.persist_terminal_runtime() {
            self.status_message = format!("terminal selected; runtime save failed: {error}");
        }
        cx.notify();
    }

    fn active_terminal(&self) -> Option<&TerminalTab> {
        self.terminals
            .iter()
            .find(|tab| tab.id == self.active_terminal_id)
            .or_else(|| self.terminals.first())
    }

    fn active_terminal_mut(&mut self) -> Option<&mut TerminalTab> {
        let active_id = self.active_terminal_id;
        if let Some(index) = self.terminals.iter().position(|tab| tab.id == active_id) {
            self.terminals.get_mut(index)
        } else {
            self.terminals.first_mut()
        }
    }

    fn active_terminal_view(&self) -> Option<gpui::Entity<TerminalView>> {
        self.active_terminal()
            .and_then(|tab| tab.panes.last())
            .map(|slot| slot.pane.view.clone())
    }

    fn desktop_pet_window(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let level = self.state.pet.level.max(1);
        let progress = self.state.pet.progress.clamp(0.0, 1.0) as f32;
        let line = self.desktop_pet_line.trim().to_string();
        let sprite_path = pet_sprite_path(
            &self.runtime.source_root,
            &self.state.support_dir,
            &self.state.pet,
            &self.pet_custom_pets,
        );
        let name = if self.state.pet.claimed && !self.state.pet.display_name.is_empty() {
            self.state.pet.display_name.clone()
        } else {
            "Codux Pet".to_string()
        };

        div()
            .size_full()
            .font_family("SF Pro Text")
            .text_color(cx.theme().foreground)
            .bg(cx.theme().transparent)
            .on_mouse_down(MouseButton::Left, |_event, window, _cx| {
                window.start_window_move();
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _event, window, cx| {
                    app.save_desktop_pet_window_origin(window, cx)
                }),
            )
            .child(
                div()
                    .size_full()
                    .p_3()
                    .flex()
                    .items_end()
                    .gap_3()
                    .child(
                        div()
                            .w(px(204.0))
                            .rounded(px(14.0))
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().popover.opacity(0.94))
                            .p_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .text_size(px(14.0))
                                            .line_height(px(18.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .truncate()
                                            .child(name),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .px_2()
                                            .h(px(20.0))
                                            .rounded_sm()
                                            .flex()
                                            .items_center()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .bg(cx.theme().accent)
                                            .text_color(cx.theme().accent_foreground)
                                            .child(format!("Lv.{level}")),
                                    ),
                            )
                            .when(!line.is_empty(), |this| {
                                this.child(
                                    div()
                                        .mt_2()
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .text_color(cx.theme().secondary_foreground)
                                        .child(line),
                                )
                            })
                            .child(
                                div()
                                    .mt_3()
                                    .h(px(5.0))
                                    .rounded_full()
                                    .overflow_hidden()
                                    .bg(cx.theme().secondary)
                                    .child(
                                        div()
                                            .h_full()
                                            .w(gpui::relative(progress))
                                            .rounded_full()
                                            .bg(cx.theme().primary),
                                    ),
                            )
                            .child(
                                div()
                                    .mt_3()
                                    .flex()
                                    .flex_col()
                                    .items_end()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(desktop_pet_action_button(
                                                "desktop-pet-mute-30",
                                                "静音 30 分钟",
                                                IconName::Moon,
                                                DESKTOP_PET_MUTE_30_MINUTES,
                                                cx,
                                            ))
                                            .child(desktop_pet_action_button(
                                                "desktop-pet-mute-hour",
                                                "静音 1 小时",
                                                IconName::Bell,
                                                DESKTOP_PET_MUTE_1_HOUR,
                                                cx,
                                            ))
                                            .child(desktop_pet_action_button(
                                                "desktop-pet-mute-today",
                                                "今日静音",
                                                IconName::Calendar,
                                                DESKTOP_PET_MUTE_TODAY,
                                                cx,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(desktop_pet_action_button(
                                                "desktop-pet-skip",
                                                "跳过当前句",
                                                IconName::Pause,
                                                DESKTOP_PET_SKIP_LINE,
                                                cx,
                                            ))
                                            .child(desktop_pet_action_button(
                                                "desktop-pet-speak-less",
                                                "少说一点",
                                                IconName::Minus,
                                                DESKTOP_PET_SPEAK_LESS,
                                                cx,
                                            ))
                                            .child(desktop_pet_action_button(
                                                "desktop-pet-speak-more",
                                                "多说一点",
                                                IconName::Plus,
                                                DESKTOP_PET_SPEAK_MORE,
                                                cx,
                                            ))
                                            .child(desktop_pet_action_button(
                                                "desktop-pet-hide",
                                                "隐藏桌面宠物",
                                                IconName::Close,
                                                DESKTOP_PET_HIDE,
                                                cx,
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .size(px(112.0))
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(desktop_pet_sprite(sprite_path, cx)),
                    ),
            )
    }
}

fn desktop_pet_sprite(sprite_path: PathBuf, cx: &mut Context<CoduxApp>) -> AnyElement {
    pet_sprite_element(sprite_path, DESKTOP_PET_SPRITE_SIZE, cx.theme().primary)
}

fn pet_sprite_element(sprite_path: PathBuf, size: f32, fallback_color: gpui::Hsla) -> AnyElement {
    let visible_width = pet_sprite_visible_width(size);

    div()
        .size(px(size))
        .overflow_hidden()
        .flex_none()
        .child(
            img(sprite_path)
                .w(px(PET_ATLAS_COLUMNS * visible_width))
                .h(px(PET_ATLAS_ROWS * size))
                .object_fit(ObjectFit::Fill)
                .with_fallback(move || {
                    div()
                        .size(px(size))
                        .rounded_full()
                        .bg(fallback_color.opacity(0.18))
                        .text_color(fallback_color)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::Heart).size_6())
                        .into_any_element()
                }),
        )
        .into_any_element()
}

impl Render for CoduxApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.register_native_menu_actions(window, cx);

        if self.window_mode == AppWindowMode::DesktopPet {
            return self.desktop_pet_window(window, cx).into_any_element();
        }

        if self.window_mode == AppWindowMode::About {
            return div()
                .size_full()
                .font_family("SF Pro Text")
                .text_color(color(theme::TEXT))
                .bg(color(theme::BG))
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.about_workspace(window, cx))
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::GitDiff {
            return div()
                .size_full()
                .font_family("SF Pro Text")
                .text_color(color(theme::TEXT))
                .bg(color(theme::BG))
                .on_key_down(cx.listener(Self::on_key_down))
                .child(git_diff_window_workspace(
                    self.git_diff_window_path.as_deref(),
                    &self.git_diff_window_content,
                    self.git_diff_window_error.as_deref(),
                    cx,
                ))
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::MemoryManager {
            return div()
                .size_full()
                .font_family("SF Pro Text")
                .text_color(color(theme::TEXT))
                .bg(color(theme::BG))
                .on_key_down(cx.listener(Self::on_key_down))
                .child(memory_manager_window_workspace(
                    &self.state.memory_manager,
                    self.memory_manager_tab,
                    self.selected_memory_entry_id.as_deref(),
                    self.selected_memory_summary_id.as_deref(),
                    self.memory_processing,
                    window,
                    cx,
                ))
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::PetClaim {
            return div()
                .size_full()
                .font_family("SF Pro Text")
                .text_color(color(theme::TEXT))
                .bg(color(theme::BG))
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.pet_claim_workspace(window, cx))
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::PetCustomInstall {
            return div()
                .size_full()
                .font_family("SF Pro Text")
                .text_color(color(theme::TEXT))
                .bg(color(theme::BG))
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.pet_custom_install_workspace(window, cx))
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::PetDex {
            return div()
                .size_full()
                .font_family("SF Pro Text")
                .text_color(color(theme::TEXT))
                .bg(color(theme::BG))
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.pet_dex_workspace(window, cx))
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::Settings {
            return div()
                .size_full()
                .font_family("SF Pro Text")
                .text_color(color(theme::TEXT))
                .bg(color(theme::BG))
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.settings_workspace(window, cx))
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::ProjectEditor {
            return div()
                .size_full()
                .font_family("SF Pro Text")
                .text_color(color(theme::TEXT))
                .bg(color(theme::BG))
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.project_editor_workspace(window, cx))
                .into_any_element();
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .font_family("SF Pro Text")
            .text_color(color(theme::TEXT))
            .bg(color(theme::BG))
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(self.project_column(cx))
                    .when(!self.task_column_collapsed, |this| {
                        this.child(self.task_column(cx))
                    })
                    .child(self.main_workspace_column(window, cx)),
            )
            .child(self.status_bar(cx))
            .into_any_element()
    }
}

impl Drop for CoduxApp {
    fn drop(&mut self) {
        if self.window_mode == AppWindowMode::Main {
            self.shutdown_runtime_state();
        }
    }
}

fn desktop_pet_action_button(
    id: &'static str,
    tooltip: &'static str,
    icon: IconName,
    action_id: &'static str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    Button::new(id)
        .compact()
        .ghost()
        .tooltip(tooltip)
        .text_color(cx.theme().secondary_foreground)
        .icon(
            Icon::new(icon)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.apply_desktop_pet_action(action_id, window, cx)
        }))
}

fn column_header(content: impl IntoElement) -> impl IntoElement {
    div()
        .h(px(44.0))
        .px(px(10.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .border_b_1()
        .border_color(color(theme::BORDER_SOFT))
        .bg(color(theme::BG_HEADER))
        .on_mouse_down(MouseButton::Left, |_event, window, _cx| {
            window.start_window_move();
        })
        .child(content)
}

fn header_icon_button(
    id: &'static str,
    icon: IconName,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .icon(Icon::new(icon).text_color(cx.theme().secondary_foreground))
        .on_click(cx.listener(on_click))
}

fn assistant_header_icon_button(
    id: &'static str,
    icon: IconName,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .compact()
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .icon(
            Icon::new(icon)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .on_click(cx.listener(on_click))
}

fn section(title: &'static str, rows: Vec<String>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .mx_3()
        .mt_3()
        .rounded_sm()
        .border_1()
        .border_color(color(theme::BORDER))
        .bg(color(theme::BG_ELEVATED))
        .child(
            div()
                .h(px(30.0))
                .px_2()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_MUTED))
                .child(title),
        )
        .children(rows.into_iter().map(|row| {
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(color(theme::TEXT_DIM))
                .child(row)
                .into_any_element()
        }))
}

#[cfg(test)]
fn restored_terminal_preview_lines(output_tail: &str) -> Vec<String> {
    output_tail
        .lines()
        .filter(|line| !line.trim().is_empty())
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|line| line.chars().take(96).collect::<String>())
        .collect()
}

fn empty_label(value: &str) -> String {
    if value.trim().is_empty() {
        "none".to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codux_runtime::terminal_layout::TerminalLayoutSummary;
    use codux_runtime::terminal_runtime::{TerminalRuntimeSessionSummary, TerminalRuntimeSummary};
    use std::path::PathBuf;

    #[test]
    fn terminal_restore_plan_uses_top_panes_and_bottom_tabs() {
        let layout = TerminalLayoutSummary {
            active_slot_id: "bottom-2".to_string(),
            active_tab_id: "bottom-2".to_string(),
            top_panes: vec![
                TerminalPaneSummary {
                    id: "top-1".to_string(),
                    title: "分屏 1".to_string(),
                    terminal_id: "term-a".to_string(),
                },
                TerminalPaneSummary {
                    id: "top-2".to_string(),
                    title: "长任务".to_string(),
                    terminal_id: "term-b".to_string(),
                },
            ],
            tabs: vec![
                TerminalTabSummary {
                    id: "bottom-1".to_string(),
                    label: "标签页 1".to_string(),
                    terminal_id: "term-c".to_string(),
                },
                TerminalTabSummary {
                    id: "bottom-2".to_string(),
                    label: "标签页 2".to_string(),
                    terminal_id: "term-d".to_string(),
                },
            ],
            top_ratios: vec![0.5, 0.5],
            bottom_ratio: 0.32,
            error: None,
        };

        let runtime = TerminalRuntimeSummary {
            sessions: vec![TerminalRuntimeSessionSummary {
                terminal_id: "term-a".to_string(),
                slot_id: "top-1".to_string(),
                tab_id: "top-1".to_string(),
                pane_index: 0,
                title: "分屏 1".to_string(),
                project_id: "project-1".to_string(),
                project_name: "Codux".to_string(),
                project_path: "/workspace/codux".to_string(),
                cwd: "/workspace/codux".to_string(),
                status: "running".to_string(),
                is_running: true,
                created_at: 1.0,
                last_active_at: 2.0,
                has_buffer: false,
                buffer_characters: 0,
                input_bytes: 0,
                last_input_at: None,
                input_history: Vec::new(),
                output_bytes: 10,
                output_tail: "restored top output".to_string(),
                source: "gpui".to_string(),
            }],
            ..Default::default()
        };
        let plan = terminal_restore_plan(&layout, &runtime);

        assert_eq!(plan.tabs.len(), 3);
        assert_eq!(plan.tabs[0].label, "主终端");
        assert_eq!(
            plan.tabs[0]
                .panes
                .iter()
                .map(|pane| pane.title.as_str())
                .collect::<Vec<_>>(),
            vec!["分屏 1", "长任务"]
        );
        assert_eq!(plan.tabs[0].panes[0].source_id.as_deref(), Some("top-1"));
        assert_eq!(plan.tabs[0].panes[0].terminal_id.as_deref(), Some("term-a"));
        assert_eq!(
            plan.tabs[0].panes[0].restored_output_tail,
            "restored top output"
        );
        assert_eq!(plan.tabs[0].panes[0].restored_output_bytes, 10);
        assert_eq!(plan.tabs[1].label, "标签页 1");
        assert_eq!(plan.tabs[1].source_id.as_deref(), Some("bottom-1"));
        assert_eq!(plan.tabs[1].terminal_id.as_deref(), Some("term-c"));
        assert_eq!(plan.tabs[2].label, "标签页 2");
        assert_eq!(plan.active_index, 2);
    }

    #[test]
    fn terminal_restore_plan_falls_back_to_single_terminal() {
        let plan = terminal_restore_plan(
            &TerminalLayoutSummary::default(),
            &TerminalRuntimeSummary::default(),
        );

        assert_eq!(plan.active_index, 0);
        assert_eq!(
            plan.tabs,
            vec![TerminalTabPlan {
                source_id: None,
                terminal_id: None,
                label: "终端 1".to_string(),
                panes: vec![TerminalPanePlan {
                    source_id: None,
                    terminal_id: None,
                    title: "终端 1".to_string(),
                    restored_output_bytes: 0,
                    restored_output_tail: String::new(),
                }],
            }]
        );
    }

    #[test]
    fn restored_terminal_preview_lines_use_last_non_empty_rows() {
        assert_eq!(
            restored_terminal_preview_lines("one\n\ntwo\nthree\nfour\nfive\n"),
            vec!["two", "three", "four", "five"]
        );
        assert_eq!(
            restored_terminal_preview_lines(&"x".repeat(120)),
            vec!["x".repeat(96)]
        );
    }

    #[test]
    fn terminal_pane_launch_context_assigns_stable_runtime_identity() {
        let base = TerminalLaunchContext {
            project_id: "project-1".to_string(),
            project_name: "Codux".to_string(),
            project_path: PathBuf::from("/workspace/codux"),
            support_dir: PathBuf::from("/support/Codux"),
            runtime_root: PathBuf::from("/runtime-root"),
            terminal_id: None,
            slot_id: None,
            session_key: None,
            session_title: None,
            session_cwd: None,
            session_instance_id: None,
            tool_permissions_file: None,
            memory_workspace_root: None,
            memory_prompt_file: None,
            memory_index_file: None,
        };

        let pane = TerminalPanePlan {
            source_id: Some("top-existing".to_string()),
            terminal_id: Some("term-existing".to_string()),
            title: "分屏 2".to_string(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };

        let context = terminal_pane_launch_context(Some(&base), 3, 1, &pane)
            .expect("context should be derived");
        let repeated = terminal_pane_launch_context(Some(&base), 3, 1, &pane)
            .expect("context should be derived");

        assert_eq!(context.terminal_id.as_deref(), Some("term-existing"));
        assert_eq!(context.slot_id.as_deref(), Some("top-existing"));
        assert_eq!(
            context.session_key.as_deref(),
            Some("gpui:project-1:term-existing:top-existing")
        );
        assert_eq!(context.session_title.as_deref(), Some("分屏 2"));
        assert_eq!(
            context.session_cwd.as_deref(),
            Some(PathBuf::from("/workspace/codux").as_path())
        );
        assert_eq!(context.session_instance_id, repeated.session_instance_id);
    }

    #[test]
    fn ai_tool_launch_command_uses_configured_models_and_quotes_values() {
        let permissions = ToolPermissionsSummary {
            codex_model: "gpt-5.5".to_string(),
            codex_effort: "high".to_string(),
            claude_code_model: "claude sonnet".to_string(),
            gemini_model: "gemini-2.5-pro".to_string(),
            opencode_model: "open'code".to_string(),
            kiro_model: String::new(),
            ..Default::default()
        };

        assert_eq!(
            ai_tool_launch_command(AIToolLauncher::Codex, &permissions),
            "codex --model gpt-5.5 --reasoning-effort high"
        );
        assert_eq!(
            ai_tool_launch_command(AIToolLauncher::Claude, &permissions),
            "claude --model 'claude sonnet'"
        );
        assert_eq!(
            ai_tool_launch_command(AIToolLauncher::OpenCode, &permissions),
            "opencode --model 'open'\\''code'"
        );
        assert_eq!(
            ai_tool_launch_command(AIToolLauncher::Kiro, &permissions),
            "kiro"
        );
    }

    #[test]
    fn ai_session_restore_command_matches_tauri_history_restore() {
        let mut session = AISessionSummary {
            id: "local-id".to_string(),
            session_key: "session key".to_string(),
            external_session_id: Some("external-1".to_string()),
            title: "Task".to_string(),
            source: "codex".to_string(),
            last_model: None,
            last_seen_at: 0.0,
            total_tokens: 0,
            cached_input_tokens: 0,
            request_count: 0,
        };

        assert_eq!(
            ai_session_restore_command(&session),
            "codex resume external-1"
        );

        session.source = "claude-code".to_string();
        assert_eq!(
            ai_session_restore_command(&session),
            "claude --resume external-1"
        );

        session.source = "opencode".to_string();
        session.external_session_id = None;
        assert_eq!(
            ai_session_restore_command(&session),
            "opencode run --session 'session key'"
        );

        session.source = "antigravity".to_string();
        assert_eq!(
            ai_session_restore_command(&session),
            "agy resume 'session key'"
        );
    }

    #[test]
    fn ssh_connect_command_uses_saved_profile_id_without_exposing_endpoint() {
        let profile = SSHProfileSummary {
            id: "profile with spaces".to_string(),
            name: "Production".to_string(),
            endpoint: "root@example.com:22".to_string(),
            credential_kind: "password".to_string(),
            updated_at: 123,
        };

        assert_eq!(
            ssh_connect_command(&profile),
            "codux-ssh 'profile with spaces'"
        );
    }

    #[test]
    fn generated_git_commit_message_prefers_staged_count() {
        let git = GitSummary {
            staged: 1,
            unstaged: 3,
            untracked: 2,
            ..Default::default()
        };
        assert_eq!(generated_git_commit_message(&git), "Update 1 staged file");

        let git = GitSummary {
            staged: 0,
            unstaged: 2,
            untracked: 1,
            ..Default::default()
        };
        assert_eq!(generated_git_commit_message(&git), "Update 3 changed files");

        assert_eq!(
            generated_git_commit_message(&GitSummary::default()),
            "Update project files"
        );
    }

    #[test]
    fn project_badge_text_uses_first_two_non_space_chars() {
        assert_eq!(
            project_badge_text_from_name(" Codux GPUI "),
            Some("CO".to_string())
        );
        assert_eq!(
            project_badge_text_from_name("项目"),
            Some("项目".to_string())
        );
        assert_eq!(project_badge_text_from_name("  "), None);
    }

    #[test]
    fn git_remote_action_label_names_remote_pushes() {
        assert_eq!(git_remote_action_label("fetch"), "fetch");
        assert_eq!(git_remote_action_label("push:origin"), "push to origin");
    }

    #[test]
    fn shortcut_text_normalizes_tauri_display_formats() {
        assert_eq!(
            normalized_shortcut_text("Cmd+Shift+P"),
            Some("Meta+Shift+P".to_string())
        );
        assert_eq!(
            normalized_shortcut_text("⌘⇧P"),
            Some("Meta+Shift+P".to_string())
        );
        assert_eq!(
            normalized_shortcut_text("Control+Alt+Delete"),
            Some("Ctrl+Alt+delete".to_string())
        );
    }

    #[test]
    fn shortcut_matching_uses_custom_value_or_default() {
        let mut shortcuts = HashMap::new();
        shortcuts.insert("view.files".to_string(), "Cmd+Shift+F / Ctrl+F".to_string());
        assert!(shortcut_matches(&shortcuts, "view.files", "⌘⇧F"));
        assert!(shortcut_matches(&shortcuts, "view.files", "⌃F"));
        assert!(!shortcut_matches(&shortcuts, "view.files", "⌘2"));

        shortcuts.clear();
        let default_terminal = if cfg!(target_os = "macos") {
            "⌘1"
        } else {
            "Ctrl+1"
        };
        assert!(shortcut_matches(
            &shortcuts,
            "view.terminal",
            default_terminal
        ));
    }

    #[test]
    fn file_search_status_message_reports_match_position() {
        assert_eq!(
            file_search_status_message(0, 0),
            "file search has no matches"
        );
        assert_eq!(file_search_status_message(1, 3), "file search match 2 of 3");
    }
}
