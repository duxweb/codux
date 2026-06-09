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
    let system_prompt = pet_speech_system_prompt(&locale, settings.pet.speech_mode.as_str());
    let prompt = pet_speech_prompt(&request);
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
            temperature: 0.8,
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

fn pet_speech_system_prompt(locale: &str, mode: &str) -> String {
    let language = pet_speech_language_label(locale);
    let persona = pet_speech_persona(mode);
    format!(
        "You are Codux's desktop pet, a small companion that watches AI coding work and helps the user keep momentum.\n\
Return minified JSON only: {{\"text\":\"...\"}}.\n\
Write exactly one random, self-initiated, original short pet line in {language}.\n\
Personality: {persona}\n\
Rules:\n\
- Do not polish, rewrite, or paraphrase any provided text.\n\
- Use facts only as guardrails for what is currently true.\n\
- You may choose your own wording, angle, and mood within the personality.\n\
- Do not invent task results, token counts, errors, or permissions that were not provided.\n\
- Be concise, natural, and suitable for a tiny speech bubble.\n\
- No markdown, no emoji, no explanations, no quotes around the sentence inside text beyond JSON syntax.\n\
- Keep text under 28 {language} characters when possible, and always under 80 Unicode characters."
    )
}

fn pet_speech_persona(mode: &str) -> &'static str {
    match mode.trim() {
        "encourage" => "warm, steady, supportive, quietly focused",
        "roast" => "playfully sarcastic but never cruel or insulting",
        "flirty" => "lightly charming and affectionate without being explicit",
        "chuunibyou" => "dramatic fantasy-style, compact, like a tiny ritual companion",
        "mixed" => "varied between supportive, playful, and slightly dramatic while staying helpful",
        _ => "warm, concise, and calm",
    }
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

fn pet_speech_prompt(request: &PetIdleSpeechRequest) -> String {
    let event = normalized_non_empty(&request.event).unwrap_or_else(|| "idle".to_string());
    let facts = normalized_non_empty(&request.facts)
        .unwrap_or_else(|| "No specific runtime facts are available.".to_string());
    format!(
        "Event: {event}\nFacts: {facts}\nSpeak as the pet with a fresh random line for this moment.\nReturn {{\"text\":\"...\"}}."
    )
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
