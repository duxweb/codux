use super::{
    super::{
        command::{
            shell_quote, windows_cmd_quote_cross_platform, windows_powershell_quote_cross_platform,
        },
        json::load_json_object,
    },
    toml::json_string_literal,
};
use crate::runtime_paths::app_slug;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone)]
pub(super) struct CodexHookTrustState {
    pub(super) key: String,
    pub(super) trusted_hash: String,
}

pub(super) fn managed_codex_hook_trust_states(
    hooks_path: &Path,
) -> Result<Vec<CodexHookTrustState>, String> {
    let root = load_json_object(hooks_path)?;
    let Some(hooks) = root.get("hooks").and_then(|value| value.as_object()) else {
        return Ok(Vec::new());
    };
    let actions = HashMap::from([
        ("SessionStart", "codex-session-start"),
        ("UserPromptSubmit", "codex-prompt-submit"),
        ("PermissionRequest", "codex-permission-request"),
        ("Stop", "codex-stop"),
    ]);
    let labels = HashMap::from([
        ("PermissionRequest", "permission_request"),
        ("SessionStart", "session_start"),
        ("Stop", "stop"),
        ("UserPromptSubmit", "user_prompt_submit"),
    ]);
    let mut states = Vec::new();
    for (event_key, event_label) in labels {
        let Some(action) = actions.get(event_key) else {
            continue;
        };
        let Some(groups) = hooks.get(event_key).and_then(|value| value.as_array()) else {
            continue;
        };
        for (group_index, group) in groups.iter().enumerate() {
            let Some(group_object) = group.as_object() else {
                continue;
            };
            let matcher = match event_key {
                "UserPromptSubmit" | "Stop" => None,
                _ => group_object.get("matcher").and_then(|value| value.as_str()),
            };
            let Some(hooks_array) = group_object.get("hooks").and_then(|value| value.as_array())
            else {
                continue;
            };
            for (handler_index, hook) in hooks_array.iter().enumerate() {
                let Some(hook_object) = hook.as_object() else {
                    continue;
                };
                if hook_object.get("type").and_then(|value| value.as_str()) != Some("command") {
                    continue;
                }
                let Some(command) = hook_object.get("command").and_then(|value| value.as_str())
                else {
                    continue;
                };
                if !is_codex_managed_hook_command(command, action, app_slug()) {
                    continue;
                }
                let timeout = hook_object
                    .get("timeout")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(600)
                    .max(1);
                let status_message = hook_object
                    .get("statusMessage")
                    .and_then(|value| value.as_str());
                states.push(CodexHookTrustState {
                    key: format!(
                        "{}:{}:{}:{}",
                        hooks_path.display(),
                        event_label,
                        group_index,
                        handler_index
                    ),
                    trusted_hash: codex_command_hook_trust_hash(
                        event_label,
                        matcher,
                        command,
                        timeout,
                        status_message,
                    ),
                });
            }
        }
    }
    Ok(states)
}

fn is_codex_managed_hook_command(command: &str, action: &str, owner: &str) -> bool {
    if command.contains("dmux-ai-state.sh")
        && command.contains(&shell_quote(action))
        && command.contains(&shell_quote(owner))
        && command.contains(&shell_quote("codex"))
    {
        return true;
    }
    if command.contains("dmux-ai-state.ps1")
        && command.contains(&windows_powershell_quote_cross_platform(action))
        && command.contains(&windows_powershell_quote_cross_platform(owner))
        && command.contains(&windows_powershell_quote_cross_platform("codex"))
    {
        return true;
    }
    command.contains("dmux-ai-state.cmd")
        && command.contains(&windows_cmd_quote_cross_platform(action))
        && command.contains(&windows_cmd_quote_cross_platform(owner))
        && command.contains(&windows_cmd_quote_cross_platform("codex"))
}

fn codex_command_hook_trust_hash(
    event_label: &str,
    matcher: Option<&str>,
    command: &str,
    timeout: i64,
    status_message: Option<&str>,
) -> String {
    let status_json = status_message
        .map(json_string_literal)
        .unwrap_or_else(|| "null".to_string());
    let hook_json = format!(
        "\"hooks\":[{{\"async\":false,\"command\":{},\"statusMessage\":{},\"timeout\":{},\"type\":\"command\"}}]",
        json_string_literal(command),
        status_json,
        timeout
    );
    let canonical_json = if let Some(matcher) = matcher {
        format!(
            "{{\"event_name\":{},\"hooks\":[{{\"async\":false,\"command\":{},\"statusMessage\":{},\"timeout\":{},\"type\":\"command\"}}],\"matcher\":{}}}",
            json_string_literal(event_label),
            json_string_literal(command),
            status_message
                .map(json_string_literal)
                .unwrap_or_else(|| "null".to_string()),
            timeout,
            json_string_literal(matcher)
        )
    } else {
        format!(
            "{{\"event_name\":{},{}}}",
            json_string_literal(event_label),
            hook_json
        )
    };
    let digest = Sha256::digest(canonical_json.as_bytes());
    format!("sha256:{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_hook_hash_is_stable_sha256() {
        let hash = codex_command_hook_trust_hash(
            "stop",
            None,
            "'/tmp/dmux-ai-state.sh' 'codex-stop' 'codux-tauri' 'codex'",
            1000,
            Some("codux codex live"),
        );

        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), "sha256:".len() + 64);
    }
}
