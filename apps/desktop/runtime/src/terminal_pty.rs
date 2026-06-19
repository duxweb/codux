use crate::ai_runtime::{
    AIHookEventMetadata, AIHookEventPayload, AIRuntimeBridge, AIRuntimeTerminalBinding,
    canonical_tool_name,
};
use anyhow::{Context, Result, anyhow};
use codux_terminal_core::{
    HeadlessTerminalScreen, TerminalDriver as CoreTerminalDriver, TerminalEventSink,
    TerminalLaunchConfig, TerminalScreenSnapshot,
    TerminalSessionHandle as CoreTerminalSessionHandle,
};
pub use codux_terminal_core::{TerminalEvent, TerminalSessionSnapshot, TerminalViewportState};
use codux_terminal_pty::{
    LocalPtyCommandMode, LocalPtyProcessHandle, LocalPtySpawnConfig, spawn_local_pty,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[cfg(not(windows))]
use std::{
    process::{Command, Stdio},
    sync::OnceLock,
};
use uuid::Uuid;

#[cfg(windows)]
const PATH_SEPARATOR: char = ';';
#[cfg(not(windows))]
const PATH_SEPARATOR: char = ':';

#[cfg(windows)]
const FALLBACK_PATH: &str = "C:\\Windows\\System32;C:\\Windows;C:\\Windows\\System32\\Wbem;C:\\Windows\\System32\\WindowsPowerShell\\v1.0";
#[cfg(not(windows))]
const FALLBACK_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin";

const INPUT_CAPTURE_LIMIT: usize = 20;
const OUTPUT_CAPTURE_LIMIT: usize = 16 * 1024;
const MIN_HISTORY_BYTES: usize = 128 * 1024;
const MAX_CONFIGURED_HISTORY_BYTES: usize = 8 * 1024 * 1024;
const TERMINAL_VIEWPORT_LEASE_TTL: Duration = Duration::from_secs(20);
const COMMON_PASSTHROUGH_ENV_KEYS: &[&str] = &[
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "LC_COLLATE",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_MONETARY",
    "LC_MEASUREMENT",
    "LC_IDENTIFICATION",
    "LC_PAPER",
    "LC_NAME",
    "LC_ADDRESS",
    "LC_TELEPHONE",
    "LC_RESPONSETIME",
];
#[cfg(unix)]
const UNIX_PASSTHROUGH_ENV_KEYS: &[&str] = &["TMPDIR", "SSH_AUTH_SOCK", "__CF_USER_TEXT_ENCODING"];
#[cfg(windows)]
const WINDOWS_PASSTHROUGH_ENV_KEYS: &[&str] = &[
    "SystemRoot",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "USERNAME",
    "USERDOMAIN",
    "OneDrive",
    "PROCESSOR_ARCHITECTURE",
    "PSModulePath",
];
const DOTENV_KEYS: &[&str] = &[
    "GEMINI_API_KEY",
    "GEMINI_MODEL",
    "GOOGLE_API_KEY",
    "GOOGLE_GEMINI_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_BASE_URL",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "CODEX_HOME",
    "OPENCODE_API_KEY",
    "OPENCODE_BASE_URL",
    "CODEWHALE_PROVIDER",
    "DEEPSEEK_API_KEY",
    "DEEPSEEK_BASE_URL",
    "DEEPSEEK_MODEL",
    "DEEPSEEK_AUTH_TOKEN",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
];

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPtyConfig {
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub command: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub scrollback_lines: Option<usize>,
    pub env: Option<HashMap<String, String>>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub terminal_id: Option<String>,
    pub slot_id: Option<String>,
    pub session_key: Option<String>,
    pub title: Option<String>,
    pub tool: Option<String>,
    /// When set, the terminal runs on this remote device over the controller.
    pub host_device_id: Option<String>,
    pub support_dir: Option<PathBuf>,
    pub runtime_root: Option<PathBuf>,
    pub session_instance_id: Option<String>,
    pub tool_permissions_file: Option<PathBuf>,
    pub memory_workspace_root: Option<PathBuf>,
    pub memory_prompt_file: Option<PathBuf>,
    pub memory_index_file: Option<PathBuf>,
}

impl From<TerminalLaunchConfig> for TerminalPtyConfig {
    fn from(config: TerminalLaunchConfig) -> Self {
        Self {
            cwd: config.cwd,
            shell: config.shell,
            command: config.command,
            cols: config.cols,
            rows: config.rows,
            scrollback_lines: config.scrollback_lines,
            env: config.env,
            project_id: config.project_id,
            project_name: config.project_name,
            terminal_id: config.terminal_id,
            slot_id: config.slot_id,
            session_key: config.session_key,
            title: config.title,
            tool: config.tool,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalLaunchContext {
    pub project_id: String,
    pub project_name: String,
    pub project_path: PathBuf,
    pub support_dir: PathBuf,
    pub runtime_root: PathBuf,
    pub terminal_id: Option<String>,
    pub slot_id: Option<String>,
    pub session_key: Option<String>,
    pub session_title: Option<String>,
    pub session_cwd: Option<PathBuf>,
    pub session_instance_id: Option<String>,
    pub tool_permissions_file: Option<PathBuf>,
    pub memory_workspace_root: Option<PathBuf>,
    pub memory_prompt_file: Option<PathBuf>,
    pub memory_index_file: Option<PathBuf>,
    pub host_device_id: Option<String>,
}

impl TerminalLaunchContext {
    pub fn to_config(&self) -> TerminalPtyConfig {
        TerminalPtyConfig {
            cwd: Some(
                self.session_cwd
                    .as_ref()
                    .unwrap_or(&self.project_path)
                    .display()
                    .to_string(),
            ),
            project_id: Some(self.project_id.clone()),
            project_name: Some(self.project_name.clone()),
            terminal_id: self.terminal_id.clone(),
            slot_id: self.slot_id.clone(),
            session_key: self.session_key.clone(),
            title: self.session_title.clone(),
            support_dir: Some(self.support_dir.clone()),
            runtime_root: Some(self.runtime_root.clone()),
            session_instance_id: self.session_instance_id.clone(),
            tool_permissions_file: self.tool_permissions_file.clone(),
            memory_workspace_root: self.memory_workspace_root.clone(),
            memory_prompt_file: self.memory_prompt_file.clone(),
            memory_index_file: self.memory_index_file.clone(),
            host_device_id: self.host_device_id.clone(),
            scrollback_lines: None,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug)]
struct TerminalViewportLease {
    state: TerminalViewportState,
    expires_at: Instant,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalInputSnapshot {
    pub bytes: usize,
    pub history: Vec<TerminalCapturedInput>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalCapturedInput {
    pub text: String,
    pub bytes: usize,
    pub timestamp: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalOutputSnapshot {
    pub bytes: usize,
    pub tail: String,
}

pub type EventSink = Arc<dyn Fn(TerminalEvent) -> bool + Send + Sync + 'static>;

pub struct TerminalManager {
    sessions: Arc<parking_lot::Mutex<HashMap<String, Arc<TerminalPtySession>>>>,
    ai_runtime: Option<Arc<AIRuntimeBridge>>,
    viewport_lease_watcher_started: std::sync::Once,
}

#[derive(Clone, Debug, Default)]
struct RequestedTerminalIdentity {
    cwd: Option<String>,
    project_id: Option<String>,
    session_key: Option<String>,
}

impl RequestedTerminalIdentity {
    fn from_config(config: &TerminalPtyConfig, context: Option<&TerminalLaunchContext>) -> Self {
        Self {
            cwd: requested_terminal_cwd(config, context),
            project_id: config
                .project_id
                .clone()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| context.map(|context| context.project_id.clone())),
            session_key: config
                .session_key
                .clone()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| context.and_then(|context| context.session_key.clone())),
        }
    }
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            ai_runtime: None,
            viewport_lease_watcher_started: std::sync::Once::new(),
        }
    }

    pub fn with_ai_runtime(ai_runtime: Arc<AIRuntimeBridge>) -> Self {
        Self {
            sessions: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            ai_runtime: Some(ai_runtime),
            viewport_lease_watcher_started: std::sync::Once::new(),
        }
    }

    pub fn list(&self) -> Vec<TerminalSessionSnapshot> {
        self.sessions
            .lock()
            .values()
            .map(|session| session.info())
            .collect()
    }

    pub fn create<F>(&self, config: TerminalPtyConfig, emit: F) -> Result<String>
    where
        F: Fn(TerminalEvent) + Send + Sync + 'static,
    {
        self.create_with_sink(
            config,
            Arc::new(move |event| {
                emit(event);
                true
            }),
        )
    }

    pub fn create_with_sink(&self, config: TerminalPtyConfig, emit: EventSink) -> Result<String> {
        let (session, _) = self.attach_or_create_with_context(config, None, emit)?;
        let id = session.id().to_string();
        Ok(id)
    }

    pub fn ensure_session_with_context(
        &self,
        config: TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
    ) -> Result<String> {
        let requested_id = config
            .terminal_id
            .clone()
            .filter(|value| !value.trim().is_empty());
        let requested_identity = RequestedTerminalIdentity::from_config(&config, context);
        if let Some(session) = requested_id
            .as_deref()
            .and_then(|id| self.sessions.lock().get(id).cloned())
        {
            if session.matches_requested_identity(&requested_identity) {
                return Ok(session.id().to_string());
            }
            self.remove_incompatible_session(&session, &requested_identity);
        }

        if let Some(ai_runtime) = &self.ai_runtime {
            ai_runtime.ensure_started().map_err(anyhow::Error::msg)?;
        }
        let (session, _writer, reader) = TerminalPtySession::spawn(config, context, None)?;
        let session = Arc::new(session);
        let id = session.id().to_string();
        self.register_ai_runtime_terminal(&session);
        let event_subscribers = session.event_subscribers.clone();
        self.sessions.lock().insert(id.clone(), session);
        self.ensure_viewport_lease_watcher();
        spawn_headless_reader(id.clone(), reader, event_subscribers);
        Ok(id)
    }

    pub fn attach_or_create_with_context(
        &self,
        config: TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
        emit: EventSink,
    ) -> Result<(Arc<TerminalPtySession>, flume::Receiver<Vec<u8>>)> {
        let requested_id = config
            .terminal_id
            .clone()
            .filter(|value| !value.trim().is_empty());
        let requested_identity = RequestedTerminalIdentity::from_config(&config, context);
        if let Some(session) = requested_id
            .as_deref()
            .and_then(|id| self.sessions.lock().get(id).cloned())
        {
            if session.matches_requested_identity(&requested_identity) {
                session.subscribe_events(emit);
                let rx = session.subscribe_output(true);
                return Ok((session, rx));
            }
            self.remove_incompatible_session(&session, &requested_identity);
        }

        if let Some(ai_runtime) = &self.ai_runtime {
            ai_runtime.ensure_started().map_err(anyhow::Error::msg)?;
        }
        let (session, _writer, reader) =
            TerminalPtySession::spawn(config, context, Some(emit.clone()))?;
        let session = Arc::new(session);
        let id = session.id().to_string();
        self.register_ai_runtime_terminal(&session);
        let rx = session.subscribe_output(true);
        let event_subscribers = session.event_subscribers.clone();
        self.sessions.lock().insert(id.clone(), session.clone());
        self.ensure_viewport_lease_watcher();
        spawn_headless_reader(id, reader, event_subscribers);
        Ok((session, rx))
    }

    pub fn write(&self, session_id: &str, data: &[u8]) -> Result<()> {
        self.session(session_id)?.write(data)
    }

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        self.session(session_id)?.resize(cols, rows)
    }

    pub fn claim_viewport(&self, session_id: &str, owner: &str) -> Result<TerminalViewportState> {
        self.session(session_id)?.claim_viewport(owner)
    }

    pub fn touch_viewport_lease(&self, session_id: &str, owner: &str) {
        if let Ok(session) = self.session(session_id) {
            session.clone_handle().touch_viewport_lease(owner);
        }
    }

    pub fn scroll_screen_lines(
        &self,
        session_id: &str,
        lines: i32,
    ) -> Result<TerminalScreenSnapshot> {
        Ok(self.session(session_id)?.scroll_screen_lines(lines))
    }

    pub fn scroll_screen_to_bottom(&self, session_id: &str) -> Result<TerminalScreenSnapshot> {
        Ok(self.session(session_id)?.scroll_screen_to_bottom())
    }

    pub fn remote_viewport_snapshot(
        &self,
        session_id: &str,
        display_offset: usize,
        overscan_rows: usize,
        max_lines: usize,
    ) -> Result<TerminalScreenSnapshot> {
        Ok(self.session(session_id)?.remote_viewport_snapshot(
            display_offset,
            overscan_rows,
            max_lines,
        ))
    }

    pub fn release_viewport(
        &self,
        session_id: &str,
        owner: &str,
    ) -> Result<Option<TerminalViewportState>> {
        self.session(session_id)?.release_viewport(owner)
    }

    pub fn resize_viewport(
        &self,
        session_id: &str,
        owner: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Option<TerminalViewportState>> {
        self.session(session_id)?.resize_viewport(owner, cols, rows)
    }

    pub fn viewport_state(&self, session_id: &str) -> Result<TerminalViewportState> {
        Ok(self.session(session_id)?.viewport_state())
    }

    #[cfg(test)]
    pub(crate) fn expire_viewport_lease_for_test(
        &self,
        session_id: &str,
    ) -> Result<Option<TerminalViewportState>> {
        let session = self
            .sessions
            .lock()
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow!("terminal session not found: {session_id}"))?;
        {
            let mut viewport = session.viewport.lock();
            viewport.expires_at = Instant::now() - Duration::from_secs(1);
        }
        Ok(session.clone_handle().release_expired_viewport_lease())
    }

    fn ensure_viewport_lease_watcher(&self) {
        let sessions = Arc::downgrade(&self.sessions);
        self.viewport_lease_watcher_started.call_once(move || {
            std::thread::Builder::new()
                .name("codux-terminal-viewport-lease".to_string())
                .spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_secs(1));
                        let Some(sessions) = sessions.upgrade() else {
                            break;
                        };
                        for session in sessions.lock().values().cloned().collect::<Vec<_>>() {
                            session.clone_handle().release_expired_viewport_lease();
                        }
                    }
                })
                .expect("spawn terminal viewport lease watcher");
        });
    }

    pub fn subscribe_events(&self, session_id: &str, emit: EventSink) -> Result<()> {
        self.session(session_id)?.subscribe_events(emit);
        Ok(())
    }

    pub fn kill(&self, session_id: &str) -> Result<()> {
        let Some(session) = self.sessions.lock().remove(session_id) else {
            return Err(anyhow!("terminal session not found: {session_id}"));
        };
        self.remove_ai_runtime_terminal(&session);
        session.kill()
    }

    pub fn snapshot(&self, session_id: &str) -> Result<String> {
        Ok(self.session(session_id)?.snapshot())
    }

    pub fn snapshot_tail(&self, session_id: &str, max_chars: usize) -> Result<(String, usize)> {
        Ok(self.session(session_id)?.snapshot_tail(max_chars))
    }

    pub fn screen_snapshot(&self, session_id: &str) -> Result<TerminalScreenSnapshot> {
        Ok(self.session(session_id)?.screen_snapshot())
    }

    pub fn input_snapshot(&self, session_id: &str) -> Result<TerminalInputSnapshot> {
        Ok(self.session(session_id)?.input_snapshot())
    }

    pub fn output_snapshot(&self, session_id: &str) -> Result<TerminalOutputSnapshot> {
        Ok(self.session(session_id)?.output_snapshot())
    }

    pub fn buffer_characters(&self, session_id: &str) -> Result<usize> {
        Ok(self.session(session_id)?.buffer_characters())
    }

    pub fn clear_history(&self, session_id: &str) -> Result<()> {
        self.session(session_id)?.clear_history();
        Ok(())
    }

    fn session(&self, session_id: &str) -> Result<Arc<TerminalPtySession>> {
        self.sessions
            .lock()
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow!("terminal session not found: {session_id}"))
    }

    fn remove_incompatible_session(
        &self,
        session: &Arc<TerminalPtySession>,
        requested: &RequestedTerminalIdentity,
    ) {
        let existing = session.info();
        crate::ai_runtime::runtime_log_line(
            "terminal-pty",
            &format!(
                "replace incompatible session id={} existing_project={} existing_cwd={} requested_project={} requested_cwd={} requested_session_key={}",
                existing.id,
                existing.project_id,
                existing.cwd,
                requested.project_id.as_deref().unwrap_or(""),
                requested.cwd.as_deref().unwrap_or(""),
                requested.session_key.as_deref().unwrap_or("")
            ),
        );
        let removed = {
            let mut sessions = self.sessions.lock();
            if sessions
                .get(&existing.id)
                .map(|current| Arc::ptr_eq(current, session))
                .unwrap_or(false)
            {
                sessions.remove(&existing.id)
            } else {
                None
            }
        };
        if let Some(removed) = removed {
            self.remove_ai_runtime_terminal(&removed);
            let _ = removed.kill();
        }
    }

    fn register_ai_runtime_terminal(&self, session: &TerminalPtySession) {
        let Some(ai_runtime) = &self.ai_runtime else {
            return;
        };
        ai_runtime.registry().upsert(session.ai_runtime_binding());
        attach_ai_runtime_terminal_output_watcher(session, Arc::clone(ai_runtime));
    }

    fn remove_ai_runtime_terminal(&self, session: &TerminalPtySession) {
        let Some(ai_runtime) = &self.ai_runtime else {
            return;
        };
        ai_runtime.registry().remove(session.id());
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct DesktopTerminalSessionHandle(Arc<TerminalPtySession>);

impl CoreTerminalSessionHandle for DesktopTerminalSessionHandle {
    fn id(&self) -> &str {
        self.0.id()
    }

    fn info(&self) -> TerminalSessionSnapshot {
        self.0.info()
    }

    fn write(&self, data: &[u8]) -> std::result::Result<(), String> {
        self.0.write(data).map_err(|error| error.to_string())
    }

    fn resize(&self, cols: u16, rows: u16) -> std::result::Result<(), String> {
        self.0.resize(cols, rows).map_err(|error| error.to_string())
    }

    fn claim_viewport(&self, owner: &str) -> std::result::Result<TerminalViewportState, String> {
        self.0
            .claim_viewport(owner)
            .map_err(|error| error.to_string())
    }

    fn release_viewport(
        &self,
        owner: &str,
    ) -> std::result::Result<Option<TerminalViewportState>, String> {
        self.0
            .release_viewport(owner)
            .map_err(|error| error.to_string())
    }

    fn resize_viewport(
        &self,
        owner: &str,
        cols: u16,
        rows: u16,
    ) -> std::result::Result<Option<TerminalViewportState>, String> {
        self.0
            .resize_viewport(owner, cols, rows)
            .map_err(|error| error.to_string())
    }

    fn viewport_state(&self) -> TerminalViewportState {
        self.0.viewport_state()
    }

    fn snapshot(&self) -> String {
        self.0.snapshot()
    }

    fn snapshot_tail(&self, max_chars: usize) -> (String, usize) {
        self.0.snapshot_tail(max_chars)
    }

    fn buffer_characters(&self) -> usize {
        self.0.buffer_characters()
    }

    fn clear_history(&self) {
        self.0.clear_history();
    }

    fn kill(&self) -> std::result::Result<(), String> {
        self.0.kill().map_err(|error| error.to_string())
    }
}

impl CoreTerminalDriver for TerminalManager {
    type Session = DesktopTerminalSessionHandle;

    fn list(&self) -> Vec<TerminalSessionSnapshot> {
        TerminalManager::list(self)
    }

    fn create(
        &self,
        config: TerminalLaunchConfig,
        emit: TerminalEventSink,
    ) -> std::result::Result<Self::Session, String> {
        let config = TerminalPtyConfig::from(config);
        let (session, _) = self
            .attach_or_create_with_context(config, None, Arc::from(emit))
            .map_err(|error| error.to_string())?;
        Ok(DesktopTerminalSessionHandle(session))
    }

    fn session(&self, session_id: &str) -> std::result::Result<Self::Session, String> {
        self.session(session_id)
            .map(DesktopTerminalSessionHandle)
            .map_err(|error| error.to_string())
    }

    fn remove(&self, session_id: &str) -> std::result::Result<(), String> {
        self.kill(session_id).map_err(|error| error.to_string())
    }

    fn subscribe_events(
        &self,
        session_id: &str,
        emit: TerminalEventSink,
    ) -> std::result::Result<(), String> {
        self.subscribe_events(session_id, Arc::from(emit))
            .map_err(|error| error.to_string())
    }
}

pub struct TerminalPtySession {
    id: String,
    stdin_writer: Arc<parking_lot::Mutex<Box<dyn Write + Send>>>,
    input_capture: Arc<parking_lot::Mutex<TerminalInputCapture>>,
    output_capture: Arc<parking_lot::Mutex<TerminalOutputCapture>>,
    history: Arc<parking_lot::Mutex<RingHistory>>,
    screen: Arc<parking_lot::Mutex<HeadlessTerminalScreen>>,
    output_subscribers: Arc<parking_lot::Mutex<Vec<flume::Sender<Vec<u8>>>>>,
    event_subscribers: Arc<parking_lot::Mutex<Vec<EventSink>>>,
    info: Arc<parking_lot::Mutex<TerminalSessionSnapshot>>,
    ai_runtime_binding: AIRuntimeTerminalBinding,
    pty_control: LocalPtyProcessHandle,
    viewport: Arc<parking_lot::Mutex<TerminalViewportLease>>,
}

#[derive(Clone)]
pub struct TerminalPtySessionHandle {
    pty_control: LocalPtyProcessHandle,
    info: Arc<parking_lot::Mutex<TerminalSessionSnapshot>>,
    viewport: Arc<parking_lot::Mutex<TerminalViewportLease>>,
    event_subscribers: Arc<parking_lot::Mutex<Vec<EventSink>>>,
    screen: Arc<parking_lot::Mutex<HeadlessTerminalScreen>>,
}

impl TerminalPtySession {
    pub fn spawn(
        config: TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
        event_sink: Option<EventSink>,
    ) -> Result<(Self, Box<dyn Write + Send>, Box<dyn Read + Send>)> {
        let id = config
            .terminal_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let cols = config.cols.unwrap_or(100).max(20);
        let rows = config.rows.unwrap_or(32).max(8);
        let shell = config.shell.clone().unwrap_or_else(default_shell);
        let cwd = requested_terminal_cwd(&config, context);
        let initial_command = config
            .command
            .clone()
            .filter(|value| !value.trim().is_empty());

        let environment = terminal_environment(&shell, cwd.as_deref(), &id, &config, context);
        let process = spawn_local_pty(LocalPtySpawnConfig {
            shell: shell.clone(),
            cwd: cwd.clone(),
            initial_command: initial_command.clone(),
            cols,
            rows,
            env: environment,
            clear_env: true,
            command_mode: LocalPtyCommandMode::InteractiveLogin,
        })
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to spawn shell {shell}"))?;
        let stdin_writer = Arc::new(parking_lot::Mutex::new(process.writer));
        let input_capture = Arc::new(parking_lot::Mutex::new(TerminalInputCapture::new(
            INPUT_CAPTURE_LIMIT,
        )));
        let output_capture = Arc::new(parking_lot::Mutex::new(TerminalOutputCapture::new(
            OUTPUT_CAPTURE_LIMIT,
        )));
        let history = Arc::new(parking_lot::Mutex::new(RingHistory::new(
            terminal_history_bytes(config.scrollback_lines, cols),
        )));
        let screen = Arc::new(parking_lot::Mutex::new(HeadlessTerminalScreen::new(
            cols as usize,
            rows as usize,
            // Serves remote scrollback views (terminal.viewport.scroll);
            // deep enough for meaningful mobile history browsing.
            config.scrollback_lines.unwrap_or(5000),
        )));
        let output_subscribers = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let event_subscribers = Arc::new(parking_lot::Mutex::new(Vec::new()));
        if let Some(event_sink) = event_sink {
            event_subscribers.lock().push(event_sink);
        }
        let now = rfc3339_now();
        let project_path = context
            .map(|context| context.project_path.display().to_string())
            .unwrap_or_else(|| cwd.clone().unwrap_or_default());
        let project_name = config
            .project_name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.map(|context| context.project_name.clone()))
            .or_else(|| project_path_name(&project_path))
            .unwrap_or_else(|| "Codux".to_string());
        let project_id = config
            .project_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.map(|context| context.project_id.clone()))
            .unwrap_or_else(|| project_name.clone());
        let slot_id = config
            .slot_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.and_then(|context| context.slot_id.clone()))
            .unwrap_or_default();
        let session_key = config
            .session_key
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.and_then(|context| context.session_key.clone()));
        let session_instance_id = config
            .session_instance_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.and_then(|context| context.session_instance_id.clone()))
            .or_else(|| Some(Uuid::new_v4().to_string().to_lowercase()));
        let title = config
            .title
            .clone()
            .unwrap_or_else(|| "Terminal".to_string());
        let info_cwd = cwd.clone().unwrap_or(project_path);
        let info = Arc::new(parking_lot::Mutex::new(TerminalSessionSnapshot {
            id: id.clone(),
            title: title.clone(),
            slot_id: slot_id.clone(),
            session_key: session_key.clone(),
            project_id: project_id.clone(),
            project_name,
            cwd: info_cwd.clone(),
            shell: shell.clone(),
            command: initial_command.clone().unwrap_or_default(),
            cols,
            rows,
            status: "running".to_string(),
            is_running: true,
            created_at: now.clone(),
            last_active_at: now,
            buffer_characters: 0,
            has_buffer: false,
        }));
        let viewport = Arc::new(parking_lot::Mutex::new(TerminalViewportLease {
            state: TerminalViewportState {
                owner: terminal_viewport_local_owner().to_string(),
                cols,
                rows,
                generation: 0,
            },
            expires_at: Instant::now() + TERMINAL_VIEWPORT_LEASE_TTL,
        }));
        let ai_runtime_binding = AIRuntimeTerminalBinding {
            terminal_id: id.clone(),
            project_id,
            slot_id,
            title,
            cwd: info_cwd,
            tool: config.tool.clone(),
            is_active: false,
            session_key,
            terminal_instance_id: session_instance_id,
        };
        spawn_waiter(
            id.clone(),
            process.control.clone(),
            info.clone(),
            event_subscribers.clone(),
        );

        let terminal_writer = CaptureWriter::new(stdin_writer.clone(), input_capture.clone());
        let terminal_reader = CaptureReader::new(
            id.clone(),
            process.reader,
            output_capture.clone(),
            history.clone(),
            screen.clone(),
            output_subscribers.clone(),
            event_subscribers.clone(),
            info.clone(),
        );
        Ok((
            Self {
                id,
                stdin_writer,
                input_capture,
                output_capture,
                history,
                screen,
                output_subscribers,
                event_subscribers,
                info,
                ai_runtime_binding,
                pty_control: process.control,
                viewport,
            },
            Box::new(terminal_writer),
            Box::new(terminal_reader),
        ))
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn clone_handle(&self) -> TerminalPtySessionHandle {
        TerminalPtySessionHandle {
            pty_control: self.pty_control.clone(),
            info: self.info.clone(),
            viewport: self.viewport.clone(),
            event_subscribers: self.event_subscribers.clone(),
            screen: self.screen.clone(),
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.clone_handle().resize(cols, rows)
    }

    pub fn claim_viewport(&self, owner: &str) -> Result<TerminalViewportState> {
        self.clone_handle().claim_viewport(owner)
    }

    pub fn release_viewport(&self, owner: &str) -> Result<Option<TerminalViewportState>> {
        self.clone_handle().release_viewport(owner)
    }

    pub fn resize_viewport(
        &self,
        owner: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Option<TerminalViewportState>> {
        self.clone_handle().resize_viewport(owner, cols, rows)
    }

    pub fn viewport_state(&self) -> TerminalViewportState {
        self.clone_handle().viewport_state()
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.stdin_writer.lock();
        writer.write_all(data)?;
        writer.flush()?;
        self.input_capture.lock().push(data);
        Ok(())
    }

    pub fn subscribe_output(&self, replay_snapshot: bool) -> flume::Receiver<Vec<u8>> {
        let (tx, rx) = flume::unbounded();
        if replay_snapshot {
            let mut snapshot = self.snapshot();
            // An alt-screen TUI (e.g. Claude Code) keeps its UI in the alternate
            // buffer, which has no scrollback and so never reaches the raw
            // history above. A re-attaching viewer that only replays the raw
            // history therefore renders blank until the TUI happens to repaint
            // (which is why re-entering the terminal "fixed" it). Append the
            // live screen keyframe -- it carries the active DEC modes
            // (alt-screen, mouse) and its ESC[2J clears only the visible screen
            // (alacritty keeps scrollback) -- so the current screen and its
            // modes are reconstructed immediately. Normal-screen sessions
            // reconstruct fully from the raw history and skip this.
            let screen = self.screen_snapshot();
            if screen.input_mode.alternate_screen && !screen.data.is_empty() {
                snapshot.push_str(&screen.data);
            }
            if !snapshot.is_empty() {
                let _ = tx.send(snapshot.into_bytes());
            }
        }
        self.output_subscribers.lock().push(tx);
        rx
    }

    pub fn subscribe_events(&self, emit: EventSink) {
        self.event_subscribers.lock().push(emit);
    }

    pub fn kill(&self) -> Result<()> {
        self.pty_control.kill().map_err(anyhow::Error::msg)
    }

    pub fn snapshot(&self) -> String {
        self.history.lock().to_text()
    }

    pub fn snapshot_tail(&self, max_chars: usize) -> (String, usize) {
        self.history.lock().tail_text(max_chars)
    }

    pub fn screen_snapshot(&self) -> TerminalScreenSnapshot {
        // Queue the snapshot under the lock, but wait for the worker reply
        // outside of it: holding the screen mutex across the round-trip
        // convoys the PTY reader and every remote handler behind one
        // slow snapshot.
        let request = self.screen.lock().snapshot_request(true);
        request.snapshot()
    }

    /// Scroll the host-side screen viewport (serves remote scrollback
    /// views: the host screen owns the authoritative scrollback at the
    /// current grid size). Returns a snapshot of the scrolled viewport
    /// stacked with one screen of overscan context above it, so the client
    /// can pre-render content revealed by inertial scrolling.
    pub fn scroll_screen_lines(&self, lines: i32) -> TerminalScreenSnapshot {
        {
            let mut screen = self.screen.lock();
            screen.scroll_lines(lines);
        }
        self.scrolled_view_snapshot()
    }

    pub fn scroll_screen_to_bottom(&self) -> TerminalScreenSnapshot {
        {
            let mut screen = self.screen.lock();
            screen.scroll_to_bottom();
        }
        self.screen_snapshot()
    }

    pub fn remote_viewport_snapshot(
        &self,
        display_offset: usize,
        overscan_rows: usize,
        max_lines: usize,
    ) -> TerminalScreenSnapshot {
        let request = self.screen.lock().remote_viewport_snapshot_request(
            display_offset,
            overscan_rows,
            max_lines,
        );
        request.snapshot()
    }

    fn scrolled_view_snapshot(&self) -> TerminalScreenSnapshot {
        let viewport = self.screen_snapshot();
        let above_offset = viewport.display_offset + viewport.rows;
        // Queue both overscan requests before waiting on either, so the
        // worker round-trips overlap instead of running serially.
        let (above_request, below_request) = {
            let screen = self.screen.lock();
            let above = screen.snapshot_at_offset_request(above_offset);
            let below = (viewport.display_offset > 0).then(|| {
                screen.snapshot_at_offset_request(
                    viewport.display_offset.saturating_sub(viewport.rows),
                )
            });
            (above, below)
        };
        let above = above_request.snapshot();
        let below = below_request.map(|request| request.snapshot());
        codux_terminal_core::stack_scrolled_snapshots(&above, &viewport, below.as_ref())
    }

    pub fn buffer_characters(&self) -> usize {
        self.history.lock().len_chars()
    }

    pub fn clear_history(&self) {
        self.history.lock().clear();
        self.screen.lock().clear();
        let mut info = self.info.lock();
        info.buffer_characters = 0;
        info.has_buffer = false;
        info.last_active_at = rfc3339_now();
    }

    pub fn info(&self) -> TerminalSessionSnapshot {
        let mut info = self.info.lock().clone();
        info.buffer_characters = self.buffer_characters();
        info.has_buffer = info.buffer_characters > 0;
        info
    }

    pub fn matches_config(
        &self,
        config: &TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
    ) -> bool {
        self.matches_requested_identity(&RequestedTerminalIdentity::from_config(config, context))
    }

    fn matches_requested_identity(&self, requested: &RequestedTerminalIdentity) -> bool {
        let info = self.info();
        if let Some(cwd) = requested.cwd.as_deref()
            && normalize_terminal_path(&info.cwd) != cwd
        {
            return false;
        }
        if let Some(project_id) = requested.project_id.as_deref()
            && info.project_id != project_id
        {
            return false;
        }
        if let Some(session_key) = requested.session_key.as_deref()
            && info.session_key.as_deref() != Some(session_key)
        {
            return false;
        }
        true
    }

    pub fn ai_runtime_binding(&self) -> AIRuntimeTerminalBinding {
        self.ai_runtime_binding.clone()
    }

    pub fn input_snapshot(&self) -> TerminalInputSnapshot {
        self.input_capture.lock().snapshot()
    }

    pub fn output_snapshot(&self) -> TerminalOutputSnapshot {
        self.output_capture.lock().snapshot()
    }
}

impl TerminalPtySessionHandle {
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.resize_viewport(terminal_viewport_local_owner(), cols, rows)
            .map(|_| ())
    }

    /// Active claim: explicit viewport ownership intent from a controller.
    /// Ordinary desktop focus/input does not call this; the current owner is
    /// the only endpoint allowed to resize the PTY.
    pub fn claim_viewport(&self, owner: &str) -> Result<TerminalViewportState> {
        self.claim_viewport_with(owner, true)
    }

    /// Passive claim: ambient paths such as the desktop prepaint. Does NOT
    /// steal an unexpired lease held by a different owner — otherwise a
    /// painting desktop pane revokes a mobile claim within one frame.
    pub fn claim_viewport_passive(&self, owner: &str) -> Result<TerminalViewportState> {
        self.claim_viewport_with(owner, false)
    }

    fn claim_viewport_with(&self, owner: &str, force: bool) -> Result<TerminalViewportState> {
        let owner = terminal_viewport_owner(owner);
        let mut viewport = self.viewport.lock();
        let now = Instant::now();
        if !force && viewport.state.owner != owner {
            return Ok(viewport.state.clone());
        }
        let state = &mut viewport.state;
        let owner_changed = state.owner != owner;
        if state.owner != owner {
            state.owner = owner;
            state.generation = state.generation.saturating_add(1);
        }
        viewport.expires_at = now + TERMINAL_VIEWPORT_LEASE_TTL;
        let state = viewport.state.clone();
        drop(viewport);
        if owner_changed {
            // A scrolled host viewport belongs to the previous owner.
            self.screen.lock().scroll_to_bottom();
            self.emit_viewport_state(&state);
        }
        Ok(state)
    }

    /// Renew the lease when traffic from the current owner proves it is
    /// still actively viewing (input, output acks). No-op for non-owners.
    pub fn touch_viewport_lease(&self, owner: &str) {
        let owner = terminal_viewport_owner(owner);
        let mut viewport = self.viewport.lock();
        if viewport.state.owner == owner {
            viewport.expires_at = Instant::now() + TERMINAL_VIEWPORT_LEASE_TTL;
        }
    }

    pub fn release_expired_viewport_lease(&self) -> Option<TerminalViewportState> {
        let mut viewport = self.viewport.lock();
        if viewport.state.owner == terminal_viewport_local_owner()
            || viewport.expires_at > Instant::now()
        {
            return None;
        }
        viewport.state.owner = terminal_viewport_local_owner().to_string();
        viewport.state.generation = viewport.state.generation.saturating_add(1);
        viewport.expires_at = Instant::now() + TERMINAL_VIEWPORT_LEASE_TTL;
        let state = viewport.state.clone();
        drop(viewport);
        self.emit_viewport_state(&state);
        Some(state)
    }

    pub fn release_viewport(&self, owner: &str) -> Result<Option<TerminalViewportState>> {
        let owner = terminal_viewport_owner(owner);
        let mut viewport = self.viewport.lock();
        if viewport.state.owner != owner {
            return Ok(None);
        }
        viewport.state.owner = terminal_viewport_local_owner().to_string();
        viewport.state.generation = viewport.state.generation.saturating_add(1);
        viewport.expires_at = Instant::now() + TERMINAL_VIEWPORT_LEASE_TTL;
        let state = viewport.state.clone();
        drop(viewport);
        self.emit_viewport_state(&state);
        Ok(Some(state))
    }

    pub fn resize_viewport(
        &self,
        owner: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Option<TerminalViewportState>> {
        let owner = terminal_viewport_owner(owner);
        let cols = cols.max(20);
        let mut rows = rows.max(8);
        // A remote viewer drives the column count (so its narrow viewport does
        // not force horizontal scrolling) but the row count ALWAYS stays the
        // host's: a phone in portrait reports many more rows than the desktop
        // window has, and adopting them would make the desktop render a grid
        // taller than its own viewport, pushing the bottom (prompt) out of view.
        // The shorter viewer scrolls vertically instead.
        //
        // This used to make an exception for an alt-screen TUI: it only repaints
        // partially on a column-only change (it assumes the terminal reflows the
        // body, which the alt screen does not), so we grew its rows to force a
        // full repaint for the viewer. But growing on claim then shrinking back
        // on release is an asymmetric round trip the no-scrollback alt screen
        // cannot survive -- the dropped top rows never come back, leaving the
        // desktop blank above the prompt. That trick is no longer needed: the
        // viewport resize now pushes an authoritative screen keyframe once the
        // repaint settles (see send_terminal_viewport_keyframe and its
        // post-resize follow-up), so the viewer adopts the host's real screen
        // directly without ever disturbing the host's row count.
        let keep_host_rows = owner != terminal_viewport_local_owner();
        let mut viewport = self.viewport.lock();
        if viewport.state.owner != owner {
            return Ok(None);
        }
        if keep_host_rows && viewport.state.rows >= 8 {
            rows = viewport.state.rows;
        }
        viewport.expires_at = Instant::now() + TERMINAL_VIEWPORT_LEASE_TTL;
        if viewport.state.cols == cols && viewport.state.rows == rows {
            return Ok(Some(viewport.state.clone()));
        }
        self.pty_control
            .resize(cols, rows)
            .map_err(anyhow::Error::msg)?;
        {
            let mut info = self.info.lock();
            info.cols = cols;
            info.rows = rows;
            info.last_active_at = rfc3339_now();
        }
        self.screen.lock().resize(cols as usize, rows as usize);
        viewport.state.cols = cols;
        viewport.state.rows = rows;
        viewport.state.generation = viewport.state.generation.saturating_add(1);
        let state = viewport.state.clone();
        drop(viewport);
        self.emit_viewport_state(&state);
        Ok(Some(state))
    }

    pub fn viewport_state(&self) -> TerminalViewportState {
        self.viewport.lock().state.clone()
    }

    fn emit_viewport_state(&self, state: &TerminalViewportState) {
        let session_id = self.info.lock().id.clone();
        emit_terminal_event(
            &self.event_subscribers,
            TerminalEvent::Viewport {
                session_id,
                owner: state.owner.clone(),
                cols: state.cols,
                rows: state.rows,
                generation: state.generation,
            },
        );
    }
}

pub fn terminal_viewport_local_owner() -> &'static str {
    "desktop"
}

