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
