use crate::notify_channels::NotificationChannelConfig;
use crate::paths::app_support_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationChannelSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_shell")]
    pub shell: String,
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
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default = "default_terminal_font_size")]
    pub terminal_font_size: String,
    #[serde(default = "default_icon_style")]
    pub icon_style: String,
    #[serde(default)]
    pub notification_channels: HashMap<String, NotificationChannelSettings>,
    #[serde(default)]
    pub shortcuts: HashMap<String, String>,
    #[serde(default)]
    pub update: UpdateSettings,
    #[serde(default)]
    pub remote: RemoteSettings,
    #[serde(default)]
    pub developer_hud: bool,
    #[serde(default = "default_developer_refresh")]
    pub developer_refresh: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default = "default_pet_speech_mode")]
    pub speech_mode: String,
    #[serde(default = "default_pet_speech_frequency")]
    pub speech_frequency: String,
    #[serde(default = "default_pet_desktop_scale")]
    pub desktop_scale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AISettings {
    #[serde(default)]
    pub global_prompt: String,
    #[serde(default)]
    pub memory: AIMemorySettings,
    #[serde(default)]
    pub pet: AIPetSettings,
    #[serde(default)]
    pub providers: Vec<AIProviderSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default = "default_memory_max_extraction_transcript_lines")]
    pub max_extraction_transcript_lines: i32,
    #[serde(default = "default_memory_max_extraction_transcript_tokens")]
    pub max_extraction_transcript_tokens: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub relay_url: String,
}

impl Default for RemoteSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            relay_url: String::new(),
        }
    }
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            channel: "stable".to_string(),
            endpoint: default_update_endpoint(),
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
            speech_mode: default_pet_speech_mode(),
            speech_frequency: default_pet_speech_frequency(),
            desktop_scale: default_pet_desktop_scale(),
        }
    }
}

impl Default for AISettings {
    fn default() -> Self {
        Self {
            global_prompt: String::new(),
            memory: AIMemorySettings::default(),
            pet: AIPetSettings::default(),
            providers: Vec::new(),
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
            shell: default_shell(),
            shows_dock_badge: default_true(),
            pet: PetSettings::default(),
            ai: AISettings::default(),
            sleep_mode: default_sleep_mode(),
            git_refresh: default_git_refresh(),
            ai_refresh: default_ai_refresh(),
            ai_background_refresh: default_ai_background_refresh(),
            statistics_mode: default_statistics_mode(),
            theme: default_theme(),
            background: default_background(),
            terminal_font_size: default_terminal_font_size(),
            icon_style: default_icon_style(),
            notification_channels: HashMap::new(),
            shortcuts: HashMap::new(),
            update: UpdateSettings::default(),
            remote: RemoteSettings::default(),
            developer_hud: false,
            developer_refresh: default_developer_refresh(),
        }
    }
}

pub struct AppSettingsStore {
    settings: Mutex<AppSettings>,
    state_file: PathBuf,
}

impl AppSettingsStore {
    pub fn load_or_seed() -> Self {
        let state_file = settings_file_path();
        let settings = load_settings(&state_file).unwrap_or_default();
        let store = Self {
            settings: Mutex::new(sanitize_settings(settings)),
            state_file,
        };
        let _ = store.save();
        store
    }

    pub fn snapshot(&self) -> AppSettings {
        self.settings
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_default()
    }

    pub fn replace(&self, next: AppSettings) -> Result<AppSettings, String> {
        let next = sanitize_settings(next);
        {
            let mut settings = self
                .settings
                .lock()
                .map_err(|_| "App settings lock poisoned.".to_string())?;
            *settings = next.clone();
        }
        self.save()?;
        Ok(next)
    }

    pub fn update(&self, apply: impl FnOnce(&mut AppSettings)) -> Result<AppSettings, String> {
        let next = {
            let mut settings = self
                .settings
                .lock()
                .map_err(|_| "App settings lock poisoned.".to_string())?;
            apply(&mut settings);
            let next = sanitize_settings((*settings).clone());
            *settings = next.clone();
            next
        };
        self.save()?;
        Ok(next)
    }

    pub fn configured_notification_channels(&self) -> Vec<NotificationChannelConfig> {
        self.snapshot()
            .notification_channels
            .into_iter()
            .filter_map(|(id, channel)| {
                let endpoint = channel.endpoint.trim().to_string();
                if !channel.enabled || endpoint.is_empty() {
                    return None;
                }
                Some(NotificationChannelConfig {
                    id,
                    endpoint,
                    token: channel.token.trim().to_string(),
                })
            })
            .collect()
    }

