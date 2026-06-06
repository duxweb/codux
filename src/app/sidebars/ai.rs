use super::*;
use crate::app::ui_helpers::{centered_empty_state, codux_tooltip_container, with_codux_tooltip};
use chrono::{Datelike as _, TimeZone as _, Timelike as _};
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};
use gpui::Hsla;
use gpui_component::input::{Input, InputState};

const AI_RECENT_USAGE_COLUMNS: usize = 20;
const AI_RECENT_USAGE_CELL_SIZE: f32 = 10.0;
const AI_RECENT_USAGE_GAP: f32 = 3.0;

struct AIUsageLabels {
    tokens: String,
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
            request_count_format: tr("ai.metric.request_count_format", "%d requests"),
            unknown_date: tr("common.unknown_date", "Unknown date"),
            weekdays,
        }
    }
}

pub(in crate::app) fn ai_stats_sidebar(
    stats: &codux_runtime::ai_history::AIHistoryStatsView,
    language: &str,
    refreshing: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title = ai_sidebar_text(language, "ai.panel.statistics_title", "AI Statistics");
    let current_project_label =
        ai_sidebar_text(language, "ai.summary.current_project", "Current Project");
    let today_total_label = ai_sidebar_text(language, "ai.summary.today_total", "Today's Total");
    let tool_ranking_label = ai_sidebar_text(language, "ai.breakdown.tool_ranking", "Tool Ranking");
    let model_ranking_label =
        ai_sidebar_text(language, "ai.breakdown.model_ranking", "Model Ranking");

    div()
        .flex()
        .flex_1()
        .h_full()
        .min_h_0()
        .flex_col()
        .child(assistant_panel_header(
            title,
            HeroIconName::Sparkles,
            header_icon_button_loading(
                "ai-stats-refresh",
                HeroIconName::ArrowPath,
                refreshing,
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
                    &stats.current_sessions,
                    language,
                    cx,
                ))
                .child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .child(div().flex_1().mr(px(12.0)).child(ai_metric_card(
                            current_project_label,
                            compact_number(stats.project_total_tokens),
                            cx,
                        )))
                        .child(div().flex_1().child(ai_metric_card(
                            today_total_label,
                            compact_number(stats.today_total_tokens),
                            cx,
                        ))),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .child(ai_today_usage_chart(stats, language, cx)),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .child(ai_recent_usage_heatmap(stats, language, cx)),
                )
                .child(div().mt(px(12.0)).child(ai_ranking_card(
                    tool_ranking_label,
                    stats.tool_rows.clone(),
                    language,
                    cx,
                )))
                .child(div().mt(px(12.0)).child(ai_ranking_card(
                    model_ranking_label,
                    stats.model_rows.clone(),
                    language,
                    cx,
                ))),
        )
}

fn ai_sidebar_text(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
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
    sessions: &[codux_runtime::ai_history::AIHistoryCurrentSessionView],
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
            .children(
                sessions
                    .iter()
                    .take(6)
                    .map(|session| ai_live_session_row(session, language, cx).into_any_element()),
            )
            .into_any_element()
    };

    ai_stats_card(title, cx).min_h(px(100.0)).child(body)
}

