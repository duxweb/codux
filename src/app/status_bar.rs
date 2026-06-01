use super::*;

pub(in crate::app) struct StatusBarView {
    app_entity: gpui::Entity<CoduxApp>,
}

#[derive(Clone)]
struct StatusGitSummary {
    branch: String,
    incoming: i64,
    outgoing: i64,
    additions: i64,
    deletions: i64,
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

    pub(in crate::app) fn notify_status_bar(&mut self, cx: &mut Context<Self>) {
        self.invalidate_status_bar(cx);
    }
}

impl CoduxApp {
    pub(super) fn status_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.state.settings.language.clone();
        let developer_hud = self.state.settings.developer_hud;
        let cpu_label = self.state.performance.cpu_label.clone();
        let memory_label = self.state.performance.memory_label.clone();
        let ai_index_count = self.ai_history_active_index_count;
        let ai_error = self.state.ai_global_history.error.clone();
        let memory_queued = self.state.memory_manager.extraction.queued;
        let memory_running = self.state.memory_manager.extraction.running;
        let memory_failed = self.state.memory_manager.extraction.failed;
        let memory_error = self.state.memory_manager.extraction.last_error.clone();
        let remote = self.state.remote.clone();
        let git = self.status_git_summary();

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
                        this.child(status_metric("status-performance-cpu", "CPU", cpu_label))
                            .child(status_separator())
                            .child(status_metric(
                                "status-performance-memory",
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
                        ai_index_count,
                        ai_error.as_deref(),
                        &language,
                        cx,
                    ))
                    .child(status_separator())
                    .child(status_memory_segment(
                        memory_queued,
                        memory_running,
                        memory_failed,
                        memory_error.as_deref(),
                        &language,
                        cx,
                    ))
                    .child(status_separator())
                    .child(status_remote_segment(&remote, &language, cx))
                    .child(status_separator())
                    .child(status_git_segment(
                        &git.branch,
                        git.additions,
                        git.deletions,
                        cx,
                    ))
                    .child(status_sync_action_button(
                        status_text(&language, "git.remote.pull", "Pull"),
                        git.incoming,
                        0x6AA1FF,
                        "status-pull",
                        cx,
                        |app, _event, window, cx| app.pull_project_git(window, cx),
                    ))
                    .child(status_sync_action_button(
                        status_text(&language, "git.remote.push", "Push"),
                        git.outgoing,
                        theme::GREEN,
                        "status-push",
                        cx,
                        |app, _event, window, cx| app.push_project_git(window, cx),
                    )),
            )
    }

    fn status_git_summary(&self) -> StatusGitSummary {
        if let Some(worktree) = super::ai_runtime_status::selected_worktree_info(&self.state) {
            let git = worktree.git_summary;
            return StatusGitSummary {
                branch: non_empty(worktree.branch).unwrap_or_else(|| self.state.git.branch.clone()),
                incoming: git.incoming,
                outgoing: git.outgoing,
                additions: git.additions,
                deletions: git.deletions,
            };
        }

        let additions = self
            .git_review
            .files
            .iter()
            .map(|file| file.additions)
            .sum();
        let deletions = self
            .git_review
            .files
            .iter()
            .map(|file| file.deletions)
            .sum();

        StatusGitSummary {
            branch: self.state.git.branch.clone(),
            incoming: self.state.git.behind,
            outgoing: self.state.git.ahead,
            additions,
            deletions,
        }
    }
}

fn non_empty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn status_sync_action_button(
    label: String,
    count: i64,
    accent: u32,
    id: &'static str,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let count = count.max(0);
    let label_color = if count > 0 { accent } else { theme::TEXT };

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
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(label_color))
                .child(label),
        )
}

fn status_metric(id: &'static str, label: &'static str, value: String) -> impl IntoElement {
    div()
        .id(id)
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_xs()
        .text_color(color(theme::TEXT_MUTED))
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
    index_count: usize,
    error: Option<&str>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let index_count_label = status_text(language, "ai.status.index_count", "Index");
    let index_color = if error.is_some() {
        0xF47C7C
    } else if index_count > 0 {
        theme::ORANGE
    } else {
        theme::TEXT_DIM
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
        .child(div().mt(px(1.0)).text_color(color(theme::TEXT)).child("AI"))
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(index_color))
                .child(format!("{index_count} {index_count_label}")),
        )
}

fn status_memory_segment(
    queued: i64,
    running: i64,
    failed: i64,
    _error: Option<&str>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let queued_label = status_text(language, "memory.status.short_queued", "Queued");
    let memory_label = status_text(language, "memory.status.short_memory", "Memory");
    let failed_label = status_text(language, "memory.status.short_failed", "Failed");
    let queued = queued.max(0);
    let running = running.max(0);
    let failed = failed.max(0);
    let queued_color = if queued > 0 {
        theme::ORANGE
    } else {
        theme::TEXT_DIM
    };
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
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT))
                .child(status_text(
                    language,
                    "memory.status.short_memory",
                    "Memory",
                )),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(queued_color))
                .child(format!("{queued} {queued_label}")),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(running_color))
                .child(format!("{running} {memory_label}")),
        )
        .when(failed > 0, |this| {
            this.child(
                div()
                    .mt(px(1.0))
                    .text_color(color(0xF47C7C))
                    .child(format!("{failed} {failed_label}")),
            )
        })
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
    additions: i64,
    deletions: i64,
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
                .child(format!("+{}", additions.max(0))),
        )
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(0xF47C7C))
                .child(format!("-{}", deletions.max(0))),
        )
}

fn status_separator() -> impl IntoElement {
    div()
        .h(px(16.0))
        .w(px(1.0))
        .mx_1()
        .bg(color(theme::BORDER_SOFT))
}
