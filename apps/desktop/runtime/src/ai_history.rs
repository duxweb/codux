mod helpers;
mod queries;
mod session_fork;
mod sessions;
mod summary;
#[cfg(test)]
mod tests;
mod types;

use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::PathBuf;
pub use types::*;

pub fn display_tokens(total_tokens: i64, cached_input_tokens: i64, include_cached: bool) -> i64 {
    total_tokens.max(0)
        + if include_cached {
            cached_input_tokens.max(0)
        } else {
            0
        }
}

pub fn stats_view(
    history: &AIHistorySummary,
    runtime_state: &crate::ai_runtime_state::AIRuntimeStateSummary,
    selected_scope_id: Option<&str>,
    statistics_mode: &str,
    now: f64,
) -> AIHistoryStatsView {
    let include_cached = statistics_mode.trim() == "includingCache";
    AIHistoryStatsView {
        project_total_tokens: display_tokens(
            history.project_total_tokens,
            history.project_cached_input_tokens,
            include_cached,
        ),
        today_total_tokens: display_tokens(
            history.today_total_tokens,
            history.today_cached_input_tokens,
            include_cached,
        ),
        current_sessions: stats_current_sessions(runtime_state, selected_scope_id, include_cached),
        today_buckets: stats_today_buckets(history, include_cached, now),
        heatmap: stats_heatmap(history, include_cached, now),
        tool_rows: rank_rows(history.tool_breakdown.iter().map(|item| {
            (
                item.key.as_str(),
                display_tokens(item.total_tokens, item.cached_input_tokens, include_cached),
            )
        })),
        model_rows: rank_rows(history.model_breakdown.iter().map(|item| {
            (
                item.key.as_str(),
                display_tokens(item.total_tokens, item.cached_input_tokens, include_cached),
            )
        })),
    }
}

fn stats_current_sessions(
    runtime_state: &crate::ai_runtime_state::AIRuntimeStateSummary,
    selected_scope_id: Option<&str>,
    include_cached: bool,
) -> Vec<AIHistoryCurrentSessionView> {
    runtime_state
        .sessions
        .iter()
        .filter(|session| {
            selected_scope_id
                .map(|scope_id| session.project_id == scope_id)
                .unwrap_or(true)
        })
        .map(|session| AIHistoryCurrentSessionView {
            tool: session.tool.clone(),
            model: session.model.clone(),
            total_tokens: display_tokens(
                session.total_tokens,
                session.cached_input_tokens,
                include_cached,
            ),
        })
        .collect()
}

fn stats_today_buckets(
    history: &AIHistorySummary,
    include_cached: bool,
    now: f64,
) -> Vec<AIHistoryUsageBucketView> {
    let day_start = crate::ai_history_normalized::local_day_start_seconds(now);
    let mut buckets = (0..48)
        .map(|index| {
            let start = day_start + f64::from(index) * 1800.0;
            let end = if index == 47 {
                day_start + 86_399.0
            } else {
                start + 1800.0
            };
            AIHistoryUsageBucketView {
                start,
                end,
                value: 0,
                request_count: 0,
                ratio: 0.0,
                opacity: 0.35,
            }
        })
        .collect::<Vec<_>>();

    for bucket in &history.today_time_buckets {
        if crate::ai_history_normalized::local_day_start_seconds(bucket.start) != day_start {
            continue;
        }
        let index = (((bucket.start - day_start) / 86_400.0) * buckets.len() as f64)
            .floor()
            .clamp(0.0, (buckets.len() - 1) as f64) as usize;
        buckets[index].value += display_tokens(
            bucket.total_tokens,
            bucket.cached_input_tokens,
            include_cached,
        );
        buckets[index].request_count += bucket.request_count.max(0);
    }

    let max_value = buckets
        .iter()
        .map(|bucket| bucket.value)
        .max()
        .unwrap_or(0)
        .max(1);
    for bucket in &mut buckets {
        bucket.ratio = (bucket.value as f32 / max_value as f32).clamp(0.0, 1.0);
        bucket.opacity = if bucket.value > 0 { 1.0 } else { 0.35 };
    }

    buckets
}

fn stats_heatmap(
    history: &AIHistorySummary,
    include_cached: bool,
    now: f64,
) -> Vec<AIHistoryHeatmapCellView> {
    const COLUMNS: usize = 20;
    let today = crate::ai_history_normalized::local_day_start_seconds(now);
    let first_day = today - (COLUMNS * 7 - 1) as f64 * 86_400.0;
    let mut values = (0..COLUMNS)
        .flat_map(|column| {
            (0..7).map(move |row| {
                let day = first_day + (column * 7 + row) as f64 * 86_400.0;
                AIHistoryHeatmapCellView {
                    day,
                    value: 0,
                    request_count: 0,
                    is_known: false,
                    opacity: 1.0,
                }
            })
        })
        .collect::<Vec<_>>();

    for day in &history.heatmap {
        let day_start = crate::ai_history_normalized::local_day_start_seconds(day.day);
        let day_offset = ((today - day_start) / 86_400.0).round() as isize;
        if (0..values.len() as isize).contains(&day_offset) {
            let index = values.len() - 1 - day_offset as usize;
            values[index].value +=
                display_tokens(day.total_tokens, day.cached_input_tokens, include_cached);
            values[index].request_count += day.request_count.max(0);
            values[index].is_known = true;
        }
    }

    let mut non_zero = values
        .iter()
        .filter_map(|cell| (cell.value > 0).then_some(cell.value))
        .collect::<Vec<_>>();
    non_zero.sort_unstable();
    for cell in &mut values {
        cell.opacity = if cell.is_known {
            heatmap_opacity(cell.value, &non_zero)
        } else {
            1.0
        };
    }

    values
}

