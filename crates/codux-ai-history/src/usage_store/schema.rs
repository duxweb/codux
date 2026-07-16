// Bumped 13 -> 14 to persist exact usage, request, and activity facts for
// membership-scoped pet progress, with canonical project paths.
//
// Bumped 12 -> 13 to keep forked Codex rollout files under their own first
// session metadata instead of the copied parent metadata that follows it.
//
// Bumped 11 -> 12 to rebuild Codex histories with exact per-field cumulative
// deltas and turn boundaries that exclude idle time between resumed prompts.
//
// Bumped 10 -> 11 to rebuild histories after fixing Claude message snapshot
// deduplication, request classification, and Codex cumulative counter resets.
//
// Bumped 9 -> 10 to fold reasoning output into persisted output_tokens. The
// raw normalized entries keep reasoning separate, but indexed buckets expose
// output as the user-facing output bucket so total_tokens = input + output and
// cached_input_tokens stays an independent cache-read bucket.
//
// Bumped 8 -> 9 to add unit-aware usage amounts (Kiro credits) while keeping
// token totals unchanged. On launch a version mismatch drops the index tables
// and re-parses every log from offset 0 so Kiro sessions with 0 token but
// metering usage are visible.
//
// Bumped 7 -> 8 to force a full re-index of historical logs after the token
// parser fixes (Claude cache_creation backfill + codex cumulative-delta
// de-inflation). On launch a version mismatch drops the index tables and
// re-parses every log from offset 0 with the corrected parser. Pet XP is
// derived from the corrected event facts, so parser fixes also correct pet XP.
const NORMALIZED_HISTORY_SCHEMA_VERSION: &str = "14";
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
        active_duration_seconds INTEGER NOT NULL,
        PRIMARY KEY (source, file_path, project_path, session_key, model, bucket_start)
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_file_usage_amount (
        source TEXT NOT NULL,
        file_path TEXT NOT NULL,
        project_path TEXT NOT NULL,
        session_key TEXT NOT NULL,
        model TEXT NOT NULL,
        bucket_start REAL NOT NULL,
        unit TEXT NOT NULL,
        value REAL NOT NULL,
        PRIMARY KEY (source, file_path, project_path, session_key, model, bucket_start, unit)
    );
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ai_history_file_usage_event (
        source TEXT NOT NULL,
        file_path TEXT NOT NULL,
        project_path TEXT NOT NULL,
        project_id TEXT NOT NULL,
        event_ordinal INTEGER NOT NULL,
        session_key TEXT NOT NULL,
        occurred_at INTEGER NOT NULL,
        total_tokens INTEGER NOT NULL,
        request_count INTEGER NOT NULL,
        active_duration_seconds INTEGER NOT NULL,
        PRIMARY KEY (source, file_path, project_path, event_ordinal)
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
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_usage_amount_project_path ON ai_history_file_usage_amount(project_path, bucket_start);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_file_usage_event_project_time ON ai_history_file_usage_event(project_path, occurred_at);",
    "CREATE INDEX IF NOT EXISTS idx_ai_history_project_index_state_indexed_at ON ai_history_project_index_state(indexed_at DESC);",
];
