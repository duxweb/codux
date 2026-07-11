use super::*;

pub(super) const SETTINGS_FORM_TEXT_SIZE: Rems = Rems(0.875);
pub(super) const SETTINGS_FORM_LINE_HEIGHT: Rems = Rems(1.125);
pub(super) const SETTINGS_FORM_DESCRIPTION_TEXT_SIZE: Rems = Rems(0.75);
pub(super) const SETTINGS_FORM_DESCRIPTION_LINE_HEIGHT: Rems = Rems(1.0625);
pub(super) const SETTINGS_ROW_LABEL_MIN_WIDTH: f32 = 180.0;
pub(super) const SETTINGS_ROW_CONTROL_MIN_WIDTH: f32 = 160.0;
pub(super) fn settings_form(children: Vec<AnyElement>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(22.0))
        .children(children)
}

pub(super) fn settings_card(
    title: Option<String>,
    description: Option<String>,
    children: Vec<AnyElement>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    settings_card_with_actions(title, description, None, children, cx)
}

pub(super) fn settings_card_with_actions(
    title: Option<String>,
    description: Option<String>,
    actions: Option<AnyElement>,
    children: Vec<AnyElement>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title_element = if title.is_some() || description.is_some() || actions.is_some() {
        Some(
            div()
                .min_h(px(28.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(SETTINGS_FORM_TEXT_SIZE)
                                .line_height(SETTINGS_FORM_LINE_HEIGHT)
                                .text_color(color(theme::TEXT))
                                .child(title.clone().unwrap_or_default()),
                        )
                        .when_some(description, |this, description| {
                            this.child(
                                div()
                                    .mt(px(3.0))
                                    .max_w(px(520.0))
                                    .text_size(SETTINGS_FORM_DESCRIPTION_TEXT_SIZE)
                                    .line_height(SETTINGS_FORM_DESCRIPTION_LINE_HEIGHT)
                                    .text_color(color(theme::TEXT_DIM))
                                    .child(description),
                            )
                        }),
                )
                .child(actions.unwrap_or_else(|| div().hidden().into_any_element())),
        )
    } else {
        None
    };

    div().w_full().child(
        GroupBox::new()
            .w_full()
            .fill()
            .when_some(title_element, |this, title| this.title(title))
            .content_style(
                div()
                    .w_full()
                    .px(px(22.0))
                    .py(px(10.0))
                    .gap(px(0.0))
                    .style()
                    .clone(),
            )
            .children(children.into_iter().enumerate().flat_map(|(index, child)| {
                let mut elements = Vec::with_capacity(if index == 0 { 1 } else { 2 });
                if index > 0 {
                    elements.push(settings_form_separator(cx));
                }
                elements.push(div().w_full().child(child).into_any_element());
                elements
            })),
    )
}

pub(super) fn settings_form_separator(cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .flex_none()
        .bg(settings_form_divider(cx))
        .into_any_element()
}

pub(super) fn settings_form_divider(cx: &mut Context<CoduxApp>) -> gpui::Hsla {
    theme::divider_for_surface(cx.theme().background)
}

pub(super) fn settings_row(
    label: impl Into<String>,
    description: Option<String>,
    control: AnyElement,
) -> impl IntoElement {
    let label = label.into();
    div()
        .min_h(px(58.0))
        .py(px(10.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.0))
        .child(
            div()
                .min_w(px(SETTINGS_ROW_LABEL_MIN_WIDTH))
                .flex_1()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(SETTINGS_FORM_TEXT_SIZE)
                        .line_height(SETTINGS_FORM_LINE_HEIGHT)
                        .text_color(color(theme::TEXT))
                        .child(label),
                )
                .child(
                    div()
                        .when(description.is_none(), |this| this.hidden())
                        .mt(px(3.0))
                        .max_w(px(420.0))
                        .text_size(SETTINGS_FORM_DESCRIPTION_TEXT_SIZE)
                        .line_height(SETTINGS_FORM_DESCRIPTION_LINE_HEIGHT)
                        .text_color(color(theme::TEXT_DIM))
                        .child(description.unwrap_or_default()),
                ),
        )
        .child(
            div()
                .w(relative(0.3))
                .min_w(px(SETTINGS_ROW_CONTROL_MIN_WIDTH))
                .max_w(relative(0.3))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_end()
                .child(control),
        )
}

pub(super) fn settings_small_button(
    id: impl Into<String>,
    value: impl Into<String>,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    settings_small_button_state(id, value, false, false, cx, action)
}

