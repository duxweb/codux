use crate::terminal::TerminalPane;
use codux_runtime::project_store::ProjectRuntimeTarget;

/// What the file-picker sub-window selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum FilePickerMode {
    /// Pick a directory (only folders are choosable; files are hidden).
    OpenFolder,
    /// Pick an existing file (folders navigate, files are choosable). Built into
    /// the picker and ready; wire a caller (e.g. an "Open file…" action) to use it.
    #[allow(dead_code)]
    OpenFile,
    /// Choose a directory + type a filename (Save As). Built and ready; wire a
    /// caller (e.g. a "Save as…" action) to use it.
    #[allow(dead_code)]
    Save,
}

/// Where the picker's chosen path is delivered when confirmed. Extensible: add a
/// variant per call site, handled in `apply_file_picker_result`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) enum FilePickerTarget {
    /// The project-editor window's directory field (add/edit a project).
    ProjectEditorPath,
    /// "Save as…" from the file sidebar: copy `source_path` to the chosen
    /// destination on `device_id` (the project's host, or local).
    SaveFileAs {
        source_path: String,
        runtime_target: ProjectRuntimeTarget,
    },
    /// SSH profile editor: choose the private key file path.
    SshPrivateKeyPath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum AppWindowMode {
    Main,
    About,
    UpdateDialog,
    GitClone,
    GitCredentials,
    GitDiff,
    FileEditor,
    FilePreview,
    MemoryManager,
    PetClaim,
    PetCustomInstall,
    PetDex,
    Settings,
    ProjectEditor,
    WorktreeCreator,
    SshProfileEditor,
    DbProfileEditor,
    FilePicker,
    DesktopPet,
}

pub(in crate::app) struct TerminalTab {
    pub(in crate::app) id: usize,
    pub(in crate::app) label: String,
    pub(in crate::app) terminal_id: Option<String>,
    pub(in crate::app) panes: Vec<TerminalPaneSlot>,
}

pub(in crate::app) struct TerminalPaneSlot {
    pub(in crate::app) title: String,
    pub(in crate::app) terminal_id: Option<String>,
    pub(in crate::app) pane: Option<TerminalPane>,
    pub(in crate::app) restored_output_bytes: usize,
    pub(in crate::app) restored_output_tail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct TerminalTabPlan {
    pub(in crate::app) terminal_id: Option<String>,
    pub(in crate::app) label: String,
    pub(in crate::app) panes: Vec<TerminalPanePlan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct TerminalPanePlan {
    pub(in crate::app) terminal_id: Option<String>,
    pub(in crate::app) title: String,
    pub(in crate::app) restored_output_bytes: usize,
    pub(in crate::app) restored_output_tail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct TerminalRestorePlan {
    pub(in crate::app) tabs: Vec<TerminalTabPlan>,
    pub(in crate::app) active_index: usize,
    pub(in crate::app) active_terminal_id: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum WorkspaceView {
    Terminal,
    Files,
    Review,
    Stats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::app) enum StatsTimeRange {
    Today,
    SevenDays,
    ThirtyDays,
    All,
}

/// Secondary panel shown alongside the terminal workspace when a file is opened
/// in split mode. The body composes the existing full-body workspace views as
/// side-by-side typed panels, so adding a new panel kind (e.g. an in-split chat
/// view) only means another variant here plus its render arm — the terminal
/// pane internals stay untouched.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum WorkspaceSplitKind {
    FileEditor,
    // Chat,  // future: chat session panel hosted in the body split
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum FileNameDraftKind {
    CreateFile,
    CreateDirectory,
    Rename,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum MemoryManagerTab {
    Active,
    Failed,
    History,
    Queue,
    Summary,
}

impl MemoryManagerTab {
    pub(in crate::app) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Failed => "failed",
            Self::History => "history",
            Self::Queue => "queue",
            Self::Summary => "summary",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct GitRunningOperation {
    pub(in crate::app) label: String,
    pub(in crate::app) cancellable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) enum PetDexSpotlight {
    Bundled(String),
    Custom(String),
    ArchiveConfirm,
}
