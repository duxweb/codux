use codux_terminal_core::{
    TerminalDriver, TerminalEvent, TerminalEventSink, TerminalLaunchConfig, TerminalSessionHandle,
    TerminalSessionSnapshot, TerminalViewportState,
};
use parking_lot::Mutex;
use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const DEFAULT_COLS: u16 = 100;
const DEFAULT_ROWS: u16 = 32;
const DEFAULT_HISTORY_LIMIT: usize = 256 * 1024;
const LOCAL_VIEWPORT_OWNER: &str = "local";

type EventSinks = Arc<Mutex<Vec<TerminalEventSink>>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocalPtyCommandMode {
    #[default]
    Default,
    InteractiveLogin,
}

#[derive(Clone, Debug)]
pub struct LocalPtySpawnConfig {
    pub shell: String,
    pub cwd: Option<String>,
    pub initial_command: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub env: HashMap<String, String>,
    pub clear_env: bool,
    pub command_mode: LocalPtyCommandMode,
}

pub struct LocalPtyProcess {
    pub writer: Box<dyn Write + Send>,
    pub reader: Box<dyn Read + Send>,
    pub control: LocalPtyProcessHandle,
}

struct LocalPtyProcessControl {
    child: Mutex<Box<dyn Child + Send + Sync>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    pty_master: Mutex<Box<dyn MasterPty + Send>>,
    // Captured at spawn; read without locking `child` (the wait thread holds that lock for the process lifetime → deadlock).
    shell_pid: Option<u32>,
}

#[derive(Clone)]
pub struct LocalPtyProcessHandle {
    inner: Arc<LocalPtyProcessControl>,
}

impl LocalPtyProcessHandle {
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.inner
            .pty_master
            .lock()
            .resize(PtySize {
                cols: cols.max(20),
                rows: rows.max(8),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| error.to_string())
    }

    pub fn kill(&self) -> Result<(), String> {
        // Kill the whole process group (the shell is its group leader via PTY setsid) so AI descendants die too — else they hold the PTY slave open and leak the reader fd+thread until app restart.
        #[cfg(unix)]
        if let Some(pid) = self.inner.shell_pid {
            unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
        }
        self.inner
            .killer
            .lock()
            .kill()
            .map_err(|error| error.to_string())
    }

    pub fn wait_exit_code(&self) -> Option<i32> {
        self.inner
            .child
            .lock()
            .wait()
            .ok()
            .map(|status| i32::try_from(status.exit_code()).unwrap_or(i32::MAX))
    }

    /// Spawn shell PID (its descendant is the AI CLI, for hook-free tool detection); reads the spawn-time value, never locks `child`.
    pub fn process_id(&self) -> Option<u32> {
        self.inner.shell_pid
    }
}

pub fn spawn_local_pty(config: LocalPtySpawnConfig) -> Result<LocalPtyProcess, String> {
    let cols = config.cols.max(20);
    let rows = config.rows.max(8);
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("failed to open PTY: {error}"))?;
    let requested_cwd = config
        .cwd
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let cwd_is_available = requested_cwd
        .as_deref()
        .is_none_or(|cwd| path_is_directory(Path::new(cwd)));
    let initial_command = if cwd_is_available {
        config.initial_command.clone()
    } else {
        requested_cwd
            .as_deref()
            .map(|cwd| resilient_cwd_command(&config.shell, cwd, config.initial_command.as_deref()))
    };
    let mut command_builder = build_shell_command(
        &config.shell,
        initial_command.as_deref(),
        config.command_mode,
    );
    if let Some(cwd) = requested_cwd
        .as_deref()
        .filter(|_| cwd_is_available)
        .map(PathBuf::from)
        .or_else(|| fallback_spawn_cwd(&config.env))
    {
        command_builder.cwd(cwd);
    }
    if config.clear_env {
        command_builder.env_clear();
    }
    for (key, value) in &config.env {
        command_builder.env(key, value);
    }
    let child = pair
        .slave
        .spawn_command(command_builder)
        .map_err(|error| format!("failed to spawn shell {}: {error}", config.shell))?;
    let killer = child.clone_killer();
    // Capture the PID now, before the wait thread takes `child.lock()` for good.
    let shell_pid = child.process_id();
    drop(pair.slave);

    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("failed to take PTY writer: {error}"))?;
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("failed to clone PTY reader: {error}"))?;
    let control = LocalPtyProcessHandle {
        inner: Arc::new(LocalPtyProcessControl {
            child: Mutex::new(child),
            killer: Mutex::new(killer),
            pty_master: Mutex::new(pair.master),
            shell_pid,
        }),
    };

    Ok(LocalPtyProcess {
        writer,
        reader,
        control,
    })
}

