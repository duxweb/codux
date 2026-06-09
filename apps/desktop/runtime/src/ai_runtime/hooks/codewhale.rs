use super::command::hook_command;
use crate::{ai_runtime::bridge::AIRuntimeToolHookConfigStatus, runtime_paths::app_slug};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MANAGED_NAME_PREFIX: &str = "codux-codewhale-";

pub(in crate::ai_runtime::hooks) const CODEWHALE_HOOKS: &[(&str, &str)] = &[
    ("session_start", "codewhale-session-start"),
    ("message_submit", "codewhale-message-submit"),
    ("tool_call_before", "codewhale-tool-call-before"),
    ("tool_call_after", "codewhale-tool-call-after"),
    ("on_error", "codewhale-error"),
    ("session_end", "codewhale-session-end"),
];

pub(in crate::ai_runtime::hooks) fn codewhale_config_path_in(home_dir: &Path) -> PathBuf {
    home_dir.join(".codewhale").join("config.toml")
}

pub(in crate::ai_runtime::hooks) fn install_codewhale_hooks_in(
    home_dir: &Path,
    managed_hook_script: &Path,
) -> Result<(), String> {
    let path = codewhale_config_path_in(home_dir);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let updated = updated_codewhale_config_text(&existing, managed_hook_script);
    if existing == updated {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, updated).map_err(|error| error.to_string())
}

pub(in crate::ai_runtime::hooks) fn codewhale_hook_config_status_in(
    home_dir: &Path,
) -> AIRuntimeToolHookConfigStatus {
    let path = codewhale_config_path_in(home_dir);
    let text = fs::read_to_string(&path).unwrap_or_default();
    let missing = CODEWHALE_HOOKS
        .iter()
        .filter_map(|(event, action)| {
            (!has_codewhale_managed_hook(&text, event, action)).then(|| format!("{event}:{action}"))
        })
        .collect::<Vec<_>>();
    AIRuntimeToolHookConfigStatus {
        configured: missing.is_empty(),
        config_path: path.display().to_string(),
        missing,
    }
}

fn updated_codewhale_config_text(existing_text: &str, managed_hook_script: &Path) -> String {
    let mut lines = existing_text
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    while lines
        .last()
        .map(|line| line.trim().is_empty())
        .unwrap_or(false)
    {
        lines.pop();
    }

    lines = remove_managed_codewhale_hook_blocks(lines);
    ensure_hooks_enabled(&mut lines);

    if !lines.is_empty() && !lines.last().unwrap_or(&String::new()).trim().is_empty() {
        lines.push(String::new());
    }
    let owner = app_slug();
    for (event, action) in CODEWHALE_HOOKS {
        lines.push("[[hooks.hooks]]".to_string());
        lines.push(format!(
            "name = {}",
            toml_string(&format!("codux-{action}"))
        ));
        lines.push(format!("event = {}", toml_string(event)));
        lines.push(format!(
            "command = {}",
            toml_string(&hook_command(
                managed_hook_script,
                action,
                owner,
                "codewhale"
            ))
        ));
        lines.push("timeout_secs = 5".to_string());
        lines.push("continue_on_error = true".to_string());
        lines.push("background = true".to_string());
        lines.push(String::new());
    }

    while lines
        .last()
        .map(|line| line.trim().is_empty())
        .unwrap_or(false)
    {
        lines.pop();
    }
    format!("{}\n", lines.join("\n"))
}

fn ensure_hooks_enabled(lines: &mut Vec<String>) {
    let Some(index) = lines.iter().position(|line| line.trim() == "[hooks]") else {
        if !lines.is_empty() && !lines.last().unwrap_or(&String::new()).trim().is_empty() {
            lines.push(String::new());
        }
        lines.push("[hooks]".to_string());
        lines.push("enabled = true".to_string());
        return;
    };

    let end = section_end(lines, index);
    let mut enabled_index = None;
    for item in (index + 1)..end {
        if lines[item]
            .trim()
            .split_once('=')
            .map(|(key, _)| key.trim() == "enabled")
            .unwrap_or(false)
        {
            enabled_index = Some(item);
            break;
        }
    }
    if let Some(enabled_index) = enabled_index {
        lines[enabled_index] = "enabled = true".to_string();
    } else {
        lines.insert(end, "enabled = true".to_string());
    }
}

