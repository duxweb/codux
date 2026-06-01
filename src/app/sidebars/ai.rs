use super::{formatting::relative_time_label_for_language, *};
use chrono::{Datelike as _, TimeZone as _, Timelike as _};
use codux_runtime::{
    ai_runtime_state::AIRuntimeStateSummary,
    i18n::translate,
    runtime_paths::{LIVE_LOG_FILE_NAME, RUNTIME_LOG_FILE_NAME},
    settings::locale_from_language_setting,
};
use gpui_component::input::{Input, InputState};

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
    global: &AIGlobalHistorySummary,
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
    let tool_rows = ai_tool_rows(
        history,
        global,
        &live_sessions,
        &indexed_baselines,
        include_cached,
    );
    let model_rows = ai_model_rows(
        history,
        global,
        &live_sessions,
        &indexed_baselines,
        include_cached,
    );

    div()
        .flex()
        .flex_1()
        .h_full()
        .min_h_0()
        .flex_col()
        .child(assistant_panel_header(
            title,
            IconName::Bot,
            header_icon_button(
                "ai-stats-refresh",
                IconName::Redo2,
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
                    global,
                    history,
                    &live_sessions,
                    &indexed_baselines,
                    include_cached,
                    language,
                    cx,
                )))
                .child(div().mt(px(12.0)).child(ai_recent_usage_heatmap(
                    global,
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

fn ai_display_tokens(total_tokens: i64, cached_input_tokens: i64, include_cached: bool) -> i64 {
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
                .text_size(px(14.0))
                .line_height(px(18.0))
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
            .text_size(px(12.0))
            .line_height(px(16.0))
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
                        .text_size(px(14.0))
                        .line_height(px(18.0))
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
                        .text_size(px(12.0))
                        .line_height(px(16.0))
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
                        .text_size(px(16.0))
                        .line_height(px(18.0))
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
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(session_total_label),
                ),
        )
}

fn ai_runtime_sessions_card(
    runtime_events: &RuntimeEventSummary,
    ai_runtime_state: &AIRuntimeStateSummary,
    selected_session: Option<&RuntimeSessionSummary>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let selected_terminal_id = selected_session.map(|session| session.terminal_id.as_str());
    let running_label = ai_sidebar_text(language, "agent.status.running", "Running");
    let waiting_label = ai_sidebar_text(language, "task_memo.status.waiting", "Waiting");
    let completed_label = ai_sidebar_text(language, "task_memo.status.completed", "Completed");
    let sessions_label = ai_sidebar_text(language, "ai.live_sessions", "AI Sessions");
    let empty_label = ai_sidebar_text(language, "ai.live_sessions.empty", "No running AI sessions");
    let sessions = if runtime_events.sessions.is_empty() {
        div()
            .mt(px(10.0))
            .h(px(46.0))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.0))
            .line_height(px(16.0))
            .text_color(color(theme::TEXT_DIM))
            .bg(ai_stats_track_surface(cx))
            .child(
                runtime_events
                    .error
                    .clone()
                    .unwrap_or_else(|| empty_label.clone()),
            )
            .into_any_element()
    } else {
        div()
            .mt(px(10.0))
            .flex()
            .flex_col()
            .children(
                runtime_events
                    .sessions
                    .iter()
                    .take(5)
                    .cloned()
                    .map(|session| {
                        let active = selected_terminal_id
                            .map(|id| id == session.terminal_id.as_str())
                            .unwrap_or(false);
                        ai_runtime_session_row(session, active, language, cx).into_any_element()
                    }),
            )
            .into_any_element()
    };

    let card = ai_stats_card(sessions_label.clone(), cx)
        .child(
            div()
                .mt(px(10.0))
                .grid()
                .grid_cols(3)
                .child(ai_runtime_metric(
                    running_label.clone(),
                    runtime_events.running_count.to_string(),
                    theme::GREEN,
                    cx,
                ))
                .child(ai_runtime_metric(
                    waiting_label.clone(),
                    runtime_events.needs_input_count.to_string(),
                    theme::ORANGE,
                    cx,
                ))
                .child(ai_runtime_metric(
                    completed_label.clone(),
                    runtime_events.completed_count.to_string(),
                    theme::TEXT_DIM,
                    cx,
                )),
        )
        .child(
            div()
                .mt(px(10.0))
                .rounded(px(7.0))
                .bg(ai_stats_track_surface(cx))
                .px(px(8.0))
                .py(px(7.0))
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
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT))
                                .truncate()
                                .child("Supervisor"),
                        )
                        .child(assistant_header_icon_button(
                            "ai-runtime-poll",
                            IconName::Redo2,
                            cx,
                            |app, _event, window, cx| app.poll_ai_runtime_state(window, cx),
                        ))
                        .child(if ai_runtime_state.completed_count > 0 {
                            assistant_header_icon_button(
                                "ai-runtime-dismiss-completion",
                                IconName::Close,
                                cx,
                                |app, _event, window, cx| {
                                    app.dismiss_selected_project_ai_completion(window, cx)
                                },
                            )
                            .into_any_element()
                        } else {
                            div().into_any_element()
                        }),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .truncate()
                        .child(format!(
                            "{} {} · {} {} · {} {} · {} {}",
                            running_label,
                            ai_runtime_state.running_count,
                            waiting_label,
                            ai_runtime_state.needs_input_count,
                            completed_label,
                            ai_runtime_state.completed_count,
                            sessions_label,
                            ai_runtime_state.session_count
                        )),
                ),
        )
        .child(sessions);

    if let Some(session) = selected_session.cloned() {
        card.child(
            div()
                .mt(px(8.0))
                .rounded(px(7.0))
                .bg(ai_stats_track_surface(cx))
                .px(px(8.0))
                .py(px(7.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(session.session_title),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_DIM))
                        .truncate()
                        .child(format!(
                            "{} · {} · {} events",
                            session.tool, session.terminal_id, session.event_count
                        )),
                ),
        )
    } else {
        card
    }
}

