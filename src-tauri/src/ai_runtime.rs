use crate::app_settings::{locale_from_language_setting, AppSettingsStore};
use crate::i18n::translate;
use crate::memory::MemoryStore;
use crate::notify_channels::{dispatch_notification_channels, NotificationDispatchRequest};
use crate::paths::{app_slug, home_dir, runtime_temp_dir};
use crate::project_store::ProjectStore;
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, Write};
#[cfg(unix)]
use std::net::Shutdown;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tauri::async_runtime::{channel, Receiver, Sender};
use tauri::Emitter;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

const RUNNING_STALE_SECONDS: f64 = 90.0;
const POLL_INTERVAL_SECONDS: u64 = 5;
const RUNNING_STATE_RENEWAL_SECONDS: f64 = 30.0;
const CODEX_INTERVAL_POLL_MINIMUM_SECONDS: f64 = 60.0;
const CODEX_LIVE_TRANSCRIPT_TAIL_BYTES: u64 = 128 * 1024;
const CODEX_LIVE_TRANSCRIPT_TAIL_LINES: usize = 260;
const TRANSCRIPT_MONITOR_INTERVAL_MS: u64 = 3_000;
const TRANSCRIPT_POLL_MINIMUM_SECONDS: f64 = 3.0;
const RUNTIME_EVENT_FILE_MAX_AGE_SECONDS: f64 = 300.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHookEventMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(rename = "wasInterrupted", skip_serializing_if = "Option::is_none")]
    pub was_interrupted: Option<bool>,
    #[serde(rename = "hasCompletedTurn", skip_serializing_if = "Option::is_none")]
    pub has_completed_turn: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHookEventPayload {
    pub kind: String,
    #[serde(rename = "terminalID")]
    pub terminal_id: String,
    #[serde(rename = "terminalInstanceID", skip_serializing_if = "Option::is_none")]
    pub terminal_instance_id: Option<String>,
    #[serde(rename = "projectID")]
    pub project_id: String,
    #[serde(rename = "projectName")]
    pub project_name: String,
    #[serde(rename = "projectPath", skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(rename = "sessionTitle")]
    pub session_title: String,
    pub tool: String,
    #[serde(rename = "aiSessionID", skip_serializing_if = "Option::is_none")]
    pub ai_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(rename = "inputTokens", skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i64>,
    #[serde(rename = "outputTokens", skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i64>,
    #[serde(rename = "cachedInputTokens", skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<i64>,
    #[serde(rename = "totalTokens", skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<i64>,
    #[serde(rename = "updatedAt")]
    pub updated_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AIHookEventMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEnvelope {
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIToolUsageEnvelope {
    pub session_id: String,
    pub session_instance_id: Option<String>,
    #[serde(rename = "externalSessionID")]
    pub external_session_id: Option<String>,
    pub project_id: String,
    pub project_name: String,
    pub project_path: Option<String>,
    pub session_title: String,
    pub tool: String,
    pub model: Option<String>,
    pub status: String,
    pub response_state: Option<String>,
    pub updated_at: f64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AIRuntimeEvent {
    Hook { payload: AIHookEventPayload },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AISessionSnapshot {
    pub terminal_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_instance_id: Option<String>,
    pub project_id: String,
    pub project_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub session_title: String,
    pub tool: String,
    #[serde(rename = "aiSessionId", skip_serializing_if = "Option::is_none")]
    pub ai_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub state: String,
    pub status: String,
    pub is_running: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub total_tokens: i64,
    pub baseline_total_tokens: i64,
    pub baseline_cached_input_tokens: i64,
    #[serde(skip_serializing)]
    pub baseline_resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<f64>,
    pub updated_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_turn_started_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_turn_started_at: Option<f64>,
    pub has_completed_turn: bool,
    pub was_interrupted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_assistant_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AIProjectPhase {
    Idle,
    Running {
        tool: String,
    },
    NeedsInput {
        tool: String,
    },
    Completed {
        tool: String,
        was_interrupted: bool,
        updated_at: f64,
    },
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AIProjectTotals {
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub running: usize,
    pub needs_input: usize,
    pub completed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIProjectStateSnapshot {
    pub project_id: String,
    pub project_phase: AIProjectPhase,
    pub completed_phase: AIProjectPhase,
    pub totals: AIProjectTotals,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AILatestCompletion {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub tool: String,
    pub was_interrupted: bool,
    pub updated_at: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeStateSnapshot {
    pub sessions: Vec<AISessionSnapshot>,
    pub projects: Vec<AIProjectStateSnapshot>,
    pub global_totals: AIProjectTotals,
    pub needs_input_count: usize,
    pub running_count: usize,
    pub completion_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_completion: Option<AILatestCompletion>,
    pub updated_at: f64,
}

#[derive(Debug, Clone)]
pub struct AIRuntimeCompletionEvent {
    pub id: String,
    pub project_name: String,
    pub tool: String,
    pub was_interrupted: bool,
    pub session: Option<AISessionSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeTerminalState {
    pub terminal_id: String,
    pub project_id: String,
    pub slot_id: String,
    pub title: String,
    pub cwd: String,
    pub tool: Option<String>,
    pub is_active: bool,
    pub session_key: Option<String>,
    pub terminal_instance_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeBridgeSnapshot {
    pub socket_path: String,
    pub wrapper_bin_path: String,
    pub zdotdir_path: String,
    pub hook_script_path: String,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeProbeRequest {
    pub terminal_id: String,
    pub terminal_instance_id: Option<String>,
    pub project_id: String,
    pub project_path: Option<String>,
    pub tool: String,
    pub external_session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub started_at: Option<f64>,
    pub updated_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeContextSnapshot {
    pub tool: String,
    #[serde(rename = "externalSessionID", skip_serializing_if = "Option::is_none")]
    pub external_session_id: Option<String>,
    #[serde(rename = "transcriptPath", skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_preview: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub total_tokens: i64,
    pub updated_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_state: Option<String>,
    pub was_interrupted: bool,
    pub has_completed_turn: bool,
    pub session_origin: String,
    pub source: String,
}

#[derive(Debug, Clone, Default)]
pub struct AIRuntimeTerminalBinding {
    pub terminal_id: String,
    pub project_id: String,
    pub slot_id: String,
    pub title: String,
    pub cwd: String,
    pub tool: Option<String>,
    pub is_active: bool,
    pub session_key: Option<String>,
    pub terminal_instance_id: Option<String>,
}

#[derive(Default)]
pub struct AIRuntimeRegistry {
    terminals: Mutex<HashMap<String, AIRuntimeTerminalBinding>>,
}

impl AIRuntimeRegistry {
    pub fn upsert(&self, binding: AIRuntimeTerminalBinding) {
        if let Ok(mut terminals) = self.terminals.lock() {
            terminals.insert(binding.terminal_id.clone(), binding);
        }
    }

    pub fn remove(&self, terminal_id: &str) {
        if let Ok(mut terminals) = self.terminals.lock() {
            terminals.remove(terminal_id);
        }
    }

    pub fn snapshot(&self) -> Vec<AIRuntimeTerminalState> {
        let Ok(terminals) = self.terminals.lock() else {
            return Vec::new();
        };
        terminals
            .values()
            .map(|binding| AIRuntimeTerminalState {
                terminal_id: binding.terminal_id.clone(),
                project_id: binding.project_id.clone(),
                slot_id: binding.slot_id.clone(),
                title: binding.title.clone(),
                cwd: binding.cwd.clone(),
                tool: binding.tool.clone(),
                is_active: binding.is_active,
                session_key: binding.session_key.clone(),
                terminal_instance_id: binding.terminal_instance_id.clone(),
            })
            .collect()
    }
}

pub struct AIRuntimeBridge {
    root_dir: PathBuf,
    wrapper_bin_dir: PathBuf,
    zdotdir: PathBuf,
    hook_script: PathBuf,
    managed_hook_script: PathBuf,
    runtime_event_dir: PathBuf,
    socket_path: PathBuf,
    registry: Arc<AIRuntimeRegistry>,
    supervisor: Arc<AIRuntimeSupervisor>,
    startup: Arc<AIRuntimeStartupState>,
}

#[derive(Default)]
struct AIRuntimeStartupState {
    status: Mutex<AIRuntimeStartupStatus>,
    ready: Condvar,
}

#[derive(Default)]
enum AIRuntimeStartupStatus {
    #[default]
    Idle,
    Starting,
    Ready,
    Failed(String),
}

fn startup_status_result(status: &AIRuntimeStartupStatus) -> Result<(), String> {
    match status {
        AIRuntimeStartupStatus::Ready => Ok(()),
        AIRuntimeStartupStatus::Failed(error) => Err(error.clone()),
        AIRuntimeStartupStatus::Idle => Err("AI runtime has not been started.".to_string()),
        AIRuntimeStartupStatus::Starting => Err("AI runtime is still starting.".to_string()),
    }
}

impl AIRuntimeBridge {
    pub fn new(
        settings: Arc<AppSettingsStore>,
        memory: Arc<MemoryStore>,
        projects: Arc<ProjectStore>,
    ) -> Self {
        let root_dir = runtime_root_dir();
        let wrapper_bin_dir = root_dir.join("scripts").join("wrappers").join("bin");
        let zdotdir = root_dir.join("scripts").join("shell-hooks").join("zsh");
        let hook_script = root_dir
            .join("scripts")
            .join("shell-hooks")
            .join("dmux-ai-hook.zsh");
        let managed_hook_script = root_dir
            .join("scripts")
            .join("wrappers")
            .join("dmux-ai-state.sh");
        let socket_path = runtime_temp_dir().join("runtime-events.sock");
        let runtime_event_dir = runtime_event_dir();
        Self {
            root_dir,
            wrapper_bin_dir,
            zdotdir,
            hook_script,
            managed_hook_script,
            runtime_event_dir,
            socket_path,
            registry: Arc::new(AIRuntimeRegistry::default()),
            supervisor: Arc::new(AIRuntimeSupervisor::new(settings, memory, projects)),
            startup: Arc::new(AIRuntimeStartupState::default()),
        }
    }

    pub fn prepare(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.wrapper_bin_dir.parent().unwrap_or(&self.root_dir))
            .map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.wrapper_bin_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.zdotdir).map_err(|error| error.to_string())?;
        fs::create_dir_all(runtime_temp_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.runtime_event_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.claude_session_map_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.opencode_session_map_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(home_dir().join(".kiro").join("agents"))
            .map_err(|error| error.to_string())?;

        stage_runtime_asset(
            "scripts/shell-hooks/dmux-ai-hook.zsh",
            &self.hook_script,
            false,
        )?;
        stage_runtime_asset(
            "scripts/shell-hooks/zsh/.zshenv",
            &self.zdotdir.join(".zshenv"),
            false,
        )?;
        stage_runtime_asset(
            "scripts/shell-hooks/zsh/.zprofile",
            &self.zdotdir.join(".zprofile"),
            false,
        )?;
        stage_runtime_asset(
            "scripts/shell-hooks/zsh/.zshrc",
            &self.zdotdir.join(".zshrc"),
            false,
        )?;
        stage_runtime_asset(
            "scripts/shell-hooks/zsh/.zlogin",
            &self.zdotdir.join(".zlogin"),
            false,
        )?;
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
        stage_runtime_asset(
            "scripts/wrappers/dmux-ai-state.cmd",
            &wrapper_dir.join("dmux-ai-state.cmd"),
            false,
        )?;
        #[cfg(windows)]
        stage_runtime_asset(
            "scripts/wrappers/dmux-ai-state.ps1",
            &wrapper_dir.join("dmux-ai-state.ps1"),
            false,
        )?;
        stage_runtime_asset(
            "scripts/wrappers/tool-wrapper.sh",
            &self
                .root_dir
                .join("scripts")
                .join("wrappers")
                .join("tool-wrapper.sh"),
            true,
        )?;
        #[cfg(not(windows))]
        stage_runtime_asset(
            "scripts/wrappers/codux-ssh-expect.exp",
            &self
                .root_dir
                .join("scripts")
                .join("wrappers")
                .join("codux-ssh-expect.exp"),
            true,
        )?;
        #[cfg(windows)]
        stage_runtime_asset(
            "scripts/wrappers/tool-wrapper.cmd",
            &self
                .root_dir
                .join("scripts")
                .join("wrappers")
                .join("tool-wrapper.cmd"),
            false,
        )?;
        #[cfg(windows)]
        stage_runtime_asset(
            "scripts/wrappers/tool-wrapper.ps1",
            &self
                .root_dir
                .join("scripts")
                .join("wrappers")
                .join("tool-wrapper.ps1"),
            false,
        )?;
        #[cfg(windows)]
        stage_runtime_asset(
            "scripts/wrappers/codux-ssh.ps1",
            &self
                .root_dir
                .join("scripts")
                .join("wrappers")
                .join("codux-ssh.ps1"),
            false,
        )?;
        stage_runtime_dir(
            "scripts/wrappers/opencode-config",
            &self
                .root_dir
                .join("scripts")
                .join("wrappers")
                .join("opencode-config"),
        )?;

        for bin_name in [
            "codex",
            "claude",
            "claude-code",
            "gemini",
            "agy",
            "opencode",
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
                stage_runtime_asset(
                    &format!("scripts/wrappers/bin/{bin_name}.cmd"),
                    &self.wrapper_bin_dir.join(format!("{bin_name}.cmd")),
                    false,
                )?;
            }
        }

        self.install_managed_hook_configs()?;

        Ok(())
    }

    pub fn start_listener_background(self: &Arc<Self>, app: AppHandle) {
        if !self.mark_starting() {
            return;
        }
        reset_runtime_live_log();
        let runtime = Arc::clone(self);
        if let Err(error) = thread::Builder::new()
            .name("codux-ai-runtime-startup".to_string())
            .spawn(move || {
                let result = runtime.start_listener_inner(app);
                runtime.finish_startup(result);
            })
        {
            self.finish_startup(Err(error.to_string()));
        }
    }

    pub fn ensure_started(&self) -> Result<(), String> {
        match self.startup.status.lock() {
            Ok(status) => match &*status {
                AIRuntimeStartupStatus::Idle => drop(status),
                AIRuntimeStartupStatus::Starting => {
                    let status = self
                        .startup
                        .ready
                        .wait_while(status, |status| {
                            matches!(status, AIRuntimeStartupStatus::Starting)
                        })
                        .map_err(|_| "AI runtime startup lock poisoned.".to_string())?;
                    return startup_status_result(&status);
                }
                AIRuntimeStartupStatus::Ready => return Ok(()),
                AIRuntimeStartupStatus::Failed(error) => return Err(error.clone()),
            },
            Err(_) => return Err("AI runtime startup lock poisoned.".to_string()),
        }

        Err("AI runtime has not been started.".to_string())
    }

    fn start_listener_inner(&self, app: AppHandle) -> Result<(), String> {
        self.prepare()?;
        self.supervisor
            .start(app.clone(), Arc::clone(&self.registry))?;
        #[cfg(unix)]
        {
            if self.socket_path.exists() {
                let _ = fs::remove_file(&self.socket_path);
            }

            let listener =
                UnixListener::bind(&self.socket_path).map_err(|error| error.to_string())?;
            let _ = fs::set_permissions(&self.socket_path, fs::Permissions::from_mode(0o700));
            let supervisor_tx = self.supervisor.hook_sender();

            thread::Builder::new()
                .name("codux-ai-runtime-listener".to_string())
                .spawn(move || {
                    for stream in listener.incoming() {
                        match stream {
                            Ok(stream) => {
                                let tx = supervisor_tx.clone();
                                thread::spawn(move || {
                                    handle_runtime_stream(stream, tx);
                                });
                            }
                            Err(_) => break,
                        }
                    }
                })
                .map_err(|error| error.to_string())?;
        }

        #[cfg(not(unix))]
        {
            let _ = app;
        }

        Ok(())
    }

    fn mark_starting(&self) -> bool {
        let Ok(mut status) = self.startup.status.lock() else {
            return false;
        };
        if !matches!(*status, AIRuntimeStartupStatus::Idle) {
            return false;
        }
        *status = AIRuntimeStartupStatus::Starting;
        true
    }

    fn finish_startup(&self, result: Result<(), String>) {
        if let Ok(mut status) = self.startup.status.lock() {
            *status = match result {
                Ok(()) => AIRuntimeStartupStatus::Ready,
                Err(error) => AIRuntimeStartupStatus::Failed(error),
            };
            self.startup.ready.notify_all();
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn wrapper_bin_dir(&self) -> &Path {
        &self.wrapper_bin_dir
    }

    pub fn zdotdir(&self) -> &Path {
        &self.zdotdir
    }

    pub fn hook_script(&self) -> &Path {
        &self.hook_script
    }

    pub fn managed_hook_script(&self) -> &Path {
        &self.managed_hook_script
    }

    pub fn registry(&self) -> Arc<AIRuntimeRegistry> {
        Arc::clone(&self.registry)
    }

    pub fn claude_session_map_dir(&self) -> PathBuf {
        runtime_temp_dir().join("claude-session-map")
    }

    pub fn opencode_session_map_dir(&self) -> PathBuf {
        runtime_temp_dir().join("opencode-session-map")
    }

    pub fn snapshot(&self) -> AIRuntimeBridgeSnapshot {
        AIRuntimeBridgeSnapshot {
            socket_path: self.socket_path.display().to_string(),
            wrapper_bin_path: self.wrapper_bin_dir.display().to_string(),
            zdotdir_path: self.zdotdir.display().to_string(),
            hook_script_path: self.hook_script.display().to_string(),
            managed_hook_script_path: self.managed_hook_script.display().to_string(),
            hook_config: self.hook_config_status(),
            terminals: self.registry.snapshot(),
        }
    }

    pub fn probe(&self, request: AIRuntimeProbeRequest) -> Option<AIRuntimeContextSnapshot> {
        let _ = (
            &request.terminal_id,
            &request.terminal_instance_id,
            &request.project_id,
        );
        match canonical_tool_name(&request.tool).as_deref() {
            Some("codex") => probe_codex_runtime(&request),
            Some("claude") => probe_claude_runtime(&request),
            Some("gemini") => probe_gemini_runtime(&request),
            Some("opencode") => probe_opencode_runtime(&request),
            Some("kiro") => probe_kiro_runtime(&request),
            _ => None,
        }
    }

    fn install_managed_hook_configs(&self) -> Result<(), String> {
        let codex_hooks_path = home_dir().join(".codex").join("hooks.json");
        install_tool_hooks(
            &codex_hooks_path,
            "codex",
            &[
                ("SessionStart", "codex-session-start", 1000, false),
                ("UserPromptSubmit", "codex-prompt-submit", 1000, false),
                ("PermissionRequest", "codex-permission-request", 1000, false),
                ("Stop", "codex-stop", 1000, false),
            ],
            self,
        )?;
        ensure_codex_config_installed(&codex_hooks_path)?;
        install_tool_hooks(
            &home_dir().join(".claude").join("settings.json"),
            "claude",
            &[
                ("SessionStart", "session-start", 10, false),
                ("UserPromptSubmit", "prompt-submit", 10, false),
                ("PreCompact", "pre-compact", 10, false),
                ("PostCompact", "post-compact", 10, false),
                ("Stop", "stop", 10, false),
                ("StopFailure", "stop-failure", 10, false),
                ("SessionEnd", "session-end", 1, false),
                ("PermissionRequest", "permission-request", 5, true),
                ("PermissionDenied", "permission-denied", 5, true),
                ("Elicitation", "elicitation", 10, true),
                ("ElicitationResult", "elicitation-result", 10, true),
            ],
            self,
        )?;
        install_tool_hooks(
            &home_dir().join(".gemini").join("settings.json"),
            "gemini",
            &[
                ("SessionStart", "session-start", 5000, false),
                ("BeforeAgent", "before-agent", 5000, false),
                ("AfterAgent", "after-agent", 5000, false),
                ("Notification", "notification", 5000, false),
                ("SessionEnd", "session-end", 5000, false),
            ],
            self,
        )?;
        install_tool_hooks(
            &home_dir()
                .join(".gemini")
                .join("antigravity-cli")
                .join("settings.json"),
            "agy",
            &[
                ("SessionStart", "session-start", 5000, false),
                ("BeforeAgent", "before-agent", 5000, false),
                ("AfterAgent", "after-agent", 5000, false),
                ("Notification", "notification", 5000, false),
                ("SessionEnd", "session-end", 5000, false),
            ],
            self,
        )?;
        install_tool_hooks(
            &home_dir()
                .join(".kiro")
                .join("agents")
                .join("codux-managed.json"),
            "kiro",
            &[
                ("agentSpawn", "session-start", 5000, false),
                ("stop", "session-end", 5000, false),
            ],
            self,
        )?;
        Ok(())
    }

    fn hook_config_status(&self) -> AIRuntimeHookConfigStatus {
        AIRuntimeHookConfigStatus {
            codex: tool_hook_config_status(
                &home_dir().join(".codex").join("hooks.json"),
                "codex",
                &[
                    ("SessionStart", "codex-session-start"),
                    ("UserPromptSubmit", "codex-prompt-submit"),
                    ("PermissionRequest", "codex-permission-request"),
                    ("Stop", "codex-stop"),
                ],
            ),
            claude: tool_hook_config_status(
                &home_dir().join(".claude").join("settings.json"),
                "claude",
                &[
                    ("SessionStart", "session-start"),
                    ("UserPromptSubmit", "prompt-submit"),
                    ("PreCompact", "pre-compact"),
                    ("PostCompact", "post-compact"),
                    ("Stop", "stop"),
                    ("StopFailure", "stop-failure"),
                    ("SessionEnd", "session-end"),
                    ("PermissionRequest", "permission-request"),
                    ("PermissionDenied", "permission-denied"),
                    ("Elicitation", "elicitation"),
                    ("ElicitationResult", "elicitation-result"),
                ],
            ),
            gemini: tool_hook_config_status(
                &home_dir().join(".gemini").join("settings.json"),
                "gemini",
                &[
                    ("SessionStart", "session-start"),
                    ("BeforeAgent", "before-agent"),
                    ("AfterAgent", "after-agent"),
                    ("Notification", "notification"),
                    ("SessionEnd", "session-end"),
                ],
            ),
            opencode: opencode_hook_config_status(
                &self
                    .root_dir
                    .join("scripts")
                    .join("wrappers")
                    .join("opencode-config"),
            ),
            kiro: tool_hook_config_status(
                &home_dir()
                    .join(".kiro")
                    .join("agents")
                    .join("codux-managed.json"),
                "kiro",
                &[("agentSpawn", "session-start"), ("stop", "session-end")],
            ),
        }
    }

    pub fn state_snapshot(&self) -> AIRuntimeStateSnapshot {
        self.supervisor.state_snapshot()
    }

    pub fn dismiss_completion(&self, project_id: String) -> bool {
        self.supervisor.dismiss_completion(project_id)
    }

    pub fn sync_window_state(&self, app: &AppHandle, settings: &AppSettingsStore) {
        apply_runtime_window_state(app, &self.state_snapshot(), settings);
    }
}

#[derive(Debug)]
enum AIRuntimeSupervisorMessage {
    HookFrame(Vec<u8>),
    Poll,
    TranscriptTail(Vec<String>),
}

struct AIRuntimeSupervisor {
    hook_tx: Sender<AIRuntimeSupervisorMessage>,
    hook_rx: Mutex<Option<Receiver<AIRuntimeSupervisorMessage>>>,
    state: Arc<AIRuntimeStateStore>,
    transcript_monitors: Arc<Mutex<HashMap<String, TranscriptMonitor>>>,
    settings: Arc<AppSettingsStore>,
    memory: Arc<MemoryStore>,
    projects: Arc<ProjectStore>,
}

impl AIRuntimeSupervisor {
    fn new(
        settings: Arc<AppSettingsStore>,
        memory: Arc<MemoryStore>,
        projects: Arc<ProjectStore>,
    ) -> Self {
        let (hook_tx, hook_rx) = channel(1024);
        Self {
            hook_tx,
            hook_rx: Mutex::new(Some(hook_rx)),
            state: Arc::new(AIRuntimeStateStore::default()),
            transcript_monitors: Arc::new(Mutex::new(HashMap::new())),
            settings,
            memory,
            projects,
        }
    }

    fn hook_sender(&self) -> Sender<AIRuntimeSupervisorMessage> {
        self.hook_tx.clone()
    }

    fn start(&self, app: AppHandle, registry: Arc<AIRuntimeRegistry>) -> Result<(), String> {
        let mut receiver = self
            .hook_rx
            .lock()
            .map_err(|_| "AI runtime supervisor lock poisoned.".to_string())?;
        let Some(hook_rx) = receiver.take() else {
            return Ok(());
        };
        let state = Arc::clone(&self.state);
        let transcript_monitors = Arc::clone(&self.transcript_monitors);
        let settings = Arc::clone(&self.settings);
        let memory = Arc::clone(&self.memory);
        let projects = Arc::clone(&self.projects);
        let poll_tx = self.hook_tx.clone();
        start_ai_runtime_poll_loop(poll_tx);
        start_ai_runtime_transcript_monitor_loop(
            self.hook_tx.clone(),
            Arc::clone(&transcript_monitors),
            runtime_event_dir(),
        );
        tauri::async_runtime::spawn(ai_runtime_supervisor_loop(
            hook_rx,
            app,
            registry,
            state,
            transcript_monitors,
            settings,
            memory,
            projects,
        ));
        Ok(())
    }

    fn state_snapshot(&self) -> AIRuntimeStateSnapshot {
        self.state.snapshot()
    }

    fn dismiss_completion(&self, project_id: String) -> bool {
        self.state.dismiss_completion(&project_id)
    }
}

async fn ai_runtime_supervisor_loop(
    mut hook_rx: Receiver<AIRuntimeSupervisorMessage>,
    app: AppHandle,
    registry: Arc<AIRuntimeRegistry>,
    state: Arc<AIRuntimeStateStore>,
    transcript_monitors: Arc<Mutex<HashMap<String, TranscriptMonitor>>>,
    settings: Arc<AppSettingsStore>,
    memory: Arc<MemoryStore>,
    projects: Arc<ProjectStore>,
) {
    while let Some(message) = hook_rx.recv().await {
        match message {
            AIRuntimeSupervisorMessage::HookFrame(frame) => {
                let Some(payload) = runtime_frame_to_hook(&frame) else {
                    runtime_log_line(
                        "runtime-ingress",
                        &format!("drop hook-frame reason=decode bytes={}", frame.len()),
                    );
                    continue;
                };
                runtime_log_line(
                    "runtime-ingress",
                    &format!(
                        "receive hook tool={} kind={} terminal={} project={}",
                        payload.tool, payload.kind, payload.terminal_id, payload.project_id
                    ),
                );
                let _ = app.emit(
                    "ai-runtime:event",
                    AIRuntimeEvent::Hook {
                        payload: payload.clone(),
                    },
                );
                let mutation = state.apply_hook(payload);
                runtime_log_line(
                    "runtime-ingress",
                    if mutation.did_change {
                        "apply hook result=changed"
                    } else {
                        "apply hook result=no-change"
                    },
                );
                if mutation.did_change {
                    emit_runtime_state(&app, &state);
                }
                refresh_transcript_monitors(
                    &transcript_monitors,
                    &state.transcript_monitored_sessions(),
                );
                apply_runtime_window_state(&app, &state.snapshot(), &settings);
                if let Some(completion) = mutation.completion {
                    handle_runtime_completion(
                        app.clone(),
                        settings.clone(),
                        memory.clone(),
                        projects.clone(),
                        completion,
                    );
                }
            }
            AIRuntimeSupervisorMessage::Poll => {
                let snapshot = state.snapshot();
                let tracked = state.runtime_tracked_sessions(now_seconds());
                if tracked.is_empty() {
                    refresh_transcript_monitors(
                        &transcript_monitors,
                        &state.transcript_monitored_sessions(),
                    );
                    apply_runtime_window_state(&app, &snapshot, &settings);
                    continue;
                }
                let mutation = poll_runtime_sessions(&state, &registry, "interval", None);
                if mutation.did_change {
                    runtime_log_line("runtime-refresh", "poll reason=interval result=changed");
                }
                if mutation.did_change {
                    emit_runtime_state(&app, &state);
                }
                refresh_transcript_monitors(
                    &transcript_monitors,
                    &state.transcript_monitored_sessions(),
                );
                apply_runtime_window_state(&app, &state.snapshot(), &settings);
                if let Some(completion) = mutation.completion {
                    handle_runtime_completion(
                        app.clone(),
                        settings.clone(),
                        memory.clone(),
                        projects.clone(),
                        completion,
                    );
                }
            }
            AIRuntimeSupervisorMessage::TranscriptTail(terminal_ids) => {
                let terminal_ids = terminal_ids
                    .into_iter()
                    .collect::<std::collections::HashSet<_>>();
                if terminal_ids.is_empty() {
                    continue;
                }
                let mutation = poll_runtime_sessions(
                    &state,
                    &registry,
                    "transcript-tail",
                    Some(&terminal_ids),
                );
                if mutation.did_change {
                    runtime_log_line(
                        "runtime-refresh",
                        "poll reason=transcript-tail result=changed",
                    );
                }
                if mutation.did_change {
                    emit_runtime_state(&app, &state);
                }
                refresh_transcript_monitors(
                    &transcript_monitors,
                    &state.transcript_monitored_sessions(),
                );
                apply_runtime_window_state(&app, &state.snapshot(), &settings);
                if let Some(completion) = mutation.completion {
                    handle_runtime_completion(
                        app.clone(),
                        settings.clone(),
                        memory.clone(),
                        projects.clone(),
                        completion,
                    );
                }
            }
        }
    }
}

fn poll_runtime_sessions(
    state: &AIRuntimeStateStore,
    registry: &AIRuntimeRegistry,
    reason: &str,
    terminal_ids: Option<&std::collections::HashSet<String>>,
) -> AIRuntimeStateMutation {
    let terminal_snapshot = registry.snapshot();
    let mut mutation = state.reconcile_bridge_snapshot(&terminal_snapshot);
    let now = now_seconds();
    let sessions = terminal_ids
        .map(|ids| state.sessions_for_terminals(ids))
        .unwrap_or_else(|| state.runtime_tracked_sessions(now));
    for session in sessions {
        if !should_poll_session(&session, reason, now_seconds()) {
            continue;
        }
        let request = probe_request_for_session(&session);
        let next = match canonical_tool_name(&request.tool).as_deref() {
            Some("codex") => probe_codex_runtime(&request),
            Some("claude") => probe_claude_runtime(&request),
            Some("gemini") => probe_gemini_runtime(&request),
            Some("opencode") => probe_opencode_runtime(&request),
            _ => None,
        };
        if let Some(snapshot) = next {
            mutation.merge(state.apply_runtime_snapshot(&session.terminal_id, snapshot));
        }
    }
    mutation
}

fn start_ai_runtime_poll_loop(tx: Sender<AIRuntimeSupervisorMessage>) {
    let _ = thread::Builder::new()
        .name("codux-ai-runtime-poller".to_string())
        .spawn(move || loop {
            thread::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECONDS));
            if tx.blocking_send(AIRuntimeSupervisorMessage::Poll).is_err() {
                break;
            }
        });
}

fn start_ai_runtime_transcript_monitor_loop(
    tx: Sender<AIRuntimeSupervisorMessage>,
    monitors: Arc<Mutex<HashMap<String, TranscriptMonitor>>>,
    runtime_event_dir: PathBuf,
) {
    let _ = thread::Builder::new()
        .name("codux-ai-runtime-transcript-monitor".to_string())
        .spawn(move || loop {
            thread::sleep(std::time::Duration::from_millis(
                TRANSCRIPT_MONITOR_INTERVAL_MS,
            ));
            let changed = match monitors.lock() {
                Ok(mut monitors) => {
                    if monitors.is_empty() {
                        Vec::new()
                    } else {
                        scan_transcript_monitors(&mut monitors, now_seconds())
                    }
                }
                Err(_) => Vec::new(),
            };
            if changed.is_empty() {
                drain_runtime_event_dir(&tx, &runtime_event_dir);
                continue;
            }
            drain_runtime_event_dir(&tx, &runtime_event_dir);
            if tx
                .blocking_send(AIRuntimeSupervisorMessage::TranscriptTail(changed))
                .is_err()
            {
                break;
            }
        });
}

fn runtime_event_dir() -> PathBuf {
    runtime_temp_dir().join("runtime-events")
}

fn drain_runtime_event_dir(tx: &Sender<AIRuntimeSupervisorMessage>, dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = now_seconds();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let age = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| now - duration.as_secs_f64())
            .unwrap_or(0.0);
        let data = fs::read(&path).ok();
        let _ = fs::remove_file(&path);
        if age > RUNTIME_EVENT_FILE_MAX_AGE_SECONDS {
            runtime_log_line(
                "hook-file",
                &format!(
                    "drop event-file reason=stale age={age:.1}s file={}",
                    path.display()
                ),
            );
            continue;
        }
        if let Some(data) = data.filter(|value| !value.is_empty()) {
            runtime_log_line(
                "hook-file",
                &format!(
                    "drain event-file bytes={} file={}",
                    data.len(),
                    path.display()
                ),
            );
            let _ = tx.blocking_send(AIRuntimeSupervisorMessage::HookFrame(data));
        }
    }
}

fn emit_runtime_state(app: &AppHandle, state: &AIRuntimeStateStore) {
    let _ = app.emit("ai-runtime:state", state.snapshot());
}

fn apply_runtime_window_state(
    app: &AppHandle,
    snapshot: &AIRuntimeStateSnapshot,
    settings: &AppSettingsStore,
) {
    let configured = settings.snapshot();
    let attention_count = snapshot.needs_input_count + snapshot.completion_count;
    let count = if configured.shows_dock_badge && attention_count > 0 {
        Some(attention_count as i64)
    } else {
        None
    };
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_badge_count(count);
        #[cfg(target_os = "macos")]
        let _ = window.set_badge_label(count.map(|value| value.to_string()));
    }
}

fn dispatch_completion_notification(
    app: AppHandle,
    settings: Arc<AppSettingsStore>,
    completion: AIRuntimeCompletionEvent,
) {
    tauri::async_runtime::spawn(async move {
        let configured = settings.snapshot();
        let locale = locale_from_setting(&configured.language);
        let title = if completion.was_interrupted {
            translate(
                &locale,
                "ai.notification.task_interrupted",
                "Task interrupted",
            )
        } else {
            translate(&locale, "ai.notification.task_completed", "Task completed")
        };
        let body = format!("{} · {}", completion.project_name, completion.tool);
        let _ = app
            .notification()
            .builder()
            .title(title.clone())
            .body(body.clone())
            .group("codux-task")
            .auto_cancel()
            .show();
        let channels = settings.configured_notification_channels();
        if channels.is_empty() {
            return;
        }
        let _ = dispatch_notification_channels(NotificationDispatchRequest {
            channels,
            title,
            body,
            group: "codux-task".to_string(),
        })
        .await;
    });
}

fn handle_runtime_completion(
    app: AppHandle,
    settings: Arc<AppSettingsStore>,
    memory: Arc<MemoryStore>,
    projects: Arc<ProjectStore>,
    completion: AIRuntimeCompletionEvent,
) {
    dispatch_completion_notification(app.clone(), Arc::clone(&settings), completion.clone());
    if let Some(session) = completion.session {
        memory.handle_completed_session(
            settings,
            projects.project_workspaces_snapshot(),
            session,
            move |event| {
                let _ = app.emit("memory:status", event.status);
                if let Some(manager) = event.manager {
                    let _ = app.emit("memory:manager", manager);
                }
            },
        );
    }
}

fn locale_from_setting(language: &str) -> String {
    locale_from_language_setting(language)
}

#[derive(Default)]
struct AIRuntimeStateCore {
    sessions: HashMap<String, AISessionSnapshot>,
    logical_baselines: HashMap<String, i64>,
    logical_cached_baselines: HashMap<String, i64>,
    dismissed_completed_at: HashMap<String, f64>,
    latest_active_started_at_by_project: HashMap<String, f64>,
    notified_completion_at: HashMap<String, f64>,
}

#[derive(Default)]
struct AIRuntimeStateStore {
    core: Mutex<AIRuntimeStateCore>,
}

#[derive(Default)]
struct AIRuntimeStateMutation {
    did_change: bool,
    completion: Option<AIRuntimeCompletionEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TranscriptSignature {
    size: u64,
    modified_millis: u128,
}

#[derive(Debug, Clone)]
struct TranscriptMonitor {
    path: String,
    signature: Option<TranscriptSignature>,
    last_poll_at: Option<f64>,
}

impl AIRuntimeStateMutation {
    fn merge(&mut self, next: AIRuntimeStateMutation) {
        self.did_change = self.did_change || next.did_change;
        match (&self.completion, next.completion) {
            (None, Some(candidate)) => self.completion = Some(candidate),
            (Some(current), Some(candidate)) if candidate.id > current.id => {
                self.completion = Some(candidate);
            }
            _ => {}
        }
    }
}

impl AIRuntimeStateStore {
    fn snapshot(&self) -> AIRuntimeStateSnapshot {
        let Ok(core) = self.core.lock() else {
            return AIRuntimeStateSnapshot::default();
        };
        state_snapshot_unlocked(&core)
    }

    fn runtime_tracked_sessions(&self, now: f64) -> Vec<AISessionSnapshot> {
        let Ok(core) = self.core.lock() else {
            return Vec::new();
        };
        core.sessions
            .values()
            .filter(|session| {
                if session.state == "responding" || session.state == "needsInput" {
                    return true;
                }
                !session.has_completed_turn
                    && now - session.updated_at <= RUNNING_STALE_SECONDS * 3.0
            })
            .cloned()
            .collect()
    }

    fn transcript_monitored_sessions(&self) -> Vec<AISessionSnapshot> {
        let Ok(core) = self.core.lock() else {
            return Vec::new();
        };
        core.sessions
            .values()
            .filter(|session| is_codex_transcript_session(session))
            .cloned()
            .collect()
    }

    fn sessions_for_terminals(
        &self,
        terminal_ids: &std::collections::HashSet<String>,
    ) -> Vec<AISessionSnapshot> {
        let Ok(core) = self.core.lock() else {
            return Vec::new();
        };
        core.sessions
            .values()
            .filter(|session| terminal_ids.contains(&session.terminal_id))
            .cloned()
            .collect()
    }

    fn dismiss_completion(&self, project_id: &str) -> bool {
        let Ok(mut core) = self.core.lock() else {
            return false;
        };
        let AIProjectPhase::Completed { updated_at, .. } =
            completed_phase_unlocked(&core, project_id)
        else {
            return false;
        };
        let previous = core
            .dismissed_completed_at
            .get(project_id)
            .copied()
            .unwrap_or(0.0);
        let next = previous.max(updated_at);
        if next <= previous {
            return false;
        }
        core.dismissed_completed_at
            .insert(project_id.to_string(), next);
        true
    }

    fn apply_hook(&self, event: AIHookEventPayload) -> AIRuntimeStateMutation {
        let previous = self
            .core
            .lock()
            .ok()
            .and_then(|core| core.sessions.get(event.terminal_id.trim()).cloned());
        let event = resolve_hook_event(event, previous.as_ref());
        let Ok(mut core) = self.core.lock() else {
            return AIRuntimeStateMutation::default();
        };
        let did_change = apply_hook_unlocked(&mut core, event);
        AIRuntimeStateMutation {
            did_change,
            completion: did_change
                .then(|| next_completion_event_unlocked(&mut core))
                .flatten(),
        }
    }

    fn apply_runtime_snapshot(
        &self,
        terminal_id: &str,
        snapshot: AIRuntimeContextSnapshot,
    ) -> AIRuntimeStateMutation {
        let Ok(mut core) = self.core.lock() else {
            return AIRuntimeStateMutation::default();
        };
        let did_change = apply_runtime_snapshot_unlocked(&mut core, terminal_id, snapshot);
        AIRuntimeStateMutation {
            did_change,
            completion: did_change
                .then(|| next_completion_event_unlocked(&mut core))
                .flatten(),
        }
    }

    fn reconcile_bridge_snapshot(
        &self,
        terminals: &[AIRuntimeTerminalState],
    ) -> AIRuntimeStateMutation {
        let Ok(mut core) = self.core.lock() else {
            return AIRuntimeStateMutation::default();
        };
        let now = now_seconds();
        let live_terminal_ids = terminals
            .iter()
            .map(|terminal| terminal.terminal_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut did_change = false;

        for terminal in terminals {
            let Some(existing) = core.sessions.get(&terminal.terminal_id).cloned() else {
                continue;
            };
            if existing.state != "responding" {
                continue;
            }
            if terminal.terminal_instance_id.is_some()
                && existing.terminal_instance_id != terminal.terminal_instance_id
            {
                core.sessions.remove(&terminal.terminal_id);
                did_change = true;
                continue;
            }
            if now - existing.updated_at > RUNNING_STALE_SECONDS {
                core.sessions.insert(
                    terminal.terminal_id.clone(),
                    mark_interrupted(existing, now),
                );
                did_change = true;
            }
        }

        let stale_ids = core
            .sessions
            .iter()
            .filter_map(|(terminal_id, session)| {
                (!live_terminal_ids.contains(terminal_id.as_str()) && session.state != "idle")
                    .then(|| terminal_id.clone())
            })
            .collect::<Vec<_>>();
        for terminal_id in stale_ids {
            if let Some(session) = core.sessions.get(&terminal_id).cloned() {
                core.sessions
                    .insert(terminal_id, mark_interrupted(session, now));
                did_change = true;
            }
        }

        AIRuntimeStateMutation {
            did_change,
            completion: did_change
                .then(|| next_completion_event_unlocked(&mut core))
                .flatten(),
        }
    }
}

fn apply_hook_unlocked(core: &mut AIRuntimeStateCore, event: AIHookEventPayload) -> bool {
    let Some(terminal_id) = normalized_string(Some(event.terminal_id.as_str())) else {
        return false;
    };
    let Some(tool) = canonical_tool_name(&event.tool) else {
        return false;
    };
    if !project_path_contains(
        event.project_path.as_deref(),
        event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.cwd.as_deref()),
    ) {
        return false;
    }

    let previous = core.sessions.get(&terminal_id).cloned();
    let terminal_instance_id = normalized_string(event.terminal_instance_id.as_deref());
    if previous
        .as_ref()
        .and_then(|session| session.terminal_instance_id.as_deref())
        .is_some()
        && terminal_instance_id.is_some()
        && previous
            .as_ref()
            .and_then(|session| session.terminal_instance_id.as_deref())
            != terminal_instance_id.as_deref()
        && event.updated_at
            < previous
                .as_ref()
                .map(|session| session.updated_at)
                .unwrap_or(0.0)
    {
        return false;
    }
    if is_tool_activity_without_loading(&event, previous.as_ref()) {
        return false;
    }

    let now = if event.updated_at > 0.0 {
        event.updated_at
    } else {
        now_seconds()
    };
    let should_reset = previous.as_ref().is_some_and(|session| {
        session.tool != tool
            || (session.terminal_instance_id.is_some()
                && terminal_instance_id.is_some()
                && session.terminal_instance_id != terminal_instance_id)
            || (session.ai_session_id.is_some()
                && normalized_string(event.ai_session_id.as_deref()).is_some()
                && session.ai_session_id != normalized_string(event.ai_session_id.as_deref()))
    });
    let base = if should_reset {
        None
    } else {
        previous.as_ref()
    };
    let ai_session_id = normalized_string(event.ai_session_id.as_deref())
        .or_else(|| base.and_then(|session| session.ai_session_id.clone()));
    let logical_key = ai_session_id
        .as_ref()
        .map(|session_id| format!("{tool}:{session_id}"));
    let total_tokens = number_or(base.map(|session| session.total_tokens), event.total_tokens);
    let cached_input_tokens = number_or(
        base.map(|session| session.cached_input_tokens),
        event.cached_input_tokens,
    );
    let baseline_total_tokens = base
        .map(|session| session.baseline_total_tokens)
        .or_else(|| {
            logical_key
                .as_ref()
                .and_then(|key| core.logical_baselines.get(key).copied())
        })
        .unwrap_or(total_tokens);
    let baseline_cached_input_tokens = base
        .map(|session| session.baseline_cached_input_tokens)
        .or_else(|| {
            logical_key
                .as_ref()
                .and_then(|key| core.logical_cached_baselines.get(key).copied())
        })
        .unwrap_or(cached_input_tokens);
    if let Some(key) = logical_key {
        core.logical_baselines
            .entry(key.clone())
            .or_insert(baseline_total_tokens);
        core.logical_cached_baselines
            .entry(key)
            .or_insert(baseline_cached_input_tokens);
    }

    let state = next_state(&event.kind, event.metadata.as_ref());
    let was_interrupted = if event.kind == "turnCompleted" || event.kind == "sessionEnded" {
        event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.was_interrupted)
            .unwrap_or(false)
    } else {
        base.map(|session| session.was_interrupted).unwrap_or(false)
    };
    let has_completed_turn = if event.kind == "turnCompleted" {
        event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.has_completed_turn)
            .unwrap_or(true)
    } else if event.kind == "sessionEnded" {
        base.map(|session| session.has_completed_turn)
            .unwrap_or(false)
    } else {
        base.map(|session| session.has_completed_turn)
            .unwrap_or(false)
    };

    if event.kind == "sessionEnded" && base.is_some() && !base.unwrap().has_completed_turn {
        core.sessions.remove(&terminal_id);
        return true;
    }

    let active_turn_started_at = if state == "responding" || state == "needsInput" {
        base.and_then(|session| session.active_turn_started_at)
            .or(Some(now))
    } else {
        None
    };
    if let Some(started_at) = active_turn_started_at {
        note_latest_active_started_at(core, &event.project_id, started_at);
    }

    let next = AISessionSnapshot {
        terminal_id: terminal_id.clone(),
        terminal_instance_id: terminal_instance_id
            .or_else(|| base.and_then(|session| session.terminal_instance_id.clone())),
        project_id: event.project_id.clone(),
        project_name: if event.project_name.trim().is_empty() {
            base.map(|session| session.project_name.clone())
                .unwrap_or_else(|| "Workspace".to_string())
        } else {
            event.project_name.clone()
        },
        project_path: normalized_string(event.project_path.as_deref())
            .or_else(|| base.and_then(|session| session.project_path.clone())),
        session_title: if event.session_title.trim().is_empty() {
            base.map(|session| session.session_title.clone())
                .unwrap_or_else(|| "Terminal".to_string())
        } else {
            event.session_title.clone()
        },
        tool,
        ai_session_id,
        model: normalized_string(event.model.as_deref())
            .or_else(|| base.and_then(|session| session.model.clone())),
        state: state.to_string(),
        status: status_for_state(&state).to_string(),
        is_running: state == "responding",
        input_tokens: number_or(base.map(|session| session.input_tokens), event.input_tokens),
        output_tokens: number_or(
            base.map(|session| session.output_tokens),
            event.output_tokens,
        ),
        cached_input_tokens,
        total_tokens,
        baseline_total_tokens,
        baseline_cached_input_tokens,
        baseline_resolved: base
            .map(|session| session.baseline_resolved)
            .unwrap_or(false),
        started_at: base.and_then(|session| session.started_at).or(Some(now)),
        updated_at: base
            .map(|session| session.updated_at)
            .unwrap_or(0.0)
            .max(now),
        active_turn_started_at,
        runtime_turn_started_at: if state == "responding" {
            base.and_then(|session| session.runtime_turn_started_at)
        } else {
            None
        },
        has_completed_turn,
        was_interrupted,
        transcript_path: event
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.transcript_path.as_deref()))
            .or_else(|| base.and_then(|session| session.transcript_path.clone())),
        notification_type: event
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.notification_type.as_deref())),
        target_tool_name: event
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.target_tool_name.as_deref())),
        message: event
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.message.as_deref())),
        latest_assistant_preview: if state == "idle" {
            None
        } else {
            base.and_then(|session| session.latest_assistant_preview.clone())
        },
    };

    if previous.as_ref() == Some(&next) {
        return false;
    }
    core.sessions.insert(terminal_id, next);
    true
}

