use crate::ai_runtime::{
    probe::codex::probe_codex_runtime,
    tool_driver::{
        AIRuntimeLifecycleHookFormat, AIRuntimeMemoryInjectionDriver, AIRuntimeToolDriver,
        AIRuntimeToolHookDriver, NO_SCREEN_PATTERNS,
    },
};

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "codex",
    aliases: &["codex"],
    process_names: &["codex"],
    wrapper_bins: &["codex"],
    initial_prompt_args: &[],
    liveness_from_process: false,
    screen_starts_idle: false,
    screen_patterns: NO_SCREEN_PATTERNS,
    hook: AIRuntimeToolHookDriver::None,
    probe: Some(probe_codex_runtime),
    resource_paths: Some(crate::ai_runtime::tool_driver::transcript_resource_paths),
    memory_injection: AIRuntimeMemoryInjectionDriver::CodexDeveloperInstructions,
    lifecycle_hook_format: AIRuntimeLifecycleHookFormat::None,
    lifecycle_hooks: &[],
    lifecycle_config: None,
};