pub fn terminal_viewport_remote_owner(device_id: &str) -> String {
    format!("remote:{}", device_id.trim())
}

fn terminal_viewport_owner(owner: &str) -> String {
    let owner = owner.trim();
    if owner.is_empty() {
        terminal_viewport_local_owner().to_string()
    } else {
        owner.to_string()
    }
}

struct CaptureReader {
    session_id: String,
    inner: Box<dyn Read + Send>,
    output_capture: Arc<parking_lot::Mutex<TerminalOutputCapture>>,
    history: Arc<parking_lot::Mutex<RingHistory>>,
    screen: Arc<parking_lot::Mutex<HeadlessTerminalScreen>>,
    output_subscribers: Arc<parking_lot::Mutex<Vec<flume::Sender<Vec<u8>>>>>,
    event_subscribers: Arc<parking_lot::Mutex<Vec<EventSink>>>,
    info: Arc<parking_lot::Mutex<TerminalSessionSnapshot>>,
    pending_utf8: Vec<u8>,
}

impl CaptureReader {
    fn new(
        session_id: String,
        inner: Box<dyn Read + Send>,
        output_capture: Arc<parking_lot::Mutex<TerminalOutputCapture>>,
        history: Arc<parking_lot::Mutex<RingHistory>>,
        screen: Arc<parking_lot::Mutex<HeadlessTerminalScreen>>,
        output_subscribers: Arc<parking_lot::Mutex<Vec<flume::Sender<Vec<u8>>>>>,
        event_subscribers: Arc<parking_lot::Mutex<Vec<EventSink>>>,
        info: Arc<parking_lot::Mutex<TerminalSessionSnapshot>>,
    ) -> Self {
        Self {
            session_id,
            inner,
            output_capture,
            history,
            screen,
            output_subscribers,
            event_subscribers,
            info,
            pending_utf8: Vec::new(),
        }
    }
}