fn apply_runtime_snapshot_unlocked(
    core: &mut AIRuntimeStateCore,
    terminal_id: &str,
    snapshot: AIRuntimeContextSnapshot,
) -> bool {
    let Some(session) = core.sessions.get(terminal_id).cloned() else {
        return false;
    };
    let mut snapshot_updated_at = snapshot.updated_at.max(session.updated_at);
    let now = now_seconds();
    if snapshot.response_state.as_deref() == Some("responding")
        && now - session.updated_at >= RUNNING_STATE_RENEWAL_SECONDS
    {
        snapshot_updated_at = snapshot_updated_at.max(now);
    }

    let mut state = session.state.clone();
    let mut was_interrupted = session.was_interrupted;
    let mut has_completed_turn = session.has_completed_turn;
    let mut active_turn_started_at = session.active_turn_started_at;
    let mut runtime_turn_started_at = session.runtime_turn_started_at;
    let snapshot_is_newer = snapshot.updated_at > session.updated_at;
    let prompt_turn_started_at = session
        .active_turn_started_at
        .or(session.started_at)
        .unwrap_or(session.updated_at);

    if snapshot.response_state.as_deref() == Some("responding") {
        if session.state == "responding"
            || (!session.was_interrupted
                && !session.has_completed_turn
                && (snapshot_is_newer || session.state == "idle"))
            || snapshot_started_after_prompt_turn(&snapshot, prompt_turn_started_at)
        {
            state = "responding".to_string();
            was_interrupted = false;
            has_completed_turn = false;
            let started = runtime_turn_started_at_for_responding_snapshot(
                &snapshot,
                prompt_turn_started_at,
                snapshot_updated_at,
            );
            active_turn_started_at = Some(started);
            runtime_turn_started_at = Some(started);
        }
    } else if snapshot.response_state.as_deref() == Some("idle")
        && (session.state == "responding"
            || session.state == "needsInput"
            || snapshot.was_interrupted
            || snapshot.has_completed_turn)
    {
        let turn_completed_at = snapshot.completed_at.or_else(|| {
            (snapshot.was_interrupted || snapshot.has_completed_turn).then_some(snapshot.updated_at)
        });
        let can_resolve_idle = if snapshot.was_interrupted || snapshot.has_completed_turn {
            turn_completed_at
                .map(|completed_at| completed_at >= prompt_turn_started_at)
                .unwrap_or(false)
        } else if session.state == "needsInput" {
            true
        } else if let Some(observed_started_at) = session.runtime_turn_started_at {
            observed_started_at >= prompt_turn_started_at
                && snapshot.updated_at >= observed_started_at
        } else {
            false
        };
        if can_resolve_idle {
            state = "idle".to_string();
            active_turn_started_at = None;
            runtime_turn_started_at = None;
            was_interrupted = snapshot.was_interrupted;
            has_completed_turn = snapshot.has_completed_turn || !was_interrupted;
        }
    }

    if let Some(started_at) = active_turn_started_at {
        note_latest_active_started_at(core, &session.project_id, started_at);
    }

    let (baseline_total_tokens, baseline_cached_input_tokens, baseline_resolved) =
        if session.baseline_resolved {
            (
                session.baseline_total_tokens,
                session.baseline_cached_input_tokens,
                true,
            )
        } else if snapshot.session_origin == "restored" {
            (
                snapshot.total_tokens.max(0),
                snapshot.cached_input_tokens.max(0),
                true,
            )
        } else {
            (
                session.baseline_total_tokens,
                session.baseline_cached_input_tokens,
                true,
            )
        };

    let next = AISessionSnapshot {
        tool: canonical_tool_name(&snapshot.tool).unwrap_or(session.tool.clone()),
        ai_session_id: normalized_string(snapshot.external_session_id.as_deref())
            .or(session.ai_session_id.clone()),
        transcript_path: normalized_string(snapshot.transcript_path.as_deref())
            .or(session.transcript_path.clone()),
        model: normalized_string(snapshot.model.as_deref()).or(session.model.clone()),
        state: state.clone(),
        status: status_for_state(&state).to_string(),
        is_running: state == "responding",
        input_tokens: session.input_tokens.max(snapshot.input_tokens.max(0)),
        output_tokens: session.output_tokens.max(snapshot.output_tokens.max(0)),
        cached_input_tokens: session
            .cached_input_tokens
            .max(snapshot.cached_input_tokens.max(0)),
        total_tokens: session.total_tokens.max(snapshot.total_tokens.max(0)),
        baseline_total_tokens,
        baseline_cached_input_tokens,
        baseline_resolved,
        updated_at: snapshot_updated_at,
        active_turn_started_at,
        runtime_turn_started_at,
        was_interrupted,
        has_completed_turn,
        latest_assistant_preview: normalized_string(snapshot.assistant_preview.as_deref())
            .or(session.latest_assistant_preview.clone()),
        ..session
    };

    if let Some(ai_session_id) = next.ai_session_id.as_ref() {
        let key = format!("{}:{ai_session_id}", next.tool);
        core.logical_baselines
            .entry(key.clone())
            .or_insert(next.baseline_total_tokens);
        core.logical_cached_baselines
            .entry(key)
            .or_insert(next.baseline_cached_input_tokens);
    }

    if core.sessions.get(terminal_id) == Some(&next) {
        return false;
    }
    core.sessions.insert(terminal_id.to_string(), next);
    true
}

