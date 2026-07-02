use crate::ai_runtime::{
    probe::codewhale::{codewhale_runtime_resource_paths, probe_codewhale_runtime},
    snapshot::AISessionSnapshot,
    tool_driver::{
        AIRuntimeLifecycleHookFormat, AIRuntimeMemoryInjectionDriver, AIRuntimeToolDriver,
        AIRuntimeToolHookDriver, NO_SCREEN_PATTERNS, lifecycle_config, lifecycle_hook,
    },
};
use std::path::PathBuf;

fn codewhale_resource_paths(session: &AISessionSnapshot) -> Vec<PathBuf> {
    codewhale_runtime_resource_paths(session.ai_session_id.as_deref())
}

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "codewhale",
    aliases: &["codewhale"],
    process_names: &["codewhale"],
    wrapper_bins: &["codewhale"],
    liveness_from_process: true,
    screen_starts_idle: false,
    screen_patterns: NO_SCREEN_PATTERNS,
    hook: AIRuntimeToolHookDriver::CodeWhaleToml,
    probe: Some(probe_codewhale_runtime),
    resource_paths: Some(codewhale_resource_paths),
    memory_injection: AIRuntimeMemoryInjectionDriver::None,
    lifecycle_hook_format: AIRuntimeLifecycleHookFormat::CodeWhaleToml,
    lifecycle_hooks: &[
        lifecycle_hook("message_submit", "codewhale-message-submit", 1, false),
        lifecycle_hook("turn_end", "codewhale-turn-end", 1, true),
    ],
    lifecycle_config: Some(lifecycle_config(
        "DEEPSEEK_MANAGED_CONFIG_PATH",
        "managed-config/codewhale.toml",
    )),
};
