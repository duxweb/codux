use super::*;

pub(in crate::app) struct StatusBarView {
    app_entity: gpui::Entity<CoduxApp>,
}

impl StatusBarView {
    pub(in crate::app) fn new(app_entity: gpui::Entity<CoduxApp>) -> Self {
        Self { app_entity }
    }
}

impl Render for StatusBarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app_entity
            .update(cx, |app, cx| app.status_bar(cx).into_any_element())
    }
}

impl CoduxApp {
    pub(in crate::app) fn status_bar_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<StatusBarView> {
        if let Some(view) = &self.status_bar_view {
            return view.clone();
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| StatusBarView::new(app_entity));
        self.status_bar_view = Some(view.clone());
        view
    }
}

impl CoduxApp {
    pub(super) fn status_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language.clone();
        let now = app_now_seconds();
        let developer_hud = self.state.settings.developer_hud;
        let cpu_label = self.state.performance.cpu_label.clone();
        let memory_label = self.state.performance.memory_label.clone();
        let ai_running_count = self.state.ai_runtime_state.running_count;
        let ai_index_count = self.ai_history_active_index_count;
        let ai_indexed = self.state.ai_global_history.indexed_project_count > 0;
        let ai_error = self.state.ai_global_history.error.clone();
        let ai_is_indexing = ai_index_count > 0;
        let ai_is_foreground_indexing =
            ai_is_indexing && now < self.ai_index_progress_visible_until;
        let memory_queued = self.state.memory_manager.extraction.queued;
        let memory_running = self.state.memory_manager.extraction.running;
        let memory_processing = self.memory_processing;
        let memory_show_processing = now < self.memory_progress_visible_until;
        let remote = self.state.remote.clone();
        let git_branch = self.state.git.branch.clone();
        let git_ahead = self.state.git.ahead;
        let git_behind = self.state.git.behind;

        div()
            .h(px(28.0))
            .w_full()
            .min_w_0()
            .px_2()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(color(theme::BORDER_SOFT))
            .bg(color(theme::STATUS_BAR))
            .text_color(color(theme::TEXT_MUTED))
            .text_xs()
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .items_center()
                    .gap_1()
                    .when(developer_hud, |this| {
                        this.child(status_metric(
                            "status-performance-cpu",
                            IconName::ChartPie,
                            "CPU",
                            cpu_label,
                        ))
                        .child(status_separator())
                        .child(status_metric(
                            "status-performance-memory",
                            IconName::GalleryVerticalEnd,
                            "MEM",
                            memory_label,
                        ))
                        .child(status_separator())
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .child(status_ai_segment(
                        ai_running_count,
                        ai_index_count,
                        ai_indexed,
                        ai_is_indexing,
                        ai_is_foreground_indexing,
                        ai_error.as_deref(),
                        &language,
                        cx,
                    ))
                    .child(status_separator())
                    .child(status_memory_segment(
                        memory_queued,
                        memory_running,
                        memory_processing,
                        memory_show_processing,
                        &language,
                        cx,
                    ))
                    .child(status_separator())
                    .child(status_remote_segment(&remote, &language, cx))
                    .child(status_separator())
                    .child(status_action_button(
                        IconName::Search,
                        status_text(&language, "common.processing", "Index"),
                        "status-index",
                        cx,
                        |app, _event, window, cx| app.process_memory_sessions_now(window, cx),
                    ))
                    .child(status_separator())
                    .child(status_git_segment(&git_branch, git_ahead, git_behind, cx))
                    .child(status_sync_action_button(
                        IconName::ArrowDown,
                        status_text(&language, "git.remote.pull", "Pull"),
                        git_behind,
                        0x6AA1FF,
                        "status-pull",
                        cx,
                        |app, _event, window, cx| app.pull_project_git(window, cx),
                    ))
                    .child(status_sync_action_button(
                        IconName::ArrowUp,
                        status_text(&language, "git.remote.push", "Push"),
                        git_ahead,
                        theme::GREEN,
                        "status-push",
                        cx,
                        |app, _event, window, cx| app.push_project_git(window, cx),
                    )),
            )
    }
}

fn status_action_button(
    icon: IconName,
    label: String,
    id: &'static str,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("status-action-{id}")))
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap_1()
        .rounded_sm()
        .text_xs()
        .text_color(color(theme::TEXT_MUTED))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(on_click))
        .child(Icon::new(icon).size_2p5())
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT))
                .child(label),
        )
}

fn status_sync_action_button(
    icon: IconName,
    label: String,
    count: i64,
    accent: u32,
    id: &'static str,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let count = count.max(0);
    let leading = if count > 0 {
        div()
            .min_w(px(16.0))
            .h(px(16.0))
            .px(px(4.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(color(accent).opacity(0.16))
            .text_color(color(accent))
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .child(count.to_string())
            .into_any_element()
    } else {
        Icon::new(icon).size_2p5().into_any_element()
    };

    div()
        .id(SharedString::from(format!("status-action-{id}")))
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap_1()
        .rounded_sm()
        .text_xs()
        .text_color(color(theme::TEXT_MUTED))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(on_click))
        .child(leading)
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT))
                .child(label),
        )
}

fn status_metric(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    value: String,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_xs()
        .text_color(color(theme::TEXT_MUTED))
        .child(Icon::new(icon).size_2p5())
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT_DIM))
                .child(label),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT))
                .child(value),
        )
}

