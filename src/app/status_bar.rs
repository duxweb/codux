use super::*;

impl CoduxApp {
    pub(super) fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(28.0))
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
            .font_weight(FontWeight::SEMIBOLD)
            .child(div().flex().items_center().gap_1().when(
                self.state.settings.developer_hud,
                |this| {
                    this.child(status_metric(
                        IconName::ChartPie,
                        "CPU",
                        self.state.performance.cpu_label.clone(),
                    ))
                    .child(status_separator())
                    .child(status_metric(
                        IconName::GalleryVerticalEnd,
                        "MEM",
                        self.state.performance.memory_label.clone(),
                    ))
                    .child(status_separator())
                    .child(status_metric(
                        IconName::Frame,
                        "GPU",
                        self.state.performance.gpu_label.clone(),
                    ))
                },
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(status_ai_segment(
                        self.state.ai_runtime_state.running_count,
                        self.state.ai_history.indexed,
                        self.state.ai_history.is_loading,
                        cx,
                    ))
                    .child(status_separator())
                    .child(status_memory_segment(
                        self.state.memory_manager.extraction.queued,
                        self.state.memory_manager.extraction.running,
                        cx,
                    ))
                    .child(status_separator())
                    .child(status_remote_segment(&self.state.remote, cx))
                    .child(status_separator())
                    .child(status_action_button(
                        IconName::Search,
                        "Index",
                        "status-index",
                        cx,
                        |app, _event, window, cx| app.process_memory_sessions_now(window, cx),
                    ))
                    .child(status_separator())
                    .child(status_git_segment(
                        &self.state.git.branch,
                        self.state.git.ahead,
                        self.state.git.behind,
                        cx,
                    ))
                    .child(status_sync_action_button(
                        IconName::ArrowDown,
                        "拉取",
                        self.state.git.behind,
                        0x6AA1FF,
                        "status-pull",
                        cx,
                        |app, _event, window, cx| app.pull_project_git(window, cx),
                    ))
                    .child(status_sync_action_button(
                        IconName::ArrowUp,
                        "推送",
                        self.state.git.ahead,
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
    label: &'static str,
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
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color(theme::TEXT_MUTED))
        .cursor_pointer()
        .hover(|style| style.bg(color(0xFFFFFF).opacity(0.10)))
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
    label: &'static str,
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
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color(theme::TEXT_MUTED))
        .cursor_pointer()
        .hover(|style| style.bg(color(0xFFFFFF).opacity(0.10)))
        .on_click(cx.listener(on_click))
        .child(leading)
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT))
                .child(label),
        )
}

fn status_metric(icon: IconName, label: &'static str, value: String) -> impl IntoElement {
    div()
        .id("status-remote-settings")
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
    indexed: bool,
    is_indexing: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let running_color = if running_count > 0 {
        theme::GREEN
    } else {
        theme::TEXT_DIM
    };
    let index_color = if indexed {
        theme::GREEN
    } else if is_indexing {
        theme::ORANGE
    } else {
        theme::TEXT_DIM
    };
    let index_label = if indexed {
        "已索引"
    } else if is_indexing {
        "索引中"
    } else {
        "未索引"
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
        .hover(|style| style.bg(color(0xFFFFFF).opacity(0.10)))
        .on_click(cx.listener(|app, _event, window, cx| {
            app.toggle_assistant_panel(AssistantPanel::AIStats, window, cx)
        }))
        .child(Icon::new(IconName::Bot).size_2p5())
        .child(div().mt(px(1.0)).text_color(color(theme::TEXT)).child("AI"))
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(running_color))
                .child(format!("{} 运行", running_count)),
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
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let queued = queued.max(0);
    let running = running.max(0);
    let queued_color = if queued > 0 {
        theme::ORANGE
    } else {
        theme::TEXT_DIM
    };
    let running_color = if running > 0 {
        theme::GREEN
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
        .hover(|style| style.bg(color(0xFFFFFF).opacity(0.10)))
        .on_click(cx.listener(|app, _event, window, cx| app.open_memory_manager_window(window, cx)))
        .child(Icon::new(IconName::BookOpen).size_2p5())
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT))
                .child("记忆"),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(queued_color))
                .child(format!("{} 等待", queued)),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(running_color))
                .child(format!("{} 索引中", running)),
        )
}

fn status_remote_segment(remote: &RemoteSummary, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    let connected = remote.enabled && (remote.online_devices > 0 || remote.status == "connected");
    let state_label = if connected {
        "已连接"
    } else if remote.enabled {
        "连接中"
    } else {
        "未连接"
    };
    let state_color = if connected {
        theme::GREEN
    } else if remote.enabled {
        theme::ORANGE
    } else {
        theme::TEXT_DIM
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
        .hover(|style| style.bg(color(0xFFFFFF).opacity(0.10)))
        .on_click(
            cx.listener(|app, _event, window, cx| app.open_remote_settings_window(window, cx)),
        )
        .child(Icon::new(IconName::Globe).size_2p5())
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT))
                .child("远程"),
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
        .hover(|style| style.bg(color(0xFFFFFF).opacity(0.10)))
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
