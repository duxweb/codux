use super::super::MemorySummary;
use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEnqueueResult {
    pub enqueued: bool,
    pub reason: String,
    pub summary: MemorySummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryExtractionTask {
    pub id: String,
    pub project_id: String,
    pub tool: String,
    pub session_id: String,
    pub transcript_path: String,
    pub workspace_path: Option<String>,
    pub source_fingerprint: String,
    pub status: String,
    pub attempts: i64,
    pub error: Option<String>,
    pub enqueued_at: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemoryExtractionStatus {
    Idle,
    Queued,
    Processing,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryExtractionStatusSnapshot {
    pub status: MemoryExtractionStatus,
    pub pending_count: i64,
    pub running_count: i64,
    pub checked_count: i64,
    pub enqueued_count: i64,
    pub last_error: Option<String>,
    pub updated_at: f64,
}
