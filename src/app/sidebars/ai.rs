use super::{formatting::relative_time_label_for_language, *};
use crate::app::ui_helpers::{centered_empty_state, codux_tooltip_container, with_codux_tooltip};
use chrono::{Datelike as _, TimeZone as _, Timelike as _};
use codux_runtime::{
    ai_runtime_state::AIRuntimeStateSummary, i18n::translate,
    settings::locale_from_language_setting,
};
use gpui::Hsla;
use gpui_component::input::{Input, InputState};
use std::collections::BTreeMap;

const AI_RECENT_USAGE_COLUMNS: usize = 20;
const AI_RECENT_USAGE_CELL_SIZE: f32 = 10.0;
const AI_RECENT_USAGE_GAP: f32 = 3.0;

#[derive(Clone)]
struct AIUsageCell {
    value: i64,
    request_count: i64,
    is_known: bool,
    tooltip: String,
}

struct AITodayUsageBucket {
    value: i64,
    request_count: i64,
    tooltip: String,
}

struct AIUsageLabels {
    tokens: String,
    request_format: String,
    request_count_format: String,
    unknown_date: String,
    weekdays: [&'static str; 7],
}

impl AIUsageLabels {
    fn load(language: &str) -> Self {
        let locale = locale_from_language_setting(language);
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        let weekdays = match locale.as_str() {
            "zh-Hans" => ["周一", "周二", "周三", "周四", "周五", "周六", "周日"],
            "zh-Hant" => ["週一", "週二", "週三", "週四", "週五", "週六", "週日"],
            "ja" => ["月", "火", "水", "木", "金", "土", "日"],
            "ko" => ["월", "화", "수", "목", "금", "토", "일"],
            _ => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
        };
        Self {
            tokens: tr("ai.metric.token", "tokens"),
            request_format: tr("ai.metric.requests_format", "%d requests"),
            request_count_format: tr("ai.metric.request_count_format", "%d requests"),
            unknown_date: tr("common.unknown_date", "Unknown date"),
            weekdays,
        }
    }
}

pub(in crate::app) fn ai_stats_sidebar(
    history: &AIHistorySummary,
    selected_project_id: Option<&str>,
    statistics_mode: &str,
    ai_runtime_state: &AIRuntimeStateSummary,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title = ai_sidebar_text(language, "ai.panel.statistics_title", "AI Statistics");
    let current_project_label =
        ai_sidebar_text(language, "ai.summary.current_project", "Current Project");
    let today_total_label = ai_sidebar_text(language, "ai.summary.today_total", "Today's Total");
    let tool_ranking_label = ai_sidebar_text(language, "ai.breakdown.tool_ranking", "Tool Ranking");
    let model_ranking_label =
        ai_sidebar_text(language, "ai.breakdown.model_ranking", "Model Ranking");
    let include_cached = statistics_mode == "includingCache";
    let live_sessions = ai_live_sessions(ai_runtime_state, selected_project_id);
    let indexed_baselines = ai_indexed_session_baselines(history);
    let live_project_total_tokens =
        ai_live_sessions_total(&live_sessions, &indexed_baselines, include_cached);
    let live_today_tokens =
        ai_live_sessions_today_total(&live_sessions, &indexed_baselines, include_cached);
    let project_total_tokens = ai_display_tokens(
        history.project_total_tokens,
        history.project_cached_input_tokens,
        include_cached,
    ) + live_project_total_tokens;
    let today_total_tokens = ai_history_today_total(history, include_cached) + live_today_tokens;
    let tool_rows = ai_tool_rows(history, &live_sessions, &indexed_baselines, include_cached);
    let model_rows = ai_model_rows(history, &live_sessions, &indexed_baselines, include_cached);

    div()
        .flex()
        .flex_1()
        .h_full()
        .min_h_0()
        .flex_col()
        .child(assistant_panel_header(
            title,
            HeroIconName::Sparkles,
            header_icon_button(
                "ai-stats-refresh",
                HeroIconName::ArrowPath,
                cx,
                |app, _event, _window, cx| app.start_ai_history_refresh(true, cx),
            ),
        ))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .p(px(12.0))
                .flex()
                .flex_col()
                .child(ai_current_session_card(
                    &live_sessions,
                    include_cached,
                    language,
                    cx,
                ))
                .child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .child(div().flex_1().mr(px(12.0)).child(ai_metric_card(
                            current_project_label,
                            compact_number(project_total_tokens),
                            cx,
                        )))
                        .child(div().flex_1().child(ai_metric_card(
                            today_total_label,
                            compact_number(today_total_tokens),
                            cx,
                        ))),
                )
                .child(div().mt(px(12.0)).child(ai_today_usage_chart(
                    history,
                    &live_sessions,
                    &indexed_baselines,
                    include_cached,
                    language,
                    cx,
                )))
                .child(div().mt(px(12.0)).child(ai_recent_usage_heatmap(
                    history,
                    &live_sessions,
                    &indexed_baselines,
                    include_cached,
                    language,
                    cx,
                )))
                .child(div().mt(px(12.0)).child(ai_ranking_card(
                    tool_ranking_label,
                    tool_rows,
                    language,
                    cx,
                )))
                .child(div().mt(px(12.0)).child(ai_ranking_card(
                    model_ranking_label,
                    model_rows,
                    language,
                    cx,
                ))),
        )
}

fn ai_sidebar_text(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
}

fn ai_live_sessions<'a>(
    ai_runtime_state: &'a AIRuntimeStateSummary,
    selected_project_id: Option<&str>,
) -> Vec<&'a codux_runtime::ai_runtime_state::AIRuntimeSessionSummary> {
    ai_runtime_state
        .sessions
        .iter()
        .filter(|session| {
            selected_project_id
                .map(|project_id| session.project_id == project_id)
                .unwrap_or(true)
        })
        .collect()
}

fn ai_live_sessions_total(
    sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    indexed_baselines: &BTreeMap<String, (i64, i64)>,
    include_cached: bool,
) -> i64 {
    sessions
        .iter()
        .map(|session| {
            ai_live_session_delta_tokens(session, indexed_baselines, include_cached, None)
        })
        .sum()
}

fn ai_live_sessions_today_total(
    sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    indexed_baselines: &BTreeMap<String, (i64, i64)>,
    include_cached: bool,
) -> i64 {
    let now = ai_now_seconds();
    let day_start = ai_local_day_start_seconds(now);
    sessions
        .iter()
        .map(|session| {
            ai_live_session_delta_tokens(
                session,
                indexed_baselines,
                include_cached,
                Some(day_start),
            )
        })
        .sum()
}

fn ai_live_session_delta_tokens(
    session: &codux_runtime::ai_runtime_state::AIRuntimeSessionSummary,
    indexed_baselines: &BTreeMap<String, (i64, i64)>,
    include_cached: bool,
    day_start: Option<f64>,
) -> i64 {
    if let Some(day_start) = day_start {
        if session.updated_at < day_start {
            return 0;
        }
    }

    let raw_total = if session.raw_total_tokens > 0 {
        session.raw_total_tokens
    } else {
        session.total_tokens + session.baseline_total_tokens
    };
    let raw_cached = if session.raw_cached_input_tokens > 0 {
        session.raw_cached_input_tokens
    } else {
        session.cached_input_tokens + session.baseline_cached_input_tokens
    };
    let (indexed_total, indexed_cached) =
        ai_indexed_baseline_for_session(session, indexed_baselines);
    let (today_total, today_cached) = day_start
        .map(|day_start| {
            let started_at = session.started_at.unwrap_or(session.updated_at);
            if ai_local_day_start_seconds(started_at) == day_start {
                (0, 0)
            } else {
                (raw_total, raw_cached)
            }
        })
        .unwrap_or((0, 0));
    let total_baseline = session
        .baseline_total_tokens
        .max(indexed_total)
        .max(today_total);
    let cached_baseline = session
        .baseline_cached_input_tokens
        .max(indexed_cached)
        .max(today_cached);

    ai_display_tokens(
        raw_total - total_baseline,
        raw_cached - cached_baseline,
        include_cached,
    )
}

fn ai_indexed_session_baselines(history: &AIHistorySummary) -> BTreeMap<String, (i64, i64)> {
    let mut baselines = BTreeMap::new();
    for session in &history.sessions {
        let Some(key) =
            ai_indexed_session_key(&session.source, session.external_session_id.as_deref())
        else {
            continue;
        };
        let entry = baselines.entry(key).or_insert((0, 0));
        entry.0 = entry.0.max(session.total_tokens);
        entry.1 = entry.1.max(session.cached_input_tokens);
    }
    baselines
}

fn ai_indexed_baseline_for_session(
    session: &codux_runtime::ai_runtime_state::AIRuntimeSessionSummary,
    indexed_baselines: &BTreeMap<String, (i64, i64)>,
) -> (i64, i64) {
    let Some(key) = ai_indexed_session_key(&session.tool, session.ai_session_id.as_deref()) else {
        return (0, 0);
    };
    indexed_baselines.get(&key).copied().unwrap_or((0, 0))
}

