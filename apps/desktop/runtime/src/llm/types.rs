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

#[derive(Debug, Clone, Copy)]
pub struct LLMProviderCompletionOptions {
    pub max_tokens: u32,
    pub temperature: f32,
    pub preserve_formatting: bool,
    pub json_response: bool,
    pub timeout_seconds: u64,
}

impl Default for LLMProviderCompletionOptions {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            temperature: 0.4,
            preserve_formatting: false,
            json_response: false,
            timeout_seconds: 15,
        }
    }
}
