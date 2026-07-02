use crate::ai_runtime::{
    snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest, AISessionSnapshot},
    state::normalized_string,
};
use serde::Serialize;
use std::path::PathBuf;

pub type AIRuntimeProbeFn = fn(&AIRuntimeProbeRequest) -> Option<AIRuntimeContextSnapshot>;
pub type AIRuntimeProbeResourceFn = fn(&AISessionSnapshot) -> Vec<PathBuf>;

#[derive(Debug, Clone, Copy)]
pub struct AIRuntimeToolDriver {
    pub id: &'static str,
    pub aliases: &'static [&'static str],
    pub process_names: &'static [&'static str],
    pub wrapper_bins: &'static [&'static str],
    pub liveness_from_process: bool,
    pub screen_starts_idle: bool,
    pub screen_patterns: AIRuntimeScreenPatterns,
    pub hook: AIRuntimeToolHookDriver,
    pub probe: Option<AIRuntimeProbeFn>,
    pub resource_paths: Option<AIRuntimeProbeResourceFn>,
    pub memory_injection: AIRuntimeMemoryInjectionDriver,
    pub lifecycle_hook_format: AIRuntimeLifecycleHookFormat,
    pub lifecycle_hooks: &'static [AIRuntimeLifecycleHookDefinition],
    pub lifecycle_config: Option<AIRuntimeLifecycleConfigDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AIRuntimeScreenPatterns {
    pub running: &'static [&'static str],
    pub waiting: &'static [&'static str],
}

pub const NO_SCREEN_PATTERNS: AIRuntimeScreenPatterns = AIRuntimeScreenPatterns {
    running: &[],
    waiting: &[],
};

pub const COMMON_SCREEN_PATTERNS: AIRuntimeScreenPatterns = AIRuntimeScreenPatterns {
    running: &[
        "esc to interrupt",
        "ctrl+c to interrupt",
        "esc to cancel",
        "interrupt)",
    ],
    waiting: &[
        "do you want",
        "would you like",
        "allow execution",
        "allow command",
        "apply this change",
        "do you want to proceed",
        "permission required",
        "press enter to confirm",
        "waiting for user confirmation",
        "yes, allow once",
        "no, and tell",
        "allow this action",
        "trust this",
        "[y/n]",
        "(y/n)",
        "y/n/t",
        "approve?",
        "allow?",
        "proceed?",
        "confirm?",
    ],
};

pub const KIRO_SCREEN_PATTERNS: AIRuntimeScreenPatterns = AIRuntimeScreenPatterns {
    running: &["kiro is working"],
    waiting: &[],
};

