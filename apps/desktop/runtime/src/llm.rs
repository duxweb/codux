use crate::{
    runtime_trace::runtime_trace,
    settings::{AIProviderSettings, AISettings, locale_from_language_setting},
};
use chrono::{Local, Timelike};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
mod tests;

/// Route the shared `codux-llm` crate's provider-call tracing back through the
/// desktop's `runtime_trace`. Installed once; cheap to call repeatedly.
pub(crate) fn install_llm_trace_hook() {
    codux_llm::set_trace_hook(runtime_trace);
}

include!("llm/types.rs");
include!("llm/provider.rs");
include!("llm/completion.rs");
include!("llm/pet_speech.rs");
