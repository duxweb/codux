use super::*;

impl CoduxApp {
    pub(in crate::app) fn pet_claim_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let catalog = self.pet_catalog.clone();
        if self.pet_claim_species.is_empty() {
            self.pet_claim_species = catalog
                .species
                .iter()
                .find(|item| item.species == "voidcat")
                .or_else(|| catalog.species.first())
                .map(|item| item.species.clone())
                .unwrap_or_else(|| "voidcat".to_string());
        }
        let selected_species = self.pet_claim_species.clone();
        let language = self.state.settings.language.clone();
        let claim_name_state = window.use_keyed_state("pet-claim-custom-name", cx, |window, cx| {
            InputState::new(window, cx).placeholder(pet_catalog_text(
                &language,
                "pet.claim.name.placeholder",
                "Leave empty to use the species name",
            ))
        });
        let preview_pet = PetSummary {
            species: if selected_species == "bundled:random" {
                fallback_random_preview_species(&catalog.species)
            } else {
                selected_species.clone()
            },
            ..self.state.pet.clone()
        };
        let fallback_species = if selected_species.is_empty() {
            catalog
                .species
                .first()
                .map(|item| item.species.clone())
                .unwrap_or_else(|| "voidcat".to_string())
        } else {
            selected_species.clone()
        };
        let custom_pets = catalog.custom_pets.clone();
        let selected_catalog_item = catalog
            .species
            .iter()
            .find(|item| item.species == selected_species)
            .cloned();
        let preview_sprite_frame = self.visible_pet_sprite_frame(PET_IDLE_FRAME_COUNT);

        child_window_shell(
            pet_catalog_text(&language, "pet.claim.window.title", "Claim Pet"),
            cx,
        )
        .child(
            div()
                .min_h_0()
                .flex_1()
                .grid()
                .grid_cols(2)
                .overflow_hidden()
                .child(
                    div()
                        .min_h_0()
                        .border_r_1()
                        .border_color(color(theme::BORDER_SOFT))
                        .p(px(12.0))
                        .overflow_y_scrollbar()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .children(catalog.species.into_iter().map(|item| {
                                    pet_claim_option_row(
                                        item,
                                        &selected_species,
                                        &self.runtime.source_root,
                                        &self.state.support_dir,
                                        &self.pet_custom_pets,
                                        &language,
                                        cx,
                                    )
                                    .into_any_element()
                                }))
                                .child(pet_claim_random_row(&selected_species, &language, cx))
                                .when(!custom_pets.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .pt(px(6.0))
                                            .px_1()
                                            .text_size(rems(0.75))
                                            .line_height(rems(1.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(color(theme::TEXT_DIM))
                                            .child(pet_catalog_text(
                                                &language,
                                                "pet.claim.custom.section",
                                                "Custom Pets",
                                            )),
                                    )
                                    .children(
                                        custom_pets.into_iter().map(|pet| {
                                            pet_claim_custom_row(
                                                pet,
                                                &selected_species,
                                                &self.state.support_dir,
                                                cx,
                                            )
                                            .into_any_element()
                                        }),
                                    )
                                }),
                        ),
                )
                .child(
                    div()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(div().min_h_0().flex_1().child(pet_claim_preview(
                            PetClaimPreviewInput {
                                pet: &preview_pet,
                                random: selected_species == "bundled:random",
                                runtime_asset_root: &self.runtime.source_root,
                                support_dir: &self.state.support_dir,
                                custom_pets: &self.pet_custom_pets,
                                catalog_item: selected_catalog_item.as_ref(),
                                language: &language,
                                sprite_frame: preview_sprite_frame,
                            },
                            cx,
                        )))
                        .child(div().px(px(20.0)).pb(px(16.0)).child(
                            Input::new(&claim_name_state).with_size(gpui_component::Size::Medium),
                        )),
                ),
        )
        .child(pet_footer_bar(pet_dialog_footer(vec![
            pet_cancel_button("pet-claim-cancel", &language, cx).into_any_element(),
            pet_footer_button(
                "pet-claim-confirm",
                pet_catalog_text(&language, "pet.claim.confirm", "Confirm Claim"),
                HeroIconName::Check,
                true,
                cx,
                move |app, _event, window, cx| {
                    let selected = if app.pet_claim_species.is_empty() {
                        fallback_species.clone()
                    } else {
                        app.pet_claim_species.clone()
                    };
                    let species = if selected == "bundled:random" {
                        random_pet_species(&app.runtime_service.pet_catalog().species)
                    } else {
                        selected
                    };
                    let custom_name = claim_name_state.read(cx).value().to_string();
                    app.claim_pet_species(species, custom_name, window, cx);
                },
            )
            .into_any_element(),
        ])))
    }

    pub(in crate::app) fn pet_custom_install_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let language = self.state.settings.language.clone();

        child_window_shell(
            pet_catalog_text(&language, "pet.custom.install.title", "Add Custom Pet"),
            cx,
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .p(px(16.0))
                .child(workspace_pet_install_form(
                    WorkspacePetInstallInput {
                        install_url: &self.pet_install_url,
                        install_display_name: &self.pet_install_display_name,
                        install_preview: self.pet_install_preview.as_ref(),
                        install_error: self.pet_install_error.as_deref(),
                        install_previewing: self.pet_install_previewing,
                        installing: self.pet_installing,
                        language: &language,
                    },
                    window,
                    cx,
                )),
        )
        .child(pet_footer_bar(pet_dialog_footer(vec![
            pet_cancel_button("pet-custom-install-cancel", &language, cx).into_any_element(),
            pet_footer_button(
                "pet-custom-install-window",
                pet_catalog_text(&language, "pet.custom.install.confirm", "Install"),
                HeroIconName::Plus,
                true,
                cx,
                |app, _event, window, cx| app.install_custom_pet(window, cx),
            )
            .custom(ButtonCustomVariant::new(cx).color(cx.theme().primary))
            .text_color(cx.theme().primary_foreground)
            .loading(self.pet_installing)
            .disabled(
                self.pet_install_preview.is_none()
                    || self.pet_install_display_name.trim().is_empty()
                    || self.pet_install_previewing
                    || self.pet_installing,
            )
            .into_any_element(),
        ])))
    }
}
fn pet_claim_random_row(
    selected_species: &str,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let selected = selected_species == "bundled:random";

    pet_select_row(
        SharedString::from("pet-claim-random"),
        selected,
        pet_catalog_text(language, "pet.claim.random.title", "Random"),
        pet_catalog_text(
            language,
            "pet.claim.random.subtitle",
            "Let Codux choose a companion",
        ),
        pet_claim_random_thumb(cx),
        cx,
    )
    .on_click(cx.listener(move |app, _event, _window, cx| {
        app.pet_claim_species = "bundled:random".to_string();
        app.status_message = "selected random pet".to_string();
        cx.notify();
    }))
}

