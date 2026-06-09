use super::*;

#[derive(Clone)]
pub(in crate::app) struct CoduxSelectOption {
    pub(in crate::app) value: String,
    pub(in crate::app) label: SharedString,
}

struct CoduxSelectState {
    open: bool,
}

impl CoduxSelectOption {
    pub(in crate::app) fn new(value: impl Into<String>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

pub(in crate::app) fn codux_select(
    id: impl Into<String>,
    value: &str,
    options: Vec<CoduxSelectOption>,
    placeholder: impl Into<SharedString>,
    width: impl Into<Length> + Clone,
    menu_width: Pixels,
    disabled: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let id = id.into();
    let state = window.use_keyed_state(
        SharedString::from(format!("codux-select-state-{id}")),
        cx,
        |_, _| CoduxSelectState { open: false },
    );
    let selected_index = options.iter().position(|item| item.value == value);
    let selected_label = selected_index
        .and_then(|index| options.get(index))
        .map(|item| item.label.clone())
        .unwrap_or_else(|| placeholder.into());
    let is_open = state.read(cx).open && !disabled;
    let action: Rc<dyn Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>)> =
        Rc::new(action);
    let trigger_id = id.clone();
    let menu_id = id.clone();
    let selected_value = value.to_string();
    let select_surface = color(theme::BG_PANEL);
    let viewport_size = window.viewport_size();

    div()
        .relative()
        .w(width)
        .min_w(px(180.0))
        .child(
            div()
                .id(SharedString::from(format!(
                    "codux-select-trigger-{trigger_id}"
                )))
                .h(px(32.0))
                .w_full()
                .px(px(10.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(if is_open {
                    color(theme::ACCENT)
                } else {
                    cx.theme().input
                })
                .bg(select_surface)
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .text_size(rems(0.8125))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .when(disabled, |this| this.opacity(0.5))
                .when(!disabled, |this| {
                    let state = state.clone();
                    this.cursor_pointer()
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_click(move |_, _window, cx| {
                            state.update(cx, |state, cx| {
                                state.open = !state.open;
                                cx.notify();
                            });
                        })
                })
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_color(if selected_index.is_some() {
                            color(theme::TEXT)
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(selected_label),
                )
                .child(
                    Icon::new(HeroIconName::ChevronDown)
                        .size_3()
                        .text_color(cx.theme().muted_foreground),
                ),
        )
        .when(is_open, |this| {
            let action = action.clone();
            let close_state = state.clone();
            this.child(
                deferred(
                    anchored().position(point(px(0.0), px(0.0))).child(
                        div()
                            .block_mouse_except_scroll()
                            .w(viewport_size.width)
                            .h(viewport_size.height)
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                window.prevent_default();
                                close_state.update(cx, |state, cx| {
                                    state.open = false;
                                    cx.notify();
                                });
                                cx.stop_propagation();
                            }),
                    ),
                )
                .with_priority(0),
            )
            .child(
                deferred(
                    anchored().snap_to_window_with_margin(px(8.0)).child(
                        div()
                            .occlude()
                            .w(menu_width)
                            .mt(px(6.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(select_surface)
                            .shadow_md()
                            .overflow_hidden()
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .child(
                                div()
                                    .h(px((options.len() as f32 * 30.0 + 8.0).min(260.0)))
                                    .p(px(4.0))
                                    .overflow_y_scrollbar()
                                    .children(options.into_iter().map(move |item| {
                                        let option_id = SharedString::from(format!(
                                            "codux-select-{menu_id}-{}",
                                            item.value
                                        ));
                                        let value = item.value.clone();
                                        let selected = item.value == selected_value;
                                        let action = action.clone();
                                        let state = state.clone();

                                        div()
                                            .id(option_id)
                                            .h(px(30.0))
                                            .w_full()
                                            .px(px(8.0))
                                            .rounded(px(6.0))
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap(px(8.0))
                                            .cursor_pointer()
                                            .text_size(rems(0.8125))
                                            .line_height(rems(1.125))
                                            .text_color(color(theme::TEXT))
                                            .hover(|style| style.bg(cx.theme().list_hover))
                                            .when(selected, |this| {
                                                this.bg(color(theme::ACCENT).opacity(0.12))
                                            })
                                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                                window.prevent_default();
                                                cx.stop_propagation();
                                            })
                                            .on_click(cx.listener(move |app, _, window, cx| {
                                                state.update(cx, |state, cx| {
                                                    state.open = false;
                                                    cx.notify();
                                                });
                                                action(app, value.clone(), window, cx);
                                                cx.notify();
                                            }))
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .truncate()
                                                    .child(item.label.clone()),
                                            )
                                            .child(
                                                div()
                                                    .w(px(14.0))
                                                    .h(px(14.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .when(!selected, |this| this.invisible())
                                                    .text_color(color(theme::ACCENT))
                                                    .child(Icon::new(HeroIconName::Check).size_3()),
                                            )
                                            .into_any_element()
                                    })),
                            ),
                    ),
                )
                .with_priority(1),
            )
        })
        .into_any_element()
}
