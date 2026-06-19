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
    REMOTE_AI_STATE, REMOTE_AI_STATS, REMOTE_ERROR, REMOTE_FILE_CREATE_DIRECTORY, REMOTE_FILE_DELETE,
    REMOTE_FILE_DELETED, REMOTE_FILE_DIRECTORY_CREATED, REMOTE_FILE_LIST, REMOTE_FILE_READ,
    REMOTE_FILE_RENAME, REMOTE_FILE_RENAMED, REMOTE_FILE_WRITE, REMOTE_FILE_WRITTEN,
    REMOTE_GIT_STATUS, REMOTE_HOST_INFO, REMOTE_PAIRING_CONFIRMED, REMOTE_PAIRING_REJECTED,
    REMOTE_PAIRING_REQUEST, REMOTE_PROJECT_LIST, REMOTE_TERMINAL_CLOSE, REMOTE_TERMINAL_CLOSED,
    REMOTE_TERMINAL_CREATE, REMOTE_TERMINAL_CREATED, REMOTE_TERMINAL_INPUT, REMOTE_TERMINAL_OUTPUT,
    REMOTE_TERMINAL_RESIZE, REMOTE_TRANSPORT_IROH,
};
use codux_remote_transport::{
    RemoteControllerTransportConfig, RemoteTransport, RemoteTransportCandidate,
};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::controller_store::{SavedRemoteHost, SavedRemoteTransport};
use super::transport_factory::RemoteTransportFactory;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
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
/// The iroh node id + relay are carried inside each transport's `ticket`.
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
    pub relay_authentication: String,
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
    let payload: Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("Pairing ticket is not valid: {error}"))?;

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
        .any(|transport| transport.kind == REMOTE_TRANSPORT_IROH && !transport.ticket.is_empty())
    {
        return Err("Pairing ticket has no usable iroh transport.".to_string());
    }
    Ok(ticket)
}

