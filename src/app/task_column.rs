use gpui_component::{InteractiveElementExt as _, menu::ContextMenuExt as _};

use super::ai_runtime_status::AIActivityState;
use super::{formatting::relative_time_label_for_language, *};

pub(in crate::app) struct TaskColumnView {
    pub(in crate::app) app_entity: gpui::Entity<CoduxApp>,
}

impl Render for TaskColumnView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app_entity
            .update(cx, |app, cx| app.task_column(cx).into_any_element())
    }
}

impl CoduxApp {
    pub(in crate::app) fn task_column_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<TaskColumnView> {
        if let Some(view) = &self.task_column_view {
            return view.clone();
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| TaskColumnView {
            app_entity: app_entity.clone(),
        });
        self.task_column_view = Some(view.clone());
        view
    }
}

#[derive(Clone)]
struct TaskColumnLabels {
    language: String,
    no_project: String,
    sessions: String,
    changed_format: String,
    open: String,
    delete: String,
    cancel: String,
    delete_confirm_format: String,
}

fn task_column_labels(language: &str) -> TaskColumnLabels {
    let locale = locale_from_language_setting(language);
    let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
    TaskColumnLabels {
        language: language.to_string(),
        no_project: tr("files.panel.no_project", "No project selected"),
        sessions: tr("ai.sessions.history", "Session History"),
        changed_format: tr("worktree.sidebar.changed_format", "%@ changed"),
        open: tr("common.open", "Open"),
        delete: tr("common.delete", "Delete"),
        cancel: tr("common.cancel", "Cancel"),
        delete_confirm_format: tr("ai.sessions.delete_confirm_format", "Delete %@?"),
    }
}

impl CoduxApp {
    pub(super) fn task_column(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let labels = task_column_labels(&self.state.settings.language);
        let project_name = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_else(|| labels.no_project.clone());

        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .h_full()
            .bg(color(theme::BG_COLUMN))
            .child(column_header(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .child(
                        div()
                            .text_sm()
                            .text_color(color(theme::TEXT))
                            .truncate()
                            .child(project_name),
                    )
                    .child(header_icon_button(
                        "task-refresh",
                        IconName::Redo2,
                        cx,
                        |app, _event, window, cx| {
                            app.reload_worktrees(window, cx);
                            app.reload_ai_history(window, cx);
                            app.reload_project_git(window, cx);
                        },
                    )),
                cx,
            ))
            .child(
                v_resizable("task-column-resizable")
                    .child(
                        resizable_panel()
                            .size(px(320.0))
                            .size_range(px(180.0)..px(560.0))
                            .child(self.task_list_area(&labels, cx)),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(180.0)..px(640.0))
                            .child(self.recent_session_area(&labels, cx)),
                    ),
            )
    }

    fn task_list_area(
        &self,
        labels: &TaskColumnLabels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let rows = Rc::new(self.state.worktrees.worktrees.clone());
        let selected_worktree_id = self.state.worktrees.selected_worktree_id.clone();
        let activity_by_worktree = self
            .state
            .worktrees
            .worktrees
            .iter()
            .map(|worktree| (worktree.id.clone(), self.ai_activity_for_worktree(worktree)))
            .collect::<HashMap<_, _>>();
        let scroll_handle = self.task_scroll_handle.clone();
        let changed_format = labels.changed_format.clone();
        div().flex().flex_col().size_full().min_h_0().child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .p_3()
                .overflow_hidden()
                .child(codux_uniform_list(
                    "task-column-worktrees",
                    rows,
                    scroll_handle,
                    None,
                    cx,
                    move |worktree, _index, _window, cx| {
                        let active = selected_worktree_id
                            .as_deref()
                            .map(|id| id == worktree.id)
                            .unwrap_or(false);
                        let activity_state = activity_by_worktree
                            .get(worktree.id.as_str())
                            .copied()
                            .unwrap_or(AIActivityState::Idle);
                        div()
                            .w_full()
                            .pb(px(4.0))
                            .child(worktree_compact_row(
                                worktree,
                                active,
                                activity_state,
                                changed_format.clone(),
                                cx,
                            ))
                            .into_any_element()
                    },
                )),
        )
    }

    fn recent_session_area(
        &self,
        labels: &TaskColumnLabels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let delete_confirm_session = self
            .ai_session_delete_confirm_id
            .as_deref()
            .and_then(|id| {
                self.state
                    .ai_history
                    .sessions
                    .iter()
                    .find(|session| session.id == id)
            })
            .cloned();

        let sessions = Rc::new(self.state.ai_history.sessions.clone());
        let selected_session_id = self.selected_ai_session_id.clone();
        let scroll_handle = self.session_scroll_handle.clone();
        let row_labels = labels.clone();
        let overlay_labels = labels.clone();

        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(session_section_heading(
                labels.sessions.clone(),
                self.state.ai_history.sessions.len(),
                cx,
            ))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .p_2()
                    .overflow_hidden()
                    .child(codux_uniform_list(
                        "task-column-recent-sessions",
                        sessions,
                        scroll_handle,
                        None,
                        cx,
                        move |session, _index, _window, cx| {
                            let active = selected_session_id
                                .as_deref()
                                .map(|id| id == session.id)
                                .unwrap_or(false);
                            div()
                                .w_full()
                                .pb(px(4.0))
                                .child(ai_session_compact_row(
                                    session,
                                    active,
                                    row_labels.clone(),
                                    cx,
                                ))
                                .into_any_element()
                        },
                    )),
            )
            .when_some(delete_confirm_session, |this, session| {
                this.child(ai_session_delete_confirm_overlay(
                    session,
                    overlay_labels,
                    cx,
                ))
            })
    }
}

