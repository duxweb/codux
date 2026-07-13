use super::*;

pub(in crate::app) fn file_editor_workspace(
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: FileEditorWorkspaceSnapshot,
    chrome_view: gpui::Entity<FileEditorChromeView>,
    content_view: gpui::Entity<FileEditorContentView>,
    _window: &mut Window,
    cx: &mut Context<FileEditorWorkspaceView>,
) -> impl IntoElement {
    let FileEditorWorkspaceSnapshot {
        tabs,
        active_path: _,
        active_preview_path: _,
        single_window,
        active_tab: _,
        active_editor: _,
        active_loading: _,
        split_active: _,
    } = snapshot;
    let empty_text = file_editor_i18n(
        app_entity.clone(),
        cx,
        "files.editor.empty",
        "Double-click a file to open it",
    );

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
                            .text_size(rems(0.8125))
                            .line_height(rems(1.125))
                            .child(empty_text),
                    ),
            )
        })
        .when(!tabs.is_empty(), |this| {
            this.child(
                gpui::AnyView::from(chrome_view).cached(
                    gpui::StyleRefinement::default()
                        .flex_none()
                        .w_full()
                        .h(px(if single_window {
                            FILE_EDITOR_TOOLBAR_HEIGHT
                        } else {
                            FILE_EDITOR_CHROME_HEIGHT
                        })),
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

pub(super) fn file_editor_tab_bar(
    app_entity: gpui::Entity<CoduxApp>,
    tabs: Vec<FileEditorTab>,
    active_path: Option<String>,
    show_split_close: bool,
    tab_scroll_handle: ScrollHandle,
    cx: &mut Context<FileEditorTabBarView>,
) -> impl IntoElement {
    let tab_order = tabs
        .iter()
        .map(|tab| tab.relative_path.clone())
        .collect::<Vec<_>>();
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
                .child(
                    div()
                        .id("file-editor-tab-scroll")
                        .flex()
                        .h_full()
                        .min_w_0()
                        .items_center()
                        .gap_1()
                        .overflow_x_scroll()
                        .track_scroll(&tab_scroll_handle)
                        .children(tabs.into_iter().map(|tab| {
                            let active = active_path.as_deref() == Some(tab.relative_path.as_str());
                            file_editor_tab_button(
                                app_entity.clone(),
                                tab,
                                active,
                                tab_order.clone(),
                                cx,
                            )
                        })),
                ),
        )
        // Dedicated "close split" control, right of the tabs. It is a flex_none
        // sibling of the flex_1 tab strip, so growing tabs clip/scroll under
        // their own area and never overlap this button.
        .when(show_split_close, |this| {
            this.child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .h_full()
                    .pl(px(8.0))
                    .ml(px(2.0))
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .id("file-editor-split-close")
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .text_color(cx.theme().muted_foreground)
                            .hover(|style| style.bg(cx.theme().secondary_hover))
                            .child(
                                Icon::new(HeroIconName::XMark)
                                    .size_3()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .on_click(cx.listener(|view, _event, window, cx| {
                                let app_entity = view.app_entity.clone();
                                defer_codux_app_update(
                                    app_entity,
                                    window,
                                    cx,
                                    move |app, window, app_cx| {
                                        app.close_file_editor_split(window, app_cx);
                                    },
                                );
                                cx.stop_propagation();
                            })),
                    ),
            )
        })
}

fn file_editor_tab_button(
    app_entity: gpui::Entity<CoduxApp>,
    tab: FileEditorTab,
    active: bool,
    tab_order: Vec<String>,
    cx: &mut Context<FileEditorTabBarView>,
) -> AnyElement {
    let select_path = tab.relative_path.clone();
    let close_path = tab.relative_path.clone();
    let target_path = tab.relative_path.clone();
    let drag_tab = tab.clone();
    let tab_button_id = SharedString::from(format!("file-editor-tab-{close_path}"));
    let close_button_id = SharedString::from(format!("file-editor-close-{close_path}"));
    let active_bg = color(theme::TEXT).opacity(0.07);
    let hover_bg = cx.theme().secondary_hover;

    let text_color = if tab.dirty {
        color(theme::TEXT)
    } else {
        cx.theme().secondary_foreground
    };

    file_editor_tab_base(text_color)
        .id(tab_button_id)
        .when(active, |this| this.bg(active_bg))
        .cursor_pointer()
        .hover(move |style| style.bg(hover_bg))
        .on_drag(
            FileEditorTabDrag {
                path: drag_tab.relative_path.clone(),
                tab: drag_tab,
                active,
            },
            move |drag, _, _, cx| {
                cx.new(|_| FileEditorTabDrag {
                    path: drag.path.clone(),
                    tab: drag.tab.clone(),
                    active: drag.active,
                })
            },
        )
        .drag_over::<FileEditorTabDrag>(move |this, _drag, _window, _cx| this)
        .on_drop(cx.listener({
            let app_entity = app_entity.clone();
            let target_path = target_path.clone();
            move |_view, drag: &FileEditorTabDrag, window, cx| {
                let Some(next_paths) =
                    reordered_ids(&tab_order, drag.path.as_str(), target_path.as_str())
                else {
                    return;
                };
                defer_codux_app_update(app_entity.clone(), window, cx, move |app, _, app_cx| {
                    app.reorder_file_editor_tabs(next_paths, app_cx);
                });
                cx.stop_propagation();
            }
        }))
        .on_click(cx.listener(move |_app, _event, window, cx| {
            let select_path = select_path.clone();
            defer_codux_app_update(
                app_entity.clone(),
                window,
                cx,
                move |app, window, app_cx| {
                    app.select_file_editor_tab(select_path, window, app_cx);
                },
            );
        }))
        .child(file_editor_tab_content(
            tab,
            cx.theme().secondary_foreground,
        ))
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
                    let close_path = close_path.clone();
                    defer_codux_app_update(app_entity, window, cx, move |app, window, app_cx| {
                        app.close_file_editor_tab(close_path, window, app_cx);
                    });
                    cx.stop_propagation();
                })),
        )
        .into_any_element()
}

