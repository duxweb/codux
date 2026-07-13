use super::*;
use crate::app::app_state::RemoteBrowseEntry;
use gpui::Focusable;
use gpui_component::input::{Input, InputEvent, InputState, SelectAll};

impl CoduxApp {
    pub(in crate::app) fn project_editor_workspace(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let language = self.state.settings.language.as_str();
        let locale = locale_from_language_setting(language);
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        let is_editing = self.project_editor_project_id.is_some();
        let title = if is_editing {
            tr("project.edit.title", "Edit Project")
        } else {
            tr("project.create.title", "Create Project")
        };
        let submit_label = if is_editing {
            tr("common.save", "Save")
        } else {
            tr("common.create", "Create")
        };
        let can_submit = !self.project_editor_saving
            && !self.project_editor_name.trim().is_empty()
            && !self.project_editor_path.trim().is_empty();

        child_window_shell(title, cx)
            .child(
                div()
                    .min_h_0()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .flex()
                    .flex_col()
                    .p(px(18.0))
                    .child(project_editor_field(
                        tr("project.editor.name", "Project Name"),
                        "project-editor-name",
                        &self.project_editor_name,
                        "Project",
                        window,
                        cx,
                        |app, value, window, cx| app.set_project_editor_name(value, window, cx),
                    ))
                    .child({
                        let (device_label, is_hosted) = match &self.project_editor_runtime_target {
                            ProjectRuntimeTarget::Local => {
                                (tr("project.editor.device.local", "Local"), false)
                            }
                            ProjectRuntimeTarget::Wsl { distribution } => {
                                (format!("WSL · {distribution}"), true)
                            }
                            ProjectRuntimeTarget::Remote { device_id } => (
                                self.runtime_service
                                    .saved_remote_hosts()
                                    .into_iter()
                                    .find(|host| &host.device_id == device_id)
                                    .map(|host| {
                                        if host.host_name.trim().is_empty() {
                                            host.host_id
                                        } else {
                                            host.host_name
                                        }
                                    })
                                    .unwrap_or_else(|| {
                                        tr("project.editor.device.remote", "Device")
                                    }),
                                true,
                            ),
                        };
                        project_editor_path_field(
                            tr("project.editor.directory", "Project Directory"),
                            tr("project.editor.choose_directory.prompt", "Choose"),
                            &self.project_editor_path,
                            device_label,
                            is_hosted,
                            window,
                            cx,
                        )
                    })
                    .child(project_editor_symbol_field(
                        tr("project.editor.icon", "Project Icon"),
                        tr("common.none", "None"),
                        self.project_editor_badge_symbol.as_deref(),
                        &self.project_editor_badge_color_hex,
                        cx,
                    ))
                    .child(project_editor_color_field(
                        tr("project.editor.color", "Project Color"),
                        &self.project_editor_badge_color_hex,
                        cx,
                    )),
            )
            .child(dialog_footer_bar(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(dialog_cancel_button(
                        "project-editor-cancel",
                        tr("common.cancel", "Cancel"),
                        cx,
                        |_app, _event, window, _cx| {
                            window.remove_window();
                        },
                    ))
                    .child(
                        dialog_primary_button(
                            "project-editor-save",
                            submit_label,
                            cx,
                            |app, _event, window, cx| {
                                app.save_project_editor(window, cx);
                            },
                        )
                        .disabled(!can_submit)
                        .loading(self.project_editor_saving),
                    ),
                cx,
            ))
    }
}

