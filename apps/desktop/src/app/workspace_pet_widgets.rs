use super::*;
use crate::app::{
    ui_helpers::{codux_tooltip_container, with_codux_tooltip},
    workspace_shared::{
        workspace_header_badge_button_content, workspace_header_button, workspace_i18n,
    },
};
use gpui::{Anchor, relative};
use gpui_component::{
    input::{Input, InputEvent, InputState},
    popover::Popover,
};

pub(in crate::app) fn workspace_pet_button(
    pet: &PetSummary,
    pet_snapshot: Option<&PetSnapshot>,
    custom_pets: &[PetCustomPet],
    runtime_asset_root: &std::path::Path,
    support_dir: &std::path::Path,
    language: &str,
    _install_url: &str,
    _install_display_name: &str,
    _install_preview: Option<&PetCustomPetInstallPreview>,
    _install_error: Option<&str>,
    _install_previewing: bool,
    _installing: bool,
    pet_name_editing: bool,
    pet_sprite_frame: usize,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let app_entity = cx.entity();
    let pet = pet.clone();
    let language = language.to_string();
    let pet_snapshot = pet_snapshot.cloned();
    let custom_pets = custom_pets.to_vec();
    let pet_sprite_path = pet_sprite_path(runtime_asset_root, support_dir, &pet, &custom_pets);
    let label = if pet.claimed {
        format!("Lv.{}", pet.level.max(1))
    } else {
        workspace_i18n(&language, "pet.claim.action", "Claim Pet")
    };
    let trigger = workspace_header_button("workspace-pet", cx)
        .secondary()
        .text_color(cx.theme().foreground)
        .child(workspace_header_badge_button_content(
            HeroIconName::Heart,
            color(0x7C4DFF),
            label,
            cx,
        ));

    if !pet.claimed {
        return trigger
            .on_click(cx.listener(|app, _event, window, cx| {
                app.defer_open_pet_claim_window(window, cx);
            }))
            .into_any_element();
    }

    let content = workspace_pet_popover_content(
        pet.clone(),
        pet_snapshot,
        pet_sprite_path,
        pet_name_editing,
        pet_sprite_frame,
        language.clone(),
        app_entity.clone(),
        window,
        cx,
    );

    Popover::new("workspace-pet-popover")
        .anchor(Anchor::TopRight)
        .appearance(false)
        .w(px(324.0))
        .trigger(trigger)
        .child(content)
        .into_any_element()
}
fn workspace_pet_dex_button(
    dex_tooltip: SharedString,
    app_entity: gpui::Entity<CoduxApp>,
) -> impl IntoElement {
    codux_tooltip_container(app_entity.clone(), "workspace-pet-dex-tooltip", dex_tooltip)
        .absolute()
        .right(px(10.0))
        .top(px(10.0))
        .child(
            Button::new("workspace-pet-dex-open")
                .compact()
                .ghost()
                .icon(Icon::new(HeroIconName::BookOpen).size_3p5())
                .on_click(move |_, window, cx| {
                    cx.update_entity(&app_entity, |app, cx| {
                        app.defer_open_pet_dex_window(window, cx);
                    });
                }),
        )
}

fn workspace_pet_rename_action_button(
    id: &'static str,
    icon: HeroIconName,
    tooltip: SharedString,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    with_codux_tooltip(
        cx.entity(),
        format!("pet-rename-tooltip-{id}"),
        Button::new(id)
            .compact()
            .ghost()
            .text_color(cx.theme().secondary_foreground)
            .icon(
                Icon::new(icon)
                    .size_3p5()
                    .text_color(cx.theme().secondary_foreground),
            )
            .on_click(cx.listener(on_click)),
        tooltip,
    )
}

fn workspace_pet_install_action_button(
    button: Button,
    tooltip: SharedString,
    label: SharedString,
    icon: HeroIconName,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    with_codux_tooltip(
        cx.entity(),
        format!("pet-install-tooltip-{tooltip}"),
        button
            .text_color(cx.theme().secondary_foreground)
            .icon(
                Icon::new(icon)
                    .size_3p5()
                    .text_color(cx.theme().secondary_foreground),
            )
            .child(workspace_pet_install_button_label(label))
            .on_click(cx.listener(on_click)),
        tooltip,
    )
}

