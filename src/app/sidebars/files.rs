use super::*;
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};
use gpui::{ClickEvent, ClipboardEntry, Point, ScrollWheelEvent};
use gpui_component::input::{Input, InputEvent, InputState, SelectAll};
use std::ops::Neg;

const FILE_TREE_DRAG_AND_DROP: bool = true;

#[derive(Clone)]
struct FileSidebarLabels {
    title: String,
    empty: String,
    open: String,
    reveal: String,
    copy_path: String,
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
        open: tr("files.panel.open", "Open"),
        reveal: tr("files.panel.reveal_finder", "Show in File Manager"),
        copy_path: tr("files.panel.copy_path", "Copy Path"),
        copy: tr("files.panel.copy", "Copy"),
        paste: tr("files.panel.paste", "Paste"),
        rename: tr("common.rename", "Rename"),
        send_terminal: tr("files.panel.insert_path_terminal", "Send to Terminal"),
        delete: tr("files.panel.delete", "Delete"),
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
    let draft_at_top = draft_kind.is_some_and(|kind| kind != FileNameDraftKind::Rename);
    let menu_app_entity = app_entity.clone();

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
                    cx.update_entity(&app_entity, |app, cx| {
                        if app.copy_selected_file_paths_to_clipboard(cx) {
                            cx.stop_propagation();
                        }
                    });
                    return;
                }
                if keystroke.modifiers.platform
                    && !keystroke.modifiers.control
                    && !keystroke.modifiers.alt
                    && !keystroke.modifiers.shift
                    && keystroke.key.eq_ignore_ascii_case("v")
                {
                    let paths = clipboard_external_paths(cx);
                    cx.update_entity(&app_entity, |app, cx| {
                        if app.paste_clipboard_file_entries(paths, window, cx) {
                            cx.stop_propagation();
                        }
                    });
                    return;
                }
                cx.update_entity(&app_entity, |app, cx| {
                    if app.handle_file_name_draft_key(event, window, cx) {
                        cx.stop_propagation();
                    }
                });
            }
        }))
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
                                .when(
                                    draft_kind
                                        .is_some_and(|kind| kind != FileNameDraftKind::Rename),
                                    |this| {
                                        let kind =
                                            draft_kind.unwrap_or(FileNameDraftKind::CreateFile);
                                        this.child(file_name_draft_row(
                                            app_entity.clone(),
                                            kind,
                                            draft_value,
                                            draft_select_all,
                                            window,
                                            cx,
                                        ))
                                    },
                                )
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
                                        .context_menu(move |menu, _window, cx| {
                                            let (has_selected, multiple, copy_paths) = cx
                                                .update_entity(&menu_app_entity, |app, _cx| {
                                                    let mut paths =
                                                        if app.selected_file_entries.is_empty() {
                                                            app.selected_file_entry
                                                                .clone()
                                                                .into_iter()
                                                                .collect::<Vec<_>>()
                                                        } else {
                                                            app.selected_file_entries
                                                                .iter()
                                                                .cloned()
                                                                .collect::<Vec<_>>()
                                                        };
                                                    paths.sort();
                                                    (!paths.is_empty(), paths.len() > 1, paths)
                                                });
                                            let open_entity = menu_app_entity.clone();
                                            let reveal_entity = menu_app_entity.clone();
                                            let copy_entity = menu_app_entity.clone();
                                            let paste_entity = menu_app_entity.clone();
                                            let rename_entity = menu_app_entity.clone();
                                            let terminal_entity = menu_app_entity.clone();
                                            let delete_entity = menu_app_entity.clone();
                                            let copy_paths_for_click = copy_paths.clone();

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
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(
                                                            copy_paths_for_click.join("\n"),
                                                        ),
                                                    );
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
                                                    let paths = clipboard_external_paths(cx);
                                                    cx.update_entity(&paste_entity, |app, cx| {
                                                        if let Some(entry) =
                                                            app.selected_file_entry()
                                                        {
                                                            app.paste_external_file_entries(
                                                                paths, entry, window, cx,
                                                            );
                                                        }
                                                    });
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
                                                    if let Some(path) =
                                                        app.selected_file_entry.clone()
                                                    {
                                                        app.send_file_path_to_active_terminal(
                                                            path, cx,
                                                        );
                                                    }
                                                });
                                                }),
                                        )
                                        .separator()
                                        .item(
                                            PopupMenuItem::new(labels.delete.clone())
                                                .icon(HeroIconName::Trash)
                                                .disabled(!has_selected)
                                                .on_click(move |_, window, cx| {
                                                    cx.update_entity(&delete_entity, |app, cx| {
                                                        app.request_delete_selected_file_entries(
                                                            window, cx,
                                                        );
                                                    });
                                                }),
                                        )
                                        })
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
        .text_size(px(12.0))
        .line_height(px(16.0))
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
        if state.value().as_ref() != value {
            state.set_value(value.clone(), window, cx);
        }
        state.focus(window, cx);
        if draft_select_all && state.selected_range().is_empty() {
            window.dispatch_action(Box::new(SelectAll), cx);
        }
    });
    cx.subscribe_in(
        &input_state,
        window,
        move |_view, state, event, window, cx| match event {
            InputEvent::Change => {
                let mut value = state.read(cx).value().to_string();
                let app_entity = app_entity.clone();
                cx.update_entity(&app_entity, |app, cx| {
                    if app.file_name_draft_select_all
                        && value.len() > "undefined".len()
                        && value.starts_with("undefined")
                    {
                        value = value["undefined".len()..].to_string();
                        state.update(cx, |state, cx| {
                            state.set_value(value.clone(), window, cx);
                        });
                    }
                    app.file_name_draft_select_all = false;
                    app.set_file_name_draft_value(value, window, cx);
                });
            }
            InputEvent::PressEnter { .. } => {
                let app_entity = app_entity.clone();
                cx.update_entity(&app_entity, |app, cx| {
                    app.confirm_file_name_draft(window, cx);
                });
            }
            InputEvent::Blur => {
                let app_entity = app_entity.clone();
                cx.update_entity(&app_entity, |app, cx| {
                    app.finish_file_name_draft_on_blur(window, cx);
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
        .text_size(px(14.0))
        .line_height(px(18.0))
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
            .text_size(px(12.0))
            .line_height(px(16.0))
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
            editing_value: if editing {
                draft_value.to_string()
            } else {
                String::new()
            },
            drag_paths,
            depth,
        });
        if expanded {
            if let Some(children) = tree_children.get(&file.relative_path) {
                rows.extend(file_tree_rows(
                    children,
                    tree_children,
                    expanded_dirs,
                    selected_entry,
                    selected_entries,
                    draft_kind,
                    draft_target,
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
    items_count_format: String,
    window: &mut Window,
    cx: &mut Context<FileSidebarView>,
) -> impl IntoElement {
    let FileTreeRow {
        file,
        active,
        expanded,
        editing,
        editing_value,
        drag_paths,
        depth,
    } = row;
    let entry = file.clone();
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
        .when(FILE_TREE_DRAG_AND_DROP, |this| {
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
        .when(FILE_TREE_DRAG_AND_DROP && is_dir, |this| {
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
                let relative_path = right_click_entry.relative_path.clone();
                view.defer_app_update(window, cx, move |app, _window, cx| {
                    app.prepare_file_context_menu_selection(relative_path, cx);
                });
            }),
        )
        .child(
            div()
                .w(px(18.0))
                .flex_none()
                .mr(px(4.0))
                .flex()
                .items_center()
                .justify_center()
                .child(if is_dir {
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
            let input_state = file_name_draft_input_state(
                app_entity,
                FileNameDraftKind::Rename,
                &editing_value,
                true,
                window,
                cx,
            );
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
                .ml(px(8.0))
                .flex_1()
                .min_w_0()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(color(theme::TEXT_MUTED))
                .truncate()
                .child(file.name)
                .into_any_element()
        })
}

pub(in crate::app) fn clipboard_external_paths(cx: &mut App) -> Vec<String> {
    let Some(item) = cx.read_from_clipboard() else {
        return Vec::new();
    };
    let mut paths = item
        .entries()
        .iter()
        .flat_map(|entry| match entry {
            ClipboardEntry::ExternalPaths(paths) => paths
                .paths()
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            ClipboardEntry::String(text) => text
                .text()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>(),
            ClipboardEntry::Image(_) => Vec::new(),
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

pub(in crate::app) fn file_preview_workspace(
    preview: &str,
    editable: bool,
    dirty: bool,
    search_open: bool,
    search_query: &str,
    search_match_index: usize,
    preview_scroll_handle: UniformListScrollHandle,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let match_lines = file_search_match_lines(preview, search_query);
    let active_line = match_lines.get(search_match_index).copied();
    let match_count = match_lines.len();
    let preview_lines = Rc::new(
        preview
            .lines()
            .map(|line| line.chars().take(160).collect::<String>())
            .collect::<Vec<_>>(),
    );
    let match_lines = Rc::new(match_lines);

    div()
        .flex()
        .flex_col()
        .flex_1()
        .child(
            div()
                .h(px(34.0))
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .text_xs()
                .text_color(color(theme::TEXT_MUTED))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(if editable {
                            if dirty {
                                "Editor · modified"
                            } else {
                                "Editor"
                            }
                        } else {
                            "Preview"
                        })
                        .when(search_open && !search_query.trim().is_empty(), |this| {
                            this.child(
                                div()
                                    .h(px(20.0))
                                    .px(px(7.0))
                                    .rounded(px(5.0))
                                    .bg(color(theme::TEXT).opacity(0.08))
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(color(theme::TEXT_MUTED))
                                    .flex()
                                    .items_center()
                                    .child(if match_count == 0 {
                                        "0".to_string()
                                    } else {
                                        format!("{}/{}", search_match_index + 1, match_count)
                                    }),
                            )
                        }),
                )
                .child(header_icon_button(
                    "file-preview-search",
                    HeroIconName::MagnifyingGlass,
                    cx,
                    |app, _event, _window, cx| app.open_file_search(cx),
                )),
        )
        .when(search_open, |this| {
            this.child(file_preview_search_bar(
                search_query,
                match_count,
                search_match_index,
                editable,
                window,
                cx,
            ))
        })
        .child(
            div()
                .flex_1()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .p_3()
                .text_xs()
                .text_color(color(theme::TEXT))
                .child(codux_uniform_list(
                    "file-preview-lines",
                    preview_lines,
                    preview_scroll_handle,
                    None,
                    cx,
                    move |line, index, _window, _cx| {
                        let matched = match_lines.binary_search(&index).is_ok();
                        let active = active_line == Some(index);
                        div()
                            .w_full()
                            .min_w_0()
                            .min_h(px(18.0))
                            .px(px(4.0))
                            .rounded(px(4.0))
                            .when(matched, |this| {
                                this.bg(color(if active { theme::ACCENT } else { theme::TEXT })
                                    .opacity(if active { 0.18 } else { 0.08 }))
                            })
                            .child(line)
                            .into_any_element()
                    },
                )),
        )
}

fn file_preview_search_bar(
    search_query: &str,
    match_count: usize,
    search_match_index: usize,
    editable: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let query_value = search_query.to_string();
    let input_state = window.use_keyed_state("file-preview-search-query", cx, |window, cx| {
        InputState::new(window, cx).placeholder("Find")
    });
    input_state.update(cx, |state, cx| {
        if state.value().as_ref() != query_value {
            state.set_value(query_value.clone(), window, cx);
        }
    });
    cx.subscribe_in(&input_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_file_search_query(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .flex_shrink_0()
        .px_3()
        .py(px(8.0))
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().title_bar)
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(260.0))
                .child(Input::new(&input_state).with_size(gpui_component::Size::Small)),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_DIM))
                .min_w(px(58.0))
                .child(if search_query.trim().is_empty() {
                    "Find".to_string()
                } else if match_count == 0 {
                    "0 matches".to_string()
                } else {
                    format!("{}/{}", search_match_index + 1, match_count)
                }),
        )
        .child(header_icon_button(
            "file-preview-search-prev",
            HeroIconName::ChevronUp,
            cx,
            |app, _event, window, cx| app.select_previous_file_search_match(window, cx),
        ))
        .child(header_icon_button(
            "file-preview-search-next",
            HeroIconName::ChevronDown,
            cx,
            |app, _event, window, cx| app.select_next_file_search_match(window, cx),
        ))
        .child(
            div()
                .flex_1()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_DIM))
                .child(if editable { "" } else { "Read only" }),
        )
        .child(header_icon_button(
            "file-preview-search-close",
            HeroIconName::XMark,
            cx,
            |app, _event, window, cx| app.close_file_search(window, cx),
        ))
}

fn file_search_match_lines(preview: &str, query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    preview
        .lines()
        .enumerate()
        .filter_map(|(index, line)| line.to_lowercase().contains(&query).then_some(index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::file_search_match_lines;

    #[test]
    fn file_search_match_lines_are_case_insensitive_and_limited() {
        let preview = "Alpha\nbeta\nALPHA beta\n";
        assert_eq!(file_search_match_lines(preview, "alpha"), vec![0, 2]);
        assert_eq!(file_search_match_lines(preview, " BETA "), vec![1, 2]);
        assert!(file_search_match_lines(preview, "").is_empty());
    }
}
