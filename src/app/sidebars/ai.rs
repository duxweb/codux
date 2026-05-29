use super::*;
use codux_runtime::ai_runtime_state::AIRuntimeStateSummary;
use gpui_component::input::{Input, InputState};

pub(in crate::app) fn ai_stats_sidebar(
    global: &AIGlobalHistorySummary,
    history: &AIHistorySummary,
    selected_project_id: Option<&str>,
    statistics_mode: &str,
    _memory: &MemorySummary,
    _memory_manager: &MemoryManagerSnapshot,
    _memory_manager_tab: MemoryManagerTab,
    _runtime_events: &RuntimeEventSummary,
    ai_runtime_state: &AIRuntimeStateSummary,
    _runtime_activity: &RuntimeActivitySummary,
    _runtime_ingress: &RuntimeIngressStatus,
    _selected_detail: Option<&AISessionDetail>,
    _selected_session_id: Option<&str>,
    _selected_memory_entry_id: Option<&str>,
    _selected_memory_summary_id: Option<&str>,
    _memory_processing: bool,
    _selected_runtime_session: Option<&RuntimeSessionSummary>,
    _window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let include_cached = statistics_mode == "includingCache";
    let live_sessions = ai_live_sessions(ai_runtime_state, selected_project_id);
    let live_project_total_tokens = ai_live_sessions_total(&live_sessions, include_cached);
    let live_today_tokens = ai_live_sessions_today_total(&live_sessions, include_cached);
    let project_total_tokens = ai_display_tokens(
        history.project_total_tokens,
        history.project_cached_input_tokens,
        include_cached,
    ) + live_project_total_tokens;
    let today_total_tokens = ai_history_today_total(history, include_cached) + live_today_tokens;
    let tool_rows = ai_tool_rows(history, global, &live_sessions, include_cached);
    let model_rows = ai_model_rows(history, global, &live_sessions, include_cached);

    div()
        .flex()
        .min_h_0()
        .flex_col()
        .child(assistant_panel_header(
            "AI 统计",
            IconName::Bot,
            header_icon_button(
                "ai-stats-refresh",
                IconName::Redo2,
                cx,
                |app, _event, window, cx| {
                    app.reload_ai_history(window, cx);
                    app.reload_runtime_activity(window, cx);
                },
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
                .child(ai_current_session_card(&live_sessions, include_cached, cx))
                .child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .child(div().flex_1().mr(px(12.0)).child(ai_metric_card(
                            "当前项目",
                            compact_number(project_total_tokens),
                            cx,
                        )))
                        .child(div().flex_1().child(ai_metric_card(
                            "今日总量",
                            compact_number(today_total_tokens),
                            cx,
                        ))),
                )
                .child(div().mt(px(12.0)).child(ai_today_usage_chart(
                    global,
                    history,
                    &live_sessions,
                    include_cached,
                    cx,
                )))
                .child(div().mt(px(12.0)).child(ai_recent_usage_heatmap(
                    global,
                    history,
                    &live_sessions,
                    include_cached,
                    cx,
                )))
                .child(
                    div()
                        .mt(px(12.0))
                        .child(ai_ranking_card("工具排行", tool_rows, cx)),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .child(ai_ranking_card("模型排行", model_rows, cx)),
                ),
        )
        .child(ai_indexing_status_bar(history, cx))
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
    include_cached: bool,
) -> i64 {
    sessions
        .iter()
        .map(|session| {
            ai_display_tokens(
                session.total_tokens,
                session.cached_input_tokens,
                include_cached,
            )
        })
        .sum()
}

fn ai_live_sessions_today_total(
    sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    include_cached: bool,
) -> i64 {
    let now = ai_now_seconds();
    let day_start = ai_local_day_start_seconds(now);
    sessions
        .iter()
        .filter(|session| session.updated_at >= day_start)
        .map(|session| {
            ai_display_tokens(
                session.total_tokens,
                session.cached_input_tokens,
                include_cached,
            )
        })
        .sum()
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
    let summary_total = ai_display_tokens(
        history.today_total_tokens,
        history.today_cached_input_tokens,
        include_cached,
    );
    summary_total.max(bucket_total).max(heatmap_total)
}

fn ai_stats_card(title: &'static str, cx: &mut Context<CoduxApp>) -> gpui::Div {
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
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT))
                .child(title),
        )
}