pub(super) fn settings_small_button_state(
    id: impl Into<String>,
    value: impl Into<String>,
    loading: bool,
    disabled: bool,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    Button::new(SharedString::from(id.into()))
        .secondary()
        .loading(loading)
        .disabled(disabled)
        .text_color(color(theme::TEXT))
        .on_click(cx.listener(action))
        .child(
            div()
                .text_size(SETTINGS_FORM_TEXT_SIZE)
                .line_height(SETTINGS_FORM_LINE_HEIGHT)
                .text_color(color(theme::TEXT))
                .child(value.into()),
        )
        .into_any_element()
}

pub(super) fn settings_icon_button_state(
    id: impl Into<SharedString>,
    icon: impl Into<Icon>,
    disabled: bool,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let icon = icon.into();
    Button::new(id.into())
        .compact()
        .ghost()
        .disabled(disabled)
        .text_color(cx.theme().secondary_foreground)
        .bg(cx.theme().transparent)
        .icon(icon.size_3p5().text_color(cx.theme().secondary_foreground))
        .on_click(cx.listener(action))
        .into_any_element()
}

pub(super) fn settings_text_input(
    id: impl Into<SharedString>,
    value: impl Into<String>,
    placeholder: impl Into<String>,
    masked: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    settings_text_input_sized(id, value, placeholder, masked, window, cx, action)
}

