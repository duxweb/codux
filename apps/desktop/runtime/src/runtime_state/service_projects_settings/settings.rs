impl RuntimeService {
    pub fn set_terminal_scrollback_lines(&self, lines: usize) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_terminal_scrollback_lines(lines)
        })
    }

    pub fn app_settings_snapshot(&self) -> AppSettings {
        AppSettingsStore::from_support_dir(self.support_dir.clone()).snapshot()
    }

    pub fn replace_app_settings(&self, settings: AppSettings) -> Result<AppSettings, String> {
        let previous = AppSettingsStore::from_support_dir(self.support_dir.clone()).snapshot();
        let settings =
            AppSettingsStore::from_support_dir(self.support_dir.clone()).replace(settings)?;
        self.sync_app_settings_side_effects(&previous, &settings)?;
        Ok(settings)
    }

    pub fn update_app_settings(
        &self,
        apply: impl FnOnce(&mut AppSettings),
    ) -> Result<AppSettings, String> {
        let previous = AppSettingsStore::from_support_dir(self.support_dir.clone()).snapshot();
        let settings = AppSettingsStore::from_support_dir(self.support_dir.clone()).update(apply)?;
        self.sync_app_settings_side_effects(&previous, &settings)?;
        Ok(settings)
    }

    fn sync_app_settings_side_effects(
        &self,
        previous: &AppSettings,
        settings: &AppSettings,
    ) -> Result<(), String> {
        if previous.language != settings.language {
            sync_process_locale_preference(settings);
        }
        if previous.icon_style != settings.icon_style {
            let _ = app_icon::apply_app_icon(&settings.icon_style);
        }
        if previous.sleep_mode != settings.sleep_mode {
            self.power_manager
                .set_sleep_prevention(settings.sleep_mode.clone())?;
        }
        if previous.remote != settings.remote {
            let _ = RemoteService::new(self.support_dir.clone()).sync_settings_background();
        }
        Ok(())
    }

    fn update_settings_with_side_effects(
        &self,
        update: impl FnOnce(SettingsService) -> Result<SettingsSummary, String>,
    ) -> Result<SettingsSummary, String> {
        let previous = AppSettingsStore::from_support_dir(self.support_dir.clone()).snapshot();
        let settings = update(SettingsService::new(self.support_dir.clone()))?;
        let app_settings = AppSettingsStore::from_support_dir(self.support_dir.clone()).snapshot();
        self.sync_app_settings_side_effects(&previous, &app_settings)?;
        Ok(settings)
    }

    pub fn set_terminal_font_size(&self, size: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_terminal_font_size(size))
    }

    pub fn set_terminal_font_family(&self, family: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_terminal_font_family(family))
    }

    pub fn set_terminal_scrollback_value(&self, lines: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_terminal_scrollback_value(lines)
        })
    }

    pub fn toggle_terminal_paste_images_as_paths(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.toggle_terminal_paste_images_as_paths()
        })
    }

    pub fn cycle_terminal_font_size(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_terminal_font_size())
    }

    pub fn cycle_terminal_scrollback_lines(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.cycle_terminal_scrollback_lines()
        })
    }

    pub fn cycle_theme(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_theme())
    }

    pub fn set_theme(&self, theme: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_theme(theme))
    }

    pub fn cycle_theme_color(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_theme_color())
    }

    pub fn set_theme_color(&self, theme_color: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_theme_color(theme_color))
    }

    pub fn cycle_icon_style(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_icon_style())
    }

    pub fn set_icon_style(&self, icon_style: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_icon_style(icon_style))
    }

    pub fn toggle_dock_badge(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_dock_badge())
    }

    pub fn cycle_language(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_language())
    }

    pub fn set_language(&self, language: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_language(language))
    }

    pub fn toggle_developer_hud(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_developer_hud())
    }

    pub fn cycle_developer_refresh(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_developer_refresh())
    }

    pub fn set_developer_refresh(&self, seconds: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_developer_refresh(seconds))
    }

    pub fn toggle_memory_enabled(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_memory_enabled())
    }

    pub fn toggle_update_enabled(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_update_enabled())
    }

    pub fn cycle_statistics_mode(&self) -> Result<SettingsSummary, String> {
        SettingsService::new(self.support_dir.clone()).cycle_statistics_mode()
    }

    pub fn set_statistics_mode(&self, mode: &str) -> Result<SettingsSummary, String> {
        SettingsService::new(self.support_dir.clone()).set_statistics_mode(mode)
    }

    pub fn set_file_open_default(&self, mode: &str) -> Result<SettingsSummary, String> {
        SettingsService::new(self.support_dir.clone()).set_file_open_default(mode)
    }

    pub fn cycle_git_refresh(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_git_refresh())
    }

    pub fn set_git_refresh(&self, seconds: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_git_refresh(seconds))
    }

    pub fn cycle_ai_refresh(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_ai_refresh())
    }

    pub fn set_ai_refresh(&self, seconds: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_ai_refresh(seconds))
    }

    pub fn cycle_ai_background_refresh(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_ai_background_refresh())
    }

    pub fn set_ai_background_refresh(&self, seconds: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_ai_background_refresh(seconds)
        })
    }

    pub fn toggle_pet_enabled(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_pet_enabled())
    }

    pub fn toggle_pet_desktop_widget(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_pet_desktop_widget())
    }

    pub fn toggle_pet_static_mode(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_pet_static_mode())
    }

    pub fn toggle_pet_reminders(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_pet_reminders())
    }

    pub fn toggle_pet_sedentary_reminders(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_pet_sedentary_reminders())
    }

    pub fn toggle_pet_late_night_reminders(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_pet_late_night_reminders())
    }

    pub fn set_pet_hydration_reminder_minutes(
        &self,
        minutes: &str,
    ) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_pet_hydration_reminder_minutes(minutes)
        })
    }

    pub fn set_pet_sedentary_reminder_minutes(
        &self,
        minutes: &str,
    ) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_pet_sedentary_reminder_minutes(minutes)
        })
    }

    pub fn set_pet_late_night_reminder_minutes(
        &self,
        minutes: &str,
    ) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_pet_late_night_reminder_minutes(minutes)
        })
    }

    pub fn cycle_pet_speech_mode(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_pet_speech_mode())
    }

    pub fn set_pet_speech_mode(&self, mode: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_pet_speech_mode(mode))
    }

    pub fn cycle_pet_speech_frequency(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_pet_speech_frequency())
    }

    pub fn set_pet_speech_frequency(&self, frequency: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_pet_speech_frequency(frequency)
        })
    }

    pub fn toggle_pet_speech_llm_enabled(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_pet_speech_llm_enabled())
    }

    pub fn toggle_pet_speech_quiet_during_work(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.toggle_pet_speech_quiet_during_work()
        })
    }

    pub fn toggle_pet_speech_louder_at_night(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.toggle_pet_speech_louder_at_night()
        })
    }

    pub fn toggle_pet_speech_mute_on_fullscreen(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.toggle_pet_speech_mute_on_fullscreen()
        })
    }

    pub fn toggle_pet_speech_quiet_hours(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_pet_speech_quiet_hours())
    }

    pub fn toggle_pet_speech_temporary_mute(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.toggle_pet_speech_temporary_mute()
        })
    }

    pub fn set_pet_speech_temporary_mute(&self, muted: bool) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_pet_speech_temporary_mute(muted)
        })
    }

    pub fn cycle_update_channel(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_update_channel())
    }

    pub fn set_update_channel(&self, channel: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_update_channel(channel))
    }

    pub fn toggle_ai_provider(&self, provider_id: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.toggle_ai_provider(provider_id))
    }

    pub fn add_ai_provider(&self, kind: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.add_ai_provider(kind))
    }

    pub fn remove_ai_provider(&self, provider_id: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.remove_ai_provider(provider_id))
    }

    pub fn update_ai_provider_string(
        &self,
        provider_id: &str,
        key: &str,
        value: &str,
    ) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.update_ai_provider_string(provider_id, key, value)
        })
    }

    pub fn set_ai_provider_bool(
        &self,
        provider_id: &str,
        key: &str,
        value: bool,
    ) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_ai_provider_bool(provider_id, key, value)
        })
    }

    pub fn test_ai_provider(&self, provider_id: &str) -> Result<LLMProviderTestResult, String> {
        SettingsService::new(self.support_dir.clone()).test_ai_provider(provider_id)
    }

    pub fn set_pet_speech_provider(&self, provider_id: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_pet_speech_provider(provider_id)
        })
    }

    pub fn set_ai_global_prompt(&self, prompt: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_ai_global_prompt(prompt))
    }

    pub fn set_git_commit_style_rules(&self, rules: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_git_commit_style_rules(rules)
        })
    }

    pub fn set_ai_memory_bool(&self, key: &str, value: bool) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_ai_memory_bool(key, value))
    }

    pub fn set_ai_memory_number(&self, key: &str, value: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_ai_memory_number(key, value))
    }

    pub fn set_ai_memory_provider(&self, provider_id: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_ai_memory_provider(provider_id)
        })
    }

    pub fn set_shortcut(&self, shortcut_id: &str, value: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_shortcut(shortcut_id, value))
    }

    pub fn reset_shortcut(&self, shortcut_id: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.reset_shortcut(shortcut_id))
    }

    pub fn set_git_commit_provider(&self, provider_id: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_git_commit_provider(provider_id)
        })
    }

    pub fn set_runtime_tool_permission(
        &self,
        tool_key: &str,
        permission: &str,
    ) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_runtime_tool_permission(tool_key, permission)
        })
    }

    pub fn set_runtime_tool_model(
        &self,
        model_key: &str,
        model: &str,
    ) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| {
            settings.set_runtime_tool_model(model_key, model)
        })
    }

    pub fn set_codex_effort(&self, effort: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_codex_effort(effort))
    }

    pub fn cycle_git_commit_tone(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_git_commit_tone())
    }

    pub fn set_git_commit_tone(&self, tone: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_git_commit_tone(tone))
    }

    pub fn cycle_git_commit_language(&self) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.cycle_git_commit_language())
    }

    pub fn set_git_commit_language(&self, language: &str) -> Result<SettingsSummary, String> {
        self.update_settings_with_side_effects(|settings| settings.set_git_commit_language(language))
    }
}
