use crate::ai_runtime::tool_driver::{AIRuntimeHookDefinition, AIRuntimeToolDriver, hook};

mod agy;
mod claude;
mod codewhale;
mod codex;
mod gemini;
mod kiro;
mod opencode;

pub const AI_RUNTIME_TOOL_DRIVERS: &[AIRuntimeToolDriver] = &[
    codex::DRIVER,
    claude::DRIVER,
    gemini::DRIVER,
    opencode::DRIVER,
    kiro::DRIVER,
    codewhale::DRIVER,
    agy::DRIVER,
];

pub(crate) const GEMINI_HOOKS: &[AIRuntimeHookDefinition] = &[
    hook("SessionStart", "session-start", 5000, false),
    hook("BeforeAgent", "before-agent", 5000, false),
    hook("AfterAgent", "after-agent", 5000, false),
    hook("Notification", "notification", 5000, false),
    hook("SessionEnd", "session-end", 5000, false),
];