impl Read for CaptureReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        if read == 0 {
            self.flush_pending_utf8();
            return Ok(0);
        }
        if read > 0 {
            let bytes = &buf[..read];
            self.output_capture.lock().push(bytes);
            self.screen.lock().process(bytes);
            self.broadcast_output(bytes);
            let text = decode_utf8_output(bytes, &mut self.pending_utf8);
            if !text.is_empty() {
                let mut history = self.history.lock();
                history.push_text(&text);
                let chars = history.len_chars();
                let mut info = self.info.lock();
                info.last_active_at = rfc3339_now();
                info.buffer_characters = chars;
                info.has_buffer = chars > 0;
            }
            emit_terminal_event(
                &self.event_subscribers,
                TerminalEvent::Output {
                    session_id: self.session_id.clone(),
                    text,
                    bytes: bytes.to_vec(),
                },
            );
        }
        Ok(read)
    }
}

impl CaptureReader {
    fn flush_pending_utf8(&mut self) {
        let text = flush_utf8_decoder(&mut self.pending_utf8);
        if text.is_empty() {
            return;
        }
        let bytes = text.as_bytes().to_vec();
        {
            let mut history = self.history.lock();
            history.push_text(&text);
            let chars = history.len_chars();
            let mut info = self.info.lock();
            info.last_active_at = rfc3339_now();
            info.buffer_characters = chars;
            info.has_buffer = chars > 0;
        }
        self.broadcast_output(&bytes);
        emit_terminal_event(
            &self.event_subscribers,
            TerminalEvent::Output {
                session_id: self.session_id.clone(),
                text,
                bytes,
            },
        );
    }

    fn broadcast_output(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut subscribers = self.output_subscribers.lock();
        subscribers.retain(|subscriber| subscriber.send(bytes.to_vec()).is_ok());
    }
}

struct CaptureWriter {
    inner: Arc<parking_lot::Mutex<Box<dyn Write + Send>>>,
    capture: Arc<parking_lot::Mutex<TerminalInputCapture>>,
}

impl CaptureWriter {
    fn new(
        inner: Arc<parking_lot::Mutex<Box<dyn Write + Send>>>,
        capture: Arc<parking_lot::Mutex<TerminalInputCapture>>,
    ) -> Self {
        Self { inner, capture }
    }
}

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.lock().write(buf)?;
        if written > 0 {
            self.capture.lock().push(&buf[..written]);
        }
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.lock().flush()
    }
}

struct TerminalOutputCapture {
    total_bytes: usize,
    limit: usize,
    tail: VecDeque<u8>,
}

impl TerminalOutputCapture {
    fn new(limit: usize) -> Self {
        Self {
            total_bytes: 0,
            limit,
            tail: VecDeque::with_capacity(limit.min(4096)),
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        self.total_bytes = self.total_bytes.saturating_add(bytes.len());
        if self.limit == 0 {
            return;
        }
        for byte in bytes {
            self.tail.push_back(*byte);
            while self.tail.len() > self.limit {
                self.tail.pop_front();
            }
        }
    }

    fn snapshot(&self) -> TerminalOutputSnapshot {
        let bytes = self.tail.iter().copied().collect::<Vec<_>>();
        TerminalOutputSnapshot {
            bytes: self.total_bytes,
            tail: String::from_utf8_lossy(&bytes).to_string(),
        }
    }
}

struct TerminalInputCapture {
    total_bytes: usize,
    limit: usize,
    history: VecDeque<TerminalCapturedInput>,
}

impl TerminalInputCapture {
    fn new(limit: usize) -> Self {
        Self {
            total_bytes: 0,
            limit,
            history: VecDeque::with_capacity(limit.min(8)),
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        self.total_bytes = self.total_bytes.saturating_add(bytes.len());
        if self.limit == 0 {
            return;
        }
        let text = String::from_utf8_lossy(bytes).to_string();
        if text.trim().is_empty() {
            return;
        }
        self.history.push_back(TerminalCapturedInput {
            text,
            bytes: bytes.len(),
            timestamp: now_seconds(),
        });
        while self.history.len() > self.limit {
            self.history.pop_front();
        }
    }

    fn snapshot(&self) -> TerminalInputSnapshot {
        TerminalInputSnapshot {
            bytes: self.total_bytes,
            history: self.history.iter().cloned().collect(),
        }
    }
}

struct RingHistory {
    max_bytes: usize,
    len_bytes: usize,
    len_chars: usize,
    chunks: VecDeque<String>,
}

impl RingHistory {
    fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            len_bytes: 0,
            len_chars: 0,
            chunks: VecDeque::new(),
        }
    }

