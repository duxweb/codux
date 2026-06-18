use crate::ai_runtime::{
    probe::kiro::probe_kiro_runtime,
    tool_driver::{
        AIRuntimeJsonHookDriver, AIRuntimeJsonHookFormat, AIRuntimeMemoryInjectionDriver,
        AIRuntimeToolDriver, AIRuntimeToolHookDriver, hook,
    },
};

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "kiro",
    aliases: &["kiro", "kiro-cli"],
    wrapper_bins: &["kiro", "kiro-cli"],
    hook: AIRuntimeToolHookDriver::Json(AIRuntimeJsonHookDriver {
        tool: "kiro",
        path_segments: &[".kiro", "agents", "codux-managed.json"],
        format: AIRuntimeJsonHookFormat::Kiro,
        definitions: &[
            // Kiro fires `userPromptSubmit` when the user submits (responding)
            // and `stop` at the END OF EACH TURN -- not at session end -- when
            // the assistant finishes responding (completed). The previous wiring
            // had no responding hook and mis-mapped `stop` to session-end, so
            // Kiro showed completion but never a running/loading state.
            hook("agentSpawn", "session-start", 5000, false),
            hook("userPromptSubmit", "prompt-submit", 5000, false),
            hook("stop", "stop", 5000, false),
        ],
    }),
    probe: Some(probe_kiro_runtime),
    memory_injection: AIRuntimeMemoryInjectionDriver::None,
};