fn remove_managed_codewhale_hook_blocks(lines: Vec<String>) -> Vec<String> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() == "[[hooks.hooks]]" {
            let end = array_table_end(&lines, index);
            if block_is_managed(&lines[index..end]) {
                index = end;
                continue;
            }
        }
        output.push(lines[index].clone());
        index += 1;
    }
    output
}

fn block_is_managed(block: &[String]) -> bool {
    let owner = app_slug();
    block.iter().any(|line| {
        let line = line.trim();
        (line.starts_with("name") && line.contains(MANAGED_NAME_PREFIX))
            || (line.starts_with("command")
                && line.contains("dmux-ai-state")
                && line.contains(&owner)
                && line.contains("codewhale"))
    })
}

fn has_codewhale_managed_hook(text: &str, event: &str, action: &str) -> bool {
    let owner = app_slug();
    let lines = text
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != "[[hooks.hooks]]" {
            index += 1;
            continue;
        }
        let end = array_table_end(&lines, index);
        let block = &lines[index..end];
        let has_event = block
            .iter()
            .any(|line| line.trim().starts_with("event") && line.contains(event));
        let has_action = block.iter().any(|line| {
            let line = line.trim();
            line.starts_with("command")
                && line.contains("dmux-ai-state")
                && line.contains(action)
                && line.contains(&owner)
                && line.contains("codewhale")
        });
        if has_event && has_action {
            return true;
        }
        index = end;
    }
    false
}

fn section_end(lines: &[String], start: usize) -> usize {
    let mut index = start + 1;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            break;
        }
        index += 1;
    }
    index
}

fn array_table_end(lines: &[String], start: usize) -> usize {
    let mut index = start + 1;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            break;
        }
        index += 1;
    }
    index
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn codewhale_config_preserves_user_hooks_and_replaces_managed_hooks() {
        let existing = r#"
[model]
name = "deepseek"

[hooks]
enabled = false

[[hooks.hooks]]
name = "custom"
event = "message_submit"
command = "echo custom"

[[hooks.hooks]]
name = "codux-codewhale-message-submit"
event = "message_submit"
command = "'/old/dmux-ai-state.sh' 'codewhale-message-submit' 'codux' 'codewhale'"
"#;
        let updated = updated_codewhale_config_text(existing, Path::new("/tmp/dmux-ai-state.sh"));

        assert!(updated.contains("[model]\nname = \"deepseek\""));
        assert!(updated.contains("enabled = true"));
        assert!(updated.contains("command = \"echo custom\""));
        assert!(!updated.contains("/old/dmux-ai-state.sh"));
        assert!(updated.contains("event = \"session_start\""));
        assert!(updated.contains("event = \"message_submit\""));
        assert!(updated.contains("event = \"session_end\""));
        assert!(has_codewhale_managed_hook(
            &updated,
            "message_submit",
            "codewhale-message-submit"
        ));
    }

    #[test]
    fn codewhale_config_does_not_remove_sections_after_managed_hook() {
        let existing = r#"
[[hooks.hooks]]
name = "codux-codewhale-message-submit"
event = "message_submit"
command = "'/old/dmux-ai-state.sh' 'codewhale-message-submit' 'codux' 'codewhale'"

[provider]
name = "deepseek"
"#;
        let updated = updated_codewhale_config_text(existing, Path::new("/tmp/dmux-ai-state.sh"));

        assert!(updated.contains("[provider]\nname = \"deepseek\""));
        assert!(!updated.contains("/old/dmux-ai-state.sh"));
        assert!(has_codewhale_managed_hook(
            &updated,
            "message_submit",
            "codewhale-message-submit"
        ));
    }

    #[test]
    fn install_codewhale_hooks_reports_real_status() {
        let home = std::env::temp_dir().join(format!("codux-codewhale-{}", Uuid::new_v4()));
        fs::create_dir_all(&home).unwrap();

        let before = codewhale_hook_config_status_in(&home);
        assert!(!before.configured);

        install_codewhale_hooks_in(&home, Path::new("/tmp/dmux-ai-state.sh")).unwrap();
        let after = codewhale_hook_config_status_in(&home);
        assert!(after.configured);
        assert!(after.missing.is_empty());

        fs::remove_dir_all(home).unwrap();
    }
}
