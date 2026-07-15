//! Minimal headless host: serve a few real runtime domains over the Iroh
//! transport so a controller (desktop client or mobile) can browse this
//! machine's files and read its host info. This is the first real slice of the
//! "headless controlled-end" — terminal/Git/AI domains follow the same
//! dispatch shape (see plan/interconnect-plan.md), reusing the stateless
//! payload builders in `codux-runtime-core`.

use codux_protocol::{
    REMOTE_AI_SESSION, REMOTE_AI_SESSION_RESULT, REMOTE_AI_STATE, REMOTE_AI_STATS, REMOTE_ERROR,
    REMOTE_FILE_BLOB, REMOTE_FILE_BYTES_WRITTEN, REMOTE_FILE_COPIED, REMOTE_FILE_COPY,
    REMOTE_FILE_CREATE_DIRECTORY, REMOTE_FILE_DELETE, REMOTE_FILE_DELETED,
    REMOTE_FILE_DIRECTORY_CREATED, REMOTE_FILE_LIST, REMOTE_FILE_MOVE, REMOTE_FILE_MOVED,
    REMOTE_FILE_READ, REMOTE_FILE_READ_BLOB, REMOTE_FILE_RENAME, REMOTE_FILE_RENAMED,
    REMOTE_FILE_WRITE, REMOTE_FILE_WRITE_BLOB, REMOTE_FILE_WRITE_BYTES, REMOTE_FILE_WRITTEN,
    REMOTE_GIT_INVOKE, REMOTE_GIT_READ, REMOTE_GIT_STATUS, REMOTE_HOST_INFO, REMOTE_HOST_METRICS,
    REMOTE_MEMORY_EXTRACT, REMOTE_MEMORY_READ, REMOTE_MEMORY_RESULT, REMOTE_PAIRING_CONFIRMED,
    REMOTE_PAIRING_REQUEST, REMOTE_PROJECT_ADD, REMOTE_PROJECT_LIST, REMOTE_PROJECT_REMOVE,
    REMOTE_SSH_LIST, REMOTE_SSH_LIST_RESULT, REMOTE_SSH_REMOVE, REMOTE_SSH_UPSERT,
    REMOTE_TERMINAL_CLOSE, REMOTE_TERMINAL_CLOSED, REMOTE_TERMINAL_CREATE, REMOTE_TERMINAL_CREATED,
    REMOTE_TERMINAL_INPUT, REMOTE_TERMINAL_OUTPUT, REMOTE_TERMINAL_STATUS, REMOTE_TRANSPORT_IROH,
    REMOTE_TRANSPORT_PING, REMOTE_TRANSPORT_PONG, REMOTE_WORKTREE_CREATE, REMOTE_WORKTREE_LIST,
    REMOTE_WORKTREE_MERGE, REMOTE_WORKTREE_REMOVE, REMOTE_WORKTREE_UPDATED,
};
use codux_remote_transport::{
    RemoteHostTransportConfig, RemoteHostTransportHandlers, RemoteTransport,
    RemoteTransportCandidate, RemoteTransportFactory, WebTunnelTcpConnectRequest,
};
use codux_runtime_core::{
    file::{
        file_copy, file_delete, file_list_payload, file_make_directory, file_move,
        file_read_blob_bytes, file_read_payload, file_rename, file_write, file_write_bytes,
    },
    git::git_status_payload,
    host::{HostInfoPayload, host_info_payload},
    project::project_list_payload,
};
use codux_runtime_live::{
    ai_runtime::AIRuntimeBridge, ai_runtime_state::AIRuntimeStateService,
    host_metrics::sample_host_metrics, terminal_pty::TerminalManager,
};

use crate::projects::AgentProjectStore;
use codux_ai_history::indexer::AIHistoryIndexer;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

/// What the agent needs to stand up a host endpoint.
pub struct AgentHostConfig {
    pub host_id: String,
    pub host_token: String,
    pub name: String,
    pub relay_preset: String,
    /// Custom relay URL (used only when `relay_preset` is "custom").
    pub relay_url: String,
    /// Optional bearer token for a custom relay.
    pub relay_authentication: String,
}

type TransportSlot = Arc<Mutex<Option<Arc<dyn RemoteTransport>>>>;
/// Our own iroh dial candidate `(node_id, relay_url)`, filled in after connect
/// so `pairing.confirmed` can hand the controller a reconnect transport.
type CandidateSlot = Arc<Mutex<Option<(String, String)>>>;
/// Devices watching a project's `ai.stats` (project_id -> device ids). A device
/// registers by requesting `ai.stats`; the poller re-pushes fresh stats to them
/// when the live AI runtime changes, so remote views tick like the desktop's.
type AIStatsWatchers = Arc<Mutex<HashMap<String, HashSet<String>>>>;

#[derive(Clone)]
struct AgentPairingState {
    active: Arc<Mutex<AgentPairingCredential>>,
    device_store_path: PathBuf,
    authorization: Arc<crate::device_store::DeviceAuthorizationCache>,
    ticket_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AgentPairingCredential {
    pairing_id: String,
    code: String,
    secret: String,
}

impl AgentPairingState {
    fn new() -> Result<Self, String> {
        let device_store_path = crate::paths::devices_path();
        Ok(Self {
            active: Arc::new(Mutex::new(new_agent_pairing_credential()?)),
            authorization: Arc::new(crate::device_store::DeviceAuthorizationCache::new(
                device_store_path.clone(),
            )),
            device_store_path,
            ticket_path: crate::paths::ticket_path(),
        })
    }

    #[cfg(test)]
    fn with_paths(device_store_path: PathBuf, ticket_path: PathBuf) -> Result<Self, String> {
        Ok(Self {
            active: Arc::new(Mutex::new(new_agent_pairing_credential()?)),
            authorization: Arc::new(crate::device_store::DeviceAuthorizationCache::new(
                device_store_path.clone(),
            )),
            device_store_path,
            ticket_path,
        })
    }

    fn publish(
        &self,
        candidate: &(String, String),
        relay_authentication: &str,
    ) -> Result<String, String> {
        let credential = self
            .active
            .lock()
            .map(|active| active.clone())
            .map_err(|_| "agent pairing state is unavailable".to_string())?;
        Ok(pairing_ticket_url(
            &credential,
            &candidate.0,
            &candidate.1,
            relay_authentication,
        ))
    }

    fn confirm(
        &self,
        handshake: codux_protocol::RemoteTransportPairingRequest,
        candidate: &CandidateSlot,
        host_id: &str,
        name: &str,
        relay_authentication: &str,
    ) -> Result<Value, String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "agent pairing state is unavailable".to_string())?;
        codux_protocol::pairing_request_matches(
            &active.pairing_id,
            &active.code,
            &active.secret,
            false,
            &handshake,
        )?;
        let device_token = random_token()?;
        let next = new_agent_pairing_credential()?;
        *active = next.clone();
        let candidate = candidate
            .lock()
            .ok()
            .and_then(|candidate| candidate.clone());
        if let Some(candidate) = candidate.as_ref() {
            let ticket =
                pairing_ticket_url(&next, &candidate.0, &candidate.1, relay_authentication);
            crate::runstate::write_ticket_at(&self.ticket_path, &ticket)?;
        }
        let devices = crate::device_store::record_at(
            &self.device_store_path,
            &handshake.device_id,
            &device_token,
            &handshake.device_name,
            handshake.platform.as_deref().unwrap_or_default(),
        )?;
        self.authorization.replace_after_write(&devices);
        let transports = candidate
            .map(|(node_id, relay_url)| {
                let transport =
                    codux_protocol::iroh_transport_candidate_with_ticket_and_authentication(
                        &relay_url,
                        node_id,
                        relay_url.clone(),
                        "",
                        relay_authentication,
                    );
                vec![codux_protocol::confirmed_transport_entry(&transport)]
            })
            .unwrap_or_default();
        Ok(json!({
            "hostId": host_id,
            "deviceId": handshake.device_id,
            "token": device_token,
            "hostName": name,
            "platform": std::env::consts::OS,
            "transports": transports,
        }))
    }

    fn is_authorized(&self, device_id: &str, device_token: &str) -> bool {
        let device_id = device_id.trim();
        let device_token = device_token.trim();
        self.authorization.is_authorized(device_id, device_token)
    }
}

