use gpui_component::{InteractiveElementExt as _, menu::ContextMenuExt as _};

use super::ai_runtime_status::AIActivityState;
use super::ui_helpers::{codux_tooltip_container, titlebar_drag_area};
use super::{formatting::relative_time_label_for_language, *};

pub(in crate::app) struct TaskColumnView {
    header_view: gpui::Entity<TaskColumnHeaderView>,
    worktree_list_view: gpui::Entity<TaskWorktreeListView>,
    session_list_view: gpui::Entity<TaskSessionListView>,
}

#[derive(Clone, PartialEq)]
struct TaskWorktreeRow {
    id: String,
    project_id: String,
    title: String,
    path: String,
    is_default: bool,
    active: bool,
    activity_state: AIActivityState,
    git_changes: usize,
    git_additions: i64,
    git_deletions: i64,
}

#[derive(Clone, PartialEq)]
struct TaskSessionRow {
    id: String,
    title: String,
    source: String,
    last_seen_at: f64,
    total_tokens: i64,
}

impl Render for TaskColumnView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        task_column_content(
            self.header_view.clone(),
            self.worktree_list_view.clone(),
            self.session_list_view.clone(),
            cx,
        )
        .into_any_element()
    }
}

impl CoduxApp {
    pub(in crate::app) fn task_column_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<TaskColumnView> {
        if let Some(view) = self.task_column_view.clone() {
            self.update_task_column_child_views(cx);
            return view;
        }
        let header_view = self.task_column_header_view(cx);
        let worktree_list_view = self.task_worktree_list_view(cx);
        let session_list_view = self.task_session_list_view(cx);
        let view = cx.new(|_| TaskColumnView {
            header_view,
            worktree_list_view,
            session_list_view,
        });
        self.task_column_view = Some(view.clone());
        view
    }

    pub(in crate::app) fn update_task_column_child_views(&mut self, cx: &mut Context<Self>) {
        let _ = self.task_column_header_view(cx);
        let _ = self.task_worktree_list_view(cx);
        let _ = self.task_session_list_view(cx);
    }
}

#[derive(Clone, PartialEq)]
struct TaskColumnLabels {
    language: String,
    no_project: String,
    no_branch: String,
    sessions: String,
    changed_format: String,
    create: String,
    refresh: String,
    open: String,
    new_session: String,
    open_folder: String,
    merge: String,
    delete: String,
}

fn task_column_labels(language: &str) -> TaskColumnLabels {
    let locale = locale_from_language_setting(language);
    let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
    TaskColumnLabels {
        language: language.to_string(),
        no_project: tr("files.panel.no_project", "No project selected"),
        no_branch: tr("git.branch.none", "No Branch"),
        sessions: tr("ai.sessions.history", "Session History"),
        changed_format: tr("worktree.sidebar.changed_format", "%@ changed"),
        create: tr("worktree.create.title", "New Worktree"),
        refresh: tr("common.refresh", "Refresh"),
        open: tr("common.open", "Open"),
        new_session: tr("ai.sessions.new_session", "New Session"),
        open_folder: tr("worktree.menu.open_folder", "Open Folder"),
        merge: tr("worktree.menu.merge", "Merge to Mainline"),
        delete: tr("common.delete", "Delete"),
    }
}

fn task_session_row(session: &AISessionSummary) -> TaskSessionRow {
    TaskSessionRow {
        id: session.id.clone(),
        title: session.title.clone(),
        source: session.source.clone(),
        last_seen_at: session.last_seen_at,
        total_tokens: session.total_tokens,
    }
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct TaskColumnHeaderSnapshot {
    project_name: String,
    refreshing: bool,
    create_label: String,
    refresh_label: String,
}

pub(in crate::app) struct TaskColumnHeaderView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: TaskColumnHeaderSnapshot,
}

