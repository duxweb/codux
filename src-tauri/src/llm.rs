use crate::app_settings::{locale_from_language_setting, AIProviderSettings, AISettings};
use crate::runtime_trace::runtime_trace;
use chrono::{Local, Timelike};
use genai::adapter::AdapterKind;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatResponseFormat};
use genai::resolver::{AuthData, Endpoint};
use genai::{Client, ModelIden, ModelSpec, ServiceTarget, WebConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMCompletionRequest {
    pub provider_id: Option<String>,
    pub prompt: String,
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMCompletionResponse {
    pub provider_id: String,
    pub provider_name: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMProviderTestResult {
    pub provider_id: String,
    pub provider_name: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetIdleSpeechRequest {
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub fallback_text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetIdleSpeechResponse {
    pub text: String,
}

#[derive(Debug, Clone, Copy)]
pub struct LLMProviderCompletionOptions {
    pub max_tokens: u32,
    pub temperature: f32,
    pub preserve_formatting: bool,
    pub json_response: bool,
    pub timeout_seconds: u64,
}

impl Default for LLMProviderCompletionOptions {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            temperature: 0.4,
            preserve_formatting: false,
            json_response: false,
            timeout_seconds: 15,
        }
    }
}

pub async fn complete_with_settings(
    settings: &AISettings,
    request: LLMCompletionRequest,
) -> Result<LLMCompletionResponse, String> {
    let provider = select_provider(settings, request.provider_id.as_deref(), &request.purpose)
        .ok_or_else(|| "No available AI provider is configured.".to_string())?;
    let text =
        complete_with_provider(provider, &request.prompt, request.system_prompt.as_deref()).await?;
    Ok(LLMCompletionResponse {
        provider_id: provider.id.clone(),
        provider_name: provider.display_name.clone(),
        text,
    })
}

pub async fn test_provider(provider: AIProviderSettings) -> Result<LLMProviderTestResult, String> {
    let provider = sanitize_test_provider(provider)?;
    let text = complete_with_provider(
        &provider,
        "Reply with exactly: Codux provider test ok",
        Some("You are a connectivity test. Output only the requested text."),
    )
    .await?;
    Ok(LLMProviderTestResult {
        provider_id: provider.id,
        provider_name: provider.display_name,
        text: sanitize_response_line(&text),
    })
}

pub async fn pet_idle_speech_with_settings(
    settings: &AISettings,
    language: &str,
    request: PetIdleSpeechRequest,
) -> Result<PetIdleSpeechResponse, String> {
    if !settings.pet.speech_llm_enabled || settings.pet.speech_mode == "off" {
        return Ok(PetIdleSpeechResponse {
            text: String::new(),
        });
    }
    let now = chrono::Utc::now().timestamp();
    if settings
        .pet
        .speech_temporary_mute_until
        .is_some_and(|until| until > now)
        || quiet_hours_active(settings)
    {
        return Ok(PetIdleSpeechResponse {
            text: String::new(),
        });
    }
    let locale = locale_from_language_setting(language);
    let fallback_text = normalized_non_empty(&request.fallback_text);
    let system_prompt = pet_speech_system_prompt(&locale);
    let prompt = if let Some(fallback_text) = fallback_text {
        pet_speech_event_prompt(&request, &fallback_text)
    } else {
        pet_speech_idle_prompt(&locale)
    };
    let provider = select_provider(
        settings,
        Some(settings.pet.speech_provider_id.as_str()),
        "petSpeech",
    )
    .ok_or_else(|| "No available AI provider is configured for pet speech.".to_string())?;
    runtime_trace(
        "ai-pet",
        &format!(
            "speech request provider_id={} kind={} model={} event={} prompt_chars={}",
            provider.id,
            provider.kind,
            fallback_model(provider, default_model_for_provider_kind(&provider.kind)),
            normalized_non_empty(&request.event).unwrap_or_else(|| "idle".to_string()),
            prompt.chars().count()
        ),
    );
    let response_text = complete_with_provider_options(
        provider,
        &prompt,
        Some(&system_prompt),
        LLMProviderCompletionOptions {
            max_tokens: 80,
            temperature: 0.2,
            preserve_formatting: true,
            json_response: true,
            ..LLMProviderCompletionOptions::default()
        },
    )
    .await?;
    let text = decode_pet_speech_response(&response_text);
    let text = sanitize_pet_speech_line(&text);
    runtime_trace(
        "ai-pet",
        &format!("speech response text_chars={}", text.chars().count()),
    );
    Ok(PetIdleSpeechResponse { text })
}

pub async fn complete_with_provider(
    provider: &AIProviderSettings,
    prompt: &str,
    system_prompt: Option<&str>,
) -> Result<String, String> {
    complete_with_provider_options(
        provider,
        prompt,
        system_prompt,
        LLMProviderCompletionOptions::default(),
    )
    .await
}

pub async fn complete_with_provider_options(
    provider: &AIProviderSettings,
    prompt: &str,
    system_prompt: Option<&str>,
    options: LLMProviderCompletionOptions,
) -> Result<String, String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Prompt cannot be empty.".to_string());
    }
    complete_genai(provider, prompt, system_prompt, options).await
}

fn select_provider<'a>(
    settings: &'a AISettings,
    provider_id: Option<&str>,
    purpose: &str,
) -> Option<&'a AIProviderSettings> {
    let requested = provider_id
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "automatic");
    if let Some(id) = requested {
        if let Some(provider) = settings.providers.iter().find(|provider| {
            provider.id == id && provider.is_enabled && supports_completion(&provider.kind)
        }) {
            return Some(provider);
        }
    }
    let selector = match purpose {
        "petSpeech" => settings.pet.speech_provider_id.as_str(),
        "memory" => settings.memory.default_extractor_provider_id.as_str(),
        "gitCommitMessage" => "automatic",
        _ => "automatic",
    };
    if selector != "automatic" {
        if let Some(provider) = settings.providers.iter().find(|provider| {
            provider.id == selector && provider.is_enabled && supports_completion(&provider.kind)
        }) {
            return Some(provider);
        }
    }
    settings
        .providers
        .iter()
        .filter(|provider| {
            provider.is_enabled
                && supports_completion(&provider.kind)
                && (purpose != "memory" || provider.use_for_memory_extraction)
        })
        .min_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| left.display_name.cmp(&right.display_name))
        })
}

