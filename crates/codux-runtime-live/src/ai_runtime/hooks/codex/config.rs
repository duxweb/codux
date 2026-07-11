use super::toml::{codex_hook_state_key, is_toml_table_header, normalized_line};
use std::{fs, path::Path};

/// Remove the codux-managed hook trust blocks from `config.toml` so the dead
/// `[hooks.state."…/.codex/hooks.json:…"]` trust entries don't linger after the
/// hooks.json entries are stripped. Leaves `[features] hooks` and the suppress
/// flag untouched (inert CLI toggles, not codux hooks).
pub(in crate::ai_runtime::hooks) fn uninstall_codex_config(
    hooks_path: &Path,
) -> Result<(), String> {
    let config_path = hooks_path
        .parent()
        .map(|parent| parent.join("config.toml"))
        .ok_or_else(|| "Codex hooks path has no parent directory.".to_string())?;
    let Ok(existing) = fs::read_to_string(&config_path) else {
        return Ok(());
    };
    let mut lines = existing
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    while lines
        .last()
        .map(|line| normalized_line(line).is_empty())
        .unwrap_or(false)
    {
        lines.pop();
    }
    let cleaned = remove_managed_codex_hook_trust_blocks(lines);
    let updated = if cleaned.is_empty() {
        String::new()
    } else {
        format!("{}\n", cleaned.join("\n"))
    };
    if existing == updated {
        return Ok(());
    }
    fs::write(config_path, updated).map_err(|error| error.to_string())
}

fn remove_managed_codex_hook_trust_blocks(lines: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if let Some(key) = codex_hook_state_key(&lines[index])
            && is_managed_codex_hook_state_key(&key)
        {
            index += 1;
            while index < lines.len() && !is_toml_table_header(&lines[index]) {
                index += 1;
            }
            continue;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninstall_codex_config_strips_managed_trust_blocks_and_keeps_user_entries() {
        let dir =
            std::env::temp_dir().join(format!("codux-codex-uninstall-{}", uuid::Uuid::new_v4()));
        let codex_dir = dir.join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let config_path = codex_dir.join("config.toml");
        let hooks_path = codex_dir.join("hooks.json");
        let managed_key = format!("{}:stop:0:0", hooks_path.display());
        std::fs::write(
            &config_path,
            format!(
                "[features]\nhooks = true\n\n[hooks.state.\"{managed_key}\"]\ntrusted_hash = \"sha256:old\"\n\n[hooks.state.\"/tmp/custom-hooks.json:stop:0:0\"]\ntrusted_hash = \"sha256:custom\"\n\n[profiles.work]\nmodel = \"gpt-5.5\"\n"
            ),
        )
        .unwrap();

        uninstall_codex_config(&hooks_path).unwrap();

        let text = std::fs::read_to_string(&config_path).unwrap();
        // codux-managed trust block removed; the user's custom block, profile and
        // inert feature toggles survive.
        assert!(!text.contains("sha256:old"));
        assert!(text.contains("/tmp/custom-hooks.json"));
        assert!(text.contains("sha256:custom"));
        assert!(text.contains("[profiles.work]\nmodel = \"gpt-5.5\""));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
