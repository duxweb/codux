use crate::ai_runtime::{
    probe::{opencode::probe_opencode_runtime, paths::opencode_runtime_resource_paths},
    snapshot::AISessionSnapshot,
    tool_driver::{
        AIRuntimeLifecycleHookFormat, AIRuntimeMemoryInjectionDriver, AIRuntimeToolDriver,
        AIRuntimeToolHookDriver, NO_SCREEN_PATTERNS,
    },
};
use std::path::PathBuf;

fn resource_paths(session: &AISessionSnapshot) -> Vec<PathBuf> {
    opencode_runtime_resource_paths(&session.terminal_id)
}

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "opencode",
    aliases: &["opencode"],
    process_names: &["opencode"],
    wrapper_bins: &["opencode"],
    liveness_from_process: false,
    screen_starts_idle: false,
    screen_patterns: NO_SCREEN_PATTERNS,
    hook: AIRuntimeToolHookDriver::OpenCodePlugin,
    probe: Some(probe_opencode_runtime),
    resource_paths: Some(resource_paths),
    memory_injection: AIRuntimeMemoryInjectionDriver::OpenCodeSystemTransform,
    lifecycle_hook_format: AIRuntimeLifecycleHookFormat::None,
    lifecycle_hooks: &[],
    lifecycle_config: None,
};
