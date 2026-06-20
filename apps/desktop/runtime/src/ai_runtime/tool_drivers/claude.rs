use crate::ai_runtime::{
    probe::claude::probe_claude_runtime,
    tool_driver::{
        AIRuntimeJsonHookDriver, AIRuntimeJsonHookFormat, AIRuntimeMemoryInjectionDriver,
        AIRuntimeToolDriver, AIRuntimeToolHookDriver, hook,
    },
};

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "claude",
    aliases: &["claude", "claude-code"],
    wrapper_bins: &["claude", "claude-code"],
    hook: AIRuntimeToolHookDriver::Json(AIRuntimeJsonHookDriver {
        tool: "claude",
        path_segments: &[".claude", "settings.json"],
        format: AIRuntimeJsonHookFormat::Standard,
        definitions: &[
            hook("SessionStart", "session-start", 10, false),
            hook("UserPromptSubmit", "prompt-submit", 10, false),
            hook("PreCompact", "pre-compact", 10, false),
            hook("PostCompact", "post-compact", 10, false),
            hook("Stop", "stop", 10, false),
            hook("StopFailure", "stop-failure", 10, false),
            hook("SessionEnd", "session-end", 1, false),
            hook("PermissionRequest", "permission-request", 5, true),
            hook("PermissionDenied", "permission-denied", 5, true),
            hook("Elicitation", "elicitation", 10, true),
            hook("ElicitationResult", "elicitation-result", 10, true),
        ],
    }),
    probe: Some(probe_claude_runtime),
    memory_injection: AIRuntimeMemoryInjectionDriver::ClaudeAppendSystemPrompt,
};
