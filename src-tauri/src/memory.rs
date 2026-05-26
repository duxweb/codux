use crate::ai_history::AISessionSummary;
use crate::ai_runtime::AISessionSnapshot;
use crate::app_settings::{AIMemorySettings, AIProviderSettings, AISettings, AppSettingsStore};
use crate::llm;
use crate::paths::{app_support_dir, home_dir, runtime_temp_dir};
use crate::project_store::{ProjectRecord, ProjectWorkspaceRecord};
use crate::runtime_trace::{runtime_trace, runtime_trace_elapsed};
use anyhow::{anyhow, Context, Result};
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

const MEMORY_CONTEXT_CANDIDATE_FANOUT: i64 = 8;
const MEMORY_WRITE_CANDIDATE_LIMIT: i64 = 8;
const MEMORY_RETRIEVAL_MAX_QUERY_TERMS: usize = 120;
const RECENT_MEMORY_FAILURE_TTL_SECONDS: f64 = 120.0;
const DEFAULT_MEMORY_MODULE: &str = "general";
const MEMORY_MERGE_SIMILARITY_THRESHOLD: f64 = 0.64;
const MEMORY_REPLACE_SIMILARITY_THRESHOLD: f64 = 0.34;
const PROJECT_PROFILE_LLM_REFRESH_COOLDOWN_SECONDS: f64 = 6.0 * 60.0 * 60.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemoryScope {
    User,
    Project,
}