    fn save(&self) -> Result<(), String> {
        let settings = self.snapshot();
        if let Some(parent) = self.state_file.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let data = serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?;
        if fs::read(&self.state_file).ok().as_deref() == Some(data.as_slice()) {
            return Ok(());
        }
        fs::write(&self.state_file, data).map_err(|error| error.to_string())
    }
}

pub fn locale_from_language_setting(language: &str) -> String {
    match language {
        "english" => "en",
        "simplifiedChinese" => "zh-Hans",
        "traditionalChinese" => "zh-Hant",
        "japanese" => "ja",
        "korean" => "ko",
        "french" => "fr",
        "german" => "de",
        "spanish" => "es",
        "portugueseBrazil" => "pt-BR",
        "russian" => "ru",
        _ => locale_from_system_setting(),
    }
    .to_string()
}

pub fn sync_process_locale_preference(settings: &AppSettings) {
    #[cfg(target_os = "macos")]
    {
        macos_sync_process_locale_preference(&settings.language);
    }
}

fn locale_from_system_setting() -> &'static str {
    let Some(locale) = sys_locale::get_locale() else {
        return "en";
    };
    locale_from_system_locale(&locale)
}

fn locale_from_system_locale(locale: &str) -> &'static str {
    let normalized = locale.replace('_', "-").to_lowercase();
    if normalized.starts_with("zh-tw")
        || normalized.starts_with("zh-hk")
        || normalized.starts_with("zh-mo")
    {
        return "zh-Hant";
    }
    if normalized.starts_with("zh") {
        return "zh-Hans";
    }
    if normalized.starts_with("ja") {
        return "ja";
    }
    if normalized.starts_with("ko") {
        return "ko";
    }
    if normalized.starts_with("fr") {
        return "fr";
    }
    if normalized.starts_with("de") {
        return "de";
    }
    if normalized.starts_with("es") {
        return "es";
    }
    if normalized.starts_with("pt-br") {
        return "pt-BR";
    }
    if normalized.starts_with("ru") {
        return "ru";
    }
    if normalized.starts_with("en") {
        return "en";
    }
    "en"
}

fn sanitize_settings(mut settings: AppSettings) -> AppSettings {
    if settings.language.trim().is_empty() {
        settings.language = default_language();
    }
    if settings.shell.trim().is_empty() {
        settings.shell = default_shell();
    }
    settings.ai = sanitize_ai_settings(settings.ai);
    settings.pet.speech_mode = match settings.pet.speech_mode.trim() {
        "off" => "off".to_string(),
        "encourage" => "encourage".to_string(),
        "roast" => "roast".to_string(),
        "flirty" => "flirty".to_string(),
        "chuunibyou" => "chuunibyou".to_string(),
        _ => default_pet_speech_mode(),
    };
    settings.pet.speech_frequency = match settings.pet.speech_frequency.trim() {
        "quiet" => "quiet".to_string(),
        "lively" => "lively".to_string(),
        "chatterbox" => "chatterbox".to_string(),
        _ => default_pet_speech_frequency(),
    };
    settings.pet.desktop_scale = sanitize_pet_desktop_scale(&settings.pet.desktop_scale);
    if settings.sleep_mode.trim().is_empty() {
        settings.sleep_mode = default_sleep_mode();
    }
    if settings.git_refresh.trim().is_empty() {
        settings.git_refresh = default_git_refresh();
    }
    if settings.ai_refresh.trim().is_empty() {
        settings.ai_refresh = default_ai_refresh();
    }
    if settings.ai_background_refresh.trim().is_empty() {
        settings.ai_background_refresh = default_ai_background_refresh();
    }
    if settings.statistics_mode.trim().is_empty() {
        settings.statistics_mode = default_statistics_mode();
    }
    if settings.theme.trim().is_empty() {
        settings.theme = default_theme();
    }
    if settings.background.trim().is_empty() {
        settings.background = default_background();
    }
    if settings.terminal_font_size.trim().is_empty() {
        settings.terminal_font_size = default_terminal_font_size();
    }
    if settings.icon_style.trim().is_empty() {
        settings.icon_style = default_icon_style();
    }
    if settings.developer_refresh.trim().is_empty() {
        settings.developer_refresh = default_developer_refresh();
    }
    settings
        .notification_channels
        .retain(|id, _| !id.trim().is_empty());
    settings.shortcuts.retain(|id, value| {
        let id = id.trim();
        let value = value.trim();
        !id.is_empty() && !value.is_empty()
    });
    settings.update.channel = match settings.update.channel.trim() {
        "beta" => "beta".to_string(),
        "nightly" => "nightly".to_string(),
        _ => "stable".to_string(),
    };
    settings.update.endpoint = settings.update.endpoint.trim().to_string();
    if settings.update.enabled
        && (settings.update.endpoint.is_empty()
            || is_legacy_update_endpoint(&settings.update.endpoint))
    {
        settings.update.endpoint = default_update_endpoint();
    }
    settings.remote.relay_url = settings.remote.relay_url.trim().to_string();
    settings
}

