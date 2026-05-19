mod ai_history;
mod ai_history_indexer;
mod ai_runtime;
mod ai_usage_store;
mod app_info;
mod app_settings;
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
mod project_store;
mod ssh;
mod terminal;
mod worktree;

use ai_history::{AIGlobalHistorySnapshot, AIHistoryProjectRequest};
use ai_history_indexer::AIHistoryIndexer;
use ai_history_indexer::AIHistoryProjectState;
use ai_runtime::{
    AIRuntimeBridge, AIRuntimeBridgeSnapshot, AIRuntimeContextSnapshot, AIRuntimeProbeRequest,
    AIRuntimeStateSnapshot,
};
use app_info::{
    AppAboutMetadata, DiagnosticsExportRequest, DiagnosticsExportResult, UpdateInstallResult,
    UpdateStatus,
};
use app_settings::{
    locale_from_language_setting, sync_process_locale_preference, AIProviderSettings, AppSettings,
    AppSettingsStore,
};
use files::{
    file_copy as copy_file_path, file_create_dir as create_file_dir,
    file_create_file as create_file_file, file_delete as delete_file_path,
    file_import_external as import_external_file_paths, file_list_children as list_file_children,
    file_open as open_file_path, file_read as read_file_path, file_rename as rename_file_path,
    file_reveal as reveal_file_path, file_write as write_file_path, FileChildrenRequest, FileCopyRequest, FileCreateRequest,
    FileEntry, FileExternalCopyRequest, FilePathRequest, FileReadResult, FileRenameRequest,
    FileWatchManager, FileWriteRequest,
};
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
    git_discard as perform_git_discard, git_fetch as perform_git_fetch,
    git_force_push as perform_git_force_push,
    git_head_commit_pushed as load_git_head_commit_pushed, git_init as perform_git_init,
    git_last_commit_message as load_git_last_commit_message,
    git_merge_branch as perform_git_merge_branch, git_pull as perform_git_pull,
    git_push as perform_git_push, git_push_remote as perform_git_push_remote,
    git_push_remote_branch as perform_git_push_remote_branch,
    git_remove_remote as perform_git_remove_remote,
    git_restore_commit as perform_git_restore_commit,
    git_revert_commit as perform_git_revert_commit, git_review as load_git_review,
    git_review_diff_file as load_git_review_diff_file,
    git_squash_merge_branch as perform_git_squash_merge_branch, git_stage as perform_git_stage,
    git_status as load_git_status, git_sync as perform_git_sync,
    git_undo_last_commit as perform_git_undo_last_commit, git_unstage as perform_git_unstage,
    GitBranchRequest, GitBranchesSnapshot, GitCloneRequest, GitCommitActionRequest,
    GitCommitRefRequest, GitCommitRequest, GitCreateBranchRequest, GitDeleteBranchRequest,
    GitDiffRequest, GitDiffSnapshot, GitPathsRequest, GitPushRemoteBranchRequest,
    GitPushRemoteRequest, GitRemoteRequest, GitRestoreCommitRequest, GitReviewDiffRequest,
    GitReviewSnapshot, GitStatusSnapshot, GitWatchManager, GitWatchRegistration,
};
use i18n::I18nBundle;
use llm::{
    LLMCompletionRequest, LLMCompletionResponse, LLMProviderTestResult, PetIdleSpeechRequest,
    PetIdleSpeechResponse,
};
use memory::{
    MemoryExtractionStatusSnapshot, MemoryManagementRequest, MemoryManagementSnapshot,
    MemoryManagerSnapshot, MemoryManagerSnapshotRequest, MemoryStore, MemorySummary,
    MemorySummaryUpdateRequest,
};
use notify_channels::{
    dispatch_notification_channels, NotificationDispatchRequest, NotificationDispatchResult,
};
use performance::{PerformanceMonitor, PerformanceSnapshot};
use pet::{
    PetCatalog, PetClaimRequest, PetCustomPet, PetCustomPetInstallPreview,
    PetCustomPetInstallRequest, PetRefreshRequest, PetRenameRequest, PetRestoreRequest,
    PetSnapshot, PetStore,
};
use power::PowerManager;
use project_store::{
    ProjectCloseRequest, ProjectCreateRequest, ProjectListSnapshot, ProjectSelectWorktreeRequest,
    ProjectStore, TerminalLayoutRecord,
};
use ssh::{SSHLaunchCommand, SSHProfileUpsertRequest, SSHProfilesSnapshot, SSHStore};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{thread, time::Duration};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID};
use tauri::utils::config::Color;
use tauri::Wry;
use tauri::{Emitter, Manager};
use tauri::{
    LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, Position, Size, WebviewUrl,
    WebviewWindowBuilder,
};
use terminal::{TerminalConfig, TerminalManager};
use worktree::{
    create_worktree, remove_worktree, worktree_snapshot as load_worktree_snapshot,
    WorktreeCreateRequest, WorktreeRemoveRequest, WorktreeSnapshot,
};

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
        Self {
            app_name: "Codux".to_string(),
            about: tr("menu.app.about_format", "About %@").replace("%@", "Codux"),
            app_menu_settings: tr("menu.app.settings", "Settings..."),
            check_updates: tr("menu.app.check_updates", "Check for Updates..."),
            services: tr("menu.app.services", "Services"),
            hide_app: tr("menu.app.hide_format", "Hide %@").replace("%@", "Codux"),
            hide_others: tr("menu.app.hide_others", "Hide Others"),
            show_all: tr("menu.app.show_all", "Show All"),
            quit: tr("menu.app.quit_format", "Quit %@").replace("%@", "Codux"),
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
            tasks_sidebar: tr("menu.view.tasks_sidebar", "Tasks Sidebar"),
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
    performance: Arc<PerformanceMonitor>,
    power: Arc<PowerManager>,
    ai_runtime: Arc<AIRuntimeBridge>,
    ai_history: Arc<AIHistoryIndexer>,
    memory: Arc<MemoryStore>,
    settings: Arc<AppSettingsStore>,
    projects: Arc<ProjectStore>,
    pet: Arc<PetStore>,
    ssh: Arc<SSHStore>,
    git_watch: Arc<GitWatchManager>,
    file_watch: Arc<FileWatchManager>,
    desktop_pet_hit_state: Arc<DesktopPetHitState>,
}