fn ai_live_session_row(
    session: &codux_runtime::ai_history::AIHistoryCurrentSessionView,
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
                        .child(compact_number(session.total_tokens)),
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
    let selected_target_title = if active_tab == MemoryManagerTab::Queue {
        ai_sidebar_text(language, "memory.manager.section.queue", "Memory Queue")
    } else if active_tab == MemoryManagerTab::Failed {
        ai_sidebar_text(language, "memory.manager.section.failed", "Failed Records")
    } else if selected_scope == "user" {
        ai_sidebar_text(language, "memory.manager.user_memory", "User Memory")
    } else {
        manager.selected_target_title.clone()
    };
    let target_rows = manager
        .target_rows
        .iter()
        .filter(|target| target.scope != "user")
        .cloned()
        .collect::<Vec<_>>();
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
                        .child(ai_memory_manager_section_switcher(
                            manager,
                            active_tab,
                            selected_scope,
                            language,
                            cx,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scrollbar()
                                .px_2()
                                .pb_3()
                                .when(!target_rows.is_empty(), |this| {
                                    this.child(ai_memory_group_label(
                                        ai_sidebar_text(
                                            language,
                                            "memory.manager.section.projects",
                                            "Projects",
                                        ),
                                        cx,
                                    ))
                                })
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
                                                        .text_size(rems(0.9375))
                                                        .line_height(rems(1.25))
                                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                                        .text_color(cx.theme().foreground)
                                                        .child(selected_target_title),
                                                )
                                                .child(
                                                    div().mt(px(10.0)).child(
                                                        ai_memory_overview_strip(
                                                            manager, active_tab, language, cx,
                                                        ),
                                                    ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .when(
                                                    selected_scope == "project"
                                                        && active_tab != MemoryManagerTab::Queue
                                                        && active_tab != MemoryManagerTab::Failed,
                                                    |this| {
                                                        this.child(
                                                            ai_memory_migrate_project_button(
                                                                manager,
                                                                selected_project_id,
                                                                language,
                                                                cx,
                                                            ),
                                                        )
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
                                                    },
                                                )
                                                .when(active_tab == MemoryManagerTab::Queue, |this| {
                                                    this.child(ai_memory_row_icon_button(
                                                        "memory-manager-window-clear-queue",
                                                        HeroIconName::XCircle,
                                                        ai_sidebar_text(
                                                            language,
                                                            "memory.manager.queue.clear",
                                                            "Clear Queue",
                                                        ),
                                                        cx,
                                                        |app, _event, window, cx| {
                                                            app.cancel_memory_extraction_queue(window, cx)
                                                        },
                                                    ))
                                                })
                                                .when(active_tab == MemoryManagerTab::Failed, |this| {
                                                    this.child(ai_memory_row_icon_button(
                                                        "memory-manager-window-clear-failed",
                                                        HeroIconName::XCircle,
                                                        ai_sidebar_text(
                                                            language,
                                                            "memory.manager.failed.clear",
                                                            "Clear Failed Records",
                                                        ),
                                                        cx,
                                                        |app, _event, window, cx| {
                                                            app.clear_memory_extraction_failures(window, cx)
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
                                .child(div().mt(px(14.0)).flex().items_center().when(
                                    active_tab != MemoryManagerTab::Queue
                                        && active_tab != MemoryManagerTab::Failed,
                                    |this| {
                                        this.child(ai_memory_manager_tab_button(
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
                                        .child(
                                            ai_memory_manager_tab_button(
                                                history_tab,
                                                MemoryManagerTab::History,
                                                active_tab,
                                                cx,
                                            ),
                                        )
                                    },
                                )),
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

/// Shared card shell for every memory manager card (queue / failed / summary /
/// profile / entry). One consistent radius, border, surface and padding so the
/// content area stops looking like a pile of mismatched boxes.
fn ai_memory_card(cx: &mut Context<CoduxApp>) -> gpui::Div {
    div()
        .w_full()
        .rounded(px(10.0))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
        .px(px(14.0))
        .py(px(12.0))
}

/// A small statistic chip (value + label) used in the content header to replace
/// the dense "%lld active, %lld archived…" text line.
fn ai_memory_stat_chip(
    value: impl Into<String>,
    label: impl Into<String>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .flex()
        .items_baseline()
        .gap(px(5.0))
        .rounded(px(7.0))
        .bg(cx.theme().secondary)
        .px(px(9.0))
        .py(px(5.0))
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(value.into()),
        )
        .child(
            div()
                .text_size(rems(0.6875))
                .line_height(rems(1.0))
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
}

/// Labeled navigation row for the left sidebar section switcher. Replaces the
/// icon-only segmented control so each destination reads clearly.
fn ai_memory_nav_row(
    id: &'static str,
    icon: HeroIconName,
    label: impl Into<String>,
    count: Option<i64>,
    active: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let label = label.into();
    let foreground = if active {
        cx.theme().foreground
    } else {
        cx.theme().muted_foreground
    };
    div()
        .id(id)
        .h(px(34.0))
        .w_full()
        .rounded(px(8.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap(px(9.0))
        .cursor_pointer()
        .text_color(foreground)
        .bg(if active {
            cx.theme().sidebar_accent
        } else {
            cx.theme().transparent
        })
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(on_click))
        .child(Icon::new(icon).size_4().text_color(foreground))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(rems(0.8125))
                .line_height(rems(1.0))
                .child(label),
        )
        .when_some(count.filter(|count| *count > 0), |this, count| {
            this.child(
                div()
                    .flex_none()
                    .rounded_full()
                    .px(px(7.0))
                    .py(px(2.0))
                    .text_size(rems(0.6875))
                    .line_height(rems(1.0))
                    .text_color(if active {
                        cx.theme().foreground
                    } else {
                        cx.theme().muted_foreground
                    })
                    .bg(if active {
                        cx.theme().primary.opacity(0.16)
                    } else {
                        cx.theme().muted
                    })
                    .child(count.to_string()),
            )
        })
}

/// Small uppercase group label for the left sidebar (e.g. "Projects").
fn ai_memory_group_label(label: impl Into<String>, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .px(px(10.0))
        .pt(px(10.0))
        .pb(px(4.0))
        .text_size(rems(0.6875))
        .line_height(rems(1.0))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(label.into())
}

/// Content-header overview. For scope tabs this renders visual stat chips; for
/// the queue/failed tabs it falls back to a concise status line.
fn ai_memory_overview_strip(
    manager: &MemoryManagerSnapshot,
    active_tab: MemoryManagerTab,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    if active_tab == MemoryManagerTab::Queue {
        let text = ai_sidebar_text(
            language,
            "memory.status.detail",
            "Memory queue: %lld pending, %lld running",
        )
        .replacen("%lld", &manager.extraction.queued.max(0).to_string(), 1)
        .replacen("%lld", &manager.extraction.running.max(0).to_string(), 1);
        return ai_memory_overview_text(text, cx);
    }
    if active_tab == MemoryManagerTab::Failed {
        let text = format!(
            "{} {}",
            manager.extraction.failed.max(0),
            ai_sidebar_text(language, "memory.status.short_failed", "Failed")
        );
        return ai_memory_overview_text(text, cx);
    }

    let overview = &manager.current_overview;
    let archived = overview.archived_entry_count + overview.merged_entry_count;
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .child(ai_memory_stat_chip(
            overview.active_entry_count.to_string(),
            ai_sidebar_text(language, "memory.manager.stat.active", "Active"),
            cx,
        ))
        .child(ai_memory_stat_chip(
            archived.to_string(),
            ai_sidebar_text(language, "memory.manager.stat.archived", "Archived"),
            cx,
        ))
        .child(ai_memory_stat_chip(
            overview.profile_count.to_string(),
            ai_sidebar_text(language, "memory.manager.stat.profiles", "Profiles"),
            cx,
        ))
        .child(ai_memory_stat_chip(
            overview.summary_count.to_string(),
            ai_sidebar_text(language, "memory.manager.stat.summaries", "Summaries"),
            cx,
        ))
        .child(ai_memory_stat_chip(
            overview.total_token_estimate.to_string(),
            ai_sidebar_text(language, "memory.manager.stat.tokens", "Tokens"),
            cx,
        ))
        .into_any_element()
}

fn ai_memory_overview_text(text: String, cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .truncate()
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
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
    if active_tab == MemoryManagerTab::Queue {
        return ai_memory_manager_queue_content(manager, language, window, cx).into_any_element();
    }

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

fn ai_memory_manager_section_switcher(
    manager: &MemoryManagerSnapshot,
    active_tab: MemoryManagerTab,
    selected_scope: &str,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let user_active = selected_scope == "user"
        && active_tab != MemoryManagerTab::Queue
        && active_tab != MemoryManagerTab::Failed;
    let queue_count = manager.extraction.queued.max(0) + manager.extraction.running.max(0);
    let failed_count = manager.extraction.failed.max(0);
    div()
        .px(px(8.0))
        .pb(px(6.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(ai_memory_nav_row(
            "ai-memory-manager-section-user",
            HeroIconName::UserCircle,
            ai_sidebar_text(language, "memory.manager.section.user", "User Memory"),
            None,
            user_active,
            cx,
            |app, _event, _window, cx| {
                app.select_memory_manager_target("user".to_string(), None, cx)
            },
        ))
        .child(ai_memory_nav_row(
            "ai-memory-manager-section-queue",
            HeroIconName::QueueList,
            ai_sidebar_text(language, "memory.manager.section.queue", "Memory Queue"),
            Some(queue_count),
            active_tab == MemoryManagerTab::Queue,
            cx,
            |app, _event, _window, cx| app.select_memory_manager_queue(cx),
        ))
        .child(ai_memory_nav_row(
            "ai-memory-manager-section-failed",
            HeroIconName::ExclamationTriangle,
            ai_sidebar_text(language, "memory.manager.section.failed", "Failed Records"),
            Some(failed_count),
            active_tab == MemoryManagerTab::Failed,
            cx,
            |app, _event, _window, cx| {
                app.select_memory_manager_failed_records(cx);
            },
        ))
}

fn ai_memory_manager_queue_content(
    manager: &MemoryManagerSnapshot,
    language: &str,
    _window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let queued = manager.extraction.queued.max(0);
    let running = manager.extraction.running.max(0);
    let failed = manager.extraction.failed.max(0);
    let has_queue = !manager.queued_extractions.is_empty();
    let empty_label = ai_sidebar_text(
        language,
        "memory.manager.queue.empty",
        "No queued memory tasks",
    );
    let queued_label = ai_sidebar_text(language, "memory.status.short_queued", "Queued");
    let running_label = ai_sidebar_text(language, "memory.status.short_remembering", "Remembering");
    let failed_label = ai_sidebar_text(language, "memory.status.short_failed", "Failed");

    div()
        .size_full()
        .flex()
        .flex_col()
        .when(!has_queue, |this| {
            this.child(ai_memory_manager_empty_row(empty_label, cx))
        })
        .when(has_queue, |this| {
            this.child(
                ai_memory_card(cx)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Spinner::new().xsmall().color(color(theme::ORANGE)))
                            .child(
                                div()
                                    .text_size(rems(0.875))
                                    .line_height(rems(1.125))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(cx.theme().foreground)
                                    .child(ai_sidebar_text(
                                        language,
                                        "memory.status.processing",
                                        "Remembering",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(12.0))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(ai_memory_queue_count_badge(queued_label, queued, cx))
                            .child(ai_memory_queue_count_badge(running_label, running, cx))
                            .child(ai_memory_queue_count_badge(failed_label, failed, cx)),
                    )
                    .when_some(manager.extraction.last_error.clone(), |this, error| {
                        this.child(
                            div()
                                .mt(px(10.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.25))
                                .text_color(cx.theme().danger)
                                .child(error),
                        )
                    }),
            )
            .children(
                manager.queued_extractions.iter().cloned().map(|task| {
                    ai_memory_queued_extraction_row(task, language, cx).into_any_element()
                }),
            )
        })
}

fn ai_memory_queue_count_badge(
    label: String,
    count: i64,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .rounded_full()
        .px(px(8.0))
        .py(px(3.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(cx.theme().muted_foreground)
        .bg(cx.theme().muted)
        .child(format!("{count} {label}"))
}

fn ai_memory_queued_extraction_row(
    task: codux_runtime::memory::MemoryExtractionTask,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let clear_id = task.id.clone();
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
    let status_label = memory_extraction_status_label(&task.status, language);
    let status_color = if task.status == "running" {
        color(theme::ORANGE)
    } else {
        color(theme::ACCENT)
    };

    ai_memory_card(cx)
        .mt(px(8.0))
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
                                .child(title),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .truncate()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(cx.theme().muted_foreground)
                                .child(subtitle),
                        ),
                )
                .child(
                    div()
                        .rounded_full()
                        .px(px(8.0))
                        .py(px(3.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(status_color)
                        .bg(status_color.opacity(0.14))
                        .child(status_label),
                )
                .when(task.status != "running", |this| {
                    this.child(ai_memory_row_icon_button(
                        format!("ai-memory-manager-clear-pending-{clear_id}"),
                        HeroIconName::Trash,
                        ai_sidebar_text(language, "common.delete", "Delete"),
                        cx,
                        move |app, _event, window, cx| {
                            app.clear_pending_memory_extraction(clear_id.clone(), window, cx)
                        },
                    ))
                }),
        )
        .child(
            div()
                .mt(px(7.0))
                .text_size(rems(0.6875))
                .line_height(rems(1.0))
                .text_color(cx.theme().muted_foreground)
                .child(memory_date_label(task.enqueued_at)),
        )
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
    let foreground = if active {
        cx.theme().foreground
    } else {
        cx.theme().muted_foreground
    };
    div()
        .id(SharedString::from(format!(
            "memory-manager-target-{}",
            target.id
        )))
        .mb(px(2.0))
        .min_h(px(48.0))
        .w_full()
        .rounded(px(8.0))
        .px(px(10.0))
        .py(px(7.0))
        .flex()
        .items_center()
        .gap(px(9.0))
        .cursor_pointer()
        .text_color(foreground)
        .bg(if active {
            cx.theme().sidebar_accent
        } else {
            cx.theme().transparent
        })
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(move |app, _event, _window, cx| {
            app.select_memory_manager_target(scope.clone(), project_id.clone(), cx)
        }))
        .child(
            Icon::new(HeroIconName::Folder)
                .size_4()
                .flex_shrink_0()
                .text_color(foreground),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .truncate()
                        .text_size(rems(0.8125))
                        .line_height(rems(1.125))
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(rems(0.6875))
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
                .text_size(rems(0.6875))
                .line_height(rems(1.0))
                .text_color(if active {
                    cx.theme().foreground
                } else {
                    cx.theme().muted_foreground
                })
                .bg(if active {
                    cx.theme().primary.opacity(0.16)
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
    let hover_bg = cx.theme().secondary_hover;
    div()
        .id(SharedString::from(format!(
            "ai-memory-manager-tab-{}",
            tab.as_str()
        )))
        .mr(px(6.0))
        .h(px(30.0))
        .px(px(12.0))
        .rounded(px(7.0))
        .flex()
        .items_center()
        .cursor_pointer()
        .text_size(rems(0.8125))
        .line_height(rems(1.0))
        .font_weight(if active {
            gpui::FontWeight::MEDIUM
        } else {
            gpui::FontWeight::NORMAL
        })
        .text_color(if active {
            cx.theme().foreground
        } else {
            cx.theme().muted_foreground
        })
        .bg(if active {
            cx.theme().secondary
        } else {
            cx.theme().transparent
        })
        .hover(move |style| style.bg(hover_bg))
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
    ai_memory_card(cx)
        .mt(px(8.0))
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
                        .flex()
                        .items_center()
                        .gap(px(7.0))
                        .child(
                            Icon::new(HeroIconName::DocumentText)
                                .size_4()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(cx.theme().foreground)
                                .child(label),
                        ),
                )
                .child(if refreshing {
                    ai_memory_refreshing_label(language, cx).into_any_element()
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
                .mt(px(9.0))
                .text_size(rems(0.8125))
                .line_height(rems(1.375))
                .text_color(cx.theme().foreground)
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
    ai_memory_card(cx)
        .mt(px(8.0))
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
                        .flex()
                        .items_center()
                        .gap(px(7.0))
                        .child(
                            Icon::new(HeroIconName::DocumentText)
                                .size_4()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(cx.theme().foreground)
                                .child(label),
                        ),
                )
                .child(if refreshing {
                    ai_memory_refreshing_label(language, cx).into_any_element()
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
                .line_height(rems(1.25))
                .text_color(cx.theme().muted_foreground)
                .child(empty_label),
        )
}

fn ai_memory_refreshing_label(language: &str, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .px(px(7.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(cx.theme().muted_foreground)
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

    ai_memory_card(cx)
        .id(SharedString::from(format!(
            "ai-memory-manager-summary-{}",
            summary.id
        )))
        .mb(px(8.0))
        .cursor_pointer()
        .when(active, |this| {
            this.border_color(cx.theme().primary.opacity(0.55))
                .bg(cx.theme().secondary_hover)
        })
        .hover(|style| style.border_color(cx.theme().primary.opacity(0.35)))
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
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(cx.theme().foreground)
                        .child(format!("{} {}", summary.scope, version_label)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .mr(px(4.0))
                                .text_size(rems(0.6875))
                                .line_height(rems(1.0))
                                .text_color(cx.theme().muted_foreground)
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
                .mt(px(9.0))
                .text_size(rems(0.8125))
                .line_height(rems(1.375))
                .text_color(cx.theme().foreground)
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
                .mb(px(16.0))
                .child(
                    div()
                        .mb(px(8.0))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().size(px(7.0)).rounded_full().bg(cx.theme().primary))
                        .child(
                            div()
                                .text_size(rems(0.6875))
                                .line_height(rems(1.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(cx.theme().muted_foreground)
                                .child(memory_module_title(&module_key, language)),
                        )
                        .child(
                            div()
                                .rounded_full()
                                .px(px(7.0))
                                .py(px(1.0))
                                .text_size(rems(0.6875))
                                .line_height(rems(1.0))
                                .text_color(cx.theme().primary)
                                .bg(cx.theme().primary.opacity(0.12))
                                .child(group_entries.len().to_string()),
                        ),
                )
                .child(div().flex().flex_col().gap(px(8.0)).children(
                    group_entries.into_iter().map(|entry| {
                        let active = selected_memory_entry_id
                            .map(|id| id == entry.id.as_str())
                            .unwrap_or(false);
                        ai_memory_manager_entry_row(entry, active, active_tab, language, cx)
                            .into_any_element()
                    }),
                ))
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
    let can_archive = active_tab == MemoryManagerTab::Active && entry.status == "active";

    ai_memory_card(cx)
        .id(SharedString::from(format!(
            "ai-memory-manager-entry-{}",
            entry.id
        )))
        .cursor_pointer()
        .when(active, |this| {
            this.border_color(cx.theme().primary.opacity(0.55))
                .bg(cx.theme().secondary_hover)
        })
        .hover(|style| style.border_color(cx.theme().primary.opacity(0.35)))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_memory_entry(select_id.clone(), window, cx)
        }))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(ai_memory_badge(
                            memory_kind_title(&entry.kind, language),
                            memory_kind_color(&entry.kind),
                        ))
                        .child(ai_memory_status_pill(&entry.status, language, cx)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .mr(px(4.0))
                                .text_size(rems(0.6875))
                                .line_height(rems(1.0))
                                .text_color(cx.theme().muted_foreground)
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
                .mt(px(9.0))
                .w_full()
                .text_size(rems(0.875))
                .line_height(rems(1.375))
                .text_color(cx.theme().foreground)
                .child(entry.content.clone()),
        )
        .child(ai_memory_entry_meta(&entry, language, cx))
        .when_some(entry.rationale.clone(), |this, rationale| {
            this.child(
                div()
                    .mt(px(6.0))
                    .w_full()
                    .text_size(rems(0.75))
                    .line_height(rems(1.25))
                    .text_color(cx.theme().muted_foreground)
                    .child(rationale),
            )
        })
        .when_some(entry.last_decision.clone(), |this, decision| {
            this.child(ai_memory_decision_row(decision, language, cx))
        })
}

fn ai_memory_failed_extraction_row(
    task: codux_runtime::memory::MemoryExtractionTask,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let retry_id = task.id.clone();
    let clear_id = task.id.clone();
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

    ai_memory_card(cx)
        .id(SharedString::from(format!(
            "ai-memory-manager-failed-{}",
            task.id
        )))
        .mb(px(8.0))
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
                                .child(title),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .truncate()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(cx.theme().muted_foreground)
                                .child(subtitle),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-manager-retry-{retry_id}"),
                            HeroIconName::ArrowPath,
                            ai_sidebar_text(language, "memory.manager.failed.retry", "Retry"),
                            cx,
                            move |app, _event, window, cx| {
                                app.retry_failed_memory_extraction(retry_id.clone(), window, cx)
                            },
                        ))
                        .child(ai_memory_row_icon_button(
                            format!("ai-memory-manager-clear-failed-{clear_id}"),
                            HeroIconName::Trash,
                            ai_sidebar_text(language, "common.delete", "Delete"),
                            cx,
                            move |app, _event, window, cx| {
                                app.clear_failed_memory_extraction(clear_id.clone(), window, cx)
                            },
                        )),
                ),
        )
        .child(
            div()
                .mt(px(9.0))
                .w_full()
                .text_size(rems(0.75))
                .line_height(rems(1.25))
                .text_color(cx.theme().danger)
                .child(error),
        )
        .child(
            div()
                .mt(px(6.0))
                .text_size(rems(0.6875))
                .line_height(rems(1.0))
                .text_color(cx.theme().muted_foreground)
                .child(memory_date_label(task.enqueued_at)),
        )
}

fn ai_memory_decision_row(
    decision: codux_runtime::memory::MemoryEntryDecisionSummary,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .mt(px(8.0))
        .flex()
        .items_center()
        .gap_2()
        .rounded(px(8.0))
        .px(px(10.0))
        .py(px(7.0))
        .bg(cx.theme().secondary_hover)
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
                .text_color(cx.theme().muted_foreground)
                .child(decision.reason),
        )
}

/// Secondary status indicator (dot + muted label) for an entry. Kept low-key so
/// the kind badge stays the primary identifier.
fn ai_memory_status_pill(
    status: &str,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let status_color = memory_status_color(status);
    div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .child(div().size(px(6.0)).rounded_full().bg(status_color))
        .child(
            div()
                .text_size(rems(0.6875))
                .line_height(rems(1.0))
                .text_color(cx.theme().muted_foreground)
                .child(memory_status_title(status, language)),
        )
}

/// Demoted meta line for an entry: module · tier · source rendered as plain
/// muted text instead of a row of coloured badges.
fn ai_memory_entry_meta(
    entry: &MemoryEntrySummary,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let mut parts = vec![
        memory_module_title(&memory_module_key(entry), language),
        memory_tier_title(&entry.tier, language),
    ];
    if let Some(source_tool) = entry
        .source_tool
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(source_tool);
    }
    div()
        .mt(px(7.0))
        .text_size(rems(0.6875))
        .line_height(rems(1.0))
        .text_color(cx.theme().muted_foreground)
        .child(parts.join(" · "))
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

fn memory_extraction_status_label(status: &str, language: &str) -> String {
    match status {
        "running" => ai_sidebar_text(language, "memory.status.short_remembering", "Remembering"),
        "queued" | "pending" => ai_sidebar_text(language, "memory.status.short_queued", "Queued"),
        "failed" => ai_sidebar_text(language, "memory.status.short_failed", "Failed"),
        _ => status.to_string(),
    }
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
    stats: &codux_runtime::ai_history::AIHistoryStatsView,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title = ai_sidebar_text(language, "ai.today_usage", "Today's Usage");
    let usage_labels = AIUsageLabels::load(language);

    ai_stats_card(title, cx)
        .min_h(px(134.0))
        .child(
            div()
                .mt(px(12.0))
                .flex()
                .items_end()
                .justify_center()
                .h(px(62.0))
                .children(
                    stats
                        .today_buckets
                        .iter()
                        .enumerate()
                        .map(|(index, bucket)| {
                            let tooltip = ai_usage_tooltip(
                                format!(
                                    "{} - {}",
                                    ai_time_label(bucket.start),
                                    ai_time_label_with_seconds(bucket.end),
                                ),
                                bucket.value,
                                bucket.request_count,
                                &usage_labels,
                            );
                            codux_tooltip_container(
                                cx.entity(),
                                SharedString::from(format!("ai-today-usage-{index}")),
                                tooltip,
                            )
                            .flex_1()
                            .min_w(px(2.0))
                            .ml(if index == 0 { px(0.0) } else { px(1.0) })
                            .h(px(10.0 + bucket.ratio.clamp(0.0, 1.0) * 56.0))
                            .rounded(px(3.0))
                            .bg(color(theme::ACCENT))
                            .opacity(bucket.opacity.clamp(0.0, 1.0))
                            .into_any_element()
                        }),
                ),
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
    stats: &codux_runtime::ai_history::AIHistoryStatsView,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title = ai_sidebar_text(language, "ai.recent_usage", "Recent Usage");
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
                .children(stats.heatmap.chunks(7).enumerate().map(|(column, days)| {
                    let app_entity = app_entity.clone();
                    let usage_labels = AIUsageLabels::load(language);
                    div()
                        .flex()
                        .w(px(AI_RECENT_USAGE_CELL_SIZE))
                        .flex_col()
                        .gap(px(AI_RECENT_USAGE_GAP))
                        .children(days.iter().cloned().enumerate().map(move |(row, cell)| {
                            let tooltip = ai_usage_tooltip(
                                ai_date_label(cell.day, &usage_labels),
                                cell.value,
                                cell.request_count,
                                &usage_labels,
                            );
                            codux_tooltip_container(
                                app_entity.clone(),
                                SharedString::from(format!("ai-recent-usage-{column}-{row}")),
                                tooltip,
                            )
                            .size(px(AI_RECENT_USAGE_CELL_SIZE))
                            .rounded(px(3.0))
                            .bg(if cell.is_known {
                                color(theme::ACCENT)
                            } else {
                                inactive_surface
                            })
                            .opacity(cell.opacity.clamp(0.0, 1.0))
                            .into_any_element()
                        }))
                        .into_any_element()
                })),
        ),
    )
}

fn ai_ranking_card(
    title: impl Into<String>,
    rows: Vec<codux_runtime::ai_history::AIHistoryRankRow>,
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
            .children(
                rows.into_iter()
                    .map(|row| ai_ranking_row(cx.entity(), row, track_surface).into_any_element()),
            )
            .into_any_element()
    })
}

fn ai_ranking_row(
    app_entity: gpui::Entity<CoduxApp>,
    row: codux_runtime::ai_history::AIHistoryRankRow,
    track_surface: gpui::Hsla,
) -> impl IntoElement {
    let value_label = compact_number(row.value);
    let tooltip = format!("{} · {} tokens", row.label, value_label);
    codux_tooltip_container(
        app_entity,
        SharedString::from(format!("ai-ranking-row-{}", row.label)),
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
                    .child(row.label),
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
                                (row.percent.clamp(0.0, 1.0) * 100.0).round() as i64
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
                    .w(gpui::relative(row.percent.clamp(0.0, 1.0)))
                    .rounded(px(4.0))
                    .bg(color(theme::ACCENT))
                    .opacity(if row.value > 0 { 1.0 } else { 0.35 }),
            ),
    )
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