fn workspace_pet_popover_content(
    pet: PetSummary,
    pet_snapshot: Option<PetSnapshot>,
    pet_sprite_path: ImageSource,
    pet_name_editing: bool,
    _pet_sprite_frame: usize,
    language: String,
    app_entity: gpui::Entity<CoduxApp>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let name = if pet.claimed && !pet.display_name.is_empty() {
        pet.display_name.clone()
    } else {
        workspace_i18n(&language, "pet.unclaimed", "No pet claimed")
    };
    let species_name = pet_snapshot
        .as_ref()
        .and_then(|snapshot| {
            snapshot
                .custom_pet
                .as_ref()
                .map(|pet| pet.display_name.clone())
        })
        .unwrap_or_else(|| workspace_pet_species_name(&pet.species, &language));
    let subtitle = if pet.custom_name.trim().is_empty() {
        None
    } else {
        Some(species_name.clone())
    };
    let sprite_fallback_color = cx.theme().primary;
    let progress = pet_snapshot
        .as_ref()
        .map(|snapshot| snapshot.progress.clone())
        .unwrap_or_else(|| codux_runtime::pet::PetProgressInfo {
            level: pet.level.max(1),
            xp_in_level: 0,
            xp_for_level: 0,
            total_xp: pet.total_xp.max(0),
            progress: pet.progress,
            is_at_max_level: false,
        });
    let stats = pet_snapshot
        .as_ref()
        .map(|snapshot| snapshot.current_stats.clone())
        .unwrap_or_default();
    let persona = pet_snapshot
        .as_ref()
        .map(|snapshot| snapshot.persona_id.clone())
        .unwrap_or_else(|| "observer".to_string());
    let persona_label = pet_persona_label(&persona, &language);
    let dex_tooltip = workspace_i18n(&language, "pet.dex.open", "Open Dex");
    let xp_label = workspace_i18n(&language, "pet.xp.label", "Experience");
    let stats_title = workspace_i18n(&language, "pet.stats.title", "Traits");
    let total_xp_label = workspace_i18n(&language, "pet.total_xp", "Total XP");
    let wisdom_label = workspace_i18n(&language, "pet.attribute.wisdom", "Wisdom");
    let chaos_label = workspace_i18n(&language, "pet.attribute.chaos", "Chaos");
    let night_label = workspace_i18n(&language, "pet.attribute.night", "Night");
    let stamina_label = workspace_i18n(&language, "pet.attribute.stamina", "Stamina");
    let empathy_label = workspace_i18n(&language, "pet.attribute.empathy", "Empathy");
    let trait_label_width = pet_trait_label_width([
        &wisdom_label,
        &chaos_label,
        &night_label,
        &stamina_label,
        &empathy_label,
    ]);

    div()
        .flex()
        .flex_col()
        .rounded(px(12.0))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().popover)
        .shadow_lg()
        .text_color(color(theme::TEXT))
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                .items_center()
                .p(px(10.0))
                .child(
                    div()
                        .size(px(104.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(pet_sprite_element(
                            pet_sprite_path,
                            104.0,
                            0,
                            0,
                            sprite_fallback_color,
                        )),
                )
                .child(workspace_pet_dex_button(
                    dex_tooltip.into(),
                    app_entity.clone(),
                ))
                .child(workspace_pet_name_row(
                    pet.clone(),
                    name,
                    subtitle,
                    pet_name_editing,
                    &language,
                    window,
                    cx,
                ))
                .child(
                    div()
                        .mt(px(8.0))
                        .rounded_full()
                        .bg(color(theme::ACCENT).opacity(0.14))
                        .px(px(10.0))
                        .py(px(4.0))
                        .text_size(rems(0.75))
                        .line_height(rems(0.75))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::ACCENT))
                        .child(persona_label),
                )
                .child(
                    div()
                        .mt(px(10.0))
                        .text_size(rems(1.625))
                        .line_height(rems(2.0))
                        .font_weight(FontWeight::BLACK)
                        .child(format!("Lv.{}", progress.level.max(1))),
                ),
        )
        .child(workspace_popover_separator())
        .child(div().p(px(10.0)).child(workspace_pet_meter(
            xp_label,
            format!(
                "{} / {}",
                compact_number(progress.xp_in_level),
                compact_number(progress.xp_for_level)
            ),
            progress.progress,
            theme::ACCENT,
        )))
        .child(workspace_popover_separator())
        .child(
            div()
                .p(px(10.0))
                .child(
                    div()
                        .mb(px(6.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::TEXT_MUTED))
                        .child(stats_title),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(workspace_pet_trait(
                            app_entity.clone(),
                            "brain",
                            wisdom_label,
                            stats.wisdom,
                            0x2F8FFF,
                            trait_label_width,
                            workspace_i18n(
                                &language,
                                "pet.attribute.wisdom.help",
                                "Reflects deeper, denser sessions with more substantial exchanges.",
                            ),
                        ))
                        .child(workspace_pet_trait(
                            app_entity.clone(),
                            "flame",
                            chaos_label,
                            stats.chaos,
                            0xFF6030,
                            trait_label_width,
                            workspace_i18n(
                                &language,
                                "pet.attribute.chaos.help",
                                "Reflects fast, jumpy, high-tempo sessions with frequent bursts.",
                            ),
                        ))
                        .child(workspace_pet_trait(
                            app_entity.clone(),
                            "moon",
                            night_label,
                            stats.night,
                            0x6060CC,
                            trait_label_width,
                            workspace_i18n(
                                &language,
                                "pet.attribute.night.help",
                                "Reflects how much of your recent activity leans into late-night hours.",
                            ),
                        ))
                        .child(workspace_pet_trait(
                            app_entity.clone(),
                            "arm",
                            stamina_label,
                            stats.stamina,
                            0x20A060,
                            trait_label_width,
                            workspace_i18n(
                                &language,
                                "pet.attribute.stamina.help",
                                "Reflects steadier sessions that hold focus across more sustained back-and-forth.",
                            ),
                        ))
                        .child(workspace_pet_trait(
                            app_entity.clone(),
                            "bandage",
                            empathy_label,
                            stats.empathy,
                            0xE060A0,
                            trait_label_width,
                            workspace_i18n(
                                &language,
                                "pet.attribute.empathy.help",
                                "Reflects patient repair work, iterative debugging, and careful refinement.",
                            ),
                        )),
                ),
        )
        .child(workspace_popover_separator())
        .child(
            div()
                .p(px(10.0))
                .text_center()
                .child(
                    div()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::TEXT_MUTED))
                        .child(total_xp_label),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(rems(0.8125))
                        .line_height(rems(1.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(compact_number(progress.total_xp)),
                ),
        )
        .when_some(pet.error, |this, error| {
            this.child(
                div()
                    .p(px(10.0))
                    .child(workspace_popover_notice(error)),
            )
        })
}