fn snapshot_started_after_prompt_turn(
    snapshot: &AIRuntimeContextSnapshot,
    prompt_turn_started_at: f64,
) -> bool {
    snapshot
        .started_at
        .map(|started_at| started_at >= prompt_turn_started_at)
        .unwrap_or(false)
}

fn runtime_turn_started_at_for_responding_snapshot(
    snapshot: &AIRuntimeContextSnapshot,
    prompt_turn_started_at: f64,
    fallback: f64,
) -> f64 {
    if let Some(started_at) = snapshot.started_at {
        if started_at >= prompt_turn_started_at {
            return started_at;
        }
    }
    snapshot.updated_at.max(fallback)
}

fn resolve_hook_event(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    match canonical_tool_name(&event.tool).as_deref() {
        Some("codex") => resolve_codex_hook_event(event, current_session),
        Some("claude") => resolve_claude_hook_event(event, current_session),
        Some("gemini") => resolve_gemini_hook_event(event, current_session),
        Some("kiro") => resolve_kiro_hook_event(event, current_session),
        _ => with_fallback(event, current_session),
    }
}

fn resolve_codex_hook_event(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    let fallback = matching_fallback_session(&event, current_session);
    let resolved = with_fallback(event, fallback);
    if resolved.kind != "turnCompleted"
        || resolved
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.transcript_path.as_deref()))
            .is_none()
    {
        return resolved;
    }
    let request = AIRuntimeProbeRequest {
        terminal_id: resolved.terminal_id.clone(),
        terminal_instance_id: resolved.terminal_instance_id.clone(),
        project_id: resolved.project_id.clone(),
        project_path: resolved.project_path.clone(),
        tool: "codex".to_string(),
        external_session_id: normalized_string(resolved.ai_session_id.as_deref())
            .or_else(|| fallback.and_then(|session| session.ai_session_id.clone())),
        transcript_path: resolved
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.transcript_path.as_deref())),
        started_at: fallback
            .and_then(|session| session.started_at)
            .or(Some(resolved.updated_at)),
        updated_at: resolved.updated_at,
    };
    probe_codex_runtime(&request)
        .map(|snapshot| merge_snapshot_into_hook(resolved.clone(), snapshot, fallback))
        .unwrap_or(resolved)
}

fn resolve_claude_hook_event(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    let fallback = matching_fallback_session(&event, current_session);
    let resolved = with_fallback(event, fallback);
    if resolved.kind != "turnCompleted" {
        return resolved;
    }
    let external_session_id = normalized_string(resolved.ai_session_id.as_deref())
        .or_else(|| fallback.and_then(|session| session.ai_session_id.clone()));
    if normalized_string(resolved.project_path.as_deref()).is_none()
        || external_session_id.is_none()
    {
        return resolved;
    }
    let request = AIRuntimeProbeRequest {
        terminal_id: resolved.terminal_id.clone(),
        terminal_instance_id: resolved.terminal_instance_id.clone(),
        project_id: resolved.project_id.clone(),
        project_path: resolved.project_path.clone(),
        tool: "claude".to_string(),
        external_session_id: external_session_id.clone(),
        transcript_path: None,
        started_at: fallback
            .and_then(|session| session.started_at)
            .or(Some(resolved.updated_at)),
        updated_at: resolved.updated_at,
    };
    probe_claude_runtime(&request)
        .map(|snapshot| {
            merge_snapshot_into_hook(
                AIHookEventPayload {
                    ai_session_id: normalized_string(resolved.ai_session_id.as_deref())
                        .or(external_session_id),
                    ..resolved.clone()
                },
                snapshot,
                fallback,
            )
        })
        .unwrap_or(resolved)
}

fn resolve_gemini_hook_event(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    let fallback = matching_fallback_session(&event, current_session);
    let resolved = with_fallback(event, fallback);
    if normalized_string(resolved.project_path.as_deref()).is_none() {
        return resolved;
    }
    let request = AIRuntimeProbeRequest {
        terminal_id: resolved.terminal_id.clone(),
        terminal_instance_id: resolved.terminal_instance_id.clone(),
        project_id: resolved.project_id.clone(),
        project_path: resolved.project_path.clone(),
        tool: "gemini".to_string(),
        external_session_id: normalized_string(resolved.ai_session_id.as_deref())
            .or_else(|| fallback.and_then(|session| session.ai_session_id.clone())),
        transcript_path: None,
        started_at: fallback
            .and_then(|session| session.started_at)
            .or(Some(resolved.updated_at)),
        updated_at: resolved.updated_at,
    };
    probe_gemini_runtime(&request)
        .map(|snapshot| merge_snapshot_into_hook(resolved.clone(), snapshot, fallback))
        .unwrap_or(resolved)
}

fn resolve_kiro_hook_event(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    let fallback = matching_fallback_session(&event, current_session);
    let resolved = with_fallback(event, fallback);
    if normalized_string(resolved.project_path.as_deref()).is_none() {
        return resolved;
    }
    let request = AIRuntimeProbeRequest {
        terminal_id: resolved.terminal_id.clone(),
        terminal_instance_id: resolved.terminal_instance_id.clone(),
        project_id: resolved.project_id.clone(),
        project_path: resolved.project_path.clone(),
        tool: "kiro".to_string(),
        external_session_id: normalized_string(resolved.ai_session_id.as_deref())
            .or_else(|| fallback.and_then(|session| session.ai_session_id.clone())),
        transcript_path: None,
        started_at: fallback
            .and_then(|session| session.started_at)
            .or(Some(resolved.updated_at)),
        updated_at: resolved.updated_at,
    };
    probe_kiro_runtime(&request)
        .map(|snapshot| merge_snapshot_into_hook(resolved.clone(), snapshot, fallback))
        .unwrap_or(resolved)
}

fn matching_fallback_session<'a>(
    event: &AIHookEventPayload,
    current_session: Option<&'a AISessionSnapshot>,
) -> Option<&'a AISessionSnapshot> {
    let session = current_session?;
    if canonical_tool_name(&session.tool) != canonical_tool_name(&event.tool) {
        return None;
    }
    let incoming_session_id = normalized_string(event.ai_session_id.as_deref());
    if incoming_session_id.is_some() && session.ai_session_id != incoming_session_id {
        return None;
    }
    if event.kind == "sessionStarted" && incoming_session_id.is_none() {
        return None;
    }
    Some(session)
}

fn with_fallback(
    mut event: AIHookEventPayload,
    fallback: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    let Some(fallback) = fallback else {
        event.tool = canonical_tool_name(&event.tool).unwrap_or(event.tool);
        return event;
    };
    event.tool = canonical_tool_name(&event.tool).unwrap_or(event.tool);
    event.ai_session_id =
        normalized_string(event.ai_session_id.as_deref()).or(fallback.ai_session_id.clone());
    event.model = normalized_string(event.model.as_deref()).or(fallback.model.clone());
    event.total_tokens = event.total_tokens.or(Some(fallback.total_tokens));
    event
}

fn merge_snapshot_into_hook(
    event: AIHookEventPayload,
    snapshot: AIRuntimeContextSnapshot,
    fallback: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    let was_interrupted = snapshot.was_interrupted
        || event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.was_interrupted)
            .unwrap_or(false);
    let has_completed_turn = snapshot.has_completed_turn
        || event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.has_completed_turn)
            .unwrap_or(!was_interrupted);
    let mut metadata = event.metadata.clone().unwrap_or(AIHookEventMetadata {
        transcript_path: None,
        notification_type: None,
        source: None,
        reason: None,
        cwd: None,
        target_tool_name: None,
        message: None,
        was_interrupted: None,
        has_completed_turn: None,
    });
    metadata.was_interrupted = Some(was_interrupted);
    metadata.has_completed_turn = Some(has_completed_turn);
    AIHookEventPayload {
        kind: if snapshot.response_state.as_deref() == Some("responding") {
            "promptSubmitted".to_string()
        } else {
            event.kind
        },
        ai_session_id: normalized_string(event.ai_session_id.as_deref())
            .or_else(|| normalized_string(snapshot.external_session_id.as_deref()))
            .or_else(|| fallback.and_then(|session| session.ai_session_id.clone())),
        model: normalized_string(event.model.as_deref())
            .or_else(|| normalized_string(snapshot.model.as_deref()))
            .or_else(|| fallback.and_then(|session| session.model.clone())),
        input_tokens: Some(number_or(
            event
                .input_tokens
                .or_else(|| fallback.map(|session| session.input_tokens)),
            Some(snapshot.input_tokens),
        )),
        output_tokens: Some(number_or(
            event
                .output_tokens
                .or_else(|| fallback.map(|session| session.output_tokens)),
            Some(snapshot.output_tokens),
        )),
        cached_input_tokens: Some(number_or(
            event
                .cached_input_tokens
                .or_else(|| fallback.map(|session| session.cached_input_tokens)),
            Some(snapshot.cached_input_tokens),
        )),
        total_tokens: Some(
            event
                .total_tokens
                .unwrap_or(0)
                .max(fallback.map(|session| session.total_tokens).unwrap_or(0))
                .max(snapshot.total_tokens),
        ),
        updated_at: event
            .updated_at
            .max(snapshot.completed_at.unwrap_or(0.0))
            .max(snapshot.updated_at),
        metadata: Some(metadata),
        ..event
    }
}

fn state_snapshot_unlocked(core: &AIRuntimeStateCore) -> AIRuntimeStateSnapshot {
    let mut sessions = core.sessions.values().cloned().collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        right
            .updated_at
            .partial_cmp(&left.updated_at)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut project_ids = sessions
        .iter()
        .map(|session| session.project_id.clone())
        .collect::<Vec<_>>();
    project_ids.sort();
    project_ids.dedup();

    let projects = project_ids
        .iter()
        .map(|project_id| AIProjectStateSnapshot {
            project_id: project_id.clone(),
            project_phase: project_phase_unlocked(core, project_id),
            completed_phase: completed_phase_unlocked(core, project_id),
            totals: project_totals_unlocked(core, Some(project_id)),
        })
        .collect::<Vec<_>>();
    let needs_input_count = projects
        .iter()
        .filter(|project| matches!(project.project_phase, AIProjectPhase::NeedsInput { .. }))
        .count();
    let running_count = projects
        .iter()
        .filter(|project| matches!(project.project_phase, AIProjectPhase::Running { .. }))
        .count();
    let completion_count = projects
        .iter()
        .filter(|project| matches!(project.completed_phase, AIProjectPhase::Completed { .. }))
        .count();
    AIRuntimeStateSnapshot {
        sessions,
        projects,
        global_totals: project_totals_unlocked(core, None),
        needs_input_count,
        running_count,
        completion_count,
        latest_completion: latest_completion_unlocked(core),
        updated_at: now_seconds(),
    }
}

fn project_phase_unlocked(core: &AIRuntimeStateCore, project_id: &str) -> AIProjectPhase {
    let sessions = sorted_project_sessions(core, project_id);
    if let Some(session) = sessions
        .iter()
        .find(|session| session.state == "needsInput")
    {
        return AIProjectPhase::NeedsInput {
            tool: session.tool.clone(),
        };
    }
    if let Some(session) = sessions
        .iter()
        .find(|session| session.state == "responding")
    {
        return AIProjectPhase::Running {
            tool: session.tool.clone(),
        };
    }
    AIProjectPhase::Idle
}

fn completed_phase_unlocked(core: &AIRuntimeStateCore, project_id: &str) -> AIProjectPhase {
    let sessions = sorted_project_sessions(core, project_id);
    if sessions
        .iter()
        .any(|session| session.state == "needsInput" || session.state == "responding")
    {
        return AIProjectPhase::Idle;
    }
    let latest_active_started_at = core
        .latest_active_started_at_by_project
        .get(project_id)
        .copied()
        .unwrap_or(0.0);
    let completed = sessions.iter().find(|session| {
        session.state == "idle"
            && (session.has_completed_turn || session.was_interrupted)
            && session.updated_at >= latest_active_started_at
    });
    let Some(completed) = completed else {
        return AIProjectPhase::Idle;
    };
    let dismissed_at = core
        .dismissed_completed_at
        .get(project_id)
        .copied()
        .unwrap_or(0.0);
    if completed.updated_at <= dismissed_at {
        return AIProjectPhase::Idle;
    }
    AIProjectPhase::Completed {
        tool: completed.tool.clone(),
        was_interrupted: completed.was_interrupted,
        updated_at: completed.updated_at,
    }
}

fn project_totals_unlocked(core: &AIRuntimeStateCore, project_id: Option<&str>) -> AIProjectTotals {
    core.sessions
        .values()
        .filter(|session| {
            project_id
                .map(|project_id| session.project_id == project_id)
                .unwrap_or(true)
        })
        .fold(AIProjectTotals::default(), |mut total, session| {
            total.total_tokens += (session.total_tokens - session.baseline_total_tokens).max(0);
            total.cached_input_tokens +=
                (session.cached_input_tokens - session.baseline_cached_input_tokens).max(0);
            total.running += usize::from(session.state == "responding");
            total.needs_input += usize::from(session.state == "needsInput");
            total.completed += usize::from(session.has_completed_turn);
            total
        })
}

fn latest_completion_unlocked(core: &AIRuntimeStateCore) -> Option<AILatestCompletion> {
    let mut latest = None;
    for project_id in core
        .sessions
        .values()
        .map(|session| session.project_id.clone())
        .collect::<std::collections::HashSet<_>>()
    {
        let AIProjectPhase::Completed {
            tool,
            was_interrupted,
            updated_at,
        } = completed_phase_unlocked(core, &project_id)
        else {
            continue;
        };
        let project_name = core
            .sessions
            .values()
            .find(|session| session.project_id == project_id)
            .map(|session| session.project_name.clone())
            .unwrap_or_else(|| project_id.clone());
        let candidate = AILatestCompletion {
            id: format!("{project_id}:{updated_at}"),
            project_id,
            project_name,
            tool,
            was_interrupted,
            updated_at,
        };
        if latest
            .as_ref()
            .map(|current: &AILatestCompletion| candidate.updated_at > current.updated_at)
            .unwrap_or(true)
        {
            latest = Some(candidate);
        }
    }
    latest
}

fn next_completion_event_unlocked(
    core: &mut AIRuntimeStateCore,
) -> Option<AIRuntimeCompletionEvent> {
    let latest = latest_completion_unlocked(core)?;
    let notified_at = core
        .notified_completion_at
        .get(&latest.project_id)
        .copied()
        .unwrap_or(0.0);
    if latest.updated_at <= notified_at {
        return None;
    }
    core.notified_completion_at
        .insert(latest.project_id.clone(), latest.updated_at);
    let session = core
        .sessions
        .values()
        .filter(|session| session.project_id == latest.project_id)
        .filter(|session| session.state == "idle")
        .filter(|session| session.has_completed_turn || session.was_interrupted)
        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
        .cloned();
    Some(AIRuntimeCompletionEvent {
        id: latest.id,
        project_name: latest.project_name,
        tool: latest.tool,
        was_interrupted: latest.was_interrupted,
        session,
    })
}

fn sorted_project_sessions<'a>(
    core: &'a AIRuntimeStateCore,
    project_id: &str,
) -> Vec<&'a AISessionSnapshot> {
    let mut sessions = core
        .sessions
        .values()
        .filter(|session| session.project_id == project_id)
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        right
            .updated_at
            .partial_cmp(&left.updated_at)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sessions
}

fn probe_request_for_session(session: &AISessionSnapshot) -> AIRuntimeProbeRequest {
    AIRuntimeProbeRequest {
        terminal_id: session.terminal_id.clone(),
        terminal_instance_id: session.terminal_instance_id.clone(),
        project_id: session.project_id.clone(),
        project_path: session.project_path.clone(),
        tool: session.tool.clone(),
        external_session_id: session.ai_session_id.clone(),
        transcript_path: session.transcript_path.clone(),
        started_at: session.started_at,
        updated_at: session.updated_at,
    }
}

