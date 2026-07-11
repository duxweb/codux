use super::*;

pub(super) fn pet_dex_spotlight_overlay(
    spotlight: PetDexSpotlight,
    catalog: &PetCatalog,
    runtime_asset_root: &Path,
    support_dir: &Path,
    language: &str,
    sprite_frame: usize,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    if spotlight == PetDexSpotlight::ArchiveConfirm {
        return pet_dex_archive_confirm_overlay(language, cx);
    }

    let detail = match spotlight {
        PetDexSpotlight::Bundled(species) => catalog
            .species
            .iter()
            .find(|item| item.species == species)
            .map(|item| {
                let pet = PetSummary {
                    species: item.species.clone(),
                    ..PetSummary::default()
                };
                (
                    pet_catalog_text(language, &item.name_key, &pet_species_name(&item.species)),
                    pet_catalog_text(language, "pet.stage.companion", "Companion"),
                    pet_catalog_text(
                        language,
                        &item.description_key,
                        &pet_species_subtitle(&item.species),
                    ),
                    pet_sprite_path(runtime_asset_root, support_dir, &pet, &[]),
                    pet_accent_color(&item.species),
                )
            }),
        PetDexSpotlight::Custom(custom_id) => catalog
            .custom_pets
            .iter()
            .find(|pet| pet.id == custom_id)
            .map(|pet| {
                (
                    pet.display_name.clone(),
                    pet_catalog_text(language, "pet.custom.installed", "Custom pet"),
                    empty_label(&pet.description),
                    custom_pet_sprite_path(support_dir, pet).into(),
                    theme::ACCENT,
                )
            }),
        PetDexSpotlight::ArchiveConfirm => None,
    };

    let Some((title, subtitle, description, sprite_path, accent)) = detail else {
        return div().into_any_element();
    };

    div()
        .id("pet-dex-spotlight-overlay")
        .occlude()
        .absolute()
        .top(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .left(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.theme().overlay)
        .p(px(24.0))
        .on_click(cx.listener(|app, _event, _window, cx| app.close_pet_dex_spotlight(cx)))
        .child(
            div()
                .id("pet-dex-spotlight-preview")
                .max_w(px(520.0))
                .flex()
                .flex_col()
                .items_center()
                .text_center()
                .child(
                    div()
                        .mx_auto()
                        .size(px(212.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(color(accent).opacity(0.09))
                        .child(pet_sprite_element(
                            sprite_path,
                            168.0,
                            sprite_frame,
                            0,
                            cx.theme().primary,
                        )),
                )
                .child(
                    div()
                        .mt(px(20.0))
                        .text_size(rems(1.5))
                        .line_height(rems(1.875))
                        .font_weight(FontWeight::BOLD)
                        .text_color(color(theme::TEXT))
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(accent))
                        .child(subtitle),
                )
                .when(!description.is_empty(), |this| {
                    this.child(
                        div()
                            .mt(px(18.0))
                            .max_w(px(420.0))
                            .text_size(rems(0.875))
                            .line_height(rems(1.375))
                            .text_color(color(theme::TEXT_MUTED))
                            .child(description),
                    )
                }),
        )
        .into_any_element()
}

pub(super) fn pet_dex_archive_confirm_overlay(
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    div()
        .id("pet-dex-archive-overlay")
        .occlude()
        .absolute()
        .top(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .left(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.theme().overlay)
        .p(px(24.0))
        .on_click(cx.listener(|app, _event, _window, cx| app.close_pet_dex_spotlight(cx)))
        .child(
            div()
                .id("pet-dex-archive-card")
                .occlude()
                .w(px(360.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(color(theme::BORDER_SOFT))
                .bg(color(theme::BG_PANEL))
                .p(px(20.0))
                .shadow_lg()
                .on_click(|_event, _window, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Icon::new(HeroIconName::Trash)
                                .size_4()
                                .text_color(color(theme::ORANGE)),
                        )
                        .child(
                            div()
                                .text_size(rems(1.0))
                                .line_height(rems(1.375))
                                .font_weight(FontWeight::BOLD)
                                .child(pet_catalog_text(
                                    language,
                                    "pet.archive.alert.title",
                                    "Archive Current Pet",
                                )),
                        ),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.25))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(pet_catalog_text(
                            language,
                            "pet.archive.alert.message",
                            "Archive this pet into the dex and choose a new companion.",
                        )),
                )
                .child(
                    div()
                        .mt(px(20.0))
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            dialog_cancel_button(
                                "pet-dex-cancel-archive",
                                pet_catalog_text(language, "common.cancel", "Cancel"),
                                cx,
                                |app, _event, _window, cx| app.close_pet_dex_spotlight(cx),
                            )
                            .compact(),
                        )
                        .child(
                            dialog_primary_button(
                                "pet-dex-confirm-archive",
                                pet_catalog_text(
                                    language,
                                    "pet.archive.confirm",
                                    "Confirm Archive",
                                ),
                                cx,
                                |app, _event, window, cx| {
                                    app.archive_current_pet_confirmed(window, cx)
                                },
                            )
                            .compact(),
                        ),
                ),
        )
        .into_any_element()
}
