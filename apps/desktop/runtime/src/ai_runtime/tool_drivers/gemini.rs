use super::GEMINI_HOOKS;
use crate::ai_runtime::{
    probe::gemini::probe_gemini_runtime,
    tool_driver::{
        AIRuntimeJsonHookDriver, AIRuntimeJsonHookFormat, AIRuntimeMemoryInjectionDriver,
        AIRuntimeToolDriver, AIRuntimeToolHookDriver,
    },
};

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "gemini",
    aliases: &["gemini"],
    wrapper_bins: &["gemini"],
    hook: AIRuntimeToolHookDriver::Json(AIRuntimeJsonHookDriver {
        tool: "gemini",
        path_segments: &[".gemini", "settings.json"],
        format: AIRuntimeJsonHookFormat::Standard,
        definitions: GEMINI_HOOKS,
    }),
    probe: Some(probe_gemini_runtime),
    memory_injection: AIRuntimeMemoryInjectionDriver::None,
};
