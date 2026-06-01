use super::*;
use crate::app::app_state::FileEditorTab;
use crate::app::ui_helpers::codux_tooltip_container;
use gpui_component::input::{Redo, Search, TabSize, Undo};

const FILE_EDITOR_TAB_BAR_HEIGHT: f32 = 38.0;
const FILE_EDITOR_TOOLBAR_HEIGHT: f32 = 56.0;
const FILE_EDITOR_CHROME_HEIGHT: f32 = FILE_EDITOR_TAB_BAR_HEIGHT + FILE_EDITOR_TOOLBAR_HEIGHT;

#[derive(Clone)]
pub(in crate::app) struct FileEditorWorkspaceSnapshot {
    tabs: Vec<FileEditorTab>,
    active_path: Option<String>,
    active_tab: Option<FileEditorTab>,
    active_editor: Option<gpui::Entity<InputState>>,
}

impl PartialEq for FileEditorWorkspaceSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.tabs == other.tabs
            && self.active_path == other.active_path
            && self.active_tab == other.active_tab
            && self.active_editor.as_ref().map(|editor| editor.entity_id())
                == other
                    .active_editor
                    .as_ref()
                    .map(|editor| editor.entity_id())
    }
}

pub(in crate::app) struct FileEditorWorkspaceView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: FileEditorWorkspaceSnapshot,
    chrome_view: Option<gpui::Entity<FileEditorChromeView>>,
    content_view: Option<gpui::Entity<FileEditorContentView>>,
}

impl FileEditorWorkspaceView {
    pub(in crate::app) fn new(
        app_entity: gpui::Entity<CoduxApp>,
        snapshot: FileEditorWorkspaceSnapshot,
    ) -> Self {
        Self {
            app_entity,
            snapshot,
            chrome_view: None,
            content_view: None,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: FileEditorWorkspaceSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }

    fn chrome_view(
        &mut self,
        snapshot: FileEditorWorkspaceSnapshot,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<FileEditorChromeView> {
        if let Some(view) = &self.chrome_view {
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }
        let view = cx.new(|_| FileEditorChromeView::new(self.app_entity.clone(), snapshot));
        self.chrome_view = Some(view.clone());
        view
    }

    fn content_view(
        &mut self,
        editor: Option<gpui::Entity<InputState>>,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<FileEditorContentView> {
        if let Some(view) = &self.content_view {
            view.update(cx, |view, cx| view.set_editor(editor, cx));
            return view.clone();
        }
        let view = cx.new(|_| FileEditorContentView::new(editor));
        self.content_view = Some(view.clone());
        view
    }

}

impl Render for FileEditorWorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot.clone();
        let chrome_view = self.chrome_view(snapshot.clone(), cx);
        let content_view = self.content_view(snapshot.active_editor.clone(), cx);
        file_editor_workspace(
            self.app_entity.clone(),
            snapshot,
            chrome_view,
            content_view,
            window,
            cx,
        )
    }
}

pub(in crate::app) struct FileEditorChromeView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: FileEditorWorkspaceSnapshot,
}

impl FileEditorChromeView {
    fn new(app_entity: gpui::Entity<CoduxApp>, snapshot: FileEditorWorkspaceSnapshot) -> Self {
        Self {
            app_entity,
            snapshot,
        }
    }

    fn set_snapshot(
        &mut self,
        snapshot: FileEditorWorkspaceSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }

    fn dispatch_active_file_editor_action(
        &self,
        action: impl gpui::Action + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app_entity = self.app_entity.clone();
        cx.update_entity(&app_entity, |app, cx| {
            if let Some(editor) = app.active_file_editor_state() {
                editor.update(cx, |state, cx| state.focus(window, cx));
                window.dispatch_action(Box::new(action), cx);
            }
        });
    }
}

impl Render for FileEditorChromeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let FileEditorWorkspaceSnapshot {
            tabs,
            active_path,
            active_tab,
            active_editor: _,
        } = self.snapshot.clone();

        div()
            .flex_none()
            .w_full()
            .child(file_editor_tab_bar(
                self.app_entity.clone(),
                tabs,
                active_path,
                cx,
            ))
            .child(file_editor_toolbar(
                self.app_entity.clone(),
                active_tab,
                cx,
            ))
    }
}

