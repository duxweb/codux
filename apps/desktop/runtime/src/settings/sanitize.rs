fn sanitize_ai_settings(mut ai: AISettings) -> AISettings {
    ai.git_commit_message_provider_id =
        sanitize_provider_reference(&ai.git_commit_message_provider_id);
    ai.git_commit_message_tone = sanitize_git_commit_tone(&ai.git_commit_message_tone);
    ai.git_commit_message_language = sanitize_git_commit_language(&ai.git_commit_message_language);
    ai.global_prompt = ai.global_prompt.trim().chars().take(4000).collect();
    ai.git_commit_message_style_rules = ai
        .git_commit_message_style_rules
        .trim()
        .chars()
        .take(4000)
        .collect();
    ai.memory.default_extractor_provider_id =
        sanitize_provider_reference(&ai.memory.default_extractor_provider_id);
    ai.memory.max_injected_user_working_memories =
        ai.memory.max_injected_user_working_memories.clamp(0, 32);
    ai.memory.max_injected_project_working_memories =
        ai.memory.max_injected_project_working_memories.clamp(0, 32);
    ai.memory.max_active_working_entries = ai.memory.max_active_working_entries.clamp(5, 200);
    ai.memory.max_summary_versions = ai.memory.max_summary_versions.clamp(1, 50);
    ai.memory.summary_target_token_budget = ai.memory.summary_target_token_budget.clamp(400, 3000);
    ai.memory.max_injected_summary_tokens = ai.memory.max_injected_summary_tokens.clamp(200, 2000);
    ai.memory.extraction_idle_delay_seconds = ai.memory.extraction_idle_delay_seconds.clamp(0, 900);
    ai.memory.session_extraction_cooldown_seconds =
        ai.memory.session_extraction_cooldown_seconds.clamp(0, 7200);
    ai.memory.max_index_sessions = ai.memory.max_index_sessions.clamp(1, 1000);
    ai.memory.max_extraction_transcript_lines =
        ai.memory.max_extraction_transcript_lines.clamp(20, 200);
    ai.memory.max_extraction_transcript_tokens = ai
        .memory
        .max_extraction_transcript_tokens
        .clamp(2000, 20000);
    ai.pet.speech_mode = sanitize_pet_speech_mode(&ai.pet.speech_mode);
    ai.pet.speech_frequency = sanitize_pet_speech_frequency(&ai.pet.speech_frequency);
    ai.pet.speech_provider_id = sanitize_provider_reference(&ai.pet.speech_provider_id);
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
    if ai.memory.default_extractor_provider_id != "automatic"
        && !ai.providers.iter().any(|provider| {
            provider.id == ai.memory.default_extractor_provider_id
                && provider.is_enabled
                && provider.use_for_memory_extraction
                && provider_supports_completion(&provider.kind)
        })
    {
        ai.memory.default_extractor_provider_id = "automatic".to_string();
    }
    if ai.pet.speech_provider_id != "automatic"
        && !ai.providers.iter().any(|provider| {
            provider.id == ai.pet.speech_provider_id
                && provider.is_enabled
                && provider_supports_completion(&provider.kind)
        })
    {
        ai.pet.speech_provider_id = "automatic".to_string();
    }
    if ai.git_commit_message_provider_id != "automatic"
        && ai.git_commit_message_provider_id != "off"
        && !ai.providers.iter().any(|provider| {
            provider.id == ai.git_commit_message_provider_id
                && provider.is_enabled
                && provider_supports_completion(&provider.kind)
        })
    {
        ai.git_commit_message_provider_id = "automatic".to_string();
    }
    ai
}

fn sanitize_ai_provider(mut provider: AIProviderSettings) -> Option<AIProviderSettings> {
    provider.id = provider.id.trim().chars().take(120).collect();
    if provider.id.is_empty() {
        return None;
    }
    provider.kind = sanitize_provider_kind(&provider.kind);
    provider.display_name = provider.display_name.trim().chars().take(80).collect();
    if provider.display_name.is_empty() {
        provider.display_name = provider_defaults(&provider.kind).0.to_string();
    }
    provider.model = provider.model.trim().chars().take(160).collect();
    if provider.model.is_empty() {
        provider.model = provider_defaults(&provider.kind).1.to_string();
    }
    provider.base_url = provider.base_url.trim().chars().take(500).collect();
    if provider.base_url.is_empty() {
        provider.base_url = provider_defaults(&provider.kind).2.to_string();
    }
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

fn block_on_llm<T: Send>(
    future: impl std::future::Future<Output = Result<T, String>> + Send,
) -> Result<T, String> {
    Ok(crate::async_runtime::block_on(future)?)
}

fn ai_providers_mut(raw: &mut Map<String, Value>) -> Result<&mut Vec<Value>, String> {
    let ai = ai_mut(raw)?;
    ai.entry("providers".to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "AI providers are not configured.".to_string())
}

fn ai_provider_mut<'a>(
    raw: &'a mut Map<String, Value>,
    provider_id: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let provider_id = provider_id.trim();
    if provider_id.is_empty() {
        return Err("AI provider id is empty.".to_string());
    }
    let providers = ai_providers_mut(raw)?;
    let Some(provider) = providers.iter_mut().find(|provider| {
        provider
            .get("id")
            .and_then(Value::as_str)
            .map(|id| id == provider_id)
            .unwrap_or(false)
    }) else {
        return Err("AI provider not found.".to_string());
    };
    provider
        .as_object_mut()
        .ok_or_else(|| "AI provider record is invalid.".to_string())
}

fn ai_mut(raw: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    raw.entry("ai".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "AI settings are invalid.".to_string())
}

fn ai_memory_mut(raw: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    let ai = ai_mut(raw)?;
    ai.entry("memory".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "AI memory settings are invalid.".to_string())
}

fn ai_pet_mut(raw: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    let ai = ai_mut(raw)?;
    ai.entry("pet".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "AI pet settings are invalid.".to_string())
}

fn ai_runtime_tools_mut(raw: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    let ai = ai_mut(raw)?;
    ai.entry("runtimeTools".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "AI runtime tool settings are invalid.".to_string())
}

fn update_mut(raw: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    raw.entry("update".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "Update settings are invalid.".to_string())
}

fn shortcuts_mut(raw: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    raw.entry("shortcuts".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "Shortcut settings are invalid.".to_string())
}

fn pet_mut(raw: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    raw.entry("pet".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "Pet settings are invalid.".to_string())
}
