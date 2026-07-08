use super::*;
use crate::ai_runtime::assets::{stage_runtime_asset, stage_runtime_dir};
use crate::ai_runtime::tool_driver::{
    self, AIRuntimeLifecycleHookFormat, ai_runtime_tool_drivers,
    ai_runtime_tool_launch_driver_config,
};
use std::fs;

impl AIRuntimeBridge {
    pub fn stage_assets(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.wrapper_bin_dir.parent().unwrap_or(&self.root_dir))
            .map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.wrapper_bin_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.zsh_hook_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.temp_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.runtime_event_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.binding_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.claude_session_map_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.opencode_session_map_dir()).map_err(|error| error.to_string())?;

        stage_runtime_asset(
            "scripts/shell-hooks/dmux-ai-hook.zsh",
            &self.zsh_hook_script(),
            false,
        )?;
        stage_runtime_asset(
            "scripts/shell-hooks/dmux-ai-hook.ps1",
            &self.powershell_hook_script(),
            false,
        )?;
        for file_name in [".zshenv", ".zprofile", ".zshrc", ".zlogin"] {
            stage_runtime_asset(
                &format!("scripts/shell-hooks/zsh/{file_name}"),
                &self.zsh_hook_dir().join(file_name),
                false,
            )?;
        }
        stage_runtime_asset(
            "scripts/wrappers/dmux-ai-state.sh",
            &self.managed_hook_script,
            true,
        )?;
        let wrapper_dir = self.root_dir.join("scripts").join("wrappers");
        #[cfg(not(windows))]
        stage_runtime_asset(
            "scripts/wrappers/dmux-ai-state.ps1",
            &wrapper_dir.join("dmux-ai-state.ps1"),
            false,
        )?;
        #[cfg(windows)]
        {
            let _ = fs::remove_file(wrapper_dir.join("dmux-ai-state.cmd"));
            stage_runtime_asset(
                "scripts/wrappers/dmux-ai-state.ps1",
                &wrapper_dir.join("dmux-ai-state.ps1"),
                false,
            )?;
        }
        stage_runtime_asset(
            "scripts/wrappers/tool-wrapper.sh",
            &wrapper_dir.join("tool-wrapper.sh"),
            true,
        )?;
        self.stage_wrapper_helper(&wrapper_dir)?;
        self.stage_tool_launch_driver_config(&wrapper_dir)?;
        self.stage_tool_lifecycle_configs(&wrapper_dir)?;
        #[cfg(not(windows))]
        stage_runtime_asset(
            "scripts/wrappers/codux-ssh-expect.exp",
            &wrapper_dir.join("codux-ssh-expect.exp"),
            true,
        )?;
        #[cfg(windows)]
        {
            let _ = fs::remove_file(wrapper_dir.join("tool-wrapper.cmd"));
            stage_runtime_asset(
                "scripts/wrappers/tool-wrapper.ps1",
                &wrapper_dir.join("tool-wrapper.ps1"),
                false,
            )?;
            stage_runtime_asset(
                "scripts/wrappers/codux-ssh.ps1",
                &wrapper_dir.join("codux-ssh.ps1"),
                false,
            )?;
            stage_runtime_asset(
                "scripts/wrappers/codux-db.ps1",
                &wrapper_dir.join("codux-db.ps1"),
                false,
            )?;
        }
        stage_runtime_dir(
            "scripts/wrappers/opencode-config",
            &wrapper_dir.join("opencode-config"),
        )?;

        let mut bin_names = ai_runtime_tool_drivers()
            .iter()
            .flat_map(|driver| driver.wrapper_bins.iter().copied())
            .collect::<Vec<_>>();
        bin_names.push("codux-ssh");
        bin_names.push("codux-db");
        for stale_bin_name in [
            "kiro",
            "codewhale-tui",
            "deepseek",
            "deepseek-tui",
            "gemini",
        ] {
            let _ = fs::remove_file(self.wrapper_bin_dir.join(stale_bin_name));
            let _ = fs::remove_file(self.wrapper_bin_dir.join(format!("{stale_bin_name}.ps1")));
            let _ = fs::remove_file(self.wrapper_bin_dir.join(format!("{stale_bin_name}.cmd")));
        }
        for bin_name in bin_names {
            #[cfg(not(windows))]
            stage_runtime_asset(
                &format!("scripts/wrappers/bin/{bin_name}"),
                &self.wrapper_bin_dir.join(bin_name),
                true,
            )?;
            #[cfg(windows)]
            {
                let _ = fs::remove_file(self.wrapper_bin_dir.join(bin_name));
                let _ = fs::remove_file(self.wrapper_bin_dir.join(format!("{bin_name}.cmd")));
                stage_runtime_asset(
                    &format!("scripts/wrappers/bin/{bin_name}.ps1"),
                    &self.wrapper_bin_dir.join(format!("{bin_name}.ps1")),
                    false,
                )?;
            }
        }

        Ok(())
    }

    fn stage_tool_launch_driver_config(&self, wrapper_dir: &Path) -> Result<(), String> {
        let path = wrapper_dir.join("tool-drivers.json");
        let bytes = serde_json::to_vec_pretty(&ai_runtime_tool_launch_driver_config())
            .map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())
    }

    #[cfg(not(windows))]
    fn stage_wrapper_helper(&self, wrapper_dir: &Path) -> Result<(), String> {
        let helper_path = wrapper_dir.join("codux-wrapper-helper");
        #[cfg(test)]
        {
            write_if_changed(&helper_path, test_wrapper_helper_script().as_bytes())?;
            set_executable(&helper_path);
        }
        #[cfg(not(test))]
        {
            let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
            if current_exe_can_act_as_wrapper_helper(&current_exe) {
                write_if_changed_from_file(&helper_path, &current_exe)?;
                set_executable(&helper_path);
            } else if !helper_path.exists() {
                runtime_log_line(
                    "runtime-startup",
                    &format!(
                        "skip wrapper helper staging: current_exe={} is not the desktop app",
                        current_exe.display()
                    ),
                );
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    fn stage_wrapper_helper(&self, wrapper_dir: &Path) -> Result<(), String> {
        let helper_path = wrapper_dir.join("codux-wrapper-helper.exe");
        // Old versions staged the GUI desktop exe under the extensionless
        // name; the PS wrappers must never pick that up again.
        let _ = fs::remove_file(wrapper_dir.join("codux-wrapper-helper"));
        #[cfg(test)]
        {
            write_if_changed(&helper_path, b"test helper")?;
        }
        #[cfg(not(test))]
        {
            if let Some(source_helper) =
                packaged_wrapper_helper_path().or_else(sibling_wrapper_helper_path)
            {
                write_if_changed_from_file(&helper_path, &source_helper)?;
                return Ok(());
            }
            let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
            if windows_console_exe_can_act_as_wrapper_helper(&current_exe) {
                write_if_changed_from_file(&helper_path, &current_exe)?;
                return Ok(());
            }
            if helper_path.exists() {
                return Ok(());
            }
            runtime_log_line(
                "runtime-startup",
                &format!(
                    "skip wrapper helper staging: packaged helper is missing current_exe={}",
                    current_exe.display()
                ),
            );
        }
        Ok(())
    }

    fn stage_tool_lifecycle_configs(&self, wrapper_dir: &Path) -> Result<(), String> {
        for driver in ai_runtime_tool_drivers() {
            let Some(config) = driver.lifecycle_config else {
                continue;
            };
            if driver.lifecycle_hook_format != AIRuntimeLifecycleHookFormat::CodeWhaleToml {
                continue;
            }
            let config_path = wrapper_dir.join(config.relative_path);
            #[cfg(windows)]
            let helper_command = codewhale_lifecycle_helper_command(
                &wrapper_dir.join("dmux-ai-state.ps1"),
                "${action}",
                "${tool}",
            );
            #[cfg(not(windows))]
            let helper_command = codewhale_lifecycle_helper_command(
                &wrapper_dir.join("dmux-ai-state.sh"),
                "${action}",
                "${tool}",
            );
            let content = codewhale_lifecycle_config_toml(
                driver.id,
                driver.lifecycle_hooks,
                &helper_command,
            )?;
            write_if_changed(&config_path, content.as_bytes())?;
            let shell_env_path = wrapper_dir
                .join("managed-env")
                .join(format!("{}.env", driver.id));
            let shell_env = format!(
                "export {}={}\n",
                config.env_var,
                shell_quote(&config_path.display().to_string())
            );
            write_if_changed(&shell_env_path, shell_env.as_bytes())?;
            let ps1_env_path = wrapper_dir
                .join("managed-env")
                .join(format!("{}.ps1", driver.id));
            let ps1_env = format!(
                "$env:{} = {}\n",
                config.env_var,
                powershell_single_quote(&config_path.display().to_string())
            );
            write_if_changed(&ps1_env_path, ps1_env.as_bytes())?;
        }
        Ok(())
    }
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if matches!(fs::read(path), Ok(existing) if existing == bytes) {
        return Ok(());
    }
    let tmp = path.with_extension(format!(
        "{}tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{extension}."))
            .unwrap_or_default()
    ));
    fs::write(&tmp, bytes).map_err(|error| error.to_string())?;
    fs::rename(&tmp, path).map_err(|error| error.to_string())
}