#[derive(Default)]
struct DesktopPetHitState {
    has_bubble: AtomicBool,
}

const DESKTOP_PET_LABEL: &str = "desktop-pet";
const DESKTOP_PET_BASE_WIDTH: f64 = 352.0;
const DESKTOP_PET_BASE_HEIGHT: f64 = 218.0;
const DESKTOP_PET_SPRITE_SIZE: f64 = 128.0;
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
const DESKTOP_PET_SCALE_UP: &str = "desktop-pet:scale-up";
const DESKTOP_PET_SCALE_DOWN: &str = "desktop-pet:scale-down";
const DESKTOP_PET_SCALE_RESET: &str = "desktop-pet:scale-reset";
const DESKTOP_PET_HIDE: &str = "desktop-pet:hide";

fn sync_desktop_pet_window(app: &tauri::AppHandle, settings: &AppSettings, pet: &PetSnapshot) {
    let should_show =
        settings.pet.enabled && settings.pet.desktop_widget && pet.claimed_at.is_some();
    if !should_show {
        if let Some(window) = app.get_webview_window(DESKTOP_PET_LABEL) {
            let _ = window.destroy();
        }
        return;
    }

    let app = app.clone();
    let settings = settings.clone();
    let _ = app.clone().run_on_main_thread(move || {
        let _ = show_desktop_pet_window(&app, &settings);
    });
}