impl CoduxApp {
    /// The file-picker sub-window: a standard child window (shared title bar via
    /// `child_window_shell`, shared `dialog_footer_bar`) for browsing a local or
    /// remote-host directory and picking a folder. The chosen path is pushed back
    /// to the project-editor window (the opener).
    pub(in crate::app) fn file_picker_window(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = locale_from_language_setting(self.state.settings.language.as_str());
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        let mode = self.file_picker_mode;
        let title = match mode {
            FilePickerMode::OpenFolder => tr("project.editor.browse.title", "Choose Folder"),
            FilePickerMode::OpenFile => tr("file.picker.open.title", "Open File"),
            FilePickerMode::Save => tr("file.picker.save.title", "Save As"),
        };
        let confirm_label = match mode {
            FilePickerMode::OpenFolder => tr("project.editor.browse.use", "Use this folder"),
            FilePickerMode::OpenFile => tr("file.picker.open.confirm", "Open"),
            FilePickerMode::Save => tr("file.picker.save.confirm", "Save"),
        };
        let current = self.project_editor_browse_path.clone();
        let can_confirm =
            self.file_picker_result_path().is_some() && !self.project_editor_browse_busy;

        // Left: the device sidebar (Local + each paired host). Clicking a
        // device re-lists from its root.
        let active_target = self.project_editor_runtime_target.clone();
        let mut devices = div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(8.0))
            .overflow_y_scrollbar()
            .child(file_picker_device_row(
                "file-picker-device-local".to_string(),
                tr("project.editor.device.local", "Local"),
                active_target == ProjectRuntimeTarget::Local,
                cx,
                |app, window, cx| {
                    app.file_picker_switch_runtime(ProjectRuntimeTarget::Local, window, cx)
                },
            ));
        for distribution in self.runtime_service.wsl_distributions().unwrap_or_default() {
            let runtime_target = ProjectRuntimeTarget::Wsl {
                distribution: distribution.name.clone(),
            };
            let selected = active_target == runtime_target;
            let target = runtime_target.clone();
            devices = devices.child(file_picker_wsl_device_row(
                distribution.name,
                selected,
                cx,
                move |app, window, cx| app.file_picker_switch_runtime(target.clone(), window, cx),
            ));
        }
        for host in self.runtime_service.saved_remote_hosts() {
            let device_id = host.device_id.clone();
            let selected = active_target
                == ProjectRuntimeTarget::Remote {
                    device_id: host.device_id.clone(),
                };
            let label = if host.host_name.trim().is_empty() {
                host.host_id.clone()
            } else {
                host.host_name.clone()
            };
            devices = devices.child(file_picker_device_row(
                format!("file-picker-device-{}", host.device_id),
                label,
                selected,
                cx,
                move |app, window, cx| {
                    app.file_picker_switch_runtime(
                        ProjectRuntimeTarget::Remote {
                            device_id: device_id.clone(),
                        },
                        window,
                        cx,
                    )
                },
            ));
        }

        // Right: breadcrumb + listing + new-folder/filename rows. The listing
        // lives in a separately constrained scroll body so it cannot grow under
        // the fixed dialog footer when a folder has many entries.
        let mut list = div()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(px(16.0))
            .pt(px(10.0))
            .pb(px(12.0));
        let visible_entries = self.project_editor_browse_entries.len()
            + usize::from(self.project_editor_browse_parent.is_some())
            + usize::from(self.file_picker_new_folder_active);
        let entry_labels = FilePickerEntryLabels {
            open: tr("common.open", "Open"),
            choose: tr("common.choose", "Choose"),
            rename: tr("common.rename", "Rename"),
            delete: tr("common.delete", "Delete"),
        };
        if let Some(parent) = self.project_editor_browse_parent.clone() {
            list = list.child(file_picker_entry_row(
                RemoteBrowseEntry {
                    name: "..".to_string(),
                    path: parent.clone(),
                    is_dir: true,
                },
                self.file_picker_active_path.as_deref() == Some(parent.as_str()),
                mode,
                &entry_labels,
                cx,
                false,
            ));
        }
        if self.project_editor_browse_busy && visible_entries == 0 {
            list = list.child(file_picker_status_state(
                HeroIconName::ArrowPath,
                tr("file.picker.loading", "Loading folder…"),
                true,
                cx,
            ));
        } else if !self.project_editor_browse_busy
            && visible_entries == 0
            && self.project_editor_browse_error.is_none()
        {
            list = list.child(file_picker_status_state(
                HeroIconName::FolderOpen,
                tr("file.picker.empty", "This folder is empty."),
                false,
                cx,
            ));
        }
        if self.file_picker_new_folder_active {
            list = list.child(file_picker_new_folder_row(
                &self.project_editor_browse_new_folder,
                tr(
                    "project.editor.browse.new_folder_placeholder",
                    "New folder name",
                ),
                window,
                cx,
            ));
        }
        for entry in &self.project_editor_browse_entries {
            if self
                .file_picker_rename_draft
                .as_ref()
                .is_some_and(|draft| draft.path == entry.path)
            {
                list = list.child(file_picker_rename_row(
                    entry.is_dir,
                    self.file_picker_rename_draft
                        .as_ref()
                        .map(|draft| draft.name.as_str())
                        .unwrap_or(entry.name.as_str()),
                    tr("file.picker.rename.placeholder", "New name"),
                    window,
                    cx,
                ));
            } else {
                let selected = self.file_picker_active_path.as_deref() == Some(entry.path.as_str())
                    || (!entry.is_dir
                        && self.file_picker_selected.as_deref() == Some(entry.path.as_str()));
                list = list.child(file_picker_entry_row(
                    entry.clone(),
                    selected,
                    mode,
                    &entry_labels,
                    cx,
                    true,
                ));
            }
        }

