use super::*;
use crate::app::ui_helpers::codux_tooltip_container;
use crate::app::workspace_shared::workspace_i18n;
use gpui::Anchor;
use gpui_component::popover::Popover;

#[derive(Clone)]
struct TerminalPaneDrag {
    pane_index: usize,
}

impl Render for TerminalPaneDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size(px(1.0)).bg(cx.theme().transparent)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct TerminalPaneDropPreview {
    pane_index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalSplitDivider {
    None,
    Left,
    Top,
}

pub(in crate::app) struct WorkspaceColumnView {
    toolbar_view: gpui::Entity<WorkspaceToolbarView>,
    body_view: gpui::Entity<WorkspaceBodyView>,
    assistant_view: gpui::Entity<WorkspaceAssistantView>,
}

impl WorkspaceColumnView {
    pub(in crate::app) fn new(
        toolbar_view: gpui::Entity<WorkspaceToolbarView>,
        body_view: gpui::Entity<WorkspaceBodyView>,
        assistant_view: gpui::Entity<WorkspaceAssistantView>,
    ) -> Self {
        Self {
            toolbar_view,
            body_view,
            assistant_view,
        }
    }
}

impl Render for WorkspaceColumnView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let assistant_snapshot = self.assistant_view.read(cx);
        let show_assistant = assistant_snapshot
            .snapshot
            .panel
            .is_some_and(|panel| assistant_panel_available(panel, &assistant_snapshot.snapshot));

        div()
            .flex()
            .flex_col()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .child(
                div().flex().flex_none().w_full().h(px(52.0)).child(
                    gpui::AnyView::from(self.toolbar_view.clone())
                        .cached(gpui::StyleRefinement::default().flex().w_full().h(px(52.0))),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .w_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .flex_basis(px(0.0))
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .child(
                                gpui::AnyView::from(self.body_view.clone()).cached(
                                    gpui::StyleRefinement::default()
                                        .flex()
                                        .size_full()
                                        .min_w(px(0.0))
                                        .min_h(px(0.0)),
                                ),
                            ),
                    )
                    .when(show_assistant, |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_none()
                                .flex_shrink_0()
                                .w(px(ASSISTANT_PANEL_WIDTH))
                                .min_w(px(ASSISTANT_PANEL_WIDTH))
                                .max_w(px(ASSISTANT_PANEL_WIDTH))
                                .h_full()
                                .overflow_hidden()
                                .child(
                                    gpui::AnyView::from(self.assistant_view.clone()).cached(
                                        gpui::StyleRefinement::default().flex().size_full(),
                                    ),
                                ),
                        )
                    }),
            )
    }
}

pub(in crate::app) struct WorkspaceToolbarView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: WorkspaceToolbarSnapshot,
}

impl WorkspaceToolbarView {
    pub(in crate::app) fn new(
        app_entity: gpui::Entity<CoduxApp>,
        snapshot: WorkspaceToolbarSnapshot,
    ) -> Self {
        Self {
            app_entity,
            snapshot,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: WorkspaceToolbarSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for WorkspaceToolbarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .w_full()
            .h_full()
            .child(self.app_entity.update(cx, |app, cx| {
                app.workspace_toolbar(window, cx).into_any_element()
            }))
    }
}

pub(in crate::app) struct WorkspaceBodyView {
    app_entity: gpui::Entity<CoduxApp>,
    pub(in crate::app) terminal_workspace_view: Option<gpui::Entity<TerminalWorkspaceView>>,
    pub(in crate::app) file_editor_workspace_view:
        Option<gpui::Entity<file_editor::FileEditorWorkspaceView>>,
    pub(in crate::app) review_workspace_view: Option<gpui::Entity<ReviewWorkspaceView>>,
    pub(in crate::app) stats_workspace_view: Option<gpui::Entity<StatsWorkspaceView>>,
}

impl WorkspaceBodyView {
    pub(in crate::app) fn new(app_entity: gpui::Entity<CoduxApp>) -> Self {
        Self {
            app_entity,
            terminal_workspace_view: None,
            file_editor_workspace_view: None,
            review_workspace_view: None,
            stats_workspace_view: None,
        }
    }
}

impl Render for WorkspaceBodyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = self.app_entity.clone();
        self.app_entity.update(cx, |app, app_cx| {
            if app.state.selected_project.is_none() && app.workspace_view != WorkspaceView::Stats {
                if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_render_visible(false, cx));
                }
                self.terminal_workspace_view = None;
                self.file_editor_workspace_view = None;
                self.review_workspace_view = None;
                self.stats_workspace_view = None;
                return app
                    .empty_project_workspace(window, app_cx)
                    .into_any_element();
            }
            if app.workspace_view == WorkspaceView::Terminal {
                let snapshot = app.terminal_workspace_snapshot();
                let terminal_view = if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_render_visible(true, cx));
                    view.update(app_cx, |view, cx| view.set_snapshot(snapshot, cx));
                    view.clone()
                } else {
                    let view =
                        app_cx.new(|_| TerminalWorkspaceView::new(app_entity.clone(), snapshot));
                    self.terminal_workspace_view = Some(view.clone());
                    view
                };
                let split_file_editor = app.workspace_split == Some(WorkspaceSplitKind::FileEditor)
                    && !app.file_editor_tabs.is_empty();
                if !split_file_editor {
                    return workspace_body_any_view(terminal_view).into_any_element();
                }
                // Split mode: terminal on the left, the existing file-editor
                // workspace (with its own tab bar) on the right, in a draggable
                // horizontal split. h_resizable persists the dragged sizes in
                // its own keyed state for the session.
                let fe_snapshot = app.file_editor_workspace_snapshot();
                let file_editor_view = if let Some(view) = &self.file_editor_workspace_view {
                    view.update(app_cx, |view, cx| view.set_snapshot(fe_snapshot, cx));
                    view.clone()
                } else {
                    let view = app_cx.new(|_| {
                        file_editor::FileEditorWorkspaceView::new(app_entity.clone(), fe_snapshot)
                    });
                    self.file_editor_workspace_view = Some(view.clone());
                    view
                };
                // Both panels carry equal flex (no fixed `.size`), so the split
                // defaults to an even 50/50 — matching the terminal splits —
                // while h_resizable still lets the user drag it. The "close
                // split" control lives in the file editor's tab bar (a dedicated
                // slot), so it never overlaps the tabs.
                h_resizable("workspace-body-file-split")
                    .child(
                        resizable_panel()
                            .size_range(px(320.0)..Pixels::MAX)
                            .child(gpui::AnyView::from(terminal_view)),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(320.0)..Pixels::MAX)
                            .child(gpui::AnyView::from(file_editor_view)),
                    )
                    .into_any_element()
            } else if app.workspace_view == WorkspaceView::Files {
                if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_render_visible(false, cx));
                }
                let snapshot = app.file_editor_workspace_snapshot();
                let file_editor_view = if let Some(view) = &self.file_editor_workspace_view {
                    view.clone()
                } else {
                    let view = app_cx.new(|_| {
                        file_editor::FileEditorWorkspaceView::new(app_entity.clone(), snapshot)
                    });
                    self.file_editor_workspace_view = Some(view.clone());
                    view
                };
                workspace_body_any_view(file_editor_view).into_any_element()
            } else if app.workspace_view == WorkspaceView::Review {
                if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_render_visible(false, cx));
                }
                let snapshot = app.review_workspace_snapshot();
                let review_view = if let Some(view) = &self.review_workspace_view {
                    view.clone()
                } else {
                    let view =
                        app_cx.new(|_| ReviewWorkspaceView::new(app_entity.clone(), snapshot));
                    self.review_workspace_view = Some(view.clone());
                    view
                };
                workspace_body_any_view(review_view).into_any_element()
            } else if app.workspace_view == WorkspaceView::Stats {
                if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_render_visible(false, cx));
                }
                let snapshot = app.stats_workspace_snapshot();
                let stats_view = if let Some(view) = &self.stats_workspace_view {
                    view.update(app_cx, |view, cx| view.set_snapshot(snapshot, cx));
                    view.clone()
                } else {
                    let view = app_cx.new(|cx| {
                        StatsWorkspaceView::new(app_entity.clone(), snapshot, window, cx)
                    });
                    self.stats_workspace_view = Some(view.clone());
                    view
                };
                workspace_body_any_view(stats_view).into_any_element()
            } else {
                app.workspace_body(window, app_cx).into_any_element()
            }
        })
    }
}

