use serde_json::Value;
use std::path::Path;

pub(super) fn is_managed_hook(value: &Value, action: &str, tool: &str) -> bool {
    is_managed_hook_action(value, action, Some(tool))
}

pub(super) fn is_managed_hook_action(value: &Value, action: &str, tool: Option<&str>) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let Some(command) = object.get("command").and_then(|value| value.as_str()) else {
        return false;
    };
    if command.contains("dmux-ai-state.sh")
        && command.contains(&shell_quote(action))
        && tool
            .map(|tool| command.contains(&shell_quote(tool)))
            .unwrap_or(true)
    {
        return true;
    }
    #[cfg(windows)]
    {
        let is_current = command.contains("dmux-ai-state.ps1")
            && command.contains(&windows_powershell_quote(action))
            && tool
                .map(|tool| command.contains(&windows_powershell_quote(tool)))
                .unwrap_or(true);
        let is_legacy = command.contains("dmux-ai-state.cmd")
            && command.contains(&windows_cmd_quote_cross_platform(action))
            && tool
                .map(|tool| command.contains(&windows_cmd_quote_cross_platform(tool)))
                .unwrap_or(true);
        is_current || is_legacy
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub(super) fn hook_command(helper_script: &Path, action: &str, owner: &str, tool: &str) -> String {
    #[cfg(windows)]
    {
        return format!(
            "powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File {} {} {} {}",
            windows_powershell_quote(&helper_script.with_extension("ps1").display().to_string()),
            windows_powershell_quote(action),
            windows_powershell_quote(owner),
            windows_powershell_quote(tool),
        );
    }

    #[cfg(not(windows))]
    [
        shell_quote(&helper_script.display().to_string()),
        shell_quote(action),
        shell_quote(owner),
        shell_quote(tool),
    ]
    .join(" ")
}

#[cfg(windows)]
fn windows_powershell_quote(value: &str) -> String {
    windows_powershell_quote_cross_platform(value)
}

pub(super) fn windows_powershell_quote_cross_platform(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(super) fn windows_cmd_quote_cross_platform(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(super) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(windows)]
    #[test]
    fn recognizes_legacy_windows_cmd_hooks_for_cleanup() {
        let hook = json!({
            "command": "cmd /d /c call \"C:\\Codux\\dmux-ai-state.cmd\" \"codex-session-start\" \"codux\" \"codex\""
        });

        assert!(is_managed_hook(&hook, "codex-session-start", "codex"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_hook_command_uses_powershell_script() {
        use std::path::Path;

        let command = hook_command(
            Path::new("C:\\Codux\\dmux-ai-state.sh"),
            "codex-session-start",
            "codux",
            "codex",
        );

        assert!(command.contains("powershell.exe"));
        assert!(command.contains("-ExecutionPolicy Bypass"));
        assert!(command.contains("dmux-ai-state.ps1"));
        assert!(!command.contains("cmd /d /c"));
        assert!(!command.contains("dmux-ai-state.cmd"));
    }
}
