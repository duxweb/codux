//! Desktop-as-controller runtime: dial OUT to a remote host (another desktop's
//! `RemoteHostRuntime` or a headless agent) and drive its domains over Iroh.
//! This is the inverse of `RemoteHostRuntime`.
//!
//! Replies are correlated by message **type** — the host echoes a domain
//! specific reply type, not a `requestId`, for general domains — using a FIFO
//! waiter list, the same proven scheme the mobile controller and the agent use.
//! The request path is synchronous (send, then block on a channel that the
//! transport's message callback feeds) so it composes cleanly with the
//! synchronous `RuntimeService` domain methods that will route through it.

use base64::Engine;
use codux_protocol::{
    REMOTE_AI_SESSION, REMOTE_AI_SESSION_RESULT, REMOTE_AI_STATE, REMOTE_AI_STATS, REMOTE_ERROR,
    REMOTE_FILE_BLOB, REMOTE_FILE_BYTES_WRITTEN, REMOTE_FILE_COPIED, REMOTE_FILE_COPY,
    REMOTE_FILE_CREATE_DIRECTORY, REMOTE_FILE_DELETE, REMOTE_FILE_DELETED,
    REMOTE_FILE_DIRECTORY_CREATED, REMOTE_FILE_LIST, REMOTE_FILE_MOVE, REMOTE_FILE_MOVED,
    REMOTE_FILE_READ, REMOTE_FILE_READ_BLOB, REMOTE_FILE_RENAME, REMOTE_FILE_RENAMED,
    REMOTE_FILE_WRITE, REMOTE_FILE_WRITE_BLOB, REMOTE_FILE_WRITTEN, REMOTE_GIT_INVOKE,
    REMOTE_GIT_READ, REMOTE_GIT_STATUS, REMOTE_HOST_INFO, REMOTE_HOST_METRICS,
    REMOTE_MEMORY_EXTRACT, REMOTE_MEMORY_READ, REMOTE_MEMORY_RESULT, REMOTE_PAIRING_CONFIRMED,
    REMOTE_PAIRING_REJECTED, REMOTE_PAIRING_REQUEST, REMOTE_PROJECT_LIST, REMOTE_TERMINAL_BUFFER,
    REMOTE_TERMINAL_BUFFER_MAX_CHARS, REMOTE_TERMINAL_CLOSE, REMOTE_TERMINAL_CLOSED,
    REMOTE_TERMINAL_CREATE, REMOTE_TERMINAL_CREATED, REMOTE_TERMINAL_INPUT, REMOTE_TERMINAL_OUTPUT,
    REMOTE_TERMINAL_VIEWPORT_RESIZE, REMOTE_TRANSPORT_IROH, REMOTE_WORKTREE_CREATE,
    REMOTE_WORKTREE_LIST, REMOTE_WORKTREE_MERGE, REMOTE_WORKTREE_REMOVE, REMOTE_WORKTREE_UPDATED,
    RemoteHostMetrics,
};
use codux_remote_transport::{
    RemoteControllerTransportConfig, RemoteTransport, RemoteTransportCandidate,
    RemoteTransportStateHandler, WebTunnelIoStream, WebTunnelTcpConnectRequest,
};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::controller_store::{SavedRemoteHost, SavedRemoteTransport};
use super::transport_factory::RemoteTransportFactory;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// Memory reads run inside the project-switch load on the single blocking
/// worker. A host that doesn't implement memory (e.g. a desktop host) never
/// replies, so the default 20s wait would freeze that worker — starving every
/// other project's terminal load. Bound it tightly: memory is non-critical for
/// a switch, fall back to local/empty fast.
const MEMORY_READ_TIMEOUT: Duration = Duration::from_secs(3);
/// The host operator must accept the pairing; allow a generous window.
const PAIRING_TIMEOUT: Duration = Duration::from_secs(90);

/// Everything needed to dial a remote host — produced from a pairing ticket.
#[derive(Clone, Debug)]
pub struct RemoteControllerTarget {
    pub host_id: String,
    pub device_id: String,
    pub device_token: String,
    pub relay_url: String,
    pub node_id: String,
    pub ticket: String,
    pub relay_authentication: String,
}

/// A pairing ticket pasted by the user: the host emits a `codux://pair?payload=`
/// URL whose base64url payload is `{code, secret, pairingId, transports[]}`.
/// Each iroh transport carries either a full `ticket`, or a `node_id` +
/// `relay_url` pair (the slim QR form) — either is enough to dial.
#[derive(Clone, Debug)]
pub struct PairingTicket {
    pub code: String,
    pub secret: String,
    pub pairing_id: String,
    pub transports: Vec<TicketTransport>,
}

#[derive(Clone, Debug)]
pub struct TicketTransport {
    pub kind: String,
    pub ticket: String,
    pub node_id: String,
    pub relay_url: String,
    pub relay_authentication: String,
}

impl TicketTransport {
    /// Whether this transport carries enough to dial: a full ticket, or a node
    /// id + relay url pair (the slim QR form).
    fn is_dialable_iroh(&self) -> bool {
        self.kind == REMOTE_TRANSPORT_IROH
            && (!self.ticket.is_empty() || (!self.node_id.is_empty() && !self.relay_url.is_empty()))
    }
}

/// A directory listing on a remote host, parsed from the `file.list` payload so
/// the UI never has to touch the wire JSON.
#[derive(Clone, Debug, Default)]
pub struct RemoteDirectoryListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<RemoteDirectoryEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct RemoteDirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

