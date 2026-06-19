use super::*;

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
fn pet_speech_decode_unwraps_json_text_in_any_shape() {
    // Bare JSON object (the screenshot case): must not leak the raw JSON.
    assert_eq!(
        decode_pet_speech_response(r#"{"text":"代码在睡觉，我在数星星"}"#),
        "代码在睡觉，我在数星星"
    );
    // Wrapped in a markdown code fence.
    assert_eq!(
        decode_pet_speech_response("```json\n{\"text\":\"代码在睡觉，我在数星星\"}\n```"),
        "代码在睡觉，我在数星星"
    );
    // Embedded in surrounding prose / reasoning.
    assert_eq!(
        decode_pet_speech_response("Sure:\n{\"text\":\"代码在睡觉，我在数星星\"}\nDone."),
        "代码在睡觉，我在数星星"
    );
    // Alternate text-bearing key.
    assert_eq!(decode_pet_speech_response(r#"{"line":"hi"}"#), "hi");
    // Plain text with no JSON wrapper passes through unchanged.
    assert_eq!(
        decode_pet_speech_response("just a plain line"),
        "just a plain line"
    );
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