fn pet_claim_option_row(
    item: PetCatalogItem,
    selected_species: &str,
    runtime_asset_root: &Path,
    support_dir: &Path,
    custom_pets: &[PetCustomPet],
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let selected = selected_species == item.species;
    let species = item.species.clone();
    let title = pet_catalog_text(language, &item.name_key, &pet_species_name(&item.species));
    let subtitle = pet_catalog_text(
        language,
        &item.subtitle_key,
        &pet_species_subtitle(&item.species),
    );
    let sprite_path = pet_sprite_path(
        runtime_asset_root,
        support_dir,
        &PetSummary {
            species: item.species.clone(),
            ..PetSummary::default()
        },
        custom_pets,
    );

    pet_select_row(
        SharedString::from(format!("pet-claim-bundled-{}", item.species)),
        selected,
        title,
        subtitle,
        pet_claim_sprite_thumb(sprite_path, cx.theme().primary, cx),
        cx,
    )
    .on_click(cx.listener(move |app, _event, _window, cx| {
        app.pet_claim_species = species.clone();
        app.status_message = format!("selected pet species: {}", species);
        cx.notify();
    }))
}

fn pet_claim_custom_row(
    pet: PetCustomPet,
    selected_species: &str,
    support_dir: &Path,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let selected = selected_species == format!("custom:{}", pet.id);
    let species = format!("custom:{}", pet.id);
    let title = pet.display_name.clone();
    let subtitle = empty_label(&pet.description);
    let sprite_path = custom_pet_sprite_path(support_dir, &pet);

    pet_select_row(
        SharedString::from(format!("pet-claim-custom-{}", pet.id)),
        selected,
        title,
        subtitle,
        pet_claim_sprite_thumb(sprite_path.into(), cx.theme().primary, cx),
        cx,
    )
    .on_click(cx.listener(move |app, _event, _window, cx| {
        app.pet_claim_species = species.clone();
        app.status_message = format!("selected custom pet: {}", species);
        cx.notify();
    }))
}

