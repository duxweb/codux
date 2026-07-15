use super::format::*;
use super::*;

pub(super) fn stats_control_row(
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

pub(super) fn stats_filter_button(
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
        .child(
            div()
                .text_size(STATS_FILTER_TEXT_SIZE)
                .line_height(STATS_FILTER_LINE_HEIGHT)
                .child(label),
        )
        .on_click(on_click)
}

pub(super) fn stats_cache_mode_tab(label: String) -> Tab {
    Tab::new().child(
        div()
            .text_size(STATS_FILTER_TEXT_SIZE)
            .line_height(STATS_FILTER_LINE_HEIGHT)
            .child(label),
    )
}

pub(super) fn stats_kpi_grid(
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

pub(super) fn stats_kpi_card(
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

pub(super) fn stats_recent_trend_card(
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

pub(super) fn stats_trend_bars(
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

pub(super) fn stats_rank_card(
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

pub(super) fn stats_rank_row(
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

pub(super) fn stats_heatmap_card(
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

pub(super) fn stats_project_table_card(
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

pub(super) fn stats_card(cx: &mut Context<workspace_views::StatsWorkspaceView>) -> gpui::Div {
    // Match the AI stats sidebar tiles: a raised tone over the panel that
    // inherits its vibrancy/opacity, no border.
    div()
        .rounded(px(12.0))
        .bg(theme::vibrancy_raised(cx.theme().sidebar))
        .p(px(14.0))
}

pub(super) fn stats_section_card(
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

pub(super) fn stats_empty(
    label: String,
    cx: &mut Context<workspace_views::StatsWorkspaceView>,
) -> gpui::Div {
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