fn workspace_body_any_view<V>(view: gpui::Entity<V>) -> impl IntoElement
where
    V: Render + 'static,
{
    gpui::AnyView::from(view).cached(
        gpui::StyleRefinement::default()
            .flex()
            .flex_1()
            .flex_basis(px(0.0))
            .w_full()
            .h_full()
            .min_w(px(0.0))
            .min_h(px(0.0)),
    )
}

impl CoduxApp {
    fn empty_project_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locale = locale_from_language_setting(&self.state.settings.language);
        let title =
            translate(&locale, "welcome.title_format", "Welcome to %@").replace("%@", "Codux");
        let subtitle = translate(
            &locale,
            "welcome.subtitle",
            "Create a new project or open an existing folder to get started",
        );
        let new_project = translate(&locale, "menu.file.new_project", "New Project");
        let open_project = translate(&locale, "welcome.open_project", "Open Project");

        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(color(theme::BG_TERMINAL))
            .child(
                div()
                    .w(px(360.0))
                    .max_w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(14.0))
                    .px_4()
                    .text_center()
                    .child(
                        img("app-icons/codux-default.svg")
                            .size(px(72.0))
                            .object_fit(ObjectFit::Contain),
                    )
                    .child(
                        div()
                            .text_size(rems(1.375))
                            .line_height(rems(1.75))
                            .text_color(color(theme::TEXT))
                            .child(title),
                    )
                    .child(
                        div()
                            .max_w(px(320.0))
                            .text_size(rems(0.8125))
                            .line_height(rems(1.25))
                            .text_color(color(theme::TEXT_DIM))
                            .child(subtitle),
                    )
                    .child(
                        div()
                            .mt_2()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("welcome-new-project")
                                    .primary()
                                    .text_size(rems(0.875))
                                    .on_click(window.listener_for(
                                        &cx.entity(),
                                        |app, _event, window, cx| {
                                            app.open_project_create_window(window, cx);
                                        },
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(Icon::new(HeroIconName::FolderPlus).size_3p5())
                                            .child(new_project),
                                    ),
                            )
                            .child(
                                Button::new("welcome-open-project")
                                    .secondary()
                                    .text_size(rems(0.875))
                                    .on_click(window.listener_for(
                                        &cx.entity(),
                                        |app, _event, window, cx| {
                                            app.open_project_folder_from_dialog(window, cx);
                                        },
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(Icon::new(HeroIconName::FolderOpen).size_3p5())
                                            .child(open_project),
                                    ),
                            ),
                    ),
            )
    }
}

pub(in crate::app) struct ReviewWorkspaceView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: super::workspace_review::ReviewWorkspaceSnapshot,
    file_list_view: Option<gpui::Entity<ReviewFileListView>>,
    diff_content_view: Option<gpui::Entity<ReviewDiffContentView>>,
}

pub(in crate::app) struct StatsWorkspaceView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: super::workspace_stats::StatsWorkspaceSnapshot,
    scroll_handle: gpui::ScrollHandle,
    container_width: Option<Pixels>,
    project_table: gpui::Entity<
        gpui_component::table::TableState<super::workspace_stats::StatsProjectTableDelegate>,
    >,
}

impl StatsWorkspaceView {
    fn new(
        app_entity: gpui::Entity<CoduxApp>,
        snapshot: super::workspace_stats::StatsWorkspaceSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let project_table = cx.new(|cx| {
            gpui_component::table::TableState::new(
                super::workspace_stats::StatsProjectTableDelegate::new(
                    snapshot.project_rows(),
                    snapshot.language().to_string(),
                ),
                window,
                cx,
            )
            .col_selectable(false)
            .col_movable(false)
        });
        Self {
            app_entity,
            snapshot,
            scroll_handle: gpui::ScrollHandle::default(),
            container_width: None,
            project_table,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: super::workspace_stats::StatsWorkspaceSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        self.project_table.update(cx, |table, cx| {
            table
                .delegate_mut()
                .set_rows(snapshot.project_rows(), snapshot.language().to_string());
            table.refresh(cx);
            cx.notify();
        });
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for StatsWorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        super::workspace_stats::stats_workspace_body(
            self.app_entity.clone(),
            self.project_table.clone(),
            self.scroll_handle.clone(),
            self.snapshot.clone(),
            self.container_width,
            cx,
        )
        .on_prepaint({
            let view = cx.entity();
            move |bounds, _, cx| {
                view.update(cx, |view, cx| {
                    let width = bounds.size.width;
                    if view
                        .container_width
                        .is_none_or(|recorded| (recorded - width).abs() > px(1.0))
                    {
                        view.container_width = Some(width);
                        cx.notify();
                    }
                });
            }
        })
        .into_any_element()
    }
}

impl ReviewWorkspaceView {
    fn new(
        app_entity: gpui::Entity<CoduxApp>,
        snapshot: super::workspace_review::ReviewWorkspaceSnapshot,
    ) -> Self {
        Self {
            app_entity,
            snapshot,
            file_list_view: None,
            diff_content_view: None,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: super::workspace_review::ReviewWorkspaceSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for ReviewWorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot.clone();
        let file_list_snapshot = snapshot.file_list_snapshot();
        let diff_content_snapshot = snapshot.diff_content_snapshot();
        let file_list_view = self.review_file_list_view(file_list_snapshot, cx);
        let diff_content_view = self.review_diff_content_view(diff_content_snapshot, cx);
        super::workspace_review::review_workspace_body(
            snapshot,
            file_list_view,
            diff_content_view,
            cx,
        )
        .into_any_element()
    }
}

impl ReviewWorkspaceView {
    fn review_file_list_view(
        &mut self,
        snapshot: super::workspace_review::ReviewFileListSnapshot,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<ReviewFileListView> {
        if let Some(view) = &self.file_list_view {
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }
        let view = cx.new(|_| ReviewFileListView::new(self.app_entity.clone(), snapshot));
        self.file_list_view = Some(view.clone());
        view
    }

    fn review_diff_content_view(
        &mut self,
        snapshot: super::workspace_review::ReviewDiffContentSnapshot,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<ReviewDiffContentView> {
        if let Some(view) = &self.diff_content_view {
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }
        let view = cx.new(|_| ReviewDiffContentView::new(snapshot));
        self.diff_content_view = Some(view.clone());
        view
    }
}

pub(in crate::app) struct ReviewFileListView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: super::workspace_review::ReviewFileListSnapshot,
}

impl ReviewFileListView {
    fn new(
        app_entity: gpui::Entity<CoduxApp>,
        snapshot: super::workspace_review::ReviewFileListSnapshot,
    ) -> Self {
        Self {
            app_entity,
            snapshot,
        }
    }

    fn set_snapshot(
        &mut self,
        snapshot: super::workspace_review::ReviewFileListSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for ReviewFileListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot.clone();
        super::workspace_review::review_file_list(self.app_entity.clone(), snapshot, cx)
            .into_any_element()
    }
}

pub(in crate::app) struct ReviewDiffContentView {
    snapshot: super::workspace_review::ReviewDiffContentSnapshot,
}

impl ReviewDiffContentView {
    fn new(snapshot: super::workspace_review::ReviewDiffContentSnapshot) -> Self {
        Self { snapshot }
    }