fn remove_ai_stats_watcher_device(device_id: &str, watchers: &AIStatsWatchers) {
    if let Ok(mut watchers) = watchers.lock() {
        for devices in watchers.values_mut() {
            devices.remove(device_id);
        }
        watchers.retain(|_, devices| !devices.is_empty());
    }
}

fn remove_ai_stats_watcher_project(project_id: &str, watchers: &AIStatsWatchers) {
    if let Ok(mut watchers) = watchers.lock() {
        watchers.remove(project_id);
    }
}

fn remove_device_state(
    device_id: &str,
    driver: &TerminalManager,
    fanout: &crate::terminals::TerminalFanout,
    ai_stats_watchers: &AIStatsWatchers,
) {
    let device_id = device_id.trim();
    if device_id.is_empty() {
        return;
    }
    let affected_sessions = driver
        .list()
        .into_iter()
        .filter(|terminal| {
            fanout
                .viewers(&terminal.id)
                .iter()
                .any(|viewer| viewer == device_id)
        })
        .map(|terminal| terminal.id)
        .collect::<Vec<_>>();
    fanout.remove_device(device_id);
    remove_ai_stats_watcher_device(device_id, ai_stats_watchers);
    for session_id in affected_sessions {
        if fanout.viewers(&session_id).is_empty() {
            driver.shrink_remote_screen_scrollback(&session_id);
        }
    }
}

/// Build the message handler that dispatches incoming envelopes to the served
/// domains and replies through the (post-connect) transport handle.
struct AgentMessageHandlerContext {
    slot: TransportSlot,
    runtime: tokio::runtime::Handle,
    driver: Arc<TerminalManager>,
    fanout: crate::terminals::TerminalFanout,
    indexer: AIHistoryIndexer,
    ai_current_sessions: Arc<AgentAICurrentSessionProvider>,
    ai_stats_watchers: AIStatsWatchers,
    host_id: String,
    name: String,
}