fn show_desktop_pet_window(app: &tauri::AppHandle, settings: &AppSettings) -> tauri::Result<()> {
    let scale = desktop_pet_scale(settings);
    let width = (DESKTOP_PET_BASE_WIDTH * scale).round();
    let height = (DESKTOP_PET_BASE_HEIGHT * scale).round();

    if let Some(window) = app.get_webview_window(DESKTOP_PET_LABEL) {
        let previous_position = window.outer_position().ok();
        let previous_size = window.inner_size().ok();
        let _ = window.set_size(Size::Logical(LogicalSize::new(width, height)));
        let _ = window.set_min_size(Some(Size::Logical(LogicalSize::new(width, height))));
        let _ = window.set_max_size(Some(Size::Logical(LogicalSize::new(width, height))));
        let _ = window.set_always_on_top(true);
        let _ = window.set_focusable(false);
        let _ = window.set_ignore_cursor_events(true);
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
        WebviewUrl::App("index.html#/desktop-pet".into()),
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
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.show();
    start_desktop_pet_hit_test_loop(app.clone());
    desktop_pet_emit_placement(&window);
    Ok(())
}

fn start_desktop_pet_hit_test_loop(app: tauri::AppHandle) {
    thread::spawn(move || {
        let mut last_click_through = true;
        loop {
            let Some(window) = app.get_webview_window(DESKTOP_PET_LABEL) else {
                break;
            };
            let hit_state = app
                .try_state::<AppState>()
                .map(|state| Arc::clone(&state.desktop_pet_hit_state));
            let has_bubble = hit_state
                .as_ref()
                .is_some_and(|state| state.has_bubble.load(Ordering::Relaxed));
            let click_through = desktop_pet_should_click_through(&window, has_bubble);
            if click_through != last_click_through {
                let _ = window.set_ignore_cursor_events(click_through);
                last_click_through = click_through;
            }
            thread::sleep(Duration::from_millis(if click_through { 220 } else { 80 }));
        }
    });
}

fn desktop_pet_should_click_through(window: &tauri::WebviewWindow<Wry>, has_bubble: bool) -> bool {
    let Ok(cursor) = window.cursor_position() else {
        return true;
    };
    let Ok(position) = window.outer_position() else {
        return true;
    };
    let Ok(size) = window.inner_size() else {
        return true;
    };
    let scale_factor = window.scale_factor().unwrap_or(1.0).max(0.1);
    let local_x = (cursor.x - f64::from(position.x)) / scale_factor;
    let local_y = (cursor.y - f64::from(position.y)) / scale_factor;
    if local_x < 0.0
        || local_y < 0.0
        || local_x > f64::from(size.width) / scale_factor
        || local_y > f64::from(size.height) / scale_factor
    {
        return true;
    }
    !desktop_pet_local_point_is_hotspot(window, local_x, local_y, has_bubble)
}

fn desktop_pet_local_point_is_hotspot(
    window: &tauri::WebviewWindow<Wry>,
    x: f64,
    y: f64,
    has_bubble: bool,
) -> bool {
    let Ok(size) = window.inner_size() else {
        return false;
    };
    let scale_factor = window.scale_factor().unwrap_or(1.0).max(0.1);
    let width = f64::from(size.width) / scale_factor;
    let height = f64::from(size.height) / scale_factor;
    let side = desktop_pet_placement_for_window(window).side;
    let sprite_x = if side == "right" {
        24.0 + DESKTOP_PET_SPRITE_VISIBLE_INSET_X
    } else {
        width - 24.0 - DESKTOP_PET_SPRITE_SIZE + DESKTOP_PET_SPRITE_VISIBLE_INSET_X
    };
    let sprite_y = height - 8.0 - DESKTOP_PET_SPRITE_SIZE + DESKTOP_PET_SPRITE_VISIBLE_INSET_TOP;
    let sprite_width = DESKTOP_PET_SPRITE_SIZE - DESKTOP_PET_SPRITE_VISIBLE_INSET_X * 2.0;
    let sprite_height = DESKTOP_PET_SPRITE_SIZE
        - DESKTOP_PET_SPRITE_VISIBLE_INSET_TOP
        - DESKTOP_PET_SPRITE_VISIBLE_INSET_BOTTOM;
    let in_sprite = x >= sprite_x
        && x <= sprite_x + sprite_width
        && y >= sprite_y
        && y <= sprite_y + sprite_height;
    let in_bubble = if has_bubble {
        let bubble_x = if side == "right" {
            width - 8.0 - 214.0
        } else {
            8.0
        };
        let bubble_y = 56.0;
        x >= bubble_x && x <= bubble_x + 214.0 && y >= bubble_y && y <= bubble_y + 88.0
    } else {
        false
    };
    in_sprite || in_bubble
}

fn desktop_pet_scale(settings: &AppSettings) -> f64 {
    let parsed = settings
        .pet
        .desktop_scale
        .trim()
        .parse::<f64>()
        .unwrap_or(1.0);
    let stepped = (parsed / 0.1).round() * 0.1;
    stepped.clamp(0.75, 1.5)
}

fn desktop_pet_scale_setting(scale: f64) -> String {
    let normalized = (scale / 0.1).round().mul_add(0.1, 0.0).clamp(0.75, 1.5);
    if (normalized - 1.0).abs() < f64::EPSILON {
        "1".to_string()
    } else {
        format!("{normalized:.1}")
    }
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
) -> Result<String, String> {
    state
        .terminals
        .create(config, move |event| {
            let _ = app.emit("terminal:event", event);
        })
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
    state
        .terminals
        .snapshot(&session_id)
        .map_err(|error| error.to_string())
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
    rebuild_app_menu(&app, &next)?;
    if let Ok(pet) = state.pet.snapshot() {
        sync_desktop_pet_window(&app, &next, &pet);
    }
    let _ = app.emit("settings:updated", next.clone());
    Ok(next)
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
    let scale = desktop_pet_scale(&settings);
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
    let scale_up = MenuItem::with_id(
        &app,
        DESKTOP_PET_SCALE_UP,
        tr("pet.desktop.scale_up", "Make Larger"),
        scale < 1.5 - f64::EPSILON,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let scale_down = MenuItem::with_id(
        &app,
        DESKTOP_PET_SCALE_DOWN,
        tr("pet.desktop.scale_down", "Make Smaller"),
        scale > 0.75 + f64::EPSILON,
        None::<&str>,
    )
    .map_err(|error| error.to_string())?;
    let scale_reset = MenuItem::with_id(
        &app,
        DESKTOP_PET_SCALE_RESET,
        tr("pet.desktop.scale_reset", "Reset Size"),
        (scale - 1.0).abs() > f64::EPSILON,
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
    let separator_3 = PredefinedMenuItem::separator(&app).map_err(|error| error.to_string())?;
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
            &scale_up,
            &scale_down,
            &scale_reset,
            &separator_3,
            &hide,
        ],
    )
    .map_err(|error| error.to_string())?;
    window.popup_menu(&menu).map_err(|error| error.to_string())
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
fn memory_manager_snapshot(
    state: tauri::State<'_, AppState>,
    request: MemoryManagerSnapshotRequest,
) -> Result<MemoryManagerSnapshot, String> {
    state
        .memory
        .manager_snapshot(request, &state.projects.projects_snapshot())
        .map_err(|error| error.to_string())
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
fn memory_index_now(state: tauri::State<'_, AppState>) {
    Arc::clone(&state.memory).process_sessions_now(
        state.settings.snapshot().ai,
        state.projects.projects_snapshot(),
        state.ai_runtime.state_snapshot().sessions,
    );
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
fn git_status(project_path: String) -> GitStatusSnapshot {
    load_git_status(project_path)
}

#[tauri::command]
fn git_stage(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_stage(request)
}

#[tauri::command]
fn git_unstage(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_unstage(request)
}

#[tauri::command]
fn git_commit(request: GitCommitRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_commit(request)
}

#[tauri::command]
fn git_commit_action(request: GitCommitActionRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_commit_action(request)
}

#[tauri::command]
fn git_amend_last_commit_message(request: GitCommitRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_amend_last_commit_message(request)
}

#[tauri::command]
fn git_last_commit_message(project_path: String) -> Result<String, String> {
    load_git_last_commit_message(project_path)
}

#[tauri::command]
fn git_undo_last_commit(project_path: String) -> Result<GitStatusSnapshot, String> {
    perform_git_undo_last_commit(project_path)
}

#[tauri::command]
fn git_head_commit_pushed(project_path: String) -> Result<bool, String> {
    load_git_head_commit_pushed(project_path)
}

#[tauri::command]
fn git_pull(project_path: String) -> Result<GitStatusSnapshot, String> {
    perform_git_pull(project_path)
}

#[tauri::command]
fn git_push(project_path: String) -> Result<GitStatusSnapshot, String> {
    perform_git_push(project_path)
}

#[tauri::command]
fn git_fetch(project_path: String) -> Result<GitStatusSnapshot, String> {
    perform_git_fetch(project_path)
}

#[tauri::command]
fn git_sync(project_path: String) -> Result<GitStatusSnapshot, String> {
    perform_git_sync(project_path)
}

#[tauri::command]
fn git_review(project_path: String, base_branch: Option<String>) -> GitReviewSnapshot {
    load_git_review(project_path, base_branch)
}

#[tauri::command]
fn git_init(project_path: String) -> Result<GitStatusSnapshot, String> {
    perform_git_init(project_path)
}

#[tauri::command]
fn git_clone(request: GitCloneRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_clone(request)
}

#[tauri::command]
fn git_discard(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_discard(request)
}

#[tauri::command]
fn git_branches(project_path: String) -> GitBranchesSnapshot {
    load_git_branches(project_path)
}

#[tauri::command]
fn git_checkout_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_checkout_branch(request)
}

#[tauri::command]
fn git_checkout_remote_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_checkout_remote_branch(request)
}

#[tauri::command]
fn git_create_branch(request: GitCreateBranchRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_create_branch(request)
}

#[tauri::command]
fn git_merge_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_merge_branch(request)
}

#[tauri::command]
fn git_squash_merge_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_squash_merge_branch(request)
}

