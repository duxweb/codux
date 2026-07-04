use super::*;
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};
use gpui::{ClickEvent, ClipboardEntry, Focusable, ImageFormat, Point, ScrollWheelEvent};
use gpui_component::input::{Input, InputEvent, InputState, SelectAll};
use std::{ops::Neg, path::Path};

const FILE_TREE_DRAG_AND_DROP: bool = true;

#[derive(Clone, Copy)]
pub(in crate::app) enum FileSidebarKeyAction {
    Rename,
    MoveSelection(isize),
    Delete,
}

#[derive(Clone)]
struct FileSidebarLabels {
    title: String,
    empty: String,
    new_file: String,
    new_folder: String,
    open: String,
    preview: String,
    reveal: String,
    copy_path: String,
    save_as: String,
    copy: String,
    paste: String,
    rename: String,
    send_terminal: String,
    delete: String,
    items_count_format: String,
}

fn file_sidebar_labels(language: &str) -> FileSidebarLabels {
    let locale = locale_from_language_setting(&language);
    let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
    FileSidebarLabels {
        title: tr("files.panel.title", "Files"),
        empty: tr("files.panel.empty", "No files"),
        new_file: tr("files.panel.new_file", "New File"),
        new_folder: tr("files.panel.new_folder", "New Folder"),
        open: tr("files.panel.open", "Open"),
        preview: tr("files.panel.open_preview", "Preview"),
        reveal: tr("files.panel.reveal_finder", "Show in File Manager"),
        copy_path: tr("files.panel.copy_path", "Copy Path"),
        save_as: tr("files.panel.save_as", "Save As…"),
        copy: tr("common.copy", "Copy"),
        paste: tr("files.panel.paste", "Paste"),
        rename: tr("common.rename", "Rename"),
        send_terminal: tr("files.panel.insert_path_terminal", "Send to Terminal"),
        delete: tr("common.delete", "Delete"),
        items_count_format: tr("files.panel.items_count_format", "%d file items"),
    }
}