#[derive(Default)]
pub struct LocalPtyDriver {
    sessions: Mutex<HashMap<String, Arc<LocalPtySession>>>,
}

impl LocalPtyDriver {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TerminalDriver for LocalPtyDriver {
    type Session = LocalPtySessionHandle;

    fn list(&self) -> Vec<TerminalSessionSnapshot> {
        self.sessions
            .lock()
            .values()
            .map(|session| LocalPtySessionHandle(Arc::clone(session)).info())
            .collect()
    }

    fn create(
        &self,
        config: TerminalLaunchConfig,
        emit: TerminalEventSink,
    ) -> Result<Self::Session, String> {
        let session = Arc::new(LocalPtySession::spawn(config, Some(emit))?);
        let handle = LocalPtySessionHandle(Arc::clone(&session));
        self.sessions
            .lock()
            .insert(session.id.clone(), Arc::clone(&session));
        Ok(handle)
    }

    fn session(&self, session_id: &str) -> Result<Self::Session, String> {
        self.sessions
            .lock()
            .get(session_id)
            .cloned()
            .map(LocalPtySessionHandle)
            .ok_or_else(|| format!("terminal session not found: {session_id}"))
    }

    fn remove(&self, session_id: &str) -> Result<(), String> {
        let Some(session) = self.sessions.lock().remove(session_id) else {
            return Err(format!("terminal session not found: {session_id}"));
        };
        LocalPtySessionHandle(session).kill()
    }

    fn subscribe_events(&self, session_id: &str, emit: TerminalEventSink) -> Result<(), String> {
        self.session(session_id)?.0.subscribe_events(emit);
        Ok(())
    }
}

pub struct LocalPtySession {
    id: String,
    stdin_writer: Mutex<Box<dyn Write + Send>>,
    history: Arc<Mutex<RingHistory>>,
    event_sinks: EventSinks,
    info: Arc<Mutex<TerminalSessionSnapshot>>,
    control: LocalPtyProcessHandle,
    viewport: Mutex<TerminalViewportState>,
}

#[derive(Clone)]
pub struct LocalPtySessionHandle(Arc<LocalPtySession>);

impl LocalPtySession {
    pub fn spawn(
        config: TerminalLaunchConfig,
        event_sink: Option<TerminalEventSink>,
    ) -> Result<Self, String> {
        let id = config
            .terminal_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let cols = config.cols.unwrap_or(DEFAULT_COLS).max(20);
        let rows = config.rows.unwrap_or(DEFAULT_ROWS).max(8);
        let shell = config.shell.clone().unwrap_or_else(default_shell);
        let cwd = config.cwd.clone().unwrap_or_else(default_cwd);
        let command = config
            .command
            .clone()
            .filter(|value| !value.trim().is_empty());

        let process = spawn_local_pty(LocalPtySpawnConfig {
            shell: shell.clone(),
            cwd: Some(cwd.clone()),
            initial_command: command.clone(),
            cols,
            rows,
            env: config.env.clone().unwrap_or_default(),
            clear_env: false,
            command_mode: LocalPtyCommandMode::Default,
        })?;
        let event_sinks = Arc::new(Mutex::new(Vec::new()));
        if let Some(event_sink) = event_sink {
            event_sinks.lock().push(event_sink);
        }
        let now = unix_timestamp_text();
        let project_name = config
            .project_name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Codux".to_string());
        let project_id = config
            .project_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| project_name.clone());
        let worktree_id = config
            .worktree_id
            .clone()
            .filter(|value| !value.trim().is_empty());
        let info = Arc::new(Mutex::new(TerminalSessionSnapshot {
            id: id.clone(),
            title: config
                .title
                .clone()
                .unwrap_or_else(|| "Terminal".to_string()),
            slot_id: config.slot_id.clone().unwrap_or_default(),
            session_key: config.session_key.clone(),
            project_id,
            worktree_id,
            project_name,
            cwd,
            shell,
            command: command.unwrap_or_default(),
            cols,
            rows,
            status: "running".to_string(),
            is_running: true,
            created_at: now.clone(),
            last_active_at: now,
            buffer_characters: 0,
            has_buffer: false,
            tool: config.tool.clone(),
        }));
        let history = Arc::new(Mutex::new(RingHistory::new(DEFAULT_HISTORY_LIMIT)));
        spawn_reader(
            id.clone(),
            process.reader,
            Arc::clone(&history),
            Arc::clone(&info),
            Arc::clone(&event_sinks),
        );
        spawn_waiter(
            id.clone(),
            process.control.clone(),
            Arc::clone(&info),
            Arc::clone(&event_sinks),
        );

