use crate::ai_runtime::{AIRuntimeBridge, AIRuntimeTerminalBinding};
use crate::app_settings::{AIRuntimeToolSettings, AppSettings, AppSettingsStore};
use crate::memory::{MemoryLaunchRequest, MemoryStore};
use crate::paths::{app_display_name, app_slug, runtime_temp_dir};
use crate::ssh::{render_ssh_launch_context, ssh_profiles_file_path};
use anyhow::{anyhow, Context, Result};
use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::Command;
#[cfg(not(windows))]
use std::sync::OnceLock;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::ipc::{Channel, Response};
use thiserror::Error;
use uuid::Uuid;

const MIN_HISTORY_BYTES: usize = 128 * 1024;
const MAX_CONFIGURED_HISTORY_BYTES: usize = 8 * 1024 * 1024;
const OUTPUT_CHUNK_BYTES: usize = 16 * 1024;
const OUTPUT_BATCH_BYTES: usize = 96 * 1024;
const OUTPUT_BATCH_DELAY: Duration = Duration::from_millis(16);

#[cfg(windows)]
const FALLBACK_PATH: &str = "C:\\Windows\\System32;C:\\Windows;C:\\Windows\\System32\\Wbem;C:\\Windows\\System32\\WindowsPowerShell\\v1.0";

#[cfg(not(windows))]
const FALLBACK_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin";

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
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
];

#[cfg(windows)]
const PATH_SEPARATOR: char = ';';