fn session_section_heading(
    title: String,
    count: usize,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .h(px(40.0))
        .px_3()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_between()
        .border_t_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().list_head)
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(cx.theme().foreground)
                .child(title),
        )
        .child(Tag::secondary().rounded_full().child(count.to_string()))
}

fn worktree_compact_row(
    worktree: WorktreeInfo,
    active: bool,
    activity_state: AIActivityState,
    changed_format: String,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let worktree_id = worktree.id.clone();
    let activity_dismiss_id = if worktree.is_default {
        worktree.project_id.clone()
    } else {
        worktree.id.clone()
    };
    let git = worktree.git_summary.clone();
    let title = worktree_row_title(&worktree);
    div()
        .id(SharedString::from(format!(
            "compact-worktree-{}",
            worktree.id
        )))
        .w_full()
        .min_w_0()
        .rounded(px(8.0))
        .px_4()
        .py_1()
        .flex()
        .items_center()
        .gap_4()
        .when(active, |this| this.bg(cx.theme().secondary_hover))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_click(cx.listener(move |app, _event, window, cx| {
            if activity_state == AIActivityState::Done {
                app.dismiss_worktree_ai_completion(&activity_dismiss_id, cx);
            }
            app.select_worktree(worktree_id.clone(), window, cx)
        }))
        .child(worktree_activity_dot(activity_state))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT_DIM))
                                .truncate()
                                .child(changed_format.replace("%@", &git.changes.to_string())),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .child(
                                    div()
                                        .text_color(color(0x3EE66B))
                                        .child(format!("+{}", git.additions.max(0))),
                                )
                                .child(
                                    div()
                                        .text_color(color(0xFF5C68))
                                        .child(format!("-{}", git.deletions.max(0))),
                                ),
                        ),
                ),
        )
}

fn worktree_activity_dot(state: AIActivityState) -> AnyElement {
    match state {
        AIActivityState::Idle => div()
            .w(px(10.0))
            .h(px(10.0))
            .rounded_full()
            .flex_shrink_0()
            .bg(color(theme::ACCENT))
            .into_any_element(),
        AIActivityState::Running => div()
            .w(px(10.0))
            .h(px(10.0))
            .flex_shrink_0()
            .rounded_full()
            .border_1()
            .border_color(color(0xFFFFFF))
            .bg(color(theme::ORANGE))
            .into_any_element(),
        AIActivityState::Review => div()
            .w(px(10.0))
            .h(px(10.0))
            .flex_shrink_0()
            .rounded_full()
            .border_2()
            .border_color(color(theme::ORANGE))
            .into_any_element(),
        AIActivityState::Done => div()
            .w(px(10.0))
            .h(px(10.0))
            .rounded_full()
            .flex_shrink_0()
            .border_1()
            .border_color(color(0xFFFFFF))
            .bg(color(theme::GREEN))
            .into_any_element(),
    }
}