        let root_label = match &active_target {
            ProjectRuntimeTarget::Local => tr("project.editor.device.local", "Local"),
            ProjectRuntimeTarget::Wsl { distribution } => format!("WSL · {distribution}"),
            ProjectRuntimeTarget::Remote { device_id } => self
                .runtime_service
                .saved_remote_hosts()
                .into_iter()
                .find(|host| &host.device_id == device_id)
                .map(|host| {
                    if host.host_name.trim().is_empty() {
                        host.host_id
                    } else {
                        host.host_name
                    }
                })
                .unwrap_or_else(|| tr("project.editor.device.remote", "Device")),
        };
        let mut right = div()
            .min_h_0()
            .flex_1()
            .flex_basis(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(color(theme::BORDER_SOFT))
                    .child(
                        gpui_component::scroll::ScrollableElement::overflow_x_scrollbar(
                            div().flex_1().min_w_0(),
                        )
                        .child(file_picker_breadcrumb(
                            &current,
                            &root_label,
                            active_target.is_hosted(),
                            cx,
                        )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .px(px(12.0))
                            .child(file_picker_refresh_button(
                                self.project_editor_browse_busy,
                                tr("common.refresh", "Refresh"),
                                cx,
                            )),
                    ),
            )
            .child(
                crate::app::scroll_compat::ScrollableElement::overflow_y_scrollbar(
                    div().min_h_0().flex_1().flex_basis(px(0.0)),
                )
                .child(list),
            );
        // Save mode: a filename row (prefilled when an existing file is clicked).
        if mode == FilePickerMode::Save {
            right = right.child(div().flex_none().min_w_0().px(px(16.0)).pb(px(12.0)).child(
                project_editor_input(
                    "file-picker-filename",
                    &self.file_picker_filename,
                    "File name",
                    window,
                    cx,
                    |app, value, window, cx| app.set_file_picker_filename(value, window, cx),
                ),
            ));
        }
        if let Some(error) = self.project_editor_browse_error.as_ref() {
            right = right.child(
                div()
                    .flex_none()
                    .px(px(16.0))
                    .pb(px(12.0))
                    .text_size(rems(0.8125))
                    .text_color(color(theme::ORANGE))
                    .child(error.clone()),
            );
        }

        let body = div().min_h_0().flex_1().overflow_hidden().flex().child(
            h_resizable("file-picker-device-split")
                .child(
                    resizable_panel()
                        .size(px(180.0))
                        .size_range(px(140.0)..px(280.0))
                        .child(devices),
                )
                .child(
                    resizable_panel()
                        .size_range(px(320.0)..Pixels::MAX)
                        .child(right),
                ),
        );

        let new_folder_disabled =
            self.project_editor_browse_busy || self.project_editor_browse_path.trim().is_empty();
        child_window_shell(SharedString::from(title), cx)
            .child(body)
            .child(dialog_footer_bar(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Button::new("file-picker-new-folder")
                            .secondary()
                            .compact()
                            .icon(Icon::new(HeroIconName::FolderPlus).size_3p5())
                            .child(dialog_button_label(tr(
                                "project.editor.browse.new_folder",
                                "New folder",
                            )))
                            .disabled(new_folder_disabled)
                            .on_click(cx.listener(|app, _event, _window, cx| {
                                app.begin_file_picker_new_folder(cx);
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(dialog_cancel_button(
                                "file-picker-cancel",
                                tr("common.cancel", "Cancel"),
                                cx,
                                |_app, _event, window, _cx| {
                                    window.remove_window();
                                },
                            ))
                            .child(
                                dialog_primary_button(
                                    "file-picker-use",
                                    confirm_label,
                                    cx,
                                    |app, _event, window, cx| app.file_picker_select(window, cx),
                                )
                                .disabled(!can_confirm),
                            ),
                    ),
                cx,
            ))
            .into_any_element()
    }
}

/// The inline new-folder name editor shown in the listing: a folder icon + a
/// text field that commits on Enter (or blur when non-empty) and dismisses on
/// blur when left empty.
fn file_picker_new_folder_row(
    value: &str,
    placeholder: String,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let value = value.to_string();
    let input_state =
        window.use_keyed_state(SharedString::from("file-picker-newfolder-inline"), cx, {
            let placeholder = placeholder.clone();
            move |window, cx| InputState::new(window, cx).placeholder(placeholder.clone())
        });
    input_state.update(cx, |state, cx| {
        if !state.focus_handle(cx).is_focused(window) {
            if state.value().as_ref() != value {
                state.set_value(value.clone(), window, cx);
            }
            state.focus(window, cx);
            if state.selected_range().is_empty() {
                window.dispatch_action(Box::new(SelectAll), cx);
            }
        }
    });
    cx.subscribe_in(
        &input_state,
        window,
        move |app, state, event, window, cx| match event {
            InputEvent::Change => app.set_project_editor_browse_new_folder(
                state.read(cx).value().to_string(),
                window,
                cx,
            ),
            InputEvent::PressEnter { .. } => app.project_editor_browse_create_folder(window, cx),
            InputEvent::Blur => {
                if app.project_editor_browse_new_folder.trim().is_empty() {
                    app.cancel_file_picker_new_folder(cx);
                } else {
                    app.project_editor_browse_create_folder(window, cx);
                }
            }
            _ => {}
        },
    )
    .detach();

    div()
        .flex()
        .items_center()
        .gap_2()
        .px(px(8.0))
        .py(px(4.0))
        .child(
            Icon::new(HeroIconName::Folder)
                .size_4()
                .text_color(color(theme::ACCENT)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(Input::new(&input_state).with_size(gpui_component::Size::Small)),
        )
        .into_any_element()
}

fn file_picker_rename_row(
    is_dir: bool,
    value: &str,
    placeholder: String,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let value = value.to_string();
    let input_state =
        window.use_keyed_state(SharedString::from("file-picker-rename-inline"), cx, {
            let placeholder = placeholder.clone();
            move |window, cx| InputState::new(window, cx).placeholder(placeholder.clone())
        });
    input_state.update(cx, |state, cx| {
        if !state.focus_handle(cx).is_focused(window) {
            if state.value().as_ref() != value {
                state.set_value(value.clone(), window, cx);
            }
            state.focus(window, cx);
            if state.selected_range().is_empty() {
                window.dispatch_action(Box::new(SelectAll), cx);
            }
        }
    });
    cx.subscribe_in(
        &input_state,
        window,
        move |app, state, event, window, cx| match event {
            InputEvent::Change => {
                app.set_file_picker_rename_name(state.read(cx).value().to_string(), window, cx)
            }
            InputEvent::PressEnter { .. } => app.confirm_file_picker_rename(window, cx),
            InputEvent::Blur => {
                if app
                    .file_picker_rename_draft
                    .as_ref()
                    .is_some_and(|draft| draft.name.trim().is_empty())
                {
                    app.cancel_file_picker_rename(cx);
                } else {
                    app.confirm_file_picker_rename(window, cx);
                }
            }
            _ => {}
        },
    )
    .detach();

    div()
        .flex()
        .items_center()
        .gap_2()
        .px(px(8.0))
        .py(px(4.0))
        .child(
            Icon::new(if is_dir {
                HeroIconName::Folder
            } else {
                HeroIconName::Document
            })
            .size_4()
            .text_color(color(if is_dir {
                theme::ACCENT
            } else {
                theme::TEXT_MUTED
            })),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(Input::new(&input_state).with_size(gpui_component::Size::Small)),
        )
        .into_any_element()
}

struct FilePickerEntryLabels {
    open: String,
    choose: String,
    rename: String,
    delete: String,
}

fn file_picker_entry_row(
    entry: RemoteBrowseEntry,
    selected: bool,
    mode: FilePickerMode,
    labels: &FilePickerEntryLabels,
    cx: &mut Context<CoduxApp>,
    allow_mutations: bool,
) -> AnyElement {
    let open_label = labels.open.clone();
    let choose_label = labels.choose.clone();
    let rename_label = labels.rename.clone();
    let delete_label = labels.delete.clone();
    let id = format!("file-picker-{}", entry.path);
    let label = entry.name.clone();
    let is_dir = entry.is_dir;
    let click_path = entry.path.clone();
    let right_click_entry = entry.clone();
    let mut row = div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .gap_2()
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, _event, _window, cx| {
                app.select_file_picker_context_entry(right_click_entry.clone(), cx);
            }),
        )
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.file_picker_choose_entry(click_path.clone(), is_dir, window, cx)
        }))
        .child(
            Icon::new(if is_dir {
                HeroIconName::Folder
            } else {
                HeroIconName::Document
            })
            .size_4()
            .text_color(color(if is_dir {
                theme::ACCENT
            } else {
                theme::TEXT_MUTED
            })),
        )
        .child(
            div()
                .text_size(rems(0.8125))
                .text_color(color(theme::TEXT))
                .truncate()
                .child(label),
        );
    if selected {
        row = row.bg(cx.theme().secondary);
    }
    let app_entity = cx.entity();
    row.context_menu(move |menu, _window, _cx| {
        let open_entity = app_entity.clone();
        let rename_entity = app_entity.clone();
        let delete_entity = app_entity.clone();
        let open_entry = entry.clone();
        let rename_entry = entry.clone();
        let delete_entry = entry.clone();
        let primary_label = if is_dir || mode == FilePickerMode::OpenFile {
            open_label.clone()
        } else {
            choose_label.clone()
        };
        let menu = menu.item(
            PopupMenuItem::new(primary_label)
                .icon(if is_dir {
                    HeroIconName::FolderOpen
                } else {
                    HeroIconName::Check
                })
                .on_click(move |_, window, cx| {
                    cx.update_entity(&open_entity, |app: &mut CoduxApp, cx| {
                        app.file_picker_choose_entry(
                            open_entry.path.clone(),
                            open_entry.is_dir,
                            window,
                            cx,
                        );
                    });
                }),
        );
        if !allow_mutations {
            return menu;
        }
        menu.separator()
            .item(
                PopupMenuItem::new(rename_label.clone())
                    .icon(HeroIconName::PencilSquare)
                    .on_click(move |_, _window, cx| {
                        cx.update_entity(&rename_entity, |app: &mut CoduxApp, cx| {
                            app.start_file_picker_rename(rename_entry.clone(), cx);
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(delete_label.clone())
                    .icon(HeroIconName::Trash)
                    .on_click(move |_, _window, cx| {
                        cx.update_entity(&delete_entity, |app: &mut CoduxApp, cx| {
                            app.request_delete_file_picker_entry(delete_entry.clone(), cx);
                        });
                    }),
            )
    })
    .into_any_element()
}

fn file_picker_status_state(
    icon: HeroIconName,
    label: String,
    loading: bool,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    div()
        .min_h(px(180.0))
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .text_color(cx.theme().muted_foreground)
        .child(
            div()
                .size(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.0))
                .bg(cx.theme().secondary.opacity(0.6))
                .child(if loading {
                    Spinner::new()
                        .small()
                        .color(cx.theme().muted_foreground)
                        .into_any_element()
                } else {
                    Icon::new(icon)
                        .size_4()
                        .text_color(cx.theme().muted_foreground)
                        .into_any_element()
                }),
        )
        .child(
            div()
                .text_size(rems(0.8125))
                .line_height(rems(1.125))
                .child(label),
        )
        .into_any_element()
}

/// A device row in the file picker's left sidebar (This Mac / a host).
fn file_picker_device_row(
    id: String,
    label: String,
    selected: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let mut row = div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .gap_2()
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .text_color(color(theme::TEXT))
        // Hover tints the text (and icon, via inheritance) the theme accent;
        // background is reserved for the selected state only.
        .hover(|style| style.text_color(cx.theme().primary))
        .child(Icon::new(HeroIconName::GlobeAlt).size_3())
        .child(div().text_size(rems(0.8125)).truncate().child(label))
        .on_click(cx.listener(move |app, _event, window, cx| on_click(app, window, cx)));
    if selected {
        row = row.bg(cx.theme().secondary);
    }
    row.into_any_element()
}

fn file_picker_wsl_device_row(
    distribution: String,
    selected: bool,
    cx: &mut Context<CoduxApp>,
    on_select: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let id = format!("file-picker-wsl-{distribution}");
    let mut row = div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .gap_2()
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .text_color(color(theme::TEXT))
        .hover(|style| style.text_color(cx.theme().primary))
        .child(Icon::new(HeroIconName::CommandLine).size_3())
        .child(
            div()
                .h(px(18.0))
                .px(px(5.0))
                .rounded(px(4.0))
                .flex_none()
                .flex()
                .items_center()
                .text_size(rems(0.625))
                .line_height(rems(0.875))
                .font_weight(FontWeight::MEDIUM)
                .bg(color(theme::ACCENT).opacity(0.12))
                .text_color(color(theme::ACCENT))
                .child("WSL"),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .text_size(rems(0.8125))
                .truncate()
                .child(distribution),
        )
        .on_click(cx.listener(move |app, _event, window, cx| on_select(app, window, cx)));
    if selected {
        row = row.bg(cx.theme().secondary);
    }
    row.into_any_element()
}

/// A Finder-style path bar for the current directory: a device root crumb
/// (computer/server icon + label) followed by chevron-separated, clickable path
/// segments. Sits under a bottom border to set it off from the listing.
fn file_picker_breadcrumb(
    path: &str,
    root_label: &str,
    is_remote: bool,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let bar = div()
        .flex()
        .items_center()
        .whitespace_nowrap()
        .gap(px(2.0))
        .px(px(16.0))
        .pt(px(16.0))
        .pb(px(10.0));
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return bar
            .child(
                div()
                    .text_size(rems(0.8125))
                    .text_color(color(theme::TEXT_MUTED))
                    .child("Loading…"),
            )
            .into_any_element();
    }
    // The root crumb stands in for the bare filesystem root: an icon + the
    // device label (e.g. "Local" / a host name) navigating to the root target
    // (`/` on POSIX, the drive list on Windows).
    let (root_target, segments) = file_picker_breadcrumb_model(trimmed);
    let mut row = bar.child(file_picker_root_crumb(
        root_label,
        &root_target,
        is_remote,
        cx,
    ));
    for segment in segments {
        row = row.child(file_picker_crumb_separator());
        row = row.child(file_picker_crumb(
            &format!("file-picker-crumb-{}", segment.target),
            &segment.label,
            &segment.target,
            cx,
        ));
    }
    row.into_any_element()
}

/// Re-list the current directory. Remote browsing never auto-refreshes (the
/// watcher is host-side), so this is the manual reload in the footer actions.
fn file_picker_refresh_button(
    loading: bool,
    tooltip: String,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    Button::new("file-picker-refresh")
        .ghost()
        .compact()
        .with_size(Size::Small)
        .icon(Icon::new(HeroIconName::ArrowPath).size_3p5())
        .tooltip(tooltip)
        .loading(loading)
        .disabled(loading)
        .on_click(cx.listener(|app, _event, window, cx| {
            let current = app.project_editor_browse_path.clone();
            if !current.trim().is_empty() {
                app.project_editor_browse_navigate(Some(current), window, cx);
            }
        }))
        .into_any_element()
}

/// One clickable breadcrumb segment: its visible label and the absolute path it
/// navigates to.
struct FilePickerCrumb {
    label: String,
    target: String,
}

/// Adapt the shared, typed-path-backed [`codux_runtime::path::breadcrumb_segments`]
/// splitter into the picker's crumb type. Splitting lives in the core crate so
/// Windows / POSIX / UNC / drive-list paths are all parsed the same way wherever
/// they're shown.
fn file_picker_breadcrumb_model(path: &str) -> (String, Vec<FilePickerCrumb>) {
    let (root_target, segments) = codux_runtime::path::breadcrumb_segments(path);
    let crumbs = segments
        .into_iter()
        .map(|(label, target)| FilePickerCrumb { label, target })
        .collect();
    (root_target, crumbs)
}

/// The `›` chevron between path segments.
fn file_picker_crumb_separator() -> AnyElement {
    Icon::new(HeroIconName::ChevronRight)
        .size_3()
        .flex_shrink_0()
        .text_color(color(theme::TEXT_DIM))
        .into_any_element()
}

/// The leading device crumb (icon + label) that navigates to the root target
/// (`/` on POSIX, the drive list on Windows).
fn file_picker_root_crumb(
    label: &str,
    root_target: &str,
    is_remote: bool,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let root_target = root_target.to_string();
    div()
        .id("file-picker-crumb-root")
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(5.0))
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary))
        .child(
            Icon::new(if is_remote {
                HeroIconName::ServerStack
            } else {
                HeroIconName::ComputerDesktop
            })
            .size_4()
            .text_color(color(theme::TEXT_MUTED)),
        )
        .child(
            div()
                .text_size(rems(0.8125))
                .font_weight(FontWeight::MEDIUM)
                .text_color(color(theme::TEXT))
                .child(label.to_string()),
        )
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.project_editor_browse_navigate(Some(root_target.clone()), window, cx)
        }))
        .into_any_element()
}