    fn set_snapshot(
        &mut self,
        snapshot: super::workspace_review::ReviewDiffContentSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for ReviewDiffContentView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot.clone();
        super::workspace_review::review_diff_content(snapshot, cx).into_any_element()
    }
}

pub(in crate::app) struct WorkspaceAssistantView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: WorkspaceAssistantSnapshot,
}

impl WorkspaceAssistantView {
    pub(in crate::app) fn new(
        app_entity: gpui::Entity<CoduxApp>,
        snapshot: WorkspaceAssistantSnapshot,
    ) -> Self {
        Self {
            app_entity,
            snapshot,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: WorkspaceAssistantSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for WorkspaceAssistantView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot.clone();
        let app_entity = self.app_entity.clone();
        div().when_some(snapshot.panel, |this, panel| {
            this.when(assistant_panel_available(panel, &snapshot), |this| {
                this.flex()
                    .flex_col()
                    .flex_none()
                    .flex_shrink_0()
                    .w(px(ASSISTANT_PANEL_WIDTH))
                    .min_w(px(ASSISTANT_PANEL_WIDTH))
                    .max_w(px(ASSISTANT_PANEL_WIDTH))
                    .h_full()
                    .bg(theme::vibrancy_panel(color(theme::BG_COLUMN)))
                    .border_l_1()
                    .border_color(cx.theme().sidebar_border)
                    .child(match panel {
                        AssistantPanel::AIStats => self.app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.ai_stats_sidebar_view(cx)).into_any_element()
                        }),
                        AssistantPanel::ServerInfo => self.app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.server_info_sidebar_view(cx)).into_any_element()
                        }),
                        AssistantPanel::SSH => self.app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.ssh_sidebar_view(cx)).into_any_element()
                        }),
                        AssistantPanel::DB => self.app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.db_sidebar_view(cx)).into_any_element()
                        }),
                        AssistantPanel::FileManager => self.app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.file_sidebar_view(cx))
                                .cached(
                                    gpui::StyleRefinement::default()
                                        .flex()
                                        .flex_col()
                                        .w_full()
                                        .h_full()
                                        .min_h_0(),
                                )
                                .into_any_element()
                        }),
                        AssistantPanel::Git => app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.git_sidebar_view(cx)).into_any_element()
                        }),
                    })
            })
        })
    }
}

impl CoduxApp {
    pub(in crate::app) fn workspace_toolbar_snapshot(&self) -> WorkspaceToolbarSnapshot {
        WorkspaceToolbarSnapshot {
            fingerprint: workspace_toolbar_fingerprint(self),
        }
    }

    pub(in crate::app) fn workspace_assistant_snapshot(&self) -> WorkspaceAssistantSnapshot {
        WorkspaceAssistantSnapshot {
            panel: self.assistant_panel,
            has_project: self.state.selected_project.is_some(),
            is_remote_project: self
                .state
                .selected_project
                .as_ref()
                .and_then(|project| project.host_device_id.as_ref())
                .is_some(),
        }
    }

    pub(in crate::app) fn workspace_column_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<WorkspaceColumnView> {
        if let Some(view) = &self.workspace_column_view {
            return view.clone();
        }
        let toolbar_view = self.workspace_toolbar_view(cx);
        let body_view = self.workspace_body_view(cx);
        let assistant_view = self.workspace_assistant_view(cx);
        let view = cx.new(|_| WorkspaceColumnView::new(toolbar_view, body_view, assistant_view));
        self.workspace_column_view = Some(view.clone());
        view
    }

    pub(in crate::app) fn workspace_toolbar_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<WorkspaceToolbarView> {
        if let Some(view) = &self.workspace_toolbar_view {
            let snapshot = self.workspace_toolbar_snapshot();
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }
        let app_entity = cx.entity();
        let snapshot = self.workspace_toolbar_snapshot();
        let view = cx.new(|_| WorkspaceToolbarView::new(app_entity, snapshot));
        self.workspace_toolbar_view = Some(view.clone());
        view
    }

    pub(in crate::app) fn workspace_body_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<WorkspaceBodyView> {
        if let Some(view) = &self.workspace_body_view {
            return view.clone();
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| WorkspaceBodyView::new(app_entity));
        self.workspace_body_view = Some(view.clone());
        view
    }

    pub(in crate::app) fn workspace_assistant_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<WorkspaceAssistantView> {
        if let Some(view) = &self.workspace_assistant_view {
            let snapshot = self.workspace_assistant_snapshot();
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }
        let app_entity = cx.entity();
        let snapshot = self.workspace_assistant_snapshot();
        let view = cx.new(|_| WorkspaceAssistantView::new(app_entity, snapshot));
        self.workspace_assistant_view = Some(view.clone());
        view
    }
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct WorkspaceToolbarSnapshot {
    fingerprint: u64,
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct WorkspaceAssistantSnapshot {
    panel: Option<AssistantPanel>,
    has_project: bool,
    is_remote_project: bool,
}

fn assistant_panel_available(panel: AssistantPanel, snapshot: &WorkspaceAssistantSnapshot) -> bool {
    match panel {
        AssistantPanel::SSH => true,
        AssistantPanel::ServerInfo => snapshot.has_project,
        _ => snapshot.has_project,
    }
}

fn workspace_toolbar_fingerprint(app: &CoduxApp) -> u64 {
    workspace_view_hash(&(
        workspace_view_key(app.workspace_view),
        assistant_panel_key(app.assistant_panel),
        app.state.selected_project.as_ref().map(|project| {
            (
                project.id.clone(),
                project.name.clone(),
                project.path.clone(),
                project.host_device_id.clone(),
            )
        }),
        app.state.settings.language.clone(),
        app.state.settings.pet_enabled,
        !app.state.projects.is_empty(),
        app.project_open_applications
            .iter()
            .filter(|application| application.installed)
            .map(|application| {
                (
                    application.id.clone(),
                    application.label.clone(),
                    application.category.clone(),
                )
            })
            .collect::<Vec<_>>(),
        workspace_pet_fingerprint(app),
        workspace_view_hash(&(
            app.state.daily_level.tokens,
            app.state.daily_level.current_tier.id.clone(),
            app.state.daily_level.current_tier.min,
        )),
    ))
}

pub(in crate::app) fn workspace_view_hash<T: std::hash::Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(value, &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

fn workspace_view_key(view: WorkspaceView) -> &'static str {
    match view {
        WorkspaceView::Terminal => "terminal",
        WorkspaceView::Files => "files",
        WorkspaceView::Review => "review",
        WorkspaceView::Stats => "stats",
    }
}

fn assistant_panel_key(panel: Option<AssistantPanel>) -> &'static str {
    match panel {
        Some(AssistantPanel::AIStats) => "ai_stats",
        Some(AssistantPanel::ServerInfo) => "server_info",
        Some(AssistantPanel::SSH) => "ssh",
        Some(AssistantPanel::DB) => "db",
        Some(AssistantPanel::FileManager) => "file_manager",
        Some(AssistantPanel::Git) => "git",
        None => "none",
    }
}

fn workspace_pet_fingerprint(app: &CoduxApp) -> u64 {
    workspace_view_hash(&[
        workspace_view_hash(&(
            app.state.pet.available,
            app.state.pet.claimed,
            app.state.pet.species.clone(),
            app.state.pet.display_name.clone(),
            app.state.pet.custom_name.clone(),
        )),
        workspace_view_hash(&(
            app.state.pet.level,
            app.state.pet.total_xp,
            app.state.pet.daily_xp,
            app.state.pet.archived_count,
            app.state.pet.custom_pet_count,
            app.state.pet.updated_at,
        )),
        workspace_view_hash(&(app.pet_snapshot.updated_at, app.pet_snapshot.progress.level)),
        workspace_view_hash(
            &app.pet_custom_pets
                .iter()
                .map(|pet| (pet.id.clone(), pet.display_name.clone(), pet.installed_at))
                .collect::<Vec<_>>(),
        ),
        workspace_view_hash(&(
            app.pet_install_url.clone(),
            app.pet_install_display_name.clone(),
            app.pet_install_error.clone(),
            app.pet_install_previewing,
            app.pet_installing,
        )),
        workspace_view_hash(&app.pet_install_preview.as_ref().map(|preview| {
            (
                preview.page_url.clone(),
                preview.zip_url.clone(),
                preview.slug.clone(),
                preview.display_name.clone(),
                preview.image_url.clone(),
                preview.local_image_path.clone(),
            )
        })),
        workspace_view_hash(&(
            app.pet_name_editing,
            app.visible_pet_sprite_frame(PET_IDLE_FRAME_COUNT),
        )),
    ])
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct TerminalWorkspaceSnapshot {
    loading: bool,
    language: String,
    layout_key: String,
    top_ratios: Vec<f64>,
    top_grid: TerminalTopGrid,
    split_tree: Option<TerminalSplitNode>,
    main_panes: Vec<TerminalPaneViewSnapshot>,
}

impl TerminalWorkspaceSnapshot {
    fn visible_terminal_views(&self) -> Vec<gpui::Entity<TerminalView>> {
        self.main_panes
            .iter()
            .filter_map(|pane| pane.view.clone())
            .collect()
    }

    fn set_terminal_views_visible<C>(&self, visible: bool, cx: &mut C)
    where
        C: AppContext,
    {
        for view in self.visible_terminal_views() {
            view.update(cx, |view, cx| view.set_render_visible(visible, cx));
        }
    }
}

#[derive(Clone)]
struct TerminalPaneViewSnapshot {
    terminal_id: Option<String>,
    view: Option<gpui::Entity<TerminalView>>,
    title: String,
    subtitle: Option<String>,
    search_open: bool,
}

impl PartialEq for TerminalPaneViewSnapshot {
    fn eq(&self, other: &Self) -> bool {
        if self.terminal_id != other.terminal_id
            || self.title != other.title
            || self.subtitle != other.subtitle
            || self.search_open != other.search_open
        {
            return false;
        }
        match (&self.view, &other.view) {
            (Some(left), Some(right)) => left.entity_id() == right.entity_id(),
            (None, None) => true,
            _ => false,
        }
    }
}

pub(in crate::app) struct TerminalWorkspaceView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: TerminalWorkspaceSnapshot,
    // Real size of the terminal workspace, recorded at prepaint. Used to
    // derive panel sizes from the persisted ratios so the first frame
    // after a layout-key switch matches the actual container.
    container_height: Option<Pixels>,
    container_width: Option<Pixels>,
    pane_drop_preview: Option<TerminalPaneDropPreview>,
    open_split_menu_pane: Option<usize>,
    split_menu_hover_epoch: u64,
}

impl TerminalWorkspaceView {
    fn new(app_entity: gpui::Entity<CoduxApp>, snapshot: TerminalWorkspaceSnapshot) -> Self {
        Self {
            app_entity,
            snapshot,
            container_height: None,
            container_width: None,
            pane_drop_preview: None,
            open_split_menu_pane: None,
            split_menu_hover_epoch: 0,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: TerminalWorkspaceSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        let next_visible = snapshot
            .visible_terminal_views()
            .into_iter()
            .map(|view| view.entity_id())
            .collect::<std::collections::HashSet<_>>();
        for view in self.snapshot.visible_terminal_views() {
            if !next_visible.contains(&view.entity_id()) {
                view.update(cx, |view, cx| view.set_render_visible(false, cx));
            }
        }
        if self
            .open_split_menu_pane
            .is_some_and(|index| index >= snapshot.main_panes.len())
        {
            self.open_split_menu_pane = None;
        }
        self.snapshot = snapshot;
        cx.notify();
    }

    pub(in crate::app) fn set_render_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.snapshot.set_terminal_views_visible(visible, cx);
    }

