use super::{
    codewhale::codewhale_hook_config_status_in, command::is_managed_hook, json::load_json_object,
    kimi::kimi_hook_config_status_in,
};
use crate::{
    ai_runtime::bridge::{AIRuntimeHookConfigStatus, AIRuntimeToolHookConfigStatus},
    ai_runtime::tool_driver::{AIRuntimeToolHookDriver, ai_runtime_tool_drivers},
    runtime_paths::{app_slug, home_dir},
};
use serde_json::{Map, Value};
use std::path::Path;

pub fn hook_config_status(opencode_config_dir: &Path) -> AIRuntimeHookConfigStatus {
    hook_config_status_in(&home_dir(), opencode_config_dir)
}

pub fn hook_config_status_in(
    home_dir: &Path,
    opencode_config_dir: &Path,
) -> AIRuntimeHookConfigStatus {
    let mut codex = AIRuntimeToolHookConfigStatus::default();
    let mut claude = AIRuntimeToolHookConfigStatus::default();
    let mut gemini = AIRuntimeToolHookConfigStatus::default();
    let mut agy = AIRuntimeToolHookConfigStatus::default();
    let mut kiro = AIRuntimeToolHookConfigStatus::default();
    let mut codewhale = AIRuntimeToolHookConfigStatus::default();
    let mut kimi = AIRuntimeToolHookConfigStatus::default();
    let opencode = opencode_hook_config_status(opencode_config_dir);

    for driver in ai_runtime_tool_drivers() {
        let status = match driver.hook {
            AIRuntimeToolHookDriver::Json(hook) => {
                let path = hook
                    .path_segments
                    .iter()
                    .fold(home_dir.to_path_buf(), |path, segment| path.join(segment));
                tool_hook_config_status(
                    &path,
                    hook.tool,
                    hook.definitions
                        .iter()
                        .map(|definition| (definition.event_key, definition.action))
                        .collect::<Vec<_>>()
                        .as_slice(),
                )
            }
            AIRuntimeToolHookDriver::CodeWhaleToml => codewhale_hook_config_status_in(home_dir),
            AIRuntimeToolHookDriver::KimiToml => kimi_hook_config_status_in(home_dir),
            AIRuntimeToolHookDriver::OpenCodePlugin | AIRuntimeToolHookDriver::None => continue,
        };
        match driver.id {
            "codex" => codex = status,
            "claude" => claude = status,
            "gemini" => gemini = status,
            "agy" => agy = status,
            "kiro" => kiro = status,
            "codewhale" => codewhale = status,
            "kimi" => kimi = status,
            _ => {}
        }
    }

    AIRuntimeHookConfigStatus {
        codex,
        claude,
        gemini,
        agy,
        opencode,
        kiro,
        codewhale,
        kimi,
    }
}

pub fn tool_hook_config_status(
    path: &Path,
    tool: &str,
    definitions: &[(&str, &str)],
) -> AIRuntimeToolHookConfigStatus {
    let owner = app_slug();
    let root = load_json_object(path).unwrap_or_default();
    let hooks = root
        .get("hooks")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let missing = definitions
        .iter()
        .filter_map(|(event_key, action)| {
            (!has_managed_hook_for_event(&hooks, event_key, action, owner, tool))
                .then(|| format!("{event_key}:{action}"))
        })
        .collect::<Vec<_>>();
    AIRuntimeToolHookConfigStatus {
        configured: missing.is_empty(),
        config_path: path.display().to_string(),
        missing,
    }
}

pub fn opencode_hook_config_status(config_dir: &Path) -> AIRuntimeToolHookConfigStatus {
    let expected = [
        "package.json",
        "plugins/dmux-runtime.js",
        "node_modules/@opencode-ai/plugin/package.json",
    ];
    let missing = expected
        .iter()
        .filter(|relative| !config_dir.join(relative).exists())
        .map(|relative| relative.to_string())
        .collect::<Vec<_>>();
    AIRuntimeToolHookConfigStatus {
        configured: missing.is_empty(),
        config_path: config_dir.display().to_string(),
        missing,
    }
}