#[tauri::command]
fn git_delete_branch(request: GitDeleteBranchRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_delete_branch(request)
}

#[tauri::command]
fn git_force_push(project_path: String) -> Result<GitStatusSnapshot, String> {
    perform_git_force_push(project_path)
}

#[tauri::command]
fn git_push_remote(request: GitPushRemoteRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_push_remote(request)
}

#[tauri::command]
fn git_push_remote_branch(
    request: GitPushRemoteBranchRequest,
) -> Result<GitStatusSnapshot, String> {
    perform_git_push_remote_branch(request)
}

#[tauri::command]
fn git_checkout_commit(request: GitCommitRefRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_checkout_commit(request)
}

#[tauri::command]
fn git_revert_commit(request: GitCommitRefRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_revert_commit(request)
}

#[tauri::command]
fn git_restore_commit(request: GitRestoreCommitRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_restore_commit(request)
}

#[tauri::command]
fn git_add_remote(request: GitRemoteRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_add_remote(request)
}

#[tauri::command]
fn git_remove_remote(request: GitRemoteRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_remove_remote(request)
}

#[tauri::command]
fn git_append_gitignore(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    perform_git_append_gitignore(request)
}

#[tauri::command]
fn git_diff_file(request: GitDiffRequest) -> GitDiffSnapshot {
    load_git_diff_file(request)
}

