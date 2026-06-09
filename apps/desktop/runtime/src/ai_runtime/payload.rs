use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHookEventMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(rename = "wasInterrupted", skip_serializing_if = "Option::is_none")]
    pub was_interrupted: Option<bool>,
    #[serde(rename = "hasCompletedTurn", skip_serializing_if = "Option::is_none")]
    pub has_completed_turn: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHookEventPayload {
    pub kind: String,
    #[serde(rename = "terminalID")]
    pub terminal_id: String,
    #[serde(rename = "terminalInstanceID", skip_serializing_if = "Option::is_none")]
    pub terminal_instance_id: Option<String>,
    #[serde(rename = "projectID")]
    pub project_id: String,
    #[serde(rename = "projectName")]
    pub project_name: String,
    #[serde(rename = "projectPath", skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(rename = "sessionTitle")]
    pub session_title: String,
    pub tool: String,
    #[serde(rename = "aiSessionID", skip_serializing_if = "Option::is_none")]
    pub ai_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(rename = "inputTokens", skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i64>,
    #[serde(rename = "outputTokens", skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i64>,
    #[serde(rename = "cachedInputTokens", skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<i64>,
    #[serde(rename = "totalTokens", skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<i64>,
    #[serde(rename = "updatedAt")]
    pub updated_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AIHookEventMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEnvelope {
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIToolUsageEnvelope {
    pub session_id: String,
    pub session_instance_id: Option<String>,
    #[serde(rename = "externalSessionID")]
    pub external_session_id: Option<String>,
    pub project_id: String,
    pub project_name: String,
    pub project_path: Option<String>,
    pub session_title: String,
    pub tool: String,
    pub model: Option<String>,
    pub status: String,
    pub response_state: Option<String>,
    pub updated_at: f64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AIRuntimeEvent {
    Hook { payload: AIHookEventPayload },
}