impl TaskColumnHeaderView {
    fn set_snapshot(&mut self, snapshot: TaskColumnHeaderSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for TaskColumnHeaderView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        task_column_header(
            self.snapshot.project_name.clone(),
            self.snapshot.refreshing,
            self.snapshot.create_label.clone(),
            self.snapshot.refresh_label.clone(),
            self.app_entity.clone(),
            cx,
        )
        .into_any_element()
    }
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct TaskWorktreeListSnapshot {
    labels: TaskColumnLabels,
    worktrees: Vec<TaskWorktreeRow>,
}

pub(in crate::app) struct TaskWorktreeListView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: TaskWorktreeListSnapshot,
    scroll_handle: UniformListScrollHandle,
}

impl TaskWorktreeListView {
    fn set_snapshot(&mut self, snapshot: TaskWorktreeListSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for TaskWorktreeListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        task_list_area(
            self.snapshot.worktrees.clone(),
            self.snapshot.labels.clone(),
            self.scroll_handle.clone(),
            self.app_entity.clone(),
            cx,
        )
        .into_any_element()
    }
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct TaskSessionListSnapshot {
    labels: TaskColumnLabels,
    sessions: Vec<TaskSessionRow>,
}

pub(in crate::app) struct TaskSessionListView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: TaskSessionListSnapshot,
    scroll_handle: UniformListScrollHandle,
}

impl TaskSessionListView {
    fn set_snapshot(&mut self, snapshot: TaskSessionListSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for TaskSessionListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        recent_session_area(
            self.snapshot.sessions.clone(),
            self.snapshot.labels.clone(),
            self.scroll_handle.clone(),
            self.app_entity.clone(),
            cx,
        )
        .into_any_element()
    }
}

impl CoduxApp {
    fn task_column_header_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<TaskColumnHeaderView> {
        let snapshot = self.task_column_header_snapshot();
        if let Some(view) = self.task_column_header_view.clone() {
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view;
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| TaskColumnHeaderView {
            app_entity,
            snapshot,
        });
        self.task_column_header_view = Some(view.clone());
        view
    }

    fn task_worktree_list_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<TaskWorktreeListView> {
        let snapshot = self.task_worktree_list_snapshot();
        if let Some(view) = self.task_worktree_list_view.clone() {
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view;
        }
        let app_entity = cx.entity();
        let scroll_handle = self.task_scroll_handle.clone();
        let view = cx.new(|_| TaskWorktreeListView {
            app_entity,
            snapshot,
            scroll_handle,
        });
        self.task_worktree_list_view = Some(view.clone());
        view
    }

    fn task_session_list_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<TaskSessionListView> {
        let snapshot = self.task_session_list_snapshot();
        if let Some(view) = self.task_session_list_view.clone() {
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view;
        }
        let app_entity = cx.entity();
        let scroll_handle = self.session_scroll_handle.clone();
        let view = cx.new(|_| TaskSessionListView {
            app_entity,
            snapshot,
            scroll_handle,
        });
        self.task_session_list_view = Some(view.clone());
        view
    }

    fn task_column_header_snapshot(&self) -> TaskColumnHeaderSnapshot {
        let labels = task_column_labels(&self.state.settings.language);
        TaskColumnHeaderSnapshot {
            project_name: self
                .state
                .selected_project
                .as_ref()
                .map(|project| project.name.clone())
                .unwrap_or(labels.no_project),
            refreshing: self.task_column_refreshing,
            create_label: labels.create,
            refresh_label: labels.refresh,
        }
    }

    fn task_worktree_list_snapshot(&self) -> TaskWorktreeListSnapshot {
        let labels = task_column_labels(&self.state.settings.language);
        let selected_worktree_id = self.state.worktrees.selected_worktree_id.clone();
        let worktrees = self
            .state
            .worktrees
            .worktrees
            .iter()
            .map(|worktree| {
                let active = selected_worktree_id
                    .as_deref()
                    .map(|id| id == worktree.id)
                    .unwrap_or(false);
                TaskWorktreeRow {
                    id: worktree.id.clone(),
                    project_id: worktree.project_id.clone(),
                    title: worktree_row_title(worktree, &labels.no_branch),
                    path: worktree.path.clone(),
                    is_default: worktree.is_default,
                    active,
                    activity_state: self.ai_activity_for_worktree(worktree),
                    git_changes: worktree.git_summary.changes,
                    git_additions: worktree.git_summary.additions,
                    git_deletions: worktree.git_summary.deletions,
                }
            })
            .collect();

        TaskWorktreeListSnapshot { labels, worktrees }
    }

