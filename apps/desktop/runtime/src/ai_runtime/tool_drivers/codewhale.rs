use crate::ai_runtime::{
    probe::codewhale::probe_codewhale_runtime,
    tool_driver::{AIRuntimeMemoryInjectionDriver, AIRuntimeToolDriver, AIRuntimeToolHookDriver},
};

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "codewhale",
    aliases: &["codewhale", "codewhale-tui", "deepseek", "deepseek-tui"],
    wrapper_bins: &["codewhale", "codewhale-tui", "deepseek", "deepseek-tui"],
    hook: AIRuntimeToolHookDriver::CodeWhaleToml,
    probe: Some(probe_codewhale_runtime),
    memory_injection: AIRuntimeMemoryInjectionDriver::None,
};
