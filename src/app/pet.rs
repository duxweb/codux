use super::*;
use codux_runtime::pet::{PetCatalog, PetCatalogItem, PetLegacyRecord, PetSnapshot, PetStats};
use gpui_component::input::{Input, InputState};

use crate::app::workspace::workspace_pet_install_form;

impl CoduxApp {
    pub(in crate::app) fn open_pet_claim_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_pet_window(
            AppWindowMode::PetClaim,
            "Claim Pet",
            size(px(680.0), px(500.0)),
            size(px(640.0), px(460.0)),
            cx,
        );
    }

    pub(in crate::app) fn open_pet_custom_install_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_pet_window(
            AppWindowMode::PetCustomInstall,
            "Add Custom Pet",
            size(px(680.0), px(320.0)),
            size(px(620.0), px(240.0)),
            cx,
        );
    }

    pub(in crate::app) fn open_pet_dex_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let bounds = Bounds::centered(None, window_size, cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(title.into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(min_size),
                is_resizable: mode == AppWindowMode::PetDex,
                ..Default::default()
            },
            move |window, cx| {
                let app = CoduxApp::new_pet_window(mode);
                theme::apply_component_theme_for_name(&app.state.settings.theme, Some(window), cx);
                let view = cx.new(|_| app);
                view.update(cx, |app, cx| {
                    app.start_pet_window_sync_loop(cx);
                    app.start_pet_sprite_animation_loop(cx);
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(_) => format!("{title} window opened"),
            Err(error) => format!("failed to open {title} window: {error}"),
        };
        cx.notify();
    }

    pub(in crate::app) fn start_pet_window_sync_loop(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.window_mode,
            AppWindowMode::PetClaim | AppWindowMode::PetCustomInstall | AppWindowMode::PetDex
        ) {
            return;
        }

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                let _ = codux_runtime::async_runtime::spawn_blocking(|| {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                })
                .await;

                if this
                    .update(cx, |app, cx| {
                        let settings_changed = app.apply_settings_update_event(cx);
                        let custom_changed = app.sync_pet_custom_install_event(cx);
                        let pet_changed = app.sync_pet_update_event(cx);
                        if settings_changed || custom_changed || pet_changed {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::app) fn sync_pet_custom_install_event(
        &mut self,
        _cx: &mut Context<Self>,
    ) -> bool {
        let event = current_pet_custom_install_event();
        self.apply_pet_custom_install_event(event)
    }

    pub(in crate::app) fn sync_pet_custom_install_event_for_activity_tick(&mut self) -> bool {
        let event = current_pet_custom_install_event();
        self.apply_pet_custom_install_event(event)
    }

    pub(in crate::app) fn sync_pet_update_event(&mut self, _cx: &mut Context<Self>) -> bool {
        let event = current_pet_update_event();
        self.apply_pet_update_event(event)
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
        self.state.pet = self.runtime_service.reload_pet();
        self.pet_custom_pets = self.runtime_service.pet_catalog().custom_pets;

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
        self.state.pet = self.runtime_service.reload_pet();
        self.pet_custom_pets = self.runtime_service.pet_catalog().custom_pets;
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
        self.sync_pet_custom_install_event(cx);
        self.sync_pet_update_event(cx);
        let catalog = self.runtime_service.pet_catalog();
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
        let claim_name_state = window.use_keyed_state("pet-claim-custom-name", cx, |window, cx| {
            InputState::new(window, cx).placeholder("留空则使用宠物名称")
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
        let language = self.state.settings.language.clone();
        let selected_catalog_item = catalog
            .species
            .iter()
            .find(|item| item.species == selected_species)
            .cloned();

        pet_window_shell(
            "领取宠物",
            "选择一个 Codux 伙伴，也可以先安装自定义宠物。",
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
                                .child(pet_claim_random_row(&selected_species, cx))
                                .when(!custom_pets.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .pt(px(6.0))
                                            .px_1()
                                            .text_size(px(12.0))
                                            .line_height(px(16.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(color(theme::TEXT_DIM))
                                            .child("自定义宠物"),
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
                            cx,
                        )))
                        .child(div().px(px(20.0)).pb(px(16.0)).child(
                            Input::new(&claim_name_state).with_size(gpui_component::Size::Medium),
                        )),
                ),
        )
        .child(pet_footer_bar(pet_window_footer(vec![
            pet_footer_button(
                "pet-claim-open-custom-install",
                "添加自定义",
                IconName::Plus,
                false,
                cx,
                |app, _event, window, cx| app.open_pet_custom_install_window(window, cx),
            )
            .into_any_element(),
            pet_footer_spacer().into_any_element(),
            pet_cancel_button("pet-claim-cancel", cx).into_any_element(),
            pet_footer_button(
                "pet-claim-confirm",
                "确认领取",
                IconName::Check,
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
        pet_window_shell(
            "添加自定义宠物",
            "粘贴 Petdex 页面，先解析预览，再安装到本地 runtime。",
            cx,
        )
        .child(
            div()
                .min_h_0()
                .flex_1()
                .p(px(16.0))
                .overflow_y_scrollbar()
                .child(workspace_pet_install_form(
                    &self.pet_install_url,
                    &self.pet_install_display_name,
                    self.pet_install_preview.as_ref(),
                    self.pet_install_error.as_deref(),
                    self.pet_install_previewing,
                    self.pet_installing,
                    window,
                    cx,
                )),
        )
        .child(pet_footer_bar(pet_window_footer(vec![
            pet_cancel_button("pet-custom-install-cancel", cx).into_any_element(),
            pet_footer_spacer().into_any_element(),
            pet_footer_button(
                "pet-custom-install-window",
                "安装",
                IconName::Plus,
                true,
                cx,
                |app, _event, window, cx| app.install_custom_pet(window, cx),
            )
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
        self.sync_pet_custom_install_event(cx);
        self.sync_pet_update_event(cx);
        let catalog = self.runtime_service.pet_catalog();
        let snapshot = self
            .runtime_service
            .pet_snapshot()
            .unwrap_or_else(|_| PetSnapshot::default());
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
        let total_count = catalog.species.len() + custom_pets.len();
        let unlocked_count = unlocked_species.len() + custom_pets.len();
        let current_name = if self.state.pet.claimed {
            self.state.pet.display_name.clone()
        } else {
            "未领取".to_string()
        };
        let current_level = if self.state.pet.claimed {
            format!("Lv.{}", snapshot.progress.level.max(1))
        } else {
            "未领取".to_string()
        };
        let archived_count = snapshot.legacy.len();
        let archived_subtitle = if archived_count == 0 {
            "暂无归档宠物"
        } else {
            "历史伙伴"
        };
        let collection_subtitle = if total_count > 0 && unlocked_count == total_count {
            "全部伙伴已解锁"
        } else {
            "继续探索"
        };
        let primary_action = if self.state.pet.claimed {
            pet_inline_button(
                "pet-dex-archive",
                "归档当前",
                IconName::Delete,
                true,
                cx,
                |app, _event, window, cx| app.archive_current_pet(window, cx),
            )
            .into_any_element()
        } else {
            pet_inline_button(
                "pet-dex-claim",
                "领取宠物",
                IconName::Heart,
                true,
                cx,
                |app, _event, window, cx| app.open_pet_claim_window(window, cx),
            )
            .into_any_element()
        };
        let add_custom_action = pet_inline_button(
            "pet-dex-open-custom-install",
            "添加自定义",
            IconName::Plus,
            true,
            cx,
            |app, _event, window, cx| app.open_pet_custom_install_window(window, cx),
        )
        .into_any_element();

        pet_window_shell(
            "宠物图鉴",
            "查看当前伙伴、已归档伙伴和已安装的自定义宠物。",
            cx,
        )
        .child(
            div()
                .min_h_0()
                .flex_1()
                .relative()
                .grid()
                .grid_cols(2)
                .overflow_hidden()
                .child(
                    div()
                        .min_h_0()
                        .border_r_1()
                        .border_color(color(theme::BORDER_SOFT))
                        .p(px(16.0))
                        .overflow_y_scrollbar()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(pet_dex_intro_card(
                            current_name,
                            current_level,
                            archived_count,
                            archived_subtitle,
                            unlocked_count,
                            total_count,
                            collection_subtitle,
                        ))
                        .child(pet_dex_current_card(
                            &self.state.pet,
                            current_custom_pet.as_ref(),
                            &self.runtime.source_root,
                            &self.state.support_dir,
                            &self.pet_custom_pets,
                            cx,
                        ))
                        .child(pet_stats_grid(
                            &snapshot.current_stats,
                            snapshot.progress.total_xp,
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(primary_action)
                                .child(add_custom_action),
                        )
                        .child(pet_legacy_section(snapshot.legacy, cx)),
                )
                .child(
                    div()
                        .min_h_0()
                        .p(px(16.0))
                        .overflow_y_scrollbar()
                        .flex()
                        .flex_col()
                        .gap(px(16.0))
                        .child(pet_catalog_section(
                            catalog.species.clone(),
                            &unlocked_species,
                            &self.state.settings.language,
                            cx,
                        ))
                        .child(pet_custom_section(
                            custom_pets,
                            self.state.support_dir.clone(),
                            cx,
                        )),
                ),
        )
        .when_some(self.pet_dex_spotlight.clone(), |this, spotlight| {
            this.child(pet_dex_spotlight_overlay(
                spotlight,
                &catalog,
                &self.runtime.source_root,
                &self.state.support_dir,
                &self.state.settings.language,
                cx,
            ))
        })
    }
}

fn pet_cancel_button(id: &'static str, cx: &mut Context<CoduxApp>) -> Button {
    Button::new(id)
        .compact()
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .label("取消")
        .on_click(|_, window, _| window.remove_window())
}

fn pet_window_shell(
    title: &'static str,
    subtitle: &'static str,
    cx: &mut Context<CoduxApp>,
) -> gpui::Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(color(theme::BG))
        .text_color(color(theme::TEXT))
        .child(
            div()
                .h(px(56.0))
                .flex_shrink_0()
                .px(px(18.0))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .child(
                    div()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .truncate()
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT_MUTED))
                                .child(subtitle),
                        ),
                )
                .child(
                    Button::new("pet-window-refresh")
                        .compact()
                        .ghost()
                        .tooltip("刷新宠物数据")
                        .text_color(cx.theme().secondary_foreground)
                        .icon(
                            Icon::new(IconName::Redo2)
                                .size_3p5()
                                .text_color(cx.theme().secondary_foreground),
                        )
                        .on_click(
                            cx.listener(|app, _event, window, cx| app.refresh_pet(window, cx)),
                        ),
                ),
        )
}

fn pet_footer_bar(footer: impl IntoElement) -> impl IntoElement {
    div()
        .h(px(54.0))
        .flex_shrink_0()
        .border_t_1()
        .border_color(color(theme::BORDER_SOFT))
        .px(px(14.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(footer)
}

fn pet_window_footer(children: Vec<AnyElement>) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .children(children)
}

fn pet_footer_spacer() -> impl IntoElement {
    div().flex_1()
}

fn pet_footer_button(
    id: &'static str,
    label: &'static str,
    icon: IconName,
    primary: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> Button {
    let button = Button::new(id)
        .compact()
        .text_color(if primary {
            cx.theme().primary_foreground
        } else {
            cx.theme().secondary_foreground
        })
        .icon(Icon::new(icon).size_3p5())
        .label(label)
        .on_click(cx.listener(on_click));
    if primary {
        button.primary()
    } else {
        button.secondary()
    }
}

fn pet_inline_button(
    id: &'static str,
    label: &'static str,
    icon: IconName,
    enabled: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> Button {
    Button::new(id)
        .compact()
        .secondary()
        .disabled(!enabled)
        .text_color(cx.theme().secondary_foreground)
        .icon(Icon::new(icon).size_3p5())
        .label(label)
        .on_click(cx.listener(on_click))
}

fn pet_claim_random_row(selected_species: &str, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    let selected = selected_species == "bundled:random";

    pet_select_row(
        SharedString::from("pet-claim-random"),
        selected,
        "随机".to_string(),
        "让 Codux 为你选择一个伙伴".to_string(),
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
        pet_claim_sprite_thumb(sprite_path, cx.theme().primary),
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
        pet_claim_sprite_thumb(sprite_path, cx.theme().primary),
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
            color(0xFFFFFF).opacity(0.035)
        })
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .child(leading)
        .child(
            div()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(17.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(12.0))
                        .line_height(px(15.0))
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
                    Icon::new(IconName::CircleCheck)
                        .size_3p5()
                        .text_color(color(theme::ACCENT))
                        .into_any_element()
                } else {
                    div().into_any_element()
                }),
        )
}

fn pet_claim_sprite_thumb(sprite_path: PathBuf, fallback_color: gpui::Hsla) -> AnyElement {
    div()
        .size(px(44.0))
        .rounded(px(8.0))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .bg(color(0xFFFFFF).opacity(0.04))
        .child(pet_sprite_element(sprite_path, 44.0, 0, fallback_color))
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
        .child(Icon::new(IconName::Asterisk).size_5().text_color(accent))
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
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let sprite_path = pet_sprite_path(runtime_asset_root, support_dir, pet, custom_pets);
    let title = if random {
        "随机".to_string()
    } else if pet.species.starts_with("custom:") {
        custom_pets
            .iter()
            .find(|custom| pet.species == format!("custom:{}", custom.id))
            .map(|custom| custom.display_name.clone())
            .unwrap_or_else(|| "自定义宠物".to_string())
    } else {
        catalog_item
            .map(|item| pet_catalog_text(language, &item.name_key, &pet_species_name(&pet.species)))
            .unwrap_or_else(|| pet_species_name(&pet.species))
    };
    let description = if random {
        pet_catalog_text(
            language,
            "pet.claim.random.description",
            "确认领取时会从内置宠物里随机选择一个伙伴。",
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
                    Icon::new(IconName::Asterisk)
                        .size_8()
                        .text_color(cx.theme().primary)
                        .into_any_element()
                } else {
                    pet_sprite_element(
                        sprite_path,
                        92.0,
                        cx.entity()
                            .read(cx)
                            .visible_pet_sprite_frame(PET_IDLE_FRAME_COUNT),
                        cx.theme().primary,
                    )
                }),
        )
        .child(
            div()
                .mt(px(14.0))
                .text_size(px(16.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::BOLD)
                .child(title),
        )
        .child(
            div()
                .mt(px(6.0))
                .max_w(px(340.0))
                .text_size(px(12.0))
                .line_height(px(18.0))
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
    runtime_asset_root: &Path,
    support_dir: &Path,
    custom_pets: &[PetCustomPet],
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let sprite_path = pet_sprite_path(runtime_asset_root, support_dir, pet, custom_pets);
    let name = if pet.claimed {
        pet.display_name.clone()
    } else {
        "还没有领取宠物".to_string()
    };
    let description = custom_pet
        .map(|pet| empty_label(&pet.description))
        .unwrap_or_else(|| pet_species_subtitle(&pet.species));

    div()
        .rounded(px(8.0))
        .bg(color(0xFFFFFF).opacity(0.055))
        .border_1()
        .border_color(color(theme::BORDER_SOFT))
        .p(px(12.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .size(px(54.0))
                .rounded(px(10.0))
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .bg(color(theme::ACCENT).opacity(0.12))
                .child(pet_sprite_element(sprite_path, 48.0, 0, cx.theme().primary)),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .truncate()
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name),
                )
                .child(
                    div()
                        .mt(px(3.0))
                        .truncate()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(description),
                ),
        )
        .child(
            Tag::info()
                .with_size(gpui_component::Size::Small)
                .child(format!("Lv.{}", pet.level.max(1))),
        )
}

fn pet_dex_intro_card(
    current_name: String,
    current_level: String,
    archived_count: usize,
    archived_subtitle: &'static str,
    unlocked_count: usize,
    total_count: usize,
    collection_subtitle: &'static str,
) -> impl IntoElement {
    div()
        .rounded(px(8.0))
        .bg(color(0xFFFFFF).opacity(0.055))
        .border_1()
        .border_color(color(theme::BORDER_SOFT))
        .p(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    Icon::new(IconName::BookOpen)
                        .size_3p5()
                        .text_color(color(theme::TEXT_MUTED)),
                )
                .child(
                    div()
                        .text_size(px(17.0))
                        .line_height(px(22.0))
                        .font_weight(FontWeight::BOLD)
                        .child("宠物图鉴"),
                ),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_MUTED))
                .child("记录你养成过的 Codux 伙伴。"),
        )
        .child(
            div()
                .mt(px(12.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(pet_dex_summary_row("当前伙伴", current_level, current_name))
                .child(pet_dex_summary_row(
                    "已归档",
                    archived_subtitle.to_string(),
                    archived_count.to_string(),
                ))
                .child(pet_dex_summary_row(
                    "图鉴收集",
                    collection_subtitle.to_string(),
                    format!("{unlocked_count}/{}", total_count.max(1)),
                )),
        )
}

fn pet_dex_summary_row(label: &'static str, subtitle: String, value: String) -> impl IntoElement {
    div()
        .rounded(px(7.0))
        .bg(color(0xFFFFFF).opacity(0.045))
        .px(px(10.0))
        .py(px(8.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .child(
            div()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT_MUTED))
                        .child(label),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .truncate()
                        .text_size(px(12.0))
                        .line_height(px(15.0))
                        .text_color(color(theme::TEXT_DIM))
                        .child(subtitle),
                ),
        )
        .child(
            div()
                .max_w(px(96.0))
                .truncate()
                .text_right()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::BOLD)
                .child(value),
        )
}

fn pet_stats_grid(stats: &PetStats, total_xp: i64) -> impl IntoElement {
    div()
        .grid()
        .grid_cols(3)
        .gap(px(8.0))
        .child(pet_stat_tile("总 XP", compact_number(total_xp)))
        .child(pet_stat_tile("智慧", stats.wisdom.to_string()))
        .child(pet_stat_tile("耐力", stats.stamina.to_string()))
        .child(pet_stat_tile("共情", stats.empathy.to_string()))
        .child(pet_stat_tile("夜行", stats.night.to_string()))
        .child(pet_stat_tile("混沌", stats.chaos.to_string()))
}

fn pet_stat_tile(label: &'static str, value: String) -> impl IntoElement {
    div()
        .rounded(px(7.0))
        .bg(color(0xFFFFFF).opacity(0.055))
        .px(px(10.0))
        .py(px(8.0))
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_DIM))
                .child(label),
        )
        .child(
            div()
                .mt(px(2.0))
                .text_size(px(16.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::BOLD)
                .child(value),
        )
}

fn pet_legacy_section(
    records: Vec<PetLegacyRecord>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .rounded(px(8.0))
        .bg(color(0xFFFFFF).opacity(0.04))
        .border_1()
        .border_color(color(theme::BORDER_SOFT))
        .overflow_hidden()
        .child(pet_section_header("归档记录", records.len()))
        .child(
            div()
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .when(records.is_empty(), |this| {
                    this.child(
                        div()
                            .px(px(6.0))
                            .py(px(8.0))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(color(theme::TEXT_DIM))
                            .child("暂无归档宠物"),
                    )
                })
                .children(records.into_iter().rev().map(|record| {
                    let legacy_id = record.id.clone();
                    div()
                        .rounded(px(6.0))
                        .px(px(7.0))
                        .py(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .hover(|style| style.bg(color(theme::BG_ROW_HOVER)))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(14.0))
                                        .line_height(px(18.0))
                                        .child(if record.custom_name.is_empty() {
                                            pet_species_name(&record.species)
                                        } else {
                                            record.custom_name
                                        }),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .line_height(px(15.0))
                                        .text_color(color(theme::TEXT_DIM))
                                        .child(format!(
                                            "Lv.{} · {}",
                                            record.progress.level, record.species
                                        )),
                                ),
                        )
                        .child(
                            Button::new(SharedString::from(format!(
                                "pet-restore-legacy-{legacy_id}"
                            )))
                            .compact()
                            .ghost()
                            .tooltip("恢复这个宠物")
                            .text_color(cx.theme().secondary_foreground)
                            .icon(Icon::new(IconName::Undo2).size_3p5())
                            .on_click(cx.listener(
                                move |app, _event, _window, cx| {
                                    match app.runtime_service.restore_archived_pet(
                                        PetRestoreRequest {
                                            legacy_id: legacy_id.clone(),
                                        },
                                    ) {
                                        Ok(_) => {
                                            app.state.pet = app.runtime_service.reload_pet();
                                            app.pet_custom_pets =
                                                app.runtime_service.pet_catalog().custom_pets;
                                            app.pet_dex_spotlight = None;
                                            let revision = publish_pet_update();
                                            if revision > 0 {
                                                app.pet_update_seen_revision = revision;
                                            }
                                            app.status_message = "pet restored".to_string();
                                        }
                                        Err(error) => {
                                            app.status_message =
                                                format!("failed to restore pet: {error}");
                                        }
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .into_any_element()
                })),
        )
}

fn pet_catalog_section(
    items: Vec<PetCatalogItem>,
    unlocked_species: &HashSet<String>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .rounded(px(8.0))
        .bg(color(0xFFFFFF).opacity(0.04))
        .border_1()
        .border_color(color(theme::BORDER_SOFT))
        .overflow_hidden()
        .child(pet_section_header("内置宠物", items.len()))
        .child(
            div()
                .p(px(10.0))
                .grid()
                .grid_cols(3)
                .gap(px(8.0))
                .children(items.into_iter().map(|item| {
                    let unlocked = unlocked_species.contains(&item.species);
                    let species = item.species.clone();
                    div()
                        .id(SharedString::from(format!("pet-dex-bundled-{species}")))
                        .cursor_pointer()
                        .rounded(px(7.0))
                        .bg(if unlocked {
                            color(0xFFFFFF).opacity(0.045)
                        } else {
                            color(0xFFFFFF).opacity(0.025)
                        })
                        .px(px(9.0))
                        .py(px(8.0))
                        .opacity(if unlocked { 1.0 } else { 0.72 })
                        .hover(|style| style.bg(color(theme::BG_ROW_HOVER)))
                        .on_click(cx.listener(move |app, _event, _window, cx| {
                            if unlocked {
                                app.show_pet_dex_spotlight(
                                    PetDexSpotlight::Bundled(species.clone()),
                                    cx,
                                );
                            }
                        }))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(if unlocked {
                                    pet_catalog_text(
                                        language,
                                        &item.name_key,
                                        &pet_species_name(&item.species),
                                    )
                                } else {
                                    "???".to_string()
                                }),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(12.0))
                                .line_height(px(15.0))
                                .truncate()
                                .text_color(color(theme::TEXT_DIM))
                                .child(if unlocked {
                                    "伙伴".to_string()
                                } else {
                                    "未解锁".to_string()
                                }),
                        )
                        .into_any_element()
                })),
        )
}

fn pet_custom_section(
    custom_pets: Vec<PetCustomPet>,
    support_dir: PathBuf,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .rounded(px(8.0))
        .bg(color(0xFFFFFF).opacity(0.04))
        .border_1()
        .border_color(color(theme::BORDER_SOFT))
        .overflow_hidden()
        .child(pet_section_header("自定义宠物", custom_pets.len()))
        .child(
            div()
                .p(px(10.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .when(custom_pets.is_empty(), |this| {
                    this.child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(color(theme::TEXT_DIM))
                            .child("还没有安装自定义宠物"),
                    )
                })
                .children(custom_pets.into_iter().map(|pet| {
                    let sprite_path = custom_pet_sprite_path(&support_dir, &pet);
                    let pet_id = pet.id.clone();
                    div()
                        .id(SharedString::from(format!("pet-dex-custom-{pet_id}")))
                        .cursor_pointer()
                        .rounded(px(7.0))
                        .px(px(7.0))
                        .py(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .hover(|style| style.bg(color(theme::BG_ROW_HOVER)))
                        .on_click(cx.listener(move |app, _event, _window, cx| {
                            app.show_pet_dex_spotlight(PetDexSpotlight::Custom(pet_id.clone()), cx);
                        }))
                        .child(
                            div()
                                .size(px(30.0))
                                .rounded(px(6.0))
                                .overflow_hidden()
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(color(0xFFFFFF).opacity(0.055))
                                .child(pet_sprite_element(
                                    sprite_path,
                                    28.0,
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
                                        .truncate()
                                        .text_size(px(14.0))
                                        .line_height(px(18.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(pet.display_name),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(12.0))
                                        .line_height(px(15.0))
                                        .text_color(color(theme::TEXT_DIM))
                                        .child(empty_label(&pet.description)),
                                ),
                        )
                        .into_any_element()
                })),
        )
}

fn pet_dex_spotlight_overlay(
    spotlight: PetDexSpotlight,
    catalog: &PetCatalog,
    runtime_asset_root: &Path,
    support_dir: &Path,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    if spotlight == PetDexSpotlight::ArchiveConfirm {
        return pet_dex_archive_confirm_overlay(cx);
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
                    "伙伴".to_string(),
                    pet_catalog_text(
                        language,
                        &item.description_key,
                        &pet_species_subtitle(&item.species),
                    ),
                    pet_sprite_path(runtime_asset_root, support_dir, &pet, &[]),
                    None,
                )
            }),
        PetDexSpotlight::Custom(custom_id) => catalog
            .custom_pets
            .iter()
            .find(|pet| pet.id == custom_id)
            .map(|pet| {
                (
                    pet.display_name.clone(),
                    "自定义宠物".to_string(),
                    empty_label(&pet.description),
                    custom_pet_sprite_path(support_dir, pet),
                    pet.source_page_url.clone(),
                )
            }),
        PetDexSpotlight::ArchiveConfirm => None,
    };

    let Some((title, subtitle, description, sprite_path, source_url)) = detail else {
        return div().into_any_element();
    };
    let source_url_for_click = source_url.clone();

    div()
        .absolute()
        .top(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .left(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(color(0x000000).opacity(0.35))
        .p(px(24.0))
        .child(
            div()
                .w(px(360.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(color(theme::BORDER_SOFT))
                .bg(color(theme::BG_PANEL))
                .p(px(20.0))
                .text_center()
                .shadow_lg()
                .child(
                    div()
                        .mx_auto()
                        .size(px(116.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(color(theme::ACCENT).opacity(0.12))
                        .child(pet_sprite_element(
                            sprite_path,
                            94.0,
                            cx.entity()
                                .read(cx)
                                .visible_pet_sprite_frame(PET_IDLE_FRAME_COUNT),
                            cx.theme().primary,
                        )),
                )
                .child(
                    div()
                        .mt(px(16.0))
                        .text_size(px(18.0))
                        .line_height(px(24.0))
                        .font_weight(FontWeight::BOLD)
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::ACCENT))
                        .child(subtitle),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .text_size(px(12.0))
                        .line_height(px(20.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(description),
                )
                .child(
                    div()
                        .mt(px(20.0))
                        .flex()
                        .justify_center()
                        .gap_2()
                        .when_some(source_url_for_click, |this, url| {
                            this.child(
                                Button::new("pet-dex-open-source")
                                    .compact()
                                    .secondary()
                                    .text_color(cx.theme().secondary_foreground)
                                    .label("打开来源")
                                    .on_click(cx.listener(move |app, _event, window, cx| {
                                        app.open_pet_source_url(url.clone(), window, cx)
                                    })),
                            )
                        })
                        .child(
                            Button::new("pet-dex-close-spotlight")
                                .compact()
                                .primary()
                                .text_color(cx.theme().primary_foreground)
                                .label("关闭")
                                .on_click(cx.listener(|app, _event, _window, cx| {
                                    app.close_pet_dex_spotlight(cx)
                                })),
                        ),
                ),
        )
        .into_any_element()
}

fn pet_dex_archive_confirm_overlay(cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .absolute()
        .top(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .left(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(color(0x000000).opacity(0.35))
        .p(px(24.0))
        .child(
            div()
                .w(px(360.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(color(theme::BORDER_SOFT))
                .bg(color(theme::BG_PANEL))
                .p(px(20.0))
                .shadow_lg()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Icon::new(IconName::Delete)
                                .size_4()
                                .text_color(color(theme::ORANGE)),
                        )
                        .child(
                            div()
                                .text_size(px(18.0))
                                .line_height(px(24.0))
                                .font_weight(FontWeight::BOLD)
                                .child("归档当前宠物"),
                        ),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .text_size(px(12.0))
                        .line_height(px(20.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child("将当前宠物归档到图鉴，然后选择新的伙伴。"),
                )
                .child(
                    div()
                        .mt(px(20.0))
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("pet-dex-cancel-archive")
                                .compact()
                                .ghost()
                                .text_color(cx.theme().secondary_foreground)
                                .label("取消")
                                .on_click(cx.listener(|app, _event, _window, cx| {
                                    app.close_pet_dex_spotlight(cx)
                                })),
                        )
                        .child(
                            Button::new("pet-dex-confirm-archive")
                                .compact()
                                .primary()
                                .text_color(cx.theme().primary_foreground)
                                .label("确认归档")
                                .on_click(cx.listener(|app, _event, window, cx| {
                                    app.archive_current_pet_confirmed(window, cx)
                                })),
                        ),
                ),
        )
        .into_any_element()
}

fn pet_section_header(label: &'static str, count: usize) -> impl IntoElement {
    div()
        .h(px(34.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .bg(color(0xFFFFFF).opacity(0.045))
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(label),
        )
        .child(
            div()
                .px(px(7.0))
                .py(px(1.0))
                .rounded(px(999.0))
                .bg(color(theme::ACCENT).opacity(0.16))
                .text_size(px(12.0))
                .line_height(px(15.0))
                .text_color(color(theme::ACCENT))
                .child(count.to_string()),
        )
}

fn pet_catalog_text(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
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
        "voidcat" => "安静观察代码变化",
        "rusthound" => "偏爱 Rust 和编译反馈",
        "goose" => "会盯住任务节奏",
        "chaossprite" => "适合高频试验",
        "code" => "默认编码伙伴",
        "sheep" => "温和的长线陪伴",
        "ox" => "稳定推进任务",
        "dragon" => "适合重构和冲刺",
        "phoenix" => "适合恢复和复盘",
        "dolphin" => "适合协作和探索",
        "penguin" => "适合终端工作流",
        "panda" => "适合安静维护",
        _ => "Codux 宠物伙伴",
    }
    .to_string()
}