fn ai_current_session_card(
    sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    include_cached: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let body =
        if sessions.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_DIM))
                .child("当前没有可显示的 AI 会话")
                .into_any_element()
        } else {
            div()
                .mt(px(10.0))
                .flex()
                .flex_col()
                .children(sessions.iter().take(6).map(|session| {
                    ai_live_session_row(session, include_cached, cx).into_any_element()
                }))
                .into_any_element()
        };

    ai_stats_card("当前会话累计", cx)
        .min_h(px(100.0))
        .child(body)
}

fn ai_live_session_row(
    session: &codux_runtime::ai_runtime_state::AIRuntimeSessionSummary,
    include_cached: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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
                        .font_weight(FontWeight::SEMIBOLD)
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .child(compact_number(ai_display_tokens(
                            session.total_tokens,
                            session.cached_input_tokens,
                            include_cached,
                        ))),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child("会话累计"),
                ),
        )
}

fn ai_indexing_status_bar(
    history: &AIHistorySummary,
    _cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let (label, accent) = if let Some(error) = history.error.as_ref() {
        (format!("索引失败 · {error}"), theme::ORANGE)
    } else if history.indexed {
        ("AI 历史索引已完成".to_string(), theme::GREEN)
    } else {
        ("AI 历史等待索引".to_string(), theme::TEXT_MUTED)
    };

    div()
        .flex_shrink_0()
        .h(px(34.0))
        .border_t_1()
        .border_color(color(theme::BORDER_SOFT))
        .px_3()
        .flex()
        .items_center()
        .justify_between()
        .bg(color(theme::BG_PANEL).opacity(0.72))
        .child(
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_DIM))
                .child(div().size(px(7.0)).rounded_full().bg(color(accent)))
                .child(div().min_w_0().truncate().child(label)),
        )
        .child(
            div()
                .ml(px(8.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(format!("{} 会话", history.session_count)),
        )
}

fn ai_runtime_sessions_card(
    runtime_events: &RuntimeEventSummary,
    ai_runtime_state: &AIRuntimeStateSummary,
    selected_session: Option<&RuntimeSessionSummary>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let selected_terminal_id = selected_session.map(|session| session.terminal_id.as_str());
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
                    .unwrap_or_else(|| "暂无运行中的 AI 会话".to_string()),
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
                        ai_runtime_session_row(session, active, cx).into_any_element()
                    }),
            )
            .into_any_element()
    };

    let card = ai_stats_card("运行中 AI 会话", cx)
        .child(
            div()
                .mt(px(10.0))
                .grid()
                .grid_cols(3)
                .child(ai_runtime_metric(
                    "运行",
                    runtime_events.running_count.to_string(),
                    theme::GREEN,
                    cx,
                ))
                .child(ai_runtime_metric(
                    "等待",
                    runtime_events.needs_input_count.to_string(),
                    theme::ORANGE,
                    cx,
                ))
                .child(ai_runtime_metric(
                    "完成",
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
                                .font_weight(FontWeight::SEMIBOLD)
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
                            "运行 {} · 等待 {} · 完成 {} · 会话 {}",
                            ai_runtime_state.running_count,
                            ai_runtime_state.needs_input_count,
                            ai_runtime_state.completed_count,
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
                        .font_weight(FontWeight::SEMIBOLD)
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
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let ingress_accent = if runtime_ingress.started {
        theme::GREEN
    } else {
        theme::ORANGE
    };
    let ingress_label = if runtime_ingress.started {
        "监听中"
    } else {
        "未监听"
    };
    let runtime_log_label = if runtime_activity.runtime_log_present {
        format!(
            "runtime.log · {}",
            compact_number(runtime_activity.runtime_log_bytes.min(i64::MAX as u64) as i64)
        )
    } else {
        "runtime.log 未生成".to_string()
    };
    let live_log_label = if runtime_activity.live_log_present {
        format!(
            "live.log · {}",
            compact_number(runtime_activity.live_log_bytes.min(i64::MAX as u64) as i64)
        )
    } else {
        "live.log 未生成".to_string()
    };

    let card = ai_stats_card("运行时入口", cx)
        .child(
            div()
                .mt(px(10.0))
                .grid()
                .grid_cols(3)
                .child(ai_runtime_metric(
                    "Ingress",
                    ingress_label.to_string(),
                    ingress_accent,
                    cx,
                ))
                .child(ai_runtime_metric(
                    "事件",
                    runtime_activity.runtime_event_count.to_string(),
                    theme::TEXT,
                    cx,
                ))
                .child(ai_runtime_metric(
                    "进程",
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
                        .font_weight(FontWeight::SEMIBOLD)
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
    label: &'static str,
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(session.session_title),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
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
                        .child(relative_time_label(session.updated_at)),
                ),
        )
}

fn ai_memory_card(
    memory: &MemorySummary,
    selected_memory_entry_id: Option<&str>,
    memory_processing: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let status_label = if memory_processing {
        "正在处理记忆队列".to_string()
    } else if memory.available {
        format!(
            "{} active · {} queued",
            memory.active_entries, memory.queued_extractions
        )
    } else {
        memory
            .error
            .clone()
            .unwrap_or_else(|| "memory unavailable".to_string())
    };
    let entry_list = if memory.recent_entries.is_empty() {
        div()
            .mt(px(10.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(42.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(color(theme::TEXT_DIM))
                    .bg(ai_stats_track_surface(cx))
                    .child("暂无最近记忆"),
            )
            .into_any_element()
    } else {
        div()
            .mt(px(10.0))
            .flex()
            .flex_col()
            .children(memory.recent_entries.iter().take(4).cloned().map(|entry| {
                let active = selected_memory_entry_id
                    .map(|id| id == entry.id.as_str())
                    .unwrap_or(false);
                ai_memory_entry_row(entry, active, cx).into_any_element()
            }))
            .into_any_element()
    };

    ai_stats_card("记忆状态", cx)
        .child(
            div()
                .mt(px(10.0))
                .grid()
                .grid_cols(3)
                .child(ai_memory_metric(
                    "工作",
                    memory.working_entries.to_string(),
                    cx,
                ))
                .child(ai_memory_metric(
                    "核心",
                    memory.core_entries.to_string(),
                    cx,
                ))
                .child(ai_memory_metric(
                    "失败",
                    memory.failed_extractions.to_string(),
                    cx,
                )),
        )
        .child(
            div()
                .mt(px(10.0))
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
                        .text_color(color(theme::TEXT_MUTED))
                        .truncate()
                        .child(status_label),
                )
                .child(assistant_header_icon_button(
                    "ai-memory-refresh",
                    IconName::Redo2,
                    cx,
                    |app, _event, window, cx| app.reload_memory(window, cx),
                ))
                .child(assistant_header_icon_button(
                    "ai-memory-process",
                    IconName::LoaderCircle,
                    cx,
                    |app, _event, window, cx| app.process_memory_sessions_now(window, cx),
                )),
        )
        .child(entry_list)
}

fn ai_memory_manager_card(
    manager: &MemoryManagerSnapshot,
    active_tab: MemoryManagerTab,
    selected_memory_entry_id: Option<&str>,
    selected_memory_summary_id: Option<&str>,
    row_limit: usize,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let overview = &manager.current_overview;
    let extraction = &manager.extraction;
    let has_active_queue = extraction.queued > 0 || extraction.running > 0;
    let target = if manager.selected_target_title.is_empty() {
        "当前项目".to_string()
    } else {
        manager.selected_target_title.clone()
    };
    let rows = match active_tab {
        MemoryManagerTab::Summary => {
            if manager.summaries.is_empty() {
                ai_memory_manager_empty_row("暂无摘要").into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .children(manager.summaries.iter().take(row_limit).map(|summary| {
                        ai_memory_manager_summary_row(
                            summary,
                            selected_memory_summary_id,
                            window,
                            cx,
                        )
                        .into_any_element()
                    }))
                    .into_any_element()
            }
        }
        MemoryManagerTab::Active | MemoryManagerTab::History => {
            if manager.entries.is_empty() {
                ai_memory_manager_empty_row("暂无记忆条目").into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .children(
                        manager
                            .entries
                            .iter()
                            .take(row_limit)
                            .cloned()
                            .map(|entry| {
                                let active = selected_memory_entry_id
                                    .map(|id| id == entry.id.as_str())
                                    .unwrap_or(false);
                                ai_memory_manager_entry_row(entry, active, cx).into_any_element()
                            }),
                    )
                    .into_any_element()
            }
        }
    };

    let mut card = ai_stats_card("记忆管理", cx)
        .child(
            div()
                .mt(px(8.0))
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(target),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_DIM))
                        .child(format!(
                            "{} tokens",
                            compact_number(overview.total_token_estimate)
                        )),
                ),
        )
        .child(
            div()
                .mt(px(10.0))
                .grid()
                .grid_cols(3)
                .child(ai_memory_metric(
                    "激活",
                    overview.active_entry_count.to_string(),
                    cx,
                ))
                .child(ai_memory_metric(
                    "归档",
                    overview.archived_entry_count.to_string(),
                    cx,
                ))
                .child(ai_memory_metric(
                    "摘要",
                    overview.summary_count.to_string(),
                    cx,
                )),
        )
        .child(
            div()
                .mt(px(10.0))
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
                        .text_color(color(theme::TEXT_MUTED))
                        .truncate()
                        .child(format!(
                            "队列 {} · 运行 {} · 失败 {}",
                            extraction.queued, extraction.running, extraction.failed
                        )),
                )
                .child(if has_active_queue {
                    assistant_header_icon_button(
                        "ai-memory-cancel-queue",
                        IconName::Close,
                        cx,
                        |app, _event, window, cx| app.cancel_memory_extraction_queue(window, cx),
                    )
                    .into_any_element()
                } else {
                    div().into_any_element()
                }),
        )
        .child(
            div()
                .mt(px(8.0))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_DIM))
                        .child("项目记忆"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(ai_memory_row_icon_button(
                            "ai-memory-open-manager-window",
                            IconName::ExternalLink,
                            cx,
                            |app, _event, window, cx| app.open_memory_manager_window(window, cx),
                        ))
                        .child(ai_memory_migrate_project_button(manager, cx))
                        .child(ai_memory_row_icon_button(
                            "ai-memory-delete-project-memory",
                            IconName::Delete,
                            cx,
                            |app, _event, window, cx| {
                                app.delete_selected_memory_project(window, cx)
                            },
                        )),
                ),
        )
        .child(
            div()
                .mt(px(10.0))
                .flex()
                .items_center()
                .child(ai_memory_manager_tab_button(
                    "记忆",
                    MemoryManagerTab::Active,
                    active_tab,
                    cx,
                ))
                .child(ai_memory_manager_tab_button(
                    "历史",
                    MemoryManagerTab::History,
                    active_tab,
                    cx,
                ))
                .child(ai_memory_manager_tab_button(
                    "摘要",
                    MemoryManagerTab::Summary,
                    active_tab,
                    cx,
                )),
        );

    if let Some(profile) = manager.project_profile.clone() {
        card = card.child(ai_memory_project_profile_row(profile, cx));
    } else {
        card = card.child(ai_memory_project_profile_empty_row(cx));
    }

    card = card.child(
        div()
            .mt(px(8.0))
            .max_h(px(230.0))
            .overflow_y_scrollbar()
            .child(rows),
    );

    if !manager.available || manager.error.is_some() {
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
                .child(
                    manager
                        .error
                        .clone()
                        .unwrap_or_else(|| "记忆管理器暂不可用".to_string()),
                ),
        )
    } else {
        card
    }
}

