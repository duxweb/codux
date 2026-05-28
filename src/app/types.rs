use crate::terminal::{TerminalLaunchContext, TerminalPane};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum AppWindowMode {
    Main,
    About,
    Settings,
    ProjectEditor,
    DesktopPet,
}

pub(in crate::app) struct TerminalTab {
    pub(in crate::app) id: usize,
    pub(in crate::app) label: String,
    pub(in crate::app) source_id: Option<String>,
    pub(in crate::app) terminal_id: Option<String>,
    pub(in crate::app) panes: Vec<TerminalPaneSlot>,
}

pub(in crate::app) struct TerminalPaneSlot {
    pub(in crate::app) title: String,
    pub(in crate::app) launch_context: Option<TerminalLaunchContext>,
    pub(in crate::app) pane: TerminalPane,
    pub(in crate::app) restored_output_bytes: usize,
    pub(in crate::app) restored_output_tail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct TerminalTabPlan {
    pub(in crate::app) source_id: Option<String>,
    pub(in crate::app) terminal_id: Option<String>,
    pub(in crate::app) label: String,
    pub(in crate::app) panes: Vec<TerminalPanePlan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct TerminalPanePlan {
    pub(in crate::app) source_id: Option<String>,
    pub(in crate::app) terminal_id: Option<String>,
    pub(in crate::app) title: String,
    pub(in crate::app) restored_output_bytes: usize,
    pub(in crate::app) restored_output_tail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct TerminalRestorePlan {
    pub(in crate::app) tabs: Vec<TerminalTabPlan>,
    pub(in crate::app) active_index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum WorkspaceView {
    Terminal,
    Files,
    Review,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum MemoryManagerTab {
    Active,
    History,
    Summary,
}

impl MemoryManagerTab {
    pub(in crate::app) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::History => "history",
            Self::Summary => "summary",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct GitRunningOperation {
    pub(in crate::app) label: String,
    pub(in crate::app) cancellable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum AIToolLauncher {
    Codex,
    Claude,
    Gemini,
    OpenCode,
    Kiro,
}
