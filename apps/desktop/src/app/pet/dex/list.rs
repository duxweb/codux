use super::*;

pub(super) struct PetDexVirtualRowsInput<'a> {
    pub(super) bundled_items: Vec<PetCatalogItem>,
    pub(super) unlocked_species: &'a HashSet<String>,
    pub(super) custom_pets: Vec<PetCustomPet>,
    pub(super) legacy_records: Vec<PetLegacyRecord>,
    pub(super) runtime_asset_root: &'a Path,
    pub(super) support_dir: &'a Path,
    pub(super) sprite_paths: &'a HashMap<String, ImageSource>,
    pub(super) language: &'a str,
}

pub(super) fn pet_dex_virtual_rows(
    input: PetDexVirtualRowsInput<'_>,
    window: &mut Window,
) -> Vec<PetDexVirtualRow> {
    let PetDexVirtualRowsInput {
        bundled_items,
        unlocked_species,
        custom_pets,
        legacy_records,
        runtime_asset_root,
        support_dir,
        sprite_paths,
        language,
    } = input;
    let columns = pet_dex_columns(window);
    let mut rows = Vec::new();
    let unlocked_count = bundled_items
        .iter()
        .filter(|item| unlocked_species.contains(&item.species))
        .count();
    let total_count = bundled_items.len();

    rows.push(PetDexVirtualRow::Spacer { height: 20.0 });
    rows.push(PetDexVirtualRow::SectionHeader {
        label: pet_catalog_text(language, "pet.dex.bundled.section", "Bundled Pets"),
        trailing: Some(pet_format_placeholders(
            &pet_catalog_text(language, "pet.dex.unlocked_count", "%@/%@ unlocked"),
            &[unlocked_count.to_string(), total_count.to_string()],
        )),
    });
    for chunk in bundled_items.chunks(columns) {
        rows.push(PetDexVirtualRow::PetCardRow {
            columns,
            cards: chunk
                .iter()
                .map(|item| {
                    let unlocked = unlocked_species.contains(&item.species);
                    PetDexCard::Bundled {
                        item: item.clone(),
                        unlocked,
                        sprite_path: sprite_paths.get(&item.species).cloned(),
                        title: if unlocked {
                            pet_catalog_text(
                                language,
                                &item.name_key,
                                &pet_species_name(&item.species),
                            )
                        } else {
                            pet_catalog_text(language, "pet.dex.unknown", "???")
                        },
                        subtitle: if unlocked {
                            pet_catalog_text(language, "pet.stage.companion", "Companion")
                        } else {
                            pet_catalog_text(language, "pet.dex.locked", "Locked")
                        },
                    }
                })
                .collect(),
        });
    }

    rows.push(PetDexVirtualRow::Spacer { height: 28.0 });
    rows.push(PetDexVirtualRow::SectionHeader {
        label: pet_catalog_text(language, "pet.claim.custom.section", "Custom Pets"),
        trailing: Some(pet_format_placeholders(
            &pet_catalog_text(language, "pet.custom.installed_count", "%@ installed"),
            &[custom_pets.len().to_string()],
        )),
    });
    if custom_pets.is_empty() {
        rows.push(PetDexVirtualRow::EmptyState {
            message: pet_catalog_text(
                language,
                "pet.custom.install.subtitle",
                "Install a Codex-format pet from Petdex.",
            ),
        });
    } else {
        for chunk in custom_pets.chunks(columns) {
            rows.push(PetDexVirtualRow::PetCardRow {
                columns,
                cards: chunk
                    .iter()
                    .map(|pet| PetDexCard::Custom {
                        pet: pet.clone(),
                        sprite_path: sprite_paths
                            .get(&format!("custom:{}", pet.id))
                            .cloned()
                            .unwrap_or_else(|| custom_pet_sprite_path(support_dir, pet).into()),
                        subtitle: pet_catalog_text(language, "pet.custom.installed", "Custom pet"),
                    })
                    .collect(),
            });
        }
    }

    rows.push(PetDexVirtualRow::Spacer { height: 28.0 });
    rows.push(PetDexVirtualRow::SectionHeader {
        label: pet_catalog_text(language, "pet.archive.history", "Archive History"),
        trailing: None,
    });
    if legacy_records.is_empty() {
        rows.push(PetDexVirtualRow::EmptyState {
            message: pet_catalog_text(language, "pet.dex.archived.none", "No archived pets yet"),
        });
    } else {
        for record in legacy_records.into_iter().rev() {
            let sprite_path = legacy_pet_sprite_path(runtime_asset_root, support_dir, &record);
            rows.push(PetDexVirtualRow::LegacyRow {
                record: Box::new(record),
                sprite_path,
                language: language.to_string(),
            });
        }
    }
    rows.push(PetDexVirtualRow::Spacer { height: 20.0 });

    rows
}

fn pet_dex_columns(window: &mut Window) -> usize {
    let width = window.viewport_size().width.as_f32();
    if width >= 1160.0 {
        5
    } else if width >= 900.0 {
        4
    } else {
        3
    }
}

