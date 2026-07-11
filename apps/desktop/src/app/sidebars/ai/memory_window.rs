use super::memory_rows::*;
use super::*;

pub(in crate::app) struct MemoryManagerWindowInput<'a> {
    pub(in crate::app) manager: &'a MemoryManagerSnapshot,
    pub(in crate::app) active_tab: MemoryManagerTab,
    pub(in crate::app) selected_scope: &'a str,
    pub(in crate::app) selected_project_id: Option<&'a str>,
    pub(in crate::app) selected_memory_entry_id: Option<&'a str>,
    pub(in crate::app) selected_memory_summary_id: Option<&'a str>,
    pub(in crate::app) memory_processing: bool,
    pub(in crate::app) memory_refreshing: bool,
    pub(in crate::app) project_profile_refreshing: bool,
    pub(in crate::app) language: &'a str,
}

pub(in crate::app) fn memory_manager_window_workspace(
    input: MemoryManagerWindowInput<'_>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let MemoryManagerWindowInput {
        manager,
        active_tab,
        selected_scope,
        selected_project_id,
        selected_memory_entry_id,
        selected_memory_summary_id,
        memory_processing,
        memory_refreshing,
        project_profile_refreshing,
        language,
    } = input;
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
        MemoryManagerContentInput {
            manager,
            active_tab,
            is_project_scope: selected_scope == "project",
            selected_memory_entry_id,
            selected_memory_summary_id,
            project_profile_refreshing,
            empty_entries,
            empty_summary,
            empty_failed,
            language,
        },
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
pub(super) fn ai_memory_card(cx: &mut Context<CoduxApp>) -> gpui::Div {
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
pub(super) fn ai_memory_stat_chip(
    value: impl Into<String>,
    label: impl Into<String>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .flex()
        .items_baseline()
        .gap(px(5.0))
        .rounded(px(6.0))
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
pub(super) fn ai_memory_nav_row(
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
pub(super) fn ai_memory_group_label(
    label: impl Into<String>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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
pub(super) fn ai_memory_overview_strip(
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

pub(super) fn ai_memory_overview_text(text: String, cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .truncate()
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
}

pub(super) fn ai_memory_manager_status_bar(
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
            this.child(div().size(px(8.0)).rounded_full().bg(color(theme::RED)))
        })
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_color(color(if error.is_some() {
                    theme::RED
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
                    .text_color(color(theme::RED))
                    .child(format!(
                        "{failed} {}",
                        ai_sidebar_text(language, "memory.status.short_failed", "Failed")
                    )),
            )
        })
}

struct MemoryManagerContentInput<'a> {
    manager: &'a MemoryManagerSnapshot,
    active_tab: MemoryManagerTab,
    is_project_scope: bool,
    selected_memory_entry_id: Option<&'a str>,
    selected_memory_summary_id: Option<&'a str>,
    project_profile_refreshing: bool,
    empty_entries: String,
    empty_summary: String,
    empty_failed: String,
    language: &'a str,
}

fn ai_memory_manager_window_content(
    input: MemoryManagerContentInput<'_>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let MemoryManagerContentInput {
        manager,
        active_tab,
        is_project_scope,
        selected_memory_entry_id,
        selected_memory_summary_id,
        project_profile_refreshing,
        empty_entries,
        empty_summary,
        empty_failed,
        language,
    } = input;
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

pub(super) fn ai_memory_manager_section_switcher(
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
