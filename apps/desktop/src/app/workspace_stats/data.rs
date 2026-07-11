use super::*;

pub(super) fn stats_range_summary(
    global: &codux_runtime::ai_history::AIGlobalHistorySummary,
    range: StatsTimeRange,
) -> Option<&codux_runtime::ai_history::AIGlobalHistoryRangeSummary> {
    let key = stats_time_range_key(range);
    global
        .range_summaries
        .iter()
        .find(|summary| summary.key == key)
}

pub(super) fn stats_time_range_key(range: StatsTimeRange) -> &'static str {
    match range {
        StatsTimeRange::Today => "today",
        StatsTimeRange::SevenDays => "sevenDays",
        StatsTimeRange::ThirtyDays => "thirtyDays",
        StatsTimeRange::All => "all",
    }
}

pub(super) fn stats_tool_rows(
    global: &codux_runtime::ai_history::AIGlobalHistorySummary,
    range: Option<&codux_runtime::ai_history::AIGlobalHistoryRangeSummary>,
    include_cached: bool,
) -> Vec<StatsRankRow> {
    let rows = range
        .map(|range| range.tool_breakdown.as_slice())
        .unwrap_or(global.tool_breakdown.as_slice());
    stats_breakdown_rows(rows, include_cached, 10)
}

pub(super) fn stats_model_rows(
    global: &codux_runtime::ai_history::AIGlobalHistorySummary,
    range: Option<&codux_runtime::ai_history::AIGlobalHistoryRangeSummary>,
    include_cached: bool,
) -> Vec<StatsRankRow> {
    let rows = range
        .map(|range| range.model_breakdown.as_slice())
        .unwrap_or(global.model_breakdown.as_slice());
    stats_breakdown_rows(rows, include_cached, 10)
}

