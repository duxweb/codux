//! The driver-agnostic event stream a session emits to its consumer (UI / remote
//! bridge), and the small value types those events carry.

use serde::Serialize;
use serde_json::Value;

use crate::timeline::TimelineItem;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub model_context_window: Option<u64>,
}

/// A server→client approval round-trip surfaced to the UI. The UI answers with
/// [`crate::codex::CodexSession::respond_approval`] using `token`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    /// Opaque token used to answer this request (the JSON-RPC id, stringified).
    pub token: String,
    /// Protocol method, e.g. `item/commandExecution/requestApproval`.
    pub method: String,
    /// One-line summary for the prompt (command line, file path, …).
    pub summary: String,
    /// Original protocol params, untouched, for faithful rendering.
    pub raw: Value,
}

#[derive(Clone, Copy, Debug)]
pub enum ApprovalDecision {
    Accept,
    AcceptForSession,
    Decline,
    Cancel,
}

impl ApprovalDecision {
    /// The wire value codex expects in `{ "decision": ... }`.
    pub fn wire(self) -> &'static str {
        match self {
            ApprovalDecision::Accept => "accept",
            ApprovalDecision::AcceptForSession => "acceptForSession",
            ApprovalDecision::Decline => "decline",
            ApprovalDecision::Cancel => "cancel",
        }
    }
}

/// Everything a consumer needs to render a live session. The session applies the
/// item/delta variants to its own [`crate::timeline::Timeline`] *before* emitting,
/// so a consumer can either react to events or just re-read the merged timeline.
#[derive(Clone, Debug)]
pub enum AgentEvent {
    ThreadStarted { thread_id: String },
    TurnStarted,
    ItemStarted(TimelineItem),
    MessageDelta { id: String, text: String },
    ReasoningDelta { id: String, text: String },
    CommandOutputDelta { id: String, text: String },
    ItemCompleted(TimelineItem),
    TokenUsage(TokenUsage),
    ApprovalRequest(ApprovalRequest),
    Status(String),
    TurnCompleted,
    Error(String),
}
