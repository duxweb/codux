mod ai_history;
mod ai_history_indexer;
mod ai_runtime;
mod ai_usage_store;
mod app_icon;
mod app_info;
mod app_settings;
mod background_queue;
mod dialog;
mod files;
mod git;
mod i18n;
mod llm;
mod memory;
mod notify_channels;
mod paths;
mod performance;
mod pet;
mod power;
mod project_activity;
mod project_store;
mod remote_p2p;
mod runtime_trace;
mod ssh;
mod terminal;
mod worktree;

use aes_gcm::aead::{Aead as _, KeyInit as _, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use ai_history::{AIGlobalHistorySnapshot, AIHistoryProjectRequest};
use ai_history_indexer::AIHistoryIndexer;
use ai_history_indexer::AIHistoryProjectState;
use ai_runtime::{
    AIRuntimeBridge, AIRuntimeBridgeSnapshot, AIRuntimeContextSnapshot, AIRuntimeProbeRequest,
    AIRuntimeStateSnapshot,
};
use app_icon::apply_app_icon;
use app_info::{
    AppAboutMetadata, DiagnosticsExportRequest, DiagnosticsExportResult, UpdateInstallResult,
    UpdateStatus,
};
use app_settings::{
    locale_from_language_setting, sync_process_locale_preference, AIProviderSettings, AppSettings,
    AppSettingsStore,
};
use dialog::{
    localized_open_dialog as open_localized_dialog, localized_save_dialog as save_localized_dialog,
    LocalizedOpenDialogRequest, LocalizedSaveDialogRequest,
};
use files::{
    file_copy as copy_file_path, file_create_dir as create_file_dir,
    file_create_file as create_file_file, file_delete as delete_file_path,
    file_import_external as import_external_file_paths, file_list_children as list_file_children,
    file_open as open_file_path, file_read as read_file_path, file_rename as rename_file_path,
    file_reveal as reveal_file_path, file_write as write_file_path, FileChildrenRequest,
    FileCopyRequest, FileCreateRequest, FileEntry, FileExternalCopyRequest, FilePathRequest,
    FileReadResult, FileRenameRequest, FileWatchManager, FileWriteRequest,
};
use futures_util::{SinkExt, StreamExt};
use git::{
    git_add_remote as perform_git_add_remote,
    git_amend_last_commit_message as perform_git_amend_last_commit_message,
    git_append_gitignore as perform_git_append_gitignore, git_branches as load_git_branches,
    git_checkout_branch as perform_git_checkout_branch,
    git_checkout_commit as perform_git_checkout_commit,
    git_checkout_remote_branch as perform_git_checkout_remote_branch,
    git_clone as perform_git_clone, git_commit as perform_git_commit,
    git_commit_action as perform_git_commit_action, git_create_branch as perform_git_create_branch,
    git_delete_branch as perform_git_delete_branch, git_diff_file as load_git_diff_file,
    git_discard as perform_git_discard, git_fetch_with_cancel as perform_git_fetch_with_cancel,
    git_force_push_with_cancel as perform_git_force_push_with_cancel,
    git_head_commit_pushed as load_git_head_commit_pushed, git_init as perform_git_init,
    git_last_commit_message as load_git_last_commit_message,
    git_merge_branch as perform_git_merge_branch,
    git_pull_with_cancel as perform_git_pull_with_cancel,
    git_push_remote_branch_with_cancel as perform_git_push_remote_branch_with_cancel,
    git_push_remote_with_cancel as perform_git_push_remote_with_cancel,
    git_push_with_cancel as perform_git_push_with_cancel,
    git_remove_remote as perform_git_remove_remote,
    git_restore_commit as perform_git_restore_commit,
    git_revert_commit as perform_git_revert_commit, git_review as load_git_review,
    git_review_diff_file as load_git_review_diff_file,
    git_review_file_content as load_git_review_file_content,
    git_squash_merge_branch as perform_git_squash_merge_branch, git_stage as perform_git_stage,
    git_status as load_git_status, git_sync_with_cancel as perform_git_sync_with_cancel,
    git_undo_last_commit as perform_git_undo_last_commit, git_unstage as perform_git_unstage,
    GitBranchRequest, GitBranchesSnapshot, GitCancelToken, GitCloneRequest, GitCommitActionRequest,
    GitCommitRefRequest, GitCommitRequest, GitCreateBranchRequest, GitDeleteBranchRequest,
    GitDiffRequest, GitDiffSnapshot, GitPathsRequest, GitPushRemoteBranchRequest,
    GitPushRemoteRequest, GitRemoteRequest, GitRestoreCommitRequest, GitReviewContentRequest,
    GitReviewContentSnapshot, GitReviewDiffRequest, GitReviewSnapshot, GitStatusSnapshot,
    GitWatchManager, GitWatchRegistration,
};
use hkdf::Hkdf;
use i18n::I18nBundle;
use llm::{
    LLMCompletionRequest, LLMCompletionResponse, LLMProviderTestResult, PetIdleSpeechRequest,
    PetIdleSpeechResponse,
};
use memory::{
    MemoryExtractionStatusSnapshot, MemoryManagementRequest, MemoryManagementSnapshot,
    MemoryManagerSnapshot, MemoryManagerSnapshotRequest, MemoryProjectMigrationRequest,
    MemoryProjectProfileRefreshResult, MemoryQueueStatusEvent, MemoryStore, MemorySummary,
    MemorySummaryUpdateRequest,
};
use notify_channels::{
    dispatch_notification_channels, NotificationDispatchRequest, NotificationDispatchResult,
};
use performance::{PerformanceMonitor, PerformanceSnapshot};
use pet::{
    hydrate_custom_pet_data_url, PetCatalog, PetClaimRequest, PetCustomPet,
    PetCustomPetInstallPreview, PetCustomPetInstallRequest, PetRefreshRequest, PetRenameRequest,
    PetRestoreRequest, PetSnapshot, PetStore,
};
use power::PowerManager;
use project_activity::{GitReviewEvent, ProjectActivityCoordinator, WorktreeSnapshotEvent};
use project_store::{
    ProjectCloseRequest, ProjectCreateRequest, ProjectDefaultPushRemoteRequest,
    ProjectListSnapshot, ProjectReorderRequest, ProjectSelectWorktreeRequest, ProjectStore,
    ProjectSummary, ProjectUpdateRequest, TerminalBottomTabRecord, TerminalLayoutRecord,
    TerminalTopPaneRecord,
};
use remote_p2p::{RemoteP2PHostTransport, RemoteP2PLane, RemoteP2PSignal};
use reqwest::header::CONTENT_TYPE;
use runtime_trace::{runtime_trace, runtime_trace_elapsed};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use ssh::{
    SSHLaunchCommand, SSHProfileTestResult, SSHProfileUpsertRequest, SSHProfilesSnapshot, SSHStore,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Seek as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::async_runtime::JoinHandle;
use tauri::ipc::{Channel, Response};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID};
use tauri::utils::config::Color;
use tauri::WindowEvent;
use tauri::Wry;
use tauri::{Emitter, Manager};
use tauri::{
    LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, Position, Size, WebviewUrl,
    WebviewWindowBuilder,
};
use terminal::{TerminalConfig, TerminalEvent, TerminalManager};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;
use worktree::{
    create_worktree, merge_worktree, remove_worktree, WorktreeCreateRequest, WorktreeMergeRequest,
    WorktreeRemoveRequest, WorktreeSnapshot,
};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopPetPlacementSnapshot {
    side: String,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopPetSavedOrigin {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectOpenApplicationSnapshot {
    id: String,
    label: String,
    category: String,
    installed: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectOpenApplicationRequest {
    project_path: String,
    application_id: String,
}

struct ProjectOpenApplicationSpec {
    id: &'static str,
    label: &'static str,
    category: &'static str,
    bundle_ids: &'static [&'static str],
    #[cfg(not(target_os = "macos"))]
    commands: &'static [&'static str],
}

#[cfg(target_os = "macos")]
macro_rules! project_open_application_spec {
    ($id:expr, $label:expr, $category:expr, $bundle_ids:expr, $commands:expr) => {
        ProjectOpenApplicationSpec {
            id: $id,
            label: $label,
            category: $category,
            bundle_ids: $bundle_ids,
        }
    };
}

#[cfg(not(target_os = "macos"))]
macro_rules! project_open_application_spec {
    ($id:expr, $label:expr, $category:expr, $bundle_ids:expr, $commands:expr) => {
        ProjectOpenApplicationSpec {
            id: $id,
            label: $label,
            category: $category,
            bundle_ids: $bundle_ids,
            commands: $commands,
        }
    };
}

const PROJECT_OPEN_APPLICATIONS: &[ProjectOpenApplicationSpec] = &[
    project_open_application_spec!(
        "vscode",
        "VS Code",
        "primary",
        &["com.microsoft.VSCode"],
        &["code"]
    ),
    project_open_application_spec!(
        "terminal",
        "Terminal",
        "primary",
        &["com.apple.Terminal"],
        &[
            "x-terminal-emulator",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal"
        ]
    ),
    project_open_application_spec!(
        "iterm",
        "iTerm2",
        "primary",
        &["com.googlecode.iterm2"],
        &["iterm2"]
    ),
    project_open_application_spec!(
        "ghostty",
        "Ghostty",
        "primary",
        &["com.mitchellh.ghostty"],
        &["ghostty"]
    ),
    project_open_application_spec!(
        "xcode",
        "Xcode",
        "primary",
        &["com.apple.dt.Xcode"],
        &["xed"]
    ),
    project_open_application_spec!(
        "intellijIdea",
        "IntelliJ IDEA",
        "ide",
        &["com.jetbrains.intellij", "com.jetbrains.intellij.ce"],
        &["idea", "idea64"]
    ),
    project_open_application_spec!(
        "webStorm",
        "WebStorm",
        "ide",
        &["com.jetbrains.WebStorm"],
        &["webstorm"]
    ),
    project_open_application_spec!(
        "phpStorm",
        "PhpStorm",
        "ide",
        &["com.jetbrains.PhpStorm"],
        &["phpstorm"]
    ),
    project_open_application_spec!(
        "pyCharm",
        "PyCharm",
        "ide",
        &["com.jetbrains.pycharm", "com.jetbrains.pycharm.ce"],
        &["pycharm"]
    ),
    project_open_application_spec!(
        "goLand",
        "GoLand",
        "ide",
        &["com.jetbrains.goland"],
        &["goland"]
    ),
    project_open_application_spec!(
        "clion",
        "CLion",
        "ide",
        &["com.jetbrains.CLion"],
        &["clion"]
    ),
    project_open_application_spec!(
        "rider",
        "Rider",
        "ide",
        &["com.jetbrains.rider"],
        &["rider"]
    ),
    project_open_application_spec!(
        "androidStudio",
        "Android Studio",
        "ide",
        &["com.google.android.studio"],
        &["studio", "android-studio"]
    ),
    project_open_application_spec!(
        "cursor",
        "Cursor",
        "ide",
        &["com.todesktop.230313mzl4w4u92", "com.yuxin.CursorPro"],
        &["cursor"]
    ),
    project_open_application_spec!("zed", "Zed", "ide", &["dev.zed.Zed"], &["zed"]),
    project_open_application_spec!(
        "sublimeText",
        "Sublime Text",
        "ide",
        &["com.sublimetext.4", "com.sublimetext.3"],
        &["subl", "sublime_text"]
    ),
    project_open_application_spec!(
        "windsurf",
        "Windsurf",
        "ide",
        &["com.exafunction.windsurf"],
        &["windsurf"]
    ),
];

struct MenuLabels {
    app_name: String,
    about: String,
    app_menu_settings: String,
    check_updates: String,
    services: String,
    hide_app: String,
    hide_others: String,
    show_all: String,
    quit: String,
    file: String,
    workspace: String,
    view: String,
    window: String,
    help: String,
    new_project: String,
    open_folder: String,
    close_current_project: String,
    close_all_projects: String,
    terminal: String,
    files: String,
    review: String,
    projects_sidebar: String,
    tasks_sidebar: String,
    git_panel: String,
    files_panel: String,
    ai_panel: String,
    ssh_panel: String,
    create_split: String,
    create_tab: String,
    diagnostics: String,
    runtime_log: String,
    live_log: String,
    website: String,
    github: String,
    minimize: String,
    zoom: String,
    close_window: String,
    #[cfg(debug_assertions)]
    devtools: String,
}

impl MenuLabels {
    fn load(settings: &AppSettings) -> Self {
        let locale = locale_from_language_setting(&settings.language);
        let tr = |key: &str, fallback: &str| i18n::translate(&locale, key, fallback);
        let app_name = paths::app_display_name();
        Self {
            app_name: app_name.to_string(),
            about: tr("menu.app.about_format", "About %@").replace("%@", app_name),
            app_menu_settings: tr("menu.app.settings", "Settings..."),
            check_updates: tr("menu.app.check_updates", "Check for Updates..."),
            services: tr("menu.app.services", "Services"),
            hide_app: tr("menu.app.hide_format", "Hide %@").replace("%@", app_name),
            hide_others: tr("menu.app.hide_others", "Hide Others"),
            show_all: tr("menu.app.show_all", "Show All"),
            quit: tr("menu.app.quit_format", "Quit %@").replace("%@", app_name),
            file: tr("menu.file", "File"),
            workspace: tr("menu.workspace", "Workspace"),
            view: tr("menu.view", "View"),
            window: tr("menu.window", "Window"),
            help: tr("menu.help", "Help"),
            new_project: tr("menu.file.new_project", "New Project"),
            open_folder: tr("menu.file.open_folder", "Open Folder..."),
            close_current_project: tr("menu.file.close_current_project", "Close Current Project"),
            close_all_projects: tr("menu.file.close_all_projects", "Close All Projects..."),
            terminal: tr("workspace.create_split.terminal", "Terminal"),
            files: tr("titlebar.files", "Files"),
            review: tr("titlebar.review", "Review"),
            projects_sidebar: tr("menu.view.projects_sidebar", "Projects Sidebar"),
            tasks_sidebar: tr("menu.view.tasks_sidebar", "Worktree Sidebar"),
            git_panel: tr("menu.view.open_git_panel", "Open Git Panel"),
            files_panel: tr("menu.view.open_files_panel", "Open Files Panel"),
            ai_panel: tr("menu.view.open_ai_panel", "Open AI Panel"),
            ssh_panel: tr("menu.view.open_ssh_panel", "Open SSH Panel"),
            create_split: tr("menu.view.create_split", "Create Split"),
            create_tab: tr("menu.view.create_tab", "Create Tab"),
            diagnostics: tr("menu.help.export_diagnostics", "Export Diagnostics..."),
            runtime_log: tr("menu.help.open_runtime_log", "Open Runtime Log"),
            live_log: tr("menu.help.open_live_log", "Open Live Log"),
            website: tr("menu.help.website", "Official Website"),
            github: tr("menu.help.github", "GitHub"),
            minimize: tr("menu.window.minimize", "Minimize"),
            zoom: tr("menu.window.zoom", "Zoom"),
            close_window: tr("menu.file.close_window", "Close Window"),
            #[cfg(debug_assertions)]
            devtools: tr("menu.help.developer_tools", "Developer Tools"),
        }
    }
}

struct MenuAccelerators {
    new_project: String,
    settings: String,
    view_terminal: String,
    view_files: String,
    view_review: String,
    create_task: String,
    editor_save: String,
    editor_search: String,
    close_active: String,
}

impl MenuAccelerators {
    fn load(settings: &AppSettings) -> Self {
        Self {
            new_project: configured_accelerator(settings, "project.create", "CmdOrCtrl+N"),
            settings: configured_accelerator(settings, "settings.open", "CmdOrCtrl+,"),
            view_terminal: configured_accelerator(settings, "view.terminal", "CmdOrCtrl+1"),
            view_files: configured_accelerator(settings, "view.files", "CmdOrCtrl+2"),
            view_review: configured_accelerator(settings, "view.review", "CmdOrCtrl+3"),
            create_task: configured_accelerator(settings, "task.create", "CmdOrCtrl+Shift+N"),
            editor_save: configured_accelerator(settings, "editor.save", "CmdOrCtrl+S"),
            editor_search: configured_accelerator(settings, "editor.search", "CmdOrCtrl+F"),
            close_active: configured_accelerator(settings, "close.active", "CmdOrCtrl+W"),
        }
    }
}

#[derive(Clone)]
struct AppState {
    terminals: Arc<TerminalManager>,
    remote: Arc<RemoteHostService>,
    performance: Arc<PerformanceMonitor>,
    power: Arc<PowerManager>,
    ai_runtime: Arc<AIRuntimeBridge>,
    ai_history: Arc<AIHistoryIndexer>,
    memory: Arc<MemoryStore>,
    settings: Arc<AppSettingsStore>,
    projects: Arc<ProjectStore>,
    project_activity: Arc<ProjectActivityCoordinator>,
    pet: Arc<PetStore>,
    ssh: Arc<SSHStore>,
    git_watch: Arc<GitWatchManager>,
    git_cancels: Arc<Mutex<HashMap<String, GitCancelToken>>>,
    file_watch: Arc<FileWatchManager>,
    desktop_pet_hit_state: Arc<DesktopPetHitState>,
    is_exiting: Arc<AtomicBool>,
}

#[derive(Default)]
struct DesktopPetHitState {
    has_bubble: AtomicBool,
    hit_test_running: AtomicBool,
}

#[derive(Debug, Clone, Copy)]
struct DesktopPetHitLayout {
    position: PhysicalPosition<i32>,
    width: f64,
    height: f64,
    scale_factor: f64,
    side: &'static str,
}

const DESKTOP_PET_LABEL: &str = "desktop-pet";
const DESKTOP_PET_BASE_WIDTH: f64 = 352.0;
const DESKTOP_PET_BASE_HEIGHT: f64 = 202.0;
const DESKTOP_PET_SPRITE_SIZE: f64 = 112.0;
const DESKTOP_PET_BUBBLE_WIDTH: f64 = 198.0;
const DESKTOP_PET_BUBBLE_HEIGHT: f64 = 78.0;
const DESKTOP_PET_BUBBLE_TOP: f64 = 52.0;
const DESKTOP_PET_SPRITE_VISIBLE_INSET_X: f64 = 18.0;
const DESKTOP_PET_SPRITE_VISIBLE_INSET_TOP: f64 = 12.0;
const DESKTOP_PET_SPRITE_VISIBLE_INSET_BOTTOM: f64 = 4.0;
const DESKTOP_PET_MARGIN: f64 = 24.0;
const DESKTOP_PET_DEFAULT_BOTTOM_MARGIN: f64 = 110.0;
const DESKTOP_PET_MUTE_30_MINUTES: &str = "desktop-pet:mute-30-minutes";
const DESKTOP_PET_MUTE_1_HOUR: &str = "desktop-pet:mute-1-hour";
const DESKTOP_PET_MUTE_TODAY: &str = "desktop-pet:mute-today";
const DESKTOP_PET_SKIP_LINE: &str = "desktop-pet:skip-line";
const DESKTOP_PET_SPEAK_MORE: &str = "desktop-pet:speak-more";
const DESKTOP_PET_SPEAK_LESS: &str = "desktop-pet:speak-less";
const DESKTOP_PET_HIDE: &str = "desktop-pet:hide";

fn sync_desktop_pet_window(app: &tauri::AppHandle, settings: &AppSettings, pet: &PetSnapshot) {
    let should_show =
        settings.pet.enabled && settings.pet.desktop_widget && pet.claimed_at.is_some();
    if !should_show {
        runtime_trace(
            "desktop-pet",
            &format!(
                "sync hide enabled={} desktop_widget={} claimed={}",
                settings.pet.enabled,
                settings.pet.desktop_widget,
                pet.claimed_at.is_some()
            ),
        );
        if let Some(window) = app.get_webview_window(DESKTOP_PET_LABEL) {
            let _ = window.set_ignore_cursor_events(true);
            if let Err(error) = window.hide() {
                runtime_trace("desktop-pet", &format!("hide_failed error={error}"));
            }
        }
        return;
    }

    match show_desktop_pet_window(app, settings) {
        Ok(()) => runtime_trace("desktop-pet", "sync show ok"),
        Err(error) => runtime_trace("desktop-pet", &format!("show_failed error={error}")),
    }
}

fn show_desktop_pet_window(app: &tauri::AppHandle, settings: &AppSettings) -> tauri::Result<()> {
    let width = DESKTOP_PET_BASE_WIDTH;
    let height = DESKTOP_PET_BASE_HEIGHT;

    if let Some(window) = app.get_webview_window(DESKTOP_PET_LABEL) {
        let previous_position = window.outer_position().ok();
        let previous_size = window.inner_size().ok();
        let _ = window.set_size(Size::Logical(LogicalSize::new(width, height)));
        let _ = window.set_min_size(Some(Size::Logical(LogicalSize::new(width, height))));
        let _ = window.set_max_size(Some(Size::Logical(LogicalSize::new(width, height))));
        let _ = window.set_always_on_top(true);
        let _ = window.set_focusable(false);
        let _ = window.set_ignore_cursor_events(false);
        start_desktop_pet_hit_test_loop(app.clone());
        if let (Some(position), Some(size)) = (previous_position, previous_size) {
            let next_position =
                desktop_pet_clamped_position_for_window(app, position, size, width, height);
            let _ = window.set_position(Position::Physical(next_position));
            desktop_pet_save_origin_from_window(&window);
            desktop_pet_emit_placement(&window);
        }
        if !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
        return Ok(());
    }

    let title = {
        let locale = locale_from_language_setting(&settings.language);
        i18n::translate(&locale, "settings.pet.desktop_widget", "Desktop Pet")
    };
    let builder = WebviewWindowBuilder::new(
        app,
        DESKTOP_PET_LABEL,
        WebviewUrl::App("desktop-pet.html".into()),
    )
    .title(title)
    .inner_size(width, height)
    .min_inner_size(width, height)
    .max_inner_size(width, height)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .background_color(Color(0, 0, 0, 0))
    .visible(false)
    .focused(false)
    .focusable(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .shadow(false)
    .accept_first_mouse(true);

    let builder = if let Some(position) = desktop_pet_initial_position(app, width, height) {
        builder.position(position.x, position.y)
    } else {
        builder
    };
    let window = builder.build()?;
    let placement_window = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Moved(_) = event {
            desktop_pet_save_origin_from_window(&placement_window);
            desktop_pet_emit_placement(&placement_window);
        }
    });
    let _ = window.set_visible_on_all_workspaces(true);
    let _ = window.set_ignore_cursor_events(false);
    let _ = window.show();
    start_desktop_pet_hit_test_loop(app.clone());
    desktop_pet_emit_placement(&window);
    Ok(())
}

fn start_desktop_pet_hit_test_loop(app: tauri::AppHandle) {
    let Some(hit_state) = app
        .try_state::<AppState>()
        .map(|state| Arc::clone(&state.desktop_pet_hit_state))
    else {
        return;
    };
    if hit_state.hit_test_running.swap(true, Ordering::AcqRel) {
        return;
    }

    thread::spawn(move || {
        let mut last_click_through = true;
        let mut cached_layout: Option<DesktopPetHitLayout> = None;
        let mut ticks_since_layout_refresh = 0_u32;
        loop {
            let Some(window) = app.get_webview_window(DESKTOP_PET_LABEL) else {
                break;
            };
            if !window.is_visible().unwrap_or(false) {
                break;
            }
            if cached_layout.is_none() || ticks_since_layout_refresh >= 10 {
                cached_layout = desktop_pet_hit_layout(&window);
                ticks_since_layout_refresh = 0;
            }
            let has_bubble = hit_state.has_bubble.load(Ordering::Relaxed);
            let click_through =
                desktop_pet_should_click_through(&window, cached_layout, has_bubble);
            if click_through != last_click_through {
                let _ = window.set_ignore_cursor_events(click_through);
                last_click_through = click_through;
            }
            ticks_since_layout_refresh = ticks_since_layout_refresh.saturating_add(1);
            thread::sleep(Duration::from_millis(if click_through { 220 } else { 80 }));
        }
        hit_state.hit_test_running.store(false, Ordering::Release);
    });
}

fn desktop_pet_hit_layout(window: &tauri::WebviewWindow<Wry>) -> Option<DesktopPetHitLayout> {
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    let scale_factor = window.scale_factor().unwrap_or(1.0).max(0.1);
    Some(DesktopPetHitLayout {
        position,
        width: f64::from(size.width) / scale_factor,
        height: f64::from(size.height) / scale_factor,
        scale_factor,
        side: desktop_pet_side_for_position(window, position).unwrap_or("left"),
    })
}

fn desktop_pet_should_click_through(
    window: &tauri::WebviewWindow<Wry>,
    layout: Option<DesktopPetHitLayout>,
    has_bubble: bool,
) -> bool {
    let Some(layout) = layout else {
        return true;
    };
    let Ok(cursor) = window.cursor_position() else {
        return true;
    };
    let local_x = (cursor.x - f64::from(layout.position.x)) / layout.scale_factor;
    let local_y = (cursor.y - f64::from(layout.position.y)) / layout.scale_factor;
    if local_x < 0.0 || local_y < 0.0 || local_x > layout.width || local_y > layout.height {
        return true;
    }
    !desktop_pet_local_point_is_hotspot(layout, local_x, local_y, has_bubble)
}

fn desktop_pet_local_point_is_hotspot(
    layout: DesktopPetHitLayout,
    x: f64,
    y: f64,
    has_bubble: bool,
) -> bool {
    let sprite_x = if layout.side == "right" {
        24.0 + DESKTOP_PET_SPRITE_VISIBLE_INSET_X
    } else {
        layout.width - 24.0 - DESKTOP_PET_SPRITE_SIZE + DESKTOP_PET_SPRITE_VISIBLE_INSET_X
    };
    let sprite_y =
        layout.height - 8.0 - DESKTOP_PET_SPRITE_SIZE + DESKTOP_PET_SPRITE_VISIBLE_INSET_TOP;
    let sprite_width = DESKTOP_PET_SPRITE_SIZE - DESKTOP_PET_SPRITE_VISIBLE_INSET_X * 2.0;
    let sprite_height = DESKTOP_PET_SPRITE_SIZE
        - DESKTOP_PET_SPRITE_VISIBLE_INSET_TOP
        - DESKTOP_PET_SPRITE_VISIBLE_INSET_BOTTOM;
    let in_sprite = x >= sprite_x
        && x <= sprite_x + sprite_width
        && y >= sprite_y
        && y <= sprite_y + sprite_height;
    let in_bubble = if has_bubble {
        let bubble_x = if layout.side == "right" {
            layout.width - 8.0 - DESKTOP_PET_BUBBLE_WIDTH
        } else {
            8.0
        };
        let bubble_y = DESKTOP_PET_BUBBLE_TOP;
        x >= bubble_x
            && x <= bubble_x + DESKTOP_PET_BUBBLE_WIDTH
            && y >= bubble_y
            && y <= bubble_y + DESKTOP_PET_BUBBLE_HEIGHT
    } else {
        false
    };
    in_sprite || in_bubble
}

fn desktop_pet_initial_position(
    app: &tauri::AppHandle,
    width: f64,
    height: f64,
) -> Option<LogicalPosition<f64>> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let scale_factor = monitor.scale_factor().max(0.1);
    let work_area = monitor.work_area();
    let position = desktop_pet_saved_origin()
        .map(|origin| LogicalPosition::new(origin.x, origin.y))
        .unwrap_or_else(|| {
            LogicalPosition::new(
                f64::from(work_area.position.x) / scale_factor
                    + f64::from(work_area.size.width) / scale_factor
                    - width
                    - DESKTOP_PET_MARGIN,
                f64::from(work_area.position.y) / scale_factor
                    + f64::from(work_area.size.height) / scale_factor
                    - height
                    - DESKTOP_PET_DEFAULT_BOTTOM_MARGIN,
            )
        });
    Some(desktop_pet_clamped_logical_position(
        app, position, width, height,
    ))
}

