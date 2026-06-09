use super::{
    toml::{
        codex_hook_state_key, is_toml_table_header, normalized_line, toml_key_name,
        toml_quoted_string, toml_section_end,
    },
    trust::{CodexHookTrustState, managed_codex_hook_trust_states},
};
use std::{fs, path::Path};

pub(in crate::ai_runtime::hooks) fn ensure_codex_config_installed(
    hooks_path: &Path,
) -> Result<(), String> {
    let config_path = hooks_path
        .parent()
        .map(|parent| parent.join("config.toml"))
        .ok_or_else(|| "Codex hooks path has no parent directory.".to_string())?;
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let updated =
        updated_codex_config_text(&existing, &managed_codex_hook_trust_states(hooks_path)?);
    if existing == updated {
        return Ok(());
    }
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(config_path, updated).map_err(|error| error.to_string())
}

fn updated_codex_config_text(existing_text: &str, states: &[CodexHookTrustState]) -> String {
    let target_line = "suppress_unstable_features_warning = true";
    let mut lines = existing_text
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::to_string)
        .filter(|line| !normalized_line(line).starts_with("suppress_unstable_features_warning"))
        .collect::<Vec<_>>();
    while lines
        .last()
        .map(|line| normalized_line(line).is_empty())
        .unwrap_or(false)
    {
        lines.pop();
    }

    if lines.is_empty() {
        lines.push(target_line.to_string());
    } else {
        let first_table = lines
            .iter()
            .position(|line| is_toml_table_header(line))
            .unwrap_or(lines.len());
        lines.insert(first_table, target_line.to_string());
        if first_table + 1 < lines.len() && !normalized_line(&lines[first_table + 1]).is_empty() {
            lines.insert(first_table + 1, String::new());
        }
    }

    ensure_codex_hooks_feature(&mut lines);
    let mut sorted_states = states.to_vec();
    sorted_states.sort_by(|left, right| left.key.cmp(&right.key));
    lines = remove_managed_codex_hook_trust_blocks(lines);
    append_codex_hook_trust_states(&mut lines, &sorted_states);
    format!("{}\n", lines.join("\n"))
}

fn ensure_codex_hooks_feature(lines: &mut Vec<String>) {
    let Some(features_index) = lines
        .iter()
        .position(|line| normalized_line(line) == "[features]")
    else {
        if !lines.is_empty() && !normalized_line(lines.last().unwrap_or(&String::new())).is_empty()
        {
            lines.push(String::new());
        }
        lines.push("[features]".to_string());
        lines.push("hooks = true".to_string());
        return;
    };
    let section_end = toml_section_end(lines, features_index);
    let mut hooks_index = None;
    let mut legacy_hooks_index = None;
    let mut removal_indices = Vec::new();
    for index in (features_index + 1)..section_end {
        match toml_key_name(&lines[index]).as_deref() {
            Some("hooks") => hooks_index = hooks_index.or(Some(index)),
            Some("codex_hooks") => legacy_hooks_index = legacy_hooks_index.or(Some(index)),
            _ => {}
        }
    }
    if let Some(index) = hooks_index {
        lines[index] = "hooks = true".to_string();
        if let Some(legacy) = legacy_hooks_index {
            removal_indices.push(legacy);
        }
    } else if let Some(index) = legacy_hooks_index {
        lines[index] = "hooks = true".to_string();
    } else {
        lines.insert(section_end, "hooks = true".to_string());
    }
    removal_indices.sort_unstable_by(|left, right| right.cmp(left));
    removal_indices.dedup();
    for index in removal_indices {
        lines.remove(index);
    }
}

fn remove_managed_codex_hook_trust_blocks(lines: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if let Some(key) = codex_hook_state_key(&lines[index]) {
            if is_managed_codex_hook_state_key(&key) {
                index += 1;
                while index < lines.len() && !is_toml_table_header(&lines[index]) {
                    index += 1;
                }
                continue;
            }
        }
        result.push(lines[index].clone());
        index += 1;
    }
    result
}

