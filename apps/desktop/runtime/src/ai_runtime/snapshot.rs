use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AISessionSnapshot {
    pub terminal_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_instance_id: Option<String>,
    pub project_id: String,
    pub project_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub session_title: String,
    pub tool: String,
    #[serde(rename = "aiSessionId", skip_serializing_if = "Option::is_none")]
    pub ai_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub state: String,
    pub status: String,
    pub is_running: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub total_tokens: i64,
    pub baseline_total_tokens: i64,
    pub baseline_cached_input_tokens: i64,
    #[serde(skip_serializing)]
    pub baseline_resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<f64>,
    pub updated_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_turn_started_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_turn_started_at: Option<f64>,
    #[serde(skip_serializing)]
    pub completed_turn_started_at: Option<f64>,
    pub has_completed_turn: bool,
    pub was_interrupted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_assistant_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<AIPlanSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIPlanSnapshot {
    pub source: String,
    pub session_id: String,
    pub updated_at: f64,
    pub items: Vec<AIPlanItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIPlanItem {
    pub text: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AIProjectPhase {
    Idle,
    Running {
        tool: String,
    },
    NeedsInput {
        tool: String,
    },
    Completed {
        tool: String,
        #[serde(rename = "wasInterrupted")]
        was_interrupted: bool,
        #[serde(rename = "updatedAt")]
        updated_at: f64,
    },
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AIProjectTotals {
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub running: usize,
    pub needs_input: usize,
    pub completed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIProjectStateSnapshot {
    pub project_id: String,
    pub project_phase: AIProjectPhase,
    pub completed_phase: AIProjectPhase,
    pub totals: AIProjectTotals,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AILatestCompletion {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub tool: String,
    pub was_interrupted: bool,
    pub updated_at: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeStateSnapshot {
    pub sessions: Vec<AISessionSnapshot>,
    pub projects: Vec<AIProjectStateSnapshot>,
    pub global_totals: AIProjectTotals,
    pub needs_input_count: usize,
    pub running_count: usize,
    pub completion_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_completion: Option<AILatestCompletion>,
    pub updated_at: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AIRuntimeCompletionEvent {
    pub id: String,
    pub project_name: String,
    pub tool: String,
    pub was_interrupted: bool,
    pub session: Option<AISessionSnapshot>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeProbeRequest {
    pub terminal_id: String,
    pub terminal_instance_id: Option<String>,
    pub project_id: String,
    pub project_path: Option<String>,
    pub tool: String,
    pub external_session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub started_at: Option<f64>,
    pub updated_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeContextSnapshot {
    pub tool: String,
    #[serde(rename = "externalSessionID", skip_serializing_if = "Option::is_none")]
    pub external_session_id: Option<String>,
    #[serde(rename = "transcriptPath", skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_preview: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub total_tokens: i64,
    pub updated_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_state: Option<String>,
    pub was_interrupted: bool,
    pub has_completed_turn: bool,
    pub session_origin: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<AIPlanSnapshot>,
}