fn desktop_pet_saved_origin() -> Option<DesktopPetSavedOrigin> {
    let data = fs::read(desktop_pet_placement_file_path()).ok()?;
    let origin: DesktopPetSavedOrigin = serde_json::from_slice(&data).ok()?;
    if origin.x.is_finite() && origin.y.is_finite() {
        Some(origin)
    } else {
        None
    }
}

fn desktop_pet_placement_file_path() -> PathBuf {
    paths::app_support_dir().join("desktop-pet-placement.json")
}

fn desktop_pet_save_origin_from_window(window: &tauri::WebviewWindow<Wry>) {
    let Ok(position) = window.outer_position() else {
        return;
    };
    let scale_factor = window.scale_factor().unwrap_or(1.0).max(0.1);
    let logical: LogicalPosition<f64> = position.to_logical(scale_factor);
    let origin = DesktopPetSavedOrigin {
        x: logical.x,
        y: logical.y,
    };
    let path = desktop_pet_placement_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_vec_pretty(&origin) {
        let _ = fs::write(path, data);
    }
}

fn desktop_pet_clamped_logical_position(
    app: &tauri::AppHandle,
    position: LogicalPosition<f64>,
    width: f64,
    height: f64,
) -> LogicalPosition<f64> {
    let monitor = app
        .monitor_from_point(position.x, position.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return LogicalPosition::new(position.x.max(0.0), position.y.max(0.0));
    };
    let scale_factor = monitor.scale_factor().max(0.1);
    let work_area = monitor.work_area();
    let min_x = f64::from(work_area.position.x) / scale_factor;
    let min_y = f64::from(work_area.position.y) / scale_factor;
    let max_x = (min_x + f64::from(work_area.size.width) / scale_factor - width).max(min_x);
    let max_y = (min_y + f64::from(work_area.size.height) / scale_factor - height).max(min_y);
    LogicalPosition::new(
        position.x.clamp(min_x, max_x),
        position.y.clamp(min_y, max_y),
    )
}

fn desktop_pet_clamped_position_for_window(
    app: &tauri::AppHandle,
    previous_position: PhysicalPosition<i32>,
    previous_size: PhysicalSize<u32>,
    width: f64,
    height: f64,
) -> PhysicalPosition<i32> {
    let previous_width = f64::from(previous_size.width).max(1.0);
    let previous_height = f64::from(previous_size.height).max(1.0);
    let center_x = f64::from(previous_position.x) + previous_width / 2.0;
    let center_y = f64::from(previous_position.y) + previous_height / 2.0;
    let monitor = app
        .monitor_from_point(center_x, center_y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return previous_position;
    };
    let scale_factor = monitor.scale_factor().max(0.1);
    let next_width = width * scale_factor;
    let next_height = height * scale_factor;
    let work_area = monitor.work_area();
    let min_x = f64::from(work_area.position.x);
    let min_y = f64::from(work_area.position.y);
    let max_x = (min_x + f64::from(work_area.size.width) - next_width).max(min_x);
    let max_y = (min_y + f64::from(work_area.size.height) - next_height).max(min_y);
    let x = (center_x - next_width / 2.0).clamp(min_x, max_x).round() as i32;
    let y = (center_y - next_height / 2.0).clamp(min_y, max_y).round() as i32;
    PhysicalPosition::new(x, y)
}

fn desktop_pet_emit_placement(window: &tauri::WebviewWindow<Wry>) {
    let snapshot = desktop_pet_placement_for_window(window);
    let _ = window.emit("desktop-pet:placement", snapshot);
}

fn desktop_pet_placement_for_window(
    window: &tauri::WebviewWindow<Wry>,
) -> DesktopPetPlacementSnapshot {
    let side = window
        .outer_position()
        .ok()
        .and_then(|position| desktop_pet_side_for_position(window, position))
        .unwrap_or("left");
    DesktopPetPlacementSnapshot {
        side: side.to_string(),
    }
}

fn desktop_pet_side_for_position(
    window: &tauri::WebviewWindow<Wry>,
    position: PhysicalPosition<i32>,
) -> Option<&'static str> {
    let size = window.inner_size().ok()?;
    let center_x = f64::from(position.x) + f64::from(size.width) / 2.0;
    let monitor = window
        .monitor_from_point(
            center_x,
            f64::from(position.y) + f64::from(size.height) / 2.0,
        )
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let work_area = monitor.work_area();
    let screen_mid_x = f64::from(work_area.position.x) + f64::from(work_area.size.width) / 2.0;
    let on_right_half = center_x > screen_mid_x;
    Some(if on_right_half { "left" } else { "right" })
}

#[tauri::command]
fn terminal_create(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    config: TerminalConfig,
    on_data: Channel<Response>,
    on_exit: Channel<i32>,
) -> Result<String, String> {
    create_terminal_session(
        Arc::clone(&state.remote),
        &state.terminals,
        app,
        config,
        Some(on_data),
        Some(on_exit),
    )
}

#[tauri::command]
fn terminal_attach(
    state: tauri::State<'_, AppState>,
    session_id: String,
    on_data: Channel<Response>,
    on_exit: Channel<i32>,
) -> Result<(), String> {
    state
        .terminals
        .attach_channels(&session_id, Some(on_data), Some(on_exit))
        .map_err(|error| error.to_string())
}

fn create_terminal_session(
    remote: Arc<RemoteHostService>,
    terminals: &TerminalManager,
    _app: tauri::AppHandle,
    config: TerminalConfig,
    on_data: Option<Channel<Response>>,
    on_exit: Option<Channel<i32>>,
) -> Result<String, String> {
    terminals
        .create_with_channels(config, move |event| {
            remote.handle_terminal_event(event);
        }, on_data, on_exit)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn terminal_write(
    state: tauri::State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    state
        .terminals
        .write(&session_id, data.as_bytes())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn terminal_resize(
    state: tauri::State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state
        .terminals
        .resize(&session_id, cols, rows)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn terminal_interrupt(state: tauri::State<'_, AppState>, session_id: String) -> Result<(), String> {
    state
        .terminals
        .write(&session_id, &[0x03])
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn terminal_clear_history(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state
        .terminals
        .clear_history(&session_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn terminal_escape(state: tauri::State<'_, AppState>, session_id: String) -> Result<(), String> {
    state
        .terminals
        .write(&session_id, &[0x1b])
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn terminal_kill(state: tauri::State<'_, AppState>, session_id: String) -> Result<(), String> {
    state
        .terminals
        .kill(&session_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn terminal_snapshot(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    let started_at = Instant::now();
    let snapshot = state
        .terminals
        .snapshot(&session_id)
        .map_err(|error| error.to_string())?;
    runtime_trace_elapsed(
        "terminal",
        "snapshot",
        started_at,
        &format!(
            "session={} chars={} bytes={}",
            session_id,
            snapshot.chars().count(),
            snapshot.len()
        ),
    );
    Ok(snapshot)
}

#[tauri::command]
fn runtime_trace_frontend(category: String, message: String) {
    runtime_trace(&category, &message);
}

#[tauri::command]
fn ai_runtime_snapshot(state: tauri::State<'_, AppState>) -> AIRuntimeBridgeSnapshot {
    state.ai_runtime.snapshot()
}

#[tauri::command]
fn ai_runtime_probe(
    state: tauri::State<'_, AppState>,
    request: AIRuntimeProbeRequest,
) -> Option<AIRuntimeContextSnapshot> {
    state.ai_runtime.probe(request)
}

#[tauri::command]
fn ai_runtime_state_snapshot(state: tauri::State<'_, AppState>) -> AIRuntimeStateSnapshot {
    state.ai_runtime.state_snapshot()
}

#[tauri::command]
fn app_runtime_ready(state: tauri::State<'_, AppState>, app: tauri::AppHandle) {
    let started_at = Instant::now();
    let snapshot = state.projects.list_snapshot();
    let project_count = snapshot.projects.len();
    let selected_project_id = snapshot
        .selected_project_id
        .clone()
        .unwrap_or_else(|| "none".to_string());
    runtime_trace(
        "startup",
        &format!("app_runtime_ready start projects={project_count} selected={selected_project_id}"),
    );
    let _ = app.emit("project:updated", snapshot.clone());
    let _ = app.emit(
        "terminal-layouts:snapshot",
        state.projects.terminal_layouts_snapshot(),
    );
    let _ = app.emit("remote:status", state.remote.snapshot());
    let _ = app.emit("ai-runtime:state", state.ai_runtime.state_snapshot());
    if let Some(project) = selected_project_summary(&snapshot) {
        state.project_activity.refresh_git_sidecars_by_path(
            app,
            Arc::clone(&state.projects),
            project.path,
        );
    }
    runtime_trace_elapsed(
        "startup",
        "app_runtime_ready finish",
        started_at,
        &format!("projects={project_count} selected={selected_project_id}"),
    );
}

#[tauri::command]
fn app_window_state(state: tauri::State<'_, AppState>, visible: bool, focused: bool) {
    state.project_activity.mark_main_window_visible(visible);
    state.project_activity.mark_main_window_focused(focused);
}

#[tauri::command]
fn ai_runtime_dismiss_completion(state: tauri::State<'_, AppState>, project_id: String) -> bool {
    state.ai_runtime.dismiss_completion(project_id)
}

#[tauri::command]
fn app_settings_get(state: tauri::State<'_, AppState>) -> AppSettings {
    state.settings.snapshot()
}

#[tauri::command]
fn app_settings_set(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let next = state.settings.replace(settings)?;
    sync_process_locale_preference(&next);
    state
        .ai_runtime
        .sync_window_state(&app, state.settings.as_ref());
    let _ = state.power.set_sleep_prevention(next.sleep_mode.clone());
    apply_app_icon(&app, &next.icon_style)?;
    rebuild_app_menu(&app, &next)?;
    if let Ok(pet) = state.pet.snapshot() {
        sync_desktop_pet_window(&app, &next, &pet);
    }
    state.remote.sync_settings(app.clone());
    let _ = app.emit("settings:updated", next.clone());
    Ok(next)
}

#[tauri::command]
fn localized_open_dialog(
    request: LocalizedOpenDialogRequest,
) -> Result<Option<Vec<String>>, String> {
    open_localized_dialog(request)
}

#[tauri::command]
fn localized_save_dialog(request: LocalizedSaveDialogRequest) -> Result<Option<String>, String> {
    save_localized_dialog(request)
}

#[tauri::command]
fn desktop_pet_placement(window: tauri::WebviewWindow<Wry>) -> DesktopPetPlacementSnapshot {
    desktop_pet_placement_for_window(&window)
}

#[tauri::command]
fn desktop_pet_set_bubble_visible(state: tauri::State<'_, AppState>, visible: bool) {
    state
        .desktop_pet_hit_state
        .has_bubble
        .store(visible, Ordering::Relaxed);
}

#[tauri::command]
fn desktop_pet_start_drag(window: tauri::WebviewWindow<Wry>) -> Result<(), String> {
    window.start_dragging().map_err(|error| error.to_string())
}

#[tauri::command]
fn desktop_pet_show_context_menu(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    window: tauri::WebviewWindow<Wry>,
) -> Result<(), String> {
    let settings = state.settings.snapshot();
    let locale = locale_from_language_setting(&settings.language);
    let tr = |key: &str, fallback: &str| i18n::translate(&locale, key, fallback);
    let mute_30 = MenuItem::with_id(
        &app,
        DESKTOP_PET_MUTE_30_MINUTES,
        tr("pet.desktop.mute_30_minutes", "Mute 30 Minutes"),
        true,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let mute_1_hour = MenuItem::with_id(
        &app,
        DESKTOP_PET_MUTE_1_HOUR,
        tr("pet.desktop.mute_1_hour", "Mute 1 Hour"),
        true,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let mute_today = MenuItem::with_id(
        &app,
        DESKTOP_PET_MUTE_TODAY,
        tr("pet.desktop.mute_today", "Mute Today"),
        true,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let skip = MenuItem::with_id(
        &app,
        DESKTOP_PET_SKIP_LINE,
        tr("pet.desktop.skip_line", "Skip Line"),
        true,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let speak_more = MenuItem::with_id(
        &app,
        DESKTOP_PET_SPEAK_MORE,
        tr("pet.desktop.speak_more", "Speak More"),
        true,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let speak_less = MenuItem::with_id(
        &app,
        DESKTOP_PET_SPEAK_LESS,
        tr("pet.desktop.speak_less", "Speak Less"),
        true,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let hide = MenuItem::with_id(
        &app,
        DESKTOP_PET_HIDE,
        tr("pet.desktop.hide", "Hide Desktop Pet"),
        true,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let separator_1 = PredefinedMenuItem::separator(&app).map_err(|error| error.to_string())?;
    let separator_2 = PredefinedMenuItem::separator(&app).map_err(|error| error.to_string())?;
    let menu = Menu::with_items(
        &app,
        &[
            &mute_30,
            &mute_1_hour,
            &mute_today,
            &separator_1,
            &skip,
            &speak_more,
            &speak_less,
            &separator_2,
            &hide,
        ],
    )
    .map_err(|error| error.to_string())?;
    window.popup_menu(&menu).map_err(|error| error.to_string())
}

#[tauri::command]
fn desktop_pet_sync_visibility(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let settings = state.settings.snapshot();
    let pet = state.pet.snapshot()?;
    let should_show =
        settings.pet.enabled && settings.pet.desktop_widget && pet.claimed_at.is_some();
    runtime_trace(
        "desktop-pet",
        &format!(
            "manual_sync should_show={should_show} enabled={} desktop_widget={} claimed={}",
            settings.pet.enabled,
            settings.pet.desktop_widget,
            pet.claimed_at.is_some()
        ),
    );
    sync_desktop_pet_window(&app, &settings, &pet);
    Ok(should_show)
}

fn update_desktop_pet_settings(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    apply: impl FnOnce(&mut AppSettings),
) -> Result<AppSettings, String> {
    let next = state.settings.update(apply)?;
    sync_process_locale_preference(&next);
    rebuild_app_menu(&app, &next)?;
    if let Ok(pet) = state.pet.snapshot() {
        sync_desktop_pet_window(&app, &next, &pet);
    }
    let _ = app.emit("settings:updated", next.clone());
    Ok(next)
}

#[tauri::command]
fn i18n_bundle_get() -> I18nBundle {
    i18n::i18n_bundle()
}

#[tauri::command]
async fn llm_complete(
    state: tauri::State<'_, AppState>,
    request: LLMCompletionRequest,
) -> Result<LLMCompletionResponse, String> {
    let settings = state.settings.snapshot().ai;
    llm::complete_with_settings(&settings, request).await
}

#[tauri::command]
async fn llm_provider_test(provider: AIProviderSettings) -> Result<LLMProviderTestResult, String> {
    llm::test_provider(provider).await
}

#[tauri::command]
async fn pet_idle_speech(
    state: tauri::State<'_, AppState>,
    request: PetIdleSpeechRequest,
) -> Result<PetIdleSpeechResponse, String> {
    let settings = state.settings.snapshot();
    llm::pet_idle_speech_with_settings(&settings.ai, &settings.language, request).await
}

#[tauri::command]
fn memory_extraction_status(
    state: tauri::State<'_, AppState>,
) -> Result<MemoryExtractionStatusSnapshot, String> {
    state
        .memory
        .extraction_status_snapshot()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_extraction_cancel(
    state: tauri::State<'_, AppState>,
) -> Result<MemoryExtractionStatusSnapshot, String> {
    state
        .memory
        .cancel_extraction_queue()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_management_snapshot(
    state: tauri::State<'_, AppState>,
    request: MemoryManagementRequest,
) -> Result<MemoryManagementSnapshot, String> {
    state
        .memory
        .management_snapshot(request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn memory_manager_snapshot(
    state: tauri::State<'_, AppState>,
    request: MemoryManagerSnapshotRequest,
) -> Result<MemoryManagerSnapshot, String> {
    let projects = state.projects.projects_snapshot();
    state
        .memory
        .manager_snapshot(request, &projects)
        .map_err(|error| error.to_string())
}

fn emit_memory_status_event(app: &tauri::AppHandle, event: MemoryQueueStatusEvent) {
    let _ = app.emit("memory:status", event.status);
    if let Some(manager) = event.manager {
        let _ = app.emit("memory:manager", manager);
    }
}

#[tauri::command]
fn memory_archive_entry(state: tauri::State<'_, AppState>, entry_id: String) -> Result<(), String> {
    state
        .memory
        .archive_entry(&entry_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_delete_entry(state: tauri::State<'_, AppState>, entry_id: String) -> Result<(), String> {
    state
        .memory
        .delete_entry(&entry_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_delete_summary(
    state: tauri::State<'_, AppState>,
    summary_id: String,
) -> Result<(), String> {
    state
        .memory
        .delete_summary(&summary_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_delete_project_profile(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<(), String> {
    state
        .memory
        .delete_project_profile(&project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_delete_project(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<(), String> {
    state
        .memory
        .delete_project_memory(&project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_migrate_project(
    state: tauri::State<'_, AppState>,
    request: MemoryProjectMigrationRequest,
) -> Result<(), String> {
    state
        .memory
        .migrate_project_memory(request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_update_summary(
    state: tauri::State<'_, AppState>,
    request: MemorySummaryUpdateRequest,
) -> Result<MemorySummary, String> {
    state
        .memory
        .update_summary(request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn memory_refresh_project_profile(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<MemoryProjectProfileRefreshResult, String> {
    let projects = state.projects.projects_snapshot();
    let project = projects
        .into_iter()
        .find(|project| project.id == project_id)
        .ok_or_else(|| "project not found".to_string())?;
    let settings = state.settings.reload_snapshot().ai;
    state
        .memory
        .force_refresh_project_profile_with_llm_detailed(&settings, &project)
        .await
        .ok_or_else(|| "failed to refresh project profile".to_string())
}

#[tauri::command]
async fn memory_index_now(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<MemoryExtractionStatusSnapshot, String> {
    let settings = state.settings.reload_snapshot().ai;
    let projects = state.projects.project_workspaces_snapshot();
    let root_projects = state.projects.projects_snapshot();
    let history_sessions = tauri::async_runtime::spawn_blocking(move || {
        ai_history::index_global_history_fresh(
            root_projects
                .into_iter()
                .map(|project| AIHistoryProjectRequest {
                    id: project.id,
                    name: project.name,
                    path: project.path,
                })
                .collect(),
        );
        ai_history::indexed_sessions_since(None)
    })
    .await
    .map_err(|error| error.to_string())?
    .unwrap_or_default();
    Arc::clone(&state.memory)
        .process_sessions_now(
            settings,
            projects,
            state.ai_runtime.state_snapshot().sessions,
            history_sessions,
            move |event| emit_memory_status_event(&app, event),
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn app_request_restart(app: tauri::AppHandle) {
    app.request_restart();
}

#[tauri::command]
async fn ai_history_project_summary(
    state: tauri::State<'_, AppState>,
    project: AIHistoryProjectRequest,
) -> Result<AIHistoryProjectState, String> {
    state.ai_history.project_summary(project).await
}

#[tauri::command]
fn ai_history_refresh_project(state: tauri::State<'_, AppState>, project: AIHistoryProjectRequest) {
    let summary = ProjectSummary {
        id: project.id,
        name: project.name,
        path: project.path,
        badge: String::new(),
        status: "active".to_string(),
        branch: "master".to_string(),
        changes: 0,
        badge_symbol: None,
        badge_color_hex: None,
        git_default_push_remote_name: None,
    };
    state
        .project_activity
        .refresh_ai_once(summary, Arc::clone(&state.ai_history));
}

#[tauri::command]
async fn ai_history_project_state(
    state: tauri::State<'_, AppState>,
    project: AIHistoryProjectRequest,
) -> Result<AIHistoryProjectState, String> {
    state.ai_history.project_state(project).await
}

#[tauri::command]
async fn ai_history_global_summary(
    state: tauri::State<'_, AppState>,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<AIGlobalHistorySnapshot, String> {
    state.ai_history.global_summary(projects).await
}

#[tauri::command]
async fn ai_history_refresh_global(
    state: tauri::State<'_, AppState>,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<(), String> {
    state.ai_history.refresh_global(projects).await
}

#[tauri::command]
async fn ai_history_global_state(
    state: tauri::State<'_, AppState>,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<Option<AIGlobalHistorySnapshot>, String> {
    state.ai_history.global_state(projects).await
}

#[tauri::command]
async fn ai_history_global_today_normalized_tokens() -> Result<i64, String> {
    tauri::async_runtime::spawn_blocking(ai_history::global_today_normalized_tokens)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn ai_history_session_rename(
    state: tauri::State<'_, AppState>,
    project: AIHistoryProjectRequest,
    session_id: String,
    title: String,
) -> Result<AIHistoryProjectState, String> {
    state
        .ai_history
        .rename_session(project, session_id, title)
        .await
}

#[tauri::command]
async fn ai_history_session_remove(
    state: tauri::State<'_, AppState>,
    project: AIHistoryProjectRequest,
    session_id: String,
) -> Result<AIHistoryProjectState, String> {
    state.ai_history.remove_session(project, session_id).await
}

fn emit_git_status_snapshot(
    app: &tauri::AppHandle,
    state: &AppState,
    project_path: &str,
    snapshot: GitStatusSnapshot,
) {
    let project = state.projects.workspace_summary_by_path(project_path);
    let _ = app.emit(
        "git:status",
        project_activity::GitStatusEvent {
            project_id: project
                .as_ref()
                .map(|value| value.id.clone())
                .unwrap_or_default(),
            project_name: project
                .as_ref()
                .map(|value| value.name.clone())
                .unwrap_or_default(),
            project_path: project
                .as_ref()
                .map(|value| value.path.clone())
                .unwrap_or_else(|| project_path.to_string()),
            snapshot,
        },
    );
}

fn emit_git_review_snapshot(
    app: &tauri::AppHandle,
    state: &AppState,
    project_path: &str,
    base_branch: Option<String>,
    snapshot: GitReviewSnapshot,
) {
    let project = state.projects.workspace_summary_by_path(project_path);
    let _ = app.emit(
        "git:review",
        GitReviewEvent {
            project_id: project
                .as_ref()
                .map(|value| value.id.clone())
                .unwrap_or_default(),
            project_name: project
                .as_ref()
                .map(|value| value.name.clone())
                .unwrap_or_default(),
            project_path: project
                .as_ref()
                .map(|value| value.path.clone())
                .unwrap_or_else(|| project_path.to_string()),
            base_branch,
            snapshot,
        },
    );
}

fn refresh_git_sidecars_for_path(app: &tauri::AppHandle, state: &AppState, project_path: &str) {
    state.project_activity.refresh_git_sidecars_by_path(
        app.clone(),
        Arc::clone(&state.projects),
        project_path.to_string(),
    );
}

fn run_git_status_command(
    state: &AppState,
    app: &tauri::AppHandle,
    project_path: String,
    action: impl FnOnce(String) -> Result<GitStatusSnapshot, String>,
) -> Result<GitStatusSnapshot, String> {
    let snapshot = action(project_path.clone())?;
    emit_git_status_snapshot(app, state, &project_path, snapshot.clone());
    refresh_git_sidecars_for_path(app, state, &project_path);
    Ok(snapshot)
}

fn git_cancel_key(project_path: &str) -> String {
    let normalized = Path::new(project_path.trim())
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(project_path.trim()));
    let mut key = normalized.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    #[cfg(windows)]
    {
        key = key.to_ascii_lowercase();
    }
    key
}

fn create_git_cancel_token(state: &AppState, project_path: &str) -> GitCancelToken {
    let token = Arc::new(AtomicBool::new(false));
    if let Ok(mut cancels) = state.git_cancels.lock() {
        cancels.insert(git_cancel_key(project_path), Arc::clone(&token));
    }
    token
}

fn clear_git_cancel_token(state: &AppState, project_path: &str, token: &GitCancelToken) {
    if let Ok(mut cancels) = state.git_cancels.lock() {
        let key = git_cancel_key(project_path);
        if cancels
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            cancels.remove(&key);
        }
    }
}

fn run_cancellable_git_status_command(
    state: &AppState,
    app: &tauri::AppHandle,
    project_path: String,
    action: impl FnOnce(String, GitCancelToken) -> Result<GitStatusSnapshot, String>,
) -> Result<GitStatusSnapshot, String> {
    let token = create_git_cancel_token(state, &project_path);
    let result = action(project_path.clone(), Arc::clone(&token));
    clear_git_cancel_token(state, &project_path, &token);
    let snapshot = result?;
    emit_git_status_snapshot(app, state, &project_path, snapshot.clone());
    refresh_git_sidecars_for_path(app, state, &project_path);
    Ok(snapshot)
}

async fn run_cancellable_git_status_command_blocking(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
    action: impl FnOnce(String, GitCancelToken) -> Result<GitStatusSnapshot, String> + Send + 'static,
) -> Result<GitStatusSnapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        run_cancellable_git_status_command(&state, &app, project_path, action)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn git_cancel(state: tauri::State<'_, AppState>, project_path: String) -> Result<(), String> {
    let key = git_cancel_key(&project_path);
    let Some(token) = state
        .git_cancels
        .lock()
        .map_err(|_| "Git cancel lock is poisoned.".to_string())?
        .get(&key)
        .cloned()
    else {
        runtime_trace(
            "git",
            &format!("cancel ignored path={project_path} reason=no-active-action"),
        );
        return Ok(());
    };
    token.store(true, Ordering::Relaxed);
    runtime_trace("git", &format!("cancel requested path={project_path}"));
    Ok(())
}

#[tauri::command]
fn git_status(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> GitStatusSnapshot {
    let started_at = Instant::now();
    let snapshot = load_git_status(project_path.clone());
    emit_git_status_snapshot(&app, &state, &project_path, snapshot.clone());
    runtime_trace_elapsed(
        "git",
        "git_status",
        started_at,
        &format!(
            "path={} repo={} staged={} unstaged={} untracked={} commits={}",
            project_path,
            snapshot.is_repository,
            snapshot.staged.len(),
            snapshot.unstaged.len(),
            snapshot.untracked.len(),
            snapshot.commits.len()
        ),
    );
    snapshot
}

#[tauri::command]
fn git_refresh_project(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) {
    runtime_trace(
        "git",
        &format!("git_refresh_project enqueue path={project_path}"),
    );
    state.project_activity.refresh_git_sidecars_by_path(
        app.clone(),
        Arc::clone(&state.projects),
        project_path.clone(),
    );
    if let Some(project) = state.projects.workspace_summary_by_path(&project_path) {
        state.project_activity.refresh_git_once(app, &project);
    }
}

#[tauri::command]
fn git_stage(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitPathsRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_stage(request)
    })
}

#[tauri::command]
fn git_unstage(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitPathsRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_unstage(request)
    })
}

#[tauri::command]
fn git_commit(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitCommitRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_commit(request)
    })
}

#[tauri::command]
fn git_commit_action(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitCommitActionRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_commit_action(request)
    })
}

#[tauri::command]
fn git_amend_last_commit_message(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitCommitRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_amend_last_commit_message(request)
    })
}

#[tauri::command]
fn git_last_commit_message(project_path: String) -> Result<String, String> {
    load_git_last_commit_message(project_path)
}

#[tauri::command]
fn git_undo_last_commit(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, project_path, perform_git_undo_last_commit)
}

#[tauri::command]
fn git_head_commit_pushed(project_path: String) -> Result<bool, String> {
    load_git_head_commit_pushed(project_path)
}

#[tauri::command]
async fn git_pull(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> Result<GitStatusSnapshot, String> {
    run_cancellable_git_status_command_blocking(state, app, project_path, |path, cancel| {
        perform_git_pull_with_cancel(path, Some(cancel))
    })
    .await
}

#[tauri::command]
async fn git_push(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> Result<GitStatusSnapshot, String> {
    run_cancellable_git_status_command_blocking(state, app, project_path, |path, cancel| {
        perform_git_push_with_cancel(path, Some(cancel))
    })
    .await
}

#[tauri::command]
async fn git_fetch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> Result<GitStatusSnapshot, String> {
    run_cancellable_git_status_command_blocking(state, app, project_path, |path, cancel| {
        perform_git_fetch_with_cancel(path, Some(cancel))
    })
    .await
}

#[tauri::command]
async fn git_sync(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> Result<GitStatusSnapshot, String> {
    run_cancellable_git_status_command_blocking(state, app, project_path, |path, cancel| {
        perform_git_sync_with_cancel(path, Some(cancel))
    })
    .await
}

#[tauri::command]
fn git_review(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
    base_branch: Option<String>,
) -> GitReviewSnapshot {
    let snapshot = load_git_review(project_path.clone(), base_branch.clone());
    emit_git_review_snapshot(&app, &state, &project_path, base_branch, snapshot.clone());
    snapshot
}

#[tauri::command]
fn git_init(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, project_path, perform_git_init)
}

#[tauri::command]
fn git_clone(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitCloneRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_clone(request)
    })
}

#[tauri::command]
fn git_discard(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitPathsRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_discard(request)
    })
}

#[tauri::command]
fn git_branches(project_path: String) -> GitBranchesSnapshot {
    load_git_branches(project_path)
}

#[tauri::command]
fn git_checkout_branch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitBranchRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_checkout_branch(request)
    })
}

#[tauri::command]
fn git_checkout_remote_branch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitBranchRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_checkout_remote_branch(request)
    })
}

#[tauri::command]
fn git_create_branch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitCreateBranchRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_create_branch(request)
    })
}

#[tauri::command]
fn git_merge_branch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitBranchRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_merge_branch(request)
    })
}

#[tauri::command]
fn git_squash_merge_branch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitBranchRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_squash_merge_branch(request)
    })
}

#[tauri::command]
fn git_delete_branch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitDeleteBranchRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_delete_branch(request)
    })
}

#[tauri::command]
async fn git_force_push(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> Result<GitStatusSnapshot, String> {
    run_cancellable_git_status_command_blocking(state, app, project_path, |path, cancel| {
        perform_git_force_push_with_cancel(path, Some(cancel))
    })
    .await
}

#[tauri::command]
async fn git_push_remote(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitPushRemoteRequest,
) -> Result<GitStatusSnapshot, String> {
    run_cancellable_git_status_command_blocking(
        state,
        app,
        request.project_path.clone(),
        |_, cancel| perform_git_push_remote_with_cancel(request, Some(cancel)),
    )
    .await
}

#[tauri::command]
async fn git_push_remote_branch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitPushRemoteBranchRequest,
) -> Result<GitStatusSnapshot, String> {
    run_cancellable_git_status_command_blocking(
        state,
        app,
        request.project_path.clone(),
        |_, cancel| perform_git_push_remote_branch_with_cancel(request, Some(cancel)),
    )
    .await
}

#[tauri::command]
fn git_checkout_commit(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitCommitRefRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_checkout_commit(request)
    })
}

#[tauri::command]
fn git_revert_commit(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitCommitRefRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_revert_commit(request)
    })
}

#[tauri::command]
fn git_restore_commit(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitRestoreCommitRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_restore_commit(request)
    })
}

#[tauri::command]
fn git_add_remote(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitRemoteRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_add_remote(request)
    })
}

#[tauri::command]
fn git_remove_remote(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitRemoteRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_remove_remote(request)
    })
}

