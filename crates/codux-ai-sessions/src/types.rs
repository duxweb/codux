use codux_ai_history::normalized::{AIHeatmapDay, AITimeBucket, AIUsageBreakdownItem};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHistorySummary {
    pub indexed: bool,
    pub indexed_at: Option<f64>,
    pub is_loading: bool,
    pub queued: bool,
    pub progress: Option<f64>,
    pub detail: String,
    pub project_total_tokens: i64,
    pub project_cached_input_tokens: i64,
    pub today_total_tokens: i64,
    pub today_cached_input_tokens: i64,
    pub session_count: usize,
    pub sessions: Vec<AISessionSummary>,
    pub heatmap: Vec<AIHeatmapDay>,
    pub today_time_buckets: Vec<AITimeBucket>,
    pub tool_breakdown: Vec<AIUsageBreakdownItem>,
    pub model_breakdown: Vec<AIUsageBreakdownItem>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIGlobalHistorySummary {
    pub indexed_project_count: usize,
    pub session_count: usize,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub today_total_tokens: i64,
    pub today_cached_input_tokens: i64,
    pub project_totals: Vec<AIProjectUsageSummary>,
    pub recent_sessions: Vec<AISessionSummary>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIProjectUsageSummary {
    pub project_path: String,
    pub project_name: String,
    pub session_count: usize,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub today_total_tokens: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AISessionSummary {
    pub id: String,
    pub session_key: String,
    pub external_session_id: Option<String>,
    pub title: String,
    pub source: String,
    pub last_model: Option<String>,
    pub last_seen_at: f64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AISessionDetail {
    pub id: String,
    pub title: String,
    pub source: String,
    pub session_key: String,
    pub external_session_id: Option<String>,
    pub first_seen_at: Option<f64>,
    pub last_seen_at: Option<f64>,
    pub active_duration_seconds: i64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
    pub files: Vec<AISessionFileSummary>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AISessionForkTarget {
    Codex,
    Claude,
    Gemini,
    Agy,
    OpenCode,
    Kiro,
    CodeWhale,
    Kimi,
    MiMo,
}

impl AISessionForkTarget {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude",
            Self::Gemini => "Gemini",
            Self::Agy => "Agy",
            Self::OpenCode => "OpenCode",
            Self::Kiro => "Kiro",
            Self::CodeWhale => "CodeWhale",
            Self::Kimi => "Kimi Code",
            Self::MiMo => "MiMo-Code",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AISessionForkRequest {
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub session_id: String,
    pub target_tool: AISessionForkTarget,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AISessionForkResult {
    pub title: String,
    pub prompt_path: String,
    pub prompt_chars: usize,
    pub omitted_items: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AISessionFileSummary {
    pub file_path: String,
    pub model: String,
    pub first_seen_at: Option<f64>,
    pub last_seen_at: Option<f64>,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryStatsView {
    pub project_total_tokens: i64,
    pub today_total_tokens: i64,
    pub current_sessions: Vec<AIHistoryCurrentSessionView>,
    pub today_buckets: Vec<AIHistoryUsageBucketView>,
    pub heatmap: Vec<AIHistoryHeatmapCellView>,
    pub tool_rows: Vec<AIHistoryRankRow>,
    pub model_rows: Vec<AIHistoryRankRow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryCurrentSessionView {
    pub tool: String,
    pub model: Option<String>,
    pub total_tokens: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryUsageBucketView {
    pub start: f64,
    pub end: f64,
    pub value: i64,
    pub request_count: i64,
    pub ratio: f32,
    pub opacity: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryHeatmapCellView {
    pub day: f64,
    pub value: i64,
    pub request_count: i64,
    pub is_known: bool,
    pub opacity: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryRankRow {
    pub label: String,
    pub value: i64,
    pub percent: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryDailyLevelView {
    pub tokens: i64,
    pub current_tier: AIHistoryDailyLevelTierView,
    pub tiers: Vec<AIHistoryDailyLevelTierView>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryDailyLevelTierView {
    pub id: String,
    pub title: String,
    pub min: i64,
    pub color: u32,
    pub icon: String,
}

#[derive(Clone, Debug)]
pub(super) struct SessionLink {
    pub(super) source: String,
    pub(super) session_key: String,
    pub(super) external_session_id: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct SessionDetailLink {
    pub(super) source: String,
    pub(super) file_path: String,
    pub(super) session_key: String,
    pub(super) external_session_id: Option<String>,
    pub(super) title: String,
    pub(super) first_seen_at: Option<f64>,
    pub(super) last_seen_at: Option<f64>,
    pub(super) last_model: Option<String>,
    pub(super) active_duration_seconds: i64,
}