fn ai_indexed_session_key(tool: &str, external_session_id: Option<&str>) -> Option<String> {
    let tool = tool.trim().to_lowercase();
    let external_session_id = external_session_id?.trim();
    if tool.is_empty() || external_session_id.is_empty() {
        return None;
    }
    Some(format!("{tool}|{external_session_id}"))
}

fn ai_live_session_counts_for_day(
    session: &codux_runtime::ai_runtime_state::AIRuntimeSessionSummary,
    day_start: f64,
) -> bool {
    if session.updated_at < day_start {
        return false;
    }
    let started_at = session.started_at.unwrap_or(session.updated_at);
    ai_local_day_start_seconds(started_at) == day_start
}

pub(in crate::app) fn ai_display_tokens(
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

fn ai_history_today_total(history: &AIHistorySummary, include_cached: bool) -> i64 {
    let bucket_total = history
        .today_time_buckets
        .iter()
        .map(|bucket| {
            ai_display_tokens(
                bucket.total_tokens,
                bucket.cached_input_tokens,
                include_cached,
            )
        })
        .sum::<i64>();
    let now = ai_now_seconds();
    let today = ai_local_day_start_seconds(now);
    let heatmap_total = history
        .heatmap
        .iter()
        .filter(|day| (ai_local_day_start_seconds(day.day) - today).abs() < 1.0)
        .map(|day| ai_display_tokens(day.total_tokens, day.cached_input_tokens, include_cached))
        .sum::<i64>();
    let summary_total = if ai_history_has_fresh_today_evidence(history, today) {
        ai_display_tokens(
            history.today_total_tokens,
            history.today_cached_input_tokens,
            include_cached,
        )
    } else {
        0
    };
    summary_total.max(bucket_total).max(heatmap_total)
}

fn ai_history_has_fresh_today_evidence(history: &AIHistorySummary, today: f64) -> bool {
    history
        .today_time_buckets
        .iter()
        .any(|bucket| ai_local_day_start_seconds(bucket.start) == today)
        || history
            .heatmap
            .iter()
            .any(|day| ai_local_day_start_seconds(day.day) == today)
        || history
            .indexed_at
            .map(|indexed_at| ai_local_day_start_seconds(indexed_at) == today)
            .unwrap_or(false)
}

fn ai_stats_card(title: impl Into<String>, cx: &mut Context<CoduxApp>) -> gpui::Div {
    let title = title.into();
    div()
        .flex()
        .flex_col()
        .rounded(px(8.0))
        .bg(ai_stats_surface(cx))
        .p(px(12.0))
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .child(title),
        )
}

fn ai_current_session_card(
    sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    include_cached: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let empty_label = ai_sidebar_text(
        language,
        "ai.live_sessions.empty",
        "There are no current AI sessions right now",
    );
    let title = ai_sidebar_text(language, "ai.live_sessions", "Current Session Totals");
    let body = if sessions.is_empty() {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_size(rems(0.75))
            .line_height(rems(1.0))
            .text_color(color(theme::TEXT_DIM))
            .child(empty_label)
            .into_any_element()
    } else {
        div()
            .mt(px(10.0))
            .flex()
            .flex_col()
            .children(sessions.iter().take(6).map(|session| {
                ai_live_session_row(session, include_cached, language, cx).into_any_element()
            }))
            .into_any_element()
    };

    ai_stats_card(title, cx).min_h(px(100.0)).child(body)
}

fn ai_live_session_row(
    session: &codux_runtime::ai_runtime_state::AIRuntimeSessionSummary,
    include_cached: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let session_total_label = ai_sidebar_text(language, "ai.metric.session_total", "Session Total");
    div()
        .mb(px(8.0))
        .rounded(px(8.0))
        .bg(ai_stats_track_surface(cx))
        .px(px(10.0))
        .py(px(8.0))
        .flex()
        .items_start()
        .justify_between()
        .gap_3()
        .child(
            div()
                .min_w_0()
                .child(
                    div()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(if session.tool.trim().is_empty() {
                            "-".to_string()
                        } else {
                            session.tool.clone()
                        }),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_DIM))
                        .truncate()
                        .child(session.model.clone().unwrap_or_else(|| "-".to_string())),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_right()
                .child(
                    div()
                        .text_size(rems(1.0))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .child(compact_number(ai_display_tokens(
                            if session.raw_total_tokens > 0 {
                                session.raw_total_tokens
                            } else {
                                session.total_tokens + session.baseline_total_tokens
                            },
                            if session.raw_cached_input_tokens > 0 {
                                session.raw_cached_input_tokens
                            } else {
                                session.cached_input_tokens + session.baseline_cached_input_tokens
                            },
                            include_cached,
                        ))),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(session_total_label),
                ),
        )
}