    fn clear(&mut self) {
        self.len_bytes = 0;
        self.len_chars = 0;
        self.chunks.clear();
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let chunk = text.to_string();
        self.len_bytes += chunk.len();
        self.len_chars += chunk.chars().count();
        self.chunks.push_back(chunk);

        while self.len_bytes > self.max_bytes {
            if let Some(chunk) = self.chunks.pop_front() {
                self.len_bytes = self.len_bytes.saturating_sub(chunk.len());
                self.len_chars = self.len_chars.saturating_sub(chunk.chars().count());
            } else {
                break;
            }
        }
    }

    fn to_text(&self) -> String {
        let mut text = String::with_capacity(self.len_bytes);
        for chunk in &self.chunks {
            text.push_str(chunk);
        }
        text
    }

    fn tail_text(&self, max_chars: usize) -> (String, usize) {
        if max_chars == 0 || self.len_chars <= max_chars {
            return (self.to_text(), 0);
        }
        let text = self.to_text();
        let start_chars = self.len_chars.saturating_sub(max_chars);
        let start_byte = byte_index_for_char_offset(&text, start_chars);
        let safe_start_byte = ansi_safe_snapshot_start(&text, start_byte);
        let safe_start_chars = text[..safe_start_byte].chars().count();
        (text[safe_start_byte..].to_string(), safe_start_chars)
    }

    fn len_chars(&self) -> usize {
        self.len_chars
    }
}

fn byte_index_for_char_offset(text: &str, char_offset: usize) -> usize {
    if char_offset == 0 {
        return 0;
    }
    text.char_indices()
        .nth(char_offset)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnsiSequenceState {
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
    String,
    StringEscape,
}

fn ansi_safe_snapshot_start(text: &str, start_byte: usize) -> usize {
    let bytes = text.as_bytes();
    let mut state = AnsiSequenceState::Ground;
    let mut index = 0;
    while index < start_byte {
        state = ansi_sequence_next_state(state, bytes[index]);
        index += 1;
    }
    if state == AnsiSequenceState::Ground {
        return start_byte;
    }
    while index < bytes.len() {
        state = ansi_sequence_next_state(state, bytes[index]);
        index += 1;
        if state == AnsiSequenceState::Ground {
            return index;
        }
    }
    bytes.len()
}

fn ansi_sequence_next_state(state: AnsiSequenceState, byte: u8) -> AnsiSequenceState {
    match state {
        AnsiSequenceState::Ground => match byte {
            0x1b => AnsiSequenceState::Escape,
            0x9b => AnsiSequenceState::Csi,
            0x9d => AnsiSequenceState::Osc,
            0x90 | 0x98 | 0x9e | 0x9f => AnsiSequenceState::String,
            _ => AnsiSequenceState::Ground,
        },
        AnsiSequenceState::Escape => match byte {
            b'[' => AnsiSequenceState::Csi,
            b']' => AnsiSequenceState::Osc,
            b'P' | b'X' | b'^' | b'_' => AnsiSequenceState::String,
            0x20..=0x2f => AnsiSequenceState::Escape,
            _ => AnsiSequenceState::Ground,
        },
        AnsiSequenceState::Csi => {
            if (0x40..=0x7e).contains(&byte) {
                AnsiSequenceState::Ground
            } else {
                AnsiSequenceState::Csi
            }
        }
        AnsiSequenceState::Osc => match byte {
            0x07 => AnsiSequenceState::Ground,
            0x1b => AnsiSequenceState::OscEscape,
            _ => AnsiSequenceState::Osc,
        },
        AnsiSequenceState::OscEscape => {
            if byte == b'\\' {
                AnsiSequenceState::Ground
            } else if byte == 0x1b {
                AnsiSequenceState::OscEscape
            } else {
                AnsiSequenceState::Osc
            }
        }
        AnsiSequenceState::String => match byte {
            0x07 => AnsiSequenceState::Ground,
            0x1b => AnsiSequenceState::StringEscape,
            _ => AnsiSequenceState::String,
        },
        AnsiSequenceState::StringEscape => {
            if byte == b'\\' {
                AnsiSequenceState::Ground
            } else if byte == 0x1b {
                AnsiSequenceState::StringEscape
            } else {
                AnsiSequenceState::String
            }
        }
    }
}

fn spawn_waiter(
    id: String,
    control: LocalPtyProcessHandle,
    info: Arc<parking_lot::Mutex<TerminalSessionSnapshot>>,
    event_subscribers: Arc<parking_lot::Mutex<Vec<EventSink>>>,
) {
    std::thread::Builder::new()
        .name(format!("codux-terminal-waiter-{id}"))
        .spawn(move || {
            let exit_code = control.wait_exit_code();
            emit_terminal_event(
                &event_subscribers,
                TerminalEvent::Exit {
                    session_id: id.clone(),
                    exit_code,
                },
            );
            let mut info = info.lock();
            info.status = "exited".to_string();
            info.is_running = false;
            info.last_active_at = rfc3339_now();
        })
        .expect("failed to spawn terminal waiter");
}

fn spawn_headless_reader(
    id: String,
    mut reader: Box<dyn Read + Send>,
    event_subscribers: Arc<parking_lot::Mutex<Vec<EventSink>>>,
) {
    std::thread::Builder::new()
        .name(format!("codux-terminal-reader-{id}"))
        .spawn(move || {
            let mut buffer = vec![0_u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => return,
                    Ok(_) => {}
                    Err(error) => {
                        emit_terminal_event(
                            &event_subscribers,
                            TerminalEvent::Error {
                                session_id: id,
                                message: error.to_string(),
                            },
                        );
                        return;
                    }
                }
            }
        })
        .expect("failed to spawn terminal reader");
}

fn emit_terminal_event(
    subscribers: &Arc<parking_lot::Mutex<Vec<EventSink>>>,
    event: TerminalEvent,
) {
    subscribers
        .lock()
        .retain(|subscriber| subscriber(event.clone()));
}

fn attach_ai_runtime_terminal_output_watcher(
    session: &TerminalPtySession,
    ai_runtime: Arc<AIRuntimeBridge>,
) {
    let binding = session.ai_runtime_binding();
    let watcher = Arc::new(parking_lot::Mutex::new(
        CodeWhaleTerminalProgressWatcher::new(binding, ai_runtime),
    ));
    session.subscribe_events(Arc::new(move |event| {
        watcher.lock().handle_terminal_event(&event);
        true
    }));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalProgressOsc {
    Started,
    Completed,
}

#[derive(Debug, Default)]
struct TerminalProgressOscParser {
    scan_tail: Vec<u8>,
}

impl TerminalProgressOscParser {
    fn push(&mut self, bytes: &[u8]) -> Vec<TerminalProgressOsc> {
        if bytes.is_empty() {
            return Vec::new();
        }
        if self.scan_tail.is_empty() && !bytes.contains(&0x1b) {
            return Vec::new();
        }
        let mut scan = Vec::with_capacity(self.scan_tail.len() + bytes.len());
        scan.extend_from_slice(&self.scan_tail);
        scan.extend_from_slice(bytes);

        let mut events = Vec::new();
        let mut index = 0;
        let mut consumed_until = 0;
        while index < scan.len() {
            let Some(relative) = scan[index..].iter().position(|byte| *byte == 0x1b) else {
                consumed_until = scan.len();
                break;
            };
            index += relative;
            let Some(rest) = scan.get(index..) else {
                break;
            };
            if b"\x1b]9;4;".starts_with(rest) {
                consumed_until = index;
                break;
            }
            let Some(body) = rest.strip_prefix(b"\x1b]9;4;") else {
                index += 1;
                consumed_until = index;
                continue;
            };
            let Some((value, terminator_len)) = terminal_progress_osc_value(body) else {
                consumed_until = index;
                break;
            };
            match value {
                b'1' => events.push(TerminalProgressOsc::Started),
                b'0' => events.push(TerminalProgressOsc::Completed),
                _ => {}
            }
            index += b"\x1b]9;4;".len() + 1 + terminator_len;
            consumed_until = index;
        }

        let tail = &scan[consumed_until.min(scan.len())..];
        let tail_len = tail.len().min(32);
        self.scan_tail.clear();
        self.scan_tail
            .extend_from_slice(&tail[tail.len().saturating_sub(tail_len)..]);
        events
    }
}

fn terminal_progress_osc_value(body: &[u8]) -> Option<(u8, usize)> {
    let value = *body.first()?;
    let rest = &body[1..];
    if rest.first().copied() == Some(0x07) {
        return Some((value, 1));
    }
    if rest.starts_with(b"\x1b\\") {
        return Some((value, 2));
    }
    None
}

struct CodeWhaleTerminalProgressWatcher {
    binding: AIRuntimeTerminalBinding,
    ai_runtime: Arc<AIRuntimeBridge>,
    parser: TerminalProgressOscParser,
}

impl CodeWhaleTerminalProgressWatcher {
    fn new(binding: AIRuntimeTerminalBinding, ai_runtime: Arc<AIRuntimeBridge>) -> Self {
        Self {
            binding,
            ai_runtime,
            parser: TerminalProgressOscParser::default(),
        }
    }

    fn handle_terminal_event(&mut self, event: &TerminalEvent) {
        let TerminalEvent::Output {
            session_id, bytes, ..
        } = event
        else {
            return;
        };
        if session_id != &self.binding.terminal_id {
            return;
        }
        for progress in self.parser.push(bytes) {
            match progress {
                TerminalProgressOsc::Started => {}
                TerminalProgressOsc::Completed => {
                    if self.current_session_is_running() {
                        self.submit_progress_hook("turnCompleted", true);
                    }
                }
            }
        }
    }

    fn current_session_is_running(&self) -> bool {
        self.ai_runtime
            .runtime_state_snapshot()
            .sessions
            .iter()
            .any(|session| {
                session.terminal_id == self.binding.terminal_id
                    && canonical_tool_name(&session.tool).as_deref() == Some("codewhale")
                    && matches!(session.state.as_str(), "responding" | "needsInput")
            })
    }

    fn submit_progress_hook(&self, kind: &str, has_completed_turn: bool) {
        let existing = self
            .ai_runtime
            .runtime_state_snapshot()
            .sessions
            .into_iter()
            .find(|session| session.terminal_id == self.binding.terminal_id);
        let payload = AIHookEventPayload {
            kind: kind.to_string(),
            terminal_id: self.binding.terminal_id.clone(),
            terminal_instance_id: self.binding.terminal_instance_id.clone(),
            project_id: self.binding.project_id.clone(),
            project_name: existing
                .as_ref()
                .map(|session| session.project_name.clone())
                .unwrap_or_else(|| "Workspace".to_string()),
            project_path: existing
                .as_ref()
                .and_then(|session| session.project_path.clone())
                .or_else(|| Some(self.binding.cwd.clone())),
            session_title: existing
                .as_ref()
                .map(|session| session.session_title.clone())
                .unwrap_or_else(|| self.binding.title.clone()),
            tool: "codewhale".to_string(),
            ai_session_id: existing
                .as_ref()
                .and_then(|session| session.ai_session_id.clone())
                .or_else(|| self.binding.session_key.clone()),
            model: existing.as_ref().and_then(|session| session.model.clone()),
            input_tokens: None,
            output_tokens: None,
            cached_input_tokens: None,
            total_tokens: None,
            updated_at: now_seconds(),
            metadata: Some(AIHookEventMetadata {
                transcript_path: None,
                notification_type: None,
                source: Some("terminal-progress-osc".to_string()),
                reason: Some(
                    if has_completed_turn {
                        "progress-completed"
                    } else {
                        "progress-started"
                    }
                    .to_string(),
                ),
                cwd: Some(self.binding.cwd.clone()),
                target_tool_name: None,
                message: None,
                was_interrupted: Some(false),
                has_completed_turn: Some(has_completed_turn),
            }),
        };
        if let Err(error) = self.ai_runtime.submit_hook_event(payload) {
            crate::ai_runtime::runtime_log_line(
                "terminal-ai-runtime",
                &format!(
                    "submit codewhale progress hook failed terminal={} kind={} error={}",
                    self.binding.terminal_id, kind, error
                ),
            );
        }
    }
}

pub fn terminal_environment(
    shell: &str,
    cwd: Option<&str>,
    session_id: &str,
    config: &TerminalPtyConfig,
    context: Option<&TerminalLaunchContext>,
) -> HashMap<String, String> {
    let home = crate::runtime_paths::home_dir();
    let home_text = home.display().to_string();
    let user = default_user();
    let session_cwd = cwd
        .map(str::to_string)
        .or_else(|| context.map(|context| context.project_path.display().to_string()))
        .unwrap_or_else(|| home_text.clone());
    let project_path = context
        .map(|context| context.project_path.display().to_string())
        .unwrap_or_else(|| session_cwd.clone());
    let project_name = config
        .project_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| context.map(|context| context.project_name.clone()))
        .or_else(|| project_path_name(&project_path))
        .unwrap_or_else(|| "Codux".to_string());
    let project_id = config
        .project_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| context.map(|context| context.project_id.clone()))
        .unwrap_or_else(|| project_name.clone());
    let terminal_id = config
        .terminal_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| context.and_then(|context| context.terminal_id.clone()))
        .unwrap_or_else(|| session_id.to_string());
    let slot_id = config
        .slot_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| context.and_then(|context| context.slot_id.clone()))
        .unwrap_or_default();
    let session_key = config
        .session_key
        .clone()
        .or_else(|| context.and_then(|context| context.session_key.clone()))
        .unwrap_or_default();
    let session_title = config
        .title
        .clone()
        .or_else(|| context.and_then(|context| context.session_title.clone()))
        .unwrap_or_else(|| "Terminal".to_string());
    let mut values = captured_shell_environment(shell, &session_cwd, &home_text, &user);
    values.insert("HOME".to_string(), home_text.clone());
    values.insert("USER".to_string(), user.clone());
    values.insert("LOGNAME".to_string(), user.clone());
    values.insert("SHELL".to_string(), shell.to_string());
    values.insert("PWD".to_string(), session_cwd.clone());
    append_passthrough_env(&mut values);

    for (key, value) in configured_dotenv(&home_text) {
        values.entry(key).or_insert(value);
    }
    for (key, value) in configured_codex_env(&home_text) {
        values.entry(key).or_insert(value);
    }

    let shell_path = values.get("PATH").cloned();
    let process_path = std::env::var("PATH").ok();
    let mut path = merged_executable_path(
        shell,
        &home_text,
        &user,
        shell_path.as_deref().or(process_path.as_deref()),
        shell_path.is_none(),
    );
    let runtime_root = config
        .runtime_root
        .as_ref()
        .or_else(|| context.map(|context| &context.runtime_root));
    if let Some(runtime_root) = runtime_root {
        let wrapper_bin = runtime_root
            .join("scripts/wrappers/bin")
            .display()
            .to_string();
        path = prepend_path_component(&wrapper_bin, &path);
        values.insert("DMUX_WRAPPER_BIN".to_string(), wrapper_bin);
        if matches!(shell_name(shell).as_deref(), Some("zsh"))
            && zsh_runtime_hook_ready(runtime_root)
        {
            let user_zdotdir = values
                .get("ZDOTDIR")
                .cloned()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| home_text.clone());
            values.insert("DMUX_USER_ZDOTDIR".to_string(), user_zdotdir);
            values.insert(
                "ZDOTDIR".to_string(),
                runtime_root
                    .join("scripts/shell-hooks/zsh")
                    .display()
                    .to_string(),
            );
            values.insert(
                "DMUX_ZSH_HOOK_SCRIPT".to_string(),
                runtime_root
                    .join("scripts/shell-hooks/dmux-ai-hook.zsh")
                    .display()
                    .to_string(),
            );
        }
    }
    if let Some(support_dir) = config
        .support_dir
        .as_ref()
        .or_else(|| context.map(|context| &context.support_dir))
    {
        values.insert(
            "DMUX_APP_SUPPORT_ROOT".to_string(),
            support_dir.display().to_string(),
        );
        values.insert(
            "CODUX_SSH_PROFILES_FILE".to_string(),
            support_dir.join("ssh_profiles.json").display().to_string(),
        );
    }
    if let Some(path) = config
        .tool_permissions_file
        .as_ref()
        .or_else(|| context.and_then(|context| context.tool_permissions_file.as_ref()))
    {
        values.insert(
            "DMUX_TOOL_PERMISSION_SETTINGS_FILE".to_string(),
            path.display().to_string(),
        );
    }
    if let Some(path) = config
        .memory_workspace_root
        .as_ref()
        .or_else(|| context.and_then(|context| context.memory_workspace_root.as_ref()))
    {
        values.insert(
            "DMUX_AI_MEMORY_WORKSPACE_ROOT".to_string(),
            path.display().to_string(),
        );
    }
    if let Some(path) = config
        .memory_prompt_file
        .as_ref()
        .or_else(|| context.and_then(|context| context.memory_prompt_file.as_ref()))
    {
        values.insert(
            "DMUX_AI_MEMORY_PROMPT_FILE".to_string(),
            path.display().to_string(),
        );
    }
    if let Some(path) = config
        .memory_index_file
        .as_ref()
        .or_else(|| context.and_then(|context| context.memory_index_file.as_ref()))
    {
        values.insert(
            "DMUX_AI_MEMORY_INDEX_FILE".to_string(),
            path.display().to_string(),
        );
    }
    values.insert("PATH".to_string(), path.clone());
    values.insert("DMUX_ORIGINAL_PATH".to_string(), path);
    values.insert("TERM".to_string(), "xterm-256color".to_string());
    values.insert("COLORTERM".to_string(), "truecolor".to_string());
    values.insert("CODEX_COLOR".to_string(), "1".to_string());
    values.insert("CODUX_GPUI".to_string(), "1".to_string());
    // Default Claude Code to its classic renderer (conversation stays in the
    // terminal's native scrollback) instead of the fullscreen alternate-screen
    // TUI. The alt screen has no scrollback and does not reflow, which is what
    // makes the desktop<->mobile viewport handoff fragile (blank top rows,
    // torn keyframes). Only sets a default -- a user who exports the var
    // themselves (e.g. "0" to keep the fullscreen TUI) is respected.
    values
        .entry("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN".to_string())
        .or_insert_with(|| "1".to_string());
    values.insert(
        "LANG".to_string(),
        values.get("LANG").cloned().unwrap_or_else(default_lang),
    );
    let lang = values.get("LANG").cloned().unwrap_or_else(default_lang);
    values.entry("LC_CTYPE".to_string()).or_insert(lang);
    values.insert("DMUX_PROJECT_ID".to_string(), project_id.clone());
    values.insert("DMUX_PROJECT_NAME".to_string(), project_name.clone());
    values.insert("DMUX_PROJECT_PATH".to_string(), project_path.clone());
    values.insert("CODUX_PROJECT_ID".to_string(), project_id);
    values.insert("CODUX_PROJECT_NAME".to_string(), project_name);
    values.insert("CODUX_PROJECT_PATH".to_string(), project_path);
    values.insert("CODUX_TERMINAL_ID".to_string(), terminal_id.clone());
    values.insert("CODUX_SLOT_ID".to_string(), slot_id.clone());
    values.insert("DMUX_SESSION_ID".to_string(), terminal_id.clone());
    values.insert("DMUX_TERMINAL_ID".to_string(), terminal_id);
    values.insert("DMUX_SLOT_ID".to_string(), slot_id);
    values.insert("DMUX_SESSION_KEY".to_string(), session_key);
    values.insert("DMUX_SESSION_TITLE".to_string(), session_title);
    values.insert("DMUX_SESSION_CWD".to_string(), session_cwd);
    values.insert(
        "DMUX_SESSION_INSTANCE_ID".to_string(),
        config
            .session_instance_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.and_then(|context| context.session_instance_id.clone()))
            .unwrap_or_else(|| Uuid::new_v4().to_string().to_lowercase()),
    );
    values.insert(
        "DMUX_RUNTIME_OWNER".to_string(),
        crate::runtime_paths::app_slug().to_string(),
    );
    values.insert(
        "DMUX_RUNTIME_EVENT_DIR".to_string(),
        crate::runtime_paths::runtime_event_dir()
            .display()
            .to_string(),
    );
    values.insert(
        "DMUX_LOG_FILE".to_string(),
        crate::runtime_paths::live_log_path().display().to_string(),
    );
    values.insert(
        "DMUX_CLAUDE_SESSION_MAP_DIR".to_string(),
        crate::runtime_paths::claude_session_map_dir()
            .display()
            .to_string(),
    );
    values.insert(
        "DMUX_OPENCODE_SESSION_MAP_DIR".to_string(),
        crate::runtime_paths::opencode_session_map_dir()
            .display()
            .to_string(),
    );

    if let Some(overrides) = &config.env {
        for (key, value) in overrides {
            values.insert(key.clone(), value.clone());
        }
    }
    ensure_utf8_locale(&mut values);
    values
}

