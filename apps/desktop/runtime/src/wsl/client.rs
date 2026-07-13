#[cfg(target_os = "windows")]
use codux_runtime_core::runtime_stdio::{
    RUNTIME_STDIO_PROTOCOL_VERSION, decode_runtime_stdio_frame,
};
use codux_runtime_core::runtime_stdio::{RuntimeStdioFrame, encode_runtime_stdio_frame};
use codux_terminal_core::TerminalEvent;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
#[cfg(target_os = "windows")]
use std::io::{BufRead, BufReader};
#[cfg(target_os = "windows")]
use std::process::Stdio;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(target_os = "windows")]
const WSL_RUNTIME_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

type TerminalOutputForwarder = Box<dyn Fn(Vec<u8>) + Send + Sync>;
type TerminalEventForwarder = Box<dyn Fn(TerminalEvent) + Send + Sync>;
type PendingRequestSender = flume::Sender<Result<Value, String>>;
type PendingRequests = Mutex<HashMap<u64, PendingRequestSender>>;

pub struct WslRuntimeClient {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    next_request_id: AtomicU64,
    pending: Arc<PendingRequests>,
    terminal_outputs: Arc<Mutex<HashMap<String, TerminalOutputForwarder>>>,
    terminal_events: Arc<Mutex<HashMap<String, TerminalEventForwarder>>>,
    alive: Arc<AtomicBool>,
}

impl WslRuntimeClient {
    #[cfg(target_os = "windows")]
    pub(crate) fn start(distribution: &str) -> Result<Arc<Self>, String> {
        let mut child = super::command()
            .args([
                "--distribution",
                distribution,
                "--exec",
                "sh",
                "-lc",
                "exec /usr/local/bin/codux runtime-stdio",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Unable to start WSL runtime: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "WSL runtime stdin is unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "WSL runtime stdout is unavailable".to_string())?;
        let stderr = child.stderr.take();
        let first_line = BufReader::new(stdout);
        Self::finish_start(distribution, child, stdin, first_line, stderr)
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn start(_distribution: &str) -> Result<Arc<Self>, String> {
        Err("WSL runtimes are available on Windows only".to_string())
    }

    #[cfg(target_os = "windows")]
    fn finish_start(
        distribution: &str,
        mut child: Child,
        stdin: ChildStdin,
        stdout: BufReader<std::process::ChildStdout>,
        stderr: Option<std::process::ChildStderr>,
    ) -> Result<Arc<Self>, String> {
        let (stdout, hello_line) = match read_hello_line(stdout, WSL_RUNTIME_HANDSHAKE_TIMEOUT) {
            Ok(result) => result,
            Err(error) => {
                terminate_child(&mut child);
                return Err(error);
            }
        };
        let hello = match decode_runtime_stdio_frame(&hello_line) {
            Ok(hello) => hello,
            Err(error) => {
                terminate_child(&mut child);
                return Err(error.to_string());
            }
        };
        match hello {
            RuntimeStdioFrame::Hello {
                protocol_version, ..
            } if protocol_version == RUNTIME_STDIO_PROTOCOL_VERSION => {}
            RuntimeStdioFrame::Hello {
                protocol_version, ..
            } => {
                terminate_child(&mut child);
                return Err(format!(
                    "WSL runtime protocol mismatch: expected {}, got {protocol_version}",
                    RUNTIME_STDIO_PROTOCOL_VERSION
                ));
            }
            _ => {
                terminate_child(&mut child);
                return Err("WSL runtime did not send a hello frame".to_string());
            }
        }
        let client = Arc::new(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            next_request_id: AtomicU64::new(1),
            pending: Arc::new(Mutex::new(HashMap::new())),
            terminal_outputs: Arc::new(Mutex::new(HashMap::new())),
            terminal_events: Arc::new(Mutex::new(HashMap::new())),
            alive: Arc::new(AtomicBool::new(true)),
        });
        spawn_stdout_reader(&client, stdout);
        if let Some(stderr) = stderr {
            let distribution = distribution.to_string();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    crate::runtime_trace::runtime_trace(
                        "wsl",
                        &format!("distribution={distribution} {line}"),
                    );
                }
            });
        }
        Ok(client)
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
            && self
                .child
                .lock()
                .map(|mut child| child.try_wait().ok().flatten().is_none())
                .unwrap_or(false)
    }

    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = flume::bounded(1);
        self.pending
            .lock()
            .map_err(|_| "WSL runtime request state is unavailable".to_string())?
            .insert(id, sender);
        let write_result = self.write_request(id, method, params);
        if let Err(error) = write_result {
            remove_pending(&self.pending, id);
            return Err(error);
        }
        match receiver.recv_timeout(Duration::from_secs(30)) {
            Ok(result) => result,
            Err(_) => {
                remove_pending(&self.pending, id);
                let message = format!("WSL runtime request timed out: {method}");
                self.invalidate(&message);
                Err(message)
            }
        }
    }

    fn notify(&self, method: &str, params: Value) -> bool {
        let frame = RuntimeStdioFrame::Notify {
            method: method.to_string(),
            params,
        };
        self.write_frame(&frame).is_ok()
    }

    fn write_request(&self, id: u64, method: &str, params: Value) -> Result<(), String> {
        let frame = RuntimeStdioFrame::Request {
            id,
            method: method.to_string(),
            params,
        };
        self.write_frame(&frame)
    }

    fn write_frame(&self, frame: &RuntimeStdioFrame) -> Result<(), String> {
        let bytes = encode_runtime_stdio_frame(frame).map_err(|error| error.to_string())?;
        let result = self
            .stdin
            .lock()
            .map_err(|_| "WSL runtime stdin is unavailable".to_string())
            .and_then(|mut stdin| {
                stdin
                    .write_all(&bytes)
                    .and_then(|_| stdin.flush())
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = &result {
            self.invalidate(error);
        }
        result
    }

    fn invalidate(&self, message: &str) {
        if !fail_runtime_state(
            &self.alive,
            &self.pending,
            &self.terminal_outputs,
            &self.terminal_events,
            message,
        ) {
            return;
        }
        if let Ok(mut child) = self.child.lock() {
            terminate_child(&mut child);
        }
    }
}

#[cfg(target_os = "windows")]
fn read_hello_line<R>(mut reader: R, timeout: Duration) -> Result<(R, Vec<u8>), String>
where
    R: BufRead + Send + 'static,
{
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut line = Vec::new();
        let result = reader.read_until(b'\n', &mut line);
        let _ = sender.send((reader, line, result));
    });
    match receiver.recv_timeout(timeout) {
        Ok((_, _, Ok(0))) => Err("WSL runtime exited before sending hello".to_string()),
        Ok((reader, line, Ok(_))) => Ok((reader, line)),
        Ok((_, _, Err(error))) => Err(format!("Unable to read WSL runtime hello: {error}")),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Err("WSL runtime handshake timed out".to_string())
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("WSL runtime handshake reader exited".to_string())
        }
    }
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