#[cfg(all(not(test), unix))]
fn write_if_changed_from_file(destination: &Path, source: &Path) -> Result<(), String> {
    use std::os::unix::fs::symlink;

    if matches!(fs::read_link(destination), Ok(existing) if existing == source) {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let tmp = destination.with_extension(format!(
        "{}tmp",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{extension}."))
            .unwrap_or_default()
    ));
    let _ = fs::remove_file(&tmp);
    symlink(source, &tmp)
        .or_else(|_| fs::copy(source, &tmp).map(|_| ()))
        .map_err(|error| error.to_string())?;
    fs::rename(&tmp, destination).map_err(|error| error.to_string())
}

#[cfg(all(not(test), windows))]
fn write_if_changed_from_file(destination: &Path, source: &Path) -> Result<(), String> {
    let source_bytes = fs::read(source).map_err(|error| error.to_string())?;
    if matches!(fs::read(destination), Ok(existing) if existing == source_bytes) {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(destination, source_bytes).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return;
    }
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
}

#[cfg(any(test, not(windows)))]
pub(super) fn current_exe_can_act_as_wrapper_helper(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.strip_suffix(".exe").unwrap_or(name))
        .is_some_and(|name| matches!(name, "codux" | "Codux" | "Codux Dev" | "codux-agent"))
}