#[derive(Debug, Clone, Copy)]
pub enum AIRuntimeToolHookDriver {
    CodeWhaleToml,
    OpenCodePlugin,
    None,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeLifecycleHookDefinition {
    pub event_key: &'static str,
    pub action: &'static str,
    pub timeout_secs: u64,
    pub background: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeLifecycleConfigDefinition {
    pub env_var: &'static str,
    pub relative_path: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AIRuntimeLifecycleHookFormat {
    None,
    CodeWhaleToml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AIRuntimeMemoryInjectionDriver {
    None,
    CodexDeveloperInstructions,
    ClaudeAppendSystemPrompt,
    #[serde(rename = "codewhaleExecAppendSystemPrompt")]
    CodeWhaleExecAppendSystemPrompt,
    #[serde(rename = "kimiAgentFile")]
    KimiAgentFile,
    #[serde(rename = "opencodeSystemTransform")]
    OpenCodeSystemTransform,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeToolLaunchDriverConfig {
    pub id: &'static str,
    pub aliases: &'static [&'static str],
    pub memory_injection: AIRuntimeMemoryInjectionDriver,
    pub lifecycle_hook_format: AIRuntimeLifecycleHookFormat,
    pub lifecycle_hooks: &'static [AIRuntimeLifecycleHookDefinition],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle_config: Option<AIRuntimeLifecycleConfigDefinition>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeToolLaunchDriverConfigFile {
    pub tools: Vec<AIRuntimeToolLaunchDriverConfig>,
}

pub const fn lifecycle_hook(
    event_key: &'static str,
    action: &'static str,
    timeout_secs: u64,
    background: bool,
) -> AIRuntimeLifecycleHookDefinition {
    AIRuntimeLifecycleHookDefinition {
        event_key,
        action,
        timeout_secs,
        background,
    }
}

pub const fn lifecycle_config(
    env_var: &'static str,
    relative_path: &'static str,
) -> AIRuntimeLifecycleConfigDefinition {
    AIRuntimeLifecycleConfigDefinition {
        env_var,
        relative_path,
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
                lifecycle_hook_format: driver.lifecycle_hook_format,
                lifecycle_hooks: driver.lifecycle_hooks,
                lifecycle_config: driver.lifecycle_config,
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
        .find(|driver| {
            driver.id == normalized || driver.aliases.iter().any(|alias| *alias == normalized)
        })
        .map(|driver| driver.id)
}

pub fn canonical_command_tool_name(command: &str) -> Option<&'static str> {
    let normalized = command.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    ai_runtime_tool_drivers()
        .iter()
        .find(|driver| driver.aliases.iter().any(|alias| *alias == normalized))
        .map(|driver| driver.id)
}

pub fn canonical_process_tool_name(command: &str) -> Option<&'static str> {
    let normalized = command.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    ai_runtime_tool_drivers()
        .iter()
        .find(|driver| {
            driver.aliases.iter().any(|alias| *alias == normalized)
                || driver
                    .process_names
                    .iter()
                    .any(|process_name| *process_name == normalized)
        })
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

pub fn process_liveness_tool(tool: &str) -> bool {
    runtime_tool_driver(tool)
        .map(|driver| driver.liveness_from_process)
        .unwrap_or(false)
}

pub fn screen_starts_idle_tool(tool: &str) -> bool {
    runtime_tool_driver(tool)
        .map(|driver| driver.screen_starts_idle)
        .unwrap_or(false)
}

pub fn runtime_screen_patterns(tool: &str) -> Vec<AIRuntimeScreenPatterns> {
    let Some(driver) = runtime_tool_driver(tool) else {
        return vec![COMMON_SCREEN_PATTERNS];
    };
    let mut patterns = vec![COMMON_SCREEN_PATTERNS];
    if driver.screen_patterns.running.is_empty() && driver.screen_patterns.waiting.is_empty() {
        return patterns;
    }
    if driver.screen_patterns != COMMON_SCREEN_PATTERNS {
        patterns.push(driver.screen_patterns);
    }
    patterns
}

pub fn transcript_resource_paths(session: &AISessionSnapshot) -> Vec<PathBuf> {
    normalized_string(session.transcript_path.as_deref())
        .map(PathBuf::from)
        .into_iter()
        .collect()
}

pub fn runtime_resource_paths(session: &AISessionSnapshot) -> Vec<PathBuf> {
    runtime_tool_driver(&session.tool)
        .and_then(|driver| driver.resource_paths)
        .map(|resource_paths| resource_paths(session))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_runtime_tool_aliases() {
        assert_eq!(canonical_tool_name("claude-code"), Some("claude"));
        assert_eq!(canonical_tool_name("reclaude"), Some("claude"));
        assert_eq!(canonical_tool_name("agy"), Some("agy"));
        assert_eq!(canonical_tool_name("codewhale"), Some("codewhale"));
        assert_eq!(canonical_tool_name("kimi-code"), Some("kimi"));
        assert_eq!(canonical_tool_name("mimo"), Some("mimo"));
        assert_eq!(canonical_tool_name("codex"), Some("codex"));
        assert_eq!(canonical_tool_name("kiro"), Some("kiro"));
        assert_eq!(canonical_tool_name("kiro-cli"), Some("kiro"));
        assert_eq!(canonical_command_tool_name("kiro"), None);
        assert_eq!(canonical_command_tool_name("kiro-cli"), Some("kiro"));
        assert_eq!(canonical_process_tool_name("kiro-cli-chat"), Some("kiro"));
    }

    #[test]
    fn codewhale_driver_registers_realtime_probe() {
        let driver = runtime_tool_driver("codewhale").expect("codewhale driver");
        assert_eq!(driver.id, "codewhale");
        assert!(driver.probe.is_some());
    }

    #[test]
    fn every_probe_driver_registers_resource_path_resolver() {
        for driver in ai_runtime_tool_drivers() {
            if driver.probe.is_none() {
                continue;
            }
            assert!(
                driver.resource_paths.is_some(),
                "driver {} has a probe but no resource path resolver",
                driver.id
            );
        }
    }

    #[test]
    fn opencode_and_mimo_monitor_database_and_session_map_file() {
        let opencode_paths = runtime_resource_paths(&session_for_tool("opencode"))
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        assert!(
            opencode_paths
                .iter()
                .any(|path| path.ends_with(".local/share/opencode/opencode.db")),
            "opencode should monitor opencode.db"
        );
        assert!(
            opencode_paths
                .iter()
                .any(|path| path.ends_with(".local/share/opencode/opencode.db-wal")),
            "opencode should monitor opencode.db WAL writes"
        );
        assert!(
            opencode_paths
                .iter()
                .any(|path| path.ends_with(".local/share/opencode/opencode-local.db")),
            "opencode should monitor current upstream local channel db"
        );
        assert!(
            opencode_paths
                .iter()
                .any(|path| path.ends_with("opencode-session-map/opencode-session-terminal-1.json")),
            "opencode should monitor its exact session map file"
        );
        assert!(
            !opencode_paths
                .iter()
                .any(|path| path.ends_with("opencode-session-map")),
            "opencode should not monitor broad session-map directories"
        );

        let mimo_paths = runtime_resource_paths(&session_for_tool("mimo"))
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        assert!(
            mimo_paths
                .iter()
                .any(|path| path.ends_with(".local/share/mimocode/mimocode.db")),
            "mimo should monitor mimocode.db"
        );
        assert!(
            mimo_paths
                .iter()
                .any(|path| path.ends_with(".local/share/mimocode/mimocode.db-wal")),
            "mimo should monitor mimocode.db WAL writes"
        );
        assert!(
            mimo_paths
                .iter()
                .any(|path| path.ends_with(".local/share/mimocode/mimocode-local.db")),
            "mimo should monitor current upstream local channel db"
        );
        assert!(
            mimo_paths
                .iter()
                .any(|path| path.ends_with("opencode-session-map/opencode-session-terminal-1.json")),
            "mimo should monitor its exact plugin session map file"
        );
        assert!(
            !mimo_paths
                .iter()
                .any(|path| path.ends_with(".local/share/opencode/opencode.db")),
            "mimo should not monitor opencode.db"
        );
    }

    #[test]
    fn kiro_monitors_current_session_files_after_session_id_resolves() {
        let paths = runtime_resource_paths(&session_for_tool("kiro-cli"))
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with(".kiro/sessions/cli/session-1.json"))
        );
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with(".kiro/sessions/cli/session-1.jsonl"))
        );
        assert!(!paths.iter().any(|path| path.ends_with("data.sqlite3")));
        assert!(
            !paths
                .iter()
                .any(|path| path.ends_with(".kiro/sessions/cli"))
        );
    }

