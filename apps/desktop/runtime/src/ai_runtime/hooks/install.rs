use super::{
    codewhale::install_codewhale_hooks_in,
    codex::ensure_codex_config_installed,
    command::{hook_command, is_managed_hook, is_managed_hook_action},
    json::{load_json_object, write_json_object},
    kimi::install_kimi_hooks_in,
};
use crate::{
    ai_runtime::tool_driver::{
        AIRuntimeJsonHookFormat, AIRuntimeToolHookDriver, ai_runtime_tool_drivers,
    },
    runtime_paths::{app_slug, home_dir},
};
use serde_json::{Map, Value, json};
use std::path::Path;

pub fn install_managed_hook_configs(managed_hook_script: &Path) -> Result<(), String> {
    install_managed_hook_configs_in(&home_dir(), managed_hook_script)
}

pub fn install_managed_hook_configs_in(
    home_dir: &Path,
    managed_hook_script: &Path,
) -> Result<(), String> {
    for driver in ai_runtime_tool_drivers() {
        match driver.hook {
            AIRuntimeToolHookDriver::Json(hook) => {
                let path = hook
                    .path_segments
                    .iter()
                    .fold(home_dir.to_path_buf(), |path, segment| path.join(segment));
                match hook.format {
                    AIRuntimeJsonHookFormat::Standard => {
                        install_tool_hooks(
                            &path,
                            hook.tool,
                            hook.definitions,
                            managed_hook_script,
                        )?;
                    }
                    AIRuntimeJsonHookFormat::Kiro => {
                        install_kiro_tool_hooks(&path, hook.definitions, managed_hook_script)?;
                    }
                }
                if hook.tool == "codex" {
                    ensure_codex_config_installed(&path)?;
                }
            }
            AIRuntimeToolHookDriver::CodeWhaleToml => {
                install_codewhale_hooks_in(home_dir, managed_hook_script)?;
            }
            AIRuntimeToolHookDriver::KimiToml => {
                install_kimi_hooks_in(home_dir, managed_hook_script)?;
            }
            AIRuntimeToolHookDriver::OpenCodePlugin | AIRuntimeToolHookDriver::None => {}
        }
    }
    Ok(())
}

fn install_tool_hooks(
    path: &Path,
    tool: &str,
    definitions: &[crate::ai_runtime::tool_driver::AIRuntimeHookDefinition],
    managed_hook_script: &Path,
) -> Result<(), String> {
    let owner = app_slug();
    if tool == "kiro" {
        return install_kiro_tool_hooks(path, definitions, managed_hook_script);
    }

    let mut root = load_json_object(path)?;
    let mut hooks = root
        .remove("hooks")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    for (event_key, action) in removed_hook_definitions(tool) {
        strip_managed_action_from_hooks(&mut hooks, event_key, action, None, Some(tool));
    }
    if tool == "claude" {
        strip_managed_action_from_hooks(
            &mut hooks,
            "Notification",
            "notification",
            None,
            Some("claude"),
        );
    }

    for definition in definitions {
        strip_managed_action_from_hooks(
            &mut hooks,
            definition.event_key,
            definition.action,
            None,
            Some(tool),
        );

        let command = hook_command(managed_hook_script, definition.action, owner, tool);
        let mut hook = Map::new();
        hook.insert("type".to_string(), Value::String("command".to_string()));
        hook.insert("command".to_string(), Value::String(command));
        hook.insert(
            "timeout".to_string(),
            Value::Number(definition.timeout_ms.into()),
        );
        hook.insert(
            "statusMessage".to_string(),
            Value::String(format!("codux {tool} live")),
        );
        if definition.is_async {
            hook.insert("async".to_string(), Value::Bool(true));
        }

        let groups = hooks
            .remove(definition.event_key)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        let mut cleaned = Vec::new();
        for group in groups {
            let Some(group_object) = group.as_object() else {
                continue;
            };
            let next_hooks = group_object
                .get("hooks")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|item| !is_managed_hook(item, definition.action, owner, tool))
                .collect::<Vec<_>>();
            if next_hooks.is_empty() {
                continue;
            }
            let mut next_group = group_object.clone();
            next_group.insert("hooks".to_string(), Value::Array(next_hooks));
            cleaned.push(Value::Object(next_group));
        }

        cleaned.push(json!({
            "matcher": "",
            "hooks": [Value::Object(hook)],
        }));
        hooks.insert(definition.event_key.to_string(), Value::Array(cleaned));
    }

    root.insert("hooks".to_string(), Value::Object(hooks));
    if tool == "gemini" || tool == "agy" {
        disable_gemini_hook_notifications(&mut root);
    }
    write_json_object(path, root)
}

