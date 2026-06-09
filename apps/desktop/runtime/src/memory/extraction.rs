mod helpers;
mod parser;
mod prompt;
mod provider;
mod types;

pub use helpers::{normalized_memory_module, parse_uuid_string, valid_summary_content};
pub use parser::{decode_extraction_response, should_stop_memory_queue_after_error};
pub use prompt::{
    extraction_system_prompt, make_extraction_prompt, memory_extraction_language_label,
    trim_memory_text,
};
pub use provider::{
    ensure_memory_provider_available, provider_summary, select_memory_provider, supports_completion,
};
pub use types::{
    MemoryExtractionItem, MemoryExtractionResponse, MemoryKind, MemoryScope, MemoryTier,
    PromptMemoryEntry, PromptMemorySummary,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{AIMemorySettings, AIProviderSettings, AISettings};

    #[test]
    fn decodes_memory_extraction_response_from_fenced_json() {
        let merge_id = "550e8400-e29b-41d4-a716-446655440000";
        let archive_id = "550e8400-e29b-41d4-a716-446655440001";
        let response = decode_extraction_response(&format!(
            "```json\n{{\"userSummary\":\"keep this\",\"workingAdd\":[{{\"content\":\"Use compact GPUI files.\",\"scope\":\"project\",\"moduleKey\":\"UI\",\"tier\":\"working\",\"kind\":\"convention\",\"mergeWith\":\"{merge_id}\",\"archive\":[\"{archive_id}\"]}}],\"projectProfileStale\":\"yes\"}}\n```"
        ))
        .unwrap();

        assert_eq!(response.user_summary.as_deref(), Some("keep this"));
        assert!(response.project_profile_refresh_recommended);
        assert_eq!(response.working_add.len(), 1);
        let item = &response.working_add[0];
        assert_eq!(item.scope, Some(MemoryScope::Project));
        assert_eq!(item.module_key.as_deref(), Some("frontend"));
        assert_eq!(item.tier, Some(MemoryTier::Working));
        assert_eq!(item.kind, MemoryKind::Convention);
        assert_eq!(item.merge_with, vec![merge_id.to_string()]);
        assert_eq!(item.archive, vec![archive_id.to_string()]);
    }

    #[test]
    fn selects_memory_provider_with_tool_bonus() {
        let settings = AISettings {
            providers: vec![
                provider("generic", "Generic", 0, true),
                provider("codex", "Codex Runtime", 1, true),
                provider("disabled", "Disabled", -10, false),
            ],
            ..Default::default()
        };
        let selected = select_memory_provider(&settings, Some("codex")).unwrap();
        assert_eq!(selected.id, "codex");
    }

    #[test]
    fn builds_extraction_prompt_with_existing_context() {
        let prompt = make_extraction_prompt(
            "User asked to keep runtime code split.",
            Some(&PromptMemorySummary {
                version: 2,
                content: "Existing user preferences.".to_string(),
            }),
            &[PromptMemoryEntry {
                id: "user-1".to_string(),
                module_key: Some("user".to_string()),
                kind: "preference".to_string(),
                content: "Prefers maintainable small files.".to_string(),
                rationale: None,
            }],
            &[],
            "codux-gpui",
            "zh-Hans",
            &AIMemorySettings {
                summary_target_token_budget: 256,
                max_extraction_transcript_tokens: 512,
                ..Default::default()
            },
        );

        assert!(prompt.contains("Memory extraction schema: codux-memory-v4"));
        assert!(prompt.contains("version=2"));
        assert!(prompt.contains("Prefers maintainable small files."));
        assert!(prompt.contains("<transcript>"));
        assert!(prompt.contains("in Simplified Chinese"));
        assert!(prompt.contains("JSON keys and enum values must remain in English"));
    }

    #[test]
    fn memory_extraction_language_label_maps_supported_locales() {
        assert_eq!(
            memory_extraction_language_label("zh-Hant"),
            "Traditional Chinese"
        );
        assert_eq!(
            memory_extraction_language_label("zh-CN"),
            "Simplified Chinese"
        );
        assert_eq!(memory_extraction_language_label("ja"), "Japanese");
        assert_eq!(memory_extraction_language_label("pt-BR"), "Portuguese");
        assert_eq!(memory_extraction_language_label("en"), "English");
    }

    fn provider(
        id: &str,
        display_name: &str,
        priority: i32,
        use_for_memory_extraction: bool,
    ) -> AIProviderSettings {
        AIProviderSettings {
            id: id.to_string(),
            kind: "openAICompatible".to_string(),
            display_name: display_name.to_string(),
            is_enabled: true,
            model: "model".to_string(),
            base_url: "https://example.test".to_string(),
            api_key: "key".to_string(),
            use_for_memory_extraction,
            priority,
        }
    }
}
