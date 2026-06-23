//! Driver-agnostic session layer.
//!
//! The UI talks to an [`AgentSession`] trait object, never a concrete driver, so
//! Codex / Claude / OpenCode plug in behind the same surface. Each driver
//! advertises a [`SessionCapabilities`] so the composer can show or hide
//! affordances it doesn't support (model list, skills, file search, rollback…).
//! The normalized [`AgentEvent`] stream + merged [`TimelineItem`] timeline are
//! the shared contract every driver emits into.

use std::sync::Arc;

use crate::codex::{
    AgentModel, AgentPermissionProfile, AgentSkill, CodexAgentDriver, CodexSession, FileHit,
    UserInputPart,
};
use crate::event::{AgentEvent, ApprovalDecision};
use crate::timeline::TimelineItem;
use crate::SessionConfig;

/// The callback a session pushes normalized events into (off the UI thread).
pub type AgentSink = Box<dyn Fn(&AgentEvent) + Send + Sync>;

/// What a driver supports, so the UI only offers features that exist.
#[derive(Clone, Copy, Debug)]
pub struct SessionCapabilities {
    pub models: bool,
    pub efforts: bool,
    pub skills: bool,
    pub permission_profiles: bool,
    pub file_search: bool,
    pub rollback: bool,
    pub review: bool,
    pub compact: bool,
}

impl SessionCapabilities {
    /// Nothing supported — a sensible base for a minimal driver.
    pub const NONE: Self = Self {
        models: false,
        efforts: false,
        skills: false,
        permission_profiles: false,
        file_search: false,
        rollback: false,
        review: false,
        compact: false,
    };
}

/// A live AI-CLI conversation, independent of which CLI backs it. Cheap to clone
/// behind `Arc`. Methods that a driver can't do return `Ok`/empty by default.
pub trait AgentSession: Send + Sync {
    fn capabilities(&self) -> SessionCapabilities;

    /// Send a turn built from mixed input parts (text / skill / mention / image).
    fn send_user_turn(&self, parts: Vec<UserInputPart>) -> Result<(), String>;

    /// Convenience: a plain-text turn.
    fn send_user_message(&self, text: &str) -> Result<(), String> {
        self.send_user_turn(vec![UserInputPart::Text(text.to_string())])
    }

    fn set_model(&self, _model: Option<String>) {}
    fn set_effort(&self, _effort: Option<String>) {}
    fn compact(&self) -> Result<(), String> {
        Ok(())
    }
    fn interrupt(&self) -> Result<(), String> {
        Ok(())
    }
    fn rollback(&self, _num_turns: u32) -> Result<(), String> {
        Ok(())
    }
    fn turns_from(&self, _item_id: &str) -> u32 {
        0
    }
    fn truncate_timeline_before(&self, _item_id: &str) {}
    fn review_uncommitted(&self) -> Result<(), String> {
        Ok(())
    }
    fn respond_approval(&self, _token: &str, _decision: ApprovalDecision) -> Result<(), String> {
        Ok(())
    }
    fn list_models(&self) -> Result<Vec<AgentModel>, String> {
        Ok(Vec::new())
    }
    fn list_skills(&self, _cwd: &str) -> Result<Vec<AgentSkill>, String> {
        Ok(Vec::new())
    }
    fn list_permission_profiles(&self, _cwd: &str) -> Result<Vec<AgentPermissionProfile>, String> {
        Ok(Vec::new())
    }
    fn search_files(&self, _query: &str, _roots: Vec<String>) -> Result<Vec<FileHit>, String> {
        Ok(Vec::new())
    }

    fn timeline_snapshot(&self) -> Vec<TimelineItem>;
    fn shutdown(&self);
}

/// Which CLI to drive. The single axis the factory branches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentKind {
    Codex,
    Claude,
    OpenCode,
}

impl AgentKind {
    pub fn id(self) -> &'static str {
        match self {
            AgentKind::Codex => "codex",
            AgentKind::Claude => "claude",
            AgentKind::OpenCode => "opencode",
        }
    }
}

/// Start a session for the given driver. `program`/`env` locate and launch the
/// backing process. Returns an `Arc<dyn AgentSession>` the UI drives uniformly.
pub fn start_session(
    kind: AgentKind,
    program: String,
    env: Vec<(String, String)>,
    cfg: &SessionConfig,
    sink: AgentSink,
) -> Result<Arc<dyn AgentSession>, String> {
    match kind {
        AgentKind::Codex => {
            let driver = CodexAgentDriver { program, env };
            let session = CodexSession::start(&driver, cfg, sink)?;
            Ok(Arc::new(session))
        }
        AgentKind::Claude => Err("Claude 驱动尚未实现".into()),
        AgentKind::OpenCode => Err("OpenCode 驱动尚未实现".into()),
    }
}