fn sanitize_ai_settings(mut ai: AISettings) -> AISettings {
    ai.global_prompt = ai.global_prompt.trim().chars().take(20_000).collect();
    ai.memory.max_injected_user_working_memories =
        ai.memory.max_injected_user_working_memories.clamp(0, 24);
    ai.memory.max_injected_project_working_memories =
        ai.memory.max_injected_project_working_memories.clamp(0, 32);
    ai.memory.max_active_working_entries = ai.memory.max_active_working_entries.clamp(5, 200);
    ai.memory.max_summary_versions = ai.memory.max_summary_versions.clamp(1, 50);
    ai.memory.summary_target_token_budget = ai.memory.summary_target_token_budget.clamp(400, 3000);
    ai.memory.max_injected_summary_tokens = ai.memory.max_injected_summary_tokens.clamp(200, 2000);
    ai.memory.extraction_idle_delay_seconds = ai.memory.extraction_idle_delay_seconds.clamp(0, 900);
    ai.memory.session_extraction_cooldown_seconds =
        ai.memory.session_extraction_cooldown_seconds.clamp(0, 7200);
    ai.memory.max_extraction_transcript_lines =
        ai.memory.max_extraction_transcript_lines.clamp(20, 200);
    ai.memory.max_extraction_transcript_tokens = ai
        .memory
        .max_extraction_transcript_tokens
        .clamp(2000, 20000);
    ai.memory.default_extractor_provider_id = sanitize_provider_selector(
        &ai.memory.default_extractor_provider_id,
        &default_ai_memory_provider_id(),
    );
    ai.pet.speech_mode =
        sanitize_pet_speech_mode(&ai.pet.speech_mode, &default_ai_pet_speech_mode());
    ai.pet.speech_frequency = sanitize_pet_speech_frequency(&ai.pet.speech_frequency);
    ai.pet.speech_provider_id =
        sanitize_provider_selector(&ai.pet.speech_provider_id, &default_ai_pet_provider_id());
    ai.pet.speech_quiet_hours_start = normalize_hour(ai.pet.speech_quiet_hours_start);
    ai.pet.speech_quiet_hours_end = normalize_hour(ai.pet.speech_quiet_hours_end);
    ai.pet.speech_temporary_mute_until = ai
        .pet
        .speech_temporary_mute_until
        .filter(|timestamp| *timestamp > 0);

    ai.providers = ai
        .providers
        .into_iter()
        .filter_map(sanitize_ai_provider)
        .collect();
    ai.providers.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    if ai.memory.default_extractor_provider_id != default_ai_memory_provider_id()
        && !ai.providers.iter().any(|provider| {
            provider.id == ai.memory.default_extractor_provider_id
                && provider.is_enabled
                && provider.use_for_memory_extraction
                && provider_supports_completion(&provider.kind)
        })
    {
        ai.memory.default_extractor_provider_id = default_ai_memory_provider_id();
    }
    if ai.pet.speech_provider_id != default_ai_pet_provider_id()
        && !ai.providers.iter().any(|provider| {
            provider.id == ai.pet.speech_provider_id
                && provider.is_enabled
                && provider_supports_completion(&provider.kind)
        })
    {
        ai.pet.speech_provider_id = default_ai_pet_provider_id();
    }
    ai
}