#[tauri::command]
fn git_review_diff_file(request: GitReviewDiffRequest) -> GitDiffSnapshot {
    load_git_review_diff_file(request)
}

#[tauri::command]
fn git_watch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_path: String,
) -> Result<GitWatchRegistration, String> {
    state.git_watch.watch(app, project_path)
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
async fn worktree_snapshot(
    state: tauri::State<'_, AppState>,
    project_id: String,
    project_path: String,
) -> Result<WorktreeSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    tauri::async_runtime::spawn_blocking(move || {
        projects.merge_worktree_snapshot(load_worktree_snapshot(project_id, project_path))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn worktree_create(
    state: tauri::State<'_, AppState>,
    request: WorktreeCreateRequest,
) -> Result<WorktreeSnapshot, String> {
    let projects = Arc::clone(&state.projects);
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
    Ok(snapshot)
}

#[tauri::command]
async fn worktree_remove(
    state: tauri::State<'_, AppState>,
    request: WorktreeRemoveRequest,
) -> Result<WorktreeSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let snapshot = remove_worktree(request)?;
        projects.merge_worktree_snapshot(snapshot)
    })
    .await
    .map_err(|error| error.to_string())??;
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
    request: PetRefreshRequest,
) -> Result<PetSnapshot, String> {
    let pet = Arc::clone(&state.pet);
    let summary = state.ai_history.global_summary(request.projects).await?;
    let input = pet::refresh_input_from_summary(summary);
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
    let projects = request.projects.clone();
    let summary = state.ai_history.global_summary(projects).await?;
    let input = pet::claim_input_from_summary(request, summary);
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
async fn project_create(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ProjectCreateRequest,
) -> Result<ProjectListSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    let snapshot = tauri::async_runtime::spawn_blocking(move || projects.create_project(request))
        .await
        .map_err(|error| error.to_string())??;
    let _ = app.emit("project:updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
async fn project_close(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ProjectCloseRequest,
) -> Result<ProjectListSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    let pet = Arc::clone(&state.pet);
    let project_id = request.project_id.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let snapshot = projects.close_project(request.project_id)?;
        let _ = pet.forget_project_baseline(&project_id);
        Ok::<_, String>(snapshot)
    })
    .await
    .map_err(|error| error.to_string())??;
    let _ = app.emit("project:updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
async fn project_close_all(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ProjectListSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    let pet = Arc::clone(&state.pet);
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let snapshot = projects.close_all_projects()?;
        let _ = pet.forget_all_project_baselines();
        Ok::<_, String>(snapshot)
    })
    .await
    .map_err(|error| error.to_string())??;
    let _ = app.emit("project:updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
async fn project_select(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    project_id: String,
) -> Result<ProjectListSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        projects.select_project(project_id)?;
        Ok::<_, String>(projects.list_snapshot())
    })
    .await
    .map_err(|error| error.to_string())??;
    let _ = app.emit("project:updated", snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
async fn project_select_worktree(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ProjectSelectWorktreeRequest,
) -> Result<ProjectListSnapshot, String> {
    let projects = Arc::clone(&state.projects);
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        projects.select_worktree(request)?;
        Ok::<_, String>(projects.list_snapshot())
    })
    .await
    .map_err(|error| error.to_string())??;
    let _ = app.emit("project:updated", snapshot.clone());
    Ok(snapshot)
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
    let settings = state.settings.snapshot().remote;
    let enabled = settings.enabled && !settings.relay_url.trim().is_empty();
    RemoteStatus {
        enabled,
        relay: if settings.relay_url.trim().is_empty() {
            "local".to_string()
        } else {
            settings.relay_url
        },
        devices: 0,
        encryption: if enabled {
            "configured".to_string()
        } else {
            "disabled".to_string()
        },
    }
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