    fn set_split_menu_open(&mut self, pane_index: usize, open: bool, cx: &mut Context<Self>) {
        let next = open.then_some(pane_index);
        if self.open_split_menu_pane == next {
            if open {
                self.split_menu_hover_epoch = self.split_menu_hover_epoch.wrapping_add(1);
            }
            return;
        }
        self.open_split_menu_pane = next;
        self.split_menu_hover_epoch = self.split_menu_hover_epoch.wrapping_add(1);
        cx.notify();
    }

    fn close_split_menu_after_hover_gap(&mut self, pane_index: usize, cx: &mut Context<Self>) {
        let epoch = self.split_menu_hover_epoch;
        cx.spawn(async move |view: gpui::WeakEntity<Self>, cx| {
            // Grace long enough to cross the trigger→popover gap without the
            // menu vanishing mid-travel.
            cx.background_executor()
                .timer(Duration::from_millis(260))
                .await;
            let _ = view.update(cx, |view, cx| {
                if view.open_split_menu_pane == Some(pane_index)
                    && view.split_menu_hover_epoch == epoch
                {
                    view.open_split_menu_pane = None;
                    view.split_menu_hover_epoch = view.split_menu_hover_epoch.wrapping_add(1);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for TerminalWorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main = terminal_main_split_area(
            self.app_entity.clone(),
            &self.snapshot.language,
            self.snapshot.main_panes.clone(),
            &self.snapshot.layout_key,
            &self.snapshot.top_ratios,
            &self.snapshot.top_grid,
            &self.snapshot.split_tree,
            self.container_width,
            self.container_height,
            self.pane_drop_preview,
            self.open_split_menu_pane,
            cx,
        );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .on_prepaint({
                let view = cx.entity();
                move |bounds, _, cx| {
                    view.update(cx, |view, cx| {
                        let height = bounds.size.height;
                        let width = bounds.size.width;
                        let changed = view
                            .container_height
                            .is_none_or(|recorded| (recorded - height).abs() > px(1.0))
                            || view
                                .container_width
                                .is_none_or(|recorded| (recorded - width).abs() > px(1.0));
                        if changed {
                            view.container_height = Some(height);
                            view.container_width = Some(width);
                            cx.notify();
                        }
                    });
                }
            })
            .child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .min_w_0()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .overflow_hidden()
                    .child(main),
            )
    }
}

impl CoduxApp {
    pub(in crate::app) fn update_file_editor_workspace_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self
            .workspace_body_view
            .as_ref()
            .and_then(|view| view.read(cx).file_editor_workspace_view.clone())
        else {
            return false;
        };
        let snapshot = self.file_editor_workspace_snapshot();
        view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
        true
    }

    pub(in crate::app) fn update_review_workspace_view(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(view) = self
            .workspace_body_view
            .as_ref()
            .and_then(|view| view.read(cx).review_workspace_view.clone())
        else {
            return false;
        };
        let snapshot = self.review_workspace_snapshot();
        view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
        true
    }

    pub(in crate::app) fn update_stats_workspace_view(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(view) = self
            .workspace_body_view
            .as_ref()
            .and_then(|view| view.read(cx).stats_workspace_view.clone())
        else {
            return false;
        };
        let snapshot = self.stats_workspace_snapshot();
        view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
        true
    }

    pub(in crate::app) fn update_terminal_workspace_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(body_view) = self.workspace_body_view.as_ref().cloned() else {
            return false;
        };
        let Some(view) = body_view.read(cx).terminal_workspace_view.clone() else {
            body_view.update(cx, |_view, cx| cx.notify());
            return true;
        };
        let snapshot = self.terminal_workspace_snapshot();
        view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
        true
    }

    pub(in crate::app) fn terminal_workspace_snapshot(&self) -> TerminalWorkspaceSnapshot {
        let ai_titles = terminal_ai_titles_by_terminal_id(&self.state.ai_runtime_state.sessions);
        let main_panes = self
            .main_terminal()
            .map(|tab| {
                tab.panes
                    .iter()
                    .enumerate()
                    .map(|(index, slot)| {
                        let terminal_id = Self::terminal_slot_terminal_id(tab, index, slot);
                        let osc_title = terminal_id
                            .as_deref()
                            .and_then(|id| self.terminal_osc_titles.get(id));
                        let (title, subtitle) = terminal_pane_display_title(
                            slot,
                            &ai_titles,
                            osc_title.map(String::as_str),
                            &self.state.settings.language,
                        );
                        let search_open = terminal_id
                            .as_deref()
                            .is_some_and(|id| self.terminal_search_open.contains(id));
                        TerminalPaneViewSnapshot {
                            terminal_id,
                            view: slot.pane.as_ref().map(|pane| pane.view.clone()),
                            title,
                            subtitle,
                            search_open,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            main_panes.len(),
        );
        let top_grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            main_panes.len(),
        );
        let split_tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &top_grid,
            &top_ratios,
            main_panes.len(),
        );
        TerminalWorkspaceSnapshot {
            loading: self.terminal_layout_loading,
            language: self.state.settings.language.clone(),
            layout_key: super::ai_runtime_status::current_terminal_layout_storage_key(&self.state)
                .unwrap_or_default(),
            top_ratios,
            top_grid,
            split_tree,
            main_panes,
        }
    }
}

pub(in crate::app) fn terminal_ai_titles_by_terminal_id(
    sessions: &[codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
) -> HashMap<String, (String, Option<String>)> {
    let mut titles = HashMap::new();
    let mut updated_at_by_terminal_id = HashMap::new();
    for session in sessions {
        let terminal_id = session.terminal_id.trim();
        if terminal_id.is_empty() {
            continue;
        }
        if updated_at_by_terminal_id
            .get(terminal_id)
            .is_some_and(|updated_at| session.updated_at < *updated_at)
        {
            continue;
        }
        updated_at_by_terminal_id.insert(terminal_id.to_string(), session.updated_at);
        titles.insert(
            terminal_id.to_string(),
            (
                terminal_ai_tool_title(&session.tool),
                session
                    .model
                    .as_deref()
                    .map(str::trim)
                    .filter(|model| !model.is_empty())
                    .map(str::to_string),
            ),
        );
    }
    titles
}

fn terminal_ai_tool_title(tool: &str) -> String {
    let tool = tool.trim();
    if tool.is_empty() {
        return "AI CLI".to_string();
    }
    tool.to_string()
}

// Local terminals spawn the user's login shell; its basename ("zsh") is the
// default label when nothing set a meaningful title.
fn default_shell_display_name() -> Option<&'static str> {
    static NAME: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        if cfg!(windows) {
            return Some("powershell".to_string());
        }
        let shell = std::env::var("SHELL").ok()?;
        let name = std::path::Path::new(shell.trim()).file_stem()?;
        let name = name.to_string_lossy().trim().to_string();
        if name.is_empty() { None } else { Some(name) }
    })
    .as_deref()
}

// Title precedence: AI tool (+model subtitle) > custom pane label > shell OSC title > shell name > default label.
pub(in crate::app) fn terminal_pane_display_title(
    slot: &TerminalPaneSlot,
    ai_titles: &HashMap<String, (String, Option<String>)>,
    osc_title: Option<&str>,
    language: &str,
) -> (String, Option<String>) {
    if let Some((title, subtitle)) = slot
        .terminal_id
        .as_deref()
        .and_then(|terminal_id| ai_titles.get(terminal_id))
    {
        return (title.clone(), subtitle.clone());
    }
    let title = slot.title.trim();
    if !title.is_empty() && !terminal_slot_title_is_generic(title, language) {
        return (title.to_string(), None);
    }
    if let Some(osc_title) = osc_title.map(str::trim).filter(|title| !title.is_empty()) {
        return (osc_title.to_string(), None);
    }
    if let Some(shell) = default_shell_display_name() {
        return (shell.to_string(), None);
    }
    if !title.is_empty() {
        return (title.to_string(), None);
    }
    let locale = locale_from_language_setting(language);
    (translate(&locale, "terminal.title", "Terminal"), None)
}

// Auto-minted pane labels ("Terminal %d"/"Split %d", localized) are placeholders, not user titles.
fn terminal_slot_title_is_generic(title: &str, language: &str) -> bool {
    let locale = locale_from_language_setting(language);
    [
        translate(&locale, "terminal.default_format", "Terminal %d"),
        translate(&locale, "terminal.split.default_format", "Split %d"),
        "Terminal %d".to_string(),
        "Split %d".to_string(),
    ]
    .iter()
    .any(|format| title_matches_numbered_format(title, format))
}

fn title_matches_numbered_format(title: &str, format: &str) -> bool {
    let Some((prefix, suffix)) = format.split_once("%d") else {
        return title == format;
    };
    title
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()))
}

const TERMINAL_SPLIT_BASE_SIZE: Pixels = px(640.0);
const TERMINAL_SPLIT_BASE_WIDTH: Pixels = px(1200.0);
const TERMINAL_TOP_PANE_MIN_WIDTH: Pixels = px(160.0);
const TERMINAL_TOP_PANE_MIN_HEIGHT: Pixels = px(120.0);

fn terminal_layout_key_for_element_id(key: &str) -> String {
    key.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn terminal_main_split_area(
    app_entity: gpui::Entity<CoduxApp>,
    language: &str,
    panes: Vec<TerminalPaneViewSnapshot>,
    layout_key: &str,
    top_ratios: &[f64],
    top_grid: &TerminalTopGrid,
    split_tree: &Option<TerminalSplitNode>,
    container_width: Option<Pixels>,
    container_height: Option<Pixels>,
    pane_drop_preview: Option<TerminalPaneDropPreview>,
    open_split_menu_pane: Option<usize>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    if panes.is_empty() {
        return div()
            .flex_1()
            .size_full()
            .bg(theme::terminal_fill(color(theme::BG_TERMINAL)))
            .into_any_element();
    }

    let pane_count = panes.len();
    let grid = terminal_top_grid_for_panes(top_grid.clone(), top_ratios, pane_count);
    let tree = terminal_split_tree_for_panes(split_tree.clone(), &grid, top_ratios, pane_count)
        .unwrap_or(TerminalSplitNode::Leaf { pane: 0 });
    let total_width = container_width.unwrap_or(TERMINAL_SPLIT_BASE_WIDTH);
    let total_height = container_height.unwrap_or(TERMINAL_SPLIT_BASE_SIZE);
    let overlay = terminal_pane_drag_overlay(
        app_entity.clone(),
        tree.clone(),
        pane_count,
        pane_drop_preview,
        cx,
    );
    let content = terminal_split_node_element(
        app_entity.clone(),
        layout_key,
        language,
        panes,
        &tree,
        Vec::new(),
        TerminalSplitDivider::None,
        pane_count,
        total_width,
        total_height,
        open_split_menu_pane,
        cx,
    );

    div()
        .relative()
        .group("terminal-pane-drag-target")
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .child(content)
        .child(overlay)
        .into_any_element()
}

fn terminal_pane_drag_overlay(
    app_entity: gpui::Entity<CoduxApp>,
    split_tree: TerminalSplitNode,
    pane_count: usize,
    pane_drop_preview: Option<TerminalPaneDropPreview>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .invisible()
        .bg(color(theme::BG_TERMINAL).opacity(0.12))
        .group_drag_over::<TerminalPaneDrag>("terminal-pane-drag-target", |this| this.visible())
        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
            cx.stop_propagation();
            window.prevent_default();
        })
        .on_drag_move::<TerminalPaneDrag>(cx.listener({
            let split_tree = split_tree.clone();
            move |view, event: &gpui::DragMoveEvent<TerminalPaneDrag>, _window, cx| {
                let Some(pane_index) = terminal_pane_drop_target_at_position(
                    &split_tree,
                    pane_count,
                    event.bounds,
                    event.event.position,
                ) else {
                    if view.pane_drop_preview.take().is_some() {
                        cx.notify();
                    }
                    return;
                };
                let next = Some(TerminalPaneDropPreview { pane_index });
                if view.pane_drop_preview != next {
                    view.pane_drop_preview = next;
                    cx.notify();
                }
            }
        }))
        .on_drop(cx.listener({
            let app_entity = app_entity.clone();
            move |view, drag: &TerminalPaneDrag, window, cx| {
                let from_index = drag.pane_index;
                let preview = view.pane_drop_preview.take();
                let target = preview
                    .map(|preview| preview.pane_index)
                    .unwrap_or(from_index);
                if target != from_index {
                    defer_terminal_workspace_app_update(
                        app_entity.clone(),
                        window,
                        cx,
                        move |app, _window, app_cx| {
                            app.swap_terminal_top_panes(from_index, target, app_cx);
                        },
                    );
                }
                cx.stop_propagation();
                cx.notify();
            }
        }))
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .left_0()
                .when_some(pane_drop_preview, |this, preview| {
                    this.children(terminal_pane_drop_placeholder(
                        &split_tree,
                        pane_count,
                        preview,
                    ))
                }),
        )
        .into_any_element()
}