pub(in crate::app) fn memory_manager_window_workspace(
    manager: &MemoryManagerSnapshot,
    active_tab: MemoryManagerTab,
    selected_memory_entry_id: Option<&str>,
    selected_memory_summary_id: Option<&str>,
    memory_processing: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .bg(color(theme::BG))
        .child(
            div()
                .h(px(56.0))
                .px_4()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .child(
                    div()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color(theme::TEXT))
                                .child("记忆管理"),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT_DIM))
                                .child(manager.selected_target_title.clone()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(assistant_header_icon_button(
                            "memory-manager-window-refresh",
                            IconName::Redo2,
                            cx,
                            |app, _event, window, cx| app.reload_memory(window, cx),
                        ))
                        .child(assistant_header_icon_button(
                            "memory-manager-window-process",
                            IconName::LoaderCircle,
                            cx,
                            |app, _event, window, cx| app.process_memory_sessions_now(window, cx),
                        )),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .p(px(14.0))
                .child(ai_memory_manager_card(
                    manager,
                    active_tab,
                    selected_memory_entry_id,
                    selected_memory_summary_id,
                    500,
                    window,
                    cx,
                )),
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
                    .child("记忆索引正在运行"),
            )
        })
}

fn ai_memory_manager_tab_button(
    label: &'static str,
    tab: MemoryManagerTab,
    active_tab: MemoryManagerTab,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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
        .font_weight(FontWeight::SEMIBOLD)
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
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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
        .tooltip("迁移项目记忆")
        .text_color(cx.theme().secondary_foreground)
        .icon(
            Icon::new(IconName::ArrowUp)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .dropdown_menu(move |menu, _window, _cx| {
            if targets.is_empty() {
                return menu.item(
                    PopupMenuItem::new("暂无可迁移目标")
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
                    PopupMenuItem::new(format!("迁移到 {title}"))
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
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .mt(px(8.0))
        .rounded(px(8.0))
        .px(px(8.0))
        .py(px(7.0))
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
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child("项目 Profile"),
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
                .mt(px(2.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_MUTED))
                .truncate()
                .child(profile.content),
        )
}

fn ai_memory_project_profile_empty_row(cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .mt(px(8.0))
        .rounded(px(8.0))
        .px(px(8.0))
        .py(px(7.0))
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
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child("项目 Profile"),
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
                .mt(px(2.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_MUTED))
                .truncate()
                .child("尚未生成项目 Profile"),
        )
}

fn ai_memory_manager_summary_row(
    summary: &codux_runtime::memory::MemorySummaryRow,
    selected_memory_summary_id: Option<&str>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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
                    .placeholder("摘要内容")
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
        .px(px(8.0))
        .py(px(7.0))
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
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(format!("{} v{}", summary.scope, summary.version)),
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
                                .child(format!("{}t", summary.token_estimate)),
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
        .child(
            div()
                .mt(px(2.0))
                .child(Input::new(&input_state).with_size(gpui_component::Size::Small)),
        )
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
                                .line_height(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color(theme::TEXT))
                                .truncate()
                                .child(entry.content.clone()),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT_DIM))
                                .truncate()
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

fn ai_memory_manager_empty_row(message: &'static str) -> impl IntoElement {
    div()
        .h(px(42.0))
        .rounded(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.0))
        .line_height(px(16.0))
        .text_color(color(theme::TEXT_DIM))
        .bg(color(0xFFFFFF).opacity(0.035))
        .child(message)
}

fn ai_memory_metric(
    label: &'static str,
    value: String,
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
                .text_color(color(theme::TEXT))
                .child(value),
        )
}

