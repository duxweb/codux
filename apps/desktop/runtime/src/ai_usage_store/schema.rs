// Bumped 7 -> 8 to force a full re-index of historical logs after the token
// parser fixes (Claude cache_creation backfill + codex cumulative-delta
// de-inflation). On launch a version mismatch drops the index tables and
// re-parses every log from offset 0 with the corrected parser. Pet XP is
// preserved by construction: it sums total_tokens (cached excluded, so the
// Claude fix is invisible to it) and only accumulates positive deltas against a
// high-water mark (so the codex de-inflation cannot lower it).
const NORMALIZED_HISTORY_SCHEMA_VERSION: &str = "8";
const RECENT_HISTORY_SESSION_LIMIT: usize = 80;

const SCHEMA_STATEMENTS: &[&str] = &[
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_file_state (
        source TEXT NOT NULL,
        file_path TEXT NOT NULL,
        project_path TEXT NOT NULL,
        file_modified_at REAL NOT NULL,
        PRIMARY KEY (source, file_path, project_path)
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_file_session_link (
        source TEXT NOT NULL,
        file_path TEXT NOT NULL,
        project_path TEXT NOT NULL,
        session_key TEXT NOT NULL,
        external_session_id TEXT,
        project_id TEXT NOT NULL,
        project_name TEXT NOT NULL,
        session_title TEXT NOT NULL,
        first_seen_at REAL NOT NULL,
        last_seen_at REAL NOT NULL,
        last_model TEXT,
        active_duration_seconds INTEGER NOT NULL,
        PRIMARY KEY (source, file_path, project_path, session_key)
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_file_usage_bucket (
        source TEXT NOT NULL,
        file_path TEXT NOT NULL,
        project_path TEXT NOT NULL,
        session_key TEXT NOT NULL,
        model TEXT NOT NULL,
        bucket_start REAL NOT NULL,
        bucket_end REAL NOT NULL,
        input_tokens INTEGER NOT NULL,
        output_tokens INTEGER NOT NULL,
        total_tokens INTEGER NOT NULL,
        cached_input_tokens INTEGER NOT NULL,
        request_count INTEGER NOT NULL,
        PRIMARY KEY (source, file_path, project_path, session_key, model, bucket_start)
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_project_index_state (
        project_path TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        project_name TEXT NOT NULL,
        indexed_at REAL NOT NULL
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_file_checkpoint (
        source TEXT NOT NULL,
        file_path TEXT NOT NULL,
        project_path TEXT NOT NULL,
        file_modified_at REAL NOT NULL,
        file_size INTEGER NOT NULL,
        last_offset INTEGER NOT NULL,
        last_indexed_at REAL NOT NULL,
        payload_json TEXT,
        PRIMARY KEY (source, file_path, project_path)
    );
    "#,
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_state_project_path ON ai_history_file_state(project_path);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_checkpoint_project_path ON ai_history_file_checkpoint(project_path);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_session_link_project_path ON ai_history_file_session_link(project_path);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_usage_bucket_project_path ON ai_history_file_usage_bucket(project_path, bucket_start);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_usage_bucket_bucket_start ON ai_history_file_usage_bucket(bucket_start);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_project_index_state_indexed_at ON ai_history_project_index_state(indexed_at DESC);",
];