fn make_handler(
    context: AgentMessageHandlerContext,
) -> codux_remote_transport::RemoteTransportMessageHandler {
    let AgentMessageHandlerContext {
        slot,
        runtime,
        driver,
        fanout,
        indexer,
        ai_current_sessions,
        ai_stats_watchers,
        host_id,
        name,
    } = context;
    Arc::new(move |source: String, data: Vec<u8>| {
        let Ok(envelope) = serde_json::from_slice::<Value>(&data) else {
            return;
        };
        let kind = envelope.get("type").and_then(Value::as_str).unwrap_or("");
        let device_id = envelope.get("deviceId").and_then(Value::as_str);
        let request_id = envelope.get("requestId").and_then(Value::as_str);
        let payload = envelope.get("payload").cloned().unwrap_or(Value::Null);

        // Terminals stream output asynchronously, so they are handled
        // imperatively (they send their own responses) rather than as a
        // single reply.
        if crate::terminals::is_terminal_kind(kind) {
            crate::terminals::handle_terminal(
                &driver, &slot, &fanout, device_id, kind, &envelope, &payload,
            );
            return;
        }

        // Memory extraction runs the engine + an LLM (async, possibly slow), so
        // it is handled imperatively on its own runtime thread and sends its own
        // reply, rather than blocking the message dispatch.
        if kind == REMOTE_MEMORY_EXTRACT {
            let slot = Arc::clone(&slot);
            let device = device_id.map(str::to_string);
            let request = request_id.map(str::to_string);
            let payload = payload.clone();
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let (reply_kind, result) = match runtime {
                    Ok(runtime) => {
                        match runtime.block_on(crate::memory::memory_extract_payload(&payload)) {
                            Ok(result) => (REMOTE_MEMORY_RESULT, result),
                            Err(error) => (REMOTE_ERROR, json!({ "message": error })),
                        }
                    }
                    Err(error) => (
                        REMOTE_ERROR,
                        json!({ "message": format!("Unable to start memory extraction: {error}") }),
                    ),
                };
                let mut envelope = json!({ "type": reply_kind, "payload": result });
                if let Some(device) = device.as_deref() {
                    envelope["deviceId"] = json!(device);
                }
                if let Some(request) = request.as_deref() {
                    envelope["requestId"] = json!(request);
                }
                if let Ok(bytes) = serde_json::to_vec(&envelope)
                    && let Ok(guard) = slot.lock()
                    && let Some(transport) = guard.as_ref()
                {
                    transport.send(bytes, device.as_deref());
                }
            });
            return;
        }

        if kind == REMOTE_HOST_METRICS {
            let slot = Arc::clone(&slot);
            let device = device_id.map(str::to_string);
            let request = request_id.map(str::to_string);
            std::thread::spawn(move || {
                let mut envelope = json!({
                    "type": REMOTE_HOST_METRICS,
                    "payload": sample_host_metrics(),
                });
                if let Some(device) = device.as_deref() {
                    envelope["deviceId"] = json!(device);
                }
                if let Some(request) = request.as_deref() {
                    envelope["requestId"] = json!(request);
                }
                if let Ok(bytes) = serde_json::to_vec(&envelope)
                    && let Ok(guard) = slot.lock()
                    && let Some(transport) = guard.as_ref()
                {
                    transport.send(bytes, device.as_deref());
                }
            });
            return;
        }

        // Binary-safe file read: publish the file's bytes to the blob store and
        // reply with a ticket the controller fetches over iroh-blobs (async).
        if kind == REMOTE_FILE_READ_BLOB {
            let slot = Arc::clone(&slot);
            let device = device_id.map(str::to_string);
            let request = request_id.map(str::to_string);
            let path = payload
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            // Run on the agent runtime (where the iroh endpoint lives), not a
            // fresh thread runtime — blob transfer drives the endpoint.
            runtime.spawn(async move {
                let result = match file_read_blob_bytes(&path) {
                    Ok(bytes) => {
                        let transport = slot.lock().ok().and_then(|guard| guard.as_ref().cloned());
                        match transport {
                            Some(transport) => match transport.publish_blob(bytes).await {
                                Ok(ticket) => json!({ "ticket": ticket }),
                                Err(error) => json!({ "error": error }),
                            },
                            None => json!({ "error": "transport unavailable" }),
                        }
                    }
                    Err(error) => json!({ "error": error.to_string() }),
                };
                let mut envelope = json!({ "type": REMOTE_FILE_BLOB, "payload": result });
                if let Some(device) = device.as_deref() {
                    envelope["deviceId"] = json!(device);
                }
                if let Some(request) = request.as_deref() {
                    envelope["requestId"] = json!(request);
                }
                if let Ok(bytes) = serde_json::to_vec(&envelope)
                    && let Ok(guard) = slot.lock()
                    && let Some(transport) = guard.as_ref()
                {
                    transport.send(bytes, device.as_deref());
                }
            });
            return;
        }

        // Binary-safe file write: fetch the controller-published blob and write
        // it (the blob counterpart of file.writeBytes; async).
        if kind == REMOTE_FILE_WRITE_BLOB {
            let slot = Arc::clone(&slot);
            let device = device_id.map(str::to_string);
            let request = request_id.map(str::to_string);
            let directory = payload
                .get("directory")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let name = payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let ticket = payload
                .get("ticket")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            runtime.spawn(async move {
                let (reply_kind, result) = {
                    let transport = slot.lock().ok().and_then(|guard| guard.as_ref().cloned());
                    let bytes = match transport {
                        Some(transport) => transport.fetch_blob(&ticket).await,
                        None => Err("transport unavailable".to_string()),
                    };
                    match bytes {
                        Ok(bytes) => match file_write_bytes(&directory, &name, &bytes) {
                            Ok(new_path) => {
                                (REMOTE_FILE_BYTES_WRITTEN, json!({ "path": new_path }))
                            }
                            Err(error) => (REMOTE_ERROR, json!({ "message": error })),
                        },
                        Err(error) => (REMOTE_ERROR, json!({ "message": error })),
                    }
                };
                let mut envelope = json!({ "type": reply_kind, "payload": result });
                if let Some(device) = device.as_deref() {
                    envelope["deviceId"] = json!(device);
                }
                if let Some(request) = request.as_deref() {
                    envelope["requestId"] = json!(request);
                }
                if let Ok(bytes) = serde_json::to_vec(&envelope)
                    && let Ok(guard) = slot.lock()
                    && let Some(transport) = guard.as_ref()
                {
                    transport.send(bytes, device.as_deref());
                }
            });
            return;
        }

        // (reply_kind, reply_payload). `None` => nothing to send back.
        let reply: Option<(&str, Value)> = match kind {
            REMOTE_TRANSPORT_PING => Some((REMOTE_TRANSPORT_PONG, json!({}))),
            codux_protocol::REMOTE_DEVICE_DISCONNECTED => {
                let source = source.trim();
                if let Some(device_id) =
                    device_id.or_else(|| (!source.is_empty()).then_some(source))
                {
                    remove_device_state(device_id, &driver, &fanout, &ai_stats_watchers);
                }
                None
            }
            REMOTE_HOST_INFO => Some((
                REMOTE_HOST_INFO,
                // Transports left empty: the controller already knows the path
                // it connected on; host.info here carries identity/capabilities.
                host_info_payload(HostInfoPayload {
                    host_id: host_id.clone(),
                    runtime_instance_id: format!("{host_id}-agent"),
                    name: name.clone(),
                    platform: std::env::consts::OS.to_string(),
                    app: "codux-agent".to_string(),
                    resource_subscriptions: vec![
                        codux_protocol::REMOTE_RESOURCE_TERMINALS.to_string(),
                    ],
                    transports: Vec::new(),
                }),
            )),
            REMOTE_FILE_LIST => {
                let path = payload.get("path").and_then(Value::as_str);
                let purpose = payload.get("purpose").and_then(Value::as_str);
                Some((REMOTE_FILE_LIST, file_list_payload(path, purpose)))
            }
            REMOTE_FILE_READ => match payload.get("path").and_then(Value::as_str) {
                Some(path) => match file_read_payload(path) {
                    Ok(value) => Some((REMOTE_FILE_READ, value)),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                },
                None => Some((REMOTE_ERROR, json!({ "message": "File path is required." }))),
            },
            REMOTE_FILE_WRITE => match (
                payload.get("path").and_then(Value::as_str),
                payload.get("content").and_then(Value::as_str),
            ) {
                (Some(path), Some(content)) => match file_write(path, content) {
                    Ok(()) => Some((REMOTE_FILE_WRITTEN, json!({ "path": path }))),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                },
                _ => Some((
                    REMOTE_ERROR,
                    json!({ "message": "File path and content are required." }),
                )),
            },
            REMOTE_FILE_RENAME => match (
                payload.get("path").and_then(Value::as_str),
                payload.get("newPath").and_then(Value::as_str),
            ) {
                (Some(path), Some(new_path)) => match file_rename(path, new_path) {
                    Ok(_) => Some((
                        REMOTE_FILE_RENAMED,
                        json!({ "path": path, "newPath": new_path }),
                    )),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                },
                _ => Some((
                    REMOTE_ERROR,
                    json!({ "message": "File path and newPath are required." }),
                )),
            },
            REMOTE_FILE_DELETE => match payload.get("path").and_then(Value::as_str) {
                Some(path) => match file_delete(path) {
                    Ok(()) => Some((REMOTE_FILE_DELETED, json!({ "path": path }))),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                },
                None => Some((REMOTE_ERROR, json!({ "message": "File path is required." }))),
            },
            REMOTE_FILE_CREATE_DIRECTORY => match payload.get("path").and_then(Value::as_str) {
                Some(path) => match file_make_directory(path) {
                    Ok(()) => Some((REMOTE_FILE_DIRECTORY_CREATED, json!({ "path": path }))),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                },
                None => Some((
                    REMOTE_ERROR,
                    json!({ "message": "Directory path is required." }),
                )),
            },
            REMOTE_FILE_COPY => {
                let path = payload.get("path").and_then(Value::as_str).unwrap_or("");
                let target = payload
                    .get("targetDir")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match file_copy(path, target) {
                    Ok(new_path) => Some((REMOTE_FILE_COPIED, json!({ "path": new_path }))),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                }
            }
            REMOTE_FILE_MOVE => {
                let path = payload.get("path").and_then(Value::as_str).unwrap_or("");
                let target = payload
                    .get("targetDir")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let overwrite = payload
                    .get("overwrite")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                match file_move(path, target, overwrite) {
                    Ok(new_path) => Some((REMOTE_FILE_MOVED, json!({ "path": new_path }))),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                }
            }
            REMOTE_FILE_WRITE_BYTES => {
                use base64::Engine;
                let directory = payload
                    .get("directory")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let name = payload.get("name").and_then(Value::as_str).unwrap_or("");
                let bytes = payload
                    .get("bytes")
                    .and_then(Value::as_str)
                    .and_then(|encoded| {
                        base64::engine::general_purpose::STANDARD
                            .decode(encoded)
                            .ok()
                    })
                    .unwrap_or_default();
                match file_write_bytes(directory, name, &bytes) {
                    Ok(new_path) => Some((REMOTE_FILE_BYTES_WRITTEN, json!({ "path": new_path }))),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                }
            }
            REMOTE_PROJECT_LIST => Some((
                REMOTE_PROJECT_LIST,
                project_list_payload(AgentProjectStore::new().list(), None, None),
            )),
            REMOTE_PROJECT_ADD => match payload.get("path").and_then(Value::as_str) {
                Some(path) => {
                    let name = payload.get("name").and_then(Value::as_str);
                    match AgentProjectStore::new().add(path, name) {
                        Ok(items) => {
                            Some((REMOTE_PROJECT_LIST, project_list_payload(items, None, None)))
                        }
                        Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                    }
                }
                None => Some((
                    REMOTE_ERROR,
                    json!({ "message": "Project path is required." }),
                )),
            },
            REMOTE_PROJECT_REMOVE => {
                let id = payload
                    .get("id")
                    .or_else(|| payload.get("projectId"))
                    .and_then(Value::as_str);
                match id {
                    Some(id) => match AgentProjectStore::new().remove(id) {
                        Ok(items) => {
                            fanout.remove_project(id);
                            remove_ai_stats_watcher_project(id, &ai_stats_watchers);
                            Some((REMOTE_PROJECT_LIST, project_list_payload(items, None, None)))
                        }
                        Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                    },
                    None => Some((
                        REMOTE_ERROR,
                        json!({ "message": "Project id is required." }),
                    )),
                }
            }
            REMOTE_GIT_STATUS => {
                let (project_id, project_path) = git_project_target(&payload);
                Some((
                    REMOTE_GIT_STATUS,
                    git_status_payload(
                        project_id.as_str(),
                        project_path.as_str(),
                        codux_git::wire::status(&project_path),
                    ),
                ))
            }
            REMOTE_GIT_INVOKE => {
                let (project_id, project_path) = git_project_target(&payload);
                let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
                let args = payload.get("args").cloned().unwrap_or(Value::Null);
                match codux_git::wire::invoke(&project_path, op, &args) {
                    Ok(()) => Some((
                        REMOTE_GIT_STATUS,
                        git_status_payload(
                            project_id.as_str(),
                            project_path.as_str(),
                            codux_git::wire::status(&project_path),
                        ),
                    )),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                }
            }
            REMOTE_GIT_READ => {
                let (project_id, project_path) = git_project_target(&payload);
                let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
                let args = payload.get("args").cloned().unwrap_or(Value::Null);
                // `stored_state` is a full status payload (needs the project
                // envelope); every other read op shares the engine table.
                if op == "stored_state" {
                    Some((
                        REMOTE_GIT_READ,
                        json!({
                            "op": op,
                            "result": git_status_payload(
                        project_id.as_str(),
                        project_path.as_str(),
                                codux_git::wire::status(&project_path),
                            ),
                        }),
                    ))
                } else {
                    match codux_git::wire::read(&project_path, op, &args) {
                        Ok(result) => {
                            Some((REMOTE_GIT_READ, json!({ "op": op, "result": result })))
                        }
                        Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                    }
                }
            }
            REMOTE_WORKTREE_LIST => {
                let project_id = payload
                    .get("projectId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let project_path = payload
                    .get("projectPath")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                Some((
                    REMOTE_WORKTREE_LIST,
                    crate::worktree::worktree_list_payload(project_id, project_path),
                ))
            }
            REMOTE_WORKTREE_CREATE | REMOTE_WORKTREE_REMOVE | REMOTE_WORKTREE_MERGE => {
                let project_id = payload
                    .get("projectId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let project_path = payload
                    .get("projectPath")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let result = match kind {
                    REMOTE_WORKTREE_CREATE => crate::worktree::worktree_create_payload(
                        project_id,
                        project_path,
                        payload
                            .get("branchName")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                        payload.get("baseBranch").and_then(Value::as_str),
                    ),
                    REMOTE_WORKTREE_MERGE => crate::worktree::worktree_merge(
                        project_path,
                        payload
                            .get("worktreePath")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                        payload.get("baseBranch").and_then(Value::as_str),
                        payload
                            .get("removeBranch")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    )
                    .map(|_| crate::worktree::worktree_list_payload(project_id, project_path)),
                    _ => crate::worktree::worktree_remove(
                        project_path,
                        payload
                            .get("worktreePath")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                        payload
                            .get("removeBranch")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    )
                    .map(|_| crate::worktree::worktree_list_payload(project_id, project_path)),
                };
                match result {
                    Ok(payload) => Some((REMOTE_WORKTREE_UPDATED, payload)),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                }
            }
            REMOTE_AI_STATS => {
                // Resolve the project (path is needed to scan its CLI history),
                // falling back to the first project like the desktop host.
                let project_id = payload
                    .get("projectId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let store = AgentProjectStore::new();
                let project = store
                    .list()
                    .into_iter()
                    .find(|item| item.id == project_id)
                    .or_else(|| store.list().into_iter().next());
                match project {
                    Some(project) => {
                        // Register the requesting device as a watcher (one project
                        // per device) so the poller re-pushes on runtime change.
                        if let Some(device_id) = device_id.filter(|value| !value.trim().is_empty())
                            && let Ok(mut watchers) = ai_stats_watchers.lock()
                        {
                            for (id, devices) in watchers.iter_mut() {
                                if id != &project.id {
                                    devices.remove(device_id);
                                }
                            }
                            watchers.retain(|_, devices| !devices.is_empty());
                            watchers
                                .entry(project.id.clone())
                                .or_default()
                                .insert(device_id.to_string());
                        }
                        Some((
                            REMOTE_AI_STATS,
                            crate::ai_stats::ai_stats_payload(
                                &indexer,
                                ai_current_sessions.as_ref(),
                                &project.id,
                                &project.name,
                                &project.path,
                            ),
                        ))
                    }
                    None => Some((
                        REMOTE_ERROR,
                        json!({ "message": "Unable to load AI stats." }),
                    )),
                }
            }
            REMOTE_MEMORY_READ => {
                // The host runs the codux-memory engine against its own store.
                match crate::memory::memory_read_payload(&payload) {
                    Ok(result) => Some((REMOTE_MEMORY_RESULT, result)),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                }
            }
            REMOTE_AI_SESSION => {
                // The host runs the codux-ai-sessions engine against its own history.
                match crate::sessions::ai_session_payload(&indexer, &payload) {
                    Ok(result) => Some((REMOTE_AI_SESSION_RESULT, result)),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                }
            }
            REMOTE_SSH_LIST => {
                // The headless host has no saved SSH profiles of its own yet, so
                // it returns an empty list using the same shared wire shape.
                Some((REMOTE_SSH_LIST_RESULT, json!({ "profiles": [] })))
            }
            REMOTE_SSH_UPSERT | REMOTE_SSH_REMOVE => Some((
                REMOTE_ERROR,
                json!({
                    "message": "SSH profile management is only available on the desktop host.",
                }),
            )),
            REMOTE_AI_STATE => {
                // The controller owns the project record and sends its path; the
                // agent indexes the host's history for that path directly.
                let project_id = payload
                    .get("projectId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let project_name = payload
                    .get("projectName")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let project_path = payload
                    .get("projectPath")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match crate::ai_stats::ai_state_payload(
                    &indexer,
                    project_id,
                    project_name,
                    project_path,
                    payload
                        .get("refresh")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                ) {
                    Ok(state) => Some((REMOTE_AI_STATE, state)),
                    Err(error) => Some((REMOTE_ERROR, json!({ "message": error }))),
                }
            }
            _ => None,
        };

        let Some((reply_kind, reply_payload)) = reply else {
            return;
        };
        let mut reply_envelope = json!({ "type": reply_kind, "payload": reply_payload });
        if let Some(device_id) = device_id {
            reply_envelope["deviceId"] = json!(device_id);
        }
        if let Some(request_id) = request_id {
            reply_envelope["requestId"] = json!(request_id);
        }
        let Ok(bytes) = serde_json::to_vec(&reply_envelope) else {
            return;
        };
        if let Ok(guard) = slot.lock()
            && let Some(transport) = guard.as_ref()
        {
            transport.send(bytes, device_id);
        }
    })
}

/// Resolve the git target project: id → stored path, path → stored id, then
/// the raw payload values (mirrors the desktop host's resolution).
fn git_project_target(payload: &Value) -> (String, String) {
    let project_id = payload
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let project_path = payload
        .get("projectPath")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let items = AgentProjectStore::new().list();
    if !project_id.is_empty()
        && let Some(item) = items.iter().find(|item| item.id == project_id)
    {
        return (item.id.clone(), item.path.clone());
    }
    if !project_path.is_empty()
        && let Some(item) = items.iter().find(|item| {
            codux_runtime_core::path::local_paths_equal(
                std::path::Path::new(&item.path),
                std::path::Path::new(project_path),
            )
        })
    {
        return (item.id.clone(), item.path.clone());
    }
    (project_id.to_string(), project_path.to_string())
}

fn random_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| format!("operating system random source unavailable: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn authorize_web_tunnel_tcp_connect(
    pairing: &AgentPairingState,
    request: WebTunnelTcpConnectRequest,
) -> Result<(), String> {
    if !pairing.is_authorized(&request.device_id, &request.device_token) {
        return Err("device is not authorized".to_string());
    }
    if is_forbidden_host(&request.host) {
        return Err("target is not allowed".to_string());
    }
    Ok(())
}

fn is_forbidden_host(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']);
    if let Ok(ip) = IpAddr::from_str(host) {
        return is_forbidden_ip(ip);
    }
    false
}

fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.octets() == [169, 254, 169, 254],
        IpAddr::V6(ip) => ip.is_unspecified(),
    }
}

