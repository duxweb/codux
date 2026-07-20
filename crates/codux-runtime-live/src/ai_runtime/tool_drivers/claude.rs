use crate::ai_runtime::{
    probe::claude::probe_claude_runtime,
    tool_driver::{
        AIRuntimeLifecycleHookFormat, AIRuntimeMemoryInjectionDriver, AIRuntimeToolDriver,
        AIRuntimeToolHookDriver, NO_SCREEN_PATTERNS,
    },
};

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "claude",
    aliases: &["claude", "claude-code", "reclaude"],
    process_names: &["claude", "claude-code", "reclaude"],
    wrapper_bins: &["claude", "claude-code", "reclaude"],
    initial_prompt_args: &[],
    liveness_from_process: false,
    screen_starts_idle: false,
    screen_patterns: NO_SCREEN_PATTERNS,
    hook: AIRuntimeToolHookDriver::None,
    probe: Some(probe_claude_runtime),
    resource_paths: Some(crate::ai_runtime::tool_driver::transcript_resource_paths),
    memory_injection: AIRuntimeMemoryInjectionDriver::AppendSystemPrompt,
    lifecycle_hook_format: AIRuntimeLifecycleHookFormat::None,
    lifecycle_hooks: &[],
    lifecycle_config: None,
};
