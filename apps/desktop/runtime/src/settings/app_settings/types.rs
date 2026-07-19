use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::defaults::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationChannelSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_true")]
    pub shows_dock_badge: bool,
    #[serde(default)]
    pub pet: PetSettings,
    #[serde(default)]
    pub ai: AISettings,
    #[serde(default = "default_sleep_mode")]
    pub sleep_mode: String,
    #[serde(default = "default_git_refresh")]
    pub git_refresh: String,
    #[serde(default = "default_ai_refresh")]
    pub ai_refresh: String,
    #[serde(default = "default_ai_background_refresh")]
    pub ai_background_refresh: String,
    #[serde(default = "default_statistics_mode")]
    pub statistics_mode: String,
    #[serde(default = "default_file_open_default")]
    pub file_open_default: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_theme_color")]
    pub theme_color: String,
    #[serde(default)]
    pub terminal_font_family: String,
    #[serde(default = "default_terminal_font_size")]
    pub terminal_font_size: String,
    #[serde(default = "default_terminal_scrollback_lines")]
    pub terminal_scrollback_lines: String,
    #[serde(default = "default_terminal_paste_images_as_paths")]
    pub terminal_paste_images_as_paths: bool,
    #[serde(default = "default_icon_style")]
    pub icon_style: String,
    #[serde(default = "default_window_style")]
    pub window_style: String,
    #[serde(default = "default_window_opacity")]
    pub window_opacity: String,
    #[serde(default)]
    pub notification_channels: HashMap<String, NotificationChannelSettings>,
    #[serde(default)]
    pub shortcuts: HashMap<String, String>,
    #[serde(default)]
    pub update: UpdateSettings,
    #[serde(default)]
    pub remote: RemoteSettings,
    #[serde(default = "default_true")]
    pub wsl_enabled: bool,
    #[serde(default)]
    pub developer_hud: bool,
    #[serde(default = "default_developer_refresh")]
    pub developer_refresh: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub desktop_widget: bool,
    #[serde(default)]
    pub static_mode: bool,
    #[serde(default)]
    pub reminders: bool,
    #[serde(default)]
    pub sedentary_reminders: bool,
    #[serde(default)]
    pub late_night_reminders: bool,
    #[serde(default = "default_pet_hydration_reminder_minutes")]
    pub hydration_reminder_minutes: String,
    #[serde(default = "default_pet_sedentary_reminder_minutes")]
    pub sedentary_reminder_minutes: String,
    #[serde(default = "default_pet_late_night_reminder_minutes")]
    pub late_night_reminder_minutes: String,
    #[serde(default = "default_pet_speech_mode")]
    pub speech_mode: String,
    #[serde(default = "default_pet_speech_frequency")]
    pub speech_frequency: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AISettings {
    #[serde(default)]
    pub global_prompt: String,
    #[serde(default = "default_git_commit_message_provider_id")]
    pub git_commit_message_provider_id: String,
    #[serde(default = "default_git_commit_message_tone")]
    pub git_commit_message_tone: String,
    #[serde(default = "default_git_commit_message_language")]
    pub git_commit_message_language: String,
    #[serde(default)]
    pub git_commit_message_style_rules: String,
    #[serde(default)]
    pub runtime_tools: AIRuntimeToolSettings,
    #[serde(default)]
    pub memory: AIMemorySettings,
    #[serde(default)]
    pub pet: AIPetSettings,
    #[serde(default)]
    pub providers: Vec<AIProviderSettings>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeToolSettings {
    #[serde(default = "default_ai_tool_permission_mode")]
    pub codex: String,
    #[serde(default = "default_ai_tool_permission_mode")]
    pub claude_code: String,
    #[serde(default = "default_ai_tool_permission_mode")]
    pub agy: String,
    #[serde(default = "default_ai_tool_permission_mode")]
    pub omp: String,
    #[serde(default = "default_ai_tool_permission_mode")]
    pub opencode: String,
    #[serde(default = "default_ai_tool_permission_mode")]
    pub kiro: String,
    #[serde(default = "default_ai_tool_permission_mode")]
    pub codewhale: String,
    #[serde(default = "default_ai_tool_permission_mode")]
    pub kimi: String,
    #[serde(default = "default_ai_tool_permission_mode")]
    pub mimo: String,
    #[serde(default)]
    pub codex_model: String,
    #[serde(default)]
    pub claude_code_model: String,
    #[serde(default)]
    pub agy_model: String,
    #[serde(default)]
    pub omp_model: String,
    #[serde(default)]
    pub opencode_model: String,
    #[serde(default)]
    pub kiro_model: String,
    #[serde(default)]
    pub codewhale_model: String,
    #[serde(default)]
    pub kimi_model: String,
    #[serde(default)]
    pub mimo_model: String,
    #[serde(default = "default_codex_effort")]
    pub codex_effort: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(default = "default_ai_memory_provider_id")]
    pub default_extractor_provider_id: String,
    #[serde(default = "default_memory_user_recall")]
    pub max_injected_user_working_memories: i32,
    #[serde(default = "default_memory_project_recall")]
    pub max_injected_project_working_memories: i32,
    #[serde(default = "default_memory_max_active_working_entries")]
    pub max_active_working_entries: i32,
    #[serde(default = "default_memory_max_summary_versions")]
    pub max_summary_versions: i32,
    #[serde(default = "default_memory_summary_target_token_budget")]
    pub summary_target_token_budget: i32,
    #[serde(default = "default_memory_max_injected_summary_tokens")]
    pub max_injected_summary_tokens: i32,
    #[serde(default = "default_memory_extraction_idle_delay_seconds")]
    pub extraction_idle_delay_seconds: i32,
    #[serde(default = "default_memory_session_extraction_cooldown_seconds")]
    pub session_extraction_cooldown_seconds: i32,
    #[serde(default = "default_true")]
    pub extraction_heuristic_gate_enabled: bool,
    #[serde(default = "default_memory_extraction_growth_threshold_lines")]
    pub extraction_growth_threshold_lines: i32,
    #[serde(default = "default_true")]
    pub recall_use_fts: bool,
    #[serde(default = "default_true")]
    pub privacy_scrub_enabled: bool,
    #[serde(default = "default_memory_max_index_sessions")]
    pub max_index_sessions: i32,
    #[serde(default = "default_memory_max_extraction_transcript_lines")]
    pub max_extraction_transcript_lines: i32,
    #[serde(default = "default_memory_max_extraction_transcript_tokens")]
    pub max_extraction_transcript_tokens: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIPetSettings {
    #[serde(default = "default_ai_pet_speech_mode")]
    pub speech_mode: String,
    #[serde(default = "default_ai_pet_speech_frequency")]
    pub speech_frequency: String,
    #[serde(default)]
    pub speech_llm_enabled: bool,
    #[serde(default = "default_ai_pet_provider_id")]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSettings {
    #[serde(default, rename = "isEnabled")]
    pub is_enabled: bool,
    #[serde(default)]
    pub relay_preset: String,
    #[serde(default)]
    pub relay_url: String,
    #[serde(default)]
    pub relay_authentication: String,
    #[serde(default, rename = "hostID")]
    pub host_id: String,
    #[serde(default)]
    pub host_token: String,
    #[serde(default)]
    pub cached_devices: Vec<RemoteHostDeviceSettings>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHostDeviceSettings {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub host_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub last_seen: String,
    #[serde(default)]
    pub revoked_at: Option<String>,
    #[serde(default)]
    pub online: Option<bool>,
}

include!("types/defaults_impl.rs");
