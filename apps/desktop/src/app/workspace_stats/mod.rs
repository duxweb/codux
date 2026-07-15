use super::*;
use crate::app::ui_helpers::codux_tooltip_container;
use chrono::{Datelike as _, TimeZone as _, Timelike as _};
use codux_runtime::i18n::translate;
use gpui::Rems;
use gpui_component::{
    Size,
    button::{Button, ButtonVariants},
    progress::Progress,
    scroll::ScrollableElement,
    tab::{Tab, TabBar},
    table::{Column, ColumnSort, DataTable, TableDelegate, TableState},
};

mod cards;
mod data;
mod fingerprint;
mod format;
mod table;
#[cfg(test)]
mod tests;

use cards::*;
use data::*;
use fingerprint::*;
use format::stats_text;
pub(in crate::app) use table::StatsProjectTableDelegate;
const STATS_TREND_DEFAULT_BUCKET_COUNT: usize = 72;
const STATS_TREND_MAX_BUCKET_COUNT: usize = 96;
const STATS_TREND_BUCKET_SECONDS: f64 = 30.0 * 60.0;
const STATS_TREND_MAX_BAR_WIDTH: f32 = 7.0;
const STATS_TREND_BUCKET_MIN_WIDTH: f32 = 12.0;
const STATS_HEATMAP_ROWS: usize = 7;
const STATS_HEATMAP_DEFAULT_COLUMNS: usize = 52;
const STATS_HEATMAP_MIN_COLUMNS: usize = 8;
const STATS_HEATMAP_MAX_COLUMNS: usize = 260;
const STATS_HEATMAP_CELL_SIZE: f32 = 13.0;
const STATS_HEATMAP_GAP: f32 = 3.0;
const STATS_CHART_CARD_HEIGHT: f32 = 198.0;
const STATS_CHART_BODY_HEIGHT: f32 = 90.0;
const STATS_FILTER_TEXT_SIZE: Rems = Rems(0.75);
const STATS_FILTER_LINE_HEIGHT: Rems = Rems(1.0);
const STATS_TABLE_BASE_WIDTH: f32 = 1200.0;

#[derive(Clone)]
pub(in crate::app) struct StatsWorkspaceSnapshot {
    language: String,
    include_cached: bool,
    time_range: StatsTimeRange,
    range_total_tokens: i64,
    range_no_cache_tokens: i64,
    range_input_tokens: i64,
    range_output_tokens: i64,
    range_cached_input_tokens: i64,
    range_request_count: i64,
    range_session_count: usize,
    range_active_duration_seconds: i64,
    trend_buckets: Vec<StatsTrendBucket>,
    heatmap: Vec<codux_runtime::ai_history::AIHistoryHeatmapCellView>,
    tool_rows: Vec<StatsRankRow>,
    model_rows: Vec<StatsRankRow>,
    project_rows: Vec<StatsProjectRow>,
    fingerprint: u64,
}

#[derive(Clone)]
struct StatsRankRow {
    label: String,
    value: i64,
    request_count: i64,
    percent: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::app) struct StatsProjectRow {
    project: String,
    project_path: String,
    total_tokens: i64,
    no_cache_tokens: i64,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    request_count: i64,
    active_duration_seconds: i64,
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
struct StatsTrendBucket {
    start_bits: u64,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    total_tokens: i64,
    no_cache_tokens: i64,
    request_count: i64,
}

struct StatsHeatmapMonthLabel {
    label: String,
    columns: usize,
}

impl PartialEq for StatsWorkspaceSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
    }
}

impl StatsWorkspaceSnapshot {
    pub(in crate::app) fn language(&self) -> &str {
        &self.language
    }

    pub(in crate::app) fn project_rows(&self) -> Vec<StatsProjectRow> {
        self.project_rows.clone()
    }
}

