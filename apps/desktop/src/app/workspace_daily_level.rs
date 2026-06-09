use super::*;
use crate::app::workspace_shared::{workspace_header_button, workspace_i18n};
use gpui::Anchor;
use gpui_component::popover::Popover;

pub(in crate::app) fn workspace_level_button(
    daily_level: &codux_runtime::ai_history::AIHistoryDailyLevelView,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let language = language.to_string();
    let current_tier = daily_level.current_tier.clone();
    let button_label = daily_level_title(&current_tier, &language);
    let daily_level = daily_level.clone();

    Popover::new("workspace-level-popover")
        .anchor(Anchor::TopRight)
        .w(px(304.0))
        .trigger(
            workspace_header_button("workspace-level", cx)
                .secondary()
                .text_color(cx.theme().foreground)
                .child(workspace_daily_level_button_content(
                    current_tier.clone(),
                    button_label,
                    cx,
                )),
        )
        .content(move |_, _, cx| {
            let theme = cx.theme();
            workspace_level_popover_content(
                daily_level.clone(),
                language.clone(),
                theme.secondary_hover,
                theme.transparent,
            )
        })
}

fn daily_level_title(
    tier: &codux_runtime::ai_history::AIHistoryDailyLevelTierView,
    language: &str,
) -> String {
    workspace_i18n(language, &format!("rank.{}", tier.id), &tier.title)
}

fn workspace_daily_level_button_content(
    tier: codux_runtime::ai_history::AIHistoryDailyLevelTierView,
    label: String,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .h(px(20.0))
        .flex()
        .items_center()
        .gap_1()
        .text_color(cx.theme().foreground)
        .child(daily_level_badge(&tier, 18.0, 8.0))
        .child(
            div()
                .text_size(rems(0.75))
                .line_height(rems(0.75))
                .child(label),
        )
}

fn workspace_level_popover_content(
    daily_level: codux_runtime::ai_history::AIHistoryDailyLevelView,
    language: String,
    hover_surface: gpui::Hsla,
    transparent: gpui::Hsla,
) -> impl IntoElement {
    let tokens = daily_level.tokens.max(0);
    let current_tier = daily_level.current_tier.clone();
    let current_title = daily_level_title(&current_tier, &language);
    let today_level_label = workspace_i18n(&language, "ai.today_level", "Today's Level");
    let today_tokens_label = workspace_i18n(&language, "ai.today_tokens", "Today's Tokens");
    let current_label = workspace_i18n(&language, "common.current", "Current");
    let need_template = workspace_i18n(&language, "common.need_format", "Need %@");

    div()
        .flex()
        .flex_col()
        .text_color(color(theme::TEXT))
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(daily_level_badge(&current_tier, 34.0, 14.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(color(theme::TEXT_MUTED))
                                .child(today_level_label),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(rems(0.9375))
                                .line_height(rems(1.125))
                                .font_weight(FontWeight::BOLD)
                                .child(current_title),
                        ),
                )
                .child(
                    div()
                        .text_right()
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(color(theme::TEXT_MUTED))
                                .child(today_tokens_label),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(rems(0.9375))
                                .line_height(rems(1.125))
                                .font_weight(FontWeight::BOLD)
                                .child(compact_number(tokens)),
                        ),
                ),
        )
        .child(div().mt(px(12.0)).flex().flex_col().gap_1().children(
            daily_level.tiers.into_iter().map(|tier| {
                let current = tier.id == current_tier.id;
                let title = daily_level_title(&tier, &language);
                let need = need_template.replace("%@", &compact_number(tier.min));
                div()
                    .rounded(px(8.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(if current { hover_surface } else { transparent })
                    .border_1()
                    .border_color(if current {
                        color(tier.color).opacity(0.28)
                    } else {
                        transparent
                    })
                    .child(daily_level_badge(&tier, 24.0, 10.0))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(rems(0.8125))
                                    .line_height(rems(1.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_size(rems(0.75))
                                    .line_height(rems(1.0))
                                    .text_color(color(theme::TEXT_MUTED))
                                    .child(need),
                            ),
                    )
                    .when(current, |this| {
                        this.child(
                            div()
                                .rounded_full()
                                .px(px(8.0))
                                .py(px(4.0))
                                .text_size(rems(0.75))
                                .line_height(rems(0.875))
                                .font_weight(FontWeight::BOLD)
                                .bg(color(tier.color).opacity(0.14))
                                .text_color(color(tier.color))
                                .child(current_label.clone()),
                        )
                    })
                    .into_any_element()
            }),
        ))
}

fn daily_level_badge(
    tier: &codux_runtime::ai_history::AIHistoryDailyLevelTierView,
    box_size: f32,
    icon_size: f32,
) -> impl IntoElement {
    div()
        .size(px(box_size))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(linear_gradient(
            135.0,
            linear_color_stop(color(tier.color), 0.0),
            linear_color_stop(color(tier.color).opacity(0.72), 1.0),
        ))
        .text_color(color(0xFFFFFF))
        .child(daily_level_icon(&tier.icon, icon_size))
}

fn daily_level_icon(icon: &str, icon_size: f32) -> impl IntoElement {
    let icon = match icon {
        "minus" => Icon::new(HeroIconName::Minus),
        "star" => Icon::new(HeroIconName::Star),
        "zap" => Icon::empty().path("rank-icons/zap.svg"),
        "shield-check" => Icon::empty().path("rank-icons/shield-check.svg"),
        "sparkles" => Icon::empty().path("rank-icons/sparkles.svg"),
        "trophy" => Icon::empty().path("rank-icons/trophy.svg"),
        "flame" => Icon::empty().path("rank-icons/flame.svg"),
        _ => Icon::new(HeroIconName::Minus),
    };
    icon.with_size(px(icon_size)).text_color(color(0xFFFFFF))
}
