use super::*;

impl CoduxApp {
    pub(in crate::app) fn note_pet_recalibration(&mut self) {
        if self.window_mode != AppWindowMode::Main || !self.state.pet.claimed {
            return;
        }
        self.pet_recalibration = Some(PetRecalibrationFx { progress: 0.0 });
    }

    pub(in crate::app) fn ensure_pet_recalibration_ticker(&mut self, cx: &mut Context<Self>) {
        if self.pet_recalibration_ticking || self.pet_recalibration.is_none() {
            return;
        }
        self.pet_recalibration_ticking = true;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                timer.timer(Duration::from_millis(33)).await;
                let keep_going = this
                    .update(cx, |app, cx| {
                        let Some(fx) = app.pet_recalibration.as_mut() else {
                            app.pet_recalibration_ticking = false;
                            return false;
                        };
                        fx.progress += 0.018;
                        if fx.progress >= 1.0 {
                            app.pet_recalibration = None;
                            app.pet_recalibration_ticking = false;
                        }
                        app.invalidate_ui_region(cx, UiRegion::Root);
                        app.pet_recalibration.is_some()
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::app) fn pet_recalibration_overlay(
        &self,
        fx: &PetRecalibrationFx,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const STAGE: f32 = 320.0;
        let progress = fx.progress.clamp(0.0, 1.0);
        let fade_in = (progress / 0.12).min(1.0);
        let fade_out = 1.0 - ((progress - 0.82) / 0.18).clamp(0.0, 1.0);
        let alpha = fade_in * fade_out;
        let accent = cx.theme().primary;
        let scan = pet_recalibration_ease((progress / 0.68).min(1.0));
        let sprite_path = pet_sprite_path(
            &self.runtime.source_root,
            &self.state.support_dir,
            &self.state.pet,
            &self.pet_custom_pets,
        );
        let title = pet_catalog_text(
            &self.state.settings.language,
            "pet.recalibration.title",
            "Experience Recalibrated",
        );
        let detail = pet_catalog_text(
            &self.state.settings.language,
            "pet.recalibration.detail",
            "Progress restored from verified history",
        );

        let mut stage = div()
            .relative()
            .size(px(STAGE))
            .flex()
            .items_center()
            .justify_center();
        for index in 0..3_usize {
            let delay = index as f32 * 0.10;
            let ring_progress = ((progress - delay) / 0.52).clamp(0.0, 1.0);
            let eased = pet_recalibration_ease(ring_progress);
            let diameter = 100.0 + 150.0 * eased;
            stage = stage.child(
                div()
                    .absolute()
                    .left(px((STAGE - diameter) / 2.0))
                    .top(px((STAGE - diameter) / 2.0))
                    .size(px(diameter))
                    .rounded_full()
                    .border_1()
                    .border_color(accent.opacity((1.0 - eased) * 0.32 * fade_out)),
            );
        }
        stage = stage
            .child(
                div()
                    .absolute()
                    .left(px(64.0))
                    .top(px(98.0 + 108.0 * scan))
                    .w(px(192.0))
                    .h(px(2.0))
                    .bg(accent.opacity(0.55 * alpha)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .size(px(132.0))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(accent.opacity(0.13 * alpha))
                            .child(pet_sprite_element(
                                sprite_path,
                                96.0,
                                self.visible_pet_sprite_frame(PET_IDLE_FRAME_COUNT),
                                0,
                                accent,
                            )),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .text_size(rems(1.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::fixed_color(0xF3F6FC).opacity(alpha))
                            .child(title),
                    )
                    .child(
                        div()
                            .mt(px(6.0))
                            .text_size(rems(0.75))
                            .text_color(theme::fixed_color(0xC7CEDB).opacity(alpha))
                            .child(detail),
                    ),
            );

        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::fixed_color(0x05070C).opacity(0.48 * alpha))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|app, _event, _window, cx| {
                    app.pet_recalibration = None;
                    cx.notify();
                }),
            )
            .child(stage)
            .into_any_element()
    }
}

fn pet_recalibration_ease(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