impl CoduxApp {
    pub(in crate::app) fn stats_workspace_snapshot(&self) -> StatsWorkspaceSnapshot {
        let global = &self.state.ai_global_history;
        let include_cached = self.state.settings.statistics_mode.trim() == "includingCache";
        let time_range = self.stats_time_range;
        let range = stats_range_summary(global, time_range);
        let range_input_tokens =
            range
                .map(|range| range.input_tokens.max(0))
                .unwrap_or_else(|| {
                    global
                        .project_totals
                        .iter()
                        .map(|project| project.input_tokens.max(0))
                        .sum()
                });
        let range_output_tokens = range
            .map(|range| range.output_tokens.max(0))
            .unwrap_or_else(|| {
                global
                    .project_totals
                    .iter()
                    .map(|project| project.output_tokens.max(0))
                    .sum()
            });
        let range_no_cache_tokens =
            range
                .map(|range| range.total_tokens.max(0))
                .unwrap_or_else(|| {
                    global
                        .project_totals
                        .iter()
                        .map(|project| project.total_tokens.max(0))
                        .sum()
                });
        let range_cached_input_tokens = range
            .map(|range| range.cached_input_tokens.max(0))
            .unwrap_or_else(|| {
                global
                    .project_totals
                    .iter()
                    .map(|project| project.cached_input_tokens.max(0))
                    .sum()
            });
        let range_total_tokens =
            stats_total_tokens(range_no_cache_tokens, range_cached_input_tokens);
        let range_request_count = range
            .map(|range| range.request_count.max(0))
            .unwrap_or_else(|| {
                global
                    .project_totals
                    .iter()
                    .map(|project| project.request_count.max(0))
                    .sum()
            });
        let range_session_count = range
            .map(|range| range.session_count)
            .unwrap_or(global.session_count);
        let range_active_duration_seconds = range
            .map(|range| range.active_duration_seconds.max(0))
            .unwrap_or_else(|| {
                global
                    .project_totals
                    .iter()
                    .map(|project| project.active_duration_seconds.max(0))
                    .sum()
            });
        let project_rows = stats_project_table_rows(global, range);
        let tool_rows = stats_tool_rows(global, range, include_cached);
        let model_rows = stats_model_rows(global, range, include_cached);
        let heatmap = stats_global_heatmap(global, include_cached);
        let trend_buckets = stats_trend_buckets(global, include_cached);
        let controls_fingerprint = super::workspace_views::workspace_view_hash(&(
            self.state.settings.language.clone(),
            self.state.settings.statistics_mode.clone(),
            self.stats_time_range,
        ));
        let range_fingerprint = super::workspace_views::workspace_view_hash(&(
            range_input_tokens,
            range_output_tokens,
            range_no_cache_tokens,
            range_cached_input_tokens,
            range_request_count,
            range_session_count,
            range_active_duration_seconds,
        ));
        let content_fingerprint = super::workspace_views::workspace_view_hash(&(
            global_fingerprint(global),
            rank_fingerprint(&tool_rows),
            rank_fingerprint(&model_rows),
            project_rows_fingerprint(&project_rows),
            trend_buckets_fingerprint(&trend_buckets),
            heatmap_fingerprint(&heatmap),
            range_fingerprint,
        ));
        let fingerprint = super::workspace_views::workspace_view_hash(&(
            controls_fingerprint,
            content_fingerprint,
        ));

        StatsWorkspaceSnapshot {
            language: self.state.settings.language.clone(),
            include_cached,
            time_range,
            range_total_tokens,
            range_no_cache_tokens,
            range_input_tokens,
            range_output_tokens,
            range_cached_input_tokens,
            range_request_count,
            range_session_count,
            range_active_duration_seconds,
            trend_buckets,
            heatmap,
            tool_rows,
            model_rows,
            project_rows,
            fingerprint,
        }
    }
}

pub(in crate::app) fn stats_workspace_body(
    app_entity: gpui::Entity<CoduxApp>,
    project_table: gpui::Entity<TableState<StatsProjectTableDelegate>>,
    scroll_handle: gpui::ScrollHandle,
    snapshot: StatsWorkspaceSnapshot,
    container_width: Option<Pixels>,
    cx: &mut Context<workspace_views::StatsWorkspaceView>,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .flex_basis(px(0.0))
        .w_full()
        .h_full()
        .min_w_0()
        .min_h_0()
        .bg(theme::vibrancy_panel(cx.theme().sidebar))
        .child(
            div()
                .relative()
                .size_full()
                .flex_1()
                .flex_basis(px(0.0))
                .min_h_0()
                .child(
                    div()
                        .id("stats-scroll-area")
                        .flex()
                        .flex_col()
                        .size_full()
                        .min_h_0()
                        .track_scroll(&scroll_handle)
                        .overflow_y_scroll()
                        .p(px(20.0))
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(16.0))
                                .child(stats_control_row(app_entity.clone(), &snapshot))
                                .child(stats_kpi_grid(&snapshot, cx))
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap(px(16.0))
                                        .min_w_0()
                                        .child(
                                            div()
                                                .flex_1()
                                                .flex_basis(px(0.0))
                                                .min_w(px(360.0))
                                                .child(stats_recent_trend_card(
                                                    app_entity.clone(),
                                                    &snapshot,
                                                    container_width,
                                                    cx,
                                                )),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .flex_basis(px(0.0))
                                                .min_w(px(360.0))
                                                .child(stats_heatmap_card(
                                                    app_entity.clone(),
                                                    &snapshot,
                                                    container_width,
                                                    cx,
                                                )),
                                        ),
                                )
                                .child(
                                    div()
                                        .grid()
                                        .grid_cols(2)
                                        .gap(px(16.0))
                                        .min_w_0()
                                        .child(stats_rank_card(
                                            stats_text(
                                                &snapshot.language,
                                                "stats.by_model",
                                                "Model Ranking",
                                            ),
                                            snapshot.model_rows.clone(),
                                            &snapshot.language,
                                            cx,
                                        ))
                                        .child(stats_rank_card(
                                            stats_text(
                                                &snapshot.language,
                                                "stats.by_tool",
                                                "Tool Ranking",
                                            ),
                                            snapshot.tool_rows.clone(),
                                            &snapshot.language,
                                            cx,
                                        )),
                                )
                                .child(stats_project_table_card(
                                    project_table,
                                    &snapshot,
                                    container_width,
                                    cx,
                                )),
                        ),
                )
                .vertical_scrollbar(&scroll_handle),
        )
}