fn ensure_utf8_locale(values: &mut HashMap<String, String>) {
    let fallback = default_lang();
    if !is_utf8_locale(values.get("LANG").map(String::as_str)) {
        values.insert("LANG".to_string(), fallback.clone());
    }

    for key in ["LC_ALL", "LC_CTYPE"] {
        if values
            .get(key)
            .is_some_and(|value| !is_utf8_locale(Some(value.as_str())))
        {
            values.insert(key.to_string(), fallback.clone());
        }
    }

    values.entry("LC_CTYPE".to_string()).or_insert(fallback);
}

fn is_utf8_locale(value: Option<&str>) -> bool {
    value
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase().replace('-', "");
            normalized.contains("utf8")
        })
        .unwrap_or(false)
}

fn append_passthrough_env(values: &mut HashMap<String, String>) {
    append_env_keys(values, COMMON_PASSTHROUGH_ENV_KEYS);
    #[cfg(unix)]
    append_env_keys(values, UNIX_PASSTHROUGH_ENV_KEYS);
    #[cfg(windows)]
    append_env_keys(values, WINDOWS_PASSTHROUGH_ENV_KEYS);
}

fn append_env_keys(values: &mut HashMap<String, String>, keys: &[&str]) {
    for key in keys {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                values.entry((*key).to_string()).or_insert(value);
            }
        }
    }
}

fn captured_shell_environment(
    shell: &str,
    cwd: &str,
    home: &str,
    user: &str,
) -> HashMap<String, String> {
    #[cfg(windows)]
    {
        let _ = shell;
        let _ = cwd;
        let _ = home;
        let _ = user;
        HashMap::new()
    }

    #[cfg(not(windows))]
    {
        static CACHE: OnceLock<parking_lot::Mutex<HashMap<String, HashMap<String, String>>>> =
            OnceLock::new();
        let cache = CACHE.get_or_init(|| parking_lot::Mutex::new(HashMap::new()));
        let key = format!("{shell}|{cwd}|{home}|{user}");
        if let Some(value) = cache.lock().get(&key) {
            return value.clone();
        }
        let value = capture_shell_environment_uncached(shell, cwd, home, user).unwrap_or_default();
        cache.lock().insert(key, value.clone());
        value
    }
}

#[cfg(not(windows))]
fn capture_shell_environment_uncached(
    shell: &str,
    cwd: &str,
    home: &str,
    user: &str,
) -> Option<HashMap<String, String>> {
    let begin_marker = "__CODUX_SHELL_ENV_BEGIN__";
    let end_marker = "__CODUX_SHELL_ENV_END__";
    let command = format!(
        "cd {}; printf '%s\\000' '{}'; command env -0; printf '%s\\000' '{}'",
        shell_quote(cwd),
        begin_marker,
        end_marker
    );
    let mut capture = Command::new(shell);
    capture
        .args(["-l", "-i", "-c", &command])
        .env_clear()
        .env("HOME", home)
        .env("USER", user)
        .env("LOGNAME", user)
        .env("SHELL", shell)
        .env("TERM", "xterm-256color")
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| FALLBACK_PATH.to_string()),
        )
        .stdin(Stdio::null());
    for key in COMMON_PASSTHROUGH_ENV_KEYS
        .iter()
        .chain(UNIX_PASSTHROUGH_ENV_KEYS.iter())
    {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                capture.env(key, value);
            }
        }
    }
    let output = capture.output().ok()?;
    parse_captured_shell_environment(&output.stdout, begin_marker, end_marker)
}

#[cfg(not(windows))]
fn parse_captured_shell_environment(
    output: &[u8],
    begin_marker: &str,
    end_marker: &str,
) -> Option<HashMap<String, String>> {
    let begin = find_bytes(output, begin_marker.as_bytes())? + begin_marker.len();
    let rest = &output[begin..];
    let end = find_bytes(rest, end_marker.as_bytes())?;
    let mut body = &rest[..end];
    while matches!(body.first(), Some(0 | b'\n' | b'\r')) {
        body = &body[1..];
    }

    let mut values = HashMap::new();
    for entry in body.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let Some(eq) = entry.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        if eq == 0 {
            continue;
        }
        let key = String::from_utf8_lossy(&entry[..eq]).to_string();
        let value = String::from_utf8_lossy(&entry[eq + 1..]).to_string();
        values.insert(key, value);
    }
    Some(values)
}

#[cfg(not(windows))]
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn configured_dotenv(home: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let allowed = DOTENV_KEYS
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    for path in dotenv_paths(home) {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        for raw_line in text.lines() {
            let mut line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(value) = line.strip_prefix("export ") {
                line = value.trim();
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if !allowed.contains(key) || values.contains_key(key) {
                continue;
            }
            values.insert(key.to_string(), unquote_env_value(value.trim()));
        }
    }
    values
}

fn configured_codex_env(home: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let codex_home = Path::new(home).join(".codex");
    if let Ok(text) = fs::read_to_string(codex_home.join("auth.json")) {
        for (key, value) in codex_auth_env_from_text(&text) {
            values.entry(key).or_insert(value);
        }
    }
    if let Ok(text) = fs::read_to_string(codex_home.join("config.toml")) {
        for (key, value) in codex_config_env_from_text(&text) {
            values.entry(key).or_insert(value);
        }
    }
    values
}

fn codex_auth_env_from_text(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return values;
    };
    if let Some(api_key) = json
        .get("OPENAI_API_KEY")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        values.insert("OPENAI_API_KEY".to_string(), api_key.to_string());
    }
    values
}

fn codex_config_env_from_text(text: &str) -> HashMap<String, String> {
    let mut root = HashMap::new();
    let mut providers: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut profiles: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut section = Vec::<String>::new();
    for raw_line in text.lines() {
        let line = strip_toml_comment(raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if let Some(next_section) = parse_toml_section(&line) {
            section = next_section;
            continue;
        }
        let Some((key, value)) = parse_toml_string_assignment(&line) else {
            continue;
        };
        match section.as_slice() {
            [] => {
                root.insert(key, value);
            }
            [table, provider] if table == "model_providers" => {
                providers
                    .entry(provider.clone())
                    .or_default()
                    .insert(key, value);
            }
            [table, profile] if table == "profiles" => {
                profiles
                    .entry(profile.clone())
                    .or_default()
                    .insert(key, value);
            }
            _ => {}
        }
    }
    let active_profile = root.get("profile").and_then(|name| profiles.get(name));
    let active_provider = active_profile
        .and_then(|profile| profile.get("model_provider"))
        .or_else(|| root.get("model_provider"))
        .map(String::as_str)
        .unwrap_or("openai");
    let base_url = providers
        .get(active_provider)
        .and_then(|provider| provider.get("base_url"))
        .or_else(|| active_profile.and_then(|profile| profile.get("openai_base_url")))
        .or_else(|| root.get("openai_base_url"))
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut values = HashMap::new();
    if let Some(base_url) = base_url {
        values.insert("OPENAI_BASE_URL".to_string(), base_url.to_string());
    }
    values
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_double_quote => escaped = true,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '#' if !in_single_quote && !in_double_quote => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_toml_section(line: &str) -> Option<Vec<String>> {
    let name = line.strip_prefix('[')?.strip_suffix(']')?.trim();
    if name.is_empty() {
        return None;
    }
    Some(
        name.split('.')
            .map(|part| unquote_env_value(part.trim()))
            .filter(|part| !part.is_empty())
            .collect(),
    )
}

fn parse_toml_string_assignment(line: &str) -> Option<(String, String)> {
    let (key, raw_value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), parse_toml_string(raw_value.trim())?))
}

fn parse_toml_string(value: &str) -> Option<String> {
    if value.len() < 2 {
        return None;
    }
    let bytes = value.as_bytes();
    match (bytes[0], bytes[value.len() - 1]) {
        (b'"', b'"') => serde_json::from_str::<String>(value).ok(),
        (b'\'', b'\'') => Some(value[1..value.len() - 1].to_string()),
        _ => None,
    }
}

fn dotenv_paths(home: &str) -> Vec<PathBuf> {
    [
        ".gemini/.env",
        ".claude/.env",
        ".codex/.env",
        ".opencode/.env",
        ".config/opencode/.env",
        ".codewhale/.env",
    ]
    .iter()
    .map(|path| Path::new(home).join(path))
    .collect()
}

