use super::{
    assets::{stage_runtime_asset, stage_runtime_dir},
    hooks::{hook_config_status, install_managed_hook_configs},
    paths::runtime_root_dir,
    registry::{AIRuntimeRegistry, AIRuntimeTerminalState},
    supervisor::{AIRuntimeSupervisor, AIRuntimeSupervisorEvent},
};
use crate::runtime_paths::{
    claude_session_map_dir_in, home_dir, opencode_session_map_dir_in, runtime_event_dir_in,
    runtime_socket_path_in, runtime_temp_dir,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeBridgeSnapshot {
    pub socket_path: String,
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
    socket_path: PathBuf,
    temp_dir: PathBuf,
    home_dir: PathBuf,
    registry: Arc<AIRuntimeRegistry>,
    supervisor: Arc<AIRuntimeSupervisor>,
}

impl AIRuntimeBridge {
    pub fn new() -> Self {
        Self::with_paths(runtime_root_dir(), runtime_temp_dir(), home_dir())
    }

    fn with_paths(root_dir: PathBuf, temp_dir: PathBuf, home_dir: PathBuf) -> Self {
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
            socket_path: runtime_socket_path_in(&temp_dir),
            temp_dir,
            home_dir,
            registry: AIRuntimeRegistry::shared(),
            supervisor: Arc::new(AIRuntimeSupervisor::new()),
        }
    }

    pub fn prepare(&self) -> Result<(), String> {
        self.stage_assets()?;
        install_managed_hook_configs(&self.managed_hook_script)
    }

    pub fn start_event_processing_background(&self) -> Result<(), String> {
        self.prepare()?;
        self.supervisor
            .start(Arc::clone(&self.registry), self.runtime_event_dir.clone())
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
        fs::create_dir_all(&self.temp_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.runtime_event_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.claude_session_map_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.opencode_session_map_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.home_dir.join(".kiro").join("agents"))
            .map_err(|error| error.to_string())?;

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

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn wrapper_bin_dir(&self) -> &Path {
        &self.wrapper_bin_dir
    }

    pub fn managed_hook_script(&self) -> &Path {
        &self.managed_hook_script
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
            socket_path: self.socket_path.display().to_string(),
            wrapper_bin_path: self.wrapper_bin_dir.display().to_string(),
            managed_hook_script_path: self.managed_hook_script.display().to_string(),
            hook_config: hook_config_status(
                &self
                    .root_dir
                    .join("scripts")
                    .join("wrappers")
                    .join("opencode-config"),
            ),
            terminals: self.registry.snapshot(),
        }
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
}
