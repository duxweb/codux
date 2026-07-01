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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIGlobalHistorySnapshot {
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub today_total_tokens: i64,
    pub today_cached_input_tokens: i64,
    pub sessions: Vec<AISessionSummary>,
    #[serde(default)]
    pub project_totals: Vec<AIProjectUsageTotal>,
    #[serde(default)]
    pub heatmap: Vec<AIHeatmapDay>,
    #[serde(default)]
    pub today_time_buckets: Vec<AITimeBucket>,
    #[serde(default)]
    pub recent_time_buckets: Vec<AITimeBucket>,
    #[serde(default)]
    pub tool_breakdown: Vec<AIUsageBreakdownItem>,
    #[serde(default)]
    pub model_breakdown: Vec<AIUsageBreakdownItem>,
    #[serde(default)]
    pub range_summaries: Vec<AIGlobalHistoryRangeSummary>,
    pub project_count: usize,
    pub indexed_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIProjectUsageTotal {
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub session_count: usize,
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    #[serde(default)]
    pub request_count: i64,
    #[serde(default)]
    pub active_duration_seconds: i64,
    pub today_total_tokens: i64,
    pub today_cached_input_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIGlobalHistoryRangeSummary {
    pub key: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
    #[serde(default)]
    pub session_count: usize,
    pub active_duration_seconds: i64,
    pub sessions: Vec<AISessionSummary>,
    pub project_totals: Vec<AIProjectUsageTotal>,
    pub tool_breakdown: Vec<AIUsageBreakdownItem>,
    pub model_breakdown: Vec<AIUsageBreakdownItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    pub usage_amounts: Vec<AIUsageAmount>,
    #[serde(default)]
    pub today_usage_amounts: Vec<AIUsageAmount>,
    pub current_tool: Option<String>,
    pub current_model: Option<String>,
    pub current_session_updated_at: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    pub usage_amounts: Vec<AIUsageAmount>,
    pub active_duration_seconds: i64,
    pub today_tokens: i64,
    pub today_cached_input_tokens: i64,
    #[serde(default)]
    pub today_usage_amounts: Vec<AIUsageAmount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHeatmapDay {
    pub day: f64,
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AITimeBucket {
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIUsageBreakdownItem {
    pub key: String,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
    #[serde(default)]
    pub usage_amounts: Vec<AIUsageAmount>,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub source: String,
    pub session_id: String,
    pub external_session_id: Option<String>,
    pub session_title: Option<String>,
    pub timestamp: f64,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub usage_amounts: Vec<AIUsageAmount>,
}

impl HistoryEntry {
    pub fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.reasoning_output_tokens
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIUsageAmount {
    pub unit: String,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct HistoryEvent {
    pub source: String,
    pub session_id: String,
    pub timestamp: f64,
    pub role: HistoryRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryRole {
    User,
    Assistant,
}

#[derive(Debug, Default, Clone)]
pub struct ParsedHistory {
    pub entries: Vec<HistoryEntry>,
    pub events: Vec<HistoryEvent>,
}

#[derive(Debug, Default, Clone)]
pub struct JSONLParseSnapshot {
    pub result: ParsedHistory,
    pub last_processed_offset: i64,
    pub payload_json: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIExternalFileCheckpointPayload {
    pub session_key: Option<String>,
    pub external_session_id: Option<String>,
    pub session_title: Option<String>,
    pub last_model: Option<String>,
    #[serde(default)]
    pub model_total_tokens_by_name: HashMap<String, i64>,
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
    usage_amounts: Vec<AIUsageAmount>,
    today_usage_amounts: Vec<AIUsageAmount>,
    active_duration_seconds: i64,
}