    fn task_session_list_snapshot(&self) -> TaskSessionListSnapshot {
        let labels = task_column_labels(&self.state.settings.language);
        let sessions = self
            .state
            .ai_history
            .sessions
            .iter()
            .map(task_session_row)
            .collect::<Vec<_>>();

        TaskSessionListSnapshot { labels, sessions }
    }
}

fn task_column_content(
    header_view: gpui::Entity<TaskColumnHeaderView>,
    worktree_list_view: gpui::Entity<TaskWorktreeListView>,
    session_list_view: gpui::Entity<TaskSessionListView>,
    _cx: &mut Context<TaskColumnView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .h_full()
        .min_h_0()
        .bg(color(theme::BG_COLUMN))
        .child(gpui::AnyView::from(header_view))
        .child(
            div().flex_1().min_h_0().overflow_hidden().child(
                v_resizable("task-column-resizable")
                    .child(
                        resizable_panel()
                            .size(px(320.0))
                            .size_range(px(96.0)..px(560.0))
                            .child(gpui::AnyView::from(worktree_list_view)),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(96.0)..px(640.0))
                            .child(gpui::AnyView::from(session_list_view)),
                    ),
            ),
        )
}

fn task_column_header(
    project_name: String,
    refreshing: bool,
    create_label: String,
    refresh_label: String,
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<TaskColumnHeaderView>,
) -> impl IntoElement {
    let create_entity = app_entity.clone();
    let refresh_entity = app_entity.clone();
    div()
        .h(px(44.0))
        .w_full()
        .px(px(10.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().title_bar)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(titlebar_drag_area(
                    "task-column-titlebar-drag",
                    div()
                        .flex_1()
                        .h_full()
                        .flex()
                        .items_center()
                        .text_sm()
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(project_name),
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            codux_tooltip_container(
                                app_entity.clone(),
                                "task-create-tooltip",
                                create_label.clone(),
                            )
                            .child(
                                Button::new("task-create")
                                    .ghost()
                                    .compact()
                                    .text_color(cx.theme().secondary_foreground)
                                    .icon(
                                        Icon::new(HeroIconName::Plus)
                                            .size_3p5()
                                            .text_color(cx.theme().secondary_foreground),
                                    )
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(
                                            &create_entity,
                                            |app: &mut CoduxApp, cx| {
                                                app.open_worktree_creator_window(window, cx);
                                            },
                                        );
                                    }),
                            ),
                        )
                        .child(
                            codux_tooltip_container(
                                app_entity,
                                "task-refresh-tooltip",
                                refresh_label,
                            )
                            .child(
                                Button::new("task-refresh")
                                    .ghost()
                                    .compact()
                                    .loading(refreshing)
                                    .disabled(refreshing)
                                    .text_color(cx.theme().secondary_foreground)
                                    .icon(
                                        Icon::new(HeroIconName::ArrowPath)
                                            .size_3p5()
                                            .text_color(cx.theme().secondary_foreground),
                                    )
                                    .on_click(move |_, _window, cx| {
                                        cx.update_entity(
                                            &refresh_entity,
                                            |app: &mut CoduxApp, cx| {
                                                app.refresh_task_column_async(cx);
                                            },
                                        );
                                    }),
                            ),
                        ),
                ),
        )
}

fn task_list_area(
    rows: Vec<TaskWorktreeRow>,
    labels: TaskColumnLabels,
    scroll_handle: UniformListScrollHandle,
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<TaskWorktreeListView>,
) -> impl IntoElement {
    let rows = Rc::new(rows);
    let row_labels = labels.clone();
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
                move |row, _index, _window, cx| {
                    div()
                        .w_full()
                        .pb(px(4.0))
                        .child(worktree_compact_row(
                            row,
                            row_labels.clone(),
                            app_entity.clone(),
                            cx,
                        ))
                        .into_any_element()
                },
            )),
    )
}

