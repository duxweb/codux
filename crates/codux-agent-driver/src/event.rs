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
    /// The current/most-recent turn's usage (the protocol's `last` breakdown).
    pub last_total_tokens: u64,
    pub last_input_tokens: u64,
    pub last_output_tokens: u64,
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

/// One step of the turn plan (`turn/plan/updated`).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub step: String,
    /// `pending` | `inProgress` | `completed`.
    pub status: String,
}

/// A selectable option of a `item/tool/requestUserInput` question.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputOption {
    pub label: String,
    pub description: String,
}

/// One question codex asks the user mid-turn (`item/tool/requestUserInput`).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<UserInputOption>,
    /// Offer a free-text "other" answer besides the options.
    pub is_other: bool,
    /// The answer is a secret (render a masked input).
    pub is_secret: bool,
}

/// A server→client question round-trip; answered with
/// [`crate::codex::CodexSession::respond_user_input`] using `token`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputRequest {
    pub token: String,
    pub questions: Vec<UserInputQuestion>,
}

/// A permission-escalation ask (`item/permissions/requestApproval`); answered
/// with [`crate::codex::CodexSession::respond_permissions`] using `token`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub token: String,
    pub reason: Option<String>,
    pub cwd: String,
    /// The requested permission profile, untouched (echoed back on grant).
    pub requested: Value,
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
    PlanDelta { id: String, text: String },
    /// Live replacement of a fileChange item's `changes` (patchUpdated).
    FileChangesUpdated { id: String, changes: Value },
    ItemCompleted(TimelineItem),
    TokenUsage(TokenUsage),
    ApprovalRequest(ApprovalRequest),
    UserInputRequest(UserInputRequest),
    PermissionRequest(PermissionRequest),
    /// The turn's todo plan (replaces the previous plan wholesale).
    TurnPlan { explanation: Option<String>, steps: Vec<PlanStep> },
    /// The CLI named/renamed the thread.
    ThreadNameUpdated(String),
    /// User-relevant advisory (compaction, model reroute, warnings): `kind` is
    /// the protocol method, `message` its human text when the wire carries one.
    Notice { kind: String, message: String },
    Status(String),
    TurnCompleted {
        /// Server-reported turn duration, when the wire carries one.
        duration_ms: Option<u64>,
    },
    Error(String),
}
