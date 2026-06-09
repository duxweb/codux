use super::*;
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};

pub(in crate::app) fn workspace_i18n(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
}

pub(in crate::app) fn workspace_header_button(
    id: &'static str,
    cx: &mut Context<CoduxApp>,
) -> Button {
    Button::new(id)
        .compact()
        .h(px(28.0))
        .text_color(cx.theme().foreground)
}

pub(in crate::app) fn workspace_header_badge_button_content(
    icon: HeroIconName,
    icon_bg: gpui::Hsla,
    label: impl Into<SharedString>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .h(px(20.0))
        .flex()
        .items_center()
        .gap_2()
        .text_color(cx.theme().foreground)
        .child(
            div()
                .size(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(icon_bg)
                .text_color(color(0xFFFFFF))
                .child(Icon::new(icon).size_2()),
        )
        .child(
            div()
                .text_size(rems(0.75))
                .text_color(cx.theme().foreground)
                .child(label.into()),
        )
}
