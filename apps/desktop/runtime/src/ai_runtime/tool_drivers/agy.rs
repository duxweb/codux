use super::GEMINI_HOOKS;
use crate::ai_runtime::{
    probe::gemini::probe_gemini_runtime,
    tool_driver::{
        AIRuntimeJsonHookDriver, AIRuntimeJsonHookFormat, AIRuntimeMemoryInjectionDriver,
        AIRuntimeToolDriver, AIRuntimeToolHookDriver,
    },
};

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "agy",
    aliases: &["agy"],
    wrapper_bins: &["agy"],
    hook: AIRuntimeToolHookDriver::Json(AIRuntimeJsonHookDriver {
        tool: "agy",
        path_segments: &[".gemini", "antigravity-cli", "settings.json"],
        format: AIRuntimeJsonHookFormat::Standard,
        definitions: GEMINI_HOOKS,
    }),
    probe: Some(probe_gemini_runtime),
    memory_injection: AIRuntimeMemoryInjectionDriver::None,
};