    #[test]
    fn kiro_monitors_session_directory_before_session_id_resolves() {
        let session = AISessionSnapshot {
            ai_session_id: None,
            ..session_for_tool("kiro-cli")
        };
        let paths = runtime_resource_paths(&session)
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with(".kiro/sessions/cli"))
        );
    }

    #[test]
    fn agy_resource_paths_use_project_matched_antigravity_db_only() {
        let session = AISessionSnapshot {
            transcript_path: Some(
                "/tmp/agy-brain/session/.system_generated/logs/transcript.jsonl".to_string(),
            ),
            ..session_for_tool("agy")
        };
        let paths = runtime_resource_paths(&session)
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        assert!(paths.is_empty());
    }

    #[test]
    fn codewhale_driver_monitors_current_session_files_only() {
        let root =
            std::env::temp_dir().join(format!("codux-codewhale-driver-{}", uuid::Uuid::new_v4()));
        let old_home = std::env::var_os("CODEWHALE_HOME");
        let sessions = root.join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(sessions.join("thread-1.json"), "{}").unwrap();
        std::fs::write(sessions.join("thread-2.json"), "{}").unwrap();
        unsafe {
            std::env::set_var("CODEWHALE_HOME", &root);
        }
        let session = AISessionSnapshot {
            tool: "codewhale".to_string(),
            ai_session_id: Some("thread-1".to_string()),
            ..session_for_tool("codewhale")
        };

        let paths = runtime_resource_paths(&session)
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with(".json") && path.ends_with("thread-1.json"))
        );
        assert!(!paths.iter().any(|path| path.ends_with("thread-2.json")));
        assert!(!paths.iter().any(|path| path.ends_with("state.db")));
        assert!(!paths.iter().any(|path| path.contains("/runtime/")));

        restore_env("CODEWHALE_HOME", old_home);
        let _ = std::fs::remove_dir_all(root);
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
        let kimi = config.tools.iter().find(|tool| tool.id == "kimi").unwrap();

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
            AIRuntimeMemoryInjectionDriver::CodeWhaleExecAppendSystemPrompt
        );
        assert_eq!(
            kimi.memory_injection,
            AIRuntimeMemoryInjectionDriver::KimiAgentFile
        );
        assert!(
            codewhale
                .lifecycle_hooks
                .iter()
                .any(|hook| hook.event_key == "turn_end" && hook.action == "codewhale-turn-end")
        );
        assert_eq!(
            codewhale.lifecycle_hook_format,
            AIRuntimeLifecycleHookFormat::CodeWhaleToml
        );
    }

    #[test]
    fn active_drivers_do_not_use_legacy_global_hook_configs() {
        for driver in ai_runtime_tool_drivers() {
            match driver.id {
                "codex" | "claude" | "kimi" | "kiro" | "agy" => {
                    assert!(
                        matches!(driver.hook, AIRuntimeToolHookDriver::None),
                        "{} must not modify global CLI hook configs",
                        driver.id
                    );
                    assert_eq!(
                        driver.lifecycle_hook_format,
                        AIRuntimeLifecycleHookFormat::None,
                        "{} must not expose staged lifecycle hooks",
                        driver.id
                    );
                    assert!(
                        driver.lifecycle_hooks.is_empty(),
                        "{} must not publish lifecycle hook definitions",
                        driver.id
                    );
                    assert!(
                        driver.lifecycle_config.is_none(),
                        "{} must not publish lifecycle config files",
                        driver.id
                    );
                }
                "codewhale" => {
                    assert!(matches!(
                        driver.hook,
                        AIRuntimeToolHookDriver::CodeWhaleToml
                    ));
                    assert_eq!(
                        driver.lifecycle_hook_format,
                        AIRuntimeLifecycleHookFormat::CodeWhaleToml
                    );
                }
                "opencode" | "mimo" => {
                    assert!(matches!(
                        driver.hook,
                        AIRuntimeToolHookDriver::OpenCodePlugin
                    ));
                    assert_eq!(
                        driver.lifecycle_hook_format,
                        AIRuntimeLifecycleHookFormat::None
                    );
                    assert!(driver.lifecycle_hooks.is_empty());
                    assert!(driver.lifecycle_config.is_none());
                }
                _ => panic!("unexpected AI runtime tool driver {}", driver.id),
            }
        }
    }

    fn restore_env(key: &str, value: Option<std::ffi::OsString>) {
        unsafe {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }

    fn session_for_tool(tool: &str) -> AISessionSnapshot {
        AISessionSnapshot {
            terminal_id: "terminal-1".to_string(),
            terminal_instance_id: Some("instance-1".to_string()),
            project_id: "project-1".to_string(),
            project_name: "Project".to_string(),
            project_path: Some("/tmp/project".to_string()),
            session_title: "Terminal".to_string(),
            tool: tool.to_string(),
            ai_session_id: Some("session-1".to_string()),
            model: None,
            state: "idle".to_string(),
            status: "idle".to_string(),
            is_running: false,
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 0,
            baseline_total_tokens: 0,
            baseline_cached_input_tokens: 0,
            usage_amounts: Vec::new(),
            baseline_usage_amounts: Vec::new(),
            baseline_resolved: false,
            started_at: Some(1.0),
            updated_at: 1.0,
            active_turn_started_at: None,
            runtime_turn_started_at: None,
            completed_turn_started_at: None,
            has_completed_turn: false,
            was_interrupted: false,
            transcript_path: Some(format!("/tmp/{tool}-transcript.jsonl")),
            notification_type: None,
            target_tool_name: None,
            message: None,
            latest_assistant_preview: None,
            plan: None,
        }
    }
}