// Desktop binaries are windows-subsystem (no console stdout); only the
// console-subsystem agent can stand in for the packaged helper.
#[cfg(all(not(test), windows))]
fn windows_console_exe_can_act_as_wrapper_helper(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("codux-agent"))
}

#[cfg(all(not(test), windows))]
fn packaged_wrapper_helper_path() -> Option<PathBuf> {
    let helper_path = std::env::current_exe()
        .ok()?
        .parent()?
        .join("runtime-root")
        .join("scripts")
        .join("wrappers")
        .join("codux-wrapper-helper.exe");
    helper_path.is_file().then_some(helper_path)
}

#[cfg(all(not(test), windows))]
fn sibling_wrapper_helper_path() -> Option<PathBuf> {
    let helper_path = std::env::current_exe()
        .ok()?
        .parent()?
        .join("codux-wrapper-helper.exe");
    helper_path.is_file().then_some(helper_path)
}

#[cfg(all(test, not(windows)))]
fn test_wrapper_helper_script() -> &'static str {
    r#"#!/bin/sh
set -eu
cmd="${2:-}"
case "$cmd" in
  tool-memory-injection)
    case "${TOOL_NAME:-}" in
      codex) printf '%s\n' codexDeveloperInstructions ;;
      claude|claude-code|reclaude) printf '%s\n' claudeAppendSystemPrompt ;;
      kimi|kimi-code) printf '%s\n' kimiAgentFile ;;
      opencode|mimo) printf '%s\n' opencodeSystemTransform ;;
    esac
    ;;
  json-string-key)
    case "${CONFIG_KEY:-}" in
      codex) sed -n 's/.*"codex"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1 ;;
      codexModel) sed -n 's/.*"codexModel"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1 ;;
      claudeCode) sed -n 's/.*"claudeCode"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1 ;;
      claudeCodeModel) sed -n 's/.*"claudeCodeModel"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1 ;;
      kimi) sed -n 's/.*"kimi"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1 ;;
      kimiModel) sed -n 's/.*"kimiModel"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1 ;;
      kiro) sed -n 's/.*"kiro"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1 ;;
      kiroModel) sed -n 's/.*"kiroModel"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1 ;;
      codewhale) sed -n 's/.*"codewhale"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1 ;;
      codewhaleModel) sed -n 's/.*"codewhaleModel"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1 ;;
    esac
    ;;
  codex-effort)
    sed -n 's/.*"codexEffort"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_PATH}" | head -n 1
    ;;
  toml-string)
    printf '"%s"\n' "$(printf '%s' "${VALUE:-}" | sed 's/\\/\\\\/g; s/"/\\"/g')"
    ;;
  hook-session-id)
    printf '%s' "${HOOK_PAYLOAD:-}" | sed -n 's/.*"session_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p; s/.*"sessionId"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1
    ;;
  hook-first-field|hook-field)
    first="${HOOK_FIELD_NAME:-${HOOK_FIELD_NAMES%% *}}"
    [ -n "$first" ] || exit 0
    printf '%s' "${HOOK_PAYLOAD:-}" | sed -n "s/.*\"$first\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" | head -n 1
    ;;
  hook-number-field)
    first="${HOOK_FIELD_NAMES%% *}"
    [ -n "$first" ] || exit 0
    printf '%s' "${HOOK_PAYLOAD:-}" | sed -n "s/.*\"$first\"[[:space:]]*:[[:space:]]*\\([0-9][0-9]*\\).*/\\1/p" | head -n 1
    ;;
  hook-notification-type|claude-memory-context|opencode-session-state|ssh-list-profiles|ssh-profile-shell)
    ;;