fn supports_completion(kind: &str) -> bool {
    provider_adapter_kind(kind).is_some()
}

fn quiet_hours_active(settings: &AISettings) -> bool {
    let Some(start) = settings.pet.speech_quiet_hours_start else {
        return false;
    };
    let Some(end) = settings.pet.speech_quiet_hours_end else {
        return false;
    };
    if start == end {
        return false;
    }
    let hour = Local::now().hour() as i32;
    if start < end {
        hour >= start && hour < end
    } else {
        hour >= start || hour < end
    }
}

fn pet_speech_system_prompt(locale: &str) -> String {
    let language = pet_speech_language_label(locale);
    format!("Return minified JSON only: {{\"text\":\"...\"}}. One short safe {language} pet line.")
}

fn pet_speech_language_label(locale: &str) -> &'static str {
    let normalized = locale.replace('_', "-").to_lowercase();
    if normalized.starts_with("zh-hant") {
        "Traditional Chinese"
    } else if normalized.starts_with("zh") {
        "Simplified Chinese"
    } else if normalized.starts_with("ja") {
        "Japanese"
    } else if normalized.starts_with("ko") {
        "Korean"
    } else if normalized.starts_with("fr") {
        "French"
    } else if normalized.starts_with("de") {
        "German"
    } else if normalized.starts_with("es") {
        "Spanish"
    } else if normalized.starts_with("pt") {
        "Portuguese"
    } else if normalized.starts_with("ru") {
        "Russian"
    } else {
        "English"
    }
}

fn pet_speech_event_prompt(request: &PetIdleSpeechRequest, fallback_text: &str) -> String {
    format!(
        "Event: {}\nFallback line: {}\nReturn {{\"text\":\"...\"}}.",
        normalized_non_empty(&request.event).unwrap_or_else(|| "activity".to_string()),
        fallback_text
    )
}

fn pet_speech_idle_prompt(locale: &str) -> String {
    let _ = locale;
    "Event: idle\nReturn {\"text\":\"...\"}.".to_string()
}

fn decode_pet_speech_response(raw: &str) -> String {
    let value = serde_json::from_str::<Value>(raw)
        .ok()
        .or_else(|| llm_json_repair::parse::<Value>(raw).ok());
    if let Some(text) = value
        .as_ref()
        .and_then(|value| value.as_object())
        .and_then(|object| {
            ["text", "line", "message", "content", "response"]
                .iter()
                .find_map(|key| object.get(*key)?.as_str())
        })
    {
        return text.to_string();
    }
    raw.to_string()
}

fn sanitize_pet_speech_line(text: &str) -> String {
    sanitize_response_line(text).chars().take(80).collect()
}