#[cfg(not(windows))]
const PATH_SEPARATOR: char = ':';

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalConfig {
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub command: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub env: Option<HashMap<String, String>>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub terminal_id: Option<String>,
    pub slot_id: Option<String>,
    pub session_key: Option<String>,
    pub title: Option<String>,
    pub tool: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TerminalEvent {
    Output {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        text: String,
        #[serde(skip)]
        bytes: Vec<u8>,
    },
    Exit {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "exitCode")]
        exit_code: Option<i32>,
    },
    Error {
        #[serde(rename = "sessionId")]
        session_id: String,
        message: String,
    },
}

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("terminal session not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

type EventSink = Arc<dyn Fn(TerminalEvent) + Send + Sync + 'static>;

enum TerminalReaderMessage {
    Data(Vec<u8>),
    Eof,
    Error(String),
}

pub struct TerminalManager {
    sessions: Mutex<HashMap<String, Arc<TerminalSession>>>,
    ai_runtime: Arc<AIRuntimeBridge>,
    settings: Arc<AppSettingsStore>,
    memory: Arc<MemoryStore>,
}

impl TerminalManager {
    pub fn new(
        ai_runtime: Arc<AIRuntimeBridge>,
        settings: Arc<AppSettingsStore>,
        memory: Arc<MemoryStore>,
    ) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            ai_runtime,
            settings,
            memory,
        }
    }

    pub fn list(&self) -> Vec<TerminalSessionSnapshot> {
        let sessions = match self.sessions.lock() {
            Ok(sessions) => sessions.values().cloned().collect::<Vec<_>>(),
            Err(_) => return Vec::new(),
        };
        sessions
            .into_iter()
            .filter_map(|session| session.info())
            .collect()
    }

    pub fn create_with_channels<F>(
        &self,
        config: TerminalConfig,
        emit: F,
        output: Option<Channel<Response>>,
        exit: Option<Channel<i32>>,
    ) -> Result<String, TerminalError>
    where
        F: Fn(TerminalEvent) + Send + Sync + 'static,
    {
        self.ai_runtime
            .ensure_started()
            .map_err(anyhow::Error::msg)?;
        let id = config
            .terminal_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let sink: EventSink = Arc::new(emit);
        let session = TerminalSession::spawn(
            id.clone(),
            config,
            sink,
            output,
            exit,
            Arc::clone(&self.ai_runtime),
            Arc::clone(&self.settings),
            Arc::clone(&self.memory),
        )?;
        self.sessions
            .lock()
            .map_err(|_| anyhow!("terminal session lock poisoned"))?
            .insert(id.clone(), session);
        Ok(id)
    }

    pub fn write(&self, session_id: &str, data: &[u8]) -> Result<(), TerminalError> {
        let session = self.session(session_id)?;
        session.write(data)
    }

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), TerminalError> {
        let session = self.session(session_id)?;
        session.resize(cols, rows)
    }

    pub fn kill(&self, session_id: &str) -> Result<(), TerminalError> {
        let session = self.session(session_id)?;
        session.kill()?;
        if let Ok(binding) = session.binding.lock() {
            self.ai_runtime.registry().remove(&binding.terminal_id);
        }
        self.sessions
            .lock()
            .map_err(|_| anyhow!("terminal session lock poisoned"))?
            .remove(session_id);
        Ok(())
    }

    pub fn kill_all(&self) {
        let sessions = match self.sessions.lock() {
            Ok(mut sessions) => sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>(),
            Err(_) => return,
        };

        for session in sessions {
            if let Ok(binding) = session.binding.lock() {
                self.ai_runtime.registry().remove(&binding.terminal_id);
            }
            let _ = session.kill();
        }
    }

    pub fn snapshot(&self, session_id: &str) -> Result<String, TerminalError> {
        let session = self.session(session_id)?;
        session.snapshot()
    }

    pub fn buffer_characters(&self, session_id: &str) -> Result<usize, TerminalError> {
        let session = self.session(session_id)?;
        session.buffer_characters()
    }

    pub fn clear_history(&self, session_id: &str) -> Result<(), TerminalError> {
        let session = self.session(session_id)?;
        session.clear_history()
    }

    pub fn attach_channels(
        &self,
        session_id: &str,
        output: Option<Channel<Response>>,
        exit: Option<Channel<i32>>,
    ) -> Result<(), TerminalError> {
        let session = self.session(session_id)?;
        session.attach_channels(output, exit);
        Ok(())
    }

    fn session(&self, session_id: &str) -> Result<Arc<TerminalSession>, TerminalError> {
        self.sessions
            .lock()
            .map_err(|_| anyhow!("terminal session lock poisoned"))?
            .get(session_id)
            .cloned()
            .ok_or_else(|| TerminalError::NotFound(session_id.to_string()))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionSnapshot {
    pub id: String,
    pub title: String,
    pub project_id: String,
    pub project_name: String,
    pub cwd: String,
    pub shell: String,
    pub command: String,
    pub cols: u16,
    pub rows: u16,
    pub status: String,
    pub is_running: bool,
    pub created_at: String,
    pub last_active_at: String,
    pub buffer_characters: usize,
    pub has_buffer: bool,
}

struct TerminalSession {
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    writer: Mutex<Box<dyn Write + Send>>,
    output_channels: Mutex<Vec<Channel<Response>>>,
    exit_channels: Mutex<Vec<Channel<i32>>>,
    history: Mutex<RingHistory>,
    binding: Mutex<AIRuntimeTerminalBinding>,
    info: Mutex<TerminalSessionSnapshot>,
}

impl TerminalSession {
    fn spawn(
        id: String,
        config: TerminalConfig,
        emit: EventSink,
        output: Option<Channel<Response>>,
        exit: Option<Channel<i32>>,
        ai_runtime: Arc<AIRuntimeBridge>,
        settings: Arc<AppSettingsStore>,
        memory: Arc<MemoryStore>,
    ) -> Result<Arc<Self>, TerminalError> {
        let TerminalConfig {
            cwd,
            shell,
            command: initial_command,
            cols,
            rows,
            env,
            project_id,
            project_name,
            terminal_id,
            slot_id,
            session_key,
            title,
            tool,
        } = config;

        let pty_system = native_pty_system();
        let cols = cols.unwrap_or(100).max(20);
        let rows = rows.unwrap_or(30).max(8);
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open PTY")?;

        let shell = shell.unwrap_or_else(default_shell);
        let cwd = normalize_terminal_cwd(cwd);
        let initial_command = initial_command.filter(|value| !value.trim().is_empty());
        let mut command = build_shell_command(&shell, initial_command.as_deref());

        if let Some(cwd) = cwd.as_deref() {
            command.cwd(PathBuf::from(cwd));
        }

        clear_terminal_environment(&mut command);
        let process_instance_id = Uuid::new_v4().to_string().to_lowercase();
        let terminal_env = TerminalEnvironment::resolve(TerminalEnvironmentRequest {
            shell: &shell,
            cwd: cwd.as_deref(),
            project_id: project_id.as_deref(),
            project_name: project_name.as_deref(),
            terminal_id: terminal_id.as_deref(),
            slot_id: slot_id.as_deref(),
            session_id: &id,
            session_key: session_key.as_deref(),
            session_title: title.as_deref(),
            process_instance_id: &process_instance_id,
            overrides: env.as_ref(),
            ai_runtime: &ai_runtime,
            settings: &settings,
            memory: &memory,
        });
        for (key, value) in terminal_env.values {
            command.env(key, value);
        }

        let child = pair
            .slave
            .spawn_command(command)
            .with_context(|| format!("failed to spawn shell {shell}"))?;
        let killer = child.clone_killer();
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone PTY reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to take PTY writer")?;

        let binding = AIRuntimeTerminalBinding {
            terminal_id: terminal_id.clone().unwrap_or_else(|| id.clone()),
            project_id: project_id.clone().unwrap_or_default(),
            slot_id: slot_id.clone().unwrap_or_default(),
            title: title.clone().unwrap_or_else(|| "Terminal".to_string()),
            cwd: cwd.clone().unwrap_or_default(),
            tool: tool.clone(),
            is_active: false,
            session_key: session_key.clone(),
            terminal_instance_id: Some(process_instance_id.clone()),
        };
        ai_runtime.registry().upsert(binding.clone());
        let now = chrono::Utc::now().to_rfc3339();
        let info = TerminalSessionSnapshot {
            id: id.clone(),
            title: title.clone().unwrap_or_else(|| "Terminal".to_string()),
            project_id: project_id.clone().unwrap_or_default(),
            project_name: project_name.clone().unwrap_or_default(),
            cwd: cwd.clone().unwrap_or_default(),
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
        };

        let history_bytes = terminal_history_bytes(&settings.snapshot(), cols);
        let session = Arc::new(Self {
            master: Mutex::new(pair.master),
            child: Mutex::new(child),
            killer: Mutex::new(killer),
            writer: Mutex::new(writer),
            output_channels: Mutex::new(output.into_iter().collect()),
            exit_channels: Mutex::new(exit.into_iter().collect()),
            history: Mutex::new(RingHistory::new(history_bytes)),
            binding: Mutex::new(binding),
            info: Mutex::new(info),
        });

        Self::spawn_reader(id.clone(), reader, Arc::clone(&session), Arc::clone(&emit));
        Self::spawn_waiter(id, Arc::clone(&session), emit);

        Ok(session)
    }

    fn write(&self, data: &[u8]) -> Result<(), TerminalError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow!("terminal writer lock poisoned"))?;
        writer.write_all(data).context("failed to write to PTY")?;
        writer.flush().context("failed to flush PTY input")?;
        Ok(())
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<(), TerminalError> {
        let master = self
            .master
            .lock()
            .map_err(|_| anyhow!("terminal master lock poisoned"))?;
        master
            .resize(PtySize {
                rows: rows.max(8),
                cols: cols.max(20),
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to resize PTY")?;
        if let Ok(mut info) = self.info.lock() {
            info.cols = cols.max(20);
            info.rows = rows.max(8);
            info.last_active_at = chrono::Utc::now().to_rfc3339();
        }
        Ok(())
    }

    fn kill(&self) -> Result<(), TerminalError> {
        let mut killer = self
            .killer
            .lock()
            .map_err(|_| anyhow!("terminal killer lock poisoned"))?;
        killer.kill().context("failed to kill terminal process")?;
        Ok(())
    }

    fn snapshot(&self) -> Result<String, TerminalError> {
        let history = self
            .history
            .lock()
            .map_err(|_| anyhow!("terminal history lock poisoned"))?;
        Ok(history.to_text())
    }

    fn buffer_characters(&self) -> Result<usize, TerminalError> {
        let history = self
            .history
            .lock()
            .map_err(|_| anyhow!("terminal history lock poisoned"))?;
        Ok(history.len_chars())
    }

    fn clear_history(&self) -> Result<(), TerminalError> {
        let mut history = self
            .history
            .lock()
            .map_err(|_| anyhow!("terminal history lock poisoned"))?;
        history.clear();
        if let Ok(mut info) = self.info.lock() {
            info.buffer_characters = 0;
            info.has_buffer = false;
            info.last_active_at = chrono::Utc::now().to_rfc3339();
        }
        Ok(())
    }

    fn info(&self) -> Option<TerminalSessionSnapshot> {
        let mut info = self.info.lock().ok()?.clone();
        if let Ok(history) = self.history.lock() {
            info.buffer_characters = history.len_chars();
            info.has_buffer = info.buffer_characters > 0;
        }
        Some(info)
    }

    fn attach_channels(
        &self,
        output: Option<Channel<Response>>,
        exit: Option<Channel<i32>>,
    ) {
        if let Some(output) = output {
            if let Ok(mut channels) = self.output_channels.lock() {
                channels.push(output);
            }
        }
        if let Some(exit) = exit {
            if let Ok(mut channels) = self.exit_channels.lock() {
                channels.push(exit);
            }
        }
    }

    fn send_output(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if let Ok(mut channels) = self.output_channels.lock() {
            channels.retain(|channel| channel.send(Response::new(bytes.to_vec())).is_ok());
        }
    }

    fn send_exit(&self, exit_code: Option<i32>) {
        let code = exit_code.unwrap_or(-1);
        if let Ok(mut channels) = self.exit_channels.lock() {
            channels.retain(|channel| channel.send(code).is_ok());
        }
    }

    fn spawn_reader(
        id: String,
        mut reader: Box<dyn Read + Send>,
        session: Arc<Self>,
        emit: EventSink,
    ) {
        let (tx, rx) = mpsc::channel::<TerminalReaderMessage>();
        let emitter_id = id.clone();
        let emitter_session = Arc::clone(&session);
        let emitter_sink = Arc::clone(&emit);
        thread::Builder::new()
            .name(format!("codux-terminal-emitter-{emitter_id}"))
            .spawn(move || {
                Self::run_output_emitter(emitter_id, emitter_session, emitter_sink, rx);
            })
            .expect("failed to spawn terminal emitter");

        thread::Builder::new()
            .name(format!("codux-terminal-reader-{id}"))
            .spawn(move || {
                let mut buffer = vec![0_u8; OUTPUT_CHUNK_BYTES];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => {
                            let _ = tx.send(TerminalReaderMessage::Eof);
                            break;
                        }
                        Ok(count) => {
                            if tx
                                .send(TerminalReaderMessage::Data(buffer[..count].to_vec()))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(error) => {
                            let _ = tx.send(TerminalReaderMessage::Error(error.to_string()));
                            break;
                        }
                    }
                }
            })
            .expect("failed to spawn terminal reader");
    }

    fn run_output_emitter(
        id: String,
        session: Arc<Self>,
        emit: EventSink,
        rx: mpsc::Receiver<TerminalReaderMessage>,
    ) {
        let mut pending_utf8 = Vec::new();
        let mut pending_bytes = Vec::with_capacity(OUTPUT_CHUNK_BYTES);

        loop {
            match rx.recv() {
                Ok(TerminalReaderMessage::Data(bytes)) => {
                    pending_bytes.extend_from_slice(&bytes);
                    while pending_bytes.len() < OUTPUT_BATCH_BYTES {
                        match rx.recv_timeout(OUTPUT_BATCH_DELAY) {
                            Ok(TerminalReaderMessage::Data(bytes)) => {
                                pending_bytes.extend_from_slice(&bytes);
                            }
                            Ok(TerminalReaderMessage::Eof) => {
                                Self::emit_output_batch(
                                    &id,
                                    &session,
                                    &emit,
                                    &mut pending_utf8,
                                    &mut pending_bytes,
                                );
                                Self::emit_pending_utf8(
                                    &id,
                                    &session,
                                    &emit,
                                    &mut pending_utf8,
                                );
                                return;
                            }
                            Ok(TerminalReaderMessage::Error(message)) => {
                                Self::emit_output_batch(
                                    &id,
                                    &session,
                                    &emit,
                                    &mut pending_utf8,
                                    &mut pending_bytes,
                                );
                                emit(TerminalEvent::Error {
                                    session_id: id,
                                    message,
                                });
                                return;
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => break,
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    Self::emit_output_batch(
                        &id,
                        &session,
                        &emit,
                        &mut pending_utf8,
                        &mut pending_bytes,
                    );
                }
                Ok(TerminalReaderMessage::Eof) => {
                    Self::emit_output_batch(
                        &id,
                        &session,
                        &emit,
                        &mut pending_utf8,
                        &mut pending_bytes,
                    );
                    Self::emit_pending_utf8(
                        &id,
                        &session,
                        &emit,
                        &mut pending_utf8,
                    );
                    return;
                }
                Ok(TerminalReaderMessage::Error(message)) => {
                    Self::emit_output_batch(
                        &id,
                        &session,
                        &emit,
                        &mut pending_utf8,
                        &mut pending_bytes,
                    );
                    emit(TerminalEvent::Error {
                        session_id: id,
                        message,
                    });
                    return;
                }
                Err(_) => return,
            }
        }
    }

    fn emit_output_batch(
        id: &str,
        session: &Arc<Self>,
        emit: &EventSink,
        pending_utf8: &mut Vec<u8>,
        pending_bytes: &mut Vec<u8>,
    ) {
        if pending_bytes.is_empty() {
            return;
        }
        let text = decode_utf8_output(pending_bytes, pending_utf8);
        let mut buffer_characters = None;
        if !text.is_empty() {
            if let Ok(mut history) = session.history.lock() {
                history.push_text(&text);
                buffer_characters = Some(history.len_chars());
            }
        }
        if let Ok(mut info) = session.info.lock() {
            info.last_active_at = chrono::Utc::now().to_rfc3339();
            if let Some(chars) = buffer_characters {
                info.buffer_characters = chars;
                info.has_buffer = chars > 0;
            } else if !text.is_empty() {
                info.buffer_characters =
                    info.buffer_characters.saturating_add(text.chars().count());
                info.has_buffer = true;
            }
        }
        let bytes = std::mem::take(pending_bytes);
        session.send_output(&bytes);
        emit(TerminalEvent::Output {
            session_id: id.to_string(),
            bytes,
            text,
        });
    }

    fn emit_pending_utf8(
        id: &str,
        session: &Arc<Self>,
        emit: &EventSink,
        pending_utf8: &mut Vec<u8>,
    ) {
        let text = flush_utf8_decoder(pending_utf8);
        if text.is_empty() {
            return;
        }
        let mut buffer_characters = None;
        if let Ok(mut history) = session.history.lock() {
            history.push_text(&text);
            buffer_characters = Some(history.len_chars());
        }
        if let Ok(mut info) = session.info.lock() {
            info.last_active_at = chrono::Utc::now().to_rfc3339();
            if let Some(chars) = buffer_characters {
                info.buffer_characters = chars;
                info.has_buffer = chars > 0;
            } else {
                info.buffer_characters =
                    info.buffer_characters.saturating_add(text.chars().count());
                info.has_buffer = true;
            }
        }
        let bytes = text.as_bytes().to_vec();
        session.send_output(&bytes);
        emit(TerminalEvent::Output {
            session_id: id.to_string(),
            bytes,
            text,
        });
    }

    fn spawn_waiter(id: String, session: Arc<Self>, emit: EventSink) {
        thread::Builder::new()
            .name(format!("codux-terminal-waiter-{id}"))
            .spawn(move || {
                let exit_code = match session.child.lock() {
                    Ok(mut child) => child.wait().ok().map(|status| status.exit_code() as i32),
                    Err(_) => None,
                };
                session.send_exit(exit_code);
                emit(TerminalEvent::Exit {
                    session_id: id,
                    exit_code,
                });
                if let Ok(mut info) = session.info.lock() {
                    info.status = "exited".to_string();
                    info.is_running = false;
                    info.last_active_at = chrono::Utc::now().to_rfc3339();
                }
            })
            .expect("failed to spawn terminal waiter");
    }
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
        return String::new();
    }
    String::from_utf8_lossy(&std::mem::take(pending)).to_string()
}

struct TerminalEnvironmentRequest<'a> {
    shell: &'a str,
    cwd: Option<&'a str>,
    project_id: Option<&'a str>,
    project_name: Option<&'a str>,
    terminal_id: Option<&'a str>,
    slot_id: Option<&'a str>,
    session_id: &'a str,
    session_key: Option<&'a str>,
    session_title: Option<&'a str>,
    process_instance_id: &'a str,
    overrides: Option<&'a HashMap<String, String>>,
    ai_runtime: &'a AIRuntimeBridge,
    settings: &'a AppSettingsStore,
    memory: &'a MemoryStore,
}

struct TerminalEnvironment {
    values: HashMap<String, String>,
}

impl TerminalEnvironment {
    fn resolve(request: TerminalEnvironmentRequest<'_>) -> Self {
        let home = home_dir();
        let user = user_name();
        let shell_name = shell_name(request.shell);
        let session_cwd = request.cwd.unwrap_or_else(|| home.as_str());
        let project_path = request.cwd.unwrap_or(session_cwd);
        let project_name = request
            .project_name
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| project_path_name(project_path))
            .unwrap_or_else(|| "Codux".to_string());
        let project_id = request
            .project_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&project_name);
        let session_title = request
            .session_title
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Terminal");
        let terminal_id = request
            .terminal_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(request.session_id);
        let slot_id = request
            .slot_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| request.session_key.unwrap_or(""));

        let mut values = HashMap::new();
        values.insert("HOME".to_string(), home.clone());
        values.insert("USER".to_string(), user.clone());
        values.insert("LOGNAME".to_string(), user.clone());
        values.insert("SHELL".to_string(), request.shell.to_string());
        values.insert("PWD".to_string(), session_cwd.to_string());

        append_passthrough_env(&mut values);

        for (key, value) in configured_dotenv(&home) {
            values.entry(key).or_insert(value);
        }
        for (key, value) in configured_codex_env(&home) {
            values.entry(key).or_insert(value);
        }

        let inherited_path = env::var("PATH").ok();
        let mut path =
            merged_executable_path(request.shell, &home, &user, inherited_path.as_deref());
        let wrapper_bin = request.ai_runtime.wrapper_bin_dir().display().to_string();
        let codux_ssh_command = request
            .ai_runtime
            .wrapper_bin_dir()
            .join(if cfg!(windows) {
                "codux-ssh.cmd"
            } else {
                "codux-ssh"
            })
            .display()
            .to_string();
        path = prepend_path_component(&wrapper_bin, &path);
        values.insert("PATH".to_string(), path);

        values.insert("TERM".to_string(), "xterm-256color".to_string());
        values.insert("TERM_PROGRAM".to_string(), app_display_name().to_string());
        values.insert(
            "TERM_PROGRAM_VERSION".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        );
        values.insert("COLORTERM".to_string(), "truecolor".to_string());
        values.insert("CODEX_COLOR".to_string(), "1".to_string());
        values.insert(
            "LANG".to_string(),
            values.get("LANG").cloned().unwrap_or_else(default_lang),
        );
        let lang = values.get("LANG").cloned().unwrap_or_else(default_lang);
        values.entry("LC_CTYPE".to_string()).or_insert(lang);

        if matches!(shell_name.as_deref(), Some("zsh")) {
            values.insert(
                "ZDOTDIR".to_string(),
                request.ai_runtime.zdotdir().display().to_string(),
            );
        }

        values.insert("DMUX_PROJECT_ID".to_string(), project_id.to_string());
        values.insert("DMUX_PROJECT_NAME".to_string(), project_name.clone());
        values.insert("DMUX_PROJECT_PATH".to_string(), project_path.to_string());
        values.insert("CODUX_PROJECT_ID".to_string(), project_id.to_string());
        values.insert("CODUX_PROJECT_NAME".to_string(), project_name.to_string());
        values.insert("CODUX_PROJECT_PATH".to_string(), project_path.to_string());
        values.insert("CODUX_TERMINAL_ID".to_string(), terminal_id.to_string());
        values.insert("CODUX_SLOT_ID".to_string(), slot_id.to_string());
        values.insert("DMUX_SESSION_ID".to_string(), terminal_id.to_string());
        values.insert("DMUX_TERMINAL_ID".to_string(), terminal_id.to_string());
        values.insert("DMUX_SLOT_ID".to_string(), slot_id.to_string());
        values.insert(
            "DMUX_SESSION_KEY".to_string(),
            request.session_key.unwrap_or("").to_string(),
        );
        values.insert("DMUX_SESSION_TITLE".to_string(), session_title.to_string());
        values.insert("DMUX_SESSION_CWD".to_string(), session_cwd.to_string());
        values.insert(
            "DMUX_SESSION_INSTANCE_ID".to_string(),
            request.process_instance_id.to_string(),
        );
        values.insert("DMUX_RUNTIME_OWNER".to_string(), app_slug().to_string());
        values.insert(
            "DMUX_RUNTIME_SOCKET".to_string(),
            request.ai_runtime.socket_path().display().to_string(),
        );
        values.insert(
            "DMUX_RUNTIME_EVENT_DIR".to_string(),
            runtime_temp_dir()
                .join("runtime-events")
                .display()
                .to_string(),
        );
        values.insert(
            "DMUX_LOG_FILE".to_string(),
            runtime_temp_dir().join("live.log").display().to_string(),
        );
        values.insert(
            "DMUX_CLAUDE_SESSION_MAP_DIR".to_string(),
            request
                .ai_runtime
                .claude_session_map_dir()
                .display()
                .to_string(),
        );
        values.insert(
            "DMUX_OPENCODE_SESSION_MAP_DIR".to_string(),
            request
                .ai_runtime
                .opencode_session_map_dir()
                .display()
                .to_string(),
        );
        values.insert("DMUX_WRAPPER_BIN".to_string(), wrapper_bin);
        values.insert(
            "CODUX_SSH_PROFILES_FILE".to_string(),
            ssh_profiles_file_path().display().to_string(),
        );
        values.insert(
            "DMUX_ZSH_HOOK_SCRIPT".to_string(),
            request.ai_runtime.hook_script().display().to_string(),
        );
        if let Some(tool_settings_file) =
            write_tool_permission_settings(&request.settings.snapshot().ai.runtime_tools)
        {
            values.insert(
                "DMUX_TOOL_PERMISSION_SETTINGS_FILE".to_string(),
                tool_settings_file.display().to_string(),
            );
        }
        values.insert(
            "DMUX_ORIGINAL_PATH".to_string(),
            values.get("PATH").cloned().unwrap_or_default(),
        );

        let ai_settings = request.settings.snapshot().ai;
        if let Some(artifacts) = request
            .memory
            .prepare_launch_artifacts(MemoryLaunchRequest {
                project_id: project_id.to_string(),
                project_name: project_name.clone(),
                workspace_path: Some(project_path.to_string()),
                settings: ai_settings,
                extra_context: render_ssh_launch_context(Some(codux_ssh_command)),
            })
        {
            values.insert(
                "DMUX_AI_MEMORY_WORKSPACE_ROOT".to_string(),
                artifacts.workspace_root,
            );
            values.insert(
                "DMUX_AI_MEMORY_PROMPT_FILE".to_string(),
                artifacts.prompt_file,
            );
            values.insert(
                "DMUX_AI_MEMORY_INDEX_FILE".to_string(),
                artifacts.index_file,
            );
        }

        if let Some(overrides) = request.overrides {
            for (key, value) in overrides {
                values.insert(key.clone(), value.clone());
            }
        }

        Self { values }
    }
}

