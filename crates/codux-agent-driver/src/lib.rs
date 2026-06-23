//! Structured "agent driver" layer for protocol-driven AI CLIs.
//!
//! Unlike the PTY path (run the CLI in a terminal and observe it via injected
//! hooks), a driver speaks the CLI's own protocol and emits a normalized
//! [`AgentEvent`] stream over a merged [`timeline::Timeline`]. Codex is the first
//! driver (`codex app-server` JSON-RPC over stdio); Claude stream-json and
//! OpenCode ACP slot in behind the same [`AgentDriver`] trait.

pub mod codex;
pub mod event;
pub mod jsonrpc;
pub mod session;
pub mod timeline;

pub use codex::{
    AgentModel, AgentPermissionProfile, AgentSkill, CodexAgentDriver, CodexSession, FileHit,
    UserInputPart,
};
pub use event::{AgentEvent, ApprovalDecision, ApprovalRequest, TokenUsage};
pub use session::{AgentKind, AgentSession, AgentSink, SessionCapabilities, start_session};
pub use timeline::{ItemStatus, Timeline, TimelineItem, TimelineKind};

use serde::Serialize;

/// Which wire protocol a driver speaks. The axis that distinguishes drivers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentTransport {
    /// `codex app-server` JSON-RPC over stdio.
    CodexAppServer,
    /// `claude --output-format=stream-json` line protocol.
    ClaudeStreamJson,
    /// `opencode acp` (Agent Client Protocol), JSON-RPC over stdio.
    OpenCodeAcp,
}

/// How to launch a driver's backing process.
#[derive(Clone, Debug)]
pub struct AgentInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Per-session settings shared across drivers.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SessionConfig {
    pub cwd: String,
    pub model: Option<String>,
    /// codex `AskForApproval` (`untrusted` | `on-failure` | `on-request` | `never`).
    pub approval_policy: String,
    /// codex `SandboxMode` (`read-only` | `workspace-write` | `danger-full-access`).
    pub sandbox: String,
}

impl SessionConfig {
    pub fn read_only(cwd: impl Into<String>) -> Self {
        Self {
            cwd: cwd.into(),
            model: None,
            approval_policy: "on-request".into(),
            sandbox: "read-only".into(),
        }
    }

    /// Codex's default interactive posture: edits within the workspace are
    /// allowed, anything riskier asks for approval.
    pub fn workspace_write(cwd: impl Into<String>) -> Self {
        Self {
            cwd: cwd.into(),
            model: None,
            approval_policy: "on-request".into(),
            sandbox: "workspace-write".into(),
        }
    }
}

/// A registered AI-CLI driver: how to launch it and what protocol it speaks.
pub trait AgentDriver: Send + Sync {
    fn id(&self) -> &str;
    fn transport(&self) -> AgentTransport;
    fn invocation(&self, cfg: &SessionConfig) -> AgentInvocation;
}

/// Registry of available drivers (the "factory"). Codex only for now.
pub struct AgentDriverFactory {
    drivers: Vec<Box<dyn AgentDriver>>,
}

impl AgentDriverFactory {
    pub fn with_defaults() -> Self {
        Self {
            drivers: vec![Box::new(CodexAgentDriver::default())],
        }
    }

    pub fn register(&mut self, driver: Box<dyn AgentDriver>) {
        self.drivers.push(driver);
    }

    pub fn get(&self, id: &str) -> Option<&dyn AgentDriver> {
        self.drivers
            .iter()
            .find(|d| d.id() == id)
            .map(AsRef::as_ref)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.drivers.iter().map(|d| d.id())
    }
}
