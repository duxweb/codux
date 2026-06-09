use crate::terminal::TerminalPane;

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
    TerminalTabEditor,
    WorktreeCreator,
    SshProfileEditor,
    DesktopPet,
}

pub(in crate::app) struct TerminalTab {
    pub(in crate::app) id: usize,
    pub(in crate::app) label: String,
    pub(in crate::app) placement: TerminalTabPlacement,
    pub(in crate::app) terminal_id: Option<String>,
    pub(in crate::app) panes: Vec<TerminalPaneSlot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum TerminalTabPlacement {
    Top,
    Bottom,
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
    pub(in crate::app) placement: TerminalTabPlacement,
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
    pub(in crate::app) active_bottom_terminal_id: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum WorkspaceView {
    Terminal,
    Files,
    Review,
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