        Ok(Self {
            id,
            stdin_writer: Mutex::new(process.writer),
            history,
            event_sinks,
            info,
            control: process.control,
            viewport: Mutex::new(TerminalViewportState {
                owner: LOCAL_VIEWPORT_OWNER.to_string(),
                cols,
                rows,
                generation: 0,
                owner_label: None,
            }),
        })
    }

    pub fn subscribe_events(&self, emit: TerminalEventSink) {
        self.event_sinks.lock().push(emit);
    }

    fn emit_viewport_state(&self, state: TerminalViewportState) {
        emit_event(
            &self.event_sinks,
            TerminalEvent::Viewport {
                session_id: self.id.clone(),
                owner: state.owner,
                cols: state.cols,
                rows: state.rows,
                generation: state.generation,
            },
        );
    }
}

impl TerminalSessionHandle for LocalPtySessionHandle {
    fn id(&self) -> &str {
        &self.0.id
    }

    fn info(&self) -> TerminalSessionSnapshot {
        let mut info = self.0.info.lock().clone();
        info.buffer_characters = self.buffer_characters();
        info.has_buffer = info.buffer_characters > 0;
        info
    }

    fn write(&self, data: &[u8]) -> Result<(), String> {
        let mut writer = self.0.stdin_writer.lock();
        writer
            .write_all(data)
            .and_then(|_| writer.flush())
            .map_err(|error| error.to_string())
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.resize_viewport(LOCAL_VIEWPORT_OWNER, cols, rows)
            .map(|_| ())
    }

    fn claim_viewport(&self, owner: &str) -> Result<TerminalViewportState, String> {
        let owner = normalize_owner(owner);
        let mut viewport = self.0.viewport.lock();
        if viewport.owner != owner {
            viewport.owner = owner;
            viewport.generation = viewport.generation.saturating_add(1);
        }
        Ok(viewport.clone())
    }

    fn release_viewport(&self, owner: &str) -> Result<Option<TerminalViewportState>, String> {
        let owner = normalize_owner(owner);
        let mut viewport = self.0.viewport.lock();
        if viewport.owner != owner {
            return Ok(None);
        }
        viewport.owner = LOCAL_VIEWPORT_OWNER.to_string();
        viewport.generation = viewport.generation.saturating_add(1);
        let state = viewport.clone();
        drop(viewport);
        self.0.emit_viewport_state(state.clone());
        Ok(Some(state))
    }

