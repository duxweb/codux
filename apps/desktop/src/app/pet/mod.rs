use super::*;
use crate::app::app_events::PetUpdateEvent;
use crate::app::app_state::{PetLevelUpFx, PetRecalibrationFx};
use crate::app::ui_helpers::with_codux_tooltip;
use codux_runtime::pet::{PetCatalog, PetCatalogItem, PetLegacyRecord, PetStats};
use gpui::{Hsla, ListSizingBehavior};
use gpui_component::{
    dialog::DialogFooter,
    input::{Input, InputState},
};
use std::{ops::Range, rc::Rc, time::Duration};

use crate::app::workspace_pet_widgets::{WorkspacePetInstallInput, workspace_pet_install_form};

mod catalog;
mod claim;
mod dex;
mod level_up;
mod recalibration;
mod widgets;

use catalog::*;
use widgets::*;

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
        let previous_level = self.state.pet.level;
        self.state.pet = self.runtime_service.reload_pet();
        self.note_pet_level_transition(previous_level);
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
                        let previous_level = app.state.pet.level;
                        app.state.pet = pet;
                        app.note_pet_level_transition(previous_level);
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
}