pub(in crate::app) fn file_directory_option(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

pub(in crate::app) fn current_directory_suffix(value: &str) -> String {
    file_directory_option(value)
        .map(|directory| format!(" / {directory}"))
        .unwrap_or_default()
}

pub(in crate::app) fn parent_relative_directory(value: &str) -> String {
    let mut parts = value
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    parts.pop();
    parts.join("/")
}

pub(in crate::app) fn file_section(
    app_entity: gpui::Entity<CoduxApp>,
    focus_handle: FocusHandle,
    _project_name: &str,
    files_empty: bool,
    draft_kind: Option<FileNameDraftKind>,
    draft_parent: Option<&str>,
    draft_value: &str,
    draft_select_all: bool,
    rows: Rc<Vec<FileTreeRow>>,
    tree_scroll_handle: UniformListScrollHandle,
    language: &str,
    refreshing: bool,
    window: &mut Window,
    cx: &mut Context<FileSidebarView>,
) -> impl IntoElement {
    let labels = file_sidebar_labels(language);
    let row_count = rows.len();
    let draft_at_top =
        draft_kind.is_some_and(|kind| kind != FileNameDraftKind::Rename) && draft_parent.is_none();

    div()
        .flex()
        .flex_1()
        .w_full()
        .h_full()
        .min_w_0()
        .min_h_0()
        .flex_col()
        .track_focus(&focus_handle)
        .on_key_down(cx.listener({
            let app_entity = app_entity.clone();
            move |_view, event: &KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                if keystroke.modifiers.platform
                    && !keystroke.modifiers.control
                    && !keystroke.modifiers.alt
                    && !keystroke.modifiers.shift
                    && keystroke.key.eq_ignore_ascii_case("c")
                {
                    // Defer: this listener runs while FileSidebarView is being
                    // updated, and the copy invalidates the file panel (which
                    // re-enters this view). Stop propagation synchronously, run
                    // the mutation after the update lease is released.
                    cx.stop_propagation();
                    let app_entity = app_entity.clone();
                    window.defer(cx, move |_window, cx| {
                        cx.update_entity(&app_entity, |app, cx| {
                            let _ = app.copy_selected_file_paths_to_clipboard(cx);
                        });
                    });
                    return;
                }
                if keystroke.modifiers.platform
                    && !keystroke.modifiers.control
                    && !keystroke.modifiers.alt
                    && !keystroke.modifiers.shift
                    && keystroke.key.eq_ignore_ascii_case("v")
                {
                    cx.stop_propagation();
                    let app_entity = app_entity.clone();
                    window.defer(cx, move |window, cx| {
                        let payload = clipboard_file_payload(cx);
                        cx.update_entity(&app_entity, |app, cx| {
                            app.paste_clipboard_file_entries(payload, window, cx);
                        });
                    });
                    return;
                }
                // Escape / Enter confirm or cancel an active name draft, which
                // re-invalidates the file panel. Decide synchronously to keep
                // key-swallow semantics, but defer the mutation so we don't
                // re-enter FileSidebarView while it is mid-update.
                let unmodified_action = !keystroke.modifiers.platform
                    && !keystroke.modifiers.control
                    && !keystroke.modifiers.alt
                    && !keystroke.modifiers.shift;
                if unmodified_action
                    && !keystroke.modifiers.function
                    && app_entity.read(cx).file_name_draft_kind.is_some()
                {
                    let key = keystroke.key.as_str();
                    let confirm = matches!(key, "enter" | "Enter" | "return" | "Return");
                    let cancel = matches!(key, "escape" | "Escape");
                    if confirm || cancel {
                        cx.stop_propagation();
                        let app_entity = app_entity.clone();
                        window.defer(cx, move |window, cx| {
                            cx.update_entity(&app_entity, |app, cx| {
                                if confirm {
                                    app.confirm_file_name_draft(window, cx);
                                } else {
                                    app.cancel_file_name_draft(window, cx);
                                }
                            });
                        });
                    }
                    return;
                }
                if unmodified_action {
                    let key = keystroke.key.as_str();
                    let action = if key.eq_ignore_ascii_case("f2") {
                        Some(FileSidebarKeyAction::Rename)
                    } else if key.eq_ignore_ascii_case("enter")
                        || key.eq_ignore_ascii_case("return")
                    {
                        Some(FileSidebarKeyAction::Rename)
                    } else if key.eq_ignore_ascii_case("up") || key.eq_ignore_ascii_case("arrowup")
                    {
                        Some(FileSidebarKeyAction::MoveSelection(-1))
                    } else if key.eq_ignore_ascii_case("down")
                        || key.eq_ignore_ascii_case("arrowdown")
                    {
                        Some(FileSidebarKeyAction::MoveSelection(1))
                    } else if key.eq_ignore_ascii_case("delete")
                        || key.eq_ignore_ascii_case("backspace")
                    {
                        Some(FileSidebarKeyAction::Delete)
                    } else {
                        None
                    };
                    if let Some(action) = action {
                        cx.stop_propagation();
                        let app_entity = app_entity.clone();
                        window.defer(cx, move |window, cx| {
                            cx.update_entity(&app_entity, |app, cx| {
                                app.handle_file_sidebar_key_action(action, window, cx);
                            });
                        });
                    }
                }
            }
        }))
        // Clicking anywhere in the panel outside the draft row (which stops
        // propagation) finishes an active name draft: empty cancels, non-empty
        // confirms — the same rule as losing focus. Deferred so we don't
        // re-enter FileSidebarView while it is mid-update.
        .on_mouse_down(
            MouseButton::Left,
            cx.listener({
                let app_entity = app_entity.clone();
                move |_view, _event, window, cx| {
                    if app_entity.read(cx).file_name_draft_kind.is_none() {
                        return;
                    }
                    let app_entity = app_entity.clone();
                    window.defer(cx, move |window, cx| {
                        cx.update_entity(&app_entity, |app, cx| {
                            app.finish_file_name_draft_on_blur(window, cx);
                        });
                    });
                }
            }),
        )
        .child(assistant_panel_header(
            labels.title.clone(),
            HeroIconName::Folder,
            div()
                .flex()
                .items_center()
                .child(assistant_header_icon_button(
                    "file-sidebar-refresh",
                    HeroIconName::ArrowPath,
                    refreshing,
                    app_entity.clone(),
                    window,
                    cx,
                    |app, _event, _window, cx| app.reload_project_files_async(cx),
                ))
                .child(assistant_header_icon_button(
                    "file-sidebar-new-file",
                    HeroIconName::Document,
                    false,
                    app_entity.clone(),
                    window,
                    cx,
                    |app, _event, window, cx| app.create_project_file(window, cx),
                ))
                .child(assistant_header_icon_button(
                    "file-sidebar-new-dir",
                    HeroIconName::Folder,
                    false,
                    app_entity.clone(),
                    window,
                    cx,
                    |app, _event, window, cx| app.create_project_directory(window, cx),
                )),
        ))
        .child(
            div()
                .flex_1()
                .w_full()
                .min_h_0()
                .p(px(12.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex_1()
                        .w_full()
                        .min_h_0()
                        .relative()
                        .overflow_hidden()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .size_full()
                                .min_h_0()
                                .when(draft_at_top, |this| {
                                    let kind = draft_kind.unwrap_or(FileNameDraftKind::CreateFile);
                                    this.child(file_name_draft_row(
                                        app_entity.clone(),
                                        kind,
                                        draft_value,
                                        draft_select_all,
                                        window,
                                        cx,
                                    ))
                                })
                                .child(if files_empty && !draft_at_top {
                                    file_empty_state(labels.empty.clone()).into_any_element()
                                } else if row_count == 0 && !draft_at_top {
                                    file_empty_state(labels.empty.clone()).into_any_element()
                                } else {
                                    div()
                                        .flex_1()
                                        .w_full()
                                        .min_w_0()
                                        .min_h_0()
                                        .flex()
                                        .flex_col()
                                        .child(codux_uniform_list(
                                            "file-tree-list",
                                            rows,
                                            tree_scroll_handle.clone(),
                                            None,
                                            cx,
                                            move |row, index, window, cx| {
                                                file_tree_entry_row(
                                                    app_entity.clone(),
                                                    row,
                                                    index,
                                                    labels.clone(),
                                                    labels.items_count_format.clone(),
                                                    window,
                                                    cx,
                                                )
                                                .into_any_element()
                                            },
                                        ))
                                        .child(file_tree_blank_scroll_area(
                                            tree_scroll_handle,
                                            cx.entity().entity_id(),
                                        ))
                                        .into_any_element()
                                }),
                        ),
                ),
        )
}

