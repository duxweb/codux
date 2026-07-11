use super::{defaults::*, types::*};
use crate::settings::sanitize_terminal_font_family;

pub(super) fn sanitize_settings(mut settings: AppSettings) -> AppSettings {
    if settings.language.trim().is_empty() {
        settings.language = default_language();
    }
    settings.ai = sanitize_ai_settings(settings.ai);
    settings.pet.speech_mode =
        sanitize_pet_speech_mode(&settings.pet.speech_mode, &default_pet_speech_mode());
    settings.pet.speech_frequency = sanitize_pet_speech_frequency(&settings.pet.speech_frequency);
    settings.pet.hydration_reminder_minutes =
        sanitize_pet_reminder_minutes(&settings.pet.hydration_reminder_minutes, 60);
    settings.pet.sedentary_reminder_minutes =
        sanitize_pet_reminder_minutes(&settings.pet.sedentary_reminder_minutes, 60);
    settings.pet.late_night_reminder_minutes =
        sanitize_pet_reminder_minutes(&settings.pet.late_night_reminder_minutes, 60);
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
    settings.statistics_mode = sanitize_statistics_mode(&settings.statistics_mode);
    settings.file_open_default = sanitize_file_open_default(&settings.file_open_default);
    if settings.theme.trim().is_empty() {
        settings.theme = default_theme();
    }
    if settings.theme_color.trim().is_empty() {
        settings.theme_color = default_theme_color();
    }
    settings.terminal_font_family = sanitize_terminal_font_family(&settings.terminal_font_family);
    if settings.terminal_font_size.trim().is_empty() {
        settings.terminal_font_size = default_terminal_font_size();
    }
    settings.terminal_scrollback_lines =
        sanitize_terminal_scrollback_lines(&settings.terminal_scrollback_lines);
    if settings.icon_style.trim().is_empty() {
        settings.icon_style = default_icon_style();
    }
    if settings.window_style.trim().is_empty() {
        settings.window_style = default_window_style();
    }
    if settings.window_opacity.trim().is_empty() {
        settings.window_opacity = default_window_opacity();
    }
    let remote_relay_url = settings.remote.relay_url.trim().to_string();
    settings.remote.relay_preset =
        sanitize_remote_relay_preset(&settings.remote.relay_preset, &remote_relay_url);
    settings.remote.relay_url = crate::remote::remote_relay_url_for_preset(
        &settings.remote.relay_preset,
        &remote_relay_url,
    );
    settings.remote.relay_authentication = settings.remote.relay_authentication.trim().to_string();
    settings
        .remote
        .cached_devices
        .retain(|device| !device.id.trim().is_empty() && device.revoked_at.is_none());
    if !settings.remote.host_id.trim().is_empty() {
        let host_id = settings.remote.host_id.trim().to_string();
        settings
            .remote
            .cached_devices
            .retain(|device| device.host_id.trim().is_empty() || device.host_id == host_id);
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
            || is_managed_update_endpoint(&settings.update.endpoint))
    {
        settings.update.endpoint = update_endpoint_for_channel(&settings.update.channel);
    }
    settings
}

fn sanitize_remote_relay_preset(preset: &str, relay_url: &str) -> String {
    crate::remote::normalize_remote_relay_preset(preset, relay_url)
}

fn sanitize_ai_settings(mut ai: AISettings) -> AISettings {
    ai.global_prompt = ai.global_prompt.trim().chars().take(20_000).collect();
    ai.git_commit_message_provider_id = sanitize_provider_selector(
        &ai.git_commit_message_provider_id,
        &default_git_commit_message_provider_id(),
    );
    ai.git_commit_message_tone = sanitize_git_commit_message_style(&ai.git_commit_message_tone);
    ai.git_commit_message_language =
        sanitize_git_commit_message_language(&ai.git_commit_message_language);
    ai.git_commit_message_style_rules = ai
        .git_commit_message_style_rules
        .trim()
        .chars()
        .take(4_000)
        .collect();
    ai.runtime_tools = sanitize_runtime_tool_settings(ai.runtime_tools);
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
    ai.memory.extraction_growth_threshold_lines =
        ai.memory.extraction_growth_threshold_lines.clamp(0, 200);
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
    if ai.git_commit_message_provider_id != default_git_commit_message_provider_id()
        && ai.git_commit_message_provider_id != "off"
        && !ai.providers.iter().any(|provider| {
            provider.id == ai.git_commit_message_provider_id
                && provider.is_enabled
                && provider_supports_completion(&provider.kind)
        })
    {
        ai.git_commit_message_provider_id = default_git_commit_message_provider_id();
    }
    ai
}