pub(in crate::app) fn memory_manager_window_workspace(
    manager: &MemoryManagerSnapshot,
    active_tab: MemoryManagerTab,
    selected_scope: &str,
    selected_project_id: Option<&str>,
    selected_memory_entry_id: Option<&str>,
    selected_memory_summary_id: Option<&str>,
    memory_processing: bool,
    memory_refreshing: bool,
    project_profile_refreshing: bool,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let window_title = ai_sidebar_text(language, "memory.manager.window.title", "Memory Manager");
    let title = ai_sidebar_text(language, "memory.manager.title", "Memory");
    let subtitle = ai_sidebar_text(
        language,
        "memory.manager.subtitle",
        "Browse and clean extracted memories",
    );
    let summary_tab = ai_sidebar_text(language, "memory.manager.tab.summary", "Summary");
    let memories_tab = ai_sidebar_text(language, "memory.manager.tab.active", "Memories");
    let history_tab = ai_sidebar_text(language, "memory.manager.tab.history", "History");
    let failed_tab = ai_sidebar_text(language, "memory.manager.tab.failed", "Failed");
    let empty_entries = ai_sidebar_text(
        language,
        "memory.manager.empty.entries",
        "No memories in this view",
    );
    let empty_summary = ai_sidebar_text(
        language,
        "memory.manager.empty.summary",
        "No summary memory",
    );
    let empty_failed = ai_sidebar_text(
        language,
        "memory.manager.empty.failed",
        "No failed memory tasks",
    );
    let selected_target_title = if selected_scope == "user" {
        ai_sidebar_text(language, "memory.manager.user_memory", "User Memory")
    } else {
        manager.selected_target_title.clone()
    };
    let overview_template = ai_sidebar_text(
        language,
        "memory.manager.overview_format",
        "%lld active, %lld archived, %lld profiles, %lld summaries, %lld tokens",
    );
    let overview = &manager.current_overview;
    let overview_label = overview_template
        .replacen("%lld", &overview.active_entry_count.to_string(), 1)
        .replacen(
            "%lld",
            &(overview.archived_entry_count + overview.merged_entry_count).to_string(),
            1,
        )
        .replacen("%lld", &overview.profile_count.to_string(), 1)
        .replacen("%lld", &overview.summary_count.to_string(), 1)
        .replacen("%lld", &overview.total_token_estimate.to_string(), 1);
    let target_rows = manager.target_rows.clone();
    let content = ai_memory_manager_window_content(
        manager,
        active_tab,
        selected_scope == "project",
        selected_memory_entry_id,
        selected_memory_summary_id,
        project_profile_refreshing,
        empty_entries,
        empty_summary,
        empty_failed,
        language,
        window,
        cx,
    );

    child_window_shell(window_title, cx)
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .w(px(260.0))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .min_h_0()
                        .border_r_1()
                        .border_color(cx.theme().sidebar_border)
                        .bg(cx.theme().sidebar)
                        .child(
                            div()
                                .px_4()
                                .pt(px(18.0))
                                .pb(px(14.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .flex_1()
                                                .text_size(rems(1.125))
                                                .line_height(rems(1.375))
                                                .text_color(cx.theme().foreground)
                                                .child(title),
                                        )
                                        .child(div().flex().items_center().gap_1().child(
                                            ai_memory_header_icon_button(
                                                "memory-manager-window-process",
                                                HeroIconName::ArrowPath,
                                                ai_sidebar_text(
                                                    language,
                                                    "memory.manager.index_now",
                                                    "Index Now",
                                                ),
                                                memory_processing,
                                                cx,
                                                |app, _event, window, cx| {
                                                    app.process_memory_sessions_now(window, cx)
                                                },
                                            ),
                                        )),
                                )
                                .child(
                                    div()
                                        .mt(px(4.0))
                                        .text_size(rems(0.75))
                                        .line_height(rems(1.0625))
                                        .text_color(cx.theme().muted_foreground)
                                        .child(subtitle),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scrollbar()
                                .px_2()
                                .pb_3()
                                .children(target_rows.into_iter().map(|target| {
                                    ai_memory_manager_target_row(
                                        target,
                                        selected_scope,
                                        selected_project_id,
                                        language,
                                        cx,
                                    )
                                    .into_any_element()
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .flex_col()
                        .bg(cx.theme().background)
                        .child(
                            div()
                                .flex_shrink_0()
                                .border_b_1()
                                .border_color(cx.theme().border)
                                .px(px(20.0))
                                .pt(px(18.0))
                                .pb(px(14.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_start()
                                        .justify_between()
                                        .gap_3()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .flex_1()
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .text_size(rems(0.875))
                                                        .line_height(rems(1.125))
                                                        .text_color(cx.theme().foreground)
                                                        .child(selected_target_title),
                                                )
                                                .child(
                                                    div()
                                                        .mt(px(4.0))
                                                        .truncate()
                                                        .text_size(rems(0.75))
                                                        .line_height(rems(1.0))
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(overview_label),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .when(selected_scope == "project", |this| {
                                                    this.child(ai_memory_migrate_project_button(
                                                        manager,
                                                        selected_project_id,
                                                        language,
                                                        cx,
                                                    ))
                                                    .child(ai_memory_row_icon_button(
                                                        "memory-manager-window-delete-project",
                                                        HeroIconName::Trash,
                                                        ai_sidebar_text(
                                                            language,
                                                            "memory.manager.delete_project",
                                                            "Delete Project Memory",
                                                        ),
                                                        cx,
                                                        |app, _event, window, cx| {
                                                            app.delete_selected_memory_project(
                                                                window, cx,
                                                            )
                                                        },
                                                    ))
                                                })
                                                .child(ai_memory_header_icon_button(
                                                    "memory-manager-window-refresh",
                                                    HeroIconName::ArrowPath,
                                                    ai_sidebar_text(
                                                        language,
                                                        "common.refresh",
                                                        "Refresh",
                                                    ),
                                                    memory_refreshing,
                                                    cx,
                                                    |app, _event, window, cx| {
                                                        app.reload_memory(window, cx)
                                                    },
                                                )),
                                        ),
                                )
                                .child(
                                    div()
                                        .mt(px(14.0))
                                        .flex()
                                        .items_center()
                                        .child(ai_memory_manager_tab_button(
                                            summary_tab,
                                            MemoryManagerTab::Summary,
                                            active_tab,
                                            cx,
                                        ))
                                        .child(ai_memory_manager_tab_button(
                                            memories_tab,
                                            MemoryManagerTab::Active,
                                            active_tab,
                                            cx,
                                        ))
                                        .child(ai_memory_manager_tab_button(
                                            history_tab,
                                            MemoryManagerTab::History,
                                            active_tab,
                                            cx,
                                        ))
                                        .child(ai_memory_manager_tab_button(
                                            failed_tab,
                                            MemoryManagerTab::Failed,
                                            active_tab,
                                            cx,
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scrollbar()
                                .p(px(14.0))
                                .child(content),
                        ),
                ),
        )
        .child(ai_memory_manager_status_bar(manager, language))
}

fn ai_memory_manager_status_bar(
    manager: &MemoryManagerSnapshot,
    language: &str,
) -> impl IntoElement {
    let queued = manager.extraction.queued.max(0);
    let running = manager.extraction.running.max(0);
    let failed = manager.extraction.failed.max(0);
    let error = manager
        .extraction
        .last_error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let active = queued > 0 || running > 0;
    let status_label = if let Some(error) = error.clone() {
        error
    } else if active {
        ai_sidebar_text(language, "memory.status.processing", "Remembering")
    } else {
        ai_sidebar_text(language, "memory.status.idle", "Memory idle")
    };

    div()
        .flex_shrink_0()
        .border_t_1()
        .border_color(color(theme::BORDER_SOFT))
        .bg(color(theme::STATUS_BAR))
        .px_4()
        .h(px(30.0))
        .flex()
        .items_center()
        .gap_2()
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(color(theme::TEXT_MUTED))
        .when(active && error.is_none(), |this| {
            this.child(Spinner::new().xsmall().color(color(theme::ORANGE)))
        })
        .when(!active && error.is_none(), |this| {
            this.child(
                div()
                    .size(px(8.0))
                    .rounded_full()
                    .bg(color(theme::TEXT_DIM)),
            )
        })
        .when_some(error.clone(), |this, _| {
            this.child(div().size(px(8.0)).rounded_full().bg(color(0xF47C7C)))
        })
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_color(color(if error.is_some() {
                    0xF47C7C
                } else if active {
                    theme::ORANGE
                } else {
                    theme::TEXT_DIM
                }))
                .child(status_label),
        )
        .when(failed > 0 && error.is_none(), |this| {
            this.child(
                div()
                    .flex_shrink_0()
                    .text_color(color(0xF47C7C))
                    .child(format!(
                        "{failed} {}",
                        ai_sidebar_text(language, "memory.status.short_failed", "Failed")
                    )),
            )
        })
}

fn ai_memory_manager_window_content(
    manager: &MemoryManagerSnapshot,
    active_tab: MemoryManagerTab,
    is_project_scope: bool,
    selected_memory_entry_id: Option<&str>,
    selected_memory_summary_id: Option<&str>,
    project_profile_refreshing: bool,
    empty_entries: String,
    empty_summary: String,
    empty_failed: String,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let mut content = div().size_full().flex().flex_col();
    if active_tab == MemoryManagerTab::Summary {
        if is_project_scope {
            if let Some(profile) = manager.project_profile.clone() {
                content = content.child(ai_memory_project_profile_row(
                    profile,
                    project_profile_refreshing,
                    language,
                    cx,
                ));
            } else {
                content = content.child(ai_memory_project_profile_empty_row(
                    project_profile_refreshing,
                    language,
                    cx,
                ));
            }
        }
        if manager.summaries.is_empty() {
            if is_project_scope {
                return content.into_any_element();
            }
            return ai_memory_manager_empty_row(empty_summary, cx).into_any_element();
        }
        return content
            .child(ai_memory_section_label(
                ai_sidebar_text(language, "memory.manager.tab.summary", "Summary"),
                cx,
            ))
            .children(manager.summaries.iter().map(|summary| {
                ai_memory_manager_summary_row(
                    summary,
                    selected_memory_summary_id,
                    language,
                    window,
                    cx,
                )
                .into_any_element()
            }))
            .into_any_element();
    }

    if active_tab == MemoryManagerTab::Failed {
        if manager.failed_extractions.is_empty() {
            return content
                .child(ai_memory_manager_empty_row(empty_failed, cx))
                .into_any_element();
        }
        return content
            .children(
                manager.failed_extractions.iter().cloned().map(|task| {
                    ai_memory_failed_extraction_row(task, language, cx).into_any_element()
                }),
            )
            .into_any_element();
    }

    if manager.entries.is_empty() {
        return content
            .child(ai_memory_manager_empty_row(empty_entries, cx))
            .into_any_element();
    }
    content
        .children(ai_memory_manager_entry_groups(
            &manager.entries,
            selected_memory_entry_id,
            active_tab,
            language,
            cx,
        ))
        .into_any_element()
}

fn ai_memory_manager_target_row(
    target: codux_runtime::memory::MemoryManagerTargetRow,
    selected_scope: &str,
    selected_project_id: Option<&str>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let scope = target.scope.clone();
    let project_id = target.project_id.clone();
    let title = if scope == "user" {
        ai_sidebar_text(language, "memory.manager.user_memory", "User Memory")
    } else {
        target.title.clone()
    };
    let subtitle = if scope == "user" {
        ai_sidebar_text(
            language,
            "memory.manager.user_memory.subtitle",
            "Cross-project preferences",
        )
    } else {
        target.subtitle.clone()
    };
    let active = scope == selected_scope
        && (scope != "project" || project_id.as_deref() == selected_project_id);
    let active_bg = cx.theme().sidebar_accent;
    div()
        .id(SharedString::from(format!(
            "memory-manager-target-{}",
            target.id
        )))
        .mb(px(6.0))
        .min_h(px(54.0))
        .w_full()
        .rounded(px(8.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .text_color(if active {
            cx.theme().foreground
        } else {
            cx.theme().muted_foreground
        })
        .bg(if active {
            active_bg
        } else {
            cx.theme().transparent
        })
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(move |app, _event, _window, cx| {
            app.select_memory_manager_target(scope.clone(), project_id.clone(), cx)
        }))
        .child(Icon::new(HeroIconName::Folder).size_4())
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .truncate()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(cx.theme().muted_foreground)
                        .child(subtitle),
                ),
        )
        .child(
            div()
                .flex_none()
                .rounded_full()
                .px(px(7.0))
                .py(px(2.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .bg(if active {
                    color(theme::ACCENT).opacity(0.16)
                } else {
                    cx.theme().muted
                })
                .child(target.count.to_string()),
        )
}

fn ai_memory_manager_tab_button(
    label: impl Into<String>,
    tab: MemoryManagerTab,
    active_tab: MemoryManagerTab,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let label = label.into();
    let active = tab == active_tab;
    let active_bg = ai_stats_track_surface(cx);
    div()
        .id(SharedString::from(format!(
            "ai-memory-manager-tab-{}",
            tab.as_str()
        )))
        .mr(px(8.0))
        .h(px(32.0))
        .px(px(14.0))
        .rounded(px(8.0))
        .flex()
        .items_center()
        .cursor_pointer()
        .text_size(rems(0.875))
        .line_height(rems(1.125))
        .text_color(if active {
            color(theme::TEXT)
        } else {
            color(theme::TEXT_MUTED)
        })
        .bg(if active {
            active_bg
        } else {
            cx.theme().transparent
        })
        .hover(move |style| style.bg(active_bg))
        .on_click(cx.listener(move |app, _event, _window, cx| app.set_memory_manager_tab(tab, cx)))
        .child(label)
}

fn ai_memory_header_icon_button(
    id: &'static str,
    icon: HeroIconName,
    tooltip: impl Into<String>,
    loading: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    with_codux_tooltip(
        cx.entity(),
        format!("ai-memory-header-tooltip-{id}"),
        Button::new(id)
            .compact()
            .ghost()
            .loading(loading)
            .text_color(cx.theme().secondary_foreground)
            .icon(
                Icon::new(icon)
                    .size_3p5()
                    .text_color(cx.theme().secondary_foreground),
            )
            .on_click(cx.listener(on_click)),
        tooltip.into(),
    )
}

fn ai_memory_section_label(label: String, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .mt(px(12.0))
        .mb(px(6.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(cx.theme().muted_foreground)
        .child(label)
}

fn ai_memory_migrate_project_button(
    manager: &MemoryManagerSnapshot,
    selected_project_id: Option<&str>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let tooltip = ai_sidebar_text(
        language,
        "memory.manager.migrate_project",
        "Rebind Project Memory",
    );
    let empty_label = ai_sidebar_text(
        language,
        "memory.manager.migrate_project.no_targets",
        "No migration targets",
    );
    let targets = manager
        .target_rows
        .iter()
        .filter(|target| {
            target.scope == "project"
                && target.is_open_project
                && target.project_id.is_some()
                && target.project_id.as_deref() != selected_project_id
        })
        .cloned()
        .collect::<Vec<_>>();
    let app_entity = cx.entity();

    with_codux_tooltip(
        cx.entity(),
        "ai-memory-migrate-project-memory-tooltip",
        Button::new("ai-memory-migrate-project-memory")
            .compact()
            .ghost()
            .text_color(cx.theme().secondary_foreground)
            .icon(
                Icon::new(HeroIconName::ArrowsRightLeft)
                    .size_3p5()
                    .text_color(cx.theme().secondary_foreground),
            )
            .dropdown_menu_with_anchor(gpui::Anchor::TopRight, move |menu, _window, _cx| {
                if targets.is_empty() {
                    return menu.item(
                        PopupMenuItem::new(empty_label.clone())
                            .icon(HeroIconName::Folder)
                            .disabled(true),
                    );
                }

                targets.iter().take(12).fold(menu, |menu, target| {
                    let Some(to_project_id) = target.project_id.clone() else {
                        return menu;
                    };
                    let title = target.title.clone();
                    let entity = app_entity.clone();
                    menu.item(
                        PopupMenuItem::new(title)
                            .icon(HeroIconName::Folder)
                            .on_click(move |_, window, cx| {
                                cx.update_entity(&entity, |app, cx| {
                                    app.migrate_selected_memory_project_to(
                                        to_project_id.clone(),
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    )
                })
            }),
        tooltip,
    )
}

fn ai_memory_project_profile_row(
    profile: codux_runtime::memory::MemoryProjectProfileSummary,
    refreshing: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let label = ai_sidebar_text(
        language,
        "memory.manager.project_profile",
        "Project Profile",
    );
    div()
        .mt(px(8.0))
        .rounded(px(8.0))
        .px(px(14.0))
        .py(px(12.0))
        .bg(ai_stats_surface(cx))
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .child(label),
                )
                .child(if refreshing {
                    ai_memory_refreshing_label(language).into_any_element()
                } else {
                    ai_memory_row_icon_button(
                        "ai-memory-refresh-project-profile",
                        HeroIconName::ArrowPath,
                        ai_sidebar_text(
                            language,
                            "memory.manager.project_profile.refresh",
                            "Regenerate Project Profile",
                        ),
                        cx,
                        |app, _event, window, cx| {
                            app.refresh_selected_memory_project_profile(window, cx)
                        },
                    )
                    .into_any_element()
                })
                .child(ai_memory_row_icon_button(
                    "ai-memory-delete-project-profile",
                    HeroIconName::Trash,
                    ai_sidebar_text(
                        language,
                        "memory.manager.project_profile.delete",
                        "Delete Project Profile",
                    ),
                    cx,
                    |app, _event, window, cx| {
                        app.delete_selected_memory_project_profile(window, cx)
                    },
                )),
        )
        .child(
            div()
                .mt(px(10.0))
                .text_size(rems(0.75))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .w_full()
                .child(profile.content),
        )
}

fn ai_memory_project_profile_empty_row(
    refreshing: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let label = ai_sidebar_text(
        language,
        "memory.manager.project_profile",
        "Project Profile",
    );
    let empty_label = ai_sidebar_text(
        language,
        "memory.manager.project_profile.empty",
        "No project profile exists",
    );
    div()
        .mt(px(8.0))
        .rounded(px(8.0))
        .px(px(14.0))
        .py(px(12.0))
        .bg(ai_stats_surface(cx))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .child(label),
                )
                .child(if refreshing {
                    ai_memory_refreshing_label(language).into_any_element()
                } else {
                    ai_memory_row_icon_button(
                        "ai-memory-create-project-profile",
                        HeroIconName::ArrowPath,
                        ai_sidebar_text(
                            language,
                            "memory.manager.project_profile.refresh",
                            "Regenerate Project Profile",
                        ),
                        cx,
                        |app, _event, window, cx| {
                            app.refresh_selected_memory_project_profile(window, cx)
                        },
                    )
                    .into_any_element()
                }),
        )
        .child(
            div()
                .mt(px(8.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0625))
                .text_color(color(theme::TEXT_MUTED))
                .child(empty_label),
        )
}

fn ai_memory_refreshing_label(language: &str) -> impl IntoElement {
    div()
        .px(px(7.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(color(theme::TEXT_DIM))
        .child(ai_sidebar_text(language, "common.processing", "Processing"))
}

fn ai_memory_manager_summary_row(
    summary: &codux_runtime::memory::MemorySummaryRow,
    selected_memory_summary_id: Option<&str>,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let summary_placeholder =
        ai_sidebar_text(language, "memory.manager.edit_summary.title", "Summary");
    let version_label = ai_sidebar_text(language, "memory.manager.summary.version_format", "v%lld")
        .replacen("%lld", &summary.version.to_string(), 1);
    let tokens_label = ai_sidebar_text(
        language,
        "memory.manager.summary.tokens_format",
        "%lld tokens",
    )
    .replacen("%lld", &summary.token_estimate.to_string(), 1);
    let summary_id = summary.id.clone();
    let save_id = summary.id.clone();
    let delete_id = summary.id.clone();
    let input_value = summary.content.clone();
    let input_state = window.use_keyed_state(
        SharedString::from(format!("ai-memory-summary-content-{}", summary.id)),
        cx,
        {
            let value = input_value.clone();
            move |window, cx| {
                InputState::new(window, cx)
                    .default_value(value.clone())
                    .placeholder(summary_placeholder.clone())
            }
        },
    );
    input_state.update(cx, |state, cx| {
        if state.value().as_ref() != input_value {
            state.set_value(input_value.clone(), window, cx);
        }
    });
    let save_state = input_state.clone();
    let active = selected_memory_summary_id
        .map(|id| id == summary.id.as_str())
        .unwrap_or(false);
    let active_bg = ai_stats_track_surface(cx);

    div()
        .id(SharedString::from(format!(
            "ai-memory-manager-summary-{}",
            summary.id
        )))
        .mb(px(6.0))
        .rounded(px(8.0))
        .px(px(12.0))
        .py(px(11.0))
        .cursor_pointer()
        .bg(if active {
            active_bg
        } else {
            ai_stats_surface(cx)
        })
        .hover(move |style| style.bg(active_bg))
        .on_click(cx.listener(move |app, _event, _window, cx| {
            app.selected_memory_summary_id = Some(summary_id.clone());
            app.status_message = format!("selected memory summary: {summary_id}");
            app.invalidate_memory_panel(cx);
        }))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .child(format!("{} {}", summary.scope, version_label)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .mr(px(4.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_DIM))
                                .child(tokens_label),
                        )
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-save-summary-{save_id}"),
                            HeroIconName::Check,
                            ai_sidebar_text(language, "common.save", "Save"),
                            cx,
                            move |app, _event, window, cx| {
                                let content = save_state.read(cx).value().to_string();
                                app.update_memory_summary_content(
                                    save_id.clone(),
                                    content,
                                    window,
                                    cx,
                                );
                            },
                        ))
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-delete-summary-{delete_id}"),
                            HeroIconName::Trash,
                            ai_sidebar_text(language, "common.delete", "Delete"),
                            cx,
                            move |app, _event, window, cx| {
                                app.selected_memory_summary_id = Some(delete_id.clone());
                                app.delete_selected_memory_summary(window, cx);
                            },
                        )),
                ),
        )
        .child(if active {
            div()
                .mt(px(8.0))
                .child(Input::new(&input_state).with_size(gpui_component::Size::Small))
                .into_any_element()
        } else {
            div()
                .mt(px(10.0))
                .text_size(rems(0.75))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .w_full()
                .child(summary.content.clone())
                .into_any_element()
        })
}

fn ai_memory_manager_entry_groups(
    entries: &[MemoryEntrySummary],
    selected_memory_entry_id: Option<&str>,
    active_tab: MemoryManagerTab,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> Vec<AnyElement> {
    let mut groups: BTreeMap<String, Vec<MemoryEntrySummary>> = BTreeMap::new();
    for entry in entries {
        groups
            .entry(memory_module_key(entry))
            .or_default()
            .push(entry.clone());
    }

    groups
        .into_iter()
        .map(|(module_key, group_entries)| {
            div()
                .mb(px(18.0))
                .child(
                    div()
                        .mb(px(8.0))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().size(px(8.0)).rounded_full().bg(color(theme::ACCENT)))
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_MUTED))
                                .child(memory_module_title(&module_key, language)),
                        )
                        .child(
                            div()
                                .rounded_full()
                                .px(px(7.0))
                                .py(px(1.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::ACCENT))
                                .bg(color(theme::ACCENT).opacity(0.12))
                                .child(group_entries.len().to_string()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(group_entries.into_iter().map(|entry| {
                            let active = selected_memory_entry_id
                                .map(|id| id == entry.id.as_str())
                                .unwrap_or(false);
                            ai_memory_manager_entry_row(entry, active, active_tab, language, cx)
                                .into_any_element()
                        })),
                )
                .into_any_element()
        })
        .collect()
}

fn ai_memory_manager_entry_row(
    entry: MemoryEntrySummary,
    active: bool,
    active_tab: MemoryManagerTab,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let select_id = entry.id.clone();
    let archive_id = entry.id.clone();
    let delete_id = entry.id.clone();
    let active_bg = ai_stats_track_surface(cx);
    let can_archive = active_tab == MemoryManagerTab::Active && entry.status == "active";

    div()
        .id(SharedString::from(format!(
            "ai-memory-manager-entry-{}",
            entry.id
        )))
        .rounded(px(8.0))
        .px(px(14.0))
        .py(px(12.0))
        .cursor_pointer()
        .bg(if active {
            active_bg
        } else {
            ai_stats_surface(cx)
        })
        .hover(move |style| style.bg(active_bg))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_memory_entry(select_id.clone(), window, cx)
        }))
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap_3()
                .child(ai_memory_entry_badges(&entry, language))
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .mr(px(4.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_DIM))
                                .child(memory_date_label(entry.updated_at)),
                        )
                        .when(can_archive, |this| {
                            this.child(ai_memory_row_icon_button(
                                format!("ai-memory-manager-archive-{archive_id}"),
                                HeroIconName::ArchiveBox,
                                ai_sidebar_text(language, "memory.manager.archive", "Archive"),
                                cx,
                                move |app, _event, window, cx| {
                                    app.selected_memory_entry_id = Some(archive_id.clone());
                                    app.archive_selected_memory_entry(window, cx);
                                },
                            ))
                        })
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-manager-delete-{delete_id}"),
                            HeroIconName::Trash,
                            ai_sidebar_text(language, "common.delete", "Delete"),
                            cx,
                            move |app, _event, window, cx| {
                                app.selected_memory_entry_id = Some(delete_id.clone());
                                app.delete_selected_memory_entry(window, cx);
                            },
                        )),
                ),
        )
        .child(
            div()
                .mt(px(10.0))
                .w_full()
                .text_size(rems(0.875))
                .line_height(rems(1.3125))
                .text_color(color(theme::TEXT))
                .child(entry.content.clone()),
        )
        .when_some(entry.rationale.clone(), |this, rationale| {
            this.child(
                div()
                    .mt(px(7.0))
                    .w_full()
                    .text_size(rems(0.75))
                    .line_height(rems(1.125))
                    .text_color(color(theme::TEXT_MUTED))
                    .child(rationale),
            )
        })
        .when_some(entry.last_decision.clone(), |this, decision| {
            this.child(ai_memory_decision_row(decision, language))
        })
}