fn file_tree_blank_scroll_area(
    scroll_handle: UniformListScrollHandle,
    entity_id: gpui::EntityId,
) -> impl IntoElement {
    div()
        .id("file-tree-blank-area")
        .block_mouse_except_scroll()
        .flex_grow()
        .on_scroll_wheel(move |event: &ScrollWheelEvent, window, cx| {
            let state = scroll_handle.0.borrow();
            let base_handle = &state.base_handle;
            let current_offset = base_handle.offset();
            let max_offset = base_handle.max_offset();
            let delta = event.delta.pixel_delta(window.line_height());
            let new_offset = (current_offset + delta).clamp(&max_offset.neg(), &Point::default());

            if new_offset != current_offset {
                base_handle.set_offset(new_offset);
                cx.notify(entity_id);
            }
        })
}

fn file_empty_state(label: impl Into<String>) -> impl IntoElement {
    let label = label.into();
    div()
        .size_full()
        .flex_1()
        .min_w_0()
        .w_full()
        .min_h(px(120.0))
        .p(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(color(theme::TEXT_DIM))
        .child(label)
}

fn assistant_header_icon_button(
    id: &'static str,
    icon: HeroIconName,
    loading: bool,
    app_entity: gpui::Entity<CoduxApp>,
    window: &mut Window,
    cx: &mut Context<FileSidebarView>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .compact()
        .ghost()
        .loading(loading)
        .text_color(cx.theme().secondary_foreground)
        .icon(
            Icon::new(icon)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .on_click(window.listener_for(&app_entity, on_click))
}

fn file_name_draft_row(
    app_entity: gpui::Entity<CoduxApp>,
    kind: FileNameDraftKind,
    value: &str,
    draft_select_all: bool,
    window: &mut Window,
    cx: &mut Context<FileSidebarView>,
) -> impl IntoElement {
    let icon = match kind {
        FileNameDraftKind::CreateDirectory => HeroIconName::Folder,
        _ => HeroIconName::Document,
    };
    let input_state =
        file_name_draft_input_state(app_entity, kind, value, draft_select_all, window, cx);

    div()
        .w_full()
        .min_w_0()
        .h(px(30.0))
        .pl(px(8.0))
        .pr(px(8.0))
        .flex()
        .items_center()
        .bg(cx.theme().transparent)
        // Clicks on the draft row position the input caret; stop them here so
        // the panel's outside-click handler does not treat them as "clicked
        // elsewhere" and finish the draft.
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .w(px(18.0))
                .flex_none()
                .mr(px(4.0))
                .flex()
                .items_center()
                .justify_center(),
        )
        .child(
            Icon::new(icon)
                .size_3p5()
                .flex_none()
                .text_color(color(theme::TEXT_MUTED)),
        )
        .child(
            div()
                .ml(px(8.0))
                .flex_1()
                .min_w_0()
                .h(px(24.0))
                .flex()
                .items_center()
                .child(file_name_draft_input(input_state)),
        )
}

fn file_name_draft_input_state(
    app_entity: gpui::Entity<CoduxApp>,
    kind: FileNameDraftKind,
    value: &str,
    draft_select_all: bool,
    window: &mut Window,
    cx: &mut Context<FileSidebarView>,
) -> gpui::Entity<InputState> {
    let placeholder = match kind {
        FileNameDraftKind::CreateFile => "filename.txt",
        FileNameDraftKind::CreateDirectory => "folder",
        FileNameDraftKind::Rename => "new name",
    };
    let value = value.to_string();
    let input_state = window.use_keyed_state(
        SharedString::from(format!("file-name-draft-{kind:?}")),
        cx,
        |window, cx| {
            InputState::new(window, cx)
                .default_value(value.clone())
                .placeholder(placeholder)
        },
    );
    input_state.update(cx, |state, cx| {
        // Initialize the field only on first appearance, before it takes focus.
        // The keyed-state observe re-renders the sidebar on every cursor blink,
        // hover, or value change; re-syncing the value or re-focusing on those
        // renders fights the user's edits (the draft value lags behind by a
        // deferred update, so a keystroke or backspace can be snapped back to
        // the stale value) and restarts the blink cursor (caret flicker). Once
        // focused, the input owns its text and we only consume Change events.
        if !state.focus_handle(cx).is_focused(window) {
            if state.value().as_ref() != value {
                state.set_value(value.clone(), window, cx);
            }
            state.focus(window, cx);
            if draft_select_all && state.selected_range().is_empty() {
                window.dispatch_action(Box::new(SelectAll), cx);
            }
        }
    });
    cx.subscribe_in(
        &input_state,
        window,
        move |_view, state, event, window, cx| match event {
            InputEvent::Change => {
                // Defer the app mutation: this subscription fires while the
                // FileSidebarView is already being updated, and the app update
                // re-invalidates the file panel (file_sidebar_view -> view.update),
                // which would re-enter this same view and panic. Running it after
                // the current update lease is released avoids the re-entrancy.
                let value = state.read(cx).value().to_string();
                let app_entity = app_entity.clone();
                window.defer(cx, move |window, cx| {
                    cx.update_entity(&app_entity, |app, cx| {
                        app.file_name_draft_select_all = false;
                        app.set_file_name_draft_value(value, window, cx);
                    });
                });
            }
            InputEvent::PressEnter { .. } => {
                let app_entity = app_entity.clone();
                window.defer(cx, move |window, cx| {
                    cx.update_entity(&app_entity, |app, cx| {
                        app.confirm_file_name_draft(window, cx);
                    });
                });
            }
            InputEvent::Blur => {
                let app_entity = app_entity.clone();
                window.defer(cx, move |window, cx| {
                    cx.update_entity(&app_entity, |app, cx| {
                        app.finish_file_name_draft_on_blur(window, cx);
                    });
                });
            }
            InputEvent::Focus => {}
        },
    )
    .detach();
    input_state
}

fn file_name_draft_input(input_state: gpui::Entity<InputState>) -> impl IntoElement {
    Input::new(&input_state)
        .appearance(true)
        .bordered(true)
        .focus_bordered(true)
        .with_size(Size::Small)
        .text_size(rems(0.875))
        .line_height(rems(1.125))
        .text_color(color(theme::TEXT_MUTED))
        .w_full()
        .h(px(24.0))
        .min_w_0()
}

#[derive(Clone)]
pub(in crate::app) struct FileTreeRow {
    file: FileEntry,
    active: bool,
    expanded: bool,
    editing: bool,
    synthetic: bool,
    editing_value: String,
    drag_paths: Vec<String>,
    depth: usize,
}

#[derive(Clone)]
struct FileTreeDrag {
    paths: Vec<String>,
    items_count_format: String,
}

impl Render for FileTreeDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .bg(color(theme::BG_PANEL))
            .border_1()
            .border_color(color(theme::BORDER_SOFT))
            .text_size(rems(0.75))
            .line_height(rems(1.0))
            .text_color(color(theme::TEXT))
            .child(if self.paths.len() == 1 {
                self.paths[0].clone()
            } else {
                self.items_count_format
                    .replace("%d", &self.paths.len().to_string())
            })
    }
}

