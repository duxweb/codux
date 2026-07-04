use super::*;
use crate::app::ui_helpers::codux_tooltip_container;
use chrono::{Datelike as _, TimeZone as _, Timelike as _};
use codux_runtime::i18n::translate;
use gpui::Rems;
use gpui_component::{
    Selectable, Size,
    button::{Button, ButtonVariants},
    progress::Progress,
    scroll::ScrollableElement,
    tab::{Tab, TabBar},
    table::{Column, ColumnSort, DataTable, TableDelegate, TableState},
};

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

fn stats_control_row(
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: &StatsWorkspaceSnapshot,
) -> impl IntoElement {
    let selected_cache_index = usize::from(snapshot.include_cached);
    let cache_mode_app_entity = app_entity.clone();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .child(
            div().flex().items_center().gap_2().children(
                [
                    StatsTimeRange::Today,
                    StatsTimeRange::SevenDays,
                    StatsTimeRange::ThirtyDays,
                    StatsTimeRange::All,
                ]
                .into_iter()
                .map(|range| {
                    stats_filter_button(
                        stats_time_range_label(range, &snapshot.language),
                        snapshot.time_range == range,
                        {
                            let app_entity = app_entity.clone();
                            move |_, _window, cx| {
                                cx.update_entity(&app_entity, |app, cx| {
                                    app.set_stats_time_range(range, cx);
                                });
                            }
                        },
                    )
                    .into_any_element()
                }),
            ),
        )
        .child(
            TabBar::new("stats-cache-mode-tabs")
                .pill()
                .with_size(Size::Small)
                .selected_index(selected_cache_index)
                .child(stats_cache_mode_tab(stats_text(
                    &snapshot.language,
                    "stats.cache_mode.normalized",
                    "No Cache",
                )))
                .child(stats_cache_mode_tab(stats_text(
                    &snapshot.language,
                    "stats.cache_mode.including_cache",
                    "With Cache",
                )))
                .on_click(move |index, window, cx| {
                    let mode = if *index == 1 {
                        "includingCache"
                    } else {
                        "normalized"
                    };
                    cx.update_entity(&cache_mode_app_entity, |app, cx| {
                        app.set_statistics_mode(mode.to_string(), window, cx);
                    });
                }),
        )
}

fn stats_filter_button(
    label: String,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let button = Button::new(SharedString::from(format!("stats-filter-{label}")));
    let button = if active {
        button.primary()
    } else {
        button.secondary()
    };

    button
        .with_size(Size::Small)
        .compact()
        .rounded(px(999.0))
        .selected(active)
        .child(
            div()
                .text_size(STATS_FILTER_TEXT_SIZE)
                .line_height(STATS_FILTER_LINE_HEIGHT)
                .child(label),
        )
        .on_click(on_click)
}

fn stats_cache_mode_tab(label: String) -> Tab {
    Tab::new().child(
        div()
            .text_size(STATS_FILTER_TEXT_SIZE)
            .line_height(STATS_FILTER_LINE_HEIGHT)
            .child(label),
    )
}

fn stats_kpi_grid(
    snapshot: &StatsWorkspaceSnapshot,
    cx: &mut Context<workspace_views::StatsWorkspaceView>,
) -> impl IntoElement {
    div()
        .grid()
        .grid_cols(4)
        .gap(px(12.0))
        .child(stats_kpi_card(
            stats_text(&snapshot.language, "stats.kpi.total", "Total Tokens"),
            compact_number(snapshot.range_total_tokens),
            stats_time_range_label(snapshot.time_range, &snapshot.language),
            HeroIconName::ChartBarSquare,
            cx,
        ))
        .child(stats_kpi_card(
            stats_text(&snapshot.language, "stats.kpi.no_cache", "No-Cache Tokens"),
            compact_number(snapshot.range_no_cache_tokens),
            stats_text(
                &snapshot.language,
                "stats.kpi.no_cache_hint",
                "Excludes cache reads",
            ),
            HeroIconName::Bolt,
            cx,
        ))
        .child(stats_kpi_card(
            stats_text(&snapshot.language, "stats.kpi.input", "Input"),
            compact_number(snapshot.range_input_tokens),
            stats_text(&snapshot.language, "stats.kpi.input_hint", "Input tokens"),
            HeroIconName::ArrowDownTray,
            cx,
        ))
        .child(stats_kpi_card(
            stats_text(&snapshot.language, "stats.kpi.output", "Output"),
            compact_number(snapshot.range_output_tokens),
            stats_text(
                &snapshot.language,
                "stats.kpi.output_hint",
                "Output includes reasoning",
            ),
            HeroIconName::ArrowUpTray,
            cx,
        ))
        .child(stats_kpi_card(
            stats_text(&snapshot.language, "stats.kpi.cache", "Cache"),
            compact_number(snapshot.range_cached_input_tokens),
            stats_text(
                &snapshot.language,
                "stats.kpi.cache_hint",
                "Cache read tokens",
            ),
            HeroIconName::CircleStack,
            cx,
        ))
        .child(stats_kpi_card(
            stats_text(&snapshot.language, "stats.kpi.requests", "Requests"),
            compact_number(snapshot.range_request_count),
            stats_text(
                &snapshot.language,
                "stats.kpi.requests_hint",
                "Selected range",
            ),
            HeroIconName::NumberedList,
            cx,
        ))
        .child(stats_kpi_card(
            stats_text(&snapshot.language, "stats.kpi.active_duration", "Runtime"),
            format_duration_short(snapshot.range_active_duration_seconds),
            stats_text(
                &snapshot.language,
                "stats.kpi.active_time",
                "Total execution time",
            ),
            HeroIconName::Clock,
            cx,
        ))
        .child(stats_kpi_card(
            stats_text(&snapshot.language, "stats.kpi.sessions", "Sessions"),
            compact_number(snapshot.range_session_count as i64),
            stats_text(
                &snapshot.language,
                "stats.kpi.sessions_hint",
                "Selected range",
            ),
            HeroIconName::UserCircle,
            cx,
        ))
}