    fn resize_viewport(
        &self,
        owner: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Option<TerminalViewportState>, String> {
        let owner = normalize_owner(owner);
        let cols = cols.max(20);
        let rows = rows.max(8);
        let mut viewport = self.0.viewport.lock();
        if viewport.owner != owner {
            return Ok(None);
        }
        if viewport.cols == cols && viewport.rows == rows {
            return Ok(Some(viewport.clone()));
        }
        self.0.control.resize(cols, rows)?;
        {
            let mut info = self.0.info.lock();
            info.cols = cols;
            info.rows = rows;
            info.last_active_at = unix_timestamp_text();
        }
        viewport.cols = cols;
        viewport.rows = rows;
        viewport.generation = viewport.generation.saturating_add(1);
        let state = viewport.clone();
        drop(viewport);
        self.0.emit_viewport_state(state.clone());
        Ok(Some(state))
    }

    fn viewport_state(&self) -> TerminalViewportState {
        self.0.viewport.lock().clone()
    }

    fn snapshot(&self) -> String {
        self.0.history.lock().to_text()
    }

    fn snapshot_tail(&self, max_chars: usize) -> (String, usize) {
        self.0.history.lock().tail_text(max_chars)
    }

    fn buffer_characters(&self) -> usize {
        self.0.history.lock().len_chars()
    }

    fn clear_history(&self) {
        self.0.history.lock().clear();
        let mut info = self.0.info.lock();
        info.buffer_characters = 0;
        info.has_buffer = false;
        info.last_active_at = unix_timestamp_text();
    }

    fn kill(&self) -> Result<(), String> {
        self.0.control.kill()
    }
}

struct RingHistory {
    chunks: VecDeque<String>,
    len_chars: usize,
    max_chars: usize,
}

impl RingHistory {
    fn new(max_chars: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            len_chars: 0,
            max_chars,
        }
    }

    fn push(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.len_chars += text.chars().count();
        self.chunks.push_back(text);
        while self.len_chars > self.max_chars {
            let Some(front) = self.chunks.pop_front() else {
                break;
            };
            let front_len = front.chars().count();
            if self.len_chars.saturating_sub(front_len) >= self.max_chars {
                self.len_chars = self.len_chars.saturating_sub(front_len);
                continue;
            }
            let keep = self.max_chars.min(self.len_chars);
            let trimmed = tail_chars(&front, keep.saturating_sub(self.len_chars - front_len));
            self.len_chars = self.len_chars.saturating_sub(front_len) + trimmed.chars().count();
            if !trimmed.is_empty() {
                self.chunks.push_front(trimmed);
            }
            break;
        }
    }

    fn clear(&mut self) {
        self.chunks.clear();
        self.len_chars = 0;
    }

    fn len_chars(&self) -> usize {
        self.len_chars
    }

    fn to_text(&self) -> String {
        self.chunks.iter().cloned().collect()
    }

    fn tail_text(&self, max_chars: usize) -> (String, usize) {
        let text = self.to_text();
        let total = text.chars().count();
        if total <= max_chars {
            return (text, total);
        }
        (tail_chars(&text, max_chars), total)
    }
}

fn spawn_reader(
    session_id: String,
    mut reader: Box<dyn Read + Send>,
    history: Arc<Mutex<RingHistory>>,
    info: Arc<Mutex<TerminalSessionSnapshot>>,
    event_sinks: EventSinks,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let bytes = buffer[..n].to_vec();
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    history.lock().push(text.clone());
                    {
                        let mut info = info.lock();
                        info.buffer_characters = history.lock().len_chars();
                        info.has_buffer = info.buffer_characters > 0;
                        info.last_active_at = unix_timestamp_text();
                    }
                    emit_event(
                        &event_sinks,
                        TerminalEvent::Output {
                            session_id: session_id.clone(),
                            text,
                            bytes,
                        },
                    );
                }
                Err(error) => {
                    emit_event(
                        &event_sinks,
                        TerminalEvent::Error {
                            session_id: session_id.clone(),
                            message: error.to_string(),
                        },
                    );
                    break;
                }
            }
        }
    });
}

fn spawn_waiter(
    session_id: String,
    control: LocalPtyProcessHandle,
    info: Arc<Mutex<TerminalSessionSnapshot>>,
    event_sinks: EventSinks,
) {
    thread::spawn(move || {
        let exit_code = control.wait_exit_code();
        {
            let mut info = info.lock();
            info.status = "exited".to_string();
            info.is_running = false;
            info.last_active_at = unix_timestamp_text();
        }
        emit_event(
            &event_sinks,
            TerminalEvent::Exit {
                session_id,
                exit_code,
            },
        );
    });
}

fn emit_event(event_sinks: &EventSinks, event: TerminalEvent) {
    let mut sinks = event_sinks.lock();
    sinks.retain(|sink| sink(event.clone()));
}

fn build_shell_command(
    shell: &str,
    command: Option<&str>,
    mode: LocalPtyCommandMode,
) -> CommandBuilder {
    if mode == LocalPtyCommandMode::InteractiveLogin {
        return build_interactive_login_shell_command(shell, command);
    }
    let mut builder = CommandBuilder::new(shell);
    if let Some(command) = command {
        if cfg!(windows) {
            builder.args(["/C", command]);
        } else {
            builder.args(["-lc", command]);
        }
    }
    builder
}