fn terminal_pane_drop_placeholder(
    split_tree: &TerminalSplitNode,
    pane_count: usize,
    preview: TerminalPaneDropPreview,
) -> Vec<AnyElement> {
    let rect = terminal_pane_rect(split_tree, pane_count, preview.pane_index);
    vec![
        div()
            .absolute()
            .left(relative(rect.left))
            .top(relative(rect.top))
            .w(relative(rect.width))
            .h(relative(rect.height))
            .p_2()
            .child(
                div()
                    .size_full()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(color(theme::ACCENT).opacity(0.70))
                    .bg(color(theme::ACCENT).opacity(0.20)),
            )
            .into_any_element(),
    ]
}

#[derive(Clone, Copy)]
pub(in crate::app) struct TerminalPaneRect {
    pub(in crate::app) left: f32,
    pub(in crate::app) top: f32,
    pub(in crate::app) width: f32,
    pub(in crate::app) height: f32,
}

pub(in crate::app) fn terminal_pane_rect(
    split_tree: &TerminalSplitNode,
    pane_count: usize,
    pane_index: usize,
) -> TerminalPaneRect {
    if let Some(rect) = terminal_pane_rect_in_node(
        split_tree,
        pane_count,
        pane_index,
        TerminalPaneRect {
            left: 0.0,
            top: 0.0,
            width: 1.0,
            height: 1.0,
        },
    ) {
        return rect;
    }
    TerminalPaneRect {
        left: 0.0,
        top: 0.0,
        width: 1.0,
        height: 1.0,
    }
}