fn ai_memory_failed_extraction_row(
    task: codux_runtime::memory::MemoryExtractionTask,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let retry_id = task.id.clone();
    let title = if task.session_id.trim().is_empty() {
        task.tool.clone()
    } else {
        format!("{} · {}", task.tool, task.session_id)
    };
    let subtitle = task
        .workspace_path
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| task.project_id.clone());
    let error = task
        .error
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            ai_sidebar_text(language, "memory.manager.failed.unknown", "Unknown error")
        });

    div()
        .id(SharedString::from(format!(
            "ai-memory-manager-failed-{}",
            task.id
        )))
        .mb(px(10.0))
        .rounded(px(8.0))
        .px(px(14.0))
        .py(px(12.0))
        .bg(ai_stats_surface(cx))
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .truncate()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .text_color(color(theme::TEXT))
                                .child(title),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .truncate()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_MUTED))
                                .child(subtitle),
                        ),
                )
                .child(ai_memory_row_icon_button(
                    format!("ai-memory-manager-retry-{retry_id}"),
                    HeroIconName::ArrowPath,
                    ai_sidebar_text(language, "memory.manager.failed.retry", "Retry"),
                    cx,
                    move |app, _event, window, cx| {
                        app.retry_failed_memory_extraction(retry_id.clone(), window, cx)
                    },
                )),
        )
        .child(
            div()
                .mt(px(10.0))
                .w_full()
                .text_size(rems(0.75))
                .line_height(rems(1.125))
                .text_color(color(0xF47C7C))
                .child(error),
        )
        .child(
            div()
                .mt(px(7.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_DIM))
                .child(memory_date_label(task.enqueued_at)),
        )
}