pub(in crate::app) struct FileEditorContentView {
    editor: Option<gpui::Entity<InputState>>,
}

impl FileEditorContentView {
    fn new(editor: Option<gpui::Entity<InputState>>) -> Self {
        Self { editor }
    }

    fn set_editor(
        &mut self,
        editor: Option<gpui::Entity<InputState>>,
        cx: &mut Context<Self>,
    ) {
        if self.editor.as_ref().map(|editor| editor.entity_id())
            == editor.as_ref().map(|editor| editor.entity_id())
        {
            return;
        }
        self.editor = editor;
        cx.notify();
    }
}

impl Render for FileEditorContentView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .size_full()
            .when_some(self.editor.clone(), |this, editor| {
                this.child(
                    Input::new(&editor)
                        .appearance(false)
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_size(cx.theme().mono_font_size)
                        .size_full(),
                )
            })
    }
}

impl CoduxApp {
    pub(super) fn open_file_editor_tab(
        &mut self,
        relative_path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project to open file".to_string();
            self.invalidate_status_bar(cx);
            return;
        };
        let key = self.file_editor_state_key(&relative_path);

        if !self
            .file_editor_tabs
            .iter()
            .any(|tab| tab.relative_path == relative_path)
        {
            match self
                .runtime_service
                .read_project_file_edit_buffer(&worktree_path, &relative_path)
            {
                Ok((content, editable)) => {
                    let language = file_language_for_path(&relative_path).to_string();
                    self.ensure_file_editor_state(
                        key.clone(),
                        relative_path.clone(),
                        language.clone(),
                        content,
                        window,
                        cx,
                    );
                    self.file_editor_tabs.push(FileEditorTab {
                        label: file_editor_label(&relative_path),
                        relative_path: relative_path.clone(),
                        editable,
                        dirty: false,
                        language,
                    });
                }
                Err(error) => {
                    self.status_message = format!("failed to open file: {error}");
                    self.invalidate_status_bar(cx);
                    return;
                }
            }
        }

        self.workspace_view = WorkspaceView::Files;
        self.assistant_panel = Some(AssistantPanel::FileManager);
        self.active_file_editor_tab = Some(relative_path.clone());
        self.set_single_file_selection(relative_path.clone());
        if let Some(editor) = self.file_editor_states.get(&key) {
            editor.update(cx, |state, cx| state.focus(window, cx));
        }
        self.save_current_worktree_view_state();
        self.persist_file_editor_layout_async(cx);
        self.status_message = format!("file opened: {relative_path}");
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceAssistant,
                UiRegion::WorkspaceBody,
                UiRegion::FileSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(super) fn select_file_editor_tab(
        &mut self,
        relative_path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_file_editor_tab = Some(relative_path.clone());
        self.set_single_file_selection(relative_path.clone());
        if let Some(editor) = self.active_file_editor_state() {
            editor.update(cx, |state, cx| state.focus(window, cx));
        }
        self.save_current_worktree_view_state();
        self.persist_file_editor_layout_async(cx);
        self.invalidate_ui(
            cx,
            [UiRegion::WorkspaceBody, UiRegion::FileSidebar, UiRegion::StatusBar],
        );
    }

    pub(super) fn close_file_editor_tab(
        &mut self,
        relative_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .file_editor_tabs
            .iter()
            .position(|tab| tab.relative_path == relative_path)
        else {
            return;
        };
        self.file_editor_tabs.remove(index);
        let key = self.file_editor_state_key(&relative_path);
        self.file_editor_states.remove(&key);

        if self.active_file_editor_tab.as_deref() == Some(relative_path.as_str()) {
            self.active_file_editor_tab = self
                .file_editor_tabs
                .get(index)
                .or_else(|| {
                    index
                        .checked_sub(1)
                        .and_then(|prev| self.file_editor_tabs.get(prev))
                })
                .map(|tab| tab.relative_path.clone());
        }
        self.save_current_worktree_view_state();
        self.persist_file_editor_layout_async(cx);
        self.invalidate_ui(
            cx,
            [UiRegion::WorkspaceBody, UiRegion::FileSidebar, UiRegion::StatusBar],
        );
    }

