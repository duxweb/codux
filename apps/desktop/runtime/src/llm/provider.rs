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
    codux_llm::supports_completion(kind)
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

fn fallback_model(provider: &AIProviderSettings, fallback: &str) -> String {
    let model = provider.model.trim();
    if model.is_empty() {
        fallback.to_string()
    } else {
        model.to_string()
    }
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
