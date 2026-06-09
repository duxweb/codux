use crate::ai_runtime::snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest};
use serde::Serialize;

pub type AIRuntimeProbeFn = fn(&AIRuntimeProbeRequest) -> Option<AIRuntimeContextSnapshot>;

#[derive(Debug, Clone, Copy)]
pub struct AIRuntimeToolDriver {
    pub id: &'static str,
    pub aliases: &'static [&'static str],
    pub wrapper_bins: &'static [&'static str],
    pub hook: AIRuntimeToolHookDriver,
    pub probe: Option<AIRuntimeProbeFn>,
    pub memory_injection: AIRuntimeMemoryInjectionDriver,
}

#[derive(Debug, Clone, Copy)]
pub enum AIRuntimeToolHookDriver {
    Json(AIRuntimeJsonHookDriver),
    CodeWhaleToml,
    KimiToml,
    OpenCodePlugin,
    None,
}

#[derive(Debug, Clone, Copy)]
pub struct AIRuntimeJsonHookDriver {
    pub tool: &'static str,
    pub path_segments: &'static [&'static str],
    pub format: AIRuntimeJsonHookFormat,
    pub definitions: &'static [AIRuntimeHookDefinition],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AIRuntimeJsonHookFormat {
    Standard,
    Kiro,
}

#[derive(Debug, Clone, Copy)]
pub struct AIRuntimeHookDefinition {
    pub event_key: &'static str,
    pub action: &'static str,
    pub timeout_ms: i64,
    pub is_async: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AIRuntimeMemoryInjectionDriver {
    None,
    CodexDeveloperInstructions,
    ClaudeAppendSystemPrompt,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeToolLaunchDriverConfig {
    pub id: &'static str,
    pub aliases: &'static [&'static str],
    pub memory_injection: AIRuntimeMemoryInjectionDriver,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeToolLaunchDriverConfigFile {
    pub tools: Vec<AIRuntimeToolLaunchDriverConfig>,
}

pub const fn hook(
    event_key: &'static str,
    action: &'static str,
    timeout_ms: i64,
    is_async: bool,
) -> AIRuntimeHookDefinition {
    AIRuntimeHookDefinition {
        event_key,
        action,
        timeout_ms,
        is_async,
    }
}

pub fn ai_runtime_tool_drivers() -> &'static [AIRuntimeToolDriver] {
    crate::ai_runtime::tool_drivers::AI_RUNTIME_TOOL_DRIVERS
}

pub fn ai_runtime_tool_launch_driver_config() -> AIRuntimeToolLaunchDriverConfigFile {
    AIRuntimeToolLaunchDriverConfigFile {
        tools: ai_runtime_tool_drivers()
            .iter()
            .map(|driver| AIRuntimeToolLaunchDriverConfig {
                id: driver.id,
                aliases: driver.aliases,
                memory_injection: driver.memory_injection,
            })
            .collect(),
    }
}

pub fn canonical_tool_name(tool: &str) -> Option<&'static str> {
    let normalized = tool.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    ai_runtime_tool_drivers()
        .iter()
        .find(|driver| driver.aliases.iter().any(|alias| *alias == normalized))
        .map(|driver| driver.id)
}

pub fn runtime_tool_driver(tool: &str) -> Option<&'static AIRuntimeToolDriver> {
    let canonical = canonical_tool_name(tool)?;
    ai_runtime_tool_drivers()
        .iter()
        .find(|driver| driver.id == canonical)
}

pub fn is_supported_runtime_tool(tool: &str) -> bool {
    canonical_tool_name(tool).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_runtime_tool_aliases() {
        assert_eq!(canonical_tool_name("claude-code"), Some("claude"));
        assert_eq!(canonical_tool_name("agy"), Some("agy"));
        assert_eq!(canonical_tool_name("codewhale-tui"), Some("codewhale"));
        assert_eq!(canonical_tool_name("deepseek-tui"), Some("codewhale"));
        assert_eq!(canonical_tool_name("kimi-code"), Some("kimi"));
        assert_eq!(canonical_tool_name("codex"), Some("codex"));
    }

    #[test]
    fn codewhale_driver_registers_realtime_probe() {
        let driver = runtime_tool_driver("deepseek-tui").expect("codewhale driver");
        assert_eq!(driver.id, "codewhale");
        assert!(driver.probe.is_some());
    }

    #[test]
    fn agy_driver_registers_realtime_probe() {
        let driver = runtime_tool_driver("agy").expect("agy driver");
        assert_eq!(driver.id, "agy");
        assert!(driver.probe.is_some());
    }

    #[test]
    fn launch_driver_config_keeps_memory_injection_per_driver() {
        let config = ai_runtime_tool_launch_driver_config();
        let codex = config.tools.iter().find(|tool| tool.id == "codex").unwrap();
        let claude = config
            .tools
            .iter()
            .find(|tool| tool.id == "claude")
            .unwrap();
        let codewhale = config
            .tools
            .iter()
            .find(|tool| tool.id == "codewhale")
            .unwrap();

        assert_eq!(
            codex.memory_injection,
            AIRuntimeMemoryInjectionDriver::CodexDeveloperInstructions
        );
        assert_eq!(
            claude.memory_injection,
            AIRuntimeMemoryInjectionDriver::ClaudeAppendSystemPrompt
        );
        assert_eq!(
            codewhale.memory_injection,
            AIRuntimeMemoryInjectionDriver::None
        );
    }
}