#[tauri::command]
fn git_append_gitignore(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: GitPathsRequest,
) -> Result<GitStatusSnapshot, String> {
    run_git_status_command(&state, &app, request.project_path.clone(), |_| {
        perform_git_append_gitignore(request)
    })
}

#[tauri::command]
fn git_diff_file(request: GitDiffRequest) -> GitDiffSnapshot {
    load_git_diff_file(request)
}

#[tauri::command]
fn git_commit_message_context(project_path: String) -> git::GitCommitMessageContextSnapshot {
    git::git_commit_message_context(project_path)
}

#[tauri::command]
fn git_review_diff_file(request: GitReviewDiffRequest) -> GitDiffSnapshot {
    load_git_review_diff_file(request)
}

#[tauri::command]
fn git_review_file_content(request: GitReviewContentRequest) -> GitReviewContentSnapshot {
    load_git_review_file_content(request)
}

#[tauri::command]
fn git_watch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> Result<GitWatchRegistration, String> {
    let activity = Arc::clone(&state.project_activity);
    let projects = Arc::clone(&state.projects);
    state
        .git_watch
        .watch(app.clone(), project_path, move |event| {
            activity.refresh_git_changed(
                app.clone(),
                Arc::clone(&projects),
                event.project_path,
                event.repository_path,
                event.changed_paths,
            );
        })
}

#[tauri::command]
fn git_unwatch(state: tauri::State<'_, AppState>, project_path: String) -> Result<(), String> {
    state.git_watch.unwatch(project_path)
}

#[tauri::command]
fn file_list_children(request: FileChildrenRequest) -> Result<Vec<FileEntry>, String> {
    list_file_children(request)
}

#[tauri::command]
fn file_read(request: FilePathRequest) -> Result<FileReadResult, String> {
    read_file_path(request)
}

#[tauri::command]
fn file_write(request: FileWriteRequest) -> Result<FileReadResult, String> {
    write_file_path(request)
}

#[tauri::command]
fn file_create_file(request: FileCreateRequest) -> Result<FileEntry, String> {
    create_file_file(request)
}

#[tauri::command]
fn file_create_dir(request: FileCreateRequest) -> Result<FileEntry, String> {
    create_file_dir(request)
}

#[tauri::command]
fn file_rename(request: FileRenameRequest) -> Result<FileEntry, String> {
    rename_file_path(request)
}

#[tauri::command]
fn file_delete(request: FilePathRequest) -> Result<(), String> {
    delete_file_path(request)
}

#[tauri::command]
fn file_copy(request: FileCopyRequest) -> Result<FileEntry, String> {
    copy_file_path(request)
}

#[tauri::command]
fn file_import_external(request: FileExternalCopyRequest) -> Result<Vec<FileEntry>, String> {
    import_external_file_paths(request)
}

#[tauri::command]
fn file_reveal(request: FilePathRequest) -> Result<(), String> {
    reveal_file_path(request)
}

#[tauri::command]
fn file_open(request: FilePathRequest) -> Result<(), String> {
    open_file_path(request)
}

#[tauri::command]
fn file_watch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> Result<files::FileWatchRegistration, String> {
    state.file_watch.watch(app, project_path)
}

#[tauri::command]
fn file_unwatch(state: tauri::State<'_, AppState>, project_path: String) -> Result<(), String> {
    state.file_watch.unwatch(project_path)
}

#[tauri::command]
async fn worktree_create(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: WorktreeCreateRequest,
) -> Result<WorktreeSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    let projects_for_snapshot = Arc::clone(&state.projects);
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let snapshot = create_worktree(request)?;
        let selected = snapshot.selected_worktree_id.clone();
        let project_id = snapshot.project_id.clone();
        if !selected.is_empty() {
            projects.select_worktree(ProjectSelectWorktreeRequest {
                project_id,
                worktree_id: selected,
            })?;
        }
        projects.merge_worktree_snapshot(snapshot)
    })
    .await
    .map_err(|error| error.to_string())??;
    let project_id = snapshot.project_id.clone();
    let project_path = snapshot
        .worktrees
        .iter()
        .find(|worktree| worktree.is_default)
        .map(|worktree| worktree.path.clone())
        .or_else(|| {
            snapshot
                .worktrees
                .first()
                .map(|worktree| worktree.path.clone())
        })
        .unwrap_or_default();
    let _ = app.emit(
        "worktree:snapshot",
        WorktreeSnapshotEvent {
            project_id,
            project_path,
            snapshot: snapshot.clone(),
        },
    );
    let _ = app.emit("project:updated", projects_for_snapshot.list_snapshot());
    Ok(snapshot)
}

#[tauri::command]
async fn worktree_remove(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: WorktreeRemoveRequest,
) -> Result<WorktreeSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    let projects_for_snapshot = Arc::clone(&state.projects);
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let snapshot = remove_worktree(request)?;
        projects.merge_worktree_snapshot(snapshot)
    })
    .await
    .map_err(|error| error.to_string())??;
    let project_id = snapshot.project_id.clone();
    let project_path = snapshot
        .worktrees
        .iter()
        .find(|worktree| worktree.is_default)
        .or_else(|| snapshot.worktrees.first())
        .map(|worktree| worktree.path.clone())
        .unwrap_or_default();
    let _ = app.emit(
        "worktree:snapshot",
        WorktreeSnapshotEvent {
            project_id,
            project_path,
            snapshot: snapshot.clone(),
        },
    );
    let _ = app.emit("project:updated", projects_for_snapshot.list_snapshot());
    Ok(snapshot)
}

#[tauri::command]
async fn worktree_merge(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: WorktreeMergeRequest,
) -> Result<WorktreeSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    let projects_for_snapshot = Arc::clone(&state.projects);
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let snapshot = merge_worktree(request)?;
        projects.merge_worktree_snapshot(snapshot)
    })
    .await
    .map_err(|error| error.to_string())??;
    let project_id = snapshot.project_id.clone();
    let project_path = snapshot
        .worktrees
        .iter()
        .find(|worktree| worktree.is_default)
        .or_else(|| snapshot.worktrees.first())
        .map(|worktree| worktree.path.clone())
        .unwrap_or_default();
    let _ = app.emit(
        "worktree:snapshot",
        WorktreeSnapshotEvent {
            project_id,
            project_path,
            snapshot: snapshot.clone(),
        },
    );
    let _ = app.emit("project:updated", projects_for_snapshot.list_snapshot());
    Ok(snapshot)
}

#[tauri::command]
fn performance_snapshot(state: tauri::State<'_, AppState>) -> PerformanceSnapshot {
    state.performance.snapshot()
}

