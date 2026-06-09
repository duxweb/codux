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
    let value = if trimmed.is_empty() { fallback } else { trimmed };
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