pub(in crate::app) fn file_tree_rows(
    files: &[FileEntry],
    tree_children: &HashMap<String, Vec<FileEntry>>,
    expanded_dirs: &HashSet<String>,
    selected_entry: Option<&str>,
    selected_entries: &HashSet<String>,
    draft_kind: Option<FileNameDraftKind>,
    draft_target: Option<&str>,
    draft_parent: Option<&str>,
    draft_value: &str,
    depth: usize,
) -> Vec<FileTreeRow> {
    let mut rows = Vec::new();
    for file in files {
        let active = selected_entry
            .map(|path| path == file.relative_path)
            .unwrap_or(false)
            || selected_entries.contains(&file.relative_path);
        let expanded = expanded_dirs.contains(&file.relative_path);
        let editing = draft_kind == Some(FileNameDraftKind::Rename)
            && draft_target == Some(file.relative_path.as_str());
        let drag_paths = if selected_entries.contains(&file.relative_path) {
            let mut paths = selected_entries.iter().cloned().collect::<Vec<_>>();
            paths.sort();
            paths
        } else {
            vec![file.relative_path.clone()]
        };
        rows.push(FileTreeRow {
            file: file.clone(),
            active,
            expanded,
            editing,
            synthetic: false,
            editing_value: if editing {
                draft_value.to_string()
            } else {
                String::new()
            },
            drag_paths,
            depth,
        });
        if expanded {
            if draft_kind.is_some_and(|kind| kind != FileNameDraftKind::Rename)
                && draft_parent == Some(file.relative_path.as_str())
            {
                rows.push(FileTreeRow {
                    file: FileEntry {
                        name: draft_value.to_string(),
                        relative_path: join_relative_child_path(&file.relative_path, draft_value),
                        kind: if draft_kind == Some(FileNameDraftKind::CreateDirectory) {
                            FileKind::Directory
                        } else {
                            FileKind::File
                        },
                        size: 0,
                    },
                    active: false,
                    expanded: false,
                    editing: true,
                    synthetic: true,
                    editing_value: draft_value.to_string(),
                    drag_paths: vec![join_relative_child_path(&file.relative_path, draft_value)],
                    depth: depth + 1,
                });
            }
            if let Some(children) = tree_children.get(&file.relative_path) {
                rows.extend(file_tree_rows(
                    children,
                    tree_children,
                    expanded_dirs,
                    selected_entry,
                    selected_entries,
                    draft_kind,
                    draft_target,
                    draft_parent,
                    draft_value,
                    depth + 1,
                ));
            }
        }
    }
    rows
}