fn new_device_id() -> String {
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
            let data = payload.get("data").and_then(Value::as_str);
            if let (Some(session_id), Some(data)) = (session_id, data) {
                if let Ok(forwarders) = self.terminal_outputs.lock() {
                    if let Some(forwarder) = forwarders.get(session_id) {
                        forwarder(data.as_bytes().to_vec());
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
        self.waiters.lock().unwrap().retain(|waiter| waiter.id != id);
    }
}

pub struct RemoteController {
    transport: Arc<dyn RemoteTransport>,
    device_id: String,
    inner: Arc<ControllerInner>,
    next_id: AtomicU64,
}

impl RemoteController {
    pub async fn connect(target: &RemoteControllerTarget) -> Result<Self, String> {
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
            Arc::new(|_, _| {}),
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
    pub async fn connect_saved(host: &SavedRemoteHost) -> Result<Self, String> {
        let iroh = host
            .transports
            .iter()
            .find(|transport| transport.kind == REMOTE_TRANSPORT_IROH)
            .ok_or_else(|| "Saved host has no iroh transport.".to_string())?;
        Self::connect(&RemoteControllerTarget {
            host_id: host.host_id.clone(),
            device_id: host.device_id.clone(),
            device_token: host.device_token.clone(),
            relay_url: iroh.relay_url.clone(),
            node_id: iroh.node_id.clone(),
            ticket: String::new(),
            relay_authentication: iroh.relay_authentication.clone(),
        })
        .await
    }

    /// Drive the pairing handshake against a host that has an active pairing:
    /// connect unpaired (self-minted device id, empty token, ticket-only iroh
    /// candidate), send `pairing.request`, and wait for the operator to confirm.
    /// On success returns the live controller plus the persistable host record.
    pub async fn pair(
        ticket: &PairingTicket,
        device_name: &str,
    ) -> Result<(Self, SavedRemoteHost), String> {
        let iroh = ticket
            .transports
            .iter()
            .find(|transport| transport.kind == REMOTE_TRANSPORT_IROH && !transport.ticket.is_empty())
            .ok_or_else(|| "Pairing ticket has no usable iroh transport.".to_string())?;
        let device_id = new_device_id();
        let controller = Self::connect(&RemoteControllerTarget {
            host_id: String::new(),
            device_id: device_id.clone(),
            device_token: String::new(),
            relay_url: String::new(),
            node_id: String::new(),
            ticket: iroh.ticket.clone(),
            relay_authentication: iroh.relay_authentication.clone(),
        })
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

    pub async fn shutdown(&self) {
        self.transport.shutdown().await;
    }

    // ---- Typed domain helpers -----------------------------------------------

    pub fn host_info(&self) -> Result<Value, String> {
        self.request(REMOTE_HOST_INFO, REMOTE_HOST_INFO, json!({}))
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
    pub fn browse_directory(&self, path: Option<&str>) -> Result<RemoteDirectoryListing, String> {
        let value = self.file_list(path, Some("projectFiles"))?;
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

    pub fn create_directory(&self, path: &str) -> Result<Value, String> {
        self.request(
            REMOTE_FILE_DIRECTORY_CREATED,
            REMOTE_FILE_CREATE_DIRECTORY,
            json!({ "path": path }),
        )
    }

    pub fn write_file(&self, path: &str, content: &str) -> Result<Value, String> {
        self.request(
            REMOTE_FILE_WRITTEN,
            REMOTE_FILE_WRITE,
            json!({ "path": path, "content": content }),
        )
    }

    pub fn delete_path(&self, path: &str) -> Result<Value, String> {
        self.request(REMOTE_FILE_DELETED, REMOTE_FILE_DELETE, json!({ "path": path }))
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

    pub fn ai_stats(&self, project_id: &str) -> Result<Value, String> {
        self.request(REMOTE_AI_STATS, REMOTE_AI_STATS, json!({ "projectId": project_id }))
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

    pub fn project_list(&self) -> Result<Value, String> {
        self.request(REMOTE_PROJECT_LIST, REMOTE_PROJECT_LIST, json!({}))
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
    ) -> Result<String, String> {
        self.create_terminal(json!({
            "cwd": cwd,
            "command": command,
            "cols": cols,
            "rows": rows,
            "projectId": project_id,
            "title": title,
        }))
    }

    /// Create a terminal on the host; returns its session id.
    pub fn create_terminal(&self, config: Value) -> Result<String, String> {
        let reply = self.request(REMOTE_TERMINAL_CREATED, REMOTE_TERMINAL_CREATE, config)?;
        reply
            .get("sessionId")
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
        self.fire(
            REMOTE_TERMINAL_RESIZE,
            json!({ "sessionId": session_id, "cols": cols, "rows": rows }),
        )
    }

    pub fn close_terminal(&self, session_id: &str) -> Result<Value, String> {
        self.request(
            REMOTE_TERMINAL_CLOSED,
            REMOTE_TERMINAL_CLOSE,
            json!({ "sessionId": session_id }),
        )
    }

    /// Send an envelope without awaiting a reply.
    fn fire(&self, kind: &str, payload: Value) -> bool {
        let envelope = json!({ "type": kind, "deviceId": self.device_id, "payload": payload });
        match serde_json::to_vec(&envelope) {
            Ok(bytes) => self.transport.send(bytes, None),
            Err(_) => false,
        }
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
            let host_slot: Arc<Mutex<Option<Arc<dyn RemoteTransport>>>> = Arc::new(Mutex::new(None));
            let reply_slot = Arc::clone(&host_slot);
            let on_message = Arc::new(move |_source: String, data: Vec<u8>| {
                let Ok(envelope) = serde_json::from_slice::<Value>(&data) else {
                    return;
                };
                let kind = envelope.get("type").and_then(Value::as_str).unwrap_or_default();
                let device_id = envelope.get("deviceId").and_then(Value::as_str);
                let reply = match kind {
                    REMOTE_PAIRING_REQUEST => Some((
                        REMOTE_PAIRING_CONFIRMED,
                        json!({ "hostId": "host-it", "deviceId": device_id.unwrap_or_default(),
                                "token": "", "hostName": "IT Host", "transports": [] }),
                    )),
                    REMOTE_HOST_INFO => {
                        Some((REMOTE_HOST_INFO, json!({ "hostId": "host-it", "name": "IT Host" })))
                    }
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
                    relay_authentication: String::new(),
                }],
            };
            let (controller, saved) = RemoteController::pair(&ticket, "test-desktop")
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
                info.1.expect("host.info reply").get("hostId").and_then(Value::as_str),
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
            let kind = envelope.get("type").and_then(Value::as_str).unwrap_or_default();
            let device_id = envelope.get("deviceId").and_then(Value::as_str);
            let reply = match kind {
                REMOTE_PAIRING_REQUEST => Some((
                    REMOTE_PAIRING_CONFIRMED,
                    json!({ "hostId": "host-it", "deviceId": device_id.unwrap_or_default(),
                            "token": "", "hostName": "IT Host", "transports": [] }),
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

        let saved = manager.pair(&ticket_url, "test-desktop").expect("pair via manager");
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
            let kind = envelope.get("type").and_then(Value::as_str).unwrap_or_default();
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
                    "payload": { "hostId": "h", "deviceId": device_id.unwrap_or_default(), "token": "", "transports": [] },
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
                        "payload": terminal_live_output_payload(text.clone(), text.len(), 1, Some(text)),
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
                    relay_authentication: String::new(),
                }],
            };
            let (controller, _saved) = RemoteController::pair(&ticket, "test").await.expect("pair");
            let controller = Arc::new(controller);

            // The router assembles terminal output, just like mobile does.
            let router = Arc::new(Mutex::new(RemoteTerminalOutputRouter::new(100_000, 100_000)));
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        inner.route(br#"{"type":"terminal.output","payload":{"data":"x"}}"#);
        let events = inner.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "terminal.output");
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
    fn saved_host_from_confirmed_maps_reply() {
        // Mirror the host's `pairing.confirmed` payload shape.
        let payload = json!({
            "hostId": "host-1",
            "deviceId": "dev-1",
            "token": "",
            "hostName": "Studio",
            "transports": [{ "kind": "iroh", "nodeId": "node-x", "relayUrl": "https://relay", "relayAuthentication": "" }],
        });
        let saved = saved_host_from_confirmed("dev-1", &payload);
        assert_eq!(saved.host_id, "host-1");
        assert_eq!(saved.host_name, "Studio");
        assert_eq!(saved.device_id, "dev-1");
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
            waiters.push(Waiter { id: 1, expect: REMOTE_FILE_LIST.to_string(), tx: tx1 });
            waiters.push(Waiter { id: 2, expect: REMOTE_FILE_LIST.to_string(), tx: tx2 });
        }
        inner.route(br#"{"type":"file.list","payload":{"n":1}}"#);
        inner.route(br#"{"type":"file.list","payload":{"n":2}}"#);
        assert_eq!(rx1.recv_timeout(Duration::from_secs(1)).unwrap().unwrap()["n"], json!(1));
        assert_eq!(rx2.recv_timeout(Duration::from_secs(1)).unwrap().unwrap()["n"], json!(2));
    }
}