    pub(super) fn mark_file_editor_dirty(
        &mut self,
        relative_path: &str,
        dirty: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self
            .file_editor_tabs
            .iter_mut()
            .find(|tab| tab.relative_path == relative_path)
        {
            tab.dirty = dirty;
        }
        if self.active_file_editor_tab.as_deref() == Some(relative_path) {
            self.file_dirty = dirty;
        }
        if self.workspace_view == WorkspaceView::Files {
            self.invalidate_workspace_body(cx);
        }
    }

    pub(super) fn active_file_editor_state(&self) -> Option<gpui::Entity<InputState>> {
        let relative_path = self.active_file_editor_tab.as_deref()?;
        self.file_editor_states
            .get(&self.file_editor_state_key(relative_path))
            .cloned()
    }

    pub(super) fn ensure_visible_file_editor_states(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tabs = self.file_editor_tabs.clone();
        for tab in tabs {
            let key = self.file_editor_state_key(&tab.relative_path);
            if self.file_editor_states.contains_key(&key) {
                continue;
            }
            let content = self
                .selected_worktree_path()
                .and_then(|path| {
                    self.runtime_service
                        .read_project_file_edit_buffer(&path, &tab.relative_path)
                        .ok()
                        .map(|(content, _)| content)
                })
                .unwrap_or_default();
            self.ensure_file_editor_state(
                key,
                tab.relative_path.clone(),
                tab.language,
                content,
                window,
                cx,
            );
        }
    }

    pub(super) fn ensure_file_editor_state(
        &mut self,
        key: String,
        relative_path: String,
        language: String,
        content: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<InputState> {
        if let Some(state) = self.file_editor_states.get(&key) {
            return state.clone();
        }

        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor(language)
                .multi_line(true)
                .tab_size(TabSize {
                    tab_size: 4,
                    ..Default::default()
                })
                .default_value(content)
        });
        cx.subscribe_in(&state, window, move |app, _state, event, _window, cx| {
            if matches!(event, InputEvent::Change) {
                app.mark_file_editor_dirty(&relative_path, true, cx);
            }
        })
        .detach();
        self.file_editor_states.insert(key, state.clone());
        state
    }

    pub(super) fn apply_file_editor_layout(&mut self, layout: FileEditorLayoutSummary) {
        if layout.tabs.is_empty() {
            return;
        }
        self.file_editor_tabs = layout
            .tabs
            .into_iter()
            .map(|tab| FileEditorTab {
                label: tab.label,
                relative_path: tab.path,
                editable: true,
                dirty: false,
                language: if tab.language.trim().is_empty() {
                    "text".to_string()
                } else {
                    tab.language
                },
            })
            .collect();
        self.active_file_editor_tab = layout
            .active_path
            .filter(|active| {
                self.file_editor_tabs
                    .iter()
                    .any(|tab| tab.relative_path == *active)
            })
            .or_else(|| {
                self.file_editor_tabs
                    .first()
                    .map(|tab| tab.relative_path.clone())
            });
        if let Some(active) = self.active_file_editor_tab.clone() {
            self.set_single_file_selection(active);
        }
        self.save_current_worktree_view_state();
    }

    pub(super) fn load_current_file_editor_layout(&mut self) {
        let Some(owner_id) = super::ai_runtime_status::terminal_layout_owner_id(&self.state) else {
            return;
        };
        let layout = self
            .runtime_service
            .reload_file_editor_layout(Some(&owner_id));
        self.apply_file_editor_layout(layout);
    }

    pub(super) fn persist_file_editor_layout_async(&self, cx: &mut Context<Self>) {
        let Some(owner_id) = super::ai_runtime_status::terminal_layout_owner_id(&self.state) else {
            return;
        };
        let tabs = self
            .file_editor_tabs
            .iter()
            .map(|tab| FileEditorTabSummary {
                path: tab.relative_path.clone(),
                label: tab.label.clone(),
                language: tab.language.clone(),
            })
            .collect::<Vec<_>>();
        let active_path = self.active_file_editor_tab.clone();
        let runtime_service = self.runtime_service.clone();
        cx.spawn(async move |_: gpui::WeakEntity<Self>, _cx| {
            let _ = codux_runtime::async_runtime::spawn_blocking(move || {
                runtime_service.save_file_editor_layout(&owner_id, tabs, active_path)
            })
            .await;
        })
        .detach();
    }

    pub(super) fn file_editor_state_key(&self, relative_path: &str) -> String {
        if let Some(key) = worktree_view_store_key(&self.state) {
            format!("{}:{}:{}", key.project_id, key.worktree_id, relative_path)
        } else {
            relative_path.to_string()
        }
    }

    pub(in crate::app) fn file_editor_workspace_snapshot(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> FileEditorWorkspaceSnapshot {
        self.ensure_visible_file_editor_states(window, cx);
        let tabs = self.file_editor_tabs.clone();
        let active_path = self.active_file_editor_tab.clone();
        let active_tab = self
            .file_editor_tabs
            .iter()
            .find(|tab| Some(tab.relative_path.as_str()) == active_path.as_deref())
            .cloned();
        let active_editor = self.active_file_editor_state();
        FileEditorWorkspaceSnapshot {
            tabs,
            active_path,
            active_tab,
            active_editor,
        }
    }
}