fn sanitize_runtime_tool_settings(mut settings: AIRuntimeToolSettings) -> AIRuntimeToolSettings {
    settings.codex = sanitize_tool_permission_mode(&settings.codex);
    settings.claude_code = sanitize_tool_permission_mode(&settings.claude_code);
    settings.agy = sanitize_tool_permission_mode(&settings.agy);
    settings.opencode = sanitize_tool_permission_mode(&settings.opencode);
    settings.kiro = default_ai_tool_permission_mode();
    settings.codewhale = sanitize_tool_permission_mode(&settings.codewhale);
    settings.kimi = default_ai_tool_permission_mode();
    settings.mimo = sanitize_tool_permission_mode(&settings.mimo);
    settings.codex_model = settings.codex_model.trim().chars().take(160).collect();
    settings.claude_code_model = settings
        .claude_code_model
        .trim()
        .chars()
        .take(160)
        .collect();
    settings.agy_model = settings.agy_model.trim().chars().take(160).collect();
    settings.opencode_model = settings.opencode_model.trim().chars().take(160).collect();
    settings.kiro_model = settings.kiro_model.trim().chars().take(160).collect();
    settings.codewhale_model = settings.codewhale_model.trim().chars().take(160).collect();
    settings.kimi_model = settings.kimi_model.trim().chars().take(160).collect();
    settings.mimo_model = settings.mimo_model.trim().chars().take(160).collect();
    settings.codex_effort = match settings.codex_effort.trim() {
        "none" => "none".to_string(),
        "minimal" => "minimal".to_string(),
        "low" => "low".to_string(),
        "medium" => "medium".to_string(),
        "high" => "high".to_string(),
        "xhigh" => "xhigh".to_string(),
        _ => default_codex_effort(),
    };
    settings
}

fn sanitize_tool_permission_mode(value: &str) -> String {
    match value.trim() {
        "fullAccess" => "fullAccess".to_string(),
        _ => default_ai_tool_permission_mode(),
    }
}

fn sanitize_ai_provider(mut provider: AIProviderSettings) -> Option<AIProviderSettings> {
    provider.id = provider.id.trim().chars().take(120).collect();
    if provider.id.is_empty() {
        return None;
    }
    provider.kind = match provider.kind.trim() {
        "openai" => "openai".to_string(),
        "openAICompatible" => "openAICompatible".to_string(),
        "anthropic" => "anthropic".to_string(),
        "deepseek" => "deepseek".to_string(),
        "gemini" => "gemini".to_string(),
        "groq" => "groq".to_string(),
        "openrouter" => "openrouter".to_string(),
        "ollama" | "localLlama" => "ollama".to_string(),
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
            "openai" => "OpenAI API".to_string(),
            "anthropic" => "Claude API".to_string(),
            "deepseek" => "DeepSeek API".to_string(),
            "gemini" => "Gemini API".to_string(),
            "groq" => "Groq API".to_string(),
            "openrouter" => "OpenRouter API".to_string(),
            "ollama" => "Ollama".to_string(),
            _ => "OpenAI API".to_string(),
        };
    }
    provider.model = provider.model.trim().chars().take(160).collect();
    if provider.model.is_empty() {
        provider.model = match provider.kind.as_str() {
            "openai" => "gpt-4.1-mini".to_string(),
            "anthropic" => "claude-3-5-haiku-latest".to_string(),
            "deepseek" => "deepseek-chat".to_string(),
            "gemini" => "gemini-2.5-flash".to_string(),
            "groq" => "llama-3.3-70b-versatile".to_string(),
            "openrouter" => "openai/gpt-4.1-mini".to_string(),
            "ollama" => "llama3.2".to_string(),
            _ => "gpt-4.1-mini".to_string(),
        };
    }
    provider.base_url = provider.base_url.trim().chars().take(500).collect();
    provider.api_key = provider.api_key.trim().chars().take(2000).collect();
    provider.priority = provider.priority.clamp(-999, 999);
    Some(provider)
}

fn provider_supports_completion(kind: &str) -> bool {
    matches!(
        kind,
        "openai"
            | "openAICompatible"
            | "anthropic"
            | "deepseek"
            | "gemini"
            | "groq"
            | "openrouter"
            | "ollama"
            | "localLlama"
    )
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

fn sanitize_pet_reminder_minutes(value: &str, default: i64) -> String {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| value.round() as i64)
        .unwrap_or(default)
        .clamp(15, 240)
        .to_string()
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

fn sanitize_git_commit_message_style(value: &str) -> String {
    match value.trim() {
        "conventional" | "concise" | "sentence" | "changelog" => value.trim().to_string(),
        _ => default_git_commit_message_tone(),
    }
}

fn sanitize_git_commit_message_language(value: &str) -> String {
    match value.trim() {
        "application" | "english" | "simplifiedChinese" | "traditionalChinese" | "japanese"
        | "korean" | "french" | "german" | "spanish" | "portugueseBrazil" | "russian" => {
            value.trim().to_string()
        }
        _ => default_git_commit_message_language(),
    }
}

fn sanitize_statistics_mode(value: &str) -> String {
    match value.trim() {
        "includingCache" => "includingCache".to_string(),
        _ => default_statistics_mode(),
    }
}

fn sanitize_file_open_default(value: &str) -> String {
    match value.trim() {
        "preview" => "preview".to_string(),
        "split" => "split".to_string(),
        _ => default_file_open_default(),
    }
}

fn sanitize_terminal_scrollback_lines(value: &str) -> String {
    let parsed = value.trim().parse::<i32>().unwrap_or(2000);
    parsed.clamp(200, 10000).to_string()
}