impl MemoryScope {
    fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }

    fn from_str(value: &str) -> Self {
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
    fn as_str(&self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Working => "working",
            Self::Archive => "archive",
        }
    }

    fn from_str(value: &str) -> Self {
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
    fn as_str(&self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::Convention => "convention",
            Self::Decision => "decision",
            Self::Fact => "fact",
            Self::BugLesson => "bug_lesson",
        }
    }

    fn from_str(value: &str) -> Self {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemoryEntryStatus {
    Active,
    Merged,
    Archived,
}

impl MemoryEntryStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Merged => "merged",
            Self::Archived => "archived",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "merged" => Self::Merged,
            "archived" => Self::Archived,
            _ => Self::Active,
        }
    }
}

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
    fn as_str(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Merge => "merge",
            Self::Replace => "replace",
            Self::Archive => "archive",
            Self::Skip => "skip",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "merge" => Self::Merge,
            "replace" => Self::Replace,
            "archive" => Self::Archive,
            "skip" => Self::Skip,
            _ => Self::Create,
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
pub struct MemoryEntry {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummary {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProjectProfile {
    pub project_id: String,
    pub content: String,
    pub source_fingerprint: String,
    pub created_at: f64,
    pub updated_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProjectProfileRefreshResult {
    pub profile: MemoryProjectProfile,
    pub used_llm: bool,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct MemoryCandidate {
    scope: MemoryScope,
    project_id: Option<String>,
    tool_id: Option<String>,
    module_key: Option<String>,
    tier: MemoryTier,
    kind: MemoryKind,
    content: String,
    rationale: Option<String>,
    source_tool: Option<String>,
    source_session_id: Option<String>,
    source_fingerprint: Option<String>,
}

#[derive(Debug, Clone)]
struct MemoryWriteDecision {
    kind: MemoryWriteDecisionKind,
    target_entry_id: Option<String>,
    reason: String,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryLaunchArtifacts {
    pub workspace_root: String,
    pub prompt_file: String,
    pub index_file: String,
}

#[derive(Debug, Clone)]
pub struct MemoryLaunchRequest {
    pub project_id: String,
    pub project_name: String,
    pub workspace_path: Option<String>,
    pub settings: AISettings,
    pub extra_context: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagementRequest {
    pub scope: String,
    pub project_id: Option<String>,
    pub tier: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
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
    fn total_count(&self) -> i64 {
        self.active_entry_count
            + self.archived_entry_count
            + self.merged_entry_count
            + self.profile_count
            + self.summary_count
    }
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagerSnapshot {
    pub target_rows: Vec<MemoryManagerTargetRow>,
    pub selected_target_title: String,
    pub current_overview: MemoryScopeOverview,
    pub project_profile: Option<MemoryProjectProfile>,
    pub entries: Vec<MemoryEntry>,
    pub summaries: Vec<MemorySummary>,
    pub extraction: MemoryExtractionStatusSnapshot,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagerSnapshotRequest {
    pub scope: String,
    pub project_id: Option<String>,
    pub tab: String,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummaryUpdateRequest {
    pub summary_id: String,
    pub content: String,
    pub max_versions: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProjectMigrationRequest {
    pub from_project_id: String,
    pub to_project_id: String,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagementSnapshot {
    pub entries: Vec<MemoryEntry>,
    pub summaries: Vec<MemorySummary>,
    pub extraction: MemoryExtractionStatusSnapshot,
}

#[derive(Debug)]
pub struct MemoryStore {
    db_path: PathBuf,
    last_enqueued_at_by_session: Mutex<HashMap<String, f64>>,
    recent_failure: Mutex<Option<RecentMemoryFailure>>,
    processing_queue: AtomicBool,
    cancel_requested: AtomicBool,
}

#[derive(Debug, Clone)]
struct RecentMemoryFailure {
    message: String,
    occurred_at: f64,
}

#[derive(Debug, Clone)]
pub struct MemoryProjectContext {
    pub project_id: String,
    pub project_name: String,
    pub workspace_path: String,
}

impl MemoryProjectContext {
    fn from_workspace(workspace: &ProjectWorkspaceRecord) -> Self {
        Self {
            project_id: workspace.root_project_id.clone(),
            project_name: workspace.root_project_name.clone(),
            workspace_path: workspace.workspace_path.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryQueueStatusEvent {
    pub status: MemoryExtractionStatusSnapshot,
    pub manager: Option<MemoryManagerSnapshot>,
}

struct MemoryQueueProcessingGuard<'a> {
    flag: &'a AtomicBool,
}

impl Drop for MemoryQueueProcessingGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

impl MemoryStore {
    pub fn load_or_create() -> Result<Self> {
        let started_at = Instant::now();
        let root = app_support_dir();
        fs::create_dir_all(&root)?;
        let store = Self {
            db_path: root.join("memory.sqlite3"),
            last_enqueued_at_by_session: Mutex::new(HashMap::new()),
            recent_failure: Mutex::new(None),
            processing_queue: AtomicBool::new(false),
            cancel_requested: AtomicBool::new(false),
        };
        store.configure()?;
        runtime_trace_elapsed("startup", "memory_store_load", started_at, "");
        Ok(store)
    }

    pub fn prepare_launch_artifacts(
        &self,
        request: MemoryLaunchRequest,
    ) -> Option<MemoryLaunchArtifacts> {
        let global_prompt = normalized_non_empty(&request.settings.global_prompt);
        let extra_context = request
            .extra_context
            .as_deref()
            .and_then(normalized_non_empty);
        let test_injection = test_memory_injection_note();
        let should_inject_memory =
            request.settings.memory.enabled && request.settings.memory.automatic_injection_enabled;
        if global_prompt.is_none()
            && extra_context.is_none()
            && test_injection.is_none()
            && !should_inject_memory
        {
            return None;
        }

        let root = runtime_temp_dir()
            .join("runtime-root")
            .join("memory-workspaces")
            .join(safe_path_segment(&request.project_id));
        let prompt_file = root.join("memory-prompt.txt");
        let index_file = root.join("MEMORY.md");
        let project_profile = request.workspace_path.as_deref().and_then(|path| {
            self.project_profile_for_launch(&request.project_id, &request.project_name, path)
        });

        let mut claude_context = self.collect_context(
            &request.project_id,
            &request.project_name,
            project_profile.clone(),
            "claude",
            &request.settings,
        );
        let mut codex_context = self.collect_context(
            &request.project_id,
            &request.project_name,
            project_profile.clone(),
            "codex",
            &request.settings,
        );
        let mut gemini_context = self.collect_context(
            &request.project_id,
            &request.project_name,
            project_profile,
            "gemini",
            &request.settings,
        );
        let extra_context = merge_optional_sections(extra_context, test_injection);
        claude_context.extra_context = extra_context.clone();
        codex_context.extra_context = extra_context.clone();
        gemini_context.extra_context = extra_context;
        let memory_context = MemoryContextPayload::merged([
            claude_context.clone(),
            codex_context.clone(),
            gemini_context.clone(),
        ]);

        let prompt_text = render_index_text(&claude_context, &root);
        let index_text = render_index_text(&memory_context, &root);
        let claude_text = render_tool_launch_text(
            &request.project_id,
            &request.project_name,
            "claude",
            &root,
            &claude_context,
        );
        let agents_text = render_tool_launch_text(
            &request.project_id,
            &request.project_name,
            "codex",
            &root,
            &codex_context,
        );
        let gemini_text = render_tool_launch_text(
            &request.project_id,
            &request.project_name,
            "gemini",
            &root,
            &gemini_context,
        );

        if prompt_text.is_empty()
            && index_text.is_empty()
            && claude_text.is_empty()
            && agents_text.is_empty()
            && gemini_text.is_empty()
        {
            return None;
        }

        if fs::create_dir_all(&root).is_err() {
            return None;
        }
        fs::write(&prompt_file, prompt_text).ok()?;
        fs::write(&index_file, index_text).ok()?;
        fs::write(
            root.join("memory-user.md"),
            render_user_memory_text(&memory_context),
        )
        .ok()?;
        fs::write(
            root.join("memory-project-profile.md"),
            render_project_profile_text(&memory_context),
        )
        .ok()?;
        fs::write(
            root.join("memory-project.md"),
            render_project_memory_text(&memory_context),
        )
        .ok()?;
        fs::write(
            root.join("memory-recent.md"),
            render_recent_memory_text(&memory_context),
        )
        .ok()?;
        fs::write(
            root.join("memory-search.md"),
            render_search_guide_text(&memory_context),
        )
        .ok()?;
        fs::write(root.join("CLAUDE.md"), claude_text).ok()?;
        fs::write(root.join("AGENTS.md"), agents_text).ok()?;
        fs::write(root.join("GEMINI.md"), gemini_text).ok()?;

        Some(MemoryLaunchArtifacts {
            workspace_root: root.display().to_string(),
            prompt_file: prompt_file.display().to_string(),
            index_file: index_file.display().to_string(),
        })
    }

    pub fn recover_interrupted_extractions(&self) -> Result<i64> {
        let started_at = Instant::now();
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memory_extraction_queue WHERE status = 'running';",
            [],
            |row| row.get(0),
        )?;
        if count == 0 {
            runtime_trace_elapsed(
                "startup",
                "memory_recover_interrupted",
                started_at,
                "count=0",
            );
            return Ok(0);
        }
        conn.execute(
            r#"
            UPDATE memory_extraction_queue
            SET status = 'pending', error = ?1
            WHERE status = 'running'
            "#,
            params!["Recovered after app restart before completion."],
        )?;
        runtime_trace_elapsed(
            "startup",
            "memory_recover_interrupted",
            started_at,
            &format!("count={count}"),
        );
        Ok(count)
    }

    pub fn management_snapshot(
        &self,
        request: MemoryManagementRequest,
    ) -> Result<MemoryManagementSnapshot> {
        let scope = MemoryScope::from_str(&request.scope);
        let tier = request.tier.as_deref().map(MemoryTier::from_str);
        let status = request.status.as_deref().map(MemoryEntryStatus::from_str);
        Ok(MemoryManagementSnapshot {
            entries: self.list_entries_for_management(
                scope.clone(),
                request.project_id.as_deref(),
                tier.as_ref(),
                status.as_ref(),
                request.limit.unwrap_or(500).clamp(1, 1000),
            )?,
            summaries: self.list_summaries_for_management(scope, request.project_id.as_deref())?,
            extraction: self.extraction_status_snapshot()?,
        })
    }

    pub fn manager_snapshot(
        &self,
        request: MemoryManagerSnapshotRequest,
        projects: &[ProjectRecord],
    ) -> Result<MemoryManagerSnapshot> {
        let scope = MemoryScope::from_str(&request.scope);
        let project_id = match scope {
            MemoryScope::User => None,
            MemoryScope::Project => request.project_id.as_deref(),
        };
        let limit = request.limit.unwrap_or(500).clamp(1, 1000);
        let target_rows = self.manager_target_rows(projects)?;
        let selected_target_title = selected_memory_target_title(&target_rows, &scope, project_id);
        let current_overview = self.memory_scope_overview(scope.clone(), project_id)?;
        let (mut entries, summaries) = match request.tab.as_str() {
            "summary" => (
                Vec::new(),
                self.list_summaries_for_management(scope.clone(), project_id)?,
            ),
            "history" => (
                self.list_history_entries_for_management(scope.clone(), project_id, limit)?,
                Vec::new(),
            ),
            _ => (
                self.list_active_entries_for_management(scope.clone(), project_id, limit)?,
                Vec::new(),
            ),
        };
        let project_profile = match scope {
            MemoryScope::Project => {
                project_id.and_then(|id| self.current_project_profile(id).ok().flatten())
            }
            MemoryScope::User => None,
        };
        self.attach_last_decisions(&mut entries)?;

        Ok(MemoryManagerSnapshot {
            target_rows,
            selected_target_title,
            current_overview,
            project_profile,
            entries,
            summaries,
            extraction: self.extraction_status_snapshot()?,
        })
    }

    pub fn archive_entry(&self, entry_id: &str) -> Result<()> {
        let now = now_seconds();
        let conn = self.connect()?;
        conn.execute(
            r#"
            UPDATE memory_entries
            SET tier = 'archive',
                status = 'archived',
                archived_at = ?1,
                updated_at = ?1
            WHERE id = ?2;
            "#,
            params![now, entry_id],
        )?;
        Ok(())
    }

    pub fn delete_entry(&self, entry_id: &str) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM memory_entries WHERE id = ?1;",
            params![entry_id],
        )?;
        Ok(())
    }

    pub fn delete_summary(&self, summary_id: &str) -> Result<()> {
        let now = now_seconds();
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM memory_summary_versions WHERE summary_id = ?1;",
            params![summary_id],
        )?;
        conn.execute(
            "DELETE FROM memory_summaries WHERE id = ?1;",
            params![summary_id],
        )?;
        conn.execute(
            r#"
            UPDATE memory_entries
            SET merged_summary_id = NULL,
                updated_at = ?1
            WHERE merged_summary_id = ?2;
            "#,
            params![now, summary_id],
        )?;
        Ok(())
    }

    pub fn delete_project_profile(&self, project_id: &str) -> Result<()> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Ok(());
        }
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM memory_project_profiles WHERE project_id = ?1;",
            params![project_id],
        )?;
        Ok(())
    }

    pub fn delete_project_memory(&self, project_id: &str) -> Result<()> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Ok(());
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        delete_project_memory_in_tx(&tx, project_id)?;
        tx.commit()?;
        Ok(())
    }

    pub fn migrate_project_memory(&self, request: MemoryProjectMigrationRequest) -> Result<()> {
        let from_project_id = request.from_project_id.trim();
        let to_project_id = request.to_project_id.trim();
        if from_project_id.is_empty() || to_project_id.is_empty() {
            return Err(anyhow!("project id cannot be empty"));
        }
        if from_project_id == to_project_id {
            return Err(anyhow!("source and target project are the same"));
        }

        let source_overview =
            self.memory_scope_overview(MemoryScope::Project, Some(from_project_id))?;
        if source_overview.total_count() == 0 {
            return Err(anyhow!("source project memory is empty"));
        }
        let target_overview =
            self.memory_scope_overview(MemoryScope::Project, Some(to_project_id))?;
        if target_overview.total_count() > 0 && !request.overwrite {
            return Err(anyhow!("target project already has memory"));
        }

        let now = now_seconds();
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;

        if request.overwrite {
            delete_project_memory_in_tx(&tx, to_project_id)?;
        }

        tx.execute(
            r#"
            UPDATE memory_entries
            SET project_id = ?1, updated_at = ?2
            WHERE scope = 'project' AND project_id = ?3;
            "#,
            params![to_project_id, now, from_project_id],
        )?;
        tx.execute(
            r#"
            UPDATE memory_summaries
            SET project_id = ?1, updated_at = ?2
            WHERE scope = 'project' AND project_id = ?3;
            "#,
            params![to_project_id, now, from_project_id],
        )?;
        tx.execute(
            r#"
            UPDATE memory_project_profiles
            SET project_id = ?1, updated_at = ?2
            WHERE project_id = ?3;
            "#,
            params![to_project_id, now, from_project_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_summary(&self, request: MemorySummaryUpdateRequest) -> Result<MemorySummary> {
        let content = request.content.trim();
        if content.is_empty() {
            return Err(anyhow!("summary content cannot be empty"));
        }
        let existing = self.summary_by_id(&request.summary_id)?;
        self.upsert_summary(
            existing.scope,
            existing.project_id.as_deref(),
            existing.tool_id.as_deref(),
            content,
            &existing.source_entry_ids,
            request.max_versions.unwrap_or(20).max(1),
        )
    }

    fn project_profile_for_launch(
        &self,
        project_id: &str,
        project_name: &str,
        workspace_path: &str,
    ) -> Option<MemoryProjectProfile> {
        let generated = build_project_profile(project_id, project_name, workspace_path)?;
        match self.upsert_project_profile(generated.clone()) {
            Ok(profile) => Some(profile),
            Err(error) => {
                append_memory_log(
                    "project-profile",
                    &format!("failed to store profile: {error}"),
                );
                Some(generated)
            }
        }
    }

    pub async fn force_refresh_project_profile_with_llm_detailed(
        &self,
        settings: &AISettings,
        project: &ProjectRecord,
    ) -> Option<MemoryProjectProfileRefreshResult> {
        self.refresh_project_profile_detailed(settings, project, true)
            .await
    }

    async fn refresh_project_profile_detailed(
        &self,
        settings: &AISettings,
        project: &ProjectRecord,
        force_llm: bool,
    ) -> Option<MemoryProjectProfileRefreshResult> {
        let generated = build_project_profile(&project.id, &project.name, &project.path)?;
        let memory_context = if force_llm {
            self.project_profile_memory_context(&project.id)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let llm_source_fingerprint = if memory_context.is_empty() {
            generated.source_fingerprint.clone()
        } else {
            project_profile_llm_source_fingerprint(&generated.source_fingerprint, &memory_context)
        };
        let llm_fingerprint = llm_project_profile_fingerprint(&llm_source_fingerprint);
        let existing = self.current_project_profile(&project.id).ok().flatten();
        if !force_llm
            && existing
                .as_ref()
                .is_some_and(|profile| profile.source_fingerprint == llm_fingerprint)
        {
            return existing.map(|profile| MemoryProjectProfileRefreshResult {
                profile,
                used_llm: true,
                fallback_reason: None,
            });
        }

        if let Some(existing) = existing.as_ref() {
            if !force_llm && !project_profile_llm_refresh_due(existing, &generated) {
                if !project_profile_fingerprints_match(
                    &existing.source_fingerprint,
                    &generated.source_fingerprint,
                ) {
                    let profile = self
                        .upsert_project_profile(generated.clone())
                        .ok()
                        .unwrap_or(generated);
                    return Some(MemoryProjectProfileRefreshResult {
                        profile,
                        used_llm: false,
                        fallback_reason: Some("Repository fingerprint changed before LLM refresh was due; stored local scan.".to_string()),
                    });
                }
                return Some(MemoryProjectProfileRefreshResult {
                    profile: existing.clone(),
                    used_llm: existing.source_fingerprint.starts_with("llm-v1:"),
                    fallback_reason: None,
                });
            }
        }

        let Some(provider) = select_memory_provider(settings, None).cloned() else {
            let profile = self
                .upsert_project_profile(generated.clone())
                .ok()
                .unwrap_or(generated);
            return Some(MemoryProjectProfileRefreshResult {
                profile,
                used_llm: false,
                fallback_reason: Some("No enabled AI provider is configured for memory extraction; stored local scan.".to_string()),
            });
        };
        let llm_profile_content =
            project_profile_content_with_memory_context(&generated.content, &memory_context);
        let prompt = make_project_profile_llm_prompt(&llm_profile_content);
        append_memory_log(
            "project-profile",
            &format!(
                "llm refresh start project_id={} provider_id={} kind={} model={} prompt_chars={} deterministic_chars={} memory_signals={} fingerprint={}",
                safe_log_value(&project.id, 80),
                safe_log_value(&provider.id, 80),
                safe_log_value(&provider.kind, 40),
                safe_log_value(&provider.model, 120),
                prompt.chars().count(),
                generated.content.chars().count(),
                memory_context.len(),
                short_log_fingerprint(&llm_fingerprint)
            ),
        );
        let response = llm::complete_with_provider_options(
            &provider,
            &prompt,
            Some(project_profile_system_prompt()),
            llm::LLMProviderCompletionOptions {
                max_tokens: 1400,
                temperature: 0.1,
                preserve_formatting: true,
                json_response: true,
                timeout_seconds: 120,
            },
        )
        .await;

        match response {
            Ok(text) => {
                append_memory_log(
                    "project-profile",
                    &format!(
                        "llm response received project_id={} chars={}",
                        safe_log_value(&project.id, 80),
                        text.chars().count()
                    ),
                );
                match decode_project_profile_llm_response_detailed(&text) {
                    Ok(content) => {
                        append_memory_log(
                            "project-profile",
                            &format!(
                                "llm refresh ok project_id={} content_chars={}",
                                safe_log_value(&project.id, 80),
                                content.chars().count()
                            ),
                        );
                        let profile = MemoryProjectProfile {
                            content,
                            source_fingerprint: llm_fingerprint,
                            ..generated
                        };
                        self.upsert_project_profile(profile.clone())
                            .ok()
                            .or(Some(profile))
                            .map(|profile| MemoryProjectProfileRefreshResult {
                                profile,
                                used_llm: true,
                                fallback_reason: None,
                            })
                    }
                    Err(error) => {
                        append_memory_log(
                            "project-profile",
                            &format!(
                                "llm decode failed project_id={} reason={} preview={}",
                                safe_log_value(&project.id, 80),
                                error,
                                safe_log_preview(&text, 1200)
                            ),
                        );
                        let profile = self
                            .upsert_project_profile(generated.clone())
                            .and_then(|profile| {
                                self.touch_project_profile(&project.id)?;
                                Ok(profile)
                            })
                            .ok()
                            .unwrap_or(generated);
                        Some(MemoryProjectProfileRefreshResult {
                            profile,
                            used_llm: false,
                            fallback_reason: Some(format!(
                                "LLM project profile decode failed: {error}; stored local scan."
                            )),
                        })
                    }
                }
            }
            Err(error) => {
                append_memory_log(
                    "project-profile",
                    &format!(
                        "llm request failed project_id={} error={}",
                        safe_log_value(&project.id, 80),
                        safe_log_value(&error, 500)
                    ),
                );
                let profile = self
                    .upsert_project_profile(generated.clone())
                    .and_then(|profile| {
                        self.touch_project_profile(&project.id)?;
                        Ok(profile)
                    })
                    .ok()
                    .unwrap_or(generated);
                Some(MemoryProjectProfileRefreshResult {
                    profile,
                    used_llm: false,
                    fallback_reason: Some(format!(
                        "LLM request failed: {error}; stored local scan."
                    )),
                })
            }
        }
    }

    fn touch_project_profile(&self, project_id: &str) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            "UPDATE memory_project_profiles SET updated_at = ?1 WHERE project_id = ?2;",
            params![now_seconds(), project_id],
        )?;
        Ok(())
    }

    fn upsert_project_profile(
        &self,
        profile: MemoryProjectProfile,
    ) -> Result<MemoryProjectProfile> {
        let now = now_seconds();
        let conn = self.connect()?;
        let existing = conn
            .query_row(
                r#"
                SELECT project_id, content, source_fingerprint, created_at, updated_at
                FROM memory_project_profiles
                WHERE project_id = ?1
                LIMIT 1;
                "#,
                params![profile.project_id],
                memory_project_profile_from_row,
            )
            .optional()?;
        if let Some(existing) = existing {
            if project_profile_fingerprints_match(
                &existing.source_fingerprint,
                &profile.source_fingerprint,
            ) {
                return Ok(existing);
            }
            conn.execute(
                r#"
                UPDATE memory_project_profiles
                SET content = ?1, source_fingerprint = ?2, updated_at = ?3
                WHERE project_id = ?4;
                "#,
                params![
                    profile.content,
                    profile.source_fingerprint,
                    now,
                    existing.project_id
                ],
            )?;
            return Ok(MemoryProjectProfile {
                created_at: existing.created_at,
                updated_at: now,
                ..profile
            });
        }
        conn.execute(
            r#"
            INSERT INTO memory_project_profiles (
                project_id, content, source_fingerprint, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5);
            "#,
            params![
                profile.project_id,
                profile.content,
                profile.source_fingerprint,
                now,
                now
            ],
        )?;
        Ok(MemoryProjectProfile {
            created_at: now,
            updated_at: now,
            ..profile
        })
    }

    fn current_project_profile(&self, project_id: &str) -> Result<Option<MemoryProjectProfile>> {
        let conn = self.connect()?;
        conn.query_row(
            r#"
            SELECT project_id, content, source_fingerprint, created_at, updated_at
            FROM memory_project_profiles
            WHERE project_id = ?1
            LIMIT 1;
            "#,
            params![project_id],
            memory_project_profile_from_row,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn process_sessions_now(
        self: Arc<Self>,
        settings: AISettings,
        projects: Vec<ProjectWorkspaceRecord>,
        sessions: Vec<AISessionSnapshot>,
        history_sessions: Vec<AISessionSummary>,
        on_status: impl Fn(MemoryQueueStatusEvent) + Send + Sync + 'static,
    ) -> Result<MemoryExtractionStatusSnapshot> {
        let started_at = Instant::now();
        if !settings.memory.enabled {
            return self.extraction_status_snapshot();
        }
        ensure_memory_provider_available(&settings)?;
        self.cancel_requested.store(false, Ordering::Release);
        let on_status: Arc<dyn Fn(MemoryQueueStatusEvent) + Send + Sync> = Arc::new(on_status);
        self.clear_recent_failure();
        let mut sessions =
            self.manual_extraction_candidates(&settings.memory, &projects, &sessions);
        sessions.extend(self.manual_extraction_candidates_from_history(
            &settings.memory,
            &projects,
            &history_sessions,
        ));
        let sessions = self.deduplicate_manual_candidates(sessions);
        let checked_count = sessions.len() as i64;
        let mut enqueued_count = 0_i64;
        for session in &sessions {
            match self.enqueue_session_for_manual_extraction(&projects, session) {
                Ok(true) => enqueued_count += 1,
                Ok(false) => {}
                Err(error) => append_memory_log("manual-enqueue", &format!("failed: {error}")),
            }
        }
        append_memory_log(
            "manual-enqueue",
            &format!("checked={checked_count} enqueued={enqueued_count}"),
        );
        runtime_trace_elapsed(
            "memory",
            "manual_enqueue",
            started_at,
            &format!(
                "projects={} runtime_sessions={} history_sessions={} checked={} enqueued={}",
                projects.len(),
                sessions.len(),
                history_sessions.len(),
                checked_count,
                enqueued_count
            ),
        );
        self.publish_queue_status(projects.as_slice(), Arc::as_ref(&on_status));
        let mut initial_status = self.extraction_status_snapshot()?;
        initial_status.checked_count = checked_count;
        initial_status.enqueued_count = enqueued_count;
        tauri::async_runtime::spawn(async move {
            if let Err(error) = self
                .process_queue(settings, projects, Arc::clone(&on_status), None)
                .await
            {
                append_memory_log("manual-extraction", &format!("failed: {error}"));
                self.publish_queue_status(&[], Arc::as_ref(&on_status));
            }
        });
        Ok(initial_status)
    }

    pub fn cancel_extraction_queue(&self) -> Result<MemoryExtractionStatusSnapshot> {
        self.cancel_requested.store(true, Ordering::Release);
        let conn = self.connect()?;
        conn.execute(
            "UPDATE memory_extraction_queue SET status = 'failed', error = ?1 WHERE status IN ('pending', 'running');",
            params!["Memory indexing stopped by user."],
        )?;
        self.extraction_status_snapshot()
    }

    pub fn extraction_status_snapshot(&self) -> Result<MemoryExtractionStatusSnapshot> {
        let conn = self.connect()?;
        let pending_count = scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM memory_extraction_queue WHERE status = 'pending';",
        )?;
        let running_count = scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM memory_extraction_queue WHERE status = 'running';",
        )?;
        let failure = self.current_recent_failure();
        let status = if running_count > 0 {
            MemoryExtractionStatus::Processing
        } else if pending_count > 0 {
            MemoryExtractionStatus::Queued
        } else if failure.is_some() {
            MemoryExtractionStatus::Failed
        } else {
            MemoryExtractionStatus::Idle
        };
        Ok(MemoryExtractionStatusSnapshot {
            status,
            pending_count,
            running_count,
            checked_count: 0,
            enqueued_count: 0,
            last_error: failure.map(|value| value.message),
            updated_at: now_seconds(),
        })
    }

    pub fn handle_completed_session(
        self: Arc<Self>,
        settings: Arc<AppSettingsStore>,
        projects: Vec<ProjectWorkspaceRecord>,
        session: AISessionSnapshot,
        on_status: impl Fn(MemoryQueueStatusEvent) + Send + Sync + 'static,
    ) {
        let on_status: Arc<dyn Fn(MemoryQueueStatusEvent) + Send + Sync> = Arc::new(on_status);
        tauri::async_runtime::spawn(async move {
            let configured = settings.snapshot().ai;
            if !configured.memory.enabled || !configured.memory.automatic_extraction_enabled {
                return;
            }
            if let Err(error) = ensure_memory_provider_available(&configured) {
                append_memory_log("auto-extraction", &format!("skipped: {error}"));
                return;
            }
            self.cancel_requested.store(false, Ordering::Release);
            self.clear_recent_failure();
            let result = self.enqueue_session_if_ready(&configured.memory, &projects, &session);
            if matches!(result, Ok(true)) {
                self.publish_queue_status(projects.as_slice(), Arc::as_ref(&on_status));
            }
            if let Err(error) = result {
                append_memory_log("auto-enqueue", &format!("failed: {error}"));
                return;
            }
            let queue_delay = (configured.memory.extraction_idle_delay_seconds > 0).then(|| {
                Duration::from_secs(configured.memory.extraction_idle_delay_seconds as u64)
            });
            if let Err(error) = self
                .process_queue(configured, projects, Arc::clone(&on_status), queue_delay)
                .await
            {
                append_memory_log("auto-extraction", &format!("failed: {error}"));
                self.publish_queue_status(&[], Arc::as_ref(&on_status));
            }
        });
    }

    fn configure(&self) -> Result<()> {
        let conn = self.connect()?;
        for statement in [
            "PRAGMA journal_mode=WAL;",
            "PRAGMA synchronous=NORMAL;",
            r#"
            CREATE TABLE IF NOT EXISTS memory_entries (
                id TEXT PRIMARY KEY,
                scope TEXT NOT NULL,
                project_id TEXT,
                tool_id TEXT,
                module_key TEXT,
                tier TEXT NOT NULL,
                kind TEXT NOT NULL,
                content TEXT NOT NULL,
                rationale TEXT,
                source_tool TEXT,
                source_session_id TEXT,
                source_fingerprint TEXT,
                normalized_hash TEXT NOT NULL,
                superseded_by TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                merged_summary_id TEXT,
                merged_at REAL,
                archived_at REAL,
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at REAL,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL
            );
            "#,
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                content, rationale, content='memory_entries', content_rowid='rowid'
            );
            "#,
            r#"
            CREATE TRIGGER IF NOT EXISTS memory_entries_ai AFTER INSERT ON memory_entries BEGIN
                INSERT INTO memory_fts(rowid, content, rationale)
                VALUES (new.rowid, new.content, COALESCE(new.rationale, ''));
            END;
            "#,
            r#"
            CREATE TRIGGER IF NOT EXISTS memory_entries_ad AFTER DELETE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content, rationale)
                VALUES('delete', old.rowid, old.content, COALESCE(old.rationale, ''));
            END;
            "#,
            r#"
            CREATE TRIGGER IF NOT EXISTS memory_entries_au AFTER UPDATE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content, rationale)
                VALUES('delete', old.rowid, old.content, COALESCE(old.rationale, ''));
                INSERT INTO memory_fts(rowid, content, rationale)
                VALUES (new.rowid, new.content, COALESCE(new.rationale, ''));
            END;
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS memory_extraction_queue (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                tool TEXT NOT NULL,
                session_id TEXT NOT NULL,
                transcript_path TEXT NOT NULL,
                workspace_path TEXT,
                source_fingerprint TEXT NOT NULL UNIQUE,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                error TEXT,
                enqueued_at REAL NOT NULL
            );
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS memory_summaries (
                id TEXT PRIMARY KEY,
                scope TEXT NOT NULL,
                project_id TEXT,
                tool_id TEXT,
                content TEXT NOT NULL,
                version INTEGER NOT NULL,
                source_entry_ids TEXT,
                token_estimate INTEGER NOT NULL DEFAULT 0,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL
            );
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS memory_project_profiles (
                project_id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                source_fingerprint TEXT NOT NULL,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL
            );
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS memory_summary_versions (
                id TEXT PRIMARY KEY,
                summary_id TEXT NOT NULL,
                version INTEGER NOT NULL,
                content TEXT NOT NULL,
                source_entry_ids TEXT,
                created_at REAL NOT NULL
            );
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS memory_decision_logs (
                id TEXT PRIMARY KEY,
                decision TEXT NOT NULL,
                entry_id TEXT,
                target_entry_id TEXT,
                reason TEXT NOT NULL,
                created_at REAL NOT NULL
            );
            "#,
            "ALTER TABLE memory_entries ADD COLUMN module_key TEXT;",
            "ALTER TABLE memory_extraction_queue ADD COLUMN workspace_path TEXT;",
            "CREATE INDEX IF NOT EXISTS idx_memory_entries_scope_project_tier ON memory_entries(scope, project_id, tier);",
            "CREATE INDEX IF NOT EXISTS idx_memory_entries_scope_project_module ON memory_entries(scope, project_id, module_key, tier, status);",
            "CREATE INDEX IF NOT EXISTS idx_memory_entries_tool ON memory_entries(tool_id);",
            "CREATE INDEX IF NOT EXISTS idx_memory_entries_hash ON memory_entries(scope, project_id, tool_id, module_key, normalized_hash);",
            "CREATE INDEX IF NOT EXISTS idx_memory_queue_status_time ON memory_extraction_queue(status, enqueued_at);",
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_summaries_scope_project_tool ON memory_summaries(scope, COALESCE(project_id, ''), COALESCE(tool_id, ''));",
            "CREATE INDEX IF NOT EXISTS idx_memory_summary_versions_summary ON memory_summary_versions(summary_id, version);",
            "CREATE INDEX IF NOT EXISTS idx_memory_decision_logs_created ON memory_decision_logs(created_at);",
            r#"
            DELETE FROM memory_summary_versions
            WHERE summary_id IN (
                SELECT id FROM memory_summaries
                WHERE content GLOB 'version=[0-9]*'
                  AND length(content) <= 24
            );
            "#,
            r#"
            DELETE FROM memory_summaries
            WHERE content GLOB 'version=[0-9]*'
              AND length(content) <= 24;
            "#,
        ] {
            if let Err(error) = conn.execute_batch(statement) {
                let message = error.to_string();
                if !message.contains("duplicate column name") {
                    return Err(error.into());
                }
            }
        }
        self.migrate_legacy_project_summaries()?;
        Ok(())
    }

    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path).with_context(|| {
            format!("failed to open memory database {}", self.db_path.display())
        })?;
        conn.busy_timeout(std::time::Duration::from_millis(3000))?;
        Ok(conn)
    }

    fn migrate_legacy_project_summaries(&self) -> Result<()> {
        let summaries = self.legacy_project_summaries()?;
        if summaries.is_empty() {
            return Ok(());
        }
        for summary in &summaries {
            let module_key = infer_memory_module_from_text(&summary.content);
            let _ = self.upsert(MemoryCandidate {
                scope: MemoryScope::Project,
                project_id: summary.project_id.clone(),
                tool_id: summary.tool_id.clone(),
                module_key: Some(module_key),
                tier: MemoryTier::Working,
                kind: MemoryKind::Fact,
                content: summary.content.clone(),
                rationale: Some("Migrated from legacy project summary.".to_string()),
                source_tool: Some("memory-migration".to_string()),
                source_session_id: None,
                source_fingerprint: Some(format!("legacy-summary-{}", summary.id)),
            });
        }
        let conn = self.connect()?;
        for summary in summaries {
            conn.execute(
                "DELETE FROM memory_summary_versions WHERE summary_id = ?1;",
                params![summary.id],
            )?;
            conn.execute(
                "DELETE FROM memory_summaries WHERE id = ?1 AND scope = 'project';",
                params![summary.id],
            )?;
        }
        Ok(())
    }

    fn legacy_project_summaries(&self) -> Result<Vec<MemorySummary>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            r#"
            SELECT id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
            FROM memory_summaries
            WHERE scope = 'project';
            "#,
        )?;
        let rows = statement.query_map([], memory_summary_from_row)?;
        Ok(rows.flatten().collect())
    }

    fn collect_context(
        &self,
        project_id: &str,
        project_name: &str,
        project_profile: Option<MemoryProjectProfile>,
        tool: &str,
        settings: &AISettings,
    ) -> MemoryContextPayload {
        let should_inject = settings.memory.enabled && settings.memory.automatic_injection_enabled;
        let user_summary = if should_inject && settings.memory.allow_cross_project_user_recall {
            self.current_summary(MemoryScope::User, None, None)
                .ok()
                .flatten()
        } else {
            None
        };
        let retrieval_query = memory_retrieval_query(project_name, tool, &settings.global_prompt);
        let user_working = if should_inject && settings.memory.allow_cross_project_user_recall {
            self.list_entries_for_context(
                MemoryScope::User,
                None,
                Some(tool),
                &[MemoryTier::Working],
                i64::from(settings.memory.max_injected_user_working_memories),
                &retrieval_query,
            )
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let project_working = if should_inject {
            self.list_entries_for_context(
                MemoryScope::Project,
                Some(project_id),
                Some(tool),
                &[MemoryTier::Working],
                i64::from(settings.memory.max_injected_project_working_memories),
                &retrieval_query,
            )
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let user_core_fallback = if should_inject
            && user_summary.is_none()
            && settings.memory.allow_cross_project_user_recall
        {
            self.list_entries_for_context(
                MemoryScope::User,
                None,
                Some(tool),
                &[MemoryTier::Core],
                4,
                &retrieval_query,
            )
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let project_core_fallback = if should_inject {
            self.list_entries_for_context(
                MemoryScope::Project,
                Some(project_id),
                Some(tool),
                &[MemoryTier::Core],
                6,
                &retrieval_query,
            )
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let accessed_ids = unique_entries(
            user_core_fallback
                .iter()
                .chain(user_working.iter())
                .chain(project_core_fallback.iter())
                .chain(project_working.iter())
                .cloned()
                .collect(),
        )
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
        let _ = self.bump_access(&accessed_ids);

        MemoryContextPayload {
            project_name: project_name.to_string(),
            project_profile: project_profile.map(|profile| profile.content),
            global_prompt: normalized_non_empty(&settings.global_prompt),
            extra_context: None,
            user_summary: user_summary.and_then(|summary| {
                trimmed_memory_text(
                    Some(&summary.content),
                    settings.memory.max_injected_summary_tokens,
                )
            }),
            user_core_fallback: unique_entries(user_core_fallback),
            project_core_fallback: unique_entries(project_core_fallback),
            user_working: unique_entries(user_working),
            project_working: unique_entries(project_working),
            user_working_limit: settings.memory.max_injected_user_working_memories,
            project_working_limit: settings.memory.max_injected_project_working_memories,
            memory_enabled: should_inject,
        }
    }

    fn current_summary(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        tool_id: Option<&str>,
    ) -> Result<Option<MemorySummary>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            r#"
            SELECT id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
            FROM memory_summaries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '')
              AND COALESCE(tool_id, '') = COALESCE(?3, '')
            LIMIT 1;
            "#,
        )?;
        let summary = statement
            .query_row(
                params![scope.as_str(), project_id, tool_id],
                memory_summary_from_row,
            )
            .optional()?
            .filter(|summary| valid_summary_content(&summary.content).is_some());
        Ok(summary)
    }

    fn list_entries(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        tool_id: Option<&str>,
        tiers: &[MemoryTier],
        limit: i64,
    ) -> Result<Vec<MemoryEntry>> {
        if tiers.is_empty() || limit <= 0 {
            return Ok(Vec::new());
        }
        let tier_values = tiers.iter().map(MemoryTier::as_str).collect::<Vec<_>>();
        let placeholders = tier_values
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            r#"
            SELECT {}
            FROM memory_entries
            WHERE scope = ?
              AND COALESCE(project_id, '') = COALESCE(?, '')
              AND (tool_id IS NULL OR tool_id = ?)
              AND tier IN ({})
              AND superseded_by IS NULL
              AND status = 'active'
            ORDER BY access_count DESC, updated_at DESC
            LIMIT ?;
            "#,
            entry_select_columns(),
            placeholders
        );
        let conn = self.connect()?;
        let mut statement = conn.prepare(&sql)?;
        let mut values = vec![
            SqlValue::Text(scope.as_str().to_string()),
            optional_text_value(project_id),
            optional_text_value(tool_id),
        ];
        values.extend(
            tier_values
                .iter()
                .map(|value| SqlValue::Text((*value).to_string())),
        );
        values.push(SqlValue::Integer(limit));
        let rows = statement.query_map(params_from_iter(values), memory_entry_from_row)?;
        Ok(rows.flatten().collect())
    }

    fn project_profile_memory_context(&self, project_id: &str) -> Result<Vec<String>> {
        let mut signals = Vec::new();
        if let Some(summary) = self.current_summary(MemoryScope::Project, Some(project_id), None)? {
            if let Some(content) = normalized_non_empty(&summary.content) {
                signals.push(format!(
                    "Project summary v{}: {}",
                    summary.version,
                    trim_memory_text(&content, 700)
                ));
            }
        }
        for entry in self.list_entries(
            MemoryScope::Project,
            Some(project_id),
            None,
            &[MemoryTier::Core, MemoryTier::Working],
            16,
        )? {
            signals.push(format!(
                "{} / {}: {}",
                entry.module_key.as_deref().unwrap_or(DEFAULT_MEMORY_MODULE),
                entry.kind.as_str(),
                trim_memory_text(&entry.content, 180)
            ));
        }
        Ok(signals.into_iter().take(18).collect())
    }

    fn list_entries_for_context(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        tool_id: Option<&str>,
        tiers: &[MemoryTier],
        limit: i64,
        query: &str,
    ) -> Result<Vec<MemoryEntry>> {
        if tiers.is_empty() || limit <= 0 {
            return Ok(Vec::new());
        }
        let candidate_limit = (limit * MEMORY_CONTEXT_CANDIDATE_FANOUT).max(limit).max(24);
        let mut candidates = self.list_entries_matching_query(
            scope.clone(),
            project_id,
            tool_id,
            tiers,
            candidate_limit,
            query,
        )?;
        if candidates.len() < limit as usize {
            candidates.extend(self.list_entries(
                scope,
                project_id,
                tool_id,
                tiers,
                candidate_limit,
            )?);
            candidates = unique_entries(candidates);
        }
        Ok(select_context_entries(
            candidates,
            tool_id,
            query,
            limit as usize,
        ))
    }

    fn list_entries_matching_query(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        tool_id: Option<&str>,
        tiers: &[MemoryTier],
        limit: i64,
        query: &str,
    ) -> Result<Vec<MemoryEntry>> {
        if tiers.is_empty() || limit <= 0 {
            return Ok(Vec::new());
        }
        let Some(match_query) = memory_fts_query(query) else {
            return Ok(Vec::new());
        };
        let tier_values = tiers.iter().map(MemoryTier::as_str).collect::<Vec<_>>();
        let placeholders = tier_values
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            r#"
            SELECT {}
            FROM memory_entries
            JOIN memory_fts ON memory_fts.rowid = memory_entries.rowid
            WHERE memory_fts MATCH ?
              AND scope = ?
              AND COALESCE(project_id, '') = COALESCE(?, '')
              AND (tool_id IS NULL OR tool_id = ?)
              AND tier IN ({})
              AND superseded_by IS NULL
              AND status = 'active'
            ORDER BY bm25(memory_fts), access_count DESC, updated_at DESC
            LIMIT ?;
            "#,
            qualified_entry_select_columns("memory_entries"),
            placeholders
        );
        let conn = self.connect()?;
        let mut statement = conn.prepare(&sql)?;
        let mut values = vec![
            SqlValue::Text(match_query),
            SqlValue::Text(scope.as_str().to_string()),
            optional_text_value(project_id),
            optional_text_value(tool_id),
        ];
        values.extend(
            tier_values
                .iter()
                .map(|value| SqlValue::Text((*value).to_string())),
        );
        values.push(SqlValue::Integer(limit));
        let rows = statement.query_map(params_from_iter(values), memory_entry_from_row)?;
        Ok(rows.flatten().collect())
    }

    fn list_entries_for_management(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        tier: Option<&MemoryTier>,
        status: Option<&MemoryEntryStatus>,
        limit: i64,
    ) -> Result<Vec<MemoryEntry>> {
        let mut clauses = vec![
            "scope = ?".to_string(),
            "COALESCE(project_id, '') = COALESCE(?, '')".to_string(),
        ];
        if tier.is_some() {
            clauses.push("tier = ?".to_string());
        }
        if status.is_some() {
            clauses.push("status = ?".to_string());
        }
        let sql = format!(
            r#"
            SELECT {}
            FROM memory_entries
            WHERE {}
            ORDER BY updated_at DESC, created_at DESC
            LIMIT ?;
            "#,
            entry_select_columns(),
            clauses.join(" AND ")
        );
        let tier_value = tier.map(MemoryTier::as_str);
        let status_value = status.map(MemoryEntryStatus::as_str);
        let mut values = vec![
            SqlValue::Text(scope.as_str().to_string()),
            optional_text_value(project_id),
        ];
        if let Some(value) = tier_value {
            values.push(SqlValue::Text(value.to_string()));
        }
        if let Some(value) = status_value {
            values.push(SqlValue::Text(value.to_string()));
        }
        values.push(SqlValue::Integer(limit));
        let conn = self.connect()?;
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values), memory_entry_from_row)?;
        Ok(rows.flatten().collect())
    }

    fn list_active_entries_for_management(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<MemoryEntry>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(&format!(
            r#"
            SELECT {}
            FROM memory_entries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '')
              AND tier IN ('core', 'working')
              AND status = 'active'
            ORDER BY updated_at DESC, created_at DESC
            LIMIT ?3;
            "#,
            entry_select_columns()
        ))?;
        let rows = statement.query_map(
            params![scope.as_str(), project_id, limit],
            memory_entry_from_row,
        )?;
        Ok(rows.flatten().collect())
    }

    fn list_history_entries_for_management(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<MemoryEntry>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(&format!(
            r#"
            SELECT {}
            FROM memory_entries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '')
              AND status IN ('archived', 'merged')
            ORDER BY updated_at DESC, created_at DESC
            LIMIT ?3;
            "#,
            entry_select_columns()
        ))?;
        let rows = statement.query_map(
            params![scope.as_str(), project_id, limit],
            memory_entry_from_row,
        )?;
        Ok(rows.flatten().collect())
    }

    fn list_summaries_for_management(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
    ) -> Result<Vec<MemorySummary>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            r#"
            SELECT id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
            FROM memory_summaries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '')
            ORDER BY updated_at DESC;
            "#,
        )?;
        let rows =
            statement.query_map(params![scope.as_str(), project_id], memory_summary_from_row)?;
        Ok(rows.flatten().collect())
    }

    fn memory_scope_overview(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
    ) -> Result<MemoryScopeOverview> {
        let conn = self.connect()?;
        let (active, archived, merged, entry_tokens, entry_updated): (
            i64,
            i64,
            i64,
            i64,
            Option<f64>,
        ) = conn.query_row(
            r#"
                SELECT
                    COALESCE(SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN status = 'archived' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN status = 'merged' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM((length(content) + length(COALESCE(rationale, '')) + 3) / 4), 0),
                    MAX(updated_at)
                FROM memory_entries
                WHERE scope = ?1
                  AND COALESCE(project_id, '') = COALESCE(?2, '');
                "#,
            params![scope.as_str(), project_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
        let (summary_count, summary_tokens, summary_updated): (i64, i64, Option<f64>) = conn
            .query_row(
                r#"
            SELECT COUNT(*), COALESCE(SUM(token_estimate), 0), MAX(updated_at)
            FROM memory_summaries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '');
            "#,
                params![scope.as_str(), project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        let (profile_count, profile_tokens, profile_updated): (i64, i64, Option<f64>) = match scope
        {
            MemoryScope::Project => conn.query_row(
                r#"
                SELECT COUNT(*), COALESCE(SUM((length(content) + 3) / 4), 0), MAX(updated_at)
                FROM memory_project_profiles
                WHERE project_id = ?1;
                "#,
                params![project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?,
            MemoryScope::User => (0, 0, None),
        };
        Ok(MemoryScopeOverview {
            active_entry_count: active,
            archived_entry_count: archived,
            merged_entry_count: merged,
            profile_count,
            summary_count,
            total_token_estimate: entry_tokens + summary_tokens + profile_tokens,
            updated_at: max_optional_f64(
                max_optional_f64(entry_updated, summary_updated),
                profile_updated,
            ),
        })
    }

    fn manager_target_rows(
        &self,
        projects: &[ProjectRecord],
    ) -> Result<Vec<MemoryManagerTargetRow>> {
        let project_by_id = projects
            .iter()
            .map(|project| (project.id.as_str(), project))
            .collect::<HashMap<_, _>>();
        let mut rows = Vec::new();
        let user_overview = self.memory_scope_overview(MemoryScope::User, None)?;
        rows.push(MemoryManagerTargetRow {
            id: "user".to_string(),
            scope: "user".to_string(),
            project_id: None,
            title: "User Memory".to_string(),
            subtitle: "Cross-project preferences".to_string(),
            count: user_overview.total_count(),
            updated_at: user_overview.updated_at,
            is_open_project: false,
        });

        for (project_id, overview) in self.project_overviews_for_management()? {
            let project = project_by_id.get(project_id.as_str()).copied();
            rows.push(MemoryManagerTargetRow {
                id: format!("project-{project_id}"),
                scope: "project".to_string(),
                project_id: Some(project_id.clone()),
                title: project
                    .map(|project| project.name.clone())
                    .unwrap_or_else(|| {
                        format!("Project {}", project_id.chars().take(8).collect::<String>())
                    }),
                subtitle: project
                    .map(|project| project.path.clone())
                    .unwrap_or_else(|| project_id.clone()),
                count: overview.total_count(),
                updated_at: overview.updated_at,
                is_open_project: project.is_some(),
            });
        }
        for project in projects {
            if rows.iter().any(|row| {
                row.scope == "project" && row.project_id.as_deref() == Some(project.id.as_str())
            }) {
                continue;
            }
            rows.push(MemoryManagerTargetRow {
                id: format!("project-{}", project.id),
                scope: "project".to_string(),
                project_id: Some(project.id.clone()),
                title: project.name.clone(),
                subtitle: project.path.clone(),
                count: 0,
                updated_at: None,
                is_open_project: true,
            });
        }
        Ok(rows)
    }

    fn project_overviews_for_management(&self) -> Result<Vec<(String, MemoryScopeOverview)>> {
        let conn = self.connect()?;
        let mut ids = HashSet::new();
        {
            let mut statement = conn.prepare(
                r#"
                SELECT DISTINCT project_id
                FROM memory_entries
                WHERE scope = 'project' AND project_id IS NOT NULL
                UNION
                SELECT DISTINCT project_id
                FROM memory_summaries
                WHERE scope = 'project' AND project_id IS NOT NULL
                UNION
                SELECT DISTINCT project_id
                FROM memory_project_profiles
                WHERE project_id IS NOT NULL;
            "#,
            )?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows.flatten() {
                ids.insert(row);
            }
        }
        let mut overviews = ids
            .into_iter()
            .filter_map(|project_id| {
                let overview = self
                    .memory_scope_overview(MemoryScope::Project, Some(project_id.as_str()))
                    .ok()?;
                (overview.total_count() > 0).then_some((project_id, overview))
            })
            .collect::<Vec<_>>();
        overviews.sort_by(|left, right| {
            right
                .1
                .updated_at
                .partial_cmp(&left.1.updated_at)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
        });
        Ok(overviews)
    }

    fn summary_by_id(&self, summary_id: &str) -> Result<MemorySummary> {
        let conn = self.connect()?;
        conn.query_row(
            r#"
            SELECT id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
            FROM memory_summaries
            WHERE id = ?1
            LIMIT 1;
            "#,
            params![summary_id],
            memory_summary_from_row,
        )
        .optional()?
        .ok_or_else(|| anyhow!("summary not found"))
    }

    fn bump_access(&self, entry_ids: &[String]) -> Result<()> {
        if entry_ids.is_empty() {
            return Ok(());
        }
        let now = now_seconds();
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        for id in entry_ids {
            tx.execute(
                r#"
                UPDATE memory_entries
                SET access_count = access_count + 1,
                    last_accessed_at = ?1
                WHERE id = ?2;
                "#,
                params![now, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn enqueue_session_if_ready(
        &self,
        memory_settings: &AIMemorySettings,
        projects: &[ProjectWorkspaceRecord],
        session: &AISessionSnapshot,
    ) -> Result<bool> {
        if session.state != "idle" || !session.has_completed_turn || session.was_interrupted {
            return Ok(false);
        }
        if memory_settings.extraction_idle_delay_seconds > 0
            && now_seconds() - session.updated_at
                < f64::from(memory_settings.extraction_idle_delay_seconds)
        {
            return Ok(false);
        }
        let Some(project) = memory_project_context(projects, session) else {
            return Ok(false);
        };
        let Some(source) = self.resolve_transcript_source(session, &project) else {
            return Ok(false);
        };
        let session_key = extraction_session_key(session);
        if let Ok(mut recent) = self.last_enqueued_at_by_session.lock() {
            if let Some(last) = recent.get(&session_key).copied() {
                if memory_settings.session_extraction_cooldown_seconds > 0
                    && now_seconds() - last
                        < f64::from(memory_settings.session_extraction_cooldown_seconds)
                {
                    return Ok(false);
                }
            }
            if self.enqueue_extraction_if_needed(
                &project.project_id,
                &project.workspace_path,
                &session.tool,
                &session_identifier(session),
                &source.location,
                &source.fingerprint,
                false,
            )? {
                recent.insert(session_key, now_seconds());
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn enqueue_session_for_manual_extraction(
        &self,
        projects: &[ProjectWorkspaceRecord],
        session: &AISessionSnapshot,
    ) -> Result<bool> {
        if session.state != "idle" || !session.has_completed_turn {
            return Ok(false);
        }
        let Some(project) = memory_project_context(projects, session) else {
            return Ok(false);
        };
        let Some(source) = self.resolve_transcript_source(session, &project) else {
            return Ok(false);
        };
        self.enqueue_extraction_if_needed(
            &project.project_id,
            &project.workspace_path,
            &session.tool,
            &session_identifier(session),
            &source.location,
            &source.fingerprint,
            false,
        )
    }

    fn manual_extraction_candidates(
        &self,
        memory_settings: &AIMemorySettings,
        projects: &[ProjectWorkspaceRecord],
        sessions: &[AISessionSnapshot],
    ) -> Vec<AISessionSnapshot> {
        let limit = memory_settings.max_index_sessions.max(1) as usize;
        let mut by_project: HashMap<String, Vec<AISessionSnapshot>> = HashMap::new();
        for session in sessions
            .iter()
            .filter(|session| session.state == "idle" && session.has_completed_turn)
        {
            let Some(project) = memory_project_context(projects, session) else {
                continue;
            };
            if self.resolve_transcript_source(session, &project).is_none() {
                continue;
            }
            by_project
                .entry(project.project_id)
                .or_default()
                .push(session.clone());
        }
        let mut candidates = Vec::new();
        for sessions in by_project.values_mut() {
            sessions.sort_by(|left, right| {
                right
                    .updated_at
                    .partial_cmp(&left.updated_at)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            sessions.truncate(limit);
            candidates.extend(sessions.iter().cloned());
        }
        candidates.sort_by(|left, right| {
            left.updated_at
                .partial_cmp(&right.updated_at)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates
    }

    fn manual_extraction_candidates_from_history(
        &self,
        memory_settings: &AIMemorySettings,
        projects: &[ProjectWorkspaceRecord],
        sessions: &[AISessionSummary],
    ) -> Vec<AISessionSnapshot> {
        let limit = memory_settings.max_index_sessions.max(1) as usize;
        let mut by_project: HashMap<String, Vec<AISessionSnapshot>> = HashMap::new();
        for summary in sessions.iter().filter(|session| {
            session.total_tokens + session.cached_input_tokens + session.request_count > 0
        }) {
            let Some(project) = memory_project_context_from_history(projects, summary) else {
                continue;
            };
            let Some(snapshot) = historical_session_snapshot(summary, &project) else {
                continue;
            };
            if self
                .resolve_transcript_source(&snapshot, &project)
                .is_none()
            {
                continue;
            }
            by_project
                .entry(project.project_id)
                .or_default()
                .push(snapshot);
        }
        let mut candidates = Vec::new();
        for sessions in by_project.values_mut() {
            sessions.sort_by(|left, right| {
                right
                    .updated_at
                    .partial_cmp(&left.updated_at)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            sessions.truncate(limit);
            candidates.extend(sessions.iter().cloned());
        }
        candidates.sort_by(|left, right| {
            left.updated_at
                .partial_cmp(&right.updated_at)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates
    }

    #[cfg(test)]
    fn test_history_snapshot_for_project(
        &self,
        projects: &[ProjectWorkspaceRecord],
        session: &AISessionSummary,
    ) -> Option<AISessionSnapshot> {
        let project = memory_project_context_from_history(projects, session)?;
        historical_session_snapshot(session, &project)
    }

    fn deduplicate_manual_candidates(
        &self,
        sessions: Vec<AISessionSnapshot>,
    ) -> Vec<AISessionSnapshot> {
        let mut seen = HashSet::new();
        let mut deduplicated = Vec::new();
        for session in sessions {
            let key = extraction_session_key(&session);
            if seen.insert(key) {
                deduplicated.push(session);
            }
        }
        deduplicated.sort_by(|left, right| {
            left.updated_at
                .partial_cmp(&right.updated_at)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        deduplicated
    }

    fn enqueue_extraction_if_needed(
        &self,
        project_id: &str,
        workspace_path: &str,
        tool: &str,
        session_id: &str,
        transcript_path: &str,
        source_fingerprint: &str,
        allow_retry_failed: bool,
    ) -> Result<bool> {
        let conn = self.connect()?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT status FROM memory_extraction_queue WHERE source_fingerprint = ?1 LIMIT 1;",
                params![source_fingerprint],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(status) = existing {
            if allow_retry_failed && status == "failed" {
                conn.execute(
                    r#"
                    UPDATE memory_extraction_queue
                    SET project_id = ?1,
                        tool = ?2,
                        session_id = ?3,
                        transcript_path = ?4,
                        workspace_path = ?5,
                        status = 'pending',
                        error = NULL,
                        enqueued_at = ?6
                    WHERE source_fingerprint = ?7;
                    "#,
                    params![
                        project_id,
                        tool,
                        session_id,
                        transcript_path,
                        workspace_path,
                        now_seconds(),
                        source_fingerprint
                    ],
                )?;
                return Ok(true);
            }
            return Ok(false);
        }
        conn.execute(
            r#"
            INSERT INTO memory_extraction_queue (
                id, project_id, tool, session_id, transcript_path, workspace_path, source_fingerprint, status, attempts, error, enqueued_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, NULL, ?8);
            "#,
            params![
                Uuid::new_v4().to_string(),
                project_id,
                tool,
                session_id,
                transcript_path,
                workspace_path,
                source_fingerprint,
                now_seconds()
            ],
        )?;
        Ok(true)
    }

    async fn process_queue(
        &self,
        settings: AISettings,
        projects: Vec<ProjectWorkspaceRecord>,
        on_status: Arc<dyn Fn(MemoryQueueStatusEvent) + Send + Sync>,
        delay_between_tasks: Option<Duration>,
    ) -> Result<()> {
        let started_at = Instant::now();
        if self
            .processing_queue
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            runtime_trace("memory", "process_queue skipped already_running=true");
            return Ok(());
        }
        let _guard = MemoryQueueProcessingGuard {
            flag: &self.processing_queue,
        };
        self.publish_queue_status(projects.as_slice(), Arc::as_ref(&on_status));
        let root_projects = root_projects_from_workspaces(&projects);
        runtime_trace(
            "memory",
            &format!("process_queue start roots={}", root_projects.len()),
        );
        let projects_by_id = projects
            .into_iter()
            .filter(|project| project.id == project.root_project_id)
            .map(|project| {
                (
                    project.root_project_id.clone(),
                    MemoryProjectContext::from_workspace(&project),
                )
            })
            .collect::<HashMap<_, _>>();
        while let Some(task) = self.next_pending_extraction_task()? {
            if self.cancel_requested.load(Ordering::Acquire) {
                append_memory_log("queue", "memory extraction queue cancelled");
                break;
            }
            if let Err(error) = self
                .process_task(
                    &settings,
                    &projects_by_id,
                    &root_projects,
                    task.clone(),
                    Arc::clone(&on_status),
                )
                .await
            {
                let _ = self.mark_extraction_task_failed(&task.id, &error.to_string());
                self.publish_queue_status_for_roots(&root_projects, Arc::as_ref(&on_status));
                if should_stop_memory_queue_after_error(&error) {
                    append_memory_log(
                        "queue",
                        &format!("memory extraction queue paused after provider error: {error}"),
                    );
                    break;
                }
            }
            if let Some(delay) = delay_between_tasks {
                if self.has_pending_extraction_task()? {
                    runtime_trace(
                        "memory",
                        &format!(
                            "process_queue delay_between_tasks seconds={}",
                            delay.as_secs()
                        ),
                    );
                    self.publish_queue_status_for_roots(&root_projects, Arc::as_ref(&on_status));
                    tokio::time::sleep(delay).await;
                }
            }
        }
        self.cancel_requested.store(false, Ordering::Release);
        self.publish_queue_status_for_roots(&root_projects, Arc::as_ref(&on_status));
        runtime_trace_elapsed("memory", "process_queue finish", started_at, "");
        Ok(())
    }

    async fn process_task(
        &self,
        settings: &AISettings,
        projects_by_id: &HashMap<String, MemoryProjectContext>,
        root_projects: &[ProjectRecord],
        task: MemoryExtractionTask,
        on_status: Arc<dyn Fn(MemoryQueueStatusEvent) + Send + Sync>,
    ) -> Result<()> {
        let started_at = Instant::now();
        self.mark_extraction_task_running(&task.id)?;
        self.publish_queue_status_for_roots(root_projects, Arc::as_ref(&on_status));
        let Some(project) = projects_by_id.get(&task.project_id) else {
            self.mark_extraction_task_done(&task.id)?;
            self.publish_queue_status_for_roots(root_projects, Arc::as_ref(&on_status));
            return Ok(());
        };
        let provider = select_memory_provider(settings, Some(&task.tool))
            .cloned()
            .ok_or_else(|| anyhow!("No available AI provider is configured."))?;
        append_memory_log(
            "provider",
            &format!(
                "memory extraction provider={} id={} kind={} model={} base_url={} task={} session={}",
                provider.display_name,
                provider.id,
                provider.kind,
                provider.model,
                provider.base_url,
                task.id,
                task.session_id
            ),
        );
        let transcript = self.resolve_transcript_for_task(&task, project)?;
        let transcript_chars = transcript.chars().count();
        let user_summary = self
            .current_summary(MemoryScope::User, None, None)
            .ok()
            .flatten();
        let user_memories = self
            .list_entries_for_context(
                MemoryScope::User,
                None,
                None,
                &[MemoryTier::Working],
                i64::from(settings.memory.max_injected_user_working_memories),
                &transcript,
            )
            .unwrap_or_default();
        let project_memories = self
            .list_entries_for_context(
                MemoryScope::Project,
                Some(&project.project_id),
                None,
                &[MemoryTier::Working],
                i64::from(settings.memory.max_injected_project_working_memories),
                &transcript,
            )
            .unwrap_or_default();
        let prompt = make_extraction_prompt(
            &transcript,
            user_summary.as_ref(),
            &user_memories,
            &project_memories,
            &project.project_name,
            &settings.memory,
        );
        append_memory_log(
            "request",
            &format!(
                "memory extraction request task={} provider={} model={} prompt_chars={} transcript_chars={}",
                task.id,
                provider_summary(&provider),
                provider.model,
                prompt.chars().count(),
                transcript_chars
            ),
        );
        let response_text = llm::complete_with_provider_options(
            &provider,
            &prompt,
            Some(extraction_system_prompt()),
            llm::LLMProviderCompletionOptions {
                max_tokens: 4096,
                temperature: 0.1,
                preserve_formatting: true,
                json_response: true,
                timeout_seconds: 120,
            },
        )
        .await
        .map_err(|error| {
            let message = format!("{} failed: {}", provider_summary(&provider), error);
            append_memory_log("error", &message);
            anyhow!(message)
        })?;
        append_memory_log(
            "response",
            &format!(
                "memory extraction response task={} response_chars={}",
                task.id,
                response_text.chars().count()
            ),
        );
        let response = decode_extraction_response(&response_text)?;
        let project_profile_refresh_recommended = response.project_profile_refresh_recommended;
        self.apply_extraction_response(response, &task, &settings.memory)?;
        if project_profile_refresh_recommended {
            append_memory_log(
                "project-profile",
                &format!(
                    "memory extraction recommended project profile refresh project={} task={}",
                    project.project_id, task.id
                ),
            );
            let project_record = ProjectRecord {
                id: project.project_id.clone(),
                name: project.project_name.clone(),
                path: project.workspace_path.clone(),
                badge_text: None,
                badge_symbol: None,
                badge_color_hex: None,
                git_default_push_remote_name: None,
            };
            let _ = self
                .force_refresh_project_profile_with_llm_detailed(settings, &project_record)
                .await;
        }
        self.mark_extraction_task_done(&task.id)?;
        self.publish_queue_status_for_roots(root_projects, Arc::as_ref(&on_status));
        runtime_trace_elapsed(
            "memory",
            "process_task",
            started_at,
            &format!(
                "task={} project={} session={} transcript_chars={}",
                task.id, task.project_id, task.session_id, transcript_chars
            ),
        );
        Ok(())
    }

    fn next_pending_extraction_task(&self) -> Result<Option<MemoryExtractionTask>> {
        let conn = self.connect()?;
        conn.query_row(
            r#"
            SELECT id, project_id, tool, session_id, transcript_path, workspace_path, source_fingerprint, status, attempts, error, enqueued_at
            FROM memory_extraction_queue
            WHERE status = 'pending'
            ORDER BY enqueued_at ASC
            LIMIT 1;
            "#,
            [],
            memory_task_from_row,
        )
        .optional()
        .map_err(Into::into)
    }

    fn has_pending_extraction_task(&self) -> Result<bool> {
        let conn = self.connect()?;
        let count = scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM memory_extraction_queue WHERE status = 'pending';",
        )?;
        Ok(count > 0)
    }

    fn mark_extraction_task_running(&self, task_id: &str) -> Result<()> {
        self.update_task_status(task_id, "running", None, true)
    }

    fn mark_extraction_task_done(&self, task_id: &str) -> Result<()> {
        self.update_task_status(task_id, "done", None, false)
    }

    fn mark_extraction_task_failed(&self, task_id: &str, error: &str) -> Result<()> {
        self.update_task_status(task_id, "failed", Some(error), false)
    }

    fn update_task_status(
        &self,
        task_id: &str,
        status: &str,
        error: Option<&str>,
        increment_attempts: bool,
    ) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            UPDATE memory_extraction_queue
            SET status = ?1,
                attempts = attempts + ?2,
                error = ?3
            WHERE id = ?4;
            "#,
            params![
                status,
                if increment_attempts { 1_i64 } else { 0_i64 },
                error,
                task_id
            ],
        )?;
        match (status, error) {
            ("failed", Some(message)) => self.record_recent_failure(message),
            _ => {}
        }
        Ok(())
    }

    fn record_recent_failure(&self, message: &str) {
        if let Ok(mut failure) = self.recent_failure.lock() {
            *failure = Some(RecentMemoryFailure {
                message: message.to_string(),
                occurred_at: now_seconds(),
            });
        }
    }

    fn clear_recent_failure(&self) {
        if let Ok(mut failure) = self.recent_failure.lock() {
            *failure = None;
        }
    }

    fn current_recent_failure(&self) -> Option<RecentMemoryFailure> {
        let now = now_seconds();
        let mut failure = self.recent_failure.lock().ok()?;
        if failure
            .as_ref()
            .is_some_and(|failure| now - failure.occurred_at > RECENT_MEMORY_FAILURE_TTL_SECONDS)
        {
            *failure = None;
        }
        failure.clone()
    }

    fn apply_extraction_response(
        &self,
        response: MemoryExtractionResponse,
        task: &MemoryExtractionTask,
        settings: &AIMemorySettings,
    ) -> Result<()> {
        for item in response.working_add {
            let Some(content) = normalized_non_empty(&item.content) else {
                continue;
            };
            if let Some(reason) = normalized_non_empty(item.skip_reason.as_deref().unwrap_or("")) {
                self.record_memory_decision(MemoryDecisionLog {
                    kind: MemoryWriteDecisionKind::Skip,
                    entry_id: None,
                    target_entry_id: None,
                    reason,
                    created_at: now_seconds(),
                })?;
                continue;
            }
            let scope = item.scope.unwrap_or(MemoryScope::Project);
            let project_id = (scope == MemoryScope::Project).then(|| task.project_id.clone());
            let explicit_decision = if let Some(target_entry_id) = item.replace {
                Some(MemoryWriteDecision {
                    kind: MemoryWriteDecisionKind::Replace,
                    target_entry_id: Some(target_entry_id),
                    reason: "provider marked this memory as replacing an existing entry"
                        .to_string(),
                })
            } else {
                item.merge_with
                    .first()
                    .cloned()
                    .map(|target_entry_id| MemoryWriteDecision {
                        kind: MemoryWriteDecisionKind::Merge,
                        target_entry_id: Some(target_entry_id),
                        reason: "provider marked this memory as a semantic merge".to_string(),
                    })
            };
            let archive_ids = item
                .archive
                .iter()
                .chain(item.merge_with.iter().skip(1))
                .cloned()
                .collect::<Vec<_>>();
            for archive_id in &archive_ids {
                self.archive_entries(&[archive_id.clone()])?;
                self.record_memory_decision(MemoryDecisionLog {
                    kind: MemoryWriteDecisionKind::Archive,
                    entry_id: None,
                    target_entry_id: Some(archive_id.clone()),
                    reason: "provider marked existing memory as stale or duplicate".to_string(),
                    created_at: now_seconds(),
                })?;
            }
            let _ = self.write_candidate_with_decision(
                MemoryCandidate {
                    scope,
                    project_id,
                    tool_id: None,
                    module_key: item
                        .module_key
                        .or_else(|| Some(DEFAULT_MEMORY_MODULE.to_string())),
                    tier: item.tier.unwrap_or(MemoryTier::Working),
                    kind: item.kind,
                    content,
                    rationale: item
                        .rationale
                        .and_then(|value| normalized_non_empty(&value)),
                    source_tool: Some(task.tool.clone()),
                    source_session_id: Some(task.session_id.clone()),
                    source_fingerprint: Some(task.source_fingerprint.clone()),
                },
                explicit_decision,
            )?;
        }

        let merged_ids = response
            .merged_entry_ids
            .iter()
            .filter_map(|value| parse_uuid_string(value))
            .collect::<Vec<_>>();

        if let Some(content) = valid_summary_content(response.user_summary.as_deref().unwrap_or(""))
        {
            let summary = self.upsert_summary(
                MemoryScope::User,
                None,
                None,
                &content,
                &merged_ids,
                settings.max_summary_versions,
            )?;
            self.mark_entries_merged(&merged_ids, &summary.id)?;
            self.merge_stale_working_entries(
                MemoryScope::User,
                None,
                settings.max_active_working_entries,
                &summary.id,
            )?;
        }
        let archive_ids = response
            .working_archive
            .iter()
            .filter_map(|value| parse_uuid_string(value))
            .collect::<Vec<_>>();
        self.archive_entries(&archive_ids)?;
        self.trim_working_entries(MemoryScope::User, None, settings.max_active_working_entries)?;
        self.trim_working_entries(
            MemoryScope::Project,
            Some(&task.project_id),
            settings.max_active_working_entries,
        )?;
        Ok(())
    }

    fn write_candidate_with_decision(
        &self,
        candidate: MemoryCandidate,
        explicit_decision: Option<MemoryWriteDecision>,
    ) -> Result<Option<MemoryEntry>> {
        let decision = explicit_decision
            .or_else(|| self.decide_memory_write(&candidate).ok().flatten())
            .unwrap_or_else(|| MemoryWriteDecision {
                kind: MemoryWriteDecisionKind::Create,
                target_entry_id: None,
                reason: "new durable memory".to_string(),
            });
        match decision.kind {
            MemoryWriteDecisionKind::Skip => {
                self.record_memory_decision(MemoryDecisionLog {
                    kind: MemoryWriteDecisionKind::Skip,
                    entry_id: None,
                    target_entry_id: decision.target_entry_id,
                    reason: decision.reason,
                    created_at: now_seconds(),
                })?;
                Ok(None)
            }
            MemoryWriteDecisionKind::Archive => {
                if let Some(target_entry_id) = decision.target_entry_id.as_deref() {
                    self.archive_entries(&[target_entry_id.to_string()])?;
                }
                self.record_memory_decision(MemoryDecisionLog {
                    kind: MemoryWriteDecisionKind::Archive,
                    entry_id: None,
                    target_entry_id: decision.target_entry_id,
                    reason: decision.reason,
                    created_at: now_seconds(),
                })?;
                Ok(None)
            }
            MemoryWriteDecisionKind::Merge => {
                if let Some(target_entry_id) = decision.target_entry_id.as_deref() {
                    let entry = self.merge_candidate_into_entry(target_entry_id, candidate)?;
                    self.record_memory_decision(MemoryDecisionLog {
                        kind: MemoryWriteDecisionKind::Merge,
                        entry_id: Some(entry.id.clone()),
                        target_entry_id: Some(target_entry_id.to_string()),
                        reason: decision.reason,
                        created_at: now_seconds(),
                    })?;
                    Ok(Some(entry))
                } else {
                    let entry = self.upsert(candidate)?;
                    self.record_memory_decision(MemoryDecisionLog {
                        kind: MemoryWriteDecisionKind::Create,
                        entry_id: Some(entry.id.clone()),
                        target_entry_id: None,
                        reason: "merge decision had no target; created memory".to_string(),
                        created_at: now_seconds(),
                    })?;
                    Ok(Some(entry))
                }
            }
            MemoryWriteDecisionKind::Replace => {
                let target_entry_id = decision.target_entry_id.clone();
                let entry = self.upsert(candidate)?;
                if let Some(target_entry_id) = target_entry_id.as_deref() {
                    self.supersede_entry(target_entry_id, &entry.id)?;
                }
                self.record_memory_decision(MemoryDecisionLog {
                    kind: MemoryWriteDecisionKind::Replace,
                    entry_id: Some(entry.id.clone()),
                    target_entry_id,
                    reason: decision.reason,
                    created_at: now_seconds(),
                })?;
                Ok(Some(entry))
            }
            MemoryWriteDecisionKind::Create => {
                let entry = self.upsert(candidate)?;
                self.record_memory_decision(MemoryDecisionLog {
                    kind: MemoryWriteDecisionKind::Create,
                    entry_id: Some(entry.id.clone()),
                    target_entry_id: None,
                    reason: decision.reason,
                    created_at: now_seconds(),
                })?;
                Ok(Some(entry))
            }
        }
    }

    fn decide_memory_write(
        &self,
        candidate: &MemoryCandidate,
    ) -> Result<Option<MemoryWriteDecision>> {
        if should_skip_memory_candidate(candidate) {
            return Ok(Some(MemoryWriteDecision {
                kind: MemoryWriteDecisionKind::Skip,
                target_entry_id: None,
                reason: "candidate is too short or low signal".to_string(),
            }));
        }
        let candidates = self.write_decision_candidates(candidate)?;
        let Some(best) = candidates
            .iter()
            .map(|entry| (memory_similarity(&candidate.content, &entry.content), entry))
            .max_by(|left, right| {
                left.0
                    .partial_cmp(&right.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        else {
            return Ok(None);
        };
        if best.0 >= MEMORY_MERGE_SIMILARITY_THRESHOLD {
            return Ok(Some(MemoryWriteDecision {
                kind: MemoryWriteDecisionKind::Merge,
                target_entry_id: Some(best.1.id.clone()),
                reason: format!("semantic duplicate score {:.2}", best.0),
            }));
        }
        if best.0 >= MEMORY_REPLACE_SIMILARITY_THRESHOLD
            && memory_candidate_conflicts(candidate, best.1)
        {
            return Ok(Some(MemoryWriteDecision {
                kind: MemoryWriteDecisionKind::Replace,
                target_entry_id: Some(best.1.id.clone()),
                reason: format!("conflicting newer memory score {:.2}", best.0),
            }));
        }
        Ok(None)
    }

    fn write_decision_candidates(&self, candidate: &MemoryCandidate) -> Result<Vec<MemoryEntry>> {
        let mut entries = self.list_entries_matching_query(
            candidate.scope.clone(),
            candidate.project_id.as_deref(),
            candidate.tool_id.as_deref(),
            &[MemoryTier::Core, MemoryTier::Working],
            MEMORY_WRITE_CANDIDATE_LIMIT,
            &candidate.content,
        )?;
        if entries.len() < MEMORY_WRITE_CANDIDATE_LIMIT as usize {
            entries.extend(self.list_entries(
                candidate.scope.clone(),
                candidate.project_id.as_deref(),
                candidate.tool_id.as_deref(),
                &[MemoryTier::Core, MemoryTier::Working],
                MEMORY_WRITE_CANDIDATE_LIMIT,
            )?);
            entries = unique_entries(entries);
        }
        entries.retain(|entry| {
            entry.module_key.as_deref() == candidate.module_key.as_deref()
                && entry.kind == candidate.kind
        });
        Ok(entries)
    }

    fn merge_candidate_into_entry(
        &self,
        entry_id: &str,
        candidate: MemoryCandidate,
    ) -> Result<MemoryEntry> {
        let conn = self.connect()?;
        let existing = conn
            .query_row(
                &format!(
                    r#"
                    SELECT {}
                    FROM memory_entries
                    WHERE id = ?1 AND status = 'active' AND superseded_by IS NULL
                    LIMIT 1;
                    "#,
                    entry_select_columns()
                ),
                params![entry_id],
                memory_entry_from_row,
            )
            .optional()?;
        let Some(mut entry) = existing else {
            return self.upsert(candidate);
        };
        let content = merge_memory_content(&entry.content, &candidate.content);
        let rationale =
            merge_optional_memory_text(entry.rationale.as_deref(), candidate.rationale.as_deref());
        let normalized_hash = sha256_hex(&normalized_memory_content(&content));
        let tier = preferred_tier(&entry.tier, &candidate.tier);
        let now = now_seconds();
        conn.execute(
            r#"
            UPDATE memory_entries
            SET tier = ?1, kind = ?2, content = ?3, rationale = ?4, source_tool = ?5,
                source_session_id = ?6, source_fingerprint = ?7, normalized_hash = ?8,
                status = 'active', merged_summary_id = NULL, merged_at = NULL, archived_at = NULL,
                updated_at = ?9
            WHERE id = ?10;
            "#,
            params![
                tier.as_str(),
                candidate.kind.as_str(),
                content,
                rationale,
                candidate.source_tool,
                candidate.source_session_id,
                candidate.source_fingerprint,
                normalized_hash,
                now,
                entry.id
            ],
        )?;
        entry.tier = tier;
        entry.kind = candidate.kind;
        entry.content = content;
        entry.rationale = rationale;
        entry.source_tool = candidate.source_tool;
        entry.source_session_id = candidate.source_session_id;
        entry.source_fingerprint = candidate.source_fingerprint;
        entry.normalized_hash = normalized_hash;
        entry.status = MemoryEntryStatus::Active;
        entry.merged_summary_id = None;
        entry.merged_at = None;
        entry.archived_at = None;
        entry.updated_at = now;
        Ok(entry)
    }

    fn supersede_entry(&self, old_entry_id: &str, new_entry_id: &str) -> Result<()> {
        if old_entry_id == new_entry_id {
            return Ok(());
        }
        let now = now_seconds();
        let conn = self.connect()?;
        conn.execute(
            r#"
            UPDATE memory_entries
            SET superseded_by = ?1, status = 'archived', archived_at = ?2, updated_at = ?2
            WHERE id = ?3 AND status = 'active';
            "#,
            params![new_entry_id, now, old_entry_id],
        )?;
        Ok(())
    }

    fn record_memory_decision(&self, decision: MemoryDecisionLog) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO memory_decision_logs (
                id, decision, entry_id, target_entry_id, reason, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6);
            "#,
            params![
                Uuid::new_v4().to_string(),
                decision.kind.as_str(),
                decision.entry_id,
                decision.target_entry_id,
                decision.reason,
                decision.created_at,
            ],
        )?;
        Ok(())
    }

    fn attach_last_decisions(&self, entries: &mut [MemoryEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let conn = self.connect()?;
        for entry in entries {
            entry.last_decision = conn
                .query_row(
                    r#"
                    SELECT decision, entry_id, target_entry_id, reason, created_at
                    FROM memory_decision_logs
                    WHERE entry_id = ?1 OR target_entry_id = ?1
                    ORDER BY created_at DESC
                    LIMIT 1;
                    "#,
                    params![entry.id],
                    memory_decision_log_from_row,
                )
                .optional()?;
        }
        Ok(())
    }

    fn upsert(&self, candidate: MemoryCandidate) -> Result<MemoryEntry> {
        let normalized_content = normalized_memory_content(&candidate.content);
        let normalized_hash = sha256_hex(&normalized_content);
        let conn = self.connect()?;
        let existing = conn
            .query_row(
                &format!(
                    r#"
                    SELECT {}
                    FROM memory_entries
                    WHERE scope = ?1
                      AND COALESCE(project_id, '') = COALESCE(?2, '')
                      AND COALESCE(tool_id, '') = COALESCE(?3, '')
                      AND COALESCE(module_key, '') = COALESCE(?4, '')
                      AND normalized_hash = ?5
                    LIMIT 1;
                    "#,
                    entry_select_columns()
                ),
                params![
                    candidate.scope.as_str(),
                    candidate.project_id.as_deref(),
                    candidate.tool_id.as_deref(),
                    candidate.module_key.as_deref(),
                    normalized_hash
                ],
                memory_entry_from_row,
            )
            .optional()?;
        let now = now_seconds();
        if let Some(mut entry) = existing {
            let tier = preferred_tier(&entry.tier, &candidate.tier);
            conn.execute(
                r#"
                UPDATE memory_entries
                SET tier = ?1, kind = ?2, content = ?3, rationale = ?4, source_tool = ?5,
                    source_session_id = ?6, source_fingerprint = ?7, module_key = ?8, status = 'active',
                    merged_summary_id = NULL, merged_at = NULL, archived_at = NULL, updated_at = ?9
                WHERE id = ?10;
                "#,
                params![
                    tier.as_str(),
                    candidate.kind.as_str(),
                    candidate.content,
                    candidate.rationale,
                    candidate.source_tool,
                    candidate.source_session_id,
                    candidate.source_fingerprint,
                    candidate.module_key,
                    now,
                    entry.id
                ],
            )?;
            entry.tier = tier;
            entry.kind = candidate.kind;
            entry.content = candidate.content;
            entry.module_key = candidate.module_key;
            entry.status = MemoryEntryStatus::Active;
            entry.updated_at = now;
            return Ok(entry);
        }

        let entry = MemoryEntry {
            id: Uuid::new_v4().to_string(),
            scope: candidate.scope,
            project_id: candidate.project_id,
            tool_id: candidate.tool_id,
            module_key: candidate.module_key,
            tier: candidate.tier,
            kind: candidate.kind,
            content: candidate.content,
            rationale: candidate.rationale,
            source_tool: candidate.source_tool,
            source_session_id: candidate.source_session_id,
            source_fingerprint: candidate.source_fingerprint,
            normalized_hash,
            superseded_by: None,
            status: MemoryEntryStatus::Active,
            merged_summary_id: None,
            merged_at: None,
            archived_at: None,
            access_count: 0,
            last_accessed_at: None,
            created_at: now,
            updated_at: now,
            last_decision: None,
        };
        conn.execute(
            r#"
            INSERT INTO memory_entries (
                id, scope, project_id, tool_id, module_key, tier, kind, content, rationale, source_tool, source_session_id,
                source_fingerprint, normalized_hash, superseded_by, status, merged_summary_id, merged_at, archived_at,
                access_count, last_accessed_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22);
            "#,
            params![
                entry.id,
                entry.scope.as_str(),
                entry.project_id,
                entry.tool_id,
                entry.module_key,
                entry.tier.as_str(),
                entry.kind.as_str(),
                entry.content,
                entry.rationale,
                entry.source_tool,
                entry.source_session_id,
                entry.source_fingerprint,
                entry.normalized_hash,
                entry.superseded_by,
                entry.status.as_str(),
                entry.merged_summary_id,
                entry.merged_at,
                entry.archived_at,
                entry.access_count,
                entry.last_accessed_at,
                entry.created_at,
                entry.updated_at,
            ],
        )?;
        Ok(entry)
    }

    fn upsert_summary(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        tool_id: Option<&str>,
        content: &str,
        source_entry_ids: &[String],
        max_versions: i32,
    ) -> Result<MemorySummary> {
        let content = content.trim();
        if content.is_empty() {
            return Err(anyhow!("summary content cannot be empty"));
        }
        let source_ids = sorted_unique(source_entry_ids);
        let source_json = serde_json::to_string(&source_ids)?;
        let now = now_seconds();
        let conn = self.connect()?;
        let existing = conn
            .query_row(
                r#"
                SELECT id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
                FROM memory_summaries
                WHERE scope = ?1
                  AND COALESCE(project_id, '') = COALESCE(?2, '')
                  AND COALESCE(tool_id, '') = COALESCE(?3, '')
                LIMIT 1;
                "#,
                params![scope.as_str(), project_id, tool_id],
                memory_summary_from_row,
            )
            .optional()?;
        let token_estimate = estimate_tokens(content);
        if let Some(existing) = existing {
            let version = existing.version + 1;
            conn.execute(
                r#"
                UPDATE memory_summaries
                SET content = ?1, version = ?2, source_entry_ids = ?3, token_estimate = ?4, updated_at = ?5
                WHERE id = ?6;
                "#,
                params![content, version, source_json, token_estimate, now, existing.id],
            )?;
            self.insert_summary_version(&existing.id, version, content, &source_ids, now)?;
            self.trim_summary_versions(&existing.id, max_versions)?;
            return Ok(MemorySummary {
                id: existing.id,
                scope,
                project_id: project_id.map(str::to_string),
                tool_id: tool_id.map(str::to_string),
                content: content.to_string(),
                version,
                source_entry_ids: source_ids,
                token_estimate,
                created_at: existing.created_at,
                updated_at: now,
            });
        }

        let summary = MemorySummary {
            id: Uuid::new_v4().to_string(),
            scope,
            project_id: project_id.map(str::to_string),
            tool_id: tool_id.map(str::to_string),
            content: content.to_string(),
            version: 1,
            source_entry_ids: source_ids,
            token_estimate,
            created_at: now,
            updated_at: now,
        };
        let source_json = serde_json::to_string(&summary.source_entry_ids)?;
        conn.execute(
            r#"
            INSERT INTO memory_summaries (
                id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);
            "#,
            params![
                summary.id,
                summary.scope.as_str(),
                summary.project_id,
                summary.tool_id,
                summary.content,
                summary.version,
                source_json,
                summary.token_estimate,
                summary.created_at,
                summary.updated_at
            ],
        )?;
        self.insert_summary_version(
            &summary.id,
            summary.version,
            &summary.content,
            &summary.source_entry_ids,
            now,
        )?;
        self.trim_summary_versions(&summary.id, max_versions)?;
        Ok(summary)
    }

    fn insert_summary_version(
        &self,
        summary_id: &str,
        version: i64,
        content: &str,
        source_ids: &[String],
        created_at: f64,
    ) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO memory_summary_versions (
                id, summary_id, version, content, source_entry_ids, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6);
            "#,
            params![
                Uuid::new_v4().to_string(),
                summary_id,
                version,
                content,
                serde_json::to_string(source_ids)?,
                created_at
            ],
        )?;
        Ok(())
    }

    fn trim_summary_versions(&self, summary_id: &str, max_versions: i32) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            DELETE FROM memory_summary_versions
            WHERE summary_id = ?1
              AND id NOT IN (
                SELECT id
                FROM memory_summary_versions
                WHERE summary_id = ?1
                ORDER BY version DESC
                LIMIT ?2
              );
            "#,
            params![summary_id, max_versions.max(1)],
        )?;
        Ok(())
    }

    fn mark_entries_merged(&self, entry_ids: &[String], summary_id: &str) -> Result<()> {
        if entry_ids.is_empty() {
            return Ok(());
        }
        let now = now_seconds();
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        for id in entry_ids {
            tx.execute(
                r#"
                UPDATE memory_entries
                SET status = 'merged', merged_summary_id = ?1, merged_at = ?2, updated_at = ?2
                WHERE id = ?3 AND status = 'active' AND tier = 'working';
                "#,
                params![summary_id, now, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn merge_stale_working_entries(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        max_active: i32,
        summary_id: &str,
    ) -> Result<()> {
        let ids = self.stale_working_entry_ids(scope, project_id, max_active)?;
        self.mark_entries_merged(&ids, summary_id)
    }

    fn trim_working_entries(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        max_active: i32,
    ) -> Result<()> {
        let ids = self.stale_working_entry_ids(scope, project_id, max_active)?;
        self.archive_entries(&ids)
    }

    fn stale_working_entry_ids(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        max_active: i32,
    ) -> Result<Vec<String>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            r#"
            SELECT id
            FROM memory_entries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '')
              AND tier = 'working'
              AND status = 'active'
            ORDER BY updated_at DESC
            LIMIT -1 OFFSET ?3;
            "#,
        )?;
        let rows = statement.query_map(
            params![scope.as_str(), project_id, i64::from(max_active.max(0))],
            |row| row.get(0),
        )?;
        Ok(rows.flatten().collect())
    }

    fn archive_entries(&self, entry_ids: &[String]) -> Result<()> {
        if entry_ids.is_empty() {
            return Ok(());
        }
        let now = now_seconds();
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        for id in entry_ids {
            tx.execute(
                r#"
                UPDATE memory_entries
                SET tier = 'archive', status = 'archived', archived_at = ?1, updated_at = ?1
                WHERE id = ?2;
                "#,
                params![now, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn resolve_transcript_source(
        &self,
        session: &AISessionSnapshot,
        project: &MemoryProjectContext,
    ) -> Option<TranscriptSource> {
        let tool = normalized_non_empty(&session.tool)?.to_lowercase();
        let session_id = session_identifier(session);

        if let Some(path) = session
            .transcript_path
            .as_deref()
            .and_then(normalized_non_empty)
            .and_then(|path| transcript_source_if_readable(&path, &tool, &session_id, false))
        {
            return Some(path);
        }

        match tool.as_str() {
            "claude" => {
                let ai_session = normalized_non_empty(session.ai_session_id.as_deref()?)?;
                claude_project_log_paths(&project.workspace_path)
                    .into_iter()
                    .find(|path| {
                        claude_log_contains_session(path, &ai_session, &project.workspace_path)
                    })
                    .and_then(|path| {
                        transcript_source_if_readable(
                            &path.display().to_string(),
                            &tool,
                            &ai_session,
                            false,
                        )
                    })
            }
            "codex" => {
                let ai_session = normalized_non_empty(session.ai_session_id.as_deref()?)?;
                find_codex_rollout_path(&project.workspace_path, &ai_session).and_then(|path| {
                    transcript_source_if_readable(
                        &path.display().to_string(),
                        &tool,
                        &ai_session,
                        false,
                    )
                })
            }
            "gemini" => {
                let files = gemini_session_paths(&project.workspace_path);
                let matching = session
                    .ai_session_id
                    .as_deref()
                    .and_then(normalized_non_empty)
                    .and_then(|ai_session| {
                        files.iter().find(|path| {
                            path.file_name()
                                .and_then(|value| value.to_str())
                                .map(|name| name.contains(&ai_session))
                                .unwrap_or(false)
                        })
                    })
                    .cloned()
                    .or_else(|| files.first().cloned());
                matching.and_then(|path| {
                    transcript_source_if_readable(
                        &path.display().to_string(),
                        &tool,
                        &session_id,
                        false,
                    )
                })
            }
            "opencode" => transcript_source_if_readable(
                &opencode_database_path().display().to_string(),
                &tool,
                &session_id,
                true,
            ),
            _ => None,
        }
    }

    fn resolve_transcript_for_task(
        &self,
        task: &MemoryExtractionTask,
        project: &MemoryProjectContext,
    ) -> Result<String> {
        let workspace_path = task
            .workspace_path
            .as_deref()
            .and_then(normalized_non_empty)
            .unwrap_or_else(|| project.workspace_path.clone());
        let tool = task.tool.to_lowercase();
        if Path::new(&task.transcript_path).is_file() {
            if tool == "opencode" && task.transcript_path.ends_with(".db") {
                if let Some(text) = fetch_opencode_transcript(
                    &workspace_path,
                    &task.session_id,
                    &task.transcript_path,
                ) {
                    return Ok(text);
                }
            } else if let Some(text) = read_transcript_file(&task.transcript_path, 80, 8000) {
                return Ok(text);
            }
        }
        match tool.as_str() {
            "claude" => {
                for path in claude_project_log_paths(&workspace_path) {
                    if let Some(text) = read_transcript_file(&path.display().to_string(), 80, 8000)
                    {
                        return Ok(text);
                    }
                }
            }
            "codex" => {
                if let Some(path) = find_codex_rollout_path(&workspace_path, &task.session_id) {
                    if let Some(text) = read_transcript_file(&path.display().to_string(), 80, 8000)
                    {
                        return Ok(text);
                    }
                }
            }
            "gemini" => {
                for path in gemini_session_paths(&workspace_path) {
                    if let Some(text) = read_transcript_file(&path.display().to_string(), 80, 8000)
                    {
                        return Ok(text);
                    }
                }
            }
            "opencode" => {
                if let Some(text) = fetch_opencode_transcript(
                    &workspace_path,
                    &task.session_id,
                    &opencode_database_path().display().to_string(),
                ) {
                    return Ok(text);
                }
            }
            _ => {}
        }
        Err(anyhow!(
            "Unable to resolve transcript for memory extraction."
        ))
    }

    fn publish_queue_status(
        &self,
        projects: &[ProjectWorkspaceRecord],
        on_status: &(dyn Fn(MemoryQueueStatusEvent) + Send + Sync),
    ) {
        self.publish_queue_status_for_roots(&root_projects_from_workspaces(projects), on_status);
    }

    fn publish_queue_status_for_roots(
        &self,
        projects: &[ProjectRecord],
        on_status: &(dyn Fn(MemoryQueueStatusEvent) + Send + Sync),
    ) {
        if let Ok(status) = self.extraction_status_snapshot() {
            let manager = self
                .manager_snapshot(
                    MemoryManagerSnapshotRequest {
                        scope: "user".to_string(),
                        project_id: None,
                        tab: "summary".to_string(),
                        limit: Some(500),
                    },
                    projects,
                )
                .ok();
            on_status(MemoryQueueStatusEvent { status, manager });
        }
    }
}

fn memory_project_context(
    projects: &[ProjectWorkspaceRecord],
    session: &AISessionSnapshot,
) -> Option<MemoryProjectContext> {
    projects
        .iter()
        .find(|project| project.id == session.project_id)
        .or_else(|| {
            session.project_path.as_ref().and_then(|path| {
                projects.iter().find(|project| {
                    paths_equivalent(Some(project.workspace_path.as_str()), path)
                        || paths_equivalent(Some(project.root_project_path.as_str()), path)
                })
            })
        })
        .map(MemoryProjectContext::from_workspace)
}

fn memory_project_context_from_history(
    projects: &[ProjectWorkspaceRecord],
    session: &AISessionSummary,
) -> Option<MemoryProjectContext> {
    projects
        .iter()
        .find(|project| {
            project.root_project_id == session.project_id
                || paths_equivalent(Some(project.workspace_path.as_str()), &session.project_path)
                || paths_equivalent(
                    Some(project.root_project_path.as_str()),
                    &session.project_path,
                )
        })
        .map(MemoryProjectContext::from_workspace)
}

fn historical_session_snapshot(
    session: &AISessionSummary,
    project: &MemoryProjectContext,
) -> Option<AISessionSnapshot> {
    let tool = normalized_non_empty(session.last_tool.as_deref()?)?.to_lowercase();
    Some(AISessionSnapshot {
        terminal_id: session.session_id.clone(),
        terminal_instance_id: None,
        project_id: project.project_id.clone(),
        project_name: project.project_name.clone(),
        project_path: Some(session.project_path.clone()),
        session_title: session.session_title.clone(),
        tool,
        ai_session_id: session.external_session_id.clone(),
        model: session.last_model.clone(),
        state: "idle".to_string(),
        status: "idle".to_string(),
        is_running: false,
        input_tokens: session.total_input_tokens,
        output_tokens: session.total_output_tokens,
        cached_input_tokens: session.cached_input_tokens,
        total_tokens: session.total_tokens,
        baseline_total_tokens: session.total_tokens,
        baseline_cached_input_tokens: session.cached_input_tokens,
        baseline_resolved: true,
        started_at: Some(session.first_seen_at),
        updated_at: session.last_seen_at,
        active_turn_started_at: None,
        runtime_turn_started_at: None,
        has_completed_turn: true,
        was_interrupted: false,
        transcript_path: None,
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
    })
}

fn root_projects_from_workspaces(projects: &[ProjectWorkspaceRecord]) -> Vec<ProjectRecord> {
    let mut seen = HashSet::new();
    projects
        .iter()
        .filter(|project| seen.insert(project.root_project_id.clone()))
        .map(|project| ProjectRecord {
            id: project.root_project_id.clone(),
            name: project.root_project_name.clone(),
            path: project.root_project_path.clone(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
            git_default_push_remote_name: project.git_default_push_remote_name.clone(),
        })
        .collect()
}

#[derive(Debug, Clone)]
struct MemoryContextPayload {
    project_name: String,
    project_profile: Option<String>,
    global_prompt: Option<String>,
    extra_context: Option<String>,
    user_summary: Option<String>,
    user_core_fallback: Vec<MemoryEntry>,
    project_core_fallback: Vec<MemoryEntry>,
    user_working: Vec<MemoryEntry>,
    project_working: Vec<MemoryEntry>,
    user_working_limit: i32,
    project_working_limit: i32,
    memory_enabled: bool,
}

impl MemoryContextPayload {
    fn has_memory(&self) -> bool {
        self.memory_enabled
            && (self.project_profile.is_some()
                || self.user_summary.is_some()
                || !self.user_core_fallback.is_empty()
                || !self.project_core_fallback.is_empty()
                || !self.user_working.is_empty()
                || !self.project_working.is_empty())
    }

    fn merged<const N: usize>(items: [MemoryContextPayload; N]) -> MemoryContextPayload {
        let mut iterator = items.into_iter();
        let Some(mut first) = iterator.next() else {
            return MemoryContextPayload {
                project_name: String::new(),
                project_profile: None,
                global_prompt: None,
                extra_context: None,
                user_summary: None,
                user_core_fallback: Vec::new(),
                project_core_fallback: Vec::new(),
                user_working: Vec::new(),
                project_working: Vec::new(),
                user_working_limit: 0,
                project_working_limit: 0,
                memory_enabled: false,
            };
        };
        let mut all = vec![first.clone()];
        all.extend(iterator);
        first.extra_context = join_optional_sections(
            all.iter()
                .filter_map(|item| item.extra_context.as_deref())
                .collect(),
        );
        first.user_core_fallback = unique_entries(
            all.iter()
                .flat_map(|item| item.user_core_fallback.clone())
                .collect(),
        );
        first.project_core_fallback = unique_entries(
            all.iter()
                .flat_map(|item| item.project_core_fallback.clone())
                .collect(),
        );
        first.user_working = unique_entries(
            all.iter()
                .flat_map(|item| item.user_working.clone())
                .collect(),
        );
        first.project_working = unique_entries(
            all.iter()
                .flat_map(|item| item.project_working.clone())
                .collect(),
        );
        if first.project_profile.is_none() {
            first.project_profile = all.iter().find_map(|item| item.project_profile.clone());
        }
        first.memory_enabled = all.iter().any(|item| item.memory_enabled);
        first
    }
}

#[derive(Debug, Clone)]
struct TranscriptSource {
    location: String,
    fingerprint: String,
}

#[derive(Debug, Clone, Default)]
struct MemoryExtractionResponse {
    user_summary: Option<String>,
    working_add: Vec<MemoryExtractionItem>,
    working_archive: Vec<String>,
    merged_entry_ids: Vec<String>,
    project_profile_refresh_recommended: bool,
}

#[derive(Debug, Clone)]
struct MemoryExtractionItem {
    scope: Option<MemoryScope>,
    module_key: Option<String>,
    tier: Option<MemoryTier>,
    kind: MemoryKind,
    content: String,
    rationale: Option<String>,
    merge_with: Vec<String>,
    replace: Option<String>,
    archive: Vec<String>,
    skip_reason: Option<String>,
}

fn entry_select_columns() -> &'static str {
    "id, scope, project_id, tool_id, module_key, tier, kind, content, rationale, source_tool, source_session_id, source_fingerprint, normalized_hash, superseded_by, status, merged_summary_id, merged_at, archived_at, access_count, last_accessed_at, created_at, updated_at"
}

fn qualified_entry_select_columns(table: &str) -> String {
    entry_select_columns()
        .split(", ")
        .map(|column| format!("{table}.{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn delete_project_memory_in_tx(tx: &rusqlite::Transaction<'_>, project_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM memory_entries WHERE scope = 'project' AND project_id = ?1;",
        params![project_id],
    )?;
    tx.execute(
        r#"
        DELETE FROM memory_summary_versions
        WHERE summary_id IN (
            SELECT id FROM memory_summaries
            WHERE scope = 'project' AND project_id = ?1
        );
        "#,
        params![project_id],
    )?;
    tx.execute(
        "DELETE FROM memory_summaries WHERE scope = 'project' AND project_id = ?1;",
        params![project_id],
    )?;
    tx.execute(
        "DELETE FROM memory_project_profiles WHERE project_id = ?1;",
        params![project_id],
    )?;
    Ok(())
}

fn memory_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntry> {
    Ok(MemoryEntry {
        id: row.get(0)?,
        scope: MemoryScope::from_str(row.get::<_, String>(1)?.as_str()),
        project_id: row.get(2)?,
        tool_id: row.get(3)?,
        module_key: row.get(4)?,
        tier: MemoryTier::from_str(row.get::<_, String>(5)?.as_str()),
        kind: MemoryKind::from_str(row.get::<_, String>(6)?.as_str()),
        content: row.get(7)?,
        rationale: row.get(8)?,
        source_tool: row.get(9)?,
        source_session_id: row.get(10)?,
        source_fingerprint: row.get(11)?,
        normalized_hash: row.get(12)?,
        superseded_by: row.get(13)?,
        status: MemoryEntryStatus::from_str(row.get::<_, String>(14)?.as_str()),
        merged_summary_id: row.get(15)?,
        merged_at: row.get(16)?,
        archived_at: row.get(17)?,
        access_count: row.get(18)?,
        last_accessed_at: row.get(19)?,
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
        last_decision: None,
    })
}

fn memory_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemorySummary> {
    let source_ids: Option<String> = row.get(6)?;
    Ok(MemorySummary {
        id: row.get(0)?,
        scope: MemoryScope::from_str(row.get::<_, String>(1)?.as_str()),
        project_id: row.get(2)?,
        tool_id: row.get(3)?,
        content: row.get(4)?,
        version: row.get(5)?,
        source_entry_ids: decode_string_array(source_ids.as_deref()),
        token_estimate: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn memory_project_profile_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MemoryProjectProfile> {
    Ok(MemoryProjectProfile {
        project_id: row.get(0)?,
        content: row.get(1)?,
        source_fingerprint: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn memory_decision_log_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryDecisionLog> {
    Ok(MemoryDecisionLog {
        kind: MemoryWriteDecisionKind::from_str(row.get::<_, String>(0)?.as_str()),
        entry_id: row.get(1)?,
        target_entry_id: row.get(2)?,
        reason: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn memory_task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryExtractionTask> {
    Ok(MemoryExtractionTask {
        id: row.get(0)?,
        project_id: row.get(1)?,
        tool: row.get(2)?,
        session_id: row.get(3)?,
        transcript_path: row.get(4)?,
        workspace_path: row.get(5)?,
        source_fingerprint: row.get(6)?,
        status: row.get(7)?,
        attempts: row.get(8)?,
        error: row.get(9)?,
        enqueued_at: row.get(10)?,
    })
}

fn scalar_i64(conn: &Connection, sql: &str) -> Result<i64> {
    conn.query_row(sql, [], |row| row.get(0))
        .map_err(Into::into)
}

fn optional_text_value(value: Option<&str>) -> SqlValue {
    value
        .map(|value| SqlValue::Text(value.to_string()))
        .unwrap_or(SqlValue::Null)
}

fn max_optional_f64(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn selected_memory_target_title(
    rows: &[MemoryManagerTargetRow],
    scope: &MemoryScope,
    project_id: Option<&str>,
) -> String {
    rows.iter()
        .find(|row| {
            row.scope == scope.as_str()
                && row.project_id.as_deref().unwrap_or("") == project_id.unwrap_or("")
        })
        .map(|row| row.title.clone())
        .unwrap_or_else(|| "User Memory".to_string())
}

fn render_tool_launch_text(
    _project_id: &str,
    project_name: &str,
    tool: &str,
    root: &Path,
    context: &MemoryContextPayload,
) -> String {
    let prompt = render_index_text(context, root);
    if prompt.is_empty() {
        return String::new();
    }
    format!(
            "Launch context for {}.\nStart with MEMORY.md. It contains stable summaries plus a small relevant working set; open topic files only when needed.\nAfter context compaction, reload MEMORY.md before continuing so durable memory is not lost.\nPrefer current repository state over stale memory.\n\n{}",
        document_tool_name(tool).replace("{}", project_name),
        prompt
    )
}

fn render_index_text(context: &MemoryContextPayload, root: &Path) -> String {
    let mut sections = Vec::new();
    if let Some(prompt) = &context.global_prompt {
        sections.push(render_summary_section("Global instructions", prompt));
    }
    if let Some(extra_context) = &context.extra_context {
        sections.push(render_summary_section(
            "Codux runtime capabilities",
            extra_context,
        ));
    }
    if !context.has_memory() {
        return sections.join("\n\n");
    }
    sections.push(format!(
        "# MEMORY.md\n\nProject context: {}\nApply relevant memory as guidance, not as source of truth.\nPrefer current repository state and user instructions over stale memory.\nAfter automatic or manual context compaction, reload this index and re-apply relevant memory before continuing.\n\n## Load order\n1. Use this index first; summaries are durable memory.\n2. Use the included working notes as a relevant top-k working set, not the complete store.\n3. Open topic files only when they are relevant to the current task.\n4. Full transcripts are not injected; use memory search only when history is needed.\n\n## Topic files\n- `memory-user.md`: cross-project user preferences and habits.\n- `memory-project.md`: project-specific decisions, conventions, and facts.\n- `memory-recent.md`: relevant working notes selected within the current injection budget.\n- `memory-search.md`: search-only memory guidance and current injection limits.\n\nMemory files directory: {}",
        context.project_name,
        root.display()
    ));
    if let Some(summary) = &context.user_summary {
        sections.push(render_summary_section("User summary", summary));
    } else if !context.user_core_fallback.is_empty() {
        sections.push(render_index_entry_list(
            "User notes index",
            &context.user_core_fallback,
        ));
    }
    if let Some(profile) = &context.project_profile {
        sections.push(render_summary_section("Project profile", profile));
    }
    if !context.project_core_fallback.is_empty() {
        sections.push(render_index_entry_list(
            "Project notes index",
            &context.project_core_fallback,
        ));
    }
    if !context.project_core_fallback.is_empty() || !context.project_working.is_empty() {
        sections.push(render_project_memory_directory(context));
    }
    if !context.user_working.is_empty() || !context.project_working.is_empty() {
        sections.push(format!(
            "[Relevant working notes index]\n- User working notes: {}\n- Project working notes: {}",
            context.user_working.len(),
            context.project_working.len()
        ));
    }
    trim_index_lines(&sections.join("\n\n"), 200)
}

fn render_user_memory_text(context: &MemoryContextPayload) -> String {
    let mut sections = vec![
        "# User Memory\n\nUse this only for cross-project user preferences, habits, and workflow choices. Do not treat project names, repository paths, commands, architecture decisions, bugs, or file locations as user memory.".to_string(),
    ];
    if let Some(summary) = &context.user_summary {
        sections.push(render_summary_section("User summary", summary));
    }
    if !context.user_core_fallback.is_empty() {
        sections.push(render_section(
            "User core notes",
            &context.user_core_fallback,
        ));
    }
    if !context.user_working.is_empty() {
        sections.push(render_section("Recent user notes", &context.user_working));
    }
    sections.join("\n\n")
}

fn render_project_profile_text(context: &MemoryContextPayload) -> String {
    let mut sections = vec![
        "# Project Profile\n\nStable project overview generated from repository files. Prefer current repository state if it differs."
            .to_string(),
    ];
    if let Some(profile) = &context.project_profile {
        sections.push(render_summary_section("Project profile", profile));
    } else {
        sections.push(
            "[Project profile]\nNo repository profile was generated for this launch.".to_string(),
        );
    }
    sections.join("\n\n")
}

fn render_project_memory_text(context: &MemoryContextPayload) -> String {
    let mut sections = vec![
        "# Project Memory\n\nUse this for project-specific decisions, conventions, module facts, and bug lessons. Project overview lives in memory-project-profile.md."
            .to_string(),
    ];
    if !context.project_core_fallback.is_empty() {
        sections.push(render_section(
            "Project core notes",
            &context.project_core_fallback,
        ));
    }
    if !context.project_working.is_empty() {
        sections.push(render_section(
            "Recent project notes",
            &context.project_working,
        ));
    }
    sections.join("\n\n")
}

fn render_recent_memory_text(context: &MemoryContextPayload) -> String {
    let mut sections = vec![
        "# Recent Working Memory\n\nThese notes are selected by relevance and budget. They are not the complete memory store and should not override current repository evidence."
            .to_string(),
    ];
    if !context.user_working.is_empty() {
        sections.push(render_section("Recent user notes", &context.user_working));
    }
    if !context.project_working.is_empty() {
        sections.push(render_section(
            "Recent project notes",
            &context.project_working,
        ));
    }
    sections.join("\n\n")
}

fn render_search_guide_text(context: &MemoryContextPayload) -> String {
    format!(
        "# Search-Only Memory\n\nFull historical transcripts are not loaded into launch context.\nDurable summaries stay in MEMORY.md; working memory is selected as a small relevant set.\nUse current repository files first. Search memory only when prior decisions,\nprevious debugging chains, or older project context are directly relevant.\n\nCurrent injected working-set budget:\n- User working notes: {}/{}\n- Project working notes: {}/{}",
        context.user_working.len(),
        context.user_working_limit,
        context.project_working.len(),
        context.project_working_limit
    )
}

fn render_section(title: &str, entries: &[MemoryEntry]) -> String {
    let lines = entries
        .iter()
        .map(|entry| {
            let module = entry
                .module_key
                .as_deref()
                .and_then(normalized_memory_module)
                .unwrap_or_else(|| DEFAULT_MEMORY_MODULE.to_string());
            if let Some(rationale) = normalized_non_empty(entry.rationale.as_deref().unwrap_or(""))
            {
                format!(
                    "- ({}) {} [{}; {}]",
                    module,
                    entry.content,
                    entry.kind.as_str(),
                    rationale
                )
            } else {
                format!("- ({}) {} [{}]", module, entry.content, entry.kind.as_str())
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("[{title}]\n{lines}")
}

fn render_index_entry_list(title: &str, entries: &[MemoryEntry]) -> String {
    let lines = entries
        .iter()
        .take(8)
        .map(|entry| {
            let module = entry
                .module_key
                .as_deref()
                .and_then(normalized_memory_module)
                .unwrap_or_else(|| DEFAULT_MEMORY_MODULE.to_string());
            format!("- {} / {}: {}", module, entry.kind.as_str(), entry.content)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("[{title}]\n{lines}")
}

fn render_project_memory_directory(context: &MemoryContextPayload) -> String {
    let mut modules = context
        .project_core_fallback
        .iter()
        .chain(context.project_working.iter())
        .filter_map(|entry| normalized_memory_module(entry.module_key.as_deref().unwrap_or("")))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    modules.sort();
    let lines = modules
        .iter()
        .map(|module| {
            let count = context
                .project_core_fallback
                .iter()
                .chain(context.project_working.iter())
                .filter(|entry| entry.module_key.as_deref() == Some(module.as_str()))
                .count();
            format!("- {module}: {count} injected notes")
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "[Project memory directory]\n{}",
        if lines.is_empty() {
            "- general: see memory-project.md".to_string()
        } else {
            lines
        }
    )
}

fn render_summary_section(title: &str, content: &str) -> String {
    format!("[{title}]\n{content}")
}

fn join_optional_sections(sections: Vec<&str>) -> Option<String> {
    let mut unique = Vec::new();
    for section in sections {
        let Some(section) = normalized_non_empty(section) else {
            continue;
        };
        if !unique.iter().any(|item: &String| item == &section) {
            unique.push(section);
        }
    }
    normalized_non_empty(&unique.join("\n\n"))
}

fn append_memory_log(category: &str, message: &str) {
    let path = runtime_temp_dir().join("live.log");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[memory] [{category}] {message}");
    }
}

fn safe_log_value(value: &str, max_chars: usize) -> String {
    let mut output = value
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if output.chars().count() > max_chars {
        output = format!(
            "{}...[truncated]",
            output.chars().take(max_chars).collect::<String>()
        );
    }
    output
}

fn safe_log_preview(value: &str, max_chars: usize) -> String {
    safe_log_value(value, max_chars)
}

fn short_log_fingerprint(value: &str) -> String {
    value.chars().take(24).collect()
}

fn trim_index_lines(text: &str, max_lines: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() <= max_lines {
        return text.to_string();
    }
    format!(
        "{}\n[Memory index truncated]",
        lines[..max_lines - 1].join("\n")
    )
}

fn trimmed_memory_text(text: Option<&str>, max_tokens: i32) -> Option<String> {
    let text = normalized_non_empty(text?)?;
    let max_chars = (max_tokens.max(50) as usize * 4).max(200);
    if text.chars().count() <= max_chars {
        return Some(text);
    }
    Some(format!(
        "{}\n[Memory summary truncated]",
        text.chars()
            .take(max_chars)
            .collect::<String>()
            .trim()
            .to_string()
    ))
}

fn select_context_entries(
    entries: Vec<MemoryEntry>,
    tool_id: Option<&str>,
    query: &str,
    limit: usize,
) -> Vec<MemoryEntry> {
    if limit == 0 || entries.is_empty() {
        return Vec::new();
    }
    let query_terms = memory_query_terms(query);
    let now = now_seconds();
    let mut scored = entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let score = memory_context_score(&entry, &query_terms, tool_id, now);
            (score, index, entry)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.1.cmp(&right.1))
    });
    unique_entries(
        scored
            .into_iter()
            .take(limit)
            .map(|(_, _, entry)| entry)
            .collect(),
    )
}

fn memory_context_score(
    entry: &MemoryEntry,
    query_terms: &[String],
    tool_id: Option<&str>,
    now: f64,
) -> f64 {
    let mut score = match entry.tier {
        MemoryTier::Core => 120.0,
        MemoryTier::Working => 40.0,
        MemoryTier::Archive => 0.0,
    };
    if let (Some(entry_tool), Some(current_tool)) = (entry.tool_id.as_deref(), tool_id) {
        if entry_tool == current_tool {
            score += 14.0;
        }
    }
    let haystack = format!(
        "{} {} {} {}",
        entry.content,
        entry.rationale.as_deref().unwrap_or(""),
        entry.kind.as_str(),
        entry.source_tool.as_deref().unwrap_or("")
    )
    .to_lowercase();
    for term in query_terms {
        if haystack.contains(term) {
            score += 20.0;
        }
    }
    score += (entry.access_count.min(20) as f64) * 1.5;
    let recency_source = entry
        .last_accessed_at
        .unwrap_or(entry.updated_at)
        .max(entry.updated_at);
    let age_days = ((now - recency_source).max(0.0)) / 86_400.0;
    score + (30.0 / (1.0 + age_days))
}

fn memory_retrieval_query(project_name: &str, tool: &str, global_prompt: &str) -> String {
    [project_name, tool, global_prompt]
        .into_iter()
        .filter_map(normalized_non_empty)
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_project_profile(
    project_id: &str,
    project_name: &str,
    workspace_path: &str,
) -> Option<MemoryProjectProfile> {
    let root = Path::new(workspace_path);
    if !root.is_dir() {
        return None;
    }
    let evidence = collect_project_profile_evidence(root);
    let overview = infer_project_overview(project_name, &evidence);
    let tech_stack = infer_project_tech_stack(&evidence);
    let commands = infer_project_commands(&evidence);
    let modules = infer_project_modules(root, &evidence);
    let source_signals = render_source_sample_signals(&evidence.source_samples);
    let app_name = evidence.app_name.as_deref().unwrap_or(project_name);

    let mut sections = Vec::new();
    sections.push(format!(
        "Project: {app_name}\nOverview: {}",
        overview.replace('\n', " ")
    ));
    if !tech_stack.is_empty() {
        sections.push(format!("Tech stack:\n{}", bullet_lines(&tech_stack)));
    }
    if !commands.is_empty() {
        sections.push(format!("Common commands:\n{}", bullet_lines(&commands)));
    }
    if !evidence.directories.is_empty() {
        sections.push(format!(
            "Top-level directories:\n{}",
            bullet_lines(&evidence.directories)
        ));
    }
    if !modules.is_empty() {
        sections.push(format!("Detected modules:\n{}", bullet_lines(&modules)));
    }
    if !source_signals.is_empty() {
        sections.push(format!(
            "Source signals:\n{}",
            bullet_lines(&source_signals)
        ));
    }
    let content = sections.join("\n\n");
    Some(MemoryProjectProfile {
        project_id: project_id.to_string(),
        content,
        source_fingerprint: evidence.source_fingerprint,
        created_at: now_seconds(),
        updated_at: now_seconds(),
    })
}

fn llm_project_profile_fingerprint(source_fingerprint: &str) -> String {
    format!("llm-v1:{source_fingerprint}")
}

fn project_profile_llm_source_fingerprint(
    source_fingerprint: &str,
    memory_context: &[String],
) -> String {
    let memory_hash = sha256_hex(&memory_context.join("\n"));
    format!("{source_fingerprint}:memory:{memory_hash}")
}

fn project_profile_content_with_memory_context(content: &str, memory_context: &[String]) -> String {
    if memory_context.is_empty() {
        return content.to_string();
    }
    format!(
        "{content}\n\nProject memory signals:\n{}",
        bullet_lines(memory_context)
    )
}

fn project_profile_fingerprints_match(existing: &str, incoming: &str) -> bool {
    existing == incoming
        || (!incoming.starts_with("llm-v1:")
            && existing == llm_project_profile_fingerprint(incoming))
}

fn project_profile_llm_refresh_due(
    existing: &MemoryProjectProfile,
    generated: &MemoryProjectProfile,
) -> bool {
    if existing.source_fingerprint == llm_project_profile_fingerprint(&generated.source_fingerprint)
    {
        return false;
    }
    let changed = !project_profile_fingerprints_match(
        &existing.source_fingerprint,
        &generated.source_fingerprint,
    );
    if !changed && existing.source_fingerprint.starts_with("llm-v1:") {
        return false;
    }
    now_seconds() - existing.updated_at >= PROJECT_PROFILE_LLM_REFRESH_COOLDOWN_SECONDS
}

fn project_profile_system_prompt() -> &'static str {
    "You improve a repository-derived project profile for an AI coding memory system.\nReturn JSON only, no markdown fences, no commentary.\nUse only the provided deterministic profile. Do not invent dependencies, commands, settings, files, or product claims."
}

fn make_project_profile_llm_prompt(deterministic_profile: &str) -> String {
    format!(
        "Rewrite this deterministic repository profile into a concise, useful project overview for future AI coding sessions.\n\nReturn minified JSON only:\n{{\"content\":\"Project: ...\\nOverview: ...\\n\\nTech stack:\\n- ...\\n\\nCommon commands:\\n- ...\\n\\nTop-level directories:\\n- ...\\n\\nDetected modules:\\n- ...\"}}\n\nRules:\n- Preserve the section names: Project, Overview, Tech stack, Common commands, Top-level directories, Detected modules.\n- Use Source signals only as repository evidence to improve Overview and Detected modules; do not copy long source snippets into the final profile.\n- Use Project memory signals only as durable post-scan corrections or additions; let them refine purpose, architecture, modules, tech stack, or commands when they are more specific than repository metadata.\n- Use only the provided deterministic profile; do not invent missing facts.\n- Keep it compact, target 500-900 tokens total.\n- Prefer concrete engineering facts over marketing language.\n- Merge duplicates and improve wording naturally; do not hard-truncate or cut mid-sentence.\n- Keep commands exactly as shown unless correcting obvious package-manager wording from the evidence.\n- If evidence is missing, omit that bullet instead of guessing.\n\nDeterministic profile:\n<profile>\n{}\n</profile>",
        deterministic_profile
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectProfileDecodeError {
    EmptyResponse,
    NoJsonCandidate,
    MalformedJson,
    MissingProfileContent,
    InvalidProfileContent,
    ProfileTooLong,
}

impl std::fmt::Display for ProjectProfileDecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            ProjectProfileDecodeError::EmptyResponse => "empty LLM response",
            ProjectProfileDecodeError::NoJsonCandidate => "response did not contain a JSON object",
            ProjectProfileDecodeError::MalformedJson => "response contained malformed JSON",
            ProjectProfileDecodeError::MissingProfileContent => {
                "JSON did not contain project profile content"
            }
            ProjectProfileDecodeError::InvalidProfileContent => {
                "project profile content was missing required sections"
            }
            ProjectProfileDecodeError::ProfileTooLong => "project profile content exceeded limit",
        };
        formatter.write_str(message)
    }
}

fn decode_project_profile_llm_response_detailed(
    raw: &str,
) -> std::result::Result<String, ProjectProfileDecodeError> {
    let stripped = strip_markdown_code_fences(raw);
    if stripped.trim().is_empty() {
        return Err(ProjectProfileDecodeError::EmptyResponse);
    }
    let (values, mut last_error) = project_profile_json_values(&stripped);
    if values.is_empty() {
        return Err(last_error);
    }
    for value in values {
        let Some(content) = project_profile_content_from_value(&value) else {
            last_error = ProjectProfileDecodeError::MissingProfileContent;
            continue;
        };
        match validate_project_profile_content(&content) {
            Ok(()) => return Ok(content),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn project_profile_json_values(raw: &str) -> (Vec<Value>, ProjectProfileDecodeError) {
    let values = llm_json_values(raw);
    let candidates = json_object_candidates(raw);
    if candidates.is_empty() {
        return (values, ProjectProfileDecodeError::NoJsonCandidate);
    }
    (values, ProjectProfileDecodeError::MalformedJson)
}

fn project_profile_content_from_value(value: &Value) -> Option<String> {
    if let Some(content) = string_from_keys(
        value,
        &[
            "content",
            "profile",
            "project_profile",
            "projectProfile",
            "projectProfileContent",
        ],
    ) {
        return Some(content);
    }
    for key in [
        "profile",
        "project_profile",
        "projectProfile",
        "result",
        "response",
        "data",
    ] {
        if let Some(content) = value.get(key).and_then(project_profile_content_from_value) {
            return Some(content);
        }
    }
    structured_project_profile_content(value)
}

fn structured_project_profile_content(value: &Value) -> Option<String> {
    let project = string_from_keys(
        value,
        &["project", "project_name", "projectName", "name", "title"],
    )?;
    let overview = string_from_keys(
        value,
        &[
            "overview",
            "summary",
            "description",
            "purpose",
            "project_overview",
            "projectOverview",
        ],
    )?;
    let mut sections = vec![
        format!("Project: {project}"),
        format!("Overview: {overview}"),
    ];
    push_project_profile_list_section(
        &mut sections,
        "Tech stack",
        list_from_keys(
            value,
            &[
                "tech_stack",
                "techStack",
                "stack",
                "technologies",
                "dependencies",
            ],
        ),
    );
    push_project_profile_list_section(
        &mut sections,
        "Common commands",
        list_from_keys(
            value,
            &[
                "common_commands",
                "commonCommands",
                "commands",
                "scripts",
                "dev_commands",
                "devCommands",
            ],
        ),
    );
    push_project_profile_list_section(
        &mut sections,
        "Top-level directories",
        list_from_keys(
            value,
            &[
                "top_level_directories",
                "topLevelDirectories",
                "directories",
                "folders",
            ],
        ),
    );
    push_project_profile_list_section(
        &mut sections,
        "Detected modules",
        list_from_keys(
            value,
            &[
                "detected_modules",
                "detectedModules",
                "modules",
                "areas",
                "components",
            ],
        ),
    );
    Some(sections.join("\n\n"))
}

fn push_project_profile_list_section(sections: &mut Vec<String>, title: &str, items: Vec<String>) {
    if items.is_empty() {
        return;
    }
    let lines = items
        .into_iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    sections.push(format!("{title}:\n{lines}"));
}

fn list_from_keys(value: &Value, keys: &[&str]) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        let values = list_from_value(value);
        if !values.is_empty() {
            return values;
        }
    }
    Vec::new()
}

fn list_from_value(value: &Value) -> Vec<String> {
    if let Some(text) = value.as_str().and_then(normalized_non_empty) {
        return text
            .lines()
            .filter_map(|line| {
                normalized_non_empty(line.trim_start_matches(['-', '*', ' '].as_slice()))
            })
            .collect();
    }
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .and_then(normalized_non_empty)
                        .or_else(|| string_from_keys(item, &["name", "label", "value", "command"]))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn validate_project_profile_content(
    content: &str,
) -> std::result::Result<(), ProjectProfileDecodeError> {
    if content.len() > 16_000 {
        return Err(ProjectProfileDecodeError::ProfileTooLong);
    }
    if content.contains("Project:") && content.contains("Overview:") {
        return Ok(());
    }
    Err(ProjectProfileDecodeError::InvalidProfileContent)
}

#[derive(Debug, Default)]
struct ProjectProfileEvidence {
    readme: Option<String>,
    package: Option<Value>,
    composer: Option<Value>,
    cargo: Option<String>,
    root_cargo: Option<String>,
    tauri: Option<Value>,
    pyproject: Option<String>,
    go_mod: Option<String>,
    pom: Option<String>,
    gradle: Option<String>,
    gemfile: Option<String>,
    dockerfile: Option<String>,
    docker_compose: Option<String>,
    app_name: Option<String>,
    directories: Vec<String>,
    source_samples: Vec<ProjectSourceSample>,
    root_markers: HashSet<String>,
    package_manager: String,
    source_fingerprint: String,
}

#[derive(Debug, Clone)]
struct ProjectSourceSample {
    path: String,
    signals: Vec<String>,
}

fn collect_project_profile_evidence(root: &Path) -> ProjectProfileEvidence {
    let readme = read_first_existing_file(
        root,
        &["README.md", "README.zh-CN.md", "README.cn.md", "readme.md"],
        18_000,
    );
    let package = read_json_file(&root.join("package.json"));
    let composer = read_json_file(&root.join("composer.json"));
    let cargo = read_limited_file(&root.join("src-tauri").join("Cargo.toml"), 10_000);
    let root_cargo = read_limited_file(&root.join("Cargo.toml"), 10_000);
    let tauri = read_json_file(&root.join("src-tauri").join("tauri.conf.json"))
        .or_else(|| read_json_file(&root.join("src-tauri").join("tauri.conf.json5")));
    let pyproject = read_limited_file(&root.join("pyproject.toml"), 20_000);
    let go_mod = read_limited_file(&root.join("go.mod"), 12_000);
    let pom = read_limited_file(&root.join("pom.xml"), 20_000);
    let gradle = read_first_existing_file(root, &["build.gradle", "build.gradle.kts"], 20_000);
    let gemfile = read_limited_file(&root.join("Gemfile"), 12_000);
    let dockerfile = read_limited_file(&root.join("Dockerfile"), 16_000);
    let docker_compose = read_first_existing_file(
        root,
        &[
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ],
        20_000,
    );
    let directories = top_level_directories(root);
    let root_markers = root_file_markers(root);
    let package_manager = detect_package_manager(root);
    let source_samples = collect_project_source_samples(root);
    let app_name = infer_project_app_name(
        package.as_ref(),
        composer.as_ref(),
        tauri.as_ref(),
        pyproject.as_deref(),
        go_mod.as_deref(),
        root,
    );
    let fingerprint_input = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        readme.as_deref().unwrap_or_default(),
        package.as_ref().map(Value::to_string).unwrap_or_default(),
        composer.as_ref().map(Value::to_string).unwrap_or_default(),
        cargo.as_deref().unwrap_or_default(),
        root_cargo.as_deref().unwrap_or_default(),
        tauri.as_ref().map(Value::to_string).unwrap_or_default(),
        pyproject.as_deref().unwrap_or_default(),
        go_mod.as_deref().unwrap_or_default(),
        pom.as_deref().unwrap_or_default(),
        gradle.as_deref().unwrap_or_default(),
        gemfile.as_deref().unwrap_or_default(),
        dockerfile.as_deref().unwrap_or_default(),
        docker_compose.as_deref().unwrap_or_default(),
        render_source_sample_signals(&source_samples).join("\n"),
        package_manager,
        directories.join("|")
    );

    ProjectProfileEvidence {
        readme,
        package,
        composer,
        cargo,
        root_cargo,
        tauri,
        pyproject,
        go_mod,
        pom,
        gradle,
        gemfile,
        dockerfile,
        docker_compose,
        app_name,
        directories,
        source_samples,
        root_markers,
        package_manager: package_manager.to_string(),
        source_fingerprint: sha256_hex(&fingerprint_input),
    }
}

fn read_limited_file(path: &Path, max_bytes: usize) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() == 0 {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    Some(text.chars().take(max_bytes).collect::<String>())
}

fn read_json_file(path: &Path) -> Option<Value> {
    read_limited_file(path, 80_000).and_then(|text| serde_json::from_str(&text).ok())
}

fn read_first_existing_file(root: &Path, names: &[&str], max_bytes: usize) -> Option<String> {
    names
        .iter()
        .find_map(|name| read_limited_file(&root.join(name), max_bytes))
}

fn collect_project_source_samples(root: &Path) -> Vec<ProjectSourceSample> {
    let mut candidates = Vec::new();
    for relative in [
        "src/main.tsx",
        "src/main.ts",
        "src/App.tsx",
        "src/App.ts",
        "src/index.tsx",
        "src/index.ts",
        "src/router.ts",
        "src/routes.ts",
        "src-tauri/src/lib.rs",
        "src-tauri/src/main.rs",
        "routes/web.php",
        "routes/api.php",
        "app/Http/Controllers",
        "cmd",
        "internal",
        "pkg",
        "src",
    ] {
        collect_source_sample_candidates(root, relative, &mut candidates);
    }

    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .filter_map(|path| source_sample_for_path(root, &path))
        .take(10)
        .collect()
}

fn collect_source_sample_candidates(root: &Path, relative: &str, output: &mut Vec<String>) {
    let path = root.join(relative);
    if path.is_file() {
        output.push(relative.to_string());
        return;
    }
    if !path.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(&path) else {
        return;
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                let nested = entry_path.join("mod.rs");
                if nested.is_file() {
                    return path_relative_to(root, &nested);
                }
                if name.ends_with("Controller") || matches!(name.as_str(), "routes" | "pages") {
                    return path_relative_to(root, &entry_path);
                }
                return None;
            }
            source_sample_supported_file(&entry_path)
                .then(|| path_relative_to(root, &entry_path))?
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|name| source_sample_priority(name));
    output.extend(files.into_iter().take(4));
}

fn source_sample_for_path(root: &Path, relative: &str) -> Option<ProjectSourceSample> {
    let path = root.join(relative);
    if path.is_dir() {
        return source_sample_for_directory(root, relative, &path);
    }
    if !source_sample_supported_file(&path) {
        return None;
    }
    let metadata = fs::metadata(&path).ok()?;
    if metadata.len() > 180_000 {
        return None;
    }
    let text = read_limited_file(&path, 24_000)?;
    let signals = extract_source_signals(&text);
    if signals.is_empty() {
        return None;
    }
    Some(ProjectSourceSample {
        path: relative.to_string(),
        signals,
    })
}

fn source_sample_for_directory(
    _root: &Path,
    relative: &str,
    path: &Path,
) -> Option<ProjectSourceSample> {
    let names = source_module_names(path)
        .into_iter()
        .take(8)
        .collect::<Vec<_>>();
    if names.is_empty() {
        return None;
    }
    Some(ProjectSourceSample {
        path: relative.to_string(),
        signals: vec![format!("contains {}", names.join(", "))],
    })
}

fn source_sample_supported_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension,
                "ts" | "tsx" | "js" | "jsx" | "rs" | "php" | "py" | "go" | "java" | "kt" | "rb"
            )
        })
}

fn source_sample_priority(path: &str) -> (usize, String) {
    let lower = path.to_lowercase();
    let rank = if lower.contains("main.") || lower.contains("app.") || lower.contains("lib.rs") {
        0
    } else if lower.contains("route") || lower.contains("controller") {
        1
    } else if lower.contains("window") || lower.contains("manager") {
        2
    } else {
        3
    };
    (rank, lower)
}

fn path_relative_to(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn extract_source_signals(text: &str) -> Vec<String> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut signals = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = compact_source_line(line);
        if trimmed.is_empty() {
            continue;
        }
        if is_source_import_export_signal(&trimmed)
            || is_source_declaration_signal(&trimmed)
            || is_source_route_signal(&trimmed)
            || is_source_runtime_signal(&trimmed)
        {
            push_source_signal(&mut signals, trimmed.clone());
        }
        if trimmed == "#[tauri::command]" {
            if let Some(next) = lines.get(index + 1).map(|line| compact_source_line(line)) {
                push_source_signal(&mut signals, format!("tauri command {}", next));
            }
        }
        if signals.len() >= 8 {
            break;
        }
    }
    signals
}

fn compact_source_line(line: &str) -> String {
    line.trim()
        .trim_start_matches("pub ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(180)
        .collect()
}

fn is_source_import_export_signal(line: &str) -> bool {
    line.starts_with("import ")
        || line.starts_with("export ")
        || line.starts_with("use crate::")
        || line.starts_with("use ")
        || line.starts_with("mod ")
}

fn is_source_declaration_signal(line: &str) -> bool {
    line.starts_with("fn ")
        || line.starts_with("async fn ")
        || line.starts_with("function ")
        || line.starts_with("const ")
        || line.starts_with("class ")
        || line.starts_with("interface ")
        || line.starts_with("type ")
        || line.starts_with("struct ")
        || line.starts_with("enum ")
        || line.starts_with("impl ")
}

fn is_source_route_signal(line: &str) -> bool {
    line.contains("Route::")
        || line.contains("router.")
        || line.contains("app.get(")
        || line.contains("app.post(")
        || line.contains("<Route")
        || line.contains("createBrowserRouter")
}

fn is_source_runtime_signal(line: &str) -> bool {
    line.contains("createRoot(")
        || line.contains("invoke_handler")
        || line.contains("tauri::generate_handler")
        || line.contains("register_plugin")
        || line.contains("createApp(")
}

fn push_source_signal(signals: &mut Vec<String>, signal: String) {
    if signal.chars().count() < 8 || signals.iter().any(|existing| existing == &signal) {
        return;
    }
    signals.push(signal);
}

fn render_source_sample_signals(samples: &[ProjectSourceSample]) -> Vec<String> {
    samples
        .iter()
        .map(|sample| format!("{}: {}", sample.path, sample.signals.join("; ")))
        .collect()
}

fn extract_readme_overview(readme: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in readme.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('<')
            || trimmed.starts_with('!')
            || trimmed.starts_with('[')
            || trimmed.starts_with('#')
            || trimmed.starts_with('|')
            || trimmed.starts_with("---")
        {
            if !lines.is_empty() {
                break;
            }
            continue;
        }
        lines.push(trimmed.to_string());
        if lines.join(" ").chars().count() > 420 {
            break;
        }
    }
    normalized_non_empty(&lines.join(" "))
}

fn infer_project_overview(project_name: &str, evidence: &ProjectProfileEvidence) -> String {
    if let Some(overview) = evidence.readme.as_deref().and_then(extract_readme_overview) {
        return overview;
    }
    if let Some(description) = json_string_field(evidence.package.as_ref(), "description") {
        return description;
    }
    if let Some(description) = json_string_field(evidence.composer.as_ref(), "description") {
        return description;
    }
    if has_php_framework(evidence, "laravel") {
        return format!(
            "{project_name} is a Laravel PHP application with Composer-managed dependencies."
        );
    }
    if has_php_framework(evidence, "symfony") {
        return format!(
            "{project_name} is a Symfony PHP application with Composer-managed dependencies."
        );
    }
    if has_php_framework(evidence, "thinkphp") {
        return format!(
            "{project_name} is a ThinkPHP application with Composer-managed dependencies."
        );
    }
    if has_php_framework(evidence, "dux") {
        return format!(
            "{project_name} is a Dux PHP application with Composer-managed dependencies."
        );
    }
    if evidence.composer.is_some() {
        return format!("{project_name} is a PHP project managed with Composer.");
    }
    if let Some(package_name) = json_string_field(evidence.package.as_ref(), "name") {
        return format!(
            "{package_name} is a JavaScript/TypeScript project managed with package scripts."
        );
    }
    if let Some(module) = go_module_name(evidence.go_mod.as_deref()) {
        return format!("{module} is a Go module.");
    }
    if evidence.pyproject.is_some() {
        return format!("{project_name} is a Python project configured with pyproject.toml.");
    }
    format!("{project_name} project workspace.")
}

fn infer_project_tech_stack(evidence: &ProjectProfileEvidence) -> Vec<String> {
    let mut stack = Vec::new();
    let deps = package_dependencies(evidence.package.as_ref());
    let composer_deps = composer_dependencies(evidence.composer.as_ref());
    if deps.iter().any(|dep| dep == "react" || dep == "react-dom") {
        stack.push("Frontend: React".to_string());
    }
    if deps
        .iter()
        .any(|dep| dep == "vue" || dep.starts_with("@vitejs/plugin-vue"))
    {
        stack.push("Frontend: Vue".to_string());
    }
    if deps.iter().any(|dep| dep == "next") {
        stack.push("Framework: Next.js".to_string());
    }
    if deps.iter().any(|dep| dep == "nuxt") {
        stack.push("Framework: Nuxt".to_string());
    }
    if deps.iter().any(|dep| dep == "typescript") {
        stack.push("Language: TypeScript".to_string());
    }
    if evidence.package.is_some() {
        stack.push(format!("Package manager: {}", evidence.package_manager));
    }
    if deps
        .iter()
        .any(|dep| dep == "vite" || dep == "@vitejs/plugin-react")
    {
        stack.push("Build: Vite".to_string());
    }
    if deps
        .iter()
        .any(|dep| dep == "tailwindcss" || dep == "@tailwindcss/vite")
    {
        stack.push("Styling: Tailwind CSS".to_string());
    }
    if deps.iter().any(|dep| dep == "zustand") {
        stack.push("State: Zustand".to_string());
    }
    if deps
        .iter()
        .any(|dep| dep.starts_with("@codemirror/") || dep == "codemirror")
    {
        stack.push("Editor: CodeMirror".to_string());
    }
    if deps.iter().any(|dep| dep.starts_with("@xterm/")) {
        stack.push("Terminal: xterm.js".to_string());
    }
    if evidence.tauri.is_some() || deps.iter().any(|dep| dep.starts_with("@tauri-apps/")) {
        stack.push("Desktop: Tauri".to_string());
    }
    if evidence.cargo.is_some() || evidence.root_cargo.is_some() {
        stack.push("Native/runtime: Rust".to_string());
    }
    if cargo_dependency_present(evidence, "genai") {
        stack.push("AI provider SDK: genai".to_string());
    }
    if cargo_dependency_present(evidence, "rusqlite") {
        stack.push("Storage: SQLite".to_string());
    }
    if evidence.composer.is_some() {
        stack.push("Language: PHP".to_string());
        stack.push("Package manager: Composer".to_string());
    }
    if composer_deps.iter().any(|dep| dep == "laravel/framework")
        || evidence_has_path(evidence, "artisan")
    {
        stack.push("Framework: Laravel".to_string());
    }
    if composer_deps.iter().any(|dep| dep.starts_with("symfony/"))
        || evidence_has_path(evidence, "bin")
    {
        stack.push("Framework: Symfony".to_string());
    }
    if composer_deps.iter().any(|dep| dep == "topthink/framework") {
        stack.push("Framework: ThinkPHP".to_string());
    }
    if composer_deps.iter().any(|dep| dep.starts_with("hyperf/")) {
        stack.push("Framework: Hyperf".to_string());
    }
    if has_php_framework(evidence, "dux") {
        stack.push("Framework: Dux".to_string());
    }
    if evidence.pyproject.is_some() {
        stack.push("Language: Python".to_string());
        stack.push("Project config: pyproject.toml".to_string());
    }
    if evidence
        .pyproject
        .as_deref()
        .is_some_and(|text| contains_token(text, "fastapi"))
    {
        stack.push("Framework: FastAPI".to_string());
    }
    if evidence
        .pyproject
        .as_deref()
        .is_some_and(|text| contains_token(text, "django"))
    {
        stack.push("Framework: Django".to_string());
    }
    if evidence
        .pyproject
        .as_deref()
        .is_some_and(|text| contains_token(text, "flask"))
    {
        stack.push("Framework: Flask".to_string());
    }
    if evidence.go_mod.is_some() {
        stack.push("Language: Go".to_string());
        stack.push("Module system: Go modules".to_string());
    }
    if evidence
        .go_mod
        .as_deref()
        .is_some_and(|text| text.contains("github.com/gin-gonic/gin"))
    {
        stack.push("Framework: Gin".to_string());
    }
    if evidence.pom.is_some() || evidence.gradle.is_some() {
        stack.push("Language/runtime: JVM".to_string());
    }
    if evidence
        .pom
        .as_deref()
        .or(evidence.gradle.as_deref())
        .is_some_and(|text| text.contains("spring-boot"))
    {
        stack.push("Framework: Spring Boot".to_string());
    }
    if evidence.pom.is_some() {
        stack.push("Build: Maven".to_string());
    }
    if evidence.gradle.is_some() {
        stack.push("Build: Gradle".to_string());
    }
    if evidence.gemfile.is_some() {
        stack.push("Language: Ruby".to_string());
        stack.push("Package manager: Bundler".to_string());
    }
    if evidence.dockerfile.is_some() || evidence.docker_compose.is_some() {
        stack.push("Runtime: Docker".to_string());
    }
    sorted_unique_strings(stack)
}

fn infer_project_commands(evidence: &ProjectProfileEvidence) -> Vec<String> {
    let mut commands = Vec::new();
    if let Some(scripts) = evidence
        .package
        .as_ref()
        .and_then(|value| value.get("scripts"))
        .and_then(Value::as_object)
    {
        for key in ["dev", "build", "test", "lint", "tauri"] {
            if scripts.contains_key(key) {
                commands.push(format!("{} {key}", evidence.package_manager));
            }
        }
        if scripts.contains_key("build") {
            commands.push(package_exec_command(
                &evidence.package_manager,
                "tsc --noEmit",
            ));
        }
    }
    if evidence.cargo.is_some() {
        commands.push("cargo check --manifest-path src-tauri/Cargo.toml".to_string());
    }
    if evidence.root_cargo.is_some() {
        commands.push("cargo check".to_string());
    }
    if evidence.composer.is_some() {
        commands.push("composer install".to_string());
        if has_composer_script(evidence.composer.as_ref(), "test") {
            commands.push("composer test".to_string());
        }
        if has_php_framework(evidence, "laravel") {
            commands.push("php artisan test".to_string());
        } else if has_php_framework(evidence, "symfony") {
            commands.push("php bin/console".to_string());
        }
    }
    if evidence.pyproject.is_some() {
        commands.push("python -m pytest".to_string());
    }
    if evidence.go_mod.is_some() {
        commands.push("go test ./...".to_string());
    }
    if evidence.pom.is_some() {
        commands.push("mvn test".to_string());
    }
    if evidence.gradle.is_some() {
        commands.push("./gradlew test".to_string());
    }
    if evidence.gemfile.is_some() {
        commands.push("bundle exec rspec".to_string());
    }
    sorted_unique_strings(commands)
}

fn package_dependencies(package: Option<&Value>) -> Vec<String> {
    let mut deps = Vec::new();
    for key in ["dependencies", "devDependencies"] {
        if let Some(object) = package
            .and_then(|value| value.get(key))
            .and_then(Value::as_object)
        {
            deps.extend(object.keys().cloned());
        }
    }
    deps
}

fn composer_dependencies(composer: Option<&Value>) -> Vec<String> {
    let mut deps = Vec::new();
    for key in ["require", "require-dev"] {
        if let Some(object) = composer
            .and_then(|value| value.get(key))
            .and_then(Value::as_object)
        {
            deps.extend(object.keys().cloned());
        }
    }
    deps
}

fn cargo_manifest_text(evidence: &ProjectProfileEvidence) -> String {
    [
        evidence.cargo.as_deref().unwrap_or_default(),
        evidence.root_cargo.as_deref().unwrap_or_default(),
    ]
    .join("\n")
}

fn cargo_dependency_present(evidence: &ProjectProfileEvidence, dependency: &str) -> bool {
    let manifest = cargo_manifest_text(evidence);
    manifest.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with(&format!("{dependency} ="))
            || trimmed.starts_with(&format!("{dependency}="))
    })
}

fn infer_project_app_name(
    package: Option<&Value>,
    composer: Option<&Value>,
    tauri: Option<&Value>,
    pyproject: Option<&str>,
    go_mod: Option<&str>,
    root: &Path,
) -> Option<String> {
    tauri
        .and_then(|value| value.get("productName"))
        .and_then(Value::as_str)
        .and_then(normalized_non_empty)
        .or_else(|| json_string_field(package, "name"))
        .or_else(|| json_string_field(composer, "name"))
        .or_else(|| pyproject_project_name(pyproject))
        .or_else(|| go_module_name(go_mod))
        .or_else(|| {
            root.file_name()
                .and_then(|value| value.to_str())
                .and_then(normalized_non_empty)
        })
}

fn json_string_field(value: Option<&Value>, key: &str) -> Option<String> {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .and_then(normalized_non_empty)
}

fn pyproject_project_name(text: Option<&str>) -> Option<String> {
    let mut in_project = false;
    for line in text?.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_project = trimmed == "[project]" || trimmed == "[tool.poetry]";
            continue;
        }
        if in_project && trimmed.starts_with("name") {
            return value_after_equals(trimmed);
        }
    }
    None
}

fn go_module_name(text: Option<&str>) -> Option<String> {
    text?.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("module ")
            .and_then(normalized_non_empty)
    })
}

fn value_after_equals(line: &str) -> Option<String> {
    line.split_once('=')
        .map(|(_, value)| value.trim().trim_matches('"').trim_matches('\''))
        .and_then(normalized_non_empty)
}

fn has_composer_script(composer: Option<&Value>, script: &str) -> bool {
    composer
        .and_then(|value| value.get("scripts"))
        .and_then(Value::as_object)
        .is_some_and(|scripts| scripts.contains_key(script))
}

fn has_php_framework(evidence: &ProjectProfileEvidence, framework: &str) -> bool {
    let deps = composer_dependencies(evidence.composer.as_ref());
    match framework {
        "laravel" => {
            deps.iter().any(|dep| dep == "laravel/framework")
                || evidence_has_path(evidence, "artisan")
        }
        "symfony" => {
            deps.iter().any(|dep| dep.starts_with("symfony/"))
                || evidence_has_path(evidence, "bin")
                    && evidence.directories.iter().any(|dir| dir == "config")
        }
        "thinkphp" => deps.iter().any(|dep| dep == "topthink/framework"),
        "dux" => {
            deps.iter()
                .any(|dep| dep.contains("dux") || dep.starts_with("duxweb/"))
                || json_string_field(evidence.composer.as_ref(), "name")
                    .is_some_and(|name| name.to_lowercase().contains("dux"))
        }
        _ => false,
    }
}

fn evidence_has_path(evidence: &ProjectProfileEvidence, name: &str) -> bool {
    evidence.directories.iter().any(|dir| dir == name) || evidence.root_markers.contains(name)
}

fn root_file_markers(root: &Path) -> HashSet<String> {
    [
        "artisan",
        "bin/console",
        "manage.py",
        "requirements.txt",
        "Dockerfile",
        ".env.example",
    ]
    .into_iter()
    .filter(|name| root.join(name).exists())
    .map(str::to_string)
    .collect()
}

fn detect_package_manager(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").exists() || root.join("pnpm-workspace.yaml").exists() {
        "pnpm"
    } else if root.join("bun.lock").exists() || root.join("bun.lockb").exists() {
        "bun"
    } else if root.join("yarn.lock").exists() {
        "yarn"
    } else {
        "npm"
    }
}

fn package_exec_command(manager: &str, command: &str) -> String {
    match manager {
        "pnpm" => format!("pnpm exec {command}"),
        "bun" => format!("bunx {command}"),
        "yarn" => format!("yarn {command}"),
        _ => format!("npx {command}"),
    }
}

fn contains_token(text: &str, token: &str) -> bool {
    text.to_lowercase().contains(&token.to_lowercase())
}

fn top_level_directories(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target" | "dist")
            {
                return None;
            }
            Some(name)
        })
        .collect::<Vec<_>>();
    dirs.sort();
    dirs.truncate(16);
    dirs
}

fn infer_project_modules(root: &Path, evidence: &ProjectProfileEvidence) -> Vec<String> {
    let mut modules = Vec::new();
    for path in [
        "src",
        "app",
        "app/Http/Controllers",
        "routes",
        "config",
        "database/migrations",
        "modules",
        "packages",
        "cmd",
        "internal",
        "pkg",
    ] {
        modules.extend(
            source_module_names(&root.join(path))
                .into_iter()
                .map(|name| {
                    if path == "src" {
                        name
                    } else {
                        format!("{path}/{name}")
                    }
                }),
        );
    }
    if has_php_framework(evidence, "laravel") {
        for path in ["app/Models", "app/Providers", "resources/views", "tests"] {
            if root.join(path).is_dir() {
                modules.push(path.to_string());
            }
        }
    }
    sorted_unique_strings(modules)
        .into_iter()
        .take(28)
        .collect()
}

fn source_module_names(src: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(src) else {
        return Vec::new();
    };
    let mut modules = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                return Some(name);
            }
            path.extension()
                .and_then(|extension| extension.to_str())
                .filter(|extension| {
                    matches!(
                        *extension,
                        "ts" | "tsx" | "rs" | "php" | "py" | "go" | "java" | "kt" | "rb"
                    )
                })
                .map(|_| name)
        })
        .collect::<Vec<_>>();
    modules.sort();
    modules.truncate(24);
    modules
}

fn bullet_lines(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("- {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sorted_unique_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut output = values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect::<Vec<_>>();
    output.sort();
    output
}

fn memory_query_terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    query
        .split(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    ',' | '.'
                        | ';'
                        | ':'
                        | '/'
                        | '\\'
                        | '|'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '<'
                        | '>'
                        | '"'
                        | '\''
                        | '`'
                )
        })
        .filter_map(|term| {
            let normalized = term.trim().to_lowercase();
            if normalized.chars().count() < 2 || !seen.insert(normalized.clone()) {
                return None;
            }
            Some(normalized)
        })
        .take(MEMORY_RETRIEVAL_MAX_QUERY_TERMS)
        .collect()
}

fn memory_fts_query(query: &str) -> Option<String> {
    let terms = memory_query_terms(query)
        .into_iter()
        .filter(|term| {
            term.chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
        .take(12)
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    normalized_non_empty(&terms.join(" OR "))
}

fn document_tool_name(tool: &str) -> &'static str {
    match tool {
        "codex" => "Codex",
        "claude" | "claude-code" => "Claude Code",
        "gemini" => "Gemini",
        _ => "AI tool",
    }
}

fn unique_entries(entries: Vec<MemoryEntry>) -> Vec<MemoryEntry> {
    let mut seen = HashSet::new();
    entries
        .into_iter()
        .filter(|entry| seen.insert(entry.id.clone()))
        .collect()
}

fn preferred_tier(existing: &MemoryTier, candidate: &MemoryTier) -> MemoryTier {
    match (existing, candidate) {
        (MemoryTier::Core, _) | (_, MemoryTier::Core) => MemoryTier::Core,
        (MemoryTier::Working, _) | (_, MemoryTier::Working) => MemoryTier::Working,
        _ => MemoryTier::Archive,
    }
}

fn should_skip_memory_candidate(candidate: &MemoryCandidate) -> bool {
    let normalized = normalized_memory_content(&candidate.content);
    if normalized.chars().count() < 12 {
        return true;
    }
    let terms = memory_similarity_terms(&normalized);
    terms.len() < 2 && normalized.chars().count() < 28
}

fn memory_similarity(left: &str, right: &str) -> f64 {
    let left_norm = normalized_memory_content(left);
    let right_norm = normalized_memory_content(right);
    if left_norm.is_empty() || right_norm.is_empty() {
        return 0.0;
    }
    if left_norm == right_norm {
        return 1.0;
    }
    let left_terms = memory_similarity_terms(&left_norm);
    let right_terms = memory_similarity_terms(&right_norm);
    let term_score = jaccard_score(&left_terms, &right_terms);
    let left_grams = memory_char_ngrams(&left_norm);
    let right_grams = memory_char_ngrams(&right_norm);
    let char_score = jaccard_score(&left_grams, &right_grams);
    term_score.max(char_score)
}

fn memory_similarity_terms(value: &str) -> HashSet<String> {
    memory_query_terms(value)
        .into_iter()
        .filter(|term| term.chars().count() >= 2)
        .collect()
}

fn memory_char_ngrams(value: &str) -> HashSet<String> {
    let chars = value
        .chars()
        .filter(|ch| !ch.is_whitespace() && !ch.is_ascii_punctuation())
        .collect::<Vec<_>>();
    if chars.len() < 2 {
        return HashSet::new();
    }
    chars
        .windows(2)
        .map(|pair| pair.iter().collect::<String>())
        .collect()
}

fn jaccard_score(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(right).count() as f64;
    let union = left.union(right).count() as f64;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn memory_candidate_conflicts(candidate: &MemoryCandidate, existing: &MemoryEntry) -> bool {
    let left = normalized_memory_content(&candidate.content);
    let right = normalized_memory_content(&existing.content);
    if left == right {
        return false;
    }
    has_memory_change_signal(&left)
        || has_memory_change_signal(&right)
        || memory_has_conflicting_architecture(&left, &right)
        || memory_has_conflicting_command_policy(&left, &right)
}

fn has_memory_change_signal(value: &str) -> bool {
    [
        "now",
        "instead",
        "replace",
        "replaced",
        "no longer",
        "deprecated",
        "switch to",
        "changed to",
        "改为",
        "换成",
        "现在",
        "不再",
        "不用",
        "不要",
    ]
    .iter()
    .any(|needle| value.contains(needle))
}

fn memory_has_conflicting_architecture(left: &str, right: &str) -> bool {
    let markers = ["arm64", "aarch64", "x86_64", "universal", "mixed"];
    let left_markers = markers
        .iter()
        .filter(|marker| left.contains(**marker))
        .collect::<HashSet<_>>();
    let right_markers = markers
        .iter()
        .filter(|marker| right.contains(**marker))
        .collect::<HashSet<_>>();
    !left_markers.is_empty() && !right_markers.is_empty() && left_markers != right_markers
}

fn memory_has_conflicting_command_policy(left: &str, right: &str) -> bool {
    let left_commands = extract_command_like_terms(left);
    let right_commands = extract_command_like_terms(right);
    if left_commands.is_empty() || right_commands.is_empty() || left_commands == right_commands {
        return false;
    }
    has_memory_change_signal(left) || has_memory_change_signal(right)
}

fn extract_command_like_terms(value: &str) -> HashSet<String> {
    value
        .split_whitespace()
        .filter(|term| {
            term.contains('/')
                || term.contains("--")
                || matches!(
                    *term,
                    "pnpm" | "npm" | "yarn" | "cargo" | "tauri" | "build" | "test" | "check"
                )
        })
        .map(|term| {
            term.trim_matches(|ch: char| ch == '`' || ch == ',' || ch == '.' || ch == ';')
                .to_string()
        })
        .collect()
}

fn merge_memory_content(existing: &str, candidate: &str) -> String {
    if memory_similarity(existing, candidate) >= 0.82 {
        return if candidate.chars().count() > existing.chars().count() {
            candidate.to_string()
        } else {
            existing.to_string()
        };
    }
    let merged = format!("{}; {}", existing.trim(), candidate.trim());
    if merged.chars().count() <= 220 {
        merged
    } else if candidate.chars().count() > existing.chars().count() {
        candidate.to_string()
    } else {
        existing.to_string()
    }
}

fn merge_optional_memory_text(left: Option<&str>, right: Option<&str>) -> Option<String> {
    match (
        left.and_then(normalized_non_empty),
        right.and_then(normalized_non_empty),
    ) {
        (Some(left), Some(right)) if memory_similarity(&left, &right) < 0.75 => {
            let merged = format!("{}; {}", left, right);
            Some(if merged.chars().count() <= 240 {
                merged
            } else if right.chars().count() > left.chars().count() {
                right
            } else {
                left
            })
        }
        (Some(left), Some(right)) => Some(if right.chars().count() > left.chars().count() {
            right
        } else {
            left
        }),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn select_memory_provider<'a>(
    settings: &'a AISettings,
    tool: Option<&str>,
) -> Option<&'a AIProviderSettings> {
    let requested = settings.memory.default_extractor_provider_id.trim();
    if !requested.is_empty() && requested != "automatic" {
        if let Some(provider) = settings.providers.iter().find(|provider| {
            provider.id == requested
                && provider.is_enabled
                && provider.use_for_memory_extraction
                && supports_completion(&provider.kind)
        }) {
            return Some(provider);
        }
    }
    let normalized_tool = tool
        .and_then(normalized_non_empty)
        .map(|value| value.to_lowercase());
    settings
        .providers
        .iter()
        .filter(|provider| {
            provider.is_enabled
                && provider.use_for_memory_extraction
                && supports_completion(&provider.kind)
        })
        .min_by(|left, right| {
            let left_tool_bonus = i32::from(
                normalized_tool
                    .as_ref()
                    .is_some_and(|tool| left.display_name.to_lowercase().contains(tool)),
            );
            let right_tool_bonus = i32::from(
                normalized_tool
                    .as_ref()
                    .is_some_and(|tool| right.display_name.to_lowercase().contains(tool)),
            );
            (left.priority - left_tool_bonus)
                .cmp(&(right.priority - right_tool_bonus))
                .then_with(|| left.display_name.cmp(&right.display_name))
        })
}

fn ensure_memory_provider_available(settings: &AISettings) -> Result<()> {
    if select_memory_provider(settings, None).is_some() {
        Ok(())
    } else {
        Err(anyhow!(
            "Memory needs an enabled AI provider with Use For Memory Extraction turned on."
        ))
    }
}

fn provider_summary(provider: &AIProviderSettings) -> String {
    format!(
        "provider={} id={} kind={} model={} base_url={}",
        provider.display_name, provider.id, provider.kind, provider.model, provider.base_url
    )
}

fn supports_completion(kind: &str) -> bool {
    matches!(
        kind,
        "openai"
            | "openAICompatible"
            | "anthropic"
            | "deepseek"
            | "gemini"
            | "groq"
            | "openrouter"
            | "ollama"
            | "localLlama"
    )
}

fn extraction_system_prompt() -> &'static str {
    "You extract and compact durable software-engineering memory from AI coding sessions.\n\nReturn JSON only.\nDo not include markdown fences.\nDo not include <think> blocks, reasoning text, analysis, explanations, or prose.\nThe first non-whitespace character of the response must be \"{\".\nDo not call tools, request scans, browse files, or infer facts outside the provided transcript and existing memory.\nTreat this as a deterministic memory compaction job, not a chat response."
}

fn make_extraction_prompt(
    transcript: &str,
    user_summary: Option<&MemorySummary>,
    user_memories: &[MemoryEntry],
    project_memories: &[MemoryEntry],
    project_name: &str,
    settings: &AIMemorySettings,
) -> String {
    format!(
        "Memory extraction schema: codux-memory-v4\nProject: {project_name}\n\nExisting user summary:\n{}\n\nRelevant user memories:\n{}\n\nRelevant project memories:\n{}\n\nTranscript:\n<transcript>\n{}\n</transcript>\n\nReturn minified JSON only, with no markdown and no line breaks:\n{{\"user_summary\":\"\",\"working_add\":[],\"working_archive\":[],\"merged_entry_ids\":[],\"project_profile_refresh_recommended\":false}}\n\nRules:\n- If nothing durable should be stored, return the exact empty JSON shape above.\n- Add at most 3 working_add items total. Prefer the highest value durable memories only.\n- Each working_add item must include content, kind, tier, scope, and module_key. scope must be exactly user or project; module_key must be a non-empty concise module name.\n- Optional item fields: merge_with, replace, archive, skip_reason.\n- project_profile_refresh_recommended must be true only when the transcript reveals likely project-wide changes to purpose, architecture, tech stack, major modules, or common commands. Do not set it for ordinary bug fixes, logs, or task progress.\n- merge_with and replace must be a single existing UUID string, not an array. If multiple existing memories are duplicates, set merge_with to the best target id and put the other duplicate ids in archive.\n- Use merge_with for semantic duplicates, replace for conflicts where the new memory supersedes an old entry, archive for stale or duplicate entry ids, skip_reason for candidates that should not be stored.\n- Keep each working_add.content concise: target 120-220 Chinese characters or 60-110 English words. Summarize the memory naturally; do not hard-truncate, cut mid-sentence, or drop critical qualifiers just to hit a number.\n- Keep rationale short: target one brief sentence. Summarize rather than truncating.\n- user_summary <= about {} tokens; empty string means keep existing user summary unchanged. If it would exceed the budget, rewrite it as a compact summary instead of truncating it.\n- Do not produce project_summary. Project profile is generated from repository files, not chat transcripts.\n- Extract only durable engineering memory. Omit temporary tasks, logs, timestamps, greetings, tool output, generic knowledge, and assistant-invented preferences.\n- scope=user only for explicit cross-project user habits/preferences; user entries should use module_key=\"user\".\n- Repository facts, commands, release flow, UI decisions, bugs, diagnostics, paths, APIs, and conventions must be scope=project and assigned a concise module_key such as frontend, tauri, terminal, memory, git, release, remote, pet, performance, or general.\n- Ambiguous or low-value information must be omitted.\n- kind must be preference, convention, decision, fact, or bug_lesson. tier must be core or working.",
        render_existing_summary(user_summary),
        render_existing_memories(user_memories),
        render_existing_memories(project_memories),
        trim_memory_text(transcript, settings.max_extraction_transcript_tokens),
        settings.summary_target_token_budget
    )
}

fn render_existing_summary(summary: Option<&MemorySummary>) -> String {
    summary
        .and_then(|summary| {
            normalized_non_empty(&summary.content)
                .map(|content| format!("version={}\n{}", summary.version, content))
        })
        .unwrap_or_else(|| "(none)".to_string())
}

fn render_existing_memories(entries: &[MemoryEntry]) -> String {
    if entries.is_empty() {
        return "(none)".to_string();
    }
    entries
        .iter()
        .map(|entry| {
            if let Some(rationale) = normalized_non_empty(entry.rationale.as_deref().unwrap_or(""))
            {
                format!(
                    "- id={} module={} [{}] {} (context: {})",
                    entry.id,
                    entry.module_key.as_deref().unwrap_or(DEFAULT_MEMORY_MODULE),
                    entry.kind.as_str(),
                    entry.content,
                    rationale
                )
            } else {
                format!(
                    "- id={} module={} [{}] {}",
                    entry.id,
                    entry.module_key.as_deref().unwrap_or(DEFAULT_MEMORY_MODULE),
                    entry.kind.as_str(),
                    entry.content
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_extraction_response(raw: &str) -> Result<MemoryExtractionResponse> {
    let stripped = strip_markdown_code_fences(raw);
    for value in llm_json_values(&stripped) {
        if let Some(response) = parse_extraction_value(&value) {
            return Ok(response);
        }
    }
    Err(anyhow!(
        "Memory extraction provider returned malformed memory JSON."
    ))
}

fn llm_json_values(raw: &str) -> Vec<Value> {
    let mut values = Vec::new();
    push_unique_json_value(&mut values, serde_json::from_str::<Value>(raw).ok());
    push_unique_json_value(&mut values, llm_json_repair::parse::<Value>(raw).ok());
    for candidate in json_object_candidates(raw) {
        push_unique_json_value(&mut values, serde_json::from_str::<Value>(&candidate).ok());
        push_unique_json_value(
            &mut values,
            llm_json_repair::parse::<Value>(&candidate).ok(),
        );
    }
    values
}

fn push_unique_json_value(values: &mut Vec<Value>, value: Option<Value>) {
    let Some(value) = value else {
        return;
    };
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn strip_markdown_code_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    trimmed
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn should_stop_memory_queue_after_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_lowercase();
    [
        "provider returned http",
        "quota",
        "rate limit",
        "rate_limit",
        "too many requests",
        "429",
        "401",
        "403",
        "api key",
        "empty response",
        "malformed memory json",
        "response body could not be decoded",
        "transport",
        "compression",
        "gateway error",
        "context window",
        "maximum context",
        "timeout",
        "timed out",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn parse_extraction_value(value: &Value) -> Option<MemoryExtractionResponse> {
    if let Some(array) = value.as_array() {
        let working_add = array
            .iter()
            .filter_map(parse_extraction_item)
            .collect::<Vec<_>>();
        if working_add.is_empty() {
            return None;
        }
        return Some(MemoryExtractionResponse {
            working_add,
            ..MemoryExtractionResponse::default()
        });
    }
    let object = value.as_object()?;
    let nested = ["memory", "response", "result"]
        .iter()
        .filter_map(|key| object.get(*key))
        .find_map(parse_extraction_value);
    if nested.is_some() {
        return nested;
    }
    let user_summary = string_from_keys(
        value,
        &[
            "user_summary",
            "userSummary",
            "user-summary",
            "global_summary",
        ],
    );
    let mut working_add = array_from_keys(
        value,
        &[
            "working_add",
            "workingAdd",
            "working-add",
            "memories",
            "memory_entries",
            "items",
        ],
    )
    .into_iter()
    .filter_map(parse_extraction_item)
    .collect::<Vec<_>>();
    if working_add.is_empty() {
        if let Some(item) = parse_extraction_item(value) {
            working_add.push(item);
        }
    }
    let working_archive = string_array_from_keys(
        value,
        &[
            "working_archive",
            "workingArchive",
            "working-archive",
            "archive_ids",
        ],
    );
    let merged_entry_ids = string_array_from_keys(
        value,
        &[
            "merged_entry_ids",
            "mergedEntryIDs",
            "merged-entry-ids",
            "merged_ids",
        ],
    );
    let project_profile_refresh_recommended = bool_from_keys(
        value,
        &[
            "project_profile_refresh_recommended",
            "projectProfileRefreshRecommended",
            "refresh_project_profile",
            "refreshProjectProfile",
            "project_profile_stale",
            "projectProfileStale",
        ],
    )
    .unwrap_or(false);
    Some(MemoryExtractionResponse {
        user_summary,
        working_add,
        working_archive,
        merged_entry_ids,
        project_profile_refresh_recommended,
    })
}

fn parse_extraction_item(value: &Value) -> Option<MemoryExtractionItem> {
    let content = string_from_keys(value, &["content", "memory", "text", "summary", "value"])?;
    let mut merge_with = uuid_array_from_keys(
        value,
        &[
            "merge_with",
            "mergeWith",
            "merge-entry-id",
            "merge_entry_id",
        ],
    );
    merge_with = unique_strings(merge_with);
    let replace = uuid_array_from_keys(
        value,
        &[
            "replace",
            "replace_id",
            "replaceId",
            "supersedes",
            "supersedes_id",
        ],
    )
    .into_iter()
    .next();
    let mut archive = uuid_array_from_keys(value, &["archive", "archive_ids", "archiveIds"]);
    archive = unique_strings(archive);
    Some(MemoryExtractionItem {
        scope: string_from_keys(value, &["scope", "target", "level"])
            .map(|value| MemoryScope::from_str(&value)),
        module_key: string_from_keys(value, &["module_key", "moduleKey", "module", "area"])
            .and_then(|value| normalized_memory_module(&value)),
        tier: string_from_keys(value, &["tier", "priority", "stability"])
            .map(|value| MemoryTier::from_str(&value)),
        kind: string_from_keys(value, &["kind", "type", "category", "memory_type"])
            .map(|value| MemoryKind::from_str(&value))
            .unwrap_or(MemoryKind::Fact),
        content,
        rationale: string_from_keys(value, &["rationale", "reason", "context", "source", "why"]),
        merge_with,
        replace,
        archive,
        skip_reason: string_from_keys(value, &["skip_reason", "skipReason", "skip"]),
    })
}

fn json_object_candidates(raw: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let bytes = raw.as_bytes();
    for (start, byte) in bytes.iter().enumerate() {
        if *byte != b'{' && *byte != b'[' {
            continue;
        }
        let mut stack = Vec::new();
        let mut in_string = false;
        let mut escaped = false;
        for (offset, current) in bytes[start..].iter().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if *current == b'\\' {
                    escaped = true;
                } else if *current == b'"' {
                    in_string = false;
                }
                continue;
            }
            match *current {
                b'"' => in_string = true,
                b'{' | b'[' => stack.push(*current),
                b'}' => {
                    if stack.pop() != Some(b'{') {
                        break;
                    }
                    if stack.is_empty() {
                        candidates.push(raw[start..=start + offset].to_string());
                        break;
                    }
                }
                b']' => {
                    if stack.pop() != Some(b'[') {
                        break;
                    }
                    if stack.is_empty() {
                        candidates.push(raw[start..=start + offset].to_string());
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    candidates
}

fn string_from_keys(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(value) = object
            .get(*key)
            .and_then(|value| value.as_str())
            .and_then(normalized_non_empty)
        {
            return Some(value);
        }
    }
    None
}

fn bool_from_keys(value: &Value, keys: &[&str]) -> Option<bool> {
    let object = value.as_object()?;
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if let Some(value) = value.as_bool() {
            return Some(value);
        }
        if let Some(text) = value.as_str().map(|text| text.trim().to_lowercase()) {
            match text.as_str() {
                "true" | "yes" | "1" => return Some(true),
                "false" | "no" | "0" => return Some(false),
                _ => {}
            }
        }
    }
    None
}

fn array_from_keys<'a>(value: &'a Value, keys: &[&str]) -> Vec<&'a Value> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    for key in keys {
        if let Some(array) = object.get(*key).and_then(|value| value.as_array()) {
            return array.iter().collect();
        }
    }
    Vec::new()
}

fn string_array_from_keys(value: &Value, keys: &[&str]) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if let Some(array) = value.as_array() {
            return array
                .iter()
                .filter_map(|item| item.as_str().and_then(normalized_non_empty))
                .collect();
        }
        if let Some(text) = value.as_str().and_then(normalized_non_empty) {
            return vec![text];
        }
    }
    Vec::new()
}

fn uuid_array_from_keys(value: &Value, keys: &[&str]) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if let Some(array) = value.as_array() {
            return array
                .iter()
                .filter_map(|item| item.as_str().and_then(parse_uuid_string))
                .collect();
        }
        if let Some(uuid) = value.as_str().and_then(parse_uuid_string) {
            return vec![uuid];
        }
    }
    Vec::new()
}

fn unique_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn decode_string_array(value: Option<&str>) -> Vec<String> {
    value
        .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

fn sorted_unique(values: &[String]) -> Vec<String> {
    let mut values = values
        .iter()
        .filter_map(|value| parse_uuid_string(value))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn parse_uuid_string(value: &str) -> Option<String> {
    let normalized = normalized_non_empty(value)?;
    Uuid::parse_str(&normalized).ok()?;
    Some(normalized)
}

fn normalized_memory_content(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn normalized_memory_module(value: &str) -> Option<String> {
    let token = normalized_token(value);
    if token.is_empty() {
        return None;
    }
    let module = match token.as_str() {
        "front" | "frontend" | "ui" | "react" | "web" => "frontend",
        "backend" | "rust" | "tauri" | "desktop" => "tauri",
        "term" | "terminal" | "pty" | "shell" => "terminal",
        "memory" | "aimemory" | "ai_memory" | "context" => "memory",
        "git" | "worktree" | "diff" => "git",
        "release" | "build" | "bundle" | "updater" | "packaging" => "release",
        "remote" | "mobile" | "handoff" | "ssh" => "remote",
        "pet" | "pets" | "companion" => "pet",
        "settings" | "config" | "preferences" => "settings",
        "performance" | "perf" | "metrics" => "performance",
        _ => token.as_str(),
    };
    Some(module.chars().take(48).collect())
}

fn infer_memory_module_from_text(value: &str) -> String {
    let text = value.to_lowercase();
    for (needle, module) in [
        ("performance", "performance"),
        ("longtask", "performance"),
        ("memory", "memory"),
        ("terminal", "terminal"),
        ("xterm", "terminal"),
        ("tauri", "tauri"),
        ("rust", "tauri"),
        ("react", "frontend"),
        ("frontend", "frontend"),
        ("ui", "frontend"),
        ("git", "git"),
        ("release", "release"),
        ("build", "release"),
        ("mobile", "remote"),
        ("ssh", "remote"),
        ("pet", "pet"),
    ] {
        if text.contains(needle) {
            return module.to_string();
        }
    }
    DEFAULT_MEMORY_MODULE.to_string()
}

fn valid_summary_content(value: &str) -> Option<String> {
    let content = normalized_non_empty(value)?;
    if content.starts_with("version=") && content.lines().count() == 1 {
        return None;
    }
    Some(content)
}

fn estimate_tokens(value: &str) -> i64 {
    (value.chars().count() as i64 + 3) / 4
}

fn trim_memory_text(text: &str, max_tokens: i32) -> String {
    let max_chars = (max_tokens.max(50) as usize * 3).max(200);
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!(
        "{}\n[Memory extraction input truncated]",
        text.chars()
            .take(max_chars)
            .collect::<String>()
            .trim()
            .to_string()
    )
}

fn compact_transcript_for_memory(text: &str, token_limit: i32) -> Option<String> {
    let mut output = Vec::new();
    let mut omitted_low_signal = 0usize;
    let mut in_code_block = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            omitted_low_signal += 1;
            continue;
        }
        if in_code_block {
            omitted_low_signal += 1;
            continue;
        }
        let char_count = trimmed.chars().count();
        if looks_like_tool_or_log_line(trimmed) {
            omitted_low_signal += 1;
            continue;
        }
        if char_count > 700 {
            output.push(format!(
                "{} … {} [omitted long pasted content, {} chars]",
                trimmed.chars().take(160).collect::<String>().trim(),
                tail_chars(trimmed, 80),
                char_count
            ));
            continue;
        }
        output.push(trimmed.to_string());
    }
    if omitted_low_signal > 0 {
        output.push(format!(
            "[omitted {} low-signal code/log/tool-output lines before memory extraction]",
            omitted_low_signal
        ));
    }
    normalized_non_empty(&trim_memory_text(&output.join("\n"), token_limit))
}

fn looks_like_tool_or_log_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    let prefixes = [
        "stdout:",
        "stderr:",
        "tool:",
        "assistant.tool",
        "user.tool",
        "[tool]",
        "[stdout]",
        "[stderr]",
        "trace:",
        "debug:",
    ];
    prefixes.iter().any(|prefix| lower.starts_with(prefix))
        || (line.len() > 260 && line.chars().filter(|ch| ch.is_ascii_punctuation()).count() > 60)
}

fn tail_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars().rev().take(max_chars).collect::<Vec<_>>();
    chars.reverse();
    chars.into_iter().collect::<String>()
}

fn transcript_source_if_readable(
    path: &str,
    tool: &str,
    session_id: &str,
    allow_database: bool,
) -> Option<TranscriptSource> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() == 0 {
        return None;
    }
    if !allow_database && read_transcript_file(path, 80, 8000).is_none() {
        return None;
    }
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs_f64())
        .unwrap_or(0.0);
    Some(TranscriptSource {
        location: path.to_string(),
        fingerprint: sha256_hex(&format!(
            "{tool}|{session_id}|{path}|{}|{modified_at}",
            metadata.len()
        )),
    })
}

fn read_transcript_file(path: &str, line_limit: i32, token_limit: i32) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text
        .lines()
        .rev()
        .take(line_limit as usize)
        .collect::<Vec<_>>();
    lines.reverse();
    compact_transcript_for_memory(lines.join("\n").trim(), token_limit)
}

fn fetch_opencode_transcript(
    project_path: &str,
    external_session_id: &str,
    database_path: &str,
) -> Option<String> {
    let conn = Connection::open(database_path).ok()?;
    let mut statement = conn
        .prepare(
            r#"
            SELECT json_extract(m.data, '$.role') AS role,
                   COALESCE(json_extract(m.data, '$.time.created'), '') AS created_at,
                   COALESCE(json_extract(m.data, '$.content'), json_extract(p.data, '$.text'), json_extract(p.data, '$.state.output'), '') AS content,
                   COALESCE(json_extract(m.data, '$.path.root'), s.directory, '') AS root_path,
                   COALESCE(json_extract(p.data, '$.type'), '') AS part_type,
                   COALESCE(json_extract(p.data, '$.tool'), '') AS tool_name
            FROM session s
            JOIN message m ON m.session_id = s.id
            LEFT JOIN part p ON p.message_id = m.id
            WHERE s.id = ?1
              AND s.time_archived IS NULL
            ORDER BY m.time_created ASC, p.time_created ASC;
            "#,
        )
        .ok()?;
    let rows = statement
        .query_map(params![external_session_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .ok()?;
    let mut lines = Vec::new();
    for row in rows.flatten() {
        let (role, created_at, content, root_path, part_type, tool_name) = row;
        if !paths_equivalent(root_path.as_deref(), project_path) {
            continue;
        }
        let Some(content) = content.and_then(|value| normalized_non_empty(&value)) else {
            continue;
        };
        let role = role.unwrap_or_else(|| "assistant".to_string());
        let prefix = if part_type.as_deref() == Some("tool") {
            format!("{}.tool[{}]", role, tool_name.unwrap_or_default())
        } else {
            role
        };
        lines.push(format!(
            "[{}] {}: {}",
            created_at.unwrap_or_default(),
            prefix,
            content
        ));
    }
    let text = lines
        .into_iter()
        .rev()
        .take(120)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    compact_transcript_for_memory(&text, 8000)
}

fn find_codex_rollout_path(project_path: &str, external_session_id: &str) -> Option<PathBuf> {
    recursive_files(&home_dir().join(".codex").join("sessions"), "jsonl")
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.contains(external_session_id))
                .unwrap_or(false)
                || codex_file_belongs_to_project(path, project_path)
        })
        .max_by_key(|path| file_modified_millis(path).unwrap_or(0))
}

fn codex_file_belongs_to_project(path: &Path, project_path: &str) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(std::result::Result::ok).take(20) {
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let row_type = row.get("type").and_then(|value| value.as_str());
        let payload = row.get("payload").unwrap_or(&Value::Null);
        if matches!(row_type, Some("session_meta") | Some("turn_context")) {
            if let Some(cwd) = payload.get("cwd").and_then(|value| value.as_str()) {
                return paths_equivalent(Some(cwd), project_path);
            }
        }
    }
    false
}

fn claude_project_log_paths(project_path: &str) -> Vec<PathBuf> {
    let directory_name = project_path.replace('/', "-").replace('.', "-");
    let direct_dir = home_dir()
        .join(".claude")
        .join("projects")
        .join(directory_name);
    let direct = directory_files(&direct_dir, "jsonl");
    if !direct.is_empty() {
        return direct;
    }
    recursive_files(&home_dir().join(".claude").join("projects"), "jsonl")
        .into_iter()
        .filter(|path| claude_log_contains_project(path, project_path))
        .collect()
}

fn claude_log_contains_project(path: &Path, project_path: &str) -> bool {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(std::result::Result::ok).take(12) {
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(cwd) = row.get("cwd").and_then(|value| value.as_str()) {
            return paths_equivalent(Some(cwd), project_path);
        }
    }
    false
}

fn claude_log_contains_session(path: &Path, external_session_id: &str, project_path: &str) -> bool {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(std::result::Result::ok) {
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if row.get("sessionId").and_then(|value| value.as_str()) != Some(external_session_id) {
            continue;
        }
        if let Some(cwd) = row.get("cwd").and_then(|value| value.as_str()) {
            return paths_equivalent(Some(cwd), project_path);
        }
        return true;
    }
    false
}

fn gemini_session_paths(project_path: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root_dir in gemini_data_roots() {
        let temp_dir = root_dir.join("tmp");
        let projects_path = root_dir.join("projects.json");
        if let Ok(data) = fs::read(&projects_path) {
            if let Ok(root) = serde_json::from_slice::<Value>(&data) {
                if let Some(projects) = root.get("projects").and_then(|value| value.as_object()) {
                    for (stored_path, value) in projects {
                        if paths_equivalent(Some(stored_path), project_path) {
                            if let Some(directory) = value.as_str().and_then(normalized_non_empty) {
                                dirs.push(temp_dir.join(directory));
                            }
                        }
                    }
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&temp_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let marker = path.join(".project_root");
                if let Ok(value) = fs::read_to_string(marker) {
                    if paths_equivalent(Some(value.trim()), project_path) {
                        dirs.push(path);
                    }
                }
            }
        }
    }
    let mut files = Vec::new();
    for dir in dirs {
        files.extend(directory_files(&dir.join("chats"), "json"));
    }
    files.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("session-"))
            .unwrap_or(false)
    });
    files.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    files
}

fn gemini_data_roots() -> Vec<PathBuf> {
    let gemini_root = home_dir().join(".gemini");
    vec![gemini_root.clone(), gemini_root.join("antigravity-cli")]
}

fn opencode_database_path() -> PathBuf {
    home_dir()
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db")
}

fn directory_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn recursive_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_recursive_files(dir, extension, &mut files);
    files.sort();
    files
}

fn collect_recursive_files(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recursive_files(&path, extension, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            files.push(path);
        }
    }
}

fn file_modified_millis(path: &Path) -> Option<u128> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn paths_equivalent(left: Option<&str>, right: &str) -> bool {
    let Some(left) = left.and_then(normalized_non_empty) else {
        return false;
    };
    let Some(right) = normalized_non_empty(right) else {
        return false;
    };
    left.trim_end_matches('/') == right.trim_end_matches('/')
}

fn extraction_session_key(session: &AISessionSnapshot) -> String {
    [
        session.project_id.clone(),
        session.tool.to_lowercase(),
        session_identifier(session),
    ]
    .join("|")
}

fn session_identifier(session: &AISessionSnapshot) -> String {
    session
        .ai_session_id
        .as_deref()
        .and_then(normalized_non_empty)
        .unwrap_or_else(|| session.terminal_id.clone())
}

fn safe_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn normalized_non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn normalized_token(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>()
        .to_lowercase()
}

fn test_memory_injection_note() -> Option<String> {
    let value = std::env::var("CODUX_MEMORY_TEST_INJECTION").ok();
    memory_test_injection_note(value.as_deref())
}

fn merge_optional_sections(left: Option<String>, right: Option<String>) -> Option<String> {
    join_optional_sections(vec![
        left.as_deref().unwrap_or(""),
        right.as_deref().unwrap_or(""),
    ])
}

fn memory_test_injection_note(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    Some(format!(
        "# Memory Injection Test\n\nWhen the user says `xxx`, reply with `xxxx`.\nThis rule is only for verifying that launch-time memory injection was loaded.\n\nTest payload: {}",
        value
    ))
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store(name: &str) -> MemoryStore {
        let root =
            std::env::temp_dir().join(format!("codux-memory-test-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create memory test dir");
        let store = MemoryStore {
            db_path: root.join("memory.sqlite3"),
            last_enqueued_at_by_session: Mutex::new(HashMap::new()),
            recent_failure: Mutex::new(None),
            processing_queue: AtomicBool::new(false),
            cancel_requested: AtomicBool::new(false),
        };
        store.configure().expect("configure memory test store");
        store
    }

    fn seed_queue_task(store: &MemoryStore, status: &str, attempts: i64, error: Option<&str>) {
        store
            .connect()
            .expect("connect")
            .execute(
                r#"
                INSERT INTO memory_extraction_queue (
                    id, project_id, tool, session_id, transcript_path, workspace_path, source_fingerprint, status, attempts, error, enqueued_at
                ) VALUES (?1, 'project-1', 'codex', ?2, '/tmp/memory.jsonl', '/tmp', ?3, ?4, ?5, ?6, ?7);
                "#,
                params![
                    Uuid::new_v4().to_string(),
                    Uuid::new_v4().to_string(),
                    Uuid::new_v4().to_string(),
                    status,
                    attempts,
                    error,
                    now_seconds()
                ],
            )
            .expect("seed queue task");
    }

    fn seed_project_memory(store: &MemoryStore, project_id: &str, content: &str) {
        store
            .upsert(MemoryCandidate {
                scope: MemoryScope::Project,
                project_id: Some(project_id.to_string()),
                tool_id: None,
                module_key: Some(DEFAULT_MEMORY_MODULE.to_string()),
                tier: MemoryTier::Working,
                kind: MemoryKind::Fact,
                content: content.to_string(),
                rationale: None,
                source_tool: Some("codex".to_string()),
                source_session_id: Some(format!("session-{project_id}")),
                source_fingerprint: Some(format!("fingerprint-{project_id}")),
            })
            .expect("seed memory entry");
        store
            .upsert_summary(
                MemoryScope::Project,
                Some(project_id),
                None,
                &format!("{content} summary"),
                &[],
                3,
            )
            .expect("seed memory summary");
    }

    #[test]
    fn migrate_project_memory_requires_overwrite_for_existing_target() {
        let store = test_store("migration");
        seed_project_memory(&store, "old-project", "old memory");
        seed_project_memory(&store, "new-project", "new memory");

        let blocked = store.migrate_project_memory(MemoryProjectMigrationRequest {
            from_project_id: "old-project".to_string(),
            to_project_id: "new-project".to_string(),
            overwrite: false,
        });
        assert!(blocked.is_err());

        store
            .migrate_project_memory(MemoryProjectMigrationRequest {
                from_project_id: "old-project".to_string(),
                to_project_id: "new-project".to_string(),
                overwrite: true,
            })
            .expect("migrate with overwrite");

        assert_eq!(
            store
                .memory_scope_overview(MemoryScope::Project, Some("old-project"))
                .expect("old overview")
                .total_count(),
            0
        );
        assert_eq!(
            store
                .memory_scope_overview(MemoryScope::Project, Some("new-project"))
                .expect("new overview")
                .total_count(),
            2
        );
    }

    #[test]
    fn compact_transcript_for_memory_omits_bulk_content_but_keeps_memory_signals() {
        let raw = format!(
            "user: 以后这个项目统一用 WebGL 终端渲染。\n```tsx\n{}\n```\nstdout: {}\nuser: 发布前要先跑 pnpm exec tsc --noEmit。\n",
            "const value = 1;\n".repeat(40),
            "x".repeat(1000)
        );
        let compacted = compact_transcript_for_memory(&raw, 800).expect("compacted transcript");
        assert!(compacted.contains("WebGL"));
        assert!(compacted.contains("pnpm exec tsc"));
        assert!(compacted.contains("omitted"));
        assert!(!compacted.contains("const value"));
        assert!(!compacted.contains(&"x".repeat(1000)));
    }

    #[test]
    fn old_failed_extractions_do_not_pollute_runtime_status() {
        let store = test_store("failed-status");
        seed_queue_task(&store, "failed", 3, Some("old quota error"));

        let status = store.extraction_status_snapshot().expect("status snapshot");

        assert_eq!(status.status, MemoryExtractionStatus::Idle);
        assert_eq!(status.last_error, None);
    }

    #[test]
    fn context_entries_prefer_relevance_over_plain_recency() {
        let store = test_store("context-rank");
        let now = now_seconds();
        let conn = store.connect().expect("connect");
        for (content, updated_at) in [
            ("recent unrelated note", now),
            (
                "older note says WebGL renderer is required for terminals",
                now - 86_400.0 * 8.0,
            ),
            ("another unrelated note", now - 10.0),
        ] {
            conn.execute(
                r#"
                INSERT INTO memory_entries (
                    id, scope, project_id, tool_id, module_key, tier, kind, content, normalized_hash, status, created_at, updated_at
                ) VALUES (?1, 'project', 'project-1', NULL, 'terminal', 'working', 'fact', ?2, ?3, 'active', ?4, ?5);
                "#,
                params![
                    Uuid::new_v4().to_string(),
                    content,
                    normalized_memory_content(content),
                    updated_at,
                    updated_at
                ],
            )
            .expect("seed entry");
        }

        let selected = store
            .list_entries_for_context(
                MemoryScope::Project,
                Some("project-1"),
                Some("codex"),
                &[MemoryTier::Working],
                1,
                "WebGL terminal rendering",
            )
            .expect("select context");

        assert_eq!(selected.len(), 1);
        assert!(selected[0].content.contains("WebGL"));
    }

    #[test]
    fn empty_extraction_json_is_valid_noop() {
        let response = decode_extraction_response(
            r#"{"user_summary":"","working_add":[],"working_archive":[],"merged_entry_ids":[]}"#,
        )
        .expect("empty extraction response should decode");

        assert_eq!(response.user_summary, None);
        assert!(response.working_add.is_empty());
        assert!(response.working_archive.is_empty());
        assert!(response.merged_entry_ids.is_empty());
        assert!(!response.project_profile_refresh_recommended);
    }

    #[test]
    fn extraction_response_can_recommend_project_profile_refresh() {
        let response = decode_extraction_response(
            r#"{"user_summary":"","working_add":[],"working_archive":[],"merged_entry_ids":[],"project_profile_refresh_recommended":true}"#,
        )
        .expect("profile refresh recommendation should decode");

        assert!(response.project_profile_refresh_recommended);
    }

    #[test]
    fn extraction_response_decodes_repaired_llm_json() {
        let response = decode_extraction_response(
            "Sure:\n```json\n{\"working_add\":[{\"content\":\"Memory refresh uses json mode plus LLM JSON repair before business validation.\",\"kind\":\"decision\",\"tier\":\"working\",\"scope\":\"project\",\"module_key\":\"memory\",}],\"working_archive\":[],\"merged_entry_ids\":[],}\n```",
        )
        .expect("repaired extraction response should decode");

        assert_eq!(response.working_add.len(), 1);
        assert_eq!(
            response.working_add[0].module_key.as_deref(),
            Some("memory")
        );
    }

    #[test]
    fn extraction_merge_with_accepts_array_for_provider_tolerance() {
        let response = decode_extraction_response(
            r#"{"user_summary":"","working_add":[{"content":"Universal build falls back to arm64 app when x86_64 OpenSSL cross-compilation is missing.","kind":"bug_lesson","tier":"core","scope":"project","module_key":"release","merge_with":["70aaced1-87de-4567-922e-b58eab7e0998","b8bc44ce-e4e2-432e-b923-4ee70c4bce0b"],"archive":["54708809-82fe-494d-a7ed-5d03feb9bb5f"]}],"working_archive":[],"merged_entry_ids":[]}"#,
        )
        .expect("array merge_with should decode");

        assert_eq!(response.working_add.len(), 1);
        assert_eq!(
            response.working_add[0].merge_with,
            vec![
                "70aaced1-87de-4567-922e-b58eab7e0998".to_string(),
                "b8bc44ce-e4e2-432e-b923-4ee70c4bce0b".to_string()
            ]
        );
        assert_eq!(
            response.working_add[0].archive,
            vec!["54708809-82fe-494d-a7ed-5d03feb9bb5f".to_string()]
        );
    }

    #[test]
    fn semantic_duplicate_memory_merges_existing_entry() {
        let store = test_store("semantic-merge");
        let first = store
            .write_candidate_with_decision(
                MemoryCandidate {
                    scope: MemoryScope::Project,
                    project_id: Some("project-1".to_string()),
                    tool_id: None,
                    module_key: Some("release".to_string()),
                    tier: MemoryTier::Working,
                    kind: MemoryKind::Convention,
                    content: "release requires pnpm test before publishing".to_string(),
                    rationale: None,
                    source_tool: Some("codex".to_string()),
                    source_session_id: Some("session-1".to_string()),
                    source_fingerprint: Some("fingerprint-1".to_string()),
                },
                None,
            )
            .expect("write first")
            .expect("first entry");
        let second = store
            .write_candidate_with_decision(
                MemoryCandidate {
                    scope: MemoryScope::Project,
                    project_id: Some("project-1".to_string()),
                    tool_id: None,
                    module_key: Some("release".to_string()),
                    tier: MemoryTier::Working,
                    kind: MemoryKind::Convention,
                    content: "publishing release requires pnpm test".to_string(),
                    rationale: None,
                    source_tool: Some("codex".to_string()),
                    source_session_id: Some("session-2".to_string()),
                    source_fingerprint: Some("fingerprint-2".to_string()),
                },
                None,
            )
            .expect("write duplicate")
            .expect("merged entry");

        assert_eq!(first.id, second.id);
        assert_eq!(
            store
                .memory_scope_overview(MemoryScope::Project, Some("project-1"))
                .expect("overview")
                .active_entry_count,
            1
        );
    }

    #[test]
    fn conflicting_memory_replaces_old_entry() {
        let store = test_store("conflict-replace");
        let old = store
            .write_candidate_with_decision(
                MemoryCandidate {
                    scope: MemoryScope::Project,
                    project_id: Some("project-1".to_string()),
                    tool_id: None,
                    module_key: Some("release".to_string()),
                    tier: MemoryTier::Working,
                    kind: MemoryKind::Fact,
                    content: "macOS app packaging uses arm64 only".to_string(),
                    rationale: None,
                    source_tool: Some("codex".to_string()),
                    source_session_id: Some("session-1".to_string()),
                    source_fingerprint: Some("fingerprint-1".to_string()),
                },
                None,
            )
            .expect("write old")
            .expect("old entry");
        let new = store
            .write_candidate_with_decision(
                MemoryCandidate {
                    scope: MemoryScope::Project,
                    project_id: Some("project-1".to_string()),
                    tool_id: None,
                    module_key: Some("release".to_string()),
                    tier: MemoryTier::Working,
                    kind: MemoryKind::Fact,
                    content: "macOS app packaging now uses universal mixed architecture"
                        .to_string(),
                    rationale: None,
                    source_tool: Some("codex".to_string()),
                    source_session_id: Some("session-2".to_string()),
                    source_fingerprint: Some("fingerprint-2".to_string()),
                },
                None,
            )
            .expect("write replacement")
            .expect("new entry");

        assert_ne!(old.id, new.id);
        let active = store
            .list_entries_for_context(
                MemoryScope::Project,
                Some("project-1"),
                Some("codex"),
                &[MemoryTier::Working],
                5,
                "macOS app packaging architecture",
            )
            .expect("active entries");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, new.id);
    }

    #[test]
    fn injected_working_limit_does_not_trim_storage() {
        let store = test_store("context-limit");
        for index in 0..8 {
            store
                .upsert(MemoryCandidate {
                    scope: MemoryScope::Project,
                    project_id: Some("project-1".to_string()),
                    tool_id: None,
                    module_key: Some(DEFAULT_MEMORY_MODULE.to_string()),
                    tier: MemoryTier::Working,
                    kind: MemoryKind::Fact,
                    content: format!("project decision {index}"),
                    rationale: None,
                    source_tool: Some("codex".to_string()),
                    source_session_id: Some(format!("session-{index}")),
                    source_fingerprint: Some(format!("fingerprint-{index}")),
                })
                .expect("seed memory entry");
        }

        let selected = store
            .list_entries_for_context(
                MemoryScope::Project,
                Some("project-1"),
                Some("codex"),
                &[MemoryTier::Working],
                2,
                "project",
            )
            .expect("select context");
        let overview = store
            .memory_scope_overview(MemoryScope::Project, Some("project-1"))
            .expect("overview");

        assert_eq!(selected.len(), 2);
        assert_eq!(overview.active_entry_count, 8);
    }

    #[test]
    fn memory_scope_overview_reports_total_token_estimate() {
        let store = test_store("overview-token-total");
        let entry = store
            .upsert(MemoryCandidate {
                scope: MemoryScope::Project,
                project_id: Some("project-token".to_string()),
                tool_id: None,
                module_key: Some(DEFAULT_MEMORY_MODULE.to_string()),
                tier: MemoryTier::Working,
                kind: MemoryKind::Fact,
                content: "project memory token accounting includes active memory entries"
                    .to_string(),
                rationale: Some("used by memory manager overview".to_string()),
                source_tool: Some("codex".to_string()),
                source_session_id: Some("session-token".to_string()),
                source_fingerprint: Some("fingerprint-token".to_string()),
            })
            .expect("entry");
        store
            .upsert_summary(
                MemoryScope::Project,
                Some("project-token"),
                None,
                "summary memory contributes its stored token estimate",
                &[entry.id],
                10,
            )
            .expect("summary");
        store
            .upsert_project_profile(MemoryProjectProfile {
                project_id: "project-token".to_string(),
                content: "Project: Token Test\nOverview: profile tokens are counted.".to_string(),
                source_fingerprint: "profile-token".to_string(),
                created_at: now_seconds(),
                updated_at: now_seconds(),
            })
            .expect("profile");

        let overview = store
            .memory_scope_overview(MemoryScope::Project, Some("project-token"))
            .expect("overview");

        assert_eq!(overview.active_entry_count, 1);
        assert_eq!(overview.summary_count, 1);
        assert_eq!(overview.profile_count, 1);
        assert!(overview.total_token_estimate > 0);
    }

    #[test]
    fn manual_enqueue_returns_failed_extraction_to_pending() {
        let store = test_store("manual-failed");
        assert!(store
            .enqueue_extraction_if_needed(
                "project-1",
                "/tmp/project-1",
                "codex",
                "session-1",
                "/tmp/transcript.jsonl",
                "fingerprint-1",
                true,
            )
            .expect("enqueue first"));
        let task = store
            .next_pending_extraction_task()
            .expect("pending lookup")
            .expect("pending task");
        store
            .mark_extraction_task_running(&task.id)
            .expect("mark running");
        store
            .mark_extraction_task_failed(&task.id, "quota exhausted")
            .expect("mark failed");

        assert!(store
            .enqueue_extraction_if_needed(
                "project-1",
                "/tmp/project-1",
                "codex",
                "session-1",
                "/tmp/transcript.jsonl",
                "fingerprint-1",
                true,
            )
            .expect("queue failed"));
        let retried = store
            .next_pending_extraction_task()
            .expect("pending lookup")
            .expect("pending task");
        assert_eq!(retried.attempts, 1);
    }

    #[test]
    fn history_snapshot_reuses_open_project_mapping() {
        let store = test_store("history-mapping");
        let projects = vec![ProjectWorkspaceRecord {
            id: "workspace-1".to_string(),
            root_project_id: "project-1".to_string(),
            root_project_name: "Project One".to_string(),
            root_project_path: "/tmp/project-1".to_string(),
            workspace_path: "/tmp/project-1".to_string(),
            git_default_push_remote_name: None,
        }];
        let summary = AISessionSummary {
            session_id: "history-session".to_string(),
            external_session_id: Some("external-session".to_string()),
            project_id: "project-1".to_string(),
            project_name: "Project One".to_string(),
            project_path: "/tmp/project-1".to_string(),
            session_title: "Terminal".to_string(),
            first_seen_at: 10.0,
            last_seen_at: 20.0,
            last_tool: Some("codex".to_string()),
            last_model: Some("gpt-4.1".to_string()),
            request_count: 4,
            total_input_tokens: 100,
            total_output_tokens: 200,
            total_tokens: 300,
            cached_input_tokens: 40,
            active_duration_seconds: 10,
            today_tokens: 300,
            today_cached_input_tokens: 40,
        };

        let snapshot = store
            .test_history_snapshot_for_project(&projects, &summary)
            .expect("history snapshot");

        assert_eq!(snapshot.project_id, "project-1");
        assert_eq!(snapshot.project_path.as_deref(), Some("/tmp/project-1"));
        assert_eq!(snapshot.ai_session_id.as_deref(), Some("external-session"));
        assert_eq!(snapshot.state, "idle");
        assert!(snapshot.has_completed_turn);
    }

    #[test]
    fn memory_test_injection_note_embeds_test_payload() {
        let note = memory_test_injection_note(Some("probe-123")).expect("test note");
        assert!(note.contains("When the user says `xxx`, reply with `xxxx`."));
        assert!(note.contains("probe-123"));
    }

    #[test]
    fn project_profile_is_generated_from_repository_files() {
        let root = std::env::temp_dir().join(format!("codux-profile-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("src-tauri")).expect("create tauri dir");
        fs::create_dir_all(root.join("src-tauri").join("src")).expect("create tauri src dir");
        fs::create_dir_all(root.join("src").join("terminal")).expect("create src dir");
        fs::write(
            root.join("README.md"),
            "# Codux\n\nA cross-platform AI development workstation with projects, terminals, Git, AI stats, memory, mobile control, and desktop companions.\n",
        )
        .expect("write readme");
        fs::write(
            root.join("package.json"),
            r#"{"name":"codux-tauri","scripts":{"dev":"vite --host 127.0.0.1","build":"tsc && vite build","test":"vitest run","tauri":"node scripts/dev/tauri.cjs"},"dependencies":{"react":"1.0.0","@tauri-apps/api":"1.0.0","zustand":"1.0.0"},"devDependencies":{"typescript":"1.0.0","vite":"1.0.0","tailwindcss":"1.0.0"}}"#,
        )
        .expect("write package");
        fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n")
            .expect("write pnpm lock");
        fs::write(
            root.join("src-tauri").join("Cargo.toml"),
            "[package]\nname = \"codux-tauri\"\n",
        )
        .expect("write cargo");
        fs::write(
            root.join("src-tauri").join("tauri.conf.json"),
            r#"{"productName":"Codux"}"#,
        )
        .expect("write tauri");
        fs::write(
            root.join("src").join("main.tsx"),
            "import React from 'react';\nimport { createRoot } from 'react-dom/client';\ncreateRoot(document.getElementById('root')!).render(<App />);\n",
        )
        .expect("write main");
        fs::write(
            root.join("src-tauri").join("src").join("lib.rs"),
            "#[tauri::command]\nasync fn memory_refresh_project_profile() {}\n",
        )
        .expect("write lib");

        let profile = build_project_profile("project-1", "Codux", root.to_str().unwrap())
            .expect("project profile");

        assert!(profile.content.contains("Project: Codux"));
        assert!(profile.content.contains("Tech stack"));
        assert!(profile.content.contains("React"));
        assert!(profile.content.contains("Tauri"));
        assert!(profile.content.contains("pnpm build"));
        assert!(profile
            .content
            .contains("cargo check --manifest-path src-tauri/Cargo.toml"));
        assert!(profile.content.contains("Source signals"));
        assert!(profile.content.contains("src/main.tsx"));
        assert!(profile.content.contains("createRoot"));
        assert!(profile
            .content
            .contains("tauri command async fn memory_refresh_project_profile"));
    }

    #[test]
    fn project_profile_infers_php_composer_without_readme() {
        let root = std::env::temp_dir().join(format!("codux-profile-php-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("app").join("Http").join("Controllers"))
            .expect("create controller dir");
        fs::create_dir_all(root.join("routes")).expect("create routes dir");
        fs::create_dir_all(root.join("database").join("migrations"))
            .expect("create migrations dir");
        fs::write(root.join("artisan"), "#!/usr/bin/env php\n").expect("write artisan");
        fs::write(
            root.join("composer.json"),
            r#"{"name":"dux/admin","require":{"php":"^8.2","laravel/framework":"^11.0"},"scripts":{"test":"phpunit"}}"#,
        )
        .expect("write composer");
        fs::write(
            root.join("app")
                .join("Http")
                .join("Controllers")
                .join("UserController.php"),
            "<?php\n",
        )
        .expect("write controller");
        fs::write(root.join("routes").join("web.php"), "<?php\n").expect("write route");

        let profile = build_project_profile("project-php", "Dux Admin", root.to_str().unwrap())
            .expect("project profile");

        assert!(profile.content.contains("Project: dux/admin"));
        assert!(profile.content.contains("Laravel PHP application"));
        assert!(profile.content.contains("Language: PHP"));
        assert!(profile.content.contains("Package manager: Composer"));
        assert!(profile.content.contains("Framework: Laravel"));
        assert!(profile.content.contains("composer install"));
        assert!(profile.content.contains("php artisan test"));
        assert!(profile
            .content
            .contains("app/Http/Controllers/UserController.php"));
        assert!(profile.content.contains("routes/web.php"));
    }

    #[test]
    fn project_profile_infers_current_codux_workspace() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root");
        let profile = build_project_profile("current-codux", "Codux", root.to_str().unwrap())
            .expect("profile");

        assert!(profile.content.contains("Project: Codux"));
        assert!(profile.content.contains("Package manager: pnpm"));
        assert!(profile.content.contains("AI provider SDK: genai"));
        assert!(profile.content.contains("Storage: SQLite"));
        assert!(profile.content.contains("Detected modules"));
        assert!(profile.content.contains("Source signals"));
        assert!(profile.content.contains("ai"));
        assert!(profile.content.contains("desktopPet.ts"));
        assert!(profile.content.contains("src-tauri"));
    }

    #[test]
    fn project_profile_llm_cache_fingerprint_preserves_cached_profile() {
        let raw = "repo-fingerprint";
        let llm = llm_project_profile_fingerprint(raw);

        assert!(project_profile_fingerprints_match(&llm, raw));
        assert!(project_profile_fingerprints_match(&llm, &llm));
        assert!(!project_profile_fingerprints_match(
            &llm,
            "different-fingerprint"
        ));
    }

    #[test]
    fn project_profile_memory_context_changes_llm_fingerprint() {
        let context = vec![
            "memory / decision: Project profile refresh uses stored memory signals.".to_string(),
        ];
        let source = project_profile_llm_source_fingerprint("repo-fingerprint", &context);
        let content = project_profile_content_with_memory_context(
            "Project: Codux\nOverview: Workspace.",
            &context,
        );

        assert!(source.contains(":memory:"));
        assert_ne!(source, "repo-fingerprint");
        assert!(content.contains("Project memory signals"));
        assert!(content.contains("stored memory signals"));
    }

    #[test]
    fn project_profile_llm_refresh_uses_cooldown() {
        let generated = MemoryProjectProfile {
            project_id: "project-profile".to_string(),
            content: "Project: Codux\nOverview: deterministic.".to_string(),
            source_fingerprint: "repo-fingerprint".to_string(),
            created_at: 1.0,
            updated_at: now_seconds(),
        };
        let fresh_existing = MemoryProjectProfile {
            source_fingerprint: generated.source_fingerprint.clone(),
            updated_at: now_seconds(),
            ..generated.clone()
        };
        let stale_existing = MemoryProjectProfile {
            source_fingerprint: generated.source_fingerprint.clone(),
            updated_at: now_seconds() - PROJECT_PROFILE_LLM_REFRESH_COOLDOWN_SECONDS - 1.0,
            ..generated.clone()
        };
        let cached_llm = MemoryProjectProfile {
            source_fingerprint: llm_project_profile_fingerprint(&generated.source_fingerprint),
            updated_at: now_seconds() - PROJECT_PROFILE_LLM_REFRESH_COOLDOWN_SECONDS - 1.0,
            ..generated.clone()
        };

        assert!(!project_profile_llm_refresh_due(
            &fresh_existing,
            &generated
        ));
        assert!(project_profile_llm_refresh_due(&stale_existing, &generated));
        assert!(!project_profile_llm_refresh_due(&cached_llm, &generated));
    }

    #[test]
    fn project_profile_llm_prompt_requires_json_and_no_hard_truncation() {
        let prompt = make_project_profile_llm_prompt("Project: Codux\nOverview: Workspace.");

        assert!(prompt.contains("Return minified JSON only"));
        assert!(prompt.contains("Use Source signals only as repository evidence"));
        assert!(prompt.contains("Use Project memory signals only as durable"));
        assert!(prompt.contains("do not hard-truncate"));
        assert!(prompt.contains("Use only the provided deterministic profile"));
    }

    #[test]
    fn project_profile_llm_response_decodes_content() {
        let content = "Project: Codux\nOverview: AI development workstation.";
        let decoded = decode_project_profile_llm_response_detailed(&format!(
            "{{\"content\":{}}}",
            serde_json::to_string(content).expect("json string")
        ))
        .expect("decoded profile");

        assert_eq!(decoded, content);
    }

    #[test]
    fn project_profile_llm_response_decodes_repaired_json() {
        let decoded = decode_project_profile_llm_response_detailed(
            "```json\n{\"content\":\"Project: Codux\\nOverview: AI workstation.\",}\n```",
        )
        .expect("decoded repaired profile");

        assert!(decoded.contains("Project: Codux"));
        assert!(decoded.contains("Overview: AI workstation."));
    }

    #[test]
    fn project_profile_llm_response_decodes_structured_profile() {
        let decoded = decode_project_profile_llm_response_detailed(
            r#"{
                "project": "Codux",
                "overview": "A Tauri AI development workstation.",
                "tech_stack": ["Rust", "React", "SQLite"],
                "commands": ["pnpm test"]
            }"#,
        )
        .expect("decoded structured profile");

        assert!(decoded.contains("Project: Codux"));
        assert!(decoded.contains("Overview: A Tauri AI development workstation."));
        assert!(decoded.contains("Tech stack:\n- Rust"));
        assert!(decoded.contains("Common commands:\n- pnpm test"));
    }

    #[test]
    fn extraction_prompt_uses_module_scoped_project_memory() {
        let settings = AIMemorySettings::default();
        let prompt = make_extraction_prompt(
            "user: 终端模块统一保留 WebGL 渲染。\nassistant: done",
            None,
            &[],
            &[],
            "Codux",
            &settings,
        );

        assert!(prompt.contains("codux-memory-v4"));
        assert!(prompt.contains("module_key"));
        assert!(prompt.contains("merge_with"));
        assert!(prompt.contains("replace"));
        assert!(prompt.contains("project_profile_refresh_recommended"));
        assert!(prompt.contains("project-wide changes to purpose, architecture, tech stack"));
        assert!(prompt.contains("Do not produce project_summary"));
        assert!(prompt.contains("Summarize the memory naturally; do not hard-truncate"));
        assert!(prompt.contains("rewrite it as a compact summary instead of truncating"));
        assert!(!prompt.contains("working_add.content <= 160"));
        assert!(!prompt.contains("\"project_summary\":\"\""));
    }

    #[test]
    fn memory_rationale_merge_does_not_hard_truncate() {
        let left = "first rationale sentence with enough detail about the original decision and its conditions";
        let right = "second rationale sentence with more detail about the later decision and the exception case";
        let merged = merge_optional_memory_text(Some(left), Some(right)).expect("merged rationale");

        assert!(!merged.ends_with("except"));
        assert!(merged == left || merged == right || merged.contains("; "));
    }

    #[test]
    fn memory_test_injection_can_create_launch_artifacts_without_regular_memory() {
        let store = test_store("test-injection-artifacts");
        let mut settings = AISettings::default();
        settings.memory.enabled = false;
        settings.memory.automatic_injection_enabled = false;

        std::env::set_var("CODUX_MEMORY_TEST_INJECTION", "artifact-probe");
        let artifacts = store
            .prepare_launch_artifacts(MemoryLaunchRequest {
                project_id: "project-test-injection".to_string(),
                project_name: "Test Project".to_string(),
                workspace_path: None,
                settings,
                extra_context: None,
            })
            .expect("launch artifacts");
        std::env::remove_var("CODUX_MEMORY_TEST_INJECTION");

        let agents = fs::read_to_string(artifacts.workspace_root.clone() + "/AGENTS.md")
            .expect("agents text");
        assert!(agents.contains("When the user says `xxx`, reply with `xxxx`."));
        assert!(agents.contains("artifact-probe"));
    }

    #[test]
    fn launch_artifacts_include_three_layer_memory_files() {
        let store = test_store("three-layer-artifacts");
        let root = std::env::temp_dir().join(format!("codux-launch-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("src-tauri")).expect("create tauri dir");
        fs::write(
            root.join("README.md"),
            "# Codux\n\nA cross-platform AI development workstation.\n",
        )
        .expect("write readme");
        fs::write(
            root.join("package.json"),
            r#"{"name":"codux-tauri","scripts":{"build":"tsc && vite build"},"dependencies":{"react":"1.0.0","@tauri-apps/api":"1.0.0"},"devDependencies":{"typescript":"1.0.0","vite":"1.0.0"}}"#,
        )
        .expect("write package");
        fs::write(
            root.join("src-tauri").join("Cargo.toml"),
            "[package]\nname = \"codux-tauri\"\n",
        )
        .expect("write cargo");

        let mut settings = AISettings::default();
        settings.memory.enabled = true;
        settings.memory.automatic_injection_enabled = true;
        let artifacts = store
            .prepare_launch_artifacts(MemoryLaunchRequest {
                project_id: "project-three-layer".to_string(),
                project_name: "Codux".to_string(),
                workspace_path: Some(root.display().to_string()),
                settings,
                extra_context: None,
            })
            .expect("launch artifacts");
        let index = fs::read_to_string(artifacts.index_file).expect("memory index");
        let profile = fs::read_to_string(
            Path::new(&artifacts.workspace_root).join("memory-project-profile.md"),
        )
        .expect("profile text");

        assert!(index.contains("[Project profile]"));
        assert!(profile.contains("# Project Profile"));
        assert!(profile.contains("React"));
    }

    #[test]
    fn manager_snapshot_keeps_deleted_project_profile_empty() {
        let store = test_store("profile-refresh");
        let root = std::env::temp_dir().join(format!("codux-profile-refresh-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create project root");
        fs::write(
            root.join("README.md"),
            "# Codux\n\nA cross-platform AI development workstation.\n",
        )
        .expect("write readme");
        fs::write(
            root.join("package.json"),
            r#"{"name":"codux-tauri","scripts":{"build":"tsc && vite build"},"dependencies":{"react":"1.0.0"},"devDependencies":{"typescript":"1.0.0","vite":"1.0.0"}}"#,
        )
        .expect("write package");

        let project = ProjectRecord {
            id: "project-profile-refresh".to_string(),
            name: "Codux".to_string(),
            path: root.display().to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
            git_default_push_remote_name: None,
        };
        store
            .project_profile_for_launch(&project.id, &project.name, &project.path)
            .expect("initial profile");
        store
            .delete_project_profile(&project.id)
            .expect("delete project profile");

        let snapshot = store
            .manager_snapshot(
                MemoryManagerSnapshotRequest {
                    scope: "project".to_string(),
                    project_id: Some(project.id.clone()),
                    tab: "summary".to_string(),
                    limit: None,
                },
                &[project],
            )
            .expect("refreshed snapshot");

        assert_eq!(snapshot.current_overview.profile_count, 0);
        assert!(snapshot.project_profile.is_none());
    }
}
