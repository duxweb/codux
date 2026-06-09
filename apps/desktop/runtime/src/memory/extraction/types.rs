use super::helpers::normalized_token;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemoryScope {
    User,
    Project,
}

impl MemoryScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match normalized_token(value).as_str() {
            "user" | "global" | "developer" | "crossproject" | "cross_project" => Self::User,
            _ => Self::Project,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemoryTier {
    Core,
    Working,
    Archive,
}

impl MemoryTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Working => "working",
            Self::Archive => "archive",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match normalized_token(value).as_str() {
            "core" | "stable" | "pinned" | "important" => Self::Core,
            "archive" | "archived" => Self::Archive,
            _ => Self::Working,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Preference,
    Convention,
    Decision,
    Fact,
    BugLesson,
}

impl MemoryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::Convention => "convention",
            Self::Decision => "decision",
            Self::Fact => "fact",
            Self::BugLesson => "bug_lesson",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match normalized_token(value).as_str() {
            "preference" | "preferences" | "userpreference" | "style" | "workflow" => {
                Self::Preference
            }
            "convention" | "conventions" | "rule" | "standard" | "pattern" => Self::Convention,
            "decision" | "decisions" | "choice" | "accepteddecision" => Self::Decision,
            "buglesson" | "bug_lesson" | "lesson" | "bug" | "regression" | "fix" | "fixpattern"
            | "fix_pattern" => Self::BugLesson,
            _ => Self::Fact,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemoryExtractionResponse {
    pub user_summary: Option<String>,
    pub working_add: Vec<MemoryExtractionItem>,
    pub working_archive: Vec<String>,
    pub merged_entry_ids: Vec<String>,
    pub project_profile_refresh_recommended: bool,
}

#[derive(Debug, Clone)]
pub struct MemoryExtractionItem {
    pub scope: Option<MemoryScope>,
    pub module_key: Option<String>,
    pub tier: Option<MemoryTier>,
    pub kind: MemoryKind,
    pub content: String,
    pub rationale: Option<String>,
    pub merge_with: Vec<String>,
    pub replace: Option<String>,
    pub archive: Vec<String>,
    pub skip_reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PromptMemorySummary {
    pub content: String,
    pub version: i64,
}

#[derive(Debug, Clone, Default)]
pub struct PromptMemoryEntry {
    pub id: String,
    pub module_key: Option<String>,
    pub kind: String,
    pub content: String,
    pub rationale: Option<String>,
}
