use super::{
    assets::{stage_runtime_asset, stage_runtime_dir},
    hooks::{hook_config_status_in, install_managed_hook_configs_in},
    event_file::clear_runtime_event_dir,
    log::runtime_log_line,
    paths::runtime_root_dir,
    registry::{AIRuntimeRegistry, AIRuntimeTerminalState},
    supervisor::{AIRuntimeSupervisor, AIRuntimeSupervisorEvent},
};
use crate::runtime_paths::{
    claude_session_map_dir_in, home_dir, opencode_session_map_dir_in, runtime_event_dir_in,
    runtime_temp_dir,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeBridgeSnapshot {
    pub runtime_event_dir: String,
    pub wrapper_bin_path: String,
    pub managed_hook_script_path: String,
    pub hook_config: AIRuntimeHookConfigStatus,
    pub terminals: Vec<AIRuntimeTerminalState>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeHookConfigStatus {
    pub codex: AIRuntimeToolHookConfigStatus,
    pub claude: AIRuntimeToolHookConfigStatus,
    pub gemini: AIRuntimeToolHookConfigStatus,
    pub opencode: AIRuntimeToolHookConfigStatus,
    pub kiro: AIRuntimeToolHookConfigStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeToolHookConfigStatus {
    pub configured: bool,
    pub config_path: String,
    pub missing: Vec<String>,
}

pub struct AIRuntimeBridge {
    root_dir: PathBuf,
    wrapper_bin_dir: PathBuf,
    managed_hook_script: PathBuf,
    runtime_event_dir: PathBuf,
    temp_dir: PathBuf,
    home_dir: PathBuf,
    registry: Arc<AIRuntimeRegistry>,
    supervisor: Arc<AIRuntimeSupervisor>,
    started: AtomicBool,
    start_lock: Mutex<()>,
    hook_config_lock: Mutex<()>,
}

impl AIRuntimeBridge {
    pub fn new() -> Self {
        Self::with_paths(runtime_root_dir(), runtime_temp_dir(), home_dir())
    }

    pub(crate) fn with_paths(root_dir: PathBuf, temp_dir: PathBuf, home_dir: PathBuf) -> Self {
        let wrapper_bin_dir = root_dir.join("scripts").join("wrappers").join("bin");
        let managed_hook_script = root_dir
            .join("scripts")
            .join("wrappers")
            .join("dmux-ai-state.sh");
        Self {
            root_dir,
            wrapper_bin_dir,
            managed_hook_script,
            runtime_event_dir: runtime_event_dir_in(&temp_dir),
            temp_dir,
            home_dir,
            registry: AIRuntimeRegistry::shared(),
            supervisor: Arc::new(AIRuntimeSupervisor::new()),
            started: AtomicBool::new(false),
            start_lock: Mutex::new(()),
            hook_config_lock: Mutex::new(()),
        }
    }

    pub fn prepare(&self) -> Result<(), String> {
        self.stage_assets()?;
        self.ensure_hook_configs_installed("prepare")
    }

    fn ensure_hook_configs_installed(&self, phase: &str) -> Result<(), String> {
        let _guard = self
            .hook_config_lock
            .lock()
            .map_err(|_| "AI runtime hook config lock poisoned.".to_string())?;
        install_managed_hook_configs_in(&self.home_dir, &self.managed_hook_script)?;
        self.log_hook_config_status(phase);
        Ok(())
    }

    pub fn start_event_processing_background(&self) -> Result<(), String> {
        if self.started.load(Ordering::Acquire) {
            self.ensure_hook_configs_installed("ensure-started")?;
            return Ok(());
        }
        let _guard = self
            .start_lock
            .lock()
            .map_err(|_| "AI runtime startup lock poisoned.".to_string())?;
        if self.started.load(Ordering::Acquire) {
            self.ensure_hook_configs_installed("ensure-started")?;
            return Ok(());
        }
        self.prepare()?;
        let removed = clear_runtime_event_dir(&self.runtime_event_dir);
        runtime_log_line(
            "runtime-startup",
            &format!("cleared stale runtime event files count={removed}"),
        );
        self.supervisor
            .start(Arc::clone(&self.registry), self.runtime_event_dir.clone())?;
        self.started.store(true, Ordering::Release);
        Ok(())
    }

    pub fn ensure_started(&self) -> Result<(), String> {
        self.start_event_processing_background()
    }

    pub fn submit_runtime_frame(&self, frame: Vec<u8>) -> Result<(), String> {
        self.supervisor.submit_frame(frame)
    }

    pub fn poll_runtime_state(&self) -> Result<(), String> {
        self.supervisor.poll_once()
    }

    pub fn runtime_state_snapshot(&self) -> super::AIRuntimeStateSnapshot {
        self.supervisor.state_snapshot()
    }

    pub fn dismiss_completion(&self, project_id: &str) -> bool {
        self.supervisor.dismiss_completion(project_id)
    }

    pub fn drain_supervisor_events(&self) -> Vec<AIRuntimeSupervisorEvent> {
        self.supervisor.drain_events()
    }

    pub fn stage_assets(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.wrapper_bin_dir.parent().unwrap_or(&self.root_dir))
            .map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.wrapper_bin_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.zsh_hook_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.temp_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.runtime_event_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.claude_session_map_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.opencode_session_map_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.home_dir.join(".kiro").join("agents"))
            .map_err(|error| error.to_string())?;

        stage_runtime_asset(
            "scripts/shell-hooks/dmux-ai-hook.zsh",
            &self.zsh_hook_script(),
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
        }
        stage_runtime_dir(
            "scripts/wrappers/opencode-config",
            &wrapper_dir.join("opencode-config"),
        )?;

        for bin_name in [
            "codex",
            "claude",
            "claude-code",
            "gemini",
            "agy",
            "opencode",
            "kiro",
            "kiro-cli",
            "codux-ssh",
        ] {
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

    pub fn wrapper_bin_dir(&self) -> &Path {
        &self.wrapper_bin_dir
    }

    pub fn managed_hook_script(&self) -> &Path {
        &self.managed_hook_script
    }

    pub fn zsh_hook_dir(&self) -> PathBuf {
        self.root_dir
            .join("scripts")
            .join("shell-hooks")
            .join("zsh")
    }

    pub fn zsh_hook_script(&self) -> PathBuf {
        self.root_dir
            .join("scripts")
            .join("shell-hooks")
            .join("dmux-ai-hook.zsh")
    }

    pub fn registry(&self) -> Arc<AIRuntimeRegistry> {
        Arc::clone(&self.registry)
    }

    pub fn claude_session_map_dir(&self) -> PathBuf {
        claude_session_map_dir_in(&self.temp_dir)
    }

    pub fn opencode_session_map_dir(&self) -> PathBuf {
        opencode_session_map_dir_in(&self.temp_dir)
    }

    pub fn snapshot(&self) -> AIRuntimeBridgeSnapshot {
        AIRuntimeBridgeSnapshot {
            runtime_event_dir: self.runtime_event_dir.display().to_string(),
            wrapper_bin_path: self.wrapper_bin_dir.display().to_string(),
            managed_hook_script_path: self.managed_hook_script.display().to_string(),
            hook_config: hook_config_status_in(
                &self.home_dir,
                &self
                    .root_dir
                    .join("scripts")
                    .join("wrappers")
                    .join("opencode-config"),
            ),
            terminals: self.registry.snapshot(),
        }
    }

    fn log_hook_config_status(&self, phase: &str) {
        let status = hook_config_status_in(
            &self.home_dir,
            &self
                .root_dir
                .join("scripts")
                .join("wrappers")
                .join("opencode-config"),
        );
        super::runtime_log_line(
            "runtime-hooks",
            &format!(
                "{phase} codex={} claude={} gemini={} opencode={} kiro={} claude_missing={}",
                status.codex.configured,
                status.claude.configured,
                status.gemini.configured,
                status.opencode.configured,
                status.kiro.configured,
                status.claude.missing.join("|")
            ),
        );
    }
}

impl Default for AIRuntimeBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn bridge_stages_runtime_assets_without_installing_hooks() {
        let dir = std::env::temp_dir().join(format!("codux-ai-bridge-{}", Uuid::new_v4()));
        let bridge =
            AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));

        bridge.stage_assets().unwrap();

        assert!(bridge.managed_hook_script().is_file());
        #[cfg(not(windows))]
        assert!(bridge.wrapper_bin_dir().join("codex").is_file());
        #[cfg(windows)]
        {
            assert!(bridge.wrapper_bin_dir().join("codex.ps1").is_file());
            assert!(!bridge.wrapper_bin_dir().join("codex.cmd").exists());
        }
        assert!(
            dir.join("root")
                .join("scripts/wrappers/opencode-config/package.json")
                .is_file()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn tool_wrapper_keeps_codux_ssh_available_to_ai_cli() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let dir = std::env::temp_dir().join(format!("codux-ai-wrapper-path-{}", Uuid::new_v4()));
        let bridge =
            AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
        bridge.stage_assets().unwrap();

        let real_bin = dir.join("real-bin");
        fs::create_dir_all(&real_bin).unwrap();
        let fake_codex = real_bin.join("codex");
        fs::write(
            &fake_codex,
            "#!/bin/sh\ncommand -v codux-ssh >/dev/null || exit 42\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_codex, permissions).unwrap();

        let wrapper = bridge.wrapper_bin_dir().join("codex");
        let search_path = format!(
            "{}:/usr/bin:/bin:/usr/sbin:/sbin",
            real_bin.display()
        );
        let zsh_dot_dir = dir.join("zsh");
        fs::create_dir_all(&zsh_dot_dir).unwrap();
        fs::write(zsh_dot_dir.join(".zshenv"), "").unwrap();
        let output = Command::new(wrapper)
            .env("PATH", &search_path)
            .env("DMUX_ORIGINAL_PATH", &search_path)
            .env("ZDOTDIR", zsh_dot_dir)
            .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "wrapper should expose codux-ssh to AI CLI, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn zsh_runtime_hook_keeps_wrapper_bin_first_after_user_startup_files() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let dir = std::env::temp_dir().join(format!("codux-zsh-wrapper-path-{}", Uuid::new_v4()));
        let bridge =
            AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
        bridge.stage_assets().unwrap();

        let real_bin = dir.join("real-bin");
        fs::create_dir_all(&real_bin).unwrap();
        let fake_codex = real_bin.join("codex");
        fs::write(&fake_codex, "#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_codex, permissions).unwrap();

        let home = dir.join("home");
        fs::write(
            home.join(".zshrc"),
            format!(
                "if [[ \"${{ZDOTDIR:-}}\" != \"{}\" ]]; then exit 61; fi\nexport PATH=\"{}:$PATH\"\n",
                home.display(),
                real_bin.display()
            ),
        )
        .unwrap();
        let output = Command::new("zsh")
            .args([
                "-l",
                "-i",
                "-c",
                "command -v codex; printf 'HISTFILE=%s\\n' \"${HISTFILE:-}\"",
            ])
            .env("HOME", &home)
            .env("USER", "codux")
            .env("LOGNAME", "codux")
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display()),
            )
            .env("DMUX_WRAPPER_BIN", bridge.wrapper_bin_dir())
            .env("DMUX_USER_ZDOTDIR", &home)
            .env("ZDOTDIR", bridge.zsh_hook_dir())
            .env("DMUX_ZSH_HOOK_SCRIPT", bridge.zsh_hook_script())
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "zsh should resolve codex, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines = stdout.lines().collect::<Vec<_>>();
        assert_eq!(
            lines.first().copied(),
            Some(bridge.wrapper_bin_dir().join("codex").display().to_string().as_str())
        );
        let expected_histfile = format!("HISTFILE={}", home.join(".zsh_history").display());
        assert_eq!(
            lines.get(1).copied(),
            Some(expected_histfile.as_str())
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn zsh_runtime_hook_preserves_user_configured_histfile() {
        use std::process::Command;

        let dir = std::env::temp_dir().join(format!("codux-zsh-histfile-{}", Uuid::new_v4()));
        let bridge =
            AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
        bridge.stage_assets().unwrap();

        let home = dir.join("home");
        fs::write(
            home.join(".zshrc"),
            "export HISTFILE=\"$HOME/.custom_zsh_history\"\n",
        )
        .unwrap();
        let output = Command::new("zsh")
            .args(["-l", "-i", "-c", "printf 'HISTFILE=%s\\n' \"$HISTFILE\""])
            .env("HOME", &home)
            .env("USER", "codux")
            .env("LOGNAME", "codux")
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env("DMUX_WRAPPER_BIN", bridge.wrapper_bin_dir())
            .env("DMUX_USER_ZDOTDIR", &home)
            .env("ZDOTDIR", bridge.zsh_hook_dir())
            .env("DMUX_ZSH_HOOK_SCRIPT", bridge.zsh_hook_script())
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "zsh should preserve user HISTFILE, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            format!("HISTFILE={}", home.join(".custom_zsh_history").display())
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn codex_wrapper_applies_tool_permissions_and_memory_injection() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let dir =
            std::env::temp_dir().join(format!("codux-codex-wrapper-perms-{}", Uuid::new_v4()));
        let bridge =
            AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
        bridge.stage_assets().unwrap();

        let real_bin = dir.join("real-bin");
        fs::create_dir_all(&real_bin).unwrap();
        let fake_codex = real_bin.join("codex");
        fs::write(&fake_codex, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_codex, permissions).unwrap();

        let permissions_file = dir.join("tool-permissions.json");
        fs::write(
            &permissions_file,
            serde_json::json!({
                "codex": "fullAccess",
                "claudeCode": "default",
                "gemini": "default",
                "opencode": "default",
                "kiro": "default",
                "codexModel": "gpt-5.1",
                "codexEffort": "high"
            })
            .to_string(),
        )
        .unwrap();
        let memory_root = dir.join("memory");
        let project_root = dir.join("project");
        fs::create_dir_all(&memory_root).unwrap();
        fs::create_dir_all(&project_root).unwrap();
        fs::write(memory_root.join("AGENTS.md"), "Use project memory.").unwrap();

        let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
        let output = Command::new(bridge.wrapper_bin_dir().join("codex"))
            .env("PATH", &search_path)
            .env("DMUX_ORIGINAL_PATH", &search_path)
            .env("DMUX_SESSION_ID", "terminal-1")
            .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
            .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
            .env("DMUX_AI_MEMORY_WORKSPACE_ROOT", &memory_root)
            .env("DMUX_PROJECT_PATH", &project_root)
            .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "wrapper should execute fake codex, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let args = String::from_utf8_lossy(&output.stdout);
        assert!(args.lines().any(|arg| arg == "--enable"));
        assert!(args.lines().any(|arg| arg == "hooks"));
        assert!(
            args.lines()
                .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox")
        );
        assert!(args.lines().any(|arg| arg == "--model=gpt-5.1"));
        assert!(args.lines().any(|arg| arg == "model_reasoning_effort=\"high\""));
        assert!(args.lines().any(|arg| arg == "--add-dir"));
        assert!(args.lines().any(|arg| arg == memory_root.display().to_string()));
        assert!(args.lines().any(|arg| arg.starts_with("developer_instructions=")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bridge_prepare_installs_claude_hooks_and_preserves_settings() {
        let dir = std::env::temp_dir().join(format!("codux-ai-bridge-{}", Uuid::new_v4()));
        let home = dir.join("home");
        let claude_settings = home.join(".claude").join("settings.json");
        fs::create_dir_all(claude_settings.parent().unwrap()).unwrap();
        fs::write(
            &claude_settings,
            serde_json::json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "PROXY_MANAGED",
                    "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721"
                },
                "includeCoAuthoredBy": false,
                "skipDangerousModePermissionPrompt": true,
                "hooks": {
                    "UserPromptSubmit": [{
                        "matcher": "",
                        "hooks": [{
                            "type": "command",
                            "command": "'/old/dmux-ai-state.sh' 'prompt-submit' 'codux' 'claude'"
                        }]
                    }]
                }
            })
            .to_string(),
        )
        .unwrap();
        let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), home);

        bridge.prepare().unwrap();

        let settings: serde_json::Value =
            serde_json::from_slice(&fs::read(&claude_settings).unwrap()).unwrap();
        assert_eq!(
            settings["env"]["ANTHROPIC_AUTH_TOKEN"].as_str(),
            Some("PROXY_MANAGED")
        );
        assert_eq!(settings["includeCoAuthoredBy"].as_bool(), Some(false));
        let command = settings["hooks"]["UserPromptSubmit"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["hooks"]
            .as_array()
            .unwrap()[0]["command"]
            .as_str()
            .unwrap();
        assert!(command.contains("dmux-ai-state.sh"));
        assert!(command.contains("'prompt-submit'"));
        assert!(command.contains("'claude'"));
        assert!(!settings.to_string().contains("/old/dmux-ai-state.sh"));
        assert!(settings["hooks"]["Stop"].as_array().is_some());
        fs::remove_dir_all(dir).unwrap();
    }
}