fn workspace_popover_separator() -> impl IntoElement {
    div().mx(px(10.0)).h(px(1.0)).bg(color(theme::BORDER_SOFT))
}

fn workspace_pet_meter(
    label: String,
    value: String,
    progress: f64,
    accent: u32,
) -> impl IntoElement {
    div()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::TEXT_MUTED))
                        .child(label),
                )
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::TEXT_DIM))
                        .child(value),
                ),
        )
        .child(
            div()
                .mt(px(6.0))
                .h(px(7.0))
                .rounded_full()
                .overflow_hidden()
                .bg(color(accent).opacity(0.15))
                .child(
                    div()
                        .h_full()
                        .w(relative(progress.clamp(0.0, 1.0) as f32))
                        .rounded_full()
                        .bg(color(accent)),
                ),
        )
}

fn workspace_pet_trait(
    app_entity: gpui::Entity<CoduxApp>,
    emoji_kind: &'static str,
    label: String,
    value: i64,
    accent: u32,
    label_width: f32,
    help: String,
) -> impl IntoElement {
    let ratio = (value as f32 / 330.0).clamp(0.0, 1.0);
    codux_tooltip_container(
        app_entity,
        SharedString::from(format!("pet-trait-{emoji_kind}")),
        help,
    )
    .flex()
    .items_center()
    .gap(px(8.0))
    .text_size(rems(0.75))
    .line_height(rems(1.0))
    .child(pet_trait_emoji(emoji_kind))
    .child(
        div()
            .w(px(label_width))
            .flex_none()
            .text_color(color(theme::TEXT_MUTED))
            .font_weight(FontWeight::MEDIUM)
            .truncate()
            .child(label),
    )
    .child(
        div()
            .flex_1()
            .min_w(px(0.0))
            .h(px(5.0))
            .rounded_full()
            .overflow_hidden()
            .bg(color(accent).opacity(0.12))
            .child(
                div()
                    .h_full()
                    .w(relative(ratio))
                    .rounded_full()
                    .bg(color(accent).opacity(0.75)),
            ),
    )
    .child(
        div()
            .w(px(34.0))
            .flex_none()
            .text_right()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(color(theme::TEXT_DIM))
            .child(compact_number(value)),
    )
}

