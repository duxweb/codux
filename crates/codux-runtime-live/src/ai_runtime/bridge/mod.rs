use super::{
    binding::clear_runtime_bindings,
    event_file::clear_runtime_event_dir,
    hooks::{hook_config_status_in, uninstall_managed_hook_configs_in},
    log::runtime_log_line,
    paths::runtime_root_dir,
    payload::AIHookEventPayload,
    registry::{AIRuntimeRegistry, AIRuntimeTerminalState},
    supervisor::{AIRuntimeSupervisor, AIRuntimeSupervisorEvent},
    terminal_activity::{TerminalActivityEvent, TerminalActivityHub, TerminalActivitySubscription},
    terminal_status::TerminalStatusEvent,
};
use crate::runtime_paths::{
    ai_runtime_binding_dir_in, claude_session_map_dir_in, home_dir, opencode_session_map_dir_in,
    runtime_event_dir_in, runtime_temp_dir,
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

mod assets;
#[cfg(test)]
mod tests;

#[cfg(test)]
use assets::*;

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
    pub opencode: AIRuntimeToolHookConfigStatus,
    pub mimo: AIRuntimeToolHookConfigStatus,
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
    binding_dir: PathBuf,
    temp_dir: PathBuf,
    home_dir: PathBuf,
    registry: Arc<AIRuntimeRegistry>,
    supervisor: Arc<AIRuntimeSupervisor>,
    terminal_activity: TerminalActivityHub,
    started: AtomicBool,
    start_lock: Mutex<()>,
    hook_config_lock: Mutex<()>,
}

impl AIRuntimeBridge {
    pub fn new() -> Self {
        Self::with_paths(runtime_root_dir(), runtime_temp_dir(), home_dir())
    }

    pub fn with_runtime_paths(root_dir: PathBuf, temp_dir: PathBuf, home_dir: PathBuf) -> Self {
        Self::with_paths(root_dir, temp_dir, home_dir)
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
            binding_dir: ai_runtime_binding_dir_in(&temp_dir),
            temp_dir,
            home_dir,
            registry: AIRuntimeRegistry::shared(),
            supervisor: Arc::new(AIRuntimeSupervisor::new()),
            terminal_activity: TerminalActivityHub::default(),
            started: AtomicBool::new(false),
            start_lock: Mutex::new(()),
            hook_config_lock: Mutex::new(()),
        }
    }

    pub fn prepare(&self) -> Result<(), String> {
        self.stage_assets()?;
        self.strip_managed_hook_configs("prepare")
    }

    /// The runtime is non-intrusive by design: it never installs hooks into the
    /// CLIs' own config files. Live state comes from process-tree tool detection,
    /// transcript probes, and screen scraping. This only strips any codux hook
    /// entries a prior build left behind, leaving each CLI genuinely hookless.
    ///
    /// Idempotent and no-write once clean, so it is safe to run on every start.
    fn strip_managed_hook_configs(&self, phase: &str) -> Result<(), String> {
        let _guard = self
            .hook_config_lock
            .lock()
            .map_err(|_| "AI runtime hook config lock poisoned.".to_string())?;
        uninstall_managed_hook_configs_in(&self.home_dir)?;
        self.log_hook_config_status(phase);
        Ok(())
    }

    pub fn start_event_processing_background(&self) -> Result<(), String> {
        if self.started.load(Ordering::Acquire) {
            self.strip_managed_hook_configs("ensure-started")?;
            return Ok(());
        }
        let _guard = self
            .start_lock
            .lock()
            .map_err(|_| "AI runtime startup lock poisoned.".to_string())?;
        if self.started.load(Ordering::Acquire) {
            self.strip_managed_hook_configs("ensure-started")?;
            return Ok(());
        }
        self.prepare()?;
        let removed = clear_runtime_event_dir(&self.runtime_event_dir);
        runtime_log_line(
            "runtime-startup",
            &format!("cleared stale runtime event files count={removed}"),
        );
        let removed_bindings = clear_runtime_bindings(&self.binding_dir);
        runtime_log_line(
            "runtime-startup",
            &format!("cleared stale runtime binding files count={removed_bindings}"),
        );
        self.supervisor.start(
            Arc::clone(&self.registry),
            self.runtime_event_dir.clone(),
            self.binding_dir.clone(),
        )?;
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

    pub fn terminal_statuses_snapshot(&self) -> Vec<TerminalStatusEvent> {
        self.supervisor.terminal_statuses_snapshot()
    }

    pub fn submit_terminal_status(&self, status: TerminalStatusEvent) -> Result<(), String> {
        self.terminal_activity
            .publish(TerminalActivityEvent::Status(status.clone()));
        self.supervisor.submit_terminal_status(status)
    }

    pub(crate) fn subscribe_terminal_activity(
        &self,
        terminal_id: &str,
    ) -> TerminalActivitySubscription {
        self.terminal_activity.subscribe(terminal_id)
    }

    pub(crate) fn submit_terminal_exit(&self, terminal_id: &str, exit_code: Option<i32>) {
        self.terminal_activity.publish(TerminalActivityEvent::Exit {
            terminal_id: terminal_id.to_string(),
            exit_code,
        });
    }

    pub(crate) fn submit_terminal_error(&self, terminal_id: &str, message: &str) {
        self.terminal_activity
            .publish(TerminalActivityEvent::Error {
                terminal_id: terminal_id.to_string(),
                message: message.to_string(),
            });
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

    pub fn powershell_hook_script(&self) -> PathBuf {
        self.root_dir
            .join("scripts")
            .join("shell-hooks")
            .join("dmux-ai-hook.ps1")
    }

    pub fn registry(&self) -> Arc<AIRuntimeRegistry> {
        Arc::clone(&self.registry)
    }

    /// Remove a closed terminal's session from the live runtime state so it
    /// stops appearing in current-session aggregates.
    pub fn remove_session(&self, terminal_id: &str) -> bool {
        self.supervisor.remove_session(terminal_id)
    }

    /// Refresh an in-flight AI turn's liveness from real terminal output. No-op
    /// unless the terminal already has a `responding` turn.
    pub fn note_output_activity(&self, terminal_id: &str, now: f64) -> bool {
        self.supervisor.note_output_activity(terminal_id, now)
    }

    /// Ask the supervisor to scrape this terminal's current screen and apply
    /// the resulting runtime signal for AI session metadata.
    pub fn refresh_screen_signal(&self, terminal_id: &str) -> bool {
        self.supervisor.refresh_screen_signal(terminal_id)
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
            hook_config: hook_config_status_in(&self.root_dir.join("scripts").join("wrappers")),
            terminals: self.registry.snapshot(),
        }
    }

    fn log_hook_config_status(&self, phase: &str) {
        let status = hook_config_status_in(&self.root_dir.join("scripts").join("wrappers"));
        super::runtime_log_line(
            "runtime-hooks",
            &format!(
                "{phase} opencode={} mimo={} codewhale={} codewhale_missing={}",
                status.opencode.configured,
                status.mimo.configured,
                status.codewhale.configured,
                status.codewhale.missing.join("|")
            ),
        );
    }
}

impl Default for AIRuntimeBridge {
    fn default() -> Self {
        Self::new()
    }
}
