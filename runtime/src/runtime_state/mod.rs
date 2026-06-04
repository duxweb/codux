use crate::{
    ai_history::{AIGlobalHistorySummary, AIHistoryService, AIHistorySummary, AISessionDetail},
    ai_history_indexer::{AIHistoryEvent, AIHistoryIndexer, AIHistoryProjectState},
    ai_history_normalized::{
        AIGlobalHistorySnapshot, AIHistoryProjectRequest, global_today_normalized_tokens,
        indexed_sessions_since, normalized_project_totals_since,
    },
    ai_runtime::{
        AIRuntimeBridge, AIRuntimeBridgeSnapshot, AIRuntimeContextSnapshot, AIRuntimeProbeRequest,
        AIRuntimeStateSnapshot, AIRuntimeSupervisorEvent,
    },
    ai_runtime_state::{AIRuntimeStateService, AIRuntimeStateSummary},
    app_icon,
    app_info::{
        AppAboutMetadata, AppDiagnosticsSnapshot, DiagnosticsExportRequest,
        DiagnosticsExportResult, UpdateInstallResult,
    },
    dialog::{
        LocalizedAlertDialogRequest, LocalizedConfirmDialogRequest, LocalizedOpenDialogRequest,
        LocalizedSaveDialogRequest,
    },
    desktop_pet::{
        DesktopPetHitLayout, DesktopPetPhysicalPosition, DesktopPetPhysicalSize,
        DesktopPetPlacementSnapshot, DesktopPetSavedOrigin, DesktopPetService,
        DesktopPetVisibilitySnapshot, DesktopPetWorkArea,
    },
    files::{
        FileChangeEvent, FileExternalCopyRequest, FileWatchManager, FileWatchRegistration,
        FilesService,
    },
    file_editor_layout::{FileEditorLayoutService, FileEditorLayoutSummary, FileEditorTabSummary},
    file_tree_state::{FileTreeStateService, FileTreeStateSummary},
    git,
    git_ui_state::{GitUiStateService, GitUiStateSummary},
    i18n::{self, I18nBundle},
    llm::{
        self, LLMCompletionRequest, LLMCompletionResponse, LLMProviderTestResult,
        PetIdleSpeechRequest, PetIdleSpeechResponse,
    },
    memory::{
        MemoryEnqueueResult, MemoryExtractionStatusSnapshot, MemoryManagementRequest,
        MemoryManagementSnapshot, MemoryManagerSnapshot, MemoryManagerSnapshotRequest,
        MemoryManualEnqueueResult, MemoryProjectMigrationRequest, MemoryProjectProfile,
        MemoryProjectProfileRefreshResult, MemoryService, MemorySummary, MemorySummaryRow,
        MemorySummaryUpdateRequest,
    },
    notification::{
        NotificationDispatchRequest, NotificationDispatchResult, NotificationService,
        NotificationSummary,
    },
    performance::{PerformanceService, PerformanceSummary},
    pet::{
        PetCatalog, PetClaimInput, PetCustomPet, PetCustomPetInstallPreview,
        PetCustomPetInstallRequest, PetProjectTokenTotal, PetRefreshInput, PetRenameRequest,
        PetRestoreRequest, PetService, PetSnapshot, PetStore, PetSummary,
        refresh_input_from_indexed_history,
    },
    power::{PowerManager, PowerService, PowerSummary},
    project_activity::{ProjectActivityCoordinator, ProjectActivityEvent, ProjectActivitySnapshot},
    project_store::{
        ProjectCloseRequest, ProjectCreateRequest, ProjectDefaultPushRemoteRequest,
        ProjectListSnapshot, ProjectMoveDirection, ProjectReorderRequest,
        ProjectSelectWorktreeRequest, ProjectStore, ProjectUpdateRequest, TerminalLayoutRecord,
        TerminalLayoutsSnapshot,
    },
    remote::{
        RemoteHostRuntime, RemotePairingInfo, RemotePairingPollResult, RemoteService,
        RemoteSummary,
    },
    runtime_activity::{RuntimeActivityService, RuntimeActivitySummary},
    runtime_bridge::RuntimeInventory,
    runtime_event::{RuntimeEventService, RuntimeEventSummary},
    runtime_paths,
    settings::{
        AppSettings, AppSettingsStore, SettingsService, SettingsSummary,
        sync_process_locale_preference,
    },
    ssh::{
        SSHLaunchCommand, SSHProfileTestResult, SSHProfileUpsertRequest, SSHProfilesSnapshot,
        SSHService, SSHStore, SSHSummary, render_ssh_launch_context_from_support_dir,
    },
    terminal_layout::{TerminalLayoutService, TerminalLayoutSummary},
    terminal_pty::TerminalManager,
    terminal_runtime::TerminalRuntimeSummary,
    tool_permissions::{ToolPermissionsService, ToolPermissionsSummary},
    update::{UpdateService, UpdateStatus, UpdateSummary},
    worktree::{
        WorktreeCreateRequest, WorktreeMergeRequest, WorktreeRemoveRequest, WorktreeService,
        WorktreeSnapshot, WorktreeSummary,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

include!("types.rs");
include!("service_core.rs");
include!("service_git_files.rs");
include!("service_ai_memory.rs");
include!("service_ssh_worktree.rs");
include!("service_system.rs");
include!("service_projects_settings.rs");
include!("state.rs");
include!("loaders.rs");
