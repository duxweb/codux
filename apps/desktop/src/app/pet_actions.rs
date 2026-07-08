use super::*;
use crate::app::app_state::PetLevelUpFx;
use chrono::Timelike as _;

/// Minimum seconds a pet activity line stays before another same-tone line may
/// replace it, so concurrent agents don't make the bubble flicker every tick.
const DESKTOP_PET_LINE_MIN_HOLD_SECS: f64 = 5.0;

fn desktop_pet_action_status(action_id: &str) -> &'static str {
    match action_id {
        DESKTOP_PET_MUTE_30_MINUTES => "desktop pet muted for 30 minutes",
        DESKTOP_PET_MUTE_1_HOUR => "desktop pet muted for 1 hour",
        DESKTOP_PET_MUTE_TODAY => "desktop pet muted until tomorrow",
        DESKTOP_PET_SKIP_LINE => "desktop pet line skipped",
        DESKTOP_PET_SPEAK_MORE => "desktop pet speech frequency increased",
        DESKTOP_PET_SPEAK_LESS => "desktop pet speech frequency lowered",
        DESKTOP_PET_HIDE => "desktop pet hidden",
        _ => "desktop pet action applied",
    }
}

impl CoduxApp {
    pub(super) fn new_desktop_pet_window_from_state(
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
        main_window_fullscreen: bool,
    ) -> Self {
        let mut app = Self::new_settings_window_from_state(state, runtime, runtime_service);
        app.window_mode = AppWindowMode::DesktopPet;
        app.status_message = "desktop pet window ready".to_string();
        app.desktop_pet_main_window_fullscreen = main_window_fullscreen;
        app
    }

    pub(super) fn new_pet_window_from_state(
        mode: AppWindowMode,
        state: RuntimeState,
        runtime: RuntimeInventory,
        runtime_service: RuntimeService,
    ) -> Self {
        let mut app = Self::new_settings_window_from_state(state, runtime, runtime_service);
        app.window_mode = mode;
        app.status_message = match mode {
            AppWindowMode::PetClaim => "pet claim window ready".to_string(),
            AppWindowMode::PetCustomInstall => "custom pet install window ready".to_string(),
            AppWindowMode::PetDex => "pet dex window ready".to_string(),
            _ => "pet window ready".to_string(),
        };
        app
    }

    fn start_desktop_pet_speech_loop_with_initial_fullscreen(
        &mut self,
        main_window_fullscreen: bool,
        cx: &mut Context<Self>,
    ) {
        if self.window_mode != AppWindowMode::DesktopPet {
            return;
        }

        self.refresh_desktop_pet_live_runtime_state();
        self.desktop_pet_main_window_fullscreen = main_window_fullscreen;
        self.refresh_desktop_pet_activity_line(cx);
        self.start_pet_sprite_animation_loop(cx);
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                timer.timer(Duration::from_millis(500)).await;

                if this
                    .update(cx, |app, cx| {
                        app.state.runtime_events = app.runtime_service.reload_runtime_events();
                        app.refresh_desktop_pet_live_runtime_state();
                        app.refresh_desktop_pet_main_window_fullscreen(cx);
                        app.refresh_desktop_pet_activity_line(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn refresh_desktop_pet_live_runtime_state(&mut self) {
        let snapshot = self.runtime_service.ai_runtime_state_snapshot();
        // This runs every 500ms while the pet window is open. When nothing is
        // tracked and nothing is currently shown there is no work to do, so skip
        // the summarize + history-stats rebuild rather than churning on an empty
        // snapshot. (When sessions just cleared, state is still non-empty for one
        // tick, so the clearing refresh still runs.)
        if snapshot.sessions.is_empty() && self.state.ai_runtime_state.sessions.is_empty() {
            return;
        }
        self.state.ai_runtime_state = self
            .runtime_service
            .summarize_ai_runtime_state_snapshot(&snapshot);
        self.state.refresh_ai_history_stats();
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn start_desktop_pet_mouse_passthrough_loop(
        &mut self,
        window_handle: gpui::WindowHandle<Self>,
        cx: &mut Context<Self>,
    ) {
        if self.window_mode != AppWindowMode::DesktopPet {
            return;
        }

        let timer = cx.background_executor().clone();
        cx.spawn(async move |_this: gpui::WeakEntity<Self>, cx| {
            loop {
                timer.timer(Duration::from_millis(50)).await;

                match window_handle.update(cx, |app, window, _cx| {
                    if app.window_mode != AppWindowMode::DesktopPet {
                        return false;
                    }
                    macos_window::sync_desktop_pet_mouse_passthrough(window);
                    true
                }) {
                    Ok(true) => {}
                    Ok(false) | Err(_) => break,
                }
            }
        })
        .detach();
    }

    pub(super) fn start_pet_event_sync_loop(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.window_mode,
            AppWindowMode::PetClaim | AppWindowMode::PetCustomInstall | AppWindowMode::PetDex
        ) {
            return;
        }

        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                timer.timer(Duration::from_millis(300)).await;

                match this.update(cx, |app, cx| {
                    if !matches!(
                        app.window_mode,
                        AppWindowMode::PetClaim
                            | AppWindowMode::PetCustomInstall
                            | AppWindowMode::PetDex
                    ) {
                        return false;
                    }

                    let changed = app.sync_pet_custom_install_event_for_activity_tick()
                        || app.sync_pet_update_event_for_activity_tick();
                    if changed {
                        app.invalidate_ui_region(cx, UiRegion::Root);
                    }
                    true
                }) {
                    Ok(true) => {}
                    Ok(false) | Err(_) => break,
                }
            }
        })
        .detach();
    }

    /// Arm the full-screen celebration when a claimed pet just gained a level.
    /// Pure state change — the ticker is started lazily from render.
    pub(in crate::app) fn note_pet_level_transition(&mut self, previous_level: i64) {
        let pet = &self.state.pet;
        if self.window_mode != AppWindowMode::Main
            || !pet.claimed
            || previous_level <= 0
            || pet.level <= previous_level
        {
            return;
        }
        self.pet_level_up = Some(PetLevelUpFx {
            level: pet.level.max(1),
            progress: 0.0,
        });
    }

    /// Debug preview (⌃⌥L in the main window): replay the celebration with the
    /// current level so the animation can be tuned without grinding XP.
    pub(in crate::app) fn preview_pet_level_up(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            return;
        }
        self.pet_level_up = Some(PetLevelUpFx {
            level: self.state.pet.level.max(1),
            progress: 0.0,
        });
        cx.notify();
    }

