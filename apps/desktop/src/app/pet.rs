use super::*;
use crate::app::app_events::PetUpdateEvent;
use crate::app::ui_helpers::with_codux_tooltip;
use codux_runtime::pet::{PetCatalog, PetCatalogItem, PetLegacyRecord, PetStats};
use gpui::{Hsla, ListSizingBehavior};
use gpui_component::{
    dialog::DialogFooter,
    input::{Input, InputState},
};
use std::{ops::Range, rc::Rc, time::Duration};

use crate::app::workspace_pet_widgets::workspace_pet_install_form;

impl CoduxApp {
    fn defer_open_pet_window(
        _window: &mut Window,
        cx: &mut Context<Self>,
        open: impl FnOnce(&mut CoduxApp, &mut Context<CoduxApp>) + 'static,
    ) {
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            this.update(cx, |app, cx| open(app, cx)).ok();
        })
        .detach();
    }

    pub(in crate::app) fn defer_open_pet_claim_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Self::defer_open_pet_window(window, cx, |app, cx| {
            app.open_pet_claim_window(cx);
        });
    }

    pub(in crate::app) fn defer_open_pet_custom_install_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Self::defer_open_pet_window(window, cx, |app, cx| {
            app.open_pet_custom_install_window(cx);
        });
    }

    pub(in crate::app) fn defer_open_pet_dex_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Self::defer_open_pet_window(window, cx, |app, cx| {
            app.open_pet_dex_window(cx);
        });
    }

    pub(in crate::app) fn open_pet_claim_window(&mut self, cx: &mut Context<Self>) {
        self.open_pet_window(
            AppWindowMode::PetClaim,
            "Claim Pet",
            size(px(680.0), px(500.0)),
            size(px(640.0), px(460.0)),
            cx,
        );
    }

    pub(in crate::app) fn open_pet_custom_install_window(&mut self, cx: &mut Context<Self>) {
        self.open_pet_window(
            AppWindowMode::PetCustomInstall,
            "Add Custom Pet",
            size(
                px(PET_CUSTOM_INSTALL_WINDOW_WIDTH),
                px(PET_CUSTOM_INSTALL_INPUT_HEIGHT),
            ),
            size(px(620.0), px(PET_CUSTOM_INSTALL_INPUT_HEIGHT)),
            cx,
        );
    }

    pub(in crate::app) fn open_pet_dex_window(&mut self, cx: &mut Context<Self>) {
        self.open_pet_window(
            AppWindowMode::PetDex,
            "Petdex",
            size(px(900.0), px(660.0)),
            size(px(780.0), px(560.0)),
            cx,
        );
    }

    fn open_pet_window(
        &mut self,
        mode: AppWindowMode,
        title: &'static str,
        window_size: gpui::Size<gpui::Pixels>,
        min_size: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let existing = match mode {
            AppWindowMode::PetClaim => Self::activate_child_window(&mut self.pet_claim_window, cx),
            AppWindowMode::PetCustomInstall => {
                Self::activate_child_window(&mut self.pet_custom_install_window, cx)
            }
            AppWindowMode::PetDex => Self::activate_child_window(&mut self.pet_dex_window, cx),
            _ => false,
        };
        if existing {
            self.status_message = format!("{title} window already opened");
            cx.notify();
            return;
        }

        self.refresh_pet_cache();
        let state = self.state.clone();
        let runtime = self.runtime.clone();
        let runtime_service = self.runtime_service.clone();
        let bounds = Bounds::centered(None, window_size, cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_child_titlebar(title)),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(min_size),
                is_resizable: mode == AppWindowMode::PetDex,
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                macos_window::configure_child_window_controls(window);
                let app = CoduxApp::new_pet_window_from_state(
                    mode,
                    state.clone(),
                    runtime.clone(),
                    runtime_service.clone(),
                );
                theme::apply_component_theme(
                    &app.state.settings.theme,
                    &app.state.settings.theme_color,
                    Some(window),
                    cx,
                );
                let view = cx.new(|_| app);
                view.update(cx, |app, cx| {
                    app.start_pet_event_sync_loop(cx);
                    if mode == AppWindowMode::PetDex {
                        app.start_pet_sprite_animation_loop(cx);
                    }
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(handle) => {
                let handle: AnyWindowHandle = handle.into();
                match mode {
                    AppWindowMode::PetClaim => self.pet_claim_window = Some(handle),
                    AppWindowMode::PetCustomInstall => {
                        self.pet_custom_install_window = Some(handle)
                    }
                    AppWindowMode::PetDex => self.pet_dex_window = Some(handle),
                    _ => {}
                }
                self.register_child_window_handle(handle);
                format!("{title} window opened")
            }
            Err(error) => format!("failed to open {title} window: {error}"),
        };
        cx.notify();
    }

    pub(in crate::app) fn refresh_pet_cache(&mut self) {
        self.state.pet = self.runtime_service.reload_pet();
        self.pet_catalog = self.runtime_service.pet_catalog();
        self.pet_snapshot = self.runtime_service.pet_snapshot().unwrap_or_default();
        self.pet_custom_pets = self.pet_catalog.custom_pets.clone();
        self.pet_sprite_paths = pet_sprite_path_cache(
            &self.runtime.source_root,
            &self.state.support_dir,
            &self.pet_catalog,
        );
    }

    pub(in crate::app) fn refresh_pet_cache_async(&mut self, cx: &mut Context<Self>) {
        let service = self.runtime_service.clone();
        let source_root = self.runtime.source_root.clone();
        let support_dir = self.state.support_dir.clone();
        self.runtime_trace("pet", "cache_refresh queued");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                service.runtime_trace_frontend("pet", "cache_refresh start");
                let pet = service.reload_pet();
                let catalog = service.pet_catalog();
                let snapshot = service.pet_snapshot().unwrap_or_default();
                let custom_pets = catalog.custom_pets.clone();
                service.runtime_trace_frontend("pet", "cache_refresh ok");
                (pet, catalog, snapshot, custom_pets)
            })
            .await;

            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok((pet, catalog, snapshot, custom_pets)) => {
                        let sprite_paths =
                            pet_sprite_path_cache(&source_root, &support_dir, &catalog);
                        app.state.pet = pet;
                        app.pet_catalog = catalog;
                        app.pet_snapshot = snapshot;
                        app.pet_custom_pets = custom_pets;
                        app.pet_sprite_paths = sprite_paths;
                    }
                    Err(error) => {
                        app.runtime_trace(
                            "pet",
                            &format!("cache_refresh failed join_error={error}"),
                        );
                    }
                }
                app.invalidate_ui_region(cx, UiRegion::Root);
            });
        })
        .detach();
    }

    pub(in crate::app) fn sync_pet_custom_install_event_for_activity_tick(&mut self) -> bool {
        let event = current_pet_custom_install_event();
        self.apply_pet_custom_install_event(event)
    }

    pub(in crate::app) fn sync_pet_update_event_for_activity_tick(&mut self) -> bool {
        let event = current_pet_update_event();
        self.apply_pet_update_event(event)
    }

    fn apply_pet_custom_install_event(&mut self, event: PetCustomInstallEvent) -> bool {
        if event.revision <= self.pet_custom_install_seen_revision {
            return false;
        }

        self.pet_custom_install_seen_revision = event.revision;
        self.refresh_pet_cache();

        let Some(custom_pet_id) = event.custom_pet_id else {
            self.status_message = "custom pet catalog refreshed".to_string();
            return true;
        };

        match self.window_mode {
            AppWindowMode::PetClaim => {
                if self
                    .pet_custom_pets
                    .iter()
                    .any(|pet| pet.id == custom_pet_id)
                {
                    self.pet_claim_species = format!("custom:{custom_pet_id}");
                    self.status_message = "custom pet catalog refreshed".to_string();
                }
            }
            AppWindowMode::PetDex => {
                self.pet_dex_spotlight = Some(PetDexSpotlight::Custom(custom_pet_id));
                self.status_message = "custom pet catalog refreshed".to_string();
            }
            _ => {
                self.status_message = "custom pet catalog refreshed".to_string();
            }
        }

        true
    }

    fn apply_pet_update_event(&mut self, event: PetUpdateEvent) -> bool {
        if event.revision <= self.pet_update_seen_revision {
            return false;
        }

        self.pet_update_seen_revision = event.revision;
        self.refresh_pet_cache();
        if self.window_mode == AppWindowMode::PetDex {
            self.pet_dex_spotlight = None;
        }
        self.status_message = "pet state refreshed".to_string();
        true
    }

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
                                        window,
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
                            &preview_pet,
                            selected_species == "bundled:random",
                            &self.runtime.source_root,
                            &self.state.support_dir,
                            &self.pet_custom_pets,
                            selected_catalog_item.as_ref(),
                            &language,
                            preview_sprite_frame,
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
                    &self.pet_install_url,
                    &self.pet_install_display_name,
                    self.pet_install_preview.as_ref(),
                    self.pet_install_error.as_deref(),
                    self.pet_install_previewing,
                    self.pet_installing,
                    &language,
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
                                        current_name,
                                        current_level,
                                        archived_count,
                                        archived_subtitle,
                                        unlocked_count,
                                        total_count,
                                        collection_subtitle,
                                        cx,
                                    ))
                                    .child(pet_dex_current_card(
                                        &self.state.pet,
                                        current_custom_pet.as_ref(),
                                        current_catalog_item.as_ref(),
                                        &self.runtime.source_root,
                                        &self.state.support_dir,
                                        &self.pet_custom_pets,
                                        &snapshot.current_stats,
                                        snapshot.progress.total_xp,
                                        snapshot.claimed_at,
                                        &language,
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
            bundled_items,
            &unlocked_species,
            custom_pets,
            legacy_records,
            &self.runtime.source_root,
            &self.state.support_dir,
            &self.pet_sprite_paths,
            &self.state.settings.language,
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

#[derive(Clone)]
enum PetDexVirtualRow {
    Spacer {
        height: f32,
    },
    SectionHeader {
        label: String,
        trailing: Option<String>,
    },
    PetCardRow {
        cards: Vec<PetDexCard>,
        columns: usize,
    },
    EmptyState {
        message: String,
    },
    LegacyRow {
        record: PetLegacyRecord,
        sprite_path: ImageSource,
        language: String,
    },
}

#[derive(Clone)]
enum PetDexCard {
    Bundled {
        item: PetCatalogItem,
        unlocked: bool,
        sprite_path: Option<ImageSource>,
        title: String,
        subtitle: String,
    },
    Custom {
        pet: PetCustomPet,
        sprite_path: ImageSource,
        subtitle: String,
    },
}

impl PetDexVirtualRow {
    fn height(&self) -> gpui::Pixels {
        px(match self {
            PetDexVirtualRow::Spacer { height } => *height,
            PetDexVirtualRow::SectionHeader { .. } => 34.0,
            PetDexVirtualRow::PetCardRow { .. } => 148.0,
            PetDexVirtualRow::EmptyState { .. } => 84.0,
            PetDexVirtualRow::LegacyRow { .. } => 72.0,
        })
    }

    fn render(
        &self,
        rows: &Rc<Vec<PetDexVirtualRow>>,
        index: usize,
        cx: &mut Context<CoduxApp>,
    ) -> gpui::Div {
        match self {
            PetDexVirtualRow::Spacer { .. } => div().w_full(),
            PetDexVirtualRow::SectionHeader { label, trailing } => div()
                .w_full()
                .px(px(20.0))
                .pt(px(8.0))
                .child(pet_section_header(label.clone(), trailing.clone())),
            PetDexVirtualRow::PetCardRow { cards, columns } => div()
                .w_full()
                .px(px(20.0))
                .pt(px(12.0))
                .flex()
                .gap(px(12.0))
                .children(
                    cards
                        .iter()
                        .cloned()
                        .map(|card| pet_dex_virtual_card(card, cx)),
                )
                .children(
                    (cards.len()..*columns)
                        .map(|_| div().flex_1().min_w_0().h(px(136.0)).into_any_element()),
                ),
            PetDexVirtualRow::EmptyState { message } => div()
                .w_full()
                .px(px(20.0))
                .pt(px(12.0))
                .child(pet_dex_empty_state(message.clone(), cx)),
            PetDexVirtualRow::LegacyRow {
                record,
                sprite_path,
                language,
            } => div()
                .w_full()
                .px(px(20.0))
                .pt(px(8.0))
                .child(pet_legacy_row(
                    record.clone(),
                    sprite_path.clone(),
                    language.clone(),
                    cx,
                )),
        }
        .when(
            matches!(self, PetDexVirtualRow::LegacyRow { .. })
                && rows
                    .get(index + 1)
                    .map(|next| !matches!(next, PetDexVirtualRow::LegacyRow { .. }))
                    .unwrap_or(true),
            |this| this.mb(px(12.0)),
        )
    }
}

fn pet_dex_virtual_rows(
    bundled_items: Vec<PetCatalogItem>,
    unlocked_species: &HashSet<String>,
    custom_pets: Vec<PetCustomPet>,
    legacy_records: Vec<PetLegacyRecord>,
    runtime_asset_root: &Path,
    support_dir: &Path,
    sprite_paths: &HashMap<String, ImageSource>,
    language: &str,
    window: &mut Window,
) -> Vec<PetDexVirtualRow> {
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
                record,
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

fn pet_dex_virtual_card(card: PetDexCard, cx: &mut Context<CoduxApp>) -> AnyElement {
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

fn pet_dex_empty_state(message: String, cx: &mut Context<CoduxApp>) -> impl IntoElement {
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

fn pet_legacy_row(
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
        .rounded(px(9.0))
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
                .rounded(px(9.0))
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

fn pet_cancel_button(id: &'static str, language: &str, cx: &mut Context<CoduxApp>) -> Button {
    Button::new(id)
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .child(pet_button_label(
            pet_catalog_text(language, "common.cancel", "Cancel"),
            cx.theme().secondary_foreground,
        ))
        .on_click(|_, window, _| window.remove_window())
}

fn pet_footer_bar(footer: impl IntoElement) -> impl IntoElement {
    div()
        .h(px(54.0))
        .flex_shrink_0()
        .border_t_1()
        .border_color(color(theme::BORDER_SOFT))
        .px(px(16.0))
        .flex()
        .items_center()
        .justify_end()
        .child(footer)
}

fn pet_dialog_footer(children: Vec<AnyElement>) -> impl IntoElement {
    DialogFooter::new().children(children)
}

fn pet_footer_button(
    id: &'static str,
    label: String,
    icon: HeroIconName,
    primary: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> Button {
    let button = Button::new(id)
        .text_color(if primary {
            cx.theme().primary_foreground
        } else {
            cx.theme().secondary_foreground
        })
        .icon(Icon::new(icon).size_3p5())
        .child(pet_button_label(
            label,
            if primary {
                cx.theme().primary_foreground
            } else {
                cx.theme().secondary_foreground
            },
        ))
        .on_click(cx.listener(on_click));
    if primary {
        button.primary()
    } else {
        button.secondary()
    }
}

fn pet_button_label(label: impl Into<SharedString>, text_color: Hsla) -> impl IntoElement {
    div()
        .text_size(rems(0.875))
        .line_height(rems(1.125))
        .text_color(text_color)
        .child(label.into())
}

fn pet_inline_button(
    id: &'static str,
    label: String,
    icon: HeroIconName,
    enabled: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> Button {
    Button::new(id)
        .compact()
        .primary()
        .disabled(!enabled)
        .text_color(cx.theme().primary_foreground)
        .w_full()
        .icon(Icon::new(icon).size_3p5())
        .child(
            div()
                .flex_none()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .child(SharedString::from(label)),
        )
        .on_click(cx.listener(on_click))
}

fn pet_dex_sidebar_action(action: AnyElement) -> impl IntoElement {
    div()
        .w_full()
        .h(px(36.0))
        .flex()
        .items_center()
        .child(action)
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
    _window: &mut Window,
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

fn pet_claim_preview(
    pet: &PetSummary,
    random: bool,
    runtime_asset_root: &Path,
    support_dir: &Path,
    custom_pets: &[PetCustomPet],
    catalog_item: Option<&PetCatalogItem>,
    language: &str,
    sprite_frame: usize,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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

fn pet_dex_current_card(
    pet: &PetSummary,
    custom_pet: Option<&PetCustomPet>,
    catalog_item: Option<&PetCatalogItem>,
    runtime_asset_root: &Path,
    support_dir: &Path,
    custom_pets: &[PetCustomPet],
    stats: &PetStats,
    total_xp: i64,
    claimed_at: Option<i64>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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

fn pet_trait_bar(emoji: &'static str, label: String, value: i64, accent: u32) -> impl IntoElement {
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

fn pet_dex_sidebar_overview(
    language: &str,
    current_name: String,
    current_level: String,
    archived_count: usize,
    archived_subtitle: String,
    unlocked_count: usize,
    total_count: usize,
    collection_subtitle: String,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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

fn pet_dex_separator() -> impl IntoElement {
    div()
        .my(px(16.0))
        .h(px(1.0))
        .w_full()
        .bg(color(theme::BORDER_SOFT).opacity(0.75))
}

fn pet_dex_summary_row(
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

fn legacy_pet_sprite_path(
    runtime_asset_root: &Path,
    support_dir: &Path,
    record: &PetLegacyRecord,
) -> ImageSource {
    if let Some(custom_pet) = record.custom_pet.as_ref() {
        return custom_pet_sprite_path(support_dir, custom_pet).into();
    }

    pet_sprite_path(
        runtime_asset_root,
        support_dir,
        &PetSummary {
            species: record.species.clone(),
            ..PetSummary::default()
        },
        &[],
    )
}

fn pet_date_label(timestamp: i64) -> String {
    use chrono::{Datelike, Local, TimeZone};

    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|date| format!("{}/{}/{}", date.year(), date.month(), date.day()))
        .unwrap_or_else(|| "Unknown date".to_string())
}

fn pet_accent_color(species: &str) -> u32 {
    match species {
        "voidcat" => 0x6A5CFF,
        "rusthound" => 0xFF8A3D,
        "goose" => 0x3E86F6,
        "chaossprite" => 0xFF4FA3,
        "code" => 0x2F8FFF,
        "sheep" => 0xF28FB8,
        "ox" => 0xF3B43F,
        "dragon" => 0xE04435,
        "phoenix" => 0xFF7A22,
        "dolphin" => 0x1E9BFF,
        "penguin" => 0x5C6D85,
        "panda" => 0x6A6F78,
        _ => theme::ACCENT,
    }
}

fn pet_dex_spotlight_overlay(
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

fn pet_dex_archive_confirm_overlay(language: &str, cx: &mut Context<CoduxApp>) -> AnyElement {
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
                .rounded(px(14.0))
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
                                    &language,
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
                            &language,
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
                                pet_catalog_text(&language, "common.cancel", "Cancel"),
                                cx,
                                |app, _event, _window, cx| app.close_pet_dex_spotlight(cx),
                            )
                            .compact(),
                        )
                        .child(
                            dialog_primary_button(
                                "pet-dex-confirm-archive",
                                pet_catalog_text(
                                    &language,
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

fn pet_section_header(label: String, trailing: Option<String>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(rems(1.0))
                .line_height(rems(1.25))
                .font_weight(FontWeight::BOLD)
                .child(label),
        )
        .when_some(trailing, |this, trailing| {
            this.child(
                div()
                    .flex_none()
                    .text_size(rems(0.75))
                    .line_height(rems(1.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color(theme::TEXT_MUTED))
                    .child(trailing),
            )
        })
}

fn pet_catalog_text(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
}

fn pet_format_placeholders(template: &str, values: &[String]) -> String {
    let mut output = template.to_string();
    for value in values {
        output = output.replacen("%@", value, 1);
    }
    output
}

fn pet_species_name(species: &str) -> String {
    match species.strip_prefix("custom:").unwrap_or(species) {
        "voidcat" => "Voidcat",
        "rusthound" => "Rusthound",
        "goose" => "Goose",
        "chaossprite" => "Chaos Sprite",
        "code" => "Code",
        "sheep" => "Sheep",
        "ox" => "Ox",
        "dragon" => "Dragon",
        "phoenix" => "Phoenix",
        "dolphin" => "Dolphin",
        "penguin" => "Penguin",
        "panda" => "Panda",
        value if value.is_empty() => "Voidcat",
        value => value,
    }
    .to_string()
}

fn pet_species_subtitle(species: &str) -> String {
    match species.strip_prefix("custom:").unwrap_or(species) {
        "voidcat" => "Quietly watches code changes",
        "rusthound" => "Likes Rust and compiler feedback",
        "goose" => "Keeps an eye on task rhythm",
        "chaossprite" => "Best for fast experiments",
        "code" => "Default coding companion",
        "sheep" => "Gentle long-running companion",
        "ox" => "Steady task mover",
        "dragon" => "Built for refactors and sprints",
        "phoenix" => "Good for recovery and review",
        "dolphin" => "Good for collaboration and exploration",
        "penguin" => "Good for terminal workflows",
        "panda" => "Good for quiet maintenance",
        _ => "Codux pet companion",
    }
    .to_string()
}
