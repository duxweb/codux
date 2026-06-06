pub(crate) mod claude;
pub(crate) mod codewhale;
pub(crate) mod codex;
mod common;
pub(crate) mod gemini;
pub(crate) mod kiro;
pub(crate) mod opencode;
pub(crate) mod paths;
mod preview;
mod usage;

use crate::ai_runtime::{
    snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest},
    tool_driver::runtime_tool_driver,
};

pub fn probe_runtime(request: &AIRuntimeProbeRequest) -> Option<AIRuntimeContextSnapshot> {
    runtime_tool_driver(&request.tool)?.probe?(request)
}
