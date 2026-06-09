use crate::{
    runtime_trace::runtime_trace,
    settings::{AIProviderSettings, AISettings, locale_from_language_setting},
};
use chrono::{Local, Timelike};
use genai::adapter::AdapterKind;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatResponseFormat};
use genai::resolver::{AuthData, Endpoint};
use genai::{Client, ModelIden, ModelSpec, ServiceTarget, WebConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[cfg(test)]
mod tests;

include!("llm/types.rs");
include!("llm/provider.rs");
include!("llm/completion.rs");
include!("llm/pet_speech.rs");