fn stats_kpi_card(
    title: String,
    value: String,
    hint: String,
    icon: HeroIconName,
    cx: &mut Context<workspace_views::StatsWorkspaceView>,
) -> impl IntoElement {
    stats_card(cx)
        // Sits in a fixed grid; allow the tile to shrink with its column so the
        // grid never forces horizontal overflow on a narrow (≥720px) window.
        .min_w_0()
        .min_h(px(112.0))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_DIM))
                        .truncate()
                        .child(title),
                )
                .child(
                    div()
                        .size(px(28.0))
                        .flex_none()
                        .rounded(px(8.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(color(theme::ACCENT).opacity(0.12))
                        .text_color(color(theme::ACCENT))
                        .child(Icon::new(icon).size_3p5()),
                ),
        )
        .child(
            div()
                .mt(px(12.0))
                .text_size(rems(1.24))
                .line_height(rems(1.55))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT))
                .truncate()
                .child(value),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .truncate()
                .child(hint),
        )
}

fn stats_recent_trend_card(
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: &StatsWorkspaceSnapshot,
    container_width: Option<Pixels>,
    cx: &mut Context<workspace_views::StatsWorkspaceView>,
) -> impl IntoElement {
    let title = stats_text(&snapshot.language, "stats.recent_trend", "Usage Trend");
    let has_usage = snapshot
        .trend_buckets
        .iter()
        .any(|bucket| bucket.total_tokens > 0 || bucket.cached_input_tokens > 0);
    let visible_buckets = stats_trend_visible_buckets(container_width);
    let data = snapshot
        .trend_buckets
        .iter()
        .skip(snapshot.trend_buckets.len().saturating_sub(visible_buckets))
        .copied()
        .collect::<Vec<_>>();
    let max_value = data
        .iter()
        .map(|bucket| bucket.total_tokens.max(0))
        .max()
        .unwrap_or(0)
        .max(1);

    stats_section_card(title, cx)
        .min_h(px(STATS_CHART_CARD_HEIGHT))
        .child(if !has_usage {
            stats_empty(
                stats_text(
                    &snapshot.language,
                    "stats.recent_trend.empty",
                    "No usage data yet",
                ),
                cx,
            )
            .into_any_element()
        } else {
            stats_trend_bars(app_entity, data, max_value, snapshot.language.clone())
                .into_any_element()
        })
}

fn stats_trend_bars(
    app_entity: gpui::Entity<CoduxApp>,
    buckets: Vec<StatsTrendBucket>,
    max_value: i64,
    language: String,
) -> impl IntoElement {
    let axis = stats_trend_axis_labels(&buckets, &language);
    div()
        .mt(px(16.0))
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_end()
                .justify_between()
                .w_full()
                .h(px(STATS_CHART_BODY_HEIGHT))
                .children(buckets.into_iter().enumerate().map(move |(index, bucket)| {
                    let value = bucket.total_tokens.max(0);
                    let ratio = if max_value <= 0 {
                        0.0
                    } else {
                        value as f32 / max_value as f32
                    };
                    codux_tooltip_container(
                        app_entity.clone(),
                        SharedString::from(format!("stats-trend-bucket-{index}")),
                        trend_bucket_tooltip(&language, bucket),
                    )
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .flex()
                    .items_end()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(STATS_TREND_MAX_BAR_WIDTH))
                            .h(px(
                                8.0 + ratio.clamp(0.0, 1.0) * (STATS_CHART_BODY_HEIGHT - 8.0)
                            ))
                            .rounded(px(3.0))
                            .bg(color(theme::ACCENT))
                            .opacity(if value > 0 { 0.95 } else { 0.14 }),
                    )
                    .into_any_element()
                })),
        )
        .child(
            div()
                .mt(px(8.0))
                .h(px(1.0))
                .bg(color(theme::ACCENT).opacity(0.22)),
        )
        .child(
            div()
                .mt(px(7.0))
                .flex()
                .justify_between()
                .gap(px(16.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .children(axis),
        )
}

fn stats_rank_card(
    title: String,
    rows: Vec<StatsRankRow>,
    language: &str,
    cx: &mut Context<workspace_views::StatsWorkspaceView>,
) -> impl IntoElement {
    stats_section_card(title, cx).child(if rows.is_empty() {
        stats_empty(
            stats_text(language, "stats.empty.usage", "No usage data yet"),
            cx,
        )
        .into_any_element()
    } else {
        div()
            .mt(px(14.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .children(rows.into_iter().take(8).enumerate().map(|(index, row)| {
                stats_rank_row(index + 1, row, language, cx).into_any_element()
            }))
            .into_any_element()
    })
}

fn stats_rank_row(
    rank: usize,
    row: StatsRankRow,
    language: &str,
    cx: &mut Context<workspace_views::StatsWorkspaceView>,
) -> impl IntoElement {
    let active = row.value > 0 || row.request_count > 0;
    // Keep a visible sliver for tiny-but-nonzero shares so the bar never collapses to nothing.
    let fill_ratio = if row.value > 0 {
        row.percent.clamp(0.018, 1.0)
    } else {
        0.0
    };
    div()
        .flex()
        .flex_col()
        .gap(px(7.0))
        .child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(8.0))
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .w(px(15.0))
                                .flex_shrink_0()
                                .text_size(rems(0.75))
                                .text_color(color(theme::TEXT_DIM))
                                .child(format!("{rank}")),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(rems(0.8125))
                                .line_height(rems(1.1))
                                .text_color(color(theme::TEXT))
                                .child(row.label),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(8.0))
                        .flex_shrink_0()
                        .child(
                            div()
                                .text_size(rems(0.8125))
                                .line_height(rems(1.1))
                                .text_color(color(theme::TEXT))
                                .child(percent_label_from_ratio(row.percent)),
                        )
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .line_height(rems(1.1))
                                .text_color(color(theme::TEXT_MUTED))
                                .child(format!(
                                    "{} · {}",
                                    compact_number(row.value),
                                    stats_text(
                                        language,
                                        "stats.request_count_short_format",
                                        "%@ req",
                                    )
                                    .replace("%@", &compact_number(row.request_count))
                                )),
                        ),
                ),
        )
        .child(
            Progress::new(format!("stats-rank-progress-{rank}"))
                .value(fill_ratio * 100.0)
                .with_size(Size::Size(px(5.0)))
                .color(if active {
                    color(theme::ACCENT)
                } else {
                    cx.theme().secondary
                }),
        )
}