impl crate::runtime_terminal::RuntimeTerminalController for WslRuntimeClient {
    fn open_terminal(
        &self,
        config: &crate::terminal_pty::TerminalPtyConfig,
    ) -> Result<String, String> {
        let params = serde_json::to_value(config).map_err(|error| error.to_string())?;
        let value = self.request("terminal.create", params)?;
        value
            .get("id")
            .or_else(|| value.get("sessionId"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "WSL runtime did not return a terminal id".to_string())
    }

    fn list_terminals(&self) -> Result<Value, String> {
        self.request("terminal.list", Value::Null)
            .map(|terminals| serde_json::json!({ "terminals": terminals }))
    }

    fn terminal_input(&self, session_id: &str, bytes: &[u8]) -> bool {
        use base64::Engine;
        self.notify(
            "terminal.input",
            serde_json::json!({
                "sessionId": session_id,
                "bytes": base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
        )
    }

    fn terminal_resize(&self, session_id: &str, cols: u16, rows: u16) -> bool {
        self.notify(
            "terminal.resize",
            serde_json::json!({ "sessionId": session_id, "cols": cols, "rows": rows }),
        )
    }

    fn close_terminal(&self, session_id: &str) -> Result<(), String> {
        self.request(
            "terminal.close",
            serde_json::json!({ "sessionId": session_id }),
        )
        .map(|_| ())
    }

    fn close_terminal_fire(&self, session_id: &str) -> bool {
        self.notify(
            "terminal.close",
            serde_json::json!({ "sessionId": session_id }),
        )
    }

    fn register_terminal_output(
        &self,
        session_id: &str,
        forwarder: crate::runtime_terminal::RuntimeTerminalOutputForwarder,
    ) {
        if let Ok(mut outputs) = self.terminal_outputs.lock() {
            outputs.insert(session_id.to_string(), forwarder);
        }
    }

    fn unregister_terminal_output(&self, session_id: &str) {
        if let Ok(mut outputs) = self.terminal_outputs.lock() {
            outputs.remove(session_id);
        }
    }

    fn register_terminal_events(
        &self,
        session_id: &str,
        forwarder: crate::runtime_terminal::RuntimeTerminalEventForwarder,
    ) {
        if let Ok(mut events) = self.terminal_events.lock() {
            events.insert(session_id.to_string(), forwarder);
        }
    }

    fn unregister_terminal_events(&self, session_id: &str) {
        if let Ok(mut events) = self.terminal_events.lock() {
            events.remove(session_id);
        }
    }
}

impl Drop for WslRuntimeClient {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        if let Ok(mut child) = self.child.lock() {
            terminate_child(&mut child);
        }
    }
}

#[cfg(target_os = "windows")]
fn spawn_stdout_reader(
    client: &Arc<WslRuntimeClient>,
    stdout: BufReader<std::process::ChildStdout>,
) {
    let pending = Arc::clone(&client.pending);
    let terminal_outputs = Arc::clone(&client.terminal_outputs);
    let terminal_events = Arc::clone(&client.terminal_events);
    let alive = Arc::clone(&client.alive);
    std::thread::spawn(move || {
        for line in stdout.split(b'\n') {
            let Ok(line) = line else { break };
            let Ok(frame) = decode_runtime_stdio_frame(&line) else {
                continue;
            };
            match frame {
                RuntimeStdioFrame::Response { id, result } => {
                    if let Some(sender) = remove_pending(&pending, id) {
                        let _ = sender.send(Ok(result));
                    }
                }
                RuntimeStdioFrame::Error {
                    id: Some(id),
                    message,
                } => {
                    if let Some(sender) = remove_pending(&pending, id) {
                        let _ = sender.send(Err(message));
                    }
                }
                RuntimeStdioFrame::Event { method, params } => {
                    if method == "terminal.output" {
                        forward_terminal_output(&terminal_outputs, &params);
                    } else if let Some(event) = terminal_event_from_stdio(&method, &params) {
                        forward_terminal_event(&terminal_events, event);
                    }
                }
                _ => {}
            }
        }
        fail_runtime_state(
            &alive,
            &pending,
            &terminal_outputs,
            &terminal_events,
            "WSL runtime exited",
        );
    });
}

#[cfg(any(target_os = "windows", test))]
fn terminal_event_from_stdio(method: &str, params: &Value) -> Option<TerminalEvent> {
    let session_id = params.get("sessionId")?.as_str()?.to_string();
    match method {
        "terminal.exit" => Some(TerminalEvent::Exit {
            session_id,
            exit_code: params
                .get("exitCode")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok()),
        }),
        "terminal.error" => Some(TerminalEvent::Error {
            session_id,
            message: params.get("message")?.as_str()?.to_string(),
        }),
        "terminal.viewport" => Some(TerminalEvent::Viewport {
            session_id,
            owner: params.get("owner")?.as_str()?.to_string(),
            cols: u16::try_from(params.get("cols")?.as_u64()?).ok()?,
            rows: u16::try_from(params.get("rows")?.as_u64()?).ok()?,
            generation: params.get("generation")?.as_u64()?,
        }),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn forward_terminal_event(
    events: &Mutex<HashMap<String, TerminalEventForwarder>>,
    event: TerminalEvent,
) {
    let session_id = match &event {
        TerminalEvent::Output { session_id, .. }
        | TerminalEvent::Exit { session_id, .. }
        | TerminalEvent::Error { session_id, .. }
        | TerminalEvent::Viewport { session_id, .. } => session_id,
    };
    if let Ok(events) = events.lock()
        && let Some(forwarder) = events.get(session_id)
    {
        forwarder(event);
    }
}

#[cfg(target_os = "windows")]
fn forward_terminal_output(
    outputs: &Mutex<HashMap<String, TerminalOutputForwarder>>,
    params: &Value,
) {
    use base64::Engine;
    let Some(session_id) = params.get("sessionId").and_then(Value::as_str) else {
        return;
    };
    let Some(bytes) = params
        .get("bytes")
        .and_then(Value::as_str)
        .and_then(|bytes| base64::engine::general_purpose::STANDARD.decode(bytes).ok())
    else {
        return;
    };
    if let Ok(outputs) = outputs.lock()
        && let Some(forwarder) = outputs.get(session_id)
    {
        forwarder(bytes);
    }
}

fn remove_pending(pending: &PendingRequests, id: u64) -> Option<PendingRequestSender> {
    pending
        .lock()
        .ok()
        .and_then(|mut pending| pending.remove(&id))
}

fn fail_pending(pending: &PendingRequests, message: &str) {
    let senders = pending
        .lock()
        .map(|mut pending| {
            pending
                .drain()
                .map(|(_, sender)| sender)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for sender in senders {
        let _ = sender.send(Err(message.to_string()));
    }
}

fn fail_terminal_events(events: &Mutex<HashMap<String, TerminalEventForwarder>>, message: &str) {
    let forwarders = events
        .lock()
        .map(|mut events| events.drain().collect::<Vec<_>>())
        .unwrap_or_default();
    for (session_id, forwarder) in forwarders {
        forwarder(TerminalEvent::Error {
            session_id,
            message: message.to_string(),
        });
    }
}

fn fail_runtime_state(
    alive: &AtomicBool,
    pending: &PendingRequests,
    terminal_outputs: &Mutex<HashMap<String, TerminalOutputForwarder>>,
    terminal_events: &Mutex<HashMap<String, TerminalEventForwarder>>,
    message: &str,
) -> bool {
    if !alive.swap(false, Ordering::AcqRel) {
        return false;
    }
    fail_pending(pending, message);
    fail_terminal_events(terminal_events, message);
    if let Ok(mut outputs) = terminal_outputs.lock() {
        outputs.clear();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(target_os = "windows")]
    #[test]
    fn reads_runtime_hello_within_deadline() {
        let reader = std::io::BufReader::new(std::io::Cursor::new(b"hello\n".to_vec()));
        let (_, line) = read_hello_line(reader, Duration::from_secs(1)).unwrap();
        assert_eq!(line, b"hello\n");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn runtime_hello_read_has_a_deadline() {
        #[derive(Debug)]
        struct DelayedReader;

        impl std::io::Read for DelayedReader {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                std::thread::sleep(Duration::from_millis(50));
                bytes[0] = b'\n';
                Ok(1)
            }
        }

        impl std::io::BufRead for DelayedReader {
            fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
                std::thread::sleep(Duration::from_millis(50));
                Ok(b"\n")
            }

            fn consume(&mut self, _amount: usize) {}
        }

        let error = read_hello_line(DelayedReader, Duration::from_millis(5)).unwrap_err();
        assert_eq!(error, "WSL runtime handshake timed out");
    }

    #[test]
    fn parses_terminal_lifecycle_events() {
        assert!(matches!(
            terminal_event_from_stdio(
                "terminal.exit",
                &json!({ "sessionId": "terminal-1", "exitCode": 7 })
            ),
            Some(TerminalEvent::Exit {
                session_id,
                exit_code: Some(7),
            }) if session_id == "terminal-1"
        ));
        assert!(matches!(
            terminal_event_from_stdio(
                "terminal.error",
                &json!({ "sessionId": "terminal-1", "message": "failed" })
            ),
            Some(TerminalEvent::Error { session_id, message })
                if session_id == "terminal-1" && message == "failed"
        ));
        assert!(matches!(
            terminal_event_from_stdio(
                "terminal.viewport",
                &json!({
                    "sessionId": "terminal-1",
                    "owner": "local",
                    "cols": 120,
                    "rows": 40,
                    "generation": 3,
                })
            ),
            Some(TerminalEvent::Viewport {
                session_id,
                owner,
                cols: 120,
                rows: 40,
                generation: 3,
            }) if session_id == "terminal-1" && owner == "local"
        ));
    }

    #[test]
    fn rejects_malformed_terminal_events() {
        assert!(terminal_event_from_stdio("terminal.exit", &json!({})).is_none());
        assert!(
            terminal_event_from_stdio(
                "terminal.viewport",
                &json!({
                    "sessionId": "terminal-1",
                    "owner": "local",
                    "cols": 100_000,
                    "rows": 40,
                    "generation": 1,
                })
            )
            .is_none()
        );
        assert!(
            terminal_event_from_stdio("terminal.unknown", &json!({ "sessionId": "terminal-1" }))
                .is_none()
        );
    }

    #[test]
    fn runtime_failure_wakes_requests_and_terminal_bindings_once() {
        let alive = AtomicBool::new(true);
        let (pending_tx, pending_rx) = flume::bounded(1);
        let pending = Mutex::new(HashMap::from([(7, pending_tx)]));
        let outputs = Mutex::new(HashMap::from([(
            "terminal-1".to_string(),
            Box::new(|_| {}) as TerminalOutputForwarder,
        )]));
        let (event_tx, event_rx) = flume::bounded(1);
        let events = Mutex::new(HashMap::from([(
            "terminal-1".to_string(),
            Box::new(move |event| {
                let _ = event_tx.send(event);
            }) as TerminalEventForwarder,
        )]));

        assert!(fail_runtime_state(
            &alive,
            &pending,
            &outputs,
            &events,
            "runtime stopped",
        ));
        assert!(!fail_runtime_state(
            &alive,
            &pending,
            &outputs,
            &events,
            "duplicate",
        ));
        assert_eq!(pending_rx.recv().unwrap().unwrap_err(), "runtime stopped");
        assert!(outputs.lock().unwrap().is_empty());
        assert!(events.lock().unwrap().is_empty());
        assert!(matches!(
            event_rx.recv().unwrap(),
            TerminalEvent::Error {
                session_id,
                message,
            } if session_id == "terminal-1" && message == "runtime stopped"
        ));
    }
}