pub(in crate::app) fn file_editor_workspace(
    _app_entity: gpui::Entity<CoduxApp>,
    snapshot: FileEditorWorkspaceSnapshot,
    chrome_view: gpui::Entity<FileEditorChromeView>,
    content_view: gpui::Entity<FileEditorContentView>,
    _window: &mut Window,
    cx: &mut Context<FileEditorWorkspaceView>,
) -> impl IntoElement {
    let FileEditorWorkspaceSnapshot {
        tabs,
        active_path: _,
        active_tab: _,
        active_editor: _,
    } = snapshot;

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .size_full()
        .bg(color(theme::BG_TERMINAL))
        .when(tabs.is_empty(), |this| {
            this.child(
                div()
                    .size_full()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        Icon::new(HeroIconName::DocumentText)
                            .size_5()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .child("Double-click a file to open it"),
                    ),
            )
        })
        .when(!tabs.is_empty(), |this| {
            this.child(
                gpui::AnyView::from(chrome_view).cached(
                    gpui::StyleRefinement::default()
                        .flex_none()
                        .w_full()
                        .h(px(FILE_EDITOR_CHROME_HEIGHT)),
                ),
            )
            .child(
                gpui::AnyView::from(content_view).cached(
                    gpui::StyleRefinement::default()
                        .flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .min_h(px(0.0))
                        .size_full(),
                ),
            )
        })
}

