use crate::app_settings::{locale_from_language_setting, AIProviderSettings, AISettings};
use crate::i18n;
use chrono::{Local, Timelike};
use serde::{Deserialize, Serialize};
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
    pub pet_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetIdleSpeechResponse {
    pub text: String,
}

#[derive(Debug, Serialize)]
struct OpenAIChatCompletionRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIChatCompletionResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoiceMessage {
    content: Option<String>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessagesRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessagesResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Clone, Copy)]
pub struct LLMProviderCompletionOptions {
    pub max_tokens: u32,
    pub temperature: f32,
    pub preserve_formatting: bool,
}

impl Default for LLMProviderCompletionOptions {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            temperature: 0.4,
            preserve_formatting: false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    text: Option<String>,
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
    let mode = resolved_pet_speech_mode(&settings.pet.speech_mode);
    let locale = locale_from_language_setting(language);
    let pet_name = normalized_non_empty(&request.pet_name)
        .unwrap_or_else(|| i18n::translate(&locale, "pet.speech.payload.pet_name", "Little One"));
    let personality = i18n::translate(
        &locale,
        &format!("pet.speech.llm.mode.{mode}"),
        "warm, specific, coach-like",
    );
    let system_prompt = fill_placeholders(
        &i18n::translate(
            &locale,
            "pet.speech.llm.idle_system_prompt_format",
            "You are a desktop pixel pet named %@. Personality: %@. Write a casual idle monologue in Simplified Chinese. Use at most 2 short lines and 36 characters total. Do not mention code, files, secrets, commands, or exact task results. Do not explain. Output only the line.",
        ),
        &[&pet_name, &personality],
    );
    let hour_label = Local::now().format("%H:%M").to_string();
    let tool = i18n::translate(&locale, "pet.speech.payload.tool", "you");
    let project = i18n::translate(&locale, "pet.speech.payload.project", "this task");
    let prompt = fill_placeholders(
        &i18n::translate(
            &locale,
            "pet.speech.llm.idle_user_prompt_format",
            "Idle event: %@\nCurrent hour: %@\nRecent tool: %@ / model: %@\nProject nickname: %@",
        ),
        &["idle.monologue", &hour_label, &tool, "AI", &project],
    );
    let completion = complete_with_settings(
        settings,
        LLMCompletionRequest {
            provider_id: Some(settings.pet.speech_provider_id.clone()),
            prompt,
            system_prompt: Some(system_prompt),
            purpose: "petSpeech".to_string(),
        },
    )
    .await?;
    Ok(PetIdleSpeechResponse {
        text: completion.text,
    })
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
    match provider.kind.as_str() {
        "anthropic" => complete_anthropic(provider, prompt, system_prompt, options).await,
        "openAICompatible" => {
            complete_openai_compatible(provider, prompt, system_prompt, options).await
        }
        "localLlama" => Err("Local llama is not available in this Tauri build yet.".to_string()),
        _ => Err("Unsupported AI provider kind.".to_string()),
    }
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
    matches!(kind, "openAICompatible" | "anthropic")
}

fn resolved_pet_speech_mode(mode: &str) -> &str {
    match mode {
        "roast" | "encourage" | "flirty" | "chuunibyou" => mode,
        "mixed" => match Local::now().hour() % 4 {
            0 => "roast",
            1 => "encourage",
            2 => "flirty",
            _ => "chuunibyou",
        },
        _ => "encourage",
    }
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

async fn complete_openai_compatible(
    provider: &AIProviderSettings,
    prompt: &str,
    system_prompt: Option<&str>,
    options: LLMProviderCompletionOptions,
) -> Result<String, String> {
    let api_key = required_api_key(provider)?;
    let url = openai_endpoint(&provider.base_url)?;
    let client = http_client()?;
    let mut messages = Vec::new();
    if let Some(system) = system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        messages.push(OpenAIMessage {
            role: "system".to_string(),
            content: system.to_string(),
        });
    }
    messages.push(OpenAIMessage {
        role: "user".to_string(),
        content: prompt.to_string(),
    });
    let response = client
        .post(url)
        .bearer_auth(api_key)
        .json(&OpenAIChatCompletionRequest {
            model: fallback_model(provider, "gpt-4.1-mini"),
            messages,
            max_tokens: options.max_tokens,
            temperature: options.temperature,
        })
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(provider_error(status.as_u16(), &body));
    }
    let decoded: OpenAIChatCompletionResponse =
        serde_json::from_str(&body).map_err(|error| error.to_string())?;
    decoded
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_deref())
        .map(|text| sanitize_provider_response(text, options.preserve_formatting))
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "The AI provider returned an empty response.".to_string())
}

async fn complete_anthropic(
    provider: &AIProviderSettings,
    prompt: &str,
    system_prompt: Option<&str>,
    options: LLMProviderCompletionOptions,
) -> Result<String, String> {
    let api_key = required_api_key(provider)?;
    let url = anthropic_endpoint(&provider.base_url)?;
    let client = http_client()?;
    let response = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&AnthropicMessagesRequest {
            model: fallback_model(provider, "claude-3-5-haiku-latest"),
            max_tokens: options.max_tokens,
            system: system_prompt
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        })
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(provider_error(status.as_u16(), &body));
    }
    let decoded: AnthropicMessagesResponse =
        serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let text = decoded
        .content
        .iter()
        .filter_map(|block| block.text.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let text = sanitize_provider_response(&text, options.preserve_formatting);
    if text.is_empty() {
        Err("The AI provider returned an empty response.".to_string())
    } else {
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
            "localLlama" => "Llama Model".to_string(),
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
    if api_key.is_empty() {
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

fn openai_endpoint(base_url: &str) -> Result<String, String> {
    normalized_endpoint(
        base_url,
        "https://api.openai.com/v1/chat/completions",
        "chat/completions",
    )
}

fn anthropic_endpoint(base_url: &str) -> Result<String, String> {
    normalized_endpoint(
        base_url,
        "https://api.anthropic.com/v1/messages",
        "messages",
    )
}

fn normalized_endpoint(base_url: &str, fallback: &str, suffix: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    let endpoint = if trimmed.is_empty() {
        fallback.to_string()
    } else if trimmed.ends_with(suffix) {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/{suffix}")
    } else {
        format!("{trimmed}/v1/{suffix}")
    };
    if !endpoint.starts_with("https://") && !endpoint.starts_with("http://") {
        return Err("The selected AI provider has an invalid base URL.".to_string());
    }
    Ok(endpoint)
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())
}

fn provider_error(status: u16, body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        format!("Provider returned HTTP {status}.")
    } else {
        body.chars().take(800).collect()
    }
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

fn fill_placeholders(template: &str, values: &[&str]) -> String {
    let mut output = template.to_string();
    for value in values {
        output = output.replacen("%@", value, 1);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_openai_base_urls() {
        assert_eq!(
            openai_endpoint("https://api.openai.com/v1").unwrap(),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            openai_endpoint("https://example.com/api").unwrap(),
            "https://example.com/api/v1/chat/completions"
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