fn pet_trait_label_width<'a>(labels: impl IntoIterator<Item = &'a String>) -> f32 {
    let max_units = labels
        .into_iter()
        .map(|label| {
            label
                .chars()
                .map(|ch| if ch.is_ascii() { 0.58 } else { 1.0 })
                .sum::<f32>()
        })
        .fold(0.0, f32::max);
    (max_units * 12.0).ceil().clamp(32.0, 76.0)
}

fn pet_trait_emoji(kind: &'static str) -> impl IntoElement {
    let emoji = match kind {
        "brain" => "🧠",
        "flame" => "🔥",
        "moon" => "🌙",
        "arm" => "💪",
        "bandage" => "🩹",
        _ => "",
    };
    div()
        .w(px(16.0))
        .text_center()
        .text_size(rems(0.75))
        .line_height(rems(0.75))
        .child(emoji)
}

fn pet_persona_label(persona: &str, language: &str) -> String {
    let fallback = match persona {
        "observer" => "Observer",
        "sprinter" => "Sprinter",
        "guardian" => "Guardian",
        "nightowl" => "Night Owl",
        "maker" => "Maker",
        value => value,
    };
    workspace_i18n(language, &format!("pet.persona.{persona}"), fallback)
}

fn workspace_pet_species_name(species: &str, language: &str) -> String {
    match species.strip_prefix("custom:") {
        Some(id) if !id.trim().is_empty() => id.to_string(),
        _ => {
            let fallback = match species {
                "voidcat" => "Voidcat",
                "fox" => "Fox",
                "panda" => "Panda",
                "otter" => "Otter",
                "owl" => "Owl",
                "dragon" => "Dragon",
                value if !value.trim().is_empty() => value,
                _ => "Pet",
            };
            workspace_i18n(language, &format!("pet.species.{species}.base"), fallback)
        }
    }
}

