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

#[test]
fn pet_speech_prompt_requests_original_persona_line() {
    let system = pet_speech_system_prompt("zh-Hans", "roast");
    assert!(system.contains("desktop pet"));
    assert!(system.contains("playfully sarcastic"));
    assert!(system.contains("random, self-initiated, original"));
    assert!(system.contains("Do not polish, rewrite, or paraphrase"));

    let prompt = pet_speech_prompt(&PetIdleSpeechRequest {
        event: "reminder.hydration".to_string(),
        facts: "A hydration reminder is due after 60 minutes.".to_string(),
    });
    assert!(prompt.contains("Event: reminder.hydration"));
    assert!(prompt.contains("Facts: A hydration reminder"));
    assert!(prompt.contains("fresh random line"));
    assert!(!prompt.contains("fallback"));
}

#[test]
fn json_schema_response_format_is_used_only_for_known_supported_providers() {
    let mut openai = provider("openai", 0, true);
    openai.kind = "openai".to_string();
    assert!(provider_supports_json_schema_response_format(&openai));

    let mut compatible = provider("deepseek-compatible", 0, true);
    compatible.base_url = "https://api.deepseek.com/v1".to_string();
    compatible.model = "deepseek-v4-flash".to_string();
    assert!(!provider_supports_json_schema_response_format(&compatible));

    let mut deepseek = provider("deepseek", 0, true);
    deepseek.kind = "deepseek".to_string();
    assert!(!provider_supports_json_schema_response_format(&deepseek));
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
