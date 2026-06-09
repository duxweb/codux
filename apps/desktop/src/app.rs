use crate::{
    heroicons::HeroIconName,
    terminal::{TerminalConfig, TerminalLaunchContext, TerminalPane, TerminalView},
    theme::{self, color},
};
use anyhow::Result;
use codux_runtime::{
    ai_history::{
        AIGlobalHistorySummary, AIHistorySummary, AISessionForkRequest, AISessionForkTarget,
        AISessionSummary,
    },
    ai_history_indexer::AIHistoryEvent,
    desktop_pet::{
        DESKTOP_PET_BASE_HEIGHT, DESKTOP_PET_BASE_WIDTH, DESKTOP_PET_HIDE, DESKTOP_PET_MUTE_1_HOUR,
        DESKTOP_PET_MUTE_30_MINUTES, DESKTOP_PET_MUTE_TODAY, DESKTOP_PET_SKIP_LINE,
        DESKTOP_PET_SPEAK_LESS, DESKTOP_PET_SPEAK_MORE, DesktopPetSavedOrigin, DesktopPetSide,
        DesktopPetWorkArea,
    },
    dialog::{
        LocalizedAlertDialogRequest, LocalizedConfirmDialogRequest, LocalizedOpenDialogRequest,
    },
    file_editor_layout::{FileEditorLayoutSummary, FileEditorTabSummary},
    files::FileChangeEvent,
    git::{
        GitBranchSummary, GitCommitSummary, GitCredentials, GitFileStatus, GitRemoteSummary,
        GitReviewContentSummary, GitReviewSummary, GitSummary,
    },
    i18n::translate,
    memory::{
        MemoryEntrySummary, MemoryExtractionStatusSnapshot, MemoryManagerSnapshot,
        MemoryProjectMigrationRequest, MemoryProjectProfileRefreshResult, MemorySummary,
        MemorySummaryUpdateRequest,
    },
    notification::{NotificationChannelConfig, NotificationDispatchRequest},
    performance::{PerformanceService, PerformanceSummary},
    pet::{
        PetCatalog, PetClaimRequest, PetCustomPet, PetCustomPetInstallPreview,
        PetCustomPetInstallRequest, PetRenameRequest, PetRestoreRequest, PetSnapshot, PetSummary,
    },
    project_activity::ProjectActivityEvent,
    project_open::ProjectOpenApplicationSummary,
    project_store::{
        ProjectCreateRequest, ProjectDefaultPushRemoteRequest, ProjectReorderRequest,
        ProjectUpdateRequest,
    },
    remote::{RemoteDeviceSummary, RemotePairingInfo, RemotePairingPollResult, RemoteSummary},
    runtime_activity::RuntimeActivitySummary,
    runtime_bridge::RuntimeInventory,
    runtime_state::{FileEntry, FileKind, ProjectInfo, RuntimeService, RuntimeState},
    settings::{SettingsSummary, locale_from_language_setting},
    ssh::{SSHConnectionProfile, SSHProfileSummary, SSHProfileUpsertRequest, SSHSummary},
    terminal_layout::{TerminalLayoutSummary, TerminalPaneSummary, TerminalTabSummary},
    terminal_pty::{TerminalManager, TerminalPtyConfig},
    terminal_runtime::{
        TerminalInputSummary, TerminalRuntimeSessionInput, TerminalRuntimeSessionSummary,
        TerminalRuntimeSummary,
    },
    worktree::{
        ProjectWorktreeGitSummary, WorktreeInfo, WorktreeSnapshot, WorktreeSummary,
        WorktreeTaskInfo,
    },
};
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext, Bounds, ClipboardItem, Context, ElementId,
    FocusHandle, FontWeight, ImageSource, InteractiveElement, IntoElement, KeyDownEvent, Length,
    MouseButton, ObjectFit, ParentElement, PathBuilder, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, Subscription, UniformListScrollHandle, Window,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowKind,
    WindowOptions, anchored, canvas, deferred, div, img, linear_color_stop, linear_gradient, point,
    prelude::FluentBuilder as _, px, relative, rems, size,
};
use gpui_component::{
    ActiveTheme, Disableable, ElementExt, Icon, Root, Sizable, Size, VirtualListScrollHandle,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem},
    resizable::{resizable_panel, v_resizable},
    spinner::Spinner,
    tag::Tag,
    v_virtual_list,
};
use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod about;
mod ai_history_mapping;
mod ai_memory_actions;
mod ai_runtime_status;
mod app_events;
mod app_helpers;
mod app_lifecycle;
mod app_render;
mod app_select;
mod app_state;
mod desktop_pet;
mod file_actions;
mod file_editor;
mod formatting;
mod git_actions;
pub(crate) mod macos_window;
pub(crate) mod native_menu;
mod pet;
mod pet_actions;
mod project_actions;
mod project_column;
mod project_column_actions;
mod project_editor;
mod runtime_actions;
mod scroll_compat;
mod settings;
mod settings_actions;
mod shell_utils;
mod shortcuts;
mod sidebars;
mod ssh_profile_editor;
mod ssh_remote_actions;
mod status_bar;
mod task_column;
mod terminal_actions;
mod terminal_float;
mod terminal_state;
mod terminal_worktree_actions;
#[cfg(test)]
mod tests;
mod types;
mod ui_helpers;
mod ui_invalidation;
mod window_actions;
mod window_shell;
mod work_scheduler;
mod workspace;
mod workspace_daily_level;
mod workspace_files;
mod workspace_pet_widgets;
mod workspace_review;
mod workspace_shared;
mod workspace_terminal;
mod workspace_toolbar;
mod workspace_views;
mod worktree_creator;

