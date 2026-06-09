#[derive(Debug, Clone)]
pub(crate) struct AIUsageStore {
    database_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct AIExternalFileSummary {
    pub(crate) source: String,
    pub(crate) file_path: String,
    pub(crate) file_modified_at: f64,
    pub(crate) file_size: i64,
    pub(crate) project_path: String,
    pub(crate) usage_buckets: Vec<AIUsageBucket>,
}

#[derive(Debug, Clone)]
pub(crate) struct AIUsageBucket {
    pub(crate) source: String,
    pub(crate) session_key: String,
    pub(crate) external_session_id: Option<String>,
    pub(crate) session_title: String,
    pub(crate) model: Option<String>,
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) bucket_start: f64,
    pub(crate) bucket_end: f64,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) total_tokens: i64,
    pub(crate) cached_input_tokens: i64,
    pub(crate) request_count: i64,
    pub(crate) active_duration_seconds: i64,
    pub(crate) first_seen_at: f64,
    pub(crate) last_seen_at: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AIUsageProjectTotal {
    pub project_id: String,
    pub total_tokens: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct AIExternalFileCheckpoint {
    pub(crate) source: String,
    pub(crate) file_path: String,
    pub(crate) project_path: String,
    pub(crate) file_modified_at: f64,
    pub(crate) file_size: i64,
    pub(crate) last_offset: i64,
    pub(crate) last_indexed_at: f64,
    pub(crate) payload_json: Option<String>,
}

#[derive(Debug, Clone)]
struct NormalizedSessionLinkRow {
    source: String,
    session_key: String,
    external_session_id: Option<String>,
    project_id: String,
    project_name: String,
    session_title: String,
    first_seen_at: f64,
    last_seen_at: f64,
    last_model: Option<String>,
    active_duration_seconds: i64,
}

#[derive(Debug, Clone)]
struct StoredUsageBucketRow {
    source: String,
    session_key: String,
    model: Option<String>,
    bucket_start: f64,
    bucket_end: f64,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cached_input_tokens: i64,
    request_count: i64,
}

#[derive(Debug, Default, Clone)]
struct PersistedSessionAccumulator {
    source: String,
    session_key: String,
    external_session_id: Option<String>,
    title: Option<String>,
    first_seen_at: f64,
    last_seen_at: f64,
    last_model: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cached_input_tokens: i64,
    request_count: i64,
    today_tokens: i64,
    today_cached_input_tokens: i64,
    active_duration_seconds: i64,
}

#[derive(Debug, Default, Clone)]
struct ParsedSessionAccumulator {
    session_key: String,
    external_session_id: Option<String>,
    title: Option<String>,
    first_seen_at: f64,
    last_seen_at: f64,
    last_model: Option<String>,
    active_duration_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JSONLIndexMode {
    Unchanged,
    Append,
    Rebuild,
}