/// Connect a host transport with the dispatch handler. Returns the transport
/// handle and the slot it has been stored in (for replies).
async fn connect_serving_host(
    cfg: &AgentHostConfig,
) -> Result<(Arc<dyn RemoteTransport>, TransportSlot, AgentPairingState), String> {
    let slot: TransportSlot = Arc::new(Mutex::new(None));
    let candidate: CandidateSlot = Arc::new(Mutex::new(None));
    let pairing = AgentPairingState::new()?;
    let ai_runtime = Arc::new(AIRuntimeBridge::new());
    let driver = Arc::new(TerminalManager::with_ai_runtime(Arc::clone(&ai_runtime)));
    let fanout = crate::terminals::TerminalFanout::new();
    // On viewport-lease expiry, hand the viewport to another phone still viewing
    // the same agent terminal (if any) instead of snapping back to the host.
    {
        let fanout = fanout.clone();
        driver.set_viewport_owner_resolver(Arc::new(
            move |session_id: &str, expired_owner: &str| {
                fanout
                    .viewers(session_id)
                    .into_iter()
                    .map(|device| {
                        codux_runtime_live::terminal_pty::terminal_viewport_remote_owner(&device)
                    })
                    .find(|owner| owner != expired_owner)
            },
        ));
    }
    let ai_current_sessions = Arc::new(AgentAICurrentSessionProvider {
        ai_runtime: Arc::clone(&ai_runtime),
    });
    let ai_stats_watchers: AIStatsWatchers = Arc::new(Mutex::new(HashMap::new()));
    let indexer = crate::ai_stats::open_indexer();
    // For a custom relay the iroh relay URL must be set explicitly; for presets
    // the transport resolves it from `relay_preset`.
    let iroh_relay_url = if cfg.relay_preset == "custom" {
        cfg.relay_url.clone()
    } else {
        String::new()
    };
    let config = RemoteHostTransportConfig {
        relay_url: cfg.relay_url.clone(),
        relay_preset: cfg.relay_preset.clone(),
        iroh_relay_url,
        iroh_relay_authentication: cfg.relay_authentication.clone(),
        host_id: cfg.host_id.clone(),
        host_token: cfg.host_token.clone(),
    };
    let host = RemoteTransportFactory::connect_host(
        &config,
        RemoteHostTransportHandlers {
            on_message: make_handler(AgentMessageHandlerContext {
                slot: Arc::clone(&slot),
                runtime: tokio::runtime::Handle::current(),
                driver: Arc::clone(&driver),
                fanout: fanout.clone(),
                indexer: indexer.clone(),
                ai_current_sessions: Arc::clone(&ai_current_sessions),
                ai_stats_watchers: Arc::clone(&ai_stats_watchers),
                host_id: cfg.host_id.clone(),
                name: cfg.name.clone(),
            }),
            on_upload: Arc::new(|_| Ok(())),
            on_state: {
                let driver = Arc::clone(&driver);
                let fanout = fanout.clone();
                let ai_stats_watchers = Arc::clone(&ai_stats_watchers);
                Arc::new(move |device_id, state| {
                    if matches!(state.as_str(), "closed" | "failed" | "disconnected") {
                        remove_device_state(&device_id, &driver, &fanout, &ai_stats_watchers);
                    }
                })
            },
            on_pairing: {
                let pairing = pairing.clone();
                let candidate = Arc::clone(&candidate);
                let host_id = cfg.host_id.clone();
                let name = cfg.name.clone();
                let relay_authentication = cfg.relay_authentication.clone();
                Arc::new(move |handshake| {
                    pairing
                        .confirm(
                            handshake,
                            &candidate,
                            &host_id,
                            &name,
                            &relay_authentication,
                        )
                        .map_err(|error| {
                            eprintln!("Agent pairing rejected: {error}");
                        })
                        .ok()
                })
            },
            on_authorize: {
                let pairing = pairing.clone();
                Arc::new(move |device_id, device_token| {
                    pairing.is_authorized(device_id, device_token)
                })
            },
            on_web_tunnel_tcp_connect: {
                let pairing = pairing.clone();
                Some(Arc::new(move |request| {
                    authorize_web_tunnel_tcp_connect(&pairing, request)
                }))
            },
            on_log: None,
        },
    )
    .await?;
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(Arc::clone(&host));
    }
    if let (Ok(mut guard), Some((node_id, relay_url))) = (candidate.lock(), host.iroh_candidate()) {
        *guard = Some((node_id, relay_url));
    }
    spawn_ai_stats_poller(
        Arc::clone(&slot),
        indexer,
        ai_current_sessions,
        ai_stats_watchers,
    );
    spawn_terminal_status_poller(Arc::clone(&slot), ai_runtime);
    Ok((host, slot, pairing))
}

