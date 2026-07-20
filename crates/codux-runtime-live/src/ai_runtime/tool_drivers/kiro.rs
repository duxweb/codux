use crate::ai_runtime::{
    probe::{kiro::probe_kiro_runtime, paths::kiro_runtime_resource_paths},
    snapshot::AISessionSnapshot,
    tool_driver::{
        AIRuntimeLifecycleHookFormat, AIRuntimeMemoryInjectionDriver, AIRuntimeToolDriver,
        AIRuntimeToolHookDriver, KIRO_SCREEN_PATTERNS,
    },
};
use std::path::PathBuf;

fn resource_paths(session: &AISessionSnapshot) -> Vec<PathBuf> {
    kiro_runtime_resource_paths(
        session.project_path.as_deref(),
        session.ai_session_id.as_deref(),
        session.started_at,
    )
}

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "kiro",
    aliases: &["kiro-cli"],
    process_names: &["kiro-cli", "kiro-cli-chat"],
    wrapper_bins: &["kiro-cli"],
    initial_prompt_args: &[],
    liveness_from_process: true,
    screen_starts_idle: true,
    screen_patterns: KIRO_SCREEN_PATTERNS,
    hook: AIRuntimeToolHookDriver::None,
    probe: Some(probe_kiro_runtime),
    resource_paths: Some(resource_paths),
    memory_injection: AIRuntimeMemoryInjectionDriver::None,
    lifecycle_hook_format: AIRuntimeLifecycleHookFormat::None,
    lifecycle_hooks: &[],
    lifecycle_config: None,
};