#[derive(serde::Serialize)]
struct RemoteStatus {
    enabled: bool,
    relay: String,
    devices: u32,
    encryption: String,
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
const CODUX_WEBSITE_URL: &str = "https://codux.dev";
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
            let tomorrow = chrono::Utc::now()
                .date_naive()
                .succ_opt()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|date| date.and_utc().timestamp())
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
        DESKTOP_PET_SCALE_UP => {
            let _ = update_desktop_pet_settings(state, app.clone(), |settings| {
                let scale = desktop_pet_scale(settings) + 0.1;
                settings.pet.desktop_scale = desktop_pet_scale_setting(scale);
            });
        }
        DESKTOP_PET_SCALE_DOWN => {
            let _ = update_desktop_pet_settings(state, app.clone(), |settings| {
                let scale = desktop_pet_scale(settings) - 0.1;
                settings.pet.desktop_scale = desktop_pet_scale_setting(scale);
            });
        }
        DESKTOP_PET_SCALE_RESET => {
            let _ = update_desktop_pet_settings(state, app.clone(), |settings| {
                settings.pet.desktop_scale = "1".to_string();
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
        tr_or_label(settings, "shortcut.task.create", "New Task"),
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
    let menu = build_app_menu(app, settings).map_err(|error| error.to_string())?;
    app.set_menu(menu).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn run() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let settings = Arc::new(AppSettingsStore::load_or_seed());
    sync_process_locale_preference(&settings.snapshot());
    let menu_settings = Arc::clone(&settings);
    let setup_settings = Arc::clone(&settings);
    let builder = tauri::Builder::default()
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

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let settings = Arc::clone(&setup_settings);
            let memory =
                Arc::new(MemoryStore::load_or_create().map_err(|error| error.to_string())?);
            let _ = memory.recover_interrupted_extractions();
            let projects = Arc::new(ProjectStore::load_or_seed());
            let ai_runtime = Arc::new(AIRuntimeBridge::new(
                Arc::clone(&settings),
                Arc::clone(&memory),
                Arc::clone(&projects),
            ));
            let power = Arc::new(PowerManager::default());
            power.start_settings_sync(Arc::clone(&settings))?;
            let state = AppState {
                terminals: Arc::new(TerminalManager::new(
                    Arc::clone(&ai_runtime),
                    Arc::clone(&settings),
                    Arc::clone(&memory),
                )),
                performance: Arc::new(PerformanceMonitor::default()),
                power,
                ai_runtime,
                ai_history: Arc::new(AIHistoryIndexer::new(app.handle().clone())),
                memory,
                settings,
                projects,
                pet: Arc::new(PetStore::load_or_seed()),
                ssh: Arc::new(SSHStore::load_or_seed()),
                git_watch: Arc::new(GitWatchManager::default()),
                file_watch: Arc::new(FileWatchManager::default()),
                desktop_pet_hit_state: Arc::new(DesktopPetHitState::default()),
            };
            state.ai_runtime.start_listener(app.handle().clone())?;
            let initial_settings = state.settings.snapshot();
            let initial_pet = state.pet.snapshot().ok();
            app.manage(state);
            if let Some(pet) = initial_pet {
                sync_desktop_pet_window(app.handle(), &initial_settings, &pet);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            project_list,
            project_create,
            project_close,
            project_close_all,
            project_select,
            project_select_worktree,
            project_open_applications,
            project_open_in_application,
            project_reveal_in_file_manager,
            terminal_layout_get,
            terminal_layout_save,
            remote_status,
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
            terminal_create,
            terminal_write,
            terminal_resize,
            terminal_interrupt,
            terminal_escape,
            terminal_kill,
            terminal_snapshot,
            ai_runtime_snapshot,
            ai_runtime_probe,
            ai_runtime_state_snapshot,
            ai_runtime_dismiss_completion,
            app_settings_get,
            app_settings_set,
            desktop_pet_placement,
            desktop_pet_set_bubble_visible,
            desktop_pet_start_drag,
            desktop_pet_show_context_menu,
            i18n_bundle_get,
            llm_complete,
            llm_provider_test,
            pet_idle_speech,
            memory_extraction_status,
            memory_management_snapshot,
            memory_manager_snapshot,
            memory_archive_entry,
            memory_delete_entry,
            memory_delete_summary,
            memory_update_summary,
            memory_index_now,
            app_request_restart,
            ai_history_project_summary,
            ai_history_project_state,
            ai_history_global_summary,
            git_status,
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
            git_review_diff_file,
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
            worktree_snapshot,
            worktree_create,
            worktree_remove,
            performance_snapshot,
            pet_catalog,
            pet_custom_install_preview,
            pet_custom_install,
            pet_refresh,
            pet_snapshot,
            pet_claim,
            pet_rename,
            pet_archive_current,
            pet_restore_archived,
            ssh_profiles,
            ssh_profile_upsert,
            ssh_profile_delete,
            ssh_launch_command,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Codux Tauri application");
}
