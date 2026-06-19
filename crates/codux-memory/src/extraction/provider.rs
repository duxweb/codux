use super::helpers::normalized_non_empty;
use crate::{MemoryConfig, MemoryProvider};

pub fn select_memory_provider<'a>(
    settings: &'a MemoryConfig,
    tool: Option<&str>,
) -> Option<&'a MemoryProvider> {
    let requested = settings.memory.default_extractor_provider_id.trim();
    if !requested.is_empty() && requested != "automatic" {
        if let Some(provider) = settings.providers.iter().find(|provider| {
            provider.id == requested
                && provider.is_enabled
                && provider.use_for_memory_extraction
                && supports_completion(&provider.kind)
        }) {
            return Some(provider);
        }
    }
    let normalized_tool = tool
        .and_then(normalized_non_empty)
        .map(|value| value.to_lowercase());
    settings
        .providers
        .iter()
        .filter(|provider| {
            provider.is_enabled
                && provider.use_for_memory_extraction
                && supports_completion(&provider.kind)
        })
        .min_by(|left, right| {
            let left_tool_bonus = i32::from(
                normalized_tool
                    .as_ref()
                    .is_some_and(|tool| left.display_name.to_lowercase().contains(tool)),
            );
            let right_tool_bonus = i32::from(
                normalized_tool
                    .as_ref()
                    .is_some_and(|tool| right.display_name.to_lowercase().contains(tool)),
            );
            (left.priority - left_tool_bonus)
                .cmp(&(right.priority - right_tool_bonus))
                .then_with(|| left.display_name.cmp(&right.display_name))
        })
}

pub fn ensure_memory_provider_available(settings: &MemoryConfig) -> Result<(), String> {
    if select_memory_provider(settings, None).is_some() {
        Ok(())
    } else {
        Err(
            "Memory needs an enabled AI provider with Use For Memory Extraction turned on."
                .to_string(),
        )
    }
}

pub fn provider_summary(provider: &MemoryProvider) -> String {
    format!(
        "provider={} id={} kind={} model={} base_url={}",
        provider.display_name, provider.id, provider.kind, provider.model, provider.base_url
    )
}

pub fn supports_completion(kind: &str) -> bool {
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