fn write_tool_permission_settings(settings: &AIRuntimeToolSettings) -> Option<PathBuf> {
    let path = runtime_temp_dir().join("tool-permissions.json");
    let parent = path.parent()?;
    fs::create_dir_all(parent).ok()?;
    let data = serde_json::to_vec(settings).ok()?;
    fs::write(&path, data).ok()?;
    Some(path)
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

    fn len_chars(&self) -> usize {
        self.len_chars
    }
}

fn terminal_history_bytes(settings: &AppSettings, cols: u16) -> usize {
    let lines = settings
        .terminal_scrollback_lines
        .trim()
        .parse::<usize>()
        .unwrap_or(500)
        .clamp(200, 10000);
    let cols = usize::from(cols.max(20));
    lines
        .saturating_mul(cols)
        .saturating_mul(4)
        .clamp(MIN_HISTORY_BYTES, MAX_CONFIGURED_HISTORY_BYTES)
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
        if let Ok(value) = env::var(key) {
            if !value.is_empty() {
                values.entry((*key).to_string()).or_insert(value);
            }
        }
    }
}

fn configured_dotenv(home: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let allowed: std::collections::HashSet<&str> = DOTENV_KEYS.iter().copied().collect();
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
            let value = unquote_env_value(value.trim());
            values.insert(key.to_string(), value);
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
    let Some(api_key) = json
        .get("OPENAI_API_KEY")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return values;
    };

    values.insert("OPENAI_API_KEY".to_string(), api_key.to_string());
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
    let value = parse_toml_string(raw_value.trim())?;
    Some((key.to_string(), value))
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
    ]
    .iter()
    .map(|path| Path::new(home).join(path))
    .collect()
}