fn file_picker_crumb(
    id: &str,
    label: &str,
    target: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let target = target.to_string();
    div()
        .id(SharedString::from(id.to_string()))
        .flex_shrink_0()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary))
        .text_size(rems(0.8125))
        .text_color(color(theme::TEXT))
        .child(label.to_string())
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.project_editor_browse_navigate(Some(target.clone()), window, cx)
        }))
        .into_any_element()
}

struct ProjectEditorSymbol {
    id: &'static str,
    icon: Option<HeroIconName>,
}

const PROJECT_EDITOR_SYMBOLS: &[ProjectEditorSymbol] = &[
    ProjectEditorSymbol {
        id: "none",
        icon: None,
    },
    ProjectEditorSymbol {
        id: "terminal",
        icon: Some(HeroIconName::CommandLine),
    },
    ProjectEditorSymbol {
        id: "folder",
        icon: Some(HeroIconName::Folder),
    },
    ProjectEditorSymbol {
        id: "shippingbox",
        icon: Some(HeroIconName::Sparkles),
    },
    ProjectEditorSymbol {
        id: "hammer",
        icon: Some(HeroIconName::WrenchScrewdriver),
    },
    ProjectEditorSymbol {
        id: "server.rack",
        icon: Some(HeroIconName::GlobeAlt),
    },
    ProjectEditorSymbol {
        id: "globe",
        icon: Some(HeroIconName::GlobeAlt),
    },
    ProjectEditorSymbol {
        id: "bolt",
        icon: Some(HeroIconName::Star),
    },
    ProjectEditorSymbol {
        id: "wrench",
        icon: Some(HeroIconName::Cog6Tooth),
    },
    ProjectEditorSymbol {
        id: "doc.text",
        icon: Some(HeroIconName::Document),
    },
    ProjectEditorSymbol {
        id: "book",
        icon: Some(HeroIconName::BookOpen),
    },
    ProjectEditorSymbol {
        id: "person.2",
        icon: Some(HeroIconName::UserCircle),
    },
];

