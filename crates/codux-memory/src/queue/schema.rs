use super::super::MemoryService;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// DB paths whose schema has already been ensured this process. `MemoryService`
/// is created fresh on every call, so without this guard the full DDL batch
/// (CREATE TABLEs + the migration ALTER, which fails-harmlessly once the column
/// exists) ran on every call -- including the 300ms status poller. Schema is
/// process-stable, so ensuring it once per path is enough.
fn schema_ensured_paths() -> &'static Mutex<HashSet<PathBuf>> {
    static PATHS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    PATHS.get_or_init(|| Mutex::new(HashSet::new()))
}

impl MemoryService {
    pub(crate) fn ensure_queue_schema(&self) -> Result<(), String> {
        if let Ok(ensured) = schema_ensured_paths().lock()
            && ensured.contains(&self.database_path)
        {
            return Ok(());
        }
        let conn = self.open_or_create_connection()?;
        conn.execute_batch(
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
                normalized_hash TEXT NOT NULL DEFAULT '',
                superseded_by TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                merged_summary_id TEXT,
                merged_at REAL,
                archived_at REAL,
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at REAL,
                created_at REAL NOT NULL DEFAULT 0,
                updated_at REAL NOT NULL DEFAULT 0,
                module_key TEXT
            );
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
            CREATE TABLE IF NOT EXISTS memory_project_profiles (
                project_id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                source_fingerprint TEXT NOT NULL,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL
            );
            CREATE TABLE IF NOT EXISTS memory_summary_versions (
                id TEXT PRIMARY KEY,
                summary_id TEXT NOT NULL,
                version INTEGER NOT NULL,
                content TEXT NOT NULL,
                source_entry_ids TEXT,
                created_at REAL NOT NULL
            );
            CREATE TABLE IF NOT EXISTS memory_decision_logs (
                id TEXT PRIMARY KEY,
                decision TEXT NOT NULL,
                entry_id TEXT,
                target_entry_id TEXT,
                reason TEXT NOT NULL,
                created_at REAL NOT NULL
            );
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
            CREATE TABLE IF NOT EXISTS memory_extraction_source_state (
                source_key TEXT PRIMARY KEY,
                last_seen_lines INTEGER NOT NULL,
                last_seen_len INTEGER NOT NULL,
                last_seen_mtime REAL NOT NULL,
                prefix_hash TEXT NOT NULL DEFAULT '',
                updated_at REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memory_queue_status_time
                ON memory_extraction_queue(status, enqueued_at);
            CREATE INDEX IF NOT EXISTS idx_memory_entries_recall
                ON memory_entries(status, project_id, tier, updated_at);
            CREATE INDEX IF NOT EXISTS idx_memory_entries_scope
                ON memory_entries(scope, project_id, tier, status);
            CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                content,
                rationale,
                module_key,
                content='memory_entries',
                content_rowid='rowid',
                tokenize='trigram'
            );
            CREATE TRIGGER IF NOT EXISTS memory_entries_ai AFTER INSERT ON memory_entries BEGIN
                INSERT INTO memory_fts(rowid, content, rationale, module_key)
                VALUES (new.rowid, new.content, COALESCE(new.rationale, ''), COALESCE(new.module_key, ''));
            END;
            CREATE TRIGGER IF NOT EXISTS memory_entries_ad AFTER DELETE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content, rationale, module_key)
                VALUES ('delete', old.rowid, old.content, COALESCE(old.rationale, ''), COALESCE(old.module_key, ''));
            END;
            CREATE TRIGGER IF NOT EXISTS memory_entries_au AFTER UPDATE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content, rationale, module_key)
                VALUES ('delete', old.rowid, old.content, COALESCE(old.rationale, ''), COALESCE(old.module_key, ''));
                INSERT INTO memory_fts(rowid, content, rationale, module_key)
                VALUES (new.rowid, new.content, COALESCE(new.rationale, ''), COALESCE(new.module_key, ''));
            END;
            INSERT INTO memory_fts(memory_fts) VALUES('rebuild');
            ALTER TABLE memory_extraction_queue ADD COLUMN workspace_path TEXT;
            "#,
        )
        .or_else(|error| {
            if error.to_string().contains("duplicate column name") {
                Ok(())
            } else {
                Err(error)
            }
        })
        .map_err(|error| error.to_string())?;
        if let Ok(mut ensured) = schema_ensured_paths().lock() {
            ensured.insert(self.database_path.clone());
        }
        Ok(())
    }
}
