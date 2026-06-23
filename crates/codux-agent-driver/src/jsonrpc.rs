//! Minimal newline-delimited JSON-RPC 2.0 client over a child process' stdio.
//!
//! This is the transport `codex app-server --listen stdio://` speaks. It is
//! deliberately runtime-agnostic: a reader thread routes inbound frames, and
//! `request()` blocks on a per-call channel. The same client serves any
//! JSON-RPC-over-stdio CLI (OpenCode ACP reuses it).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};

/// A frame the server pushed to us that is *not* a response to one of our
/// requests: either a one-way notification (the event stream) or a server→client
/// request we must answer (approvals, elicitations).
pub enum Inbound {
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
}

/// id → channel that unblocks the matching `request()` call.
type PendingMap = Arc<Mutex<HashMap<i64, Sender<Result<Value, String>>>>>;

pub struct JsonRpcClient {
    stdin: Mutex<ChildStdin>,
    next_id: AtomicI64,
    pending: PendingMap,
    child: Mutex<Child>,
}

impl JsonRpcClient {
    /// Spawn `cmd` with piped stdio and start the reader/stderr threads. Returns
    /// the client plus a receiver of inbound notifications and server-requests.
    pub fn spawn(mut cmd: Command) -> std::io::Result<(Arc<Self>, Receiver<Inbound>)> {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (inbound_tx, inbound_rx) = channel();

        // Reader: route every line to the right place.
        {
            let pending = pending.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let Ok(msg) = serde_json::from_str::<Value>(line) else {
                        continue;
                    };
                    let has_id = msg.get("id").is_some();
                    let has_method = msg.get("method").is_some();
                    if has_id && has_method {
                        let _ = inbound_tx.send(Inbound::ServerRequest {
                            id: msg.get("id").cloned().unwrap_or(Value::Null),
                            method: msg["method"].as_str().unwrap_or_default().to_string(),
                            params: msg.get("params").cloned().unwrap_or(Value::Null),
                        });
                    } else if has_id {
                        if let Some(id) = msg.get("id").and_then(Value::as_i64)
                            && let Some(tx) = pending.lock().remove(&id)
                        {
                            let res = if let Some(err) = msg.get("error") {
                                Err(err.to_string())
                            } else {
                                Ok(msg.get("result").cloned().unwrap_or(Value::Null))
                            };
                            let _ = tx.send(res);
                        }
                    } else if has_method {
                        let _ = inbound_tx.send(Inbound::Notification {
                            method: msg["method"].as_str().unwrap_or_default().to_string(),
                            params: msg.get("params").cloned().unwrap_or(Value::Null),
                        });
                    }
                }
            });
        }

        // Drain stderr so the child never blocks on a full pipe.
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if !line.is_empty() {
                    eprintln!("[agent stderr] {line}");
                }
            }
        });

        let client = Arc::new(Self {
            stdin: Mutex::new(stdin),
            next_id: AtomicI64::new(0),
            pending,
            child: Mutex::new(child),
        });
        Ok((client, inbound_rx))
    }

    fn write(&self, msg: &Value) -> Result<(), String> {
        let line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        let mut stdin = self.stdin.lock();
        stdin.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
        stdin.write_all(b"\n").map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())
    }

    /// Send a request and block until the matching response arrives.
    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = channel();
        self.pending.lock().insert(id, tx);
        let mut msg = json!({ "jsonrpc": "2.0", "id": id, "method": method });
        if !params.is_null() {
            msg["params"] = params;
        }
        self.write(&msg)?;
        match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(res) => res,
            Err(_) => {
                self.pending.lock().remove(&id);
                Err(format!("request `{method}` timed out"))
            }
        }
    }

    /// Send a one-way notification (no response expected).
    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let mut msg = json!({ "jsonrpc": "2.0", "method": method });
        if !params.is_null() {
            msg["params"] = params;
        }
        self.write(&msg)
    }

    /// Answer a server→client request.
    pub fn respond(&self, id: Value, result: Value) -> Result<(), String> {
        self.write(&json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    pub fn kill(&self) {
        let _ = self.child.lock().kill();
    }
}