fn project_editor_field(
    label: String,
    id: &'static str,
    value: &str,
    placeholder: impl Into<String>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .mb(px(24.0))
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .child(label),
        )
        .child(project_editor_input(
            id,
            value,
            placeholder,
            window,
            cx,
            action,
        ))
        .into_any_element()
}

fn project_editor_symbol_field(
    label: String,
    none_label: String,
    selected_symbol: Option<&str>,
    selected_color: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let accent = hex_color(selected_color).unwrap_or(theme::ACCENT);
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .mb(px(24.0))
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .child(label),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .children(PROJECT_EDITOR_SYMBOLS.iter().map(|symbol| {
                    let id = symbol.id;
                    let selected = if id == "none" {
                        selected_symbol.is_none()
                    } else {
                        selected_symbol == Some(id)
                    };
                    div()
                        .id(SharedString::from(format!("project-editor-symbol-{id}")))
                        .size(px(36.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(color(if selected {
                            theme::BORDER
                        } else {
                            theme::BORDER_SOFT
                        }))
                        .bg(if selected {
                            cx.theme().secondary_hover
                        } else {
                            cx.theme().secondary
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|style| style.bg(cx.theme().secondary_hover))
                        .on_click(cx.listener(move |app, _event, window, cx| {
                            let next = (id != "none").then(|| id.to_string());
                            app.set_project_editor_badge_symbol(next, window, cx);
                        }))
                        .child(match symbol.icon {
                            Some(icon) => Icon::new(icon)
                                .size_4()
                                .text_color(color(accent))
                                .into_any_element(),
                            None => div()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_MUTED))
                                .child(none_label.clone())
                                .into_any_element(),
                        })
                        .into_any_element()
                })),
        )
        .into_any_element()
}

