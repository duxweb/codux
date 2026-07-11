use super::*;
use gpui::{Anchor, Rems};
use gpui_component::{
    Disableable, Sizable,
    button::Button,
    menu::{DropdownMenu, PopupMenuItem},
};

const CODUX_SELECT_TEXT_SIZE: Rems = Rems(0.875);
const CODUX_SELECT_LINE_HEIGHT: Rems = Rems(1.125);

#[derive(Clone)]
pub(in crate::app) struct CoduxSelectOption {
    pub(in crate::app) value: String,
    pub(in crate::app) label: SharedString,
}

impl CoduxSelectOption {
    pub(in crate::app) fn new(value: impl Into<String>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

pub(in crate::app) struct CoduxSelectConfig {
    pub(in crate::app) id: String,
    pub(in crate::app) value: String,
    pub(in crate::app) options: Vec<CoduxSelectOption>,
    pub(in crate::app) placeholder: SharedString,
    pub(in crate::app) width: Length,
    pub(in crate::app) menu_width: Pixels,
    pub(in crate::app) disabled: bool,
}

type CoduxSelectAction = Rc<dyn Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>)>;

pub(in crate::app) fn codux_select(
    config: CoduxSelectConfig,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let CoduxSelectConfig {
        id,
        value,
        options,
        placeholder,
        width,
        menu_width,
        disabled,
    } = config;
    let selected_index = options.iter().position(|item| item.value == value);
    let selected_label = selected_index
        .and_then(|index| options.get(index))
        .map(|item| item.label.clone())
        .unwrap_or(placeholder);
    let action: CoduxSelectAction = Rc::new(action);
    let selected_value = value;
    let app_entity = cx.entity();

    Button::new(SharedString::from(format!("codux-select-trigger-{id}")))
        .outline()
        .with_size(gpui_component::Size::Medium)
        .disabled(disabled)
        .w(width)
        .min_w(px(180.0))
        .child(
            div()
                .flex()
                .w_full()
                .min_w_0()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_size(CODUX_SELECT_TEXT_SIZE)
                        .line_height(CODUX_SELECT_LINE_HEIGHT)
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
                        .flex_shrink_0()
                        .text_color(if disabled {
                            cx.theme().foreground.opacity(0.3)
                        } else {
                            cx.theme().foreground.opacity(0.5)
                        }),
                ),
        )
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
            options.iter().fold(
                menu.min_w(menu_width).max_w(menu_width).scrollable(true),
                |menu, item| {
                    let value = item.value.clone();
                    let selected = value == selected_value;
                    let action = action.clone();
                    let app_entity = app_entity.clone();
                    menu.item(
                        PopupMenuItem::new(item.label.clone())
                            .checked(selected)
                            .on_click(move |_, window, cx| {
                                cx.update_entity(&app_entity, |app, cx| {
                                    action(app, value.clone(), window, cx);
                                    cx.notify();
                                });
                            }),
                    )
                },
            )
        })
        .into_any_element()
}
