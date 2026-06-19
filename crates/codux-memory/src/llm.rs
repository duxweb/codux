//! Bridge from the memory engine's provider calls to the shared `codux-llm`
//! crate. The moved engine code calls `llm::complete_with_provider_options`
//! with a [`crate::MemoryProvider`]; this converts it to a
//! [`codux_llm::LlmProvider`] and runs the shared completion core.

use crate::MemoryProvider;

pub use codux_llm::{LlmCompletionOptions as LLMProviderCompletionOptions, LlmJsonSchema as LLMJsonSchema};

pub async fn complete_with_provider_options(
    provider: &MemoryProvider,
    prompt: &str,
    system_prompt: Option<&str>,
    options: LLMProviderCompletionOptions,
) -> Result<String, String> {
    codux_llm::complete(&provider.to_llm_provider(), prompt, system_prompt, options).await
}