fn ai_memory_decision_row(
    decision: codux_runtime::memory::MemoryEntryDecisionSummary,
    language: &str,
) -> impl IntoElement {
    div()
        .mt(px(10.0))
        .flex()
        .items_center()
        .gap_2()
        .rounded(px(8.0))
        .px(px(10.0))
        .py(px(8.0))
        .bg(color(theme::TEXT).opacity(0.045))
        .child(ai_memory_badge(
            memory_decision_title(&decision.kind, language),
            memory_decision_color(&decision.kind),
        ))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(decision.reason),
        )
}

fn ai_memory_entry_badges(entry: &MemoryEntrySummary, language: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(ai_memory_badge(
            memory_kind_title(&entry.kind, language),
            memory_kind_color(&entry.kind),
        ))
        .child(ai_memory_badge(
            memory_module_title(&memory_module_key(entry), language),
            color(theme::ACCENT),
        ))
        .child(ai_memory_badge(
            memory_tier_title(&entry.tier, language),
            memory_tier_color(&entry.tier),
        ))
        .child(ai_memory_badge(
            memory_status_title(&entry.status, language),
            memory_status_color(&entry.status),
        ))
        .when_some(entry.source_tool.clone(), |this, source_tool| {
            this.child(ai_memory_badge(source_tool, color(0x7B8190)))
        })
}

fn ai_memory_badge(label: String, badge_color: Hsla) -> impl IntoElement {
    div()
        .rounded_full()
        .px(px(9.0))
        .py(px(3.0))
        .text_size(rems(0.75))
        .line_height(rems(0.9375))
        .text_color(badge_color)
        .bg(badge_color.opacity(0.14))
        .child(label)
}

fn ai_memory_manager_empty_row(
    message: impl Into<String>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    centered_empty_state(HeroIconName::Inbox, message, cx)
}

fn ai_memory_row_icon_button(
    id: impl Into<SharedString>,
    icon: HeroIconName,
    tooltip: impl Into<String>,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let tooltip = tooltip.into();
    let id = id.into();
    with_codux_tooltip(
        cx.entity(),
        SharedString::from(format!("ai-memory-row-tooltip-{id}")),
        Button::new(id)
            .compact()
            .ghost()
            .text_color(cx.theme().secondary_foreground)
            .icon(
                Icon::new(icon)
                    .size_3p5()
                    .text_color(cx.theme().secondary_foreground),
            )
            .on_click(cx.listener(on_click)),
        tooltip,
    )
}

fn memory_module_key(entry: &MemoryEntrySummary) -> String {
    let module = entry.module_key.trim();
    if module.is_empty() {
        "general".to_string()
    } else {
        module.to_string()
    }
}

fn memory_kind_title(kind: &str, language: &str) -> String {
    let fallback = match kind {
        "preference" => "Preference",
        "convention" => "Convention",
        "decision" => "Decision",
        "fact" => "Fact",
        "bug_lesson" => "Bug Lesson",
        _ => kind,
    };
    ai_sidebar_text(language, &format!("memory.kind.{kind}"), fallback)
}