fn file_tree_entry_row(
    app_entity: gpui::Entity<CoduxApp>,
    row: FileTreeRow,
    index: usize,
    labels: FileSidebarLabels,
    items_count_format: String,
    window: &mut Window,
    cx: &mut Context<FileSidebarView>,
) -> impl IntoElement {
    let FileTreeRow {
        file,
        active,
        expanded,
        editing,
        synthetic,
        editing_value,
        drag_paths,
        depth,
    } = row;
    let entry = file.clone();
    let menu_entry = file.clone();
    let right_click_entry = file.clone();
    let drop_entry = file.clone();
    let is_dir = matches!(file.kind, FileKind::Directory);
    let hover_surface = cx.theme().list_hover;
    let icon = if is_dir {
        if expanded {
            HeroIconName::FolderOpen
        } else {
            HeroIconName::Folder
        }
    } else {
        HeroIconName::Document
    };
    let indent = px(8.0 + depth as f32 * 14.0);

    div()
        .id(SharedString::from(format!("file-tree-row-{index}")))
        .w_full()
        .min_w_0()
        .h(px(24.0))
        .pl(indent)
        .pr(px(8.0))
        .flex()
        .items_center()
        .when(active, |this| this.bg(hover_surface))
        .hover(move |style| style.bg(hover_surface))
        .on_click(cx.listener(move |view, event: &ClickEvent, window, cx| {
            if editing {
                return;
            }
            view.focus_handle.focus(window, cx);
            let entry = entry.clone();
            let extend = event.modifiers().shift;
            let toggle = event.modifiers().control || event.modifiers().platform;
            let open = !is_dir && event.click_count() >= 2;
            view.defer_app_update(window, cx, move |app, window, cx| {
                app.select_file_entry_from_click(entry, extend, toggle, open, window, cx);
            });
        }))
        .when(FILE_TREE_DRAG_AND_DROP && !synthetic, |this| {
            let drag_payload = drag_paths.clone();
            let drag_items_count_format = items_count_format.clone();
            this.on_drag(
                FileTreeDrag {
                    paths: drag_payload,
                    items_count_format: drag_items_count_format,
                },
                move |drag, _, _, cx| {
                    cx.new(|_| FileTreeDrag {
                        paths: drag.paths.clone(),
                        items_count_format: drag.items_count_format.clone(),
                    })
                },
            )
        })
        .when(FILE_TREE_DRAG_AND_DROP && is_dir && !synthetic, |this| {
            this.drag_over::<FileTreeDrag>(|this, _drag, _window, cx| {
                this.bg(cx.theme().drop_target)
                    .border_1()
                    .border_color(color(theme::ACCENT).opacity(0.45))
            })
            .on_drop(cx.listener(move |view, drag: &FileTreeDrag, window, cx| {
                let paths = drag.paths.clone();
                let target = drop_entry.relative_path.clone();
                view.defer_app_update(window, cx, move |app, window, cx| {
                    app.move_file_entries_to_directory(paths, target, window, cx);
                });
                cx.stop_propagation();
            }))
        })
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, _event, window, cx| {
                if synthetic {
                    return;
                }
                let relative_path = right_click_entry.relative_path.clone();
                view.defer_app_update(window, cx, move |app, _window, cx| {
                    app.prepare_file_context_menu_selection(relative_path, cx);
                });
            }),
        )
        .context_menu({
            let app_entity = app_entity.clone();
            move |menu, _window, cx| {
                if synthetic {
                    return menu;
                }
                file_tree_context_menu(
                    menu,
                    app_entity.clone(),
                    menu_entry.clone(),
                    labels.clone(),
                    cx,
                )
            }
        })
        .child(
            div()
                .w(px(18.0))
                .flex_none()
                .mr(px(4.0))
                .flex()
                .items_center()
                .justify_center()
                .child(if is_dir && !synthetic {
                    Icon::new(if expanded {
                        HeroIconName::ChevronDown
                    } else {
                        HeroIconName::ChevronRight
                    })
                    .size_3()
                    .text_color(color(theme::TEXT_MUTED))
                    .into_any_element()
                } else {
                    div().into_any_element()
                }),
        )
        .child(
            Icon::new(icon)
                .size_3p5()
                .flex_none()
                .text_color(color(if is_dir {
                    theme::ACCENT
                } else {
                    theme::TEXT_MUTED
                })),
        )
        .child(if editing {
            // Synthetic rows are in-folder create drafts; real rows edit as renames.
            let kind = if !synthetic {
                FileNameDraftKind::Rename
            } else if is_dir {
                FileNameDraftKind::CreateDirectory
            } else {
                FileNameDraftKind::CreateFile
            };
            let input_state =
                file_name_draft_input_state(app_entity, kind, &editing_value, true, window, cx);
            div()
                .ml(px(8.0))
                .flex_1()
                .min_w_0()
                .h(px(24.0))
                .flex()
                .items_center()
                .child(file_name_draft_input(input_state))
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(format!("file-tree-row-name-{index}")))
                .ml(px(8.0))
                .flex_1()
                .min_w_0()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .truncate()
                .child(file.name)
                .into_any_element()
        })
}