fn refresh_transcript_monitors(
    monitors: &Arc<Mutex<HashMap<String, TranscriptMonitor>>>,
    sessions: &[AISessionSnapshot],
) {
    let Ok(mut monitors) = monitors.lock() else {
        return;
    };
    let desired = sessions
        .iter()
        .filter_map(|session| {
            if canonical_tool_name(&session.tool).as_deref() != Some("codex") {
                return None;
            }
            let path = normalized_string(session.transcript_path.as_deref())?;
            Some((session.terminal_id.clone(), path))
        })
        .collect::<HashMap<_, _>>();
    monitors.retain(|terminal_id, _| desired.contains_key(terminal_id));
    for (terminal_id, path) in desired {
        if monitors
            .get(&terminal_id)
            .map(|monitor| monitor.path == path)
            .unwrap_or(false)
        {
            continue;
        }
        monitors.insert(
            terminal_id,
            TranscriptMonitor {
                signature: transcript_signature(Path::new(&path)),
                path,
                last_poll_at: None,
            },
        );
    }
}

fn scan_transcript_monitors(
    monitors: &mut HashMap<String, TranscriptMonitor>,
    now: f64,
) -> Vec<String> {
    let mut changed = Vec::new();
    for (terminal_id, monitor) in monitors.iter_mut() {
        let signature = transcript_signature(Path::new(&monitor.path));
        if signature == monitor.signature {
            continue;
        }
        if monitor
            .last_poll_at
            .map(|last_poll_at| now - last_poll_at < TRANSCRIPT_POLL_MINIMUM_SECONDS)
            .unwrap_or(false)
        {
            continue;
        }
        monitor.signature = signature;
        monitor.last_poll_at = Some(now);
        changed.push(terminal_id.clone());
    }
    changed
}

fn transcript_signature(path: &Path) -> Option<TranscriptSignature> {
    let metadata = fs::metadata(path).ok()?;
    let modified_millis = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    Some(TranscriptSignature {
        size: metadata.len(),
        modified_millis,
    })
}

fn should_poll_session(session: &AISessionSnapshot, reason: &str, now: f64) -> bool {
    if reason == "transcript-tail" && is_codex_transcript_session(session) {
        return true;
    }
    if canonical_tool_name(&session.tool).as_deref() == Some("codex")
        && normalized_string(session.transcript_path.as_deref()).is_some()
        && reason == "interval"
        && now - session.updated_at < CODEX_INTERVAL_POLL_MINIMUM_SECONDS
    {
        return false;
    }
    session.state == "responding" || session.state == "needsInput" || !session.has_completed_turn
}

fn is_codex_transcript_session(session: &AISessionSnapshot) -> bool {
    canonical_tool_name(&session.tool).as_deref() == Some("codex")
        && normalized_string(session.transcript_path.as_deref()).is_some()
}

fn next_state(kind: &str, metadata: Option<&AIHookEventMetadata>) -> &'static str {
    match kind {
        "promptSubmitted" => "responding",
        "sessionStarted" => "idle",
        "memoryRefreshing" => "responding",
        "needsInput" => "needsInput",
        "turnCompleted" | "sessionEnded" => "idle",
        _ if metadata
            .and_then(|metadata| metadata.notification_type.as_deref())
            .and_then(|value| normalized_string(Some(value)))
            .is_some() =>
        {
            "needsInput"
        }
        _ => "idle",
    }
}

fn status_for_state(state: &str) -> &'static str {
    match state {
        "responding" => "running",
        "needsInput" => "needs-input",
        _ => "idle",
    }
}

fn mark_interrupted(session: AISessionSnapshot, updated_at: f64) -> AISessionSnapshot {
    AISessionSnapshot {
        state: "idle".to_string(),
        status: "idle".to_string(),
        is_running: false,
        was_interrupted: true,
        has_completed_turn: false,
        active_turn_started_at: None,
        runtime_turn_started_at: None,
        updated_at,
        ..session
    }
}

fn is_tool_activity_without_loading(
    event: &AIHookEventPayload,
    previous: Option<&AISessionSnapshot>,
) -> bool {
    if event.kind != "promptSubmitted"
        || event
            .metadata
            .as_ref()
            .and_then(|metadata| normalized_string(metadata.source.as_deref()))
            .as_deref()
            != Some("tool-use")
    {
        return false;
    }
    previous
        .map(|session| session.has_completed_turn || session.was_interrupted)
        .unwrap_or(true)
}

fn note_latest_active_started_at(core: &mut AIRuntimeStateCore, project_id: &str, started_at: f64) {
    let previous = core
        .latest_active_started_at_by_project
        .get(project_id)
        .copied()
        .unwrap_or(0.0);
    if started_at > previous {
        core.latest_active_started_at_by_project
            .insert(project_id.to_string(), started_at);
    }
}

fn project_path_contains(project_path: Option<&str>, cwd: Option<&str>) -> bool {
    let Some(project) = normalize_path_string(project_path) else {
        return true;
    };
    let Some(current) = normalize_path_string(cwd) else {
        return true;
    };
    current == project || current.starts_with(&format!("{project}/"))
}

fn normalize_path_string(path: Option<&str>) -> Option<String> {
    normalized_string(path).map(|value| value.trim_end_matches('/').to_string())
}

fn number_or(previous: Option<i64>, value: Option<i64>) -> i64 {
    value
        .map(|value| value.max(0))
        .unwrap_or(previous.unwrap_or(0))
}

#[cfg(unix)]
fn handle_runtime_stream(mut stream: UnixStream, hook_tx: Sender<AIRuntimeSupervisorMessage>) {
    let mut buffer = Vec::new();
    let _ = stream.read_to_end(&mut buffer);
    let _ = stream.shutdown(Shutdown::Both);
    if buffer.is_empty() {
        return;
    }
    let _ = hook_tx.blocking_send(AIRuntimeSupervisorMessage::HookFrame(buffer));
}

fn runtime_frame_to_hook(buffer: &[u8]) -> Option<AIHookEventPayload> {
    let buffer = buffer.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(buffer);
    let envelope = serde_json::from_slice::<RuntimeEnvelope>(buffer).ok()?;
    let payload = match envelope.kind.as_str() {
        "ai-hook" => serde_json::from_value::<AIHookEventPayload>(envelope.payload).ok(),
        "opencode-runtime" => serde_json::from_value::<AIToolUsageEnvelope>(envelope.payload)
            .ok()
            .and_then(opencode_runtime_to_hook),
        _ => None,
    };
    payload
}

fn opencode_runtime_to_hook(envelope: AIToolUsageEnvelope) -> Option<AIHookEventPayload> {
    if envelope.session_id.trim().is_empty() || envelope.project_id.trim().is_empty() {
        return None;
    }

    let response_state = envelope.response_state.as_deref();
    let (kind, metadata) = match response_state {
        Some("responding") => ("promptSubmitted", None),
        Some("idle") if envelope.status == "completed" => (
            "turnCompleted",
            Some(opencode_runtime_metadata(&envelope.status, false, true)),
        ),
        Some("idle") => (
            "turnCompleted",
            Some(opencode_runtime_metadata(&envelope.status, true, false)),
        ),
        _ if envelope.status == "running" => ("promptSubmitted", None),
        _ => ("turnCompleted", None),
    };

    Some(AIHookEventPayload {
        kind: kind.to_string(),
        terminal_id: envelope.session_id,
        terminal_instance_id: envelope.session_instance_id,
        project_id: envelope.project_id,
        project_name: envelope.project_name,
        project_path: envelope.project_path,
        session_title: envelope.session_title,
        tool: envelope.tool,
        ai_session_id: envelope.external_session_id,
        model: envelope.model,
        input_tokens: envelope.input_tokens,
        output_tokens: envelope.output_tokens,
        cached_input_tokens: envelope.cached_input_tokens,
        total_tokens: envelope.total_tokens,
        updated_at: envelope.updated_at,
        metadata,
    })
}

fn opencode_runtime_metadata(
    status: &str,
    was_interrupted: bool,
    has_completed_turn: bool,
) -> AIHookEventMetadata {
    AIHookEventMetadata {
        transcript_path: None,
        notification_type: None,
        source: Some("opencode-runtime".to_string()),
        reason: Some(status.to_string()),
        cwd: None,
        target_tool_name: None,
        message: None,
        was_interrupted: Some(was_interrupted),
        has_completed_turn: Some(has_completed_turn),
    }
}

fn canonical_tool_name(tool: &str) -> Option<String> {
    let normalized = normalized_string(Some(tool))?.to_lowercase();
    match normalized.as_str() {
        "claude-code" => Some("claude".to_string()),
        "agy" => Some("gemini".to_string()),
        _ => Some(normalized),
    }
}

fn probe_codex_runtime(request: &AIRuntimeProbeRequest) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let file_path = normalized_string(request.transcript_path.as_deref())
        .map(PathBuf::from)
        .or_else(|| {
            let external_id = normalized_string(request.external_session_id.as_deref())?;
            find_codex_rollout_path(&project_path, &external_id)
        })?;
    let transcript_path = file_path.display().to_string();
    let parsed = request
        .started_at
        .and_then(|started_at| {
            parse_codex_runtime_state_tail(
                &file_path,
                Some(&project_path),
                started_at,
                request.updated_at,
            )
        })
        .or_else(|| parse_codex_runtime_state(&file_path, Some(&project_path)))?;
    Some(AIRuntimeContextSnapshot {
        tool: "codex".to_string(),
        external_session_id: normalized_string(request.external_session_id.as_deref()),
        transcript_path: Some(transcript_path),
        model: parsed.model,
        assistant_preview: parsed.assistant_preview,
        input_tokens: parsed.input_tokens.unwrap_or(0),
        output_tokens: parsed.output_tokens.unwrap_or(0),
        cached_input_tokens: parsed.cached_input_tokens.unwrap_or(0),
        total_tokens: parsed.total_tokens.unwrap_or(0),
        updated_at: parsed.updated_at.unwrap_or(request.updated_at),
        started_at: parsed.started_at,
        completed_at: parsed.completed_at,
        response_state: parsed.response_state,
        was_interrupted: parsed.was_interrupted,
        has_completed_turn: parsed.has_completed_turn,
        session_origin: parsed.origin,
        source: "probe".to_string(),
    })
}

fn probe_claude_runtime(request: &AIRuntimeProbeRequest) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let external_id = normalized_string(request.external_session_id.as_deref())?;
    let file_urls = claude_project_log_paths(&project_path);
    let mut aggregate: Option<ClaudeAggregate> = None;
    for file_url in file_urls {
        let Some(next) = parse_claude_log_runtime_state(&file_url, &project_path, &external_id)
        else {
            continue;
        };
        aggregate = Some(match aggregate {
            Some(existing) => existing.merge(next),
            None => next,
        });
    }
    let aggregate = aggregate?;
    let started_at = aggregate.started_at();
    let completed_at = aggregate.completed_at();
    let response_state = aggregate.response_state();
    let was_interrupted = aggregate.was_interrupted();
    let has_completed_turn = aggregate.has_completed_turn();
    Some(AIRuntimeContextSnapshot {
        tool: "claude".to_string(),
        external_session_id: Some(external_id),
        transcript_path: None,
        model: aggregate.model,
        assistant_preview: aggregate.assistant_preview,
        input_tokens: aggregate.input_tokens,
        output_tokens: aggregate.output_tokens,
        cached_input_tokens: aggregate.cached_input_tokens,
        total_tokens: aggregate.total_tokens,
        updated_at: aggregate.updated_at.max(request.updated_at),
        started_at,
        completed_at,
        response_state,
        was_interrupted,
        has_completed_turn,
        session_origin: "unknown".to_string(),
        source: "probe".to_string(),
    })
}

fn probe_gemini_runtime(request: &AIRuntimeProbeRequest) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let preferred_id = normalized_string(request.external_session_id.as_deref());
    let states = gemini_session_paths(&project_path)
        .into_iter()
        .take(16)
        .filter_map(|path| parse_gemini_runtime_state(&path))
        .collect::<Vec<_>>();
    if states.is_empty() {
        return None;
    }

    let mut preferred_match: Option<GeminiParsedState> = None;
    let mut current_launch_match: Option<GeminiParsedState> = None;
    let mut candidate_match: Option<GeminiParsedState> = None;
    for state in states {
        let is_current_launch = request
            .started_at
            .map(|started| state.started_at >= started)
            .unwrap_or(false);
        if preferred_id.as_deref() == Some(state.external_session_id.as_str()) {
            preferred_match = Some(state.clone());
        }
        if is_current_launch {
            if current_launch_match
                .as_ref()
                .map(|existing| state.updated_at > existing.updated_at)
                .unwrap_or(true)
            {
                current_launch_match = Some(state.clone());
            }
            continue;
        }
        if candidate_match
            .as_ref()
            .map(|existing| state.updated_at > existing.updated_at)
            .unwrap_or(true)
        {
            candidate_match = Some(state);
        }
    }

    let authoritative = preferred_id.is_some();
    let mut state = if authoritative {
        preferred_match?
    } else {
        current_launch_match.or(preferred_match).or_else(|| {
            if request.started_at.is_none() {
                candidate_match
            } else {
                None
            }
        })?
    };
    state.origin = if request
        .started_at
        .map(|started| state.started_at >= started)
        .unwrap_or(false)
    {
        "fresh".to_string()
    } else {
        "restored".to_string()
    };

    let has_completed_turn = state.response_state.as_deref() == Some("idle");
    Some(AIRuntimeContextSnapshot {
        tool: "gemini".to_string(),
        external_session_id: Some(state.external_session_id),
        transcript_path: None,
        model: state.model,
        assistant_preview: state.assistant_preview,
        input_tokens: state.input_tokens,
        output_tokens: state.output_tokens,
        cached_input_tokens: state.cached_input_tokens,
        total_tokens: state.total_tokens,
        updated_at: state.updated_at.max(request.updated_at),
        started_at: Some(state.started_at),
        completed_at: state.completed_at,
        response_state: state.response_state,
        was_interrupted: false,
        has_completed_turn,
        session_origin: state.origin,
        source: "probe".to_string(),
    })
}

fn probe_kiro_runtime(request: &AIRuntimeProbeRequest) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let external_session_id = normalized_string(request.external_session_id.as_deref())?;
    let file_path = find_kiro_session_path(&project_path, &external_session_id)?;
    let parsed = parse_kiro_runtime_state(&file_path, Some(&project_path))?;
    Some(AIRuntimeContextSnapshot {
        tool: "kiro".to_string(),
        external_session_id: Some(external_session_id),
        transcript_path: Some(file_path.display().to_string()),
        model: parsed.model,
        assistant_preview: parsed.assistant_preview,
        input_tokens: parsed.input_tokens,
        output_tokens: parsed.output_tokens,
        cached_input_tokens: parsed.cached_input_tokens,
        total_tokens: parsed.total_tokens,
        updated_at: parsed.updated_at.unwrap_or(request.updated_at),
        started_at: parsed.started_at,
        completed_at: parsed.completed_at,
        response_state: parsed.response_state,
        was_interrupted: parsed.was_interrupted,
        has_completed_turn: parsed.has_completed_turn,
        session_origin: parsed.origin,
        source: "probe".to_string(),
    })
}

fn probe_opencode_runtime(request: &AIRuntimeProbeRequest) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let external_session_id = normalized_string(request.external_session_id.as_deref())?;
    let database_path = home_dir()
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    if !database_path.exists() {
        return None;
    }
    let conn = rusqlite::Connection::open(&database_path).ok()?;
    let mut statement = conn
        .prepare(
            r#"
            SELECT m.data, m.time_created, s.time_updated, COALESCE(s.directory, '')
            FROM session s
            LEFT JOIN message m ON m.session_id = s.id
            WHERE s.id = ?1
              AND s.time_archived IS NULL
            ORDER BY m.time_created DESC;
            "#,
        )
        .ok()?;
    let rows = statement
        .query_map([external_session_id.as_str()], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .ok()?;

    let mut had_row = false;
    let mut latest_model = None;
    let mut input_tokens = 0;
    let mut output_tokens = 0;
    let mut cached_input_tokens = 0;
    let mut total_tokens = 0;
    let mut updated_at = 0.0f64;
    let mut last_user_at = 0.0f64;
    let mut last_completion_at = 0.0f64;
    let mut assistant_preview = None;

    for row in rows.flatten() {
        let (data, message_created_at, session_updated_at, session_directory) = row;
        let payload = data
            .as_deref()
            .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
            .unwrap_or(serde_json::Value::Null);
        let root_path = payload
            .get("path")
            .and_then(|value| value.get("root"))
            .and_then(|value| value.as_str())
            .or(session_directory.as_deref());
        if !paths_equivalent(root_path, &project_path) {
            continue;
        }
        had_row = true;
        if latest_model.is_none() {
            latest_model = payload
                .get("modelID")
                .and_then(|value| value.as_str())
                .and_then(|value| normalized_string(Some(value)));
        }
        let tokens = payload.get("tokens").unwrap_or(&serde_json::Value::Null);
        let cache = tokens.get("cache").unwrap_or(&serde_json::Value::Null);
        let input = json_i64(tokens.get("input"));
        let output = json_i64(tokens.get("output"));
        let cache_read = json_i64(cache.get("read"));
        let reasoning = json_i64(tokens.get("reasoning"));
        input_tokens += input;
        output_tokens += output;
        cached_input_tokens += cache_read;
        total_tokens += input + output + reasoning;

        let created_at = payload
            .get("time")
            .and_then(|value| value.get("created"))
            .and_then(opencode_value_timestamp)
            .or_else(|| message_created_at.map(|value| value / 1000.0))
            .unwrap_or(0.0);
        let completed_at = payload
            .get("time")
            .and_then(|value| value.get("completed"))
            .and_then(opencode_value_timestamp);
        let role = payload
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let finish_reason = payload
            .get("finish")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if role == "user" {
            last_user_at = last_user_at.max(created_at);
        } else if role == "assistant" {
            if assistant_preview.is_none() {
                assistant_preview = opencode_assistant_preview(&payload);
            }
            if is_opencode_final_assistant_finish(finish_reason, completed_at) {
                last_completion_at = last_completion_at.max(completed_at.unwrap_or(created_at));
            }
        }
        updated_at = updated_at.max(created_at);
        updated_at = updated_at.max(completed_at.unwrap_or(0.0));
        updated_at = updated_at.max(session_updated_at.unwrap_or(0.0) / 1000.0);
    }

    if !had_row {
        return None;
    }
    let response_state = if last_user_at > 0.0 {
        if last_user_at > last_completion_at {
            Some("responding".to_string())
        } else {
            Some("idle".to_string())
        }
    } else if total_tokens > 0 {
        Some("idle".to_string())
    } else {
        None
    };
    let has_completed_turn = last_completion_at > 0.0 && last_completion_at >= last_user_at;
    Some(AIRuntimeContextSnapshot {
        tool: "opencode".to_string(),
        external_session_id: Some(external_session_id),
        transcript_path: Some(database_path.display().to_string()),
        model: latest_model,
        assistant_preview,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        total_tokens,
        updated_at: updated_at.max(request.updated_at),
        started_at: (last_user_at > 0.0).then_some(last_user_at),
        completed_at: has_completed_turn.then_some(last_completion_at),
        response_state,
        was_interrupted: false,
        has_completed_turn,
        session_origin: if total_tokens > 0 {
            "restored"
        } else {
            "fresh"
        }
        .to_string(),
        source: "probe".to_string(),
    })
}

fn is_opencode_final_assistant_finish(value: &str, completed_at: Option<f64>) -> bool {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        return completed_at.is_some();
    }
    normalized != "tool-calls"
}

fn opencode_assistant_preview(payload: &serde_json::Value) -> Option<String> {
    joined_preview_from_values(&[
        payload.get("content"),
        payload.get("text"),
        payload.get("message"),
        payload.get("parts"),
    ])
}

fn gemini_assistant_preview(message: &serde_json::Value) -> Option<String> {
    joined_preview_from_values(&[
        message.get("content"),
        message.get("text"),
        message.get("message"),
        message.get("parts"),
    ])
}

fn opencode_value_timestamp(value: &serde_json::Value) -> Option<f64> {
    let raw = value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .or_else(|| value.as_f64().map(|value| value.to_string()))?;
    if let Ok(milliseconds) = raw.parse::<f64>() {
        return Some(milliseconds / 1000.0);
    }
    parse_iso8601_seconds(&raw)
}

fn runtime_root_dir() -> PathBuf {
    runtime_temp_dir().join("runtime-root")
}

fn runtime_live_log_path() -> PathBuf {
    runtime_temp_dir().join("live.log")
}

fn reset_runtime_live_log() {
    let path = runtime_live_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    rotate_runtime_log(&path);
    let _ = fs::write(path, "");
}

fn runtime_log_line(category: &str, message: &str) {
    let path = runtime_live_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    rotate_runtime_log(&path);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| format!("{:.3}", duration.as_secs_f64()))
        .unwrap_or_else(|_| "0.000".to_string());
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{timestamp}] [{category}] {message}");
    }
}

fn rotate_runtime_log(path: &Path) {
    const MAX_BYTES: u64 = 1_000_000;
    const ROTATION_COUNT: usize = 5;
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() <= MAX_BYTES {
        return;
    }
    for index in (1..=ROTATION_COUNT).rev() {
        let current = rotated_log_path(path, index);
        if !current.exists() {
            continue;
        }
        if index == ROTATION_COUNT {
            let _ = fs::remove_file(&current);
            continue;
        }
        let next = rotated_log_path(path, index + 1);
        let _ = fs::rename(current, next);
    }
    let _ = fs::rename(path, rotated_log_path(path, 1));
}

fn rotated_log_path(path: &Path, index: usize) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "live.log".to_string());
    path.with_file_name(format!("{file_name}.{index}"))
}