fn memory_module_title(module_key: &str, language: &str) -> String {
    let fallback = match module_key {
        "general" => "General",
        "project" => "Project",
        "terminal" => "Terminal",
        "git" => "Git",
        "ui" => "UI",
        "runtime" => "Runtime",
        _ => module_key,
    };
    ai_sidebar_text(language, &format!("memory.module.{module_key}"), fallback)
}

fn memory_tier_title(tier: &str, language: &str) -> String {
    let fallback = match tier {
        "core" => "Core",
        "working" => "Working",
        "archive" => "Archive",
        _ => tier,
    };
    ai_sidebar_text(language, &format!("memory.tier.{tier}"), fallback)
}

fn memory_status_title(status: &str, language: &str) -> String {
    let fallback = match status {
        "active" => "Active",
        "merged" => "Merged",
        "archived" => "Archived",
        _ => status,
    };
    ai_sidebar_text(language, &format!("memory.status.{status}"), fallback)
}

fn memory_decision_title(decision: &str, language: &str) -> String {
    let fallback = match decision {
        "create" => "Created",
        "merge" => "Merged",
        "replace" => "Replaced",
        "archive" => "Archived",
        "skip" => "Skipped",
        _ => decision,
    };
    ai_sidebar_text(language, &format!("memory.decision.{decision}"), fallback)
}

fn memory_kind_color(kind: &str) -> Hsla {
    color(match kind {
        "preference" => 0x8C6FF7,
        "convention" => 0x2F7FBD,
        "decision" => 0xB8781D,
        "fact" => 0x337A6B,
        "bug_lesson" => 0xC25555,
        _ => 0x7B8190,
    })
}

fn memory_tier_color(tier: &str) -> Hsla {
    color(match tier {
        "core" => 0x3D80FA,
        "working" => 0x2E9B5F,
        "archive" => 0x7B8190,
        _ => 0x7B8190,
    })
}

fn memory_status_color(status: &str) -> Hsla {
    color(match status {
        "active" => 0x2E9B5F,
        "merged" => 0x6E6E8B,
        "archived" => 0x7B8190,
        _ => 0x7B8190,
    })
}

fn memory_decision_color(decision: &str) -> Hsla {
    color(match decision {
        "create" => 0x2E9B5F,
        "merge" => 0x3D80FA,
        "replace" => 0xB8781D,
        "archive" => 0x7B8190,
        "skip" => 0xC25555,
        _ => 0x7B8190,
    })
}

fn memory_date_label(seconds: f64) -> String {
    chrono::Local
        .timestamp_opt(seconds as i64, 0)
        .single()
        .map(|date| date.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

fn ai_metric_card(
    label: impl Into<String>,
    value: String,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let label = label.into();
    div()
        .flex_1()
        .min_h(px(72.0))
        .rounded(px(8.0))
        .bg(ai_stats_surface(cx))
        .p(px(12.0))
        .flex()
        .flex_col()
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT_MUTED))
                .child(label),
        )
        .child(
            div()
                .mt(px(10.0))
                .text_size(rems(1.125))
                .line_height(rems(1.375))
                .text_color(color(theme::TEXT))
                .child(value),
        )
}

fn ai_today_usage_chart(
    history: &AIHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    indexed_baselines: &BTreeMap<String, (i64, i64)>,
    include_cached: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title = ai_sidebar_text(language, "ai.today_usage", "Today's Usage");
    let usage_labels = AIUsageLabels::load(language);
    let values = ai_today_bucket_values(
        history,
        live_sessions,
        indexed_baselines,
        include_cached,
        &usage_labels,
    );
    let max_value = values
        .iter()
        .map(|bucket| bucket.value)
        .max()
        .unwrap_or(0)
        .max(1);

    ai_stats_card(title, cx)
        .min_h(px(134.0))
        .child(
            div()
                .mt(px(12.0))
                .flex()
                .items_end()
                .justify_center()
                .h(px(62.0))
                .children(values.into_iter().enumerate().map(|(index, bucket)| {
                    let ratio = bucket.value as f32 / max_value as f32;
                    codux_tooltip_container(
                        cx.entity(),
                        SharedString::from(format!("ai-today-usage-{index}")),
                        bucket.tooltip,
                    )
                    .flex_1()
                    .min_w(px(2.0))
                    .ml(if index == 0 { px(0.0) } else { px(1.0) })
                    .h(px(10.0 + ratio * 56.0))
                    .rounded(px(3.0))
                    .bg(color(theme::ACCENT))
                    .opacity(if bucket.value > 0 { 1.0 } else { 0.35 })
                    .into_any_element()
                })),
        )
        .child(
            div()
                .mt(px(10.0))
                .h(px(1.0))
                .bg(color(theme::ACCENT).opacity(0.26)),
        )
        .child(
            div()
                .mt(px(10.0))
                .flex()
                .justify_between()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child("00:00")
                .child("06:00")
                .child("12:00")
                .child("18:00")
                .child("23:59"),
        )
}

fn ai_recent_usage_heatmap(
    history: &AIHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    indexed_baselines: &BTreeMap<String, (i64, i64)>,
    include_cached: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title = ai_sidebar_text(language, "ai.recent_usage", "Recent Usage");
    let usage_labels = AIUsageLabels::load(language);
    let values = ai_recent_heatmap_values(
        history,
        live_sessions,
        indexed_baselines,
        include_cached,
        &usage_labels,
    );
    let mut non_zero = values
        .iter()
        .filter_map(|cell| (cell.value > 0).then_some(cell.value))
        .collect::<Vec<_>>();
    non_zero.sort_unstable();
    let inactive_surface = ai_stats_track_surface(cx);
    let grid_height = 7.0 * AI_RECENT_USAGE_CELL_SIZE + 6.0 * AI_RECENT_USAGE_GAP;
    let grid_width = AI_RECENT_USAGE_COLUMNS as f32 * AI_RECENT_USAGE_CELL_SIZE
        + (AI_RECENT_USAGE_COLUMNS - 1) as f32 * AI_RECENT_USAGE_GAP;
    let app_entity = cx.entity();

    ai_stats_card(title, cx).p(px(20.0)).child(
        div().mt(px(14.0)).w_full().flex().justify_center().child(
            div()
                .flex()
                .gap(px(AI_RECENT_USAGE_GAP))
                .w(px(grid_width))
                .h(px(grid_height))
                .children(values.chunks(7).enumerate().map(|(column, days)| {
                    let non_zero = non_zero.clone();
                    let app_entity = app_entity.clone();
                    div()
                        .flex()
                        .w(px(AI_RECENT_USAGE_CELL_SIZE))
                        .flex_col()
                        .gap(px(AI_RECENT_USAGE_GAP))
                        .children(days.iter().cloned().enumerate().map(move |(row, cell)| {
                            let opacity = ai_usage_heatmap_opacity(cell.value, &non_zero);
                            codux_tooltip_container(
                                app_entity.clone(),
                                SharedString::from(format!("ai-recent-usage-{column}-{row}")),
                                cell.tooltip,
                            )
                            .size(px(AI_RECENT_USAGE_CELL_SIZE))
                            .rounded(px(3.0))
                            .bg(if cell.is_known {
                                color(theme::ACCENT)
                            } else {
                                inactive_surface
                            })
                            .opacity(if cell.is_known { opacity } else { 1.0 })
                            .into_any_element()
                        }))
                        .into_any_element()
                })),
        ),
    )
}

fn ai_ranking_card(
    title: impl Into<String>,
    rows: Vec<(String, i64, f32)>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let track_surface = ai_stats_track_surface(cx);
    let empty_label = ai_sidebar_text(language, "ai.empty.no_stats", "No AI Stats Yet");
    ai_stats_card(title, cx).child(if rows.is_empty() {
        div()
            .mt(px(12.0))
            .text_size(rems(0.75))
            .line_height(rems(1.0))
            .text_color(color(theme::TEXT_DIM))
            .child(empty_label)
            .into_any_element()
    } else {
        div()
            .mt(px(12.0))
            .flex()
            .flex_col()
            .children(rows.into_iter().map(|(label, value, percent)| {
                ai_ranking_row(cx.entity(), label, value, percent, track_surface).into_any_element()
            }))
            .into_any_element()
    })
}

fn ai_ranking_row(
    app_entity: gpui::Entity<CoduxApp>,
    label: String,
    value: i64,
    percent: f32,
    track_surface: gpui::Hsla,
) -> impl IntoElement {
    let value_label = compact_number(value);
    let tooltip = format!("{label} · {value_label} tokens");
    codux_tooltip_container(
        app_entity,
        SharedString::from(format!("ai-ranking-row-{label}")),
        tooltip,
    )
    .mb(px(10.0))
    .child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(rems(0.875))
                    .line_height(rems(1.25))
                    .text_color(color(theme::TEXT))
                    .truncate()
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .w(px(78.0))
                            .text_right()
                            .text_size(rems(0.875))
                            .line_height(rems(1.25))
                            .text_color(color(theme::TEXT_MUTED))
                            .child(value_label),
                    )
                    .child(
                        div()
                            .w(px(34.0))
                            .text_right()
                            .text_size(rems(0.75))
                            .line_height(rems(1.25))
                            .text_color(color(theme::TEXT_DIM))
                            .child(format!(
                                "{}%",
                                (percent.clamp(0.0, 1.0) * 100.0).round() as i64
                            )),
                    ),
            ),
    )
    .child(
        div()
            .mt(px(6.0))
            .h(px(4.0))
            .w_full()
            .rounded(px(4.0))
            .bg(track_surface)
            .child(
                div()
                    .h_full()
                    .w(gpui::relative(percent.clamp(0.0, 1.0)))
                    .rounded(px(4.0))
                    .bg(color(theme::ACCENT))
                    .opacity(if value > 0 { 1.0 } else { 0.35 }),
            ),
    )
}

