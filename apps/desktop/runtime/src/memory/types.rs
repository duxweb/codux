use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::settings::app_settings::AISettings;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummary {
    pub available: bool,
    pub active_entries: i64,
    pub core_entries: i64,
    pub working_entries: i64,
    pub archived_entries: i64,
    pub summaries: i64,
    pub queued_extractions: i64,
    pub failed_extractions: i64,
    pub project_profile_present: bool,
    pub recent_entries: Vec<MemoryEntrySummary>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntrySummary {
    pub id: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub tool_id: Option<String>,
    pub tier: String,
    pub kind: String,
    pub module_key: String,
    pub status: String,
    pub content: String,
    pub rationale: Option<String>,
    pub source_tool: Option<String>,
    pub source_session_id: Option<String>,
    pub merged_summary_id: Option<String>,
    pub archived_at: Option<f64>,
    pub access_count: i64,
    pub created_at: f64,
    pub updated_at: f64,
    pub last_decision: Option<MemoryEntryDecisionSummary>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntryDecisionSummary {
    pub kind: String,
    pub entry_id: Option<String>,
    pub target_entry_id: Option<String>,
    pub reason: String,
    pub created_at: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagerSnapshot {
    pub available: bool,
    pub target_rows: Vec<MemoryManagerTargetRow>,
    pub selected_target_title: String,
    pub current_overview: MemoryScopeOverview,
    pub project_profile: Option<MemoryProjectProfileSummary>,
    pub entries: Vec<MemoryEntrySummary>,
    pub summaries: Vec<MemorySummaryRow>,
    pub queued_extractions: Vec<crate::memory::queue::MemoryExtractionTask>,
    pub failed_extractions: Vec<crate::memory::queue::MemoryExtractionTask>,
    pub extraction: MemoryExtractionSummary,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagerTargetRow {
    pub id: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub count: i64,
    pub updated_at: Option<f64>,
    pub is_open_project: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryScopeOverview {
    pub active_entry_count: i64,
    pub archived_entry_count: i64,
    pub merged_entry_count: i64,
    pub profile_count: i64,
    pub summary_count: i64,
    pub total_token_estimate: i64,
    pub updated_at: Option<f64>,
}

impl MemoryScopeOverview {
    pub(super) fn total_count(&self) -> i64 {
        self.active_entry_count
            + self.archived_entry_count
            + self.merged_entry_count
            + self.profile_count
            + self.summary_count
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProjectProfileSummary {
    pub project_id: String,
    pub content: String,
    pub source_fingerprint: String,
    pub created_at: f64,
    pub updated_at: f64,
}

pub type MemoryProjectProfile = MemoryProjectProfileSummary;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProjectProfileRefreshResult {
    pub profile: MemoryProjectProfile,
    pub used_llm: bool,
    pub fallback_reason: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummaryRow {
    pub id: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub tool_id: Option<String>,
    pub content: String,
    pub version: i64,
    pub source_entry_ids: Vec<String>,
    pub token_estimate: i64,
    pub created_at: f64,
    pub updated_at: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummaryUpdateRequest {
    pub summary_id: String,
    pub content: String,
    pub max_versions: Option<i32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProjectMigrationRequest {
    pub from_project_id: String,
    pub to_project_id: String,
    pub overwrite: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryExtractionSummary {
    pub queued: i64,
    pub running: i64,
    pub failed: i64,
    pub last_error: Option<String>,
}

pub struct MemoryService {
    pub(super) database_path: PathBuf,
}

pub type MemoryStore = MemoryService;

#[derive(Clone, Debug, Default)]
pub struct MemoryLaunchArtifacts {
    pub workspace_root: PathBuf,
    pub prompt_file: PathBuf,
    pub index_file: PathBuf,
}

#[derive(Clone, Debug)]
pub struct MemoryLaunchRequest {
    pub project_id: String,
    pub project_name: String,
    pub workspace_path: Option<String>,
    pub settings: AISettings,
    pub extra_context: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagementRequest {
    pub scope: String,
    pub project_id: Option<String>,
    pub tier: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagerSnapshotRequest {
    pub scope: String,
    pub project_id: Option<String>,
    pub tab: String,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagementSnapshot {
    pub available: bool,
    pub entries: Vec<MemoryEntrySummary>,
    pub summaries: Vec<MemorySummaryRow>,
    pub extraction: MemoryExtractionSummary,
    pub error: Option<String>,
}