fn pet_select_row(
    id: SharedString,
    selected: bool,
    title: String,
    subtitle: String,
    leading: AnyElement,
    cx: &mut Context<CoduxApp>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .cursor_pointer()
        .rounded(px(10.0))
        .border_1()
        .border_color(if selected {
            color(theme::ACCENT).opacity(0.6)
        } else {
            cx.theme().transparent
        })
        .px(px(10.0))
        .py(px(7.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .bg(if selected {
            color(theme::ACCENT).opacity(0.1)
        } else {
            cx.theme().secondary
        })
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .child(leading)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .text_size(rems(0.8125))
                        .line_height(rems(1.0625))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(rems(0.75))
                        .line_height(rems(0.9375))
                        .text_color(color(theme::TEXT_MUTED))
                        .truncate()
                        .child(subtitle),
                ),
        )
        .child(
            div()
                .size(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .child(if selected {
                    Icon::new(HeroIconName::CheckCircle)
                        .size_3p5()
                        .text_color(color(theme::ACCENT))
                        .into_any_element()
                } else {
                    div().into_any_element()
                }),
        )
}

fn pet_claim_sprite_thumb(
    sprite_path: ImageSource,
    fallback_color: gpui::Hsla,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    div()
        .size(px(44.0))
        .rounded(px(8.0))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.theme().group_box)
        .child(pet_sprite_element(sprite_path, 44.0, 0, 0, fallback_color))
        .into_any_element()
}

fn pet_claim_random_thumb(cx: &mut Context<CoduxApp>) -> AnyElement {
    let accent = cx.theme().primary;
    div()
        .size(px(44.0))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(accent.opacity(0.12))
        .text_color(accent)
        .child(
            Icon::new(HeroIconName::Sparkles)
                .size_5()
                .text_color(accent),
        )
        .into_any_element()
}

struct PetClaimPreviewInput<'a> {
    pet: &'a PetSummary,
    random: bool,
    runtime_asset_root: &'a Path,
    support_dir: &'a Path,
    custom_pets: &'a [PetCustomPet],
    catalog_item: Option<&'a PetCatalogItem>,
    language: &'a str,
    sprite_frame: usize,
}

fn pet_claim_preview(
    input: PetClaimPreviewInput<'_>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let PetClaimPreviewInput {
        pet,
        random,
        runtime_asset_root,
        support_dir,
        custom_pets,
        catalog_item,
        language,
        sprite_frame,
    } = input;
    let sprite_path = pet_sprite_path(runtime_asset_root, support_dir, pet, custom_pets);
    let title = if random {
        pet_catalog_text(language, "pet.claim.random.title", "Random")
    } else if pet.species.starts_with("custom:") {
        custom_pets
            .iter()
            .find(|custom| pet.species == format!("custom:{}", custom.id))
            .map(|custom| custom.display_name.clone())
            .unwrap_or_else(|| pet_catalog_text(language, "pet.custom.missing", "Custom Pet"))
    } else {
        catalog_item
            .map(|item| pet_catalog_text(language, &item.name_key, &pet_species_name(&pet.species)))
            .unwrap_or_else(|| pet_species_name(&pet.species))
    };
    let description = if random {
        pet_catalog_text(
            language,
            "pet.claim.random.description",
            "Let Codux choose one companion for you.",
        )
    } else {
        catalog_item
            .map(|item| {
                pet_catalog_text(
                    language,
                    &item.description_key,
                    &pet_species_subtitle(&pet.species),
                )
            })
            .unwrap_or_else(|| pet_species_subtitle(&pet.species))
    };

    div()
        .min_h_0()
        .p(px(20.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .text_center()
        .child(
            div()
                .size(px(118.0))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .bg(color(theme::ACCENT).opacity(0.12))
                .child(if random {
                    Icon::new(HeroIconName::Sparkles)
                        .size_8()
                        .text_color(cx.theme().primary)
                        .into_any_element()
                } else {
                    pet_sprite_element(sprite_path, 92.0, sprite_frame, 0, cx.theme().primary)
                }),
        )
        .child(
            div()
                .mt(px(14.0))
                .text_size(rems(1.0))
                .line_height(rems(1.25))
                .font_weight(FontWeight::BOLD)
                .child(title),
        )
        .child(
            div()
                .mt(px(6.0))
                .max_w(px(340.0))
                .text_size(rems(0.75))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT_MUTED))
                .child(description),
        )
}

fn fallback_random_preview_species(items: &[PetCatalogItem]) -> String {
    items
        .first()
        .map(|item| item.species.clone())
        .unwrap_or_else(|| "voidcat".to_string())
}

fn random_pet_species(items: &[PetCatalogItem]) -> String {
    if items.is_empty() {
        return "voidcat".to_string();
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as usize)
        .unwrap_or_default();
    items[nanos % items.len()].species.clone()
}