/// Watch the live AI runtime and re-push `ai.stats` to watching devices whenever
/// a project's current sessions change. The headless host has no UI tick, so a
/// lightweight poll drives the same real-time updates the desktop emits from its
/// runtime tick.
fn spawn_ai_stats_poller(
    slot: TransportSlot,
    indexer: AIHistoryIndexer,
    provider: Arc<AgentAICurrentSessionProvider>,
    watchers: AIStatsWatchers,
) {
    tokio::spawn(async move {
        use codux_protocol::RemoteAICurrentSession;
        use codux_runtime_core::ai_stats::RemoteAICurrentSessionProvider;
        let mut last: HashMap<String, Vec<RemoteAICurrentSession>> = HashMap::new();
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(1000));
        loop {
            ticker.tick().await;
            let projects = AgentProjectStore::new().list();
            let project_ids = projects
                .iter()
                .map(|project| project.id.as_str())
                .collect::<HashSet<_>>();
            let snapshot = match watchers.lock() {
                Ok(mut watchers) => {
                    watchers.retain(|project_id, devices| {
                        !devices.is_empty() && project_ids.contains(project_id.as_str())
                    });
                    watchers.clone()
                }
                Err(_) => continue,
            };
            last.retain(|project_id, _| snapshot.contains_key(project_id));
            if snapshot.is_empty() {
                continue;
            }
            for (project_id, devices) in snapshot {
                if devices.is_empty() {
                    continue;
                }
                let current = provider.current_sessions(&project_id);
                if last.get(&project_id) == Some(&current) {
                    continue;
                }
                last.insert(project_id.clone(), current);
                let Some(project) = projects.iter().find(|item| item.id == project_id) else {
                    continue;
                };
                let payload = crate::ai_stats::ai_stats_payload(
                    &indexer,
                    provider.as_ref(),
                    &project.id,
                    &project.name,
                    &project.path,
                );
                for device in &devices {
                    let envelope =
                        json!({ "type": REMOTE_AI_STATS, "payload": payload, "deviceId": device });
                    let Ok(bytes) = serde_json::to_vec(&envelope) else {
                        continue;
                    };
                    if let Ok(guard) = slot.lock()
                        && let Some(transport) = guard.as_ref()
                    {
                        transport.send(bytes, Some(device));
                    }
                }
            }
        }
    });
}

/// Forward live terminal status events (loading/waiting/completed dots) to
/// connected controllers; this is also the headless host's only supervisor
/// drain, so the event queue stays empty instead of riding its cap.
fn spawn_terminal_status_poller(slot: TransportSlot, ai_runtime: Arc<AIRuntimeBridge>) {
    tokio::spawn(async move {
        use codux_runtime_live::ai_runtime::AIRuntimeSupervisorEvent;
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(1000));
        loop {
            ticker.tick().await;
            for event in ai_runtime.drain_supervisor_events() {
                let AIRuntimeSupervisorEvent::TerminalStatus { status } = event else {
                    continue;
                };
                let Ok(payload) = serde_json::to_value(&status) else {
                    continue;
                };
                let envelope = json!({ "type": REMOTE_TERMINAL_STATUS, "payload": payload });
                let Ok(bytes) = serde_json::to_vec(&envelope) else {
                    continue;
                };
                if let Ok(guard) = slot.lock()
                    && let Some(transport) = guard.as_ref()
                {
                    transport.send(bytes, None);
                }
            }
        }
    });
}

struct AgentAICurrentSessionProvider {
    ai_runtime: Arc<AIRuntimeBridge>,
}

impl codux_runtime_core::ai_stats::RemoteAICurrentSessionProvider
    for AgentAICurrentSessionProvider
{
    fn current_sessions(&self, project_id: &str) -> Vec<codux_protocol::RemoteAICurrentSession> {
        let snapshot = self.ai_runtime.runtime_state_snapshot();
        let summary = AIRuntimeStateService::new(&crate::projects::agent_data_dir())
            .summary_from_runtime_snapshot(&snapshot);
        codux_runtime_live::ai_runtime_state::remote_current_sessions_from_runtime_state(
            &summary, project_id,
        )
    }
}

/// Run the headless host until the process is stopped, printing the pairing
/// candidate so a controller can connect.
pub async fn run_host(cfg: AgentHostConfig) -> Result<(), String> {
    let (host, _slot, pairing) = connect_serving_host(&cfg).await?;
    let web_test = match crate::web_test::start_background() {
        Ok(server) => Some(server),
        Err(error) => {
            eprintln!("Codux web test page disabled: {error}");
            None
        }
    };
    println!("Codux host ready.");
    println!("  device: {}", cfg.name);
    println!("  config: {}", crate::paths::config_path().display());
    if let Some(server) = &web_test {
        println!("  web:    http://{}/", server.address);
    }
    let (node_id, relay) = host.iroh_candidate().unwrap_or_default();
    if !node_id.is_empty() {
        println!("  node:   {node_id}");
        println!("  relay:  {relay}");
        // The pairing QR carries nodeId + relayUrl (NOT the bulky iroh endpoint
        // ticket) so it stays small and phone-scannable — matching the desktop
        // host's format. The controller dials from nodeId + relayUrl and the full
        // ticket is exchanged after it connects.
        let pairing =
            pairing.publish(&(node_id.clone(), relay.clone()), &cfg.relay_authentication)?;
        println!("  pair:   run `codux link` or `codux qrcode`");
        if verbose_startup_output() {
            println!("pairingTicket={pairing}");
        }
        // Publish for `codux link` / `codux qrcode` to read.
        crate::runstate::write_ticket(&pairing)?;
    }
    if let Some(ticket) = host.iroh_endpoint_ticket()
        && verbose_startup_output()
    {
        println!("ticket={ticket}");
    }
    // Publish status for `codux status` / `codux stop`.
    crate::runstate::write_status(&crate::runstate::DaemonStatus {
        pid: std::process::id(),
        started_at: chrono::Utc::now().to_rfc3339(),
        host_id: cfg.host_id.clone(),
        device_name: cfg.name.clone(),
        node_id,
        relay,
        web_test_url: web_test
            .as_ref()
            .map(|server| format!("http://{}/", server.address))
            .unwrap_or_default(),
    });
    // Serve until the process is terminated.
    std::future::pending::<()>().await;
    Ok(())
}

