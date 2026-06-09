use super::*;
use crate::app::ui_helpers::codux_tooltip_container;
use gpui_component::InteractiveElementExt as _;

#[derive(Clone)]
struct TerminalBottomTabDrag {
    terminal_id: usize,
    label: String,
    active: bool,
}

impl Render for TerminalBottomTabDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        terminal_bottom_tab_container(
            if self.active {
                cx.theme().foreground
            } else {
                cx.theme().secondary_foreground
            },
            if self.active {
                cx.theme().secondary_hover
            } else {
                cx.theme().transparent
            },
        )
        .child(terminal_bottom_tab_label(self.label.clone()))
    }
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .bg(color(theme::BG_TERMINAL))
            .child(
                div().flex().flex_none().w_full().h(px(44.0)).child(
                    gpui::AnyView::from(self.toolbar_view.clone())
                        .cached(gpui::StyleRefinement::default().flex().w_full().h(px(44.0))),
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
                    .child(gpui::AnyView::from(self.body_view.clone()))
                    .child(gpui::AnyView::from(self.assistant_view.clone())),
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
}

impl WorkspaceBodyView {
    pub(in crate::app) fn new(app_entity: gpui::Entity<CoduxApp>) -> Self {
        Self {
            app_entity,
            terminal_workspace_view: None,
            file_editor_workspace_view: None,
            review_workspace_view: None,
        }
    }
}

impl Render for WorkspaceBodyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = self.app_entity.clone();
        self.app_entity.update(cx, |app, app_cx| {
            if app.state.selected_project.is_none() {
                self.terminal_workspace_view = None;
                self.file_editor_workspace_view = None;
                self.review_workspace_view = None;
                return app
                    .empty_project_workspace(window, app_cx)
                    .into_any_element();
            }
            if app.workspace_view == WorkspaceView::Terminal {
                let snapshot = app.terminal_workspace_snapshot();
                let terminal_view = if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_snapshot(snapshot, cx));
                    view.clone()
                } else {
                    let view =
                        app_cx.new(|_| TerminalWorkspaceView::new(app_entity.clone(), snapshot));
                    self.terminal_workspace_view = Some(view.clone());
                    view
                };
                gpui::AnyView::from(terminal_view).into_any_element()
            } else if app.workspace_view == WorkspaceView::Files {
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
                gpui::AnyView::from(file_editor_view).into_any_element()
            } else if app.workspace_view == WorkspaceView::Review {
                let snapshot = app.review_workspace_snapshot();
                let review_view = if let Some(view) = &self.review_workspace_view {
                    view.clone()
                } else {
                    let view =
                        app_cx.new(|_| ReviewWorkspaceView::new(app_entity.clone(), snapshot));
                    self.review_workspace_view = Some(view.clone());
                    view
                };
                gpui::AnyView::from(review_view).into_any_element()
            } else {
                app.workspace_body(window, app_cx).into_any_element()
            }
        })
    }
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
            this.when(
                snapshot.has_project || panel == AssistantPanel::SSH,
                |this| {
                    this.flex()
                        .flex_col()
                        .flex_none()
                        .flex_shrink_0()
                        .w(px(ASSISTANT_PANEL_WIDTH))
                        .min_w(px(ASSISTANT_PANEL_WIDTH))
                        .max_w(px(ASSISTANT_PANEL_WIDTH))
                        .h_full()
                        .bg(color(theme::BG_COLUMN))
                        .border_l_1()
                        .border_color(cx.theme().sidebar_border)
                        .child(match panel {
                            AssistantPanel::AIStats => self.app_entity.update(cx, |app, cx| {
                                gpui::AnyView::from(app.ai_stats_sidebar_view(cx))
                                    .into_any_element()
                            }),
                            AssistantPanel::SSH => self.app_entity.update(cx, |app, cx| {
                                gpui::AnyView::from(app.ssh_sidebar_view(cx)).into_any_element()
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
                },
            )
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
    }
}

fn assistant_panel_key(panel: Option<AssistantPanel>) -> &'static str {
    match panel {
        Some(AssistantPanel::AIStats) => "ai_stats",
        Some(AssistantPanel::SSH) => "ssh",
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
    layout_key: String,
    bottom_ratio: f64,
    main_panes: Vec<TerminalPaneViewSnapshot>,
    bottom_tabs: Vec<TerminalBottomTabViewSnapshot>,
    active_bottom: Option<TerminalPaneViewSnapshot>,
}

#[derive(Clone)]
struct TerminalPaneViewSnapshot {
    view: Option<gpui::Entity<TerminalView>>,
}

impl PartialEq for TerminalPaneViewSnapshot {
    fn eq(&self, other: &Self) -> bool {
        match (&self.view, &other.view) {
            (Some(left), Some(right)) => left.entity_id() == right.entity_id(),
            (None, None) => true,
            _ => false,
        }
    }
}

#[derive(Clone, PartialEq)]
struct TerminalBottomTabViewSnapshot {
    id: usize,
    label: String,
    active: bool,
}

pub(in crate::app) struct TerminalWorkspaceView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: TerminalWorkspaceSnapshot,
    tab_scroll_handle: ScrollHandle,
}

impl TerminalWorkspaceView {
    fn new(app_entity: gpui::Entity<CoduxApp>, snapshot: TerminalWorkspaceSnapshot) -> Self {
        Self {
            app_entity,
            snapshot,
            tab_scroll_handle: ScrollHandle::new(),
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
        if active_terminal_bottom_tab_id(&self.snapshot.bottom_tabs)
            != active_terminal_bottom_tab_id(&snapshot.bottom_tabs)
            && let Some(index) = snapshot.bottom_tabs.iter().position(|tab| tab.active)
        {
            self.tab_scroll_handle.scroll_to_item(index);
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for TerminalWorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_bottom_tabs = !self.snapshot.bottom_tabs.is_empty();
        let bottom_ratio = clamp_terminal_bottom_ratio(self.snapshot.bottom_ratio);
        let bottom_size = terminal_bottom_panel_size(bottom_ratio);
        let main_size = (TERMINAL_SPLIT_BASE_SIZE - bottom_size).max(px(220.0));
        let split_id = SharedString::from(format!(
            "workspace-terminal-split-{}",
            terminal_layout_key_for_element_id(&self.snapshot.layout_key)
        ));
        let main = terminal_main_split_area(
            self.app_entity.clone(),
            self.snapshot.main_panes.clone(),
            cx,
        );
        let bottom = terminal_bottom_tabs_area(
            self.app_entity.clone(),
            self.snapshot.bottom_tabs.clone(),
            self.snapshot.active_bottom.clone(),
            self.tab_scroll_handle.clone(),
            cx,
        );

        let base = div()
            .flex()
            .flex_col()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .bg(color(theme::BG_TERMINAL));

        if !has_bottom_tabs {
            return base
                .child(
                    div()
                        .flex_1()
                        .flex_basis(px(0.0))
                        .min_w_0()
                        .min_h_0()
                        .w_full()
                        .child(main),
                )
                .child(div().h(TERMINAL_BOTTOM_TAB_BAR_HEIGHT).child(bottom));
        }

        base.child(
            v_resizable(split_id)
                .on_resize({
                    let app_entity = self.app_entity.clone();
                    let layout_key = self.snapshot.layout_key.clone();
                    move |state, window, cx| {
                        let sizes = state.read(cx).sizes().clone();
                        let Some(ratio) = terminal_bottom_ratio_from_sizes(&sizes) else {
                            return;
                        };
                        window.defer(cx, {
                            let app_entity = app_entity.clone();
                            let layout_key = layout_key.clone();
                            move |_window, cx| {
                                let _ = app_entity.update(cx, |app, cx| {
                                    app.update_terminal_bottom_ratio(layout_key, ratio, cx);
                                });
                            }
                        });
                    }
                })
                .child(
                    resizable_panel()
                        .size(main_size)
                        .size_range(px(220.0)..px(900.0))
                        .child(main),
                )
                .child(
                    resizable_panel()
                        .size(bottom_size)
                        .size_range(TERMINAL_BOTTOM_PANEL_MIN_SIZE..px(520.0))
                        .child(bottom),
                ),
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

    pub(in crate::app) fn update_terminal_workspace_view(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self
            .workspace_body_view
            .as_ref()
            .and_then(|view| view.read(cx).terminal_workspace_view.clone())
        else {
            return;
        };
        let snapshot = self.terminal_workspace_snapshot();
        view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
    }

    pub(in crate::app) fn terminal_workspace_snapshot(&self) -> TerminalWorkspaceSnapshot {
        let main_panes = self
            .main_terminal()
            .map(|tab| {
                tab.panes
                    .iter()
                    .map(|slot| TerminalPaneViewSnapshot {
                        view: slot.pane.as_ref().map(|pane| pane.view.clone()),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let bottom_tabs = self
            .bottom_terminals()
            .map(|terminal| TerminalBottomTabViewSnapshot {
                id: terminal.id,
                label: terminal.label.clone(),
                active: terminal.id == self.active_terminal_id,
            })
            .collect::<Vec<_>>();

        let active_bottom = self
            .active_bottom_terminal()
            .and_then(|tab| tab.panes.first())
            .map(|slot| TerminalPaneViewSnapshot {
                view: slot.pane.as_ref().map(|pane| pane.view.clone()),
            });

        TerminalWorkspaceSnapshot {
            loading: self.terminal_layout_loading,
            layout_key: super::ai_runtime_status::current_terminal_layout_storage_key(&self.state)
                .unwrap_or_default(),
            bottom_ratio: clamp_terminal_bottom_ratio(self.state.terminal_layout.bottom_ratio),
            main_panes,
            bottom_tabs,
            active_bottom,
        }
    }
}

fn active_terminal_bottom_tab_id(tabs: &[TerminalBottomTabViewSnapshot]) -> Option<usize> {
    tabs.iter().find(|tab| tab.active).map(|tab| tab.id)
}

const TERMINAL_SPLIT_BASE_SIZE: Pixels = px(640.0);
const TERMINAL_BOTTOM_TAB_BAR_HEIGHT: Pixels = px(40.0);
const TERMINAL_BOTTOM_PANEL_MIN_SIZE: Pixels = px(128.0);

fn terminal_bottom_panel_size(ratio: f64) -> Pixels {
    px((TERMINAL_SPLIT_BASE_SIZE.as_f32() as f64 * ratio) as f32)
        .clamp(TERMINAL_BOTTOM_PANEL_MIN_SIZE, px(360.0))
}

fn terminal_bottom_ratio_from_sizes(sizes: &[Pixels]) -> Option<f64> {
    let [main, bottom, ..] = sizes else {
        return None;
    };
    let total = main.as_f32() + bottom.as_f32();
    if total <= 1.0 {
        return None;
    }
    Some(clamp_terminal_bottom_ratio(
        (bottom.as_f32() / total) as f64,
    ))
}

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
    panes: Vec<TerminalPaneViewSnapshot>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    if panes.is_empty() {
        return div()
            .flex_1()
            .size_full()
            .bg(color(theme::BG_TERMINAL))
            .into_any_element();
    }

    let pane_count = panes.len();
    div()
        .flex()
        .flex_1()
        .flex_basis(px(0.0))
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .children(panes.into_iter().enumerate().map(move |(index, slot)| {
            terminal_pane(app_entity.clone(), index, pane_count, slot, cx)
        }))
        .into_any_element()
}

fn terminal_pane(
    app_entity: gpui::Entity<CoduxApp>,
    index: usize,
    pane_count: usize,
    slot: TerminalPaneViewSnapshot,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    let close_id = SharedString::from(format!("terminal-pane-close-{index}"));
    let float_id = SharedString::from(format!("terminal-pane-float-{index}"));
    let add_id = SharedString::from(format!("terminal-pane-add-{index}"));

    div()
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
        .border_l_1()
        .border_color(color(if index == 0 {
            theme::BG_TERMINAL
        } else {
            theme::BORDER_SOFT
        }))
        .child(
            div()
                .flex_1()
                .flex_basis(px(0.0))
                .min_w_0()
                .min_h_0()
                .child(match slot.view {
                    Some(view) => gpui::AnyView::from(view).into_any_element(),
                    None => div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(color(theme::TEXT_DIM))
                        .child("Terminal mounting...")
                        .into_any_element(),
                }),
        )
        .child(
            div()
                .absolute()
                .top_2()
                .right_2()
                .flex()
                .items_center()
                .gap_1()
                .child(terminal_pane_control_button(
                    app_entity.clone(),
                    float_id,
                    HeroIconName::ArrowTopRightOnSquare,
                    "浮窗",
                    pane_count > 1,
                    cx,
                    move |app, window, cx| app.float_terminal_pane(index, window, cx),
                ))
                .child(terminal_pane_control_button(
                    app_entity.clone(),
                    add_id,
                    HeroIconName::Plus,
                    "新建分屏",
                    true,
                    cx,
                    |app, window, cx| app.split_terminal(window, cx),
                ))
                .child(terminal_pane_control_button(
                    app_entity,
                    close_id,
                    HeroIconName::XMark,
                    "关闭分屏",
                    pane_count > 1,
                    cx,
                    move |app, window, cx| app.close_terminal_pane(index, window, cx),
                )),
        )
        .into_any_element()
}

fn terminal_bottom_tabs_area(
    app_entity: gpui::Entity<CoduxApp>,
    tabs: Vec<TerminalBottomTabViewSnapshot>,
    active: Option<TerminalPaneViewSnapshot>,
    tab_scroll_handle: ScrollHandle,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    let has_bottom_tabs = active.is_some();
    let tab_order = tabs
        .iter()
        .map(|tab| tab.id.to_string())
        .collect::<Vec<_>>();

    div()
        .flex()
        .flex_col()
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .child(
            div()
                .h(TERMINAL_BOTTOM_TAB_BAR_HEIGHT)
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .when(!has_bottom_tabs, |this| {
                            this.child(
                                div()
                                    .px_2()
                                    .text_size(rems(0.75))
                                    .line_height(rems(1.0))
                                    .text_color(cx.theme().secondary_foreground)
                                    .child("终端"),
                            )
                        })
                        .child(
                            div()
                                .id("terminal-bottom-tab-scroll")
                                .flex()
                                .h_full()
                                .min_w_0()
                                .items_center()
                                .gap_1()
                                .overflow_x_scroll()
                                .track_scroll(&tab_scroll_handle)
                                .children(tabs.into_iter().map(|tab| {
                                    terminal_bottom_tab_button(
                                        app_entity.clone(),
                                        tab,
                                        tab_order.clone(),
                                        cx,
                                    )
                                    .into_any_element()
                                })),
                        ),
                )
                .child(terminal_bottom_add_button(app_entity.clone(), cx)),
        )
        .when_some(active, |this, tab| {
            this.child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .min_h_0()
                    .overflow_hidden()
                    .child(terminal_bottom_content(tab)),
            )
        })
        .into_any_element()
}

fn terminal_bottom_content(tab: TerminalPaneViewSnapshot) -> AnyElement {
    div()
        .size_full()
        .min_h_0()
        .overflow_hidden()
        .child(match tab.view {
            Some(view) => gpui::AnyView::from(view).into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(color(theme::TEXT_DIM))
                .child("Terminal mounting...")
                .into_any_element(),
        })
        .into_any_element()
}

fn terminal_pane_control_button(
    app_entity: gpui::Entity<CoduxApp>,
    id: SharedString,
    icon: HeroIconName,
    tooltip: &'static str,
    enabled: bool,
    cx: &mut Context<TerminalWorkspaceView>,
    on_click: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let text_color = if enabled {
        cx.theme().secondary_foreground
    } else {
        color(theme::TEXT_DIM)
    };
    let button = codux_tooltip_container(app_entity.clone(), id, tooltip)
        .size(px(28.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_sm()
        .text_color(text_color)
        .child(Icon::new(icon).size_3p5().text_color(text_color));

    if enabled {
        let on_click = std::rc::Rc::new(on_click);
        button
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().secondary_hover))
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

fn terminal_bottom_tab_button(
    app_entity: gpui::Entity<CoduxApp>,
    tab: TerminalBottomTabViewSnapshot,
    tab_order: Vec<String>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> impl IntoElement {
    let terminal_id = tab.id;
    let target_terminal_id = terminal_id;
    let drag_label = tab.label.clone();
    let drag_active = tab.active;
    let text_color = if tab.active {
        cx.theme().foreground
    } else {
        cx.theme().secondary_foreground
    };
    let background = if tab.active {
        cx.theme().secondary_hover
    } else {
        cx.theme().transparent
    };
    terminal_bottom_tab_container(text_color, background)
        .id(SharedString::from(format!(
            "terminal-bottom-tab-{terminal_id}"
        )))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_double_click(cx.listener({
            let app_entity = app_entity.clone();
            let label = tab.label.clone();
            move |_view, _event, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                let label = label.clone();
                defer_terminal_workspace_app_update(
                    app_entity.clone(),
                    window,
                    cx,
                    move |app, window, app_cx| {
                        app.open_terminal_tab_editor_window(
                            terminal_id,
                            label.clone(),
                            window,
                            app_cx,
                        );
                    },
                );
            }
        }))
        .on_drag(
            TerminalBottomTabDrag {
                terminal_id,
                label: drag_label,
                active: drag_active,
            },
            move |drag, _, _, cx| {
                cx.new(|_| TerminalBottomTabDrag {
                    terminal_id: drag.terminal_id,
                    label: drag.label.clone(),
                    active: drag.active,
                })
            },
        )
        .drag_over::<TerminalBottomTabDrag>(move |this, _drag, _window, _cx| this)
        .on_drop(cx.listener({
            let app_entity = app_entity.clone();
            move |_view, drag: &TerminalBottomTabDrag, window, cx| {
                let Some(next_ids) = reordered_ids(
                    &tab_order,
                    drag.terminal_id.to_string().as_str(),
                    target_terminal_id.to_string().as_str(),
                ) else {
                    return;
                };
                let next_terminal_ids = next_ids
                    .into_iter()
                    .filter_map(|id| id.parse::<usize>().ok())
                    .collect::<Vec<_>>();
                defer_terminal_workspace_app_update(
                    app_entity.clone(),
                    window,
                    cx,
                    move |app, _window, app_cx| {
                        app.reorder_bottom_terminal_tabs(next_terminal_ids, app_cx);
                    },
                );
                cx.stop_propagation();
            }
        }))
        .on_click(cx.listener({
            let app_entity = app_entity.clone();
            move |_view, _event, window, cx| {
                defer_terminal_workspace_app_update(
                    app_entity.clone(),
                    window,
                    cx,
                    move |app, window, app_cx| app.select_terminal_tab(terminal_id, window, app_cx),
                );
            }
        }))
        .child(terminal_bottom_tab_label(tab.label))
        .child(
            div()
                .id(SharedString::from(format!(
                    "terminal-bottom-tab-close-{terminal_id}"
                )))
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .text_color(cx.theme().secondary_foreground)
                .hover(|style| style.bg(cx.theme().secondary_hover))
                .on_click(cx.listener(move |_view, _event, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                    defer_terminal_workspace_app_update(
                        app_entity.clone(),
                        window,
                        cx,
                        move |app, window, app_cx| {
                            app.close_terminal_tab(terminal_id, window, app_cx)
                        },
                    );
                }))
                .child(Icon::new(HeroIconName::XMark).size_3()),
        )
}

fn terminal_bottom_tab_container(text_color: gpui::Hsla, background: gpui::Hsla) -> gpui::Div {
    div()
        .h(px(32.0))
        .px_3()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap_2()
        .rounded_md()
        .text_color(text_color)
        .bg(background)
}

fn terminal_bottom_tab_label(label: String) -> AnyElement {
    div()
        .text_size(rems(0.75))
        .line_height(rems(0.875))
        .child(label)
        .into_any_element()
}

fn terminal_bottom_add_button(
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> impl IntoElement {
    div()
        .id("terminal-bottom-tab-add")
        .size(px(26.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .text_color(cx.theme().secondary_foreground)
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_click(cx.listener(move |_view, _event, window, cx| {
            defer_terminal_workspace_app_update(
                app_entity.clone(),
                window,
                cx,
                |app, window, app_cx| app.add_terminal_tab(window, app_cx),
            );
        }))
        .child(Icon::new(HeroIconName::Plus).size_3p5())
}

fn defer_terminal_workspace_app_update(
    app_entity: gpui::Entity<CoduxApp>,
    window: &mut Window,
    cx: &mut Context<TerminalWorkspaceView>,
    update: impl FnOnce(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) {
    defer_codux_app_update(app_entity, window, cx, update);
}