fn file_tree_context_menu(
    menu: PopupMenu,
    app_entity: gpui::Entity<CoduxApp>,
    entry: FileEntry,
    labels: FileSidebarLabels,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let (has_selected, multiple, selected_is_directory) =
        cx.update_entity(&app_entity, |app, cx| {
            app.prepare_file_context_menu_selection(entry.relative_path.clone(), cx);
            let paths = app.selected_file_entry_paths();
            let selected_is_directory = paths
                .first()
                .and_then(|path| app.file_tree_entry(path))
                .is_some_and(|entry| matches!(entry.kind, FileKind::Directory));
            (!paths.is_empty(), paths.len() > 1, selected_is_directory)
        });
    let open_entity = app_entity.clone();
    let preview_entity = app_entity.clone();
    let reveal_entity = app_entity.clone();
    let copy_path_entity = app_entity.clone();
    let copy_entity = app_entity.clone();
    let save_as_entity = app_entity.clone();
    let paste_entity = app_entity.clone();
    let new_file_entity = app_entity.clone();
    let new_folder_entity = app_entity.clone();
    let rename_entity = app_entity.clone();
    let terminal_entity = app_entity.clone();
    let delete_entity = app_entity;

    menu.item(
        PopupMenuItem::new(labels.open.clone())
            .icon(HeroIconName::ArrowTopRightOnSquare)
            .disabled(!has_selected || multiple)
            .on_click(move |_, window, cx| {
                cx.update_entity(&open_entity, |app, cx| {
                    app.open_selected_file_entry(window, cx);
                });
            }),
    )
    .item(
        PopupMenuItem::new(labels.preview.clone())
            .icon(HeroIconName::Eye)
            .disabled(!has_selected || multiple || selected_is_directory)
            .on_click(move |_, window, cx| {
                cx.update_entity(&preview_entity, |app, cx| {
                    app.open_selected_file_preview(window, cx);
                });
            }),
    )
    .item(
        PopupMenuItem::new(labels.reveal.clone())
            .icon(HeroIconName::FolderOpen)
            .disabled(!has_selected || multiple)
            .on_click(move |_, window, cx| {
                cx.update_entity(&reveal_entity, |app, cx| {
                    app.reveal_selected_file_entry(window, cx);
                });
            }),
    )
    .item(
        PopupMenuItem::new(labels.copy_path.clone())
            .icon(HeroIconName::DocumentDuplicate)
            .disabled(!has_selected)
            .on_click(move |_, _window, cx| {
                cx.update_entity(&copy_path_entity, |app, cx| {
                    app.copy_selected_file_paths_to_clipboard(cx);
                });
            }),
    )
    .item(
        PopupMenuItem::new(labels.save_as.clone())
            .icon(HeroIconName::ArrowDownTray)
            .disabled(!has_selected || multiple || selected_is_directory)
            .on_click(move |_, window, cx| {
                cx.update_entity(&save_as_entity, |app, cx| {
                    app.save_as_selected_file_entry(window, cx);
                });
            }),
    )
    .separator()
    .item(
        PopupMenuItem::new(labels.copy.clone())
            .icon(HeroIconName::DocumentDuplicate)
            .disabled(!has_selected || multiple)
            .on_click(move |_, window, cx| {
                cx.update_entity(&copy_entity, |app, cx| {
                    app.copy_selected_file_entry(window, cx);
                });
            }),
    )
    .item(
        PopupMenuItem::new(labels.paste.clone())
            .icon(HeroIconName::DocumentDuplicate)
            .on_click(move |_, window, cx| {
                let payload = clipboard_file_payload(cx);
                cx.update_entity(&paste_entity, |app, cx| {
                    if let Some(entry) = app.selected_file_entry() {
                        app.paste_external_file_entries(payload, entry, window, cx);
                    }
                });
            }),
    )
    .separator()
    .item(
        PopupMenuItem::new(labels.new_file.clone())
            .icon(HeroIconName::Document)
            .disabled(!matches!(entry.kind, FileKind::Directory))
            .on_click({
                let parent = entry.relative_path.clone();
                move |_, window, cx| {
                    cx.update_entity(&new_file_entity, |app, cx| {
                        app.create_project_file_in_directory(parent.clone(), window, cx);
                    });
                }
            }),
    )
    .item(
        PopupMenuItem::new(labels.new_folder.clone())
            .icon(HeroIconName::Folder)
            .disabled(!matches!(entry.kind, FileKind::Directory))
            .on_click({
                let parent = entry.relative_path.clone();
                move |_, window, cx| {
                    cx.update_entity(&new_folder_entity, |app, cx| {
                        app.create_project_directory_in_directory(parent.clone(), window, cx);
                    });
                }
            }),
    )
    .item(
        PopupMenuItem::new(labels.rename.clone())
            .icon(HeroIconName::Language)
            .disabled(!has_selected || multiple)
            .on_click(move |_, window, cx| {
                cx.update_entity(&rename_entity, |app, cx| {
                    app.rename_selected_file_entry(window, cx);
                });
            }),
    )
    .item(
        PopupMenuItem::new(labels.send_terminal.clone())
            .icon(HeroIconName::CommandLine)
            .disabled(!has_selected || multiple)
            .on_click(move |_, _window, cx| {
                cx.update_entity(&terminal_entity, |app, cx| {
                    if let Some(path) = app.selected_file_entry.clone() {
                        app.send_file_path_to_active_terminal(path, cx);
                    }
                });
            }),
    )
    .separator()
    .item(
        PopupMenuItem::new(labels.delete)
            .icon(HeroIconName::Trash)
            .disabled(!has_selected)
            .on_click(move |_, window, cx| {
                cx.update_entity(&delete_entity, |app, cx| {
                    app.request_delete_selected_file_entries(window, cx);
                });
            }),
    )
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::app) struct ClipboardFilePayload {
    pub paths: Vec<String>,
    pub images: Vec<ClipboardImageFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct ClipboardImageFile {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

pub(in crate::app) fn clipboard_file_payload(cx: &mut App) -> ClipboardFilePayload {
    let Some(item) = cx.read_from_clipboard() else {
        return ClipboardFilePayload::default();
    };
    let mut paths = Vec::new();
    let mut images = Vec::new();
    for entry in item.entries() {
        match entry {
            ClipboardEntry::ExternalPaths(external_paths) => external_paths
                .paths()
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .for_each(|path| paths.push(path)),
            ClipboardEntry::String(text) => text
                .text()
                .lines()
                .map(str::trim)
                .filter(|line| clipboard_text_line_may_be_file_path(line))
                .map(str::to_string)
                .for_each(|path| paths.push(path)),
            ClipboardEntry::Image(image) if !image.bytes.is_empty() => {
                images.push(ClipboardImageFile {
                    file_name: clipboard_image_file_name(image.format),
                    bytes: image.bytes.clone(),
                });
            }
            ClipboardEntry::Image(_) => {}
        }
    }
    paths.sort();
    paths.dedup();
    ClipboardFilePayload { paths, images }
}

fn clipboard_image_file_name(format: ImageFormat) -> String {
    format!("pasted-image.{}", clipboard_image_extension(format))
}

fn clipboard_image_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
        ImageFormat::Ico => "ico",
        ImageFormat::Pnm => "pnm",
    }
}