const RUNTIME_ASSETS: &[(&str, &[u8])] = &[
    (
        "scripts/shell-hooks/dmux-ai-hook.zsh",
        include_bytes!("../runtime-assets/scripts/shell-hooks/dmux-ai-hook.zsh"),
    ),
    (
        "scripts/shell-hooks/zsh/.zlogin",
        include_bytes!("../runtime-assets/scripts/shell-hooks/zsh/.zlogin"),
    ),
    (
        "scripts/shell-hooks/zsh/.zprofile",
        include_bytes!("../runtime-assets/scripts/shell-hooks/zsh/.zprofile"),
    ),
    (
        "scripts/shell-hooks/zsh/.zshenv",
        include_bytes!("../runtime-assets/scripts/shell-hooks/zsh/.zshenv"),
    ),
    (
        "scripts/shell-hooks/zsh/.zshrc",
        include_bytes!("../runtime-assets/scripts/shell-hooks/zsh/.zshrc"),
    ),
    (
        "scripts/wrappers/bin/claude",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/claude"),
    ),
    (
        "scripts/wrappers/bin/claude-code",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/claude-code"),
    ),
    (
        "scripts/wrappers/bin/claude-code.cmd",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/claude-code.cmd"),
    ),
    (
        "scripts/wrappers/bin/claude.cmd",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/claude.cmd"),
    ),
    (
        "scripts/wrappers/bin/codex",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/codex"),
    ),
    (
        "scripts/wrappers/bin/codex.cmd",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/codex.cmd"),
    ),
    (
        "scripts/wrappers/bin/codux-ssh",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/codux-ssh"),
    ),
    (
        "scripts/wrappers/bin/codux-ssh.cmd",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/codux-ssh.cmd"),
    ),
    (
        "scripts/wrappers/codux-ssh-expect.exp",
        include_bytes!("../runtime-assets/scripts/wrappers/codux-ssh-expect.exp"),
    ),
    (
        "scripts/wrappers/codux-ssh.ps1",
        include_bytes!("../runtime-assets/scripts/wrappers/codux-ssh.ps1"),
    ),
    (
        "scripts/wrappers/bin/gemini",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/gemini"),
    ),
    (
        "scripts/wrappers/bin/gemini.cmd",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/gemini.cmd"),
    ),
    (
        "scripts/wrappers/bin/agy",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/agy"),
    ),
    (
        "scripts/wrappers/bin/agy.cmd",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/agy.cmd"),
    ),
    (
        "scripts/wrappers/bin/opencode",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/opencode"),
    ),
    (
        "scripts/wrappers/bin/opencode.cmd",
        include_bytes!("../runtime-assets/scripts/wrappers/bin/opencode.cmd"),
    ),
    (
        "scripts/wrappers/dmux-ai-state.cmd",
        include_bytes!("../runtime-assets/scripts/wrappers/dmux-ai-state.cmd"),
    ),
    (
        "scripts/wrappers/dmux-ai-state.ps1",
        include_bytes!("../runtime-assets/scripts/wrappers/dmux-ai-state.ps1"),
    ),
    (
        "scripts/wrappers/dmux-ai-state.sh",
        include_bytes!("../runtime-assets/scripts/wrappers/dmux-ai-state.sh"),
    ),
    (
        "scripts/wrappers/opencode-config/package.json",
        include_bytes!("../runtime-assets/scripts/wrappers/opencode-config/package.json"),
    ),
    (
        "scripts/wrappers/opencode-config/plugins/dmux-runtime.js",
        include_bytes!(
            "../runtime-assets/scripts/wrappers/opencode-config/plugins/dmux-runtime.js"
        ),
    ),
    (
        "scripts/wrappers/tool-wrapper.cmd",
        include_bytes!("../runtime-assets/scripts/wrappers/tool-wrapper.cmd"),
    ),
    (
        "scripts/wrappers/tool-wrapper.ps1",
        include_bytes!("../runtime-assets/scripts/wrappers/tool-wrapper.ps1"),
    ),
    (
        "scripts/wrappers/tool-wrapper.sh",
        include_bytes!("../runtime-assets/scripts/wrappers/tool-wrapper.sh"),
    ),
];

fn runtime_asset_content(relative_path: &str) -> Option<&'static [u8]> {
    RUNTIME_ASSETS
        .iter()
        .find_map(|(path, content)| (*path == relative_path).then_some(*content))
}

fn install_tool_hooks(
    path: &Path,
    tool: &str,
    definitions: &[(&str, &str, i64, bool)],
    runtime: &AIRuntimeBridge,
) -> Result<(), String> {
    if tool == "kiro" {
        return install_kiro_tool_hooks(path, definitions, runtime);
    }

    let mut root = load_json_object(path)?;
    let mut hooks = root
        .remove("hooks")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    for (event_key, action) in removed_hook_definitions(tool) {
        strip_managed_action_from_hooks(&mut hooks, event_key, action, Some(tool));
    }
    if tool == "claude" {
        strip_managed_action_from_hooks(&mut hooks, "Notification", "notification", Some("claude"));
    }

    for (event_key, action, timeout, is_async) in definitions {
        let command = hook_command(runtime.managed_hook_script(), action, app_slug(), tool);
        let mut hook = serde_json::Map::new();
        hook.insert(
            "type".to_string(),
            serde_json::Value::String("command".to_string()),
        );
        hook.insert("command".to_string(), serde_json::Value::String(command));
        hook.insert(
            "timeout".to_string(),
            serde_json::Value::Number((*timeout).into()),
        );
        hook.insert(
            "statusMessage".to_string(),
            serde_json::Value::String(format!("codux {tool} live")),
        );
        if *is_async {
            hook.insert("async".to_string(), serde_json::Value::Bool(true));
        }

        let groups = hooks
            .remove(*event_key)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        let mut cleaned = Vec::new();
        for group in groups {
            let Some(group_object) = group.as_object() else {
                continue;
            };
            let existing_hooks = group_object
                .get("hooks")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            let next_hooks: Vec<serde_json::Value> = existing_hooks
                .into_iter()
                .filter(|item| !is_managed_hook(item, action, tool))
                .collect();
            if next_hooks.is_empty() {
                continue;
            }
            let mut next_group = group_object.clone();
            next_group.insert("hooks".to_string(), serde_json::Value::Array(next_hooks));
            cleaned.push(serde_json::Value::Object(next_group));
        }

        cleaned.push(serde_json::json!({
            "matcher": "",
            "hooks": [serde_json::Value::Object(hook)],
        }));
        hooks.insert((*event_key).to_string(), serde_json::Value::Array(cleaned));
    }

    root.insert("hooks".to_string(), serde_json::Value::Object(hooks));
    if tool == "gemini" || tool == "agy" {
        disable_gemini_hook_notifications(&mut root);
    }
    write_json_object(path, root)
}

fn install_kiro_tool_hooks(
    path: &Path,
    definitions: &[(&str, &str, i64, bool)],
    runtime: &AIRuntimeBridge,
) -> Result<(), String> {
    let mut root = load_json_object(path)?;
    ensure_kiro_agent_config_fields(&mut root);
    let mut hooks = root
        .remove("hooks")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    for (event_key, action, timeout, is_async) in definitions {
        let command = hook_command(runtime.managed_hook_script(), action, app_slug(), "kiro");
        let mut hook = serde_json::Map::new();
        hook.insert("command".to_string(), serde_json::Value::String(command));
        hook.insert(
            "timeout_ms".to_string(),
            serde_json::Value::Number((*timeout).into()),
        );
        hook.insert(
            "matcher".to_string(),
            serde_json::Value::String(String::new()),
        );
        if *is_async {
            hook.insert("async".to_string(), serde_json::Value::Bool(true));
        }

        let entries = hooks
            .remove(*event_key)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        let mut cleaned = Vec::new();
        for entry in entries {
            if is_managed_hook(&entry, action, "kiro") {
                continue;
            }
            let Some(entry_object) = entry.as_object() else {
                continue;
            };
            if let Some(existing_hooks) =
                entry_object.get("hooks").and_then(|value| value.as_array())
            {
                let next_hooks = existing_hooks
                    .iter()
                    .filter(|item| !is_managed_hook(item, action, "kiro"))
                    .cloned()
                    .collect::<Vec<_>>();
                if next_hooks.is_empty() {
                    continue;
                }
                let mut next_entry = entry_object.clone();
                next_entry.insert("hooks".to_string(), serde_json::Value::Array(next_hooks));
                cleaned.push(serde_json::Value::Object(next_entry));
                continue;
            }
            cleaned.push(entry);
        }

        cleaned.push(serde_json::Value::Object(hook));
        hooks.insert((*event_key).to_string(), serde_json::Value::Array(cleaned));
    }

    root.insert("hooks".to_string(), serde_json::Value::Object(hooks));
    write_json_object(path, root)
}

fn ensure_kiro_agent_config_fields(root: &mut serde_json::Map<String, serde_json::Value>) {
    ensure_json_string_field(root, "name", "Codux Managed");
    ensure_json_string_field(root, "description", "Codux runtime lifecycle hook bridge.");
    ensure_json_string_field(root, "prompt", "Codux managed runtime hook agent.");
}

fn ensure_json_string_field(
    root: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    default_value: &str,
) {
    let is_valid = root
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if !is_valid {
        root.insert(
            key.to_string(),
            serde_json::Value::String(default_value.to_string()),
        );
    }
}

fn disable_gemini_hook_notifications(root: &mut serde_json::Map<String, serde_json::Value>) {
    let mut hooks_config = root
        .remove("hooksConfig")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    hooks_config.insert("notifications".to_string(), serde_json::Value::Bool(false));
    root.insert(
        "hooksConfig".to_string(),
        serde_json::Value::Object(hooks_config),
    );
}

fn tool_hook_config_status(
    path: &Path,
    tool: &str,
    definitions: &[(&str, &str)],
) -> AIRuntimeToolHookConfigStatus {
    let root = load_json_object(path).unwrap_or_default();
    let hooks = root
        .get("hooks")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let missing = definitions
        .iter()
        .filter_map(|(event_key, action)| {
            has_managed_hook_for_event(&hooks, event_key, action, tool)
                .then_some(())
                .is_none()
                .then(|| format!("{event_key}:{action}"))
        })
        .collect::<Vec<_>>();
    AIRuntimeToolHookConfigStatus {
        configured: missing.is_empty(),
        config_path: path.display().to_string(),
        missing,
    }
}

fn has_managed_hook_for_event(
    hooks: &serde_json::Map<String, serde_json::Value>,
    event_key: &str,
    action: &str,
    tool: &str,
) -> bool {
    hooks
        .get(event_key)
        .and_then(|value| value.as_array())
        .map(|groups| {
            groups.iter().any(|group| {
                is_managed_hook(group, action, tool)
                    || group
                        .get("hooks")
                        .and_then(|value| value.as_array())
                        .map(|items| items.iter().any(|item| is_managed_hook(item, action, tool)))
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn opencode_hook_config_status(config_dir: &Path) -> AIRuntimeToolHookConfigStatus {
    let expected = [
        "package.json",
        "plugins/dmux-runtime.js",
        "node_modules/@opencode-ai/plugin/package.json",
    ];
    let missing = expected
        .iter()
        .filter(|relative| !config_dir.join(relative).exists())
        .map(|relative| relative.to_string())
        .collect::<Vec<_>>();
    AIRuntimeToolHookConfigStatus {
        configured: missing.is_empty(),
        config_path: config_dir.display().to_string(),
        missing,
    }
}

fn removed_hook_definitions(tool: &str) -> &'static [(&'static str, &'static str)] {
    match tool {
        "codex" => &[
            ("PreToolUse", "codex-pre-tool-use"),
            ("PostToolUse", "codex-post-tool-use"),
            ("SessionEnd", "codex-session-end"),
        ],
        "claude" => &[
            ("PreToolUse", "pre-tool-use"),
            ("PostToolUse", "post-tool-use"),
            ("PostToolUseFailure", "post-tool-use-failure"),
        ],
        _ => &[],
    }
}

fn strip_managed_action_from_hooks(
    hooks: &mut serde_json::Map<String, serde_json::Value>,
    event_key: &str,
    action: &str,
    tool: Option<&str>,
) {
    let groups = hooks
        .remove(event_key)
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    if groups.is_empty() {
        return;
    }

    let mut cleaned_groups = Vec::new();
    for group in groups {
        let Some(group_object) = group.as_object() else {
            continue;
        };
        let existing_hooks = group_object
            .get("hooks")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let next_hooks = existing_hooks
            .into_iter()
            .filter(|item| !is_managed_hook_action(item, action, tool))
            .collect::<Vec<_>>();
        if next_hooks.is_empty() {
            continue;
        }
        let mut next_group = group_object.clone();
        next_group.insert("hooks".to_string(), serde_json::Value::Array(next_hooks));
        cleaned_groups.push(serde_json::Value::Object(next_group));
    }

    if !cleaned_groups.is_empty() {
        hooks.insert(
            event_key.to_string(),
            serde_json::Value::Array(cleaned_groups),
        );
    }
}

#[derive(Debug, Clone)]
struct CodexHookTrustState {
    key: String,
    trusted_hash: String,
}

fn ensure_codex_config_installed(hooks_path: &Path) -> Result<(), String> {
    let config_path = home_dir().join(".codex").join("config.toml");
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let updated =
        updated_codex_config_text(&existing, &managed_codex_hook_trust_states(hooks_path)?);
    if existing == updated {
        return Ok(());
    }
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(config_path, updated).map_err(|error| error.to_string())
}

fn managed_codex_hook_trust_states(hooks_path: &Path) -> Result<Vec<CodexHookTrustState>, String> {
    let root = load_json_object(hooks_path)?;
    let Some(hooks) = root.get("hooks").and_then(|value| value.as_object()) else {
        return Ok(Vec::new());
    };
    let actions = HashMap::from([
        ("SessionStart", "codex-session-start"),
        ("UserPromptSubmit", "codex-prompt-submit"),
        ("PermissionRequest", "codex-permission-request"),
        ("Stop", "codex-stop"),
    ]);
    let labels = HashMap::from([
        ("PermissionRequest", "permission_request"),
        ("SessionStart", "session_start"),
        ("Stop", "stop"),
        ("UserPromptSubmit", "user_prompt_submit"),
    ]);
    let mut states = Vec::new();
    for (event_key, event_label) in labels {
        let Some(action) = actions.get(event_key) else {
            continue;
        };
        let Some(groups) = hooks.get(event_key).and_then(|value| value.as_array()) else {
            continue;
        };
        for (group_index, group) in groups.iter().enumerate() {
            let Some(group_object) = group.as_object() else {
                continue;
            };
            let matcher = match event_key {
                "UserPromptSubmit" | "Stop" => None,
                _ => group_object.get("matcher").and_then(|value| value.as_str()),
            };
            let Some(hooks_array) = group_object.get("hooks").and_then(|value| value.as_array())
            else {
                continue;
            };
            for (handler_index, hook) in hooks_array.iter().enumerate() {
                let Some(hook_object) = hook.as_object() else {
                    continue;
                };
                if hook_object.get("type").and_then(|value| value.as_str()) != Some("command") {
                    continue;
                }
                let Some(command) = hook_object.get("command").and_then(|value| value.as_str())
                else {
                    continue;
                };
                if !is_codex_managed_hook_command(command, action) {
                    continue;
                }
                let timeout = hook_object
                    .get("timeout")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(600)
                    .max(1);
                let status_message = hook_object
                    .get("statusMessage")
                    .and_then(|value| value.as_str());
                states.push(CodexHookTrustState {
                    key: format!(
                        "{}:{}:{}:{}",
                        hooks_path.display(),
                        event_label,
                        group_index,
                        handler_index
                    ),
                    trusted_hash: codex_command_hook_trust_hash(
                        event_label,
                        matcher,
                        command,
                        timeout,
                        status_message,
                    ),
                });
            }
        }
    }
    Ok(states)
}

fn is_codex_managed_hook_command(command: &str, action: &str) -> bool {
    if command.contains("dmux-ai-state.sh")
        && command.contains(&shell_quote(action))
        && command.contains(&shell_quote("codex"))
    {
        return true;
    }
    is_windows_codex_managed_hook_command(command, action)
}

fn is_windows_codex_managed_hook_command(command: &str, action: &str) -> bool {
    command.contains("dmux-ai-state.cmd")
        && command.contains(&windows_cmd_quote_cross_platform(action))
        && command.contains(&windows_cmd_quote_cross_platform("codex"))
}

fn codex_command_hook_trust_hash(
    event_label: &str,
    matcher: Option<&str>,
    command: &str,
    timeout: i64,
    status_message: Option<&str>,
) -> String {
    let status_json = status_message
        .map(json_string_literal)
        .unwrap_or_else(|| "null".to_string());
    let hook_json = format!(
        "\"hooks\":[{{\"async\":false,\"command\":{},\"statusMessage\":{},\"timeout\":{},\"type\":\"command\"}}]",
        json_string_literal(command),
        status_json,
        timeout
    );
    let canonical_json = if let Some(matcher) = matcher {
        format!(
            "{{\"event_name\":{},\"hooks\":[{{\"async\":false,\"command\":{},\"statusMessage\":{},\"timeout\":{},\"type\":\"command\"}}],\"matcher\":{}}}",
            json_string_literal(event_label),
            json_string_literal(command),
            status_message.map(json_string_literal).unwrap_or_else(|| "null".to_string()),
            timeout,
            json_string_literal(matcher)
        )
    } else {
        format!(
            "{{\"event_name\":{},{}}}",
            json_string_literal(event_label),
            hook_json
        )
    };
    let digest = Sha256::digest(canonical_json.as_bytes());
    format!("sha256:{digest:x}")
}

fn updated_codex_config_text(existing_text: &str, states: &[CodexHookTrustState]) -> String {
    let target_line = "suppress_unstable_features_warning = true";
    let features_header = "[features]";
    let hooks_feature_line = "hooks = true";

    let mut lines = existing_text
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::to_string)
        .filter(|line| !normalized_line(line).starts_with("suppress_unstable_features_warning"))
        .collect::<Vec<_>>();
    while lines
        .last()
        .map(|line| normalized_line(line).is_empty())
        .unwrap_or(false)
    {
        lines.pop();
    }

    if lines.is_empty() {
        lines.push(target_line.to_string());
    } else {
        let first_table = lines
            .iter()
            .position(|line| is_toml_table_header(line))
            .unwrap_or(lines.len());
        let mut insertion_index = first_table;
        while insertion_index > 0 && normalized_line(&lines[insertion_index - 1]).is_empty() {
            insertion_index -= 1;
        }
        lines.insert(insertion_index, target_line.to_string());
        if insertion_index < first_table
            && insertion_index + 1 < lines.len()
            && !normalized_line(&lines[insertion_index + 1]).is_empty()
        {
            lines.insert(insertion_index + 1, String::new());
        }
    }

    ensure_codex_hooks_feature(&mut lines, features_header, hooks_feature_line);
    let mut sorted_states = states.to_vec();
    sorted_states.sort_by(|left, right| left.key.cmp(&right.key));
    let trust_keys = sorted_states
        .iter()
        .map(|state| state.key.as_str())
        .collect::<Vec<_>>();
    lines = remove_codex_hook_trust_blocks(lines, &trust_keys);
    append_codex_hook_trust_states(&mut lines, &sorted_states);
    format!("{}\n", lines.join("\n"))
}

fn ensure_codex_hooks_feature(
    lines: &mut Vec<String>,
    features_header: &str,
    hooks_feature_line: &str,
) {
    let Some(features_index) = lines
        .iter()
        .position(|line| normalized_line(line) == features_header)
    else {
        if !lines.is_empty() && !normalized_line(lines.last().unwrap_or(&String::new())).is_empty()
        {
            lines.push(String::new());
        }
        lines.push(features_header.to_string());
        lines.push(hooks_feature_line.to_string());
        return;
    };
    let section_end = toml_section_end(lines, features_index);
    let mut hooks_index = None;
    let mut legacy_hooks_index = None;
    let mut removal_indices = Vec::new();
    for index in (features_index + 1)..section_end {
        match toml_key_name(&lines[index]).as_deref() {
            Some("hooks") => {
                if hooks_index.is_none() {
                    hooks_index = Some(index);
                } else {
                    removal_indices.push(index);
                }
            }
            Some("codex_hooks") => {
                if legacy_hooks_index.is_none() {
                    legacy_hooks_index = Some(index);
                } else {
                    removal_indices.push(index);
                }
            }
            _ => {}
        }
    }
    if let Some(index) = hooks_index {
        lines[index] = hooks_feature_line.to_string();
        if let Some(legacy) = legacy_hooks_index {
            removal_indices.push(legacy);
        }
    } else if let Some(index) = legacy_hooks_index {
        lines[index] = hooks_feature_line.to_string();
    } else {
        let mut insertion_index = section_end;
        while insertion_index > features_index + 1
            && normalized_line(&lines[insertion_index - 1]).is_empty()
        {
            insertion_index -= 1;
        }
        lines.insert(insertion_index, hooks_feature_line.to_string());
    }
    removal_indices.sort_unstable_by(|left, right| right.cmp(left));
    removal_indices.dedup();
    for index in removal_indices {
        lines.remove(index);
    }
}

fn remove_codex_hook_trust_blocks(lines: Vec<String>, keys: &[&str]) -> Vec<String> {
    if keys.is_empty() {
        return lines;
    }
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if let Some(key) = codex_hook_state_key(&lines[index]) {
            if keys.contains(&key.as_str()) {
                index += 1;
                while index < lines.len() && !is_toml_table_header(&lines[index]) {
                    index += 1;
                }
                continue;
            }
        }
        result.push(lines[index].clone());
        index += 1;
    }
    result
}

fn append_codex_hook_trust_states(lines: &mut Vec<String>, states: &[CodexHookTrustState]) {
    if states.is_empty() {
        return;
    }
    if !lines
        .iter()
        .any(|line| normalized_line(line) == "[hooks.state]")
    {
        if !lines.is_empty() && !normalized_line(lines.last().unwrap_or(&String::new())).is_empty()
        {
            lines.push(String::new());
        }
        lines.push("[hooks.state]".to_string());
    }
    for state in states {
        if !lines.is_empty() && !normalized_line(lines.last().unwrap_or(&String::new())).is_empty()
        {
            lines.push(String::new());
        }
        lines.push(format!("[hooks.state.{}]", toml_quoted_string(&state.key)));
        lines.push(format!(
            "trusted_hash = {}",
            toml_quoted_string(&state.trusted_hash)
        ));
    }
}

fn toml_section_end(lines: &[String], start: usize) -> usize {
    let mut index = start + 1;
    while index < lines.len() && !is_toml_table_header(&lines[index]) {
        index += 1;
    }
    index
}

fn toml_key_name(line: &str) -> Option<String> {
    let trimmed = normalized_line(line);
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (key, _) = trimmed.split_once('=')?;
    Some(key.trim().to_string())
}

fn codex_hook_state_key(line: &str) -> Option<String> {
    let trimmed = normalized_line(line);
    let raw = trimmed
        .strip_prefix("[hooks.state.")
        .and_then(|value| value.strip_suffix(']'))?;
    parse_toml_basic_string(raw).or_else(|| parse_toml_literal_string(raw))
}

fn parse_toml_basic_string(value: &str) -> Option<String> {
    let raw = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut output = String::new();
    let mut chars = raw.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match chars.next()? {
            '"' => output.push('"'),
            '\\' => output.push('\\'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            other => {
                output.push('\\');
                output.push(other);
            }
        }
    }
    Some(output)
}

fn parse_toml_literal_string(value: &str) -> Option<String> {
    value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .map(str::to_string)
}

fn is_toml_table_header(line: &str) -> bool {
    let trimmed = normalized_line(line);
    trimmed.starts_with('[') && trimmed.ends_with(']')
}

fn normalized_line(line: &str) -> String {
    line.trim().to_string()
}

fn toml_quoted_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            _ => output.push(character),
        }
    }
    output.push('"');
    output
}

fn json_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn load_json_object(path: &Path) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    if !path.exists() {
        return Ok(serde_json::Map::new());
    }
    let data = fs::read(path).map_err(|error| error.to_string())?;
    if data.is_empty() {
        return Ok(serde_json::Map::new());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&data).unwrap_or_else(|_| serde_json::json!({}));
    Ok(value.as_object().cloned().unwrap_or_default())
}

fn write_json_object(
    path: &Path,
    root: serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let data = serde_json::to_vec_pretty(&serde_json::Value::Object(root))
        .map_err(|error| error.to_string())?;
    if fs::read(path).ok().as_deref() == Some(data.as_slice()) {
        return Ok(());
    }
    fs::write(path, data).map_err(|error| error.to_string())
}

fn is_managed_hook(value: &serde_json::Value, action: &str, tool: &str) -> bool {
    is_managed_hook_action(value, action, Some(tool))
}