fn project_editor_color_field(
    label: String,
    selected_color: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .child(label),
        )
        .child(div().flex().flex_wrap().items_center().gap_3().children(
            PROJECT_BADGE_COLORS.iter().map(|value| {
                let selected = *value == selected_color;
                let swatch = hex_color(value).unwrap_or(theme::ACCENT);
                div()
                    .id(SharedString::from(format!("project-editor-color-{value}")))
                    .size(px(24.0))
                    .rounded_full()
                    .bg(color(swatch))
                    .border_1()
                    .border_color(color(if selected {
                        theme::TEXT
                    } else {
                        theme::BORDER_SOFT
                    }))
                    .cursor_pointer()
                    .hover(|style| style.opacity(0.86))
                    .on_click(cx.listener(move |app, _event, window, cx| {
                        app.set_project_editor_badge_color((*value).to_string(), window, cx);
                    }))
                    .into_any_element()
            }),
        ))
        .into_any_element()
}

fn hex_color(value: &str) -> Option<u32> {
    let value = value.trim().trim_start_matches('#');
    if value.len() == 6 {
        u32::from_str_radix(value, 16).ok()
    } else {
        None
    }
}

fn project_editor_input(
    id: &'static str,
    value: &str,
    placeholder: impl Into<String>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let value = value.to_string();
    let placeholder = placeholder.into();
    let input_state = window.use_keyed_state(SharedString::from(id), cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .placeholder(placeholder.clone())
    });
    input_state.update(cx, |state, cx| {
        if state.value().as_ref() != value {
            state.set_value(value.clone(), window, cx);
        }
    });
    cx.subscribe_in(
        &input_state,
        window,
        move |app, state, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                action(app, state.read(cx).value().to_string(), window, cx);
            }
        },
    )
    .detach();

    Input::new(&input_state)
        .with_size(gpui_component::Size::Medium)
        .into_any_element()
}