fn stats_heatmap_card(
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: &StatsWorkspaceSnapshot,
    container_width: Option<Pixels>,
    cx: &mut Context<workspace_views::StatsWorkspaceView>,
) -> impl IntoElement {
    let title = stats_text(&snapshot.language, "stats.recent_usage", "Activity Heatmap");
    let inactive = color(theme::TEXT_DIM).opacity(0.36);
    let total_columns = snapshot.heatmap.len() / STATS_HEATMAP_ROWS;
    let visible_columns = stats_heatmap_visible_columns(container_width).min(total_columns);
    let first_column = total_columns.saturating_sub(visible_columns);
    let cells = snapshot
        .heatmap
        .chunks(STATS_HEATMAP_ROWS)
        .skip(first_column)
        .flat_map(|column| column.iter().cloned())
        .collect::<Vec<_>>();
    let month_labels = stats_heatmap_month_labels(&cells, &snapshot.language);
    let visible_width = visible_columns as f32 * STATS_HEATMAP_CELL_SIZE
        + visible_columns.saturating_sub(1) as f32 * STATS_HEATMAP_GAP;
    stats_section_card(title, cx)
        .min_h(px(STATS_CHART_CARD_HEIGHT))
        .child(
            div()
                .mt(px(12.0))
                .w_full()
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(
                    div()
                        .w(px(visible_width))
                        .flex()
                        .gap(px(STATS_HEATMAP_GAP))
                        .children(cells.chunks(STATS_HEATMAP_ROWS).enumerate().map(
                            |(column, days)| {
                                let app_entity = app_entity.clone();
                                div()
                                    .w(px(STATS_HEATMAP_CELL_SIZE))
                                    .flex_none()
                                    .flex()
                                    .flex_col()
                                    .gap(px(STATS_HEATMAP_GAP))
                                    .children(days.iter().cloned().enumerate().map(
                                        move |(row, cell)| {
                                            let tooltip =
                                                heatmap_cell_tooltip(&snapshot.language, &cell);
                                            codux_tooltip_container(
                                                app_entity.clone(),
                                                SharedString::from(format!(
                                                    "stats-heatmap-{column}-{row}"
                                                )),
                                                tooltip,
                                            )
                                            .size(px(STATS_HEATMAP_CELL_SIZE))
                                            .rounded(px(4.0))
                                            .bg(if cell.is_known {
                                                color(theme::ACCENT)
                                            } else {
                                                inactive
                                            })
                                            .opacity(cell.opacity.clamp(0.0, 1.0))
                                            .into_any_element()
                                        },
                                    ))
                                    .into_any_element()
                            },
                        )),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .w(px(visible_width))
                        .flex()
                        .gap(px(STATS_HEATMAP_GAP))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .children(month_labels.into_iter().map(|month| {
                            let width = month.columns as f32 * STATS_HEATMAP_CELL_SIZE
                                + month.columns.saturating_sub(1) as f32 * STATS_HEATMAP_GAP;
                            div()
                                .w(px(width))
                                .flex_none()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(month.label)
                                .into_any_element()
                        })),
                ),
        )
}

fn stats_project_table_card(
    project_table: gpui::Entity<TableState<StatsProjectTableDelegate>>,
    snapshot: &StatsWorkspaceSnapshot,
    container_width: Option<Pixels>,
    cx: &mut Context<workspace_views::StatsWorkspaceView>,
) -> impl IntoElement {
    let title = stats_text(&snapshot.language, "stats.projects", "Project Stats");
    let table_width = stats_project_table_width(container_width);
    project_table.update(cx, |table, cx| {
        if table
            .delegate_mut()
            .set_layout_width(table_width, snapshot.language().to_string())
        {
            table.refresh(cx);
        }
    });
    stats_section_card(title, cx)
        .h(px(430.0))
        .child(if snapshot.project_rows.is_empty() {
            stats_empty(
                stats_text(
                    &snapshot.language,
                    "stats.projects.empty",
                    "No project stats yet",
                ),
                cx,
            )
            .into_any_element()
        } else {
            div()
                .mt(px(14.0))
                .flex_1()
                .min_h_0()
                .h(px(360.0))
                .rounded(px(10.0))
                .overflow_hidden()
                .child(
                    DataTable::new(&project_table)
                        .large()
                        .scrollbar_visible(true, true),
                )
                .into_any_element()
        })
}

fn stats_card(cx: &mut Context<workspace_views::StatsWorkspaceView>) -> gpui::Div {
    // Match the AI stats sidebar tiles: a raised tone over the panel that
    // inherits its vibrancy/opacity, no border.
    div()
        .rounded(px(12.0))
        .bg(theme::vibrancy_raised(cx.theme().sidebar))
        .p(px(14.0))
}