fn terminal_pane_rect_in_node(
    node: &TerminalSplitNode,
    pane_count: usize,
    pane_index: usize,
    rect: TerminalPaneRect,
) -> Option<TerminalPaneRect> {
    match node {
        TerminalSplitNode::Leaf { pane } => {
            (*pane == pane_index && *pane < pane_count).then_some(rect)
        }
        TerminalSplitNode::Split {
            axis,
            ratios,
            children,
        } => {
            let ratios = codux_runtime::terminal_layout::normalize_split_ratios(
                ratios.clone(),
                children.len(),
            );
            let mut offset = 0.0_f32;
            for (child, ratio) in children.iter().zip(ratios) {
                let ratio = ratio as f32;
                let child_rect = match axis {
                    SplitAxis::Horizontal => TerminalPaneRect {
                        left: rect.left + offset,
                        top: rect.top,
                        width: rect.width * ratio,
                        height: rect.height,
                    },
                    SplitAxis::Vertical => TerminalPaneRect {
                        left: rect.left,
                        top: rect.top + offset,
                        width: rect.width,
                        height: rect.height * ratio,
                    },
                };
                if let Some(rect) =
                    terminal_pane_rect_in_node(child, pane_count, pane_index, child_rect)
                {
                    return Some(rect);
                }
                offset += match axis {
                    SplitAxis::Horizontal => rect.width * ratio,
                    SplitAxis::Vertical => rect.height * ratio,
                };
            }
            None
        }
    }
}

pub(in crate::app) fn terminal_pane_drop_target_at_position(
    split_tree: &TerminalSplitNode,
    pane_count: usize,
    bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
) -> Option<usize> {
    if pane_count == 0 || bounds.size.width <= px(0.0) || bounds.size.height <= px(0.0) {
        return None;
    }
    let x = ((position.x - bounds.left()) / bounds.size.width).clamp(0.0, 0.999_999);
    let y = ((position.y - bounds.top()) / bounds.size.height).clamp(0.0, 0.999_999);
    terminal_pane_drop_target_in_node(
        split_tree,
        pane_count,
        x,
        y,
        TerminalPaneRect {
            left: 0.0,
            top: 0.0,
            width: 1.0,
            height: 1.0,
        },
    )
}

fn terminal_pane_drop_target_in_node(
    node: &TerminalSplitNode,
    pane_count: usize,
    x: f32,
    y: f32,
    rect: TerminalPaneRect,
) -> Option<usize> {
    match node {
        TerminalSplitNode::Leaf { pane } if *pane < pane_count => Some(*pane),
        TerminalSplitNode::Leaf { .. } => None,
        TerminalSplitNode::Split {
            axis,
            ratios,
            children,
        } => {
            let ratios = codux_runtime::terminal_layout::normalize_split_ratios(
                ratios.clone(),
                children.len(),
            );
            let mut offset = 0.0_f32;
            for (index, (child, ratio)) in children.iter().zip(ratios).enumerate() {
                let ratio = ratio as f32;
                let child_rect = match axis {
                    SplitAxis::Horizontal => TerminalPaneRect {
                        left: rect.left + offset,
                        top: rect.top,
                        width: rect.width * ratio,
                        height: rect.height,
                    },
                    SplitAxis::Vertical => TerminalPaneRect {
                        left: rect.left,
                        top: rect.top + offset,
                        width: rect.width,
                        height: rect.height * ratio,
                    },
                };
                let inside = x >= child_rect.left
                    && x <= child_rect.left + child_rect.width
                    && y >= child_rect.top
                    && y <= child_rect.top + child_rect.height;
                if inside || index + 1 == children.len() {
                    return terminal_pane_drop_target_in_node(child, pane_count, x, y, child_rect);
                }
                offset += match axis {
                    SplitAxis::Horizontal => rect.width * ratio,
                    SplitAxis::Vertical => rect.height * ratio,
                };
            }
            None
        }
    }
}

fn terminal_split_node_element(
    app_entity: gpui::Entity<CoduxApp>,
    layout_key: &str,
    language: &str,
    panes: Vec<TerminalPaneViewSnapshot>,
    node: &TerminalSplitNode,
    path: Vec<usize>,
    divider: TerminalSplitDivider,
    pane_count: usize,
    total_width: Pixels,
    total_height: Pixels,
    open_split_menu_pane: Option<usize>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    let content = match node {
        TerminalSplitNode::Leaf { pane } => {
            let slot = panes.get(*pane).cloned();
            slot.map(|slot| {
                terminal_pane(
                    app_entity,
                    *pane,
                    language,
                    pane_count,
                    slot,
                    open_split_menu_pane,
                    cx,
                )
            })
            .unwrap_or_else(|| div().size_full().into_any_element())
        }
        TerminalSplitNode::Split {
            axis,
            ratios,
            children,
        } => {
            let split_id = SharedString::from(format!(
                "workspace-terminal-split-tree-{}-{}-{}",
                terminal_layout_key_for_element_id(layout_key),
                pane_count,
                path.iter()
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>()
                    .join("-")
            ));
            let resize_app_entity = app_entity.clone();
            let resize_layout_key = layout_key.to_string();
            let resize_path = path.clone();
            let ratios = codux_runtime::terminal_layout::normalize_split_ratios(
                ratios.clone(),
                children.len(),
            );
            let render_child = move |(index, child): (usize, &TerminalSplitNode)| {
                let mut child_path = path.clone();
                child_path.push(index);
                let divider = if index == 0 {
                    TerminalSplitDivider::None
                } else {
                    match axis {
                        SplitAxis::Horizontal => TerminalSplitDivider::Left,
                        SplitAxis::Vertical => TerminalSplitDivider::Top,
                    }
                };
                let ratio = ratios
                    .get(index)
                    .copied()
                    .unwrap_or(1.0 / children.len().max(1) as f64);
                let child_element = terminal_split_node_element(
                    app_entity.clone(),
                    layout_key,
                    language,
                    panes.clone(),
                    child,
                    child_path,
                    divider,
                    pane_count,
                    total_width,
                    total_height,
                    open_split_menu_pane,
                    cx,
                );
                match axis {
                    SplitAxis::Horizontal => resizable_panel()
                        .size(px((total_width.as_f32() as f64 * ratio) as f32))
                        .size_range(TERMINAL_TOP_PANE_MIN_WIDTH..Pixels::MAX)
                        .child(child_element),
                    SplitAxis::Vertical => resizable_panel()
                        .size(px((total_height.as_f32() as f64 * ratio) as f32))
                        .size_range(TERMINAL_TOP_PANE_MIN_HEIGHT..Pixels::MAX)
                        .child(child_element),
                }
            };
            match axis {
                SplitAxis::Horizontal => h_resizable(split_id)
                    .on_resize({
                        let resize_app_entity = resize_app_entity.clone();
                        let resize_layout_key = resize_layout_key.clone();
                        let resize_path = resize_path.clone();
                        move |state: &gpui::Entity<ResizableState>, window, cx| {
                            let sizes = state.read(cx).sizes().clone();
                            let Some(ratios) = terminal_top_ratios_from_sizes(&sizes) else {
                                return;
                            };
                            window.defer(cx, {
                                let app_entity = resize_app_entity.clone();
                                let layout_key = resize_layout_key.clone();
                                let path = resize_path.clone();
                                move |_window, cx| {
                                    let _ = app_entity.update(cx, |app, cx| {
                                        app.update_terminal_split_ratios(
                                            layout_key, path, ratios, cx,
                                        );
                                    });
                                }
                            });
                        }
                    })
                    .children(children.iter().enumerate().map(render_child))
                    .into_any_element(),
                SplitAxis::Vertical => v_resizable(split_id)
                    .on_resize(move |state: &gpui::Entity<ResizableState>, window, cx| {
                        let sizes = state.read(cx).sizes().clone();
                        let Some(ratios) = terminal_top_ratios_from_sizes(&sizes) else {
                            return;
                        };
                        window.defer(cx, {
                            let app_entity = resize_app_entity.clone();
                            let layout_key = resize_layout_key.clone();
                            let path = resize_path.clone();
                            move |_window, cx| {
                                let _ = app_entity.update(cx, |app, cx| {
                                    app.update_terminal_split_ratios(layout_key, path, ratios, cx);
                                });
                            }
                        });
                    })
                    .children(children.iter().enumerate().map(render_child))
                    .into_any_element(),
            }
        }
    };
    terminal_split_divider(content, divider)
}

