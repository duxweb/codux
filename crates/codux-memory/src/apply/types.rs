use crate::extraction::{MemoryKind, MemoryScope, MemoryTier};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryWriteDecisionKind {
    Create,
    Merge,
    Replace,
    Archive,
    Skip,
}

impl MemoryWriteDecisionKind {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Merge => "merge",
            Self::Replace => "replace",
            Self::Archive => "archive",
            Self::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDecisionLog {
    pub kind: MemoryWriteDecisionKind,
    pub entry_id: Option<String>,
    pub target_entry_id: Option<String>,
    pub reason: String,
    pub created_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMemoryEntry {
    pub id: String,
    pub scope: MemoryScope,
    pub project_id: Option<String>,
    pub tool_id: Option<String>,
    pub module_key: Option<String>,
    pub tier: MemoryTier,
    pub kind: MemoryKind,
    pub content: String,
    pub rationale: Option<String>,
    pub source_tool: Option<String>,
    pub source_session_id: Option<String>,
    pub source_fingerprint: Option<String>,
    pub normalized_hash: String,
    pub superseded_by: Option<String>,
    pub status: MemoryEntryStatus,
    pub merged_summary_id: Option<String>,
    pub merged_at: Option<f64>,
    pub archived_at: Option<f64>,
    pub access_count: i64,
    pub last_accessed_at: Option<f64>,
    pub created_at: f64,
    pub updated_at: f64,
    pub last_decision: Option<MemoryDecisionLog>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemoryEntryStatus {
    Active,
    Merged,
    Archived,
}

impl MemoryEntryStatus {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Merged => "merged",
            Self::Archived => "archived",
        }
    }

    pub(super) fn from_str(value: &str) -> Self {
        match value {
            "merged" => Self::Merged,
            "archived" => Self::Archived,
            _ => Self::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMemorySummary {
    pub id: String,
    pub scope: MemoryScope,
    pub project_id: Option<String>,
    pub tool_id: Option<String>,
    pub content: String,
    pub version: i64,
    pub source_entry_ids: Vec<String>,
    pub token_estimate: i64,
    pub created_at: f64,
    pub updated_at: f64,
}

#[derive(Debug, Clone)]
pub(super) struct MemoryCandidate {
    pub(super) scope: MemoryScope,
    pub(super) project_id: Option<String>,
    pub(super) tool_id: Option<String>,
    pub(super) module_key: Option<String>,
    pub(super) tier: MemoryTier,
    pub(super) kind: MemoryKind,
    pub(super) content: String,
    pub(super) rationale: Option<String>,
    pub(super) source_tool: Option<String>,
    pub(super) source_session_id: Option<String>,
    pub(super) source_fingerprint: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct MemoryWriteDecision {
    pub(super) kind: MemoryWriteDecisionKind,
    pub(super) target_entry_id: Option<String>,
    pub(super) reason: String,
}
