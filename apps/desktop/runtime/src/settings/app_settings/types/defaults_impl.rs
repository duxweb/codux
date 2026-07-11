impl Default for UpdateSettings {
    fn default() -> Self {
        let channel = default_update_channel();
        Self {
            enabled: true,
            endpoint: update_endpoint_for_channel(channel),
            channel: channel.to_string(),
        }
    }
}

impl Default for PetSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            desktop_widget: false,
            static_mode: false,
            reminders: false,
            sedentary_reminders: false,
            late_night_reminders: false,
            hydration_reminder_minutes: default_pet_hydration_reminder_minutes(),
            sedentary_reminder_minutes: default_pet_sedentary_reminder_minutes(),
            late_night_reminder_minutes: default_pet_late_night_reminder_minutes(),
            speech_mode: default_pet_speech_mode(),
            speech_frequency: default_pet_speech_frequency(),
        }
    }
}

impl Default for AISettings {
    fn default() -> Self {
        Self {
            global_prompt: String::new(),
            git_commit_message_provider_id: default_git_commit_message_provider_id(),
            git_commit_message_tone: default_git_commit_message_tone(),
            git_commit_message_language: default_git_commit_message_language(),
            git_commit_message_style_rules: String::new(),
            runtime_tools: AIRuntimeToolSettings::default(),
            memory: AIMemorySettings::default(),
            pet: AIPetSettings::default(),
            providers: Vec::new(),
        }
    }
}

impl Default for AIRuntimeToolSettings {
    fn default() -> Self {
        Self {
            codex: default_ai_tool_permission_mode(),
            claude_code: default_ai_tool_permission_mode(),
            agy: default_ai_tool_permission_mode(),
            opencode: default_ai_tool_permission_mode(),
            kiro: default_ai_tool_permission_mode(),
            codewhale: default_ai_tool_permission_mode(),
            kimi: default_ai_tool_permission_mode(),
            mimo: default_ai_tool_permission_mode(),
            codex_model: String::new(),
            claude_code_model: String::new(),
            agy_model: String::new(),
            opencode_model: String::new(),
            kiro_model: String::new(),
            codewhale_model: String::new(),
            kimi_model: String::new(),
            mimo_model: String::new(),
            codex_effort: default_codex_effort(),
        }
    }
}

impl Default for AIMemorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            automatic_injection_enabled: true,
            automatic_extraction_enabled: true,
            allow_cross_project_user_recall: true,
            default_extractor_provider_id: default_ai_memory_provider_id(),
            max_injected_user_working_memories: default_memory_user_recall(),
            max_injected_project_working_memories: default_memory_project_recall(),
            max_active_working_entries: default_memory_max_active_working_entries(),
            max_summary_versions: default_memory_max_summary_versions(),
            summary_target_token_budget: default_memory_summary_target_token_budget(),
            max_injected_summary_tokens: default_memory_max_injected_summary_tokens(),
            extraction_idle_delay_seconds: default_memory_extraction_idle_delay_seconds(),
            session_extraction_cooldown_seconds: default_memory_session_extraction_cooldown_seconds(
            ),
            extraction_heuristic_gate_enabled: true,
            extraction_growth_threshold_lines: default_memory_extraction_growth_threshold_lines(),
            recall_use_fts: true,
            privacy_scrub_enabled: true,
            max_index_sessions: default_memory_max_index_sessions(),
            max_extraction_transcript_lines: default_memory_max_extraction_transcript_lines(),
            max_extraction_transcript_tokens: default_memory_max_extraction_transcript_tokens(),
        }
    }
}

impl Default for AIPetSettings {
    fn default() -> Self {
        Self {
            speech_mode: default_ai_pet_speech_mode(),
            speech_frequency: default_ai_pet_speech_frequency(),
            speech_llm_enabled: false,
            speech_provider_id: default_ai_pet_provider_id(),
            speech_quiet_during_work: true,
            speech_louder_at_night: false,
            speech_mute_on_fullscreen: true,
            speech_quiet_hours_start: None,
            speech_quiet_hours_end: None,
            speech_temporary_mute_until: None,
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: default_language(),
            shows_dock_badge: default_true(),
            pet: PetSettings::default(),
            ai: AISettings::default(),
            sleep_mode: default_sleep_mode(),
            git_refresh: default_git_refresh(),
            ai_refresh: default_ai_refresh(),
            ai_background_refresh: default_ai_background_refresh(),
            statistics_mode: default_statistics_mode(),
            file_open_default: default_file_open_default(),
            theme: default_theme(),
            theme_color: default_theme_color(),
            terminal_font_family: String::new(),
            terminal_font_size: default_terminal_font_size(),
            terminal_scrollback_lines: default_terminal_scrollback_lines(),
            terminal_paste_images_as_paths: default_terminal_paste_images_as_paths(),
            icon_style: default_icon_style(),
            window_style: default_window_style(),
            window_opacity: default_window_opacity(),
            notification_channels: HashMap::new(),
            shortcuts: HashMap::new(),
            update: UpdateSettings::default(),
            remote: RemoteSettings::default(),
            developer_hud: false,
            developer_refresh: default_developer_refresh(),
        }
    }
}