    pub(in crate::app) fn ensure_pet_level_up_ticker(&mut self, cx: &mut Context<Self>) {
        if self.pet_level_up_ticking || self.pet_level_up.is_none() {
            return;
        }
        self.pet_level_up_ticking = true;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                timer.timer(Duration::from_millis(33)).await;
                let keep_going = this
                    .update(cx, |app, cx| {
                        let Some(fx) = app.pet_level_up.as_mut() else {
                            app.pet_level_up_ticking = false;
                            return false;
                        };
                        fx.progress += 0.014;
                        if fx.progress >= 1.0 {
                            app.pet_level_up = None;
                            app.pet_level_up_ticking = false;
                        }
                        app.invalidate_ui_region(cx, UiRegion::Root);
                        app.pet_level_up.is_some()
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::app) fn start_pet_sprite_animation_loop(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.window_mode,
            AppWindowMode::DesktopPet | AppWindowMode::PetDex
        ) {
            return;
        }
        if self.state.settings.pet_static_mode {
            return;
        }
        if self.pet_sprite_animation_active {
            return;
        }
        self.pet_sprite_animation_active = true;

        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let mut desktop_cycle_frame = 0usize;
            loop {
                let delay = this
                    .read_with(cx, |app, _cx| {
                        if app.window_mode == AppWindowMode::DesktopPet {
                            DESKTOP_PET_FRAME_INTERVAL
                        } else {
                            PET_DEX_FRAME_INTERVAL
                        }
                    })
                    .unwrap_or(PET_DEX_FRAME_INTERVAL);
                timer.timer(delay).await;

                match this.update(cx, |app, cx| {
                    if app.window_mode == AppWindowMode::PetDex
                        && !matches!(
                            app.pet_dex_spotlight,
                            Some(PetDexSpotlight::Bundled(_) | PetDexSpotlight::Custom(_))
                        )
                    {
                        app.pet_sprite_animation_active = false;
                        return false;
                    }
                    app.pet_sprite_frame = app.pet_sprite_frame.wrapping_add(1);
                    if app.window_mode == AppWindowMode::DesktopPet {
                        desktop_cycle_frame =
                            desktop_cycle_frame.wrapping_add(1) % app.desktop_pet_frame_count();
                    }
                    app.invalidate_ui_region(cx, UiRegion::Root);
                    true
                }) {
                    Ok(true) => {}
                    Ok(false) | Err(_) => break,
                }

                if desktop_cycle_frame == 0 {
                    let should_rest = this
                        .read_with(cx, |app, _cx| app.window_mode == AppWindowMode::DesktopPet)
                        .unwrap_or(false);
                    if should_rest {
                        timer.timer(DESKTOP_PET_ANIMATION_REST).await;
                    }
                }
            }
        })
        .detach();
    }

    pub(super) fn open_desktop_pet_window(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            return;
        }

        if let Some(handle) = self.desktop_pet_window {
            if handle.update(cx, |_view, _window, _cx| {}).is_ok() {
                self.status_message = "desktop pet window already opened".to_string();
                self.invalidate_ui_region(cx, UiRegion::Root);
                return;
            }
            self.desktop_pet_window = None;
        }

        match self.runtime_service.desktop_pet_should_show() {
            Ok(true) => {}
            Ok(false) => {
                self.status_message =
                    "desktop pet needs pet enabled, desktop widget enabled, and a claimed pet"
                        .to_string();
                self.invalidate_ui_region(cx, UiRegion::Root);
                return;
            }
            Err(error) => {
                self.status_message = format!("failed to check desktop pet: {error}");
                self.invalidate_ui_region(cx, UiRegion::Root);
                return;
            }
        }
        self.runtime_service
            .desktop_pet_set_bubble_visible(!self.desktop_pet_line.trim().is_empty());
        let parent_main_window = cx.entity().downgrade();

        let display = cx.primary_display();
        let display_id = display.as_ref().map(|display| display.id());
        let visible_bounds = display
            .as_ref()
            .map(|display| display.visible_bounds())
            .unwrap_or_else(|| Bounds::centered(None, size(px(1280.0), px(820.0)), cx));
        let work_area = DesktopPetWorkArea {
            x: visible_bounds.origin.x.to_f64(),
            y: visible_bounds.origin.y.to_f64(),
            width: visible_bounds.size.width.to_f64(),
            height: visible_bounds.size.height.to_f64(),
            scale_factor: 1.0,
        };
        let origin = self.runtime_service.desktop_pet_initial_position(work_area);
        let main_window_fullscreen = self.main_window_fullscreen;
        let bounds = Bounds::new(
            point(px(origin.x as f32), px(origin.y as f32)),
            size(
                px(DESKTOP_PET_BASE_WIDTH as f32),
                px(DESKTOP_PET_BASE_HEIGHT as f32),
            ),
        );

        let result = cx.open_window(
            WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(
                    px(DESKTOP_PET_BASE_WIDTH as f32),
                    px(DESKTOP_PET_BASE_HEIGHT as f32),
                )),
                display_id,
                focus: false,
                show: true,
                kind: WindowKind::PopUp,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| {
                macos_window::make_desktop_pet_window_transparent(window);
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                macos_window::sync_desktop_pet_mouse_passthrough(window);
                let app = CoduxApp::new_desktop_pet_window_from_state(
                    self.state.clone(),
                    self.runtime.clone(),
                    self.runtime_service.clone(),
                    main_window_fullscreen,
                );
                theme::apply_component_theme(
                    &app.state.settings.theme,
                    &app.state.settings.theme_color,
                    Some(window),
                    cx,
                );
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                let window_handle = window.window_handle().downcast::<CoduxApp>();
                let view = cx.new(|_| app);
                view.update(cx, |app, cx| {
                    app.parent_main_window = Some(parent_main_window);
                    app.start_desktop_pet_speech_loop_with_initial_fullscreen(
                        main_window_fullscreen,
                        cx,
                    );
                    #[cfg(any(target_os = "macos", target_os = "windows"))]
                    if let Some(window_handle) = window_handle {
                        app.start_desktop_pet_mouse_passthrough_loop(window_handle, cx);
                    }
                });
                view
            },
        );

        self.status_message = match result {
            Ok(handle) => {
                self.desktop_pet_window = Some(handle.into());
                "desktop pet window opened".to_string()
            }
            Err(error) => format!("failed to open desktop pet window: {error}"),
        };
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn close_desktop_pet_window(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.desktop_pet_window.take() {
            let _ = handle.update(cx, |_view, window, _cx| window.remove_window());
        }
        self.runtime_service.desktop_pet_set_bubble_visible(false);
    }

    pub(super) fn sync_desktop_pet_window(
        &mut self,
        report_unavailable: bool,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.desktop_pet_should_show() {
            Ok(true) => self.open_desktop_pet_window(cx),
            Ok(false) => {
                self.close_desktop_pet_window(cx);
                if report_unavailable {
                    self.status_message =
                        "desktop pet needs pet enabled, desktop widget enabled, and a claimed pet"
                            .to_string();
                    self.invalidate_ui_region(cx, UiRegion::Root);
                }
            }
            Err(error) => {
                self.close_desktop_pet_window(cx);
                if report_unavailable {
                    self.status_message = format!("failed to check desktop pet: {error}");
                    self.invalidate_ui_region(cx, UiRegion::Root);
                }
            }
        }
    }

    pub(super) fn refresh_desktop_pet_activity_line(&mut self, cx: &mut Context<Self>) {
        let line = desktop_pet_runtime_activity_line(
            &self.state.ai_runtime_state,
            &self.state.settings.language,
        );
        if !line.text.trim().is_empty() {
            self.set_desktop_pet_activity(line.text, line.tone, line.plan_items, cx);
            self.request_desktop_pet_llm_line(cx);
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        if let Some(context) = desktop_pet_reminder_line(
            &self.state.settings,
            &self.pet_snapshot,
            &self.state.settings.language,
            now,
            &mut self.desktop_pet_next_hydration_reminder_at,
            &mut self.desktop_pet_next_sedentary_reminder_at,
            &mut self.desktop_pet_next_late_night_reminder_at,
        ) {
            let fallback = context.fallback_text.clone();
            let tone = context.tone;
            self.set_desktop_pet_activity_line(fallback, tone, cx);
            self.request_desktop_pet_llm_context(context, cx);
            return;
        }

        if self.desktop_pet_line.trim().is_empty() {
            self.request_desktop_pet_idle_llm_line(now, cx);
        } else if self.desktop_pet_line_visible_until > 0.0
            && now < self.desktop_pet_line_visible_until
        {
            self.request_desktop_pet_idle_llm_line(now, cx);
        } else {
            self.set_desktop_pet_activity_line(String::new(), DesktopPetActivityTone::Normal, cx);
        }
    }

    pub(super) fn set_desktop_pet_activity_line(
        &mut self,
        line: String,
        tone: DesktopPetActivityTone,
        cx: &mut Context<Self>,
    ) {
        self.set_desktop_pet_activity(line, tone, Vec::new(), cx);
    }

    /// Set the line bypassing the same-tone hold. Used for the LLM "speech"
    /// line, which is intentional, rate-limited personality (not the multi-agent
    /// flicker the hold guards against).
    pub(super) fn set_desktop_pet_activity_line_forced(
        &mut self,
        line: String,
        tone: DesktopPetActivityTone,
        cx: &mut Context<Self>,
    ) {
        self.set_desktop_pet_activity_with_hold(line, tone, Vec::new(), true, cx);
    }

    pub(super) fn set_desktop_pet_activity(
        &mut self,
        line: String,
        tone: DesktopPetActivityTone,
        plan_items: Vec<DesktopPetPlanItem>,
        cx: &mut Context<Self>,
    ) {
        self.set_desktop_pet_activity_with_hold(line, tone, plan_items, false, cx);
    }

    fn set_desktop_pet_activity_with_hold(
        &mut self,
        line: String,
        tone: DesktopPetActivityTone,
        plan_items: Vec<DesktopPetPlanItem>,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        // When several agents run at once, the activity line recomputes every
        // refresh tick and the most-recently-updated session keeps changing, so
        // the bubble flickers between A/B/C roughly once a second. Hold each
        // message for DESKTOP_PET_LINE_MIN_HOLD_SECS before rotating to another
        // line of the *same* tone (same priority band). A tone change is a real
        // state escalation (e.g. running -> needs-permission) and passes through
        // immediately; clearing to idle is governed elsewhere.
        let is_same_tone_rotation = !force
            && tone == self.desktop_pet_tone
            && !line.trim().is_empty()
            && !self.desktop_pet_line.trim().is_empty()
            && line != self.desktop_pet_line;
        if is_same_tone_rotation && now < self.desktop_pet_line_hold_until {
            return;
        }
        if self.desktop_pet_line != line
            || self.desktop_pet_tone != tone
            || self.desktop_pet_plan_items != plan_items
        {
            let has_line = !line.trim().is_empty();
            self.desktop_pet_line = line;
            self.desktop_pet_tone = tone;
            self.desktop_pet_plan_items = plan_items;
            self.desktop_pet_line_hold_until = if has_line {
                now + DESKTOP_PET_LINE_MIN_HOLD_SECS
            } else {
                0.0
            };
            self.desktop_pet_line_visible_until = if has_line { now + 10.0 } else { 0.0 };
            self.runtime_service
                .desktop_pet_set_bubble_visible(!self.desktop_pet_line.trim().is_empty());
            self.invalidate_ui_region(cx, UiRegion::Root);
        }
    }

    pub(super) fn request_desktop_pet_llm_line(&mut self, cx: &mut Context<Self>) {
        let Some(context) =
            desktop_pet_llm_context(&self.state.ai_runtime_state, &self.state.settings.language)
        else {
            self.desktop_pet_active_llm_key.clear();
            return;
        };
        self.request_desktop_pet_llm_context(context, cx);
    }

    pub(super) fn request_desktop_pet_idle_llm_line(&mut self, now: f64, cx: &mut Context<Self>) {
        let policy = desktop_pet_speech_policy(
            &self.state.settings,
            "idle.monologue",
            self.desktop_pet_main_window_fullscreen,
            chrono::Local::now().hour(),
        );
        if !policy.allowed {
            self.desktop_pet_active_llm_key.clear();
            self.desktop_pet_next_idle_llm_at = 0.0;
            return;
        }
        let cooldown = policy.cooldown_seconds;
        if self.desktop_pet_next_idle_llm_at <= 0.0 {
            self.desktop_pet_next_idle_llm_at = now + cooldown;
            return;
        }
        if now < self.desktop_pet_next_idle_llm_at {
            return;
        }
        self.desktop_pet_next_idle_llm_at = now + cooldown;
        self.request_desktop_pet_llm_context(
            DesktopPetLlmContext {
                event: "idle.monologue",
                // Shown when the LLM is unavailable — localize it (the facts stay
                // English as context; the model is told to reply in the UI
                // language).
                fallback_text: desktop_pet_species_line(
                    &self.pet_snapshot,
                    &self.state.settings.language,
                    "idle",
                ),
                facts: "The user is idle or between AI tasks.".to_string(),
                tone: DesktopPetActivityTone::Normal,
                tool: "idle".to_string(),
                updated_at: now,
            },
            cx,
        );
    }

    pub(super) fn request_desktop_pet_llm_context(
        &mut self,
        context: DesktopPetLlmContext,
        cx: &mut Context<Self>,
    ) {
        let policy = desktop_pet_speech_policy(
            &self.state.settings,
            context.event,
            self.desktop_pet_main_window_fullscreen,
            chrono::Local::now().hour(),
        );
        if !policy.allowed {
            self.desktop_pet_active_llm_key.clear();
            return;
        }

        let key = format!(
            "{}:{}:{}:{}",
            context.event, context.tool, context.updated_at, context.facts
        );
        self.desktop_pet_active_llm_key = key.clone();
        if self.desktop_pet_requested_llm_key == key {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        let cooldown = policy.cooldown_seconds;
        if now - self.desktop_pet_last_llm_requested_at < cooldown {
            return;
        }
        // Suppress overlapping requests: the cooldown can be as low as ~30s, so
        // a slow provider could otherwise have a second request launched on top
        // of the first. One in flight at a time; the next tick re-requests once
        // it lands.
        if self.desktop_pet_llm_in_flight {
            return;
        }

        self.desktop_pet_requested_llm_key = key.clone();
        self.desktop_pet_last_llm_requested_at = now;
        self.desktop_pet_llm_generation = self.desktop_pet_llm_generation.wrapping_add(1);
        self.desktop_pet_llm_in_flight = true;
        let generation = self.desktop_pet_llm_generation;
        let service = self.runtime_service.clone();
        let event = context.event.to_string();
        let facts = context.facts.clone();
        let tone = context.tone;
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let request = codux_runtime::llm::PetIdleSpeechRequest { event, facts };
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.pet_idle_speech(request)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_desktop_pet_llm_line(key, tone, generation, result, cx);
            });
        })
        .detach();
    }

    pub(super) fn refresh_desktop_pet_main_window_fullscreen(&mut self, cx: &mut Context<Self>) {
        let Some(parent) = self.parent_main_window.clone() else {
            self.desktop_pet_main_window_fullscreen = false;
            return;
        };
        self.desktop_pet_main_window_fullscreen = parent
            .read_with(cx, |app, _cx| app.main_window_fullscreen)
            .unwrap_or(false);
    }

    pub(super) fn apply_desktop_pet_llm_line(
        &mut self,
        key: String,
        tone: DesktopPetActivityTone,
        generation: u64,
        result: Result<codux_runtime::llm::PetIdleSpeechResponse, String>,
        cx: &mut Context<Self>,
    ) {
        // Drop a response that a newer dispatch has superseded -- it must not
        // overwrite a fresher line, and it must not clear the newer request's
        // in-flight flag.
        if generation != self.desktop_pet_llm_generation {
            return;
        }
        self.desktop_pet_llm_in_flight = false;
        if self.desktop_pet_active_llm_key != key {
            // The desired key changed while this was in flight; let the next
            // tick request the current key.
            self.desktop_pet_requested_llm_key.clear();
            return;
        }
        match result {
            Ok(response) => {
                let text = normalized_desktop_pet_preview(Some(&response.text)).unwrap_or_default();
                if text.is_empty() {
                    // Empty line: don't hold the cooldown/requested slot for a
                    // key that produced nothing -- allow a retry.
                    self.desktop_pet_requested_llm_key.clear();
                } else {
                    self.set_desktop_pet_activity_line_forced(text, tone, cx);
                }
            }
            Err(_) => {
                // Transient failure: clear the requested key so the next tick
                // can retry instead of waiting out the full cooldown.
                self.desktop_pet_requested_llm_key.clear();
            }
        }
    }

    pub(in crate::app) fn run_pet_change_async(
        &mut self,
        action: &'static str,
        status: String,
        task: impl FnOnce(RuntimeService) -> Result<(), String> + Send + 'static,
        after_success: impl FnOnce(&mut CoduxApp, &mut Context<CoduxApp>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let service = self.runtime_service.clone();
        self.runtime_trace("pet", &format!("{action} queued"));
        self.status_message = status;
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                service.runtime_trace_frontend("pet", &format!("{action} start"));
                let result = task(service.clone());
                match &result {
                    Ok(_) => service.runtime_trace_frontend("pet", &format!("{action} ok")),
                    Err(error) => {
                        service.runtime_trace_frontend(
                            "pet",
                            &format!("{action} failed error={error}"),
                        );
                    }
                }
                result
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join pet action: {error}")));

            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(_) => {
                        app.refresh_pet_cache_async(cx);
                        let revision = publish_pet_update();
                        if revision > 0 {
                            app.pet_update_seen_revision = revision;
                        }
                        if app.window_mode == AppWindowMode::Main {
                            app.sync_desktop_pet_window(false, cx);
                        }
                        after_success(app, cx);
                    }
                    Err(error) => {
                        app.status_message = format!("failed to update pet: {error}");
                    }
                }
                app.invalidate_ui_region(cx, UiRegion::Root);
            });
        })
        .detach();
    }

    pub(super) fn save_desktop_pet_window_origin(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = window.bounds();
        let origin = DesktopPetSavedOrigin {
            x: bounds.origin.x.to_f64(),
            y: bounds.origin.y.to_f64(),
        };
        if let Err(error) = self.runtime_service.save_desktop_pet_origin(origin) {
            self.status_message = format!("failed to save desktop pet position: {error}");
            self.invalidate_ui_region(cx, UiRegion::Root);
        }
    }

    pub(super) fn apply_desktop_pet_action(
        &mut self,
        action_id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.runtime_trace(
            "desktop-pet",
            &format!("menu_action queued action={action_id}"),
        );
        self.status_message = "desktop pet action queued".to_string();
        self.invalidate_ui_region(cx, UiRegion::Root);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                service.runtime_trace_frontend(
                    "desktop-pet",
                    &format!("menu_action start action={action_id}"),
                );
                let result = service.apply_desktop_pet_menu_action(action_id).map(|_| {
                    if matches!(action_id, DESKTOP_PET_SKIP_LINE | DESKTOP_PET_HIDE) {
                        service.desktop_pet_set_bubble_visible(false);
                    }
                    service.reload_state()
                });
                match &result {
                    Ok(_) => service.runtime_trace_frontend(
                        "desktop-pet",
                        &format!("menu_action ok action={action_id}"),
                    ),
                    Err(error) => service.runtime_trace_frontend(
                        "desktop-pet",
                        &format!("menu_action failed action={action_id} error={error}"),
                    ),
                }
                result
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join desktop pet action: {error}")));

            let should_close = result.is_ok() && action_id == DESKTOP_PET_HIDE;
            let _ = this.update(cx, |app, cx| {
                app.apply_desktop_pet_action_result(action_id, result, cx);
            });
            if should_close {
                let _ = window_handle.update(cx, |_root, window, _cx| window.remove_window());
            }
        })
        .detach();
    }

    fn apply_desktop_pet_action_result(
        &mut self,
        action_id: &'static str,
        result: Result<RuntimeState, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(state) => {
                self.state.settings = state.settings;
                let previous_level = self.state.pet.level;
                self.state.pet = state.pet;
                self.note_pet_level_transition(previous_level);
                if action_id == DESKTOP_PET_SKIP_LINE {
                    self.desktop_pet_line.clear();
                    self.desktop_pet_tone = DesktopPetActivityTone::Normal;
                    self.desktop_pet_plan_items.clear();
                }
                self.status_message = desktop_pet_action_status(action_id).to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to apply desktop pet action: {error}");
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(in crate::app) fn apply_project_help_action(
        &mut self,
        action_id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.runtime_trace("help", &format!("action {action_id}"));
        match action_id {
            "help:about" => self.open_about_window(window, cx),
            "help:check-updates" => self.open_update_dialog_window(window, cx),
            "help:star-github" => self.prompt_github_star(cx),
            "help:open-folder" => self.open_project_folder_from_dialog(window, cx),
            "help:export-diagnostics" => self.export_diagnostics(cx),
            "help:runtime-log" => self.open_runtime_log(cx),
            "help:live-log" => self.open_live_log(cx),
            "help:website" => self.open_codux_website(cx),
            "help:github" => self.open_codux_github(cx),
            _ => {}
        }
    }

    pub(super) fn set_pet_install_url(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pet_install_url = value;
        self.pet_install_preview = None;
        self.pet_install_error = None;
        resize_pet_custom_install_window(window, PET_CUSTOM_INSTALL_INPUT_HEIGHT);
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_install_display_name(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pet_install_display_name = value;
        self.pet_install_error = None;
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn preview_custom_pet_install(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pet_install_previewing || self.pet_installing {
            self.status_message = "custom pet install task is already running".to_string();
            self.pet_install_error = Some("custom pet install task is already running".to_string());
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        let page_url = self.pet_install_url.trim().to_string();
        if page_url.is_empty() {
            self.status_message = "enter a Petdex URL first".to_string();
            self.pet_install_error = Some("enter a Petdex URL first".to_string());
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        let display_name = self.pet_install_display_name.trim().to_string();
        let request = PetCustomPetInstallRequest {
            page_url: page_url.clone(),
            display_name: display_name.clone(),
        };

        let service = self.runtime_service.clone();
        self.pet_install_previewing = true;
        self.pet_install_error = None;
        self.status_message = "custom pet preview loading".to_string();
        window.resize(size(
            px(PET_CUSTOM_INSTALL_WINDOW_WIDTH),
            px(PET_CUSTOM_INSTALL_INPUT_HEIGHT),
        ));
        let window_handle = window.window_handle();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn(async move {
                service.resolve_custom_pet_install(request).await
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_custom_pet_preview_result(
                    page_url,
                    display_name,
                    result,
                    window_handle,
                    cx,
                );
            });
        })
        .detach();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn apply_custom_pet_preview_result(
        &mut self,
        page_url: String,
        display_name: String,
        result: Result<PetCustomPetInstallPreview, String>,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        self.pet_install_previewing = false;
        if !self.pet_install_input_matches(&page_url, &display_name) {
            self.status_message = "stale custom pet preview ignored".to_string();
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        match result {
            Ok(preview) => {
                self.pet_install_display_name = preview.display_name.clone();
                self.status_message =
                    format!("custom pet preview loaded: {}", preview.display_name);
                self.pet_install_preview = Some(preview);
                self.pet_install_error = None;
                resize_pet_custom_install_window_handle(
                    window_handle,
                    PET_CUSTOM_INSTALL_READY_HEIGHT,
                    cx,
                );
            }
            Err(error) => {
                self.pet_install_preview = None;
                let message = format!("failed to preview custom pet: {error}");
                self.status_message = message.clone();
                self.pet_install_error = Some(message);
                resize_pet_custom_install_window_handle(
                    window_handle,
                    PET_CUSTOM_INSTALL_ERROR_HEIGHT,
                    cx,
                );
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn open_pet_market(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.open_url("https://petdex.crafter.run") {
            Ok(_) => self.status_message = "Petdex opened".to_string(),
            Err(error) => self.status_message = format!("failed to open Petdex: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn install_custom_pet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pet_install_previewing || self.pet_installing {
            self.status_message = "custom pet install task is already running".to_string();
            self.pet_install_error = Some("custom pet install task is already running".to_string());
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        if self.pet_install_preview.is_none() {
            self.status_message = "parse the Petdex page before installing".to_string();
            self.pet_install_error = Some("parse the Petdex page before installing".to_string());
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        let page_url = self.pet_install_url.trim().to_string();
        if page_url.is_empty() {
            self.status_message = "enter a Petdex URL first".to_string();
            self.pet_install_error = Some("enter a Petdex URL first".to_string());
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        let display_name = self.pet_install_display_name.trim().to_string();
        if display_name.is_empty() {
            self.status_message = "enter a pet name before installing".to_string();
            self.pet_install_error = Some("enter a pet name before installing".to_string());
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        let request = PetCustomPetInstallRequest {
            page_url: page_url.clone(),
            display_name: display_name.clone(),
        };

        let service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.pet_installing = true;
        self.pet_install_error = None;
        self.status_message = "custom pet install started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn(async move {
                let custom_pet = service.install_custom_pet(request).await?;
                Ok((
                    service.reload_pet(),
                    custom_pet.id,
                    format!("custom pet installed: {}", custom_pet.display_name),
                ))
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                let should_close = result.is_ok();
                app.apply_custom_pet_install_result(page_url, display_name, result, cx);
                if should_close {
                    let _ = window_handle.update(cx, |_view, window, _cx| window.remove_window());
                }
            });
        })
        .detach();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn apply_custom_pet_install_result(
        &mut self,
        page_url: String,
        display_name: String,
        result: Result<(PetSummary, String, String), String>,
        cx: &mut Context<Self>,
    ) {
        self.pet_installing = false;
        match result {
            Ok((_pet, custom_pet_id, status_message)) => {
                let matches_input = self.pet_install_input_matches(&page_url, &display_name);
                self.refresh_pet_cache();
                let revision = publish_pet_custom_install(custom_pet_id);
                if revision > 0 {
                    self.pet_custom_install_seen_revision = revision;
                }
                if matches_input {
                    self.pet_install_url.clear();
                    self.pet_install_display_name.clear();
                    self.pet_install_preview = None;
                }
                self.pet_install_error = None;
                self.status_message = status_message;
            }
            Err(error) => {
                let message = format!("failed to install custom pet: {error}");
                self.status_message = message.clone();
                self.pet_install_error = Some(message);
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn pet_install_input_matches(&self, page_url: &str, display_name: &str) -> bool {
        self.pet_install_url.trim() == page_url
            && self.pet_install_display_name.trim() == display_name
    }

    pub(super) fn claim_pet_species(
        &mut self,
        species: String,
        custom_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trimmed_species = species.trim();
        let window_handle = window.window_handle();
        if let Some(custom_id) = trimmed_species.strip_prefix("custom:") {
            if let Some(custom_pet) = self
                .pet_custom_pets
                .iter()
                .find(|pet| pet.id == custom_id)
                .cloned()
            {
                let display_name = custom_pet.display_name.clone();
                let request = PetClaimRequest {
                    species: format!("custom:{}", custom_pet.id.clone()),
                    custom_name: custom_name.trim().to_string(),
                    custom_pet: Some(custom_pet),
                    _projects: Vec::new(),
                };
                self.run_pet_change_async(
                    "claim_custom_pet",
                    format!("claiming custom pet: {display_name}"),
                    move |service| {
                        let request = PetClaimRequest {
                            custom_pet: request
                                .custom_pet
                                .map(|pet| service.hydrate_custom_pet_data_url(pet)),
                            ..request
                        };
                        service.claim_pet_from_indexed_history(request).map(|_| ())
                    },
                    move |app, cx| {
                        app.status_message = format!("custom pet claimed: {display_name}");
                        let _ = window_handle.update(cx, |_root, window, _cx| {
                            window.remove_window();
                        });
                    },
                    cx,
                );
                self.invalidate_ui_region(cx, UiRegion::Root);
                return;
            }
        }

        let species = if trimmed_species.is_empty() {
            self.pet_catalog
                .species
                .first()
                .map(|item| item.species.clone())
                .unwrap_or_else(|| "voidcat".to_string())
        } else {
            trimmed_species.to_string()
        };
        let request = PetClaimRequest {
            species,
            custom_name: custom_name.trim().to_string(),
            custom_pet: None,
            _projects: Vec::new(),
        };

        self.run_pet_change_async(
            "claim_pet",
            "claiming pet".to_string(),
            move |service| service.claim_pet_from_indexed_history(request).map(|_| ()),
            move |app, cx| {
                app.status_message = "pet claimed".to_string();
                let _ = window_handle.update(cx, |_root, window, _cx| {
                    window.remove_window();
                });
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn rename_current_pet_to(
        &mut self,
        custom_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.pet.claimed {
            self.status_message = "no pet to rename".to_string();
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }

        let request = PetRenameRequest {
            custom_name: custom_name.trim().to_string(),
        };
        self.run_pet_change_async(
            "rename_pet",
            "renaming pet".to_string(),
            move |service| service.rename_pet(request).map(|_| ()),
            |app, _cx| {
                app.pet_name_editing = false;
                app.status_message = "pet renamed".to_string();
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn start_current_pet_rename(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.pet.claimed {
            self.pet_name_editing = true;
            self.status_message = "pet rename editor opened".to_string();
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn cancel_current_pet_rename(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pet_name_editing = false;
        self.status_message = "pet rename cancelled".to_string();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn show_pet_dex_spotlight(
        &mut self,
        spotlight: PetDexSpotlight,
        cx: &mut Context<Self>,
    ) {
        self.pet_dex_spotlight = Some(spotlight);
        self.start_pet_sprite_animation_loop(cx);
        self.status_message = "pet dex detail opened".to_string();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn close_pet_dex_spotlight(&mut self, cx: &mut Context<Self>) {
        self.pet_dex_spotlight = None;
        self.status_message = "pet dex detail closed".to_string();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn archive_current_pet(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.pet_dex_spotlight = Some(PetDexSpotlight::ArchiveConfirm);
        self.status_message = "confirm pet archive".to_string();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn archive_current_pet_confirmed(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_pet_change_async(
            "archive_pet",
            "archiving pet".to_string(),
            |service| service.archive_current_pet().map(|_| ()),
            |app, _cx| {
                app.pet_dex_spotlight = None;
                app.status_message = "pet archived".to_string();
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn desktop_pet_side(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DesktopPetSide {
        let bounds = window.bounds();
        let display = cx.primary_display();
        let visible_bounds = display
            .as_ref()
            .map(|display| display.visible_bounds())
            .unwrap_or_else(|| Bounds::centered(None, size(px(1280.0), px(820.0)), cx));
        let work_area = DesktopPetWorkArea {
            x: visible_bounds.origin.x.to_f64(),
            y: visible_bounds.origin.y.to_f64(),
            width: visible_bounds.size.width.to_f64(),
            height: visible_bounds.size.height.to_f64(),
            scale_factor: 1.0,
        };
        let side = self
            .runtime_service
            .desktop_pet_placement(
                codux_runtime::desktop_pet::DesktopPetPhysicalPosition {
                    x: bounds.origin.x.to_f64(),
                    y: bounds.origin.y.to_f64(),
                },
                codux_runtime::desktop_pet::DesktopPetPhysicalSize {
                    width: bounds.size.width.to_f64(),
                    height: bounds.size.height.to_f64(),
                },
                work_area,
            )
            .side;
        if side.as_str() == DesktopPetSide::Right.as_str() {
            DesktopPetSide::Right
        } else {
            DesktopPetSide::Left
        }
    }

    pub(super) fn desktop_pet_animation(&self) -> DesktopPetAnimation {
        if !self.state.pet.claimed {
            return DesktopPetAnimation {
                row: 6,
                frame_count: PET_WAITING_FRAME_COUNT,
            };
        }
        if self
            .state
            .ai_runtime_state
            .sessions
            .iter()
            .any(|session| session.state == "needs-input")
        {
            return DesktopPetAnimation {
                row: 8,
                frame_count: PET_REVIEW_FRAME_COUNT,
            };
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        let has_running_session = self
            .state
            .ai_runtime_state
            .sessions
            .iter()
            .any(|session| session.state == "running");
        if let Some(session) = self
            .state
            .ai_runtime_state
            .sessions
            .iter()
            .filter(|session| {
                session.state != "running"
                    && session.state != "needs-input"
                    && session.has_completed_turn
                    && now - session.updated_at <= DESKTOP_PET_COMPLETION_VISIBLE_SECONDS
                    && !has_running_session
            })
            .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
        {
            return if session.was_interrupted {
                DesktopPetAnimation {
                    row: 5,
                    frame_count: PET_FAILED_FRAME_COUNT,
                }
            } else {
                DesktopPetAnimation {
                    row: 3,
                    frame_count: PET_WAVING_FRAME_COUNT,
                }
            };
        }

        if self
            .state
            .ai_runtime_state
            .sessions
            .iter()
            .any(|session| session.state == "running")
            || self.state.pet.daily_xp > 0
        {
            return DesktopPetAnimation {
                row: 7,
                frame_count: PET_RUNNING_FRAME_COUNT,
            };
        }

        DesktopPetAnimation {
            row: 0,
            frame_count: PET_IDLE_FRAME_COUNT,
        }
    }

    pub(super) fn desktop_pet_frame_count(&self) -> usize {
        self.desktop_pet_animation().frame_count.max(1)
    }

    pub(super) fn desktop_pet_window(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let focus_handle = self.root_focus_handle.clone();
        let line = self.desktop_pet_line.trim().to_string();
        let plan_items = self.desktop_pet_plan_items.clone();
        let sprite_path = pet_sprite_path(
            &self.runtime.source_root,
            &self.state.support_dir,
            &self.state.pet,
            &self.pet_custom_pets,
        );
        let animation = self.desktop_pet_animation();
        let sprite_frame = self.visible_pet_sprite_frame(animation.frame_count);
        let sprite_visible_width = pet_sprite_visible_width(DESKTOP_PET_SPRITE_SIZE);
        let menu_entries = desktop_pet_menu_entries(&self.state.settings.language);
        let side = self.desktop_pet_side(window, cx);
        let bubble_is_left_tail = side == DesktopPetSide::Right;
        let tone = self.desktop_pet_tone;

        div()
            .size_full()
            .text_color(cx.theme().foreground)
            .bg(cx.theme().transparent)
            .when_some(focus_handle.as_ref(), |this, focus_handle| {
                this.track_focus(focus_handle)
            })
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .size_full()
                    .relative()
                    .bg(cx.theme().transparent)
                    .when(!line.is_empty(), |this| {
                        this.child(desktop_pet_bubble(
                            line,
                            tone,
                            plan_items,
                            &self.state.settings.language,
                            bubble_is_left_tail,
                        ))
                    })
                    .child(
                        div()
                            .absolute()
                            .bottom(px(DESKTOP_PET_SPRITE_BOTTOM))
                            .w(px(sprite_visible_width))
                            .h(px(DESKTOP_PET_SPRITE_SIZE))
                            .overflow_hidden()
                            .when(side == DesktopPetSide::Right, |this| {
                                this.left(px(DESKTOP_PET_SPRITE_SIDE))
                            })
                            .when(side == DesktopPetSide::Left, |this| {
                                this.right(px(DESKTOP_PET_SPRITE_SIDE))
                            })
                            .window_control_area(WindowControlArea::Drag)
                            .on_mouse_down(MouseButton::Left, |_event, window, _cx| {
                                window.start_window_move();
                            })
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(
                                    move |_app, event: &gpui::MouseDownEvent, window, cx| {
                                        _app.runtime_trace("desktop-pet", "native_menu open");
                                        macos_window::spawn_desktop_pet_native_menu(
                                            window,
                                            event.position,
                                            menu_entries.clone(),
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    },
                                ),
                            )
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|app, _event, window, cx| {
                                    app.save_desktop_pet_window_origin(window, cx)
                                }),
                            )
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(desktop_pet_sprite(
                                sprite_path,
                                sprite_frame,
                                animation.row,
                                cx,
                            )),
                    ),
            )
    }
}
