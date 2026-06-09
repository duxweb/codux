use crate::ai_runtime::tool_driver::{
    AIRuntimeMemoryInjectionDriver, AIRuntimeToolDriver, AIRuntimeToolHookDriver,
};

pub const DRIVER: AIRuntimeToolDriver = AIRuntimeToolDriver {
    id: "kimi",
    aliases: &["kimi", "kimi-code"],
    wrapper_bins: &["kimi", "kimi-code"],
    hook: AIRuntimeToolHookDriver::KimiToml,
    probe: None,
    memory_injection: AIRuntimeMemoryInjectionDriver::None,
};