pub use self::app_state::CoduxApp;
pub(crate) use self::app_state::{active_settings_snapshot, set_active_settings_snapshot};

pub(crate) const MAIN_WINDOW_DEFAULT_WIDTH: f32 = 1280.0;
pub(crate) const MAIN_WINDOW_DEFAULT_HEIGHT: f32 = 820.0;
pub(crate) const MAIN_WINDOW_MIN_WIDTH: f32 = 1240.0;
pub(crate) const MAIN_WINDOW_MIN_HEIGHT: f32 = 640.0;
pub(crate) const PROJECT_COLUMN_COLLAPSED_WIDTH: f32 = 80.0;
pub(crate) const PROJECT_COLUMN_EXPANDED_WIDTH: f32 = 232.0;
pub(crate) const TASK_COLUMN_FIXED_WIDTH: f32 = 240.0;
pub(crate) const ASSISTANT_PANEL_WIDTH: f32 = 320.0;

use self::{
    ai_history_mapping::{
        AI_SESSION_FORK_TARGETS, ai_history_project_requests, ai_history_should_replace,
        ai_history_summary_from_project_state, ai_history_summary_from_state_or_status,
        ai_history_worktree_request, ai_session_fork_command, ai_session_restore_command,
        normalized_ai_history_snapshot_to_summary,
        normalized_global_ai_history_snapshot_to_summary,
    },
    app_events::{
        ChildWindowUpdateKind, PetCustomInstallEvent, current_child_window_update_event,
        current_pet_custom_install_event, current_pet_update_event, current_settings_update_event,
        publish_child_window_update, publish_pet_custom_install, publish_pet_update,
        publish_settings_update, publish_ssh_update, publish_statistics_settings_update,
    },
    app_helpers::{
        PROJECT_BADGE_COLORS, defer_codux_app_update, generated_git_branch_name,
        generated_git_commit_message, generated_project_child_name, git_remote_action_label,
        join_relative_child_path, normalized_git_action_paths, plural,
        project_badge_text_from_name, reordered_ids,
    },
    app_state::{
        AIProviderTestResult, GIT_CREDENTIALS_COMPACT_HEIGHT, GIT_CREDENTIALS_WINDOW_WIDTH,
        GitOperationCompletion, PET_CUSTOM_INSTALL_ERROR_HEIGHT, PET_CUSTOM_INSTALL_INPUT_HEIGHT,
        PET_CUSTOM_INSTALL_READY_HEIGHT, PET_CUSTOM_INSTALL_WINDOW_WIDTH, PET_DEX_FRAME_INTERVAL,
        PendingTerminalClose, ProjectSwitchLoad, ProjectSwitchPrimaryLoad, ProjectSwitchTaskLoad,
        ProjectSwitchTerminalLoad, RuntimeActivityTickResult, RuntimeScheduledRefresh,
        SSHProfileTestDisplay, TerminalCloseTarget, UpdateDialogPhase, WorktreeScopeKey,
        WorktreeSidebarLoad, WorktreeSwitchLoad, app_git_review, app_now_seconds,
        current_worktree_scope_key, git_status_tree_key, resize_git_credentials_window,
        resize_pet_custom_install_window, resize_pet_custom_install_window_handle,
        settings_with_active_restart_locked_values, worktree_summary_has_git_counts,
        worktree_summary_has_rows,
    },
    desktop_pet::*,
    formatting::compact_number,
    project_column::{ProjectColumnView, ProjectListState},
    scroll_compat::{ScrollableElement, codux_uniform_list},
    settings::SettingsPane,
    shell_utils::{shell_quote, terminal_command_text},
    shortcuts::{shortcut_display_from_keystroke, shortcut_matches},
    sidebars::{
        AssistantPanel, ClipboardFilePayload, FileSidebarView, GitSidebarLabels,
        clipboard_file_payload, current_directory_suffix, file_directory_option,
        git_clone_window_workspace, git_credentials_window_workspace, git_diff_window_workspace,
        git_review_file_list, git_review_workspace, memory_manager_window_workspace,
        parent_relative_directory,
    },
    ssh_profile_editor::ssh_profile_editor_workspace,
    status_bar::StatusBarView,
    task_column::{
        TaskColumnHeaderView, TaskColumnView, TaskSessionListView, TaskWorktreeListView,
    },
    terminal_float::terminal_float_window,
    terminal_state::{
        bottom_terminal_id, clamp_terminal_bottom_ratio, normalize_terminal_restore_state,
        prepare_memory_launch_artifacts, refresh_terminal_pane_config,
        restore_terminal_tabs_skeleton, spawn_terminal_tabs, terminal_config_for_settings,
        terminal_launch_context, terminal_pane_summary, terminal_pane_terminal_id,
        terminal_pty_config_for_terminal_id, terminal_restore_plan_for_language,
        terminal_tab_summary, terminal_top_ratios_for_panes, top_terminal_id,
    },
    types::*,
    ui_helpers::{
        assistant_header_icon_button, column_header, dialog_button_label, dialog_cancel_button,
        dialog_footer_bar, dialog_primary_button, dialog_secondary_button, empty_label,
        header_icon_button, header_icon_button_loading,
    },
    ui_invalidation::UiRegion,
    window_shell::child_window_shell,
    work_scheduler::ScheduledWorkPolicy,
    workspace_views::WorkspaceColumnView,
};
