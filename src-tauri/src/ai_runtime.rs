use crate::app_settings::{locale_from_language_setting, AppSettingsStore};
use crate::i18n::translate;
use crate::memory::MemoryStore;
use crate::notify_channels::{dispatch_notification_channels, NotificationDispatchRequest};
use crate::project_store::ProjectStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::net::Shutdown;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::async_runtime::{channel, Receiver, Sender};
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::Emitter;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

const RUNNING_STALE_SECONDS: f64 = 90.0;
const POLL_INTERVAL_SECONDS: u64 = 12;
const RUNNING_STATE_RENEWAL_SECONDS: f64 = 30.0;
const CODEX_INTERVAL_POLL_MINIMUM_SECONDS: f64 = 60.0;

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
    pub terminals: Vec<AIRuntimeTerminalState>,
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
    socket_path: PathBuf,
    registry: Arc<AIRuntimeRegistry>,
    supervisor: Arc<AIRuntimeSupervisor>,
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
        Self {
            root_dir,
            wrapper_bin_dir,
            zdotdir,
            hook_script,
            managed_hook_script,
            socket_path,
            registry: Arc::new(AIRuntimeRegistry::default()),
            supervisor: Arc::new(AIRuntimeSupervisor::new(settings, memory, projects)),
        }
    }

    pub fn prepare(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.wrapper_bin_dir.parent().unwrap_or(&self.root_dir))
            .map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.wrapper_bin_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.zdotdir).map_err(|error| error.to_string())?;
        fs::create_dir_all(runtime_temp_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.claude_session_map_dir()).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.opencode_session_map_dir()).map_err(|error| error.to_string())?;

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
        stage_runtime_asset(
            "scripts/wrappers/tool-wrapper.sh",
            &self
                .root_dir
                .join("scripts")
                .join("wrappers")
                .join("tool-wrapper.sh"),
            true,
        )?;
        stage_runtime_asset(
            "scripts/wrappers/opencode-config/package.json",
            &self
                .root_dir
                .join("scripts")
                .join("wrappers")
                .join("opencode-config")
                .join("package.json"),
            false,
        )?;
        stage_runtime_asset(
            "scripts/wrappers/opencode-config/plugins/dmux-runtime.js",
            &self
                .root_dir
                .join("scripts")
                .join("wrappers")
                .join("opencode-config")
                .join("plugins")
                .join("dmux-runtime.js"),
            false,
        )?;

        for bin_name in [
            "codex",
            "claude",
            "claude-code",
            "gemini",
            "opencode",
            "codux-ssh",
        ] {
            stage_runtime_asset(
                &format!("scripts/wrappers/bin/{bin_name}"),
                &self.wrapper_bin_dir.join(bin_name),
                true,
            )?;
        }

        self.install_managed_hook_configs()?;

        Ok(())
    }

    pub fn start_listener(&self, app: AppHandle) -> Result<(), String> {
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
                ("Stop", "stop", 10, false),
                ("StopFailure", "stop-failure", 10, false),
                ("SessionEnd", "session-end", 1, false),
                ("PermissionRequest", "permission-request", 5, true),
                ("PermissionDenied", "permission-denied", 5, true),
                ("Elicitation", "elicitation", 10, false),
                ("ElicitationResult", "elicitation-result", 10, false),
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
        Ok(())
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
}