fn recent_session_area(
    sessions: Vec<TaskSessionRow>,
    labels: TaskColumnLabels,
    scroll_handle: UniformListScrollHandle,
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<TaskSessionListView>,
) -> impl IntoElement {
    let session_count = sessions.len();
    let sessions = Rc::new(sessions);
    let row_labels = labels.clone();
    let row_app_entity = app_entity.clone();

    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .child(session_section_heading(
            labels.sessions.clone(),
            session_count,
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
                        div()
                            .w_full()
                            .pb(px(4.0))
                            .child(ai_session_compact_row(
                                session,
                                row_labels.clone(),
                                row_app_entity.clone(),
                                cx,
                            ))
                            .into_any_element()
                    },
                )),
        )
}

fn session_section_heading(
    title: String,
    count: usize,
    cx: &mut Context<TaskSessionListView>,
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
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(cx.theme().foreground)
                .child(title),
        )
        .child(Tag::secondary().rounded_full().child(count.to_string()))
}

fn worktree_compact_row(
    worktree: TaskWorktreeRow,
    labels: TaskColumnLabels,
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<TaskWorktreeListView>,
) -> impl IntoElement {
    let worktree_id = worktree.id.clone();
    let menu_worktree_id = worktree.id.clone();
    let menu_worktree_path = worktree.path.clone();
    let is_default = worktree.is_default;
    let activity_dismiss_id = if worktree.is_default {
        worktree.project_id.clone()
    } else {
        worktree.id.clone()
    };
    let activity_state = worktree.activity_state;
    let select_entity = app_entity.clone();
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
        .when(worktree.active, |this| this.bg(cx.theme().secondary_hover))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_click(move |_, window, cx| {
            cx.update_entity(&select_entity, |app, cx| {
                if activity_state == AIActivityState::Done {
                    app.dismiss_worktree_ai_completion(&activity_dismiss_id, cx);
                }
                app.select_worktree(worktree_id.clone(), window, cx)
            });
        })
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
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(worktree.title),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_DIM))
                                .truncate()
                                .child(
                                    labels
                                        .changed_format
                                        .replace("%@", &worktree.git_changes.to_string()),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .child(
                                    div()
                                        .text_color(color(0x3EE66B))
                                        .child(format!("+{}", worktree.git_additions.max(0))),
                                )
                                .child(
                                    div()
                                        .text_color(color(0xFF5C68))
                                        .child(format!("-{}", worktree.git_deletions.max(0))),
                                ),
                        ),
                ),
        )
        .context_menu(move |menu, _window, _cx| {
            let open_entity = app_entity.clone();
            let open_path = menu_worktree_path.clone();
            let merge_entity = app_entity.clone();
            let merge_worktree_id = menu_worktree_id.clone();
            let remove_entity = app_entity.clone();
            let remove_worktree_id = menu_worktree_id.clone();

            let menu = menu.item(
                PopupMenuItem::new(labels.open_folder.clone())
                    .icon(HeroIconName::Folder)
                    .on_click(move |_, _window, cx| {
                        cx.update_entity(&open_entity, |app, cx| {
                            app.open_worktree_folder(open_path.clone(), cx);
                        });
                    }),
            );

            if is_default {
                return menu;
            }

            menu.separator()
                .item(
                    PopupMenuItem::new(labels.merge.clone())
                        .icon(HeroIconName::ArrowDownTray)
                        .on_click(move |_, _window, cx| {
                            cx.update_entity(&merge_entity, |app, cx| {
                                app.merge_worktree_by_id(merge_worktree_id.clone(), cx);
                            });
                        }),
                )
                .separator()
                .item(
                    PopupMenuItem::new(labels.delete.clone())
                        .icon(HeroIconName::Trash)
                        .on_click(move |_, _window, cx| {
                            cx.update_entity(&remove_entity, |app, cx| {
                                app.request_remove_worktree_by_id(
                                    remove_worktree_id.clone(),
                                    false,
                                    cx,
                                );
                            });
                        }),
                )
        })
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
            .bg(color(theme::GREEN))
            .into_any_element(),
    }
}