fn sanitize_ai_provider(mut provider: AIProviderSettings) -> Option<AIProviderSettings> {
    provider.id = provider.id.trim().chars().take(120).collect();
    if provider.id.is_empty() {
        return None;
    }
    provider.kind = match provider.kind.trim() {
        "anthropic" => "anthropic".to_string(),
        "localLlama" => "localLlama".to_string(),
        _ => "openAICompatible".to_string(),
    };
    provider.display_name = provider
        .display_name
        .trim()
        .chars()
        .take(80)
        .collect::<String>();
    if provider.display_name.is_empty() {
        provider.display_name = match provider.kind.as_str() {
            "anthropic" => "Claude API".to_string(),
            "localLlama" => "Llama Model".to_string(),
            _ => "OpenAI API".to_string(),
        };
    }
    provider.model = provider.model.trim().chars().take(160).collect();
    if provider.model.is_empty() {
        provider.model = match provider.kind.as_str() {
            "anthropic" => "claude-3-5-haiku-latest".to_string(),
            "localLlama" => "qwen2.5-coder-1.5b-instruct-q4_k_m".to_string(),
            _ => "gpt-4.1-mini".to_string(),
        };
    }
    provider.base_url = provider.base_url.trim().chars().take(500).collect();
    provider.api_key = provider.api_key.trim().chars().take(2000).collect();
    provider.priority = provider.priority.clamp(-999, 999);
    Some(provider)
}

fn provider_supports_completion(kind: &str) -> bool {
    matches!(kind, "openAICompatible" | "anthropic")
}

fn sanitize_pet_speech_mode(value: &str, fallback: &str) -> String {
    match value.trim() {
        "off" => "off".to_string(),
        "encourage" => "encourage".to_string(),
        "roast" => "roast".to_string(),
        "flirty" => "flirty".to_string(),
        "chuunibyou" => "chuunibyou".to_string(),
        "mixed" => "mixed".to_string(),
        _ => fallback.to_string(),
    }
}

fn sanitize_pet_speech_frequency(value: &str) -> String {
    match value.trim() {
        "quiet" => "quiet".to_string(),
        "lively" => "lively".to_string(),
        "chatterbox" => "chatterbox".to_string(),
        _ => "normal".to_string(),
    }
}

fn sanitize_pet_desktop_scale(value: &str) -> String {
    let parsed = value.trim().parse::<f64>().unwrap_or(1.0);
    let stepped = (parsed / 0.1).round() * 0.1;
    let normalized = stepped.clamp(0.75, 1.5);
    if (normalized - 1.0).abs() < f64::EPSILON {
        "1".to_string()
    } else {
        format!("{normalized:.1}")
    }
}

fn normalize_hour(hour: Option<i32>) -> Option<i32> {
    hour.map(|value| value.clamp(0, 23))
}

fn sanitize_provider_selector(value: &str, automatic: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        automatic.to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}

fn load_settings(path: &Path) -> Option<AppSettings> {
    let data = fs::read(path).ok()?;
    if data.is_empty() {
        return None;
    }
    serde_json::from_slice(&data).ok()
}

fn settings_file_path() -> PathBuf {
    app_support_dir().join("settings.json")
}

fn default_language() -> String {
    "system".to_string()
}

fn default_shell() -> String {
    "system".to_string()
}

fn default_true() -> bool {
    true
}

fn default_pet_speech_mode() -> String {
    "mixed".to_string()
}

fn default_pet_speech_frequency() -> String {
    "normal".to_string()
}

fn default_pet_desktop_scale() -> String {
    "1".to_string()
}

fn default_ai_pet_speech_mode() -> String {
    "off".to_string()
}

fn default_ai_pet_speech_frequency() -> String {
    "normal".to_string()
}

fn default_ai_memory_provider_id() -> String {
    "automatic".to_string()
}

fn default_ai_pet_provider_id() -> String {
    "automatic".to_string()
}

fn default_memory_user_recall() -> i32 {
    4
}

fn default_memory_project_recall() -> i32 {
    6
}

fn default_memory_max_active_working_entries() -> i32 {
    50
}

fn default_memory_max_summary_versions() -> i32 {
    10
}

fn default_memory_summary_target_token_budget() -> i32 {
    900
}

fn default_memory_max_injected_summary_tokens() -> i32 {
    900
}

fn default_memory_extraction_idle_delay_seconds() -> i32 {
    120
}

fn default_memory_session_extraction_cooldown_seconds() -> i32 {
    900
}

fn default_memory_max_extraction_transcript_lines() -> i32 {
    80
}

fn default_memory_max_extraction_transcript_tokens() -> i32 {
    8000
}

fn default_sleep_mode() -> String {
    "off".to_string()
}