fn is_managed_codex_hook_state_key(key: &str) -> bool {
    let normalized = key.replace('\\', "/");
    let Some((path, event, _, _)) = parse_codex_hook_state_key(&normalized) else {
        return false;
    };
    path.ends_with("/.codex/hooks.json")
        && matches!(
            event,
            "permission_request" | "session_start" | "stop" | "user_prompt_submit"
        )
}

fn parse_codex_hook_state_key(key: &str) -> Option<(&str, &str, &str, &str)> {
    let (head, handler) = key.rsplit_once(':')?;
    let (head, group) = head.rsplit_once(':')?;
    let (path, event) = head.rsplit_once(':')?;
    Some((path, event, group, handler))
}

fn append_codex_hook_trust_states(lines: &mut Vec<String>, states: &[CodexHookTrustState]) {
    if states.is_empty() {
        return;
    }
    if !lines
        .iter()
        .any(|line| normalized_line(line) == "[hooks.state]")
    {
        if !lines.is_empty() && !normalized_line(lines.last().unwrap_or(&String::new())).is_empty()
        {
            lines.push(String::new());
        }
        lines.push("[hooks.state]".to_string());
    }
    for state in states {
        if !lines.is_empty() && !normalized_line(lines.last().unwrap_or(&String::new())).is_empty()
        {
            lines.push(String::new());
        }
        lines.push(format!("[hooks.state.{}]", toml_quoted_string(&state.key)));
        lines.push(format!(
            "trusted_hash = {}",
            toml_quoted_string(&state.trusted_hash)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_config_updater_preserves_profiles_and_replaces_hook_states() {
        let existing = r#"
[features]
codex_hooks = false

[hooks.state."/tmp/home/.codex/hooks.json:stop:0:0"]
trusted_hash = "sha256:old"

[profiles.work]
model = "gpt-5.5"
"#;
        let updated = updated_codex_config_text(
            existing,
            &[CodexHookTrustState {
                key: "/tmp/home/.codex/hooks.json:stop:0:0".to_string(),
                trusted_hash: "sha256:new".to_string(),
            }],
        );

        assert!(updated.contains("suppress_unstable_features_warning = true"));
        assert!(updated.contains("[features]\nhooks = true"));
        assert!(!updated.contains("codex_hooks"));
        assert!(!updated.contains("sha256:old"));
        assert!(updated.contains("[hooks.state.\"/tmp/home/.codex/hooks.json:stop:0:0\"]"));
        assert!(updated.contains("trusted_hash = \"sha256:new\""));
        assert!(updated.contains("[profiles.work]\nmodel = \"gpt-5.5\""));
    }

    #[test]
    fn codex_config_updater_removes_stale_managed_hook_states() {
        let existing = r#"
[hooks.state."/tmp/codux-test/home/.codex/hooks.json:user_prompt_submit:0:0"]
trusted_hash = "sha256:old"

[hooks.state."/tmp/custom-hooks.json:user_prompt_submit:0:0"]
trusted_hash = "sha256:custom"

[profiles.work]
model = "gpt-5.5"
"#;
        let updated = updated_codex_config_text(
            existing,
            &[CodexHookTrustState {
                key: "/Users/user/.codex/hooks.json:user_prompt_submit:0:0".to_string(),
                trusted_hash: "sha256:new".to_string(),
            }],
        );

        assert!(!updated.contains("/tmp/codux-test/home/.codex/hooks.json"));
        assert!(updated.contains("/tmp/custom-hooks.json"));
        assert!(updated.contains("/Users/user/.codex/hooks.json"));
        assert!(updated.contains("trusted_hash = \"sha256:new\""));
        assert!(updated.contains("[profiles.work]\nmodel = \"gpt-5.5\""));
    }

    #[test]
    fn codex_config_updater_removes_legacy_hooks_when_hooks_exists() {
        let updated =
            updated_codex_config_text("[features]\nhooks = false\ncodex_hooks = false\n", &[]);

        assert!(updated.contains("[features]\nhooks = true"));
        assert!(!updated.contains("codex_hooks"));
    }
}