fn worktree_row_title(worktree: &WorktreeInfo) -> String {
    let branch = if worktree.branch.trim().is_empty() {
        "main"
    } else {
        worktree.branch.trim()
    };
    if worktree.is_default {
        return branch.to_string();
    }

    let name = worktree.name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    branch
        .split('/')
        .filter(|segment| !segment.is_empty())
        .next_back()
        .unwrap_or(branch)
        .to_string()
}

fn ai_session_compact_row(
    session: AISessionSummary,
    active: bool,
    labels: TaskColumnLabels,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let session_id = session.id.clone();
    let restore_session_id = session.id.clone();
    let right_click_session_id = session.id.clone();
    let menu_session_id = session.id.clone();
    let app_entity = cx.entity();
    let last_seen = relative_time_label_for_language(session.last_seen_at, &labels.language);
    div()
        .id(SharedString::from(format!(
            "compact-session-{}",
            session.id
        )))
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .rounded(px(8.0))
        .px_2()
        .py_2()
        .when(active, |this| this.bg(cx.theme().secondary_hover))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_ai_session(session_id.clone(), window, cx)
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, _event, window, cx| {
                app.select_ai_session(right_click_session_id.clone(), window, cx)
            }),
        )
        .on_double_click(cx.listener(move |app, _event, window, cx| {
            app.selected_ai_session_id = Some(restore_session_id.clone());
            app.restore_selected_ai_session(window, cx);
        }))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_sm()
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(session.title),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(color(theme::TEXT_DIM))
                        .child(last_seen),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .min_w_0()
                .text_xs()
                .text_color(color(theme::TEXT_DIM))
                .child(div().min_w_0().flex_1().truncate().child(session.source))
                .child(
                    div()
                        .flex_shrink_0()
                        .child(compact_number(session.total_tokens)),
                ),
        )
        .context_menu(move |menu, _window, _cx| {
            let open_entity = app_entity.clone();
            let open_session_id = menu_session_id.clone();
            let remove_entity = app_entity.clone();
            let remove_session_id = menu_session_id.clone();

            menu.item(
                PopupMenuItem::new(labels.open.clone())
                    .icon(IconName::SquareTerminal)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&open_entity, |app, cx| {
                            app.selected_ai_session_id = Some(open_session_id.clone());
                            app.restore_selected_ai_session(window, cx);
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(labels.delete.clone())
                    .icon(IconName::Delete)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&remove_entity, |app, cx| {
                            app.request_remove_ai_session(remove_session_id.clone(), window, cx);
                        });
                    }),
            )
        })
}

fn ai_session_delete_confirm_overlay(
    session: AISessionSummary,
    labels: TaskColumnLabels,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .absolute()
        .top(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .left(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.theme().overlay)
        .p(px(16.0))
        .child(
            div()
                .w(px(250.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(color(theme::BORDER_SOFT))
                .bg(color(theme::BG_PANEL))
                .p(px(14.0))
                .shadow_lg()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Icon::new(IconName::Delete)
                                .size_4()
                                .text_color(color(theme::ORANGE)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .truncate()
                                .child(labels.delete.clone()),
                        ),
                )
                .child(
                    div()
                        .mt(px(10.0))
                        .text_size(px(12.0))
                        .line_height(px(18.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(labels.delete_confirm_format.replace("%@", &session.title)),
                )
                .child(
                    div()
                        .mt(px(14.0))
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("ai-session-delete-cancel")
                                .compact()
                                .ghost()
                                .text_color(cx.theme().secondary_foreground)
                                .label(labels.cancel)
                                .on_click(cx.listener(|app, _event, _window, cx| {
                                    app.cancel_remove_ai_session(cx)
                                })),
                        )
                        .child(
                            Button::new("ai-session-delete-confirm")
                                .compact()
                                .primary()
                                .text_color(cx.theme().primary_foreground)
                                .label(labels.delete)
                                .on_click(cx.listener(|app, _event, window, cx| {
                                    app.confirm_remove_ai_session(window, cx)
                                })),
                        ),
                ),
        )
}