fn is_managed_hook_action(value: &serde_json::Value, action: &str, tool: Option<&str>) -> bool {
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
        command.contains("dmux-ai-state.cmd")
            && command.contains(&windows_cmd_quote(action))
            && tool
                .map(|tool| command.contains(&windows_cmd_quote(tool)))
                .unwrap_or(true)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn hook_command(helper_script: &Path, action: &str, owner: &str, tool: &str) -> String {
    #[cfg(windows)]
    {
        return format!(
            "cmd /d /c call {} {} {} {}",
            windows_cmd_quote(&helper_script.with_extension("cmd").display().to_string()),
            windows_cmd_quote(action),
            windows_cmd_quote(owner),
            windows_cmd_quote(tool),
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
fn windows_cmd_quote(value: &str) -> String {
    windows_cmd_quote_cross_platform(value)
}

fn windows_cmd_quote_cross_platform(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn stage_runtime_asset(
    relative_path: &str,
    destination: &Path,
    executable: bool,
) -> Result<(), String> {
    #[cfg(not(unix))]
    let _ = executable;

    let content = runtime_asset_content(relative_path)
        .ok_or_else(|| format!("runtime asset {relative_path} was not embedded"))?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let should_write = match fs::read(destination) {
        Ok(existing) => existing != content,
        Err(_) => true,
    };

    if should_write {
        fs::write(destination, content).map_err(|error| error.to_string())?;
    }

    #[cfg(unix)]
    if executable {
        let permissions = fs::Permissions::from_mode(0o755);
        let _ = fs::set_permissions(destination, permissions);
    }

    Ok(())
}

fn stage_runtime_dir(relative_path: &str, destination: &Path) -> Result<(), String> {
    let prefix = format!("{}/", relative_path.trim_end_matches('/'));
    let mut staged = 0usize;
    for (asset_path, _) in RUNTIME_ASSETS {
        let Some(child_path) = asset_path.strip_prefix(&prefix) else {
            continue;
        };
        stage_runtime_asset(asset_path, &destination.join(child_path), false)?;
        staged += 1;
    }
    if staged == 0 {
        return Err(format!(
            "runtime asset directory {relative_path} was not embedded"
        ));
    }
    Ok(())
}

#[derive(Default)]
struct CodexParsedState {
    model: Option<String>,
    assistant_preview: Option<String>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    total_tokens: Option<i64>,
    updated_at: Option<f64>,
    started_at: Option<f64>,
    completed_at: Option<f64>,
    response_state: Option<String>,
    was_interrupted: bool,
    has_completed_turn: bool,
    origin: String,
    last_user_message_at: Option<f64>,
}

fn parse_codex_runtime_state(
    file_path: &Path,
    project_path: Option<&str>,
) -> Option<CodexParsedState> {
    let file = fs::File::open(file_path).ok()?;
    let reader = BufReader::new(file);
    parse_codex_runtime_lines(
        reader.lines().map_while(Result::ok),
        project_path,
        None,
        None,
    )
}

fn parse_codex_runtime_state_tail(
    file_path: &Path,
    project_path: Option<&str>,
    fallback_started_at: f64,
    fallback_updated_at: f64,
) -> Option<CodexParsedState> {
    let metadata = fs::metadata(file_path).ok()?;
    if metadata.len() <= CODEX_LIVE_TRANSCRIPT_TAIL_BYTES {
        return parse_codex_runtime_state(file_path, project_path);
    }
    let mut file = fs::File::open(file_path).ok()?;
    let start = metadata
        .len()
        .saturating_sub(CODEX_LIVE_TRANSCRIPT_TAIL_BYTES);
    file.seek(std::io::SeekFrom::Start(start)).ok()?;
    let mut reader = BufReader::with_capacity(32 * 1024, file);
    if start > 0 {
        let mut partial = String::new();
        reader.read_line(&mut partial).ok()?;
    }
    let lines = read_recent_jsonl_lines(reader, CODEX_LIVE_TRANSCRIPT_TAIL_LINES)?;
    parse_codex_runtime_lines(
        lines.into_iter(),
        project_path,
        Some(fallback_started_at),
        Some(fallback_updated_at),
    )
}

fn read_recent_jsonl_lines<R>(mut reader: R, limit: usize) -> Option<Vec<String>>
where
    R: BufRead,
{
    if limit == 0 {
        return Some(Vec::new());
    }
    let mut lines = VecDeque::with_capacity(limit);
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).ok()?;
        if bytes == 0 {
            break;
        }
        while line.ends_with(['\n', '\r']) {
            line.pop();
        }
        if line.is_empty() {
            continue;
        }
        if lines.len() == limit {
            lines.pop_front();
        }
        lines.push_back(line);
    }
    Some(lines.into_iter().collect())
}

fn parse_codex_runtime_lines<I>(
    lines: I,
    project_path: Option<&str>,
    fallback_started_at: Option<f64>,
    fallback_updated_at: Option<f64>,
) -> Option<CodexParsedState>
where
    I: Iterator<Item = String>,
{
    let mut state = CodexParsedState {
        origin: "unknown".to_string(),
        ..Default::default()
    };
    let mut latest_cumulative_usage: Option<UsageTotals> = None;
    let mut usage_at_turn_start: Option<UsageTotals> = None;

    for line in lines {
        let Ok(row) = serde_json::from_str::<CodexTranscriptRow>(&line) else {
            continue;
        };
        let timestamp = row.timestamp.as_deref().and_then(parse_iso8601_seconds);
        if let Some(timestamp) = timestamp {
            state.updated_at = Some(state.updated_at.unwrap_or(timestamp).max(timestamp));
        }
        let row_type = row.row_type.as_deref();
        let payload = row
            .payload
            .and_then(|payload| serde_json::from_str::<CodexPayloadFields>(payload.get()).ok())
            .unwrap_or_default();

        if let Some(preview) = codex_assistant_preview(row_type, &payload) {
            state.assistant_preview = Some(preview);
        }

        if row_type == Some("turn_context") {
            if project_path
                .map(|project| payload.cwd.as_deref() == Some(project))
                .unwrap_or(true)
            {
                if let Some(model) = payload
                    .model
                    .as_deref()
                    .and_then(|value| normalized_string(Some(value)))
                {
                    state.model = Some(model);
                }
            }
            continue;
        }

        let event_type = payload.payload_type.as_deref();
        let is_user_message = (row_type == Some("event_msg") && event_type == Some("user_message"))
            || (row_type == Some("response_item")
                && event_type == Some("message")
                && payload.role.as_deref() == Some("user"));
        if is_user_message {
            let user_message_at = timestamp.or(state.updated_at);
            if user_message_at > state.last_user_message_at {
                state.last_user_message_at = user_message_at;
                if let Some(user_message_at) = user_message_at {
                    if state
                        .completed_at
                        .is_some_and(|completed_at| user_message_at > completed_at)
                    {
                        state.started_at = Some(user_message_at);
                        usage_at_turn_start = latest_cumulative_usage.clone();
                        state.completed_at = None;
                        state.was_interrupted = false;
                        state.has_completed_turn = false;
                    }
                }
            }
        }
        let is_final_answer = (row_type == Some("event_msg")
            && event_type == Some("agent_message")
            && payload.phase.as_deref() == Some("final_answer"))
            || (row_type == Some("response_item")
                && event_type == Some("message")
                && payload.phase.as_deref() == Some("final_answer"));
        if is_final_answer {
            let completed_at = timestamp.or(state.updated_at);
            if completed_at >= state.completed_at {
                state.completed_at = completed_at;
                state.was_interrupted = false;
                state.has_completed_turn = true;
            }
            continue;
        }

        if row_type != Some("event_msg") {
            continue;
        }
        match event_type {
            Some("task_started") => {
                state.started_at = payload.started_at.or(timestamp);
                usage_at_turn_start = latest_cumulative_usage.clone();
                state.was_interrupted = false;
                state.has_completed_turn = false;
            }
            Some("task_complete") => {
                let completed_at = payload.completed_at.or(timestamp);
                if completed_at >= state.completed_at {
                    state.completed_at = completed_at;
                    state.was_interrupted = false;
                    state.has_completed_turn = true;
                }
            }
            Some("turn_aborted") => {
                let completed_at = payload.completed_at.or(timestamp);
                if completed_at >= state.completed_at {
                    state.completed_at = completed_at;
                    state.was_interrupted = true;
                    state.has_completed_turn = false;
                }
            }
            Some("token_count") => {
                let info = payload
                    .info
                    .and_then(|info| serde_json::from_str::<CodexTokenInfo>(info.get()).ok());
                let total_usage = info
                    .as_ref()
                    .and_then(|info| info.total_token_usage.as_ref())
                    .and_then(usage_totals_from_fields);
                let last_usage = info
                    .as_ref()
                    .and_then(|info| info.last_token_usage.as_ref())
                    .and_then(usage_totals_from_fields);
                if let Some(total_usage) = total_usage.clone() {
                    latest_cumulative_usage = Some(total_usage);
                }
                let resolved = resolve_runtime_usage(
                    total_usage,
                    usage_at_turn_start
                        .clone()
                        .or_else(|| latest_cumulative_usage.clone()),
                    last_usage,
                );
                if let Some(resolved) = resolved {
                    state.input_tokens = Some(resolved.input_tokens);
                    state.output_tokens = Some(resolved.output_tokens);
                    state.cached_input_tokens = Some(resolved.cached_input_tokens);
                    state.total_tokens = Some(resolved.total_tokens);
                }
            }
            _ => {}
        }
    }

    if state.started_at.is_none() {
        state.started_at = fallback_started_at;
    }
    if let Some(fallback_updated_at) = fallback_updated_at {
        state.updated_at = Some(
            state
                .updated_at
                .unwrap_or(fallback_updated_at)
                .max(fallback_updated_at),
        );
    }
    state.response_state = match (state.started_at, state.completed_at) {
        (Some(started_at), Some(completed_at)) if completed_at >= started_at => {
            Some("idle".to_string())
        }
        (None, Some(_)) => Some("idle".to_string()),
        (Some(_), _) => Some("responding".to_string()),
        _ => None,
    };
    let final_usage = match state.response_state.as_deref() {
        Some("idle") => latest_cumulative_usage,
        _ => None,
    };
    if let Some(final_usage) = final_usage {
        state.input_tokens = Some(final_usage.input_tokens);
        state.output_tokens = Some(final_usage.output_tokens);
        state.cached_input_tokens = Some(final_usage.cached_input_tokens);
        state.total_tokens = Some(final_usage.total_tokens);
    }
    if state.response_state.as_deref() == Some("responding") {
        let historical_total = usage_at_turn_start
            .as_ref()
            .map(|usage| usage.total_tokens + usage.cached_input_tokens)
            .unwrap_or(0);
        state.origin = if historical_total > 0 {
            "restored"
        } else {
            "fresh"
        }
        .to_string();
    }
    Some(state)
}

#[derive(Deserialize)]
struct CodexTranscriptRow<'a> {
    #[serde(borrow)]
    timestamp: Option<Cow<'a, str>>,
    #[serde(rename = "type", borrow)]
    row_type: Option<Cow<'a, str>>,
    #[serde(borrow)]
    payload: Option<&'a RawValue>,
}

#[derive(Default, Deserialize)]
struct CodexPayloadFields<'a> {
    #[serde(rename = "type", borrow)]
    payload_type: Option<Cow<'a, str>>,
    #[serde(borrow)]
    phase: Option<Cow<'a, str>>,
    #[serde(borrow)]
    role: Option<Cow<'a, str>>,
    #[serde(borrow)]
    cwd: Option<Cow<'a, str>>,
    #[serde(borrow)]
    model: Option<Cow<'a, str>>,
    started_at: Option<f64>,
    completed_at: Option<f64>,
    #[serde(borrow)]
    info: Option<&'a RawValue>,
    #[serde(borrow)]
    message: Option<&'a RawValue>,
    #[serde(borrow)]
    text: Option<&'a RawValue>,
    #[serde(borrow)]
    content: Option<&'a RawValue>,
    #[serde(borrow)]
    summary: Option<&'a RawValue>,
    #[serde(borrow)]
    summary_text: Option<&'a RawValue>,
}

#[derive(Default, Deserialize)]
struct CodexTokenInfo {
    total_token_usage: Option<UsageTotalsFields>,
    last_token_usage: Option<UsageTotalsFields>,
}

#[derive(Default, Deserialize)]
struct UsageTotalsFields {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    cache_read_input_tokens: Option<i64>,
    reasoning_output_tokens: Option<i64>,
    total_tokens: Option<i64>,
}

#[derive(Clone, Default)]
struct UsageTotals {
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    total_tokens: i64,
}

fn usage_totals_from_fields(fields: &UsageTotalsFields) -> Option<UsageTotals> {
    let raw_input_tokens = fields.input_tokens.unwrap_or(0);
    let raw_output_tokens = fields.output_tokens.unwrap_or(0);
    let cached_input_tokens = fields
        .cached_input_tokens
        .or(fields.cache_read_input_tokens)
        .unwrap_or(0);
    let reasoning_output_tokens = fields.reasoning_output_tokens.unwrap_or(0);
    if raw_input_tokens == 0 && raw_output_tokens == 0 {
        if let Some(raw_total_tokens) = fields.total_tokens {
            if raw_total_tokens > 0 {
                return Some(UsageTotals {
                    input_tokens: raw_total_tokens,
                    output_tokens: 0,
                    cached_input_tokens,
                    total_tokens: raw_total_tokens,
                });
            }
        }
    }
    let input_tokens = (raw_input_tokens - cached_input_tokens).max(0);
    let output_tokens = (raw_output_tokens - reasoning_output_tokens).max(0);
    let total_tokens = input_tokens + output_tokens + reasoning_output_tokens;
    if input_tokens == 0 && output_tokens == 0 && cached_input_tokens == 0 && total_tokens == 0 {
        return None;
    }
    Some(UsageTotals {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        total_tokens,
    })
}

#[cfg(test)]
fn parse_usage_totals(value: &serde_json::Value) -> Option<UsageTotals> {
    serde_json::from_value::<UsageTotalsFields>(value.clone())
        .ok()
        .and_then(|fields| usage_totals_from_fields(&fields))
}

fn resolve_runtime_usage(
    total_usage: Option<UsageTotals>,
    base_usage: Option<UsageTotals>,
    last_usage: Option<UsageTotals>,
) -> Option<UsageTotals> {
    if total_usage.is_none() && last_usage.is_none() {
        return None;
    }
    let Some(last_usage) = last_usage else {
        return total_usage;
    };
    let base_usage = base_usage.unwrap_or_default();
    if let Some(total_usage) = total_usage {
        let total_with_cache = total_usage.total_tokens + total_usage.cached_input_tokens;
        let base_with_cache = base_usage.total_tokens + base_usage.cached_input_tokens;
        if total_with_cache > base_with_cache {
            return Some(total_usage);
        }
        if total_with_cache == base_with_cache {
            let last_with_cache = last_usage.total_tokens + last_usage.cached_input_tokens;
            if last_with_cache == total_with_cache {
                return Some(total_usage);
            }
        }
    }
    Some(UsageTotals {
        input_tokens: base_usage.input_tokens + last_usage.input_tokens,
        output_tokens: base_usage.output_tokens + last_usage.output_tokens,
        cached_input_tokens: base_usage.cached_input_tokens + last_usage.cached_input_tokens,
        total_tokens: base_usage.total_tokens + last_usage.total_tokens,
    })
}

fn codex_assistant_preview(
    row_type: Option<&str>,
    payload: &CodexPayloadFields<'_>,
) -> Option<String> {
    let payload_type = payload.payload_type.as_deref();
    match row_type {
        Some("event_msg") if payload_type == Some("agent_message") => {
            sanitized_preview_from_raw_values(&[payload.message, payload.text, payload.content])
        }
        Some("response_item") if payload_type == Some("reasoning") => {
            sanitized_preview_from_raw_values(&[
                payload.summary,
                payload.summary_text,
                payload.text,
            ])
        }
        Some("response_item") if payload_type == Some("agentMessage") => {
            sanitized_preview_from_raw_values(&[payload.text, payload.content, payload.message])
        }
        Some("response_item") if payload_type == Some("message") => {
            sanitized_preview_from_raw_values(&[payload.content, payload.message, payload.text])
        }
        _ => None,
    }
}

fn sanitized_preview_from_raw_values(values: &[Option<&RawValue>]) -> Option<String> {
    let parsed = values
        .iter()
        .map(|value| {
            value.and_then(|value| serde_json::from_str::<serde_json::Value>(value.get()).ok())
        })
        .collect::<Vec<_>>();
    let refs = parsed
        .iter()
        .map(|value| value.as_ref())
        .collect::<Vec<_>>();
    sanitized_preview_from_values(&refs)
}

fn sanitized_preview_from_values(values: &[Option<&serde_json::Value>]) -> Option<String> {
    for value in values.iter().flatten() {
        for text in flatten_text(value) {
            if let Some(preview) = sanitized_preview(&text) {
                return Some(preview);
            }
        }
    }
    None
}

fn joined_preview_from_values(values: &[Option<&serde_json::Value>]) -> Option<String> {
    let mut lines = Vec::new();
    'outer: for value in values.iter().flatten() {
        for text in flatten_text(value) {
            for line in text
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
            {
                lines.push(line.to_string());
                if lines.len() >= 3 {
                    break 'outer;
                }
            }
        }
    }
    let preview = lines.join("\n");
    sanitized_preview(&preview)
}

fn flatten_text(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(text) => vec![text.clone()],
        serde_json::Value::Array(items) => items.iter().flat_map(flatten_text).collect(),
        serde_json::Value::Object(object) => ["text", "content", "message", "summary"]
            .into_iter()
            .filter_map(|key| object.get(key))
            .flat_map(flatten_text)
            .collect(),
        _ => Vec::new(),
    }
}

fn sanitized_preview(value: &str) -> Option<String> {
    let preview = value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    let preview = preview.trim();
    if preview.is_empty() {
        None
    } else {
        Some(preview.chars().take(180).collect())
    }
}

#[derive(Default)]
struct ClaudeAggregate {
    model: Option<String>,
    assistant_preview: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    total_tokens: i64,
    updated_at: f64,
    last_user_at: f64,
    last_completion_at: f64,
    last_interrupted_at: f64,
    last_completed_turn_at: f64,
}

impl ClaudeAggregate {
    fn merge(self, other: Self) -> Self {
        Self {
            model: other.model.or(self.model),
            assistant_preview: other.assistant_preview.or(self.assistant_preview),
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cached_input_tokens: self.cached_input_tokens + other.cached_input_tokens,
            total_tokens: self.total_tokens + other.total_tokens,
            updated_at: self.updated_at.max(other.updated_at),
            last_user_at: self.last_user_at.max(other.last_user_at),
            last_completion_at: self.last_completion_at.max(other.last_completion_at),
            last_interrupted_at: self.last_interrupted_at.max(other.last_interrupted_at),
            last_completed_turn_at: self
                .last_completed_turn_at
                .max(other.last_completed_turn_at),
        }
    }

    fn started_at(&self) -> Option<f64> {
        (self.last_user_at > 0.0).then_some(self.last_user_at)
    }

    fn completed_at(&self) -> Option<f64> {
        let completion = self.last_completed_turn_at.max(self.last_interrupted_at);
        (completion > 0.0).then_some(completion)
    }

    fn response_state(&self) -> Option<String> {
        if self.last_user_at <= 0.0 {
            return None;
        }
        if self.last_user_at > self.last_completion_at {
            Some("responding".to_string())
        } else {
            Some("idle".to_string())
        }
    }

    fn was_interrupted(&self) -> bool {
        if self.last_interrupted_at <= 0.0 {
            return false;
        }
        let latest_conflicting_at = self.last_user_at.max(self.last_completed_turn_at);
        self.last_interrupted_at >= latest_conflicting_at
    }

    fn has_completed_turn(&self) -> bool {
        if self.last_completed_turn_at <= 0.0 {
            return false;
        }
        let latest_conflicting_at = self.last_user_at.max(self.last_interrupted_at);
        self.last_completed_turn_at >= latest_conflicting_at
    }
}

fn parse_claude_log_runtime_state(
    file_path: &Path,
    project_path: &str,
    external_session_id: &str,
) -> Option<ClaudeAggregate> {
    let file = fs::File::open(file_path).ok()?;
    let reader = BufReader::new(file);
    let mut aggregate = ClaudeAggregate::default();
    let mut matched = false;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if row.get("sessionId").and_then(|value| value.as_str()) != Some(external_session_id) {
            continue;
        }
        if let Some(cwd) = row.get("cwd").and_then(|value| value.as_str()) {
            if !paths_equivalent(Some(cwd), project_path) {
                continue;
            }
        }
        matched = true;
        let timestamp = row
            .get("timestamp")
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds)
            .unwrap_or_else(now_seconds);
        aggregate.updated_at = aggregate.updated_at.max(timestamp);
        let message = row.get("message").unwrap_or(&serde_json::Value::Null);
        let role = message
            .get("role")
            .and_then(|value| value.as_str())
            .or_else(|| row.get("type").and_then(|value| value.as_str()));
        if role == Some("user") {
            if is_claude_interrupted_row(&row) {
                aggregate.last_interrupted_at = aggregate.last_interrupted_at.max(timestamp);
                aggregate.last_completion_at = aggregate.last_completion_at.max(timestamp);
            } else {
                aggregate.last_user_at = aggregate.last_user_at.max(timestamp);
            }
        } else if role == Some("assistant") {
            let stop_reason = message.get("stop_reason").and_then(|value| value.as_str());
            if stop_reason == Some("end_turn") {
                aggregate.last_completed_turn_at = aggregate.last_completed_turn_at.max(timestamp);
                aggregate.last_completion_at = aggregate.last_completion_at.max(timestamp);
            }
            if let Some(preview) =
                sanitized_preview_from_values(&[message.get("content"), row.get("content")])
            {
                aggregate.assistant_preview = Some(preview);
            }
        } else if role == Some("system") {
            let subtype = row.get("subtype").and_then(|value| value.as_str());
            if matches!(subtype, Some("turn_duration" | "stop_hook_summary")) {
                aggregate.last_completion_at = aggregate.last_completion_at.max(timestamp);
            }
        }
        if let Some(model) = first_string_deep(&row, &["model"]) {
            aggregate.model = Some(model);
        }
        if let Some(usage) = first_object_deep(&row, &["usage"]) {
            aggregate.input_tokens += json_i64(usage.get("input_tokens"));
            aggregate.output_tokens += json_i64(usage.get("output_tokens"));
            aggregate.cached_input_tokens += json_i64(usage.get("cache_creation_input_tokens"))
                + json_i64(usage.get("cache_read_input_tokens"));
            aggregate.total_tokens +=
                json_i64(usage.get("input_tokens")) + json_i64(usage.get("output_tokens"));
        }
    }

    if !matched {
        return None;
    }
    Some(aggregate)
}

fn is_claude_interrupted_row(row: &serde_json::Value) -> bool {
    let text = row.to_string().to_lowercase();
    text.contains("interrupted") || text.contains("cancelled") || text.contains("aborted")
}

#[derive(Clone)]
struct GeminiParsedState {
    external_session_id: String,
    model: Option<String>,
    assistant_preview: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    total_tokens: i64,
    started_at: f64,
    updated_at: f64,
    completed_at: Option<f64>,
    response_state: Option<String>,
    origin: String,
}