fn has_managed_hook_for_event(
    hooks: &Map<String, Value>,
    event_key: &str,
    action: &str,
    owner: &str,
    tool: &str,
) -> bool {
    hooks
        .get(event_key)
        .and_then(|value| value.as_array())
        .map(|groups| {
            groups.iter().any(|group| {
                is_managed_hook(group, action, owner, tool)
                    || group
                        .get("hooks")
                        .and_then(|value| value.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .any(|item| is_managed_hook(item, action, owner, tool))
                        })
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_paths::app_slug;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn tool_hook_config_status_requires_claude_compaction_hooks() {
        let root = std::env::temp_dir().join(format!("codux-claude-hooks-{}.json", Uuid::new_v4()));
        fs::write(
            &root,
            serde_json::json!({
                "hooks": {
                    "PreCompact": [{
                        "matcher": "",
                        "hooks": [{
                            "type": "command",
                            "command": format!("'/tmp/dmux-ai-state.sh' 'pre-compact' '{}' 'claude'", app_slug()),
                            "timeout": 10
                        }]
                    }],
                    "PostCompact": [{
                        "matcher": "",
                        "hooks": [{
                            "type": "command",
                            "command": format!("'/tmp/dmux-ai-state.sh' 'post-compact' '{}' 'claude'", app_slug()),
                            "timeout": 10
                        }]
                    }]
                }
            })
            .to_string(),
        )
        .unwrap();

        let status = tool_hook_config_status(
            &root,
            "claude",
            &[
                ("PreCompact", "pre-compact"),
                ("PostCompact", "post-compact"),
                ("Stop", "stop"),
            ],
        );

        assert!(!status.configured);
        assert_eq!(status.missing, vec!["Stop:stop"]);
        fs::remove_file(root).unwrap();
    }

    #[test]
    fn tool_hook_config_status_ignores_other_owner_hooks() {
        let root = std::env::temp_dir().join(format!("codux-owner-hooks-{}.json", Uuid::new_v4()));
        let other_owner = if app_slug() == "codux" {
            "codux-dev"
        } else {
            "codux"
        };
        fs::write(
            &root,
            serde_json::json!({
                "hooks": {
                    "Stop": [{
                        "matcher": "",
                        "hooks": [{
                            "type": "command",
                            "command": format!("'/tmp/dmux-ai-state.sh' 'stop' '{}' 'claude'", other_owner),
                            "timeout": 10
                        }]
                    }]
                }
            })
            .to_string(),
        )
        .unwrap();

        let status = tool_hook_config_status(&root, "claude", &[("Stop", "stop")]);

        assert!(!status.configured);
        assert_eq!(status.missing, vec!["Stop:stop"]);
        fs::remove_file(root).unwrap();
    }

    #[test]
    fn tool_hook_config_status_accepts_kiro_flat_hooks() {
        let root = std::env::temp_dir().join(format!("codux-kiro-hooks-{}.json", Uuid::new_v4()));
        fs::write(
            &root,
            serde_json::json!({
                "hooks": {
                    "agentSpawn": [{
                        "command": format!("'/tmp/dmux-ai-state.sh' 'session-start' '{}' 'kiro'", app_slug()),
                        "timeout_ms": 5000,
                        "matcher": ""
                    }],
                    "stop": [{
                        "command": format!("'/tmp/dmux-ai-state.sh' 'session-end' '{}' 'kiro'", app_slug()),
                        "timeout_ms": 5000,
                        "matcher": ""
                    }]
                }
            })
            .to_string(),
        )
        .unwrap();

        let status = tool_hook_config_status(
            &root,
            "kiro",
            &[("agentSpawn", "session-start"), ("stop", "session-end")],
        );

        assert!(status.configured);
        assert!(status.missing.is_empty());
        fs::remove_file(root).unwrap();
    }

    #[test]
    fn hook_config_status_reports_missing_codewhale_hooks() {
        let home = std::env::temp_dir().join(format!("codux-codewhale-hooks-{}", Uuid::new_v4()));
        fs::create_dir_all(&home).unwrap();

        let status = hook_config_status_in(&home, &home.join("opencode-config"));

        assert!(!status.codewhale.configured);
        assert!(!status.codewhale.missing.is_empty());
        assert!(
            status
                .codewhale
                .config_path
                .ends_with(".codewhale/config.toml")
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn hook_config_status_tracks_agy_separately_from_gemini() {
        let home = std::env::temp_dir().join(format!("codux-agy-hooks-{}", Uuid::new_v4()));
        fs::create_dir_all(home.join(".gemini")).unwrap();
        fs::write(
            home.join(".gemini/settings.json"),
            serde_json::json!({
                "hooks": {
                    "SessionStart": [{
                        "matcher": "",
                        "hooks": [{
                            "type": "command",
                            "command": format!("'/tmp/dmux-ai-state.sh' 'session-start' '{}' 'gemini'", app_slug()),
                            "timeout": 5000,
                            "statusMessage": "codux gemini live"
                        }]
                    }]
                }
            })
            .to_string(),
        )
        .unwrap();

        let status = hook_config_status_in(&home, &home.join("opencode-config"));

        assert!(
            !status
                .gemini
                .missing
                .iter()
                .any(|item| item.starts_with("agy:"))
        );
        assert!(
            status
                .agy
                .config_path
                .ends_with(".gemini/antigravity-cli/settings.json")
        );
        assert!(!status.agy.missing.is_empty());
        fs::remove_dir_all(home).unwrap();
    }
}