esac
"#
}

fn codewhale_lifecycle_config_toml(
    tool: &str,
    hooks: &[tool_driver::AIRuntimeLifecycleHookDefinition],
    helper_command_template: &str,
) -> Result<String, String> {
    let mut content = String::new();
    content.push_str("[hooks]\n");
    content.push_str("enabled = true\n\n");
    for hook in hooks {
        let timeout_secs = hook.timeout_secs.max(1);
        let background = if hook.background { "true" } else { "false" };
        let command = helper_command_template
            .replace("${action}", hook.action)
            .replace("${tool}", tool);
        content.push_str("[[hooks.hooks]]\n");
        content.push_str(&format!(
            "name = {}\n",
            toml_string(&format!("codux-{tool}-{}", hook.action))?
        ));
        content.push_str(&format!("event = {}\n", toml_string(hook.event_key)?));
        content.push_str(&format!("command = {}\n", toml_string(&command)?));
        content.push_str(&format!("timeout_secs = {timeout_secs}\n"));
        content.push_str(&format!("background = {background}\n"));
        content.push_str("continue_on_error = true\n\n");
    }
    Ok(content)
}

#[cfg(not(windows))]
fn codewhale_lifecycle_helper_command(helper_path: &Path, action: &str, tool: &str) -> String {
    [
        helper_path.display().to_string(),
        action.into(),
        tool.into(),
    ]
    .iter()
    .map(|part| shell_quote(part))
    .collect::<Vec<_>>()
    .join(" ")
}

#[cfg(windows)]
fn codewhale_lifecycle_helper_command(helper_path: &Path, action: &str, tool: &str) -> String {
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File {} {} {}",
        windows_cmd_quote(&helper_path.display().to_string()),
        windows_cmd_quote(action),
        windows_cmd_quote(tool)
    )
}

fn toml_string(value: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn powershell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(windows)]
fn windows_cmd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}
