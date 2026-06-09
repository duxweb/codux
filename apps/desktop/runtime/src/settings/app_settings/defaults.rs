pub(super) fn default_language() -> String {
    "system".to_string()
}

pub(super) fn default_true() -> bool {
    true
}

pub(super) fn default_pet_speech_mode() -> String {
    "mixed".to_string()
}

pub(super) fn default_pet_speech_frequency() -> String {
    "normal".to_string()
}

pub(super) fn default_pet_hydration_reminder_minutes() -> String {
    "60".to_string()
}

pub(super) fn default_pet_sedentary_reminder_minutes() -> String {
    "60".to_string()
}

pub(super) fn default_pet_late_night_reminder_minutes() -> String {
    "60".to_string()
}

pub(super) fn default_ai_pet_speech_mode() -> String {
    "off".to_string()
}

pub(super) fn default_ai_pet_speech_frequency() -> String {
    "normal".to_string()
}

pub(super) fn default_ai_memory_provider_id() -> String {
    "automatic".to_string()
}

pub(super) fn default_ai_pet_provider_id() -> String {
    "automatic".to_string()
}

pub(super) fn default_git_commit_message_provider_id() -> String {
    "automatic".to_string()
}

pub(super) fn default_git_commit_message_tone() -> String {
    "conventional".to_string()
}

pub(super) fn default_git_commit_message_language() -> String {
    "application".to_string()
}

pub(super) fn default_ai_tool_permission_mode() -> String {
    "default".to_string()
}

pub(super) fn default_codex_effort() -> String {
    "medium".to_string()
}

pub(super) fn default_memory_user_recall() -> i32 {
    4
}

pub(super) fn default_memory_project_recall() -> i32 {
    6
}

pub(super) fn default_memory_max_active_working_entries() -> i32 {
    50
}

pub(super) fn default_memory_max_summary_versions() -> i32 {
    10
}

pub(super) fn default_memory_summary_target_token_budget() -> i32 {
    900
}

pub(super) fn default_memory_max_injected_summary_tokens() -> i32 {
    900
}

pub(super) fn default_memory_extraction_idle_delay_seconds() -> i32 {
    300
}

pub(super) fn default_memory_session_extraction_cooldown_seconds() -> i32 {
    900
}

pub(super) fn default_memory_max_index_sessions() -> i32 {
    20
}

pub(super) fn default_memory_max_extraction_transcript_lines() -> i32 {
    80
}

pub(super) fn default_memory_max_extraction_transcript_tokens() -> i32 {
    8000
}

pub(super) fn default_sleep_mode() -> String {
    "off".to_string()
}

pub(super) fn default_git_refresh() -> String {
    "60".to_string()
}

pub(super) fn default_ai_refresh() -> String {
    "180".to_string()
}

pub(super) fn default_ai_background_refresh() -> String {
    "600".to_string()
}

pub(super) fn default_statistics_mode() -> String {
    "normalized".to_string()
}

pub(super) fn default_file_open_default() -> String {
    "edit".to_string()
}

pub(super) fn default_theme() -> String {
    "Auto".to_string()
}

pub(super) fn default_theme_color() -> String {
    "Blue".to_string()
}

pub(super) fn default_terminal_font_size() -> String {
    "14".to_string()
}

pub(super) fn default_terminal_scrollback_lines() -> String {
    "2000".to_string()
}

pub(super) fn default_terminal_paste_images_as_paths() -> bool {
    true
}

pub(super) fn default_icon_style() -> String {
    "default".to_string()
}

pub(super) fn default_developer_refresh() -> String {
    "3".to_string()
}

pub(super) fn default_update_channel() -> &'static str {
    if env!("CARGO_PKG_VERSION").contains('-') {
        "beta"
    } else {
        "stable"
    }
}

pub(super) fn update_endpoint_for_channel(channel: &str) -> String {
    match channel {
        "beta" => "https://raw.githubusercontent.com/duxweb/codux/main/updates/beta/latest.json",
        _ => "https://raw.githubusercontent.com/duxweb/codux/main/updates/stable/latest.json",
    }
    .to_string()
}

pub(super) fn is_managed_update_endpoint(endpoint: &str) -> bool {
    matches!(
        endpoint,
        "https://github.com/duxweb/codux/releases/latest/download/latest.json"
            | "https://github.com/duxweb/codux/releases/download/beta/latest.json"
            | "https://raw.githubusercontent.com/duxweb/codux/main/updates/stable/latest.json"
            | "https://raw.githubusercontent.com/duxweb/codux/main/updates/beta/latest.json"
    )
}
