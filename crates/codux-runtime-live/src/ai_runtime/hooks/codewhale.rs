use crate::{
    ai_runtime::{
        bridge::AIRuntimeToolHookConfigStatus, tool_driver::AIRuntimeLifecycleHookDefinition,
    },
    runtime_paths::app_slug,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MANAGED_NAME_PREFIX: &str = "codux-codewhale-";

pub(in crate::ai_runtime::hooks) fn codewhale_config_path_in(home_dir: &Path) -> PathBuf {
    home_dir.join(".codewhale").join("config.toml")
}

/// Strip codux-managed codewhale hook blocks, leaving the user's config intact.
/// Never creates the file if absent and skips the write when nothing changed.
pub(in crate::ai_runtime::hooks) fn uninstall_codewhale_hooks_in(
    home_dir: &Path,
) -> Result<(), String> {
    let path = codewhale_config_path_in(home_dir);
    let Ok(existing) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let mut lines = existing
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
    let cleaned = remove_managed_codewhale_hook_blocks(lines);
    let updated = if cleaned.is_empty() {
        String::new()
    } else {
        format!("{}\n", cleaned.join("\n"))
    };
    if existing == updated {
        return Ok(());
    }
    fs::write(path, updated).map_err(|error| error.to_string())
}

pub(in crate::ai_runtime::hooks) fn codewhale_hook_config_status_in(
    config_path: &Path,
    hooks: &[AIRuntimeLifecycleHookDefinition],
) -> AIRuntimeToolHookConfigStatus {
    let text = fs::read_to_string(config_path).unwrap_or_default();
    let missing = hooks
        .iter()
        .filter(|hook| !has_codewhale_managed_hook(&text, hook.event_key, hook.action))
        .map(|hook| format!("{}:{}", hook.event_key, hook.action))
        .collect::<Vec<_>>();
    AIRuntimeToolHookConfigStatus {
        configured: missing.is_empty(),
        config_path: config_path.display().to_string(),
        missing,
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
                && line.contains(owner)
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
                && line.contains(owner)
                && line.contains("codewhale")
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn uninstall_codewhale_strips_managed_hooks_and_keeps_user_config() {
        let home = std::env::temp_dir().join(format!("codux-codewhale-{}", Uuid::new_v4()));
        let path = codewhale_config_path_in(&home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"
[model]
name = "deepseek"

[hooks]
enabled = true

[[hooks.hooks]]
name = "custom"
event = "message_submit"
command = "echo custom"

[[hooks.hooks]]
name = "codux-codewhale-message-submit"
event = "message_submit"
command = "'/old/dmux-ai-state.sh' 'codewhale-message-submit' 'codux' 'codewhale'"

[provider]
name = "deepseek"
"#,
        )
        .unwrap();

        uninstall_codewhale_hooks_in(&home).unwrap();

        let updated = fs::read_to_string(&path).unwrap();
        // The user's model/provider sections and their own hook survive; only the
        // codux-managed block is removed.
        assert!(updated.contains("[model]\nname = \"deepseek\""));
        assert!(updated.contains("command = \"echo custom\""));
        assert!(updated.contains("[provider]\nname = \"deepseek\""));
        assert!(!updated.contains("/old/dmux-ai-state.sh"));
        assert!(!has_codewhale_managed_hook(
            &updated,
            "message_submit",
            "codewhale-message-submit"
        ));
        fs::remove_dir_all(home).unwrap();
    }
}
