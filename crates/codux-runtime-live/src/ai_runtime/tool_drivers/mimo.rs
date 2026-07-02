use crate::ai_runtime::{
    probe::{opencode::probe_mimo_runtime, paths::mimo_runtime_resource_paths},
    snapshot::AISessionSnapshot,
    tool_driver::{
        AIRuntimeLifecycleHookFormat, AIRuntimeMemoryInjectionDriver, AIRuntimeToolDriver,
        AIRuntimeToolHookDriver, NO_SCREEN_PATTERNS,
    },
};
use std::path::PathBuf;

fn resource_paths(session: &AISessionSnapshot) -> Vec<PathBuf> {
    mimo_runtime_resource_paths(&session.terminal_id)
}

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "mimo",
    aliases: &["mimo"],
    process_names: &["mimo"],
    wrapper_bins: &["mimo"],
    liveness_from_process: false,
    screen_starts_idle: false,
    screen_patterns: NO_SCREEN_PATTERNS,
    hook: AIRuntimeToolHookDriver::OpenCodePlugin,
    probe: Some(probe_mimo_runtime),
    resource_paths: Some(resource_paths),
    memory_injection: AIRuntimeMemoryInjectionDriver::OpenCodeSystemTransform,
    lifecycle_hook_format: AIRuntimeLifecycleHookFormat::None,
    lifecycle_hooks: &[],
    lifecycle_config: None,
};