fn terminal_split_divider(child: AnyElement, divider: TerminalSplitDivider) -> AnyElement {
    let element = div()
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .child(child);
    match divider {
        TerminalSplitDivider::None => element,
        TerminalSplitDivider::Left => element.border_l_1().border_color(color(theme::BORDER_SOFT)),
        TerminalSplitDivider::Top => element.border_t_1().border_color(color(theme::BORDER_SOFT)),
    }
    .into_any_element()
}

fn terminal_top_ratios_from_sizes(sizes: &[Pixels]) -> Option<Vec<f64>> {
    if sizes.len() < 2 {
        return None;
    }
    let total = sizes.iter().map(|size| size.as_f32() as f64).sum::<f64>();
    if total <= 1.0 {
        return None;
    }
    Some(
        sizes
            .iter()
            .map(|size| size.as_f32() as f64 / total)
            .collect(),
    )
}

fn terminal_pane(
    app_entity: gpui::Entity<CoduxApp>,
    index: usize,
    language: &str,
    pane_count: usize,
    slot: TerminalPaneViewSnapshot,
    open_split_menu_pane: Option<usize>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    let close_id = SharedString::from(format!("terminal-pane-close-{index}"));
    let float_id = SharedString::from(format!("terminal-pane-float-{index}"));
    let collapse_id = SharedString::from(format!("terminal-pane-collapse-{index}"));
    let add_id = SharedString::from(format!("terminal-pane-add-{index}"));
    let session_drop_entity = app_entity.clone();
    let pane_view = slot.view.clone();
    let drop_terminal_id = slot.terminal_id.clone();
    // The search bar floats over the same top-right corner as the controls.
    let search_open = slot.search_open;

    // Flat pane: hairline divider against the previous column, controls float
    // over the terminal's top-right corner and appear on hover.
    div()
        .id(SharedString::from(format!("terminal-pane-{index}")))
        .relative()
        .group("terminal-pane")
        .flex()
        .flex_col()
        .flex_1()
        .flex_basis(px(0.0))
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .bg(theme::terminal_fill(color(theme::BG_TERMINAL)))
        .child(
            div()
                .flex_1()
                .flex_basis(px(0.0))
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(match pane_view {
                    Some(view) => gpui::AnyView::from(view).into_any_element(),
                    None => div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(theme::terminal_fill(color(theme::BG_TERMINAL)))
                        .text_color(color(theme::TEXT_DIM))
                        .child(workspace_i18n(
                            language,
                            "terminal.detached.mounting",
                            "Mounting terminal...",
                        ))
                        .into_any_element(),
                }),
        )
        .when(!search_open, |pane| {
            pane.child(
                div()
                    .absolute()
                    .top(px(6.0))
                    .right(px(8.0))
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded(px(6.0))
                    .p(px(2.0))
                    .bg(theme::elevate(color(theme::BG_TERMINAL), 0.08).opacity(0.92))
                    .opacity(0.0)
                    .group_hover("terminal-pane", |style| style.opacity(1.0))
                    // The popover overlay lives outside the group, so hovering the
                    // menu would fade the controls out — pin them while it's open.
                    .when(open_split_menu_pane == Some(index), |style| {
                        style.opacity(1.0)
                    })
                    .child(terminal_pane_drag_handle(app_entity.clone(), index, cx))
                    .child(terminal_pane_control_button(
                        app_entity.clone(),
                        float_id,
                        HeroIconName::ArrowTopRightOnSquare,
                        SharedString::from(workspace_i18n(
                            language,
                            "terminal.detach",
                            "Open in Separate Window",
                        )),
                        pane_count > 1,
                        cx,
                        move |app, window, cx| app.float_terminal_pane(index, window, cx),
                    ))
                    .child(terminal_pane_control_button(
                        app_entity.clone(),
                        collapse_id,
                        HeroIconName::ChevronDown,
                        SharedString::from(workspace_i18n(
                            language,
                            "terminal.collapse",
                            "Collapse to Sidebar",
                        )),
                        pane_count > 1,
                        cx,
                        move |app, window, cx| app.collapse_terminal_pane(index, window, cx),
                    ))
                    .child(terminal_pane_split_button(
                        app_entity.clone(),
                        add_id,
                        index,
                        open_split_menu_pane,
                        cx,
                    ))
                    .child(terminal_pane_control_button(
                        app_entity,
                        close_id,
                        HeroIconName::XMark,
                        SharedString::from(workspace_i18n(
                            language,
                            "terminal.split.close",
                            "Close Split",
                        )),
                        pane_count > 1,
                        cx,
                        move |app, window, cx| app.close_terminal_pane(index, window, cx),
                    )),
            )
        })
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .left_0()
                .invisible()
                .p_2()
                .group_drag_over::<TaskSessionDrag>("terminal-pane", |this| this.visible())
                .on_drop(
                    cx.listener(move |_view, drag: &TaskSessionDrag, window, cx| {
                        let session_id = drag.session_id.clone();
                        let terminal_id = drop_terminal_id.clone();
                        defer_terminal_workspace_app_update(
                            session_drop_entity.clone(),
                            window,
                            cx,
                            move |app, window, app_cx| {
                                app.paste_ai_session_restore_to_main_pane(
                                    terminal_id.as_deref(),
                                    &session_id,
                                    window,
                                    app_cx,
                                );
                            },
                        );
                        cx.stop_propagation();
                    }),
                )
                .child(
                    div()
                        .size_full()
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(color(theme::ACCENT).opacity(0.70))
                        .bg(color(theme::ACCENT).opacity(0.12)),
                ),
        )
        .into_any_element()
}