fn unquote_env_value(value: &str) -> String {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn terminal_history_bytes(scrollback_lines: Option<usize>, cols: u16) -> usize {
    let lines = scrollback_lines.unwrap_or(500).clamp(200, 10_000);
    usize::from(cols.max(20))
        .saturating_mul(lines)
        .saturating_mul(4)
        .clamp(MIN_HISTORY_BYTES, MAX_CONFIGURED_HISTORY_BYTES)
}

fn decode_utf8_output(bytes: &[u8], pending: &mut Vec<u8>) -> String {
    if pending.is_empty() {
        return decode_utf8_complete_prefix(bytes, pending);
    }
    pending.extend_from_slice(bytes);
    let combined = std::mem::take(pending);
    decode_utf8_complete_prefix(&combined, pending)
}

fn decode_utf8_complete_prefix(bytes: &[u8], pending: &mut Vec<u8>) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(error) => {
            let valid_up_to = error.valid_up_to();
            let (valid, rest) = bytes.split_at(valid_up_to);
            if error.error_len().is_none() {
                pending.extend_from_slice(rest);
                return String::from_utf8_lossy(valid).to_string();
            }
            String::from_utf8_lossy(bytes).to_string()
        }
    }
}

fn flush_utf8_decoder(pending: &mut Vec<u8>) -> String {
    if pending.is_empty() {
        String::new()
    } else {
        String::from_utf8_lossy(&std::mem::take(pending)).to_string()
    }
}

fn normalize_terminal_cwd(cwd: Option<String>) -> Option<String> {
    cwd.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(normalize_terminal_path(trimmed))
        }
    })
}

fn requested_terminal_cwd(
    config: &TerminalPtyConfig,
    context: Option<&TerminalLaunchContext>,
) -> Option<String> {
    normalize_terminal_cwd(
        config
            .cwd
            .clone()
            .or_else(|| {
                context.and_then(|context| {
                    context
                        .session_cwd
                        .as_ref()
                        .map(|cwd| cwd.display().to_string())
                })
            })
            .or_else(|| context.map(|context| context.project_path.display().to_string())),
    )
}

#[cfg(windows)]
fn normalize_terminal_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    path.to_string()
}

#[cfg(not(windows))]
fn normalize_terminal_path(path: &str) -> String {
    path.to_string()
}

pub fn default_shell() -> String {
    if cfg!(target_os = "windows") {
        windows_default_shell()
    } else {
        std::env::var("SHELL")
            .ok()
            .filter(|shell| valid_shell_path(shell))
            .or_else(default_unix_login_shell)
            .unwrap_or_else(|| "/bin/zsh".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn default_unix_login_shell() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(shell) = macos_login_shell(default_user().as_str()) {
            return Some(shell);
        }
    }
    passwd_login_shell(default_user().as_str())
}

#[cfg(target_os = "windows")]
fn default_unix_login_shell() -> Option<String> {
    None
}

fn valid_shell_path(shell: &str) -> bool {
    let trimmed = shell.trim();
    !trimmed.is_empty() && !matches!(trimmed, "/bin/sh" | "sh") && Path::new(trimmed).is_file()
}

#[cfg(target_os = "macos")]
fn macos_login_shell(user: &str) -> Option<String> {
    let output = Command::new("/usr/bin/dscl")
        .args([".", "-read", &format!("/Users/{user}"), "UserShell"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace()
        .last()
        .map(str::trim)
        .filter(|shell| valid_shell_path(shell))
        .map(ToOwned::to_owned)
}

#[cfg(not(target_os = "macos"))]
fn macos_login_shell(_user: &str) -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
fn passwd_login_shell(user: &str) -> Option<String> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        if name != user {
            return None;
        }
        let shell = fields.nth(5)?;
        valid_shell_path(shell).then(|| shell.to_string())
    })
}

#[cfg(target_os = "windows")]
fn passwd_login_shell(_user: &str) -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn windows_default_shell() -> String {
    windows_shell_candidates()
        .into_iter()
        .find(|path| Path::new(path).exists())
        .unwrap_or_else(|| "powershell.exe".to_string())
}

#[cfg(not(target_os = "windows"))]
fn windows_default_shell() -> String {
    String::new()
}

#[cfg(target_os = "windows")]
fn windows_shell_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        candidates.push(
            Path::new(&program_files)
                .join("PowerShell")
                .join("7")
                .join("pwsh.exe")
                .display()
                .to_string(),
        );
    }
    if let Ok(system_root) = std::env::var("SystemRoot").or_else(|_| std::env::var("WINDIR")) {
        candidates.push(
            Path::new(&system_root)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
                .display()
                .to_string(),
        );
    }
    candidates.push("powershell.exe".to_string());
    candidates
}

fn merged_executable_path(
    shell: &str,
    home: &str,
    user: &str,
    inherited_path: Option<&str>,
    include_login_shell_path: bool,
) -> String {
    let default_path = inherited_path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(FALLBACK_PATH);
    let login_shell_path = include_login_shell_path
        .then(|| resolved_login_shell_path(shell, home, user))
        .flatten();
    let user_tool_paths = [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
        Path::new(home).join(".local/bin").display().to_string(),
        Path::new(home).join(".bun/bin").display().to_string(),
        Path::new(home).join(".cargo/bin").display().to_string(),
        Path::new(home).join(".opencode/bin").display().to_string(),
    ];
    let mut paths = Vec::new();
    if let Some(login_shell_path) = login_shell_path {
        push_path_components(&mut paths, &login_shell_path);
    }
    #[cfg(not(windows))]
    {
        for path in user_tool_paths {
            push_path(&mut paths, path);
        }
    }
    #[cfg(windows)]
    {
        let _ = user_tool_paths;
    }
    push_path_components(&mut paths, default_path);
    paths.join(&PATH_SEPARATOR.to_string())
}

fn push_path_components(paths: &mut Vec<String>, value: &str) {
    for path in value.split(PATH_SEPARATOR) {
        push_path(paths, path);
    }
}

fn push_path(paths: &mut Vec<String>, value: impl AsRef<str>) {
    let value = value.as_ref().trim();
    if value.is_empty() || paths.iter().any(|existing| existing == value) {
        return;
    }
    paths.push(value.to_string());
}

fn prepend_path_component(component: &str, path: &str) -> String {
    if component.trim().is_empty()
        || path
            .split(PATH_SEPARATOR)
            .any(|existing| existing.trim() == component.trim())
    {
        return path.to_string();
    }
    if path.trim().is_empty() {
        component.to_string()
    } else {
        format!("{component}{PATH_SEPARATOR}{path}")
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn resolved_login_shell_path(shell: &str, home: &str, user: &str) -> Option<String> {
    #[cfg(windows)]
    {
        let _ = shell;
        let _ = home;
        let _ = user;
        None
    }
    #[cfg(not(windows))]
    {
        static CACHE: OnceLock<parking_lot::Mutex<HashMap<String, Option<String>>>> =
            OnceLock::new();
        let cache = CACHE.get_or_init(|| parking_lot::Mutex::new(HashMap::new()));
        let key = format!("{shell}|{home}|{user}");
        if let Some(value) = cache.lock().get(&key) {
            return value.clone();
        }
        let resolved = resolve_login_shell_path_uncached(shell, home, user);
        cache.lock().insert(key, resolved.clone());
        resolved
    }
}

#[cfg(not(windows))]
fn resolve_login_shell_path_uncached(shell: &str, home: &str, user: &str) -> Option<String> {
    let begin_marker = "__CODUX_LOGIN_PATH_BEGIN__";
    let end_marker = "__CODUX_LOGIN_PATH_END__";
    let output = Command::new(shell)
        .args([
            "-lic",
            &format!("printf '{begin_marker}%s{end_marker}' \"$PATH\""),
        ])
        .env_clear()
        .env("HOME", home)
        .env("USER", user)
        .env("LOGNAME", user)
        .env("SHELL", shell)
        .env("PATH", FALLBACK_PATH)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let begin = text.find(begin_marker)? + begin_marker.len();
    let end = text[begin..].find(end_marker)? + begin;
    let path = text[begin..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

fn shell_name(shell: &str) -> Option<String> {
    Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.trim_start_matches('-').to_ascii_lowercase())
}

fn zsh_runtime_hook_ready(runtime_root: &Path) -> bool {
    let hook_dir = runtime_root.join("scripts/shell-hooks/zsh");
    hook_dir.join(".zshenv").is_file()
        && hook_dir.join(".zprofile").is_file()
        && hook_dir.join(".zshrc").is_file()
        && runtime_root
            .join("scripts/shell-hooks/dmux-ai-hook.zsh")
            .is_file()
}

fn project_path_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
}

fn default_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "codux".to_string())
}

fn default_lang() -> String {
    "en_US.UTF-8".to_string()
}

fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default()
}