fn file_editor_tab_bar(
    app_entity: gpui::Entity<CoduxApp>,
    tabs: Vec<FileEditorTab>,
    active_path: Option<String>,
    cx: &mut Context<FileEditorChromeView>,
) -> impl IntoElement {
    div()
        .h(px(FILE_EDITOR_TAB_BAR_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap_1()
        .px(px(10.0))
        .py(px(5.0))
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(color(theme::BG_TERMINAL))
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_1()
                .overflow_x_hidden()
                .children(tabs.into_iter().map(|tab| {
                    let active = active_path.as_deref() == Some(tab.relative_path.as_str());
                    file_editor_tab_button(app_entity.clone(), tab, active, cx)
                })),
        )
}

fn file_editor_tab_button(
    app_entity: gpui::Entity<CoduxApp>,
    tab: FileEditorTab,
    active: bool,
    cx: &mut Context<FileEditorChromeView>,
) -> AnyElement {
    let select_path = tab.relative_path.clone();
    let close_path = tab.relative_path.clone();
    let tab_button_id = SharedString::from(format!("file-editor-tab-{close_path}"));
    let close_button_id = SharedString::from(format!("file-editor-close-{close_path}"));
    let active_bg = color(theme::TEXT).opacity(0.07);
    let hover_bg = cx.theme().secondary_hover;

    div()
        .id(tab_button_id)
        .h(px(28.0))
        .min_w(px(96.0))
        .max_w(px(220.0))
        .flex_none()
        .flex()
        .items_center()
        .rounded(px(6.0))
        .text_size(px(12.5))
        .line_height(px(16.0))
        .text_color(if tab.dirty {
            color(theme::TEXT)
        } else {
            cx.theme().secondary_foreground
        })
        .when(active, |this| this.bg(active_bg))
        .cursor_pointer()
        .hover(move |style| style.bg(hover_bg))
        .on_click(cx.listener(move |_app, _event, window, cx| {
            cx.update_entity(&app_entity, |app, cx| {
                app.select_file_editor_tab(select_path.clone(), window, cx);
            });
        }))
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .items_center()
                .gap_2()
                .pl(px(10.0))
                .pr(px(4.0))
                .child(
                    div()
                        .size(px(6.0))
                        .flex_none()
                        .rounded_full()
                        .when(tab.dirty, |this| this.bg(color(theme::ORANGE)))
                        .when(!tab.dirty, |this| {
                            this.bg(color(theme::TEXT_DIM).opacity(0.0))
                        }),
                )
                .child(
                    Icon::new(HeroIconName::DocumentText)
                        .size_3()
                        .text_color(cx.theme().secondary_foreground),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(tab.label),
                ),
        )
        .child(
            div()
                .id(close_button_id)
                .mr(px(5.0))
                .size(px(18.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .text_color(cx.theme().muted_foreground)
                .hover(|style| style.bg(cx.theme().secondary_hover))
                .child(
                    Icon::new(HeroIconName::XMark)
                        .size_3()
                        .text_color(cx.theme().muted_foreground),
                )
                .on_click(cx.listener(move |view, _event, window, cx| {
                    let app_entity = view.app_entity.clone();
                    cx.update_entity(&app_entity, |app, cx| {
                        app.close_file_editor_tab(close_path.clone(), window, cx);
                    });
                    cx.stop_propagation();
                })),
        )
        .into_any_element()
}

fn file_editor_toolbar(
    app_entity: gpui::Entity<CoduxApp>,
    active_tab: Option<FileEditorTab>,
    cx: &mut Context<FileEditorChromeView>,
) -> impl IntoElement {
    let active_dirty = active_tab.as_ref().is_some_and(|tab| tab.dirty);
    let read_only = active_tab.as_ref().is_none_or(|tab| !tab.editable);
    let (active_label, active_parent) = active_tab
        .as_ref()
        .map(|tab| {
            (
                tab.label.clone(),
                file_editor_parent_label(&tab.relative_path, &tab.label),
            )
        })
        .unwrap_or_default();

    div()
        .h(px(FILE_EDITOR_TOOLBAR_HEIGHT))
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .px(px(18.0))
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().title_bar)
        .child(
            div()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(active_label),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_DIM))
                        .truncate()
                        .child(active_parent),
                ),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(file_editor_toolbar_button(
                    app_entity.clone(),
                    "file-editor-save",
                    HeroIconName::CheckCircle,
                    file_editor_i18n(app_entity.clone(), cx, "common.save", "Save"),
                    if active_dirty {
                        color(theme::GREEN)
                    } else {
                        cx.theme().secondary_foreground
                    },
                    !active_dirty || read_only,
                    cx,
                    |view, _event, window, cx| {
                        let app_entity = view.app_entity.clone();
                        cx.update_entity(&app_entity, |app, cx| {
                            app.save_selected_file_preview(window, cx);
                        });
                    },
                ))
                .child(file_editor_toolbar_button(
                    app_entity.clone(),
                    "file-editor-undo",
                    HeroIconName::ArrowUturnLeft,
                    file_editor_i18n(app_entity.clone(), cx, "common.undo", "Undo"),
                    cx.theme().secondary_foreground,
                    read_only,
                    cx,
                    |view, _event, window, cx| {
                        view.dispatch_active_file_editor_action(Undo, window, cx);
                    },
                ))
                .child(file_editor_toolbar_button(
                    app_entity.clone(),
                    "file-editor-redo",
                    HeroIconName::ArrowUturnRight,
                    file_editor_i18n(app_entity.clone(), cx, "common.redo", "Redo"),
                    cx.theme().secondary_foreground,
                    read_only,
                    cx,
                    |view, _event, window, cx| {
                        view.dispatch_active_file_editor_action(Redo, window, cx);
                    },
                ))
                .child(file_editor_toolbar_button(
                    app_entity.clone(),
                    "file-editor-search",
                    HeroIconName::MagnifyingGlass,
                    file_editor_i18n(app_entity.clone(), cx, "shortcut.editor.search", "Search File"),
                    cx.theme().secondary_foreground,
                    false,
                    cx,
                    |view, _event, window, cx| {
                        view.dispatch_active_file_editor_action(Search, window, cx);
                    },
                ))
                .child(file_editor_toolbar_button(
                    app_entity.clone(),
                    "file-editor-copy-path",
                    HeroIconName::ClipboardDocument,
                    file_editor_i18n(app_entity.clone(), cx, "files.panel.copy_path", "Copy Path"),
                    cx.theme().secondary_foreground,
                    false,
                    cx,
                    |view, _event, _window, cx| {
                        let app_entity = view.app_entity.clone();
                        cx.update_entity(&app_entity, |app, cx| {
                            app.copy_selected_file_paths_to_clipboard(cx);
                        });
                    },
                ))
                .child(file_editor_toolbar_button(
                    app_entity.clone(),
                    "file-editor-reload",
                    HeroIconName::ArrowPath,
                    file_editor_i18n(app_entity.clone(), cx, "common.reload", "Reload"),
                    cx.theme().secondary_foreground,
                    false,
                    cx,
                    |view, _event, window, cx| {
                        let app_entity = view.app_entity.clone();
                        cx.update_entity(&app_entity, |app, cx| {
                            app.reload_active_file_editor_tab(window, cx);
                        });
                    },
                ))
                .child(file_editor_toolbar_button(
                    app_entity.clone(),
                    "file-editor-reveal",
                    HeroIconName::Folder,
                    file_editor_i18n(
                        app_entity.clone(),
                        cx,
                        "files.panel.reveal_finder",
                        "Show in File Manager",
                    ),
                    cx.theme().secondary_foreground,
                    false,
                    cx,
                    |view, _event, window, cx| {
                        let app_entity = view.app_entity.clone();
                        cx.update_entity(&app_entity, |app, cx| {
                            app.reveal_selected_file_entry(window, cx);
                        });
                    },
                )),
        )
}

