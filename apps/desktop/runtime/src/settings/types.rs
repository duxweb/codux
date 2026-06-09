#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSummary {
    pub language: String,
    pub theme: String,
    pub theme_color: String,
    pub icon_style: String,
    pub shows_dock_badge: bool,
    pub terminal_font_family: String,
    pub terminal_font_size: String,
    pub terminal_scrollback_lines: String,
    pub terminal_paste_images_as_paths: bool,
    pub file_open_default: String,
    pub git_refresh: String,
    pub ai_refresh: String,
    pub ai_background_refresh: String,
    pub statistics_mode: String,
    pub sleep_mode: String,
    pub provider_count: usize,
    pub ai_providers: Vec<AIProviderSummary>,
    pub ai_global_prompt: String,
    pub ai_global_prompt_chars: usize,
    pub git_commit_provider_id: String,
    pub git_commit_tone: String,
    pub git_commit_language: String,
    pub git_commit_style_rules: String,
    pub git_commit_style_rules_chars: usize,
    pub runtime_tool_count: usize,
    pub memory_enabled: bool,
    pub memory_automatic_injection_enabled: bool,
    pub memory_automatic_extraction_enabled: bool,
    pub memory_extraction_idle_delay_seconds: String,
    pub memory_session_extraction_cooldown_seconds: String,
    pub memory_max_index_sessions: String,
    pub memory_max_injected_user_working_memories: String,
    pub memory_max_injected_project_working_memories: String,
    pub memory_max_active_working_entries: String,
    pub memory_max_summary_versions: String,
    pub memory_summary_target_token_budget: String,
    pub memory_max_injected_summary_tokens: String,
    pub memory_max_extraction_transcript_lines: String,
    pub memory_max_extraction_transcript_tokens: String,
    pub memory_allow_cross_project_user_recall: bool,
    pub memory_default_extractor_provider_id: String,
    pub remote_enabled: bool,
    pub remote_relay_preset: String,
    pub remote_server_url: String,
    pub remote_cached_devices: usize,
    pub update_enabled: bool,
    pub update_channel: String,
    pub pet_enabled: bool,
    pub pet_desktop_widget: bool,
    pub pet_static_mode: bool,
    pub pet_reminders: bool,
    pub pet_sedentary_reminders: bool,
    pub pet_late_night_reminders: bool,
    pub pet_hydration_reminder_minutes: String,
    pub pet_sedentary_reminder_minutes: String,
    pub pet_late_night_reminder_minutes: String,
    pub pet_speech_mode: String,
    pub pet_speech_frequency: String,
    pub pet_speech_llm_enabled: bool,
    pub pet_speech_provider_id: String,
    pub pet_speech_quiet_during_work: bool,
    pub pet_speech_louder_at_night: bool,
    pub pet_speech_mute_on_fullscreen: bool,
    pub pet_speech_quiet_hours_enabled: bool,
    pub pet_speech_temporary_muted: bool,
    pub developer_hud: bool,
    pub developer_refresh: String,
    pub shortcuts: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIProviderSummary {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub model: String,
    pub base_url: String,
    pub enabled: bool,
    pub memory_extraction: bool,
    pub priority: i64,
    pub api_key_configured: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AISettings {
    #[serde(default)]
    pub global_prompt: String,
    #[serde(default)]
    pub git_commit_message_provider_id: String,
    #[serde(default)]
    pub git_commit_message_tone: String,
    #[serde(default)]
    pub git_commit_message_language: String,
    #[serde(default)]
    pub git_commit_message_style_rules: String,
    #[serde(default)]
    pub memory: AIMemorySettings,
    #[serde(default)]
    pub pet: AIPetSettings,
    #[serde(default)]
    pub providers: Vec<AIProviderSettings>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIMemorySettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub automatic_injection_enabled: bool,
    #[serde(default = "default_true")]
    pub automatic_extraction_enabled: bool,
    #[serde(default = "default_true")]
    pub allow_cross_project_user_recall: bool,
    #[serde(default)]
    pub default_extractor_provider_id: String,
    #[serde(default)]
    pub max_injected_user_working_memories: i32,
    #[serde(default)]
    pub max_injected_project_working_memories: i32,
    #[serde(default)]
    pub max_active_working_entries: i32,
    #[serde(default)]
    pub max_summary_versions: i32,
    #[serde(default)]
    pub summary_target_token_budget: i32,
    #[serde(default)]
    pub max_injected_summary_tokens: i32,
    #[serde(default)]
    pub extraction_idle_delay_seconds: i32,
    #[serde(default)]
    pub session_extraction_cooldown_seconds: i32,
    #[serde(default)]
    pub max_index_sessions: i32,
    #[serde(default)]
    pub max_extraction_transcript_lines: i32,
    #[serde(default)]
    pub max_extraction_transcript_tokens: i32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIPetSettings {
    #[serde(default)]
    pub speech_mode: String,
    #[serde(default)]
    pub speech_frequency: String,
    #[serde(default)]
    pub speech_llm_enabled: bool,
    #[serde(default)]
    pub speech_provider_id: String,
    #[serde(default = "default_true")]
    pub speech_quiet_during_work: bool,
    #[serde(default)]
    pub speech_louder_at_night: bool,
    #[serde(default = "default_true")]
    pub speech_mute_on_fullscreen: bool,
    #[serde(default)]
    pub speech_quiet_hours_start: Option<i32>,
    #[serde(default)]
    pub speech_quiet_hours_end: Option<i32>,
    #[serde(default)]
    pub speech_temporary_mute_until: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIProviderSettings {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    pub model: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_true")]
    pub use_for_memory_extraction: bool,
    #[serde(default)]
    pub priority: i32,
}

impl Default for AIProviderSettings {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: "openAICompatible".to_string(),
            display_name: String::new(),
            is_enabled: true,
            model: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            use_for_memory_extraction: true,
            priority: 0,
        }
    }
}