fn ai_runtime_infrastructure_card(
    runtime_activity: &RuntimeActivitySummary,
    runtime_ingress: &RuntimeIngressStatus,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let listening_label = ai_sidebar_text(language, "status.connected", "Connected");
    let disconnected_label = ai_sidebar_text(language, "status.disconnected", "Disconnected");
    let events_label = ai_sidebar_text(language, "ai.runtime.events", "Events");
    let processes_label = ai_sidebar_text(language, "ai.runtime.processes", "Processes");
    let runtime_title = ai_sidebar_text(language, "ai.runtime.ingress", "Runtime Ingress");
    let log_missing_label = ai_sidebar_text(language, "ai.runtime.log_missing", "not created");
    let ingress_accent = if runtime_ingress.started {
        theme::GREEN
    } else {
        theme::ORANGE
    };
    let ingress_label = if runtime_ingress.started {
        listening_label
    } else {
        disconnected_label
    };
    let runtime_log_label = if runtime_activity.runtime_log_present {
        format!(
            "{} · {}",
            RUNTIME_LOG_FILE_NAME,
            compact_number(runtime_activity.runtime_log_bytes.min(i64::MAX as u64) as i64)
        )
    } else {
        format!("{RUNTIME_LOG_FILE_NAME} {log_missing_label}")
    };
    let live_log_label = if runtime_activity.live_log_present {
        format!(
            "{} · {}",
            LIVE_LOG_FILE_NAME,
            compact_number(runtime_activity.live_log_bytes.min(i64::MAX as u64) as i64)
        )
    } else {
        format!("{LIVE_LOG_FILE_NAME} {log_missing_label}")
    };

    let card = ai_stats_card(runtime_title, cx)
        .child(
            div()
                .mt(px(10.0))
                .grid()
                .grid_cols(3)
                .child(ai_runtime_metric(
                    ai_sidebar_text(language, "ai.runtime.ingress_metric", "Ingress"),
                    ingress_label.to_string(),
                    ingress_accent,
                    cx,
                ))
                .child(ai_runtime_metric(
                    events_label,
                    runtime_activity.runtime_event_count.to_string(),
                    theme::TEXT,
                    cx,
                ))
                .child(ai_runtime_metric(
                    processes_label,
                    runtime_activity.running_ai_processes.len().to_string(),
                    theme::TEXT,
                    cx,
                )),
        )
        .child(
            div()
                .mt(px(10.0))
                .rounded(px(7.0))
                .bg(ai_stats_track_surface(cx))
                .px(px(8.0))
                .py(px(7.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(runtime_ingress.message.clone()),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_DIM))
                        .truncate()
                        .child(runtime_ingress.socket_path.display().to_string()),
                ),
        )
        .child(
            div()
                .mt(px(8.0))
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(div().min_w_0().flex_1().truncate().child(runtime_log_label))
                .child(div().min_w_0().flex_1().truncate().child(live_log_label)),
        );

    if let Some(error) = runtime_activity.error.clone() {
        card.child(
            div()
                .mt(px(8.0))
                .rounded(px(7.0))
                .px(px(8.0))
                .py(px(6.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::ORANGE))
                .bg(color(theme::ORANGE).opacity(0.12))
                .child(error),
        )
    } else {
        card
    }
}