fn heatmap_opacity(value: i64, non_zero: &[i64]) -> f32 {
    if value <= 0 {
        return 0.14;
    }
    if non_zero.len() <= 1 {
        return 1.0;
    }

    let upper = non_zero
        .iter()
        .position(|candidate| *candidate > value)
        .unwrap_or(non_zero.len());
    let rank = upper.saturating_sub(1);
    let ratio = rank as f32 / (non_zero.len().saturating_sub(1)).max(1) as f32;

    if ratio < 0.1 {
        0.14
    } else if ratio < 0.2 {
        0.22
    } else if ratio < 0.32 {
        0.30
    } else if ratio < 0.44 {
        0.40
    } else if ratio < 0.56 {
        0.52
    } else if ratio < 0.68 {
        0.64
    } else if ratio < 0.8 {
        0.76
    } else if ratio < 0.92 {
        0.88
    } else {
        1.0
    }
}

fn rank_rows<'a>(rows: impl Iterator<Item = (&'a str, i64)>) -> Vec<AIHistoryRankRow> {
    let mut totals = BTreeMap::<String, i64>::new();
    for (label, value) in rows {
        let label = label.trim();
        if label.is_empty() || label.eq_ignore_ascii_case("unknown") || value <= 0 {
            continue;
        }
        *totals.entry(label.to_string()).or_default() += value;
    }

    let max_value = totals.values().copied().max().unwrap_or(1).max(1);
    let mut rows = totals.into_iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| right.1.cmp(&left.1));
    rows.into_iter()
        .take(4)
        .map(|(label, value)| AIHistoryRankRow {
            label,
            value,
            percent: value as f32 / max_value as f32,
        })
        .collect()
}

pub fn daily_level_view(tokens: i64) -> AIHistoryDailyLevelView {
    let tokens = tokens.max(0);
    let tiers = DAILY_LEVEL_TIERS
        .iter()
        .map(|tier| tier.view())
        .collect::<Vec<_>>();
    let current_tier = tiers
        .iter()
        .rev()
        .find(|tier| tokens >= tier.min)
        .cloned()
        .unwrap_or_else(|| tiers.first().cloned().unwrap_or_default());
    AIHistoryDailyLevelView {
        tokens,
        current_tier,
        tiers,
    }
}

struct DailyLevelTier {
    id: &'static str,
    title: &'static str,
    min: i64,
    color: u32,
    icon: &'static str,
}

impl DailyLevelTier {
    fn view(&self) -> AIHistoryDailyLevelTierView {
        AIHistoryDailyLevelTierView {
            id: self.id.to_string(),
            title: self.title.to_string(),
            min: self.min,
            color: self.color,
            icon: self.icon.to_string(),
        }
    }
}

const DAILY_LEVEL_TIERS: [DailyLevelTier; 8] = [
    DailyLevelTier {
        id: "iron",
        title: "Iron",
        min: 0,
        color: 0x5B616D,
        icon: "minus",
    },
    DailyLevelTier {
        id: "bronze",
        title: "Bronze",
        min: 1_000_000,
        color: 0xC98663,
        icon: "zap",
    },
    DailyLevelTier {
        id: "silver",
        title: "Silver",
        min: 3_000_000,
        color: 0xC8D1E3,
        icon: "shield-check",
    },
    DailyLevelTier {
        id: "gold",
        title: "Gold",
        min: 6_000_000,
        color: 0xE8AA34,
        icon: "star",
    },
    DailyLevelTier {
        id: "platinum",
        title: "Platinum",
        min: 10_000_000,
        color: 0x7ED6D8,
        icon: "star",
    },
    DailyLevelTier {
        id: "diamond",
        title: "Diamond",
        min: 18_000_000,
        color: 0x59A7FF,
        icon: "sparkles",
    },
    DailyLevelTier {
        id: "master",
        title: "Master",
        min: 30_000_000,
        color: 0x9A72FF,
        icon: "trophy",
    },
    DailyLevelTier {
        id: "grandmaster",
        title: "Grandmaster",
        min: 50_000_000,
        color: 0xFF5E8E,
        icon: "flame",
    },
];

pub struct AIHistoryService {
    database_path: PathBuf,
}

impl AIHistoryService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            database_path: support_dir.join("ai-usage.sqlite3"),
        }
    }

    fn open_connection(&self) -> Result<Connection, String> {
        if !self.database_path.is_file() {
            return Err("ai-usage.sqlite3 not found".to_string());
        }
        Connection::open(&self.database_path).map_err(|error| error.to_string())
    }
}