fn pet_dex_card_frame(id: SharedString) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_1()
        .min_w_0()
        .h(px(136.0))
        .rounded(px(8.0))
        .border_1()
        .px(px(10.0))
        .py(px(12.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .text_center()
}

pub(super) fn pet_dex_virtual_card(card: PetDexCard, cx: &mut Context<CoduxApp>) -> AnyElement {
    match card {
        PetDexCard::Bundled {
            item,
            unlocked,
            sprite_path,
            title,
            subtitle,
        } => {
            let species = item.species.clone();
            pet_dex_card_frame(SharedString::from(format!("pet-dex-bundled-{species}")))
                .cursor_pointer()
                .border_color(if unlocked {
                    color(theme::ACCENT).opacity(0.25)
                } else {
                    color(theme::BORDER_SOFT)
                })
                .bg(if unlocked {
                    cx.theme().secondary
                } else {
                    cx.theme().group_box
                })
                .opacity(if unlocked { 1.0 } else { 0.8 })
                .hover(|style| style.bg(cx.theme().secondary_hover))
                .on_click(cx.listener(move |app, _event, _window, cx| {
                    if unlocked {
                        app.show_pet_dex_spotlight(PetDexSpotlight::Bundled(species.clone()), cx);
                    }
                }))
                .child(
                    div()
                        .size(px(56.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(if unlocked {
                            color(pet_accent_color(&item.species)).opacity(0.16)
                        } else {
                            cx.theme().secondary
                        })
                        .child(if unlocked {
                            if let Some(sprite_path) = sprite_path {
                                pet_sprite_element(sprite_path, 44.0, 0, 0, cx.theme().primary)
                            } else {
                                div()
                                    .text_size(rems(1.75))
                                    .line_height(rems(2.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(color(theme::TEXT_DIM))
                                    .child("?")
                                    .into_any_element()
                            }
                        } else {
                            div()
                                .text_size(rems(1.75))
                                .line_height(rems(2.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(color(theme::TEXT_DIM))
                                .child("?")
                                .into_any_element()
                        }),
                )
                .child(
                    div()
                        .mt(px(10.0))
                        .w_full()
                        .truncate()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .truncate()
                        .text_color(if unlocked {
                            color(theme::TEXT_MUTED)
                        } else {
                            color(theme::TEXT_DIM)
                        })
                        .child(subtitle),
                )
                .into_any_element()
        }
        PetDexCard::Custom {
            pet,
            sprite_path,
            subtitle,
        } => {
            let pet_id = pet.id.clone();
            pet_dex_card_frame(SharedString::from(format!("pet-dex-custom-{pet_id}")))
                .cursor_pointer()
                .border_color(color(theme::ACCENT).opacity(0.25))
                .bg(cx.theme().secondary)
                .hover(|style| style.bg(cx.theme().secondary_hover))
                .on_click(cx.listener(move |app, _event, _window, cx| {
                    app.show_pet_dex_spotlight(PetDexSpotlight::Custom(pet_id.clone()), cx);
                }))
                .child(
                    div()
                        .size(px(56.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(color(theme::ACCENT).opacity(0.12))
                        .child(pet_sprite_element(
                            sprite_path,
                            44.0,
                            0,
                            0,
                            cx.theme().primary,
                        )),
                )
                .child(
                    div()
                        .mt(px(10.0))
                        .w_full()
                        .truncate()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(pet.display_name),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .w_full()
                        .truncate()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::ACCENT))
                        .child(subtitle),
                )
                .into_any_element()
        }
    }
}

pub(super) fn pet_dex_empty_state(message: String, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .rounded(px(10.0))
        .border_1()
        .border_color(color(theme::BORDER_SOFT))
        .bg(cx.theme().group_box)
        .px(px(14.0))
        .py(px(24.0))
        .text_center()
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(color(theme::TEXT_DIM))
        .child(message)
}

pub(super) fn pet_legacy_row(
    record: PetLegacyRecord,
    sprite_path: ImageSource,
    language: String,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let legacy_id = record.id.clone();
    let pet_name = if record.custom_name.trim().is_empty() {
        record
            .custom_pet
            .as_ref()
            .map(|pet| pet.display_name.clone())
            .unwrap_or_else(|| pet_species_name(&record.species))
    } else {
        record.custom_name.clone()
    };

    div()
        .rounded(px(8.0))
        .bg(cx.theme().secondary)
        .px(px(12.0))
        .py(px(10.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .child(
            div()
                .size(px(44.0))
                .rounded(px(8.0))
                .overflow_hidden()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .bg(cx.theme().group_box)
                .child(pet_sprite_element(
                    sprite_path,
                    38.0,
                    0,
                    0,
                    cx.theme().primary,
                )),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(pet_name),
                        )
                        .child(
                            div()
                                .rounded_full()
                                .bg(color(theme::ACCENT).opacity(0.12))
                                .px(px(8.0))
                                .py(px(2.0))
                                .text_size(rems(0.75))
                                .line_height(rems(0.875))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(color(theme::ACCENT))
                                .child(pet_catalog_text(
                                    &language,
                                    "pet.stage.companion",
                                    "Companion",
                                )),
                        ),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_DIM))
                        .child(format!(
                            "{} XP · Lv.{}",
                            compact_number(record.total_xp),
                            record.progress.level
                        )),
                ),
        )
        .child(
            div()
                .w(px(100.0))
                .flex_none()
                .text_right()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_DIM))
                .child(pet_date_label(record.retired_at)),
        )
        .child(with_codux_tooltip(
            cx.entity(),
            format!("pet-restore-legacy-tooltip-{legacy_id}"),
            Button::new(SharedString::from(format!(
                "pet-restore-legacy-{legacy_id}"
            )))
            .compact()
            .ghost()
            .text_color(cx.theme().secondary_foreground)
            .icon(Icon::new(HeroIconName::ArrowUturnLeft).size_3p5())
            .on_click(cx.listener(move |app, _event, _window, cx| {
                let legacy_id = legacy_id.clone();
                app.run_pet_change_async(
                    "restore_pet",
                    "restoring pet".to_string(),
                    move |service| {
                        service
                            .restore_archived_pet(PetRestoreRequest { legacy_id })
                            .map(|_| ())
                    },
                    |app, _cx| {
                        app.pet_dex_spotlight = None;
                        app.status_message = "pet restored".to_string();
                    },
                    cx,
                );
            })),
            pet_catalog_text(&language, "pet.archive.restore.action", "Restore"),
        ))
}
