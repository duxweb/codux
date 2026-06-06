use crate::ai_runtime::{
    probe::kiro::probe_kiro_runtime,
    tool_driver::{
        AIRuntimeJsonHookDriver, AIRuntimeJsonHookFormat, AIRuntimeToolDriver,
        AIRuntimeToolHookDriver, hook,
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
            hook("agentSpawn", "session-start", 5000, true),
            hook("stop", "session-end", 5000, true),
        ],
    }),
    probe: Some(probe_kiro_runtime),
};