fn status_ai_segment(
    running_count: usize,
    index_count: usize,
    indexed: bool,
    is_indexing: bool,
    is_foreground_indexing: bool,
    error: Option<&str>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let running_label = status_text(language, "agent.status.running", "Running");
    let index_count_label = status_text(language, "ai.status.index_count", "Index");
    let indexing_label = status_text(language, "ai.indexing.status.short_indexing", "Indexing");
    let indexed_label = status_text(language, "ai.indexing.status.short_indexed", "Indexed");
    let loading = is_indexing;
    let running_color = if running_count > 0 {
        theme::GREEN
    } else {
        theme::TEXT_DIM
    };
    let active_index_color = if index_count > 0 {
        theme::ORANGE
    } else {
        theme::TEXT_DIM
    };
    let index_color = if loading {
        theme::ORANGE
    } else if error.is_some() {
        theme::ORANGE
    } else if indexed {
        0x5F9F70
    } else {
        theme::TEXT_DIM
    };
    let index_label = if loading {
        indexing_label
    } else if indexed {
        indexed_label
    } else {
        indexed_label
    };

    div()
        .id("status-ai-panel")
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_xs()
        .text_color(color(theme::TEXT_MUTED))
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(|app, _event, window, cx| {
            app.toggle_assistant_panel(AssistantPanel::AIStats, window, cx)
        }))
        .child(if loading {
            status_activity_dot(if is_foreground_indexing {
                theme::ACCENT
            } else {
                theme::ORANGE
            })
            .into_any_element()
        } else {
            Icon::new(IconName::Bot).size_2p5().into_any_element()
        })
        .child(div().mt(px(1.0)).text_color(color(theme::TEXT)).child("AI"))
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(running_color))
                .child(format!("{running_count} {running_label}")),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(active_index_color))
                .child(format!("{index_count} {index_count_label}")),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(index_color))
                .child(index_label),
        )
}

fn status_memory_segment(
    queued: i64,
    running: i64,
    processing: bool,
    show_processing: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let running_label = status_text(language, "agent.status.running", "Running");
    let index_count_label = status_text(language, "ai.status.index_count", "Index");
    let indexed_label = status_text(language, "ai.indexing.status.short_indexed", "Indexed");
    let indexing_label = status_text(language, "memory.status.short_indexing", "Indexing");
    let queued = queued.max(0);
    let running = running.max(0);
    let total_queued = queued + running;
    let loading = processing || running > 0;
    let visible_loading = loading || show_processing;
    let queued_color = theme::TEXT_DIM;
    let running_color = if running > 0 {
        theme::ORANGE
    } else {
        theme::TEXT_DIM
    };

    div()
        .id("status-memory-panel")
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_xs()
        .text_color(color(theme::TEXT_MUTED))
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(|app, _event, window, cx| app.open_memory_manager_window(window, cx)))
        .child(if visible_loading {
            status_activity_dot(theme::ORANGE).into_any_element()
        } else {
            Icon::new(IconName::BookOpen).size_2p5().into_any_element()
        })
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT))
                .child(status_text(
                    language,
                    "memory.manager.window.title",
                    "Memory",
                )),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(running_color))
                .child(format!("{running} {running_label}")),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(queued_color))
                .child(format!("{total_queued} {index_count_label}")),
        )
        .when(visible_loading, |this| {
            this.child(
                div()
                    .mt(px(1.0))
                    .text_color(color(theme::ORANGE))
                    .child(indexing_label),
            )
        })
        .when(!visible_loading, |this| {
            this.child(
                div()
                    .mt(px(1.0))
                    .text_color(color(0x5F9F70))
                    .child(indexed_label),
            )
        })
}

fn status_activity_dot(accent: u32) -> impl IntoElement {
    div()
        .size(px(9.0))
        .rounded_full()
        .border_1()
        .border_color(color(0xFFFFFF))
        .bg(color(accent))
}

fn status_remote_segment(
    remote: &RemoteSummary,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let remote_label = status_text(language, "settings.tab.remote", "Remote");
    let connected_label = status_text(language, "remote.status.connected_label", "Connected");
    let connecting_label = status_text(language, "remote.status.connecting_label", "Connecting");
    let disconnected_label =
        status_text(language, "remote.status.disconnected_label", "Disconnected");
    let state_label = match remote.status.as_str() {
        "connected" => connected_label,
        "connecting" => connecting_label,
        _ => disconnected_label,
    };
    let state_color = match remote.status.as_str() {
        "connected" => theme::GREEN,
        "connecting" => theme::ORANGE,
        _ => theme::TEXT_DIM,
    };

    div()
        .id("status-remote-settings")
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_xs()
        .text_color(color(theme::TEXT_MUTED))
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(
            cx.listener(|app, _event, window, cx| app.open_remote_settings_window(window, cx)),
        )
        .child(Icon::new(IconName::Globe).size_2p5())
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT))
                .child(remote_label),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(state_color))
                .child(state_label),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT_DIM))
                .child(format!("{}/{}", remote.online_devices, remote.devices)),
        )
}

fn status_text(language: &str, key: &str, fallback: &str) -> String {
    translate(&locale_from_language_setting(language), key, fallback)
}

fn status_git_segment(
    branch: &str,
    ahead: i64,
    behind: i64,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .id("status-git-panel")
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_xs()
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(|app, _event, window, cx| {
            app.toggle_assistant_panel(AssistantPanel::Git, window, cx)
        }))
        .child(Icon::new(IconName::Github).size_2p5())
        .child(
            div()
                .mt(px(1.0))
                .max_w(px(180.0))
                .truncate()
                .text_color(color(theme::TEXT))
                .child(branch.to_string()),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::GREEN))
                .child(format!("+{}", ahead.max(0))),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(0xF47C7C))
                .child(format!("-{}", behind.max(0))),
        )
}

fn status_separator() -> impl IntoElement {
    div()
        .h(px(16.0))
        .w(px(1.0))
        .mx_1()
        .bg(color(theme::BORDER_SOFT))
}
