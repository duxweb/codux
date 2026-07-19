use crate::ai_runtime::tool_driver::AIRuntimeToolDriver;

mod agy;
mod claude;
mod codewhale;
mod codex;
mod kimi;
mod kiro;
mod mimo;
mod omp;
mod opencode;

// Note: agy (Antigravity) is tracked through its current SQLite conversation
// database.
pub const AI_RUNTIME_TOOL_DRIVERS: &[AIRuntimeToolDriver] = &[
    codex::DRIVER,
    claude::DRIVER,
    opencode::DRIVER,
    kiro::DRIVER,
    codewhale::DRIVER,
    kimi::DRIVER,
    mimo::DRIVER,
    omp::DRIVER,
    agy::DRIVER,
];