fn clipboard_text_line_may_be_file_path(line: &str) -> bool {
    if line.is_empty()
        || line.len() > 4096
        || line.starts_with("data:")
        || line.starts_with("http://")
        || line.starts_with("https://")
        || line.starts_with('<')
    {
        return false;
    }
    Path::new(line).exists()
}

#[cfg(test)]
mod tests {
    use super::{clipboard_image_extension, clipboard_text_line_may_be_file_path, file_tree_rows};
    use crate::app::FileNameDraftKind;
    use codux_runtime::runtime_state::{FileEntry, FileKind};
    use gpui::ImageFormat;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn clipboard_text_line_filter_rejects_browser_image_payloads() {
        assert!(!clipboard_text_line_may_be_file_path(
            "data:image/png;base64,abc"
        ));
        assert!(!clipboard_text_line_may_be_file_path(
            "https://example.com/image.png"
        ));
        assert!(!clipboard_text_line_may_be_file_path("<img src=\"x\">"));
    }

    #[test]
    fn clipboard_image_extensions_match_gpui_formats() {
        assert_eq!(clipboard_image_extension(ImageFormat::Png), "png");
        assert_eq!(clipboard_image_extension(ImageFormat::Jpeg), "jpg");
        assert_eq!(clipboard_image_extension(ImageFormat::Webp), "webp");
    }

    #[test]
    fn file_tree_rows_puts_create_draft_under_expanded_parent() {
        let files = vec![FileEntry {
            name: "src".to_string(),
            relative_path: "src".to_string(),
            kind: FileKind::Directory,
            size: 0,
        }];
        let expanded_dirs = HashSet::from(["src".to_string()]);

        let rows = file_tree_rows(
            &files,
            &HashMap::new(),
            &expanded_dirs,
            None,
            &HashSet::new(),
            Some(FileNameDraftKind::CreateFile),
            None,
            Some("src"),
            "main.rs",
            0,
        );

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].file.relative_path, "src/main.rs");
        assert!(rows[1].editing);
        assert_eq!(rows[1].depth, 1);
    }
}