fn unquote_env_value(value: &str) -> String {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0];
        let last = bytes[value.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn merged_executable_path(
    shell: &str,
    home: &str,
    user: &str,
    inherited_path: Option<&str>,
) -> String {
    let default_path = inherited_path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(FALLBACK_PATH);
    let login_shell_path = resolved_login_shell_path(shell, home, user);
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
    if component.trim().is_empty() {
        return path.to_string();
    }
    if path
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
        static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let key = format!("{shell}|{home}|{user}");

        if let Ok(cache) = cache.lock() {
            if let Some(value) = cache.get(&key) {
                return value.clone();
            }
        }

        let resolved = resolve_login_shell_path_uncached(shell, home, user);
        if let Ok(mut cache) = cache.lock() {
            cache.insert(key, resolved.clone());
        }
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
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

fn home_dir() -> String {
    env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| dirs_from_user_profile())
        .unwrap_or_else(|| ".".to_string())
}

fn dirs_from_user_profile() -> Option<String> {
    #[cfg(windows)]
    {
        env::var("USERPROFILE")
            .ok()
            .filter(|value| !value.trim().is_empty())
    }

    #[cfg(not(windows))]
    {
        None
    }
}

fn user_name() -> String {
    env::var("USER")
        .or_else(|_| env::var("LOGNAME"))
        .or_else(|_| env::var("USERNAME"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "codux".to_string())
}

fn default_lang() -> String {
    "en_US.UTF-8".to_string()
}

fn clear_terminal_environment(command: &mut CommandBuilder) {
    #[cfg(not(windows))]
    command.env_clear();

    #[cfg(windows)]
    {
        let _ = command;
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

fn project_path_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
}

#[cfg(unix)]
fn build_shell_command(shell: &str, initial_command: Option<&str>) -> CommandBuilder {
    let shell_name = shell_name(shell);
    let mut command = CommandBuilder::new(shell);

    if matches!(shell_name.as_deref(), Some("zsh")) {
        if let Some(initial_command) = initial_command {
            command.args(["+o", "prompt_sp", "-i", "-l", "-c", initial_command]);
        } else {
            command.args(["+o", "prompt_sp", "-i", "-l"]);
        }
        return command;
    }

    if matches!(shell_name.as_deref(), Some("bash" | "sh")) {
        if let Some(initial_command) = initial_command {
            command.args(["-i", "-l", "-c", initial_command]);
        } else {
            command.args(["-i", "-l"]);
        }
        return command;
    }

    if let Some(initial_command) = initial_command {
        command.args(["-l", "-c", initial_command]);
    }
    command
}

#[cfg(windows)]
fn build_shell_command(shell: &str, initial_command: Option<&str>) -> CommandBuilder {
    let shell_name = shell_name(shell);
    let mut command = CommandBuilder::new(shell);

    if matches!(
        shell_name.as_deref(),
        Some("powershell.exe" | "powershell" | "pwsh.exe" | "pwsh")
    ) {
        command.args(["-NoLogo", "-NoProfile", "-NoExit"]);
        if matches!(shell_name.as_deref(), Some("powershell.exe" | "powershell")) {
            command.args(["-ExecutionPolicy", "Bypass"]);
        }
        if let Some(initial_command) = initial_command {
            command.args(["-Command", initial_command]);
        }
        return command;
    }

    if let Some(initial_command) = initial_command {
        command.arg(initial_command);
    }
    command
}

fn shell_name(shell: &str) -> Option<String> {
    Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.trim_start_matches('-').to_ascii_lowercase())
}

#[cfg(target_os = "windows")]
fn default_shell() -> String {
    windows_shell_candidates()
        .into_iter()
        .find(|path| Path::new(path).exists())
        .unwrap_or_else(|| "powershell.exe".to_string())
}

#[cfg(not(target_os = "windows"))]
fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

#[cfg(target_os = "windows")]
fn windows_shell_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Ok(program_files) = env::var("ProgramFiles") {
        candidates.push(
            Path::new(&program_files)
                .join("PowerShell")
                .join("7")
                .join("pwsh.exe")
                .display()
                .to_string(),
        );
    }
    if let Ok(system_root) = env::var("SystemRoot").or_else(|_| env::var("WINDIR")) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_decoder_keeps_split_multibyte_characters() {
        let mut pending = Vec::new();
        assert_eq!(decode_utf8_output(&[0xe6, 0x8e], &mut pending), "");
        assert_eq!(decode_utf8_output(&[0xa8], &mut pending), "推");
        assert!(pending.is_empty());
    }

    #[test]
    fn utf8_decoder_preserves_valid_prefix_before_split_character() {
        let mut pending = Vec::new();
        assert_eq!(decode_utf8_output(&[b'a', 0xe9, 0x94], &mut pending), "a");
        assert_eq!(decode_utf8_output(&[0x99], &mut pending), "错");
    }

    #[test]
    fn terminal_output_event_omits_raw_bytes_from_json() {
        let event = TerminalEvent::Output {
            session_id: "term-1".to_string(),
            text: "推".to_string(),
            bytes: "推".as_bytes().to_vec(),
        };
        let value = serde_json::to_value(event).expect("terminal event should serialize");

        assert_eq!(value["kind"], "output");
        assert_eq!(value["sessionId"], "term-1");
        assert_eq!(value["text"], "推");
        assert!(value.get("bytesBase64").is_none());
        assert!(value.get("bytes").is_none());
        assert!(value.get("data").is_none());
    }

    #[test]
    fn ring_history_tracks_character_length_without_redecoding() {
        let mut history = RingHistory::new(32);

        history.push_text("ab");
        history.push_text("推理");

        assert_eq!(history.to_text(), "ab推理");
        assert_eq!(history.len_chars(), 4);
    }

    #[test]
    fn ring_history_updates_character_length_when_trimming() {
        let mut history = RingHistory::new(6);

        history.push_text("abc");
        history.push_text("推");
        history.push_text("de");

        assert_eq!(history.to_text(), "推de");
        assert_eq!(history.len_chars(), 3);
    }

    #[test]
    fn ring_history_clear_removes_buffer() {
        let mut history = RingHistory::new(32);

        history.push_text("abc推");
        history.clear();

        assert_eq!(history.to_text(), "");
        assert_eq!(history.len_chars(), 0);
    }

    #[test]
    fn codex_auth_env_reads_openai_api_key() {
        let values = codex_auth_env_from_text(r#"{"OPENAI_API_KEY":"sk-test"}"#);

        assert_eq!(
            values.get("OPENAI_API_KEY").map(String::as_str),
            Some("sk-test")
        );
    }

    #[test]
    fn codex_auth_env_ignores_invalid_json() {
        let values = codex_auth_env_from_text("{");

        assert!(values.is_empty());
    }

    #[test]
    fn codex_config_env_reads_active_provider_base_url() {
        let values = codex_config_env_from_text(
            r#"
model_provider = "custom"

[model_providers.custom]
base_url = "https://api.example.com/openai/v1"
"#,
        );

        assert_eq!(
            values.get("OPENAI_BASE_URL").map(String::as_str),
            Some("https://api.example.com/openai/v1")
        );
    }

    #[test]
    fn codex_config_env_reads_profile_provider_base_url() {
        let values = codex_config_env_from_text(
            r#"
profile = "work"
model_provider = "openai"
openai_base_url = "https://api.openai.com/v1"

[profiles.work]
model_provider = "custom"

[model_providers.custom]
base_url = "https://proxy.example.com/v1" # inline comment
"#,
        );

        assert_eq!(
            values.get("OPENAI_BASE_URL").map(String::as_str),
            Some("https://proxy.example.com/v1")
        );
    }

    #[test]
    fn codex_config_env_reads_openai_base_url_for_builtin_provider() {
        let values = codex_config_env_from_text(
            r#"
model_provider = "openai"
openai_base_url = "https://us.api.openai.com/v1"
"#,
        );

        assert_eq!(
            values.get("OPENAI_BASE_URL").map(String::as_str),
            Some("https://us.api.openai.com/v1")
        );
    }
}