struct AIRuntimeSupervisor {
    hook_tx: Sender<AIRuntimeSupervisorMessage>,
    hook_rx: Mutex<Option<Receiver<AIRuntimeSupervisorMessage>>>,
    state: Arc<AIRuntimeStateStore>,
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
        let settings = Arc::clone(&self.settings);
        let memory = Arc::clone(&self.memory);
        let projects = Arc::clone(&self.projects);
        let poll_tx = self.hook_tx.clone();
        start_ai_runtime_poll_loop(poll_tx);
        tauri::async_runtime::spawn(ai_runtime_supervisor_loop(
            hook_rx, app, registry, state, settings, memory, projects,
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
    settings: Arc<AppSettingsStore>,
    memory: Arc<MemoryStore>,
    projects: Arc<ProjectStore>,
) {
    while let Some(message) = hook_rx.recv().await {
        match message {
            AIRuntimeSupervisorMessage::HookFrame(frame) => {
                let Some(payload) = runtime_frame_to_hook(&frame) else {
                    continue;
                };
                let _ = app.emit(
                    "ai-runtime:event",
                    AIRuntimeEvent::Hook {
                        payload: payload.clone(),
                    },
                );
                let mutation = state.apply_hook(payload);
                if mutation.did_change {
                    emit_runtime_state(&app, &state);
                }
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
                    apply_runtime_window_state(&app, &snapshot, &settings);
                    continue;
                }
                let terminal_snapshot = registry.snapshot();
                let mut mutation = state.reconcile_bridge_snapshot(&terminal_snapshot);
                for session in tracked {
                    if !should_poll_session(&session, "interval", now_seconds()) {
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
                        mutation
                            .merge(state.apply_runtime_snapshot(&session.terminal_id, snapshot));
                    }
                }
                if mutation.did_change {
                    emit_runtime_state(&app, &state);
                }
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
        let _ = window.set_badge_label(count.map(|value| value.to_string()));
        let progress = if snapshot.running_count > 0 {
            ProgressBarState {
                status: Some(ProgressBarStatus::Normal),
                progress: Some(35),
            }
        } else {
            ProgressBarState {
                status: Some(ProgressBarStatus::None),
                progress: None,
            }
        };
        let _ = window.set_progress_bar(progress);
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
    dispatch_completion_notification(app, Arc::clone(&settings), completion.clone());
    if let Some(session) = completion.session {
        memory.handle_completed_session(settings, projects.projects_snapshot(), session);
    }
}

fn locale_from_setting(language: &str) -> String {
    locale_from_language_setting(language)
}

#[derive(Default)]
struct AIRuntimeStateCore {
    sessions: HashMap<String, AISessionSnapshot>,
    logical_baselines: HashMap<String, i64>,
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
    let baseline_total_tokens = base
        .map(|session| session.baseline_total_tokens)
        .or_else(|| {
            logical_key
                .as_ref()
                .and_then(|key| core.logical_baselines.get(key).copied())
        })
        .unwrap_or(total_tokens);
    if let Some(key) = logical_key {
        core.logical_baselines
            .entry(key)
            .or_insert(baseline_total_tokens);
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
        cached_input_tokens: number_or(
            base.map(|session| session.cached_input_tokens),
            event.cached_input_tokens,
        ),
        total_tokens,
        baseline_total_tokens,
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

    if snapshot.response_state.as_deref() == Some("responding") {
        if !session.was_interrupted && !session.has_completed_turn {
            state = "responding".to_string();
            was_interrupted = false;
            has_completed_turn = false;
            let started = snapshot.started_at.unwrap_or(snapshot_updated_at);
            active_turn_started_at = active_turn_started_at.or(Some(started));
            runtime_turn_started_at = runtime_turn_started_at.or(Some(started));
        }
    } else if snapshot.response_state.as_deref() == Some("idle")
        && (session.state == "responding"
            || session.state == "needsInput"
            || snapshot.was_interrupted
            || snapshot.has_completed_turn)
    {
        state = "idle".to_string();
        active_turn_started_at = None;
        runtime_turn_started_at = None;
        was_interrupted = snapshot.was_interrupted;
        has_completed_turn = snapshot.has_completed_turn || !was_interrupted;
    }

    if let Some(started_at) = active_turn_started_at {
        note_latest_active_started_at(core, &session.project_id, started_at);
    }

    let next = AISessionSnapshot {
        tool: canonical_tool_name(&snapshot.tool).unwrap_or(session.tool.clone()),
        ai_session_id: normalized_string(snapshot.external_session_id.as_deref())
            .or(session.ai_session_id.clone()),
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
        updated_at: snapshot_updated_at,
        active_turn_started_at,
        runtime_turn_started_at,
        was_interrupted,
        has_completed_turn,
        latest_assistant_preview: normalized_string(snapshot.assistant_preview.as_deref())
            .or(session.latest_assistant_preview.clone()),
        ..session
    };

    if core.sessions.get(terminal_id) == Some(&next) {
        return false;
    }
    core.sessions.insert(terminal_id.to_string(), next);
    true
}

fn resolve_hook_event(
    event: AIHookEventPayload,
    current_session: Option<&AISessionSnapshot>,
) -> AIHookEventPayload {
    match canonical_tool_name(&event.tool).as_deref() {
        Some("codex") => resolve_codex_hook_event(event, current_session),
        Some("claude") => resolve_claude_hook_event(event, current_session),
        Some("gemini") => resolve_gemini_hook_event(event, current_session),
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

fn should_poll_session(session: &AISessionSnapshot, reason: &str, now: f64) -> bool {
    if canonical_tool_name(&session.tool).as_deref() == Some("codex")
        && normalized_string(session.transcript_path.as_deref()).is_some()
        && reason == "interval"
        && now - session.updated_at < CODEX_INTERVAL_POLL_MINIMUM_SECONDS
    {
        return false;
    }
    session.state == "responding" || session.state == "needsInput" || !session.has_completed_turn
}

fn next_state(kind: &str, metadata: Option<&AIHookEventMetadata>) -> &'static str {
    match kind {
        "promptSubmitted" => "responding",
        "sessionStarted" => "idle",
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
    let parsed = parse_codex_runtime_state(&file_path, Some(&project_path))?;
    Some(AIRuntimeContextSnapshot {
        tool: "codex".to_string(),
        external_session_id: normalized_string(request.external_session_id.as_deref()),
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
    Some(AIRuntimeContextSnapshot {
        tool: "claude".to_string(),
        external_session_id: Some(external_id),
        model: aggregate.model,
        assistant_preview: aggregate.assistant_preview,
        input_tokens: aggregate.input_tokens,
        output_tokens: aggregate.output_tokens,
        cached_input_tokens: aggregate.cached_input_tokens,
        total_tokens: aggregate.total_tokens,
        updated_at: aggregate.updated_at.max(request.updated_at),
        started_at: aggregate.started_at,
        completed_at: aggregate.completed_at,
        response_state: aggregate.response_state,
        was_interrupted: aggregate.was_interrupted,
        has_completed_turn: aggregate.has_completed_turn,
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
        current_launch_match
            .or(preferred_match)
            .or(candidate_match)?
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
        model: state.model,
        assistant_preview: None,
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
    let conn = rusqlite::Connection::open(database_path).ok()?;
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
        } else if role == "assistant"
            && is_opencode_final_assistant_finish(finish_reason, completed_at)
        {
            last_completion_at = last_completion_at.max(completed_at.unwrap_or(created_at));
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
        model: latest_model,
        assistant_preview: None,
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

fn runtime_temp_dir() -> PathBuf {
    std::env::temp_dir().join("codux-tauri")
}

fn runtime_root_dir() -> PathBuf {
    runtime_temp_dir().join("runtime-root")
}

fn runtime_assets_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime-assets")
}

fn install_tool_hooks(
    path: &Path,
    tool: &str,
    definitions: &[(&str, &str, i64, bool)],
    runtime: &AIRuntimeBridge,
) -> Result<(), String> {
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
        let command = hook_command(runtime.managed_hook_script(), action, "codux-tauri", tool);
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
    write_json_object(path, root)
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
    command.contains("dmux-ai-state.sh")
        && command.contains(&shell_quote(action))
        && command.contains(&shell_quote("codex"))
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
    let prefix = "[hooks.state.\"";
    let suffix = "\"]";
    if !trimmed.starts_with(prefix) || !trimmed.ends_with(suffix) {
        return None;
    }
    Some(
        trimmed[prefix.len()..trimmed.len() - suffix.len()]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\"),
    )
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
    command.contains("dmux-ai-state.sh")
        && command.contains(&shell_quote(action))
        && tool
            .map(|tool| command.contains(&shell_quote(tool)))
            .unwrap_or(true)
}

fn hook_command(helper_script: &Path, action: &str, owner: &str, tool: &str) -> String {
    [
        shell_quote(&helper_script.display().to_string()),
        shell_quote(action),
        shell_quote(owner),
        shell_quote(tool),
    ]
    .join(" ")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn stage_runtime_asset(
    relative_path: &str,
    destination: &Path,
    executable: bool,
) -> Result<(), String> {
    let source = runtime_assets_root().join(relative_path);
    let content = fs::read(&source)
        .map_err(|error| format!("read asset {} failed: {error}", source.display()))?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let should_write = match fs::read(destination) {
        Ok(existing) => existing != content,
        Err(_) => true,
    };

    if should_write {
        fs::write(destination, &content).map_err(|error| error.to_string())?;
    }

    #[cfg(unix)]
    if executable {
        let permissions = fs::Permissions::from_mode(0o755);
        let _ = fs::set_permissions(destination, permissions);
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
}

fn parse_codex_runtime_state(
    file_path: &Path,
    project_path: Option<&str>,
) -> Option<CodexParsedState> {
    let file = fs::File::open(file_path).ok()?;
    let reader = BufReader::new(file);
    let mut state = CodexParsedState {
        origin: "unknown".to_string(),
        ..Default::default()
    };
    let mut latest_cumulative_usage: Option<UsageTotals> = None;
    let mut usage_at_turn_start: Option<UsageTotals> = None;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let timestamp = row
            .get("timestamp")
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds);
        if let Some(timestamp) = timestamp {
            state.updated_at = Some(state.updated_at.unwrap_or(timestamp).max(timestamp));
        }
        let row_type = row.get("type").and_then(|value| value.as_str());
        let payload = row.get("payload").unwrap_or(&serde_json::Value::Null);

        if state.assistant_preview.is_none() {
            state.assistant_preview = codex_assistant_preview(row_type, payload);
        }

        if row_type == Some("turn_context") {
            if project_path
                .map(|project| payload.get("cwd").and_then(|value| value.as_str()) == Some(project))
                .unwrap_or(true)
            {
                if let Some(model) = payload
                    .get("model")
                    .and_then(|value| value.as_str())
                    .and_then(|value| normalized_string(Some(value)))
                {
                    state.model = Some(model);
                }
            }
            continue;
        }

        let event_type = payload.get("type").and_then(|value| value.as_str());
        let is_final_answer = (row_type == Some("event_msg")
            && event_type == Some("agent_message")
            && payload.get("phase").and_then(|value| value.as_str()) == Some("final_answer"))
            || (row_type == Some("response_item")
                && event_type == Some("message")
                && payload.get("phase").and_then(|value| value.as_str()) == Some("final_answer"));
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
                state.started_at = payload
                    .get("started_at")
                    .and_then(|value| value.as_f64())
                    .or(timestamp);
                usage_at_turn_start = latest_cumulative_usage.clone();
                state.was_interrupted = false;
                state.has_completed_turn = false;
            }
            Some("task_complete") => {
                let completed_at = payload
                    .get("completed_at")
                    .and_then(|value| value.as_f64())
                    .or(timestamp);
                if completed_at >= state.completed_at {
                    state.completed_at = completed_at;
                    state.was_interrupted = false;
                    state.has_completed_turn = true;
                }
            }
            Some("turn_aborted") => {
                let completed_at = payload
                    .get("completed_at")
                    .and_then(|value| value.as_f64())
                    .or(timestamp);
                if completed_at >= state.completed_at {
                    state.completed_at = completed_at;
                    state.was_interrupted = true;
                    state.has_completed_turn = false;
                }
            }
            Some("token_count") => {
                let info = payload.get("info").unwrap_or(&serde_json::Value::Null);
                let total_usage = info.get("total_token_usage").and_then(parse_usage_totals);
                let last_usage = info.get("last_token_usage").and_then(parse_usage_totals);
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

#[derive(Clone, Default)]
struct UsageTotals {
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    total_tokens: i64,
}

fn parse_usage_totals(value: &serde_json::Value) -> Option<UsageTotals> {
    let object = value.as_object()?;
    let input_tokens = json_i64(object.get("input_tokens"))
        + json_i64(object.get("cached_input_tokens"))
        + json_i64(object.get("cache_read_input_tokens"));
    let output_tokens = json_i64(object.get("output_tokens"));
    let cached_input_tokens = json_i64(object.get("cached_input_tokens"))
        + json_i64(object.get("cache_read_input_tokens"));
    let total_tokens = object
        .get("total_tokens")
        .and_then(|value| value.as_i64())
        .unwrap_or(input_tokens + output_tokens);
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

fn resolve_runtime_usage(
    total_usage: Option<UsageTotals>,
    base_usage: Option<UsageTotals>,
    last_usage: Option<UsageTotals>,
) -> Option<UsageTotals> {
    if let (Some(total), Some(base)) = (total_usage.clone(), base_usage) {
        return Some(UsageTotals {
            input_tokens: (total.input_tokens - base.input_tokens).max(0),
            output_tokens: (total.output_tokens - base.output_tokens).max(0),
            cached_input_tokens: (total.cached_input_tokens - base.cached_input_tokens).max(0),
            total_tokens: (total.total_tokens - base.total_tokens).max(0),
        });
    }
    last_usage.or(total_usage)
}

fn codex_assistant_preview(row_type: Option<&str>, payload: &serde_json::Value) -> Option<String> {
    let payload_type = payload.get("type").and_then(|value| value.as_str());
    match row_type {
        Some("event_msg") if payload_type == Some("agent_message") => {
            sanitized_preview_from_values(&[
                payload.get("message"),
                payload.get("text"),
                payload.get("content"),
            ])
        }
        Some("response_item") if payload_type == Some("reasoning") => {
            sanitized_preview_from_values(&[
                payload.get("summary"),
                payload.get("summary_text"),
                payload.get("text"),
            ])
        }
        Some("response_item") if payload_type == Some("agentMessage") => {
            sanitized_preview_from_values(&[
                payload.get("text"),
                payload.get("content"),
                payload.get("message"),
            ])
        }
        Some("response_item") if payload_type == Some("message") => {
            sanitized_preview_from_values(&[
                payload.get("content"),
                payload.get("message"),
                payload.get("text"),
            ])
        }
        _ => None,
    }
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
    started_at: Option<f64>,
    completed_at: Option<f64>,
    response_state: Option<String>,
    was_interrupted: bool,
    has_completed_turn: bool,
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
            started_at: min_option(self.started_at, other.started_at),
            completed_at: max_option(self.completed_at, other.completed_at),
            response_state: if other.updated_at >= self.updated_at {
                other.response_state
            } else {
                self.response_state
            },
            was_interrupted: self.was_interrupted || other.was_interrupted,
            has_completed_turn: self.has_completed_turn || other.has_completed_turn,
        }
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
    let mut last_user_at: Option<f64> = None;
    let mut last_assistant_at: Option<f64> = None;

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
        aggregate.started_at = min_option(aggregate.started_at, Some(timestamp));
        let message = row.get("message").unwrap_or(&serde_json::Value::Null);
        let role = message
            .get("role")
            .and_then(|value| value.as_str())
            .or_else(|| row.get("type").and_then(|value| value.as_str()));
        if role == Some("user") {
            last_user_at = Some(timestamp);
        }
        if role == Some("assistant") {
            last_assistant_at = Some(timestamp);
            aggregate.completed_at = Some(timestamp);
            aggregate.has_completed_turn = true;
            if aggregate.assistant_preview.is_none() {
                aggregate.assistant_preview =
                    sanitized_preview_from_values(&[message.get("content"), row.get("content")]);
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
            aggregate.total_tokens += json_i64(usage.get("input_tokens"))
                + json_i64(usage.get("output_tokens"))
                + json_i64(usage.get("cache_creation_input_tokens"))
                + json_i64(usage.get("cache_read_input_tokens"));
        }
        if is_claude_interrupted_row(&row) {
            aggregate.was_interrupted = true;
            aggregate.has_completed_turn = false;
            aggregate.completed_at = Some(timestamp);
        }
    }

    if !matched {
        return None;
    }
    aggregate.response_state = match (last_user_at, last_assistant_at) {
        (Some(user_at), Some(assistant_at)) if assistant_at >= user_at => Some("idle".to_string()),
        (Some(_), _) => Some("responding".to_string()),
        _ => Some("idle".to_string()),
    };
    if aggregate.response_state.as_deref() == Some("idle") && !aggregate.was_interrupted {
        aggregate.has_completed_turn = true;
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
    let temp_dir = home_dir().join(".gemini").join("tmp");
    let mut dirs = Vec::new();
    let projects_path = home_dir().join(".gemini").join("projects.json");
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

fn min_option(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn max_option(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
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
    fn runtime_frame_parser_rejects_unknown_envelope_kind() {
        let frame = br#"{"kind":"unknown","payload":{}}"#;

        assert!(runtime_frame_to_hook(frame).is_none());
    }
}
