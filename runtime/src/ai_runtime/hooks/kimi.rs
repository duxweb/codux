use super::command::hook_command;
use crate::{ai_runtime::bridge::AIRuntimeToolHookConfigStatus, runtime_paths::app_slug};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MANAGED_NAME_PREFIX: &str = "codux-kimi-";

pub(in crate::ai_runtime::hooks) const KIMI_HOOKS: &[(&str, &str)] = &[
    ("SessionStart", "session-start"),
    ("UserPromptSubmit", "prompt-submit"),
    ("PreToolUse", "before-agent"),
    ("PostToolUse", "after-agent"),
    ("PermissionRequest", "permission-request"),
    ("Stop", "stop"),
    ("SubagentStop", "after-agent"),
    ("PreCompact", "pre-compact"),
    ("PostCompact", "post-compact"),
    ("SessionEnd", "session-end"),
    ("Notification", "notification"),
];

pub(in crate::ai_runtime::hooks) fn kimi_config_path_in(home_dir: &Path) -> PathBuf {
    home_dir.join(".kimi-code").join("config.toml")
}

pub(in crate::ai_runtime::hooks) fn install_kimi_hooks_in(
    home_dir: &Path,
    managed_hook_script: &Path,
) -> Result<(), String> {
    let path = kimi_config_path_in(home_dir);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let updated = updated_kimi_config_text(&existing, managed_hook_script);
    if existing == updated {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, updated).map_err(|error| error.to_string())
}

pub(in crate::ai_runtime::hooks) fn kimi_hook_config_status_in(
    home_dir: &Path,
) -> AIRuntimeToolHookConfigStatus {
    let path = kimi_config_path_in(home_dir);
    let text = fs::read_to_string(&path).unwrap_or_default();
    let missing = KIMI_HOOKS
        .iter()
        .filter_map(|(event, action)| {
            (!has_kimi_managed_hook(&text, event, action)).then(|| format!("{event}:{action}"))
        })
        .collect::<Vec<_>>();
    AIRuntimeToolHookConfigStatus {
        configured: missing.is_empty(),
        config_path: path.display().to_string(),
        missing,
    }
}

fn updated_kimi_config_text(existing_text: &str, managed_hook_script: &Path) -> String {
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

    lines = remove_managed_kimi_hook_blocks(lines);

    if !lines.is_empty() && !lines.last().unwrap_or(&String::new()).trim().is_empty() {
        lines.push(String::new());
    }
    let owner = app_slug();
    for (event, action) in KIMI_HOOKS {
        lines.push("[[hooks]]".to_string());
        lines.push(format!("event = {}", toml_string(event)));
        lines.push("matcher = \"*\"".to_string());
        lines.push(format!(
            "command = {}",
            toml_string(&hook_command(managed_hook_script, action, owner, "kimi"))
        ));
        lines.push("timeout = 5".to_string());
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

fn remove_managed_kimi_hook_blocks(lines: Vec<String>) -> Vec<String> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() == "[[hooks]]" {
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
                && line.contains("kimi"))
            || (line.starts_with("command")
                && line.contains("dmux-ai-state")
                && line.contains("codux-kimi-"))
    })
}

fn has_kimi_managed_hook(text: &str, event: &str, action: &str) -> bool {
    let owner = app_slug();
    let lines = text
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != "[[hooks]]" {
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
                && line.contains("kimi")
        });
        if has_event && has_action {
            return true;
        }
        index = end;
    }
    false
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
    fn kimi_config_preserves_user_hooks_and_replaces_managed_hooks() {
        let existing = r#"
model = "kimi-k2"

[[hooks]]
name = "custom"
event = "UserPromptSubmit"
command = "echo custom"

[[hooks]]
name = "codux-kimi-prompt-submit"
event = "UserPromptSubmit"
command = "'/old/dmux-ai-state.sh' 'prompt-submit' 'codux' 'kimi'"
"#;
        let updated = updated_kimi_config_text(existing, Path::new("/tmp/dmux-ai-state.sh"));

        assert!(updated.contains("model = \"kimi-k2\""));
        assert!(updated.contains("command = \"echo custom\""));
        assert!(!updated.contains("/old/dmux-ai-state.sh"));
        assert!(updated.contains("matcher = \"*\""));
        assert!(updated.contains("event = \"SessionStart\""));
        assert!(updated.contains("event = \"UserPromptSubmit\""));
        assert!(updated.contains("event = \"SessionEnd\""));
        assert!(has_kimi_managed_hook(
            &updated,
            "UserPromptSubmit",
            "prompt-submit"
        ));
    }

    #[test]
    fn install_kimi_hooks_reports_real_status() {
        let home = std::env::temp_dir().join(format!("codux-kimi-{}", Uuid::new_v4()));
        fs::create_dir_all(&home).unwrap();

        let before = kimi_hook_config_status_in(&home);
        assert!(!before.configured);

        install_kimi_hooks_in(&home, Path::new("/tmp/dmux-ai-state.sh")).unwrap();
        let after = kimi_hook_config_status_in(&home);
        assert!(after.configured);
        assert!(after.missing.is_empty());

        fs::remove_dir_all(home).unwrap();
    }
}
