#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryProjectRequest {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AIHistorySourceFingerprint {
    pub files: Vec<AIHistorySourceFileFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AIHistorySourceFileFingerprint {
    pub source: String,
    pub path: String,
    pub modified_millis: u128,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHistorySnapshot {
    pub project_id: String,
    pub project_name: String,
    pub project_summary: AIProjectUsageSummary,
    pub sessions: Vec<AISessionSummary>,
    pub heatmap: Vec<AIHeatmapDay>,
    pub today_time_buckets: Vec<AITimeBucket>,
    pub tool_breakdown: Vec<AIUsageBreakdownItem>,
    pub model_breakdown: Vec<AIUsageBreakdownItem>,
    pub indexed_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIGlobalHistorySnapshot {
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub today_total_tokens: i64,
    pub today_cached_input_tokens: i64,
    pub sessions: Vec<AISessionSummary>,
    pub project_count: usize,
    pub indexed_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIProjectUsageSummary {
    pub project_id: String,
    pub project_name: String,
    pub current_session_tokens: i64,
    pub current_session_cached_input_tokens: i64,
    pub project_total_tokens: i64,
    pub project_cached_input_tokens: i64,
    pub today_total_tokens: i64,
    pub today_cached_input_tokens: i64,
    pub current_tool: Option<String>,
    pub current_model: Option<String>,
    pub current_session_updated_at: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AISessionSummary {
    pub session_id: String,
    pub external_session_id: Option<String>,
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub session_title: String,
    pub first_seen_at: f64,
    pub last_seen_at: f64,
    pub last_tool: Option<String>,
    pub last_model: Option<String>,
    pub request_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub active_duration_seconds: i64,
    pub today_tokens: i64,
    pub today_cached_input_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHeatmapDay {
    pub day: f64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AITimeBucket {
    pub start: f64,
    pub end: f64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIUsageBreakdownItem {
    pub key: String,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoryEntry {
    pub(crate) source: String,
    pub(crate) session_id: String,
    pub(crate) external_session_id: Option<String>,
    pub(crate) session_title: Option<String>,
    pub(crate) timestamp: f64,
    pub(crate) model: Option<String>,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) cached_input_tokens: i64,
    pub(crate) reasoning_output_tokens: i64,
}

impl HistoryEntry {
    pub(crate) fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.reasoning_output_tokens
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HistoryEvent {
    pub(crate) source: String,
    pub(crate) session_id: String,
    pub(crate) timestamp: f64,
    pub(crate) role: HistoryRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryRole {
    User,
    Assistant,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ParsedHistory {
    pub(crate) entries: Vec<HistoryEntry>,
    pub(crate) events: Vec<HistoryEvent>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct JSONLParseSnapshot {
    pub(crate) result: ParsedHistory,
    pub(crate) last_processed_offset: i64,
    pub(crate) payload_json: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AIExternalFileCheckpointPayload {
    pub(crate) session_key: Option<String>,
    pub(crate) external_session_id: Option<String>,
    pub(crate) session_title: Option<String>,
    pub(crate) last_model: Option<String>,
    #[serde(default)]
    pub(crate) model_total_tokens_by_name: HashMap<String, i64>,
}

#[derive(Debug, Default)]
struct SessionAccumulator {
    source: String,
    session_id: String,
    external_session_id: Option<String>,
    title: Option<String>,
    first_seen_at: f64,
    last_seen_at: f64,
    model: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    reasoning_output_tokens: i64,
    request_count: i64,
    today_tokens: i64,
    today_cached_input_tokens: i64,
    active_duration_seconds: i64,
}