fn project_editor_path_field(
    label: String,
    choose_label: String,
    path: &str,
    device_label: String,
    is_remote: bool,
    _window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    // The directory is read-only: it can only be set via the picker (Choose), so
    // the device + path always stay consistent. The whole box is clickable.
    let device_prefix = div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .pr(px(8.0))
        .mr(px(8.0))
        .border_r_1()
        .border_color(color(theme::BORDER_SOFT))
        .child(
            Icon::new(if is_remote {
                HeroIconName::ServerStack
            } else {
                HeroIconName::ComputerDesktop
            })
            .size_4()
            .text_color(color(theme::TEXT_MUTED)),
        )
        .child(
            div()
                .max_w(px(140.0))
                .truncate()
                .text_size(rems(0.8125))
                .text_color(color(theme::TEXT_MUTED))
                .child(device_label),
        );
    let has_path = !path.trim().is_empty();
    let path_text = if has_path {
        path.to_string()
    } else {
        "/path/to/project".to_string()
    };
    let path_box = div()
        .id("project-editor-path")
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .h(px(34.0))
        .px(px(10.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(color(theme::BORDER_SOFT))
        .bg(cx.theme().secondary.opacity(0.4))
        .cursor_pointer()
        .hover(|style| style.border_color(color(theme::BORDER)))
        .on_click(cx.listener(|app, _event, window, cx| {
            app.choose_project_editor_directory(window, cx);
        }))
        .child(device_prefix)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(rems(0.8125))
                .text_color(color(if has_path {
                    theme::TEXT
                } else {
                    theme::TEXT_DIM
                }))
                .child(path_text),
        );

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .mb(px(24.0))
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .child(label),
        )
        .child(
            div().flex().items_center().gap_2().child(path_box).child(
                Button::new("project-editor-choose-path")
                    .secondary()
                    .compact()
                    .text_color(cx.theme().secondary_foreground)
                    .child(dialog_button_label(choose_label))
                    .on_click(cx.listener(|app, _event, window, cx| {
                        app.choose_project_editor_directory(window, cx);
                    })),
            ),
        )
        .into_any_element()
}