fn stats_section_card(
    title: String,
    cx: &mut Context<workspace_views::StatsWorkspaceView>,
) -> gpui::Div {
    stats_card(cx).min_w_0().flex().flex_col().child(
        div()
            .text_size(rems(0.875))
            .line_height(rems(1.125))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(color(theme::TEXT))
            .child(title),
    )
}

fn stats_empty(label: String, cx: &mut Context<workspace_views::StatsWorkspaceView>) -> gpui::Div {
    div()
        .mt(px(14.0))
        .min_h(px(92.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.0))
        .bg(cx.theme().secondary.opacity(0.35))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(color(theme::TEXT_DIM))
        .child(label)
}

fn stats_text(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
}

fn stats_time_range_label(range: StatsTimeRange, language: &str) -> String {
    match range {
        StatsTimeRange::Today => stats_text(language, "stats.range.today", "Today"),
        StatsTimeRange::SevenDays => stats_text(language, "stats.range.7d", "7 Days"),
        StatsTimeRange::ThirtyDays => stats_text(language, "stats.range.30d", "30 Days"),
        StatsTimeRange::All => stats_text(language, "stats.range.all", "All"),
    }
}

fn stats_date_label(day: Option<f64>) -> String {
    let Some(day) = day else {
        return String::new();
    };
    match chrono::Local.timestamp_opt(day as i64, 0).single() {
        Some(time) => format!("{}-{:02}-{:02}", time.year(), time.month(), time.day()),
        None => String::new(),
    }
}

fn stats_month_axis_label(month: u32, language: &str) -> String {
    match locale_from_language_setting(language).as_str() {
        "zh-Hans" | "zh-Hant" | "ja" => format!("{month}月"),
        "ko" => format!("{month}월"),
        _ => match month {
            1 => stats_text(language, "stats.month.short.january", "Jan"),
            2 => stats_text(language, "stats.month.short.february", "Feb"),
            3 => stats_text(language, "stats.month.short.march", "Mar"),
            4 => stats_text(language, "stats.month.short.april", "Apr"),
            5 => stats_text(language, "stats.month.short.may", "May"),
            6 => stats_text(language, "stats.month.short.june", "Jun"),
            7 => stats_text(language, "stats.month.short.july", "Jul"),
            8 => stats_text(language, "stats.month.short.august", "Aug"),
            9 => stats_text(language, "stats.month.short.september", "Sep"),
            10 => stats_text(language, "stats.month.short.october", "Oct"),
            11 => stats_text(language, "stats.month.short.november", "Nov"),
            12 => stats_text(language, "stats.month.short.december", "Dec"),
            _ => String::new(),
        },
    }
}

fn stats_heatmap_month_labels(
    cells: &[codux_runtime::ai_history::AIHistoryHeatmapCellView],
    language: &str,
) -> Vec<StatsHeatmapMonthLabel> {
    let mut labels = Vec::<StatsHeatmapMonthLabel>::new();
    let mut current_month = None::<u32>;
    for column in cells.chunks(STATS_HEATMAP_ROWS) {
        let Some(day) = column.first().and_then(|cell| {
            chrono::Local
                .timestamp_opt(cell.day as i64, 0)
                .single()
                .map(|date| date.month())
        }) else {
            continue;
        };
        if current_month != Some(day) {
            labels.push(StatsHeatmapMonthLabel {
                label: stats_month_axis_label(day, language),
                columns: 1,
            });
            current_month = Some(day);
        } else if let Some(label) = labels.last_mut() {
            label.columns += 1;
        }
    }
    for label in &mut labels {
        if label.columns < 2 {
            label.label.clear();
        }
    }
    labels
}

fn stats_heatmap_visible_columns(container_width: Option<Pixels>) -> usize {
    let Some(width) = container_width else {
        return STATS_HEATMAP_DEFAULT_COLUMNS;
    };
    let content_width = (width.as_f32() - 40.0).max(0.0);
    let two_column_min_width = 360.0 * 2.0 + 16.0;
    let card_width = if content_width >= two_column_min_width {
        (content_width - 16.0) / 2.0
    } else {
        content_width
    };
    let inner_width = (card_width - 28.0).max(STATS_HEATMAP_CELL_SIZE);
    let column_width = STATS_HEATMAP_CELL_SIZE + STATS_HEATMAP_GAP;
    ((inner_width + STATS_HEATMAP_GAP) / column_width)
        .floor()
        .max(STATS_HEATMAP_MIN_COLUMNS as f32)
        .min(STATS_HEATMAP_MAX_COLUMNS as f32) as usize
}

fn stats_trend_visible_buckets(container_width: Option<Pixels>) -> usize {
    let Some(width) = container_width else {
        return STATS_TREND_DEFAULT_BUCKET_COUNT;
    };
    let content_width = (width.as_f32() - 40.0).max(0.0);
    let two_column_min_width = 360.0 * 2.0 + 16.0;
    let card_width = if content_width >= two_column_min_width {
        (content_width - 16.0) / 2.0
    } else {
        content_width
    };
    let inner_width = (card_width - 28.0).max(STATS_TREND_BUCKET_MIN_WIDTH);
    (inner_width / STATS_TREND_BUCKET_MIN_WIDTH)
        .floor()
        .max(12.0)
        .min(STATS_TREND_MAX_BUCKET_COUNT as f32) as usize
}

fn stats_project_table_width(container_width: Option<Pixels>) -> f32 {
    container_width
        .map(|width| (width.as_f32() - 40.0 - 28.0).max(STATS_TABLE_BASE_WIDTH))
        .unwrap_or(STATS_TABLE_BASE_WIDTH)
}