fn ai_runtime_metric(
    label: String,
    value: String,
    accent: u32,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .mr(px(6.0))
        .rounded(px(7.0))
        .bg(ai_stats_track_surface(cx))
        .px(px(8.0))
        .py(px(6.0))
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_DIM))
                .child(label),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_size(px(16.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::BOLD)
                .text_color(color(accent))
                .child(value),
        )
}

fn ai_runtime_session_row(
    session: RuntimeSessionSummary,
    active: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let terminal_id = session.terminal_id.clone();
    let active_bg = ai_stats_track_surface(cx);
    let state_color = match session.state.as_str() {
        "running" => theme::GREEN,
        "needs-input" => theme::ORANGE,
        "completed" => theme::TEXT_DIM,
        _ => theme::TEXT_MUTED,
    };

    div()
        .id(SharedString::from(format!(
            "assistant-runtime-session-{}",
            session.terminal_id
        )))
        .mb(px(6.0))
        .rounded(px(8.0))
        .px(px(8.0))
        .py(px(7.0))
        .cursor_pointer()
        .bg(if active {
            active_bg
        } else {
            cx.theme().transparent
        })
        .hover(move |style| style.bg(active_bg))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_runtime_session(terminal_id.clone(), window, cx)
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
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(session.session_title),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(state_color))
                        .child(session.state),
                ),
        )
        .child(
            div()
                .mt(px(2.0))
                .flex()
                .items_center()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .child(format!("{} · {}", session.tool, session.project_name)),
                )
                .child(
                    div()
                        .ml(px(8.0))
                        .flex_shrink_0()
                        .text_color(color(theme::TEXT_DIM))
                        .child(relative_time_label_for_language(
                            session.updated_at,
                            language,
                        )),
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
        empty_entries,
        empty_summary,
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
                                                .text_size(px(17.0))
                                                .line_height(px(22.0))
                                                .text_color(cx.theme().foreground)
                                                .child(title),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .child(assistant_header_icon_button(
                                                    "memory-manager-window-process",
                                                    IconName::Redo2,
                                                    cx,
                                                    |app, _event, window, cx| {
                                                        app.process_memory_sessions_now(window, cx)
                                                    },
                                                ))
                                                .when(memory_processing, |this| {
                                                    this.child(assistant_header_icon_button(
                                                        "memory-manager-window-stop",
                                                        IconName::Close,
                                                        cx,
                                                        |app, _event, window, cx| {
                                                            app.cancel_memory_extraction_queue(
                                                                window, cx,
                                                            )
                                                        },
                                                    ))
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .mt(px(4.0))
                                        .text_size(px(12.0))
                                        .line_height(px(17.0))
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
                                                        .text_size(px(14.0))
                                                        .line_height(px(18.0))
                                                        .text_color(cx.theme().foreground)
                                                        .child(selected_target_title),
                                                )
                                                .child(
                                                    div()
                                                        .mt(px(4.0))
                                                        .truncate()
                                                        .text_size(px(12.0))
                                                        .line_height(px(16.0))
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
                                                        manager, language, cx,
                                                    ))
                                                    .child(ai_memory_row_icon_button(
                                                        "memory-manager-window-delete-project",
                                                        IconName::Delete,
                                                        cx,
                                                        |app, _event, window, cx| {
                                                            app.delete_selected_memory_project(
                                                                window, cx,
                                                            )
                                                        },
                                                    ))
                                                })
                                                .child(assistant_header_icon_button(
                                                    "memory-manager-window-refresh",
                                                    IconName::Redo2,
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
        .when(memory_processing, |this| {
            this.child(
                div()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(color(theme::BORDER_SOFT))
                    .px_4()
                    .py_2()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(color(theme::TEXT_MUTED))
                    .child(ai_sidebar_text(
                        language,
                        "memory.status.processing",
                        "Memory indexing is running",
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
    empty_entries: String,
    empty_summary: String,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let mut content = div().flex().flex_col();
    if active_tab == MemoryManagerTab::Summary {
        if is_project_scope {
            if let Some(profile) = manager.project_profile.clone() {
                content = content.child(ai_memory_project_profile_row(profile, language, cx));
            } else {
                content = content.child(ai_memory_project_profile_empty_row(language, cx));
            }
        }
        if manager.summaries.is_empty() {
            return content
                .child(ai_memory_manager_empty_row(empty_summary))
                .into_any_element();
        }
        return content
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

    if manager.entries.is_empty() {
        return content
            .child(ai_memory_manager_empty_row(empty_entries))
            .into_any_element();
    }
    content
        .children(manager.entries.iter().cloned().map(|entry| {
            let active = selected_memory_entry_id
                .map(|id| id == entry.id.as_str())
                .unwrap_or(false);
            ai_memory_manager_entry_row(entry, active, cx).into_any_element()
        }))
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
        .child(Icon::new(IconName::Folder).size_4())
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .truncate()
                        .text_size(px(13.0))
                        .line_height(px(17.0))
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(11.0))
                        .line_height(px(15.0))
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
                .text_size(px(11.0))
                .line_height(px(14.0))
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
        .mr(px(6.0))
        .h(px(24.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .flex()
        .items_center()
        .cursor_pointer()
        .text_size(px(12.0))
        .line_height(px(16.0))
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

fn ai_memory_migrate_project_button(
    manager: &MemoryManagerSnapshot,
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
    let target_label = ai_sidebar_text(language, "memory.manager.migrate_project.target", "Target");
    let targets = manager
        .target_rows
        .iter()
        .filter(|target| {
            target.scope == "project" && target.project_id.is_some() && !target.is_open_project
        })
        .cloned()
        .collect::<Vec<_>>();
    let app_entity = cx.entity();

    Button::new("ai-memory-migrate-project-memory")
        .compact()
        .ghost()
        .tooltip(tooltip)
        .text_color(cx.theme().secondary_foreground)
        .icon(
            Icon::new(IconName::ArrowUp)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .dropdown_menu(move |menu, _window, _cx| {
            if targets.is_empty() {
                return menu.item(
                    PopupMenuItem::new(empty_label.clone())
                        .icon(IconName::ArrowUp)
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
                    PopupMenuItem::new(format!("{target_label}: {title}"))
                        .icon(IconName::ArrowUp)
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
        })
}

fn ai_memory_project_profile_row(
    profile: codux_runtime::memory::MemoryProjectProfileSummary,
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
        .border_1()
        .border_color(color(theme::ACCENT).opacity(0.20))
        .px(px(12.0))
        .py(px(11.0))
        .bg(ai_stats_track_surface(cx))
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
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .text_color(color(theme::TEXT))
                        .child(label),
                )
                .child(ai_memory_row_icon_button(
                    "ai-memory-refresh-project-profile",
                    IconName::Redo2,
                    cx,
                    |app, _event, window, cx| {
                        app.refresh_selected_memory_project_profile(window, cx)
                    },
                ))
                .child(ai_memory_row_icon_button(
                    "ai-memory-delete-project-profile",
                    IconName::Delete,
                    cx,
                    |app, _event, window, cx| {
                        app.delete_selected_memory_project_profile(window, cx)
                    },
                )),
        )
        .child(
            div()
                .mt(px(10.0))
                .text_size(px(12.0))
                .line_height(px(18.0))
                .text_color(color(theme::TEXT))
                .w_full()
                .child(profile.content),
        )
}

fn ai_memory_project_profile_empty_row(
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
        .border_1()
        .border_color(cx.theme().border)
        .px(px(12.0))
        .py(px(11.0))
        .bg(ai_stats_track_surface(cx))
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
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .text_color(color(theme::TEXT))
                        .child(label),
                )
                .child(ai_memory_row_icon_button(
                    "ai-memory-create-project-profile",
                    IconName::Redo2,
                    cx,
                    |app, _event, window, cx| {
                        app.refresh_selected_memory_project_profile(window, cx)
                    },
                )),
        )
        .child(
            div()
                .mt(px(8.0))
                .text_size(px(12.0))
                .line_height(px(17.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(empty_label),
        )
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
        .border_1()
        .border_color(if active {
            color(theme::ACCENT).opacity(0.28)
        } else {
            cx.theme().border
        })
        .px(px(12.0))
        .py(px(11.0))
        .cursor_pointer()
        .bg(if active {
            active_bg
        } else {
            cx.theme().transparent
        })
        .hover(move |style| style.bg(active_bg))
        .on_click(cx.listener(move |app, _event, _window, cx| {
            app.selected_memory_summary_id = Some(summary_id.clone());
            app.status_message = format!("selected memory summary: {summary_id}");
            cx.notify();
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
                        .text_size(px(14.0))
                        .line_height(px(18.0))
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
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT_DIM))
                                .child(tokens_label),
                        )
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-save-summary-{save_id}"),
                            IconName::Check,
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
                            IconName::Delete,
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
                .text_size(px(12.0))
                .line_height(px(18.0))
                .text_color(color(theme::TEXT))
                .w_full()
                .child(summary.content.clone())
                .into_any_element()
        })
}

fn ai_memory_manager_entry_row(
    entry: MemoryEntrySummary,
    active: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let select_id = entry.id.clone();
    let archive_id = entry.id.clone();
    let restore_id = entry.id.clone();
    let delete_id = entry.id.clone();
    let active_bg = ai_stats_track_surface(cx);

    div()
        .id(SharedString::from(format!(
            "ai-memory-manager-entry-{}",
            entry.id
        )))
        .mb(px(6.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(if active {
            color(theme::ACCENT).opacity(0.28)
        } else {
            cx.theme().border
        })
        .px(px(12.0))
        .py(px(11.0))
        .cursor_pointer()
        .bg(if active {
            active_bg
        } else {
            cx.theme().transparent
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
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(18.0))
                                .text_color(color(theme::TEXT))
                                .w_full()
                                .child(entry.content.clone()),
                        )
                        .child(
                            div()
                                .mt(px(8.0))
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT_DIM))
                                .child(format!(
                                    "{} · {} · {} · {}",
                                    entry.scope, entry.tier, entry.kind, entry.status
                                )),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-manager-archive-{archive_id}"),
                            IconName::Minus,
                            cx,
                            move |app, _event, window, cx| {
                                app.selected_memory_entry_id = Some(archive_id.clone());
                                app.archive_selected_memory_entry(window, cx);
                            },
                        ))
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-manager-restore-{restore_id}"),
                            IconName::Undo2,
                            cx,
                            move |app, _event, window, cx| {
                                app.selected_memory_entry_id = Some(restore_id.clone());
                                app.restore_selected_memory_entry(window, cx);
                            },
                        ))
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-manager-delete-{delete_id}"),
                            IconName::Delete,
                            cx,
                            move |app, _event, window, cx| {
                                app.selected_memory_entry_id = Some(delete_id.clone());
                                app.delete_selected_memory_entry(window, cx);
                            },
                        )),
                ),
        )
}

fn ai_memory_manager_empty_row(message: impl Into<String>) -> impl IntoElement {
    let message = message.into();
    div()
        .h(px(42.0))
        .rounded(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.0))
        .line_height(px(16.0))
        .text_color(color(theme::TEXT_DIM))
        .bg(color(theme::BG_PANEL))
        .child(message)
}