fn path_is_directory(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

fn fallback_spawn_cwd(env: &HashMap<String, String>) -> Option<PathBuf> {
    home_from_env(env)
        .or_else(process_home_dir)
        .filter(|path| path_is_directory(path))
        .or_else(|| {
            let current = std::env::current_dir().ok()?;
            path_is_directory(&current).then_some(current)
        })
        .or_else(root_spawn_cwd)
}

fn home_from_env(env: &HashMap<String, String>) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env.get("USERPROFILE")
            .or_else(|| env.get("HOME"))
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        env.get("HOME")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
    }
}

fn process_home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn root_spawn_cwd() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("SystemDrive")
            .map(PathBuf::from)
            .filter(|path| path_is_directory(path))
    }
    #[cfg(not(windows))]
    {
        Some(PathBuf::from("/"))
    }
}

#[cfg(unix)]
fn resilient_cwd_command(shell: &str, cwd: &str, command: Option<&str>) -> String {
    let fallback_shell = format!("exec {} -i -l", shell_quote(shell));
    let mut script = format!(
        "__codux_target={}; __codux_wait_logged=0; \
         __codux_wait_count=0; \
         until cd \"$__codux_target\" 2>/dev/null; do \
         if [ \"$__codux_wait_logged\" = 0 ]; then \
         printf '\\r\\nCodux: waiting for working directory to return: %s\\r\\n' \"$__codux_target\"; \
         __codux_wait_logged=1; fi; \
         __codux_wait_count=$((__codux_wait_count + 1)); \
         if [ \"$__codux_wait_count\" -ge 30 ]; then \
         printf 'Codux: working directory did not return; starting in HOME.\\r\\n'; \
         cd \"$HOME\" 2>/dev/null || cd /; \
         unset __codux_target __codux_wait_logged __codux_wait_count; \
         {}; fi; sleep 2; done; \
         if [ \"$__codux_wait_logged\" = 1 ]; then \
         printf 'Codux: working directory is available again.\\r\\n'; fi; \
         unset __codux_target __codux_wait_logged __codux_wait_count; ",
        shell_quote(cwd),
        fallback_shell,
    );
    if let Some(command) = command.filter(|value| !value.trim().is_empty()) {
        script.push_str(command);
    } else {
        script.push_str(&fallback_shell);
    }
    script
}

#[cfg(windows)]
fn resilient_cwd_command(_shell: &str, cwd: &str, command: Option<&str>) -> String {
    let command = command.unwrap_or("powershell.exe -NoLogo -NoExit");
    format!(
        "$coduxTarget = {}; $coduxWaitCount = 0; while (-not (Test-Path -LiteralPath $coduxTarget -PathType Container)) {{ Write-Host \"Codux: waiting for working directory to return: $coduxTarget\"; Start-Sleep -Seconds 2; $coduxWaitCount += 1; if ($coduxWaitCount -ge 30) {{ Write-Host \"Codux: working directory did not return; starting in HOME.\"; Set-Location -LiteralPath $HOME; powershell.exe -NoLogo -NoExit; exit }} }}; Set-Location -LiteralPath $coduxTarget; {command}",
        powershell_quote(cwd),
    )
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        "''".to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(windows)]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(unix)]
fn build_interactive_login_shell_command(shell: &str, command: Option<&str>) -> CommandBuilder {
    let shell_name = shell_name(shell);
    let mut builder = CommandBuilder::new(shell);
    if is_zsh_shell_name(shell_name.as_deref()) {
        if let Some(command) = command {
            builder.args(["+o", "prompt_sp", "-i", "-l", "-c", command]);
        } else {
            builder.args(["+o", "prompt_sp", "-i", "-l"]);
        }
        return builder;
    }
    if matches!(shell_name.as_deref(), Some("bash" | "sh")) {
        if let Some(command) = command {
            builder.args(["-i", "-l", "-c", command]);
        } else {
            builder.args(["-i", "-l"]);
        }
        return builder;
    }
    if let Some(command) = command {
        builder.args(["-l", "-c", command]);
    }
    builder
}