pub(super) fn file_editor_tab_base(text_color: gpui::Hsla) -> gpui::Div {
    div()
        .h(px(28.0))
        .min_w(px(96.0))
        .max_w(px(220.0))
        .flex_none()
        .flex()
        .items_center()
        .rounded(px(6.0))
        .text_size(rems(0.78125))
        .line_height(rems(1.0))
        .text_color(text_color)
}

pub(super) fn file_editor_tab_content(tab: FileEditorTab, icon_color: gpui::Hsla) -> gpui::Div {
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
                .text_color(icon_color),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(tab.label),
        )
}

pub(super) fn file_editor_toolbar(
    app_entity: gpui::Entity<CoduxApp>,
    active_tab: Option<FileEditorTab>,
    window_header: bool,
    cx: &mut Context<FileEditorToolbarView>,
) -> impl IntoElement {
    let active_dirty = active_tab.as_ref().is_some_and(|tab| tab.dirty);
    let read_only = active_tab.as_ref().is_none_or(|tab| !tab.editable);
    let active_label = active_tab
        .as_ref()
        .map(|tab| tab.label.clone())
        .unwrap_or_default();

    div()
        .h(px(FILE_EDITOR_TOOLBAR_HEIGHT))
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .pr(px(12.0))
        .when(window_header && cfg!(target_os = "macos"), |this| {
            this.pl(px(86.0))
        })
        .when(!(window_header && cfg!(target_os = "macos")), |this| {
            this.pl(px(18.0))
        })
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().title_bar)
        .when(window_header && !cfg!(target_os = "windows"), |this| {
            this.window_control_area(WindowControlArea::Drag)
        })
        .child(
            div()
                .min_w_0()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .when(window_header && cfg!(target_os = "windows"), |this| {
                    this.window_control_area(WindowControlArea::Drag)
                })
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .truncate()
                .child(active_label),
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
                    (
                        if active_dirty {
                            color(theme::GREEN)
                        } else {
                            cx.theme().secondary_foreground
                        },
                        !active_dirty || read_only,
                    ),
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
                    (cx.theme().secondary_foreground, read_only),
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
                    (cx.theme().secondary_foreground, read_only),
                    cx,
                    |view, _event, window, cx| {
                        view.dispatch_active_file_editor_action(Redo, window, cx);
                    },
                ))
                .child(file_editor_toolbar_button(
                    app_entity.clone(),
                    "file-editor-search",
                    HeroIconName::MagnifyingGlass,
                    file_editor_i18n(
                        app_entity.clone(),
                        cx,
                        "shortcut.editor.search",
                        "Search File",
                    ),
                    (cx.theme().secondary_foreground, false),
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
                    (cx.theme().secondary_foreground, false),
                    cx,
                    |view, _event, _window, cx| {
                        let app_entity = view.app_entity.clone();
                        cx.update_entity(&app_entity, |app, cx| {
                            app.copy_active_file_editor_path_to_clipboard(cx);
                        });
                    },
                ))
                .child(file_editor_toolbar_button(
                    app_entity.clone(),
                    "file-editor-reload",
                    HeroIconName::ArrowPath,
                    file_editor_i18n(app_entity.clone(), cx, "common.reload", "Reload"),
                    (cx.theme().secondary_foreground, false),
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
                    (cx.theme().secondary_foreground, false),
                    cx,
                    |view, _event, _window, cx| {
                        let app_entity = view.app_entity.clone();
                        cx.update_entity(&app_entity, |app, cx| {
                            app.run_active_file_editor_file_system_action("reveal", cx);
                        });
                    },
                ))
                .when(window_header && !cfg!(target_os = "macos"), |this| {
                    this.child(window_close_control(
                        "file-editor-window-close",
                        28.0,
                        true,
                        cx,
                    ))
                }),
        )
}

fn file_editor_toolbar_button(
    app_entity: gpui::Entity<CoduxApp>,
    id: &'static str,
    icon: HeroIconName,
    tooltip: String,
    state: (gpui::Hsla, bool),
    cx: &mut Context<FileEditorToolbarView>,
    on_click: impl Fn(
        &mut FileEditorToolbarView,
        &gpui::ClickEvent,
        &mut Window,
        &mut Context<FileEditorToolbarView>,
    ) + 'static,
) -> impl IntoElement {
    let (icon_color, disabled) = state;
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