fn parse_gemini_runtime_state(file_path: &Path) -> Option<GeminiParsedState> {
    let data = fs::read(file_path).ok()?;
    let object: serde_json::Value = serde_json::from_slice(&data).ok()?;
    let external_session_id = object
        .get("sessionId")
        .and_then(|value| value.as_str())
        .and_then(|value| normalized_string(Some(value)))?;
    let messages = object
        .get("messages")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let started_at = object
        .get("startTime")
        .and_then(|value| value.as_str())
        .and_then(parse_iso8601_seconds)
        .or_else(|| {
            messages
                .iter()
                .filter_map(|message| {
                    message
                        .get("timestamp")
                        .and_then(|value| value.as_str())
                        .and_then(parse_iso8601_seconds)
                })
                .min_by(|left, right| left.total_cmp(right))
        })
        .unwrap_or(0.0);
    let updated_at = object
        .get("lastUpdated")
        .and_then(|value| value.as_str())
        .and_then(parse_iso8601_seconds)
        .or_else(|| {
            messages
                .iter()
                .filter_map(|message| {
                    message
                        .get("timestamp")
                        .and_then(|value| value.as_str())
                        .and_then(parse_iso8601_seconds)
                })
                .max_by(|left, right| left.total_cmp(right))
        })
        .unwrap_or(started_at);

    let mut model = None;
    let mut input_tokens = 0;
    let mut output_tokens = 0;
    let mut cached_input_tokens = 0;
    let mut total_tokens = 0;
    let mut last_relevant_type: Option<String> = None;
    let mut assistant_preview = None;

    for message in messages {
        if let Some(message_type) = message.get("type").and_then(|value| value.as_str()) {
            if message_type != "warning" {
                last_relevant_type = Some(message_type.to_string());
            }
            if message_type != "gemini" {
                continue;
            }
        }
        if let Some(candidate_model) = message
            .get("model")
            .and_then(|value| value.as_str())
            .and_then(|value| normalized_string(Some(value)))
        {
            model = Some(candidate_model);
        }
        if let Some(preview) = gemini_assistant_preview(&message) {
            assistant_preview = Some(preview);
        }
        let tokens = message.get("tokens").unwrap_or(&serde_json::Value::Null);
        let cached = json_i64(tokens.get("cached"));
        let thoughts = json_i64(tokens.get("thoughts"));
        let input = (json_i64(tokens.get("input")) - cached).max(0);
        let output = (json_i64(tokens.get("output")) - thoughts).max(0);
        let total = tokens
            .get("total")
            .and_then(|value| value.as_i64())
            .map(|value| (value - cached).max(0))
            .unwrap_or(input + output + thoughts);
        input_tokens += input;
        output_tokens += output;
        cached_input_tokens += cached.max(0);
        total_tokens += total.max(0);
    }

    let response_state = match last_relevant_type.as_deref() {
        Some("user") => Some("responding".to_string()),
        Some("gemini") | Some("error") | Some("info") => Some("idle".to_string()),
        _ if total_tokens > 0 || model.is_some() => Some("idle".to_string()),
        _ => None,
    };
    let completed_at = (response_state.as_deref() == Some("idle")).then_some(updated_at);
    Some(GeminiParsedState {
        external_session_id,
        model,
        assistant_preview,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        total_tokens,
        started_at,
        updated_at,
        completed_at,
        response_state,
        origin: "unknown".to_string(),
    })
}

fn find_kiro_session_path(project_path: &str, external_session_id: &str) -> Option<PathBuf> {
    let sessions_dir = home_dir().join(".kiro").join("sessions").join("cli");
    if !sessions_dir.exists() {
        return None;
    }
    let mut candidates = recursive_files(&sessions_dir, "json")
        .into_iter()
        .filter(|path| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .map(|value| value.contains(external_session_id))
                .unwrap_or(false)
                || kiro_file_belongs_to_project(path, project_path)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    candidates.into_iter().next()
}

fn kiro_file_belongs_to_project(file_path: &Path, project_path: &str) -> bool {
    let Ok(data) = fs::read_to_string(file_path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return false;
    };
    let matches = [
        value.get("projectPath").and_then(|v| v.as_str()),
        value
            .get("project")
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str()),
        value.get("cwd").and_then(|v| v.as_str()),
        value.get("workingDirectory").and_then(|v| v.as_str()),
    ]
    .into_iter()
    .flatten()
    .any(|candidate| paths_equivalent(Some(candidate), project_path));
    matches
}

#[derive(Debug, Clone)]
struct KiroParsedState {
    model: Option<String>,
    assistant_preview: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    total_tokens: i64,
    updated_at: Option<f64>,
    started_at: Option<f64>,
    completed_at: Option<f64>,
    response_state: Option<String>,
    was_interrupted: bool,
    has_completed_turn: bool,
    origin: String,
}

fn parse_kiro_runtime_state(
    file_path: &Path,
    project_path: Option<&str>,
) -> Option<KiroParsedState> {
    let data = fs::read_to_string(file_path).ok()?;
    let value = serde_json::from_str::<Value>(&data).ok()?;
    let mut model = normalized_string(
        value
            .get("model")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("modelId").and_then(|v| v.as_str())),
    );
    let mut assistant_preview = normalized_string(
        value
            .get("assistantPreview")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("preview").and_then(|v| v.as_str())),
    );
    let input_tokens = json_i64(
        value
            .get("inputTokens")
            .or_else(|| value.get("input_tokens")),
    );
    let output_tokens = json_i64(
        value
            .get("outputTokens")
            .or_else(|| value.get("output_tokens")),
    );
    let cached_input_tokens = json_i64(
        value
            .get("cachedInputTokens")
            .or_else(|| value.get("cached_input_tokens")),
    );
    let total_tokens = json_i64(
        value
            .get("totalTokens")
            .or_else(|| value.get("total_tokens")),
    );
    let updated_at = value
        .get("updatedAt")
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|value| value as f64)));
    let started_at = value
        .get("startedAt")
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|value| value as f64)));
    let completed_at = value
        .get("completedAt")
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|value| value as f64)));
    let response_state = normalized_string(
        value
            .get("responseState")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("state").and_then(|v| v.as_str())),
    );
    let was_interrupted = value
        .get("wasInterrupted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let has_completed_turn = value
        .get("hasCompletedTurn")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| response_state.as_deref() == Some("idle"));
    if model.is_none() {
        model = value
            .get("session")
            .and_then(|v| v.get("model"))
            .and_then(|v| v.as_str())
            .and_then(|v| normalized_string(Some(v)));
    }
    if assistant_preview.is_none() {
        assistant_preview = value
            .get("messages")
            .and_then(|v| v.as_array())
            .and_then(|messages| messages.iter().rev().find_map(kiro_message_preview));
    }
    let origin = if project_path
        .and_then(|project| {
            value
                .get("projectPath")
                .and_then(|v| v.as_str())
                .or_else(|| value.get("cwd").and_then(|v| v.as_str()))
                .map(|current| paths_equivalent(Some(current), project))
        })
        .unwrap_or(false)
    {
        "fresh".to_string()
    } else {
        "restored".to_string()
    };

    Some(KiroParsedState {
        model,
        assistant_preview,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        total_tokens,
        updated_at,
        started_at,
        completed_at,
        response_state,
        was_interrupted,
        has_completed_turn,
        origin,
    })
}

fn kiro_message_preview(value: &Value) -> Option<String> {
    let role = value.get("role").and_then(|v| v.as_str()).unwrap_or("");
    if role != "assistant" {
        return None;
    }
    if let Some(text) = value.get("content").and_then(|v| v.as_str()) {
        return normalized_string(Some(text));
    }
    value
        .get("parts")
        .and_then(|v| v.as_array())
        .and_then(|parts| {
            parts.iter().find_map(|part| {
                part.get("text")
                    .and_then(|v| v.as_str())
                    .and_then(|text| normalized_string(Some(text)))
            })
        })
}

fn find_codex_rollout_path(project_path: &str, external_session_id: &str) -> Option<PathBuf> {
    let sessions_dir = home_dir().join(".codex").join("sessions");
    recursive_files(&sessions_dir, "jsonl")
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.contains(external_session_id))
                .unwrap_or(false)
                || codex_file_belongs_to_project(path, project_path)
        })
        .max_by_key(|path| file_modified_millis(path).unwrap_or(0))
}

fn codex_file_belongs_to_project(path: &Path, project_path: &str) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok).take(20) {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let row_type = row.get("type").and_then(|value| value.as_str());
        let payload = row.get("payload").unwrap_or(&serde_json::Value::Null);
        if matches!(row_type, Some("session_meta") | Some("turn_context")) {
            if let Some(cwd) = payload.get("cwd").and_then(|value| value.as_str()) {
                return paths_equivalent(Some(cwd), project_path);
            }
        }
    }
    false
}

fn claude_project_log_paths(project_path: &str) -> Vec<PathBuf> {
    let directory_name = project_path.replace('/', "-").replace('.', "-");
    let direct_dir = home_dir()
        .join(".claude")
        .join("projects")
        .join(directory_name);
    let direct = directory_files(&direct_dir, "jsonl");
    if !direct.is_empty() {
        return direct;
    }
    recursive_files(&home_dir().join(".claude").join("projects"), "jsonl")
        .into_iter()
        .filter(|path| {
            let Ok(file) = fs::File::open(path) else {
                return false;
            };
            let reader = BufReader::new(file);
            for line in reader.lines().map_while(Result::ok).take(12) {
                let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                if let Some(cwd) = row.get("cwd").and_then(|value| value.as_str()) {
                    return paths_equivalent(Some(cwd), project_path);
                }
            }
            false
        })
        .collect()
}

fn gemini_session_paths(project_path: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root_dir in gemini_data_roots() {
        let temp_dir = root_dir.join("tmp");
        let projects_path = root_dir.join("projects.json");
        if let Ok(data) = fs::read(&projects_path) {
            if let Ok(root) = serde_json::from_slice::<serde_json::Value>(&data) {
                if let Some(projects) = root.get("projects").and_then(|value| value.as_object()) {
                    for (stored_path, value) in projects {
                        if paths_equivalent(Some(stored_path), project_path) {
                            if let Some(directory) = value
                                .as_str()
                                .and_then(|value| normalized_string(Some(value)))
                            {
                                dirs.push(temp_dir.join(directory));
                            }
                        }
                    }
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&temp_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let marker = path.join(".project_root");
                if let Ok(value) = fs::read_to_string(marker) {
                    if paths_equivalent(Some(value.trim()), project_path) {
                        dirs.push(path);
                    }
                }
            }
        }
    }
    let mut files = Vec::new();
    for dir in dirs {
        files.extend(directory_files(&dir.join("chats"), "json"));
    }
    files.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("session-"))
            .unwrap_or(false)
    });
    files.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    files
}

fn gemini_data_roots() -> Vec<PathBuf> {
    let gemini_root = home_dir().join(".gemini");
    vec![gemini_root.clone(), gemini_root.join("antigravity-cli")]
}

fn directory_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn recursive_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_recursive_files(dir, extension, &mut files);
    files.sort();
    files
}

fn collect_recursive_files(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recursive_files(&path, extension, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            files.push(path);
        }
    }
}

fn file_modified_millis(path: &Path) -> Option<u128> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn parse_iso8601_seconds(value: &str) -> Option<f64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| {
            date.timestamp() as f64 + f64::from(date.timestamp_subsec_micros()) / 1_000_000.0
        })
}

fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

fn first_string_deep(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    match value {
        serde_json::Value::Object(object) => {
            for key in keys {
                if let Some(value) = object
                    .get(*key)
                    .and_then(|value| value.as_str())
                    .and_then(|value| normalized_string(Some(value)))
                {
                    return Some(value);
                }
            }
            for child in object.values() {
                if let Some(value) = first_string_deep(child, keys) {
                    return Some(value);
                }
            }
            None
        }
        serde_json::Value::Array(items) => {
            items.iter().find_map(|item| first_string_deep(item, keys))
        }
        _ => None,
    }
}

fn first_object_deep<'a>(
    value: &'a serde_json::Value,
    keys: &[&str],
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    match value {
        serde_json::Value::Object(object) => {
            for key in keys {
                if let Some(child) = object.get(*key).and_then(|value| value.as_object()) {
                    return Some(child);
                }
            }
            for child in object.values() {
                if let Some(value) = first_object_deep(child, keys) {
                    return Some(value);
                }
            }
            None
        }
        serde_json::Value::Array(items) => {
            items.iter().find_map(|item| first_object_deep(item, keys))
        }
        _ => None,
    }
}

fn json_i64(value: Option<&serde_json::Value>) -> i64 {
    value.and_then(|value| value.as_i64()).unwrap_or(0)
}