async fn complete_genai(
    provider: &AIProviderSettings,
    prompt: &str,
    system_prompt: Option<&str>,
    options: LLMProviderCompletionOptions,
) -> Result<String, String> {
    let adapter_kind = provider_adapter_kind(&provider.kind)
        .ok_or_else(|| "Unsupported AI provider kind.".to_string())?;
    let model = fallback_model(provider, default_model_for_provider_kind(&provider.kind));
    let service_target = genai_service_target(provider, adapter_kind, &model)?;
    runtime_trace(
        "ai-llm",
        &format!(
            "request start kind={} provider_id={} model={} base_url={} prompt_chars={} system_chars={} max_tokens={} temperature={:.2} json_response={}",
            provider.kind,
            provider.id,
            model,
            provider.base_url,
            prompt.chars().count(),
            system_prompt
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.chars().count())
                .unwrap_or(0),
            options.max_tokens,
            options.temperature,
            options.json_response
        ),
    );
    let mut request = ChatRequest::default();
    if let Some(system) = system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.with_system(system);
    }
    request = request.append_message(ChatMessage::user(prompt));
    let client = Client::builder()
        .with_web_config(
            WebConfig::default().with_timeout(Duration::from_secs(options.timeout_seconds.max(1))),
        )
        .build();
    let chat_options = ChatOptions::default()
        .with_max_tokens(options.max_tokens)
        .with_temperature(f64::from(options.temperature))
        .with_capture_raw_body(true);
    let chat_options = if options.json_response {
        chat_options.with_response_format(ChatResponseFormat::JsonMode)
    } else {
        chat_options
    };
    let response = client
        .exec_chat(
            ModelSpec::from_target(service_target),
            request,
            Some(&chat_options),
        )
        .await
        .map_err(|error| provider_call_error(provider, &model, error))?;
    let text = response.content.into_joined_texts().unwrap_or_default();
    let text = sanitize_provider_response(&text, options.preserve_formatting);
    if text.is_empty() {
        runtime_trace(
            "ai-llm",
            &format!("request empty provider_id={} model={}", provider.id, model),
        );
        Err("The AI provider returned an empty response.".to_string())
    } else {
        runtime_trace(
            "ai-llm",
            &format!(
                "request ok kind={} provider_id={} model={} text_chars={}",
                provider.kind,
                provider.id,
                model,
                text.chars().count()
            ),
        );
        Ok(text)
    }
}

fn sanitize_provider_response(text: &str, preserve_formatting: bool) -> String {
    if preserve_formatting {
        text.trim()
            .trim_matches(|ch| matches!(ch, '"' | '\'' | '“' | '”' | '‘' | '’'))
            .to_string()
    } else {
        sanitize_response_line(text)
    }
}

fn sanitize_test_provider(mut provider: AIProviderSettings) -> Result<AIProviderSettings, String> {
    provider.id = provider.id.trim().to_string();
    if provider.id.is_empty() {
        provider.id = "provider-test".to_string();
    }
    provider.display_name = provider.display_name.trim().to_string();
    if provider.display_name.is_empty() {
        provider.display_name = match provider.kind.as_str() {
            "anthropic" => "Claude API".to_string(),
            "deepseek" => "DeepSeek API".to_string(),
            "gemini" => "Gemini API".to_string(),
            "groq" => "Groq API".to_string(),
            "openrouter" => "OpenRouter API".to_string(),
            "ollama" | "localLlama" => "Ollama".to_string(),
            _ => "OpenAI API".to_string(),
        };
    }
    if !supports_completion(&provider.kind) {
        return Err("Only HTTP API providers can be tested in this build.".to_string());
    }
    Ok(provider)
}

fn required_api_key(provider: &AIProviderSettings) -> Result<&str, String> {
    let api_key = provider.api_key.trim();
    if api_key.is_empty() && !provider_kind_allows_empty_api_key(&provider.kind) {
        Err("The selected AI provider is missing an API key.".to_string())
    } else {
        Ok(api_key)
    }
}

fn fallback_model(provider: &AIProviderSettings, fallback: &str) -> String {
    let model = provider.model.trim();
    if model.is_empty() {
        fallback.to_string()
    } else {
        model.to_string()
    }
}

fn provider_adapter_kind(kind: &str) -> Option<AdapterKind> {
    match kind {
        "openai" | "openAICompatible" => Some(AdapterKind::OpenAI),
        "anthropic" => Some(AdapterKind::Anthropic),
        "deepseek" => Some(AdapterKind::DeepSeek),
        "gemini" => Some(AdapterKind::Gemini),
        "groq" => Some(AdapterKind::Groq),
        "openrouter" => Some(AdapterKind::OpenRouter),
        "ollama" | "localLlama" => Some(AdapterKind::Ollama),
        _ => None,
    }
}