fn ai_memory_entry_row(
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
        .id(SharedString::from(format!("ai-memory-entry-{}", entry.id)))
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
                                .line_height(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color(theme::TEXT))
                                .truncate()
                                .child(entry.content.clone()),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT_DIM))
                                .child(format!(
                                    "{} · {} · {}",
                                    entry.scope, entry.kind, entry.status
                                )),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-archive-entry-{archive_id}"),
                            IconName::Minus,
                            cx,
                            move |app, _event, window, cx| {
                                app.selected_memory_entry_id = Some(archive_id.clone());
                                app.archive_selected_memory_entry(window, cx);
                            },
                        ))
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-restore-entry-{restore_id}"),
                            IconName::Undo2,
                            cx,
                            move |app, _event, window, cx| {
                                app.selected_memory_entry_id = Some(restore_id.clone());
                                app.restore_selected_memory_entry(window, cx);
                            },
                        ))
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-delete-entry-{delete_id}"),
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

fn ai_tool_launcher_card(cx: &mut Context<CoduxApp>) -> impl IntoElement {
    ai_stats_card("启动 AI 工具", cx)
        .child(
            div()
                .mt(px(12.0))
                .flex()
                .child(div().flex_1().mr(px(8.0)).child(ai_tool_launcher_button(
                    "ai-tool-codex",
                    "Codex",
                    IconName::Bot,
                    AIToolLauncher::Codex,
                    cx,
                )))
                .child(div().flex_1().child(ai_tool_launcher_button(
                    "ai-tool-claude",
                    "Claude",
                    IconName::Heart,
                    AIToolLauncher::Claude,
                    cx,
                ))),
        )
        .child(
            div()
                .mt(px(8.0))
                .flex()
                .child(div().flex_1().mr(px(8.0)).child(ai_tool_launcher_button(
                    "ai-tool-gemini",
                    "Gemini",
                    IconName::Star,
                    AIToolLauncher::Gemini,
                    cx,
                )))
                .child(div().flex_1().child(ai_tool_launcher_button(
                    "ai-tool-opencode",
                    "OpenCode",
                    IconName::SquareTerminal,
                    AIToolLauncher::OpenCode,
                    cx,
                ))),
        )
        .child(div().mt(px(8.0)).child(ai_tool_launcher_button(
            "ai-tool-kiro",
            "Kiro",
            IconName::File,
            AIToolLauncher::Kiro,
            cx,
        )))
}