fn default_git_refresh() -> String {
    "60".to_string()
}

fn default_ai_refresh() -> String {
    "180".to_string()
}

fn default_ai_background_refresh() -> String {
    "600".to_string()
}

fn default_statistics_mode() -> String {
    "normalized".to_string()
}

fn default_theme() -> String {
    "Auto".to_string()
}

fn default_background() -> String {
    "Auto".to_string()
}

fn default_terminal_font_size() -> String {
    "14".to_string()
}

fn default_icon_style() -> String {
    "default".to_string()
}

fn default_developer_refresh() -> String {
    "3".to_string()
}

fn default_update_endpoint() -> String {
    "https://github.com/duxweb/codux/releases/download/tauri-stable/latest.json".to_string()
}

fn is_legacy_update_endpoint(endpoint: &str) -> bool {
    matches!(
        endpoint,
        "https://github.com/duxweb/codux/releases/latest/download/codux-tauri-latest.json"
            | "https://github.com/duxweb/codux/releases/latest/download/latest.json"
    )
}

#[cfg(target_os = "macos")]
fn macos_sync_process_locale_preference(language: &str) {
    use core_foundation_sys::array::{kCFTypeArrayCallBacks, CFArrayCreate};
    use core_foundation_sys::base::{kCFAllocatorDefault, CFRelease};
    use core_foundation_sys::preferences::{
        kCFPreferencesCurrentApplication, CFPreferencesAppSynchronize, CFPreferencesSetAppValue,
    };
    use core_foundation_sys::propertylist::CFPropertyListRef;
    use core_foundation_sys::string::{kCFStringEncodingUTF8, CFStringCreateWithCString};
    use std::ffi::CString;
    use std::os::raw::c_void;
    use std::ptr;

    let key = CString::new("AppleLanguages").expect("static string contains no nul");
    let key_ref = unsafe {
        CFStringCreateWithCString(kCFAllocatorDefault, key.as_ptr(), kCFStringEncodingUTF8)
    };
    if key_ref.is_null() {
        return;
    }

    unsafe {
        if language == "system" {
            CFPreferencesSetAppValue(
                key_ref,
                ptr::null::<c_void>() as CFPropertyListRef,
                kCFPreferencesCurrentApplication,
            );
            let _ = CFPreferencesAppSynchronize(kCFPreferencesCurrentApplication);
            CFRelease(key_ref.cast());
            return;
        }
    }

    let locale = locale_from_language_setting(language);
    let locale = CString::new(locale).unwrap_or_else(|_| CString::new("en").unwrap());
    let locale_ref = unsafe {
        CFStringCreateWithCString(kCFAllocatorDefault, locale.as_ptr(), kCFStringEncodingUTF8)
    };
    if locale_ref.is_null() {
        unsafe {
            CFRelease(key_ref.cast());
        }
        return;
    }

    let values = [locale_ref.cast::<c_void>()];
    let languages_ref = unsafe {
        CFArrayCreate(
            kCFAllocatorDefault,
            values.as_ptr(),
            values.len() as isize,
            &kCFTypeArrayCallBacks,
        )
    };

    unsafe {
        if !languages_ref.is_null() {
            CFPreferencesSetAppValue(
                key_ref,
                languages_ref.cast(),
                kCFPreferencesCurrentApplication,
            );
            let _ = CFPreferencesAppSynchronize(kCFPreferencesCurrentApplication);
            CFRelease(languages_ref.cast());
        }
        CFRelease(locale_ref.cast());
        CFRelease(key_ref.cast());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_language_settings_map_to_supported_locales() {
        assert_eq!(locale_from_language_setting("simplifiedChinese"), "zh-Hans");
        assert_eq!(
            locale_from_language_setting("traditionalChinese"),
            "zh-Hant"
        );
        assert_eq!(locale_from_language_setting("portugueseBrazil"), "pt-BR");
    }

    #[test]
    fn system_locale_mapping_matches_frontend_locale_mapping() {
        assert_eq!(locale_from_system_locale("zh_CN"), "zh-Hans");
        assert_eq!(locale_from_system_locale("zh-Hans-CN"), "zh-Hans");
        assert_eq!(locale_from_system_locale("zh_TW"), "zh-Hant");
        assert_eq!(locale_from_system_locale("pt_BR"), "pt-BR");
        assert_eq!(locale_from_system_locale("en_US"), "en");
    }
}
