pub(crate) mod agy;
pub(crate) mod claude;
pub(crate) mod codewhale;
pub(crate) mod codex;
mod common;
pub(crate) mod kimi;
pub(crate) mod kiro;
pub(crate) mod omp;
pub(crate) mod opencode;
pub(crate) mod paths;
mod preview;
mod usage;

use crate::ai_runtime::{
    snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest},
    tool_driver::{canonical_tool_name, runtime_tool_driver},
};

pub fn probe_runtime(request: &AIRuntimeProbeRequest) -> Option<AIRuntimeContextSnapshot> {
    runtime_tool_driver(&request.tool)?.probe?(request)
}

pub(crate) fn probe_runtime_with_claude_cache(
    request: &AIRuntimeProbeRequest,
    claude_cache: &mut claude::ClaudeProbeCache,
) -> Option<AIRuntimeContextSnapshot> {
    if canonical_tool_name(&request.tool) == Some("claude") {
        return claude::probe_claude_runtime_with_cache(request, claude_cache);
    }
    probe_runtime(request)
}