fn trend_bucket_time_range(bucket: StatsTrendBucket) -> String {
    let timestamp = f64::from_bits(bucket.start_bits);
    let start = chrono::Local.timestamp_opt(timestamp as i64, 0).single();
    let end = chrono::Local
        .timestamp_opt((timestamp + STATS_TREND_BUCKET_SECONDS) as i64, 0)
        .single();
    match (start, end) {
        (Some(start), Some(end)) => format!(
            "{:02}/{:02} {:02}:{:02} - {:02}:{:02}",
            start.month(),
            start.day(),
            start.hour(),
            start.minute(),
            end.hour(),
            end.minute()
        ),
        _ => String::new(),
    }
}

fn trend_bucket_axis_label(language: &str, bucket: StatsTrendBucket) -> String {
    let timestamp = f64::from_bits(bucket.start_bits);
    match chrono::Local.timestamp_opt(timestamp as i64, 0).single() {
        Some(time) => match locale_from_language_setting(language).as_str() {
            "zh-Hans" | "zh-Hant" | "ja" => format!(
                "{}月{}日 {:02}:{:02}",
                time.month(),
                time.day(),
                time.hour(),
                time.minute()
            ),
            "ko" => format!(
                "{}월 {}일 {:02}:{:02}",
                time.month(),
                time.day(),
                time.hour(),
                time.minute()
            ),
            _ => format!(
                "{:02}/{:02} {:02}:{:02}",
                time.month(),
                time.day(),
                time.hour(),
                time.minute()
            ),
        },
        None => String::new(),
    }
}

fn stats_trend_axis_labels(buckets: &[StatsTrendBucket], language: &str) -> Vec<AnyElement> {
    if buckets.is_empty() {
        return Vec::new();
    }
    stats_trend_axis_indexes(buckets.len())
        .into_iter()
        .enumerate()
        .map(|(position, index)| {
            let mut item = div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(trend_bucket_axis_label(language, buckets[index]));
            if position == 1 && buckets.len() >= 3 {
                item = item.text_align(gpui::TextAlign::Center);
            } else if position > 0 {
                item = item.text_align(gpui::TextAlign::Right);
            }
            item.into_any_element()
        })
        .collect()
}

fn stats_trend_axis_indexes(bucket_count: usize) -> Vec<usize> {
    if bucket_count == 0 {
        Vec::new()
    } else if bucket_count == 1 {
        vec![0]
    } else if bucket_count < 3 {
        vec![0, bucket_count - 1]
    } else {
        vec![0, bucket_count / 2, bucket_count - 1]
    }
}

fn trend_bucket_tooltip(language: &str, bucket: StatsTrendBucket) -> String {
    let total = stats_total_tokens(bucket.no_cache_tokens, bucket.cached_input_tokens);
    format!(
        "{}\n{} {}\n{} {}\n{} {}\n{} {}",
        trend_bucket_time_range(bucket),
        stats_text(language, "stats.tooltip.input", "Input"),
        compact_number(bucket.input_tokens),
        stats_text(language, "stats.tooltip.output", "Output"),
        compact_number(bucket.output_tokens),
        stats_text(language, "stats.tooltip.cache", "Cache"),
        compact_number(bucket.cached_input_tokens),
        stats_text(language, "stats.tooltip.total", "Total"),
        compact_number(total)
    )
}

fn heatmap_cell_tooltip(
    language: &str,
    cell: &codux_runtime::ai_history::AIHistoryHeatmapCellView,
) -> String {
    let total = stats_total_tokens(cell.total_tokens, cell.cached_input_tokens);
    format!(
        "{}\n{} {}\n{} {}\n{} {}\n{} {}\n{} {}",
        stats_date_label(Some(cell.day)),
        stats_text(language, "stats.tooltip.input", "Input"),
        compact_number(cell.input_tokens),
        stats_text(language, "stats.tooltip.output", "Output"),
        compact_number(cell.output_tokens),
        stats_text(language, "stats.tooltip.cache", "Cache"),
        compact_number(cell.cached_input_tokens),
        stats_text(language, "stats.tooltip.total", "Total"),
        compact_number(total),
        stats_text(language, "stats.tooltip.requests", "Requests"),
        compact_number(cell.request_count),
    )
}

fn format_duration_short(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{seconds}s")
    }
}

fn percent_label_from_ratio(ratio: f32) -> String {
    let percent = (ratio as f64 * 100.0).clamp(0.0, 999.0);
    if percent >= 10.0 {
        format!("{percent:.0}%")
    } else {
        format!("{percent:.1}%")
    }
}

fn stats_range_summary<'a>(
    global: &'a codux_runtime::ai_history::AIGlobalHistorySummary,
    range: StatsTimeRange,
) -> Option<&'a codux_runtime::ai_history::AIGlobalHistoryRangeSummary> {
    let key = stats_time_range_key(range);
    global
        .range_summaries
        .iter()
        .find(|summary| summary.key == key)
}

fn stats_time_range_key(range: StatsTimeRange) -> &'static str {
    match range {
        StatsTimeRange::Today => "today",
        StatsTimeRange::SevenDays => "sevenDays",
        StatsTimeRange::ThirtyDays => "thirtyDays",
        StatsTimeRange::All => "all",
    }
}

fn stats_tool_rows(
    global: &codux_runtime::ai_history::AIGlobalHistorySummary,
    range: Option<&codux_runtime::ai_history::AIGlobalHistoryRangeSummary>,
    include_cached: bool,
) -> Vec<StatsRankRow> {
    let rows = range
        .map(|range| range.tool_breakdown.as_slice())
        .unwrap_or(global.tool_breakdown.as_slice());
    stats_breakdown_rows(rows, include_cached, 10)
}

