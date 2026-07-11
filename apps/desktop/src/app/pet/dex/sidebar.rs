use super::*;

pub(super) struct PetDexCurrentCardInput<'a> {
    pub(super) pet: &'a PetSummary,
    pub(super) custom_pet: Option<&'a PetCustomPet>,
    pub(super) catalog_item: Option<&'a PetCatalogItem>,
    pub(super) runtime_asset_root: &'a Path,
    pub(super) support_dir: &'a Path,
    pub(super) custom_pets: &'a [PetCustomPet],
    pub(super) stats: &'a PetStats,
    pub(super) total_xp: i64,
    pub(super) claimed_at: Option<i64>,
    pub(super) language: &'a str,
}

pub(super) fn pet_dex_current_card(
    input: PetDexCurrentCardInput<'_>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let PetDexCurrentCardInput {
        pet,
        custom_pet,
        catalog_item,
        runtime_asset_root,
        support_dir,
        custom_pets,
        stats,
        total_xp,
        claimed_at,
        language,
    } = input;
    let sprite_path = pet_sprite_path(runtime_asset_root, support_dir, pet, custom_pets);
    let name = if pet.claimed {
        pet.display_name.clone()
    } else {
        pet_catalog_text(language, "pet.dex.no_current_pet", "No active pet yet")
    };
    let description = custom_pet
        .map(|pet| empty_label(&pet.description))
        .or_else(|| {
            catalog_item.map(|item| {
                pet_catalog_text(
                    language,
                    &item.subtitle_key,
                    &pet_species_subtitle(&pet.species),
                )
            })
        })
        .unwrap_or_else(|| pet_species_subtitle(&pet.species));
    let level = pet_format_placeholders(
        &pet_catalog_text(language, "pet.dex.current_level_format", "Lv.%@"),
        &[pet.level.max(1).to_string()],
    );

    div()
        .child(
            div()
                .mb(px(8.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_MUTED))
                .child(pet_catalog_text(
                    language,
                    "pet.dex.current_pet",
                    "Current Pet",
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .size(px(84.0))
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(pet_sprite_element(
                            sprite_path,
                            84.0,
                            0,
                            0,
                            cx.theme().primary,
                        )),
                )
                .child(
                    div()
                        .max_w(px(210.0))
                        .truncate()
                        .text_center()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .font_weight(FontWeight::BOLD)
                        .child(name),
                )
                .child(
                    div()
                        .max_w(px(210.0))
                        .truncate()
                        .text_center()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(format!("{description} · {level}")),
                ),
        )
        .child(
            div()
                .mt(px(16.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(pet_trait_bar(
                    "🧠",
                    pet_catalog_text(language, "pet.attribute.wisdom", "Wisdom"),
                    stats.wisdom,
                    0x2F8FFF,
                ))
                .child(pet_trait_bar(
                    "🔥",
                    pet_catalog_text(language, "pet.attribute.chaos", "Chaos"),
                    stats.chaos,
                    0xFF6030,
                ))
                .child(pet_trait_bar(
                    "🌙",
                    pet_catalog_text(language, "pet.attribute.night", "Night"),
                    stats.night,
                    0x6060CC,
                ))
                .child(pet_trait_bar(
                    "💪",
                    pet_catalog_text(language, "pet.attribute.stamina", "Stamina"),
                    stats.stamina,
                    0x20A060,
                ))
                .child(pet_trait_bar(
                    "🩹",
                    pet_catalog_text(language, "pet.attribute.empathy", "Empathy"),
                    stats.empathy,
                    0xE060A0,
                )),
        )
        .child(
            div()
                .mt(px(14.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .rounded_full()
                        .bg(color(theme::ACCENT).opacity(0.12))
                        .px(px(10.0))
                        .py(px(5.0))
                        .text_size(rems(0.75))
                        .line_height(rems(0.875))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::ACCENT))
                        .child(pet_catalog_text(
                            language,
                            "pet.stage.companion",
                            "Companion",
                        )),
                )
                .when_some(claimed_at, |this, timestamp| {
                    this.child(
                        div()
                            .text_size(rems(0.75))
                            .line_height(rems(1.0))
                            .text_color(color(theme::TEXT_DIM))
                            .child(pet_date_label(timestamp)),
                    )
                }),
        )
        .child(
            div()
                .mt(px(12.0))
                .rounded(px(8.0))
                .bg(cx.theme().group_box)
                .px(px(12.0))
                .py(px(9.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .child(
                    div()
                        .text_color(color(theme::TEXT_MUTED))
                        .child(pet_catalog_text(language, "pet.total_xp", "Total XP")),
                )
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .child(compact_number(total_xp)),
                ),
        )
}

pub(super) fn pet_trait_bar(
    emoji: &'static str,
    label: String,
    value: i64,
    accent: u32,
) -> impl IntoElement {
    let ratio = (value as f32 / 330.0).clamp(0.0, 1.0);
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .child(div().w(px(18.0)).child(emoji))
        .child(
            div()
                .w(px(34.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(color(theme::TEXT_MUTED))
                .child(label),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .h(px(5.0))
                .rounded_full()
                .overflow_hidden()
                .bg(color(accent).opacity(0.16))
                .child(
                    div()
                        .h_full()
                        .w(gpui::relative(ratio))
                        .rounded_full()
                        .bg(color(accent).opacity(0.75)),
                ),
        )
        .child(
            div()
                .w(px(34.0))
                .text_right()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_DIM))
                .child(compact_number(value)),
        )
}

pub(super) struct PetDexSidebarOverview {
    pub(super) current_name: String,
    pub(super) current_level: String,
    pub(super) archived_count: usize,
    pub(super) archived_subtitle: String,
    pub(super) unlocked_count: usize,
    pub(super) total_count: usize,
    pub(super) collection_subtitle: String,
}

pub(super) fn pet_dex_sidebar_overview(
    language: &str,
    overview: PetDexSidebarOverview,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let PetDexSidebarOverview {
        current_name,
        current_level,
        archived_count,
        archived_subtitle,
        unlocked_count,
        total_count,
        collection_subtitle,
    } = overview;
    div()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    Icon::new(HeroIconName::BookOpen)
                        .size_3p5()
                        .text_color(color(theme::TEXT_MUTED)),
                )
                .child(
                    div()
                        .text_size(rems(1.0625))
                        .line_height(rems(1.375))
                        .font_weight(FontWeight::BOLD)
                        .child(pet_catalog_text(language, "pet.dex.title", "Pet Dex")),
                ),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(pet_catalog_text(
                    language,
                    "pet.dex.subtitle",
                    "A record of every coding companion you've raised",
                )),
        )
        .child(pet_dex_separator())
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(pet_dex_summary_row(
                    pet_catalog_text(language, "pet.dex.current_companion", "Current Companion"),
                    current_level,
                    current_name,
                    cx,
                ))
                .child(pet_dex_summary_row(
                    pet_catalog_text(language, "pet.dex.archived", "Archived"),
                    archived_subtitle.to_string(),
                    archived_count.to_string(),
                    cx,
                ))
                .child(pet_dex_summary_row(
                    pet_catalog_text(language, "pet.dex.collection", "Dex Collection"),
                    collection_subtitle.to_string(),
                    format!("{unlocked_count}/{}", total_count.max(1)),
                    cx,
                )),
        )
        .child(pet_dex_separator())
}

pub(super) fn pet_dex_separator() -> impl IntoElement {
    div()
        .my(px(16.0))
        .h(px(1.0))
        .w_full()
        .bg(color(theme::BORDER_SOFT).opacity(0.75))
}

pub(super) fn pet_dex_summary_row(
    label: String,
    subtitle: String,
    value: String,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .rounded(px(8.0))
        .bg(cx.theme().group_box)
        .px(px(12.0))
        .py(px(10.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .child(
            div()
                .min_w_0()
                .child(
                    div()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT_MUTED))
                        .child(label),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .truncate()
                        .text_size(rems(0.75))
                        .line_height(rems(0.9375))
                        .text_color(color(theme::TEXT_DIM))
                        .child(subtitle),
                ),
        )
        .child(
            div()
                .max_w(px(96.0))
                .truncate()
                .text_right()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .font_weight(FontWeight::BOLD)
                .child(value),
        )
}
