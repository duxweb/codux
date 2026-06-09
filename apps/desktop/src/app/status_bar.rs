use super::*;

pub(in crate::app) struct StatusBarView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: StatusBarSnapshot,
}

#[derive(Clone, PartialEq)]
struct StatusGitSummary {
    branch: String,
    is_repository: bool,
    incoming: i64,
    outgoing: i64,
    additions: i64,
    deletions: i64,
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct StatusBarSnapshot {
    language: String,
    theme_is_light: bool,
    developer_hud: bool,
    runtime_ready: bool,
    runtime_queue_busy: bool,
    cpu_label: String,
    memory_label: String,
    ai_index_count: usize,
    ai_error: Option<String>,
    memory_queued: i64,
    memory_running: i64,
    memory_failed: i64,
    memory_error: Option<String>,
    remote_status: String,
    remote_devices: usize,
    remote_online_devices: usize,
    git: StatusGitSummary,
    git_running_label: Option<String>,
}

impl StatusBarView {
    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: StatusBarSnapshot,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.snapshot == snapshot {
            return false;
        }
        self.snapshot = snapshot;
        cx.notify();
        true
    }
}

impl Render for StatusBarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        status_bar_content(self.app_entity.clone(), self.snapshot.clone(), cx).into_any_element()
    }
}

impl CoduxApp {
    pub(in crate::app) fn status_bar_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<StatusBarView> {
        let snapshot = self.status_bar_snapshot();
        if let Some(view) = &self.status_bar_view {
            view.update(cx, |view, cx| {
                view.set_snapshot(snapshot, cx);
            });
            return view.clone();
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| StatusBarView {
            app_entity,
            snapshot,
        });
        self.status_bar_view = Some(view.clone());
        view
    }
}

impl CoduxApp {
    pub(in crate::app) fn status_bar_snapshot(&self) -> StatusBarSnapshot {
        StatusBarSnapshot {
            language: self.state.settings.language.clone(),
            theme_is_light: theme::terminal_theme_palette_for_appearance(
                &self.state.settings.theme,
                self.window_appearance,
            )
            .is_light,
            developer_hud: self.state.settings.developer_hud,
            runtime_ready: self.runtime_ready,
            runtime_queue_busy: {
                let status = codux_runtime::async_runtime::blocking_queue_status();
                status.queued > 0 || status.running > 0
            },
            cpu_label: self.state.performance.cpu_label.clone(),
            memory_label: self.state.performance.memory_label.clone(),
            ai_index_count: self.ai_history_active_index_count,
            ai_error: self.state.ai_global_history.error.clone(),
            memory_queued: self.state.memory_manager.extraction.queued,
            memory_running: self.state.memory_manager.extraction.running,
            memory_failed: (self.state.memory_manager.extraction.failed
                - self.memory_status_seen_failed_count)
                .max(0),
            memory_error: self.state.memory_manager.extraction.last_error.clone(),
            remote_status: self.state.remote.status.clone(),
            remote_devices: self.state.remote.devices,
            remote_online_devices: self.state.remote.online_devices,
            git: self.status_git_summary(),
            git_running_label: self
                .git_running_operation
                .as_ref()
                .map(|operation| operation.label.clone()),
        }
    }

