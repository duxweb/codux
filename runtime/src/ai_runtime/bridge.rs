use super::{
    assets::{stage_runtime_asset, stage_runtime_dir},
    event_file::clear_runtime_event_dir,
    hooks::{hook_config_status_in, install_managed_hook_configs_in},
    log::runtime_log_line,
    paths::runtime_root_dir,
    payload::AIHookEventPayload,
    registry::{AIRuntimeRegistry, AIRuntimeTerminalState},
    supervisor::{AIRuntimeSupervisor, AIRuntimeSupervisorEvent},
    tool_driver::{ai_runtime_tool_drivers, ai_runtime_tool_launch_driver_config},
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
    pub agy: AIRuntimeToolHookConfigStatus,
    pub opencode: AIRuntimeToolHookConfigStatus,
    pub kiro: AIRuntimeToolHookConfigStatus,
    pub codewhale: AIRuntimeToolHookConfigStatus,
    pub kimi: AIRuntimeToolHookConfigStatus,
}

#[derive(Debug, Clone, Default, Serialize)]
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

    pub fn submit_hook_event(&self, payload: AIHookEventPayload) -> Result<(), String> {
        let frame = serde_json::to_vec(&serde_json::json!({
            "kind": "ai-hook",
            "payload": payload,
        }))
        .map_err(|error| error.to_string())?;
        self.submit_runtime_frame(frame)
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
        self.stage_tool_launch_driver_config(&wrapper_dir)?;
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

        let mut bin_names = ai_runtime_tool_drivers()
            .iter()
            .flat_map(|driver| driver.wrapper_bins.iter().copied())
            .collect::<Vec<_>>();
        bin_names.push("codux-ssh");
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
                "{phase} codex={} claude={} gemini={} agy={} opencode={} kiro={} codewhale={} kimi={} claude_missing={}",
                status.codex.configured,
                status.claude.configured,
                status.gemini.configured,
                status.agy.configured,
                status.opencode.configured,
                status.kiro.configured,
                status.codewhale.configured,
                status.kimi.configured,
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
        {
            assert!(bridge.wrapper_bin_dir().join("codex").is_file());
            assert!(bridge.wrapper_bin_dir().join("codewhale").is_file());
            assert!(bridge.wrapper_bin_dir().join("codewhale-tui").is_file());
            assert!(bridge.wrapper_bin_dir().join("deepseek").is_file());
            assert!(bridge.wrapper_bin_dir().join("deepseek-tui").is_file());
            assert!(bridge.wrapper_bin_dir().join("kimi").is_file());
            assert!(bridge.wrapper_bin_dir().join("kimi-code").is_file());
        }
        #[cfg(windows)]
        {
            assert!(bridge.wrapper_bin_dir().join("codex.ps1").is_file());
            assert!(bridge.wrapper_bin_dir().join("codewhale.ps1").is_file());
            assert!(bridge.wrapper_bin_dir().join("codewhale-tui.ps1").is_file());
            assert!(bridge.wrapper_bin_dir().join("deepseek.ps1").is_file());
            assert!(bridge.wrapper_bin_dir().join("deepseek-tui.ps1").is_file());
            assert!(bridge.wrapper_bin_dir().join("kimi.ps1").is_file());
            assert!(bridge.wrapper_bin_dir().join("kimi-code.ps1").is_file());
            assert!(!bridge.wrapper_bin_dir().join("codex.cmd").exists());
        }
        assert!(
            dir.join("root")
                .join("scripts/wrappers/opencode-config/package.json")
                .is_file()
        );
        let launch_config: serde_json::Value = serde_json::from_slice(
            &fs::read(dir.join("root").join("scripts/wrappers/tool-drivers.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(launch_config["tools"][0]["id"].as_str(), Some("codex"));
        assert!(
            launch_config["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["id"] == "claude"
                    && tool["memoryInjection"] == "claudeAppendSystemPrompt")
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
        let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
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
            Some(
                bridge
                    .wrapper_bin_dir()
                    .join("codex")
                    .display()
                    .to_string()
                    .as_str()
            )
        );
        let expected_histfile = format!("HISTFILE={}", home.join(".zsh_history").display());
        assert_eq!(lines.get(1).copied(), Some(expected_histfile.as_str()));
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
        assert!(
            args.lines()
                .any(|arg| arg == "model_reasoning_effort=\"high\"")
        );
        assert!(args.lines().any(|arg| arg == "--add-dir"));
        assert!(
            args.lines()
                .any(|arg| arg == memory_root.display().to_string())
        );
        assert!(
            args.lines()
                .any(|arg| arg.starts_with("developer_instructions="))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn claude_wrapper_applies_driver_memory_prompt_injection() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let dir =
            std::env::temp_dir().join(format!("codux-claude-wrapper-memory-{}", Uuid::new_v4()));
        let bridge =
            AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
        bridge.stage_assets().unwrap();

        let real_bin = dir.join("real-bin");
        fs::create_dir_all(&real_bin).unwrap();
        let fake_claude = real_bin.join("claude");
        fs::write(&fake_claude, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        let mut permissions = fs::metadata(&fake_claude).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_claude, permissions).unwrap();

        let prompt_file = dir.join("memory-prompt.txt");
        fs::write(&prompt_file, "Use Claude memory.").unwrap();

        let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
        let output = Command::new(bridge.wrapper_bin_dir().join("claude"))
            .env("PATH", &search_path)
            .env("DMUX_ORIGINAL_PATH", &search_path)
            .env("DMUX_SESSION_ID", "terminal-1")
            .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
            .env("DMUX_AI_MEMORY_PROMPT_FILE", &prompt_file)
            .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "wrapper should execute fake claude, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let args = String::from_utf8_lossy(&output.stdout);
        assert!(args.lines().any(|arg| arg == "--append-system-prompt"));
        assert!(args.lines().any(|arg| arg == "Use Claude memory."));
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

    #[test]
    fn bridge_prepare_installs_codex_config_in_bridge_home() {
        let dir = std::env::temp_dir().join(format!("codux-codex-config-{}", Uuid::new_v4()));
        let home = dir.join("home");
        let config = home.join(".codex").join("config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "[profiles.work]\nmodel = \"gpt-5\"\n").unwrap();
        let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), home);

        bridge.prepare().unwrap();

        let text = fs::read_to_string(&config).unwrap();
        assert!(text.contains("suppress_unstable_features_warning = true"));
        assert!(text.contains("[features]\nhooks = true"));
        assert!(text.contains("[profiles.work]\nmodel = \"gpt-5\""));
        assert!(
            text.contains(&format!(
                "[hooks.state.\"{}",
                dir.join("home").join(".codex").join("hooks.json").display()
            )),
            "{text}"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bridge_prepare_installs_codewhale_hooks() {
        let dir = std::env::temp_dir().join(format!("codux-codewhale-hooks-{}", Uuid::new_v4()));
        let home = dir.join("home");
        let config = home.join(".codewhale").join("config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(
            &config,
            r#"
[model]
name = "deepseek"

[hooks]
enabled = false

[[hooks.hooks]]
name = "custom"
event = "message_submit"
command = "echo custom"
"#,
        )
        .unwrap();
        let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), home);

        bridge.prepare().unwrap();

        let text = fs::read_to_string(&config).unwrap();
        assert!(text.contains("[model]\nname = \"deepseek\""));
        assert!(text.contains("enabled = true"));
        assert!(text.contains("command = \"echo custom\""));
        assert!(text.contains("event = \"session_start\""));
        assert!(text.contains("event = \"message_submit\""));
        assert!(text.contains("codewhale-message-submit"));
        assert!(text.contains("dmux-ai-state.sh"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn codewhale_wrapper_applies_configured_model_and_resume_session() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let dir = std::env::temp_dir().join(format!("codux-codewhale-wrapper-{}", Uuid::new_v4()));
        let bridge =
            AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
        bridge.stage_assets().unwrap();

        let real_bin = dir.join("real-bin");
        fs::create_dir_all(&real_bin).unwrap();
        let fake_codewhale = real_bin.join("codewhale");
        fs::write(
            &fake_codewhale,
            "#!/bin/sh\nprintf 'external=%s\\n' \"$DMUX_EXTERNAL_SESSION_ID\"\nprintf 'model=%s\\n' \"$DMUX_ACTIVE_AI_MODEL\"\nprintf '%s\\n' \"$@\"\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&fake_codewhale).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_codewhale, permissions).unwrap();

        let permissions_file = dir.join("tool-permissions.json");
        fs::write(
            &permissions_file,
            serde_json::json!({
                "codewhale": "fullAccess",
                "codewhaleModel": "deepseek-chat"
            })
            .to_string(),
        )
        .unwrap();

        let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
        let output = Command::new(bridge.wrapper_bin_dir().join("codewhale"))
            .args(["resume", "session-1"])
            .env("PATH", &search_path)
            .env("DMUX_ORIGINAL_PATH", &search_path)
            .env("DMUX_SESSION_ID", "terminal-1")
            .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
            .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
            .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "wrapper should execute fake codewhale, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let args = String::from_utf8_lossy(&output.stdout);
        assert!(args.lines().any(|arg| arg == "external=session-1"));
        assert!(args.lines().any(|arg| arg == "model=deepseek-chat"));
        assert!(args.lines().any(|arg| arg == "--yolo"));
        assert!(args.lines().any(|arg| arg == "--model"));
        assert!(args.lines().any(|arg| arg == "deepseek-chat"));
        assert!(args.lines().any(|arg| arg == "resume"));
        assert!(args.lines().any(|arg| arg == "session-1"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn kimi_wrapper_applies_configured_model_without_unknown_permission_args() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let dir = std::env::temp_dir().join(format!("codux-kimi-wrapper-{}", Uuid::new_v4()));
        let bridge =
            AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
        bridge.stage_assets().unwrap();

        let real_bin = dir.join("real-bin");
        fs::create_dir_all(&real_bin).unwrap();
        let fake_kimi = real_bin.join("kimi");
        fs::write(&fake_kimi, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        let mut permissions = fs::metadata(&fake_kimi).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_kimi, permissions).unwrap();

        let permissions_file = dir.join("tool-permissions.json");
        fs::write(
            &permissions_file,
            serde_json::json!({
                "kimi": "fullAccess",
                "kimiModel": "kimi-k2"
            })
            .to_string(),
        )
        .unwrap();

        let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
        let output = Command::new(bridge.wrapper_bin_dir().join("kimi"))
            .arg("hello")
            .env("PATH", &search_path)
            .env("DMUX_ORIGINAL_PATH", &search_path)
            .env("DMUX_SESSION_ID", "terminal-1")
            .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
            .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
            .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "wrapper should execute fake kimi, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let args = String::from_utf8_lossy(&output.stdout);
        assert!(args.lines().any(|arg| arg == "--model"), "{args}");
        assert!(args.lines().any(|arg| arg == "kimi-k2"), "{args}");
        assert!(!args.lines().any(|arg| arg == "--approval-mode"), "{args}");
        assert!(!args.lines().any(|arg| arg == "yolo"), "{args}");
        assert!(args.lines().any(|arg| arg == "hello"), "{args}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn codewhale_hook_writes_runtime_event() {
        use std::process::{Command, Stdio};

        let dir = std::env::temp_dir().join(format!("codux-codewhale-event-{}", Uuid::new_v4()));
        let bridge =
            AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
        bridge.stage_assets().unwrap();
        let events = dir.join("events");
        fs::create_dir_all(&events).unwrap();

        let mut child = Command::new(bridge.managed_hook_script())
            .args(["codewhale-message-submit", "codux", "codewhale"])
            .env("DMUX_RUNTIME_OWNER", "codux")
            .env("DMUX_SESSION_ID", "terminal-1")
            .env("DMUX_SESSION_INSTANCE_ID", "instance-1")
            .env("DMUX_PROJECT_ID", "project-1")
            .env("DMUX_PROJECT_NAME", "Project")
            .env("DMUX_PROJECT_PATH", "/tmp/project")
            .env("DMUX_SESSION_TITLE", "CodeWhale")
            .env("DMUX_RUNTIME_EVENT_DIR", &events)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        {
            use std::io::Write;
            let stdin = child.stdin.as_mut().unwrap();
            stdin
                .write_all(
                    br#"{"event":"message_submit","session_id":"cw-session-1","workspace":"/tmp/project","text":"hello"}"#,
                )
                .unwrap();
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "hook failed stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );

        let mut entries = fs::read_dir(&events)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        assert_eq!(entries.len(), 1);
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&entries[0]).unwrap()).unwrap();
        assert_eq!(value["kind"], "ai-hook");
        assert_eq!(value["payload"]["kind"], "promptSubmitted");
        assert_eq!(value["payload"]["tool"], "codewhale");
        assert_eq!(value["payload"]["aiSessionID"], "cw-session-1");
        assert_eq!(value["payload"]["metadata"]["source"], "user-input");
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn codewhale_hook_without_payload_does_not_block() {
        use std::process::{Command, Stdio};

        let dir = std::env::temp_dir().join(format!("codux-codewhale-empty-{}", Uuid::new_v4()));
        let bridge =
            AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
        bridge.stage_assets().unwrap();
        let events = dir.join("events");
        fs::create_dir_all(&events).unwrap();

        let output = Command::new(bridge.managed_hook_script())
            .args(["codewhale-message-submit", "codux", "codewhale"])
            .env("DMUX_RUNTIME_OWNER", "codux")
            .env("DMUX_SESSION_ID", "terminal-1")
            .env("DMUX_SESSION_INSTANCE_ID", "instance-1")
            .env("DMUX_PROJECT_ID", "project-1")
            .env("DMUX_PROJECT_NAME", "Project")
            .env("DMUX_PROJECT_PATH", "/tmp/project")
            .env("DMUX_SESSION_TITLE", "CodeWhale")
            .env("DMUX_RUNTIME_EVENT_DIR", &events)
            .stdin(Stdio::null())
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "hook failed stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut entries = fs::read_dir(&events)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        assert_eq!(entries.len(), 1);
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&entries[0]).unwrap()).unwrap();
        assert_eq!(value["payload"]["kind"], "promptSubmitted");
        assert_eq!(value["payload"]["tool"], "codewhale");
        assert_eq!(value["payload"]["aiSessionID"], serde_json::Value::Null);
        assert_eq!(value["payload"]["metadata"]["source"], "user-input");
        fs::remove_dir_all(dir).unwrap();
    }
}
