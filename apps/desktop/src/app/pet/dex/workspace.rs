use super::*;

impl CoduxApp {
    pub(in crate::app) fn pet_dex_workspace(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let catalog = self.pet_catalog.clone();
        let snapshot = self.pet_snapshot.clone();
        let current_custom_pet = snapshot.custom_pet.as_ref().map(|pet| {
            self.runtime_service
                .hydrate_custom_pet_data_url(pet.clone())
        });
        let mut unlocked_species = HashSet::new();
        if self.state.pet.claimed && snapshot.custom_pet.is_none() {
            unlocked_species.insert(self.state.pet.species.clone());
        }
        for record in &snapshot.legacy {
            if record.custom_pet.is_none() {
                unlocked_species.insert(record.species.clone());
            }
        }
        let custom_pets = catalog.custom_pets.clone();
        let current_catalog_item = catalog
            .species
            .iter()
            .find(|item| item.species == self.state.pet.species)
            .cloned();
        let total_count = catalog.species.len() + custom_pets.len();
        let unlocked_count = unlocked_species.len() + custom_pets.len();
        let language = self.state.settings.language.clone();
        let current_name = if self.state.pet.claimed {
            self.state.pet.display_name.clone()
        } else {
            pet_catalog_text(&language, "pet.dex.unclaimed", "Not Claimed")
        };
        let current_level = if self.state.pet.claimed {
            pet_format_placeholders(
                &pet_catalog_text(&language, "pet.dex.current_level_format", "Lv.%@"),
                &[snapshot.progress.level.max(1).to_string()],
            )
        } else {
            pet_catalog_text(&language, "pet.dex.unclaimed", "Not Claimed")
        };
        let archived_count = snapshot.legacy.len();
        let archived_subtitle = if archived_count == 0 {
            pet_catalog_text(&language, "pet.dex.archived.none", "No archived pets yet")
        } else {
            pet_catalog_text(&language, "pet.dex.archived.history", "Past companions")
        };
        let collection_subtitle = if total_count > 0 && unlocked_count == total_count {
            pet_catalog_text(
                &language,
                "pet.dex.collection.complete",
                "All companions unlocked",
            )
        } else {
            pet_catalog_text(&language, "pet.dex.collection.continue", "Keep exploring")
        };
        let primary_action = if self.state.pet.claimed {
            pet_inline_button(
                "pet-dex-archive",
                pet_catalog_text(&language, "pet.archive.action", "Archive"),
                HeroIconName::Trash,
                true,
                cx,
                |app, _event, window, cx| app.archive_current_pet(window, cx),
            )
            .into_any_element()
        } else {
            pet_inline_button(
                "pet-dex-claim",
                pet_catalog_text(&language, "pet.claim.action", "Claim Pet"),
                HeroIconName::Heart,
                true,
                cx,
                |app, _event, window, cx| app.defer_open_pet_claim_window(window, cx),
            )
            .into_any_element()
        };
        let add_custom_action = pet_inline_button(
            "pet-dex-open-custom-install",
            pet_catalog_text(&language, "pet.custom.install.action", "Add Custom Pet"),
            HeroIconName::Plus,
            true,
            cx,
            |app, _event, window, cx| app.defer_open_pet_custom_install_window(window, cx),
        )
        .into_any_element();

        child_window_shell(
            pet_catalog_text(&language, "pet.dex.window.title", "Pet Dex"),
            cx,
        )
        .child(
            div()
                .min_h_0()
                .flex_1()
                .relative()
                .flex()
                .overflow_hidden()
                .child(
                    div()
                        .w(px(270.0))
                        .flex_none()
                        .min_h_0()
                        .border_r_1()
                        .border_color(color(theme::BORDER_SOFT))
                        .flex()
                        .flex_col()
                        .child(
                            div().min_h_0().flex_1().overflow_y_scrollbar().child(
                                div()
                                    .p(px(16.0))
                                    .flex()
                                    .flex_col()
                                    .child(pet_dex_sidebar_overview(
                                        &language,
                                        PetDexSidebarOverview {
                                            current_name,
                                            current_level,
                                            archived_count,
                                            archived_subtitle,
                                            unlocked_count,
                                            total_count,
                                            collection_subtitle,
                                        },
                                        cx,
                                    ))
                                    .child(pet_dex_current_card(
                                        PetDexCurrentCardInput {
                                            pet: &self.state.pet,
                                            custom_pet: current_custom_pet.as_ref(),
                                            catalog_item: current_catalog_item.as_ref(),
                                            runtime_asset_root: &self.runtime.source_root,
                                            support_dir: &self.state.support_dir,
                                            custom_pets: &self.pet_custom_pets,
                                            stats: &snapshot.current_stats,
                                            total_xp: snapshot.progress.total_xp,
                                            claimed_at: snapshot.claimed_at,
                                            language: &language,
                                        },
                                        cx,
                                    )),
                            ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .border_t_1()
                                .border_color(color(theme::BORDER_SOFT))
                                .p(px(16.0))
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .child(pet_dex_sidebar_action(primary_action))
                                .child(pet_dex_sidebar_action(add_custom_action)),
                        ),
                )
                .child(self.pet_dex_virtual_content(
                    catalog.species.clone(),
                    unlocked_species,
                    custom_pets,
                    snapshot.legacy,
                    _window,
                    cx,
                )),
        )
        .when_some(self.pet_dex_spotlight.clone(), |this, spotlight| {
            this.child(pet_dex_spotlight_overlay(
                spotlight,
                &catalog,
                &self.runtime.source_root,
                &self.state.support_dir,
                &self.state.settings.language,
                self.visible_pet_sprite_frame(PET_IDLE_FRAME_COUNT),
                cx,
            ))
        })
    }

    fn pet_dex_virtual_content(
        &mut self,
        bundled_items: Vec<PetCatalogItem>,
        unlocked_species: HashSet<String>,
        custom_pets: Vec<PetCustomPet>,
        legacy_records: Vec<PetLegacyRecord>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = Rc::new(pet_dex_virtual_rows(
            PetDexVirtualRowsInput {
                bundled_items,
                unlocked_species: &unlocked_species,
                custom_pets,
                legacy_records,
                runtime_asset_root: &self.runtime.source_root,
                support_dir: &self.state.support_dir,
                sprite_paths: &self.pet_sprite_paths,
                language: &self.state.settings.language,
            },
            window,
        ));
        let item_sizes = Rc::new(
            rows.iter()
                .map(|row| size(px(1.0), row.height()))
                .collect::<Vec<_>>(),
        );
        let scroll_handle = self.pet_dex_scroll_handle.clone();

        div()
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .child(
                v_virtual_list(
                    cx.entity().clone(),
                    "pet-dex-virtual-content",
                    item_sizes,
                    move |_app, visible_range: Range<usize>, _window, cx| {
                        visible_range
                            .filter_map(|index| {
                                rows.get(index).map(|row| row.render(&rows, index, cx))
                            })
                            .collect::<Vec<_>>()
                    },
                )
                .track_scroll(&scroll_handle)
                .with_sizing_behavior(ListSizingBehavior::Auto),
            )
            .vertical_scrollbar(&scroll_handle)
            .into_any_element()
    }
}
