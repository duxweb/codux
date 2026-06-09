use super::*;

impl CoduxApp {
    pub(super) fn set_terminal_font_family(
        &mut self,
        family: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.settings.terminal_font_family == family {
            return;
        }
        self.save_settings_async(
            "set_terminal_font_family",
            "saving terminal font family",
            move |service| service.set_terminal_font_family(&family),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.apply_terminal_text_settings(cx);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_terminal_font_size(
        &mut self,
        size: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_terminal_font_size",
            "saving terminal font size",
            move |service| service.set_terminal_font_size(&size),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.apply_terminal_text_settings(cx);
                app.status_message = format!(
                    "terminal font size saved: {}",
                    app.state.settings.terminal_font_size
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_terminal_scrollback_lines(
        &mut self,
        lines: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.settings.terminal_scrollback_lines == lines {
            return;
        }
        self.save_settings_async(
            "set_terminal_scrollback_lines",
            "saving terminal scrollback",
            move |service| service.set_terminal_scrollback_value(&lines),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "terminal scrollback saved: {}",
                    app.state.settings.terminal_scrollback_lines
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_terminal_paste_images_as_paths(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "toggle_terminal_paste_images_as_paths",
            "saving terminal paste setting",
            move |service| service.toggle_terminal_paste_images_as_paths(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.apply_terminal_text_settings(cx);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn terminal_config_from_settings(&self) -> TerminalConfig {
        terminal_config_for_settings(&self.state.settings, self.window_appearance)
    }

    pub(super) fn apply_terminal_text_settings(&self, cx: &mut Context<Self>) {
        let config = self.terminal_config_from_settings();
        for tab in &self.terminals {
            for slot in &tab.panes {
                if let Some(pane) = &slot.pane {
                    let config = config.clone();
                    pane.view.update(cx, |terminal, cx| {
                        terminal.update_config(config, cx);
                    });
                }
            }
        }
    }

    pub(super) fn apply_settings_update_event(&mut self, cx: &mut Context<Self>) -> bool {
        let event = current_settings_update_event();
        if event.revision <= self.settings_seen_revision {
            return false;
        }

        self.settings_seen_revision = event.revision;
        let settings = self.runtime_service.reload_settings();
        if event.statistics_revision == event.revision {
            self.apply_settings_summary_local(settings);
            self.status_message = "AI statistics mode updated".to_string();
            if self.window_mode == AppWindowMode::Main {
                self.invalidate_ui(
                    cx,
                    [
                        UiRegion::WorkspaceAssistant,
                        UiRegion::AIStatsSidebar,
                        UiRegion::StatusBar,
                    ],
                );
            }
            return true;
        }

        self.replace_settings_summary(settings);
        self.normalize_selected_ai_provider();
        self.normalize_selected_notification_channel();
        self.normalize_selected_remote_device();
        cx.set_menus(native_menu::codux_menus(&self.state.settings.language));
        theme::apply_component_theme(
            &self.state.settings.theme,
            &self.state.settings.theme_color,
            None,
            cx,
        );
        if self.window_mode == AppWindowMode::Main {
            self.apply_terminal_text_settings(cx);
            self.sync_desktop_pet_window(false, cx);
        }
        self.status_message = "settings updated".to_string();
        true
    }

    pub(super) fn replace_settings_summary(&mut self, settings: SettingsSummary) {
        self.state.settings = settings_with_active_restart_locked_values(&settings);
        self.state.remote = self.runtime_service.reload_remote();
        self.state.notifications = self.runtime_service.reload_notifications();
        self.state.power = self
            .runtime_service
            .power_summary(&self.state.settings.sleep_mode);
        self.state.refresh_ai_history_stats();
    }

    pub(super) fn apply_settings_summary_local(&mut self, settings: SettingsSummary) {
        self.state.settings = settings_with_active_restart_locked_values(&settings);
        self.state.refresh_ai_history_stats();
    }

    pub(super) fn apply_settings_summary(&mut self, settings: SettingsSummary) {
        self.replace_settings_summary(settings);
        let revision = publish_settings_update();
        publish_child_window_update(ChildWindowUpdateKind::Settings);
        if revision > 0 {
            self.settings_seen_revision = revision;
        }
    }

    pub(super) fn apply_async_settings_summary(&mut self, settings: SettingsSummary) {
        self.apply_settings_summary_local(settings);
        let revision = publish_settings_update();
        publish_child_window_update(ChildWindowUpdateKind::Settings);
        if revision > 0 {
            self.settings_seen_revision = revision;
        }
    }

    fn save_settings_async(
        &mut self,
        action: &'static str,
        status: &'static str,
        save: impl FnOnce(RuntimeService) -> Result<SettingsSummary, String> + Send + 'static,
        apply: impl FnOnce(&mut CoduxApp, SettingsSummary, &mut Context<CoduxApp>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let service = self.runtime_service.clone();
        self.runtime_trace("settings", &format!("{action} queued"));
        self.status_message = status.to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                service.runtime_trace_frontend("settings", &format!("{action} start"));
                let result = save(service.clone());
                match &result {
                    Ok(_) => service.runtime_trace_frontend("settings", &format!("{action} ok")),
                    Err(error) => service.runtime_trace_frontend(
                        "settings",
                        &format!("{action} failed error={error}"),
                    ),
                }
                result
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join settings save: {error}")));

            let _ = this.update(cx, |app, cx| match result {
                Ok(settings) => apply(app, settings, cx),
                Err(error) => {
                    app.status_message = format!("failed to save settings: {error}");
                    app.invalidate_ui_region(cx, UiRegion::Root);
                }
            });
        })
        .detach();
    }

    fn run_settings_task_async<T: Send + 'static>(
        &mut self,
        action: &'static str,
        status: &'static str,
        task: impl FnOnce(RuntimeService) -> Result<T, String> + Send + 'static,
        apply: impl FnOnce(&mut CoduxApp, T, &mut Context<CoduxApp>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let service = self.runtime_service.clone();
        self.runtime_trace("settings", &format!("{action} queued"));
        self.status_message = status.to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                service.runtime_trace_frontend("settings", &format!("{action} start"));
                let result = task(service.clone());
                match &result {
                    Ok(_) => service.runtime_trace_frontend("settings", &format!("{action} ok")),
                    Err(error) => service.runtime_trace_frontend(
                        "settings",
                        &format!("{action} failed error={error}"),
                    ),
                }
                result
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join settings task: {error}")));

            let _ = this.update(cx, |app, cx| match result {
                Ok(value) => apply(app, value, cx),
                Err(error) => {
                    app.status_message = format!("failed to update settings: {error}");
                    app.invalidate_ui_region(cx, UiRegion::Root);
                }
            });
        })
        .detach();
    }

    pub(super) fn set_theme(&mut self, theme: String, window: &mut Window, cx: &mut Context<Self>) {
        let service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.runtime_trace("settings", &format!("set_theme queued value={theme}"));
        self.status_message = "saving theme".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                service
                    .runtime_trace_frontend("settings", &format!("set_theme start value={theme}"));
                let result = service.set_theme(&theme);
                match &result {
                    Ok(_) => service.runtime_trace_frontend("settings", "set_theme ok"),
                    Err(error) => service.runtime_trace_frontend(
                        "settings",
                        &format!("set_theme failed error={error}"),
                    ),
                }
                result
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join theme save: {error}")));
            let _ = window_handle.update(cx, |_root, window, cx| {
                let _ = this.update(cx, |app, cx| {
                    app.apply_theme_save_result(result, window, cx);
                });
            });
        })
        .detach();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    fn apply_theme_save_result(
        &mut self,
        result: Result<SettingsSummary, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(settings) => {
                self.apply_async_settings_summary(settings);
                theme::apply_component_theme(
                    &self.state.settings.theme,
                    &self.state.settings.theme_color,
                    Some(window),
                    cx,
                );
                self.apply_terminal_text_settings(cx);
                self.status_message = "theme saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save theme: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_theme_color(
        &mut self,
        theme_color: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.runtime_trace(
            "settings",
            &format!("set_theme_color queued value={theme_color}"),
        );
        self.status_message = "saving theme color".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                service.runtime_trace_frontend(
                    "settings",
                    &format!("set_theme_color start value={theme_color}"),
                );
                let result = service.set_theme_color(&theme_color);
                match &result {
                    Ok(_) => service.runtime_trace_frontend("settings", "set_theme_color ok"),
                    Err(error) => service.runtime_trace_frontend(
                        "settings",
                        &format!("set_theme_color failed error={error}"),
                    ),
                }
                result
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join theme color save: {error}")));
            let _ = window_handle.update(cx, |_root, window, cx| {
                let _ = this.update(cx, |app, cx| {
                    app.apply_theme_color_save_result(result, window, cx);
                });
            });
        })
        .detach();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    fn apply_theme_color_save_result(
        &mut self,
        result: Result<SettingsSummary, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(settings) => {
                self.apply_async_settings_summary(settings);
                theme::apply_component_theme(
                    &self.state.settings.theme,
                    &self.state.settings.theme_color,
                    Some(window),
                    cx,
                );
                self.apply_terminal_text_settings(cx);
                self.status_message =
                    format!("theme color saved: {}", self.state.settings.theme_color);
            }
            Err(error) => self.status_message = format!("failed to save theme color: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_icon_style(
        &mut self,
        icon_style: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_icon_style",
            "saving icon style",
            move |service| service.set_icon_style(&icon_style),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                let _ = codux_runtime::app_icon::apply_app_icon(&app.state.settings.icon_style);
                app.status_message = format!("icon style saved: {}", app.state.settings.icon_style);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_dock_badge(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.save_settings_async(
            "toggle_dock_badge",
            "saving dock badge",
            |service| service.toggle_dock_badge(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "dock badge saved: {}",
                    if app.state.settings.shows_dock_badge {
                        "on"
                    } else {
                        "off"
                    }
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_language(
        &mut self,
        language: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_restart_language = active_settings_snapshot()
            .filter(|active| active.language == language)
            .map(|_| None)
            .unwrap_or_else(|| Some(language.clone()));
        self.save_settings_async(
            "set_language",
            "saving language",
            move |service| service.set_language(&language),
            |app, settings, cx| {
                let saved_language = settings.language.clone();
                app.apply_async_settings_summary(settings);
                app.pending_restart_language = active_settings_snapshot()
                    .filter(|active| active.language == saved_language)
                    .map(|_| None)
                    .unwrap_or(Some(saved_language));
                let message = super::settings::settings_text(
                    &app.state.settings.language,
                    "settings.language.restart_required",
                    "Restart the app to apply the selected language.",
                );
                app.status_message = message.clone();
                app.show_toast(message, cx);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_developer_hud(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_settings_task_async(
            "toggle_developer_hud",
            "saving developer HUD",
            |service| {
                let settings = service.toggle_developer_hud()?;
                let performance = settings
                    .developer_hud
                    .then(codux_runtime::performance::PerformanceService::summary);
                Ok((settings, performance))
            },
            |app, (settings, performance), cx| {
                app.apply_async_settings_summary(settings);
                if let Some(performance) = performance {
                    app.state.performance = performance;
                }
                app.normalize_selected_ai_provider();
                app.status_message = "developer HUD setting saved".to_string();
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_developer_refresh(
        &mut self,
        seconds: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_developer_refresh",
            "saving developer refresh",
            move |service| service.set_developer_refresh(&seconds),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "developer refresh saved: {}",
                    app.state.settings.developer_refresh
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_update_enabled(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let repo_root = std::env::current_dir().unwrap_or_default();
        self.run_settings_task_async(
            "toggle_update_enabled",
            "saving update setting",
            move |service| {
                let settings = service.toggle_update_enabled()?;
                let update = service.reload_update_settings(repo_root);
                Ok((settings, update))
            },
            |app, (settings, update), cx| {
                app.apply_async_settings_summary(settings);
                app.state.update = update;
                app.status_message = format!(
                    "update setting saved: {}",
                    if app.state.settings.update_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_statistics_mode(
        &mut self,
        mode: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_statistics_mode",
            "saving AI statistics mode",
            move |service| service.set_statistics_mode(&mode),
            |app, settings, cx| {
                app.apply_settings_summary_local(settings);
                let revision = publish_statistics_settings_update();
                publish_child_window_update(ChildWindowUpdateKind::Settings);
                if revision > 0 {
                    app.settings_seen_revision = revision;
                }
                app.status_message = format!(
                    "AI statistics mode saved: {}",
                    app.state.settings.statistics_mode
                );
                if app.window_mode == AppWindowMode::Main {
                    app.invalidate_ui(
                        cx,
                        [
                            UiRegion::WorkspaceAssistant,
                            UiRegion::AIStatsSidebar,
                            UiRegion::StatusBar,
                        ],
                    );
                } else {
                    app.invalidate_ui_region(cx, UiRegion::Root);
                }
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_file_open_default(
        &mut self,
        mode: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_file_open_default",
            "saving file open default",
            move |service| service.set_file_open_default(&mode),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "file open default saved: {}",
                    app.state.settings.file_open_default
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_git_refresh(
        &mut self,
        seconds: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_git_refresh",
            "saving Git refresh",
            move |service| service.set_git_refresh(&seconds),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message =
                    format!("Git refresh saved: {}", app.state.settings.git_refresh);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_refresh(
        &mut self,
        seconds: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_ai_refresh",
            "saving AI refresh",
            move |service| service.set_ai_refresh(&seconds),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!("AI refresh saved: {}", app.state.settings.ai_refresh);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_background_refresh(
        &mut self,
        seconds: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_ai_background_refresh",
            "saving AI background refresh",
            move |service| service.set_ai_background_refresh(&seconds),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "AI background refresh saved: {}",
                    app.state.settings.ai_background_refresh
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_enabled(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.save_settings_async(
            "toggle_pet_enabled",
            "saving pet setting",
            |service| service.toggle_pet_enabled(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.sync_desktop_pet_window(false, cx);
                app.status_message = format!(
                    "pet setting saved: {}",
                    if app.state.settings.pet_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_desktop_widget(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "toggle_pet_desktop_widget",
            "saving desktop pet setting",
            |service| service.toggle_pet_desktop_widget(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                let enabled = app.state.settings.pet_desktop_widget;
                app.status_message = format!(
                    "desktop pet setting saved: {}",
                    if enabled { "on" } else { "off" }
                );
                if app.window_mode == AppWindowMode::Main {
                    app.sync_desktop_pet_window(false, cx);
                }
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_static_mode(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.save_settings_async(
            "toggle_pet_static_mode",
            "saving pet static mode",
            |service| service.toggle_pet_static_mode(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "pet static mode saved: {}",
                    if app.state.settings.pet_static_mode {
                        "on"
                    } else {
                        "off"
                    }
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_reminders(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.save_settings_async(
            "toggle_pet_reminders",
            "saving pet reminders",
            |service| service.toggle_pet_reminders(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "pet reminders saved: {}",
                    if app.state.settings.pet_reminders {
                        "on"
                    } else {
                        "off"
                    }
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_sedentary_reminders(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "toggle_pet_sedentary_reminders",
            "saving pet sedentary reminders",
            |service| service.toggle_pet_sedentary_reminders(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_late_night_reminders(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "toggle_pet_late_night_reminders",
            "saving pet late-night reminders",
            |service| service.toggle_pet_late_night_reminders(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_hydration_reminder_minutes(
        &mut self,
        minutes: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_pet_hydration_reminder_minutes",
            "saving pet hydration reminder interval",
            move |service| service.set_pet_hydration_reminder_minutes(&minutes),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_sedentary_reminder_minutes(
        &mut self,
        minutes: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_pet_sedentary_reminder_minutes",
            "saving pet sedentary reminder interval",
            move |service| service.set_pet_sedentary_reminder_minutes(&minutes),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_late_night_reminder_minutes(
        &mut self,
        minutes: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_pet_late_night_reminder_minutes",
            "saving pet late-night reminder interval",
            move |service| service.set_pet_late_night_reminder_minutes(&minutes),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_speech_mode(
        &mut self,
        mode: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_pet_speech_mode",
            "saving pet speech mode",
            move |service| service.set_pet_speech_mode(&mode),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "pet speech mode saved: {}",
                    app.state.settings.pet_speech_mode
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_speech_frequency(
        &mut self,
        frequency: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_pet_speech_frequency",
            "saving pet speech frequency",
            move |service| service.set_pet_speech_frequency(&frequency),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "pet speech frequency saved: {}",
                    app.state.settings.pet_speech_frequency
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_speech_llm_enabled(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "toggle_pet_speech_llm_enabled",
            "saving pet speech LLM",
            |service| service.toggle_pet_speech_llm_enabled(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "pet speech LLM saved: {}",
                    if app.state.settings.pet_speech_llm_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_speech_quiet_during_work(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "toggle_pet_speech_quiet_during_work",
            "saving pet speech work-hours setting",
            |service| service.toggle_pet_speech_quiet_during_work(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = "pet speech work-hours setting saved".to_string();
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_speech_louder_at_night(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "toggle_pet_speech_louder_at_night",
            "saving pet speech night setting",
            |service| service.toggle_pet_speech_louder_at_night(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = "pet speech night setting saved".to_string();
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_speech_mute_on_fullscreen(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "toggle_pet_speech_mute_on_fullscreen",
            "saving pet speech fullscreen setting",
            |service| service.toggle_pet_speech_mute_on_fullscreen(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = "pet speech fullscreen setting saved".to_string();
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_speech_quiet_hours(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "toggle_pet_speech_quiet_hours",
            "saving pet speech quiet-hours setting",
            |service| service.toggle_pet_speech_quiet_hours(),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = "pet speech quiet-hours setting saved".to_string();
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_speech_temporary_mute(&mut self, muted: bool, cx: &mut Context<Self>) {
        self.save_settings_async(
            "set_pet_speech_temporary_mute",
            "saving pet speech temporary mute setting",
            move |service| service.set_pet_speech_temporary_mute(muted),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = "pet speech temporary mute setting saved".to_string();
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn normalize_selected_notification_channel(&mut self) {
        let selected_still_exists = self
            .selected_notification_channel_id
            .as_deref()
            .map(|id| {
                self.state
                    .notifications
                    .channels
                    .iter()
                    .any(|channel| channel.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_notification_channel_id = self
                .state
                .notifications
                .channels
                .first()
                .map(|channel| channel.id.clone());
        }
    }

    pub(super) fn set_notification_channel_enabled(
        &mut self,
        channel_id: String,
        enabled: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_notification_channel_enabled(&channel_id, enabled)
        {
            Ok(notifications) => {
                self.state.notifications = notifications;
                self.selected_notification_channel_id = Some(channel_id);
                self.normalize_selected_notification_channel();
                self.status_message = "notification channel setting saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save notification channel: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn update_notification_channel_string(
        &mut self,
        channel_id: String,
        key: &'static str,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .update_notification_channel_string(&channel_id, key, &value)
        {
            Ok(notifications) => {
                self.state.notifications = notifications;
                self.selected_notification_channel_id = Some(channel_id);
                self.normalize_selected_notification_channel();
                self.status_message = "notification channel setting saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save notification channel: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn test_notification_channel(
        &mut self,
        channel_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.notification_testing_channel_id.is_some() {
            self.status_message = "notification test is already running".to_string();
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        let service = self.runtime_service.clone();
        self.notification_testing_channel_id = Some(channel_id.clone());
        self.selected_notification_channel_id = Some(channel_id.clone());
        self.status_message = "notification test started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_channel_id = channel_id.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.test_notification_channel(&worker_channel_id)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_notification_test_result(channel_id, result, cx);
            });
        })
        .detach();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn apply_notification_test_result(
        &mut self,
        channel_id: String,
        result: Result<codux_runtime::notification::NotificationDispatchResult, String>,
        cx: &mut Context<Self>,
    ) {
        if self.notification_testing_channel_id.as_deref() == Some(channel_id.as_str()) {
            self.notification_testing_channel_id = None;
        }
        match result {
            Ok(result) => {
                if result.failed.is_empty() {
                    self.status_message = format!("notification test sent: {}", result.sent);
                } else {
                    let failures = result
                        .failed
                        .iter()
                        .map(|failure| format!("{}: {}", failure.id, failure.message))
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.status_message =
                        format!("notification test sent {}, failed: {failures}", result.sent);
                }
            }
            Err(error) => {
                self.status_message = format!("notification test failed: {error}");
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_update_channel(
        &mut self,
        channel: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_update_channel",
            "saving update channel",
            move |service| service.set_update_channel(&channel),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "update channel saved: {}",
                    app.state.settings.update_channel
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_sleep_mode(
        &mut self,
        mode: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_sleep_mode(&mode) {
            Ok((settings, power)) => {
                self.apply_settings_summary(settings);
                self.state.power = power;
                self.status_message = format!(
                    "sleep prevention mode saved: {}",
                    self.state.settings.sleep_mode
                );
            }
            Err(error) => self.status_message = format!("failed to save sleep mode: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn select_ai_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = self
            .state
            .settings
            .ai_providers
            .iter()
            .find(|provider| provider.id == provider_id)
        else {
            self.status_message = "AI provider is no longer available".to_string();
            self.normalize_selected_ai_provider();
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        };
        self.selected_ai_provider_id = Some(provider.id.clone());
        self.status_message = format!("selected AI provider: {}", provider.display_name);
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_git_commit_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_git_commit_provider(&provider_id) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.normalize_selected_ai_provider();
                self.status_message = format!(
                    "Git commit provider saved: {}",
                    self.state.settings.git_commit_provider_id
                );
            }
            Err(error) => {
                self.status_message = format!("failed to set Git commit provider: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_speech_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_pet_speech_provider(&provider_id) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet LLM provider saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save pet LLM provider: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_global_prompt(
        &mut self,
        prompt: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_ai_global_prompt(&prompt) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "AI global prompt saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save AI global prompt: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_git_commit_style_rules(
        &mut self,
        rules: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_git_commit_style_rules(&rules) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "Git commit style rules saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save Git commit style rules: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn add_ai_provider(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.add_ai_provider("openAICompatible") {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.selected_ai_provider_id = self
                    .state
                    .settings
                    .ai_providers
                    .last()
                    .map(|provider| provider.id.clone());
                self.normalize_selected_ai_provider();
                self.status_message = "AI provider added".to_string();
            }
            Err(error) => self.status_message = format!("failed to add AI provider: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn remove_ai_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.remove_ai_provider(&provider_id) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                if self.ai_provider_testing_id.as_deref() == Some(provider_id.as_str()) {
                    self.ai_provider_testing_id = None;
                }
                self.normalize_selected_ai_provider();
                self.status_message = "AI provider removed".to_string();
            }
            Err(error) => self.status_message = format!("failed to remove AI provider: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn update_ai_provider_string(
        &mut self,
        provider_id: String,
        key: &'static str,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .update_ai_provider_string(&provider_id, key, &value)
        {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.selected_ai_provider_id = Some(provider_id);
                self.normalize_selected_ai_provider();
                self.status_message = "AI provider saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save AI provider: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_provider_bool(
        &mut self,
        provider_id: String,
        key: &'static str,
        value: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_ai_provider_bool(&provider_id, key, value)
        {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.selected_ai_provider_id = Some(provider_id);
                self.normalize_selected_ai_provider();
                self.status_message = "AI provider saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save AI provider: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn test_ai_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai_provider_testing_id.is_some() {
            self.status_message = "AI provider test is already running".to_string();
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        if !self
            .state
            .settings
            .ai_providers
            .iter()
            .any(|provider| provider.id == provider_id)
        {
            self.status_message = "AI provider not found".to_string();
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }

        let service = self.runtime_service.clone();
        self.ai_provider_testing_id = Some(provider_id.clone());
        self.ai_provider_test_result = None;
        self.selected_ai_provider_id = Some(provider_id.clone());
        self.status_message = "AI provider test started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_provider_id = provider_id.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.test_ai_provider(&worker_provider_id)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_ai_provider_test_result(provider_id, result, cx);
            });
        })
        .detach();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn apply_ai_provider_test_result(
        &mut self,
        provider_id: String,
        result: Result<codux_runtime::llm::LLMProviderTestResult, String>,
        cx: &mut Context<Self>,
    ) {
        if self.ai_provider_testing_id.as_deref() == Some(provider_id.as_str()) {
            self.ai_provider_testing_id = None;
        }
        match result {
            Ok(result) => {
                self.ai_provider_test_result = Some(AIProviderTestResult {
                    provider_id,
                    message: result.text.clone(),
                    ok: true,
                });
                self.status_message = format!(
                    "AI provider test ok: {} · {}",
                    result.provider_name, result.text
                );
            }
            Err(error) => {
                self.ai_provider_test_result = Some(AIProviderTestResult {
                    provider_id,
                    message: error.clone(),
                    ok: false,
                });
                self.status_message = format!("AI provider test failed: {error}");
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_memory_bool(
        &mut self,
        key: &'static str,
        value: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_ai_memory_bool",
            "saving memory setting",
            move |service| service.set_ai_memory_bool(key, value),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = "memory setting saved".to_string();
                prepare_memory_launch_artifacts(&app.runtime_service, &app.state);
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_memory_number(
        &mut self,
        key: &'static str,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_ai_memory_number",
            "saving memory setting",
            move |service| service.set_ai_memory_number(key, &value),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = "memory setting saved".to_string();
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_memory_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_ai_memory_provider",
            "saving memory provider",
            move |service| service.set_ai_memory_provider(&provider_id),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = "memory extraction provider saved".to_string();
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn record_shortcut(
        &mut self,
        shortcut_id: &'static str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.recording_shortcut_id = Some(shortcut_id.to_string());
        self.status_message = "record shortcut, press Esc to cancel".to_string();
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn reset_shortcut(
        &mut self,
        shortcut_id: &'static str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.reset_shortcut(shortcut_id) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                if self.recording_shortcut_id.as_deref() == Some(shortcut_id) {
                    self.recording_shortcut_id = None;
                }
                self.status_message = "shortcut reset".to_string();
            }
            Err(error) => self.status_message = format!("failed to reset shortcut: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_git_commit_tone(
        &mut self,
        tone: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_git_commit_tone",
            "saving Git commit style",
            move |service| service.set_git_commit_tone(&tone),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "Git commit style saved: {}",
                    app.state.settings.git_commit_tone
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_git_commit_language(
        &mut self,
        language: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_settings_async(
            "set_git_commit_language",
            "saving Git commit language",
            move |service| service.set_git_commit_language(&language),
            |app, settings, cx| {
                app.apply_async_settings_summary(settings);
                app.status_message = format!(
                    "Git commit language saved: {}",
                    app.state.settings.git_commit_language
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_runtime_tool_permission(
        &mut self,
        tool_key: &'static str,
        permission: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_settings_task_async(
            "set_runtime_tool_permission",
            "saving runtime tool permission",
            move |service| {
                let settings = service.set_runtime_tool_permission(tool_key, &permission)?;
                let permissions = service.sync_tool_permissions();
                Ok((settings, permissions))
            },
            move |app, (settings, permissions), cx| {
                app.apply_async_settings_summary(settings);
                app.state.tool_permissions = permissions;
                app.status_message = format!("{tool_key} permission saved");
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_runtime_tool_model(
        &mut self,
        model_key: &'static str,
        model: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_settings_task_async(
            "set_runtime_tool_model",
            "saving runtime tool model",
            move |service| {
                let settings = service.set_runtime_tool_model(model_key, &model)?;
                let permissions = service.sync_tool_permissions();
                Ok((settings, permissions))
            },
            move |app, (settings, permissions), cx| {
                app.apply_async_settings_summary(settings);
                app.state.tool_permissions = permissions;
                app.status_message = format!("{model_key} saved");
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_codex_effort(
        &mut self,
        effort: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_settings_task_async(
            "set_codex_effort",
            "saving Codex effort",
            move |service| {
                let settings = service.set_codex_effort(&effort)?;
                let permissions = service.sync_tool_permissions();
                Ok((settings, permissions))
            },
            |app, (settings, permissions), cx| {
                app.apply_async_settings_summary(settings);
                app.state.tool_permissions = permissions;
                app.status_message = format!(
                    "Codex effort saved: {}",
                    app.state.tool_permissions.codex_effort
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            },
            cx,
        );
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn normalize_selected_ai_provider(&mut self) {
        let selected_still_exists = self
            .selected_ai_provider_id
            .as_deref()
            .map(|id| {
                self.state
                    .settings
                    .ai_providers
                    .iter()
                    .any(|provider| provider.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_ai_provider_id = self
                .state
                .settings
                .ai_providers
                .first()
                .map(|provider| provider.id.clone());
        }
    }
}
