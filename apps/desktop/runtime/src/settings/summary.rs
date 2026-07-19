fn summary_from_raw(raw: &Map<String, Value>) -> SettingsSummary {
    let defaults = SettingsSummary::default();
    let ai = raw.get("ai").and_then(Value::as_object);
    let memory = ai
        .and_then(|ai| ai.get("memory"))
        .and_then(Value::as_object);
    let ai_pet = ai.and_then(|ai| ai.get("pet")).and_then(Value::as_object);
    let remote = raw.get("remote").and_then(Value::as_object);
    let update = raw.get("update").and_then(Value::as_object);
    let pet = raw.get("pet").and_then(Value::as_object);

    SettingsSummary {
        language: string_value(raw, "language", defaults.language),
        theme: string_value(raw, "theme", defaults.theme),
        theme_color: raw
            .get("themeColor")
            .and_then(Value::as_str)
            .map(sanitize_theme_color)
            .unwrap_or(defaults.theme_color),
        icon_style: raw
            .get("iconStyle")
            .and_then(Value::as_str)
            .map(sanitize_icon_style)
            .unwrap_or(defaults.icon_style),
        window_style: raw
            .get("windowStyle")
            .and_then(Value::as_str)
            .map(sanitize_window_style)
            .unwrap_or(defaults.window_style),
        window_opacity: raw
            .get("windowOpacity")
            .and_then(Value::as_str)
            .map(|value| sanitize_opacity_percent(value, 80))
            .unwrap_or(defaults.window_opacity),
        shows_dock_badge: raw
            .get("showsDockBadge")
            .and_then(Value::as_bool)
            .unwrap_or(defaults.shows_dock_badge),
        terminal_font_family: raw
            .get("terminalFontFamily")
            .and_then(Value::as_str)
            .map(sanitize_terminal_font_family)
            .unwrap_or(defaults.terminal_font_family),
        terminal_font_size: raw
            .get("terminalFontSize")
            .and_then(Value::as_str)
            .map(|value| numeric_string(value, 14, 10, 28).to_string())
            .unwrap_or(defaults.terminal_font_size),
        terminal_padding: raw
            .get("terminalPadding")
            .and_then(Value::as_str)
            .map(|value| numeric_string(value, 10, 0, 40).to_string())
            .unwrap_or(defaults.terminal_padding),
        terminal_line_height: raw
            .get("terminalLineHeight")
            .and_then(Value::as_str)
            .map(|value| float_string(value, 1.45, 1.0, 2.0))
            .unwrap_or(defaults.terminal_line_height),
        terminal_scrollback_lines: raw
            .get("terminalScrollbackLines")
            .and_then(Value::as_str)
            .map(|value| numeric_string(value, 2000, 200, 10_000).to_string())
            .unwrap_or(defaults.terminal_scrollback_lines),
        terminal_paste_images_as_paths: raw
            .get("terminalPasteImagesAsPaths")
            .and_then(Value::as_bool)
            .unwrap_or(defaults.terminal_paste_images_as_paths),
        terminal_shell: string_value(raw, "terminalShell", defaults.terminal_shell),
        file_open_default: raw
            .get("fileOpenDefault")
            .and_then(Value::as_str)
            .map(sanitize_file_open_default)
            .unwrap_or(defaults.file_open_default),
        git_refresh: raw
            .get("gitRefresh")
            .and_then(Value::as_str)
            .map(|value| numeric_string(value, 60, 1, 86_400).to_string())
            .unwrap_or(defaults.git_refresh),
        ai_refresh: raw
            .get("aiRefresh")
            .and_then(Value::as_str)
            .map(|value| numeric_string(value, 180, 1, 86_400).to_string())
            .unwrap_or(defaults.ai_refresh),
        ai_background_refresh: raw
            .get("aiBackgroundRefresh")
            .and_then(Value::as_str)
            .map(|value| numeric_string(value, 600, 1, 86_400).to_string())
            .unwrap_or(defaults.ai_background_refresh),
        statistics_mode: raw
            .get("statisticsMode")
            .and_then(Value::as_str)
            .map(sanitize_statistics_mode)
            .unwrap_or(defaults.statistics_mode),
        sleep_mode: string_value(raw, "sleepMode", defaults.sleep_mode),
        provider_count: ai
            .and_then(|ai| ai.get("providers"))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
        ai_providers: ai
            .and_then(|ai| ai.get("providers"))
            .and_then(Value::as_array)
            .map(|providers| providers.iter().filter_map(provider_summary).collect())
            .unwrap_or_default(),
        ai_global_prompt: ai
            .and_then(|ai| ai.get("globalPrompt"))
            .and_then(Value::as_str)
            .map(|value| value.chars().take(20_000).collect())
            .unwrap_or(defaults.ai_global_prompt),
        ai_global_prompt_chars: ai
            .and_then(|ai| ai.get("globalPrompt"))
            .and_then(Value::as_str)
            .map(|value| bounded_trimmed_chars(value, 20_000))
            .unwrap_or(0),
        git_commit_provider_id: ai
            .and_then(|ai| ai.get("gitCommitMessageProviderId"))
            .and_then(Value::as_str)
            .unwrap_or("automatic")
            .to_string(),
        git_commit_tone: ai
            .and_then(|ai| ai.get("gitCommitMessageTone"))
            .and_then(Value::as_str)
            .map(sanitize_git_commit_tone)
            .unwrap_or(defaults.git_commit_tone),
        git_commit_language: ai
            .and_then(|ai| ai.get("gitCommitMessageLanguage"))
            .and_then(Value::as_str)
            .map(sanitize_git_commit_language)
            .unwrap_or(defaults.git_commit_language),
        git_commit_style_rules: ai
            .and_then(|ai| ai.get("gitCommitMessageStyleRules"))
            .and_then(Value::as_str)
            .map(|value| value.chars().take(4_000).collect())
            .unwrap_or(defaults.git_commit_style_rules),
        git_commit_style_rules_chars: ai
            .and_then(|ai| ai.get("gitCommitMessageStyleRules"))
            .and_then(Value::as_str)
            .map(|value| bounded_trimmed_chars(value, 4_000))
            .unwrap_or(0),
        runtime_tool_count: ai
            .and_then(|ai| ai.get("runtimeTools"))
            .map(runtime_tool_count)
            .unwrap_or(defaults.runtime_tool_count),
        memory_enabled: memory
            .and_then(|memory| memory.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.memory_enabled),
        memory_automatic_injection_enabled: memory
            .and_then(|memory| memory.get("automaticInjectionEnabled"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.memory_automatic_injection_enabled),
        memory_automatic_extraction_enabled: memory
            .and_then(|memory| memory.get("automaticExtractionEnabled"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.memory_automatic_extraction_enabled),
        memory_extraction_idle_delay_seconds: memory
            .and_then(|memory| memory.get("extractionIdleDelaySeconds"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_extraction_idle_delay_seconds),
        memory_session_extraction_cooldown_seconds: memory
            .and_then(|memory| memory.get("sessionExtractionCooldownSeconds"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_session_extraction_cooldown_seconds),
        memory_extraction_heuristic_gate_enabled: memory
            .and_then(|memory| memory.get("extractionHeuristicGateEnabled"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.memory_extraction_heuristic_gate_enabled),
        memory_extraction_growth_threshold_lines: memory
            .and_then(|memory| memory.get("extractionGrowthThresholdLines"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_extraction_growth_threshold_lines),
        memory_recall_use_fts: memory
            .and_then(|memory| memory.get("recallUseFts"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.memory_recall_use_fts),
        memory_privacy_scrub_enabled: memory
            .and_then(|memory| memory.get("privacyScrubEnabled"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.memory_privacy_scrub_enabled),
        memory_max_index_sessions: memory
            .and_then(|memory| memory.get("maxIndexSessions"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_max_index_sessions),
        memory_max_injected_user_working_memories: memory
            .and_then(|memory| memory.get("maxInjectedUserWorkingMemories"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_max_injected_user_working_memories),
        memory_max_injected_project_working_memories: memory
            .and_then(|memory| memory.get("maxInjectedProjectWorkingMemories"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_max_injected_project_working_memories),
        memory_max_active_working_entries: memory
            .and_then(|memory| memory.get("maxActiveWorkingEntries"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_max_active_working_entries),
        memory_max_summary_versions: memory
            .and_then(|memory| memory.get("maxSummaryVersions"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_max_summary_versions),
        memory_summary_target_token_budget: memory
            .and_then(|memory| memory.get("summaryTargetTokenBudget"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_summary_target_token_budget),
        memory_max_injected_summary_tokens: memory
            .and_then(|memory| memory.get("maxInjectedSummaryTokens"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_max_injected_summary_tokens),
        memory_max_extraction_transcript_lines: memory
            .and_then(|memory| memory.get("maxExtractionTranscriptLines"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_max_extraction_transcript_lines),
        memory_max_extraction_transcript_tokens: memory
            .and_then(|memory| memory.get("maxExtractionTranscriptTokens"))
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .unwrap_or(defaults.memory_max_extraction_transcript_tokens),
        memory_allow_cross_project_user_recall: memory
            .and_then(|memory| memory.get("allowCrossProjectUserRecall"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.memory_allow_cross_project_user_recall),
        memory_default_extractor_provider_id: memory
            .and_then(|memory| memory.get("defaultExtractorProviderId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or(defaults.memory_default_extractor_provider_id),
        remote_enabled: remote
            .and_then(|remote| remote.get("isEnabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        remote_relay_preset: remote
            .map(|remote| {
                let relay_url = remote
                    .get("relayUrl")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let preset = remote
                    .get("relayPreset")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_default();
                crate::remote::normalize_remote_relay_preset(preset, relay_url)
            })
            .unwrap_or_else(|| crate::remote::remote_relay_preset_for_url("")),
        remote_relay_url: remote
            .and_then(|remote| remote.get("relayUrl"))
            .and_then(Value::as_str)
            .map(str::trim)
            .map(|value| {
                if value.is_empty() {
                    crate::remote::remote_relay_url_for_preset("", "")
                } else {
                    value.to_string()
                }
            })
            .unwrap_or_else(|| crate::remote::remote_relay_url_for_preset("", "")),
        remote_relay_authentication: remote
            .and_then(|remote| remote.get("relayAuthentication"))
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string(),
        remote_cached_devices: remote
            .and_then(|remote| remote.get("cachedDevices"))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
        update_enabled: update
            .and_then(|update| update.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.update_enabled),
        update_channel: update
            .and_then(|update| update.get("channel"))
            .and_then(Value::as_str)
            .unwrap_or("stable")
            .to_string(),
        pet_enabled: pet
            .and_then(|pet| pet.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.pet_enabled),
        pet_desktop_widget: pet
            .and_then(|pet| pet.get("desktopWidget"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.pet_desktop_widget),
        pet_static_mode: pet
            .and_then(|pet| pet.get("staticMode"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.pet_static_mode),
        pet_reminders: pet
            .and_then(|pet| pet.get("reminders"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.pet_reminders),
        pet_sedentary_reminders: pet
            .and_then(|pet| pet.get("sedentaryReminders"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.pet_sedentary_reminders),
        pet_late_night_reminders: pet
            .and_then(|pet| pet.get("lateNightReminders"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.pet_late_night_reminders),
        pet_hydration_reminder_minutes: pet
            .and_then(|pet| pet.get("hydrationReminderMinutes"))
            .and_then(Value::as_str)
            .map(|value| numeric_string(value, 60, 15, 240).to_string())
            .unwrap_or(defaults.pet_hydration_reminder_minutes),
        pet_sedentary_reminder_minutes: pet
            .and_then(|pet| pet.get("sedentaryReminderMinutes"))
            .and_then(Value::as_str)
            .map(|value| numeric_string(value, 60, 15, 240).to_string())
            .unwrap_or(defaults.pet_sedentary_reminder_minutes),
        pet_late_night_reminder_minutes: pet
            .and_then(|pet| pet.get("lateNightReminderMinutes"))
            .and_then(Value::as_str)
            .map(|value| numeric_string(value, 60, 15, 240).to_string())
            .unwrap_or(defaults.pet_late_night_reminder_minutes),
        pet_speech_mode: ai_pet
            .and_then(|pet| pet.get("speechMode"))
            .and_then(Value::as_str)
            .map(sanitize_pet_speech_mode)
            .unwrap_or(defaults.pet_speech_mode),
        pet_speech_frequency: ai_pet
            .and_then(|pet| pet.get("speechFrequency"))
            .and_then(Value::as_str)
            .map(sanitize_pet_speech_frequency)
            .unwrap_or(defaults.pet_speech_frequency),
        pet_speech_llm_enabled: ai_pet
            .and_then(|pet| pet.get("speechLlmEnabled"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.pet_speech_llm_enabled),
        pet_speech_provider_id: ai_pet
            .and_then(|pet| pet.get("speechProviderId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or(defaults.pet_speech_provider_id),
        pet_speech_quiet_during_work: ai_pet
            .and_then(|pet| pet.get("speechQuietDuringWork"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.pet_speech_quiet_during_work),
        pet_speech_louder_at_night: ai_pet
            .and_then(|pet| pet.get("speechLouderAtNight"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.pet_speech_louder_at_night),
        pet_speech_mute_on_fullscreen: ai_pet
            .and_then(|pet| pet.get("speechMuteOnFullscreen"))
            .and_then(Value::as_bool)
            .unwrap_or(defaults.pet_speech_mute_on_fullscreen),
        pet_speech_quiet_hours_enabled: ai_pet
            .and_then(|pet| pet.get("speechQuietHoursStart"))
            .filter(|value| !value.is_null())
            .is_some()
            && ai_pet
                .and_then(|pet| pet.get("speechQuietHoursEnd"))
                .filter(|value| !value.is_null())
                .is_some(),
        pet_speech_temporary_muted: ai_pet
            .and_then(|pet| pet.get("speechTemporaryMuteUntil"))
            .and_then(Value::as_i64)
            .map(|until| until > current_unix_seconds())
            .unwrap_or(defaults.pet_speech_temporary_muted),
        wsl_enabled: raw
            .get("wslEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(defaults.wsl_enabled),
        developer_hud: raw
            .get("developerHud")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        developer_refresh: raw
            .get("developerRefresh")
            .and_then(Value::as_str)
            .map(|value| numeric_string(value, 3, 1, 86_400).to_string())
            .unwrap_or(defaults.developer_refresh),
        shortcuts: raw
            .get("shortcuts")
            .and_then(Value::as_object)
            .map(|shortcuts| {
                shortcuts
                    .iter()
                    .filter_map(|(id, value)| {
                        value
                            .as_str()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(|value| (id.clone(), value.chars().take(120).collect()))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn provider_summary(value: &Value) -> Option<AIProviderSummary> {
    let provider = value.as_object()?;
    Some(AIProviderSummary {
        id: string_from_value(provider, "id", "unknown"),
        kind: string_from_value(provider, "kind", "unknown"),
        display_name: string_from_value(provider, "displayName", "Untitled provider"),
        model: string_from_value(provider, "model", "default"),
        base_url: string_from_value(provider, "baseUrl", ""),
        enabled: provider
            .get("isEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        memory_extraction: provider
            .get("useForMemoryExtraction")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        priority: provider
            .get("priority")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        api_key_configured: provider
            .get("apiKey")
            .and_then(Value::as_str)
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false),
    })
}

fn runtime_tool_count(value: &Value) -> usize {
    const TOOL_KEYS: [&str; 9] = [
        "codex",
        "claudeCode",
        "agy",
        "omp",
        "opencode",
        "kiro",
        "codewhale",
        "kimi",
        "mimo",
    ];
    match value {
        Value::Object(tools) => TOOL_KEYS
            .iter()
            .filter(|key| tools.contains_key(**key))
            .count()
            .max(TOOL_KEYS.len()),
        Value::Array(tools) => tools.len(),
        _ => 0,
    }
}

fn string_from_value(raw: &Map<String, Value>, key: &str, fallback: &str) -> String {
    raw.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

fn string_value(raw: &Map<String, Value>, key: &str, fallback: String) -> String {
    raw.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or(fallback)
}