#[tauri::command]
async fn pet_refresh(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    _request: PetRefreshRequest,
) -> Result<PetSnapshot, String> {
    let pet = Arc::clone(&state.pet);
    let current_snapshot = pet.snapshot()?;
    let claimed_at = current_snapshot.claimed_at;
    let cutoff = claimed_at.map(|value| value as f64);
    let input = tauri::async_runtime::spawn_blocking(move || {
        let project_totals = ai_history::normalized_project_totals_since(cutoff)
            .map_err(|error| error.to_string())?;
        let all_time_total_tokens =
            ai_history::global_all_time_normalized_tokens().map_err(|error| error.to_string())?;
        let sessions =
            ai_history::indexed_sessions_since(cutoff).map_err(|error| error.to_string())?;
        Ok::<_, String>(pet::refresh_input_from_indexed_history(
            claimed_at,
            project_totals,
            all_time_total_tokens,
            sessions,
        ))
    })
    .await
    .map_err(|error| error.to_string())??;
    let snapshot = tauri::async_runtime::spawn_blocking(move || pet.refresh(input))
        .await
        .map_err(|error| error.to_string())??;
    sync_desktop_pet_window(&app, &state.settings.snapshot(), &snapshot);
    let _ = app.emit("pet:updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
async fn pet_catalog() -> Result<PetCatalog, String> {
    tauri::async_runtime::spawn_blocking(PetStore::catalog)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn pet_custom_install_preview(
    request: PetCustomPetInstallRequest,
) -> Result<PetCustomPetInstallPreview, String> {
    PetStore::resolve_custom_pet_install(request).await
}

#[tauri::command]
async fn pet_custom_install(request: PetCustomPetInstallRequest) -> Result<PetCustomPet, String> {
    PetStore::install_custom_pet(request).await
}

#[tauri::command]
async fn pet_custom_sprite(pet: PetCustomPet) -> Result<PetCustomPet, String> {
    tauri::async_runtime::spawn_blocking(move || hydrate_custom_pet_data_url(pet))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn pet_snapshot(state: tauri::State<'_, AppState>) -> Result<PetSnapshot, String> {
    let pet = Arc::clone(&state.pet);
    tauri::async_runtime::spawn_blocking(move || pet.snapshot())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn pet_claim(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: PetClaimRequest,
) -> Result<PetSnapshot, String> {
    let pet = Arc::clone(&state.pet);
    let input = tauri::async_runtime::spawn_blocking(move || {
        let all_time_total_tokens =
            ai_history::global_all_time_normalized_tokens().map_err(|error| error.to_string())?;
        Ok::<_, String>(pet::claim_input_from_indexed_history(
            request,
            all_time_total_tokens,
        ))
    })
    .await
    .map_err(|error| error.to_string())??;
    let snapshot = tauri::async_runtime::spawn_blocking(move || pet.claim(input))
        .await
        .map_err(|error| error.to_string())??;
    sync_desktop_pet_window(&app, &state.settings.snapshot(), &snapshot);
    let _ = app.emit("pet:updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
async fn pet_rename(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: PetRenameRequest,
) -> Result<PetSnapshot, String> {
    let pet = Arc::clone(&state.pet);
    let snapshot = tauri::async_runtime::spawn_blocking(move || pet.rename(request))
        .await
        .map_err(|error| error.to_string())??;
    sync_desktop_pet_window(&app, &state.settings.snapshot(), &snapshot);
    let _ = app.emit("pet:updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
async fn pet_archive_current(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<PetSnapshot, String> {
    let pet = Arc::clone(&state.pet);
    let snapshot = tauri::async_runtime::spawn_blocking(move || pet.archive_current())
        .await
        .map_err(|error| error.to_string())??;
    sync_desktop_pet_window(&app, &state.settings.snapshot(), &snapshot);
    let _ = app.emit("pet:updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
async fn pet_restore_archived(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: PetRestoreRequest,
) -> Result<PetSnapshot, String> {
    let pet = Arc::clone(&state.pet);
    let snapshot = tauri::async_runtime::spawn_blocking(move || pet.restore_archived(request))
        .await
        .map_err(|error| error.to_string())??;
    sync_desktop_pet_window(&app, &state.settings.snapshot(), &snapshot);
    let _ = app.emit("pet:updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
async fn ssh_profiles(state: tauri::State<'_, AppState>) -> Result<SSHProfilesSnapshot, String> {
    let ssh = Arc::clone(&state.ssh);
    tauri::async_runtime::spawn_blocking(move || Ok::<_, String>(ssh.snapshot()))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn ssh_profile_upsert(
    state: tauri::State<'_, AppState>,
    request: SSHProfileUpsertRequest,
) -> Result<SSHProfilesSnapshot, String> {
    let ssh = Arc::clone(&state.ssh);
    tauri::async_runtime::spawn_blocking(move || ssh.upsert(request))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn ssh_profile_delete(
    state: tauri::State<'_, AppState>,
    profile_id: String,
) -> Result<SSHProfilesSnapshot, String> {
    let ssh = Arc::clone(&state.ssh);
    tauri::async_runtime::spawn_blocking(move || ssh.delete(profile_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn ssh_profile_test(
    state: tauri::State<'_, AppState>,
    request: SSHProfileUpsertRequest,
) -> Result<SSHProfileTestResult, String> {
    let ssh = Arc::clone(&state.ssh);
    let ai_runtime = Arc::clone(&state.ai_runtime);
    tauri::async_runtime::spawn_blocking(move || {
        ai_runtime.prepare()?;
        ssh.test_profile(request, ai_runtime.wrapper_bin_dir())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn ssh_launch_command(
    state: tauri::State<'_, AppState>,
    profile_id: String,
) -> Result<SSHLaunchCommand, String> {
    let ssh = Arc::clone(&state.ssh);
    tauri::async_runtime::spawn_blocking(move || ssh.launch_command(profile_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn project_list(state: tauri::State<'_, AppState>) -> Result<ProjectListSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    tauri::async_runtime::spawn_blocking(move || projects.list_snapshot())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn project_mark_active(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project: ProjectSummary,
) {
    mark_project_active_with_watch(&state, app, project);
}

fn mark_project_active_with_watch(
    state: &AppState,
    app: tauri::AppHandle,
    project: ProjectSummary,
) {
    state.project_activity.mark_project_active(project.clone());
    let activity = Arc::clone(&state.project_activity);
    let projects = Arc::clone(&state.projects);
    let _ = state
        .git_watch
        .watch(app.clone(), project.path.clone(), move |event| {
            activity.refresh_git_changed(
                app.clone(),
                Arc::clone(&projects),
                event.project_path,
                event.repository_path,
                event.changed_paths,
            );
        });
}

#[tauri::command]
async fn project_create(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ProjectCreateRequest,
) -> Result<(), String> {
    let projects = Arc::clone(&state.projects);
    let known_project_ids = projects
        .projects_snapshot()
        .into_iter()
        .map(|project| project.id)
        .collect::<std::collections::HashSet<_>>();
    let snapshot = tauri::async_runtime::spawn_blocking(move || projects.create_project(request))
        .await
        .map_err(|error| error.to_string())??;
    if let Some(project) = selected_project_summary(&snapshot) {
        if known_project_ids.contains(&project.id) {
            mark_project_active_with_watch(&state, app.clone(), project);
        } else {
            state.project_activity.refresh_project_now(
                app.clone(),
                project.clone(),
                Arc::clone(&state.ai_history),
            );
            mark_project_active_with_watch(&state, app.clone(), project);
        }
    }
    let _ = app.emit("project:updated", snapshot);
    Ok(())
}

#[tauri::command]
async fn project_close(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ProjectCloseRequest,
) -> Result<(), String> {
    let projects = Arc::clone(&state.projects);
    let pet = Arc::clone(&state.pet);
    let project_id = request.project_id.clone();
    let project_id_for_pet = project_id.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let snapshot = projects.close_project(request.project_id)?;
        let _ = pet.forget_project_baseline(&project_id_for_pet);
        Ok::<_, String>(snapshot)
    })
    .await
    .map_err(|error| error.to_string())??;
    state.project_activity.remove_project(&project_id);
    let _ = app.emit("project:updated", snapshot);
    Ok(())
}

#[tauri::command]
async fn project_close_all(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let projects = Arc::clone(&state.projects);
    let pet = Arc::clone(&state.pet);
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let snapshot = projects.close_all_projects()?;
        let _ = pet.forget_all_project_baselines();
        Ok::<_, String>(snapshot)
    })
    .await
    .map_err(|error| error.to_string())??;
    state.project_activity.clear();
    let _ = app.emit("project:updated", snapshot);
    Ok(())
}

#[tauri::command]
async fn project_select(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_id: String,
) -> Result<(), String> {
    let projects = Arc::clone(&state.projects);
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        projects.select_project(project_id)?;
        Ok::<_, String>(projects.list_snapshot())
    })
    .await
    .map_err(|error| error.to_string())??;
    if let Some(project) = selected_project_summary(&snapshot) {
        mark_project_active_with_watch(&state, app.clone(), project);
    }
    let _ = app.emit("project:updated", snapshot);
    Ok(())
}

#[tauri::command]
async fn project_update(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ProjectUpdateRequest,
) -> Result<(), String> {
    let projects = Arc::clone(&state.projects);
    let snapshot = tauri::async_runtime::spawn_blocking(move || projects.update_project(request))
        .await
        .map_err(|error| error.to_string())??;
    if let Some(project) = selected_project_summary(&snapshot) {
        mark_project_active_with_watch(&state, app.clone(), project);
    }
    let _ = app.emit("project:updated", snapshot);
    Ok(())
}

#[tauri::command]
async fn project_reorder(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ProjectReorderRequest,
) -> Result<(), String> {
    let projects = Arc::clone(&state.projects);
    let snapshot = tauri::async_runtime::spawn_blocking(move || projects.reorder_projects(request))
        .await
        .map_err(|error| error.to_string())??;
    let _ = app.emit("project:updated", snapshot);
    Ok(())
}

#[tauri::command]
async fn project_select_worktree(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ProjectSelectWorktreeRequest,
) -> Result<(), String> {
    let projects = Arc::clone(&state.projects);
    let worktree_id = request.worktree_id.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        projects.select_worktree(request)?;
        Ok::<_, String>(projects.list_snapshot())
    })
    .await
    .map_err(|error| error.to_string())??;
    if let Some(project) = state
        .projects
        .root_project_summary_for_workspace_id(&worktree_id)
        .or_else(|| selected_project_summary(&snapshot))
    {
        mark_project_active_with_watch(&state, app.clone(), project);
    }
    let _ = app.emit("project:updated", snapshot);
    Ok(())
}

#[tauri::command]
async fn project_set_default_push_remote(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ProjectDefaultPushRemoteRequest,
) -> Result<(), String> {
    let projects = Arc::clone(&state.projects);
    let snapshot =
        tauri::async_runtime::spawn_blocking(move || projects.set_default_push_remote(request))
            .await
            .map_err(|error| error.to_string())??;
    let _ = app.emit("project:updated", snapshot);
    Ok(())
}

fn selected_project_summary(snapshot: &ProjectListSnapshot) -> Option<ProjectSummary> {
    snapshot
        .selected_project_id
        .as_ref()
        .and_then(|selected_id| {
            snapshot
                .projects
                .iter()
                .find(|project| &project.id == selected_id)
        })
        .or_else(|| snapshot.projects.first())
        .cloned()
}

#[tauri::command]
fn project_open_applications() -> Vec<ProjectOpenApplicationSnapshot> {
    PROJECT_OPEN_APPLICATIONS
        .iter()
        .map(|spec| ProjectOpenApplicationSnapshot {
            id: spec.id.to_string(),
            label: spec.label.to_string(),
            category: spec.category.to_string(),
            installed: project_open_application_installed(spec),
        })
        .collect()
}

#[tauri::command]
fn project_open_in_application(request: ProjectOpenApplicationRequest) -> Result<(), String> {
    let path = PathBuf::from(request.project_path.trim());
    if !path.is_dir() {
        return Err("Project path does not exist.".to_string());
    }
    let spec = PROJECT_OPEN_APPLICATIONS
        .iter()
        .find(|item| item.id == request.application_id)
        .ok_or_else(|| "Unsupported application.".to_string())?;
    open_project_path_in_application(&path, spec)
}

#[tauri::command]
fn project_reveal_in_file_manager(project_path: String) -> Result<(), String> {
    let path = PathBuf::from(project_path.trim());
    if !path.exists() {
        return Err("Project path does not exist.".to_string());
    }
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn project_open_application_url(spec: &ProjectOpenApplicationSpec) -> Option<String> {
    spec.bundle_ids.iter().find_map(|bundle_id| {
        Command::new("mdfind")
            .arg(format!("kMDItemCFBundleIdentifier == '{bundle_id}'"))
            .output()
            .ok()
            .and_then(|output| {
                if !output.status.success() {
                    return None;
                }
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .map(ToOwned::to_owned)
            })
    })
}

fn project_open_application_installed(spec: &ProjectOpenApplicationSpec) -> bool {
    #[cfg(target_os = "macos")]
    {
        project_open_application_url(spec).is_some()
    }
    #[cfg(not(target_os = "macos"))]
    {
        spec.commands.iter().any(|command| command_in_path(command))
    }
}

fn open_project_path_in_application(
    path: &PathBuf,
    spec: &ProjectOpenApplicationSpec,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        for bundle_id in spec.bundle_ids {
            if Command::new("open")
                .args(["-b", bundle_id, &path.display().to_string()])
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
            {
                return Ok(());
            }
        }
        if spec.id == "vscode" {
            let url = format!("vscode://file{}", path.display());
            return tauri_plugin_opener::open_url(url, None::<&str>)
                .map_err(|error| error.to_string());
        }
        return Err(format!("{} not found.", spec.label));
    }
    #[cfg(not(target_os = "macos"))]
    {
        for command in spec.commands {
            if command_in_path(command) {
                return Command::new(command)
                    .arg(path)
                    .spawn()
                    .map(|_| ())
                    .map_err(|error| error.to_string());
            }
        }
        Err(format!("{} not found.", spec.label))
    }
}

#[cfg(not(target_os = "macos"))]
fn command_in_path(command: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return true;
        }
        #[cfg(target_os = "windows")]
        {
            return ["exe", "cmd", "bat"]
                .iter()
                .any(|extension| dir.join(format!("{command}.{extension}")).is_file());
        }
        #[cfg(not(target_os = "windows"))]
        false
    })
}

#[tauri::command]
async fn terminal_layout_get(
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<Option<TerminalLayoutRecord>, String> {
    let projects = Arc::clone(&state.projects);
    tauri::async_runtime::spawn_blocking(move || projects.terminal_layout(&project_id))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn terminal_layout_save(
    state: tauri::State<'_, AppState>,
    project_id: String,
    snapshot: TerminalLayoutRecord,
) -> Result<TerminalLayoutRecord, String> {
    let projects = Arc::clone(&state.projects);
    tauri::async_runtime::spawn_blocking(move || {
        projects.save_terminal_layout(project_id, snapshot)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn remote_status(state: tauri::State<'_, AppState>) -> RemoteStatus {
    state.remote.snapshot()
}

#[tauri::command]
fn remote_snapshot_emit(state: tauri::State<'_, AppState>, app: tauri::AppHandle) {
    state.remote.emit_snapshot(&app);
}

#[tauri::command]
async fn remote_reconnect(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RemoteStatus, String> {
    state.remote.reconnect(app).await
}

#[tauri::command]
async fn remote_devices_refresh(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RemoteStatus, String> {
    state.remote.refresh_devices(app).await
}

#[tauri::command]
async fn remote_device_revoke(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    device_id: String,
) -> Result<RemoteStatus, String> {
    state.remote.revoke_device(app, device_id).await
}

#[tauri::command]
async fn remote_pairing_create(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RemoteStatus, String> {
    state.remote.create_pairing(app).await
}

#[tauri::command]
async fn remote_pairing_cancel(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    pairing_id: String,
) -> Result<RemoteStatus, String> {
    state.remote.cancel_pairing(app, pairing_id).await
}

#[tauri::command]
async fn remote_pairing_confirm(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    pairing_id: String,
) -> Result<RemoteStatus, String> {
    state.remote.confirm_pairing(app, pairing_id).await
}

#[tauri::command]
async fn remote_pairing_reject(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    pairing_id: String,
) -> Result<RemoteStatus, String> {
    state.remote.reject_pairing(app, pairing_id).await
}

#[tauri::command]
fn power_set_sleep_prevention(
    state: tauri::State<'_, AppState>,
    mode: String,
) -> Result<bool, String> {
    state.power.set_sleep_prevention(mode)
}

#[tauri::command]
async fn notification_dispatch_channels(
    request: NotificationDispatchRequest,
) -> NotificationDispatchResult {
    dispatch_notification_channels(request).await
}

#[tauri::command]
fn app_about_metadata(app: tauri::AppHandle) -> AppAboutMetadata {
    app_info::about_metadata(&app)
}

#[tauri::command]
async fn app_update_status(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<UpdateStatus, String> {
    let settings = state.settings.snapshot();
    Ok(app_info::update_status(&app, &settings).await)
}

#[tauri::command]
async fn app_update_install(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<UpdateInstallResult, String> {
    let settings = state.settings.snapshot();
    app_info::install_update(&app, &settings).await
}

#[tauri::command]
async fn diagnostics_export(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: DiagnosticsExportRequest,
) -> Result<DiagnosticsExportResult, String> {
    let settings = state.settings.snapshot();
    let projects = state.projects.list_snapshot();
    let ai_runtime = state.ai_runtime.snapshot();
    let ai_state = state.ai_runtime.state_snapshot();
    let performance = state.performance.snapshot();
    let ssh = state.ssh.snapshot();
    tauri::async_runtime::spawn_blocking(move || {
        app_info::export_diagnostics(
            &app,
            request,
            settings,
            projects,
            ai_runtime,
            ai_state,
            performance,
            ssh,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn app_open_runtime_log() -> Result<(), String> {
    app_info::open_runtime_log()
}

#[tauri::command]
fn app_open_live_log() -> Result<(), String> {
    app_info::open_live_log()
}

#[tauri::command]
fn app_open_url(url: String) -> Result<(), String> {
    app_info::open_url(&url)
}

#[tauri::command]
fn app_toggle_devtools(
    #[cfg_attr(not(debug_assertions), allow(unused_variables))] app: tauri::AppHandle,
) {
    #[cfg(debug_assertions)]
    if let Some(window) = app.get_webview_window("main") {
        toggle_devtools(&window);
    }
}

#[tauri::command]
fn app_window_close(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if window.label() == "main" {
        if shutdown_runtime_state(&state.terminals, &state.is_exiting) {
            app.exit(0);
        }
        return Ok(());
    }
    window.destroy().map_err(|error| error.to_string())
}

fn shutdown_runtime_state(terminals: &TerminalManager, is_exiting: &AtomicBool) -> bool {
    if is_exiting.swap(true, Ordering::SeqCst) {
        return false;
    }
    terminals.kill_all();
    true
}

struct RemoteHostService {
    app: tauri::AppHandle,
    settings: Arc<AppSettingsStore>,
    projects: Arc<ProjectStore>,
    terminals: Arc<TerminalManager>,
    ai_history: Arc<AIHistoryIndexer>,
    snapshot: Mutex<RemoteStatus>,
    socket_tx: Mutex<Option<mpsc::UnboundedSender<String>>>,
    connection_generation: AtomicU64,
    send_seq_by_device: Mutex<HashMap<String, i64>>,
    receive_seq_by_device: Mutex<HashMap<String, i64>>,
    terminal_viewers_by_session: Mutex<HashMap<String, HashSet<String>>>,
    terminal_upload_sessions: Mutex<HashMap<String, RemoteTerminalUploadSession>>,
    reconnect_task: Mutex<Option<JoinHandle<()>>>,
    pairing_poll_task: Mutex<Option<JoinHandle<()>>>,
    p2p: Mutex<Option<Arc<RemoteP2PHostTransport>>>,
}

struct RemoteLayoutTerminal {
    project: ProjectSummary,
    slot_id: String,
    title: String,
    session_key: String,
}

impl RemoteHostService {
    fn new(
        app: tauri::AppHandle,
        settings: Arc<AppSettingsStore>,
        projects: Arc<ProjectStore>,
        terminals: Arc<TerminalManager>,
        ai_history: Arc<AIHistoryIndexer>,
    ) -> Self {
        let status = RemoteStatus::from_settings(&settings.snapshot().remote, None, None);
        Self {
            app,
            settings,
            projects,
            terminals,
            ai_history,
            snapshot: Mutex::new(status),
            socket_tx: Mutex::new(None),
            connection_generation: AtomicU64::new(0),
            send_seq_by_device: Mutex::new(HashMap::new()),
            receive_seq_by_device: Mutex::new(HashMap::new()),
            terminal_viewers_by_session: Mutex::new(HashMap::new()),
            terminal_upload_sessions: Mutex::new(HashMap::new()),
            reconnect_task: Mutex::new(None),
            pairing_poll_task: Mutex::new(None),
            p2p: Mutex::new(None),
        }
    }

    fn snapshot(&self) -> RemoteStatus {
        self.snapshot
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| {
                RemoteStatus::from_settings(&self.settings.snapshot().remote, None, None)
            })
    }

    fn start(self: &Arc<Self>, app: tauri::AppHandle) {
        self.ensure_p2p_transport();
        self.emit_snapshot(&app);
        self.sync_settings(app);
    }

    fn emit_snapshot(&self, app: &tauri::AppHandle) {
        let _ = app.emit("remote:status", self.snapshot());
    }

    fn ensure_p2p_transport(self: &Arc<Self>) {
        if self
            .p2p
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .is_some()
        {
            return;
        }
        let weak_for_signal = Arc::downgrade(self);
        let weak_for_message = Arc::downgrade(self);
        let weak_for_state = Arc::downgrade(self);
        let Ok(transport) = RemoteP2PHostTransport::new(
            Arc::new(move |signal: RemoteP2PSignal| {
                if let Some(service) = weak_for_signal.upgrade() {
                    service.send_relay(&signal.kind, Some(&signal.device_id), None, signal.payload);
                }
            }),
            Arc::new(move |device_id: String, data: Vec<u8>| {
                if let Some(service) = weak_for_message.upgrade() {
                    tauri::async_runtime::spawn(async move {
                        service.handle_p2p_message(device_id, data).await;
                    });
                }
            }),
            Arc::new(move |device_id: String, state: String| {
                if let Some(service) = weak_for_state.upgrade() {
                    if matches!(state.as_str(), "closed" | "failed" | "disconnected") {
                        service.remove_terminal_viewer(Some(&device_id));
                    }
                }
            }),
        ) else {
            return;
        };
        if let Ok(mut current) = self.p2p.lock() {
            *current = Some(transport);
        }
    }

    fn sync_settings(self: &Arc<Self>, app: tauri::AppHandle) {
        let remote = self.settings.snapshot().remote;
        if !remote.is_enabled || remote_server_url(&remote).trim().is_empty() {
            self.connection_generation.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut tx) = self.socket_tx.lock() {
                *tx = None;
            }
            self.update_snapshot(
                RemoteStatus::from_settings(
                    &remote,
                    Some("stopped"),
                    Some("Remote Host stopped.".to_string()),
                ),
                Some(&app),
            );
            return;
        }
        self.spawn_connect_loop(app, 0);
    }

    fn spawn_connect_loop(self: &Arc<Self>, app: tauri::AppHandle, initial_delay_ms: u64) {
        let generation = self.connection_generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut task) = self.reconnect_task.lock() {
            if let Some(handle) = task.take() {
                handle.abort();
            }
            let service = Arc::clone(self);
            *task = Some(tauri::async_runtime::spawn(async move {
                if initial_delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(initial_delay_ms)).await;
                }
                service.connect_loop(app, generation).await;
            }));
        }
    }

    async fn reconnect(self: &Arc<Self>, app: tauri::AppHandle) -> Result<RemoteStatus, String> {
        self.register_host(Some(&app)).await?;
        self.spawn_connect_loop(app.clone(), 0);
        Ok(self.snapshot())
    }

    async fn refresh_devices(
        self: &Arc<Self>,
        app: tauri::AppHandle,
    ) -> Result<RemoteStatus, String> {
        if self.settings.snapshot().remote.host_id.trim().is_empty() {
            self.register_host(Some(&app)).await?;
        }
        self.load_devices(Some(&app)).await?;
        Ok(self.snapshot())
    }

    async fn revoke_device(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        device_id: String,
    ) -> Result<RemoteStatus, String> {
        let remote = self.settings.snapshot().remote;
        if device_id.trim().is_empty() {
            return Err("Missing device id.".to_string());
        }
        if remote.host_id.trim().is_empty() || remote.host_token.trim().is_empty() {
            return Err("Remote Host is not registered.".to_string());
        }
        remote_post::<Value>(
            &remote_server_url(&remote),
            "/api/devices/revoke",
            json!({
                "hostId": remote.host_id,
                "token": remote.host_token,
                "deviceId": device_id,
            }),
        )
        .await?;
        let next_settings = self.settings.update(|current| {
            current
                .remote
                .cached_devices
                .retain(|device| device.id != device_id);
        })?;
        let mut status = self.snapshot();
        status.device_list.retain(|device| device.id != device_id);
        let synced = RemoteStatus::from_settings(
            &next_settings.remote,
            Some(&status.status),
            Some("Device removed.".to_string()),
        );
        status.devices = synced.devices;
        status.message = synced.message;
        self.update_snapshot(status, Some(&app));
        let _ = self.load_devices(Some(&app)).await;
        Ok(self.snapshot())
    }

    async fn create_pairing(
        self: &Arc<Self>,
        app: tauri::AppHandle,
    ) -> Result<RemoteStatus, String> {
        self.register_host(Some(&app)).await?;
        let remote = self.settings.snapshot().remote;
        let body = json!({
            "hostId": remote.host_id,
            "token": remote.host_token,
        });
        let mut pairing =
            remote_post::<RemotePairingInfo>(&remote_server_url(&remote), "/api/pairings", body)
                .await?;
        pairing.host_public_key =
            (!remote.host_public_key.trim().is_empty()).then(|| remote.host_public_key.clone());
        pairing.crypto_version = Some(1);
        pairing.qr_payload = remote_pairing_qr_payload(&remote, &pairing);
        let mut status = self.snapshot();
        status.pairing = Some(pairing.clone());
        status.status = "connected".to_string();
        status.message = format!("Pairing code: {}", pairing.code);
        self.update_snapshot(status.clone(), Some(&app));
        self.start_pairing_poll(app, pairing);
        Ok(status)
    }

    async fn cancel_pairing(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        pairing_id: String,
    ) -> Result<RemoteStatus, String> {
        self.reject_pairing(app, pairing_id).await
    }

    async fn confirm_pairing(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        pairing_id: String,
    ) -> Result<RemoteStatus, String> {
        self.pairing_decision(
            &app,
            "/api/pairings/confirm",
            &pairing_id,
            "Pairing confirmed.",
        )
        .await
    }

    async fn reject_pairing(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        pairing_id: String,
    ) -> Result<RemoteStatus, String> {
        self.pairing_decision(
            &app,
            "/api/pairings/reject",
            &pairing_id,
            "Pairing rejected.",
        )
        .await
    }

    async fn pairing_decision(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        path: &str,
        pairing_id: &str,
        message: &str,
    ) -> Result<RemoteStatus, String> {
        let remote = self.settings.snapshot().remote;
        if pairing_id.trim().is_empty() {
            return Err("Missing pairing id.".to_string());
        }
        if remote.host_id.trim().is_empty() || remote.host_token.trim().is_empty() {
            return Err("Remote Host is not registered.".to_string());
        }
        remote_post::<Value>(
            &remote_server_url(&remote),
            path,
            json!({
                "hostId": remote.host_id,
                "token": remote.host_token,
                "pairingId": pairing_id,
            }),
        )
        .await?;
        let mut status = self.snapshot();
        status.pairing = status
            .pairing
            .filter(|pairing| pairing.pairing_id != pairing_id);
        status
            .pending_pairings
            .retain(|pairing| pairing.id != pairing_id);
        status.message = message.to_string();
        self.update_snapshot(status.clone(), Some(app));
        self.stop_pairing_poll();
        if path.ends_with("/confirm") {
            self.load_devices(Some(app)).await?;
        }
        Ok(self.snapshot())
    }

    fn start_pairing_poll(self: &Arc<Self>, app: tauri::AppHandle, pairing: RemotePairingInfo) {
        self.stop_pairing_poll();
        let service = Arc::clone(self);
        if let Ok(mut task) = self.pairing_poll_task.lock() {
            *task = Some(tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let still_active = service
                        .snapshot()
                        .pairing
                        .as_ref()
                        .map(|current| current.pairing_id == pairing.pairing_id)
                        .unwrap_or(false);
                    if !still_active {
                        return;
                    }
                    if service.poll_pairing_status(&app, &pairing).await {
                        return;
                    }
                }
            }));
        }
    }

    fn stop_pairing_poll(&self) {
        if let Ok(mut task) = self.pairing_poll_task.lock() {
            if let Some(handle) = task.take() {
                handle.abort();
            }
        }
    }

    async fn poll_pairing_status(
        &self,
        app: &tauri::AppHandle,
        pairing: &RemotePairingInfo,
    ) -> bool {
        let remote = self.settings.snapshot().remote;
        let response = remote_post::<RemotePairingStatusResponse>(
            &remote_server_url(&remote),
            "/api/pairings/status",
            json!({
                "code": pairing.code,
                "secret": pairing.secret,
            }),
        )
        .await;
        let Ok(response) = response else {
            let mut status = self.snapshot();
            if status
                .pairing
                .as_ref()
                .map(|current| current.pairing_id.as_str())
                == Some(pairing.pairing_id.as_str())
            {
                status.pairing = None;
                status.message = "Pairing failed.".to_string();
                self.update_snapshot(status, Some(app));
            }
            return true;
        };
        match response.status.as_str() {
            "claimed" => {
                self.show_pending_pairing(
                    app,
                    response
                        .pairing_id
                        .unwrap_or_else(|| pairing.pairing_id.clone()),
                    response
                        .device_name
                        .unwrap_or_else(|| "Mobile Device".to_string()),
                    response.device_public_key.unwrap_or_default(),
                    response.code.unwrap_or_else(|| pairing.code.clone()),
                    pairing.secret.clone(),
                );
                true
            }
            "confirmed" | "rejected" => {
                let mut status = self.snapshot();
                if status
                    .pairing
                    .as_ref()
                    .map(|current| current.pairing_id.as_str())
                    == Some(pairing.pairing_id.as_str())
                {
                    status.pairing = None;
                    self.update_snapshot(status, Some(app));
                }
                true
            }
            _ => false,
        }
    }

    async fn connect_loop(self: Arc<Self>, app: tauri::AppHandle, generation: u64) {
        let mut delay = 1_u64;
        loop {
            if generation != self.connection_generation.load(Ordering::SeqCst) {
                return;
            }
            let remote = self.settings.snapshot().remote;
            if !remote.is_enabled {
                return;
            }
            if let Err(error) = self.connect_once(app.clone(), generation).await {
                let mut status = RemoteStatus::from_settings(
                    &self.settings.snapshot().remote,
                    Some("failed"),
                    Some(error),
                );
                status.pairing = self.snapshot().pairing;
                self.update_snapshot(status, Some(&app));
            }
            if generation != self.connection_generation.load(Ordering::SeqCst) {
                return;
            }
            tokio::time::sleep(Duration::from_secs(delay)).await;
            delay = (delay * 2).min(30);
        }
    }

    async fn connect_once(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        generation: u64,
    ) -> Result<(), String> {
        self.register_host(Some(&app)).await?;
        self.load_devices(Some(&app)).await.ok();
        let remote = self.settings.snapshot().remote;
        let ws_url = remote_url(
            &remote_server_url(&remote),
            "/ws/host",
            &[
                ("hostId", remote.host_id.as_str()),
                ("token", remote.host_token.as_str()),
            ],
            true,
        )?;
        let (socket, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(remote_error_message)?;
        let (mut write, mut read) = socket.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        if let Ok(mut current) = self.socket_tx.lock() {
            *current = Some(tx);
        }
        let mut status = RemoteStatus::from_settings(
            &self.settings.snapshot().remote,
            Some("connected"),
            Some("Remote Host connected.".to_string()),
        );
        status.pairing = self.snapshot().pairing;
        self.update_snapshot(status, Some(&app));

        let writer = tauri::async_runtime::spawn(async move {
            while let Some(message) = rx.recv().await {
                if write
                    .send(WebSocketMessage::Text(message.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        while let Some(message) = read.next().await {
            if generation != self.connection_generation.load(Ordering::SeqCst) {
                writer.abort();
                return Ok(());
            }
            match message {
                Ok(WebSocketMessage::Text(text)) => {
                    self.handle_socket_text(app.clone(), text.to_string()).await;
                }
                Ok(WebSocketMessage::Close(_)) => break,
                Ok(_) => {}
                Err(error) => {
                    writer.abort();
                    return Err(error.to_string());
                }
            }
        }
        writer.abort();
        if let Ok(mut current) = self.socket_tx.lock() {
            *current = None;
        }
        Err("Remote Host disconnected.".to_string())
    }

    async fn register_host(self: &Arc<Self>, app: Option<&tauri::AppHandle>) -> Result<(), String> {
        let mut remote = self.settings.snapshot().remote;
        if !remote.is_enabled {
            self.update_snapshot(
                RemoteStatus::from_settings(
                    &remote,
                    Some("stopped"),
                    Some("Remote Host stopped.".to_string()),
                ),
                app,
            );
            return Ok(());
        }
        if remote_server_url(&remote).trim().is_empty() {
            self.update_snapshot(
                RemoteStatus::from_settings(
                    &remote,
                    Some("stopped"),
                    Some("Remote not configured.".to_string()),
                ),
                app,
            );
            return Ok(());
        }
        if remote.host_id.trim().is_empty() {
            remote.host_id = uuid::Uuid::new_v4().to_string();
        }
        if remote.host_token.trim().is_empty() {
            remote.host_token = remote_random_token();
        }
        ensure_remote_host_identity(&mut remote);
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RegisterResponse {
            host_id: String,
            token: String,
        }
        let response = remote_post::<RegisterResponse>(
            &remote_server_url(&remote),
            "/api/hosts/register",
            json!({
                "hostId": remote.host_id,
                "name": remote_host_name(),
                "token": remote.host_token,
                "publicKey": remote.host_public_key,
            }),
        )
        .await?;
        remote.host_id = response.host_id;
        remote.host_token = response.token;
        let next_settings = self.settings.update(|current| {
            current.remote = remote.clone();
        })?;
        self.update_snapshot(
            RemoteStatus::from_settings(
                &next_settings.remote,
                Some("connecting"),
                Some("Connecting relay...".to_string()),
            ),
            app,
        );
        Ok(())
    }

    async fn load_devices(self: &Arc<Self>, app: Option<&tauri::AppHandle>) -> Result<(), String> {
        let remote = self.settings.snapshot().remote;
        let relay = remote_server_url(&remote);
        if relay.trim().is_empty()
            || remote.host_id.trim().is_empty()
            || remote.host_token.trim().is_empty()
        {
            return Ok(());
        }
        #[derive(serde::Deserialize)]
        struct DeviceList {
            devices: Vec<app_settings::RemoteHostDeviceSettings>,
        }
        let path = format!("/api/hosts/{}/devices", remote.host_id);
        let url = remote_url(
            &relay,
            &path,
            &[("token", remote.host_token.as_str())],
            false,
        )?;
        let client = remote_http_client()?;
        let response = client.get(url).send().await.map_err(remote_error_message)?;
        let mut list = remote_parse_response::<DeviceList>(response).await?;
        list.devices.retain(|device| device.revoked_at.is_none());
        let next_settings = self.settings.update(|current| {
            current.remote.cached_devices = list
                .devices
                .iter()
                .cloned()
                .map(|mut device| {
                    device.online = Some(false);
                    device
                })
                .collect();
        })?;
        let mut status = self.snapshot();
        let synced = RemoteStatus::from_settings(
            &next_settings.remote,
            Some(&status.status),
            Some(status.message.clone()),
        );
        status.devices = synced.devices;
        status.device_list = list
            .devices
            .into_iter()
            .map(RemoteHostDevice::from)
            .collect();
        status.host_id = synced.host_id;
        status.encryption = synced.encryption;
        self.update_snapshot(status, app);
        Ok(())
    }

    async fn handle_socket_text(self: &Arc<Self>, app: tauri::AppHandle, text: String) {
        let Ok(raw) = serde_json::from_str::<RemoteEnvelope>(&text) else {
            return;
        };
        let Some(envelope) = self.decrypt_envelope_if_needed(raw).await else {
            return;
        };
        self.handle_remote_envelope(app, envelope).await;
    }

    async fn handle_remote_envelope(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        envelope: RemoteEnvelope,
    ) {
        match envelope.kind.as_str() {
            "pairing.request" => {
                self.handle_pairing_request(envelope, &app).await;
            }
            "host.info" => {
                self.send(
                    "host.info",
                    envelope.device_id.as_deref(),
                    None,
                    json!({
                        "hostId": self.settings.snapshot().remote.host_id,
                        "name": remote_host_name(),
                        "platform": std::env::consts::OS,
                        "app": "Codux",
                    }),
                );
            }
            "device.connected" => {
                self.update_device_online(envelope.device_id.as_deref(), true, Some(&app));
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            "device.disconnected" => {
                self.update_device_online(envelope.device_id.as_deref(), false, Some(&app));
                self.remove_terminal_viewer(envelope.device_id.as_deref());
                if let (Some(p2p), Some(device_id)) = (
                    self.p2p.lock().ok().and_then(|value| value.clone()),
                    envelope.device_id.as_deref(),
                ) {
                    p2p.close(device_id).await;
                }
            }
            "project.list" => {
                self.send_project_list(envelope.device_id.as_deref());
            }
            "terminal.list" => {
                self.send_terminal_list(envelope.device_id.as_deref());
            }
            "file.list" => {
                let path = envelope.payload.get("path").and_then(Value::as_str);
                let purpose = envelope.payload.get("purpose").and_then(Value::as_str);
                self.send(
                    "file.list",
                    envelope.device_id.as_deref(),
                    None,
                    remote_file_list(path, purpose),
                );
            }
            "file.read" => {
                self.handle_file_read(&envelope);
            }
            "file.write" => {
                self.handle_file_write(&envelope);
            }
            "file.rename" => {
                self.handle_file_rename(&envelope);
            }
            "file.delete" => {
                self.handle_file_delete(&envelope);
            }
            "project.add" => {
                self.handle_project_add(&envelope);
            }
            "project.edit" => {
                self.handle_project_edit(&envelope);
            }
            "project.remove" => {
                self.handle_project_remove(&envelope);
            }
            "ai.stats" => {
                self.handle_ai_stats(&envelope).await;
            }
            "terminal.create" => {
                self.handle_terminal_create(app, &envelope);
            }
            "terminal.buffer" => {
                self.handle_terminal_buffer(&envelope);
            }
            "terminal.input" => {
                self.handle_terminal_input(&envelope);
            }
            "terminal.resize" => {
                self.handle_terminal_resize(&envelope);
            }
            "terminal.close" => {
                self.handle_terminal_close(&envelope);
            }
            "terminal.signal" => {
                self.handle_terminal_signal(&envelope);
            }
            "terminal.upload" => {
                self.handle_terminal_upload(&envelope);
            }
            "terminal.upload.start" => {
                self.handle_terminal_upload_start(&envelope);
            }
            "terminal.upload.chunk" => {
                self.handle_terminal_upload_chunk(&envelope);
            }
            "terminal.upload.finish" => {
                self.handle_terminal_upload_finish(&envelope);
            }
            "terminal.upload.cancel" => {
                self.handle_terminal_upload_cancel(&envelope);
            }
            "p2p.ping" => {
                self.send_terminal_data(
                    "p2p.pong",
                    envelope.device_id.as_deref(),
                    None,
                    envelope.payload.clone(),
                );
            }
            "p2p.offer" => {
                self.ensure_p2p_transport();
                if let (Some(p2p), Some(device_id)) = (
                    self.p2p.lock().ok().and_then(|value| value.clone()),
                    envelope.device_id.clone(),
                ) {
                    p2p.handle_offer(device_id, envelope.payload).await;
                }
            }
            "p2p.candidate" => {
                self.ensure_p2p_transport();
                if let (Some(p2p), Some(device_id)) = (
                    self.p2p.lock().ok().and_then(|value| value.clone()),
                    envelope.device_id.clone(),
                ) {
                    p2p.handle_candidate(device_id, envelope.payload).await;
                }
            }
            _ => {}
        }
    }

    async fn handle_p2p_message(self: Arc<Self>, device_id: String, data: Vec<u8>) {
        let Ok(envelope) = serde_json::from_slice::<RemoteEnvelope>(&data) else {
            remote_runtime_log(
                "p2p-recv",
                &format!("drop reason=decode device={device_id} bytes={}", data.len()),
            );
            return;
        };
        remote_runtime_log(
            "p2p-recv",
            &format!(
                "type={} session={} device={} bytes={}",
                envelope.kind,
                envelope.session_id.as_deref().unwrap_or("-"),
                device_id,
                data.len()
            ),
        );
        let Some(envelope) = self
            .decrypt_envelope_if_needed(envelope.with_device_id(device_id))
            .await
        else {
            remote_runtime_log("p2p-recv", "drop reason=decrypt-or-sequence");
            return;
        };
        self.handle_remote_envelope(self.app.clone(), envelope)
            .await;
    }

    async fn handle_pairing_request(&self, envelope: RemoteEnvelope, app: &tauri::AppHandle) {
        let pairing_id = envelope
            .payload
            .get("pairingId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if pairing_id.is_empty() {
            return;
        }
        let device_name = envelope
            .payload
            .get("deviceName")
            .and_then(Value::as_str)
            .unwrap_or("Mobile Device")
            .to_string();
        let device_public_key = envelope
            .payload
            .get("devicePublicKey")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let pairing_code = envelope
            .payload
            .get("code")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                self.snapshot()
                    .pairing
                    .filter(|pairing| pairing.pairing_id == pairing_id)
                    .map(|pairing| pairing.code)
            })
            .unwrap_or_default();
        let pairing_secret = self
            .snapshot()
            .pairing
            .filter(|pairing| pairing.pairing_id == pairing_id)
            .map(|pairing| pairing.secret)
            .unwrap_or_default();
        self.show_pending_pairing(
            app,
            pairing_id,
            device_name,
            device_public_key,
            pairing_code,
            pairing_secret,
        );
    }

    fn show_pending_pairing(
        &self,
        app: &tauri::AppHandle,
        pairing_id: String,
        device_name: String,
        device_public_key: String,
        pairing_code: String,
        pairing_secret: String,
    ) {
        if pairing_id.is_empty() {
            return;
        }
        let match_code = remote_pairing_match_code(
            &self.settings.snapshot().remote,
            &pairing_code,
            &pairing_secret,
            &device_public_key,
        )
        .unwrap_or_else(|| pairing_code.clone());
        let mut status = self.snapshot();
        if status
            .pairing
            .as_ref()
            .map(|pairing| pairing.pairing_id.as_str())
            == Some(pairing_id.as_str())
        {
            status.pairing = None;
        }
        if let Some(existing) = status
            .pending_pairings
            .iter_mut()
            .find(|pairing| pairing.id == pairing_id)
        {
            existing.device_name = device_name;
            existing.device_public_key = device_public_key;
            existing.code = match_code;
        } else {
            status.pending_pairings.push(RemotePendingPairing {
                id: pairing_id,
                device_name,
                device_public_key,
                code: match_code,
            });
        }
        status.message = "Confirm device pairing.".to_string();
        self.update_snapshot(status, Some(app));
        self.stop_pairing_poll();
    }

    fn send_project_and_terminal_lists(&self, device_id: Option<&str>) {
        self.send_project_list(device_id);
        self.send_terminal_list(device_id);
    }

    fn send_project_list(&self, device_id: Option<&str>) {
        self.send(
            "project.list",
            device_id,
            None,
            json!({ "projects": self.remote_projects() }),
        );
    }

    fn send_terminal_list(&self, device_id: Option<&str>) {
        self.send(
            "terminal.list",
            device_id,
            None,
            json!({ "terminals": self.remote_terminals() }),
        );
    }

    fn remote_projects(&self) -> Vec<Value> {
        self.projects
            .projects_snapshot()
            .into_iter()
            .map(|project| {
                json!({
                    "id": project.id,
                    "name": project.name,
                    "path": project.path,
                })
            })
            .collect()
    }

    fn remote_terminals(&self) -> Vec<Value> {
        let running = self
            .terminals
            .list()
            .into_iter()
            .map(remote_terminal_snapshot_payload)
            .collect::<Vec<_>>();
        let mut known = running
            .iter()
            .filter_map(|value| value.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<HashSet<_>>();
        let mut terminals = running;
        for terminal in self.remote_layout_terminals() {
            if let Some(id) = terminal.get("id").and_then(Value::as_str) {
                if known.insert(id.to_string()) {
                    terminals.push(terminal);
                }
            }
        }
        terminals
    }

    fn remote_layout_terminals(&self) -> Vec<Value> {
        let projects_by_id = self
            .projects
            .list_snapshot()
            .projects
            .into_iter()
            .map(|project| (project.id.clone(), project))
            .collect::<HashMap<_, _>>();
        let layouts = self
            .projects
            .terminal_layouts_snapshot()
            .layouts
            .into_iter()
            .filter_map(|(project_id, layout)| {
                projects_by_id
                    .get(&project_id)
                    .cloned()
                    .map(|project| (project, layout))
            })
            .flat_map(|(project, layout)| {
                let mut values = Vec::new();
                for pane in layout.top_panes {
                    values.push(remote_layout_top_pane_payload(&project, &pane));
                }
                for tab in layout.tabs {
                    values.push(remote_layout_tab_payload(&project, &tab));
                }
                values
            })
            .collect::<Vec<_>>();
        let mut known_project_ids = self
            .projects
            .terminal_layouts_snapshot()
            .layouts
            .keys()
            .cloned()
            .collect::<HashSet<_>>();
        let mut values = layouts;
        for project in projects_by_id.values() {
            if known_project_ids.insert(project.id.clone()) {
                values.push(remote_default_top_terminal_payload(project));
            }
        }
        values
    }

    fn remote_layout_terminal(&self, session_id: &str) -> Option<RemoteLayoutTerminal> {
        let projects_by_id = self
            .projects
            .list_snapshot()
            .projects
            .into_iter()
            .map(|project| (project.id.clone(), project))
            .collect::<HashMap<_, _>>();
        for (project_id, layout) in self.projects.terminal_layouts_snapshot().layouts {
            let Some(project) = projects_by_id.get(&project_id).cloned() else {
                continue;
            };
            for pane in layout.top_panes {
                if pane.terminal_id == session_id {
                    return Some(RemoteLayoutTerminal {
                        project,
                        slot_id: pane.id.clone(),
                        title: pane.title,
                        session_key: terminal_session_key(&project_id, &pane.id),
                    });
                }
            }
            for tab in layout.tabs {
                if tab.terminal_id == session_id {
                    return Some(RemoteLayoutTerminal {
                        project,
                        slot_id: tab.id.clone(),
                        title: tab.label,
                        session_key: terminal_session_key(&project_id, &tab.id),
                    });
                }
            }
        }
        if let Some((project_id, slot_id)) = default_remote_terminal_parts(session_id) {
            let project = projects_by_id.get(project_id)?.clone();
            if slot_id == "top-1" {
                return Some(RemoteLayoutTerminal {
                    project,
                    slot_id: slot_id.to_string(),
                    title: "Split 1".to_string(),
                    session_key: terminal_session_key(project_id, slot_id),
                });
            }
        }
        None
    }

    fn handle_file_read(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        match remote_file_read(path) {
            Ok(payload) => self.send("file.read", envelope.device_id.as_deref(), None, payload),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_file_write(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        let Some(content) = envelope.payload.get("content").and_then(Value::as_str) else {
            self.send_error(envelope, "File content is required.");
            return;
        };
        match remote_file_write(path, content) {
            Ok(()) => self.send(
                "file.written",
                envelope.device_id.as_deref(),
                None,
                json!({ "path": path }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_file_rename(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        let Some(new_path) = envelope.payload.get("newPath").and_then(Value::as_str) else {
            self.send_error(envelope, "New file path is required.");
            return;
        };
        match remote_file_rename(path, new_path) {
            Ok(()) => self.send(
                "file.renamed",
                envelope.device_id.as_deref(),
                None,
                json!({ "path": path, "newPath": new_path }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_file_delete(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        match fs::remove_file(path).or_else(|_| fs::remove_dir_all(path)) {
            Ok(()) => self.send(
                "file.deleted",
                envelope.device_id.as_deref(),
                None,
                json!({ "path": path }),
            ),
            Err(error) => self.send_error(envelope, &error.to_string()),
        }
    }

    fn handle_project_add(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "Project path is required.");
            return;
        };
        let name = envelope
            .payload
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                Path::new(path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Project")
                    .to_string()
            });
        match self.projects.create_project(ProjectCreateRequest {
            name,
            path: path.to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
        }) {
            Ok(snapshot) => {
                let project_id = snapshot.selected_project_id.unwrap_or_default();
                self.send(
                    "project.updated",
                    envelope.device_id.as_deref(),
                    None,
                    json!({ "action": "add", "projectId": project_id }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_project_edit(&self, envelope: &RemoteEnvelope) {
        let Some(project_id) = envelope.payload.get("projectId").and_then(Value::as_str) else {
            self.send_error(envelope, "Project id is required.");
            return;
        };
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "Project path is required.");
            return;
        };
        let name = envelope
            .payload
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                Path::new(path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Project")
                    .to_string()
            });
        match self.projects.update_project(ProjectUpdateRequest {
            project_id: project_id.to_string(),
            name,
            path: path.to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
        }) {
            Ok(_) => {
                self.send(
                    "project.updated",
                    envelope.device_id.as_deref(),
                    None,
                    json!({ "action": "edit", "projectId": project_id }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_project_remove(&self, envelope: &RemoteEnvelope) {
        let Some(project_id) = envelope.payload.get("projectId").and_then(Value::as_str) else {
            self.send_error(envelope, "Project id is required.");
            return;
        };
        match self.projects.close_project(project_id.to_string()) {
            Ok(_) => {
                self.send(
                    "project.updated",
                    envelope.device_id.as_deref(),
                    None,
                    json!({ "action": "remove", "projectId": project_id }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    async fn handle_ai_stats(&self, envelope: &RemoteEnvelope) {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let project = self
            .projects
            .projects_snapshot()
            .into_iter()
            .find(|project| project.id == project_id)
            .or_else(|| self.projects.projects_snapshot().into_iter().next());
        let Some(project) = project else {
            self.send_error(envelope, "Unable to load AI stats.");
            return;
        };
        let request = AIHistoryProjectRequest {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
        };
        match self.ai_history.project_state(request).await {
            Ok(state) => match serde_json::to_value(state) {
                Ok(mut value) => {
                    let snapshot = value
                        .get_mut("snapshot")
                        .map(Value::take)
                        .filter(|value| !value.is_null());
                    let mut payload = snapshot.unwrap_or_else(|| {
                        json!({
                            "projectId": project.id,
                            "projectName": project.name,
                            "projectSummary": {},
                            "sessions": [],
                            "heatmap": [],
                            "todayTimeBuckets": [],
                            "toolBreakdown": [],
                            "modelBreakdown": [],
                        })
                    });
                    if let Some(object) = payload.as_object_mut() {
                        object.insert(
                            "updatedAt".to_string(),
                            json!(chrono::Utc::now().to_rfc3339()),
                        );
                    }
                    self.send("ai.stats", envelope.device_id.as_deref(), None, payload);
                }
                Err(error) => self.send_error(envelope, &error.to_string()),
            },
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_terminal_create(self: &Arc<Self>, app: tauri::AppHandle, envelope: &RemoteEnvelope) {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let command = envelope
            .payload
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default();
        let project = project_id
            .as_deref()
            .and_then(|id| {
                self.projects
                    .projects_snapshot()
                    .into_iter()
                    .find(|project| project.id == id)
            })
            .or_else(|| self.projects.projects_snapshot().into_iter().next());
        let Some(project) = project else {
            self.send_error(envelope, "Unable to create terminal.");
            return;
        };
        let device_id = envelope.device_id.clone();
        let title = if command.trim().is_empty() {
            "Terminal".to_string()
        } else {
            command.clone()
        };
        match create_terminal_session(
            Arc::clone(self),
            &self.terminals,
            app,
            TerminalConfig {
                cwd: Some(project.path.clone()),
                shell: None,
                command: (!command.trim().is_empty()).then_some(command),
                cols: envelope
                    .payload
                    .get("cols")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16),
                rows: envelope
                    .payload
                    .get("rows")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16),
                env: None,
                project_id: Some(project.id.clone()),
                project_name: Some(project.name.clone()),
                terminal_id: None,
                slot_id: None,
                session_key: None,
                title: Some(title),
                tool: None,
            },
            None,
            None,
        ) {
            Ok(session_id) => {
                self.register_terminal_viewer(&session_id, device_id.as_deref());
                self.send_terminal_data(
                    "terminal.created",
                    envelope.device_id.as_deref(),
                    Some(&session_id),
                    self.remote_terminal_payload(&session_id)
                        .unwrap_or_else(|| json!({ "id": session_id })),
                );
                self.send_terminal_list(envelope.device_id.as_deref());
                self.send_terminal_buffer(&session_id, envelope.device_id.as_deref(), 0);
            }
            Err(error) => self.send_error(envelope, &error.to_string()),
        }
    }

    fn handle_terminal_buffer(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            self.send_error(envelope, "Terminal session is required.");
            return;
        };
        let offset = envelope
            .payload
            .get("offset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if let Err(error) = self.ensure_remote_terminal_started(session_id, envelope) {
            self.send_error(envelope, &error);
            return;
        }
        remote_runtime_log(
            "terminal-buffer",
            &format!(
                "request session={} device={} offset={}",
                session_id,
                envelope.device_id.as_deref().unwrap_or("-"),
                offset
            ),
        );
        self.send_terminal_buffer(session_id, envelope.device_id.as_deref(), offset);
    }

    fn handle_terminal_input(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            self.send_error(envelope, "Terminal session is required.");
            return;
        };
        let Some(data) = envelope.payload.get("data").and_then(Value::as_str) else {
            self.send_error(envelope, "Terminal input is required.");
            return;
        };
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        if let Some(input_id) = envelope.payload.get("inputId").and_then(Value::as_str) {
            self.send_terminal_data(
                "terminal.input.ack",
                envelope.device_id.as_deref(),
                Some(session_id),
                json!({ "inputId": input_id, "ok": true, "accepted": true }),
            );
        }
        if let Err(error) = self.ensure_remote_terminal_started(session_id, envelope) {
            self.send_error(envelope, &error);
            return;
        }
        if let Err(error) = self.terminals.write(session_id, data.as_bytes()) {
            self.send_error(envelope, &error.to_string());
        }
    }

    fn handle_terminal_resize(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        let cols = envelope
            .payload
            .get("cols")
            .and_then(Value::as_u64)
            .unwrap_or(100) as u16;
        let rows = envelope
            .payload
            .get("rows")
            .and_then(Value::as_u64)
            .unwrap_or(30) as u16;
        if self
            .ensure_remote_terminal_started(session_id, envelope)
            .is_err()
        {
            return;
        }
        let _ = self.terminals.resize(session_id, cols, rows);
    }

    fn handle_terminal_close(&self, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        match self.terminals.kill(session_id) {
            Ok(()) => {
                self.send_terminal_data(
                    "terminal.closed",
                    envelope.device_id.as_deref(),
                    Some(session_id),
                    json!({ "id": session_id }),
                );
                self.send_terminal_list(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error.to_string()),
        }
    }

    fn handle_terminal_signal(&self, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        let signal = envelope
            .payload
            .get("signal")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match signal {
            "interrupt" => {
                let _ = self.terminals.write(session_id, &[0x03]);
            }
            "escape" => {
                let _ = self.terminals.write(session_id, &[0x1b]);
            }
            _ => {}
        }
    }

    fn handle_terminal_upload(&self, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            self.send_error(envelope, "Terminal session is required.");
            return;
        };
        let Some(data) = envelope.payload.get("data").and_then(Value::as_str) else {
            self.send_error(envelope, "Upload data is required.");
            return;
        };
        let bytes = match remote_base64_url_decode(data).or_else(|_| {
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
                .map_err(remote_error_message)
        }) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.send_error(envelope, &error);
                return;
            }
        };
        let name = sanitized_remote_upload_name(
            envelope
                .payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("upload.png"),
        );
        let kind = remote_terminal_upload_kind(&envelope.payload);
        match self.write_terminal_upload_file(session_id, &name, &bytes) {
            Ok(path) => {
                self.finish_terminal_upload(envelope.device_id.as_deref(), session_id, path, &kind)
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_terminal_upload_start(&self, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            self.send_terminal_upload_ack(
                envelope,
                "start",
                None,
                false,
                Some("Terminal session is required."),
            );
            return;
        };
        let Some(upload_id) = envelope.payload.get("uploadId").and_then(Value::as_str) else {
            self.send_terminal_upload_ack(
                envelope,
                "start",
                None,
                false,
                Some("Upload id is required."),
            );
            return;
        };
        let total_bytes = envelope
            .payload
            .get("totalBytes")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let total_chunks = envelope
            .payload
            .get("totalChunks")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if total_bytes == 0 || total_bytes > 20 * 1024 * 1024 || total_chunks == 0 {
            self.send_terminal_upload_ack(
                envelope,
                "start",
                None,
                false,
                Some("Upload size is not supported."),
            );
            return;
        }
        let name = sanitized_remote_upload_name(
            envelope
                .payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("upload.png"),
        );
        let kind = remote_terminal_upload_kind(&envelope.payload);
        let directory = remote_terminal_upload_directory(session_id);
        if let Err(error) = fs::create_dir_all(&directory) {
            self.send_terminal_upload_ack(envelope, "start", None, false, Some(&error.to_string()));
            return;
        }
        let final_path = unique_remote_upload_path(&directory, &name);
        let partial_path = final_path.with_extension(format!(
            "{}.part-{}",
            final_path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("upload"),
            upload_id
        ));
        if fs::File::create(&partial_path).is_err() {
            self.send_terminal_upload_ack(
                envelope,
                "start",
                None,
                false,
                Some("Unable to create upload file."),
            );
            return;
        }
        if let Ok(mut uploads) = self.terminal_upload_sessions.lock() {
            uploads.insert(
                upload_id.to_string(),
                RemoteTerminalUploadSession {
                    session_id: session_id.to_string(),
                    device_id: envelope.device_id.clone(),
                    total_bytes,
                    total_chunks,
                    partial_path,
                    final_path,
                    kind,
                    received_chunks: HashSet::new(),
                    received_bytes: 0,
                },
            );
        }
        self.send_terminal_upload_ack(envelope, "start", None, true, None);
    }

    fn handle_terminal_upload_chunk(&self, envelope: &RemoteEnvelope) {
        let Some(upload_id) = envelope.payload.get("uploadId").and_then(Value::as_str) else {
            self.send_terminal_upload_ack(
                envelope,
                "chunk",
                None,
                false,
                Some("Upload id is required."),
            );
            return;
        };
        let chunk_index = envelope
            .payload
            .get("chunkIndex")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let offset = envelope
            .payload
            .get("offset")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let Some(data) = envelope.payload.get("data").and_then(Value::as_str) else {
            self.send_terminal_upload_ack(
                envelope,
                "chunk",
                Some(chunk_index),
                false,
                Some("Upload data is required."),
            );
            return;
        };
        let bytes = match remote_base64_url_decode(data).or_else(|_| {
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
                .map_err(remote_error_message)
        }) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.send_terminal_upload_ack(
                    envelope,
                    "chunk",
                    Some(chunk_index),
                    false,
                    Some(&error),
                );
                return;
            }
        };
        let mut uploads = match self.terminal_upload_sessions.lock() {
            Ok(uploads) => uploads,
            Err(_) => {
                self.send_terminal_upload_ack(
                    envelope,
                    "chunk",
                    Some(chunk_index),
                    false,
                    Some("Upload lock failed."),
                );
                return;
            }
        };
        let Some(session) = uploads.get_mut(upload_id) else {
            self.send_terminal_upload_ack(
                envelope,
                "chunk",
                Some(chunk_index),
                false,
                Some("Upload not found."),
            );
            return;
        };
        if chunk_index >= session.total_chunks || offset + bytes.len() as u64 > session.total_bytes
        {
            self.send_terminal_upload_ack(
                envelope,
                "chunk",
                Some(chunk_index),
                false,
                Some("Invalid upload chunk."),
            );
            return;
        }
        match fs::OpenOptions::new()
            .write(true)
            .open(&session.partial_path)
        {
            Ok(mut file) => {
                if file.seek(std::io::SeekFrom::Start(offset)).is_err()
                    || file.write_all(&bytes).is_err()
                {
                    self.send_terminal_upload_ack(
                        envelope,
                        "chunk",
                        Some(chunk_index),
                        false,
                        Some("Upload write failed."),
                    );
                    return;
                }
                session.received_chunks.insert(chunk_index);
                session.received_bytes = session.received_bytes.saturating_add(bytes.len() as u64);
                self.send_terminal_upload_ack(envelope, "chunk", Some(chunk_index), true, None);
            }
            Err(error) => self.send_terminal_upload_ack(
                envelope,
                "chunk",
                Some(chunk_index),
                false,
                Some(&error.to_string()),
            ),
        }
    }

    fn handle_terminal_upload_finish(&self, envelope: &RemoteEnvelope) {
        let Some(upload_id) = envelope.payload.get("uploadId").and_then(Value::as_str) else {
            self.send_terminal_upload_ack(
                envelope,
                "finish",
                None,
                false,
                Some("Upload id is required."),
            );
            return;
        };
        let session = match self.terminal_upload_sessions.lock() {
            Ok(mut uploads) => uploads.remove(upload_id),
            Err(_) => None,
        };
        let Some(session) = session else {
            self.send_terminal_upload_ack(
                envelope,
                "finish",
                None,
                false,
                Some("Upload not found."),
            );
            return;
        };
        if session.received_chunks.len() != session.total_chunks {
            self.send_terminal_upload_ack(
                envelope,
                "finish",
                None,
                false,
                Some("Upload is missing chunks."),
            );
            return;
        }
        if fs::rename(&session.partial_path, &session.final_path).is_err() {
            self.send_terminal_upload_ack(
                envelope,
                "finish",
                None,
                false,
                Some("Upload finish failed."),
            );
            return;
        }
        self.send_terminal_upload_ack(envelope, "finish", None, true, None);
        self.finish_terminal_upload(
            session.device_id.as_deref(),
            &session.session_id,
            session.final_path,
            &session.kind,
        );
    }

    fn handle_terminal_upload_cancel(&self, envelope: &RemoteEnvelope) {
        let Some(upload_id) = envelope.payload.get("uploadId").and_then(Value::as_str) else {
            return;
        };
        let session = self
            .terminal_upload_sessions
            .lock()
            .ok()
            .and_then(|mut uploads| uploads.remove(upload_id));
        if let Some(session) = session {
            let _ = fs::remove_file(session.partial_path);
        }
        self.send_terminal_upload_ack(envelope, "cancel", None, true, None);
    }

    fn write_terminal_upload_file(
        &self,
        session_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, String> {
        let directory = remote_terminal_upload_directory(session_id);
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let path = unique_remote_upload_path(&directory, name);
        fs::write(&path, bytes).map_err(|error| error.to_string())?;
        Ok(path)
    }

    fn finish_terminal_upload(
        &self,
        device_id: Option<&str>,
        session_id: &str,
        path: PathBuf,
        kind: &str,
    ) {
        let text = format!("{} ", terminal_upload_path_input(&path));
        let _ = self.terminals.write(session_id, text.as_bytes());
        self.send_terminal_data(
            "terminal.uploaded",
            device_id,
            Some(session_id),
            json!({
                "path": path.to_string_lossy().to_string(),
                "name": path.file_name().and_then(|value| value.to_str()).unwrap_or_default(),
                "kind": kind,
                "mode": "path",
                "tool": Value::Null,
                "inserted": true,
            }),
        );
    }

    fn send_terminal_upload_ack(
        &self,
        envelope: &RemoteEnvelope,
        stage: &str,
        chunk_index: Option<usize>,
        ok: bool,
        message: Option<&str>,
    ) {
        let mut payload = json!({
            "uploadId": envelope.payload.get("uploadId").cloned().unwrap_or(Value::Null),
            "stage": stage,
            "ok": ok,
        });
        if let Some(chunk_index) = chunk_index {
            payload["chunkIndex"] = json!(chunk_index);
        } else if let Some(value) = envelope.payload.get("chunkIndex") {
            payload["chunkIndex"] = value.clone();
        }
        if let Some(message) = message {
            payload["message"] = json!(message);
        }
        self.send_terminal_data(
            "terminal.upload.ack",
            envelope.device_id.as_deref(),
            envelope.session_id.as_deref(),
            payload,
        );
    }

    fn send_terminal_buffer(&self, session_id: &str, device_id: Option<&str>, offset: usize) {
        self.register_terminal_viewer(session_id, device_id);
        match self.terminals.snapshot(session_id) {
            Ok(data) => {
                let total_characters = data.chars().count();
                let clamped = offset.min(total_characters);
                let chunk: String = data.chars().skip(clamped).collect();
                remote_runtime_log(
                    "terminal-buffer",
                    &format!(
                        "send session={} device={} requestedOffset={} offset={} bufferLen={} chunkLen={}",
                        session_id,
                        device_id.unwrap_or("-"),
                        offset,
                        clamped,
                        total_characters,
                        chunk.chars().count()
                    ),
                );
                self.send_terminal_data(
                    "terminal.output",
                    device_id,
                    Some(session_id),
                    json!({
                        "data": chunk,
                        "buffer": true,
                        "offset": clamped,
                        "bufferLength": total_characters,
                    }),
                );
            }
            Err(error) => {
                remote_runtime_log(
                    "terminal-buffer",
                    &format!("error session={} message={}", session_id, error),
                );
                self.send_relay(
                    "error",
                    device_id,
                    Some(session_id),
                    json!({ "message": error.to_string() }),
                );
            }
        }
    }

    fn remote_terminal_payload(&self, session_id: &str) -> Option<Value> {
        self.remote_terminals()
            .into_iter()
            .find(|value| value.get("id").and_then(Value::as_str) == Some(session_id))
    }

    fn ensure_remote_terminal_started(
        self: &Arc<Self>,
        session_id: &str,
        envelope: &RemoteEnvelope,
    ) -> Result<(), String> {
        if self.terminals.snapshot(session_id).is_ok() {
            return Ok(());
        }
        let Some(layout) = self.remote_layout_terminal(session_id) else {
            return Ok(());
        };
        create_terminal_session(
            Arc::clone(self),
            &self.terminals,
            self.app.clone(),
            TerminalConfig {
                cwd: Some(layout.project.path.clone()),
                shell: None,
                command: None,
                cols: envelope
                    .payload
                    .get("cols")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16),
                rows: envelope
                    .payload
                    .get("rows")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16),
                env: None,
                project_id: Some(layout.project.id.clone()),
                project_name: Some(layout.project.name.clone()),
                terminal_id: Some(session_id.to_string()),
                slot_id: Some(layout.slot_id),
                session_key: Some(layout.session_key),
                title: Some(layout.title),
                tool: Some("auto".to_string()),
            },
            None,
            None,
        )?;
        self.send_terminal_list(envelope.device_id.as_deref());
        Ok(())
    }

    fn register_terminal_viewer(&self, session_id: &str, device_id: Option<&str>) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        if let Ok(mut viewers) = self.terminal_viewers_by_session.lock() {
            viewers
                .entry(session_id.to_string())
                .or_default()
                .insert(device_id.to_string());
        }
    }

    fn remove_terminal_viewer(&self, device_id: Option<&str>) {
        let Some(device_id) = device_id else {
            return;
        };
        if let Ok(mut viewers) = self.terminal_viewers_by_session.lock() {
            for session_viewers in viewers.values_mut() {
                session_viewers.remove(device_id);
            }
            viewers.retain(|_, value| !value.is_empty());
        }
    }

    fn handle_terminal_event(&self, event: TerminalEvent) {
        if let TerminalEvent::Output {
            session_id, text, ..
        } = event
        {
            let viewers = self
                .terminal_viewers_by_session
                .lock()
                .ok()
                .and_then(|viewers| viewers.get(&session_id).cloned())
                .unwrap_or_default();
            if viewers.is_empty() {
                return;
            }
            let buffer_length = self.terminals.buffer_characters(&session_id).unwrap_or(0);
            for device_id in viewers {
                remote_runtime_log(
                    "terminal-output",
                    &format!(
                        "stream session={} device={} chunkLen={} bufferLen={}",
                        session_id,
                        device_id,
                        text.chars().count(),
                        buffer_length
                    ),
                );
                self.send_terminal_data(
                    "terminal.output",
                    Some(&device_id),
                    Some(&session_id),
                    json!({
                        "data": text,
                        "bufferLength": buffer_length,
                    }),
                );
            }
        }
    }

    fn send_error(&self, envelope: &RemoteEnvelope, message: &str) {
        self.send_relay(
            "error",
            envelope.device_id.as_deref(),
            envelope.session_id.as_deref(),
            json!({ "message": message }),
        );
    }

    fn send_terminal_data(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) {
        let inner = RemoteOutgoingEnvelope {
            kind: kind.to_string(),
            device_id: device_id.map(str::to_string),
            session_id: session_id.map(str::to_string),
            seq: None,
            payload: payload.clone(),
        };
        let Some(p2p) = self.p2p.lock().ok().and_then(|value| value.clone()) else {
            self.send_relay(kind, device_id, session_id, payload);
            return;
        };
        let Ok(data) = serde_json::to_vec(&inner) else {
            self.send_relay(kind, device_id, session_id, payload);
            return;
        };
        let lane = remote_p2p_lane(kind);
        let relay_text = self.outgoing_relay_text(kind, device_id, session_id, payload);
        let socket_tx = self.socket_tx.lock().ok().and_then(|value| value.clone());
        let device_id = device_id.map(str::to_string);
        let kind_label = kind.to_string();
        let session_label = session_id.map(str::to_string);
        let data_len = data.len();
        tauri::async_runtime::spawn(async move {
            let sent_p2p = p2p.send(data, device_id.as_deref(), lane).await;
            remote_runtime_log(
                "p2p-send",
                &format!(
                    "type={} session={} device={} ok={} bytes={}",
                    kind_label,
                    session_label.as_deref().unwrap_or("-"),
                    device_id.as_deref().unwrap_or("-"),
                    sent_p2p,
                    data_len
                ),
            );
            if sent_p2p {
                return;
            }
            if let (Some(tx), Some(text)) = (socket_tx, relay_text) {
                let relay_ok = tx.send(text).is_ok();
                remote_runtime_log(
                    "relay-fallback",
                    &format!(
                        "type={} session={} device={} reason={} ok={}",
                        kind_label,
                        session_label.as_deref().unwrap_or("-"),
                        device_id.as_deref().unwrap_or("-"),
                        "p2p-send-failed",
                        relay_ok
                    ),
                );
            }
        });
    }

    fn send_relay(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) {
        let Some(text) = self.outgoing_relay_text(kind, device_id, session_id, payload) else {
            return;
        };
        if let Ok(current) = self.socket_tx.lock() {
            if let Some(tx) = current.as_ref() {
                let _ = tx.send(text);
            }
        }
    }

    fn outgoing_relay_text(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) -> Option<String> {
        let inner = RemoteOutgoingEnvelope {
            kind: kind.to_string(),
            device_id: device_id.map(str::to_string),
            session_id: session_id.map(str::to_string),
            seq: None,
            payload,
        };
        let envelope = self
            .encrypted_outgoing_envelope(inner)
            .unwrap_or_else(|_| RemoteOutgoingEnvelope {
                kind: "secure.required".to_string(),
                device_id: device_id.map(str::to_string),
                session_id: session_id.map(str::to_string),
                seq: None,
                payload: json!({
                    "message": "End-to-end encryption is required. Please pair this mobile device again."
                }),
            });
        serde_json::to_string(&envelope).ok()
    }

    fn send(&self, kind: &str, device_id: Option<&str>, session_id: Option<&str>, payload: Value) {
        self.send_relay(kind, device_id, session_id, payload);
    }

    async fn decrypt_envelope_if_needed(&self, envelope: RemoteEnvelope) -> Option<RemoteEnvelope> {
        if envelope.kind != "secure.message" {
            return Some(envelope);
        }
        let device_id = envelope.device_id.clone()?;
        let device = self
            .snapshot()
            .device_list
            .into_iter()
            .find(|device| device.id == device_id && !device.public_key.trim().is_empty())?;
        let remote = self.settings.snapshot().remote;
        let key = remote_e2e_symmetric_key(
            &remote.host_private_key,
            &device.public_key,
            &remote.host_id,
            &device_id,
        )
        .ok()?;
        let plaintext =
            remote_e2e_decrypt(&envelope.payload, &key, &remote.host_id, &device_id).ok()?;
        let mut inner = serde_json::from_slice::<RemoteEnvelope>(&plaintext).ok()?;
        if let Some(seq) = inner.seq {
            if let Ok(mut received) = self.receive_seq_by_device.lock() {
                let previous = received.get(&device_id).copied().unwrap_or(0);
                if seq <= previous {
                    return None;
                }
                received.insert(device_id.clone(), seq);
            }
        }
        inner.device_id = Some(device_id);
        Some(inner)
    }

    fn encrypted_outgoing_envelope(
        &self,
        mut inner: RemoteOutgoingEnvelope,
    ) -> Result<RemoteOutgoingEnvelope, String> {
        let Some(device_id) = inner
            .device_id
            .clone()
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(inner);
        };
        let device = self
            .snapshot()
            .device_list
            .into_iter()
            .find(|device| device.id == device_id && !device.public_key.trim().is_empty())
            .ok_or_else(|| "Missing device encryption key.".to_string())?;
        let remote = self.settings.snapshot().remote;
        let seq = {
            let mut send_seq = self
                .send_seq_by_device
                .lock()
                .map_err(|_| "Remote sequence lock poisoned.".to_string())?;
            let next = send_seq.get(&device_id).copied().unwrap_or(0) + 1;
            send_seq.insert(device_id.clone(), next);
            next
        };
        inner.seq = Some(seq);
        let plaintext = serde_json::to_vec(&inner).map_err(remote_error_message)?;
        let key = remote_e2e_symmetric_key(
            &remote.host_private_key,
            &device.public_key,
            &remote.host_id,
            &device_id,
        )?;
        let payload = remote_e2e_encrypt(&plaintext, &key, &remote.host_id, &device_id)?;
        Ok(RemoteOutgoingEnvelope {
            kind: "secure.message".to_string(),
            device_id: Some(device_id),
            session_id: inner.session_id,
            seq: None,
            payload,
        })
    }

    fn update_device_online(
        &self,
        device_id: Option<&str>,
        online: bool,
        app: Option<&tauri::AppHandle>,
    ) {
        let Some(device_id) = device_id else {
            return;
        };
        let mut status = self.snapshot();
        if let Some(device) = status
            .device_list
            .iter_mut()
            .find(|device| device.id == device_id)
        {
            device.online = Some(online);
            if online {
                device.last_seen = chrono::Utc::now().to_rfc3339();
            }
        }
        self.update_snapshot(status, app);
    }

    fn update_snapshot(&self, status: RemoteStatus, app: Option<&tauri::AppHandle>) {
        if let Ok(mut current) = self.snapshot.lock() {
            *current = status.clone();
        }
        if let Some(app) = app {
            let _ = app.emit("remote:status", status);
        }
    }
}

async fn remote_post<T: serde::de::DeserializeOwned>(
    base: &str,
    path: &str,
    body: Value,
) -> Result<T, String> {
    let url = remote_url(base, path, &[], false)?;
    let client = remote_http_client()?;
    let response = client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(remote_error_message)?;
    remote_parse_response(response).await
}

async fn remote_parse_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, String> {
    let status = response.status();
    let bytes = response.bytes().await.map_err(remote_error_message)?;
    if !status.is_success() {
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(error) = value.get("error").and_then(Value::as_str) {
                return Err(error.to_string());
            }
        }
        return Err(String::from_utf8_lossy(&bytes).to_string());
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "Remote response decode failed: {error}. Body: {}",
            String::from_utf8_lossy(&bytes)
        )
    })
}

fn remote_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .use_rustls_tls()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(remote_error_message)
}

fn remote_runtime_log(category: &str, message: &str) {
    let path = paths::app_support_dir().join("runtime.log");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let timestamp = chrono::Local::now().to_rfc3339();
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{timestamp}] [{category}] {message}");
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RemoteEnvelope {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, rename = "deviceId")]
    device_id: Option<String>,
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
    #[serde(default)]
    seq: Option<i64>,
    #[serde(default)]
    payload: Value,
}

impl RemoteEnvelope {
    fn with_device_id(mut self, device_id: String) -> Self {
        self.device_id = Some(device_id);
        self
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct RemoteOutgoingEnvelope {
    #[serde(rename = "type")]
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "deviceId")]
    device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sessionId")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seq: Option<i64>,
    payload: Value,
}

struct RemoteTerminalUploadSession {
    session_id: String,
    device_id: Option<String>,
    total_bytes: u64,
    total_chunks: usize,
    partial_path: PathBuf,
    final_path: PathBuf,
    kind: String,
    received_chunks: HashSet<usize>,
    received_bytes: u64,
}

impl RemoteStatus {
    fn from_settings(
        settings: &app_settings::RemoteSettings,
        status: Option<&str>,
        message: Option<String>,
    ) -> Self {
        let relay = remote_server_url(settings);
        let enabled = settings.is_enabled && !relay.trim().is_empty();
        let devices = settings
            .cached_devices
            .iter()
            .filter(|device| device.revoked_at.is_none())
            .cloned()
            .map(RemoteHostDevice::from)
            .collect::<Vec<_>>();
        let status = status
            .unwrap_or(if enabled { "connecting" } else { "stopped" })
            .to_string();
        let message = message.unwrap_or_else(|| {
            if enabled {
                "Connecting relay...".to_string()
            } else {
                "Remote Host stopped.".to_string()
            }
        });
        Self {
            enabled,
            relay,
            devices: devices.len() as u32,
            encryption: if enabled && !settings.host_public_key.trim().is_empty() {
                "configured".to_string()
            } else if enabled {
                "pending".to_string()
            } else {
                "disabled".to_string()
            },
            status,
            message,
            host_id: settings.host_id.clone(),
            pairing: None,
            device_list: devices,
            pending_pairings: Vec::new(),
        }
    }
}

fn remote_terminal_snapshot_payload(terminal: terminal::TerminalSessionSnapshot) -> Value {
    json!({
        "id": terminal.id,
        "title": terminal.title,
        "displayTitle": if terminal.project_name.trim().is_empty() {
            terminal.title.clone()
        } else {
            format!("{} · {}", terminal.project_name, terminal.title)
        },
        "projectId": terminal.project_id,
        "projectName": terminal.project_name,
        "projectPath": terminal.cwd,
        "cwd": terminal.cwd,
        "shell": terminal.shell,
        "command": terminal.command,
        "kind": "desktop-shared",
        "ownerKind": std::env::consts::OS,
        "ownerDeviceId": "",
        "ownerDeviceName": remote_host_name(),
        "resizeOwner": std::env::consts::OS,
        "cols": terminal.cols,
        "rows": terminal.rows,
        "gridSource": std::env::consts::OS,
        "status": terminal.status,
        "isRunning": terminal.is_running,
        "createdAt": terminal.created_at,
        "lastActiveAt": terminal.last_active_at,
        "bufferCharacters": terminal.buffer_characters,
        "hasBuffer": terminal.has_buffer,
    })
}

fn remote_layout_top_pane_payload(project: &ProjectSummary, pane: &TerminalTopPaneRecord) -> Value {
    remote_layout_terminal_payload(project, &pane.id, &pane.title, &pane.terminal_id)
}

fn remote_layout_tab_payload(project: &ProjectSummary, tab: &TerminalBottomTabRecord) -> Value {
    remote_layout_terminal_payload(project, &tab.id, &tab.label, &tab.terminal_id)
}

fn remote_default_top_terminal_payload(project: &ProjectSummary) -> Value {
    let slot_id = "top-1";
    let title = "Split 1";
    remote_layout_terminal_payload(
        project,
        slot_id,
        title,
        &default_remote_terminal_id(&project.id, slot_id),
    )
}

fn remote_layout_terminal_payload(
    project: &ProjectSummary,
    slot_id: &str,
    title: &str,
    terminal_id: &str,
) -> Value {
    json!({
        "id": terminal_id,
        "title": title,
        "displayTitle": format!("{} · {}", project.name, title),
        "projectId": project.id,
        "projectName": project.name,
        "projectPath": project.path,
        "cwd": project.path,
        "shell": "login shell",
        "command": "",
        "kind": "desktop-shared",
        "ownerKind": std::env::consts::OS,
        "ownerDeviceId": "",
        "ownerDeviceName": remote_host_name(),
        "resizeOwner": std::env::consts::OS,
        "cols": 0,
        "rows": 0,
        "gridSource": std::env::consts::OS,
        "status": "idle",
        "isRunning": false,
        "createdAt": "",
        "lastActiveAt": "",
        "bufferCharacters": 0,
        "hasBuffer": false,
        "slotId": slot_id,
    })
}

fn terminal_session_key(project_id: &str, slot_id: &str) -> String {
    format!("{project_id}:{slot_id}")
}

fn remote_p2p_lane(kind: &str) -> RemoteP2PLane {
    match kind {
        "terminal.upload.ack" | "terminal.uploaded" => RemoteP2PLane::Upload,
        _ => RemoteP2PLane::Terminal,
    }
}

fn default_remote_terminal_id(project_id: &str, slot_id: &str) -> String {
    format!("remote-default:{project_id}:{slot_id}")
}

fn default_remote_terminal_parts(session_id: &str) -> Option<(&str, &str)> {
    let rest = session_id.strip_prefix("remote-default:")?;
    rest.rsplit_once(':')
}

fn remote_file_list(path: Option<&str>, purpose: Option<&str>) -> Value {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let requested = path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&home);
    let requested_path = PathBuf::from(requested);
    let directory = if requested_path.is_dir() {
        requested_path
    } else {
        requested_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(&home))
    };
    let mut entries = fs::read_dir(&directory)
        .ok()
        .into_iter()
        .flat_map(|read_dir| read_dir.filter_map(Result::ok))
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            let is_directory = path.is_dir();
            Some(json!({
                "name": name,
                "path": path.to_string_lossy().to_string(),
                "isDirectory": is_directory,
            }))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        let left_dir = left
            .get("isDirectory")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let right_dir = right
            .get("isDirectory")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        right_dir.cmp(&left_dir).then_with(|| {
            left.get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_lowercase()
                .cmp(
                    &right
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_lowercase(),
                )
        })
    });
    let mut payload = json!({
        "path": directory.to_string_lossy().to_string(),
        "parent": directory.parent().map(|path| path.to_string_lossy().to_string()).unwrap_or_default(),
        "entries": entries,
    });
    if let Some(purpose) = purpose {
        payload["purpose"] = Value::String(purpose.to_string());
    }
    payload
}

fn remote_file_read(path: &str) -> Result<Value, String> {
    let path = PathBuf::from(path);
    if path.is_dir() {
        return Err("Cannot open a directory as a file.".to_string());
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > 2 * 1024 * 1024 {
        return Err("File is larger than 2MB and cannot be opened on mobile yet.".to_string());
    }
    let content = fs::read_to_string(&path)
        .map_err(|_| "Only UTF-8 text files can be edited on mobile.".to_string())?;
    Ok(json!({
        "path": path.to_string_lossy().to_string(),
        "name": path.file_name().and_then(|value| value.to_str()).unwrap_or_default(),
        "content": content,
        "size": content.len(),
    }))
}

fn remote_file_write(path: &str, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|error| error.to_string())
}

fn remote_file_rename(path: &str, new_path: &str) -> Result<(), String> {
    let source = PathBuf::from(path);
    let destination = PathBuf::from(new_path);
    if source.parent() != destination.parent() {
        return Err("Rename must stay in the same directory.".to_string());
    }
    if destination.exists() {
        return Err("A file with this name already exists.".to_string());
    }
    fs::rename(source, destination).map_err(|error| error.to_string())
}

fn remote_terminal_upload_directory(session_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join("CoduxUploads")
        .join(sanitized_remote_upload_name(session_id))
}

fn sanitized_remote_upload_name(value: &str) -> String {
    let name = Path::new(value)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("upload.png");
    let cleaned = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .to_string();
    if cleaned.is_empty() {
        "upload.png".to_string()
    } else {
        cleaned
    }
}

fn remote_terminal_upload_kind(payload: &Value) -> String {
    let kind = payload
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("image")
        .trim()
        .to_ascii_lowercase();
    if kind == "file" {
        "file".to_string()
    } else {
        "image".to_string()
    }
}

fn terminal_upload_path_input(path: &Path) -> String {
    quote_terminal_path(&path.to_string_lossy())
}

#[cfg(windows)]
fn quote_terminal_path(value: &str) -> String {
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '&' | '(' | ')' | '[' | ']' | '{' | '}'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(not(windows))]
fn quote_terminal_path(value: &str) -> String {
    if value.chars().any(|ch| {
        ch.is_whitespace()
            || matches!(
                ch,
                '\'' | '"' | '\\' | '$' | '`' | '!' | '&' | '(' | ')' | ';' | '<' | '>' | '|'
            )
    }) {
        format!("'{}'", value.replace('\'', "'\\''"))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod remote_upload_tests {
    use super::quote_terminal_path;

    #[test]
    fn leaves_plain_terminal_path_unquoted() {
        assert_eq!(
            quote_terminal_path("/tmp/CoduxUploads/file.txt"),
            "/tmp/CoduxUploads/file.txt"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn quotes_unix_terminal_path_with_spaces() {
        assert_eq!(
            quote_terminal_path("/tmp/Codux Uploads/file name.txt"),
            "'/tmp/Codux Uploads/file name.txt'"
        );
    }

    #[cfg(windows)]
    #[test]
    fn quotes_windows_terminal_path_with_spaces() {
        assert_eq!(
            quote_terminal_path(r"C:\Users\Codux User\AppData\Local\Temp\file name.txt"),
            r#""C:\Users\Codux User\AppData\Local\Temp\file name.txt""#
        );
    }
}

fn unique_remote_upload_path(directory: &Path, file_name: &str) -> PathBuf {
    let file_name = sanitized_remote_upload_name(file_name);
    let path = PathBuf::from(&file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("upload");
    let extension = path.extension().and_then(|value| value.to_str());
    let mut candidate = directory.join(&file_name);
    let mut index = 1;
    while candidate.exists() {
        let next = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem}-{index}.{extension}"),
            _ => format!("{stem}-{index}"),
        };
        candidate = directory.join(next);
        index += 1;
    }
    candidate
}

fn remote_url(
    base: &str,
    path: &str,
    query: &[(&str, &str)],
    websocket: bool,
) -> Result<String, String> {
    let mut url = url::Url::parse(base.trim()).map_err(|error| error.to_string())?;
    url.set_path(path);
    url.set_query(None);
    if websocket {
        let scheme = match url.scheme() {
            "https" => "wss",
            "http" => "ws",
            other => other,
        }
        .to_string();
        let _ = url.set_scheme(&scheme);
    }
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in query {
            pairs.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

fn remote_server_url(settings: &app_settings::RemoteSettings) -> String {
    if settings.server_url.trim().is_empty() {
        "http://127.0.0.1:8088".to_string()
    } else {
        settings.server_url.trim().to_string()
    }
}

fn remote_host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| paths::app_display_name().to_string())
}

fn remote_random_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn ensure_remote_host_identity(settings: &mut app_settings::RemoteSettings) {
    if let Some(private_key) = remote_e2e_private_key(&settings.host_private_key) {
        let public_key = X25519PublicKey::from(&private_key);
        let derived_public = remote_base64_url_encode(public_key.as_bytes());
        if settings.host_public_key.trim().is_empty() || settings.host_public_key == derived_public
        {
            settings.host_public_key = derived_public;
            return;
        }
    }
    let private_key = StaticSecret::random();
    let public_key = X25519PublicKey::from(&private_key);
    settings.host_private_key = remote_base64_url_encode(private_key.to_bytes().as_slice());
    settings.host_public_key = remote_base64_url_encode(public_key.as_bytes());
}

fn remote_pairing_qr_payload(
    settings: &app_settings::RemoteSettings,
    pairing: &RemotePairingInfo,
) -> String {
    let payload = json!({
        "server": remote_server_url(settings),
        "code": pairing.code,
        "secret": pairing.secret,
        "hostName": remote_host_name(),
        "hostPublicKey": settings.host_public_key,
        "cryptoVersion": 1,
    });
    serde_json::to_vec(&payload)
        .ok()
        .map(|data| remote_base64_url_encode(&data))
        .unwrap_or_default()
}

fn remote_pairing_match_code(
    settings: &app_settings::RemoteSettings,
    pairing_code: &str,
    pairing_secret: &str,
    device_public_key: &str,
) -> Option<String> {
    if settings.host_public_key.trim().is_empty() || device_public_key.trim().is_empty() {
        return None;
    }
    let material = format!(
        "codux-e2e-match-v1|{}|{}|{}|{}",
        settings.host_public_key, device_public_key, pairing_code, pairing_secret
    );
    let digest = Sha256::digest(material.as_bytes());
    let prefix = digest
        .iter()
        .take(3)
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    Some(format!("{}-{}", &prefix[..3], &prefix[3..]))
}

fn remote_e2e_private_key(value: &str) -> Option<StaticSecret> {
    let bytes = remote_base64_url_decode(value).ok()?;
    let array: [u8; 32] = bytes.as_slice().try_into().ok()?;
    Some(StaticSecret::from(array))
}

fn remote_e2e_symmetric_key(
    host_private_key: &str,
    remote_public_key: &str,
    host_id: &str,
    device_id: &str,
) -> Result<[u8; 32], String> {
    let private_key = remote_e2e_private_key(host_private_key)
        .ok_or_else(|| "Invalid host private key.".to_string())?;
    let public_bytes = remote_base64_url_decode(remote_public_key)?;
    let public_array: [u8; 32] = public_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "Invalid device public key.".to_string())?;
    let public_key = X25519PublicKey::from(public_array);
    let shared = private_key.diffie_hellman(&public_key);
    let salt = format!("codux-e2e-v1|{host_id}|{device_id}");
    let hkdf = Hkdf::<Sha256>::new(Some(salt.as_bytes()), shared.as_bytes());
    let mut key = [0_u8; 32];
    hkdf.expand(b"codux-remote-payload-v1", &mut key)
        .map_err(|_| "Failed to derive encryption key.".to_string())?;
    Ok(key)
}

fn remote_e2e_encrypt(
    plaintext: &[u8],
    key: &[u8; 32],
    host_id: &str,
    device_id: &str,
) -> Result<Value, String> {
    let nonce_bytes = uuid::Uuid::new_v4().as_bytes()[..12].to_vec();
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let aad = format!("codux-e2e-aad-v1|{host_id}|{device_id}");
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "Failed to encrypt remote payload.".to_string())?;
    if encrypted.len() < 16 {
        return Err("Invalid encrypted payload.".to_string());
    }
    let (ciphertext, tag) = encrypted.split_at(encrypted.len() - 16);
    Ok(json!({
        "v": 1,
        "alg": "X25519-HKDF-SHA256-AES-256-GCM",
        "nonce": remote_base64_url_encode(&nonce_bytes),
        "ciphertext": remote_base64_url_encode(ciphertext),
        "tag": remote_base64_url_encode(tag),
    }))
}

fn remote_e2e_decrypt(
    payload: &Value,
    key: &[u8; 32],
    host_id: &str,
    device_id: &str,
) -> Result<Vec<u8>, String> {
    if payload.get("v").and_then(Value::as_i64) != Some(1) {
        return Err("Unsupported encrypted payload.".to_string());
    }
    let nonce = remote_base64_url_decode(
        payload
            .get("nonce")
            .and_then(Value::as_str)
            .ok_or_else(|| "Missing nonce.".to_string())?,
    )?;
    let mut ciphertext = remote_base64_url_decode(
        payload
            .get("ciphertext")
            .and_then(Value::as_str)
            .ok_or_else(|| "Missing ciphertext.".to_string())?,
    )?;
    let tag = remote_base64_url_decode(
        payload
            .get("tag")
            .and_then(Value::as_str)
            .ok_or_else(|| "Missing tag.".to_string())?,
    )?;
    ciphertext.extend_from_slice(&tag);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let aad = format!("codux-e2e-aad-v1|{host_id}|{device_id}");
    cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "Failed to decrypt remote payload.".to_string())
}

fn remote_base64_url_encode(data: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, data)
}

fn remote_base64_url_decode(value: &str) -> Result<Vec<u8>, String> {
    base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, value)
        .map_err(|error| error.to_string())
}

fn remote_error_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteStatus {
    enabled: bool,
    relay: String,
    devices: u32,
    encryption: String,
    status: String,
    message: String,
    host_id: String,
    pairing: Option<RemotePairingInfo>,
    device_list: Vec<RemoteHostDevice>,
    pending_pairings: Vec<RemotePendingPairing>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteHostDevice {
    id: String,
    host_id: String,
    name: String,
    public_key: String,
    created_at: String,
    last_seen: String,
    revoked_at: Option<String>,
    online: Option<bool>,
}

impl From<app_settings::RemoteHostDeviceSettings> for RemoteHostDevice {
    fn from(device: app_settings::RemoteHostDeviceSettings) -> Self {
        Self {
            id: device.id,
            host_id: device.host_id,
            name: device.name,
            public_key: device.public_key,
            created_at: device.created_at,
            last_seen: device.last_seen,
            revoked_at: device.revoked_at,
            online: device.online,
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemotePairingInfo {
    pairing_id: String,
    code: String,
    secret: String,
    host_public_key: Option<String>,
    crypto_version: Option<u32>,
    expires_at: String,
    qr_payload: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemotePendingPairing {
    id: String,
    device_name: String,
    device_public_key: String,
    code: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemotePairingStatusResponse {
    status: String,
    pairing_id: Option<String>,
    code: Option<String>,
    device_name: Option<String>,
    device_public_key: Option<String>,
}

#[cfg(debug_assertions)]
fn toggle_devtools(window: &tauri::WebviewWindow) {
    if window.is_devtools_open() {
        window.close_devtools();
    } else {
        window.open_devtools();
    }
}

const MENU_TOGGLE_DEVTOOLS: &str = "toggle-devtools";
const MENU_SHOW_ABOUT: &str = "show-about";
const MENU_OPEN_SETTINGS: &str = "open-settings";
const MENU_CHECK_UPDATES: &str = "check-updates";
const MENU_EXPORT_DIAGNOSTICS: &str = "export-diagnostics";
const MENU_OPEN_RUNTIME_LOG: &str = "open-runtime-log";
const MENU_OPEN_LIVE_LOG: &str = "open-live-log";
const MENU_OPEN_WEBSITE: &str = "open-website";
const MENU_OPEN_GITHUB: &str = "open-github";
const MENU_NEW_PROJECT: &str = "new-project";
const MENU_OPEN_FOLDER: &str = "open-folder";
const MENU_CLOSE_CURRENT_PROJECT: &str = "close-current-project";
const MENU_CLOSE_ALL_PROJECTS: &str = "close-all-projects";
const MENU_VIEW_TERMINAL: &str = "view-terminal";
const MENU_VIEW_FILES: &str = "view-files";
const MENU_VIEW_REVIEW: &str = "view-review";
const MENU_TOGGLE_PROJECTS: &str = "toggle-projects";
const MENU_TOGGLE_TASKS: &str = "toggle-tasks";
const MENU_OPEN_GIT_PANEL: &str = "open-git-panel";
const MENU_OPEN_FILES_PANEL: &str = "open-files-panel";
const MENU_OPEN_AI_PANEL: &str = "open-ai-panel";
const MENU_OPEN_SSH_PANEL: &str = "open-ssh-panel";
const MENU_CREATE_SPLIT: &str = "create-split";
const MENU_CREATE_TAB: &str = "create-tab";
const MENU_CREATE_TASK: &str = "create-task";
const MENU_EDITOR_SAVE: &str = "editor-save";
const MENU_EDITOR_SEARCH: &str = "editor-search";
const MENU_CLOSE_ACTIVE: &str = "close-active";
const CODUX_WEBSITE_URL: &str = "https://codux.dux.cn";
const CODUX_GITHUB_URL: &str = "https://github.com/duxweb/codux";

fn apply_desktop_pet_menu_action(app: &tauri::AppHandle, id: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    match id {
        DESKTOP_PET_MUTE_30_MINUTES => {
            let _ = update_desktop_pet_settings(state, app.clone(), |settings| {
                settings.ai.pet.speech_temporary_mute_until =
                    Some(chrono::Utc::now().timestamp() + 1800);
            });
        }
        DESKTOP_PET_MUTE_1_HOUR => {
            let _ = update_desktop_pet_settings(state, app.clone(), |settings| {
                settings.ai.pet.speech_temporary_mute_until =
                    Some(chrono::Utc::now().timestamp() + 3600);
            });
        }
        DESKTOP_PET_MUTE_TODAY => {
            let tomorrow = chrono::Local::now()
                .date_naive()
                .succ_opt()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .and_then(|date| date.and_local_timezone(chrono::Local).single())
                .map(|date| date.timestamp())
                .unwrap_or_else(|| chrono::Utc::now().timestamp() + 86_400);
            let _ = update_desktop_pet_settings(state, app.clone(), |settings| {
                settings.ai.pet.speech_temporary_mute_until = Some(tomorrow);
            });
        }
        DESKTOP_PET_SKIP_LINE => {
            if let Some(window) = app.get_webview_window(DESKTOP_PET_LABEL) {
                let _ = window.emit("desktop-pet:skip-line", ());
            }
        }
        DESKTOP_PET_SPEAK_MORE => {
            let _ = update_desktop_pet_settings(state, app.clone(), |settings| {
                settings.ai.pet.speech_frequency =
                    desktop_pet_raised_speech_frequency(&settings.ai.pet.speech_frequency);
            });
        }
        DESKTOP_PET_SPEAK_LESS => {
            let _ = update_desktop_pet_settings(state, app.clone(), |settings| {
                settings.ai.pet.speech_frequency =
                    desktop_pet_lowered_speech_frequency(&settings.ai.pet.speech_frequency);
            });
        }
        DESKTOP_PET_HIDE => {
            let _ = update_desktop_pet_settings(state, app.clone(), |settings| {
                settings.pet.desktop_widget = false;
            });
        }
        _ => {}
    }
}

fn desktop_pet_raised_speech_frequency(value: &str) -> String {
    match value.trim() {
        "quiet" => "normal".to_string(),
        "normal" => "lively".to_string(),
        "lively" => "chatterbox".to_string(),
        "chatterbox" => "chatterbox".to_string(),
        _ => "lively".to_string(),
    }
}

fn desktop_pet_lowered_speech_frequency(value: &str) -> String {
    match value.trim() {
        "quiet" => "quiet".to_string(),
        "normal" => "quiet".to_string(),
        "lively" => "normal".to_string(),
        "chatterbox" => "lively".to_string(),
        _ => "quiet".to_string(),
    }
}

fn configured_accelerator(settings: &AppSettings, id: &str, default: &str) -> String {
    settings
        .shortcuts
        .get(id)
        .and_then(|value| shortcut_to_accelerator(value))
        .unwrap_or_else(|| default.to_string())
}

fn shortcut_to_accelerator(value: &str) -> Option<String> {
    let first = value
        .split('/')
        .map(str::trim)
        .find(|item| !item.is_empty())?;
    let normalized = first
        .replace('⌘', "Cmd+")
        .replace('⌃', "Ctrl+")
        .replace('⌥', "Alt+")
        .replace('⇧', "Shift+")
        .replace("Command", "Cmd")
        .replace("Control", "Ctrl")
        .replace("Option", "Alt")
        .replace("Meta", "Cmd")
        .replace(char::is_whitespace, "");
    let mut parts: Vec<String> = normalized
        .split('+')
        .filter(|part| !part.is_empty())
        .map(|part| match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "meta" => "CmdOrCtrl".to_string(),
            "ctrl" | "control" => "Ctrl".to_string(),
            "alt" | "option" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "," => "Comma".to_string(),
            "\\" => "Backslash".to_string(),
            key if key.len() == 1 => key.to_ascii_uppercase(),
            _ => part.to_string(),
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    if parts.iter().all(|part| {
        matches!(
            part.as_str(),
            "CmdOrCtrl" | "Ctrl" | "Alt" | "Shift" | "Command" | "Cmd"
        )
    }) {
        return None;
    }
    let key = parts.pop()?;
    let mut modifiers = Vec::new();
    for candidate in ["CmdOrCtrl", "Ctrl", "Alt", "Shift"] {
        if parts.iter().any(|part| part == candidate) {
            modifiers.push(candidate);
        }
    }
    modifiers.push(key.as_str());
    Some(modifiers.join("+"))
}

fn build_app_menu(app: &tauri::AppHandle, settings: &AppSettings) -> tauri::Result<Menu<Wry>> {
    let labels = MenuLabels::load(settings);
    let accelerators = MenuAccelerators::load(settings);

    let about = MenuItem::with_id(
        app,
        MENU_SHOW_ABOUT,
        labels.about.clone(),
        true,
        None::<&str>,
    )?;
    let settings_item = MenuItem::with_id(
        app,
        MENU_OPEN_SETTINGS,
        labels.app_menu_settings.clone(),
        true,
        Some(accelerators.settings.as_str()),
    )?;
    let updates = MenuItem::with_id(
        app,
        MENU_CHECK_UPDATES,
        labels.check_updates.clone(),
        true,
        None::<&str>,
    )?;
    let app_menu = Submenu::with_items(
        app,
        labels.app_name.clone(),
        true,
        &[
            &about,
            &updates,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, Some(labels.services.as_str()))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some(labels.hide_app.as_str()))?,
            &PredefinedMenuItem::hide_others(app, Some(labels.hide_others.as_str()))?,
            &PredefinedMenuItem::show_all(app, Some(labels.show_all.as_str()))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some(labels.quit.as_str()))?,
        ],
    )?;

    let new_project = MenuItem::with_id(
        app,
        MENU_NEW_PROJECT,
        labels.new_project,
        true,
        Some(accelerators.new_project.as_str()),
    )?;
    let open_folder = MenuItem::with_id(
        app,
        MENU_OPEN_FOLDER,
        labels.open_folder,
        true,
        Some("CmdOrCtrl+O"),
    )?;
    let close_current_project = MenuItem::with_id(
        app,
        MENU_CLOSE_CURRENT_PROJECT,
        labels.close_current_project,
        true,
        None::<&str>,
    )?;
    let close_all_projects = MenuItem::with_id(
        app,
        MENU_CLOSE_ALL_PROJECTS,
        labels.close_all_projects,
        true,
        None::<&str>,
    )?;
    let close_window = PredefinedMenuItem::close_window(app, Some(labels.close_window.as_str()))?;
    let close_active = MenuItem::with_id(
        app,
        MENU_CLOSE_ACTIVE,
        tr_or_label(
            settings,
            "menu.file.close_current_split",
            "Close Current Split",
        ),
        true,
        Some(accelerators.close_active.as_str()),
    )?;
    let file_menu = Submenu::with_items(
        app,
        labels.file,
        true,
        &[
            &new_project,
            &open_folder,
            &PredefinedMenuItem::separator(app)?,
            &close_current_project,
            &close_all_projects,
            &PredefinedMenuItem::separator(app)?,
            &close_active,
            &close_window,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        tr_or_label(settings, "menu.edit", "Edit"),
        true,
        &[
            &PredefinedMenuItem::undo(
                app,
                Some(tr_or_label(settings, "common.undo", "Undo").as_str()),
            )?,
            &PredefinedMenuItem::redo(
                app,
                Some(tr_or_label(settings, "common.redo", "Redo").as_str()),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(
                app,
                Some(tr_or_label(settings, "common.cut", "Cut").as_str()),
            )?,
            &PredefinedMenuItem::copy(
                app,
                Some(tr_or_label(settings, "common.copy", "Copy").as_str()),
            )?,
            &PredefinedMenuItem::paste(
                app,
                Some(tr_or_label(settings, "common.paste", "Paste").as_str()),
            )?,
            &PredefinedMenuItem::select_all(
                app,
                Some(tr_or_label(settings, "common.select_all", "Select All").as_str()),
            )?,
        ],
    )?;

    let view_terminal = MenuItem::with_id(
        app,
        MENU_VIEW_TERMINAL,
        labels.terminal,
        true,
        Some(accelerators.view_terminal.as_str()),
    )?;
    let view_files = MenuItem::with_id(
        app,
        MENU_VIEW_FILES,
        labels.files,
        true,
        Some(accelerators.view_files.as_str()),
    )?;
    let view_review = MenuItem::with_id(
        app,
        MENU_VIEW_REVIEW,
        labels.review,
        true,
        Some(accelerators.view_review.as_str()),
    )?;
    let toggle_projects = MenuItem::with_id(
        app,
        MENU_TOGGLE_PROJECTS,
        labels.projects_sidebar,
        true,
        Some("CmdOrCtrl+Option+P"),
    )?;
    let toggle_tasks = MenuItem::with_id(
        app,
        MENU_TOGGLE_TASKS,
        labels.tasks_sidebar,
        true,
        Some("CmdOrCtrl+Option+T"),
    )?;
    let git_panel = MenuItem::with_id(
        app,
        MENU_OPEN_GIT_PANEL,
        labels.git_panel,
        true,
        Some("CmdOrCtrl+Shift+G"),
    )?;
    let files_panel = MenuItem::with_id(
        app,
        MENU_OPEN_FILES_PANEL,
        labels.files_panel,
        true,
        Some("CmdOrCtrl+Shift+F"),
    )?;
    let ai_panel = MenuItem::with_id(
        app,
        MENU_OPEN_AI_PANEL,
        labels.ai_panel,
        true,
        Some("CmdOrCtrl+Shift+A"),
    )?;
    let ssh_panel = MenuItem::with_id(
        app,
        MENU_OPEN_SSH_PANEL,
        labels.ssh_panel,
        true,
        Some("CmdOrCtrl+Shift+S"),
    )?;
    let create_split = MenuItem::with_id(
        app,
        MENU_CREATE_SPLIT,
        labels.create_split,
        true,
        Some("CmdOrCtrl+Shift+\\"),
    )?;
    let create_tab = MenuItem::with_id(
        app,
        MENU_CREATE_TAB,
        labels.create_tab,
        true,
        Some("CmdOrCtrl+Shift+T"),
    )?;
    let create_task = MenuItem::with_id(
        app,
        MENU_CREATE_TASK,
        tr_or_label(settings, "shortcut.task.create", "New Worktree"),
        true,
        Some(accelerators.create_task.as_str()),
    )?;
    let editor_save = MenuItem::with_id(
        app,
        MENU_EDITOR_SAVE,
        tr_or_label(settings, "shortcut.editor.save", "Save File"),
        true,
        Some(accelerators.editor_save.as_str()),
    )?;
    let editor_search = MenuItem::with_id(
        app,
        MENU_EDITOR_SEARCH,
        tr_or_label(settings, "shortcut.editor.search", "Search File"),
        true,
        Some(accelerators.editor_search.as_str()),
    )?;
    let workspace_menu = Submenu::with_id_and_items(
        app,
        "codux-workspace-menu",
        labels.workspace,
        true,
        &[
            &view_terminal,
            &view_files,
            &view_review,
            &PredefinedMenuItem::separator(app)?,
            &toggle_projects,
            &toggle_tasks,
            &PredefinedMenuItem::separator(app)?,
            &create_split,
            &create_tab,
            &create_task,
            &PredefinedMenuItem::separator(app)?,
            &editor_save,
            &editor_search,
        ],
    )?;

    let view_menu = Submenu::with_items(
        app,
        labels.view,
        true,
        &[
            &git_panel,
            &files_panel,
            &ai_panel,
            &ssh_panel,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(
                app,
                Some(
                    tr_or_label(
                        settings,
                        "menu.view.toggle_full_screen",
                        "Toggle Full Screen",
                    )
                    .as_str(),
                ),
            )?,
        ],
    )?;

    let window_menu = Submenu::with_id_and_items(
        app,
        tauri::menu::WINDOW_SUBMENU_ID,
        labels.window,
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(labels.minimize.as_str()))?,
            &PredefinedMenuItem::maximize(app, Some(labels.zoom.as_str()))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some(labels.close_window.as_str()))?,
        ],
    )?;

    let diagnostics = MenuItem::with_id(
        app,
        MENU_EXPORT_DIAGNOSTICS,
        labels.diagnostics,
        true,
        None::<&str>,
    )?;
    let runtime_log = MenuItem::with_id(
        app,
        MENU_OPEN_RUNTIME_LOG,
        labels.runtime_log,
        true,
        None::<&str>,
    )?;
    let live_log = MenuItem::with_id(app, MENU_OPEN_LIVE_LOG, labels.live_log, true, None::<&str>)?;
    let website = MenuItem::with_id(app, MENU_OPEN_WEBSITE, labels.website, true, None::<&str>)?;
    let github = MenuItem::with_id(app, MENU_OPEN_GITHUB, labels.github, true, None::<&str>)?;
    #[cfg(debug_assertions)]
    let devtools = MenuItem::with_id(
        app,
        MENU_TOGGLE_DEVTOOLS,
        labels.devtools,
        true,
        Some("CmdOrCtrl+Option+I"),
    )?;

    let help_menu = Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        labels.help,
        true,
        &[
            &diagnostics,
            &runtime_log,
            &live_log,
            &PredefinedMenuItem::separator(app)?,
            &website,
            &github,
            #[cfg(debug_assertions)]
            &PredefinedMenuItem::separator(app)?,
            #[cfg(debug_assertions)]
            &devtools,
        ],
    )?;

    Menu::with_items(
        app,
        &[
            &app_menu,
            &file_menu,
            &edit_menu,
            &workspace_menu,
            &view_menu,
            &window_menu,
            &help_menu,
        ],
    )
}

fn tr_or_label(settings: &AppSettings, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(&settings.language);
    i18n::translate(&locale, key, fallback)
}

fn rebuild_app_menu(app: &tauri::AppHandle, settings: &AppSettings) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        let _ = settings;
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let menu = build_app_menu(app, settings).map_err(|error| error.to_string())?;
        app.set_menu(menu).map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn configure_windows_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.set_decorations(false);
    let _ = window.set_background_color(Some(Color(44, 48, 55, 255)));
}

#[cfg(debug_assertions)]
fn configure_debug_identity(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title(paths::app_display_name());
    }
    configure_debug_process_name();
}

#[cfg(not(debug_assertions))]
fn configure_debug_identity(_app: &tauri::AppHandle) {}

#[cfg(all(debug_assertions, target_os = "macos"))]
fn configure_debug_process_name() {
    use objc2_foundation::{NSProcessInfo, NSString};

    let name = NSString::from_str(paths::app_display_name());
    NSProcessInfo::processInfo().setProcessName(&name);
}

#[cfg(all(debug_assertions, not(target_os = "macos")))]
fn configure_debug_process_name() {}

pub fn run() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let settings = Arc::new(AppSettingsStore::load_or_seed());
    sync_process_locale_preference(&settings.snapshot());
    let menu_settings = Arc::clone(&settings);
    let setup_settings = Arc::clone(&settings);
    let builder = tauri::Builder::default();
    #[cfg(not(target_os = "windows"))]
    let builder = builder
        .menu(move |app| {
            let settings = menu_settings.snapshot();
            build_app_menu(app, &settings)
        })
        .on_menu_event(|app, event| {
            let event_id = event.id().0.as_str();
            if event_id.starts_with("desktop-pet:") {
                apply_desktop_pet_menu_action(app, event_id);
                return;
            }
            if let Some(window) = app.get_webview_window("main") {
                match event_id {
                    MENU_NEW_PROJECT => {
                        let _ = window.emit("app-menu:project-create", ());
                    }
                    MENU_OPEN_FOLDER => {
                        let _ = window.emit("app-menu:project-open-folder", ());
                    }
                    MENU_CLOSE_CURRENT_PROJECT => {
                        let _ = window.emit("app-menu:project-close-current", ());
                    }
                    MENU_CLOSE_ALL_PROJECTS => {
                        let _ = window.emit("app-menu:project-close-all", ());
                    }
                    MENU_VIEW_TERMINAL => {
                        let _ = window.emit("app-menu:view", "terminal");
                    }
                    MENU_VIEW_FILES => {
                        let _ = window.emit("app-menu:view", "files");
                    }
                    MENU_VIEW_REVIEW => {
                        let _ = window.emit("app-menu:view", "review");
                    }
                    MENU_TOGGLE_PROJECTS => {
                        let _ = window.emit("app-menu:toggle-sidebar", "projects");
                    }
                    MENU_TOGGLE_TASKS => {
                        let _ = window.emit("app-menu:toggle-sidebar", "tasks");
                    }
                    MENU_OPEN_GIT_PANEL => {
                        let _ = window.emit("app-menu:right-panel", "git");
                    }
                    MENU_OPEN_FILES_PANEL => {
                        let _ = window.emit("app-menu:right-panel", "files");
                    }
                    MENU_OPEN_AI_PANEL => {
                        let _ = window.emit("app-menu:right-panel", "ai");
                    }
                    MENU_OPEN_SSH_PANEL => {
                        let _ = window.emit("app-menu:right-panel", "ssh");
                    }
                    MENU_CREATE_SPLIT => {
                        let _ = window.emit("app-menu:workspace-command", "add-top-terminal-split");
                    }
                    MENU_CREATE_TAB => {
                        let _ =
                            window.emit("app-menu:workspace-command", "add-bottom-terminal-tab");
                    }
                    MENU_CREATE_TASK => {
                        let _ = window.emit("app-menu:task-create", ());
                    }
                    MENU_EDITOR_SAVE => {
                        let _ = window.emit("app-menu:workspace-command", "editor-save");
                    }
                    MENU_EDITOR_SEARCH => {
                        let _ = window.emit("app-menu:workspace-command", "editor-search");
                    }
                    MENU_CLOSE_ACTIVE => {
                        if let Some(focused) = app
                            .webview_windows()
                            .into_values()
                            .find(|candidate| candidate.is_focused().unwrap_or(false))
                        {
                            if focused.label() == "main" {
                                let _ = focused.emit("app-menu:workspace-command", "close-active");
                            } else {
                                let _ = focused.close();
                            }
                        }
                    }
                    MENU_OPEN_SETTINGS => {
                        let _ = window.emit("app-menu:settings", ());
                    }
                    MENU_CHECK_UPDATES => {
                        let _ = window.emit("app-menu:check-updates", ());
                    }
                    MENU_EXPORT_DIAGNOSTICS => {
                        let _ = window.emit("app-menu:export-diagnostics", ());
                    }
                    MENU_OPEN_RUNTIME_LOG => {
                        let _ = app_info::open_runtime_log();
                    }
                    MENU_OPEN_LIVE_LOG => {
                        let _ = app_info::open_live_log();
                    }
                    MENU_OPEN_WEBSITE => {
                        let _ = app_info::open_url(CODUX_WEBSITE_URL);
                    }
                    MENU_OPEN_GITHUB => {
                        let _ = app_info::open_url(CODUX_GITHUB_URL);
                    }
                    MENU_SHOW_ABOUT => {
                        let _ = window.emit("app-menu:about", ());
                    }
                    MENU_TOGGLE_DEVTOOLS => {
                        #[cfg(debug_assertions)]
                        if let Some(focused) = app
                            .webview_windows()
                            .into_values()
                            .find(|candidate| candidate.is_focused().unwrap_or(false))
                        {
                            toggle_devtools(&focused);
                        } else {
                            toggle_devtools(&window);
                        }
                    }
                    _ => {}
                }
            }
        });
    #[cfg(target_os = "windows")]
    {
        let _ = menu_settings;
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let setup_started_at = Instant::now();
            runtime_trace("startup", "setup start");
            configure_debug_identity(app.handle());
            #[cfg(target_os = "windows")]
            configure_windows_main_window(app.handle());
            let settings = Arc::clone(&setup_settings);
            let memory =
                Arc::new(MemoryStore::load_or_create().map_err(|error| error.to_string())?);
            let _ = memory.recover_interrupted_extractions();
            let projects = Arc::new(ProjectStore::load_or_default());
            let project_count = projects.projects_snapshot().len();
            let ai_runtime = Arc::new(AIRuntimeBridge::new(
                Arc::clone(&settings),
                Arc::clone(&memory),
                Arc::clone(&projects),
            ));
            let ai_history = Arc::new(AIHistoryIndexer::new(app.handle().clone()));
            let project_activity = Arc::new(ProjectActivityCoordinator::new());
            project_activity.seed_projects(projects.projects_snapshot());
            let power = Arc::new(PowerManager::default());
            power.start_settings_sync(Arc::clone(&settings))?;
            let terminals = Arc::new(TerminalManager::new(
                Arc::clone(&ai_runtime),
                Arc::clone(&settings),
                Arc::clone(&memory),
            ));
            let remote = Arc::new(RemoteHostService::new(
                app.handle().clone(),
                Arc::clone(&settings),
                Arc::clone(&projects),
                Arc::clone(&terminals),
                Arc::clone(&ai_history),
            ));
            let state = AppState {
                terminals,
                remote: Arc::clone(&remote),
                performance: Arc::new(PerformanceMonitor::default()),
                power,
                ai_runtime,
                ai_history: Arc::clone(&ai_history),
                memory,
                settings: Arc::clone(&settings),
                projects: Arc::clone(&projects),
                project_activity: Arc::clone(&project_activity),
                pet: Arc::new(PetStore::load_or_seed()),
                ssh: Arc::new(SSHStore::load_or_seed()),
                git_watch: Arc::new(GitWatchManager::default()),
                git_cancels: Arc::new(Mutex::new(HashMap::new())),
                file_watch: Arc::new(FileWatchManager::default()),
                desktop_pet_hit_state: Arc::new(DesktopPetHitState::default()),
                is_exiting: Arc::new(AtomicBool::new(false)),
            };
            state
                .ai_runtime
                .start_listener_background(app.handle().clone());
            let initial_settings = state.settings.snapshot();
            let initial_pet = state.pet.snapshot().ok();
            runtime_trace(
                "startup",
                &format!(
                    "setup stores loaded projects={} pet_cached={}",
                    project_count,
                    initial_pet.is_some()
                ),
            );
            let terminals_for_window_events = Arc::clone(&state.terminals);
            let is_exiting_for_window_events = Arc::clone(&state.is_exiting);
            Arc::clone(&project_activity).start(
                app.handle().clone(),
                Arc::clone(&settings),
                ai_history,
                Arc::clone(&projects),
            );
            remote.start(app.handle().clone());
            app.manage(state);
            let _ = apply_app_icon(app.handle(), &initial_settings.icon_style);
            if let Some(pet) = initial_pet {
                sync_desktop_pet_window(app.handle(), &initial_settings, &pet);
            }
            let activity_for_window_events = Arc::clone(&project_activity);
            if let Some(main_window) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                activity_for_window_events
                    .mark_main_window_visible(main_window.is_visible().unwrap_or(true));
                activity_for_window_events
                    .mark_main_window_focused(main_window.is_focused().unwrap_or(false));
                main_window.on_window_event(move |event| match event {
                    WindowEvent::Focused(focused) => {
                        activity_for_window_events.mark_main_window_focused(*focused);
                        if *focused {
                            activity_for_window_events.mark_main_window_visible(true);
                        }
                    }
                    WindowEvent::CloseRequested { api, .. } => {
                        activity_for_window_events.mark_main_window_visible(false);
                        activity_for_window_events.mark_main_window_focused(false);
                        if shutdown_runtime_state(
                            &terminals_for_window_events,
                            &is_exiting_for_window_events,
                        ) {
                            api.prevent_close();
                            app_handle.exit(0);
                        }
                    }
                    WindowEvent::Destroyed => {
                        activity_for_window_events.mark_main_window_visible(false);
                        activity_for_window_events.mark_main_window_focused(false);
                    }
                    _ => {}
                });
            } else {
                project_activity.mark_main_window_visible(true);
            }
            runtime_trace_elapsed(
                "startup",
                "setup finish",
                setup_started_at,
                &format!("projects={project_count}"),
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            project_list,
            project_create,
            project_close,
            project_close_all,
            project_mark_active,
            project_select,
            project_update,
            project_reorder,
            project_select_worktree,
            project_set_default_push_remote,
            project_open_applications,
            project_open_in_application,
            project_reveal_in_file_manager,
            terminal_layout_get,
            terminal_layout_save,
            remote_status,
            remote_snapshot_emit,
            remote_reconnect,
            remote_devices_refresh,
            remote_device_revoke,
            remote_pairing_create,
            remote_pairing_cancel,
            remote_pairing_confirm,
            remote_pairing_reject,
            power_set_sleep_prevention,
            notification_dispatch_channels,
            app_about_metadata,
            app_update_status,
            app_update_install,
            diagnostics_export,
            app_open_runtime_log,
            app_open_live_log,
            app_open_url,
            app_toggle_devtools,
            runtime_trace_frontend,
            app_window_close,
            terminal_create,
            terminal_attach,
            terminal_write,
            terminal_resize,
            terminal_interrupt,
            terminal_clear_history,
            terminal_escape,
            terminal_kill,
            terminal_snapshot,
            ai_runtime_snapshot,
            ai_runtime_probe,
            ai_runtime_state_snapshot,
            app_runtime_ready,
            app_window_state,
            ai_runtime_dismiss_completion,
            app_settings_get,
            app_settings_set,
            localized_open_dialog,
            localized_save_dialog,
            desktop_pet_placement,
            desktop_pet_set_bubble_visible,
            desktop_pet_start_drag,
            desktop_pet_show_context_menu,
            desktop_pet_sync_visibility,
            i18n_bundle_get,
            llm_complete,
            llm_provider_test,
            pet_idle_speech,
            memory_extraction_status,
            memory_extraction_cancel,
            memory_management_snapshot,
            memory_manager_snapshot,
            memory_archive_entry,
            memory_delete_entry,
            memory_delete_summary,
            memory_delete_project_profile,
            memory_delete_project,
            memory_migrate_project,
            memory_update_summary,
            memory_refresh_project_profile,
            memory_index_now,
            app_request_restart,
            ai_history_project_summary,
            ai_history_refresh_project,
            ai_history_project_state,
            ai_history_global_summary,
            ai_history_refresh_global,
            ai_history_global_state,
            ai_history_global_today_normalized_tokens,
            ai_history_session_rename,
            ai_history_session_remove,
            git_status,
            git_cancel,
            git_refresh_project,
            git_stage,
            git_unstage,
            git_commit,
            git_commit_action,
            git_amend_last_commit_message,
            git_last_commit_message,
            git_undo_last_commit,
            git_head_commit_pushed,
            git_pull,
            git_push,
            git_fetch,
            git_sync,
            git_review,
            git_init,
            git_clone,
            git_discard,
            git_branches,
            git_checkout_branch,
            git_checkout_remote_branch,
            git_create_branch,
            git_merge_branch,
            git_squash_merge_branch,
            git_delete_branch,
            git_force_push,
            git_push_remote,
            git_push_remote_branch,
            git_checkout_commit,
            git_revert_commit,
            git_restore_commit,
            git_add_remote,
            git_remove_remote,
            git_append_gitignore,
            git_diff_file,
            git_commit_message_context,
            git_review_diff_file,
            git_review_file_content,
            git_watch,
            git_unwatch,
            file_list_children,
            file_read,
            file_write,
            file_create_file,
            file_create_dir,
            file_rename,
            file_delete,
            file_copy,
            file_import_external,
            file_reveal,
            file_open,
            file_watch,
            file_unwatch,
            worktree_create,
            worktree_remove,
            worktree_merge,
            performance_snapshot,
            pet_catalog,
            pet_custom_install_preview,
            pet_custom_install,
            pet_custom_sprite,
            pet_refresh,
            pet_snapshot,
            pet_claim,
            pet_rename,
            pet_archive_current,
            pet_restore_archived,
            ssh_profiles,
            ssh_profile_upsert,
            ssh_profile_delete,
            ssh_profile_test,
            ssh_launch_command,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Codux application")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
                if let Some(state) = app.try_state::<AppState>() {
                    shutdown_runtime_state(&state.terminals, &state.is_exiting);
                }
            }
            _ => {}
        });
}