#[cfg(test)]
mod breadcrumb_tests {
    use super::file_picker_breadcrumb_model;

    fn crumbs(path: &str) -> (String, Vec<(String, String)>) {
        let (root, segments) = file_picker_breadcrumb_model(path);
        (
            root,
            segments
                .into_iter()
                .map(|segment| (segment.label, segment.target))
                .collect(),
        )
    }

    #[test]
    fn posix_path_splits_on_forward_slash() {
        let (root, segments) = crumbs("/Users/dux/project");
        assert_eq!(root, "/");
        assert_eq!(
            segments,
            vec![
                ("Users".into(), "/Users".into()),
                ("dux".into(), "/Users/dux".into()),
                ("project".into(), "/Users/dux/project".into()),
            ]
        );
    }

    #[test]
    fn windows_drive_path_splits_into_clickable_crumbs() {
        let (root, segments) = crumbs(r"C:\Users\dux");
        assert_eq!(root, codux_runtime::path::FILE_LIST_DRIVES_SENTINEL);
        assert_eq!(
            segments,
            vec![
                ("C:".into(), r"C:\".into()),
                ("Users".into(), r"C:\Users".into()),
                ("dux".into(), r"C:\Users\dux".into()),
            ]
        );
    }

    #[test]
    fn windows_forward_slash_path_also_splits() {
        let (_root, segments) = crumbs("C:/Users/dux");
        let targets: Vec<String> = segments.into_iter().map(|(_, target)| target).collect();
        assert_eq!(targets, vec![r"C:\", r"C:\Users", r"C:\Users\dux"]);
    }

    #[test]
    fn windows_drive_root_shows_only_drive_crumb() {
        let (_root, segments) = crumbs(r"C:\");
        assert_eq!(segments, vec![("C:".into(), r"C:\".into())]);
    }

    #[test]
    fn drive_list_view_has_no_segments() {
        let (root, segments) = crumbs(codux_runtime::path::FILE_LIST_DRIVES_SENTINEL);
        assert_eq!(root, codux_runtime::path::FILE_LIST_DRIVES_SENTINEL);
        assert!(segments.is_empty());
    }
}