fn terminal_pane_drag_handle(
    app_entity: gpui::Entity<CoduxApp>,
    pane_index: usize,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    let drag_icon = div()
        .id(SharedString::from(format!(
            "terminal-pane-drag-source-{pane_index}"
        )))
        .size(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .child(
            Icon::new(HeroIconName::ArrowsPointingOut)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_drag(TerminalPaneDrag { pane_index }, move |drag, _, _, cx| {
            cx.stop_propagation();
            cx.new(|_| TerminalPaneDrag {
                pane_index: drag.pane_index,
            })
        });

    div()
        .id(SharedString::from(format!(
            "terminal-pane-drag-handle-{pane_index}"
        )))
        .size(px(28.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .text_color(cx.theme().secondary_foreground)
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
            cx.stop_propagation();
            window.prevent_default();
        })
        .child(drag_icon)
        .map(|this| {
            codux_tooltip_container(
                app_entity,
                SharedString::from(format!("terminal-pane-drag-tooltip-{pane_index}")),
                "拖动分屏",
            )
            .child(this)
        })
        .into_any_element()
}

fn terminal_pane_split_button(
    app_entity: gpui::Entity<CoduxApp>,
    id: SharedString,
    pane_index: usize,
    open_split_menu_pane: Option<usize>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    const DIRECTIONS: [TerminalSplitDirection; 4] = [
        TerminalSplitDirection::Left,
        TerminalSplitDirection::Right,
        TerminalSplitDirection::Up,
        TerminalSplitDirection::Down,
    ];
    let is_open = open_split_menu_pane.is_some_and(|index| index == pane_index);
    let view = cx.entity();
    let content_id = SharedString::from(format!("{id}-menu-content"));
    let button = Button::new(SharedString::from(format!("{id}-default")))
        .with_size(Size::Size(px(22.0)))
        .rounded(px(3.0))
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(cx.theme().secondary_foreground)
                .hover(cx.theme().secondary_hover)
                .active(cx.theme().secondary_hover),
        )
        .icon(
            Icon::new(HeroIconName::Plus)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .on_hover(split_menu_hover_listener(view.clone(), pane_index))
        .on_click({
            let app_entity = app_entity.clone();
            let view = view.clone();
            move |_, window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.open_split_menu_pane = None;
                    cx.notify();
                });
                cx.update_entity(&app_entity, |app, cx| {
                    app.split_terminal_direction(
                        TerminalSplitDirection::Right,
                        TerminalSplitScope::Inner,
                        pane_index,
                        window,
                        cx,
                    );
                });
            }
        });

    Popover::new(id)
        .anchor(Anchor::TopRight)
        .appearance(false)
        .overlay_closable(false)
        .open(is_open)
        .trigger(button)
        .content(move |_, _window, cx| {
            // Icon-only grid: row 1 = split inside the current pane (dashed
            // frame), row 2 = split the whole layout (solid frame, edge slice).
            let row = |scope: TerminalSplitScope,
                       app_entity: &gpui::Entity<CoduxApp>,
                       view: &gpui::Entity<TerminalWorkspaceView>| {
                div()
                    .flex()
                    .gap_1()
                    .children(DIRECTIONS.into_iter().map(|direction| {
                        terminal_split_direction_menu_button(
                            app_entity.clone(),
                            view.clone(),
                            pane_index,
                            direction,
                            scope,
                        )
                    }))
            };
            div()
                .id(content_id.clone())
                .flex()
                .flex_col()
                .gap_1()
                .rounded_lg()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().popover)
                .shadow_lg()
                .p_1()
                .on_hover(split_menu_hover_listener(view.clone(), pane_index))
                .child(row(TerminalSplitScope::Inner, &app_entity, &view))
                .child(row(TerminalSplitScope::Root, &app_entity, &view))
        })
        .into_any_element()
}

fn split_menu_hover_listener(
    view: gpui::Entity<TerminalWorkspaceView>,
    pane_index: usize,
) -> impl Fn(&bool, &mut Window, &mut gpui::App) + 'static {
    move |hovered, _window, cx| {
        let _ = view.update(cx, |view, cx| {
            if *hovered {
                view.set_split_menu_open(pane_index, true, cx);
            } else {
                view.close_split_menu_after_hover_gap(pane_index, cx);
            }
        });
    }
}

fn terminal_split_direction_menu_button(
    app_entity: gpui::Entity<CoduxApp>,
    view: gpui::Entity<TerminalWorkspaceView>,
    pane_index: usize,
    direction: TerminalSplitDirection,
    scope: TerminalSplitScope,
) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "terminal-pane-split-{pane_index}-{scope:?}-{direction:?}"
        )))
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .hover(|style| style.bg(color(theme::ACCENT).opacity(0.12)))
        .child(terminal_split_direction_icon(direction, scope))
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.open_split_menu_pane = None;
                cx.notify();
            });
            cx.update_entity(&app_entity, |app, cx| {
                app.split_terminal_direction(direction, scope, pane_index, window, cx);
            });
        })
        .into_any_element()
}

/// Split glyphs: INNER = dashed frame (the current pane) cut in half, the new
/// half filled; ROOT = solid frame (the whole layout) with a new slice pushed
/// in from that edge.
fn terminal_split_direction_icon(
    direction: TerminalSplitDirection,
    scope: TerminalSplitScope,
) -> AnyElement {
    // Frames must stay legible on the dark popover: border rides on text-level
    // grey (dashed gaps read darker than solid, so no extra dimming), the new
    // slice is near-solid accent, the remaining area a faint grey wash.
    let frame_line = color(theme::TEXT_DIM).opacity(0.9);
    let active = color(theme::ACCENT).opacity(0.95);
    let inactive = color(theme::TEXT_DIM).opacity(0.16);
    let horizontal = matches!(
        direction,
        TerminalSplitDirection::Left | TerminalSplitDirection::Right
    );
    let before = matches!(
        direction,
        TerminalSplitDirection::Left | TerminalSplitDirection::Up
    );

    let frame = div()
        .size(px(22.0))
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(frame_line)
        .flex()
        .map(|frame| if horizontal { frame } else { frame.flex_col() })
        .map(|frame| match scope {
            TerminalSplitScope::Inner => frame.border_dashed(),
            TerminalSplitScope::Root => frame,
        });

    let (new_cell, old_cell) = match scope {
        // Inner: the pane splits 50/50, new half filled.
        TerminalSplitScope::Inner => (
            div()
                .flex_1()
                .rounded(px(1.0))
                .bg(active)
                .into_any_element(),
            div()
                .flex_1()
                .rounded(px(1.0))
                .bg(inactive)
                .into_any_element(),
        ),
        // Root: a narrow full-length slice lands at the edge of the layout.
        TerminalSplitScope::Root => (
            div()
                .map(|cell| {
                    if horizontal {
                        cell.w(px(5.0)).h_full()
                    } else {
                        cell.h(px(5.0)).w_full()
                    }
                })
                .flex_none()
                .rounded(px(1.0))
                .bg(active)
                .into_any_element(),
            div()
                .flex_1()
                .rounded(px(1.0))
                .bg(inactive)
                .into_any_element(),
        ),
    };

    if before {
        frame.child(new_cell).child(old_cell)
    } else {
        frame.child(old_cell).child(new_cell)
    }
    .into_any_element()
}

fn terminal_pane_control_button(
    app_entity: gpui::Entity<CoduxApp>,
    id: SharedString,
    icon: HeroIconName,
    tooltip: SharedString,
    enabled: bool,
    cx: &mut Context<TerminalWorkspaceView>,
    on_click: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let text_color = if enabled {
        cx.theme().secondary_foreground
    } else {
        color(theme::TEXT_DIM)
    };
    let inner = div()
        .size(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .child(Icon::new(icon).size_3p5().text_color(text_color));
    let inner = if enabled {
        inner.hover(|style| style.bg(cx.theme().secondary_hover))
    } else {
        inner
    };
    let button = codux_tooltip_container(app_entity.clone(), id, tooltip)
        .size(px(28.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .text_color(text_color)
        .child(inner);

    if enabled {
        let on_click = std::rc::Rc::new(on_click);
        button
            .cursor_pointer()
            .on_click(cx.listener(move |_view, _event, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                let on_click = on_click.clone();
                defer_terminal_workspace_app_update(
                    app_entity.clone(),
                    window,
                    cx,
                    move |app, window, app_cx| on_click(app, window, app_cx),
                );
            }))
            .into_any_element()
    } else {
        button.opacity(0.45).into_any_element()
    }
}

fn defer_terminal_workspace_app_update(
    app_entity: gpui::Entity<CoduxApp>,
    window: &mut Window,
    cx: &mut Context<TerminalWorkspaceView>,
    update: impl FnOnce(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) {
    defer_codux_app_update(app_entity, window, cx, update);
}