    fn status_git_summary(&self) -> StatusGitSummary {
        if let Some(worktree) = super::ai_runtime_status::selected_worktree_info(&self.state) {
            let git = worktree.git_summary;
            return StatusGitSummary {
                branch: if self.state.git.is_repository {
                    non_empty(worktree.branch).unwrap_or_else(|| self.state.git.branch.clone())
                } else {
                    String::new()
                },
                is_repository: self.state.git.is_repository,
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
            is_repository: self.state.git.is_repository,
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

fn status_bar_content(
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: StatusBarSnapshot,
    cx: &mut Context<StatusBarView>,
) -> impl IntoElement {
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
        .text_size(rems(0.75))
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap_1()
                .child(status_runtime_ready_segment(
                    snapshot.runtime_ready,
                    snapshot.runtime_queue_busy,
                    &snapshot.language,
                ))
                .child(status_separator())
                .when(snapshot.developer_hud, |this| {
                    this.child(status_metric(
                        "status-performance-cpu",
                        "CPU",
                        snapshot.cpu_label.clone(),
                    ))
                    .child(status_separator())
                    .child(status_metric(
                        "status-performance-memory",
                        "MEM",
                        snapshot.memory_label.clone(),
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
                    app_entity.clone(),
                    snapshot.ai_index_count,
                    snapshot.ai_error.as_deref(),
                    &snapshot.language,
                    cx,
                ))
                .child(status_separator())
                .child(status_memory_segment(
                    app_entity.clone(),
                    snapshot.memory_queued,
                    snapshot.memory_running,
                    snapshot.memory_failed,
                    snapshot.memory_error.as_deref(),
                    &snapshot.language,
                    cx,
                ))
                .child(status_separator())
                .child(status_remote_segment(
                    app_entity.clone(),
                    &snapshot.remote_status,
                    snapshot.remote_devices,
                    snapshot.remote_online_devices,
                    &snapshot.language,
                    cx,
                ))
                .child(status_separator())
                .when(snapshot.git.is_repository, |this| {
                    let git_running_label = snapshot.git_running_label.as_deref();
                    let git_operation_running = git_running_label.is_some();
                    let pull_running = git_running_label == Some("pull");
                    let push_running = git_running_label
                        .is_some_and(|label| label == "push" || label.starts_with("push:"));
                    let branch = if snapshot.git.branch.trim().is_empty() {
                        status_text(&snapshot.language, "git.branch.none", "No Branch")
                    } else {
                        snapshot.git.branch.clone()
                    };
                    this.child(status_git_segment(
                        app_entity.clone(),
                        &branch,
                        snapshot.git.additions,
                        snapshot.git.deletions,
                        cx,
                    ))
                    .child(status_sync_action_button(
                        app_entity.clone(),
                        status_text(&snapshot.language, "git.remote.pull", "Pull"),
                        snapshot.git.incoming,
                        snapshot.theme_is_light,
                        0x6AA1FF,
                        "status-pull",
                        pull_running,
                        git_operation_running,
                        cx,
                        |app, _event, window, cx| app.pull_project_git(window, cx),
                    ))
                    .child(status_sync_action_button(
                        app_entity,
                        status_text(&snapshot.language, "git.remote.push", "Push"),
                        snapshot.git.outgoing,
                        snapshot.theme_is_light,
                        theme::GREEN,
                        "status-push",
                        push_running,
                        git_operation_running,
                        cx,
                        |app, _event, window, cx| app.push_project_git(window, cx),
                    ))
                }),
        )
}

fn status_runtime_ready_segment(
    runtime_ready: bool,
    runtime_queue_busy: bool,
    language: &str,
) -> impl IntoElement {
    let label = if !runtime_ready {
        status_text(language, "runtime.status.preparing", "Preparing")
    } else if runtime_queue_busy {
        status_text(language, "runtime.status.busy", "Busy")
    } else {
        status_text(language, "runtime.status.ready", "Ready")
    };
    let color_value = if !runtime_ready || runtime_queue_busy {
        theme::ORANGE
    } else {
        theme::GREEN
    };

    div()
        .id("status-runtime-ready")
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_size(rems(0.75))
        .text_color(color(theme::TEXT_MUTED))
        .child(div().size(px(6.0)).rounded_full().bg(color(color_value)))
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(theme::TEXT))
                .child(label),
        )
}

fn status_sync_action_button(
    app_entity: gpui::Entity<CoduxApp>,
    label: String,
    count: i64,
    theme_is_light: bool,
    accent: u32,
    id: &'static str,
    loading: bool,
    disabled: bool,
    cx: &mut Context<StatusBarView>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let count = count.max(0);
    let count_bg = if theme_is_light { 0xFFFFFF } else { 0x000000 };
    let count_text = if theme_is_light { 0x111111 } else { 0xA8ADB8 };

    div()
        .id(SharedString::from(format!("status-action-{id}")))
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .rounded_sm()
        .text_size(rems(0.75))
        .text_color(color(theme::TEXT_MUTED))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(|style| style.bg(cx.theme().list_hover))
        })
        .when(disabled, |this| this.opacity(0.58))
        .on_click(move |event, window, cx| {
            if disabled {
                cx.stop_propagation();
                return;
            }
            cx.update_entity(&app_entity, |app, cx| on_click(app, event, window, cx));
        })
        .when(loading, |this| {
            this.child(Spinner::new().xsmall().color(color(if count > 0 {
                accent
            } else {
                theme::TEXT_MUTED
            })))
        })
        .when(count > 0, |this| {
            this.child(
                div()
                    .size(px(15.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(theme::fixed_color(count_bg))
                    .text_size(rems(0.625))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::fixed_color(count_text))
                    .child(count.min(99).to_string()),
            )
        })
        .child(div().text_color(color(theme::TEXT)).child(label))
}

fn status_metric(id: &'static str, label: &'static str, value: String) -> impl IntoElement {
    div()
        .id(id)
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_size(rems(0.75))
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
    app_entity: gpui::Entity<CoduxApp>,
    index_count: usize,
    error: Option<&str>,
    language: &str,
    cx: &mut Context<StatusBarView>,
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
        .text_size(rems(0.75))
        .text_color(color(theme::TEXT_MUTED))
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(move |_, window, cx| {
            cx.update_entity(&app_entity, |app, cx| {
                app.toggle_assistant_panel(AssistantPanel::AIStats, window, cx);
            });
        })
        .child(div().mt(px(1.0)).text_color(color(theme::TEXT)).child("AI"))
        .child(
            div()
                .mt(px(1.0))
                .text_color(color(index_color))
                .child(format!("{index_count} {index_count_label}")),
        )
}

fn status_memory_segment(
    app_entity: gpui::Entity<CoduxApp>,
    queued: i64,
    running: i64,
    failed: i64,
    _error: Option<&str>,
    language: &str,
    cx: &mut Context<StatusBarView>,
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
        .text_size(rems(0.75))
        .text_color(color(theme::TEXT_MUTED))
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(move |_, window, cx| {
            cx.update_entity(&app_entity, |app, cx| {
                app.open_memory_manager_window(window, cx);
            });
        })
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
    app_entity: gpui::Entity<CoduxApp>,
    status: &str,
    devices: usize,
    online_devices: usize,
    language: &str,
    cx: &mut Context<StatusBarView>,
) -> impl IntoElement {
    let remote_label = status_text(language, "settings.tab.remote", "Remote");
    let connected_label = status_text(language, "remote.status.connected_label", "Connected");
    let connecting_label = status_text(language, "remote.status.connecting_label", "Connecting");
    let disconnected_label =
        status_text(language, "remote.status.disconnected_label", "Disconnected");
    let state_label = match status {
        "connected" => connected_label,
        "connecting" => connecting_label,
        _ => disconnected_label,
    };
    let state_color = match status {
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
        .text_size(rems(0.75))
        .text_color(color(theme::TEXT_MUTED))
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(move |_, window, cx| {
            cx.update_entity(&app_entity, |app, cx| {
                app.open_remote_settings_window(window, cx);
            });
        })
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
                .child(format!("{online_devices}/{devices}")),
        )
}

fn status_text(language: &str, key: &str, fallback: &str) -> String {
    translate(&locale_from_language_setting(language), key, fallback)
}

fn status_git_segment(
    app_entity: gpui::Entity<CoduxApp>,
    branch: &str,
    additions: i64,
    deletions: i64,
    cx: &mut Context<StatusBarView>,
) -> impl IntoElement {
    div()
        .id("status-git-panel")
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_size(rems(0.75))
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(move |_, window, cx| {
            cx.update_entity(&app_entity, |app, cx| {
                app.toggle_assistant_panel(AssistantPanel::Git, window, cx);
            });
        })
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