fn ai_memory_row_icon_button(
    id: impl Into<SharedString>,
    icon: IconName,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    Button::new(id.into())
        .compact()
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .icon(
            Icon::new(icon)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .on_click(cx.listener(on_click))
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
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(label),
        )
        .child(
            div()
                .mt(px(10.0))
                .text_size(px(17.0))
                .line_height(px(22.0))
                .text_color(color(theme::TEXT))
                .child(value),
        )
}

fn ai_today_usage_chart(
    global: &AIGlobalHistorySummary,
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
        global,
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
                    div()
                        .id(SharedString::from(format!("ai-today-usage-{index}")))
                        .flex_1()
                        .min_w(px(2.0))
                        .ml(if index == 0 { px(0.0) } else { px(1.0) })
                        .h(px(10.0 + ratio * 56.0))
                        .rounded(px(3.0))
                        .bg(color(theme::ACCENT))
                        .opacity(if bucket.value > 0 { 1.0 } else { 0.35 })
                        .tooltip(move |window, cx| {
                            Tooltip::new(bucket.tooltip.clone())
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .build(window, cx)
                        })
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
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_MUTED))
                .child("00:00")
                .child("06:00")
                .child("12:00")
                .child("18:00")
                .child("23:59"),
        )
}

fn ai_recent_usage_heatmap(
    global: &AIGlobalHistorySummary,
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
        global,
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

    ai_stats_card(title, cx).p(px(20.0)).child(
        div().mt(px(14.0)).w_full().flex().justify_center().child(
            div()
                .flex()
                .gap(px(AI_RECENT_USAGE_GAP))
                .w(px(grid_width))
                .h(px(grid_height))
                .children(values.chunks(7).enumerate().map(|(column, days)| {
                    let non_zero = non_zero.clone();
                    div()
                        .flex()
                        .w(px(AI_RECENT_USAGE_CELL_SIZE))
                        .flex_col()
                        .gap(px(AI_RECENT_USAGE_GAP))
                        .children(days.iter().cloned().enumerate().map(move |(row, cell)| {
                            let opacity = ai_usage_heatmap_opacity(cell.value, &non_zero);
                            div()
                                .id(SharedString::from(format!(
                                    "ai-recent-usage-{column}-{row}"
                                )))
                                .size(px(AI_RECENT_USAGE_CELL_SIZE))
                                .rounded(px(3.0))
                                .bg(if cell.is_known {
                                    color(theme::ACCENT)
                                } else {
                                    inactive_surface
                                })
                                .opacity(if cell.is_known { opacity } else { 1.0 })
                                .tooltip(move |window, cx| {
                                    Tooltip::new(cell.tooltip.clone())
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .build(window, cx)
                                })
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
            .text_size(px(12.0))
            .line_height(px(16.0))
            .text_color(color(theme::TEXT_DIM))
            .child(empty_label)
            .into_any_element()
    } else {
        div()
            .mt(px(12.0))
            .flex()
            .flex_col()
            .children(rows.into_iter().map(|(label, value, percent)| {
                ai_ranking_row(label, value, percent, track_surface).into_any_element()
            }))
            .into_any_element()
    })
}

fn ai_ranking_row(
    label: String,
    value: i64,
    percent: f32,
    track_surface: gpui::Hsla,
) -> impl IntoElement {
    let value_label = compact_number(value);
    let tooltip = format!("{label} · {value_label} tokens");
    div()
        .id(SharedString::from(format!("ai-ranking-row-{label}")))
        .mb(px(10.0))
        .tooltip(move |window, cx| {
            Tooltip::new(tooltip.clone())
                .text_size(px(12.0))
                .line_height(px(16.0))
                .build(window, cx)
        })
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
                        .text_size(px(13.0))
                        .line_height(px(20.0))
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
                                .text_size(px(13.0))
                                .line_height(px(20.0))
                                .text_color(color(theme::TEXT_MUTED))
                                .child(value_label),
                        )
                        .child(
                            div()
                                .w(px(34.0))
                                .text_right()
                                .text_size(px(11.0))
                                .line_height(px(20.0))
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

fn ai_history_sessions<'a>(
    history: &'a AIHistorySummary,
    global: &'a AIGlobalHistorySummary,
) -> Vec<&'a AISessionSummary> {
    let mut sessions = Vec::new();
    sessions.extend(history.sessions.iter());
    sessions.extend(global.recent_sessions.iter());
    sessions
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
    global: &AIGlobalHistorySummary,
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
        for session in ai_history_sessions(history, global) {
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

    if buckets.iter().all(|bucket| bucket.value == 0) {
        let last = buckets.len() - 1;
        buckets[last].value = ai_display_tokens(
            global.today_total_tokens,
            global.today_cached_input_tokens,
            include_cached,
        )
        .max(ai_history_today_total(history, include_cached));
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
    global: &AIGlobalHistorySummary,
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
        for session in ai_history_sessions(history, global) {
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

    if values.iter().all(|cell| cell.value == 0) {
        let last = values.len() - 1;
        values[last].value = ai_display_tokens(
            global.today_total_tokens,
            global.today_cached_input_tokens,
            include_cached,
        )
        .max(ai_history_today_total(history, include_cached));
        values[last].is_known = values[last].value > 0;
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
    global: &AIGlobalHistorySummary,
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
        ai_history_sessions(history, global)
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
    global: &AIGlobalHistorySummary,
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
        ai_history_sessions(history, global)
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
    selected_session_id: Option<&str>,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let selected_id =
        selected_session_id.or_else(|| history.sessions.first().map(|session| session.id.as_str()));
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
                                .text_size(px(14.0))
                                .line_height(px(18.0))
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
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT_MUTED))
                                .bg(ai_stats_track_surface(cx))
                                .child(history.session_count.to_string()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
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
                            let active = selected_id
                                .map(|id| id == session.id.as_str())
                                .unwrap_or(false);
                            ai_session_list_row(session, active, language, cx).into_any_element()
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
        .text_size(px(12.0))
        .line_height(px(16.0))
        .text_color(color(theme::TEXT_DIM))
        .bg(cx.theme().group_box)
        .child(message)
}

fn ai_session_list_row(
    session: AISessionSummary,
    active: bool,
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
        .bg(if active {
            active_bg
        } else {
            cx.theme().transparent
        })
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
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(session.title),
                )
                .child(
                    div()
                        .ml(px(10.0))
                        .flex_shrink_0()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
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
                .text_size(px(12.0))
                .line_height(px(16.0))
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
                            IconName::Check,
                            cx,
                            move |app, _event, window, cx| {
                                let title = save_state.read(cx).value().to_string();
                                app.rename_selected_ai_session_to(title, window, cx);
                            },
                        ))
                        .child(assistant_header_icon_button(
                            "assistant-ai-restore-session",
                            IconName::SquareTerminal,
                            cx,
                            move |app, _event, window, cx| {
                                app.selected_ai_session_id = Some(restore_id.clone());
                                app.restore_selected_ai_session(window, cx);
                            },
                        ))
                        .child(assistant_header_icon_button(
                            "assistant-ai-remove-session",
                            IconName::Delete,
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
                .text_size(px(12.0))
                .line_height(px(16.0))
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
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .bg(ai_stats_track_surface(cx))
                        .child(file.file_path.clone())
                        .into_any_element()
                })),
        )
}
