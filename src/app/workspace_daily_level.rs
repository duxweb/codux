use super::*;
use crate::app::workspace_shared::{workspace_header_button, workspace_i18n};
use gpui::Anchor;
use gpui_component::popover::Popover;

pub(in crate::app) fn workspace_level_button(
    tokens: i64,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let tokens = tokens.max(0);
    let tier = daily_level_tier(tokens);
    let language = language.to_string();
    let button_label = daily_level_title(&tier, &language);

    Popover::new("workspace-level-popover")
        .anchor(Anchor::TopRight)
        .w(px(304.0))
        .trigger(
            workspace_header_button("workspace-level", cx)
                .secondary()
                .text_color(cx.theme().foreground)
                .child(workspace_daily_level_button_content(
                    tier.clone(),
                    button_label,
                    cx,
                )),
        )
        .content(move |_, _, cx| {
            let theme = cx.theme();
            workspace_level_popover_content(
                tokens,
                tier.clone(),
                language.clone(),
                theme.secondary_hover,
                theme.transparent,
            )
        })
}

pub(in crate::app) fn workspace_today_level_tokens(state: &RuntimeState) -> i64 {
    state.ai_global_history.today_total_tokens.max(0)
}
#[derive(Clone)]
struct DailyLevelTier {
    id: &'static str,
    title: &'static str,
    min: i64,
    color: u32,
    icon: DailyLevelIcon,
}

#[derive(Clone)]
enum DailyLevelIcon {
    Component(HeroIconName),
    Asset(&'static str),
}

const DAILY_LEVEL_TIERS: [DailyLevelTier; 8] = [
    DailyLevelTier {
        id: "iron",
        title: "Iron",
        min: 0,
        color: 0x5B616D,
        icon: DailyLevelIcon::Component(HeroIconName::Minus),
    },
    DailyLevelTier {
        id: "bronze",
        title: "Bronze",
        min: 1_000_000,
        color: 0xC98663,
        icon: DailyLevelIcon::Asset("rank-icons/zap.svg"),
    },
    DailyLevelTier {
        id: "silver",
        title: "Silver",
        min: 3_000_000,
        color: 0xC8D1E3,
        icon: DailyLevelIcon::Asset("rank-icons/shield-check.svg"),
    },
    DailyLevelTier {
        id: "gold",
        title: "Gold",
        min: 6_000_000,
        color: 0xE8AA34,
        icon: DailyLevelIcon::Component(HeroIconName::Star),
    },
    DailyLevelTier {
        id: "platinum",
        title: "Platinum",
        min: 10_000_000,
        color: 0x7ED6D8,
        icon: DailyLevelIcon::Component(HeroIconName::Star),
    },
    DailyLevelTier {
        id: "diamond",
        title: "Diamond",
        min: 18_000_000,
        color: 0x59A7FF,
        icon: DailyLevelIcon::Asset("rank-icons/sparkles.svg"),
    },
    DailyLevelTier {
        id: "master",
        title: "Master",
        min: 30_000_000,
        color: 0x9A72FF,
        icon: DailyLevelIcon::Asset("rank-icons/trophy.svg"),
    },
    DailyLevelTier {
        id: "grandmaster",
        title: "Grandmaster",
        min: 50_000_000,
        color: 0xFF5E8E,
        icon: DailyLevelIcon::Asset("rank-icons/flame.svg"),
    },
];

fn daily_level_tier(tokens: i64) -> DailyLevelTier {
    DAILY_LEVEL_TIERS
        .iter()
        .rev()
        .find(|tier| tokens >= tier.min)
        .cloned()
        .unwrap_or_else(|| DAILY_LEVEL_TIERS[0].clone())
}

fn daily_level_title(tier: &DailyLevelTier, language: &str) -> String {
    workspace_i18n(language, &format!("rank.{}", tier.id), tier.title)
}

fn workspace_daily_level_button_content(
    tier: DailyLevelTier,
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
    tokens: i64,
    current_tier: DailyLevelTier,
    language: String,
    hover_surface: gpui::Hsla,
    transparent: gpui::Hsla,
) -> impl IntoElement {
    let tokens = tokens.max(0);
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
            DAILY_LEVEL_TIERS.into_iter().map(|tier| {
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

fn daily_level_badge(tier: &DailyLevelTier, box_size: f32, icon_size: f32) -> impl IntoElement {
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
        .child(daily_level_icon(tier.icon.clone(), icon_size))
}

fn daily_level_icon(icon: DailyLevelIcon, icon_size: f32) -> impl IntoElement {
    let icon = match icon {
        DailyLevelIcon::Component(name) => Icon::new(name),
        DailyLevelIcon::Asset(path) => Icon::empty().path(path),
    };
    icon.with_size(px(icon_size)).text_color(color(0xFFFFFF))
}
