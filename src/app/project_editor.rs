use super::*;
use gpui_component::input::{Input, InputEvent, InputState};

impl CoduxApp {
    pub(in crate::app) fn project_editor_workspace(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_editing = self.project_editor_project_id.is_some();
        let title = if is_editing {
            "编辑项目"
        } else {
            "新建项目"
        };
        let submit_label = if is_editing { "保存" } else { "创建" };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(color(theme::BG))
            .child(column_header(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(color(theme::TEXT))
                            .child(title),
                    )
                    .child(header_icon_button(
                        "project-editor-close",
                        IconName::Close,
                        cx,
                        |_app, _event, window, _cx| window.remove_window(),
                    )),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p(px(18.0))
                    .child(project_editor_field(
                        "项目名称",
                        "project-editor-name",
                        &self.project_editor_name,
                        "Project",
                        window,
                        cx,
                        |app, value, window, cx| app.set_project_editor_name(value, window, cx),
                    ))
                    .child(project_editor_path_field(
                        &self.project_editor_path,
                        window,
                        cx,
                    ))
                    .child(project_editor_symbol_field(
                        self.project_editor_badge_symbol.as_deref(),
                        &self.project_editor_badge_color_hex,
                        cx,
                    ))
                    .child(project_editor_color_field(
                        &self.project_editor_badge_color_hex,
                        cx,
                    ))
                    .child(
                        div()
                            .mt(px(4.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(color(theme::TEXT_DIM))
                                    .truncate()
                                    .child(self.status_message.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Button::new("project-editor-cancel")
                                            .ghost()
                                            .text_color(cx.theme().secondary_foreground)
                                            .label("取消")
                                            .on_click(cx.listener(|_app, _event, window, _cx| {
                                                window.remove_window();
                                            })),
                                    )
                                    .child(
                                        Button::new("project-editor-save")
                                            .secondary()
                                            .text_color(cx.theme().secondary_foreground)
                                            .label(submit_label)
                                            .on_click(cx.listener(|app, _event, window, cx| {
                                                app.save_project_editor(window, cx);
                                            })),
                                    ),
                            ),
                    ),
            )
    }
}

struct ProjectEditorSymbol {
    id: &'static str,
    icon: Option<IconName>,
}

const PROJECT_EDITOR_SYMBOLS: &[ProjectEditorSymbol] = &[
    ProjectEditorSymbol {
        id: "none",
        icon: None,
    },
    ProjectEditorSymbol {
        id: "terminal",
        icon: Some(IconName::SquareTerminal),
    },
    ProjectEditorSymbol {
        id: "folder",
        icon: Some(IconName::Folder),
    },
    ProjectEditorSymbol {
        id: "shippingbox",
        icon: Some(IconName::Bot),
    },
    ProjectEditorSymbol {
        id: "hammer",
        icon: Some(IconName::Settings2),
    },
    ProjectEditorSymbol {
        id: "server.rack",
        icon: Some(IconName::Globe),
    },
    ProjectEditorSymbol {
        id: "globe",
        icon: Some(IconName::Globe),
    },
    ProjectEditorSymbol {
        id: "bolt",
        icon: Some(IconName::Star),
    },
    ProjectEditorSymbol {
        id: "wrench",
        icon: Some(IconName::Settings),
    },
    ProjectEditorSymbol {
        id: "doc.text",
        icon: Some(IconName::File),
    },
    ProjectEditorSymbol {
        id: "book",
        icon: Some(IconName::BookOpen),
    },
    ProjectEditorSymbol {
        id: "person.2",
        icon: Some(IconName::CircleUser),
    },
];

fn project_editor_field(
    label: &'static str,
    id: &'static str,
    value: &str,
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
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
    selected_symbol: Option<&str>,
    selected_color: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let accent = hex_color(selected_color).unwrap_or(theme::ACCENT);
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT))
                .child("项目图标"),
        )
        .child(
            div()
                .grid()
                .grid_cols(6)
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
                        .h(px(34.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(color(if selected {
                            theme::BORDER
                        } else {
                            theme::BORDER_SOFT
                        }))
                        .bg(color(0xFFFFFF).opacity(if selected { 0.10 } else { 0.04 }))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|style| style.bg(color(0xFFFFFF).opacity(0.08)))
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
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color(theme::TEXT_MUTED))
                                .child("无")
                                .into_any_element(),
                        })
                        .into_any_element()
                })),
        )
        .into_any_element()
}

fn project_editor_color_field(selected_color: &str, cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT))
                .child("项目颜色"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .children(PROJECT_BADGE_COLORS.iter().map(|value| {
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
                })),
        )
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
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let value = value.to_string();
    let input_state = window.use_keyed_state(SharedString::from(id), cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .placeholder(placeholder)
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
    path: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT))
                .child("项目目录"),
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
                        .text_color(cx.theme().secondary_foreground)
                        .label("选择")
                        .on_click(cx.listener(|app, _event, window, cx| {
                            app.choose_project_editor_directory(window, cx);
                        })),
                ),
        )
        .into_any_element()
}
