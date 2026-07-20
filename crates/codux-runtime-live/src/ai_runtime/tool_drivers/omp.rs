use crate::ai_runtime::{
    probe::{omp::probe_omp_runtime, paths::omp_runtime_resource_paths},
    snapshot::AISessionSnapshot,
    tool_driver::{
        AIRuntimeLifecycleHookFormat, AIRuntimeMemoryInjectionDriver, AIRuntimeToolDriver,
        AIRuntimeToolHookDriver, NO_SCREEN_PATTERNS,
    },
};
use std::path::PathBuf;

fn resource_paths(session: &AISessionSnapshot) -> Vec<PathBuf> {
    omp_runtime_resource_paths(
        session.project_path.as_deref(),
        session.ai_session_id.as_deref(),
        session.transcript_path.as_deref(),
        session.started_at,
    )
}

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "omp",
    aliases: &["omp"],
    process_names: &["omp"],
    wrapper_bins: &["omp"],
    initial_prompt_args: &[],
    liveness_from_process: false,
    screen_starts_idle: false,
    screen_patterns: NO_SCREEN_PATTERNS,
    hook: AIRuntimeToolHookDriver::None,
    probe: Some(probe_omp_runtime),
    resource_paths: Some(resource_paths),
    memory_injection: AIRuntimeMemoryInjectionDriver::AppendSystemPrompt,
    lifecycle_hook_format: AIRuntimeLifecycleHookFormat::None,
    lifecycle_hooks: &[],
    lifecycle_config: None,
};
