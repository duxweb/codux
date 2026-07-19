use codux_ai_history::normalized::{AIHeatmapDay, AITimeBucket, AIUsageBreakdownItem};
use codux_protocol::RemoteAIUsageAmount;
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
    pub heatmap: Vec<AIHeatmapDay>,
    pub today_time_buckets: Vec<AITimeBucket>,
    #[serde(default)]
    pub recent_time_buckets: Vec<AITimeBucket>,
    pub tool_breakdown: Vec<AIUsageBreakdownItem>,
    pub model_breakdown: Vec<AIUsageBreakdownItem>,
    pub range_summaries: Vec<AIGlobalHistoryRangeSummary>,
    pub recent_sessions: Vec<AISessionSummary>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIProjectUsageSummary {
    #[serde(default)]
    pub project_id: String,
    pub project_path: String,
    pub project_name: String,
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
    #[serde(default)]
    pub today_cached_input_tokens: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    pub project_totals: Vec<AIProjectUsageSummary>,
    pub tool_breakdown: Vec<AIUsageBreakdownItem>,
    pub model_breakdown: Vec<AIUsageBreakdownItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AISessionSummary {
    pub id: String,
    pub session_key: String,
    pub external_session_id: Option<String>,
    pub title: String,
    pub source: String,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
    pub last_model: Option<String>,
    pub last_seen_at: f64,
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
    #[serde(default)]
    pub active_duration_seconds: i64,
    #[serde(default)]
    pub usage_amounts: Vec<AIUsageAmount>,
}

impl From<AISessionSummary> for codux_protocol::RemoteAISessionSummary {
    fn from(summary: AISessionSummary) -> Self {
        Self {
            id: summary.id,
            title: summary.title,
            tool: summary.source,
            model: summary.last_model,
            time: summary.last_seen_at,
            size: summary.total_tokens,
            usage_amounts: summary
                .usage_amounts
                .into_iter()
                .map(|amount| RemoteAIUsageAmount {
                    unit: amount.unit,
                    value: amount.value,
                })
                .collect(),
        }
    }
}

impl AIHistoryCurrentSessionView {
    pub fn from_remote(
        session: codux_protocol::RemoteAICurrentSession,
        include_cached: bool,
    ) -> Self {
        Self {
            session_id: session.session_id,
            terminal_id: session.terminal_id,
            title: session.title,
            tool: session.tool,
            model: session.model,
            status: session.status,
            is_running: session.is_running,
            total_tokens: display_current_session_tokens(
                session.total_tokens,
                session.cached_input_tokens,
                include_cached,
            ),
            current_total_tokens: display_current_session_tokens(
                session.current_total_tokens,
                session.current_cached_input_tokens,
                include_cached,
            ),
            usage_amounts: session
                .usage_amounts
                .into_iter()
                .map(AIUsageAmount::from)
                .collect(),
            current_usage_amounts: session
                .current_usage_amounts
                .into_iter()
                .map(AIUsageAmount::from)
                .collect(),
            cached_input_tokens: if include_cached {
                session.cached_input_tokens.max(0)
            } else {
                0
            },
            current_cached_input_tokens: if include_cached {
                session.current_cached_input_tokens.max(0)
            } else {
                0
            },
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIUsageAmount {
    pub unit: String,
    pub value: f64,
}

impl From<RemoteAIUsageAmount> for AIUsageAmount {
    fn from(amount: RemoteAIUsageAmount) -> Self {
        Self {
            unit: amount.unit,
            value: amount.value,
        }
    }
}

pub fn ai_current_session_views(
    sessions: impl IntoIterator<Item = codux_protocol::RemoteAICurrentSession>,
    include_cached: bool,
) -> Vec<AIHistoryCurrentSessionView> {
    sessions
        .into_iter()
        .map(|session| AIHistoryCurrentSessionView::from_remote(session, include_cached))
        .collect()
}

fn display_current_session_tokens(
    total_tokens: i64,
    cached_input_tokens: i64,
    include_cached: bool,
) -> i64 {
    total_tokens.max(0)
        + if include_cached {
            cached_input_tokens.max(0)
        } else {
            0
        }
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
    Agy,
    Omp,
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
            Self::Agy => "Agy",
            Self::Omp => "Oh My Pi",
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
    #[serde(default, rename = "sessionId")]
    pub session_id: String,
    #[serde(default, rename = "terminalId")]
    pub terminal_id: Option<String>,
    #[serde(default)]
    pub title: String,
    pub tool: String,
    pub model: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default, rename = "isRunning")]
    pub is_running: bool,
    pub total_tokens: i64,
    #[serde(default)]
    pub current_total_tokens: i64,
    #[serde(default)]
    pub usage_amounts: Vec<AIUsageAmount>,
    #[serde(default)]
    pub current_usage_amounts: Vec<AIUsageAmount>,
    #[serde(default)]
    pub cached_input_tokens: i64,
    #[serde(default)]
    pub current_cached_input_tokens: i64,
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
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub total_tokens: i64,
    #[serde(default)]
    pub cached_input_tokens: i64,
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
