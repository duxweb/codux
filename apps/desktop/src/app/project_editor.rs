use super::*;
use gpui_component::input::{Input, InputEvent, InputState};

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
                    .child(self.project_editor_device_field(window, cx))
                    .child(project_editor_path_field(
                        tr("project.editor.directory", "Project Directory"),
                        tr("project.editor.choose_directory.prompt", "Choose"),
                        &self.project_editor_path,
                        window,
                        cx,
                    ))
                    .when(self.project_editor_browse_open, |element| {
                        element.child(self.project_editor_browse_panel(window, cx))
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

    /// The "Device" selector: This Mac (local) + each paired remote host, plus
    /// an inline "pair a new device" form. Selecting a remote device marks the
    /// project as hosted there (its domains route over the controller).
    fn project_editor_device_field(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = locale_from_language_setting(self.state.settings.language.as_str());
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        let hosts = self.runtime_service.saved_remote_hosts();
        let selected = self.project_editor_host_device_id.clone();

        let mut row = div().flex().flex_wrap().items_center().gap_2().child(
            project_editor_device_chip(
                "project-editor-device-local".to_string(),
                tr("project.editor.device.local", "This Mac"),
                selected.is_none(),
                cx,
                |app, window, cx| app.set_project_editor_host_device_id(None, window, cx),
            ),
        );
        for host in &hosts {
            let device_id = host.device_id.clone();
            let is_selected = selected.as_deref() == Some(device_id.as_str());
            let label = if host.host_name.trim().is_empty() {
                host.host_id.clone()
            } else {
                host.host_name.clone()
            };
            row = row.child(project_editor_device_chip(
                format!("project-editor-device-{device_id}"),
                label,
                is_selected,
                cx,
                move |app, window, cx| {
                    app.set_project_editor_host_device_id(Some(device_id.clone()), window, cx)
                },
            ));
        }
        row = row.child(
            Button::new("project-editor-device-pair")
                .secondary()
                .compact()
                .text_color(cx.theme().secondary_foreground)
                .child(dialog_button_label(tr(
                    "project.editor.device.pair",
                    "Pair device…",
                )))
                .on_click(cx.listener(|app, _event, window, cx| {
                    app.toggle_project_editor_pairing(window, cx);
                })),
        );

        let mut field = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .mb(px(24.0))
            .child(
                div()
                    .text_size(rems(0.875))
                    .line_height(rems(1.125))
                    .text_color(color(theme::TEXT))
                    .child(tr("project.editor.device", "Device")),
            )
            .child(row);
        if self.project_editor_pairing_open {
            field = field.child(self.project_editor_pairing_panel(window, cx));
        }
        field.into_any_element()
    }

    fn project_editor_pairing_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = locale_from_language_setting(self.state.settings.language.as_str());
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        let mut panel = div()
            .mt(px(10.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(color(theme::BORDER_SOFT))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(rems(0.8125))
                    .text_color(color(theme::TEXT_MUTED))
                    .child(tr(
                        "project.editor.device.pair.hint",
                        "Paste the pairing ticket the host shows (codux://pair…).",
                    )),
            )
            .child(project_editor_input(
                "project-editor-pairing-ticket",
                &self.project_editor_pairing_ticket,
                "codux://pair?payload=…",
                window,
                cx,
                |app, value, window, cx| app.set_project_editor_pairing_ticket(value, window, cx),
            ))
            .child(project_editor_input(
                "project-editor-pairing-name",
                &self.project_editor_pairing_name,
                "This device name",
                window,
                cx,
                |app, value, window, cx| app.set_project_editor_pairing_name(value, window, cx),
            ));
        if let Some(error) = self.project_editor_pairing_error.as_ref() {
            panel = panel.child(
                div()
                    .text_size(rems(0.8125))
                    .text_color(color(theme::ORANGE))
                    .child(error.clone()),
            );
        }
        panel
            .child(
                div().flex().child(
                    dialog_primary_button(
                        "project-editor-pairing-submit",
                        tr("project.editor.device.pair.submit", "Pair"),
                        cx,
                        |app, _event, window, cx| app.submit_project_editor_pairing(window, cx),
                    )
                    .disabled(self.project_editor_pairing_busy)
                    .loading(self.project_editor_pairing_busy),
                ),
            )
            .into_any_element()
    }
}

impl CoduxApp {
    /// Inline remote directory browser: navigate the host's folders, create a
    /// folder, and pick one as the project root.
    fn project_editor_browse_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = locale_from_language_setting(self.state.settings.language.as_str());
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        let current = self.project_editor_browse_path.clone();

        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .max_h(px(200.0))
            .overflow_y_scrollbar();
        if let Some(parent) = self.project_editor_browse_parent.clone() {
            list = list.child(project_editor_browse_row(
                "project-editor-browse-up".to_string(),
                "..".to_string(),
                cx,
                move |app, window, cx| {
                    app.project_editor_browse_navigate(Some(parent.clone()), window, cx)
                },
            ));
        }
        for entry in &self.project_editor_browse_entries {
            let path = entry.path.clone();
            list = list.child(project_editor_browse_row(
                format!("project-editor-browse-{path}"),
                entry.name.clone(),
                cx,
                move |app, window, cx| {
                    app.project_editor_browse_navigate(Some(path.clone()), window, cx)
                },
            ));
        }

        let mut panel = div()
            .mt(px(8.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(color(theme::BORDER_SOFT))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(rems(0.8125))
                    .text_color(color(theme::TEXT_MUTED))
                    .truncate()
                    .child(if current.trim().is_empty() {
                        tr("project.editor.browse.loading", "Loading…")
                    } else {
                        current.clone()
                    }),
            )
            .child(list)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(project_editor_input(
                        "project-editor-browse-newfolder",
                        &self.project_editor_browse_new_folder,
                        "New folder name",
                        window,
                        cx,
                        |app, value, window, cx| {
                            app.set_project_editor_browse_new_folder(value, window, cx)
                        },
                    )))
                    .child(
                        Button::new("project-editor-browse-create")
                            .secondary()
                            .compact()
                            .text_color(cx.theme().secondary_foreground)
                            .child(dialog_button_label(tr(
                                "project.editor.browse.new_folder",
                                "New folder",
                            )))
                            .disabled(self.project_editor_browse_busy)
                            .on_click(cx.listener(|app, _event, window, cx| {
                                app.project_editor_browse_create_folder(window, cx);
                            })),
                    ),
            );
        if let Some(error) = self.project_editor_browse_error.as_ref() {
            panel = panel.child(
                div()
                    .text_size(rems(0.8125))
                    .text_color(color(theme::ORANGE))
                    .child(error.clone()),
            );
        }
        panel
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        dialog_primary_button(
                            "project-editor-browse-use",
                            tr("project.editor.browse.use", "Use this folder"),
                            cx,
                            |app, _event, window, cx| app.project_editor_browse_select(window, cx),
                        )
                        .disabled(current.trim().is_empty() || self.project_editor_browse_busy),
                    )
                    .child(dialog_cancel_button(
                        "project-editor-browse-cancel",
                        tr("common.cancel", "Cancel"),
                        cx,
                        |app, _event, window, cx| app.close_project_editor_browse(window, cx),
                    )),
            )
            .into_any_element()
    }
}

fn project_editor_browse_row(
    id: String,
    label: String,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .gap_2()
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_click(cx.listener(move |app, _event, window, cx| on_click(app, window, cx)))
        .child(
            Icon::new(HeroIconName::Folder)
                .size_3()
                .text_color(color(theme::TEXT_MUTED)),
        )
        .child(
            div()
                .text_size(rems(0.8125))
                .text_color(color(theme::TEXT))
                .truncate()
                .child(label),
        )
        .into_any_element()
}

fn project_editor_device_chip(
    id: String,
    label: String,
    selected: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    div()
        .id(SharedString::from(id))
        .px(px(12.0))
        .py(px(6.0))
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
        .text_size(rems(0.8125))
        .text_color(color(theme::TEXT))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_click(cx.listener(move |app, _event, window, cx| on_click(app, window, cx)))
        .child(label)
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
                        .child(match symbol.icon.clone() {
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
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
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
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_1().min_w_0().child(project_editor_input(
                    "project-editor-path",
                    path,
                    "/path/to/project",
                    window,
                    cx,
                    |app, value, window, cx| app.set_project_editor_path(value, window, cx),
                )))
                .child(
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
