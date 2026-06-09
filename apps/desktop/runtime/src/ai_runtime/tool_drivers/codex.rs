use crate::ai_runtime::{
    probe::codex::probe_codex_runtime,
    tool_driver::{
        AIRuntimeJsonHookDriver, AIRuntimeJsonHookFormat, AIRuntimeMemoryInjectionDriver,
        AIRuntimeToolDriver, AIRuntimeToolHookDriver, hook,
    },
};

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "codex",
    aliases: &["codex"],
    wrapper_bins: &["codex"],
    hook: AIRuntimeToolHookDriver::Json(AIRuntimeJsonHookDriver {
        tool: "codex",
        path_segments: &[".codex", "hooks.json"],
        format: AIRuntimeJsonHookFormat::Standard,
        definitions: &[
            hook("SessionStart", "codex-session-start", 1000, false),
            hook("UserPromptSubmit", "codex-prompt-submit", 1000, false),
            hook("PermissionRequest", "codex-permission-request", 1000, false),
            hook("Stop", "codex-stop", 1000, false),
        ],
    }),
    probe: Some(probe_codex_runtime),
    memory_injection: AIRuntimeMemoryInjectionDriver::CodexDeveloperInstructions,
};