fn stats_model_rows(
    global: &codux_runtime::ai_history::AIGlobalHistorySummary,
    range: Option<&codux_runtime::ai_history::AIGlobalHistoryRangeSummary>,
    include_cached: bool,
) -> Vec<StatsRankRow> {
    let rows = range
        .map(|range| range.model_breakdown.as_slice())
        .unwrap_or(global.model_breakdown.as_slice());
    stats_breakdown_rows(rows, include_cached, 10)
}

fn stats_breakdown_rows(
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

fn rank_rows_from_values(rows: Vec<(String, i64, i64)>, limit: usize) -> Vec<StatsRankRow> {
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

fn stats_project_table_rows(
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

fn stats_total_tokens(no_cache_tokens: i64, cached_input_tokens: i64) -> i64 {
    no_cache_tokens.max(0) + cached_input_tokens.max(0)
}

fn stats_global_heatmap(
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

fn stats_trend_buckets(
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

fn global_fingerprint(global: &codux_runtime::ai_history::AIGlobalHistorySummary) -> u64 {
    let projects = super::workspace_views::workspace_view_hash(
        &global
            .project_totals
            .iter()
            .map(|project| {
                (
                    project.project_path.clone(),
                    project.project_name.clone(),
                    project.input_tokens,
                    project.output_tokens,
                    project.total_tokens,
                    project.cached_input_tokens,
                    project.request_count,
                    project.active_duration_seconds,
                    project.today_total_tokens,
                )
            })
            .collect::<Vec<_>>(),
    );
    let ranges = super::workspace_views::workspace_view_hash(
        &global
            .range_summaries
            .iter()
            .map(|range| {
                (
                    range.key.clone(),
                    range.input_tokens,
                    range.output_tokens,
                    range.total_tokens,
                    range.cached_input_tokens,
                    range.request_count,
                    range.session_count,
                    range.active_duration_seconds,
                    range.project_totals.len(),
                    range.tool_breakdown.len(),
                    range.model_breakdown.len(),
                )
            })
            .collect::<Vec<_>>(),
    );
    let heatmap = super::workspace_views::workspace_view_hash(
        &global
            .heatmap
            .iter()
            .map(|day| {
                (
                    day.day.to_bits(),
                    day.input_tokens,
                    day.output_tokens,
                    day.total_tokens,
                    day.cached_input_tokens,
                    day.request_count,
                )
            })
            .collect::<Vec<_>>(),
    );
    let recent = super::workspace_views::workspace_view_hash(
        &global
            .recent_time_buckets
            .iter()
            .map(|bucket| {
                (
                    bucket.start.to_bits(),
                    bucket.input_tokens,
                    bucket.output_tokens,
                    bucket.total_tokens,
                    bucket.cached_input_tokens,
                    bucket.request_count,
                )
            })
            .collect::<Vec<_>>(),
    );
    super::workspace_views::workspace_view_hash(&(
        projects,
        ranges,
        heatmap,
        recent,
        global.total_tokens,
        global.cached_input_tokens,
        global.today_total_tokens,
        global.today_cached_input_tokens,
        global.session_count,
        global.indexed_project_count,
    ))
}

fn rank_fingerprint(rows: &[StatsRankRow]) -> u64 {
    super::workspace_views::workspace_view_hash(
        &rows
            .iter()
            .map(|row| {
                (
                    row.label.clone(),
                    row.value,
                    row.request_count,
                    (row.percent * 10_000.0) as i64,
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn project_rows_fingerprint(rows: &[StatsProjectRow]) -> u64 {
    super::workspace_views::workspace_view_hash(
        &rows
            .iter()
            .map(|row| {
                (
                    row.project.clone(),
                    row.project_path.clone(),
                    row.total_tokens,
                    row.no_cache_tokens,
                    row.input_tokens,
                    row.output_tokens,
                    row.cached_input_tokens,
                    row.request_count,
                    row.active_duration_seconds,
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn trend_buckets_fingerprint(rows: &[StatsTrendBucket]) -> u64 {
    super::workspace_views::workspace_view_hash(rows)
}

fn heatmap_fingerprint(rows: &[codux_runtime::ai_history::AIHistoryHeatmapCellView]) -> u64 {
    super::workspace_views::workspace_view_hash(
        &rows
            .iter()
            .map(|cell| {
                (
                    cell.day.to_bits(),
                    cell.value,
                    cell.input_tokens,
                    cell.output_tokens,
                    cell.total_tokens,
                    cell.cached_input_tokens,
                    cell.request_count,
                    cell.is_known,
                    (cell.opacity * 10_000.0) as i64,
                )
            })
            .collect::<Vec<_>>(),
    )
}

#[derive(Clone)]
pub(in crate::app) struct StatsProjectTableDelegate {
    rows: Vec<StatsProjectRow>,
    language: String,
    layout_width: f32,
    columns: Vec<Column>,
}

impl StatsProjectTableDelegate {
    pub(in crate::app) fn new(rows: Vec<StatsProjectRow>, language: String) -> Self {
        let columns = stats_project_table_columns(&language, STATS_TABLE_BASE_WIDTH);
        Self {
            rows,
            language,
            layout_width: STATS_TABLE_BASE_WIDTH,
            columns,
        }
    }

    pub(in crate::app) fn set_rows(&mut self, rows: Vec<StatsProjectRow>, language: String) {
        self.rows = rows;
        if self.language != language {
            self.columns = stats_project_table_columns(&language, self.layout_width);
            self.language = language;
        }
    }

    pub(in crate::app) fn set_layout_width(&mut self, layout_width: f32, language: String) -> bool {
        let layout_width = layout_width.max(STATS_TABLE_BASE_WIDTH);
        if self.language == language && (self.layout_width - layout_width).abs() < 1.0 {
            return false;
        }
        self.language = language;
        self.layout_width = layout_width;
        self.columns = stats_project_table_columns(&self.language, self.layout_width);
        true
    }

    fn sort_rows(&mut self, col_ix: usize, sort: ColumnSort) {
        if matches!(sort, ColumnSort::Default) {
            self.rows.sort_by(|left, right| {
                right
                    .total_tokens
                    .cmp(&left.total_tokens)
                    .then_with(|| left.project.cmp(&right.project))
            });
            return;
        }
        let descending = matches!(sort, ColumnSort::Descending);
        self.rows.sort_by(|left, right| {
            let ordering = match col_ix {
                0 => left.project.cmp(&right.project),
                1 => left.total_tokens.cmp(&right.total_tokens),
                2 => left.no_cache_tokens.cmp(&right.no_cache_tokens),
                3 => left.input_tokens.cmp(&right.input_tokens),
                4 => left.output_tokens.cmp(&right.output_tokens),
                5 => left.cached_input_tokens.cmp(&right.cached_input_tokens),
                6 => left.request_count.cmp(&right.request_count),
                7 => left
                    .active_duration_seconds
                    .cmp(&right.active_duration_seconds),
                _ => left.project.cmp(&right.project),
            };
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
}

impl TableDelegate for StatsProjectTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let column = self.column(col_ix, cx);
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .text_size(rems(0.8125))
            .line_height(rems(1.125))
            .text_color(color(theme::TEXT_MUTED))
            .child(column.name)
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(("stats-project-row", row_ix))
            .bg(if row_ix % 2 == 0 {
                cx.theme().secondary.opacity(0.12)
            } else {
                cx.theme().transparent
            })
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(row) = self.rows.get(row_ix) else {
            return div().into_any_element();
        };
        let text = match col_ix {
            0 => row.project.clone(),
            1 => compact_number(row.total_tokens),
            2 => compact_number(row.no_cache_tokens),
            3 => compact_number(row.input_tokens),
            4 => compact_number(row.output_tokens),
            5 => compact_number(row.cached_input_tokens),
            6 => compact_number(row.request_count),
            7 => format_duration_short(row.active_duration_seconds),
            _ => String::new(),
        };
        let cell = div()
            .size_full()
            .flex()
            .items_center()
            .px_3()
            .text_size(rems(0.875))
            .line_height(rems(1.125))
            .text_color(if col_ix == 0 {
                color(theme::TEXT)
            } else {
                color(theme::TEXT_MUTED)
            });
        if col_ix == 0 {
            cell.child(div().truncate().child(text)).into_any_element()
        } else {
            cell.justify_end()
                .child(div().text_align(gpui::TextAlign::Right).child(text))
                .into_any_element()
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        self.sort_rows(col_ix, sort);
        cx.notify();
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(rems(0.8125))
            .text_color(cx.theme().muted_foreground)
            .child(stats_text(
                &self.language,
                "stats.projects.empty",
                "No project stats yet",
            ))
            .into_any_element()
    }
}

fn stats_project_table_columns(language: &str, layout_width: f32) -> Vec<Column> {
    let project_width = 414.0 + (layout_width - STATS_TABLE_BASE_WIDTH).max(0.0) * 0.34;
    let metric_ratio = ((layout_width - project_width) / (STATS_TABLE_BASE_WIDTH - 414.0)).max(1.0);
    vec![
        Column::new(
            "project",
            stats_text(language, "stats.table.project", "Project"),
        )
        .width(px(project_width))
        .min_width(px(220.0))
        .sortable(),
        Column::new(
            "total",
            stats_text(language, "stats.table.total", "Total Tokens"),
        )
        .width(px(126.0 * metric_ratio))
        .text_right()
        .sortable(),
        Column::new(
            "no_cache",
            stats_text(language, "stats.table.no_cache", "No-Cache Tokens"),
        )
        .width(px(132.0 * metric_ratio))
        .text_right()
        .sortable(),
        Column::new("input", stats_text(language, "stats.table.input", "Input"))
            .width(px(104.0 * metric_ratio))
            .text_right()
            .sortable(),
        Column::new(
            "output",
            stats_text(language, "stats.table.output", "Output"),
        )
        .width(px(104.0 * metric_ratio))
        .text_right()
        .sortable(),
        Column::new("cache", stats_text(language, "stats.table.cache", "Cache"))
            .width(px(104.0 * metric_ratio))
            .text_right()
            .sortable(),
        Column::new(
            "requests",
            stats_text(language, "stats.table.requests", "Requests"),
        )
        .width(px(106.0 * metric_ratio))
        .text_right()
        .sortable(),
        Column::new(
            "duration",
            stats_text(language, "stats.table.duration", "Runtime"),
        )
        .width(px(110.0 * metric_ratio))
        .text_right()
        .sortable(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_table_total_tokens_always_include_cache() {
        let mut global = codux_runtime::ai_history::AIGlobalHistorySummary::default();
        global
            .project_totals
            .push(codux_runtime::ai_history::AIProjectUsageSummary {
                project_path: "/tmp/project".to_string(),
                project_name: "Project".to_string(),
                input_tokens: 80,
                output_tokens: 20,
                total_tokens: 100,
                cached_input_tokens: 40,
                request_count: 2,
                ..Default::default()
            });

        let rows = stats_project_table_rows(&global, None);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].no_cache_tokens, 100);
        assert_eq!(rows[0].cached_input_tokens, 40);
        assert_eq!(rows[0].total_tokens, 140);
    }

    #[test]
    fn range_project_table_total_tokens_always_include_cache() {
        let mut global = codux_runtime::ai_history::AIGlobalHistorySummary::default();
        global
            .project_totals
            .push(codux_runtime::ai_history::AIProjectUsageSummary {
                project_path: "/tmp/all".to_string(),
                project_name: "All".to_string(),
                total_tokens: 10,
                cached_input_tokens: 90,
                request_count: 1,
                ..Default::default()
            });
        let mut range = codux_runtime::ai_history::AIGlobalHistoryRangeSummary {
            key: "today".to_string(),
            ..Default::default()
        };
        range
            .project_totals
            .push(codux_runtime::ai_history::AIProjectUsageSummary {
                project_path: "/tmp/range".to_string(),
                project_name: "Range".to_string(),
                total_tokens: 30,
                cached_input_tokens: 12,
                request_count: 1,
                ..Default::default()
            });

        let rows = stats_project_table_rows(&global, Some(&range));

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].project, "Range");
        assert_eq!(rows[0].no_cache_tokens, 30);
        assert_eq!(rows[0].cached_input_tokens, 12);
        assert_eq!(rows[0].total_tokens, 42);
    }

    #[test]
    fn trend_bucket_tooltip_total_always_includes_cache() {
        let start = chrono::Local
            .with_ymd_and_hms(2026, 7, 4, 13, 30, 0)
            .unwrap()
            .timestamp() as f64;
        let tooltip = trend_bucket_tooltip(
            "english",
            StatsTrendBucket {
                start_bits: start.to_bits(),
                input_tokens: 80,
                output_tokens: 20,
                cached_input_tokens: 40,
                total_tokens: 100,
                no_cache_tokens: 100,
                request_count: 2,
            },
        );

        assert_eq!(
            tooltip,
            "07/04 13:30 - 14:00\nInput 80\nOutput 20\nCache 40\nTotal 140"
        );
    }

    #[test]
    fn month_axis_label_uses_current_language() {
        assert_eq!(stats_month_axis_label(7, "simplifiedChinese"), "7月");
        assert_eq!(stats_month_axis_label(7, "english"), "Jul");
    }

    #[test]
    fn heatmap_month_labels_group_visible_columns() {
        let jan_22 = chrono::Local
            .with_ymd_and_hms(2026, 1, 22, 0, 0, 0)
            .unwrap()
            .timestamp() as f64;
        let jan_29 = chrono::Local
            .with_ymd_and_hms(2026, 1, 29, 0, 0, 0)
            .unwrap()
            .timestamp() as f64;
        let feb_5 = chrono::Local
            .with_ymd_and_hms(2026, 2, 5, 0, 0, 0)
            .unwrap()
            .timestamp() as f64;
        let mut cells = Vec::new();
        for day in [jan_22, jan_29, feb_5] {
            for row in 0..STATS_HEATMAP_ROWS {
                cells.push(codux_runtime::ai_history::AIHistoryHeatmapCellView {
                    day: day + row as f64 * 24.0 * 60.0 * 60.0,
                    ..Default::default()
                });
            }
        }

        let labels = stats_heatmap_month_labels(&cells, "english");

        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].label, "Jan");
        assert_eq!(labels[0].columns, 2);
        assert_eq!(labels[1].label, "");
        assert_eq!(labels[1].columns, 1);
    }

    #[test]
    fn trend_bucket_axis_label_includes_date_and_time() {
        let start = chrono::Local
            .with_ymd_and_hms(2026, 7, 4, 13, 45, 0)
            .unwrap()
            .timestamp() as f64;
        let bucket = StatsTrendBucket {
            start_bits: start.to_bits(),
            ..Default::default()
        };

        assert_eq!(
            trend_bucket_axis_label("simplifiedChinese", bucket),
            "7月4日 13:45"
        );
        assert_eq!(trend_bucket_axis_label("english", bucket), "07/04 13:45");
    }

    #[test]
    fn trend_axis_indexes_do_not_duplicate_single_bucket() {
        assert_eq!(stats_trend_axis_indexes(0), Vec::<usize>::new());
        assert_eq!(stats_trend_axis_indexes(1), vec![0]);
        assert_eq!(stats_trend_axis_indexes(2), vec![0, 1]);
        assert_eq!(stats_trend_axis_indexes(5), vec![0, 2, 4]);
    }

    #[test]
    fn trend_bucket_visual_total_respects_cache_mode() {
        let mut global = codux_runtime::ai_history::AIGlobalHistorySummary::default();
        global
            .recent_time_buckets
            .push(codux_runtime::ai_history_normalized::AITimeBucket {
                start: 0.0,
                end: 1800.0,
                input_tokens: 80,
                output_tokens: 20,
                total_tokens: 100,
                cached_input_tokens: 40,
                request_count: 2,
            });

        let no_cache = stats_trend_buckets(&global, false);
        let with_cache = stats_trend_buckets(&global, true);

        assert_eq!(no_cache[0].total_tokens, 100);
        assert_eq!(with_cache[0].total_tokens, 140);
        assert_eq!(no_cache[0].no_cache_tokens, 100);
        assert_eq!(with_cache[0].no_cache_tokens, 100);
        assert_eq!(no_cache[0].cached_input_tokens, 40);
        assert_eq!(with_cache[0].cached_input_tokens, 40);
    }

    #[test]
    fn heatmap_cell_tooltip_includes_token_breakdown() {
        let tooltip = heatmap_cell_tooltip(
            "english",
            &codux_runtime::ai_history::AIHistoryHeatmapCellView {
                day: 0.0,
                value: 100,
                input_tokens: 80,
                output_tokens: 20,
                total_tokens: 100,
                cached_input_tokens: 40,
                request_count: 2,
                is_known: true,
                opacity: 1.0,
            },
        );

        assert!(tooltip.contains("Input 80"));
        assert!(tooltip.contains("Output 20"));
        assert!(tooltip.contains("Cache 40"));
        assert!(tooltip.contains("Total 140"));
        assert!(tooltip.contains("Requests 2"));
    }
}
