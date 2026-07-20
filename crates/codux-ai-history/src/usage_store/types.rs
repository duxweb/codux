#[derive(Debug, Clone)]
pub struct AIUsageStore {
    database_path: PathBuf,
}

type UsageBucketKey = (String, String, String, i64);

#[derive(Debug, Clone)]
pub struct AIExternalFileSummary {
    pub source: String,
    pub file_path: String,
    pub file_modified_at: f64,
    pub file_size: i64,
    pub project_path: String,
    pub usage_buckets: Vec<AIUsageBucket>,
    pub usage_events: Vec<AIUsageEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AIUsageEvent {
    pub project_id: String,
    pub session_key: String,
    pub occurred_at: i64,
    pub total_tokens: i64,
    pub request_count: i64,
    pub active_duration_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AIUsageInterval {
    pub project_path: String,
    pub included_at: i64,
    pub excluded_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AIUsageIntervalSession {
    pub project_path: String,
    pub source: String,
    pub session_key: String,
    pub first_seen_at: i64,
    pub request_count: i64,
    pub total_tokens: i64,
    pub active_duration_seconds: i64,
}

#[derive(Debug, Clone)]
pub struct AIUsageBucket {
    pub source: String,
    pub session_key: String,
    pub external_session_id: Option<String>,
    pub session_title: String,
    pub model: Option<String>,
    pub project_id: String,
    pub project_name: String,
    pub bucket_start: f64,
    pub bucket_end: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub usage_amounts: Vec<AIUsageAmount>,
    pub request_count: i64,
    pub active_duration_seconds: i64,
    pub first_seen_at: f64,
    pub last_seen_at: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AIUsageProjectTotal {
    pub project_id: String,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AIGlobalRangeTotals {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
    pub active_duration_seconds: i64,
    pub session_count: usize,
}

#[derive(Debug, Clone)]
pub struct AIExternalFileCheckpoint {
    pub source: String,
    pub file_path: String,
    pub project_path: String,
    pub file_modified_at: f64,
    pub file_size: i64,
    pub last_offset: i64,
    pub last_indexed_at: f64,
    pub payload_json: Option<String>,
}

#[derive(Debug, Clone)]
struct NormalizedSessionLinkRow {
    source: String,
    file_path: String,
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
    active_duration_seconds: i64,
    usage_amounts: Vec<AIUsageAmount>,
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
    metadata_at: f64,
    metadata_source_key: String,
    model_at: f64,
    model_source_key: String,
    model_from_link: bool,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cached_input_tokens: i64,
    request_count: i64,
    today_tokens: i64,
    today_cached_input_tokens: i64,
    usage_amounts: Vec<AIUsageAmount>,
    today_usage_amounts: Vec<AIUsageAmount>,
    active_duration_seconds: i64,
}

#[derive(Debug, Default, Clone)]
struct ParsedSessionAccumulator {
    session_key: String,
    external_session_id: Option<String>,
    title: Option<String>,
    title_at: f64,
    first_seen_at: f64,
    last_seen_at: f64,
    last_model: Option<String>,
    model_at: f64,
    active_duration_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JSONLIndexMode {
    Unchanged,
    Append,
    Rebuild,
}