fn rfc3339_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::time::{Duration, Instant};

    #[test]
    fn output_capture_keeps_limited_tail_and_total_bytes() {
        let mut capture = TerminalOutputCapture::new(5);
        capture.push(b"hello");
        capture.push(b" world");
        let snapshot = capture.snapshot();
        assert_eq!(snapshot.bytes, 11);
        assert_eq!(snapshot.tail, "world");
    }

    #[test]
    fn output_replay_uses_terminal_history_not_limited_tail() {
        let mut history = RingHistory::new(1024);
        history.push_text("hello");
        history.push_text(" world");
        assert_eq!(history.to_text(), "hello world");

        let mut capture = TerminalOutputCapture::new(5);
        capture.push(b"hello world");
        assert_eq!(capture.snapshot().tail, "world");
    }

    #[test]
    fn terminal_history_tail_returns_recent_window_and_offset() {
        let mut history = RingHistory::new(1024);
        history.push_text("hello");
        history.push_text(" world");

        assert_eq!(history.tail_text(5), ("world".to_string(), 6));
        assert_eq!(history.tail_text(20), ("hello world".to_string(), 0));
    }

    #[test]
    fn terminal_history_tail_starts_after_partial_csi_sequence() {
        let mut history = RingHistory::new(1024);
        history.push_text("line 1\n");
        history.push_text("\x1b[12;27Hprompt");

        let (tail, offset) = history.tail_text(9);

        assert_eq!(tail, "prompt");
        assert_eq!(offset, "line 1\n\x1b[12;27H".chars().count());
    }

    #[test]
    fn terminal_history_tail_starts_after_partial_osc_sequence() {
        let mut history = RingHistory::new(1024);
        history.push_text("line 1\n");
        history.push_text("\x1b]0;Codux\x07prompt");

        let (tail, offset) = history.tail_text(10);

        assert_eq!(tail, "prompt");
        assert_eq!(offset, "line 1\n\x1b]0;Codux\x07".chars().count());
    }

    #[test]
    fn headless_screen_snapshot_replays_current_screen_not_raw_tail() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"old line\n\x1b[2J\x1b[Htop\x1b[3;5Hbottom");

        let snapshot = screen.snapshot();

        assert!(snapshot.data.contains("\x1b[H\x1b[2J"));
        assert!(snapshot.data.contains("top"));
        assert!(snapshot.data.contains("bottom"));
        assert!(!snapshot.data.contains("old line"));
        assert_eq!(snapshot.cols, 20);
        assert_eq!(snapshot.rows, 4);
    }

    #[test]
    fn headless_screen_snapshot_tracks_resize() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.resize(30, 10);
        screen.process(b"ready");

        let snapshot = screen.snapshot();

        assert!(snapshot.data.contains("ready"));
        assert_eq!(snapshot.cols, 30);
        assert_eq!(snapshot.rows, 10);
    }

    #[test]
    fn headless_screen_snapshot_does_not_insert_spaces_after_wide_chars() {
        let mut screen = HeadlessTerminalScreen::new(40, 4, 100);
        screen.process("第 2003行 测 试 文 本".as_bytes());

        let snapshot = screen.snapshot();

        assert!(
            snapshot.data.contains("第 2003行 测 试 文 本"),
            "{}",
            snapshot.data.escape_debug()
        );
        assert!(!snapshot.data.contains("第  2003"));
        assert!(!snapshot.data.contains("测  试"));
    }

    #[test]
    fn input_capture_keeps_limited_history_and_total_bytes() {
        let mut capture = TerminalInputCapture::new(2);
        capture.push(b"ls\n");
        capture.push(b" ");
        capture.push(b"pwd\n");
        capture.push(b"echo ok\n");
        let snapshot = capture.snapshot();
        assert_eq!(snapshot.bytes, 16);
        assert_eq!(snapshot.history.len(), 2);
        assert_eq!(snapshot.history[0].text, "pwd\n");
        assert_eq!(snapshot.history[1].text, "echo ok\n");
    }

    #[test]
    fn utf8_decoder_keeps_split_multibyte_characters() {
        let mut pending = Vec::new();
        assert_eq!(decode_utf8_output(&[0xe6, 0x8e], &mut pending), "");
        assert_eq!(decode_utf8_output(&[0xa8], &mut pending), "推");
        assert!(pending.is_empty());
    }

    #[test]
    fn utf8_decoder_flushes_incomplete_tail_on_eof() {
        let mut pending = Vec::new();
        assert_eq!(decode_utf8_output(&[0xe6, 0x8e], &mut pending), "");
        assert_eq!(flush_utf8_decoder(&mut pending), "�");
        assert!(pending.is_empty());
    }

    #[test]
    fn terminal_progress_osc_parser_detects_split_start_and_completion() {
        let mut parser = TerminalProgressOscParser::default();

        assert!(parser.push(b"noise\x1b]9;").is_empty());
        assert_eq!(parser.push(b"4;1\x07"), vec![TerminalProgressOsc::Started]);
        assert_eq!(
            parser.push(b"\x1b]9;4;0\x1b\\"),
            vec![TerminalProgressOsc::Completed]
        );
    }

    #[test]
    fn terminal_progress_osc_parser_ignores_incomplete_sequence() {
        let mut parser = TerminalProgressOscParser::default();

        assert!(parser.push(b"\x1b]9;4;0").is_empty());
        assert_eq!(parser.push(b"\x07"), vec![TerminalProgressOsc::Completed]);
    }

    #[test]
    fn terminal_history_bytes_respects_configured_scrollback() {
        assert_eq!(terminal_history_bytes(Some(10_000), 100), 4 * 100 * 10_000);
        assert_eq!(terminal_history_bytes(Some(1), 100), MIN_HISTORY_BYTES);
    }

    #[test]
    fn terminal_environment_forces_utf8_locale() {
        let mut config = TerminalPtyConfig::default();
        config.env = Some(HashMap::from([
            ("LANG".to_string(), "C".to_string()),
            ("LC_ALL".to_string(), "C".to_string()),
            ("LC_CTYPE".to_string(), "POSIX".to_string()),
        ]));

        let env = terminal_environment("/bin/zsh", None, "term-1", &config, None);

        assert_eq!(env.get("LANG").map(String::as_str), Some("en_US.UTF-8"));
        assert_eq!(env.get("LC_ALL").map(String::as_str), Some("en_US.UTF-8"));
        assert_eq!(env.get("LC_CTYPE").map(String::as_str), Some("en_US.UTF-8"));
    }

    #[test]
    fn terminal_environment_does_not_set_term_program() {
        let config = TerminalPtyConfig::default();

        let env = terminal_environment("/bin/zsh", None, "term-1", &config, None);

        assert!(!env.contains_key("TERM_PROGRAM"));
        assert!(!env.contains_key("TERM_PROGRAM_VERSION"));
    }

    #[test]
    fn terminal_environment_preserves_real_term_program() {
        let mut config = TerminalPtyConfig::default();
        config.env = Some(HashMap::from([
            ("TERM_PROGRAM".to_string(), "Ghostty".to_string()),
            ("TERM_PROGRAM_VERSION".to_string(), "1.2.3".to_string()),
        ]));

        let env = terminal_environment("/bin/zsh", None, "term-1", &config, None);

        assert_eq!(env.get("TERM_PROGRAM").map(String::as_str), Some("Ghostty"));
        assert_eq!(
            env.get("TERM_PROGRAM_VERSION").map(String::as_str),
            Some("1.2.3")
        );
    }

    #[test]
    fn terminal_environment_injects_codux_runtime_context() {
        let temp =
            std::env::temp_dir().join(format!("codux-terminal-runtime-root-{}", Uuid::new_v4()));
        let runtime_root = temp.join("runtime-root");
        fs::create_dir_all(runtime_root.join("scripts/shell-hooks/zsh")).unwrap();
        fs::write(
            runtime_root.join("scripts/shell-hooks/zsh/.zshenv"),
            "# test\n",
        )
        .unwrap();
        fs::write(
            runtime_root.join("scripts/shell-hooks/zsh/.zprofile"),
            "# test\n",
        )
        .unwrap();
        fs::write(
            runtime_root.join("scripts/shell-hooks/zsh/.zshrc"),
            "# test\n",
        )
        .unwrap();
        fs::write(
            runtime_root.join("scripts/shell-hooks/dmux-ai-hook.zsh"),
            "# test\n",
        )
        .unwrap();
        let context = TerminalLaunchContext {
            project_id: "project-1".to_string(),
            project_name: "Codux".to_string(),
            project_path: PathBuf::from("/workspace/codux"),
            support_dir: PathBuf::from("/support/Codux"),
            runtime_root: runtime_root.clone(),
            terminal_id: Some("gpui-term-1".to_string()),
            slot_id: Some("gpui-pane-1-1".to_string()),
            session_key: Some("gpui:project-1:gpui-term-1:gpui-pane-1-1".to_string()),
            session_title: Some("终端 1".to_string()),
            session_cwd: Some(PathBuf::from("/workspace/codux")),
            session_instance_id: Some("session-instance-1".to_string()),
            tool_permissions_file: Some(PathBuf::from("/tmp/codux/tool-permissions.json")),
            memory_workspace_root: Some(PathBuf::from("/tmp/codux/memory-workspaces/project-1")),
            memory_prompt_file: Some(PathBuf::from(
                "/tmp/codux/memory-workspaces/project-1/memory-prompt.txt",
            )),
            memory_index_file: Some(PathBuf::from(
                "/tmp/codux/memory-workspaces/project-1/MEMORY.md",
            )),
            host_device_id: None,
        };
        let env = terminal_environment(
            "/bin/zsh",
            Some("/workspace/codux"),
            "gpui-term-1",
            &context.to_config(),
            Some(&context),
        );
        let path = env.get("PATH").expect("PATH should be set");
        assert!(path.starts_with(runtime_root.join("scripts/wrappers/bin").to_str().unwrap()));
        assert_eq!(
            env.get("DMUX_PROJECT_PATH").map(String::as_str),
            Some("/workspace/codux")
        );
        // Claude Code defaults to its classic (scrollback) renderer for a clean
        // desktop<->mobile handoff, unless the user set the var themselves.
        assert_eq!(
            env.get("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            env.get("CODUX_TERMINAL_ID").map(String::as_str),
            Some("gpui-term-1")
        );
        assert_eq!(
            env.get("DMUX_SESSION_INSTANCE_ID").map(String::as_str),
            Some("session-instance-1")
        );
        assert_eq!(
            env.get("DMUX_AI_MEMORY_INDEX_FILE").map(String::as_str),
            Some("/tmp/codux/memory-workspaces/project-1/MEMORY.md")
        );
        assert_eq!(
            env.get("DMUX_WRAPPER_BIN").map(String::as_str),
            Some(runtime_root.join("scripts/wrappers/bin").to_str().unwrap())
        );
        assert_eq!(
            env.get("DMUX_APP_SUPPORT_ROOT").map(String::as_str),
            Some("/support/Codux")
        );
        assert_eq!(
            env.get("CODUX_SSH_PROFILES_FILE").map(String::as_str),
            Some("/support/Codux/ssh_profiles.json")
        );
        assert_eq!(
            env.get("DMUX_USER_ZDOTDIR").map(String::as_str),
            env.get("HOME").map(String::as_str)
        );
        assert_eq!(
            env.get("ZDOTDIR").map(String::as_str),
            Some(
                runtime_root
                    .join("scripts/shell-hooks/zsh")
                    .to_str()
                    .unwrap()
            )
        );
        assert_eq!(
            env.get("DMUX_ZSH_HOOK_SCRIPT").map(String::as_str),
            Some(
                runtime_root
                    .join("scripts/shell-hooks/dmux-ai-hook.zsh")
                    .to_str()
                    .unwrap()
            )
        );
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn terminal_environment_does_not_override_zdotdir_when_runtime_zsh_hook_is_incomplete() {
        let temp = std::env::temp_dir().join(format!(
            "codux-terminal-runtime-root-missing-hook-{}",
            Uuid::new_v4()
        ));
        let runtime_root = temp.join("runtime-root");
        fs::create_dir_all(runtime_root.join("scripts/shell-hooks/zsh")).unwrap();
        let context = TerminalLaunchContext {
            project_id: "project-1".to_string(),
            project_name: "Codux".to_string(),
            project_path: PathBuf::from("/workspace/codux"),
            support_dir: PathBuf::from("/support/Codux"),
            runtime_root: runtime_root.clone(),
            terminal_id: Some("gpui-term-1".to_string()),
            slot_id: Some("gpui-pane-1-1".to_string()),
            session_key: Some("gpui:project-1:gpui-term-1:gpui-pane-1-1".to_string()),
            session_title: Some("Terminal 1".to_string()),
            session_cwd: Some(PathBuf::from("/workspace/codux")),
            session_instance_id: Some("session-instance-1".to_string()),
            tool_permissions_file: None,
            memory_workspace_root: None,
            memory_prompt_file: None,
            memory_index_file: None,
            host_device_id: None,
        };

        let env = terminal_environment(
            "/bin/zsh",
            Some("/workspace/codux"),
            "gpui-term-1",
            &context.to_config(),
            Some(&context),
        );

        assert_ne!(
            env.get("ZDOTDIR").map(String::as_str),
            Some(
                runtime_root
                    .join("scripts/shell-hooks/zsh")
                    .to_str()
                    .unwrap()
            )
        );
        assert!(!env.contains_key("DMUX_ZSH_HOOK_SCRIPT"));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn terminal_environment_keeps_runtime_context_compact() {
        let context = TerminalLaunchContext {
            project_id: "project-1".to_string(),
            project_name: "Codux".to_string(),
            project_path: PathBuf::from("/workspace/codux"),
            support_dir: PathBuf::from("/support/Codux"),
            runtime_root: PathBuf::from("/runtime-assets"),
            terminal_id: Some("gpui-term-1".to_string()),
            slot_id: Some("gpui-pane-1-1".to_string()),
            session_key: Some("gpui:project-1:gpui-term-1:gpui-pane-1-1".to_string()),
            session_title: Some("Terminal 1".to_string()),
            session_cwd: Some(PathBuf::from("/workspace/codux")),
            session_instance_id: Some("session-instance-1".to_string()),
            tool_permissions_file: Some(PathBuf::from("/tmp/codux/tool-permissions.json")),
            memory_workspace_root: Some(PathBuf::from("/tmp/codux/memory-workspaces/project-1")),
            memory_prompt_file: Some(PathBuf::from(
                "/tmp/codux/memory-workspaces/project-1/memory-prompt.txt",
            )),
            memory_index_file: Some(PathBuf::from(
                "/tmp/codux/memory-workspaces/project-1/MEMORY.md",
            )),
            host_device_id: None,
        };

        let env = terminal_environment(
            "/bin/zsh",
            Some("/workspace/codux"),
            "gpui-term-1",
            &context.to_config(),
            Some(&context),
        );
        let total_bytes = env
            .iter()
            .map(|(key, value)| key.len() + value.len() + 2)
            .sum::<usize>();

        assert!(total_bytes < 16 * 1024);
    }

    #[cfg(not(windows))]
    #[test]
    fn parses_noisy_shell_environment_capture() {
        let mut output = Vec::new();
        output.extend_from_slice(b"startup noise");
        output.extend_from_slice(b"__BEGIN__\0PATH=/opt/bin:/usr/bin\0HISTFILE=/tmp/history\0");
        output.extend_from_slice(b"__END__\0more noise");

        let env = parse_captured_shell_environment(&output, "__BEGIN__", "__END__").unwrap();

        assert_eq!(
            env.get("PATH").map(String::as_str),
            Some("/opt/bin:/usr/bin")
        );
        assert_eq!(
            env.get("HISTFILE").map(String::as_str),
            Some("/tmp/history")
        );
    }

    #[cfg(unix)]
    #[test]
    fn remote_visible_viewport_expires_back_to_desktop() {
        let manager = TerminalManager::new();
        let temp =
            std::env::temp_dir().join(format!("codux-terminal-viewport-lock-{}", Uuid::new_v4()));
        fs::create_dir_all(&temp).unwrap();
        let session_id = manager
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf ready".to_string()),
                    cwd: Some(temp.to_string_lossy().to_string()),
                    cols: Some(100),
                    rows: Some(32),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");
        let session = manager.session(&session_id).expect("session");
        let handle = session.clone_handle();

        handle
            .claim_viewport("remote:phone")
            .expect("remote visible claim");
        handle
            .resize_viewport("remote:phone", 72, 18)
            .expect("remote resize")
            .expect("remote resize accepted");
        {
            let mut viewport = session.viewport.lock();
            viewport.expires_at = Instant::now() - Duration::from_secs(1);
        }

        let expired = handle
            .release_expired_viewport_lease()
            .expect("expired viewport state");
        assert_eq!(expired.owner, terminal_viewport_local_owner());
        // The remote viewer drives columns only; the row count stays the host's
        // (32), so the desktop never adopts a grid taller than its viewport.
        assert_eq!((expired.cols, expired.rows), (72, 32));

        let accepted = handle
            .resize_viewport(terminal_viewport_local_owner(), 100, 32)
            .expect("desktop resize after lease expiry")
            .expect("desktop resize accepted");
        let state = handle.viewport_state();
        assert_eq!(state.owner, terminal_viewport_local_owner());
        assert_eq!((accepted.cols, accepted.rows), (100, 32));

        let _ = session.kill();
        fs::remove_dir_all(temp).ok();
    }

    #[cfg(unix)]
    #[test]
    fn desktop_resize_waits_for_remote_viewport_release() {
        let manager = TerminalManager::new();
        let temp = std::env::temp_dir().join(format!(
            "codux-terminal-viewport-release-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&temp).unwrap();
        let session_id = manager
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf ready".to_string()),
                    cwd: Some(temp.to_string_lossy().to_string()),
                    cols: Some(100),
                    rows: Some(32),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");
        let session = manager.session(&session_id).expect("session");
        let handle = session.clone_handle();

        handle.claim_viewport("remote:phone").expect("remote claim");
        handle
            .resize_viewport("remote:phone", 72, 18)
            .expect("remote resize")
            .expect("remote resize accepted");

        let ignored = handle
            .resize_viewport(terminal_viewport_local_owner(), 120, 40)
            .expect("desktop resize while remote owns");
        assert!(ignored.is_none());
        assert_eq!(handle.viewport_state().owner, "remote:phone");
        // Remote drives columns only; rows stay the host's (32).
        assert_eq!(
            (handle.viewport_state().cols, handle.viewport_state().rows),
            (72, 32)
        );

        handle
            .release_viewport("remote:phone")
            .expect("remote release")
            .expect("release state");
        let accepted = handle
            .resize_viewport(terminal_viewport_local_owner(), 120, 40)
            .expect("desktop resize after release")
            .expect("desktop resize accepted");
        assert_eq!(accepted.owner, terminal_viewport_local_owner());
        assert_eq!((accepted.cols, accepted.rows), (120, 40));

        let _ = session.kill();
        fs::remove_dir_all(temp).ok();
    }

    #[cfg(unix)]
    #[test]
    fn viewport_keepalive_prevents_remote_lease_expiry() {
        let manager = TerminalManager::new();
        let temp = std::env::temp_dir().join(format!(
            "codux-terminal-viewport-keepalive-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&temp).unwrap();
        let session_id = manager
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf ready".to_string()),
                    cwd: Some(temp.to_string_lossy().to_string()),
                    cols: Some(100),
                    rows: Some(32),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");
        let session = manager.session(&session_id).expect("session");
        let handle = session.clone_handle();

        handle.claim_viewport("remote:phone").expect("remote claim");
        {
            let mut viewport = session.viewport.lock();
            viewport.expires_at = Instant::now() - Duration::from_secs(1);
        }
        handle.touch_viewport_lease("remote:phone");
        assert!(handle.release_expired_viewport_lease().is_none());
        assert_eq!(handle.viewport_state().owner, "remote:phone");

        let _ = session.kill();
        fs::remove_dir_all(temp).ok();
    }

    #[cfg(unix)]
    #[test]
    fn terminal_manager_reuses_session_and_broadcasts_to_subscribers() {
        let manager = TerminalManager::new();
        let emit: EventSink = Arc::new(|_| true);
        let config = TerminalPtyConfig {
            terminal_id: Some(format!("test-terminal-{}", Uuid::new_v4())),
            shell: Some("/bin/cat".to_string()),
            cols: Some(80),
            rows: Some(24),
            scrollback_lines: Some(100),
            ..Default::default()
        };

        let (first_session, first_rx) = manager
            .attach_or_create_with_context(config.clone(), None, emit.clone())
            .expect("terminal should start");
        first_session
            .write(b"first-shared-output\n")
            .expect("write should succeed");
        assert!(
            recv_until_contains(&first_rx, "first-shared-output", Duration::from_secs(2))
                .contains("first-shared-output")
        );

        let (second_session, second_rx) = manager
            .attach_or_create_with_context(config, None, emit)
            .expect("terminal should attach");
        assert!(Arc::ptr_eq(&first_session, &second_session));

        first_session
            .write(b"second-shared-output\n")
            .expect("write should succeed");
        assert!(
            recv_until_contains(&first_rx, "second-shared-output", Duration::from_secs(2))
                .contains("second-shared-output")
        );
        assert!(
            recv_until_contains(&second_rx, "second-shared-output", Duration::from_secs(2))
                .contains("second-shared-output")
        );

        let _ = first_session.kill();
    }

    #[cfg(unix)]
    #[test]
    fn reattach_appends_keyframe_only_for_alt_screen_session() {
        let manager = TerminalManager::new();
        let emit: EventSink = Arc::new(|_| true);
        let config = TerminalPtyConfig {
            terminal_id: Some(format!("test-altscreen-{}", Uuid::new_v4())),
            shell: Some("/bin/cat".to_string()),
            cols: Some(80),
            rows: Some(24),
            scrollback_lines: Some(100),
            ..Default::default()
        };

        let (session, first_rx) = manager
            .attach_or_create_with_context(config.clone(), None, emit.clone())
            .expect("terminal should start");

        // Normal screen: a re-attach replays only the raw history; it never
        // appends the keyframe (identified by its cursor-hide repaint prefix).
        session
            .write(b"normal-line\n")
            .expect("write should succeed");
        assert!(
            recv_until_contains(&first_rx, "normal-line", Duration::from_secs(2))
                .contains("normal-line")
        );
        let (_normal_session, normal_rx) = manager
            .attach_or_create_with_context(config.clone(), None, emit.clone())
            .expect("terminal should attach");
        let normal_replay = recv_until_contains(&normal_rx, "normal-line", Duration::from_secs(2));
        assert!(normal_replay.contains("normal-line"));
        assert!(!normal_replay.contains("\x1b[?25l"));

        // Enter the alternate screen and let it apply to the live screen.
        session
            .write(b"\x1b[?1049h\x1b[2J\x1b[HALT_SCREEN_MARKER\n")
            .expect("write should succeed");
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && !session.screen_snapshot().input_mode.alternate_screen {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(session.screen_snapshot().input_mode.alternate_screen);

        // Alt screen: the re-attach replay now carries the live keyframe, so the
        // current screen and its alt-screen mode are reconstructed even though
        // the alternate buffer never reached the raw history.
        let (_alt_session, alt_rx) = manager
            .attach_or_create_with_context(config, None, emit)
            .expect("terminal should attach");
        let alt_replay = recv_until_contains(&alt_rx, "\x1b[?25l", Duration::from_secs(2));
        assert!(alt_replay.contains("\x1b[?25l"));
        assert!(alt_replay.contains("\x1b[?1049h"));

        let _ = session.kill();
    }

    #[cfg(unix)]
    #[test]
    fn terminal_manager_ensures_session_before_ui_attach() {
        let manager = TerminalManager::new();
        let terminal_id = format!("test-prewarm-terminal-{}", Uuid::new_v4());
        let config = TerminalPtyConfig {
            terminal_id: Some(terminal_id.clone()),
            shell: Some("/bin/cat".to_string()),
            cols: Some(80),
            rows: Some(24),
            scrollback_lines: Some(100),
            ..Default::default()
        };

        let ensured_id = manager
            .ensure_session_with_context(config.clone(), None)
            .expect("terminal should prewarm");
        assert_eq!(ensured_id, terminal_id);

        let emit: EventSink = Arc::new(|_| true);
        let (session, rx) = manager
            .attach_or_create_with_context(config, None, emit)
            .expect("terminal should attach");
        assert_eq!(session.id(), ensured_id);
        session
            .write(b"prewarm-shared-output\n")
            .expect("write should succeed");
        assert!(
            recv_until_contains(&rx, "prewarm-shared-output", Duration::from_secs(2))
                .contains("prewarm-shared-output")
        );

        let _ = session.kill();
    }

    #[cfg(unix)]
    #[test]
    fn terminal_manager_replaces_same_terminal_id_when_identity_changes() {
        let manager = TerminalManager::new();
        let emit: EventSink = Arc::new(|_| true);
        let terminal_id = format!("test-scoped-terminal-{}", Uuid::new_v4());
        let first_cwd = std::env::temp_dir().join(format!("codux-pty-first-{}", Uuid::new_v4()));
        let second_cwd = std::env::temp_dir().join(format!("codux-pty-second-{}", Uuid::new_v4()));
        fs::create_dir_all(&first_cwd).unwrap();
        fs::create_dir_all(&second_cwd).unwrap();

        let first_config = TerminalPtyConfig {
            terminal_id: Some(terminal_id.clone()),
            shell: Some("/bin/cat".to_string()),
            cwd: Some(first_cwd.display().to_string()),
            project_id: Some("worktree-a".to_string()),
            session_key: Some(format!("gpui:worktree-a:{terminal_id}")),
            cols: Some(80),
            rows: Some(24),
            scrollback_lines: Some(100),
            ..Default::default()
        };
        let second_config = TerminalPtyConfig {
            cwd: Some(second_cwd.display().to_string()),
            project_id: Some("worktree-b".to_string()),
            session_key: Some(format!("gpui:worktree-b:{terminal_id}")),
            ..first_config.clone()
        };

        let (first_session, _) = manager
            .attach_or_create_with_context(first_config, None, emit.clone())
            .expect("first terminal should start");
        assert_eq!(first_session.info().cwd, first_cwd.display().to_string());
        assert_eq!(first_session.info().project_id, "worktree-a");

        let (second_session, _) = manager
            .attach_or_create_with_context(second_config, None, emit)
            .expect("second terminal should replace incompatible session");
        assert!(!Arc::ptr_eq(&first_session, &second_session));
        assert_eq!(second_session.id(), terminal_id);
        assert_eq!(second_session.info().cwd, second_cwd.display().to_string());
        assert_eq!(second_session.info().project_id, "worktree-b");

        let _ = second_session.kill();
        let _ = fs::remove_dir_all(first_cwd);
        let _ = fs::remove_dir_all(second_cwd);
    }

    #[cfg(unix)]
    #[test]
    fn terminal_manager_uses_context_session_cwd_for_identity() {
        let manager = TerminalManager::new();
        let emit: EventSink = Arc::new(|_| true);
        let terminal_id = format!("test-context-cwd-terminal-{}", Uuid::new_v4());
        let project_cwd = std::env::temp_dir().join(format!("codux-project-{}", Uuid::new_v4()));
        let worktree_cwd = std::env::temp_dir().join(format!("codux-worktree-{}", Uuid::new_v4()));
        fs::create_dir_all(&project_cwd).unwrap();
        fs::create_dir_all(&worktree_cwd).unwrap();
        let context = TerminalLaunchContext {
            project_id: "worktree-context".to_string(),
            project_name: "Context Worktree".to_string(),
            project_path: project_cwd.clone(),
            support_dir: std::env::temp_dir(),
            runtime_root: std::env::temp_dir(),
            terminal_id: Some(terminal_id.clone()),
            slot_id: None,
            session_key: Some(format!("gpui:worktree-context:{terminal_id}")),
            session_title: None,
            session_cwd: Some(worktree_cwd.clone()),
            session_instance_id: None,
            tool_permissions_file: None,
            memory_workspace_root: None,
            memory_prompt_file: None,
            memory_index_file: None,
            host_device_id: None,
        };
        let config = TerminalPtyConfig {
            terminal_id: Some(terminal_id),
            shell: Some("/bin/cat".to_string()),
            cols: Some(80),
            rows: Some(24),
            scrollback_lines: Some(100),
            ..Default::default()
        };

        let (session, _) = manager
            .attach_or_create_with_context(config, Some(&context), emit)
            .expect("terminal should use context session cwd");

        assert_eq!(session.info().cwd, worktree_cwd.display().to_string());
        assert_eq!(session.info().project_id, "worktree-context");

        let _ = session.kill();
        let _ = fs::remove_dir_all(project_cwd);
        let _ = fs::remove_dir_all(worktree_cwd);
    }

    #[test]
    fn terminal_event_subscribers_are_pruned_when_sink_is_closed() {
        let subscribers: Arc<parking_lot::Mutex<Vec<EventSink>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        subscribers
            .lock()
            .push(Arc::new(move |_| tx.send(()).is_ok()));

        emit_terminal_event(
            &subscribers,
            TerminalEvent::Exit {
                session_id: "session-a".to_string(),
                exit_code: None,
            },
        );
        assert_eq!(subscribers.lock().len(), 1);
        drop(rx);

        emit_terminal_event(
            &subscribers,
            TerminalEvent::Exit {
                session_id: "session-a".to_string(),
                exit_code: None,
            },
        );
        assert!(subscribers.lock().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn terminal_manager_registers_ai_runtime_terminal_lifecycle() {
        let dir = std::env::temp_dir().join(format!("codux-terminal-bridge-{}", Uuid::new_v4()));
        let bridge = Arc::new(AIRuntimeBridge::with_paths(
            dir.join("root"),
            dir.join("temp"),
            dir.join("home"),
        ));
        let manager = TerminalManager::with_ai_runtime(Arc::clone(&bridge));
        let terminal_id = format!("test-ai-terminal-{}", Uuid::new_v4());
        let config = TerminalPtyConfig {
            terminal_id: Some(terminal_id.clone()),
            project_id: Some("project-1".to_string()),
            slot_id: Some("slot-1".to_string()),
            session_key: Some("session-key-1".to_string()),
            title: Some("Codex".to_string()),
            tool: Some("codex".to_string()),
            shell: Some("/bin/cat".to_string()),
            cols: Some(80),
            rows: Some(24),
            scrollback_lines: Some(100),
            ..Default::default()
        };
        let emit: EventSink = Arc::new(|_| true);

        let (session, _) = manager
            .attach_or_create_with_context(config, None, emit)
            .expect("terminal should start");

        let terminals = bridge.registry().snapshot();
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].terminal_id, terminal_id);
        assert_eq!(terminals[0].project_id, "project-1");
        assert_eq!(terminals[0].slot_id, "slot-1");
        assert_eq!(terminals[0].tool.as_deref(), Some("codex"));

        manager.kill(session.id()).expect("terminal should stop");
        assert!(bridge.registry().snapshot().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn codewhale_terminal_progress_osc_completes_running_session() {
        let dir = std::env::temp_dir().join(format!(
            "codux-codewhale-terminal-progress-{}",
            Uuid::new_v4()
        ));
        let bridge = Arc::new(AIRuntimeBridge::with_paths(
            dir.join("root"),
            dir.join("temp"),
            dir.join("home"),
        ));
        bridge.ensure_started().expect("runtime should start");
        let terminal_id = format!("test-codewhale-terminal-{}", Uuid::new_v4());
        let binding = AIRuntimeTerminalBinding {
            terminal_id: terminal_id.clone(),
            project_id: "project-1".to_string(),
            slot_id: "slot-1".to_string(),
            title: "Terminal".to_string(),
            cwd: "/tmp/project".to_string(),
            tool: None,
            is_active: false,
            session_key: Some("codewhale-session-1".to_string()),
            terminal_instance_id: Some("terminal-instance-1".to_string()),
        };
        let mut watcher =
            CodeWhaleTerminalProgressWatcher::new(binding.clone(), Arc::clone(&bridge));
        bridge
            .submit_hook_event(AIHookEventPayload {
                kind: "promptSubmitted".to_string(),
                terminal_id: terminal_id.clone(),
                terminal_instance_id: binding.terminal_instance_id.clone(),
                project_id: "project-1".to_string(),
                project_name: "Codux".to_string(),
                project_path: Some("/tmp/project".to_string()),
                session_title: "Terminal".to_string(),
                tool: "codewhale".to_string(),
                ai_session_id: Some("codewhale-session-1".to_string()),
                model: None,
                input_tokens: None,
                output_tokens: None,
                cached_input_tokens: None,
                total_tokens: None,
                updated_at: now_seconds(),
                metadata: None,
            })
            .expect("prompt hook should submit");
        wait_for_session_state(&bridge, &terminal_id, "responding", Duration::from_secs(2));

        watcher.handle_terminal_event(&TerminalEvent::Output {
            session_id: terminal_id.clone(),
            text: String::new(),
            bytes: b"\x1b]9;4;0\x07".to_vec(),
        });
        wait_for_session_state(&bridge, &terminal_id, "idle", Duration::from_secs(2));

        let snapshot = bridge.runtime_state_snapshot();
        let session = snapshot
            .sessions
            .iter()
            .find(|session| session.terminal_id == terminal_id)
            .expect("session should exist");
        assert_eq!(session.tool, "codewhale");
        assert!(session.has_completed_turn);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn terminal_progress_osc_does_not_complete_non_codewhale_session() {
        let dir = std::env::temp_dir().join(format!(
            "codux-codewhale-terminal-progress-ignore-{}",
            Uuid::new_v4()
        ));
        let bridge = Arc::new(AIRuntimeBridge::with_paths(
            dir.join("root"),
            dir.join("temp"),
            dir.join("home"),
        ));
        bridge.ensure_started().expect("runtime should start");
        let terminal_id = format!("test-codex-terminal-{}", Uuid::new_v4());
        let binding = AIRuntimeTerminalBinding {
            terminal_id: terminal_id.clone(),
            project_id: "project-1".to_string(),
            slot_id: "slot-1".to_string(),
            title: "Terminal".to_string(),
            cwd: "/tmp/project".to_string(),
            tool: None,
            is_active: false,
            session_key: Some("codex-session-1".to_string()),
            terminal_instance_id: Some("terminal-instance-1".to_string()),
        };
        let mut watcher =
            CodeWhaleTerminalProgressWatcher::new(binding.clone(), Arc::clone(&bridge));
        bridge
            .submit_hook_event(AIHookEventPayload {
                kind: "promptSubmitted".to_string(),
                terminal_id: terminal_id.clone(),
                terminal_instance_id: binding.terminal_instance_id.clone(),
                project_id: "project-1".to_string(),
                project_name: "Codux".to_string(),
                project_path: Some("/tmp/project".to_string()),
                session_title: "Terminal".to_string(),
                tool: "codex".to_string(),
                ai_session_id: Some("codex-session-1".to_string()),
                model: None,
                input_tokens: None,
                output_tokens: None,
                cached_input_tokens: None,
                total_tokens: None,
                updated_at: now_seconds(),
                metadata: None,
            })
            .expect("prompt hook should submit");
        wait_for_session_state(&bridge, &terminal_id, "responding", Duration::from_secs(2));

        watcher.handle_terminal_event(&TerminalEvent::Output {
            session_id: terminal_id.clone(),
            text: String::new(),
            bytes: b"\x1b]9;4;0\x07".to_vec(),
        });
        std::thread::sleep(Duration::from_millis(150));

        let snapshot = bridge.runtime_state_snapshot();
        let session = snapshot
            .sessions
            .iter()
            .find(|session| session.terminal_id == terminal_id)
            .expect("session should exist");
        assert_eq!(session.tool, "codex");
        assert_eq!(session.state, "responding");
        assert!(!session.has_completed_turn);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    fn recv_until_contains(
        rx: &flume::Receiver<Vec<u8>>,
        needle: &str,
        timeout: Duration,
    ) -> String {
        let deadline = Instant::now() + timeout;
        let mut text = String::new();
        while Instant::now() < deadline && !text.contains(needle) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining.min(Duration::from_millis(100))) {
                Ok(bytes) => text.push_str(&String::from_utf8_lossy(&bytes)),
                Err(flume::RecvTimeoutError::Timeout) => {}
                Err(flume::RecvTimeoutError::Disconnected) => break,
            }
        }
        text
    }

    #[cfg(unix)]
    fn wait_for_session_state(
        bridge: &AIRuntimeBridge,
        terminal_id: &str,
        state: &str,
        timeout: Duration,
    ) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if bridge
                .runtime_state_snapshot()
                .sessions
                .iter()
                .any(|session| session.terminal_id == terminal_id && session.state == state)
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!(
            "terminal {terminal_id} did not reach state {state}; snapshot={:?}",
            bridge.runtime_state_snapshot().sessions
        );
    }
}