fn normalized_string(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

fn paths_equivalent(left: Option<&str>, right: &str) -> bool {
    let Some(left) = normalized_string(left) else {
        return false;
    };
    let Some(right) = normalized_string(Some(right)) else {
        return false;
    };
    let left = left.trim_end_matches('/');
    let right = right.trim_end_matches('/');
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn codex_config_updater_preserves_user_config_and_manages_hook_state() {
        let existing = r#"
model = "gpt-5.5"

[features]
codex_hooks = false

[hooks.state."/tmp/hooks.json:stop:0:0"]
trusted_hash = "sha256:old"

[profiles.work]
model = "gpt-5.5"
"#;
        let updated = updated_codex_config_text(
            existing,
            &[CodexHookTrustState {
                key: "/tmp/hooks.json:stop:0:0".to_string(),
                trusted_hash: "sha256:new".to_string(),
            }],
        );

        assert!(updated.contains("suppress_unstable_features_warning = true"));
        assert!(updated.contains("[features]\nhooks = true"));
        assert!(!updated.contains("codex_hooks"));
        assert!(!updated.contains("sha256:old"));
        assert!(updated.contains("[hooks.state.\"/tmp/hooks.json:stop:0:0\"]"));
        assert!(updated.contains("trusted_hash = \"sha256:new\""));
        assert!(updated.contains("[profiles.work]\nmodel = \"gpt-5.5\""));
    }

    #[test]
    fn codex_config_updater_removes_literal_string_hook_state_duplicates() {
        let existing = r#"
[hooks.state]

[hooks.state.'C:\Users\dux\.codex\hooks.json:stop:0:0']
trusted_hash = "sha256:old-literal"

[hooks.state."C:\\Users\\dux\\.codex\\hooks.json:stop:0:0"]
trusted_hash = "sha256:old-basic"
"#;
        let updated = updated_codex_config_text(
            existing,
            &[CodexHookTrustState {
                key: r#"C:\Users\dux\.codex\hooks.json:stop:0:0"#.to_string(),
                trusted_hash: "sha256:new".to_string(),
            }],
        );

        assert!(!updated.contains("sha256:old-literal"));
        assert!(!updated.contains("sha256:old-basic"));
        assert_eq!(updated.matches("[hooks.state.").count(), 1);
        assert!(updated.contains(r#"[hooks.state."C:\\Users\\dux\\.codex\\hooks.json:stop:0:0"]"#));
        assert!(updated.contains("trusted_hash = \"sha256:new\""));
    }

    #[test]
    fn codex_hook_hash_is_stable_sha256() {
        let hash = codex_command_hook_trust_hash(
            "stop",
            None,
            "'/tmp/dmux-ai-state.sh' 'codex-stop' 'codux-tauri' 'codex'",
            1000,
            Some("codux codex live"),
        );

        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), "sha256:".len() + 64);
        assert_eq!(
            hash,
            codex_command_hook_trust_hash(
                "stop",
                None,
                "'/tmp/dmux-ai-state.sh' 'codex-stop' 'codux-tauri' 'codex'",
                1000,
                Some("codux codex live"),
            )
        );
    }

    #[test]
    fn codex_managed_hook_command_matches_unix_and_windows_commands() {
        assert!(is_codex_managed_hook_command(
            "'/tmp/dmux-ai-state.sh' 'codex-stop' 'codux-tauri' 'codex'",
            "codex-stop"
        ));
        assert!(is_codex_managed_hook_command(
            r#"cmd /d /c call "C:\Users\dux\AppData\Local\codux-tauri\scripts\wrappers\dmux-ai-state.cmd" "codex-stop" "codux-tauri" "codex""#,
            "codex-stop"
        ));
        assert!(!is_codex_managed_hook_command(
            r#"cmd /d /c call "C:\Users\dux\AppData\Local\codux-tauri\scripts\wrappers\dmux-ai-state.cmd" "codex-stop" "codux-tauri" "claude""#,
            "codex-stop"
        ));
    }

    #[test]
    fn canonical_tool_name_treats_agy_as_gemini_compatible() {
        assert_eq!(canonical_tool_name("agy").as_deref(), Some("gemini"));
    }

    #[test]
    fn tool_hook_config_status_requires_claude_compaction_hooks() {
        let root = std::env::temp_dir().join(format!(
            "codux-claude-hooks-{}-{}.json",
            std::process::id(),
            now_seconds().to_bits()
        ));
        fs::write(
            &root,
            r#"{
              "hooks": {
                "PreCompact": [
                  {
                    "matcher": "",
                    "hooks": [
                      {
                        "type": "command",
                        "command": "'/tmp/dmux-ai-state.sh' 'pre-compact' 'codux-tauri' 'claude'",
                        "timeout": 10
                      }
                    ]
                  }
                ],
                "PostCompact": [
                  {
                    "matcher": "",
                    "hooks": [
                      {
                        "type": "command",
                        "command": "'/tmp/dmux-ai-state.sh' 'post-compact' 'codux-tauri' 'claude'",
                        "timeout": 10
                      }
                    ]
                  }
                ]
              }
            }"#,
        )
        .unwrap();

        let status = tool_hook_config_status(
            &root,
            "claude",
            &[
                ("PreCompact", "pre-compact"),
                ("PostCompact", "post-compact"),
            ],
        );

        assert!(status.configured);
        assert!(status.missing.is_empty());
        let _ = fs::remove_file(root);
    }

    #[test]
    fn managed_hook_status_accepts_kiro_flat_hooks() {
        let root = std::env::temp_dir().join(format!(
            "codux-kiro-hooks-{}-{}.json",
            std::process::id(),
            now_seconds().to_bits()
        ));
        fs::write(
            &root,
            r#"{
              "name": "Codux Managed",
              "description": "Codux runtime lifecycle hook bridge.",
              "prompt": "Codux managed runtime hook agent.",
              "hooks": {
                "agentSpawn": [
                  {
                    "matcher": "",
                    "command": "'/tmp/dmux-ai-state.sh' 'session-start' 'codux-tauri' 'kiro'",
                    "timeout_ms": 5000
                  }
                ],
                "stop": [
                  {
                    "matcher": "",
                    "command": "'/tmp/dmux-ai-state.sh' 'session-end' 'codux-tauri' 'kiro'",
                    "timeout_ms": 5000
                  }
                ]
              }
            }"#,
        )
        .unwrap();

        let status = tool_hook_config_status(
            &root,
            "kiro",
            &[("agentSpawn", "session-start"), ("stop", "session-end")],
        );

        assert!(status.configured);
        assert!(status.missing.is_empty());
        let _ = fs::remove_file(root);
    }

    #[test]
    fn kiro_agent_fields_are_completed_without_overwriting_user_values() {
        let mut root = serde_json::Map::new();
        root.insert(
            "name".to_string(),
            serde_json::Value::String("Custom Agent".to_string()),
        );

        ensure_kiro_agent_config_fields(&mut root);

        assert_eq!(
            root.get("name").and_then(|value| value.as_str()),
            Some("Custom Agent")
        );
        assert_eq!(
            root.get("description").and_then(|value| value.as_str()),
            Some("Codux runtime lifecycle hook bridge.")
        );
        assert_eq!(
            root.get("prompt").and_then(|value| value.as_str()),
            Some("Codux managed runtime hook agent.")
        );
    }

    #[test]
    fn install_gemini_hooks_disables_hook_notifications() {
        let mut config = serde_json::Map::new();
        config.insert(
            "hooksConfig".to_string(),
            serde_json::json!({ "notifications": true, "other": "kept" }),
        );
        disable_gemini_hook_notifications(&mut config);
        assert_eq!(
            config
                .get("hooksConfig")
                .and_then(|value| value.get("notifications"))
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            config
                .get("hooksConfig")
                .and_then(|value| value.get("other"))
                .and_then(|value| value.as_str()),
            Some("kept")
        );
    }

    #[test]
    fn opencode_runtime_envelope_maps_to_standard_hook_payload() {
        let payload = opencode_runtime_to_hook(AIToolUsageEnvelope {
            session_id: "term-1".to_string(),
            session_instance_id: Some("instance-1".to_string()),
            external_session_id: Some("open-1".to_string()),
            project_id: "project-1".to_string(),
            project_name: "Project".to_string(),
            project_path: Some("/tmp/project".to_string()),
            session_title: "Agent".to_string(),
            tool: "opencode".to_string(),
            model: Some("sonnet".to_string()),
            status: "completed".to_string(),
            response_state: Some("idle".to_string()),
            updated_at: 1000.0,
            input_tokens: Some(10),
            output_tokens: Some(20),
            total_tokens: Some(30),
            cached_input_tokens: Some(4),
        })
        .unwrap();

        assert_eq!(payload.kind, "turnCompleted");
        assert_eq!(payload.terminal_id, "term-1");
        assert_eq!(payload.tool, "opencode");
        assert_eq!(payload.ai_session_id.as_deref(), Some("open-1"));
        assert_eq!(payload.total_tokens, Some(30));
        let metadata = payload.metadata.unwrap();
        assert_eq!(metadata.source.as_deref(), Some("opencode-runtime"));
        assert_eq!(metadata.was_interrupted, Some(false));
        assert_eq!(metadata.has_completed_turn, Some(true));
    }

    #[test]
    fn runtime_frame_parser_accepts_standard_ai_hook_envelope() {
        let frame = br#"{
          "kind": "ai-hook",
          "payload": {
            "kind": "promptSubmitted",
            "terminalID": "term-1",
            "terminalInstanceID": "instance-1",
            "projectID": "project-1",
            "projectName": "Project",
            "projectPath": "/tmp/project",
            "sessionTitle": "Split 1",
            "tool": "codex",
            "aiSessionID": "rollout-1",
            "model": "gpt-5.5",
            "totalTokens": 12,
            "updatedAt": 1000
          }
        }"#;

        let payload = runtime_frame_to_hook(frame).unwrap();

        assert_eq!(payload.kind, "promptSubmitted");
        assert_eq!(payload.terminal_id, "term-1");
        assert_eq!(payload.terminal_instance_id.as_deref(), Some("instance-1"));
        assert_eq!(payload.project_id, "project-1");
        assert_eq!(payload.tool, "codex");
        assert_eq!(payload.total_tokens, Some(12));
    }

    #[test]
    fn runtime_frame_parser_accepts_utf8_bom_envelope() {
        let frame = b"\xEF\xBB\xBF{\"kind\":\"ai-hook\",\"payload\":{\"kind\":\"promptSubmitted\",\"terminalID\":\"term-1\",\"projectID\":\"project-1\",\"projectName\":\"Project\",\"sessionTitle\":\"Split 1\",\"tool\":\"codex\",\"updatedAt\":1000}}";

        let payload = runtime_frame_to_hook(frame).unwrap();

        assert_eq!(payload.kind, "promptSubmitted");
        assert_eq!(payload.terminal_id, "term-1");
        assert_eq!(payload.project_id, "project-1");
    }

    #[test]
    fn runtime_frame_parser_accepts_memory_refreshing_envelope() {
        let frame = br#"{
          "kind": "ai-hook",
          "payload": {
            "kind": "memoryRefreshing",
            "terminalID": "term-1",
            "projectID": "project-1",
            "projectName": "Project",
            "sessionTitle": "Claude",
            "tool": "claude",
            "updatedAt": 1000,
            "metadata": { "source": "post-compact" }
          }
        }"#;

        let payload = runtime_frame_to_hook(frame).unwrap();

        assert_eq!(payload.kind, "memoryRefreshing");
        assert_eq!(payload.tool, "claude");
        assert_eq!(
            payload
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.source.as_deref()),
            Some("post-compact")
        );
    }

    #[test]
    fn runtime_frame_parser_rejects_unknown_envelope_kind() {
        let frame = br#"{"kind":"unknown","payload":{}}"#;

        assert!(runtime_frame_to_hook(frame).is_none());
    }

    #[test]
    fn codex_usage_totals_exclude_cache_and_reasoning_from_standard_totals() {
        let usage = parse_usage_totals(&serde_json::json!({
            "input_tokens": 19446,
            "cached_input_tokens": 4480,
            "output_tokens": 323,
            "reasoning_output_tokens": 61,
            "total_tokens": 19769
        }))
        .unwrap();

        assert_eq!(usage.input_tokens, 14966);
        assert_eq!(usage.output_tokens, 262);
        assert_eq!(usage.cached_input_tokens, 4480);
        assert_eq!(usage.total_tokens, 15289);
    }

    #[test]
    fn codex_usage_totals_keep_raw_total_when_only_total_is_reported() {
        let usage = parse_usage_totals(&serde_json::json!({
            "input_tokens": 0,
            "output_tokens": 0,
            "cached_input_tokens": 7,
            "total_tokens": 42
        }))
        .unwrap();

        assert_eq!(usage.input_tokens, 42);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.cached_input_tokens, 7);
        assert_eq!(usage.total_tokens, 42);
    }

    #[test]
    fn codex_runtime_usage_matches_swift_total_last_resolution() {
        let total = UsageTotals {
            input_tokens: 110,
            output_tokens: 30,
            cached_input_tokens: 20,
            total_tokens: 140,
        };
        let base = UsageTotals {
            input_tokens: 100,
            output_tokens: 20,
            cached_input_tokens: 10,
            total_tokens: 120,
        };
        let last = UsageTotals {
            input_tokens: 10,
            output_tokens: 10,
            cached_input_tokens: 10,
            total_tokens: 20,
        };

        let usage = resolve_runtime_usage(Some(total), Some(base), Some(last)).unwrap();

        assert_eq!(usage.input_tokens, 110);
        assert_eq!(usage.output_tokens, 30);
        assert_eq!(usage.cached_input_tokens, 20);
        assert_eq!(usage.total_tokens, 140);
    }

    #[test]
    fn codex_runtime_usage_adds_last_usage_when_total_does_not_advance() {
        let base = UsageTotals {
            input_tokens: 100,
            output_tokens: 20,
            cached_input_tokens: 10,
            total_tokens: 120,
        };
        let last = UsageTotals {
            input_tokens: 8,
            output_tokens: 12,
            cached_input_tokens: 2,
            total_tokens: 20,
        };

        let usage = resolve_runtime_usage(Some(base.clone()), Some(base), Some(last)).unwrap();

        assert_eq!(usage.input_tokens, 108);
        assert_eq!(usage.output_tokens, 32);
        assert_eq!(usage.cached_input_tokens, 12);
        assert_eq!(usage.total_tokens, 140);
    }

    #[test]
    fn runtime_snapshot_sets_restored_session_baseline() {
        let mut core = AIRuntimeStateCore::default();
        assert!(apply_hook_unlocked(
            &mut core,
            test_hook("promptSubmitted", 1000.0)
        ));

        assert!(apply_runtime_snapshot_unlocked(
            &mut core,
            "terminal-1",
            AIRuntimeContextSnapshot {
                tool: "codex".to_string(),
                external_session_id: Some("session-1".to_string()),
                transcript_path: None,
                model: Some("gpt-5.5".to_string()),
                assistant_preview: None,
                input_tokens: 1_000,
                output_tokens: 200,
                cached_input_tokens: 3_000,
                total_tokens: 1_200,
                updated_at: 1005.0,
                started_at: Some(900.0),
                completed_at: None,
                response_state: Some("responding".to_string()),
                was_interrupted: false,
                has_completed_turn: false,
                session_origin: "restored".to_string(),
                source: "probe".to_string(),
            }
        ));

        let session = core.sessions.get("terminal-1").unwrap();
        assert_eq!(session.baseline_total_tokens, 1_200);
        assert_eq!(session.baseline_cached_input_tokens, 3_000);
        assert!(session.baseline_resolved);
        assert_eq!(
            project_totals_unlocked(&core, Some("project-1")).total_tokens,
            0
        );
        assert_eq!(
            project_totals_unlocked(&core, Some("project-1")).cached_input_tokens,
            0
        );
    }

    #[test]
    fn memory_refreshing_hook_keeps_session_running() {
        let mut core = AIRuntimeStateCore::default();
        let mut event = test_hook("memoryRefreshing", 1000.0);
        event.tool = "claude".to_string();
        event.metadata = Some(AIHookEventMetadata {
            source: Some("post-compact".to_string()),
            ..empty_metadata()
        });

        assert!(apply_hook_unlocked(&mut core, event));

        let session = core.sessions.get("terminal-1").unwrap();
        assert_eq!(session.state, "responding");
        assert!(session.is_running);
        assert_eq!(session.status, "running");
    }

    #[test]
    fn codex_transcript_abort_parses_as_interrupted_turn() {
        let transcript = write_temp_transcript(
            "codex-abort",
            &[
                r#"{"timestamp":"2026-04-21T03:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codex-project"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:01Z","type":"event_msg","payload":{"type":"task_started","started_at":1713668401}}"#,
                r#"{"timestamp":"2026-04-21T03:00:03Z","type":"event_msg","payload":{"type":"turn_aborted","completed_at":1713668403}}"#,
            ],
        );

        let parsed = parse_codex_runtime_state(&transcript, Some("/tmp/codex-project")).unwrap();

        assert_eq!(parsed.response_state.as_deref(), Some("idle"));
        assert!(parsed.was_interrupted);
        assert!(!parsed.has_completed_turn);
    }

    #[test]
    fn codex_transcript_tail_parser_reads_large_file_recent_rows() {
        let path = std::env::temp_dir().join(format!(
            "codex-tail-large-{}-{}.jsonl",
            std::process::id(),
            now_seconds().to_bits()
        ));
        let mut content = String::new();
        for index in 0..5_000 {
            content.push_str(&format!(
                r#"{{"timestamp":"2026-04-21T02:00:00Z","type":"event_msg","payload":{{"type":"agent_message","message":"old-{index}"}}}}"#
            ));
            content.push('\n');
        }
        content.push_str(
            r#"{"timestamp":"2026-04-21T03:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codex-project"}}"#,
        );
        content.push('\n');
        content.push_str(
            r#"{"timestamp":"2026-04-21T03:00:01Z","type":"event_msg","payload":{"type":"task_started","started_at":1713668401}}"#,
        );
        content.push('\n');
        fs::write(&path, content).unwrap();

        let parsed =
            parse_codex_runtime_state_tail(&path, Some("/tmp/codex-project"), 0.0, 0.0).unwrap();

        assert_eq!(parsed.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(parsed.response_state.as_deref(), Some("responding"));
        assert!(!parsed.has_completed_turn);
    }

    #[test]
    fn codex_transcript_user_message_after_completion_starts_next_turn() {
        let transcript = write_temp_transcript(
            "codex-queued-turn",
            &[
                r#"{"timestamp":"2026-04-21T03:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codex-project"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:01Z","type":"event_msg","payload":{"type":"task_started","started_at":1713668401}}"#,
                r#"{"timestamp":"2026-04-21T03:00:10Z","type":"event_msg","payload":{"type":"task_complete","completed_at":1713668410}}"#,
                r#"{"timestamp":"2026-04-21T03:00:12Z","type":"response_item","payload":{"type":"message","role":"user","content":"queued"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:12Z","type":"event_msg","payload":{"type":"user_message","message":"queued"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:14Z","type":"response_item","payload":{"type":"reasoning","summary":[{"text":"working"}]}}"#,
            ],
        );

        let parsed = parse_codex_runtime_state(&transcript, Some("/tmp/codex-project")).unwrap();

        assert_eq!(parsed.response_state.as_deref(), Some("responding"));
        assert_eq!(parsed.started_at, Some(1776740412.0));
        assert_eq!(parsed.completed_at, None);
        assert!(!parsed.has_completed_turn);
        assert!(!parsed.was_interrupted);
    }

    #[test]
    fn codex_probe_applies_interrupted_snapshot_without_stop_hook() {
        let transcript = write_temp_transcript(
            "codex-probe-abort",
            &[
                r#"{"timestamp":"2026-04-21T03:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codex-project"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:01Z","type":"event_msg","payload":{"type":"task_started","started_at":1713668401}}"#,
                r#"{"timestamp":"2026-04-21T03:00:03Z","type":"event_msg","payload":{"type":"turn_aborted","completed_at":1713668403}}"#,
            ],
        );
        let mut core = AIRuntimeStateCore::default();
        assert!(apply_hook_unlocked(
            &mut core,
            AIHookEventPayload {
                kind: "promptSubmitted".to_string(),
                terminal_id: "terminal-1".to_string(),
                terminal_instance_id: Some("instance-1".to_string()),
                project_id: "project-1".to_string(),
                project_name: "Project".to_string(),
                project_path: Some("/tmp/codex-project".to_string()),
                session_title: "Codex".to_string(),
                tool: "codex".to_string(),
                ai_session_id: Some("session-1".to_string()),
                model: Some("gpt-5.4".to_string()),
                input_tokens: None,
                output_tokens: None,
                cached_input_tokens: None,
                total_tokens: None,
                updated_at: 1713668401.0,
                metadata: Some(AIHookEventMetadata {
                    transcript_path: Some(transcript.display().to_string()),
                    ..AIHookEventMetadata {
                        transcript_path: None,
                        notification_type: None,
                        source: None,
                        reason: None,
                        cwd: None,
                        target_tool_name: None,
                        message: None,
                        was_interrupted: None,
                        has_completed_turn: None,
                    }
                }),
            }
        ));
        let request = probe_request_for_session(core.sessions.get("terminal-1").unwrap());
        let snapshot = probe_codex_runtime(&request).unwrap();

        assert!(apply_runtime_snapshot_unlocked(
            &mut core,
            "terminal-1",
            snapshot
        ));
        let session = core.sessions.get("terminal-1").unwrap();
        assert_eq!(session.state, "idle");
        assert!(session.was_interrupted);
        assert!(!session.has_completed_turn);
        assert!(matches!(
            completed_phase_unlocked(&core, "project-1"),
            AIProjectPhase::Completed {
                was_interrupted: true,
                ..
            }
        ));
    }

    #[test]
    fn runtime_snapshot_promotes_completed_session_back_to_responding() {
        let mut core = AIRuntimeStateCore::default();
        assert!(apply_hook_unlocked(
            &mut core,
            test_hook("promptSubmitted", 1000.0)
        ));
        assert!(apply_hook_unlocked(
            &mut core,
            AIHookEventPayload {
                kind: "turnCompleted".to_string(),
                total_tokens: Some(150),
                updated_at: 1010.0,
                metadata: Some(AIHookEventMetadata {
                    has_completed_turn: Some(true),
                    ..empty_metadata()
                }),
                ..test_hook("turnCompleted", 1010.0)
            }
        ));
        assert!(matches!(
            completed_phase_unlocked(&core, "project-1"),
            AIProjectPhase::Completed { .. }
        ));

        assert!(apply_runtime_snapshot_unlocked(
            &mut core,
            "terminal-1",
            AIRuntimeContextSnapshot {
                tool: "codex".to_string(),
                external_session_id: Some("session-1".to_string()),
                transcript_path: None,
                model: Some("gpt-5.5".to_string()),
                assistant_preview: None,
                input_tokens: 120,
                output_tokens: 80,
                cached_input_tokens: 0,
                total_tokens: 200,
                updated_at: 1020.0,
                started_at: Some(1020.0),
                completed_at: None,
                response_state: Some("responding".to_string()),
                was_interrupted: false,
                has_completed_turn: false,
                session_origin: "fresh".to_string(),
                source: "probe".to_string(),
            }
        ));

        let session = core.sessions.get("terminal-1").unwrap();
        assert_eq!(session.state, "responding");
        assert!(!session.has_completed_turn);
        assert!(matches!(
            project_phase_unlocked(&core, "project-1"),
            AIProjectPhase::Running { .. }
        ));
        assert_eq!(
            completed_phase_unlocked(&core, "project-1"),
            AIProjectPhase::Idle
        );
    }

    #[test]
    fn runtime_snapshot_promotes_needs_input_session_back_to_responding() {
        let mut core = AIRuntimeStateCore::default();
        assert!(apply_hook_unlocked(
            &mut core,
            test_hook("promptSubmitted", 1000.0)
        ));
        assert!(apply_hook_unlocked(
            &mut core,
            AIHookEventPayload {
                kind: "needsInput".to_string(),
                updated_at: 1005.0,
                metadata: Some(AIHookEventMetadata {
                    notification_type: Some("permission".to_string()),
                    ..empty_metadata()
                }),
                ..test_hook("needsInput", 1005.0)
            }
        ));
        assert!(matches!(
            project_phase_unlocked(&core, "project-1"),
            AIProjectPhase::NeedsInput { .. }
        ));

        assert!(apply_runtime_snapshot_unlocked(
            &mut core,
            "terminal-1",
            AIRuntimeContextSnapshot {
                tool: "codex".to_string(),
                external_session_id: Some("session-1".to_string()),
                transcript_path: None,
                model: Some("gpt-5.5".to_string()),
                assistant_preview: None,
                input_tokens: 120,
                output_tokens: 80,
                cached_input_tokens: 0,
                total_tokens: 200,
                updated_at: 1020.0,
                started_at: None,
                completed_at: None,
                response_state: Some("responding".to_string()),
                was_interrupted: false,
                has_completed_turn: false,
                session_origin: "fresh".to_string(),
                source: "probe".to_string(),
            }
        ));

        let session = core.sessions.get("terminal-1").unwrap();
        assert_eq!(session.state, "responding");
        assert!(matches!(
            project_phase_unlocked(&core, "project-1"),
            AIProjectPhase::Running { .. }
        ));
    }

    #[test]
    fn transcript_monitor_detects_tail_change_once_per_minimum_interval() {
        let transcript = write_temp_transcript("codex-tail", &["initial"]);
        let path = transcript.display().to_string();
        let mut monitors = HashMap::from([(
            "terminal-1".to_string(),
            TranscriptMonitor {
                path: path.clone(),
                signature: transcript_signature(Path::new(&path)),
                last_poll_at: None,
            },
        )]);
        fs::OpenOptions::new()
            .append(true)
            .open(&transcript)
            .unwrap()
            .write_all(b"next\n")
            .unwrap();

        assert_eq!(
            scan_transcript_monitors(&mut monitors, 100.0),
            vec!["terminal-1".to_string()]
        );
        fs::OpenOptions::new()
            .append(true)
            .open(&transcript)
            .unwrap()
            .write_all(b"again\n")
            .unwrap();
        assert!(scan_transcript_monitors(&mut monitors, 101.0).is_empty());
        assert_eq!(
            scan_transcript_monitors(&mut monitors, 103.0),
            vec!["terminal-1".to_string()]
        );
    }

    #[test]
    fn transcript_tail_poll_checks_completed_codex_sessions() {
        let transcript = write_temp_transcript("codex-tail-poll", &["initial"]);
        let path = transcript.display().to_string();
        let mut core = AIRuntimeStateCore::default();
        assert!(apply_hook_unlocked(
            &mut core,
            AIHookEventPayload {
                metadata: Some(AIHookEventMetadata {
                    transcript_path: Some(path),
                    has_completed_turn: Some(true),
                    ..empty_metadata()
                }),
                ..test_hook("turnCompleted", 1010.0)
            }
        ));
        let session = core.sessions.get("terminal-1").unwrap();
        assert_eq!(session.state, "idle");
        assert!(session.has_completed_turn);
        assert!(should_poll_session(session, "transcript-tail", 1020.0));
    }

    #[test]
    fn runtime_snapshot_backfills_codex_transcript_path_for_restored_session() {
        let transcript = write_temp_transcript(
            "codex-restored-transcript",
            &[
                r#"{"timestamp":"2026-04-21T03:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codex-project"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:01Z","type":"event_msg","payload":{"type":"task_started","started_at":1713668401}}"#,
            ],
        );
        let snapshot = probe_codex_runtime(&AIRuntimeProbeRequest {
            terminal_id: "terminal-1".to_string(),
            terminal_instance_id: Some("instance-1".to_string()),
            project_id: "project-1".to_string(),
            project_path: Some("/tmp/codex-project".to_string()),
            tool: "codex".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some(transcript.display().to_string()),
            started_at: Some(1713668400.0),
            updated_at: 1713668400.0,
        })
        .unwrap();
        let mut core = AIRuntimeStateCore::default();
        assert!(apply_hook_unlocked(
            &mut core,
            AIHookEventPayload {
                metadata: Some(empty_metadata()),
                ..test_hook("sessionStarted", 1713668400.0)
            }
        ));

        assert!(apply_runtime_snapshot_unlocked(
            &mut core,
            "terminal-1",
            snapshot
        ));

        let session = core.sessions.get("terminal-1").unwrap();
        assert_eq!(
            session.transcript_path.as_deref(),
            Some(transcript.to_str().unwrap())
        );
        assert!(is_codex_transcript_session(session));
    }

    #[test]
    fn claude_streaming_assistant_without_end_turn_stays_responding() {
        let transcript = write_temp_transcript(
            "claude-streaming",
            &[
                r#"{"timestamp":"2026-04-21T03:00:00Z","type":"user","cwd":"/tmp/claude-project","sessionId":"claude-1","message":{"role":"user","content":"hi"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:02Z","type":"assistant","cwd":"/tmp/claude-project","sessionId":"claude-1","message":{"role":"assistant","content":"working","stop_reason":null}}"#,
            ],
        );

        let parsed =
            parse_claude_log_runtime_state(&transcript, "/tmp/claude-project", "claude-1").unwrap();

        assert_eq!(parsed.response_state().as_deref(), Some("responding"));
        assert!(!parsed.has_completed_turn());
        assert!(!parsed.was_interrupted());
        assert_eq!(parsed.completed_at(), None);
    }

    #[test]
    fn claude_end_turn_and_interruption_follow_latest_event() {
        let transcript = write_temp_transcript(
            "claude-interrupt",
            &[
                r#"{"timestamp":"2026-04-21T03:00:00Z","type":"user","cwd":"/tmp/claude-project","sessionId":"claude-1","message":{"role":"user","content":"hi"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:02Z","type":"assistant","cwd":"/tmp/claude-project","sessionId":"claude-1","message":{"role":"assistant","content":"done","stop_reason":"end_turn"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:04Z","type":"user","cwd":"/tmp/claude-project","sessionId":"claude-1","message":{"role":"user","content":"again"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:05Z","type":"user","cwd":"/tmp/claude-project","sessionId":"claude-1","message":{"role":"user","content":"[Request interrupted by user]"}}"#,
            ],
        );

        let parsed =
            parse_claude_log_runtime_state(&transcript, "/tmp/claude-project", "claude-1").unwrap();

        assert_eq!(parsed.response_state().as_deref(), Some("idle"));
        assert!(parsed.was_interrupted());
        assert!(!parsed.has_completed_turn());
    }

    #[test]
    fn gemini_runtime_state_extracts_assistant_preview() {
        let transcript = write_temp_transcript(
            "gemini-preview",
            &[r#"{
              "sessionId": "gemini-session",
              "startTime": "2026-04-21T09:00:00Z",
              "lastUpdated": "2026-04-21T09:00:06Z",
              "messages": [
                {
                  "type": "user",
                  "timestamp": "2026-04-21T09:00:00Z",
                  "content": "hello"
                },
                {
                  "type": "gemini",
                  "timestamp": "2026-04-21T09:00:06Z",
                  "model": "gemini-2.5-pro",
                  "content": [{"text": "我先检查项目结构。"}, {"text": "然后确认入口和配置。"}],
                  "tokens": {
                    "input": 140,
                    "output": 60,
                    "thoughts": 15,
                    "cached": 25,
                    "total": 240
                  }
                }
              ]
            }"#],
        );

        let parsed = parse_gemini_runtime_state(&transcript).unwrap();

        assert_eq!(
            parsed.assistant_preview.as_deref(),
            Some("我先检查项目结构。\n然后确认入口和配置。")
        );
    }

    #[test]
    fn opencode_assistant_preview_extracts_nested_text() {
        let payload = serde_json::json!({
            "role": "assistant",
            "parts": [
                { "type": "text", "text": "我先检查项目结构。" },
                { "type": "text", "text": "然后确认入口和配置。" }
            ]
        });

        assert_eq!(
            opencode_assistant_preview(&payload).as_deref(),
            Some("我先检查项目结构。\n然后确认入口和配置。")
        );
    }

    #[test]
    fn codex_runtime_state_keeps_latest_assistant_preview() {
        let transcript = write_temp_transcript(
            "codex-preview",
            &[
                r#"{"timestamp":"2026-04-21T03:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/tmp/codex-project"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:01Z","type":"event_msg","payload":{"type":"agent_message","message":"旧的摘要"}}"#,
                r#"{"timestamp":"2026-04-21T03:00:02Z","type":"event_msg","payload":{"type":"agent_message","message":"新的实时摘要"}}"#,
            ],
        );

        let parsed = parse_codex_runtime_state(&transcript, Some("/tmp/codex-project")).unwrap();

        assert_eq!(parsed.assistant_preview.as_deref(), Some("新的实时摘要"));
    }

    fn write_temp_transcript(prefix: &str, rows: &[&str]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}.jsonl",
            std::process::id(),
            now_seconds().to_bits()
        ));
        fs::write(&path, rows.join("\n") + "\n").unwrap();
        path
    }

    fn test_hook(kind: &str, updated_at: f64) -> AIHookEventPayload {
        AIHookEventPayload {
            kind: kind.to_string(),
            terminal_id: "terminal-1".to_string(),
            terminal_instance_id: Some("instance-1".to_string()),
            project_id: "project-1".to_string(),
            project_name: "Project".to_string(),
            project_path: Some("/tmp/project".to_string()),
            session_title: "Codex".to_string(),
            tool: "codex".to_string(),
            ai_session_id: Some("session-1".to_string()),
            model: Some("gpt-5.5".to_string()),
            input_tokens: None,
            output_tokens: None,
            cached_input_tokens: None,
            total_tokens: None,
            updated_at,
            metadata: None,
        }
    }

    fn empty_metadata() -> AIHookEventMetadata {
        AIHookEventMetadata {
            transcript_path: None,
            notification_type: None,
            source: None,
            reason: None,
            cwd: None,
            target_tool_name: None,
            message: None,
            was_interrupted: None,
            has_completed_turn: None,
        }
    }
}