fn provider_kind_allows_empty_api_key(kind: &str) -> bool {
    matches!(kind, "ollama" | "localLlama")
}

fn default_model_for_provider_kind(kind: &str) -> &'static str {
    match kind {
        "anthropic" => "claude-3-5-haiku-latest",
        "deepseek" => "deepseek-chat",
        "gemini" => "gemini-2.5-flash",
        "groq" => "llama-3.3-70b-versatile",
        "openrouter" => "openai/gpt-4.1-mini",
        "ollama" | "localLlama" => "llama3.2",
        _ => "gpt-4.1-mini",
    }
}

fn default_endpoint_for_provider_kind(kind: &str) -> &'static str {
    match kind {
        "anthropic" => "https://api.anthropic.com/v1/",
        "deepseek" => "https://api.deepseek.com/",
        "gemini" => "https://generativelanguage.googleapis.com/v1beta/",
        "groq" => "https://api.groq.com/openai/v1/",
        "openrouter" => "https://openrouter.ai/api/v1/",
        "ollama" | "localLlama" => "http://localhost:11434",
        _ => "https://api.openai.com/v1/",
    }
}

fn genai_service_target(
    provider: &AIProviderSettings,
    adapter_kind: AdapterKind,
    model: &str,
) -> Result<ServiceTarget, String> {
    let endpoint = provider_endpoint(provider)?;
    let auth = if provider_kind_allows_empty_api_key(&provider.kind) {
        AuthData::None
    } else {
        AuthData::from_single(required_api_key(provider)?.to_string())
    };
    Ok(ServiceTarget {
        endpoint: Endpoint::from_owned(endpoint),
        auth,
        model: ModelIden::new(adapter_kind, model),
    })
}

fn provider_endpoint(provider: &AIProviderSettings) -> Result<String, String> {
    let endpoint = normalized_provider_base_url(
        &provider.base_url,
        default_endpoint_for_provider_kind(&provider.kind),
    );
    if !endpoint.starts_with("https://") && !endpoint.starts_with("http://") {
        return Err("The selected AI provider has an invalid base URL.".to_string());
    }
    Ok(endpoint)
}

fn normalized_provider_base_url(base_url: &str, fallback: &str) -> String {
    let trimmed = base_url.trim();
    let value = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    if value.ends_with('/') {
        value.to_string()
    } else {
        format!("{value}/")
    }
}

fn provider_call_error(provider: &AIProviderSettings, model: &str, error: genai::Error) -> String {
    runtime_trace(
        "ai-llm",
        &format!(
            "request failed kind={} provider_id={} model={} error={}",
            provider.kind, provider.id, model, error
        ),
    );
    format!(
        "{} request failed for model {}: {}",
        provider.display_name,
        model,
        sanitize_response_line(&error.to_string())
    )
}

fn sanitize_response_line(text: &str) -> String {
    text.replace('\r', " ")
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '“' | '”' | '‘' | '’'))
        .chars()
        .take(500)
        .collect()
}

fn normalized_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_provider_base_urls_for_genai() {
        assert_eq!(
            normalized_provider_base_url("https://api.openai.com/v1", "https://fallback.test/v1/"),
            "https://api.openai.com/v1/"
        );
        assert_eq!(
            normalized_provider_base_url("", "https://fallback.test/v1/"),
            "https://fallback.test/v1/"
        );
    }

    #[test]
    fn selects_memory_provider_by_priority() {
        let settings = AISettings {
            providers: vec![
                provider("a", 10, true),
                provider("b", 1, true),
                provider("c", 0, false),
            ],
            ..AISettings::default()
        };

        let selected = select_provider(&settings, None, "memory").unwrap();

        assert_eq!(selected.id, "b");
    }

    #[test]
    fn pet_speech_language_follows_resolved_ui_locale() {
        assert_eq!(pet_speech_language_label("zh-Hans"), "Simplified Chinese");
        assert_eq!(pet_speech_language_label("zh-Hant"), "Traditional Chinese");
        assert_eq!(pet_speech_language_label("ja"), "Japanese");
        assert_eq!(pet_speech_language_label("pt-BR"), "Portuguese");
        assert_eq!(pet_speech_language_label("en"), "English");
    }

    fn provider(id: &str, priority: i32, use_for_memory_extraction: bool) -> AIProviderSettings {
        AIProviderSettings {
            id: id.to_string(),
            kind: "openAICompatible".to_string(),
            display_name: id.to_string(),
            is_enabled: true,
            model: "gpt-4.1-mini".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "key".to_string(),
            use_for_memory_extraction,
            priority,
        }
    }
}