fn ai_history_sessions(history: &AIHistorySummary) -> Vec<&AISessionSummary> {
    history.sessions.iter().collect()
}

fn ai_now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

fn ai_local_day_start_seconds(timestamp: f64) -> f64 {
    codux_runtime::ai_history_normalized::local_day_start_seconds(timestamp)
}

fn ai_usage_tooltip(
    label: String,
    tokens: i64,
    request_count: i64,
    labels: &AIUsageLabels,
) -> String {
    let requests = if request_count > 0 {
        format!(
            " · {}",
            labels
                .request_count_format
                .replace("%d", &request_count.to_string())
        )
    } else {
        String::new()
    };
    format!(
        "{label} · {} {}{requests}",
        compact_number(tokens.max(0)),
        labels.tokens
    )
}

fn ai_usage_heatmap_opacity(value: i64, non_zero: &[i64]) -> f32 {
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

fn ai_time_label(timestamp: f64) -> String {
    chrono::Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .map(|date| format!("{:02}:{:02}", date.hour(), date.minute()))
        .unwrap_or_else(|| "00:00".to_string())
}

fn ai_time_label_with_seconds(timestamp: f64) -> String {
    chrono::Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .map(|date| {
            format!(
                "{:02}:{:02}:{:02}",
                date.hour(),
                date.minute(),
                date.second()
            )
        })
        .unwrap_or_else(|| "23:59:59".to_string())
}

fn ai_date_label(timestamp: f64, labels: &AIUsageLabels) -> String {
    chrono::Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .map(|date| {
            format!(
                "{}/{} {}",
                date.month(),
                date.day(),
                ai_weekday_label(date.weekday(), labels)
            )
        })
        .unwrap_or_else(|| labels.unknown_date.clone())
}

fn ai_weekday_label(weekday: chrono::Weekday, labels: &AIUsageLabels) -> &'static str {
    match weekday {
        chrono::Weekday::Mon => labels.weekdays[0],
        chrono::Weekday::Tue => labels.weekdays[1],
        chrono::Weekday::Wed => labels.weekdays[2],
        chrono::Weekday::Thu => labels.weekdays[3],
        chrono::Weekday::Fri => labels.weekdays[4],
        chrono::Weekday::Sat => labels.weekdays[5],
        chrono::Weekday::Sun => labels.weekdays[6],
    }
}

fn ai_today_bucket_values(
    history: &AIHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    indexed_baselines: &BTreeMap<String, (i64, i64)>,
    include_cached: bool,
    labels: &AIUsageLabels,
) -> Vec<AITodayUsageBucket> {
    let mut buckets = (0..48)
        .map(|_| AITodayUsageBucket {
            value: 0,
            request_count: 0,
            tooltip: String::new(),
        })
        .collect::<Vec<_>>();
    let mut has_indexed_buckets = false;
    let now = ai_now_seconds();
    let day_start = ai_local_day_start_seconds(now);
    if !history.today_time_buckets.is_empty() {
        for bucket in &history.today_time_buckets {
            if ai_local_day_start_seconds(bucket.start) != day_start {
                continue;
            }
            let index = (((bucket.start - day_start) / 86_400.0) * buckets.len() as f64)
                .floor()
                .clamp(0.0, (buckets.len() - 1) as f64) as usize;
            buckets[index].value += ai_display_tokens(
                bucket.total_tokens,
                bucket.cached_input_tokens,
                include_cached,
            );
            buckets[index].request_count += bucket.request_count.max(0);
        }
        has_indexed_buckets = buckets.iter().any(|bucket| bucket.value > 0);
    }

    if !has_indexed_buckets {
        for session in ai_history_sessions(history) {
            if session.last_seen_at < day_start {
                continue;
            }
            let bucket = (((session.last_seen_at - day_start) / 86_400.0) * buckets.len() as f64)
                .floor()
                .clamp(0.0, (buckets.len() - 1) as f64) as usize;
            buckets[bucket].value += ai_display_tokens(
                session.total_tokens,
                session.cached_input_tokens,
                include_cached,
            );
            buckets[bucket].request_count += session.request_count.max(0);
        }
    }

    for session in live_sessions {
        if !ai_live_session_counts_for_day(session, day_start) {
            continue;
        }
        let bucket = (((session.updated_at - day_start) / 86_400.0) * buckets.len() as f64)
            .floor()
            .clamp(0.0, (buckets.len() - 1) as f64) as usize;
        buckets[bucket].value += ai_live_session_delta_tokens(
            session,
            indexed_baselines,
            include_cached,
            Some(day_start),
        );
    }

    for (index, bucket) in buckets.iter_mut().enumerate() {
        let start = day_start + index as f64 * 1800.0;
        let end = if index == 47 {
            day_start + 86_399.0
        } else {
            start + 1800.0
        };
        bucket.tooltip = ai_usage_tooltip(
            format!(
                "{} - {}",
                ai_time_label(start),
                if index == 47 {
                    ai_time_label_with_seconds(end)
                } else {
                    ai_time_label(end)
                }
            ),
            bucket.value,
            bucket.request_count,
            labels,
        );
    }

    buckets
}

fn ai_recent_heatmap_values(
    history: &AIHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    indexed_baselines: &BTreeMap<String, (i64, i64)>,
    include_cached: bool,
    labels: &AIUsageLabels,
) -> Vec<AIUsageCell> {
    let now = ai_now_seconds();
    let today = ai_local_day_start_seconds(now);
    let first_day = today - (AI_RECENT_USAGE_COLUMNS * 7 - 1) as f64 * 86_400.0;
    let mut values = (0..AI_RECENT_USAGE_COLUMNS)
        .flat_map(|column| {
            (0..7).map(move |row| {
                let day = first_day + (column * 7 + row) as f64 * 86_400.0;
                AIUsageCell {
                    value: 0,
                    request_count: 0,
                    is_known: false,
                    tooltip: ai_usage_tooltip(ai_date_label(day, labels), 0, 0, labels),
                }
            })
        })
        .collect::<Vec<_>>();
    let mut has_indexed_heatmap = false;
    if !history.heatmap.is_empty() {
        for day in &history.heatmap {
            let day_start = ai_local_day_start_seconds(day.day);
            let day_offset = ((today - day_start) / 86_400.0).round() as isize;
            if (0..values.len() as isize).contains(&day_offset) {
                let index = values.len() - 1 - day_offset as usize;
                values[index].value +=
                    ai_display_tokens(day.total_tokens, day.cached_input_tokens, include_cached);
                values[index].request_count += day.request_count.max(0);
                values[index].is_known = true;
            }
        }
        has_indexed_heatmap = values.iter().any(|cell| cell.value > 0);
    }

    if !has_indexed_heatmap {
        for session in ai_history_sessions(history) {
            let session_day = ai_local_day_start_seconds(session.last_seen_at);
            let day_offset = ((today - session_day) / 86_400.0).round() as isize;
            if (0..values.len() as isize).contains(&day_offset) {
                let index = values.len() - 1 - day_offset as usize;
                values[index].value += ai_display_tokens(
                    session.total_tokens,
                    session.cached_input_tokens,
                    include_cached,
                );
                values[index].request_count += session.request_count.max(0);
                values[index].is_known = true;
            }
        }
    }

    for session in live_sessions {
        let session_day = ai_local_day_start_seconds(session.updated_at);
        let day_offset = ((today - session_day) / 86_400.0).round() as isize;
        if (0..values.len() as isize).contains(&day_offset) {
            let index = values.len() - 1 - day_offset as usize;
            values[index].value +=
                ai_live_session_delta_tokens(session, indexed_baselines, include_cached, None);
            values[index].request_count += 1;
            values[index].is_known = true;
        }
    }

    for (index, cell) in values.iter_mut().enumerate() {
        let day = first_day + index as f64 * 86_400.0;
        cell.tooltip = ai_usage_tooltip(
            ai_date_label(day, labels),
            cell.value,
            cell.request_count,
            labels,
        );
    }

    values
}