#[cfg(windows)]
fn build_interactive_login_shell_command(shell: &str, command: Option<&str>) -> CommandBuilder {
    let shell_name = shell_name(shell);
    let mut builder = CommandBuilder::new(shell);
    if matches!(
        shell_name.as_deref(),
        Some("powershell.exe" | "powershell" | "pwsh.exe" | "pwsh")
    ) {
        builder.args(["-NoLogo", "-NoProfile", "-NoExit"]);
        builder.args(["-ExecutionPolicy", "Bypass"]);
        // -NoProfile skips user profiles, so the Codux shell integration
        // (OSC 133 marks) is dot-sourced explicitly when staged.
        let hook_prefix = "if ($env:DMUX_PS_HOOK_SCRIPT -and (Test-Path -LiteralPath $env:DMUX_PS_HOOK_SCRIPT)) { . $env:DMUX_PS_HOOK_SCRIPT }; ";
        let command = match command {
            Some(command) => format!("{hook_prefix}{command}"),
            None => hook_prefix.to_string(),
        };
        builder.args(["-Command", &command]);
        return builder;
    }
    if let Some(command) = command {
        builder.arg(command);
    }
    builder
}

fn shell_name(shell: &str) -> Option<String> {
    let file_name = PathBuf::from(shell)
        .file_name()?
        .to_string_lossy()
        .to_string();
    if file_name.is_empty() {
        None
    } else {
        Some(file_name.to_ascii_lowercase())
    }
}

fn is_zsh_shell_name(name: Option<&str>) -> bool {
    let Some(name) = name else {
        return false;
    };
    let Some(rest) = name.trim_start_matches('-').strip_prefix("zsh") else {
        return false;
    };
    rest.is_empty()
        || rest
            .chars()
            .next()
            .is_some_and(|ch| !ch.is_ascii_alphanumeric())
}

