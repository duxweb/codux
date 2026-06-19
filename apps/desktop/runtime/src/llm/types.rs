#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMCompletionRequest {
    pub provider_id: Option<String>,
    pub prompt: String,
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMCompletionResponse {
    pub provider_id: String,
    pub provider_name: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMProviderTestResult {
    pub provider_id: String,
    pub provider_name: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetIdleSpeechRequest {
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub facts: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetIdleSpeechResponse {
    pub text: String,
}

// The provider-completion option/schema types live in the shared `codux-llm`
// crate (the agent + memory crate reuse the same genai core). Alias them so the
// desktop's call sites keep their existing names.
pub use codux_llm::{LlmCompletionOptions as LLMProviderCompletionOptions, LlmJsonSchema as LLMJsonSchema};