fn install_kiro_tool_hooks(
    path: &Path,
    definitions: &[crate::ai_runtime::tool_driver::AIRuntimeHookDefinition],
    managed_hook_script: &Path,
) -> Result<(), String> {
    let owner = app_slug();
    let mut root = load_json_object(path)?;
    ensure_kiro_agent_config_fields(&mut root);
    let mut hooks = root
        .remove("hooks")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    for definition in definitions {
        strip_managed_action_from_hooks(
            &mut hooks,
            definition.event_key,
            definition.action,
            None,
            Some("kiro"),
        );

        let command = hook_command(managed_hook_script, definition.action, owner, "kiro");
        let mut hook = Map::new();
        hook.insert("command".to_string(), Value::String(command));
        hook.insert(
            "timeout_ms".to_string(),
            Value::Number(definition.timeout_ms.into()),
        );
        hook.insert("matcher".to_string(), Value::String(String::new()));
        if definition.is_async {
            hook.insert("async".to_string(), Value::Bool(true));
        }

        let entries = hooks
            .remove(definition.event_key)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        let mut cleaned = Vec::new();
        for entry in entries {
            if is_managed_hook(&entry, definition.action, owner, "kiro") {
                continue;
            }
            cleaned.push(entry);
        }
        cleaned.push(Value::Object(hook));
        hooks.insert(definition.event_key.to_string(), Value::Array(cleaned));
    }

    root.insert("hooks".to_string(), Value::Object(hooks));
    write_json_object(path, root)
}

fn ensure_kiro_agent_config_fields(root: &mut Map<String, Value>) {
    ensure_json_string_field(root, "name", "Codux Managed");
    ensure_json_string_field(root, "description", "Codux runtime lifecycle hook bridge.");
    ensure_json_string_field(root, "prompt", "Codux managed runtime hook agent.");
}

fn ensure_json_string_field(root: &mut Map<String, Value>, key: &str, default_value: &str) {
    let is_valid = root
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if !is_valid {
        root.insert(key.to_string(), Value::String(default_value.to_string()));
    }
}

fn disable_gemini_hook_notifications(root: &mut Map<String, Value>) {
    let mut hooks_config = root
        .remove("hooksConfig")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    hooks_config.insert("notifications".to_string(), Value::Bool(false));
    root.insert("hooksConfig".to_string(), Value::Object(hooks_config));
}

fn removed_hook_definitions(tool: &str) -> &'static [(&'static str, &'static str)] {
    match tool {
        "codex" => &[
            ("PreToolUse", "codex-pre-tool-use"),
            ("PostToolUse", "codex-post-tool-use"),
            ("SessionEnd", "codex-session-end"),
        ],
        "claude" => &[
            ("PreToolUse", "pre-tool-use"),
            ("PostToolUse", "post-tool-use"),
            ("PostToolUseFailure", "post-tool-use-failure"),
        ],
        _ => &[],
    }
}

fn strip_managed_action_from_hooks(
    hooks: &mut Map<String, Value>,
    event_key: &str,
    action: &str,
    owner: Option<&str>,
    tool: Option<&str>,
) {
    let groups = hooks
        .remove(event_key)
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    if groups.is_empty() {
        return;
    }

    let mut cleaned_groups = Vec::new();
    for group in groups {
        let Some(group_object) = group.as_object() else {
            continue;
        };
        let next_hooks = group_object
            .get("hooks")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|item| !is_managed_hook_action(item, action, owner, tool))
            .collect::<Vec<_>>();
        if next_hooks.is_empty() {
            continue;
        }
        let mut next_group = group_object.clone();
        next_group.insert("hooks".to_string(), Value::Array(next_hooks));
        cleaned_groups.push(Value::Object(next_group));
    }

    if !cleaned_groups.is_empty() {
        hooks.insert(event_key.to_string(), Value::Array(cleaned_groups));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_managed_action_removes_all_codux_owners_when_owner_is_unspecified() {
        let mut hooks = Map::new();
        hooks.insert(
            "Stop".to_string(),
            json!([
                {
                    "matcher": "",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "'/tmp/codux/dmux-ai-state.sh' 'codex-stop' 'codux' 'codex'"
                        },
                        {
                            "type": "command",
                            "command": "'/tmp/codux-dev/dmux-ai-state.sh' 'codex-stop' 'codux-dev' 'codex'"
                        },
                        {
                            "type": "command",
                            "command": "'/tmp/custom.sh' 'codex-stop' 'custom' 'codex'"
                        }
                    ]
                }
            ])
            .as_array()
            .cloned()
            .map(Value::Array)
            .unwrap(),
        );

        strip_managed_action_from_hooks(&mut hooks, "Stop", "codex-stop", None, Some("codex"));

        let remaining = hooks
            .get("Stop")
            .and_then(|value| value.as_array())
            .and_then(|groups| groups.first())
            .and_then(|group| group.get("hooks"))
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        assert_eq!(remaining.len(), 1);
        assert!(remaining[0]["command"].as_str().unwrap().contains("custom"));
    }
}