fn worktree_row_title(worktree: &WorktreeInfo, no_branch: &str) -> String {
    let branch = worktree.branch.trim();
    let name = worktree.name.trim();

    if branch.is_empty() || branch == "uninitialized" {
        return no_branch.to_string();
    }

    if worktree.is_default {
        return branch.to_string();
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn worktree(name: &str, branch: &str, is_default: bool) -> WorktreeInfo {
        WorktreeInfo {
            id: "worktree-1".to_string(),
            project_id: "project-1".to_string(),
            name: name.to_string(),
            branch: branch.to_string(),
            path: "/workspace/project".to_string(),
            status: "active".to_string(),
            is_default,
            exists: true,
            git_summary: Default::default(),
        }
    }

    #[test]
    fn worktree_row_title_uses_worktree_fields_without_git_panel_state() {
        assert_eq!(
            worktree_row_title(&worktree("Task A", "feature/task-a", false), "No Branch"),
            "Task A"
        );
        assert_eq!(
            worktree_row_title(&worktree("", "feature/task-b", false), "No Branch"),
            "task-b"
        );
        assert_eq!(
            worktree_row_title(&worktree("Main", "main", true), "No Branch"),
            "main"
        );
        assert_eq!(
            worktree_row_title(&worktree("Draft", "uninitialized", false), "No Branch"),
            "No Branch"
        );
    }
}

fn ai_session_compact_row(
    session: TaskSessionRow,
    labels: TaskColumnLabels,
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<TaskSessionListView>,
) -> impl IntoElement {
    let restore_session_id = session.id.clone();
    let menu_session_id = session.id.clone();
    let last_seen = relative_time_label_for_language(session.last_seen_at, &labels.language);
    let restore_entity = app_entity.clone();
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
        .py_1()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_double_click(move |_, window, cx| {
            cx.update_entity(&restore_entity, |app, cx| {
                app.selected_ai_session_id = Some(restore_session_id.clone());
                app.restore_selected_ai_session(window, cx);
            });
        })
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
                        .text_size(rems(0.75))
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
                .text_size(rems(0.75))
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
            let fork_entity = app_entity.clone();
            let fork_session_id = menu_session_id.clone();
            let fork_label = labels.new_session.clone();
            let remove_entity = app_entity.clone();
            let remove_session_id = menu_session_id.clone();

            menu.item(
                PopupMenuItem::new(labels.open.clone())
                    .icon(HeroIconName::CommandLine)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&open_entity, |app, cx| {
                            app.selected_ai_session_id = Some(open_session_id.clone());
                            app.restore_selected_ai_session(window, cx);
                        });
                    }),
            )
            .submenu_with_icon(
                Some(Icon::new(HeroIconName::Plus)),
                fork_label,
                _window,
                _cx,
                move |menu, _window, _cx| {
                    AI_SESSION_FORK_TARGETS
                        .iter()
                        .copied()
                        .fold(menu, |menu, target| {
                            let target_entity = fork_entity.clone();
                            let target_session_id = fork_session_id.clone();
                            menu.item(
                                PopupMenuItem::new(target.display_name().to_string())
                                    .icon(HeroIconName::CommandLine)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&target_entity, |app, cx| {
                                            app.fork_ai_session_to_tool(
                                                target_session_id.clone(),
                                                target,
                                                window,
                                                cx,
                                            );
                                        });
                                    }),
                            )
                        })
                },
            )
            .item(
                PopupMenuItem::new(labels.delete.clone())
                    .icon(HeroIconName::Trash)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&remove_entity, |app, cx| {
                            app.request_remove_ai_session(remove_session_id.clone(), window, cx);
                        });
                    }),
            )
        })
}