fn file_editor_toolbar_button(
    app_entity: gpui::Entity<CoduxApp>,
    id: &'static str,
    icon: HeroIconName,
    tooltip: String,
    icon_color: gpui::Hsla,
    disabled: bool,
    cx: &mut Context<FileEditorChromeView>,
    on_click: impl Fn(
        &mut FileEditorChromeView,
        &gpui::ClickEvent,
        &mut Window,
        &mut Context<FileEditorChromeView>,
    ) + 'static,
) -> impl IntoElement {
    codux_tooltip_container(app_entity, id, tooltip).child(
        Button::new(id)
            .compact()
            .ghost()
            .disabled(disabled)
            .icon(
                Icon::new(icon)
                    .with_size(Size::XSmall)
                    .text_color(icon_color),
            )
            .on_click(cx.listener(on_click)),
    )
}

fn file_editor_parent_label(relative_path: &str, label: &str) -> String {
    let parent = Path::new(relative_path)
        .parent()
        .and_then(|path| path.to_str())
        .unwrap_or_default();
    if parent.trim().is_empty() || parent == "." || parent == label {
        "/".to_string()
    } else {
        parent.to_string()
    }
}

fn file_editor_i18n(
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<FileEditorChromeView>,
    key: &str,
    fallback: &str,
) -> String {
    cx.update_entity(&app_entity, |app, _cx| {
        let locale = locale_from_language_setting(&app.state.settings.language);
        translate(&locale, key, fallback)
    })
}

fn file_editor_label(relative_path: &str) -> String {
    Path::new(relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(relative_path)
        .to_string()
}

fn file_language_for_path(relative_path: &str) -> &'static str {
    let extension = Path::new(relative_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "rs" => "rust",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" => "typescript",
        "jsx" => "javascript",
        "json" => "json",
        "md" | "markdown" => "markdown",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "html" | "htm" => "html",
        "css" | "scss" | "sass" | "less" => "css",
        "sh" | "bash" | "zsh" => "bash",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" => "cpp",
        "swift" => "swift",
        "lua" => "lua",
        "sql" => "sql",
        "xml" => "xml",
        _ => "text",
    }
}