fn ai_tool_rows(
    history: &AIHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    indexed_baselines: &BTreeMap<String, (i64, i64)>,
    include_cached: bool,
) -> Vec<(String, i64, f32)> {
    if !history.tool_breakdown.is_empty() {
        return ai_rank_rows(
            history
                .tool_breakdown
                .iter()
                .map(|item| {
                    (
                        item.key.clone(),
                        ai_display_tokens(
                            item.total_tokens,
                            item.cached_input_tokens,
                            include_cached,
                        ),
                    )
                })
                .chain(live_sessions.iter().map(|session| {
                    (
                        session.tool.clone(),
                        ai_live_session_delta_tokens(
                            session,
                            indexed_baselines,
                            include_cached,
                            None,
                        ),
                    )
                })),
        );
    }

    ai_rank_rows(
        ai_history_sessions(history)
            .into_iter()
            .map(|session| {
                (
                    session.source.clone(),
                    ai_display_tokens(
                        session.total_tokens,
                        session.cached_input_tokens,
                        include_cached,
                    ),
                )
            })
            .chain(live_sessions.iter().map(|session| {
                (
                    session.tool.clone(),
                    ai_live_session_delta_tokens(session, indexed_baselines, include_cached, None),
                )
            })),
    )
}

fn ai_model_rows(
    history: &AIHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    indexed_baselines: &BTreeMap<String, (i64, i64)>,
    include_cached: bool,
) -> Vec<(String, i64, f32)> {
    if !history.model_breakdown.is_empty() {
        return ai_rank_rows(
            history
                .model_breakdown
                .iter()
                .map(|item| {
                    (
                        item.key.clone(),
                        ai_display_tokens(
                            item.total_tokens,
                            item.cached_input_tokens,
                            include_cached,
                        ),
                    )
                })
                .chain(live_sessions.iter().filter_map(|session| {
                    session.model.clone().map(|model| {
                        (
                            model,
                            ai_live_session_delta_tokens(
                                session,
                                indexed_baselines,
                                include_cached,
                                None,
                            ),
                        )
                    })
                })),
        );
    }

    ai_rank_rows(
        ai_history_sessions(history)
            .into_iter()
            .filter_map(|session| {
                session.last_model.clone().map(|model| {
                    (
                        model,
                        ai_display_tokens(
                            session.total_tokens,
                            session.cached_input_tokens,
                            include_cached,
                        ),
                    )
                })
            })
            .chain(live_sessions.iter().filter_map(|session| {
                session.model.clone().map(|model| {
                    (
                        model,
                        ai_live_session_delta_tokens(
                            session,
                            indexed_baselines,
                            include_cached,
                            None,
                        ),
                    )
                })
            })),
    )
}

fn ai_rank_rows(rows: impl Iterator<Item = (String, i64)>) -> Vec<(String, i64, f32)> {
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
        .map(|(label, value)| (label, value, value as f32 / max_value as f32))
        .collect()
}

fn ai_sessions_panel(
    history: &AIHistorySummary,
    selected_detail: Option<&AISessionDetail>,
    _selected_session_id: Option<&str>,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let indexed_label = ai_sidebar_text(language, "ai.indexing.status.completed", "Index complete");
    let unindexed_label = ai_sidebar_text(language, "ai.empty.no_stats", "No AI Stats Yet");
    div()
        .flex()
        .min_h_0()
        .flex_col()
        .child(
            div()
                .h(px(38.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .justify_between()
                .bg(cx.theme().list_head)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .text_color(color(theme::TEXT))
                                .child(ai_sidebar_text(
                                    language,
                                    "ai.sessions.history",
                                    "Session History",
                                )),
                        )
                        .child(
                            div()
                                .ml(px(8.0))
                                .px(px(7.0))
                                .h(px(20.0))
                                .rounded(px(10.0))
                                .flex()
                                .items_center()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_MUTED))
                                .bg(ai_stats_track_surface(cx))
                                .child(history.session_count.to_string()),
                        ),
                )
                .child(
                    div()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_DIM))
                        .child(if history.indexed {
                            indexed_label
                        } else {
                            unindexed_label
                        }),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h(px(160.0))
                .overflow_y_scrollbar()
                .p(px(8.0))
                .child(if history.sessions.is_empty() {
                    ai_empty_sessions(history, language, cx).into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .children(history.sessions.iter().take(12).cloned().map(|session| {
                            ai_session_list_row(session, language, cx).into_any_element()
                        }))
                        .into_any_element()
                }),
        )
        .child(if let Some(detail) = selected_detail {
            ai_session_detail_summary(detail, language, window, cx).into_any_element()
        } else {
            div().into_any_element()
        })
}

fn ai_empty_sessions(
    history: &AIHistorySummary,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let message = history
        .error
        .as_ref()
        .map(|error| {
            ai_sidebar_text(language, "ai.indexing.status.failed", "Index failed") + ": " + error
        })
        .unwrap_or_else(|| {
            ai_sidebar_text(
                language,
                "ai.sessions.empty_project",
                "No AI sessions have been indexed for this project yet.",
            )
        });
    div()
        .h(px(92.0))
        .rounded(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .px(px(12.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(color(theme::TEXT_DIM))
        .bg(cx.theme().group_box)
        .child(message)
}

fn ai_session_list_row(
    session: AISessionSummary,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let session_id = session.id.clone();
    let active_bg = ai_stats_track_surface(cx);
    let usage_labels = AIUsageLabels::load(language);
    div()
        .id(SharedString::from(format!(
            "assistant-ai-session-{}",
            session.id
        )))
        .mb(px(6.0))
        .rounded(px(8.0))
        .px(px(8.0))
        .py(px(7.0))
        .cursor_pointer()
        .bg(cx.theme().transparent)
        .hover(move |style| style.bg(active_bg))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_ai_session(session_id.clone(), window, cx)
        }))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(session.title),
                )
                .child(
                    div()
                        .ml(px(10.0))
                        .flex_shrink_0()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_DIM))
                        .child(relative_time_label_for_language(
                            session.last_seen_at,
                            language,
                        )),
                ),
        )
        .child(
            div()
                .mt(px(2.0))
                .flex()
                .items_center()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(div().min_w_0().flex_1().truncate().child(format!(
                    "{} · {} · {}",
                    session.source,
                    usage_labels
                        .request_format
                        .replace("%d", &session.request_count.to_string()),
                    compact_number(session.total_tokens)
                )))
                .child(
                    div()
                        .ml(px(8.0))
                        .flex_shrink_0()
                        .text_color(color(theme::TEXT_DIM))
                        .child(session.last_model.unwrap_or_else(|| "unknown".to_string())),
                ),
        )
}

fn ai_session_detail_summary(
    detail: &AISessionDetail,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title_placeholder =
        ai_sidebar_text(language, "ai.session.rename.placeholder", "Session title");
    let usage_labels = AIUsageLabels::load(language);
    let restore_id = detail.id.clone();
    let remove_id = detail.id.clone();
    let title_state = window.use_keyed_state(
        SharedString::from(format!("ai-session-title-{}", detail.id)),
        cx,
        {
            let title = detail.title.clone();
            move |window, cx| {
                InputState::new(window, cx)
                    .default_value(title.clone())
                    .placeholder(title_placeholder.clone())
            }
        },
    );
    let save_state = title_state.clone();
    div()
        .flex_shrink_0()
        .border_t_1()
        .border_color(color(theme::BORDER_SOFT))
        .p(px(10.0))
        .bg(ai_stats_surface(cx))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(Input::new(&title_state).with_size(gpui_component::Size::Small)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(assistant_header_icon_button(
                            "assistant-ai-rename-session",
                            HeroIconName::Check,
                            cx,
                            move |app, _event, window, cx| {
                                let title = save_state.read(cx).value().to_string();
                                app.rename_selected_ai_session_to(title, window, cx);
                            },
                        ))
                        .child(assistant_header_icon_button(
                            "assistant-ai-restore-session",
                            HeroIconName::CommandLine,
                            cx,
                            move |app, _event, window, cx| {
                                app.selected_ai_session_id = Some(restore_id.clone());
                                app.restore_selected_ai_session(window, cx);
                            },
                        ))
                        .child(assistant_header_icon_button(
                            "assistant-ai-remove-session",
                            HeroIconName::Trash,
                            cx,
                            move |app, _event, window, cx| {
                                app.selected_ai_session_id = Some(remove_id.clone());
                                app.remove_selected_ai_session(window, cx);
                            },
                        )),
                ),
        )
        .child(
            div()
                .mt(px(5.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .truncate()
                .child(format!(
                    "{} · {} · {} · cached {}",
                    detail.source,
                    usage_labels
                        .request_format
                        .replace("%d", &detail.request_count.to_string()),
                    compact_number(detail.total_tokens),
                    compact_number(detail.cached_input_tokens)
                )),
        )
        .child(
            div()
                .mt(px(8.0))
                .flex()
                .flex_wrap()
                .children(detail.files.iter().take(4).map(|file| {
                    div()
                        .mr(px(6.0))
                        .mb(px(6.0))
                        .px(px(7.0))
                        .h(px(22.0))
                        .rounded(px(6.0))
                        .flex()
                        .items_center()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .bg(ai_stats_track_surface(cx))
                        .child(file.file_path.clone())
                        .into_any_element()
                })),
        )
}