pub(super) fn stats_breakdown_rows(
    rows: &[codux_runtime::ai_history_normalized::AIUsageBreakdownItem],
    include_cached: bool,
    limit: usize,
) -> Vec<StatsRankRow> {
    let mut values = rows
        .iter()
        .filter_map(|row| {
            let label = row.key.trim();
            if label.is_empty() || label.eq_ignore_ascii_case("unknown") {
                return None;
            }
            let value = codux_runtime::ai_history::display_tokens(
                row.total_tokens,
                row.cached_input_tokens,
                include_cached,
            );
            (value > 0 || row.request_count > 0)
                .then(|| (label.to_string(), value.max(0), row.request_count.max(0)))
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    rank_rows_from_values(values, limit)
}

pub(super) fn rank_rows_from_values(
    rows: Vec<(String, i64, i64)>,
    limit: usize,
) -> Vec<StatsRankRow> {
    let total_value = rows.iter().map(|(_, value, _)| *value).sum::<i64>().max(1);
    rows.into_iter()
        .take(limit)
        .map(|(label, value, request_count)| StatsRankRow {
            label,
            value,
            request_count,
            percent: value as f32 / total_value as f32,
        })
        .collect()
}

pub(super) fn stats_project_table_rows(
    global: &codux_runtime::ai_history::AIGlobalHistorySummary,
    range: Option<&codux_runtime::ai_history::AIGlobalHistoryRangeSummary>,
) -> Vec<StatsProjectRow> {
    let projects = range
        .map(|range| range.project_totals.as_slice())
        .unwrap_or(global.project_totals.as_slice());
    let mut rows = projects
        .iter()
        .map(|project| {
            let no_cache_tokens = project.total_tokens.max(0);
            let cached_input_tokens = project.cached_input_tokens.max(0);
            let total_tokens = stats_total_tokens(no_cache_tokens, cached_input_tokens);
            let project_label = if project.project_name.trim().is_empty() {
                project.project_path.clone()
            } else {
                project.project_name.clone()
            };
            StatsProjectRow {
                project: project_label,
                project_path: project.project_path.clone(),
                total_tokens,
                no_cache_tokens,
                input_tokens: project.input_tokens.max(0),
                output_tokens: project.output_tokens.max(0),
                cached_input_tokens,
                request_count: project.request_count.max(0),
                active_duration_seconds: project.active_duration_seconds.max(0),
            }
        })
        .filter(|row| {
            row.total_tokens > 0 || row.request_count > 0 || row.active_duration_seconds > 0
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .total_tokens
            .cmp(&left.total_tokens)
            .then_with(|| left.project.cmp(&right.project))
    });
    rows
}

pub(super) fn stats_total_tokens(no_cache_tokens: i64, cached_input_tokens: i64) -> i64 {
    no_cache_tokens.max(0) + cached_input_tokens.max(0)
}

pub(super) fn stats_global_heatmap(
    global: &codux_runtime::ai_history::AIGlobalHistorySummary,
    include_cached: bool,
) -> Vec<codux_runtime::ai_history::AIHistoryHeatmapCellView> {
    let today = codux_runtime::ai_history_normalized::local_day_start_seconds(app_now_seconds());
    let first_day = today - (STATS_HEATMAP_MAX_COLUMNS * STATS_HEATMAP_ROWS - 1) as f64 * 86_400.0;
    let mut values = (0..STATS_HEATMAP_MAX_COLUMNS)
        .flat_map(|column| {
            (0..STATS_HEATMAP_ROWS).map(move |row| {
                let day = first_day + (column * STATS_HEATMAP_ROWS + row) as f64 * 86_400.0;
                codux_runtime::ai_history::AIHistoryHeatmapCellView {
                    day,
                    value: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    cached_input_tokens: 0,
                    request_count: 0,
                    is_known: false,
                    opacity: 1.0,
                }
            })
        })
        .collect::<Vec<_>>();

    for day in &global.heatmap {
        let day_start = codux_runtime::ai_history_normalized::local_day_start_seconds(day.day);
        let day_offset = ((today - day_start) / 86_400.0).round() as isize;
        if (0..values.len() as isize).contains(&day_offset) {
            let index = values.len() - 1 - day_offset as usize;
            values[index].value += codux_runtime::ai_history::display_tokens(
                day.total_tokens,
                day.cached_input_tokens,
                include_cached,
            );
            values[index].input_tokens += day.input_tokens.max(0);
            values[index].output_tokens += day.output_tokens.max(0);
            values[index].total_tokens += day.total_tokens.max(0);
            values[index].cached_input_tokens += day.cached_input_tokens.max(0);
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
        cell.opacity = if !cell.is_known || cell.value <= 0 {
            0.14
        } else if non_zero.len() <= 1 {
            1.0
        } else {
            let upper = non_zero
                .iter()
                .position(|candidate| *candidate > cell.value)
                .unwrap_or(non_zero.len());
            let ratio =
                upper.saturating_sub(1) as f32 / (non_zero.len().saturating_sub(1)).max(1) as f32;
            0.18 + ratio.clamp(0.0, 1.0) * 0.82
        };
    }
    values
}

pub(super) fn stats_trend_buckets(
    global: &codux_runtime::ai_history::AIGlobalHistorySummary,
    include_cached: bool,
) -> Vec<StatsTrendBucket> {
    let mut rows = global
        .recent_time_buckets
        .iter()
        .map(|bucket| StatsTrendBucket {
            start_bits: bucket.start.to_bits(),
            input_tokens: bucket.input_tokens.max(0),
            output_tokens: bucket.output_tokens.max(0),
            cached_input_tokens: bucket.cached_input_tokens.max(0),
            no_cache_tokens: bucket.total_tokens.max(0),
            total_tokens: codux_runtime::ai_history::display_tokens(
                bucket.total_tokens.max(0),
                bucket.cached_input_tokens.max(0),
                include_cached,
            ),
            request_count: bucket.request_count.max(0),
        })
        .collect::<Vec<_>>();
    if rows.len() > STATS_TREND_MAX_BUCKET_COUNT {
        rows = rows.split_off(rows.len() - STATS_TREND_MAX_BUCKET_COUNT);
    }
    rows
}