/// Parse a pasted `codux://pair?payload=<base64url>` ticket (or a bare base64url
/// payload) into its fields.
pub fn parse_pairing_ticket(input: &str) -> Result<PairingTicket, String> {
    let trimmed = input.trim();
    let encoded = if let Some(rest) = trimmed.strip_prefix("codux://pair") {
        url::Url::parse(&format!("codux://pair{rest}"))
            .map_err(|error| error.to_string())?
            .query_pairs()
            .find(|(key, _)| key == "payload")
            .map(|(_, value)| value.into_owned())
            .ok_or_else(|| "Pairing ticket is missing its payload.".to_string())?
    } else {
        trimmed.to_string()
    };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .map_err(|error| format!("Pairing ticket is not valid base64url: {error}"))?;
    let payload: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Pairing ticket is not valid: {error}"))?;

    let field = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let transports = payload
        .get("transports")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| TicketTransport {
                    kind: item
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or(REMOTE_TRANSPORT_IROH)
                        .to_string(),
                    ticket: item
                        .get("ticket")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    node_id: item
                        .get("nodeId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    relay_url: item
                        .get("relayUrl")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    relay_authentication: item
                        .get("relayAuthentication")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let ticket = PairingTicket {
        code: field("code"),
        secret: field("secret"),
        pairing_id: field("pairingId"),
        transports,
    };
    if ticket.code.is_empty() || ticket.secret.is_empty() || ticket.pairing_id.is_empty() {
        return Err("Pairing ticket is missing its code, secret, or pairing id.".to_string());
    }
    if !ticket
        .transports
        .iter()
        .any(TicketTransport::is_dialable_iroh)
    {
        return Err("Pairing ticket has no usable iroh transport.".to_string());
    }
    Ok(ticket)
}

pub(super) fn new_device_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

struct Waiter {
    id: u64,
    expect: String,
    tx: Sender<Result<Value, String>>,
}

type TerminalSink = Box<dyn Fn(Value) + Send + Sync>;
type TerminalOutputForwarder = Box<dyn Fn(Vec<u8>) + Send + Sync>;

#[derive(Default)]
struct ControllerInner {
    waiters: Mutex<Vec<Waiter>>,
    events: Mutex<VecDeque<(String, Value)>>,
    // Raw full-envelope sink (used by tests that drive a RemoteOutputRouter).
    terminal_sink: Mutex<Option<TerminalSink>>,
    // Per-session byte forwarders for terminal.output — the desktop terminal UI
    // registers one per remote session and the model parses the bytes itself.
    terminal_outputs: Mutex<HashMap<String, TerminalOutputForwarder>>,
}

impl ControllerInner {
    /// Route one inbound envelope: resolve the first waiter expecting this reply
    /// type, fail the oldest waiter on `error`, or queue it as an unsolicited
    /// event (resource update, terminal output, broadcast).
    fn route(&self, data: &[u8]) {
        let Ok(envelope) = serde_json::from_slice::<Value>(data) else {
            return;
        };
        let kind = envelope
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let payload = envelope.get("payload").cloned().unwrap_or(Value::Null);
        // Terminal output never goes through the waiter/event path. A raw sink
        // (tests) gets the full envelope; otherwise demux by sessionId to the
        // per-session byte forwarder (the desktop terminal UI).
        if kind == REMOTE_TERMINAL_OUTPUT {
            if let Ok(sink) = self.terminal_sink.lock() {
                if let Some(sink) = sink.as_ref() {
                    sink(envelope);
                    return;
                }
            }
            let session_id = envelope.get("sessionId").and_then(Value::as_str);
            let data = payload.get("data").and_then(Value::as_str).unwrap_or("");
            // Feed ONLY the live byte stream into the emulator — exactly like a
            // local PTY relay. The host also emits full-screen `screenData`
            // keyframes (on attach and on every resize), but our desktop emulator
            // keeps its OWN scrollback built from this byte stream, so replaying a
            // keyframe on top of it DUPLICATES the screen into scrollback — and a
            // window drag fires many resize keyframes, so the content piled up
            // many times over. Resize is handled by the shell's own repaint in the
            // live stream, just as it is for a local terminal. (The keyframe stays
            // in the protocol for grid-reconciling clients; this emulator isn't
            // one — see the tmux-style notes in the remote terminal design.)
            let bytes = (!data.is_empty()).then(|| data.as_bytes().to_vec());
            if let (Some(session_id), Some(bytes)) = (session_id, bytes) {
                if let Ok(forwarders) = self.terminal_outputs.lock() {
                    if let Some(forwarder) = forwarders.get(session_id) {
                        forwarder(bytes);
                    }
                }
            }
            return;
        }
        {
            let mut waiters = self.waiters.lock().unwrap();
            if kind == REMOTE_ERROR {
                if !waiters.is_empty() {
                    let waiter = waiters.remove(0);
                    let message = payload
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("remote host error")
                        .to_string();
                    let _ = waiter.tx.send(Err(message));
                    return;
                }
            } else if let Some(index) = waiters.iter().position(|waiter| waiter.expect == kind) {
                let waiter = waiters.remove(index);
                let _ = waiter.tx.send(Ok(payload));
                return;
            }
        }
        self.events.lock().unwrap().push_back((kind, payload));
    }

    fn remove_waiter(&self, id: u64) {
        self.waiters
            .lock()
            .unwrap()
            .retain(|waiter| waiter.id != id);
    }
}

pub struct RemoteController {
    transport: Arc<dyn RemoteTransport>,
    device_id: String,
    inner: Arc<ControllerInner>,
    next_id: AtomicU64,
}

impl RemoteController {
    pub async fn connect(
        target: &RemoteControllerTarget,
        on_state: RemoteTransportStateHandler,
    ) -> Result<Self, String> {
        let inner = Arc::new(ControllerInner::default());
        let config = RemoteControllerTransportConfig {
            relay_url: target.relay_url.clone(),
            host_id: target.host_id.clone(),
            device_id: target.device_id.clone(),
            device_token: target.device_token.clone(),
            transports: vec![RemoteTransportCandidate {
                kind: REMOTE_TRANSPORT_IROH.to_string(),
                url: String::new(),
                node_id: target.node_id.clone(),
                relay_url: target.relay_url.clone(),
                ticket: target.ticket.clone(),
                relay_authentication: target.relay_authentication.clone(),
            }],
        };
        let message_inner = Arc::clone(&inner);
        let transport = RemoteTransportFactory::connect_controller(
            &config,
            Arc::new(move |_source: String, data: Vec<u8>| message_inner.route(&data)),
            on_state,
        )
        .await?;
        Ok(Self {
            transport,
            device_id: target.device_id.clone(),
            inner,
            next_id: AtomicU64::new(1),
        })
    }

    /// Reconnect to a previously paired host (no fresh handshake — the host
    /// caches our `device_id`). Uses the saved iroh `node_id` + `relay_url`,
    /// since the original pairing ticket is single-use.
    pub async fn connect_saved(
        host: &SavedRemoteHost,
        on_state: RemoteTransportStateHandler,
    ) -> Result<Self, String> {
        let iroh = host
            .transports
            .iter()
            .find(|transport| transport.kind == REMOTE_TRANSPORT_IROH)
            .ok_or_else(|| "Saved host has no iroh transport.".to_string())?;
        Self::connect(
            &RemoteControllerTarget {
                host_id: host.host_id.clone(),
                device_id: host.device_id.clone(),
                device_token: host.device_token.clone(),
                relay_url: iroh.relay_url.clone(),
                node_id: iroh.node_id.clone(),
                ticket: String::new(),
                relay_authentication: iroh.relay_authentication.clone(),
            },
            on_state,
        )
        .await
    }

    /// Drive the pairing handshake against a host that has an active pairing:
    /// connect unpaired (self-minted device id, empty token, ticket-only iroh
    /// candidate), send `pairing.request`, and wait for the operator to confirm.
    /// On success returns the live controller plus the persistable host record.
    pub async fn pair(
        ticket: &PairingTicket,
        device_name: &str,
        device_id: String,
        on_state: RemoteTransportStateHandler,
    ) -> Result<(Self, SavedRemoteHost), String> {
        let iroh = ticket
            .transports
            .iter()
            .find(|transport| transport.is_dialable_iroh())
            .ok_or_else(|| "Pairing ticket has no usable iroh transport.".to_string())?;
        let controller = Self::connect(
            &RemoteControllerTarget {
                host_id: String::new(),
                device_id: device_id.clone(),
                device_token: String::new(),
                // Carry whichever the ticket provided — a full ticket, or a
                // node id + relay url pair (slim QR). `connect` hands all of them
                // to the transport candidate, which dials with the ticket if
                // present and otherwise from node id + relay url.
                relay_url: iroh.relay_url.clone(),
                node_id: iroh.node_id.clone(),
                ticket: iroh.ticket.clone(),
                relay_authentication: iroh.relay_authentication.clone(),
            },
            on_state,
        )
        .await?;

        let request = json!({
            "type": REMOTE_PAIRING_REQUEST,
            "deviceId": device_id,
            "payload": {
                "pairingId": ticket.pairing_id,
                "code": ticket.code,
                "secret": ticket.secret,
                "deviceName": device_name,
                "deviceId": device_id,
                "platform": std::env::consts::OS,
            },
        });
        let bytes = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
        if !controller.transport.send(bytes, None) {
            controller.shutdown().await;
            return Err("Failed to send the pairing request to the host.".to_string());
        }

        let deadline = Instant::now() + PAIRING_TIMEOUT;
        loop {
            for (kind, payload) in controller.drain_events() {
                if kind == REMOTE_PAIRING_CONFIRMED {
                    let saved = saved_host_from_confirmed(&device_id, &payload);
                    return Ok((controller, saved));
                }
                if kind == REMOTE_PAIRING_REJECTED {
                    controller.shutdown().await;
                    return Err("The host rejected the pairing request.".to_string());
                }
            }
            if Instant::now() >= deadline {
                controller.shutdown().await;
                return Err("Timed out waiting for the host to confirm pairing.".to_string());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Send a request and block until a reply of type `expect` arrives (or the
    /// host returns an error, or the request times out).
    pub fn request(&self, expect: &str, kind: &str, payload: Value) -> Result<Value, String> {
        self.request_with_timeout(expect, kind, payload, DEFAULT_REQUEST_TIMEOUT)
    }

    pub fn request_with_timeout(
        &self,
        expect: &str,
        kind: &str,
        payload: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.inner.waiters.lock().unwrap().push(Waiter {
            id,
            expect: expect.to_string(),
            tx,
        });
        let envelope = json!({ "type": kind, "deviceId": self.device_id, "payload": payload });
        let bytes = match serde_json::to_vec(&envelope) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.inner.remove_waiter(id);
                return Err(error.to_string());
            }
        };
        if !self.transport.send(bytes, None) {
            self.inner.remove_waiter(id);
            return Err(format!("failed to send {kind} to remote host"));
        }
        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(_) => {
                self.inner.remove_waiter(id);
                Err(format!("timed out waiting for {expect} from remote host"))
            }
        }
    }

    /// Drain unsolicited messages (resource updates, terminal output, broadcasts).
    pub fn drain_events(&self) -> Vec<(String, Value)> {
        self.inner.events.lock().unwrap().drain(..).collect()
    }

    /// Drain unsolicited `ai.stats` payloads the host pushed (live runtime
    /// updates), leaving any other queued events in place. Returns them oldest
    /// first; callers apply the latest.
    pub fn drain_pushed_ai_stats(&self) -> Vec<Value> {
        let mut events = self.inner.events.lock().unwrap();
        let mut payloads = Vec::new();
        events.retain(|(kind, payload)| {
            if kind == REMOTE_AI_STATS {
                payloads.push(payload.clone());
                false
            } else {
                true
            }
        });
        payloads
    }

    pub async fn shutdown(&self) {
        self.transport.shutdown().await;
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn web_tunnel_tcp_connect(
        &self,
        request: WebTunnelTcpConnectRequest,
    ) -> Result<Box<dyn WebTunnelIoStream>, String> {
        crate::async_runtime::block_on(self.transport.web_tunnel_tcp_connect(request))
    }

    // ---- Typed domain helpers -----------------------------------------------

    pub fn host_info(&self) -> Result<Value, String> {
        self.request(REMOTE_HOST_INFO, REMOTE_HOST_INFO, json!({}))
    }

    pub fn host_metrics(&self) -> Result<RemoteHostMetrics, String> {
        let value = self.request(REMOTE_HOST_METRICS, REMOTE_HOST_METRICS, json!({}))?;
        serde_json::from_value(value).map_err(|error| error.to_string())
    }

    pub fn file_list(&self, path: Option<&str>, purpose: Option<&str>) -> Result<Value, String> {
        let mut payload = json!({});
        if let Some(path) = path {
            payload["path"] = json!(path);
        }
        if let Some(purpose) = purpose {
            payload["purpose"] = json!(purpose);
        }
        self.request(REMOTE_FILE_LIST, REMOTE_FILE_LIST, payload)
    }

    /// List a remote directory and parse it into a typed listing.
    pub fn browse_directory(
        &self,
        path: Option<&str>,
        purpose: Option<&str>,
    ) -> Result<RemoteDirectoryListing, String> {
        let value = self.file_list(path, purpose.or(Some("projectFiles")))?;
        Ok(RemoteDirectoryListing {
            path: value
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            parent: value
                .get("parent")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            entries: value
                .get("entries")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .map(|entry| RemoteDirectoryEntry {
                            name: entry
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            path: entry
                                .get("path")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            is_dir: entry
                                .get("isDirectory")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub fn read_file(&self, path: &str) -> Result<Value, String> {
        self.request(REMOTE_FILE_READ, REMOTE_FILE_READ, json!({ "path": path }))
    }

    /// Read a file's bytes binary-safely: ask the host to publish them to its
    /// blob store, then fetch the blob over iroh-blobs (content-addressed, the
    /// same path the terminal upload uses). For cross-device file copy.
    pub fn read_file_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        let reply = self.request(
            REMOTE_FILE_BLOB,
            REMOTE_FILE_READ_BLOB,
            json!({ "path": path }),
        )?;
        if let Some(error) = reply.get("error").and_then(Value::as_str) {
            return Err(error.to_string());
        }
        let ticket = reply
            .get("ticket")
            .and_then(Value::as_str)
            .ok_or_else(|| "host did not return a blob ticket".to_string())?
            .to_string();
        let transport = self.transport.clone();
        crate::async_runtime::block_on(async move { transport.fetch_blob(&ticket).await })
    }

    pub fn create_directory(&self, path: &str) -> Result<Value, String> {
        self.request(
            REMOTE_FILE_DIRECTORY_CREATED,
            REMOTE_FILE_CREATE_DIRECTORY,
            json!({ "path": path }),
        )
    }

    fn reply_path(value: Value) -> String {
        value
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    /// Copy a file/dir into `target_dir` on the host; returns the new path.
    pub fn copy_path(&self, path: &str, target_dir: &str) -> Result<String, String> {
        self.request(
            REMOTE_FILE_COPIED,
            REMOTE_FILE_COPY,
            json!({ "path": path, "targetDir": target_dir }),
        )
        .map(Self::reply_path)
    }

    pub fn move_path(
        &self,
        path: &str,
        target_dir: &str,
        overwrite: bool,
    ) -> Result<String, String> {
        self.request(
            REMOTE_FILE_MOVED,
            REMOTE_FILE_MOVE,
            json!({ "path": path, "targetDir": target_dir, "overwrite": overwrite }),
        )
        .map(Self::reply_path)
    }

    /// Write raw bytes as `name` in `directory` on the host. Binary-safe over
    /// iroh-blobs: publish the bytes, then ask the host to fetch + write them
    /// (the same content-addressed path as the file read and terminal upload).
    pub fn write_bytes(&self, directory: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
        let transport = self.transport.clone();
        let bytes = bytes.to_vec();
        let ticket =
            crate::async_runtime::block_on(async move { transport.publish_blob(bytes).await })?;
        self.request(
            REMOTE_FILE_BYTES_WRITTEN,
            REMOTE_FILE_WRITE_BLOB,
            json!({ "directory": directory, "name": name, "ticket": ticket }),
        )
        .map(Self::reply_path)
    }

    pub fn write_file(&self, path: &str, content: &str) -> Result<Value, String> {
        self.request(
            REMOTE_FILE_WRITTEN,
            REMOTE_FILE_WRITE,
            json!({ "path": path, "content": content }),
        )
    }

    pub fn delete_path(&self, path: &str) -> Result<Value, String> {
        self.request(
            REMOTE_FILE_DELETED,
            REMOTE_FILE_DELETE,
            json!({ "path": path }),
        )
    }

    pub fn rename_path(&self, path: &str, new_path: &str) -> Result<Value, String> {
        self.request(
            REMOTE_FILE_RENAMED,
            REMOTE_FILE_RENAME,
            json!({ "path": path, "newPath": new_path }),
        )
    }

    pub fn git_status(&self, project_id: &str, project_path: &str) -> Result<Value, String> {
        self.request(
            REMOTE_GIT_STATUS,
            REMOTE_GIT_STATUS,
            json!({ "projectId": project_id, "projectPath": project_path }),
        )
    }

    /// Generic git mutation — replies with the refreshed `git.status` payload.
    pub fn git_invoke(&self, op: &str, project_path: &str, args: Value) -> Result<Value, String> {
        self.request(
            REMOTE_GIT_STATUS,
            REMOTE_GIT_INVOKE,
            json!({ "projectPath": project_path, "op": op, "args": args }),
        )
    }

    /// Generic git read — returns the `result` payload for `op`.
    pub fn git_read(&self, op: &str, project_path: &str, args: Value) -> Result<Value, String> {
        let reply = self.request(
            REMOTE_GIT_READ,
            REMOTE_GIT_READ,
            json!({ "projectPath": project_path, "op": op, "args": args }),
        )?;
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn ai_stats(&self, project_id: &str) -> Result<Value, String> {
        self.request(
            REMOTE_AI_STATS,
            REMOTE_AI_STATS,
            json!({ "projectId": project_id }),
        )
    }

    /// Full `AIHistoryProjectState` for a hosted project (the desktop AI panel's
    /// shape), indexed on the host from the project path we send.
    pub fn ai_state(
        &self,
        project_id: &str,
        project_name: &str,
        project_path: &str,
    ) -> Result<codux_ai_history::indexer::AIHistoryProjectState, String> {
        let value = self.request(
            REMOTE_AI_STATE,
            REMOTE_AI_STATE,
            json!({
                "projectId": project_id,
                "projectName": project_name,
                "projectPath": project_path,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| error.to_string())
    }

    /// Run a memory read query on the host. `args` is merged with `op`; the
    /// host replies `{op, result}` and this returns the `result` JSON.
    pub fn memory_read(&self, op: &str, args: Value) -> Result<Value, String> {
        let mut payload = match args {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        payload.insert("op".to_string(), Value::String(op.to_string()));
        let value = self.request_with_timeout(
            REMOTE_MEMORY_READ,
            REMOTE_MEMORY_RESULT,
            Value::Object(payload),
            MEMORY_READ_TIMEOUT,
        )?;
        Ok(value.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Trigger a memory extraction run on the host with a forwarded provider
    /// config. The config (incl. its API key) is used for the run and not
    /// persisted on the host. Returns the refreshed extraction status JSON.
    /// Allows a long timeout — extraction runs an LLM over the host's sessions.
    pub fn memory_extract(&self, config: Value, output_locale: &str) -> Result<Value, String> {
        let value = self.request_with_timeout(
            REMOTE_MEMORY_RESULT,
            REMOTE_MEMORY_EXTRACT,
            json!({ "config": config, "outputLocale": output_locale }),
            std::time::Duration::from_secs(300),
        )?;
        Ok(value.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Run an AI-session op on the host (`detail`/`rename`/`remove`/`fork`).
    /// `args` is merged with `op`; returns the `result` JSON.
    pub fn ai_session(&self, op: &str, args: Value) -> Result<Value, String> {
        let mut payload = match args {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        payload.insert("op".to_string(), Value::String(op.to_string()));
        let value = self.request(
            REMOTE_AI_SESSION,
            REMOTE_AI_SESSION_RESULT,
            Value::Object(payload),
        )?;
        Ok(value.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn project_list(&self) -> Result<Value, String> {
        self.request(REMOTE_PROJECT_LIST, REMOTE_PROJECT_LIST, json!({}))
    }

    /// List a project's worktrees on the host.
    pub fn worktree_list(&self, project_id: &str, project_path: &str) -> Result<Value, String> {
        self.request(
            REMOTE_WORKTREE_LIST,
            REMOTE_WORKTREE_LIST,
            json!({ "projectId": project_id, "projectPath": project_path }),
        )
    }

    pub fn worktree_create(
        &self,
        project_id: &str,
        project_path: &str,
        branch_name: &str,
        base_branch: Option<&str>,
    ) -> Result<Value, String> {
        self.request(
            REMOTE_WORKTREE_UPDATED,
            REMOTE_WORKTREE_CREATE,
            json!({
                "projectId": project_id,
                "projectPath": project_path,
                "branchName": branch_name,
                "baseBranch": base_branch,
            }),
        )
    }

    pub fn worktree_remove(
        &self,
        project_id: &str,
        project_path: &str,
        worktree_path: &str,
        remove_branch: bool,
    ) -> Result<Value, String> {
        self.request(
            REMOTE_WORKTREE_UPDATED,
            REMOTE_WORKTREE_REMOVE,
            json!({
                "projectId": project_id,
                "projectPath": project_path,
                "worktreePath": worktree_path,
                "removeBranch": remove_branch,
            }),
        )
    }

    pub fn worktree_merge(
        &self,
        project_id: &str,
        project_path: &str,
        worktree_path: &str,
        base_branch: Option<&str>,
        remove_branch: bool,
    ) -> Result<Value, String> {
        self.request(
            REMOTE_WORKTREE_UPDATED,
            REMOTE_WORKTREE_MERGE,
            json!({
                "projectId": project_id,
                "projectPath": project_path,
                "worktreePath": worktree_path,
                "baseBranch": base_branch,
                "removeBranch": remove_branch,
            }),
        )
    }

    // ---- Terminal -----------------------------------------------------------

    /// Register where `terminal.output` frames are delivered (a RemoteOutputRouter
    /// feed). Replaces any previous sink.
    pub fn set_terminal_sink(&self, sink: TerminalSink) {
        if let Ok(mut guard) = self.inner.terminal_sink.lock() {
            *guard = Some(sink);
        }
    }

    /// Forward this session's `terminal.output` bytes to `forwarder` (the desktop
    /// terminal model's byte channel).
    pub fn register_terminal_output(&self, session_id: &str, forwarder: TerminalOutputForwarder) {
        if let Ok(mut guard) = self.inner.terminal_outputs.lock() {
            guard.insert(session_id.to_string(), forwarder);
        }
    }

    pub fn unregister_terminal_output(&self, session_id: &str) {
        if let Ok(mut guard) = self.inner.terminal_outputs.lock() {
            guard.remove(session_id);
        }
    }

    /// Typed terminal create (keeps `serde_json` out of the UI crate).
    pub fn open_terminal(
        &self,
        cwd: Option<&str>,
        command: Option<&str>,
        cols: Option<u16>,
        rows: Option<u16>,
        project_id: Option<&str>,
        title: Option<&str>,
        terminal_id: Option<&str>,
    ) -> Result<String, String> {
        // Pass our stable terminal id so the host keys the session by it and
        // RE-ATTACHES to the still-running shell on a later open (persistent
        // remote terminals) instead of spawning a fresh one each switch.
        self.create_terminal(json!({
            "cwd": cwd,
            "command": command,
            "cols": cols,
            "rows": rows,
            "projectId": project_id,
            "title": title,
            "terminalId": terminal_id,
        }))
    }

    /// Create a terminal on the host; returns its session id.
    pub fn create_terminal(&self, config: Value) -> Result<String, String> {
        let reply = self.request(REMOTE_TERMINAL_CREATED, REMOTE_TERMINAL_CREATE, config)?;
        // The host carries the session id at the envelope top level, but the
        // waiter only sees `payload` — where the host puts the new terminal object
        // keyed by `id` (and the bare fallback is `{ "id": … }`). Accept `id` as
        // well as `sessionId` so the attach doesn't fail with "missing sessionId"
        // even though the host did create the terminal.
        reply
            .get("sessionId")
            .or_else(|| reply.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "terminal.created reply missing sessionId".to_string())
    }

    /// Send keystrokes (fire-and-forget — input is high-frequency).
    pub fn terminal_input(&self, session_id: &str, data: &str) -> bool {
        self.fire(
            REMOTE_TERMINAL_INPUT,
            json!({ "sessionId": session_id, "data": data }),
        )
    }

    pub fn terminal_resize(&self, session_id: &str, cols: u16, rows: u16) -> bool {
        self.terminal_viewport_resize(session_id, cols, rows)
    }

    pub fn terminal_viewport_resize(&self, session_id: &str, cols: u16, rows: u16) -> bool {
        self.fire(
            REMOTE_TERMINAL_VIEWPORT_RESIZE,
            json!({ "sessionId": session_id, "cols": cols, "rows": rows }),
        )
    }

    pub fn terminal_buffer_tail(&self, session_id: &str) -> bool {
        self.fire(
            REMOTE_TERMINAL_BUFFER,
            json!({
                "sessionId": session_id,
                "offset": 0,
                "maxChars": REMOTE_TERMINAL_BUFFER_MAX_CHARS,
                "tail": true,
            }),
        )
    }

    pub fn close_terminal(&self, session_id: &str) -> Result<Value, String> {
        self.request(
            REMOTE_TERMINAL_CLOSED,
            REMOTE_TERMINAL_CLOSE,
            json!({ "sessionId": session_id }),
        )
    }

    /// Fire-and-forget terminal close, for reaping a host PTY on a user-initiated
    /// close. Cleanup doesn't need the ack, and this is called on the UI thread —
    /// so it must not block on a request round-trip.
    pub fn close_terminal_fire(&self, session_id: &str) -> bool {
        self.fire(REMOTE_TERMINAL_CLOSE, json!({ "sessionId": session_id }))
    }

    /// Send an envelope without awaiting a reply.
    fn fire(&self, kind: &str, payload: Value) -> bool {
        let mut envelope = json!({ "type": kind, "deviceId": self.device_id, "payload": payload });
        // The host reads the session id from the envelope top level (`sessionId`,
        // see RemoteEnvelope), NOT from the payload. Lift it so session-keyed ops
        // (terminal.input / terminal.resize) actually target their session instead
        // of being rejected with "Terminal session is required" — this is what
        // blocked the shell's reply to its own `\e[6n` cursor query (and typing,
        // and resize).
        if let Some(session_id) = envelope
            .get("payload")
            .and_then(|payload| payload.get("sessionId"))
            .cloned()
        {
            envelope["sessionId"] = session_id;
        }
        match serde_json::to_vec(&envelope) {
            Ok(bytes) => self.transport.send(bytes, None),
            Err(_) => false,
        }
    }
}

impl crate::host_browser::HostBrowserController for RemoteController {
    fn tcp_connect(
        &self,
        request: WebTunnelTcpConnectRequest,
    ) -> Result<Box<dyn WebTunnelIoStream>, String> {
        self.web_tunnel_tcp_connect(request)
    }
}

/// Build the persistable host record from a `pairing.confirmed` payload.
fn saved_host_from_confirmed(device_id: &str, payload: &Value) -> SavedRemoteHost {
    let field = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let transports = payload
        .get("transports")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| SavedRemoteTransport {
                    kind: item
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or(REMOTE_TRANSPORT_IROH)
                        .to_string(),
                    node_id: item
                        .get("nodeId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    relay_url: item
                        .get("relayUrl")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    relay_authentication: item
                        .get("relayAuthentication")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    SavedRemoteHost {
        device_id: device_id.to_string(),
        host_id: field("hostId"),
        host_name: field("hostName"),
        device_token: field("token"),
        platform: field("platform"),
        transports,
    }
}

#[cfg(test)]
mod e2e {
    //! End-to-end: pair the controller against a real in-process host over iroh,
    //! then drive a domain request. Ignored by default (iroh endpoint setup is
    //! slow and needs no network for direct dial); run with `--ignored`.
    use super::*;
    use codux_remote_transport::{RemoteHostTransportConfig, RemoteTransportFactory as Shared};

    #[test]
    #[ignore = "in-process iroh round trip; run with: cargo test -p codux-runtime -- --ignored controller"]
    fn controller_pairs_and_drives_real_host() {
        crate::async_runtime::block_on(async {
            // A minimal host: auto-confirm pairing and answer host.info, replying
            // through its own transport handle (filled in after connect).
            let host_slot: Arc<Mutex<Option<Arc<dyn RemoteTransport>>>> =
                Arc::new(Mutex::new(None));
            let reply_slot = Arc::clone(&host_slot);
            let on_message = Arc::new(move |_source: String, data: Vec<u8>| {
                let Ok(envelope) = serde_json::from_slice::<Value>(&data) else {
                    return;
                };
                let kind = envelope
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let device_id = envelope.get("deviceId").and_then(Value::as_str);
                let reply = match kind {
                    REMOTE_PAIRING_REQUEST => Some((
                        REMOTE_PAIRING_CONFIRMED,
                        json!({ "hostId": "host-it", "deviceId": device_id.unwrap_or_default(),
                                "token": "device-token-it", "hostName": "IT Host", "transports": [] }),
                    )),
                    REMOTE_HOST_INFO => Some((
                        REMOTE_HOST_INFO,
                        json!({ "hostId": "host-it", "name": "IT Host" }),
                    )),
                    _ => None,
                };
                if let Some((reply_kind, reply_payload)) = reply {
                    let mut envelope = json!({ "type": reply_kind, "payload": reply_payload });
                    if let Some(device_id) = device_id {
                        envelope["deviceId"] = json!(device_id);
                    }
                    if let (Ok(bytes), Ok(guard)) =
                        (serde_json::to_vec(&envelope), reply_slot.lock())
                    {
                        if let Some(transport) = guard.as_ref() {
                            transport.send(bytes, device_id);
                        }
                    }
                }
            });

            let config = RemoteHostTransportConfig {
                relay_url: "https://relay.example".to_string(),
                relay_preset: "global".to_string(),
                iroh_relay_url: String::new(),
                iroh_relay_authentication: String::new(),
                host_id: "host-it".to_string(),
                host_token: "token-it".to_string(),
            };
            let host = Shared::connect_host(
                &config,
                on_message,
                Arc::new(|_| Ok(())),
                Arc::new(|_, _| {}),
                Arc::new(|_| {}),
                None,
                None,
            )
            .await
            .expect("host connects");
            *host_slot.lock().unwrap() = Some(Arc::clone(&host));
            let endpoint_ticket = host.iroh_endpoint_ticket().expect("host ticket");

            let ticket = PairingTicket {
                code: "c".to_string(),
                secret: "s".to_string(),
                pairing_id: "p".to_string(),
                transports: vec![TicketTransport {
                    kind: REMOTE_TRANSPORT_IROH.to_string(),
                    ticket: endpoint_ticket,
                    node_id: String::new(),
                    relay_url: String::new(),
                    relay_authentication: String::new(),
                }],
            };
            let (controller, saved) = RemoteController::pair(
                &ticket,
                "test-desktop",
                new_device_id(),
                Arc::new(|_, _| {}),
            )
            .await
            .expect("pairing succeeds");
            assert_eq!(saved.host_id, "host-it");
            assert_eq!(saved.host_name, "IT Host");

            // A real domain request over the same paired connection.
            let info = crate::async_runtime::spawn_blocking(move || {
                let info = controller.host_info();
                (controller, info)
            })
            .await
            .unwrap();
            assert_eq!(
                info.1
                    .expect("host.info reply")
                    .get("hostId")
                    .and_then(Value::as_str),
                Some("host-it")
            );

            info.0.shutdown().await;
            host.shutdown().await;
        });
    }

    #[test]
    #[ignore = "in-process iroh round trip; run with: cargo test -p codux-runtime -- --ignored controller"]
    fn manager_pairs_from_ticket_and_browses() {
        use super::super::controller_manager::RemoteControllerManager;

        // Host: auto-confirm pairing, answer file.list with one fixed entry.
        let host_slot: Arc<Mutex<Option<Arc<dyn RemoteTransport>>>> = Arc::new(Mutex::new(None));
        let reply_slot = Arc::clone(&host_slot);
        let on_message = Arc::new(move |_source: String, data: Vec<u8>| {
            let Ok(envelope) = serde_json::from_slice::<Value>(&data) else {
                return;
            };
            let kind = envelope
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let device_id = envelope.get("deviceId").and_then(Value::as_str);
            let reply = match kind {
                REMOTE_PAIRING_REQUEST => Some((
                    REMOTE_PAIRING_CONFIRMED,
                    json!({ "hostId": "host-it", "deviceId": device_id.unwrap_or_default(),
                            "token": "device-token-it", "hostName": "IT Host", "transports": [] }),
                )),
                REMOTE_FILE_LIST => Some((
                    REMOTE_FILE_LIST,
                    json!({ "path": "/remote", "parent": "/",
                            "entries": [{ "name": "src", "path": "/remote/src", "isDirectory": true }] }),
                )),
                _ => None,
            };
            if let Some((reply_kind, reply_payload)) = reply {
                let mut envelope = json!({ "type": reply_kind, "payload": reply_payload });
                if let Some(device_id) = device_id {
                    envelope["deviceId"] = json!(device_id);
                }
                if let (Ok(bytes), Ok(guard)) = (serde_json::to_vec(&envelope), reply_slot.lock()) {
                    if let Some(transport) = guard.as_ref() {
                        transport.send(bytes, device_id);
                    }
                }
            }
        });

        // Stand up the host and mint a pasteable ticket from its endpoint ticket.
        let ticket_url = crate::async_runtime::block_on(async {
            let config = codux_remote_transport::RemoteHostTransportConfig {
                relay_url: "https://relay.example".to_string(),
                relay_preset: "global".to_string(),
                iroh_relay_url: String::new(),
                iroh_relay_authentication: String::new(),
                host_id: "host-it".to_string(),
                host_token: "token-it".to_string(),
            };
            let host = codux_remote_transport::RemoteTransportFactory::connect_host(
                &config,
                on_message,
                Arc::new(|_| Ok(())),
                Arc::new(|_, _| {}),
                Arc::new(|_| {}),
                None,
                None,
            )
            .await
            .expect("host connects");
            *host_slot.lock().unwrap() = Some(Arc::clone(&host));
            let endpoint_ticket = host.iroh_endpoint_ticket().expect("host ticket");
            // Keep the host alive for the duration of the test.
            std::mem::forget(host);
            let payload = json!({
                "code": "c", "secret": "s", "pairingId": "p",
                "transports": [{ "kind": REMOTE_TRANSPORT_IROH, "ticket": endpoint_ticket }],
            });
            let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&payload).unwrap());
            format!("codux://pair?payload={encoded}")
        });

        // The full user path: paste ticket -> pair (persists + caches) -> browse.
        let support = std::env::temp_dir().join(format!(
            "codux-controller-mgr-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&support).unwrap();
        let manager = RemoteControllerManager::new(support.clone());

        let saved = manager
            .pair(&ticket_url, "test-desktop")
            .expect("pair via manager");
        assert_eq!(saved.host_id, "host-it");
        // Persisted to the store.
        assert_eq!(manager.saved_hosts().len(), 1);

        let listing = manager
            .controller_for(&saved.device_id)
            .expect("cached controller")
            .file_list(Some("/remote"), Some("projectFiles"))
            .expect("remote browse");
        assert_eq!(
            listing["entries"][0]["name"].as_str(),
            Some("src"),
            "remote directory listing routed through the manager"
        );

        std::fs::remove_dir_all(support).ok();
    }

    #[test]
    #[ignore = "in-process iroh round trip; run with: cargo test -p codux-runtime -- --ignored controller"]
    fn controller_streams_terminal_output_into_the_router() {
        use codux_protocol::terminal_live_output_payload;
        use codux_terminal_core::RemoteTerminalOutputRouter;

        // Host: reply terminal.created, then echo terminal.input back as a
        // terminal.output frame (a fake PTY).
        let host_slot: Arc<Mutex<Option<Arc<dyn RemoteTransport>>>> = Arc::new(Mutex::new(None));
        let reply_slot = Arc::clone(&host_slot);
        let on_message = Arc::new(move |_source: String, data: Vec<u8>| {
            let Ok(envelope) = serde_json::from_slice::<Value>(&data) else {
                return;
            };
            let kind = envelope
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let device_id = envelope.get("deviceId").and_then(Value::as_str);
            let send = |value: Value| {
                if let (Ok(bytes), Ok(guard)) = (serde_json::to_vec(&value), reply_slot.lock()) {
                    if let Some(transport) = guard.as_ref() {
                        transport.send(bytes, device_id);
                    }
                }
            };
            match kind {
                REMOTE_PAIRING_REQUEST => send(json!({
                    "type": REMOTE_PAIRING_CONFIRMED, "deviceId": device_id,
                    "payload": { "hostId": "h", "deviceId": device_id.unwrap_or_default(), "token": "device-token-it", "transports": [] },
                })),
                REMOTE_TERMINAL_CREATE => send(json!({
                    "type": REMOTE_TERMINAL_CREATED, "deviceId": device_id,
                    "payload": { "sessionId": "t1" },
                })),
                REMOTE_TERMINAL_INPUT => {
                    let text = envelope
                        .get("payload")
                        .and_then(|payload| payload.get("data"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    send(json!({
                        "type": REMOTE_TERMINAL_OUTPUT,
                        "sessionId": "t1",
                        "deviceId": device_id,
                        "payload": terminal_live_output_payload(text.clone(), text.len(), 1),
                    }));
                }
                _ => {}
            }
        });

        crate::async_runtime::block_on(async {
            let config = codux_remote_transport::RemoteHostTransportConfig {
                relay_url: "https://relay.example".to_string(),
                relay_preset: "global".to_string(),
                iroh_relay_url: String::new(),
                iroh_relay_authentication: String::new(),
                host_id: "h".to_string(),
                host_token: "t".to_string(),
            };
            let host = codux_remote_transport::RemoteTransportFactory::connect_host(
                &config,
                on_message,
                Arc::new(|_| Ok(())),
                Arc::new(|_, _| {}),
                Arc::new(|_| {}),
                None,
                None,
            )
            .await
            .expect("host");
            *host_slot.lock().unwrap() = Some(Arc::clone(&host));
            let ticket = PairingTicket {
                code: "c".to_string(),
                secret: "s".to_string(),
                pairing_id: "p".to_string(),
                transports: vec![TicketTransport {
                    kind: REMOTE_TRANSPORT_IROH.to_string(),
                    ticket: host.iroh_endpoint_ticket().expect("ticket"),
                    node_id: String::new(),
                    relay_url: String::new(),
                    relay_authentication: String::new(),
                }],
            };
            let (controller, _saved) =
                RemoteController::pair(&ticket, "test", new_device_id(), Arc::new(|_, _| {}))
                    .await
                    .expect("pair");
            let controller = Arc::new(controller);

            // The router assembles terminal output, just like mobile does.
            let router = Arc::new(Mutex::new(RemoteTerminalOutputRouter::new(
                100_000, 100_000,
            )));
            let router_for_sink = Arc::clone(&router);
            controller.set_terminal_sink(Box::new(move |envelope| {
                if let Ok(mut router) = router_for_sink.lock() {
                    router.accept(&envelope, Some("t1"));
                }
            }));

            let controller_for_calls = Arc::clone(&controller);
            let assembled = crate::async_runtime::spawn_blocking(move || {
                let session = controller_for_calls
                    .create_terminal(json!({ "command": "echo", "cwd": "/tmp" }))
                    .expect("create terminal");
                router.lock().unwrap().bind_session(&session, false);
                controller_for_calls.terminal_input(&session, "hello-remote-term");
                // Poll until the echoed output is assembled by the router.
                for _ in 0..50 {
                    if let Some(content) = router.lock().unwrap().content(&session) {
                        if content.contains("hello-remote-term") {
                            return true;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                false
            })
            .await
            .unwrap();
            assert!(assembled, "router assembled the remote terminal output");

            controller.shutdown().await;
            host.shutdown().await;
        });
    }

    /// A dropped controller reconnecting with the SAME device id resumes the
    /// SAME host terminal session: the host keeps the session and routes its
    /// output by device id to whichever connection is current, so re-registering
    /// the per-session forwarder on the fresh controller (the desktop's rebind)
    /// is enough to resume — no new session, no host-side change.
    #[test]
    #[ignore = "in-process iroh round trip; run with: cargo test -p codux-runtime -- --ignored controller"]
    fn controller_reconnect_resumes_same_terminal_session() {
        use codux_protocol::terminal_live_output_payload;

        // Fake host: echo terminal.input back as terminal.output(sessionId=t1),
        // routed to the sender's device id — i.e. to whichever connection holds
        // that device id now (the real host's peer map behaves the same).
        let host_slot: Arc<Mutex<Option<Arc<dyn RemoteTransport>>>> = Arc::new(Mutex::new(None));
        let reply_slot = Arc::clone(&host_slot);
        let on_message = Arc::new(move |_source: String, data: Vec<u8>| {
            let Ok(envelope) = serde_json::from_slice::<Value>(&data) else {
                return;
            };
            let kind = envelope
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let device_id = envelope.get("deviceId").and_then(Value::as_str);
            let send = |value: Value| {
                if let (Ok(bytes), Ok(guard)) = (serde_json::to_vec(&value), reply_slot.lock()) {
                    if let Some(transport) = guard.as_ref() {
                        transport.send(bytes, device_id);
                    }
                }
            };
            match kind {
                REMOTE_TERMINAL_CREATE => send(json!({
                    "type": REMOTE_TERMINAL_CREATED, "deviceId": device_id,
                    "payload": { "sessionId": "t1" },
                })),
                REMOTE_TERMINAL_INPUT => {
                    let text = envelope
                        .get("payload")
                        .and_then(|payload| payload.get("data"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    send(json!({
                        "type": REMOTE_TERMINAL_OUTPUT,
                        "sessionId": "t1",
                        "deviceId": device_id,
                        "payload": terminal_live_output_payload(text.clone(), text.len(), 1),
                    }));
                }
                _ => {}
            }
        });

        fn wait_for_marker(rx: &mpsc::Receiver<Vec<u8>>, marker: &str) -> bool {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut accumulated = String::new();
            while Instant::now() < deadline {
                if let Ok(bytes) = rx.recv_timeout(Duration::from_millis(200)) {
                    accumulated.push_str(&String::from_utf8_lossy(&bytes));
                    if accumulated.contains(marker) {
                        return true;
                    }
                }
            }
            false
        }

        crate::async_runtime::block_on(async {
            let config = codux_remote_transport::RemoteHostTransportConfig {
                relay_url: "https://relay.example".to_string(),
                relay_preset: "global".to_string(),
                iroh_relay_url: String::new(),
                iroh_relay_authentication: String::new(),
                host_id: "h".to_string(),
                host_token: "t".to_string(),
            };
            let host = codux_remote_transport::RemoteTransportFactory::connect_host(
                &config,
                on_message,
                Arc::new(|_| Ok(())),
                Arc::new(|_, _| {}),
                Arc::new(|_| {}),
                None,
                None,
            )
            .await
            .expect("host");
            *host_slot.lock().unwrap() = Some(Arc::clone(&host));

            // Same device id + ticket for both connections — a reconnect, not a
            // new device.
            let device_id = new_device_id();
            let target = RemoteControllerTarget {
                host_id: "h".to_string(),
                device_id: device_id.clone(),
                device_token: String::new(),
                relay_url: String::new(),
                node_id: String::new(),
                ticket: host.iroh_endpoint_ticket().expect("ticket"),
                relay_authentication: String::new(),
            };

            // Controller A: create the session and verify output flows.
            let controller_a = Arc::new(
                RemoteController::connect(&target, Arc::new(|_, _| {}))
                    .await
                    .expect("A"),
            );
            let (tx_a, rx_a) = mpsc::channel::<Vec<u8>>();
            controller_a.register_terminal_output(
                "t1",
                Box::new(move |bytes| {
                    let _ = tx_a.send(bytes);
                }),
            );
            let ca = Arc::clone(&controller_a);
            let got_a = crate::async_runtime::spawn_blocking(move || {
                let session = ca
                    .create_terminal(json!({ "command": "echo", "cwd": "/tmp" }))
                    .expect("create");
                assert_eq!(session, "t1");
                ca.terminal_input("t1", "before-drop");
                wait_for_marker(&rx_a, "before-drop")
            })
            .await
            .unwrap();
            assert!(got_a, "controller A receives the session output");
            controller_a.shutdown().await;

            // Controller B: reconnect with the same device id, re-register the
            // forwarder for the SAME session (the desktop's rebind), and verify
            // the host routes the resumed session's output to this new connection.
            let controller_b = Arc::new(
                RemoteController::connect(&target, Arc::new(|_, _| {}))
                    .await
                    .expect("B"),
            );
            let (tx_b, rx_b) = mpsc::channel::<Vec<u8>>();
            controller_b.register_terminal_output(
                "t1",
                Box::new(move |bytes| {
                    let _ = tx_b.send(bytes);
                }),
            );
            let cb = Arc::clone(&controller_b);
            let got_b = crate::async_runtime::spawn_blocking(move || {
                cb.terminal_input("t1", "after-reconnect");
                wait_for_marker(&rx_b, "after-reconnect")
            })
            .await
            .unwrap();
            assert!(
                got_b,
                "reconnected controller B resumes the same session without re-creating it"
            );

            controller_b.shutdown().await;
            host.shutdown().await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use codux_protocol::RemoteTransportKind;

    struct NoopTransport;

    #[async_trait]
    impl RemoteTransport for NoopTransport {
        fn kind(&self) -> RemoteTransportKind {
            RemoteTransportKind::Iroh
        }

        fn send(&self, _data: Vec<u8>, _device_id: Option<&str>) -> bool {
            false
        }

        async fn shutdown(&self) {}
    }

    fn waiter(inner: &ControllerInner, expect: &str) -> mpsc::Receiver<Result<Value, String>> {
        let (tx, rx) = mpsc::channel();
        inner.waiters.lock().unwrap().push(Waiter {
            id: 1,
            expect: expect.to_string(),
            tx,
        });
        rx
    }

    #[test]
    fn route_resolves_waiter_by_reply_type() {
        let inner = ControllerInner::default();
        let rx = waiter(&inner, REMOTE_GIT_STATUS);
        inner.route(br#"{"type":"git.status","payload":{"isRepository":true}}"#);
        let result = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(result.unwrap()["isRepository"], json!(true));
    }

    #[test]
    fn route_error_fails_oldest_waiter() {
        let inner = ControllerInner::default();
        let rx = waiter(&inner, REMOTE_FILE_LIST);
        inner.route(br#"{"type":"error","payload":{"message":"nope"}}"#);
        let result = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(result, Err("nope".to_string()));
    }

    #[test]
    fn route_unmatched_reply_is_queued_as_event() {
        let inner = ControllerInner::default();
        // An unsolicited message with no waiter (here a broadcast) is queued for
        // `drain_events`. `terminal.output` is the one exception — it is demuxed
        // to a per-session forwarder and never queued (see below).
        inner.route(br#"{"type":"pairing.confirmed","payload":{"hostId":"h"}}"#);
        let events = inner.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "pairing.confirmed");
    }

    #[test]
    fn route_terminal_output_is_not_queued_as_event() {
        let inner = ControllerInner::default();
        // No sink, no matching forwarder: the frame is dropped, never queued.
        inner.route(br#"{"type":"terminal.output","payload":{"data":"x"}}"#);
        assert!(inner.events.lock().unwrap().is_empty());
    }

    #[test]
    fn unregister_terminal_output_stops_forwarding_session_bytes() {
        let inner = Arc::new(ControllerInner::default());
        let controller = RemoteController {
            transport: Arc::new(NoopTransport),
            device_id: "device-1".to_string(),
            inner: Arc::clone(&inner),
            next_id: AtomicU64::new(1),
        };
        let (tx, rx) = mpsc::channel();
        controller.register_terminal_output(
            "session-1",
            Box::new(move |bytes| {
                let _ = tx.send(bytes);
            }),
        );

        inner
            .route(br#"{"type":"terminal.output","sessionId":"session-1","payload":{"data":"x"}}"#);
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), b"x");

        controller.unregister_terminal_output("session-1");
        inner
            .route(br#"{"type":"terminal.output","sessionId":"session-1","payload":{"data":"y"}}"#);
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn terminal_output_ignores_screen_data_when_live_data_is_empty() {
        let inner = Arc::new(ControllerInner::default());
        let controller = RemoteController {
            transport: Arc::new(NoopTransport),
            device_id: "device-1".to_string(),
            inner: Arc::clone(&inner),
            next_id: AtomicU64::new(1),
        };
        let (tx, rx) = mpsc::channel();
        controller.register_terminal_output(
            "session-1",
            Box::new(move |bytes| {
                let _ = tx.send(bytes);
            }),
        );

        inner.route(
            br#"{"type":"terminal.output","sessionId":"session-1","payload":{"data":"","screenData":"\u001b[2J\u001b[Hready"}}"#,
        );

        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn terminal_output_forwards_live_data_without_screen_data() {
        let inner = Arc::new(ControllerInner::default());
        let controller = RemoteController {
            transport: Arc::new(NoopTransport),
            device_id: "device-1".to_string(),
            inner: Arc::clone(&inner),
            next_id: AtomicU64::new(1),
        };
        let (tx, rx) = mpsc::channel();
        controller.register_terminal_output(
            "session-1",
            Box::new(move |bytes| {
                let _ = tx.send(bytes);
            }),
        );

        inner.route(
            br#"{"type":"terminal.output","sessionId":"session-1","payload":{"data":"history\n","screenData":"\u001b[2J\u001b[Htui"}}"#,
        );

        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            b"history\n"
        );
    }

    #[test]
    fn parse_pairing_ticket_decodes_host_payload() {
        // Mirror the host's `remote_pairing_payload` shape exactly.
        let payload = json!({
            "code": "1234",
            "secret": "s3cr3t",
            "pairingId": "pair-abc",
            "transports": [{ "kind": "iroh", "ticket": "ticket-blob", "relayAuthentication": "auth" }],
        });
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        let url = format!("codux://pair?payload={encoded}");

        let ticket = parse_pairing_ticket(&url).unwrap();
        assert_eq!(ticket.code, "1234");
        assert_eq!(ticket.secret, "s3cr3t");
        assert_eq!(ticket.pairing_id, "pair-abc");
        assert_eq!(ticket.transports.len(), 1);
        assert_eq!(ticket.transports[0].ticket, "ticket-blob");
        assert_eq!(ticket.transports[0].relay_authentication, "auth");

        // The bare base64url payload (no codux:// prefix) parses too.
        assert!(parse_pairing_ticket(&encoded).is_ok());
    }

    #[test]
    fn parse_pairing_ticket_rejects_incomplete() {
        let payload = json!({ "code": "1", "secret": "", "pairingId": "p", "transports": [] });
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        assert!(parse_pairing_ticket(&encoded).is_err());
    }

    #[test]
    fn parse_pairing_ticket_accepts_node_and_relay_without_ticket() {
        // The slim QR form: node id + relay url, no iroh endpoint ticket.
        let payload = json!({
            "code": "1234",
            "secret": "s3cr3t",
            "pairingId": "pair-abc",
            "transports": [{ "kind": "iroh", "nodeId": "node-x", "relayUrl": "https://relay.example/" }],
        });
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        let ticket = parse_pairing_ticket(&encoded).expect("slim ticket should parse");
        assert_eq!(ticket.transports[0].node_id, "node-x");
        assert_eq!(ticket.transports[0].relay_url, "https://relay.example/");
        assert!(ticket.transports[0].ticket.is_empty());
    }

    #[test]
    fn saved_host_from_confirmed_maps_reply() {
        // Mirror the host's `pairing.confirmed` payload shape.
        let payload = json!({
            "hostId": "host-1",
            "deviceId": "dev-1",
            "token": "device-token-1",
            "hostName": "Studio",
            "transports": [{ "kind": "iroh", "nodeId": "node-x", "relayUrl": "https://relay", "relayAuthentication": "" }],
        });
        let saved = saved_host_from_confirmed("dev-1", &payload);
        assert_eq!(saved.host_id, "host-1");
        assert_eq!(saved.host_name, "Studio");
        assert_eq!(saved.device_id, "dev-1");
        assert_eq!(saved.device_token, "device-token-1");
        assert_eq!(saved.transports.len(), 1);
        assert_eq!(saved.transports[0].node_id, "node-x");
        assert_eq!(saved.transports[0].relay_url, "https://relay");
    }

    #[test]
    fn route_matches_same_type_waiters_fifo() {
        let inner = ControllerInner::default();
        let (tx1, rx1) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();
        {
            let mut waiters = inner.waiters.lock().unwrap();
            waiters.push(Waiter {
                id: 1,
                expect: REMOTE_FILE_LIST.to_string(),
                tx: tx1,
            });
            waiters.push(Waiter {
                id: 2,
                expect: REMOTE_FILE_LIST.to_string(),
                tx: tx2,
            });
        }
        inner.route(br#"{"type":"file.list","payload":{"n":1}}"#);
        inner.route(br#"{"type":"file.list","payload":{"n":2}}"#);
        assert_eq!(
            rx1.recv_timeout(Duration::from_secs(1)).unwrap().unwrap()["n"],
            json!(1)
        );
        assert_eq!(
            rx2.recv_timeout(Duration::from_secs(1)).unwrap().unwrap()["n"],
            json!(2)
        );
    }
}