pub(super) fn settings_text_input_sized(
    id: impl Into<SharedString>,
    value: impl Into<String>,
    placeholder: impl Into<String>,
    masked: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let value = value.into();
    let placeholder = placeholder.into();
    let key = SharedString::from(format!("settings-input-{}", id.into()));
    let state = window.use_keyed_state(key, cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .placeholder(placeholder.clone())
            .masked(masked)
    });
    state.update(cx, |state, cx| {
        if state.value().as_ref() != value.as_str() {
            state.set_value(value.clone(), window, cx);
        }
    });
    cx.subscribe_in(&state, window, move |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            action(app, state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .w_full()
        .min_w_0()
        .child(
            Input::new(&state)
                .with_size(gpui_component::Size::Medium)
                .w_full(),
        )
        .into_any_element()
}
pub(super) fn settings_textarea(
    id: &'static str,
    value: &str,
    rows: usize,
    placeholder: impl Into<String>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let value = value.to_string();
    let placeholder = placeholder.into();
    let state = window.use_keyed_state(
        SharedString::from(format!("settings-textarea-{id}")),
        cx,
        |window, cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(rows)
                .default_value(value.clone())
                .placeholder(placeholder.clone())
        },
    );
    state.update(cx, |state, cx| {
        if state.value().as_ref() != value.as_str() {
            state.set_value(value.clone(), window, cx);
        }
    });
    cx.subscribe_in(&state, window, move |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            action(app, state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .w_full()
        .min_w_0()
        .child(
            Input::new(&state)
                .with_size(gpui_component::Size::Medium)
                .h(px((rows as f32 * 28.0).max(84.0))),
        )
        .into_any_element()
}

pub(super) fn settings_toggle(
    id: impl Into<String>,
    checked: bool,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    settings_toggle_state(id, checked, false, cx, action)
}

pub(super) fn settings_toggle_state(
    id: impl Into<String>,
    checked: bool,
    disabled: bool,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let app_entity = cx.entity();
    Switch::new(SharedString::from(id.into()))
        .checked(checked)
        .disabled(disabled)
        .with_size(gpui_component::Size::Medium)
        .on_click(move |_, window, cx| {
            cx.update_entity(&app_entity, |app, cx| {
                action(app, window, cx);
            });
        })
        .into_any_element()
}

pub(super) fn settings_select_impl(
    id: impl Into<String>,
    value: &str,
    options: Vec<(String, SharedString)>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    language: &str,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    settings_select_state(id, value, options, (false, language), window, cx, action)
}

pub(super) fn settings_select_state(
    id: impl Into<String>,
    value: &str,
    options: Vec<(String, SharedString)>,
    state: (bool, &str),
    _window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let (disabled, language) = state;
    let select_id = format!("settings-select-{}", id.into());
    let options = options
        .into_iter()
        .map(|(value, label)| CoduxSelectOption::new(value, label))
        .collect();
    codux_select(
        CoduxSelectConfig {
            id: select_id,
            value: value.to_string(),
            options,
            placeholder: settings_text(language, "common.choose", "Choose").into(),
            width: relative(1.0).into(),
            menu_width: px(220.0),
            disabled,
        },
        cx,
        action,
    )
}

/// A friendly device-type label from an OS id (`std::env::consts::OS` or a
/// client-reported platform). Falls back to a generic label when unknown.
pub(super) fn device_type_label(platform: &str, language: &str) -> String {
    match platform.trim().to_ascii_lowercase().as_str() {
        "macos" | "darwin" | "mac" => "macOS".to_string(),
        "ios" | "ipados" => "iOS".to_string(),
        "android" => settings_text(language, "device.type.android", "Android"),
        "linux" => "Linux".to_string(),
        "windows" => "Windows".to_string(),
        "" => settings_text(language, "device.type.unknown", "Remote device"),
        other => other.to_string(),
    }
}

/// Connection-status tag for a host this desktop controls, from its client link
/// state. Absent (never connected this session) reads as disconnected.
pub(super) fn host_link_status_tag(
    link: Option<codux_runtime::remote::ControllerLinkState>,
    path: Option<codux_runtime::remote::ControllerLinkPath>,
    language: &str,
) -> AnyElement {
    use codux_runtime::remote::{ControllerLinkPath, ControllerLinkState};
    match link {
        Some(ControllerLinkState::Connected) => {
            let connected = settings_text(language, "remote.status.connected_label", "Connected");
            // Append the route so a LAN/p2p direct link is distinguishable from a
            // relay-routed one (the path arrives a beat after "connected").
            let label = match path {
                Some(ControllerLinkPath::Direct) => format!(
                    "{connected} · {}",
                    settings_text(language, "remote.path.direct_label", "Direct")
                ),
                Some(ControllerLinkPath::Relay) => format!(
                    "{connected} · {}",
                    settings_text(language, "remote.path.relay_label", "Relay")
                ),
                None => connected,
            };
            settings_status_tag(label, theme::GREEN)
        }
        Some(ControllerLinkState::Connecting) => settings_status_tag(
            settings_text(language, "remote.status.connecting_label", "Connecting"),
            theme::ORANGE,
        ),
        _ => settings_status_tag(
            settings_text(language, "remote.status.disconnected_label", "Disconnected"),
            theme::TEXT_DIM,
        ),
    }
}

pub(super) fn settings_status_tag(value: impl Into<String>, accent: u32) -> AnyElement {
    div()
        .h(px(24.0))
        .px(px(9.0))
        .rounded(px(6.0))
        .bg(color(accent).opacity(0.14))
        .text_color(color(accent))
        .flex()
        .items_center()
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .child(value.into())
        .into_any_element()
}

pub(super) fn settings_checkmark(selected: bool) -> AnyElement {
    div()
        .when(!selected, |this| this.hidden())
        .absolute()
        .top(px(4.0))
        .right(px(4.0))
        .size(px(13.0))
        .rounded_full()
        .bg(color(theme::ACCENT))
        .flex()
        .items_center()
        .justify_center()
        .text_color(color(0xFFFFFF))
        .child(Icon::new(HeroIconName::Check).size_2())
        .into_any_element()
}

pub(super) fn settings_selectable_tile(
    id: impl Into<String>,
    label: impl Into<String>,
    preview: AnyElement,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    div()
        .id(SharedString::from(id.into()))
        .w_full()
        .min_w(px(112.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .text_color(color(theme::TEXT))
        .on_click(cx.listener(action))
        .child(preview)
        .child(
            div()
                .w_full()
                .text_align(gpui::TextAlign::Center)
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .truncate()
                .child(label.into()),
        )
        .into_any_element()
}

pub(super) fn settings_selectable_tile_cell(tile: AnyElement) -> AnyElement {
    div().min_w_0().flex_1().child(tile).into_any_element()
}

pub(super) fn settings_selectable_tile_rows(
    tiles: Vec<AnyElement>,
    columns: usize,
    gap: Pixels,
) -> AnyElement {
    let columns = columns.max(1);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    for tile in tiles {
        row.push(settings_selectable_tile_cell(tile));
        if row.len() == columns {
            rows.push(row);
            row = Vec::new();
        }
    }
    if !row.is_empty() {
        let filler_count = columns.saturating_sub(row.len());
        row.extend((0..filler_count).map(|_| div().min_w_0().flex_1().into_any_element()));
        rows.push(row);
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(gap)
        .children(rows.into_iter().map(move |row| {
            div()
                .w_full()
                .flex()
                .gap(gap)
                .children(row)
                .into_any_element()
        }))
        .into_any_element()
}
