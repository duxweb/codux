#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn defaults_terminal_scrollback_to_2000_lines() {
        let support_dir = temp_dir("settings-default-scrollback");
        let summary = SettingsService::new(support_dir).summary();
        assert_eq!(summary.terminal_scrollback_lines, "2000");
    }

    #[test]
    fn summarizes_ai_providers_without_secret_fields() {
        let support_dir = temp_dir("settings");
        fs::write(
            support_dir.join("settings.json"),
            r#"
            {
              "language": "system",
              "themeColor": "Blue",
              "iconStyle": "default",
              "showsDockBadge": true,
              "terminalFontSize": "14",
              "terminalScrollbackLines": "500",
              "gitRefresh": "60",
              "aiRefresh": "180",
              "aiBackgroundRefresh": "600",
              "statisticsMode": "includingCache",
              "developerRefresh": "3",
              "pet": {
                "enabled": true,
                "desktopWidget": false,
                "staticMode": false,
                "reminders": false,
                "sedentaryReminders": false,
                "lateNightReminders": false,
                "hydrationReminderMinutes": "30",
                "sedentaryReminderMinutes": "90",
                "lateNightReminderMinutes": "120",
                "speechMode": "mixed",
                "speechFrequency": "normal",
                "customField": "keep-me"
              },
              "update": {
                "enabled": false,
                "channel": "stable",
                "endpoint": "file:///tmp/latest.json"
              },
              "ai": {
                "globalPrompt": "  Keep answers concise.  ",
                "gitCommitMessageProviderId": "api-openAICompatible-1",
                "gitCommitMessageTone": "conventional",
                "gitCommitMessageLanguage": "application",
                "gitCommitMessageStyleRules": "  Keep scope clear.  ",
                "memory": {
                  "enabled": true,
                  "automaticInjectionEnabled": true,
                  "automaticExtractionEnabled": true,
                  "allowCrossProjectUserRecall": true,
                  "defaultExtractorProviderId": "automatic",
                  "maxInjectedUserWorkingMemories": 4,
                  "maxInjectedProjectWorkingMemories": 6,
                  "maxActiveWorkingEntries": 50,
                  "maxSummaryVersions": 10,
                  "summaryTargetTokenBudget": 900,
                  "maxInjectedSummaryTokens": 900,
                  "extractionIdleDelaySeconds": 300,
                  "sessionExtractionCooldownSeconds": 900,
                  "maxIndexSessions": 20,
                  "maxExtractionTranscriptLines": 80,
                  "maxExtractionTranscriptTokens": 8000
                },
                "pet": {
                  "speechMode": "off",
                  "speechFrequency": "quiet",
                  "speechLlmEnabled": false,
                  "speechProviderId": "api-openAICompatible-1",
                  "speechQuietDuringWork": true,
                  "speechLouderAtNight": false,
                  "speechMuteOnFullscreen": true,
                  "speechQuietHoursStart": null,
                  "speechQuietHoursEnd": null,
                  "speechTemporaryMuteUntil": null,
                  "customPetSpeechField": "preserve"
                },
                "runtimeTools": {
                  "codex": "default",
                  "claudeCode": "default",
                  "gemini": "default",
                  "opencode": "default",
                  "kiro": "default",
                  "codewhale": "default",
                  "codexModel": "",
                  "claudeCodeModel": "",
                  "geminiModel": "",
                  "opencodeModel": "",
                  "kiroModel": "",
                  "codewhaleModel": "",
                  "codexEffort": "medium"
                },
                "providers": [
                  {
                    "id": "api-openAICompatible-1",
                    "kind": "openAICompatible",
                    "displayName": "DeepSeek",
                    "isEnabled": true,
                    "model": "deepseek-v4-flash",
                    "baseUrl": "https://api.deepseek.com/v1",
                    "apiKey": "secret-value",
                    "useForMemoryExtraction": true,
                    "priority": 2
                  }
                ]
              }
            }
            "#,
        )
        .expect("settings");

        let summary = SettingsService::new(support_dir.clone()).summary();
        assert_eq!(summary.provider_count, 1);
        assert_eq!(summary.theme_color, "Blue");
        assert_eq!(summary.icon_style, "default");
        assert!(summary.shows_dock_badge);
        assert!(summary.pet_enabled);
        assert!(!summary.pet_desktop_widget);
        assert!(!summary.pet_static_mode);
        assert!(!summary.pet_reminders);
        assert!(!summary.pet_sedentary_reminders);
        assert!(!summary.pet_late_night_reminders);
        assert_eq!(summary.pet_hydration_reminder_minutes, "30");
        assert_eq!(summary.pet_sedentary_reminder_minutes, "90");
        assert_eq!(summary.pet_late_night_reminder_minutes, "120");
        assert_eq!(summary.pet_speech_mode, "off");
        assert_eq!(summary.pet_speech_frequency, "quiet");
        assert!(!summary.pet_speech_llm_enabled);
        assert_eq!(summary.pet_speech_provider_id, "api-openAICompatible-1");
        assert!(summary.pet_speech_quiet_during_work);
        assert!(!summary.pet_speech_louder_at_night);
        assert!(summary.pet_speech_mute_on_fullscreen);
        assert!(!summary.pet_speech_quiet_hours_enabled);
        assert!(!summary.pet_speech_temporary_muted);
        assert_eq!(summary.terminal_font_family, "");
        assert_eq!(summary.terminal_font_size, "14");
        assert_eq!(summary.terminal_scrollback_lines, "500");
        assert_eq!(summary.git_refresh, "60");
        assert_eq!(summary.ai_refresh, "180");
        assert_eq!(summary.ai_background_refresh, "600");
        assert_eq!(summary.developer_refresh, "3");
        assert_eq!(summary.statistics_mode, "includingCache");
        assert_eq!(
            summary.ai_global_prompt_chars,
            "Keep answers concise.".chars().count()
        );
        assert_eq!(summary.git_commit_provider_id, "api-openAICompatible-1");
        assert_eq!(summary.git_commit_tone, "conventional");
        assert_eq!(summary.git_commit_language, "application");
        assert_eq!(
            summary.git_commit_style_rules_chars,
            "Keep scope clear.".chars().count()
        );
        assert_eq!(summary.runtime_tool_count, 6);
        assert_eq!(summary.memory_extraction_idle_delay_seconds, "300");
        assert_eq!(summary.memory_session_extraction_cooldown_seconds, "900");
        assert_eq!(summary.memory_max_index_sessions, "20");
        assert_eq!(summary.memory_max_injected_user_working_memories, "4");
        assert_eq!(summary.memory_max_injected_project_working_memories, "6");
        assert_eq!(summary.memory_max_active_working_entries, "50");
        assert_eq!(summary.memory_max_summary_versions, "10");
        assert_eq!(summary.memory_summary_target_token_budget, "900");
        assert_eq!(summary.memory_max_injected_summary_tokens, "900");
        assert_eq!(summary.memory_max_extraction_transcript_lines, "80");
        assert_eq!(summary.memory_max_extraction_transcript_tokens, "8000");
        assert_eq!(summary.sleep_mode, "off");
        let provider = summary.ai_providers.first().expect("provider");
        assert_eq!(provider.display_name, "DeepSeek");
        assert_eq!(provider.kind, "openAICompatible");
        assert_eq!(provider.model, "deepseek-v4-flash");
        assert_eq!(provider.base_url, "https://api.deepseek.com/v1");
        assert!(provider.enabled);
        assert!(provider.memory_extraction);
        assert_eq!(provider.priority, 2);
        let serialized = serde_json::to_string(&summary).expect("serialize");
        assert!(!serialized.contains("secret-value"));
        assert!(!serialized.contains("\"apiKey\""));

        let toggled = SettingsService::new(support_dir.clone())
            .toggle_ai_provider("api-openAICompatible-1")
            .expect("toggle provider");
        assert!(!toggled.ai_providers[0].enabled);
        let updated = fs::read_to_string(support_dir.join("settings.json")).expect("updated");
        assert!(updated.contains("secret-value"));

        let selected = SettingsService::new(support_dir.clone())
            .set_git_commit_provider("api-openAICompatible-1")
            .expect("set git provider");
        assert_eq!(selected.git_commit_provider_id, "api-openAICompatible-1");
        let selected = SettingsService::new(support_dir.clone())
            .set_git_commit_provider("automatic")
            .expect("set automatic git provider");
        assert_eq!(selected.git_commit_provider_id, "automatic");
        let missing = SettingsService::new(support_dir.clone()).toggle_ai_provider("missing");
        assert!(missing.is_err());

        let language = SettingsService::new(support_dir.clone())
            .cycle_language()
            .expect("cycle language");
        assert_eq!(language.language, "english");
        let theme_color = SettingsService::new(support_dir.clone())
            .cycle_theme_color()
            .expect("cycle theme color");
        assert_eq!(theme_color.theme_color, "Sky");
        let icon_style = SettingsService::new(support_dir.clone())
            .cycle_icon_style()
            .expect("cycle icon style");
        assert_eq!(icon_style.icon_style, "cobalt");
        let dock_badge = SettingsService::new(support_dir.clone())
            .toggle_dock_badge()
            .expect("toggle dock badge");
        assert!(!dock_badge.shows_dock_badge);

        let memory = SettingsService::new(support_dir.clone())
            .toggle_memory_enabled()
            .expect("toggle memory");
        assert!(!memory.memory_enabled);
        let updated = fs::read_to_string(support_dir.join("settings.json")).expect("updated");
        assert!(updated.contains("\"automaticInjectionEnabled\": true"));

        let memory_number = SettingsService::new(support_dir.clone())
            .set_ai_memory_number("maxInjectedUserWorkingMemories", "8")
            .expect("set user memory injection count");
        assert_eq!(memory_number.memory_max_injected_user_working_memories, "8");
        let memory_number = SettingsService::new(support_dir.clone())
            .set_ai_memory_number("maxExtractionTranscriptTokens", "12000")
            .expect("set transcript token limit");
        assert_eq!(memory_number.memory_max_extraction_transcript_tokens, "12000");

        let terminal_font = SettingsService::new(support_dir.clone())
            .cycle_terminal_font_size()
            .expect("cycle terminal font size");
        assert_eq!(terminal_font.terminal_font_size, "16");
        let terminal_font_family = SettingsService::new(support_dir.clone())
            .set_terminal_font_family("Menlo")
            .expect("set terminal font family");
        assert_eq!(terminal_font_family.terminal_font_family, "Menlo");
        let scrollback = SettingsService::new(support_dir.clone())
            .cycle_terminal_scrollback_lines()
            .expect("cycle scrollback");
        assert_eq!(scrollback.terminal_scrollback_lines, "1000");
        let git_refresh = SettingsService::new(support_dir.clone())
            .cycle_git_refresh()
            .expect("cycle git refresh");
        assert_eq!(git_refresh.git_refresh, "120");
        let ai_refresh = SettingsService::new(support_dir.clone())
            .cycle_ai_refresh()
            .expect("cycle ai refresh");
        assert_eq!(ai_refresh.ai_refresh, "300");
        let ai_background = SettingsService::new(support_dir.clone())
            .cycle_ai_background_refresh()
            .expect("cycle ai background refresh");
        assert_eq!(ai_background.ai_background_refresh, "900");
        let developer_refresh = SettingsService::new(support_dir.clone())
            .cycle_developer_refresh()
            .expect("cycle developer refresh");
        assert_eq!(developer_refresh.developer_refresh, "5");

        let statistics = SettingsService::new(support_dir.clone())
            .cycle_statistics_mode()
            .expect("cycle statistics mode");
        assert_eq!(statistics.statistics_mode, "normalized");
        let git_tone = SettingsService::new(support_dir.clone())
            .cycle_git_commit_tone()
            .expect("cycle git commit tone");
        assert_eq!(git_tone.git_commit_tone, "concise");
        let git_language = SettingsService::new(support_dir.clone())
            .cycle_git_commit_language()
            .expect("cycle git commit language");
        assert_eq!(git_language.git_commit_language, "english");
        let updated = fs::read_to_string(support_dir.join("settings.json")).expect("updated");
        assert!(updated.contains("\"globalPrompt\": \"  Keep answers concise.  \""));
        assert!(updated.contains("\"gitCommitMessageStyleRules\": \"  Keep scope clear.  \""));

        let update = SettingsService::new(support_dir.clone())
            .toggle_update_enabled()
            .expect("toggle update");
        assert!(update.update_enabled);
        let update = SettingsService::new(support_dir.clone())
            .cycle_update_channel()
            .expect("cycle update channel");
        assert_eq!(update.update_channel, "beta");
        let updated = fs::read_to_string(support_dir.join("settings.json")).expect("updated");
        assert!(updated.contains("\"endpoint\": \"file:///tmp/latest.json\""));

        let pet = SettingsService::new(support_dir.clone())
            .toggle_pet_enabled()
            .expect("toggle pet enabled");
        assert!(!pet.pet_enabled);
        let pet = SettingsService::new(support_dir.clone())
            .toggle_pet_desktop_widget()
            .expect("toggle desktop pet");
        assert!(pet.pet_desktop_widget);
        let pet = SettingsService::new(support_dir.clone())
            .toggle_pet_static_mode()
            .expect("toggle static pet");
        assert!(pet.pet_static_mode);
        let pet = SettingsService::new(support_dir.clone())
            .toggle_pet_reminders()
            .expect("toggle pet reminders");
        assert!(pet.pet_reminders);
        let pet = SettingsService::new(support_dir.clone())
            .toggle_pet_sedentary_reminders()
            .expect("toggle sedentary reminders");
        assert!(pet.pet_sedentary_reminders);
        let pet = SettingsService::new(support_dir.clone())
            .toggle_pet_late_night_reminders()
            .expect("toggle late-night reminders");
        assert!(pet.pet_late_night_reminders);
        let pet = SettingsService::new(support_dir.clone())
            .set_pet_hydration_reminder_minutes("5")
            .expect("set hydration reminder interval");
        assert_eq!(pet.pet_hydration_reminder_minutes, "15");
        let pet = SettingsService::new(support_dir.clone())
            .set_pet_sedentary_reminder_minutes("180")
            .expect("set sedentary reminder interval");
        assert_eq!(pet.pet_sedentary_reminder_minutes, "180");
        let pet = SettingsService::new(support_dir.clone())
            .set_pet_late_night_reminder_minutes("999")
            .expect("set late-night reminder interval");
        assert_eq!(pet.pet_late_night_reminder_minutes, "240");
        let summary = SettingsService::new(support_dir.clone()).summary();
        assert_eq!(summary.pet_hydration_reminder_minutes, "15");
        assert_eq!(summary.pet_sedentary_reminder_minutes, "180");
        assert_eq!(summary.pet_late_night_reminder_minutes, "240");
        let updated = fs::read_to_string(support_dir.join("settings.json")).expect("updated");
        assert!(updated.contains("\"speechMode\": \"mixed\""));
        assert!(updated.contains("\"speechFrequency\": \"normal\""));
        assert!(updated.contains("\"customField\": \"keep-me\""));

        let speech = SettingsService::new(support_dir.clone())
            .cycle_pet_speech_mode()
            .expect("cycle pet speech mode");
        assert_eq!(speech.pet_speech_mode, "mixed");
        let speech = SettingsService::new(support_dir.clone())
            .cycle_pet_speech_frequency()
            .expect("cycle pet speech frequency");
        assert_eq!(speech.pet_speech_frequency, "normal");
        let speech = SettingsService::new(support_dir.clone())
            .toggle_pet_speech_llm_enabled()
            .expect("toggle pet speech llm");
        assert!(speech.pet_speech_llm_enabled);
        let speech = SettingsService::new(support_dir.clone())
            .toggle_pet_speech_quiet_during_work()
            .expect("toggle pet quiet work");
        assert!(!speech.pet_speech_quiet_during_work);
        let speech = SettingsService::new(support_dir.clone())
            .toggle_pet_speech_louder_at_night()
            .expect("toggle pet night speech");
        assert!(speech.pet_speech_louder_at_night);
        let speech = SettingsService::new(support_dir.clone())
            .toggle_pet_speech_mute_on_fullscreen()
            .expect("toggle pet fullscreen mute");
        assert!(!speech.pet_speech_mute_on_fullscreen);
        let speech = SettingsService::new(support_dir.clone())
            .toggle_pet_speech_quiet_hours()
            .expect("toggle pet quiet hours");
        assert!(speech.pet_speech_quiet_hours_enabled);
        let speech = SettingsService::new(support_dir.clone())
            .toggle_pet_speech_temporary_mute()
            .expect("toggle pet temp mute");
        assert!(speech.pet_speech_temporary_muted);
        let updated = fs::read_to_string(support_dir.join("settings.json")).expect("updated");
        assert!(updated.contains("\"speechProviderId\": \"api-openAICompatible-1\""));
        assert!(updated.contains("\"customPetSpeechField\": \"preserve\""));
        assert!(updated.contains("\"apiKey\": \"secret-value\""));

        let sleep = SettingsService::new(support_dir.clone())
            .set_sleep_mode("powerAdapterOnly")
            .expect("set sleep mode");
        assert_eq!(sleep.sleep_mode, "powerAdapterOnly");
        crate::config::flush_all_config_writes();
        let updated = fs::read_to_string(support_dir.join("settings.json")).expect("updated");
        let updated: serde_json::Value = serde_json::from_str(&updated).expect("updated json");
        assert_eq!(
            updated
                .get("sleepMode")
                .and_then(|value| value.as_str()),
            Some("powerAdapterOnly")
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn accepts_all_gpui_shortcut_setting_ids() {
        let support_dir = temp_dir("settings-shortcuts");
        let service = SettingsService::new(support_dir.clone());
        for shortcut_id in [
            "view.terminal",
            "view.files",
            "view.review",
            "project.create",
            "project.open_folder",
            "settings.open",
            "task.create",
            "sidebar.projects.toggle",
            "sidebar.tasks.toggle",
            "assistant.git.open",
            "assistant.files.open",
            "assistant.ai.open",
            "assistant.ssh.open",
            "terminal.split.create",
            "terminal.tab.create",
            "editor.save",
            "editor.search",
            "close.active",
            "panel.git",
            "panel.ai",
            "terminal.split",
            "terminal.tab",
        ] {
            let summary = service
                .set_shortcut(shortcut_id, "Cmd+Shift+P")
                .expect("set shortcut");
            assert_eq!(
                summary.shortcuts.get(shortcut_id),
                Some(&"Cmd+Shift+P".to_string())
            );
            let summary = service.reset_shortcut(shortcut_id).expect("reset shortcut");
            assert!(!summary.shortcuts.contains_key(shortcut_id));
        }

        assert!(service.set_shortcut("unsupported.shortcut", "Cmd+P").is_err());
        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn update_channel_keeps_managed_endpoint_in_sync() {
        let support_dir = temp_dir("settings-update-channel");
        fs::write(
            support_dir.join("settings.json"),
            r#"
            {
              "update": {
                "enabled": true,
                "channel": "stable",
                "endpoint": "https://github.com/duxweb/codux/releases/latest/download/latest.json"
              }
            }
            "#,
        )
        .expect("settings");

        let service = SettingsService::new(support_dir.clone());
        let summary = service
            .set_update_channel("beta")
            .expect("set beta channel");
        assert_eq!(summary.update_channel, "beta");
        crate::config::flush_all_config_writes();
        let updated = fs::read_to_string(support_dir.join("settings.json")).expect("updated");
        assert!(updated.contains(
            "\"endpoint\": \"https://raw.githubusercontent.com/duxweb/codux/main/updates/beta/latest.json\""
        ));

        let summary = service
            .set_update_channel("stable")
            .expect("set stable channel");
        assert_eq!(summary.update_channel, "stable");
        crate::config::flush_all_config_writes();
        let updated = fs::read_to_string(support_dir.join("settings.json")).expect("updated");
        assert!(updated.contains(
            "\"endpoint\": \"https://raw.githubusercontent.com/duxweb/codux/main/updates/stable/latest.json\""
        ));

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn update_channel_keeps_raw_endpoint_in_sync() {
        let support_dir = temp_dir("settings-update-channel-raw");
        fs::write(
            support_dir.join("settings.json"),
            r#"
            {
              "update": {
                "enabled": true,
                "channel": "stable",
                "endpoint": "https://raw.githubusercontent.com/duxweb/codux/main/updates/stable/latest.json"
              }
            }
            "#,
        )
        .expect("settings");

        let service = SettingsService::new(support_dir.clone());
        let summary = service
            .set_update_channel("beta")
            .expect("set beta channel");
        assert_eq!(summary.update_channel, "beta");
        crate::config::flush_all_config_writes();
        let updated = fs::read_to_string(support_dir.join("settings.json")).expect("updated");
        assert!(updated.contains(
            "\"endpoint\": \"https://raw.githubusercontent.com/duxweb/codux/main/updates/beta/latest.json\""
        ));

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_server_url_summary_keeps_empty_value() {
        let support_dir = temp_dir("settings-remote-empty-url");
        fs::write(
            support_dir.join("settings.json"),
            r#"
            {
              "remote": {
                "isEnabled": true,
                "irohRelayURL": "   "
              }
            }
            "#,
        )
        .expect("settings");

        let service = SettingsService::new(support_dir.clone());
        let summary = service.summary();
        assert_eq!(summary.remote_server_url, "");

        let (updated, remote) = crate::runtime_state::RuntimeService::new(support_dir.clone())
            .set_remote_server_url("")
            .expect("update remote server");
        assert_eq!(updated.remote_server_url, "");
        assert_eq!(remote.relay, "");

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_server_url_summary_ignores_legacy_server_url() {
        let support_dir = temp_dir("settings-remote-legacy-url");
        fs::write(
            support_dir.join("settings.json"),
            r#"
            {
              "remote": {
                "isEnabled": true,
                "serverURL": "http://legacy-relay.example"
              }
            }
            "#,
        )
        .expect("settings");

        let summary = SettingsService::new(support_dir.clone()).summary();
        assert_eq!(summary.remote_server_url, "");

        fs::remove_dir_all(support_dir).ok();
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("codux-gpui-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }
}