fn default_shell() -> String {
    if cfg!(windows) {
        "powershell.exe".to_string()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

fn default_cwd() -> String {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .display()
        .to_string()
}

fn normalize_owner(owner: &str) -> String {
    let owner = owner.trim();
    if owner.is_empty() {
        LOCAL_VIEWPORT_OWNER.to_string()
    } else {
        owner.to_string()
    }
}

fn tail_chars(value: &str, max_chars: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max_chars {
        return value.to_string();
    }
    chars[chars.len() - max_chars..].iter().collect()
}

fn unix_timestamp_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_history_trims_on_character_boundaries() {
        let mut history = RingHistory::new(4);
        history.push("a你好".to_string());
        history.push("bc".to_string());

        assert_eq!(history.to_text(), "你好bc");
        assert_eq!(history.len_chars(), 4);
    }

    #[test]
    fn local_driver_captures_command_output() {
        let driver = LocalPtyDriver::new();
        let session = driver
            .create(
                TerminalLaunchConfig {
                    command: Some("printf codux-pty-test".to_string()),
                    ..Default::default()
                },
                Box::new(|_| true),
            )
            .expect("create local pty session");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            if session.snapshot().contains("codux-pty-test") {
                let _ = session.kill();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let snapshot = session.snapshot();
        let _ = session.kill();
        assert!(
            snapshot.contains("codux-pty-test"),
            "snapshot did not contain command output: {snapshot:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn spawn_waits_for_missing_cwd_to_return() {
        let cwd = std::env::temp_dir().join(format!("codux-pty-cwd-{}", Uuid::new_v4()));
        let cwd_text = cwd.display().to_string();
        let mkdir_path = cwd.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            fs::create_dir_all(mkdir_path).expect("create delayed cwd");
        });

        let mut process = spawn_local_pty(LocalPtySpawnConfig {
            shell: "/bin/sh".to_string(),
            cwd: Some(cwd_text.clone()),
            initial_command: Some("pwd; printf codux-recovered; exit".to_string()),
            cols: 80,
            rows: 24,
            env: HashMap::from([(
                "HOME".to_string(),
                std::env::temp_dir().display().to_string(),
            )]),
            clear_env: false,
            command_mode: LocalPtyCommandMode::Default,
        })
        .expect("spawn pty while cwd is missing");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut output = String::new();
        let mut buffer = [0; 512];
        while std::time::Instant::now() < deadline {
            match process.reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    output.push_str(&String::from_utf8_lossy(&buffer[..count]));
                    if output.contains("codux-recovered") {
                        let _ = process.control.kill();
                        let _ = fs::remove_dir_all(cwd);
                        assert!(output.contains(&cwd_text), "output was {output:?}");
                        return;
                    }
                }
                Err(_) => break,
            }
        }

        let _ = process.control.kill();
        let _ = fs::remove_dir_all(cwd);
        panic!("PTY did not recover after cwd returned; output was {output:?}");
    }

    #[cfg(unix)]
    #[test]
    fn resilient_cwd_command_has_home_fallback() {
        let script = resilient_cwd_command("/bin/sh", "/Volumes/Missing/project", None);

        assert!(script.contains("__codux_wait_count"));
        assert!(script.contains("-ge 30"));
        assert!(script.contains("starting in HOME"));
        assert!(script.contains("exec '/bin/sh' -i -l"));
    }

    // Regression: `process_id()` must not lock `child` — the wait thread holds that lock for the process lifetime, so doing so deadlocks.
    #[cfg(unix)]
    #[test]
    fn process_id_does_not_deadlock_while_wait_thread_holds_child_lock() {
        let process = spawn_local_pty(LocalPtySpawnConfig {
            shell: "/bin/sh".to_string(),
            cwd: None,
            initial_command: Some("sleep 30".to_string()),
            cols: 80,
            rows: 24,
            env: HashMap::new(),
            clear_env: false,
            command_mode: LocalPtyCommandMode::Default,
        })
        .expect("spawn pty");
        let control = process.control.clone();

        let waiter = control.clone();
        std::thread::spawn(move || {
            let _ = waiter.wait_exit_code();
        });
        std::thread::sleep(std::time::Duration::from_millis(200));

        let (tx, rx) = std::sync::mpsc::channel();
        let probe = control.clone();
        std::thread::spawn(move || {
            let _ = tx.send(probe.process_id());
        });
        let pid = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("process_id() deadlocked while the wait thread held the child lock");
        assert!(pid.is_some(), "process_id() should report the shell pid");

        let _ = control.kill();
    }

    #[cfg(unix)]
    #[test]
    fn interactive_login_command_mode_matches_desktop_zsh_args() {
        let command = build_shell_command(
            "/bin/zsh",
            Some("printf codux"),
            LocalPtyCommandMode::InteractiveLogin,
        );
        let argv: Vec<_> = command
            .get_argv()
            .iter()
            .map(|value| value.to_string_lossy().to_string())
            .collect();

        assert_eq!(
            argv,
            vec![
                "/bin/zsh".to_string(),
                "+o".to_string(),
                "prompt_sp".to_string(),
                "-i".to_string(),
                "-l".to_string(),
                "-c".to_string(),
                "printf codux".to_string(),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn interactive_login_command_mode_treats_named_zsh_wrapper_as_zsh() {
        let command = build_shell_command(
            "/Users/example/.local/bin/zsh (kiro-cli-term)",
            Some("printf codux"),
            LocalPtyCommandMode::InteractiveLogin,
        );
        let argv: Vec<_> = command
            .get_argv()
            .iter()
            .map(|value| value.to_string_lossy().to_string())
            .collect();

        assert_eq!(
            argv,
            vec![
                "/Users/example/.local/bin/zsh (kiro-cli-term)".to_string(),
                "+o".to_string(),
                "prompt_sp".to_string(),
                "-i".to_string(),
                "-l".to_string(),
                "-c".to_string(),
                "printf codux".to_string(),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn interactive_login_command_mode_matches_desktop_bash_args() {
        let command = build_shell_command(
            "/bin/bash",
            Some("printf codux"),
            LocalPtyCommandMode::InteractiveLogin,
        );
        let argv: Vec<_> = command
            .get_argv()
            .iter()
            .map(|value| value.to_string_lossy().to_string())
            .collect();

        assert_eq!(
            argv,
            vec![
                "/bin/bash".to_string(),
                "-i".to_string(),
                "-l".to_string(),
                "-c".to_string(),
                "printf codux".to_string(),
            ]
        );
    }
}