fn ai_tool_launcher_button(
    id: &'static str,
    label: &'static str,
    icon: IconName,
    tool: AIToolLauncher,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    Button::new(id)
        .compact()
        .secondary()
        .text_color(color(theme::TEXT))
        .justify_start()
        .w_full()
        .child(
            div()
                .h(px(28.0))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Icon::new(icon)
                        .size_3p5()
                        .text_color(color(theme::TEXT_MUTED)),
                )
                .child(
                    div()
                        .mt(px(1.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .child(label),
                ),
        )
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.launch_ai_tool(tool, window, cx);
        }))
}

fn ai_metric_card(
    label: &'static str,
    value: String,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_MUTED))
                .child(label),
        )
        .child(
            div()
                .mt(px(10.0))
                .text_size(px(17.0))
                .line_height(px(22.0))
                .font_weight(FontWeight::BOLD)
                .text_color(color(theme::TEXT))
                .child(value),
        )
}

fn ai_today_usage_chart(
    global: &AIGlobalHistorySummary,
    history: &AIHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    include_cached: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let values = ai_today_bucket_values(global, history, live_sessions, include_cached);
    let max_value = values.iter().copied().max().unwrap_or(0).max(1);

    ai_stats_card("今日用量", cx)
        .min_h(px(134.0))
        .child(
            div()
                .mt(px(12.0))
                .flex()
                .items_end()
                .justify_center()
                .h(px(62.0))
                .children(values.into_iter().enumerate().map(|(index, value)| {
                    let ratio = value as f32 / max_value as f32;
                    div()
                        .w(px(7.0))
                        .ml(if index == 0 { px(0.0) } else { px(4.0) })
                        .h(px(10.0 + ratio * 56.0))
                        .rounded(px(4.0))
                        .bg(color(theme::ACCENT))
                        .opacity(if value > 0 { 1.0 } else { 0.35 })
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
    include_cached: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let values = ai_recent_heatmap_values(global, history, live_sessions, include_cached);
    let max_value = values.iter().copied().max().unwrap_or(0).max(1);
    let inactive_surface = ai_stats_track_surface(cx);
    ai_stats_card("近期用量", cx).child(div().mt(px(12.0)).flex().flex_wrap().children(
        values.into_iter().map(|value| {
            let ratio = value as f32 / max_value as f32;
            div()
                .size(px(10.0))
                .mr(px(6.0))
                .mb(px(6.0))
                .rounded(px(3.0))
                .bg(if value > 0 {
                    color(theme::ACCENT)
                } else {
                    inactive_surface
                })
                .opacity(if value > 0 { 0.35 + ratio * 0.65 } else { 1.0 })
                .into_any_element()
        }),
    ))
}

fn ai_ranking_card(
    title: &'static str,
    rows: Vec<(String, i64, f32)>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let track_surface = ai_stats_track_surface(cx);
    ai_stats_card(title, cx).child(if rows.is_empty() {
        div()
            .mt(px(12.0))
            .text_size(px(12.0))
            .line_height(px(16.0))
            .text_color(color(theme::TEXT_DIM))
            .child("暂无 AI 统计")
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
    div()
        .mb(px(10.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .mr(px(8.0))
                        .flex_1()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(label),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT_MUTED))
                        .child(compact_number(value)),
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

fn ai_today_bucket_values(
    global: &AIGlobalHistorySummary,
    history: &AIHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    include_cached: bool,
) -> [i64; 10] {
    let mut buckets = [0_i64; 10];
    let mut has_indexed_buckets = false;
    if !history.today_time_buckets.is_empty() {
        for bucket in &history.today_time_buckets {
            let index = (((bucket.start - history.today_time_buckets[0].start) / 86_400.0)
                * buckets.len() as f64)
                .floor()
                .clamp(0.0, (buckets.len() - 1) as f64) as usize;
            buckets[index] += ai_display_tokens(
                bucket.total_tokens,
                bucket.cached_input_tokens,
                include_cached,
            );
        }
        has_indexed_buckets = buckets.iter().any(|value| *value > 0);
    }

    let now = ai_now_seconds();
    let day_start = ai_local_day_start_seconds(now);

    if !has_indexed_buckets {
        for session in ai_history_sessions(history, global) {
            if session.last_seen_at < day_start {
                continue;
            }
            let bucket = (((session.last_seen_at - day_start) / 86_400.0) * buckets.len() as f64)
                .floor()
                .clamp(0.0, (buckets.len() - 1) as f64) as usize;
            buckets[bucket] += ai_display_tokens(
                session.total_tokens,
                session.cached_input_tokens,
                include_cached,
            );
        }
    }

    for session in live_sessions {
        if session.updated_at < day_start {
            continue;
        }
        let bucket = (((session.updated_at - day_start) / 86_400.0) * buckets.len() as f64)
            .floor()
            .clamp(0.0, (buckets.len() - 1) as f64) as usize;
        buckets[bucket] += ai_display_tokens(
            session.total_tokens,
            session.cached_input_tokens,
            include_cached,
        );
    }

    if buckets.iter().all(|value| *value == 0) {
        buckets[buckets.len() - 1] = ai_display_tokens(
            global.today_total_tokens,
            global.today_cached_input_tokens,
            include_cached,
        )
        .max(ai_history_today_total(history, include_cached));
    }

    buckets
}

fn ai_recent_heatmap_values(
    global: &AIGlobalHistorySummary,
    history: &AIHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
    include_cached: bool,
) -> [i64; 84] {
    let mut values = [0_i64; 84];
    let mut has_indexed_heatmap = false;
    if !history.heatmap.is_empty() {
        let now = ai_now_seconds();
        let today = ai_local_day_start_seconds(now);
        for day in &history.heatmap {
            let day_start = ai_local_day_start_seconds(day.day);
            let day_offset = ((today - day_start) / 86_400.0).round() as isize;
            if (0..values.len() as isize).contains(&day_offset) {
                let index = values.len() - 1 - day_offset as usize;
                values[index] +=
                    ai_display_tokens(day.total_tokens, day.cached_input_tokens, include_cached);
            }
        }
        has_indexed_heatmap = values.iter().any(|value| *value > 0);
    }

    let now = ai_now_seconds();
    let today = ai_local_day_start_seconds(now);

    if !has_indexed_heatmap {
        for session in ai_history_sessions(history, global) {
            let session_day = ai_local_day_start_seconds(session.last_seen_at);
            let day_offset = ((today - session_day) / 86_400.0).round() as isize;
            if (0..values.len() as isize).contains(&day_offset) {
                let index = values.len() - 1 - day_offset as usize;
                values[index] += ai_display_tokens(
                    session.total_tokens,
                    session.cached_input_tokens,
                    include_cached,
                );
            }
        }
    }

    for session in live_sessions {
        let session_day = ai_local_day_start_seconds(session.updated_at);
        let day_offset = ((today - session_day) / 86_400.0).round() as isize;
        if (0..values.len() as isize).contains(&day_offset) {
            let index = values.len() - 1 - day_offset as usize;
            values[index] += ai_display_tokens(
                session.total_tokens,
                session.cached_input_tokens,
                include_cached,
            );
        }
    }

    if values.iter().all(|value| *value == 0) {
        values[values.len() - 1] = ai_display_tokens(
            global.today_total_tokens,
            global.today_cached_input_tokens,
            include_cached,
        )
        .max(ai_history_today_total(history, include_cached));
    }

    values
}

fn ai_tool_rows(
    history: &AIHistorySummary,
    global: &AIGlobalHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
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
                        ai_display_tokens(
                            session.total_tokens,
                            session.cached_input_tokens,
                            include_cached,
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
                    ai_display_tokens(
                        session.total_tokens,
                        session.cached_input_tokens,
                        include_cached,
                    ),
                )
            })),
    )
}

fn ai_model_rows(
    history: &AIHistorySummary,
    global: &AIGlobalHistorySummary,
    live_sessions: &[&codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
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
                            ai_display_tokens(
                                session.total_tokens,
                                session.cached_input_tokens,
                                include_cached,
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
                        ai_display_tokens(
                            session.total_tokens,
                            session.cached_input_tokens,
                            include_cached,
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
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let selected_id =
        selected_session_id.or_else(|| history.sessions.first().map(|session| session.id.as_str()));
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
                .bg(ai_stats_surface(cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color(theme::TEXT))
                                .child("会话记录"),
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
                                .font_weight(FontWeight::SEMIBOLD)
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
                            "已索引"
                        } else {
                            "未索引"
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
                    ai_empty_sessions(history).into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .children(history.sessions.iter().take(12).cloned().map(|session| {
                            let active = selected_id
                                .map(|id| id == session.id.as_str())
                                .unwrap_or(false);
                            ai_session_list_row(session, active, cx).into_any_element()
                        }))
                        .into_any_element()
                }),
        )
        .child(if let Some(detail) = selected_detail {
            ai_session_detail_summary(detail, window, cx).into_any_element()
        } else {
            div().into_any_element()
        })
}

fn ai_empty_sessions(history: &AIHistorySummary) -> impl IntoElement {
    let message = history
        .error
        .as_ref()
        .map(|error| format!("索引失败: {error}"))
        .unwrap_or_else(|| "当前项目还没有索引到 AI 会话".to_string());
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
        .bg(color(0xFFFFFF).opacity(0.035))
        .child(message)
}

fn ai_session_list_row(
    session: AISessionSummary,
    active: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let session_id = session.id.clone();
    let active_bg = ai_stats_track_surface(cx);
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
                        .font_weight(FontWeight::SEMIBOLD)
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
                        .child(relative_time_label(session.last_seen_at)),
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
                    "{} · {} 请求 · {}",
                    session.source,
                    session.request_count,
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
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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
                    .placeholder("会话标题")
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
                    "{} · {} 请求 · {} · cached {}",
                    detail.source,
                    detail.request_count,
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
