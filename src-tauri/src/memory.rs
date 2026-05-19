use crate::ai_runtime::AISessionSnapshot;
use crate::app_settings::{AIMemorySettings, AIProviderSettings, AISettings, AppSettingsStore};
use crate::llm;
use crate::paths::{app_support_dir, home_dir, runtime_temp_dir};
use crate::project_store::ProjectRecord;
use anyhow::{anyhow, Context, Result};
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const MAX_EXTRACTION_ATTEMPTS: i64 = 3;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntry {
    pub id: String,
    pub scope: MemoryScope,
    pub project_id: Option<String>,
    pub tool_id: Option<String>,
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

#[derive(Debug, Clone)]
struct MemoryCandidate {
    scope: MemoryScope,
    project_id: Option<String>,
    tool_id: Option<String>,
    tier: MemoryTier,
    kind: MemoryKind,
    content: String,
    rationale: Option<String>,
    source_tool: Option<String>,
    source_session_id: Option<String>,
    source_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryExtractionTask {
    pub id: String,
    pub project_id: String,
    pub tool: String,
    pub session_id: String,
    pub transcript_path: String,
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
    pub last_error: Option<String>,
    pub updated_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryLaunchArtifacts {
    pub workspace_root: String,
    pub workspace_link: String,
    pub prompt_file: String,
    pub index_file: String,
}

#[derive(Debug, Clone)]
pub struct MemoryLaunchRequest {
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub settings: AISettings,
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
    pub summary_count: i64,
    pub updated_at: Option<f64>,
}

impl MemoryScopeOverview {
    fn total_count(&self) -> i64 {
        self.active_entry_count
            + self.archived_entry_count
            + self.merged_entry_count
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryManagerSnapshot {
    pub target_rows: Vec<MemoryManagerTargetRow>,
    pub selected_target_title: String,
    pub current_overview: MemoryScopeOverview,
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
    processing_lock: Mutex<()>,
}

impl MemoryStore {
    pub fn load_or_create() -> Result<Self> {
        let root = app_support_dir();
        fs::create_dir_all(&root)?;
        let store = Self {
            db_path: root.join("memory.sqlite3"),
            last_enqueued_at_by_session: Mutex::new(HashMap::new()),
            processing_lock: Mutex::new(()),
        };
        store.configure()?;
        Ok(store)
    }

    pub fn prepare_launch_artifacts(
        &self,
        request: MemoryLaunchRequest,
    ) -> Option<MemoryLaunchArtifacts> {
        let global_prompt = normalized_non_empty(&request.settings.global_prompt);
        let should_inject_memory =
            request.settings.memory.enabled && request.settings.memory.automatic_injection_enabled;
        if global_prompt.is_none() && !should_inject_memory {
            return None;
        }

        let root = runtime_temp_dir()
            .join("runtime-root")
            .join("memory-workspaces")
            .join(safe_path_segment(&request.project_id));
        let workspace_link = root.join("workspace");
        let prompt_file = root.join("memory-prompt.txt");
        let index_file = root.join("MEMORY.md");

        let claude_context = self.collect_context(
            &request.project_id,
            &request.project_name,
            "claude",
            &request.settings,
        );
        let codex_context = self.collect_context(
            &request.project_id,
            &request.project_name,
            "codex",
            &request.settings,
        );
        let gemini_context = self.collect_context(
            &request.project_id,
            &request.project_name,
            "gemini",
            &request.settings,
        );
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
        replace_workspace_link(&workspace_link, Path::new(&request.project_path)).ok()?;
        fs::write(&prompt_file, prompt_text).ok()?;
        fs::write(&index_file, index_text).ok()?;
        fs::write(
            root.join("memory-user.md"),
            render_user_memory_text(&memory_context),
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
            workspace_link: workspace_link.display().to_string(),
            prompt_file: prompt_file.display().to_string(),
            index_file: index_file.display().to_string(),
        })
    }

    pub fn recover_interrupted_extractions(&self) -> Result<i64> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memory_extraction_queue WHERE status = 'running';",
            [],
            |row| row.get(0),
        )?;
        if count == 0 {
            return Ok(0);
        }
        conn.execute(
            r#"
            UPDATE memory_extraction_queue
            SET status = 'pending', error = ?1
            WHERE status = 'running'
              AND attempts < ?2;
            "#,
            params![
                "Recovered after app restart before completion.",
                MAX_EXTRACTION_ATTEMPTS
            ],
        )?;
        conn.execute(
            r#"
            UPDATE memory_extraction_queue
            SET status = 'failed', error = ?1
            WHERE status = 'running'
              AND attempts >= ?2;
            "#,
            params![
                "Recovered after app restart before completion. Retry limit reached.",
                MAX_EXTRACTION_ATTEMPTS
            ],
        )?;
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
        let (entries, summaries) = match request.tab.as_str() {
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

        Ok(MemoryManagerSnapshot {
            target_rows,
            selected_target_title,
            current_overview,
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

    pub fn process_sessions_now(
        self: Arc<Self>,
        settings: AISettings,
        projects: Vec<ProjectRecord>,
        sessions: Vec<AISessionSnapshot>,
    ) {
        if !settings.memory.enabled {
            return;
        }
        tauri::async_runtime::spawn(async move {
            for session in &sessions {
                if let Err(error) = self.enqueue_session_for_manual_extraction(&projects, session) {
                    eprintln!("memory manual enqueue failed: {error}");
                }
            }
            if let Err(error) = self.process_queue(settings, projects) {
                eprintln!("memory manual extraction failed: {error}");
            }
        });
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
        let terminal: Option<(String, Option<String>)> = conn
            .query_row(
                r#"
                SELECT status, error
                FROM memory_extraction_queue
                WHERE status IN ('done', 'failed')
                ORDER BY enqueued_at DESC
                LIMIT 1;
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let status = if running_count > 0 {
            MemoryExtractionStatus::Processing
        } else if pending_count > 0 {
            MemoryExtractionStatus::Queued
        } else if terminal.as_ref().map(|value| value.0.as_str()) == Some("failed") {
            MemoryExtractionStatus::Failed
        } else {
            MemoryExtractionStatus::Idle
        };
        Ok(MemoryExtractionStatusSnapshot {
            status,
            pending_count,
            running_count,
            last_error: terminal.and_then(|value| value.1),
            updated_at: now_seconds(),
        })
    }

    pub fn handle_completed_session(
        self: Arc<Self>,
        settings: Arc<AppSettingsStore>,
        projects: Vec<ProjectRecord>,
        session: AISessionSnapshot,
    ) {
        tauri::async_runtime::spawn(async move {
            let configured = settings.snapshot().ai;
            if !configured.memory.enabled || !configured.memory.automatic_extraction_enabled {
                return;
            }
            if let Err(error) = self
                .enqueue_session_if_ready(&configured.memory, &projects, &session)
                .and_then(|_| self.process_queue(configured, projects))
            {
                eprintln!("memory extraction failed: {error}");
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
            CREATE TABLE IF NOT EXISTS memory_summary_versions (
                id TEXT PRIMARY KEY,
                summary_id TEXT NOT NULL,
                version INTEGER NOT NULL,
                content TEXT NOT NULL,
                source_entry_ids TEXT,
                created_at REAL NOT NULL
            );
            "#,
            "CREATE INDEX IF NOT EXISTS idx_memory_entries_scope_project_tier ON memory_entries(scope, project_id, tier);",
            "CREATE INDEX IF NOT EXISTS idx_memory_entries_tool ON memory_entries(tool_id);",
            "CREATE INDEX IF NOT EXISTS idx_memory_entries_hash ON memory_entries(scope, project_id, tool_id, normalized_hash);",
            "CREATE INDEX IF NOT EXISTS idx_memory_queue_status_time ON memory_extraction_queue(status, enqueued_at);",
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_summaries_scope_project_tool ON memory_summaries(scope, COALESCE(project_id, ''), COALESCE(tool_id, ''));",
            "CREATE INDEX IF NOT EXISTS idx_memory_summary_versions_summary ON memory_summary_versions(summary_id, version);",
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
            conn.execute_batch(statement)?;
        }
        Ok(())
    }

    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path).with_context(|| {
            format!("failed to open memory database {}", self.db_path.display())
        })?;
        conn.busy_timeout(std::time::Duration::from_millis(3000))?;
        Ok(conn)
    }

    fn collect_context(
        &self,
        project_id: &str,
        project_name: &str,
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
        let project_summary = if should_inject {
            self.current_summary(MemoryScope::Project, Some(project_id), None)
                .ok()
                .flatten()
        } else {
            None
        };
        let user_working = if should_inject && settings.memory.allow_cross_project_user_recall {
            self.list_entries(
                MemoryScope::User,
                None,
                Some(tool),
                &[MemoryTier::Working],
                i64::from(settings.memory.max_injected_user_working_memories),
            )
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let project_working = if should_inject {
            self.list_entries(
                MemoryScope::Project,
                Some(project_id),
                Some(tool),
                &[MemoryTier::Working],
                i64::from(settings.memory.max_injected_project_working_memories),
            )
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let user_core_fallback = if should_inject
            && user_summary.is_none()
            && settings.memory.allow_cross_project_user_recall
        {
            self.list_entries(MemoryScope::User, None, Some(tool), &[MemoryTier::Core], 4)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let project_core_fallback = if should_inject && project_summary.is_none() {
            self.list_entries(
                MemoryScope::Project,
                Some(project_id),
                Some(tool),
                &[MemoryTier::Core],
                6,
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
            global_prompt: normalized_non_empty(&settings.global_prompt),
            user_summary: user_summary.and_then(|summary| {
                trimmed_memory_text(
                    Some(&summary.content),
                    settings.memory.max_injected_summary_tokens,
                )
            }),
            project_summary: project_summary.and_then(|summary| {
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
        let (active, archived, merged, entry_updated): (i64, i64, i64, Option<f64>) = conn
            .query_row(
                r#"
                SELECT
                    COALESCE(SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN status = 'archived' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN status = 'merged' THEN 1 ELSE 0 END), 0),
                    MAX(updated_at)
                FROM memory_entries
                WHERE scope = ?1
                  AND COALESCE(project_id, '') = COALESCE(?2, '');
                "#,
                params![scope.as_str(), project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        let (summary_count, summary_updated): (i64, Option<f64>) = conn.query_row(
            r#"
            SELECT COUNT(*), MAX(updated_at)
            FROM memory_summaries
            WHERE scope = ?1
              AND COALESCE(project_id, '') = COALESCE(?2, '');
            "#,
            params![scope.as_str(), project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(MemoryScopeOverview {
            active_entry_count: active,
            archived_entry_count: archived,
            merged_entry_count: merged,
            summary_count,
            updated_at: max_optional_f64(entry_updated, summary_updated),
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
                WHERE scope = 'project' AND project_id IS NOT NULL;
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
        projects: &[ProjectRecord],
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
        let Some(project) = projects
            .iter()
            .find(|project| project.id == session.project_id)
            .or_else(|| {
                session.project_path.as_ref().and_then(|path| {
                    projects
                        .iter()
                        .find(|project| paths_equivalent(Some(project.path.as_str()), path))
                })
            })
        else {
            return Ok(false);
        };
        let Some(source) = self.resolve_transcript_source(session, project) else {
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
                &project.id,
                &session.tool,
                &session_identifier(session),
                &source.location,
                &source.fingerprint,
            )? {
                recent.insert(session_key, now_seconds());
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn enqueue_session_for_manual_extraction(
        &self,
        projects: &[ProjectRecord],
        session: &AISessionSnapshot,
    ) -> Result<bool> {
        if session.state != "idle" || !session.has_completed_turn {
            return Ok(false);
        }
        let Some(project) = projects
            .iter()
            .find(|project| project.id == session.project_id)
            .or_else(|| {
                session.project_path.as_ref().and_then(|path| {
                    projects
                        .iter()
                        .find(|project| paths_equivalent(Some(project.path.as_str()), path))
                })
            })
        else {
            return Ok(false);
        };
        let Some(source) = self.resolve_transcript_source(session, project) else {
            return Ok(false);
        };
        self.enqueue_extraction_if_needed(
            &project.id,
            &session.tool,
            &session_identifier(session),
            &source.location,
            &source.fingerprint,
        )
    }

    fn enqueue_extraction_if_needed(
        &self,
        project_id: &str,
        tool: &str,
        session_id: &str,
        transcript_path: &str,
        source_fingerprint: &str,
    ) -> Result<bool> {
        let conn = self.connect()?;
        let existing: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memory_extraction_queue WHERE source_fingerprint = ?1;",
            params![source_fingerprint],
            |row| row.get(0),
        )?;
        if existing > 0 {
            return Ok(false);
        }
        conn.execute(
            r#"
            INSERT INTO memory_extraction_queue (
                id, project_id, tool, session_id, transcript_path, source_fingerprint, status, attempts, error, enqueued_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', 0, NULL, ?7);
            "#,
            params![
                Uuid::new_v4().to_string(),
                project_id,
                tool,
                session_id,
                transcript_path,
                source_fingerprint,
                now_seconds()
            ],
        )?;
        Ok(true)
    }

    fn process_queue(&self, settings: AISettings, projects: Vec<ProjectRecord>) -> Result<()> {
        let _guard = self
            .processing_lock
            .try_lock()
            .map_err(|_| anyhow!("memory extraction queue is already processing"))?;
        let projects_by_id = projects
            .into_iter()
            .map(|project| (project.id.clone(), project))
            .collect::<HashMap<_, _>>();
        while let Some(task) = self.next_pending_extraction_task()? {
            if let Err(error) = self.process_task(&settings, &projects_by_id, task.clone()) {
                let _ = self.mark_extraction_task_failed(&task.id, &error.to_string());
            }
        }
        Ok(())
    }

    fn process_task(
        &self,
        settings: &AISettings,
        projects_by_id: &HashMap<String, ProjectRecord>,
        task: MemoryExtractionTask,
    ) -> Result<()> {
        self.mark_extraction_task_running(&task.id)?;
        let Some(project) = projects_by_id.get(&task.project_id) else {
            self.mark_extraction_task_done(&task.id)?;
            return Ok(());
        };
        let provider = select_memory_provider(settings, Some(&task.tool))
            .ok_or_else(|| anyhow!("No available AI provider is configured."))?;
        let transcript = self.resolve_transcript_for_task(&task, project)?;
        let user_summary = self
            .current_summary(MemoryScope::User, None, None)
            .ok()
            .flatten();
        let project_summary = self
            .current_summary(MemoryScope::Project, Some(&project.id), None)
            .ok()
            .flatten();
        let user_memories = self
            .list_entries(
                MemoryScope::User,
                None,
                None,
                &[MemoryTier::Working],
                i64::from(settings.memory.max_injected_user_working_memories),
            )
            .unwrap_or_default();
        let project_memories = self
            .list_entries(
                MemoryScope::Project,
                Some(&project.id),
                None,
                &[MemoryTier::Working],
                i64::from(settings.memory.max_injected_project_working_memories),
            )
            .unwrap_or_default();
        let prompt = make_extraction_prompt(
            &transcript,
            user_summary.as_ref(),
            project_summary.as_ref(),
            &user_memories,
            &project_memories,
            &project.name,
            &settings.memory,
        );
        let response_text = tauri::async_runtime::block_on(llm::complete_with_provider(
            provider,
            &prompt,
            Some(extraction_system_prompt()),
        ))
        .map_err(|error| anyhow!(error))?;
        let response = decode_extraction_response(&response_text)?;
        self.apply_extraction_response(response, &task, &settings.memory)?;
        self.mark_extraction_task_done(&task.id)?;
        Ok(())
    }

    fn next_pending_extraction_task(&self) -> Result<Option<MemoryExtractionTask>> {
        let conn = self.connect()?;
        conn.query_row(
            r#"
            SELECT id, project_id, tool, session_id, transcript_path, source_fingerprint, status, attempts, error, enqueued_at
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
        Ok(())
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
            let scope = item.scope.unwrap_or(MemoryScope::Project);
            let project_id = (scope == MemoryScope::Project).then(|| task.project_id.clone());
            let _ = self.upsert(MemoryCandidate {
                scope,
                project_id,
                tool_id: None,
                tier: item.tier.unwrap_or(MemoryTier::Working),
                kind: item.kind,
                content,
                rationale: item
                    .rationale
                    .and_then(|value| normalized_non_empty(&value)),
                source_tool: Some(task.tool.clone()),
                source_session_id: Some(task.session_id.clone()),
                source_fingerprint: Some(task.source_fingerprint.clone()),
            })?;
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
        if let Some(content) =
            valid_summary_content(response.project_summary.as_deref().unwrap_or(""))
        {
            let summary = self.upsert_summary(
                MemoryScope::Project,
                Some(&task.project_id),
                None,
                &content,
                &merged_ids,
                settings.max_summary_versions,
            )?;
            self.mark_entries_merged(&merged_ids, &summary.id)?;
            self.merge_stale_working_entries(
                MemoryScope::Project,
                Some(&task.project_id),
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
                      AND normalized_hash = ?4
                    LIMIT 1;
                    "#,
                    entry_select_columns()
                ),
                params![
                    candidate.scope.as_str(),
                    candidate.project_id.as_deref(),
                    candidate.tool_id.as_deref(),
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
                    source_session_id = ?6, source_fingerprint = ?7, status = 'active',
                    merged_summary_id = NULL, merged_at = NULL, archived_at = NULL, updated_at = ?8
                WHERE id = ?9;
                "#,
                params![
                    tier.as_str(),
                    candidate.kind.as_str(),
                    candidate.content,
                    candidate.rationale,
                    candidate.source_tool,
                    candidate.source_session_id,
                    candidate.source_fingerprint,
                    now,
                    entry.id
                ],
            )?;
            entry.tier = tier;
            entry.kind = candidate.kind;
            entry.content = candidate.content;
            entry.status = MemoryEntryStatus::Active;
            entry.updated_at = now;
            return Ok(entry);
        }

        let entry = MemoryEntry {
            id: Uuid::new_v4().to_string(),
            scope: candidate.scope,
            project_id: candidate.project_id,
            tool_id: candidate.tool_id,
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
        };
        conn.execute(
            r#"
            INSERT INTO memory_entries (
                id, scope, project_id, tool_id, tier, kind, content, rationale, source_tool, source_session_id,
                source_fingerprint, normalized_hash, superseded_by, status, merged_summary_id, merged_at, archived_at,
                access_count, last_accessed_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21);
            "#,
            params![
                entry.id,
                entry.scope.as_str(),
                entry.project_id,
                entry.tool_id,
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
        project: &ProjectRecord,
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
                claude_project_log_paths(&project.path)
                    .into_iter()
                    .find(|path| claude_log_contains_session(path, &ai_session, &project.path))
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
                find_codex_rollout_path(&project.path, &ai_session).and_then(|path| {
                    transcript_source_if_readable(
                        &path.display().to_string(),
                        &tool,
                        &ai_session,
                        false,
                    )
                })
            }
            "gemini" => {
                let files = gemini_session_paths(&project.path);
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
        project: &ProjectRecord,
    ) -> Result<String> {
        let tool = task.tool.to_lowercase();
        if Path::new(&task.transcript_path).is_file() {
            if tool == "opencode" && task.transcript_path.ends_with(".db") {
                if let Some(text) = fetch_opencode_transcript(
                    &project.path,
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
                for path in claude_project_log_paths(&project.path) {
                    if let Some(text) = read_transcript_file(&path.display().to_string(), 80, 8000)
                    {
                        return Ok(text);
                    }
                }
            }
            "codex" => {
                if let Some(path) = find_codex_rollout_path(&project.path, &task.session_id) {
                    if let Some(text) = read_transcript_file(&path.display().to_string(), 80, 8000)
                    {
                        return Ok(text);
                    }
                }
            }
            "gemini" => {
                for path in gemini_session_paths(&project.path) {
                    if let Some(text) = read_transcript_file(&path.display().to_string(), 80, 8000)
                    {
                        return Ok(text);
                    }
                }
            }
            "opencode" => {
                if let Some(text) = fetch_opencode_transcript(
                    &project.path,
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
}

#[derive(Debug, Clone)]
struct MemoryContextPayload {
    project_name: String,
    global_prompt: Option<String>,
    user_summary: Option<String>,
    project_summary: Option<String>,
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
            && (self.user_summary.is_some()
                || self.project_summary.is_some()
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
                global_prompt: None,
                user_summary: None,
                project_summary: None,
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
    project_summary: Option<String>,
    working_add: Vec<MemoryExtractionItem>,
    working_archive: Vec<String>,
    merged_entry_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct MemoryExtractionItem {
    scope: Option<MemoryScope>,
    tier: Option<MemoryTier>,
    kind: MemoryKind,
    content: String,
    rationale: Option<String>,
}

fn entry_select_columns() -> &'static str {
    "id, scope, project_id, tool_id, tier, kind, content, rationale, source_tool, source_session_id, source_fingerprint, normalized_hash, superseded_by, status, merged_summary_id, merged_at, archived_at, access_count, last_accessed_at, created_at, updated_at"
}

fn memory_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntry> {
    Ok(MemoryEntry {
        id: row.get(0)?,
        scope: MemoryScope::from_str(row.get::<_, String>(1)?.as_str()),
        project_id: row.get(2)?,
        tool_id: row.get(3)?,
        tier: MemoryTier::from_str(row.get::<_, String>(4)?.as_str()),
        kind: MemoryKind::from_str(row.get::<_, String>(5)?.as_str()),
        content: row.get(6)?,
        rationale: row.get(7)?,
        source_tool: row.get(8)?,
        source_session_id: row.get(9)?,
        source_fingerprint: row.get(10)?,
        normalized_hash: row.get(11)?,
        superseded_by: row.get(12)?,
        status: MemoryEntryStatus::from_str(row.get::<_, String>(13)?.as_str()),
        merged_summary_id: row.get(14)?,
        merged_at: row.get(15)?,
        archived_at: row.get(16)?,
        access_count: row.get(17)?,
        last_accessed_at: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
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

fn memory_task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryExtractionTask> {
    Ok(MemoryExtractionTask {
        id: row.get(0)?,
        project_id: row.get(1)?,
        tool: row.get(2)?,
        session_id: row.get(3)?,
        transcript_path: row.get(4)?,
        source_fingerprint: row.get(5)?,
        status: row.get(6)?,
        attempts: row.get(7)?,
        error: row.get(8)?,
        enqueued_at: row.get(9)?,
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
        "Launch context for {}.\nStart with MEMORY.md, then open topic files only when relevant to the task.\nPrefer current repository state over stale memory.\n\n{}",
        document_tool_name(tool).replace("{}", project_name),
        prompt
    )
}

fn render_index_text(context: &MemoryContextPayload, root: &Path) -> String {
    let mut sections = Vec::new();
    if let Some(prompt) = &context.global_prompt {
        sections.push(render_summary_section("Global instructions", prompt));
    }
    if !context.has_memory() {
        return sections.join("\n\n");
    }
    sections.push(format!(
        "# MEMORY.md\n\nProject context: {}\nApply relevant memory as guidance, not as source of truth.\nPrefer current repository state and user instructions over stale memory.\n\n## Load order\n1. Use this index first.\n2. Open topic files only when they are relevant to the current task.\n3. Full transcripts are not injected; use memory search only when history is needed.\n\n## Topic files\n- `memory-user.md`: cross-project user preferences and habits.\n- `memory-project.md`: project-specific decisions, conventions, and facts.\n- `memory-recent.md`: fresh working notes from recent sessions.\n- `memory-search.md`: search-only memory guidance and current injection limits.\n\nMemory workspace: {}\nProject workspace symlink: `workspace/`",
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
    if let Some(summary) = &context.project_summary {
        sections.push(render_summary_section("Project summary", summary));
    } else if !context.project_core_fallback.is_empty() {
        sections.push(render_index_entry_list(
            "Project notes index",
            &context.project_core_fallback,
        ));
    }
    if !context.user_working.is_empty() || !context.project_working.is_empty() {
        sections.push(format!(
            "[Recent notes index]\n- User working notes: {}\n- Project working notes: {}",
            context.user_working.len(),
            context.project_working.len()
        ));
    }
    trim_index_lines(&sections.join("\n\n"), 200)
}

fn render_user_memory_text(context: &MemoryContextPayload) -> String {
    let mut sections = vec![
        "# User Memory\n\nUse this only when cross-project user preferences matter.".to_string(),
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

fn render_project_memory_text(context: &MemoryContextPayload) -> String {
    let mut sections = vec![
        "# Project Memory\n\nUse this only when project-specific decisions, conventions, or facts matter."
            .to_string(),
    ];
    if let Some(summary) = &context.project_summary {
        sections.push(render_summary_section("Project summary", summary));
    }
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
        "# Recent Working Memory\n\nThese notes are short-lived and should not override current repository evidence."
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
        "# Search-Only Memory\n\nFull historical transcripts are not loaded into launch context.\nUse current repository files first. Search memory only when prior decisions,\nprevious debugging chains, or older project context are directly relevant.\n\nCurrent injected limits:\n- User working notes: {}/{}\n- Project working notes: {}/{}",
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
            if let Some(rationale) = normalized_non_empty(entry.rationale.as_deref().unwrap_or(""))
            {
                format!(
                    "- {} [{}; {}]",
                    entry.content,
                    entry.kind.as_str(),
                    rationale
                )
            } else {
                format!("- {} [{}]", entry.content, entry.kind.as_str())
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
        .map(|entry| format!("- {}: {}", entry.kind.as_str(), entry.content))
        .collect::<Vec<_>>()
        .join("\n");
    format!("[{title}]\n{lines}")
}

fn render_summary_section(title: &str, content: &str) -> String {
    format!("[{title}]\n{content}")
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

fn document_tool_name(tool: &str) -> &'static str {
    match tool {
        "codex" => "Codex",
        "claude" | "claude-code" => "Claude Code",
        "gemini" => "Gemini",
        _ => "AI tool",
    }
}

fn replace_workspace_link(link: &Path, target: &Path) -> Result<()> {
    if link.exists() || link.symlink_metadata().is_ok() {
        let _ = fs::remove_file(link);
        let _ = fs::remove_dir_all(link);
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)?;
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)?;
    }
    Ok(())
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

fn supports_completion(kind: &str) -> bool {
    matches!(kind, "openAICompatible" | "anthropic")
}

fn extraction_system_prompt() -> &'static str {
    "You extract and compact durable software-engineering memory from AI coding sessions.\n\nReturn JSON only.\nDo not include markdown fences.\nDo not include <think> blocks, reasoning text, analysis, explanations, or prose.\nThe first non-whitespace character of the response must be \"{\".\nDo not call tools, request scans, browse files, or infer facts outside the provided transcript and existing memory.\nTreat this as a deterministic memory compaction job, not a chat response."
}

fn make_extraction_prompt(
    transcript: &str,
    user_summary: Option<&MemorySummary>,
    project_summary: Option<&MemorySummary>,
    user_memories: &[MemoryEntry],
    project_memories: &[MemoryEntry],
    project_name: &str,
    settings: &AIMemorySettings,
) -> String {
    format!(
        "Memory extraction schema version: dmux-memory-v2\n\nProject: {project_name}\n\nExisting user summary:\n{}\n\nExisting project summary:\n{}\n\nRecent user working entries:\n{}\n\nRecent project working entries:\n{}\n\nTranscript:\n<transcript>\n{}\n</transcript>\n\nReturn JSON with this exact shape and no extra keys:\n{{\n  \"user_summary\": \"merged durable user memory, or empty string to keep unchanged\",\n  \"project_summary\": \"merged durable project memory, or empty string to keep unchanged\",\n  \"working_add\": [{{\"scope\":\"user|project\",\"tier\":\"core|working\",\"kind\":\"preference|convention|decision|fact|bug_lesson\",\"content\":\"...\",\"rationale\":\"...\"}}],\n  \"working_archive\": [\"uuid\"],\n  \"merged_entry_ids\": [\"uuid\"]\n}}\n\nStable extraction keywords and categories:\n- preference: explicit user preferences, communication style, review style, workflow style, tool choices, permission/confirmation preferences.\n- convention: stable coding standards, repository conventions, naming/path rules, testing/build commands, localization or documentation rules.\n- decision: accepted architectural or product decisions that should guide future implementation.\n- fact: durable repository facts discovered from the session, such as source-of-truth paths, runtime data locations, feature boundaries, or known command surfaces.\n- bug_lesson: reproducible bug cause, fix pattern, regression guard, or diagnostic chain that should prevent repeated debugging.\n\nNegative signals:\n- Do not store greetings, progress updates, temporary todo items, one-off command output, timestamps, broad explanations, or generic programming knowledge.\n- Do not store full transcript text or raw logs.\n- Do not invent preferences from assistant wording; user-stated rules and confirmed repo facts have priority.\n\nCompaction rules:\n- Merge old summary + useful transcript facts into a concise total summary; do not append a changelog.\n- user_summary contains only durable cross-project developer habits and preferences.\n- project_summary contains only durable repository-specific memory for this project.\n- working_add is for extracted atomic memories that should remain browseable after extraction.\n- Set working_add.tier to \"core\" only for stable preferences, conventions, accepted decisions, source-of-truth paths, and reusable bug lessons. Use \"working\" for fresh short-lived facts.\n- merged_entry_ids should include only older active memory ids already represented by the returned summary.\n- Keep each summary under about {} tokens.\n- If a summary should stay unchanged, return an empty string for that summary.",
        render_existing_summary(user_summary),
        render_existing_summary(project_summary),
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
                    "- id={} [{}] {} (context: {})",
                    entry.id,
                    entry.kind.as_str(),
                    entry.content,
                    rationale
                )
            } else {
                format!(
                    "- id={} [{}] {}",
                    entry.id,
                    entry.kind.as_str(),
                    entry.content
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_extraction_response(raw: &str) -> Result<MemoryExtractionResponse> {
    for candidate in json_object_candidates(raw) {
        if let Ok(value) = serde_json::from_str::<Value>(&candidate) {
            if let Some(response) = parse_extraction_value(&value) {
                return Ok(response);
            }
        }
    }
    Err(anyhow!(
        "Memory extraction provider returned malformed memory JSON."
    ))
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
    let project_summary = string_from_keys(
        value,
        &[
            "project_summary",
            "projectSummary",
            "project-summary",
            "repo_summary",
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
    if user_summary.is_none()
        && project_summary.is_none()
        && working_add.is_empty()
        && working_archive.is_empty()
        && merged_entry_ids.is_empty()
    {
        return None;
    }
    Some(MemoryExtractionResponse {
        user_summary,
        project_summary,
        working_add,
        working_archive,
        merged_entry_ids,
    })
}

fn parse_extraction_item(value: &Value) -> Option<MemoryExtractionItem> {
    let content = string_from_keys(value, &["content", "memory", "text", "summary", "value"])?;
    Some(MemoryExtractionItem {
        scope: string_from_keys(value, &["scope", "target", "level"])
            .map(|value| MemoryScope::from_str(&value)),
        tier: string_from_keys(value, &["tier", "priority", "stability"])
            .map(|value| MemoryTier::from_str(&value)),
        kind: string_from_keys(value, &["kind", "type", "category", "memory_type"])
            .map(|value| MemoryKind::from_str(&value))
            .unwrap_or(MemoryKind::Fact),
        content,
        rationale: string_from_keys(value, &["rationale", "reason", "context", "source", "why"]),
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
    let trimmed = trim_memory_text(lines.join("\n").trim(), token_limit);
    normalized_non_empty(&trimmed)
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
    normalized_non_empty(&trim_memory_text(&text, 8000))
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
    let temp_dir = home_dir().join(".gemini").join("tmp");
    let mut dirs = Vec::new();
    let projects_path = home_dir().join(".gemini").join("projects.json");
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
