use super::*;

impl CoduxApp {
    pub(super) fn set_terminal_font_size(
        &mut self,
        size: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_terminal_font_size(&size) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.apply_terminal_text_settings(cx);
                self.status_message = format!(
                    "terminal font size saved: {}",
                    self.state.settings.terminal_font_size
                );
            }
            Err(error) => self.status_message = format!("failed to save font size: {error}"),
        }
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
        match self.runtime_service.set_terminal_scrollback_value(&lines) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "terminal scrollback saved: {}",
                    self.state.settings.terminal_scrollback_lines
                );
            }
            Err(error) => self.status_message = format!("failed to save scrollback: {error}"),
        }
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
    }

    pub(super) fn apply_settings_summary_local(&mut self, settings: SettingsSummary) {
        self.state.settings = settings_with_active_restart_locked_values(&settings);
    }

    pub(super) fn apply_settings_summary(&mut self, settings: SettingsSummary) {
        self.replace_settings_summary(settings);
        let revision = publish_settings_update();
        publish_child_window_update(ChildWindowUpdateKind::Settings);
        if revision > 0 {
            self.settings_seen_revision = revision;
        }
    }

    pub(super) fn set_theme(&mut self, theme: String, window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.set_theme(&theme) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
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
        match self.runtime_service.set_theme_color(&theme_color) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
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
        match self.runtime_service.set_icon_style(&icon_style) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                let _ = codux_runtime::app_icon::apply_app_icon(&self.state.settings.icon_style);
                self.status_message =
                    format!("icon style saved: {}", self.state.settings.icon_style);
            }
            Err(error) => self.status_message = format!("failed to save icon style: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_dock_badge(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_dock_badge() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "dock badge saved: {}",
                    if self.state.settings.shows_dock_badge {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save dock badge: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_language(
        &mut self,
        language: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_language(&language) {
            Ok(settings) => {
                self.state.settings = settings_with_active_restart_locked_values(&settings);
                self.status_message = "language saved. Restart Codux to apply it.".to_string();
            }
            Err(error) => self.status_message = format!("failed to save language: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_shell(
        &mut self,
        shell: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_shell(&shell) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!("shell saved: {}", self.state.settings.shell);
            }
            Err(error) => self.status_message = format!("failed to save shell: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_developer_hud(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_developer_hud() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                if self.state.settings.developer_hud {
                    self.state.performance = self.runtime_service.reload_performance();
                }
                self.normalize_selected_ai_provider();
                self.status_message = "developer HUD setting saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save developer HUD: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_developer_refresh(
        &mut self,
        seconds: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_developer_refresh(&seconds) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "developer refresh saved: {}",
                    self.state.settings.developer_refresh
                );
            }
            Err(error) => {
                self.status_message = format!("failed to save developer refresh: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_update_enabled(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_update_enabled() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.state.update = self
                    .runtime_service
                    .reload_update_settings(std::env::current_dir().unwrap_or_default());
                self.status_message = format!(
                    "update setting saved: {}",
                    if self.state.settings.update_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save update setting: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_statistics_mode(
        &mut self,
        mode: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_statistics_mode(&mode) {
            Ok(settings) => {
                self.apply_settings_summary_local(settings);
                let revision = publish_statistics_settings_update();
                publish_child_window_update(ChildWindowUpdateKind::Settings);
                if revision > 0 {
                    self.settings_seen_revision = revision;
                }
                self.status_message = format!(
                    "AI statistics mode saved: {}",
                    self.state.settings.statistics_mode
                );
            }
            Err(error) => {
                self.status_message = format!("failed to save AI statistics mode: {error}")
            }
        }
        if self.window_mode == AppWindowMode::Main {
            self.invalidate_ui(
                cx,
                [
                    UiRegion::WorkspaceAssistant,
                    UiRegion::AIStatsSidebar,
                    UiRegion::StatusBar,
                ],
            );
        } else {
            self.invalidate_ui_region(cx, UiRegion::Root);
        }
    }

    pub(super) fn set_git_refresh(
        &mut self,
        seconds: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_git_refresh(&seconds) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message =
                    format!("Git refresh saved: {}", self.state.settings.git_refresh);
            }
            Err(error) => self.status_message = format!("failed to save Git refresh: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_refresh(
        &mut self,
        seconds: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_ai_refresh(&seconds) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message =
                    format!("AI refresh saved: {}", self.state.settings.ai_refresh);
            }
            Err(error) => self.status_message = format!("failed to save AI refresh: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_background_refresh(
        &mut self,
        seconds: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_ai_background_refresh(&seconds) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "AI background refresh saved: {}",
                    self.state.settings.ai_background_refresh
                );
            }
            Err(error) => {
                self.status_message = format!("failed to save AI background refresh: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_enabled(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_pet_enabled() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.sync_desktop_pet_window(false, cx);
                self.status_message = format!(
                    "pet setting saved: {}",
                    if self.state.settings.pet_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save pet setting: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_desktop_widget(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.toggle_pet_desktop_widget() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                let enabled = self.state.settings.pet_desktop_widget;
                self.status_message = format!(
                    "desktop pet setting saved: {}",
                    if enabled { "on" } else { "off" }
                );
                if self.window_mode == AppWindowMode::Main {
                    self.sync_desktop_pet_window(false, cx);
                }
            }
            Err(error) => {
                self.status_message = format!("failed to save desktop pet setting: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_static_mode(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_pet_static_mode() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet static mode saved: {}",
                    if self.state.settings.pet_static_mode {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save pet static mode: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_reminders(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.runtime_service.toggle_pet_reminders() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet reminders saved: {}",
                    if self.state.settings.pet_reminders {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save pet reminders: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_speech_mode(
        &mut self,
        mode: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_pet_speech_mode(&mode) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet speech mode saved: {}",
                    self.state.settings.pet_speech_mode
                );
            }
            Err(error) => self.status_message = format!("failed to save pet speech mode: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_speech_frequency(
        &mut self,
        frequency: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_pet_speech_frequency(&frequency) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet speech frequency saved: {}",
                    self.state.settings.pet_speech_frequency
                );
            }
            Err(error) => {
                self.status_message = format!("failed to save pet speech frequency: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_speech_llm_enabled(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.toggle_pet_speech_llm_enabled() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "pet speech LLM saved: {}",
                    if self.state.settings.pet_speech_llm_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Err(error) => self.status_message = format!("failed to save pet speech LLM: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_speech_quiet_during_work(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.toggle_pet_speech_quiet_during_work() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet speech work-hours setting saved".to_string();
            }
            Err(error) => {
                self.status_message =
                    format!("failed to save pet speech work-hours setting: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_speech_louder_at_night(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.toggle_pet_speech_louder_at_night() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet speech night setting saved".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to save pet speech night setting: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_speech_mute_on_fullscreen(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.toggle_pet_speech_mute_on_fullscreen() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet speech fullscreen setting saved".to_string();
            }
            Err(error) => {
                self.status_message =
                    format!("failed to save pet speech fullscreen setting: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn toggle_pet_speech_quiet_hours(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.toggle_pet_speech_quiet_hours() {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet speech quiet-hours setting saved".to_string();
            }
            Err(error) => {
                self.status_message =
                    format!("failed to save pet speech quiet-hours setting: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_pet_speech_temporary_mute(&mut self, muted: bool, cx: &mut Context<Self>) {
        match self.runtime_service.set_pet_speech_temporary_mute(muted) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "pet speech temporary mute setting saved".to_string();
            }
            Err(error) => {
                self.status_message =
                    format!("failed to save pet speech temporary mute setting: {error}")
            }
        }
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
        match self.runtime_service.set_update_channel(&channel) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.state.update = self
                    .runtime_service
                    .reload_update_settings(std::env::current_dir().unwrap_or_default());
                self.status_message = format!(
                    "update channel saved: {}",
                    self.state.settings.update_channel
                );
            }
            Err(error) => self.status_message = format!("failed to save update channel: {error}"),
        }
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
        match self.runtime_service.set_ai_memory_bool(key, value) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "memory setting saved".to_string();
                prepare_memory_launch_artifacts(&self.state);
            }
            Err(error) => self.status_message = format!("failed to save memory setting: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_memory_number(
        &mut self,
        key: &'static str,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_ai_memory_number(key, &value) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "memory setting saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save memory setting: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_ai_memory_provider(
        &mut self,
        provider_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_ai_memory_provider(&provider_id) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = "memory extraction provider saved".to_string();
            }
            Err(error) => self.status_message = format!("failed to save memory provider: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_agent_split_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.agent_split_enabled = enabled;
        self.status_message = format!(
            "agent split setting saved: {}",
            if enabled { "on" } else { "off" }
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
        match self.runtime_service.set_git_commit_tone(&tone) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "Git commit style saved: {}",
                    self.state.settings.git_commit_tone
                );
            }
            Err(error) => self.status_message = format!("failed to save Git commit style: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_git_commit_language(
        &mut self,
        language: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_git_commit_language(&language) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.status_message = format!(
                    "Git commit language saved: {}",
                    self.state.settings.git_commit_language
                );
            }
            Err(error) => {
                self.status_message = format!("failed to save Git commit language: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_runtime_tool_permission(
        &mut self,
        tool_key: &'static str,
        permission: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_runtime_tool_permission(tool_key, &permission)
        {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
                self.status_message = format!("{tool_key} permission saved");
            }
            Err(error) => {
                self.status_message = format!("failed to save {tool_key} permission: {error}")
            }
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_runtime_tool_model(
        &mut self,
        model_key: &'static str,
        model: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .set_runtime_tool_model(model_key, &model)
        {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
                self.status_message = format!("{model_key} saved");
            }
            Err(error) => self.status_message = format!("failed to save {model_key}: {error}"),
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(super) fn set_codex_effort(
        &mut self,
        effort: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.set_codex_effort(&effort) {
            Ok(settings) => {
                self.apply_settings_summary(settings);
                self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
                self.status_message = format!(
                    "Codex effort saved: {}",
                    self.state.tool_permissions.codex_effort
                );
            }
            Err(error) => self.status_message = format!("failed to save Codex effort: {error}"),
        }
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
