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
    // The genai integration lives in the shared `codux-llm` crate. The desktop
    // keeps its richer `AISettings`-aware selection above and forwards the
    // chosen provider to the shared core.
    install_llm_trace_hook();
    codux_llm::complete(&llm_provider_from_settings(provider), prompt, system_prompt, options).await
}

fn llm_provider_from_settings(provider: &AIProviderSettings) -> codux_llm::LlmProvider {
    codux_llm::LlmProvider {
        id: provider.id.clone(),
        kind: provider.kind.clone(),
        display_name: provider.display_name.clone(),
        model: provider.model.clone(),
        base_url: provider.base_url.clone(),
        api_key: provider.api_key.clone(),
    }
}