fn workspace_pet_name_row(
    pet: PetSummary,
    name: String,
    subtitle: Option<String>,
    editing: bool,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    if !editing {
        return div()
            .mt(px(12.0))
            .flex()
            .items_baseline()
            .justify_center()
            .gap_1()
            .min_w_0()
            .child(
                div()
                    .id("pet-name-edit-trigger")
                    .cursor_pointer()
                    .text_size(rems(1.0625))
                    .line_height(rems(1.375))
                    .font_weight(FontWeight::BOLD)
                    .truncate()
                    .on_click(cx.listener(|app, _event, window, cx| {
                        app.start_current_pet_rename(window, cx)
                    }))
                    .child(name),
            )
            .when_some(subtitle, |this, subtitle| {
                this.child(
                    div()
                        .max_w(px(92.0))
                        .truncate()
                        .text_size(rems(0.875))
                        .line_height(rems(1.25))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::TEXT_MUTED))
                        .child(subtitle),
                )
            })
            .into_any_element();
    }

    let value = pet.custom_name.clone();
    let placeholder = workspace_i18n(language, "pet.name.placeholder", "Pet Name");
    let name_state = window.use_keyed_state("pet-rename-custom-name", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .placeholder(placeholder)
    });
    name_state.update(cx, |state, cx| {
        if state.value().as_ref() != pet.custom_name {
            state.set_value(pet.custom_name.clone(), window, cx);
        }
    });
    cx.subscribe_in(&name_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::PressEnter { .. }) {
            app.rename_current_pet_to(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();
    let save_state = name_state.clone();

    div()
        .mt(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .gap_1()
        .child(
            div()
                .w(px(150.0))
                .child(Input::new(&name_state).with_size(gpui_component::Size::Small)),
        )
        .child(workspace_pet_rename_action_button(
            "pet-rename-current",
            HeroIconName::Check,
            workspace_i18n(&language, "pet.name.save", "Save pet name").into(),
            cx,
            move |app, _event, window, cx| {
                let custom_name = save_state.read(cx).value().to_string();
                app.rename_current_pet_to(custom_name, window, cx)
            },
        ))
        .child(workspace_pet_rename_action_button(
            "pet-rename-cancel",
            HeroIconName::XMark,
            workspace_i18n(&language, "common.cancel", "Cancel").into(),
            cx,
            |app, _event, window, cx| app.cancel_current_pet_rename(window, cx),
        ))
        .into_any_element()
}

pub(in crate::app) fn workspace_pet_install_form(
    install_url: &str,
    install_display_name: &str,
    install_preview: Option<&PetCustomPetInstallPreview>,
    install_error: Option<&str>,
    install_previewing: bool,
    installing: bool,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let url_value = install_url.to_string();
    let name_value = install_display_name.to_string();
    let url_placeholder = workspace_i18n(
        language,
        "pet.custom.install.url.placeholder",
        "https://petdex.crafter.run/zh/pets/boba",
    );
    let name_placeholder = workspace_i18n(language, "pet.custom.install.name.label", "Pet Name");
    let url_state = window.use_keyed_state("pet-install-url", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(url_value.clone())
            .placeholder(url_placeholder.clone())
    });
    url_state.update(cx, |state, cx| {
        if state.value().as_ref() != install_url {
            state.set_value(install_url.to_string(), window, cx);
        }
    });
    cx.subscribe_in(&url_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_pet_install_url(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    let name_state = window.use_keyed_state("pet-install-display-name", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(name_value.clone())
            .placeholder(name_placeholder.clone())
    });
    name_state.update(cx, |state, cx| {
        if state.value().as_ref() != install_display_name {
            state.set_value(install_display_name.to_string(), window, cx);
        }
    });
    cx.subscribe_in(&name_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_pet_install_display_name(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .text_size(rems(0.875))
        .line_height(rems(1.125))
        .child(
            div()
                .rounded(px(8.0))
                .bg(cx.theme().group_box)
                .p(px(14.0))
                .child(
                    div()
                        .mb(px(8.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(workspace_i18n(
                            &language,
                            "pet.custom.install.url.label",
                            "Petdex Page URL",
                        )),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().flex_1().min_w_0().child(
                                Input::new(&url_state).with_size(gpui_component::Size::Medium),
                            ),
                        )
                        .child(workspace_pet_install_action_button(
                            Button::new("pet-custom-market").ghost(),
                            workspace_i18n(
                                &language,
                                "pet.custom.market.title",
                                "Petdex Marketplace",
                            )
                            .into(),
                            workspace_i18n(&language, "pet.custom.market.action", "Get Pets")
                                .into(),
                            HeroIconName::ArrowTopRightOnSquare,
                            cx,
                            |app, _event, window, cx| app.open_pet_market(window, cx),
                        ))
                        .child(workspace_pet_install_action_button(
                            Button::new("pet-preview-custom")
                                .secondary()
                                .loading(install_previewing)
                                .disabled(
                                    install_url.trim().is_empty()
                                        || install_previewing
                                        || installing,
                                ),
                            workspace_i18n(
                                &language,
                                "pet.custom.install.preview.label",
                                "Pet Preview",
                            )
                            .into(),
                            if install_previewing {
                                workspace_i18n(
                                    &language,
                                    "pet.custom.install.resolving",
                                    "Reading Petdex page...",
                                )
                                .into()
                            } else if install_preview.is_some() {
                                workspace_i18n(
                                    &language,
                                    "pet.custom.install.resolve_again",
                                    "Parse Again",
                                )
                                .into()
                            } else {
                                workspace_i18n(&language, "pet.custom.install.resolve", "Parse")
                                    .into()
                            },
                            HeroIconName::Eye,
                            cx,
                            |app, _event, window, cx| app.preview_custom_pet_install(window, cx),
                        )),
                ),
        )
        .when_some(install_preview.cloned(), |this, preview| {
            this.child(workspace_pet_install_preview(
                preview,
                &name_state,
                installing,
                &language,
                cx,
            ))
        })
        .when(installing, |this| {
            this.child(
                div()
                    .rounded(px(8.0))
                    .bg(color(theme::ACCENT).opacity(0.1))
                    .px(px(12.0))
                    .py(px(8.0))
                    .text_size(rems(0.75))
                    .line_height(rems(1.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color(theme::ACCENT))
                    .child(workspace_i18n(
                        &language,
                        "pet.custom.install.installing.detail",
                        "Downloading, unpacking, and validating the pet package.",
                    )),
            )
        })
        .when_some(install_error.map(str::to_string), |this, error| {
            this.child(workspace_pet_install_error(error))
        })
}

fn workspace_pet_install_preview(
    preview: PetCustomPetInstallPreview,
    name_state: &gpui::Entity<InputState>,
    installing: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let image = if let Some(path) = preview
        .local_image_path
        .as_ref()
        .filter(|path| !path.trim().is_empty())
    {
        img(PathBuf::from(path))
            .size_full()
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| workspace_pet_install_preview_fallback())
            .into_any_element()
    } else if let Some(url) = preview
        .image_url
        .as_ref()
        .filter(|url| !url.trim().is_empty())
    {
        img(url.clone())
            .size_full()
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| workspace_pet_install_preview_fallback())
            .into_any_element()
    } else {
        workspace_pet_install_preview_fallback()
    };

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(color(theme::BORDER_SOFT))
        .bg(cx.theme().group_box)
        .p(px(14.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(14.0))
                .child(
                    div()
                        .size(px(104.0))
                        .rounded(px(10.0))
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(color(theme::ACCENT).opacity(0.1))
                        .child(image),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .child(preview.display_name.clone()),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.25))
                                .text_color(color(theme::TEXT_MUTED))
                                .child(empty_label(&preview.description)),
                        )
                        .child(
                            div()
                                .mt(px(8.0))
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(color(theme::TEXT_DIM))
                                .child(Icon::new(HeroIconName::ArrowTopRightOnSquare).size_3())
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .child(pet_install_host_label(&preview.page_url)),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(workspace_i18n(
                            language,
                            "pet.custom.install.name.label",
                            "Pet Name",
                        )),
                )
                .child(
                    Input::new(name_state)
                        .with_size(gpui_component::Size::Medium)
                        .disabled(installing),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(7.0))
                .child(workspace_pet_install_check(workspace_i18n(
                    language,
                    "pet.custom.install.validation.page",
                    "Petdex page verified",
                )))
                .child(workspace_pet_install_check(workspace_i18n(
                    language,
                    "pet.custom.install.validation.package",
                    "Package link found",
                )))
                .child(workspace_pet_install_check(workspace_i18n(
                    language,
                    "pet.custom.install.validation.format",
                    "Codex-format check runs during install",
                ))),
        )
}

fn workspace_pet_install_button_label(label: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_size(rems(0.875))
        .line_height(rems(1.125))
        .child(label.into())
}

fn workspace_pet_install_preview_fallback() -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::new(HeroIconName::InformationCircle)
                .size_8()
                .text_color(color(theme::ACCENT)),
        )
        .into_any_element()
}

fn workspace_pet_install_check(text: String) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(color(theme::TEXT_MUTED))
        .child(
            Icon::new(HeroIconName::CheckCircle)
                .size_3p5()
                .text_color(color(theme::GREEN)),
        )
        .child(text)
}

fn workspace_pet_install_error(error: String) -> impl IntoElement {
    div()
        .rounded(px(8.0))
        .bg(color(theme::ORANGE).opacity(0.12))
        .px(px(12.0))
        .py(px(8.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color(theme::ORANGE))
        .child(error)
}

fn pet_install_host_label(page_url: &str) -> String {
    let trimmed = page_url.trim();
    trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed)
        .split('/')
        .next()
        .filter(|host| !host.trim().is_empty())
        .unwrap_or("petdex.crafter.run")
        .to_string()
}

fn workspace_popover_notice(message: String) -> impl IntoElement {
    div()
        .rounded(px(6.0))
        .bg(color(theme::ORANGE).opacity(0.12))
        .px_2()
        .py_1()
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(color(theme::ORANGE))
        .child(message)
}