fn verbose_startup_output() -> bool {
    matches!(
        std::env::var("CODUX_AGENT_VERBOSE").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

/// Build the `codux://pair?payload=<base64url>` pairing URL the desktop
/// controller pastes / the phone scans. Carries the minimum needed to dial —
/// nodeId + relayUrl (+ relay auth) — NOT the bulky iroh endpoint ticket, so the
/// QR stays small and scannable (the ticket ~doubles QR density). The random
/// pairing credential is single-use and rotates after confirmation.
fn pairing_ticket_url(
    credential: &AgentPairingCredential,
    node_id: &str,
    relay_url: &str,
    relay_authentication: &str,
) -> String {
    use base64::Engine;
    // Build the dial candidate and serialize it through the SHARED payload
    // builder so the desktop and agent hosts emit byte-identical QR transports —
    // the ticket-free shape (nodeId + relayUrl) is defined once in codux_protocol.
    let candidate = codux_protocol::iroh_transport_candidate_with_ticket_and_authentication(
        relay_url,
        node_id,
        relay_url,
        "",
        relay_authentication,
    );
    let payload = codux_protocol::pairing_payload(
        &credential.code,
        &credential.secret,
        &credential.pairing_id,
        &[candidate],
    );
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    format!("codux://pair?payload={encoded}")
}

fn new_agent_pairing_credential() -> Result<AgentPairingCredential, String> {
    let token = random_token()?;
    Ok(AgentPairingCredential {
        pairing_id: random_token()?,
        code: token.chars().take(6).collect(),
        secret: random_token()?,
    })
}

/// In-process round trip: stand up the serving host, connect a controller, and
/// exercise the full file domain (list, mkdir, write, read, delete) end to end.
/// Proves the headless host actually serves real domains over the transport.
pub async fn run_serve_smoke_async() -> Result<String, String> {
    use codux_remote_transport::RemoteControllerTransportConfig;
    use tokio::sync::mpsc;

    let cfg = AgentHostConfig {
        host_id: "host-serve-smoke".to_string(),
        host_token: "token-serve-smoke".to_string(),
        name: "codux-agent-smoke".to_string(),
        relay_preset: "global".to_string(),
        relay_url: "https://relay.example".to_string(),
        relay_authentication: String::new(),
    };
    // Keep the smoke's project store out of the real ~/.codux-agent.
    let data_dir = std::env::temp_dir().join(format!("codux-agent-data-{}", std::process::id()));
    // Safe: the smoke sets this before any host/controller task is spawned.
    unsafe {
        std::env::set_var("CODUX_AGENT_DATA_DIR", &data_dir);
    }
    let (host, _slot, pairing) = connect_serving_host(&cfg).await?;
    let (node_id, relay_url) = host
        .iroh_candidate()
        .ok_or_else(|| "iroh host candidate missing".to_string())?;

    // Every reply from the host is forwarded as (type, payload).
    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel::<(String, Value)>();
    let device_id = "device-serve-smoke".to_string();
    let controller_config = RemoteControllerTransportConfig {
        relay_url: cfg.relay_url.clone(),
        host_id: cfg.host_id.clone(),
        device_id: device_id.clone(),
        device_token: "token-serve-smoke".to_string(),
        transports: vec![RemoteTransportCandidate {
            kind: REMOTE_TRANSPORT_IROH.to_string(),
            url: "https://relay.example/v3".to_string(),
            node_id,
            relay_url,
            ticket: host.iroh_endpoint_ticket().unwrap_or_default(),
            relay_authentication: String::new(),
        }],
    };
    let controller = RemoteTransportFactory::connect_controller(
        &controller_config,
        Arc::new(move |_source: String, data: Vec<u8>| {
            if let Ok(envelope) = serde_json::from_slice::<Value>(&data)
                && let Some(kind) = envelope.get("type").and_then(Value::as_str)
            {
                let payload = envelope.get("payload").cloned().unwrap_or(Value::Null);
                let _ = reply_tx.send((kind.to_string(), payload));
            }
        }),
        Arc::new(|_, _| {}),
        None,
    )
    .await?;

    let controller_ref = &controller;
    let device_ref = &device_id;
    let request = |kind: &str, payload: Value| -> Result<(), String> {
        let envelope = json!({ "type": kind, "deviceId": device_ref, "payload": payload });
        let bytes = serde_json::to_vec(&envelope).map_err(|error| error.to_string())?;
        if !controller_ref.send(bytes, None) {
            return Err(format!("controller send failed for {kind}"));
        }
        Ok(())
    };
    async fn expect(
        rx: &mut mpsc::UnboundedReceiver<(String, Value)>,
        want: &str,
    ) -> Result<Value, String> {
        loop {
            let (kind, payload) =
                tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
                    .await
                    .map_err(|_| format!("timeout waiting for {want}"))?
                    .ok_or_else(|| format!("channel closed waiting for {want}"))?;
            if kind == want {
                return Ok(payload);
            }
            if kind == REMOTE_ERROR {
                return Err(format!(
                    "host error while waiting for {want}: {}",
                    payload.get("message").and_then(Value::as_str).unwrap_or("")
                ));
            }
        }
    }

    let run = async {
        let pairing = pairing.active.lock().unwrap().clone();
        // 0. pairing handshake
        request(
            REMOTE_PAIRING_REQUEST,
            json!({
                "pairingId": pairing.pairing_id,
                "code": pairing.code,
                "secret": pairing.secret,
                "deviceName": "smoke-controller",
                "deviceId": device_id,
            }),
        )?;
        let confirmed = expect(&mut reply_rx, REMOTE_PAIRING_CONFIRMED).await?;
        if confirmed.get("hostId").and_then(Value::as_str) != Some(cfg.host_id.as_str()) {
            return Err(format!(
                "pairing.confirmed missing matching hostId: {confirmed}"
            ));
        }
        if confirmed.get("deviceId").and_then(Value::as_str) != Some(device_id.as_str()) {
            return Err("pairing.confirmed did not echo the device id".to_string());
        }
        let device_token = confirmed
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| "pairing.confirmed missing device token".to_string())?;
        if !controller.set_device_credentials(&device_id, device_token) {
            return Err("failed to activate paired device credentials".to_string());
        }

        request(REMOTE_HOST_METRICS, json!({}))?;
        let metrics = expect(&mut reply_rx, REMOTE_HOST_METRICS).await?;
        if metrics
            .get("sampledAtMillis")
            .and_then(Value::as_u64)
            .is_none()
        {
            return Err(format!(
                "host.metrics reply missing sampledAtMillis: {metrics}"
            ));
        }

        // 1. list
        request(REMOTE_FILE_LIST, json!({ "purpose": "projectFiles" }))?;
        let listed = expect(&mut reply_rx, REMOTE_FILE_LIST).await?;
        if listed.get("entries").is_none() {
            return Err("file.list reply missing entries".to_string());
        }

        // 2. mkdir / write / read / delete in a unique temp dir
        let dir = std::env::temp_dir().join(format!("codux-agent-serve-{}", std::process::id()));
        let dir = dir.to_string_lossy().to_string();
        let file = format!("{dir}/probe.txt");

        request(REMOTE_FILE_CREATE_DIRECTORY, json!({ "path": dir }))?;
        expect(&mut reply_rx, REMOTE_FILE_DIRECTORY_CREATED).await?;
        if !std::path::Path::new(&dir).is_dir() {
            return Err("createDirectory did not create the directory".to_string());
        }

        request(
            REMOTE_FILE_WRITE,
            json!({ "path": file, "content": "codux-agent" }),
        )?;
        expect(&mut reply_rx, REMOTE_FILE_WRITTEN).await?;

        request(REMOTE_FILE_READ, json!({ "path": file }))?;
        let read = expect(&mut reply_rx, REMOTE_FILE_READ).await?;
        if read.get("content").and_then(Value::as_str) != Some("codux-agent") {
            return Err(format!("file.read returned unexpected content: {read}"));
        }

        // Binary-safe read over iroh-blobs: the host publishes the bytes and we
        // fetch the blob by ticket (the cross-device Save-As read path).
        request(REMOTE_FILE_READ_BLOB, json!({ "path": file }))?;
        let blob = expect(&mut reply_rx, REMOTE_FILE_BLOB).await?;
        let ticket = blob
            .get("ticket")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("file.readBlob reply missing ticket: {blob}"))?;
        let bytes = controller.fetch_blob(ticket).await?;
        if bytes != b"codux-agent" {
            return Err(format!(
                "file.readBlob fetched wrong bytes ({} bytes)",
                bytes.len()
            ));
        }

        // Binary-safe write over iroh-blobs: publish bytes, the host fetches the
        // blob and writes the file; read it back to confirm.
        let write_ticket = controller.publish_blob(b"blob-write".to_vec()).await?;
        request(
            REMOTE_FILE_WRITE_BLOB,
            json!({ "directory": dir, "name": "blob.txt", "ticket": write_ticket }),
        )?;
        let written = expect(&mut reply_rx, REMOTE_FILE_BYTES_WRITTEN).await?;
        let written_path = written
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("file.writeBlob reply missing path: {written}"))?
            .to_string();
        request(REMOTE_FILE_READ, json!({ "path": written_path }))?;
        let reread = expect(&mut reply_rx, REMOTE_FILE_READ).await?;
        if reread.get("content").and_then(Value::as_str) != Some("blob-write") {
            return Err(format!("file.writeBlob round-trip mismatch: {reread}"));
        }

        request(REMOTE_FILE_DELETE, json!({ "path": dir }))?;
        expect(&mut reply_rx, REMOTE_FILE_DELETED).await?;
        if std::path::Path::new(&dir).exists() {
            return Err("delete did not remove the directory".to_string());
        }

        // 3. project domain (add / remove)
        let project_path = format!("/tmp/codux-agent-project-{}", std::process::id());
        request(
            REMOTE_PROJECT_ADD,
            json!({ "path": project_path, "name": "Smoke" }),
        )?;
        let added = expect(&mut reply_rx, REMOTE_PROJECT_LIST).await?;
        let project_id = added
            .get("projects")
            .and_then(Value::as_array)
            .and_then(|projects| {
                projects.iter().find(|project| {
                    project.get("path").and_then(Value::as_str) == Some(project_path.as_str())
                })
            })
            .and_then(|project| project.get("id").and_then(Value::as_str))
            .ok_or_else(|| "project.add did not register the project".to_string())?
            .to_string();

        request(REMOTE_PROJECT_REMOVE, json!({ "id": project_id }))?;
        let removed = expect(&mut reply_rx, REMOTE_PROJECT_LIST).await?;
        let still_present = removed
            .get("projects")
            .and_then(Value::as_array)
            .map(|projects| {
                projects.iter().any(|project| {
                    project.get("path").and_then(Value::as_str) == Some(project_path.as_str())
                })
            })
            .unwrap_or(false);
        if still_present {
            return Err("project.remove did not remove the project".to_string());
        }

        // 4. terminal domain (create -> input -> streamed output -> close)
        request(
            REMOTE_TERMINAL_CREATE,
            json!({ "command": "sh", "cwd": "/tmp" }),
        )?;
        let created = expect(&mut reply_rx, REMOTE_TERMINAL_CREATED).await?;
        let terminal_id = created
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| "terminal.create returned no sessionId".to_string())?
            .to_string();
        request(
            REMOTE_TERMINAL_INPUT,
            json!({ "sessionId": terminal_id, "data": "printf codux-terminal-ok\n" }),
        )?;
        let marker = "codux-terminal-ok";
        loop {
            let (kind, payload) =
                tokio::time::timeout(std::time::Duration::from_secs(8), reply_rx.recv())
                    .await
                    .map_err(|_| "timeout waiting for terminal output".to_string())?
                    .ok_or_else(|| "channel closed waiting for terminal output".to_string())?;
            if kind == REMOTE_TERMINAL_OUTPUT {
                let data = payload.get("data").and_then(Value::as_str).unwrap_or("");
                if data.contains(marker) {
                    break;
                }
            }
        }
        request(REMOTE_TERMINAL_CLOSE, json!({ "sessionId": terminal_id }))?;
        expect(&mut reply_rx, REMOTE_TERMINAL_CLOSED).await?;

        // 5. git domain (status against a fresh temp repo)
        let repo_dir = std::env::temp_dir().join(format!("codux-agent-git-{}", std::process::id()));
        let repo_dir = repo_dir.to_string_lossy().to_string();
        git2::Repository::init(&repo_dir).map_err(|error| error.to_string())?;
        std::fs::write(format!("{repo_dir}/probe.txt"), "x").map_err(|error| error.to_string())?;
        request(
            REMOTE_GIT_STATUS,
            json!({ "projectId": "p", "projectPath": repo_dir }),
        )?;
        let status = expect(&mut reply_rx, REMOTE_GIT_STATUS).await?;
        if status.get("isRepository").and_then(Value::as_bool) != Some(true) {
            return Err("git.status did not detect the repository".to_string());
        }
        if status.get("untracked").and_then(Value::as_u64).unwrap_or(0) < 1 {
            return Err("git.status missing the untracked file".to_string());
        }
        // git operations via the generic invoke/read: stage → commit → branch → diff
        request(
            REMOTE_GIT_INVOKE,
            json!({ "projectPath": repo_dir, "op": "stage", "args": { "paths": ["probe.txt"] } }),
        )?;
        let staged = expect(&mut reply_rx, REMOTE_GIT_STATUS).await?;
        if staged.get("staged").and_then(Value::as_u64).unwrap_or(0) < 1 {
            return Err(format!("git stage did not stage the file: {staged}"));
        }
        request(
            REMOTE_GIT_INVOKE,
            json!({ "projectPath": repo_dir, "op": "commit", "args": { "message": "smoke commit" } }),
        )?;
        let committed = expect(&mut reply_rx, REMOTE_GIT_STATUS).await?;
        if committed.get("staged").and_then(Value::as_u64).unwrap_or(9) != 0 {
            return Err(format!("git commit did not clean the tree: {committed}"));
        }
        // create + checkout a branch (shells out to git)
        request(
            REMOTE_GIT_INVOKE,
            json!({ "projectPath": repo_dir, "op": "create_branch", "args": { "branch": "feature", "checkout": true } }),
        )?;
        let branched = expect(&mut reply_rx, REMOTE_GIT_STATUS).await?;
        if branched.get("branch").and_then(Value::as_str) != Some("feature") {
            return Err(format!(
                "git create_branch did not switch branch: {branched}"
            ));
        }
        std::fs::write(format!("{repo_dir}/probe.txt"), "y").map_err(|error| error.to_string())?;
        request(
            REMOTE_GIT_READ,
            json!({ "projectPath": repo_dir, "op": "diff", "args": { "filePath": "probe.txt" } }),
        )?;
        let diff = expect(&mut reply_rx, REMOTE_GIT_READ).await?;
        if diff
            .pointer("/result/diff")
            .and_then(Value::as_str)
            .is_none()
        {
            return Err(format!("git read diff missing result: {diff}"));
        }
        // worktree list: the repo's main worktree shows up
        request(
            REMOTE_WORKTREE_LIST,
            json!({ "projectId": "p", "projectPath": repo_dir }),
        )?;
        let worktrees = expect(&mut reply_rx, REMOTE_WORKTREE_LIST).await?;
        let base_count = worktrees
            .get("worktrees")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        if base_count == 0 {
            return Err(format!("worktree.list returned no worktrees: {worktrees}"));
        }
        // worktree create → the list grows by one
        request(
            REMOTE_WORKTREE_CREATE,
            json!({ "projectId": "p", "projectPath": repo_dir, "branchName": "smoke-wt" }),
        )?;
        let created = expect(&mut reply_rx, REMOTE_WORKTREE_UPDATED).await?;
        if created
            .get("worktrees")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0)
            <= base_count
        {
            return Err(format!("worktree.create did not add a worktree: {created}"));
        }
        let _ = std::fs::remove_dir_all(&repo_dir);

        // 6. ai stats domain (single-reply usage snapshot for a project)
        let stats_project = format!("/tmp/codux-agent-ai-{}", std::process::id());
        request(
            REMOTE_PROJECT_ADD,
            json!({ "path": stats_project, "name": "AI" }),
        )?;
        let added = expect(&mut reply_rx, REMOTE_PROJECT_LIST).await?;
        let stats_project_id = added
            .get("projects")
            .and_then(Value::as_array)
            .and_then(|projects| {
                projects.iter().find(|project| {
                    project.get("path").and_then(Value::as_str) == Some(stats_project.as_str())
                })
            })
            .and_then(|project| project.get("id").and_then(Value::as_str))
            .ok_or_else(|| "project.add did not register the AI project".to_string())?
            .to_string();
        request(REMOTE_AI_STATS, json!({ "projectId": stats_project_id }))?;
        let stats = expect(&mut reply_rx, REMOTE_AI_STATS).await?;
        if stats.get("projectId").and_then(Value::as_str) != Some(stats_project_id.as_str()) {
            return Err(format!(
                "ai.stats reply missing matching projectId: {stats}"
            ));
        }
        if stats.get("sessions").and_then(Value::as_array).is_none() {
            return Err("ai.stats reply missing sessions array".to_string());
        }

        // ai.state: full AIHistoryProjectState for a desktop controller
        request(
            REMOTE_AI_STATE,
            json!({ "projectId": "p", "projectName": "AI", "projectPath": stats_project }),
        )?;
        let state = expect(&mut reply_rx, REMOTE_AI_STATE).await?;
        if state.get("projectId").and_then(Value::as_str) != Some("p") {
            return Err(format!(
                "ai.state reply missing matching projectId: {state}"
            ));
        }

        // memory.read: the host runs the codux-memory engine against its store.
        request(
            REMOTE_MEMORY_READ,
            json!({ "op": "summary", "projectPath": stats_project }),
        )?;
        let memory = expect(&mut reply_rx, REMOTE_MEMORY_RESULT).await?;
        if memory.get("op").and_then(Value::as_str) != Some("summary") {
            return Err(format!("memory.read reply missing op: {memory}"));
        }
        if memory.get("result").is_none() {
            return Err(format!("memory.read reply missing result: {memory}"));
        }

        // memory.extract: run an extraction pass. With no provider configured and
        // no indexed sessions, the engine processes an empty queue and returns a
        // status — exercising the async write path end to end without an LLM.
        request(
            REMOTE_MEMORY_EXTRACT,
            json!({ "config": {}, "outputLocale": "en" }),
        )?;
        let extract = expect(&mut reply_rx, REMOTE_MEMORY_RESULT).await?;
        if extract.get("op").and_then(Value::as_str) != Some("extract") {
            return Err(format!("memory.extract reply missing op: {extract}"));
        }

        // ai.session: the host runs the codux-ai-sessions engine. An unknown
        // detail request must return a correlated protocol error.
        request(
            REMOTE_AI_SESSION,
            json!({ "op": "detail", "projectPath": stats_project, "sessionId": "none" }),
        )?;
        let session = expect(&mut reply_rx, REMOTE_ERROR).await?;
        if session.get("message").and_then(Value::as_str) != Some("Session not found.") {
            return Err(format!("ai.session reply missing error: {session}"));
        }

        Ok::<(), String>(())
    };
    let result = run.await;

    host.shutdown().await;
    controller.shutdown().await;
    let _ = std::fs::remove_dir_all(&data_dir);
    result?;
    Ok("codux-agent-serve-ok\npairing + file (+blob) + project + terminal + git + ai + memory (read+extract) + ai-session domains verified".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_ai_stats_watcher_device_clears_every_project() {
        let watchers: AIStatsWatchers = Arc::new(Mutex::new(HashMap::from([
            (
                "project-1".to_string(),
                HashSet::from(["phone-a".to_string(), "phone-b".to_string()]),
            ),
            (
                "project-2".to_string(),
                HashSet::from(["phone-a".to_string()]),
            ),
        ])));

        remove_ai_stats_watcher_device("phone-a", &watchers);

        assert_eq!(
            watchers.lock().unwrap().get("project-1").cloned(),
            Some(HashSet::from(["phone-b".to_string()]))
        );
        assert!(!watchers.lock().unwrap().contains_key("project-2"));
    }

    #[test]
    fn remove_ai_stats_watcher_project_clears_every_device() {
        let watchers: AIStatsWatchers = Arc::new(Mutex::new(HashMap::from([
            (
                "project-1".to_string(),
                HashSet::from(["phone-a".to_string(), "phone-b".to_string()]),
            ),
            (
                "project-2".to_string(),
                HashSet::from(["phone-a".to_string()]),
            ),
        ])));

        remove_ai_stats_watcher_project("project-1", &watchers);

        let watchers = watchers.lock().unwrap();
        assert!(!watchers.contains_key("project-1"));
        assert!(watchers.contains_key("project-2"));
    }

    #[test]
    fn pairing_credentials_are_random_and_rotate_after_confirmation() {
        let data_dir = std::env::temp_dir().join(format!(
            "codux-agent-pairing-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let state = AgentPairingState::with_paths(
            data_dir.join("devices.json"),
            data_dir.join("pair-ticket.json"),
        )
        .unwrap();
        let first = state.active.lock().unwrap().clone();
        let candidate = Arc::new(Mutex::new(None));
        let response = state.confirm(
            codux_protocol::RemoteTransportPairingRequest {
                device_id: "phone-1".to_string(),
                device_name: "Phone".to_string(),
                platform: Some("ios".to_string()),
                pairing_id: Some(first.pairing_id.clone()),
                pairing_code: Some(first.code.clone()),
                pairing_secret: Some(first.secret.clone()),
            },
            &candidate,
            "host-1",
            "Agent",
            "",
        );
        let second = state.active.lock().unwrap().clone();

        assert!(response.is_ok());
        assert_ne!(first, second);
        assert!(
            state
                .confirm(
                    codux_protocol::RemoteTransportPairingRequest {
                        device_id: "phone-2".to_string(),
                        device_name: "Other".to_string(),
                        platform: None,
                        pairing_id: Some(first.pairing_id),
                        pairing_code: Some(first.code),
                        pairing_secret: Some(first.secret),
                    },
                    &candidate,
                    "host-1",
                    "Agent",
                    "",
                )
                .is_err()
        );
        assert!(
            state.is_authorized(
                "phone-1",
                response
                    .as_ref()
                    .ok()
                    .and_then(|value| value.get("token"))
                    .and_then(Value::as_str)
                    .unwrap()
            )
        );
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn pairing_ticket_write_failure_still_invalidates_used_credential() {
        let data_dir = std::env::temp_dir().join(format!(
            "codux-agent-pairing-write-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let ticket_path = data_dir.join("ticket-directory");
        std::fs::create_dir_all(&ticket_path).unwrap();
        let state =
            AgentPairingState::with_paths(data_dir.join("devices.json"), ticket_path).unwrap();
        let first = state.active.lock().unwrap().clone();
        let candidate = Arc::new(Mutex::new(Some((
            "node-1".to_string(),
            "https://relay.example".to_string(),
        ))));
        let request = codux_protocol::RemoteTransportPairingRequest {
            device_id: "phone-1".to_string(),
            device_name: "Phone".to_string(),
            platform: Some("ios".to_string()),
            pairing_id: Some(first.pairing_id.clone()),
            pairing_code: Some(first.code.clone()),
            pairing_secret: Some(first.secret.clone()),
        };

        assert!(
            state
                .confirm(request.clone(), &candidate, "host-1", "Agent", "")
                .is_err()
        );
        assert_ne!(*state.active.lock().unwrap(), first);
        assert!(
            state
                .confirm(request, &candidate, "host-1", "Agent", "")
                .is_err()
        );
        assert!(!state.device_store_path.exists());

        let _ = std::fs::remove_dir_all(data_dir);
    }
}
