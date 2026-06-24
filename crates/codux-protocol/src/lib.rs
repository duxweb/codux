use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

pub const REMOTE_PROTOCOL_VERSION: &str = "v3.1";
pub const REMOTE_TERMINAL_BUFFER_MAX_CHARS: usize = 200_000;
pub const REMOTE_TERMINAL_BUFFER_CHUNK_CHARS: usize = 16_384;
pub const REMOTE_RELAY_TICKET_TTL_SECS: u64 = 60;
pub const REMOTE_RELAY_TICKET_MAX_ENTRIES: usize = 4096;
pub const REMOTE_RELAY_TICKET_PAYLOAD_MAX_BYTES: usize = 64 << 10;
pub const REMOTE_RELAY_MAX_MESSAGE_BYTES: usize = 1 << 20;
pub const REMOTE_RELAY_BURST_LIMIT: usize = 120;
pub const REMOTE_RELAY_RATE_WINDOW_SECS: u64 = 10;

pub const REMOTE_RESOURCE_SUBSCRIBE: &str = "resource.subscribe";
pub const REMOTE_RESOURCE_UNSUBSCRIBE: &str = "resource.unsubscribe";
pub const REMOTE_RESOURCE_BASELINE: &str = "resource.baseline";
pub const REMOTE_RESOURCE_DELTA: &str = "resource.delta";
pub const REMOTE_RESOURCE_RESYNC: &str = "resource.resync";
pub const REMOTE_RESOURCE_PROJECTS: &str = "projects";
pub const REMOTE_RESOURCE_TERMINALS: &str = "terminals";
pub const REMOTE_RESOURCE_WORKTREES: &str = "worktrees";
pub const REMOTE_RESOURCE_GIT_STATUS: &str = "git.status";
pub const REMOTE_RESOURCE_AI_STATS: &str = "ai.stats";
pub const REMOTE_RESOURCE_FILES: &str = "files";

pub const REMOTE_HELLO: &str = "hello";
pub const REMOTE_ERROR: &str = "error";
pub const REMOTE_RELAY_ERROR: &str = "relay.error";
pub const REMOTE_HOST_INFO: &str = "host.info";
pub const REMOTE_HOST_OFFLINE: &str = "host.offline";
pub const REMOTE_DEVICE_INFO: &str = "device.info";
pub const REMOTE_DEVICE_CONNECTED: &str = "device.connected";
pub const REMOTE_DEVICE_DISCONNECTED: &str = "device.disconnected";
pub const REMOTE_PAIRING_REQUEST: &str = "pairing.request";
pub const REMOTE_PAIRING_CONFIRMED: &str = "pairing.confirmed";
pub const REMOTE_PAIRING_REJECTED: &str = "pairing.rejected";
pub const REMOTE_TRANSPORT_PING: &str = "transport.ping";
pub const REMOTE_TRANSPORT_PONG: &str = "transport.pong";
pub const REMOTE_PROJECT_LIST: &str = "project.list";
pub const REMOTE_PROJECT_SELECT: &str = "project.select";
pub const REMOTE_PROJECT_SELECTED: &str = "project.selected";
pub const REMOTE_PROJECT_ADD: &str = "project.add";
pub const REMOTE_PROJECT_EDIT: &str = "project.edit";
pub const REMOTE_PROJECT_REMOVE: &str = "project.remove";
pub const REMOTE_PROJECT_UPDATED: &str = "project.updated";
pub const REMOTE_WORKTREE_LIST: &str = "worktree.list";
pub const REMOTE_WORKTREE_SELECT: &str = "worktree.select";
pub const REMOTE_WORKTREE_CREATE: &str = "worktree.create";
pub const REMOTE_WORKTREE_MERGE: &str = "worktree.merge";
pub const REMOTE_WORKTREE_DELETE: &str = "worktree.delete";
pub const REMOTE_WORKTREE_REMOVE: &str = "worktree.remove";
pub const REMOTE_WORKTREE_UPDATED: &str = "worktree.updated";
pub const REMOTE_TERMINAL_LIST: &str = "terminal.list";
pub const REMOTE_TERMINAL_SUBSCRIBE: &str = "terminal.subscribe";
pub const REMOTE_TERMINAL_UNSUBSCRIBE: &str = "terminal.unsubscribe";
pub const REMOTE_TERMINAL_CREATE: &str = "terminal.create";
pub const REMOTE_TERMINAL_CREATED: &str = "terminal.created";
pub const REMOTE_TERMINAL_CLOSE: &str = "terminal.close";
pub const REMOTE_TERMINAL_CLOSED: &str = "terminal.closed";
pub const REMOTE_TERMINAL_BUFFER: &str = "terminal.buffer";
pub const REMOTE_TERMINAL_OUTPUT: &str = "terminal.output";
pub const REMOTE_TERMINAL_OUTPUT_ACK: &str = "terminal.output.ack";
pub const REMOTE_TERMINAL_INPUT: &str = "terminal.input";
pub const REMOTE_TERMINAL_INPUT_ACK: &str = "terminal.input.ack";
pub const REMOTE_TERMINAL_SIGNAL: &str = "terminal.signal";
pub const REMOTE_TERMINAL_RESIZE: &str = "terminal.resize";
pub const REMOTE_TERMINAL_VIEWPORT_CLAIM: &str = "terminal.viewport.claim";
pub const REMOTE_TERMINAL_VIEWPORT_RESIZE: &str = "terminal.viewport.resize";
pub const REMOTE_TERMINAL_VIEWPORT_RELEASE: &str = "terminal.viewport.release";
pub const REMOTE_TERMINAL_VIEWPORT_STATE: &str = "terminal.viewport.state";
pub const REMOTE_TERMINAL_VIEWPORT_SCROLL: &str = "terminal.viewport.scroll";
pub const REMOTE_TERMINAL_VIEWPORT_SCROLLED: &str = "terminal.viewport.scrolled";
pub const REMOTE_TERMINAL_UPLOAD_BLOB: &str = "terminal.upload.blob";
pub const REMOTE_TERMINAL_UPLOADED: &str = "terminal.uploaded";
pub const REMOTE_FILE_LIST: &str = "file.list";
pub const REMOTE_FILE_READ: &str = "file.read";
pub const REMOTE_FILE_WRITE: &str = "file.write";
pub const REMOTE_FILE_WRITTEN: &str = "file.written";
pub const REMOTE_FILE_RENAME: &str = "file.rename";
pub const REMOTE_FILE_RENAMED: &str = "file.renamed";
pub const REMOTE_FILE_DELETE: &str = "file.delete";
pub const REMOTE_FILE_DELETED: &str = "file.deleted";
pub const REMOTE_FILE_CREATE_DIRECTORY: &str = "file.createDirectory";
pub const REMOTE_FILE_DIRECTORY_CREATED: &str = "file.directoryCreated";
pub const REMOTE_FILE_COPY: &str = "file.copy";
pub const REMOTE_FILE_COPIED: &str = "file.copied";
pub const REMOTE_FILE_MOVE: &str = "file.move";
pub const REMOTE_FILE_MOVED: &str = "file.moved";
pub const REMOTE_FILE_WRITE_BYTES: &str = "file.writeBytes";
pub const REMOTE_FILE_BYTES_WRITTEN: &str = "file.bytesWritten";
/// Read a file's bytes binary-safely: the host publishes them to its blob store
/// and replies `file.blob {ticket}`; the controller fetches the blob over
/// iroh-blobs. Used for cross-device file copy (Save As).
pub const REMOTE_FILE_READ_BLOB: &str = "file.readBlob";
pub const REMOTE_FILE_BLOB: &str = "file.blob";
/// Write a file's bytes binary-safely: the controller publishes them and sends
/// `file.writeBlob {directory, name, ticket}`; the host fetches the blob and
/// writes it, replying `file.bytesWritten {path}`. The blob counterpart of
/// `file.writeBytes` (which uses base64) — content-addressed over iroh-blobs.
pub const REMOTE_FILE_WRITE_BLOB: &str = "file.writeBlob";
pub const REMOTE_GIT_STATUS: &str = "git.status";
/// Generic git mutation (`{op, projectPath, args}`) → refreshed `git.status`.
pub const REMOTE_GIT_INVOKE: &str = "git.invoke";
/// Generic git read query (`{op, projectPath, args}`) → `{op, result}`.
pub const REMOTE_GIT_READ: &str = "git.read";
pub const REMOTE_AI_STATS: &str = "ai.stats";
/// Full `AIHistoryProjectState` (incl. the snapshot) for a desktop controller —
/// distinct from `ai.stats`, which serves the derived baseline view to mobile.
pub const REMOTE_AI_STATE: &str = "ai.state";

/// Generic memory read query (`{op, projectId, …}`) → `{op, result}`. The host
/// runs the codux-memory engine against its own memory store. Ops: `summary`,
/// `manager`, `management`, `status`.
pub const REMOTE_MEMORY_READ: &str = "memory.read";
/// Reply to `memory.read`: `{op, result}` carrying the op's JSON snapshot.
pub const REMOTE_MEMORY_RESULT: &str = "memory.result";
/// Trigger memory extraction on the host with a controller-forwarded provider
/// config (`{config, outputLocale}`). The host runs the engine and replies
/// `memory.result` with the refreshed extraction status. The forwarded provider
/// config is used for the run and not persisted.
pub const REMOTE_MEMORY_EXTRACT: &str = "memory.extract";

/// Generic AI-session op (`{op, projectPath, …}`) → `{op, result}`. The host
/// runs the codux-ai-sessions engine against its own history. Ops: `detail`,
/// `rename`, `remove`, `fork`.
pub const REMOTE_AI_SESSION: &str = "ai.session";
/// Reply to `ai.session`: `{op, result}` carrying the op's JSON.
pub const REMOTE_AI_SESSION_RESULT: &str = "ai.session.result";

/// Request the host's saved SSH profiles (the host owns them — a remote
/// terminal's ssh is the host's). Reply carries `RemoteSshProfileSummary`s.
pub const REMOTE_SSH_LIST: &str = "ssh.list";
/// Reply to `ssh.list`: `{ profiles: [RemoteSshProfileSummary] }`.
pub const REMOTE_SSH_LIST_RESULT: &str = "ssh.list.result";
/// Add or update a saved SSH profile on the host. Payload is an
/// `SSHProfileUpsertRequest` shape; reply is a fresh `ssh.list.result`. Only the
/// desktop host owns SSH profiles — the headless agent rejects this.
pub const REMOTE_SSH_UPSERT: &str = "ssh.upsert";
/// Remove a saved SSH profile by id: `{ id }`. Reply is a fresh `ssh.list.result`.
pub const REMOTE_SSH_REMOVE: &str = "ssh.remove";

pub const REMOTE_TRANSPORT_IROH: &str = "iroh";
pub const REMOTE_TRANSPORT_ROLE_HOST: &str = "host";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteTransportKind {
    Iroh,
}

impl RemoteTransportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Iroh => REMOTE_TRANSPORT_IROH,
        }
    }
}

/// One AI conversation-history record sent over `ai.session` (op `list`).
/// Lean by design — every host produces this exact shape and every controller
/// (desktop or mobile) parses it, so the wire fields live here once.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAISessionSummary {
    pub id: String,
    pub title: String,
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Last-seen time (epoch seconds).
    pub time: f64,
    /// Total tokens (the "size" of the session).
    pub size: i64,
}

/// One live AI runtime session sent inside `ai.stats.currentSessions`.
/// This is distinct from `RemoteAISessionSummary`: history records come from
/// indexed CLI logs, while current sessions come from the host runtime.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAICurrentSession {
    #[serde(default, rename = "sessionId")]
    pub session_id: String,
    #[serde(
        default,
        rename = "terminalId",
        skip_serializing_if = "Option::is_none"
    )]
    pub terminal_id: Option<String>,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default, rename = "isRunning")]
    pub is_running: bool,
    #[serde(default, rename = "totalTokens")]
    pub total_tokens: i64,
    #[serde(default, rename = "cachedInputTokens")]
    pub cached_input_tokens: i64,
}

/// One saved SSH profile sent over `ssh.list`. The host owns the profiles
/// (a remote terminal's ssh is the host's anyway), so the host sends this list;
/// secrets are never included.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSshProfileSummary {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub credential: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTransportCandidate {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "nodeId")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "relayUrl")]
    pub relay_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_authentication: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteTransportPairingRequest {
    pub device_id: String,
    pub device_name: String,
    /// The requesting device's OS (`std::env::consts::OS`), if it reported one.
    /// Used to label the device type in the host's device list.
    pub platform: Option<String>,
    pub pairing_id: Option<String>,
    pub pairing_code: Option<String>,
    pub pairing_secret: Option<String>,
}

pub fn iroh_transport_candidate(
    url: impl Into<String>,
    node_id: impl Into<String>,
    relay_url: impl Into<String>,
) -> RemoteTransportCandidate {
    iroh_transport_candidate_with_ticket(url, node_id, relay_url, "")
}

pub fn iroh_transport_candidate_with_ticket(
    url: impl Into<String>,
    node_id: impl Into<String>,
    relay_url: impl Into<String>,
    ticket: impl Into<String>,
) -> RemoteTransportCandidate {
    iroh_transport_candidate_with_ticket_and_authentication(url, node_id, relay_url, ticket, "")
}

pub fn iroh_transport_candidate_with_ticket_and_authentication(
    url: impl Into<String>,
    node_id: impl Into<String>,
    relay_url: impl Into<String>,
    ticket: impl Into<String>,
    relay_authentication: impl Into<String>,
) -> RemoteTransportCandidate {
    let ticket = ticket.into();
    let relay_authentication = relay_authentication.into();
    RemoteTransportCandidate {
        kind: REMOTE_TRANSPORT_IROH.to_string(),
        role: Some(REMOTE_TRANSPORT_ROLE_HOST.to_string()),
        url: Some(url.into()),
        node_id: Some(node_id.into()),
        relay_url: Some(relay_url.into()),
        ticket: (!ticket.trim().is_empty()).then_some(ticket),
        relay_authentication: (!relay_authentication.trim().is_empty())
            .then_some(relay_authentication),
    }
}

/// One transport entry for a pairing QR / `codux://pair?payload=` URL. Encodes
/// the MINIMUM needed to dial — `nodeId` + `relayUrl` (+ relay auth) — and
/// deliberately omits the bulky iroh endpoint `ticket` (it ~doubles QR density
/// and hurts scan reliability); the host re-sends the full transport set on
/// `pairing.confirmed` once the controller connects. THE single definition of
/// the pairing-QR transport shape — shared by every Rust host so a format change
/// happens in one place, not once per host.
pub fn pairing_transport_entry(candidate: &RemoteTransportCandidate) -> Value {
    let mut item = serde_json::Map::new();
    item.insert("kind".to_string(), json!(candidate.kind));
    let non_empty = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    if let Some(node_id) = non_empty(&candidate.node_id) {
        item.insert("nodeId".to_string(), json!(node_id));
    }
    if let Some(relay_url) = non_empty(&candidate.relay_url) {
        item.insert("relayUrl".to_string(), json!(relay_url));
    }
    if let Some(authentication) = non_empty(&candidate.relay_authentication) {
        item.insert("relayAuthentication".to_string(), json!(authentication));
    }
    Value::Object(item)
}

/// The full pairing payload (`code` / `secret` / `pairingId` / `transports`)
/// embedded in the pairing QR and the `codux://pair?payload=` URL. Shared by the
/// desktop and headless agent hosts so the wire format lives in exactly one
/// place (the mobile client is Dart and parses the same shape).
pub fn pairing_payload(
    code: &str,
    secret: &str,
    pairing_id: &str,
    candidates: &[RemoteTransportCandidate],
) -> Value {
    json!({
        "code": code,
        "secret": secret,
        "pairingId": pairing_id,
        "transports": candidates
            .iter()
            .map(pairing_transport_entry)
            .collect::<Vec<_>>(),
    })
}

#[derive(Clone, Debug, Deserialize)]
pub struct RemoteEnvelope {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, rename = "deviceId")]
    pub device_id: Option<String>,
    #[serde(default, rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub seq: Option<i64>,
    #[serde(default)]
    pub payload: Value,
}

impl RemoteEnvelope {
    pub fn with_device_id(mut self, device_id: String) -> Self {
        self.device_id = Some(device_id);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteOutgoingEnvelope {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "deviceId")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    pub payload: Value,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRelayEnvelope {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub device_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
}

pub fn relay_hello_envelope(
    host_id: impl Into<String>,
    device_id: impl Into<String>,
    payload: Value,
    at: Option<i64>,
) -> RemoteRelayEnvelope {
    RemoteRelayEnvelope {
        kind: REMOTE_HELLO.to_string(),
        host_id: host_id.into(),
        device_id: device_id.into(),
        payload: Some(payload),
        at,
        ..RemoteRelayEnvelope::default()
    }
}

pub fn relay_error_envelope(
    host_id: impl Into<String>,
    device_id: impl Into<String>,
    error: impl Into<String>,
    at: Option<i64>,
) -> RemoteRelayEnvelope {
    RemoteRelayEnvelope {
        kind: REMOTE_RELAY_ERROR.to_string(),
        host_id: host_id.into(),
        device_id: device_id.into(),
        error: error.into(),
        at,
        ..RemoteRelayEnvelope::default()
    }
}

pub fn relay_blocks_message_type(kind: &str) -> bool {
    matches!(kind, REMOTE_FILE_WRITE)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteRelayPolicy {
    pub max_message_bytes: usize,
    pub burst_limit: usize,
    pub rate_window_secs: u64,
    pub ticket_ttl_secs: u64,
    pub ticket_max_entries: usize,
    pub ticket_payload_max_bytes: usize,
}

impl Default for RemoteRelayPolicy {
    fn default() -> Self {
        Self {
            max_message_bytes: REMOTE_RELAY_MAX_MESSAGE_BYTES,
            burst_limit: REMOTE_RELAY_BURST_LIMIT,
            rate_window_secs: REMOTE_RELAY_RATE_WINDOW_SECS,
            ticket_ttl_secs: REMOTE_RELAY_TICKET_TTL_SECS,
            ticket_max_entries: REMOTE_RELAY_TICKET_MAX_ENTRIES,
            ticket_payload_max_bytes: REMOTE_RELAY_TICKET_PAYLOAD_MAX_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RemoteRelayPeerWindow {
    pub window_started_at_ms: i64,
    pub count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteRelayDecision {
    Allow,
    Reject(&'static str),
}

impl RemoteRelayPolicy {
    pub fn validate_ticket_payload_size(&self, bytes: usize) -> RemoteRelayDecision {
        if bytes == 0 || bytes > self.ticket_payload_max_bytes {
            RemoteRelayDecision::Reject("ticket_payload_too_large")
        } else {
            RemoteRelayDecision::Allow
        }
    }

    pub fn validate_ticket_capacity(&self, active_entries: usize) -> RemoteRelayDecision {
        if active_entries >= self.ticket_max_entries {
            RemoteRelayDecision::Reject("too_many_active_tickets")
        } else {
            RemoteRelayDecision::Allow
        }
    }

    pub fn validate_message(
        &self,
        envelope: &RemoteRelayEnvelope,
        encoded_size: usize,
        peer_window: &mut RemoteRelayPeerWindow,
        now_ms: i64,
    ) -> RemoteRelayDecision {
        let rate_window_ms = (self.rate_window_secs as i64).saturating_mul(1000);
        if peer_window.window_started_at_ms > 0
            && now_ms.saturating_sub(peer_window.window_started_at_ms) < rate_window_ms
        {
            peer_window.count = peer_window.count.saturating_add(1);
        } else {
            peer_window.window_started_at_ms = now_ms;
            peer_window.count = 1;
        }
        if peer_window.count > self.burst_limit {
            return RemoteRelayDecision::Reject("rate_limited");
        }
        if encoded_size > self.max_message_bytes {
            return RemoteRelayDecision::Reject("message_too_large");
        }
        if relay_blocks_message_type(&envelope.kind) {
            return RemoteRelayDecision::Reject("upload_requires_p2p");
        }
        RemoteRelayDecision::Allow
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteResourceSubscriptionTarget {
    pub resource: String,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub baseline: bool,
}

impl RemoteResourceSubscriptionTarget {
    pub fn from_payload(session_id: Option<&str>, payload: &Value) -> Result<Self, String> {
        let resource = payload
            .get("resource")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Resource is required.".to_string())?;
        let project_id = payload
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let session_id = session_id
            .or_else(|| payload.get("sessionId").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        Ok(Self {
            resource: resource.to_string(),
            project_id,
            session_id,
            baseline: payload
                .get("baseline")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }
}

pub struct RemoteTerminalBufferWindow {
    pub data: String,
    pub screen_data: Option<String>,
    pub offset: usize,
    pub total_characters: usize,
    pub truncated: bool,
    pub output_seq: Option<i64>,
    pub request_id: Option<String>,
    pub tail: bool,
    pub has_previous: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteTerminalSubscriptionTarget {
    Project { project_id: String },
    Session { session_id: String },
}

impl RemoteTerminalSubscriptionTarget {
    pub fn from_payload(session_id: Option<&str>, payload: &Value) -> Result<Self, String> {
        let scope = payload
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("session");
        match scope {
            "project" => {
                let project_id = payload
                    .get("projectId")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Project id is required.".to_string())?;
                Ok(Self::Project {
                    project_id: project_id.to_string(),
                })
            }
            "session" => {
                let session_id = session_id
                    .or_else(|| payload.get("sessionId").and_then(Value::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Terminal session id is required.".to_string())?;
                Ok(Self::Session {
                    session_id: session_id.to_string(),
                })
            }
            _ => Err("Unsupported terminal subscription scope.".to_string()),
        }
    }

    pub fn baseline_requested(payload: &Value) -> bool {
        payload
            .get("baseline")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}

#[derive(Default)]
pub struct RemoteResourceSubscriptions {
    devices_by_resource: Mutex<HashMap<RemoteResourceSubscriptionKey, HashSet<String>>>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct RemoteResourceSubscriptionKey {
    resource: String,
    project_id: Option<String>,
    session_id: Option<String>,
}

impl RemoteResourceSubscriptionKey {
    fn new(resource: &str, project_id: Option<&str>, session_id: Option<&str>) -> Option<Self> {
        let resource = clean_subscription_part(resource)?;
        Some(Self {
            resource,
            project_id: project_id.and_then(clean_subscription_part),
            session_id: session_id.and_then(clean_subscription_part),
        })
    }
}

impl RemoteResourceSubscriptions {
    pub fn subscribe(
        &self,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
        device_id: &str,
    ) {
        let Some(key) = RemoteResourceSubscriptionKey::new(resource, project_id, session_id) else {
            return;
        };
        let Some(device_id) = clean_subscription_part(device_id) else {
            return;
        };
        if let Ok(mut subscriptions) = self.devices_by_resource.lock() {
            subscriptions.entry(key).or_default().insert(device_id);
        }
    }

    pub fn unsubscribe(
        &self,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
        device_id: &str,
    ) {
        let Some(key) = RemoteResourceSubscriptionKey::new(resource, project_id, session_id) else {
            return;
        };
        let Some(device_id) = clean_subscription_part(device_id) else {
            return;
        };
        if let Ok(mut subscriptions) = self.devices_by_resource.lock() {
            if let Some(devices) = subscriptions.get_mut(&key) {
                devices.remove(&device_id);
            }
            subscriptions.retain(|_, devices| !devices.is_empty());
        }
    }

    pub fn remove_device(&self, device_id: &str) {
        let Some(device_id) = clean_subscription_part(device_id) else {
            return;
        };
        if let Ok(mut subscriptions) = self.devices_by_resource.lock() {
            for devices in subscriptions.values_mut() {
                devices.remove(&device_id);
            }
            subscriptions.retain(|_, devices| !devices.is_empty());
        }
    }

    pub fn remove_session(&self, session_id: &str) {
        let Some(session_id) = clean_subscription_part(session_id) else {
            return;
        };
        if let Ok(mut subscriptions) = self.devices_by_resource.lock() {
            subscriptions.retain(|key, _| key.session_id.as_deref() != Some(session_id.as_str()));
        }
    }

    pub fn remove_project(&self, project_id: &str) {
        let Some(project_id) = clean_subscription_part(project_id) else {
            return;
        };
        if let Ok(mut subscriptions) = self.devices_by_resource.lock() {
            subscriptions.retain(|key, _| key.project_id.as_deref() != Some(project_id.as_str()));
        }
    }

    pub fn clear(&self) {
        if let Ok(mut subscriptions) = self.devices_by_resource.lock() {
            subscriptions.clear();
        }
    }

    pub fn devices_for(
        &self,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
    ) -> HashSet<String> {
        let Some(resource) = clean_subscription_part(resource) else {
            return HashSet::new();
        };
        let project_id = project_id.and_then(clean_subscription_part);
        let session_id = session_id.and_then(clean_subscription_part);
        let Ok(subscriptions) = self.devices_by_resource.lock() else {
            return HashSet::new();
        };
        let mut devices = HashSet::new();
        for (key, subscribed_devices) in subscriptions.iter() {
            if key.resource != resource {
                continue;
            }
            if key.project_id.as_deref().is_some()
                && key.project_id.as_deref() != project_id.as_deref()
            {
                continue;
            }
            if key.session_id.as_deref().is_some()
                && key.session_id.as_deref() != session_id.as_deref()
            {
                continue;
            }
            devices.extend(subscribed_devices.iter().cloned());
        }
        devices
    }
}

#[derive(Default)]
pub struct RemoteTerminalSubscriptions {
    resources: RemoteResourceSubscriptions,
}

impl RemoteTerminalSubscriptions {
    pub fn add_session_viewer(&self, session_id: &str, device_id: &str) {
        self.resources
            .subscribe(REMOTE_RESOURCE_TERMINALS, None, Some(session_id), device_id);
    }

    pub fn add_project_subscriber(&self, project_id: &str, device_id: &str) {
        self.resources
            .subscribe(REMOTE_RESOURCE_TERMINALS, Some(project_id), None, device_id);
    }

    pub fn remove_session_viewer(&self, session_id: &str, device_id: &str) {
        self.resources
            .unsubscribe(REMOTE_RESOURCE_TERMINALS, None, Some(session_id), device_id);
    }

    pub fn remove_project_subscriber(&self, project_id: &str, device_id: &str) {
        self.resources
            .unsubscribe(REMOTE_RESOURCE_TERMINALS, Some(project_id), None, device_id);
    }

    pub fn remove_project_session_viewers<'a>(
        &self,
        session_ids: impl IntoIterator<Item = &'a str>,
        device_id: &str,
    ) {
        for session_id in session_ids {
            self.remove_session_viewer(session_id, device_id);
        }
    }

    pub fn remove_device(&self, device_id: &str) {
        self.resources.remove_device(device_id);
    }

    pub fn remove_session(&self, session_id: &str) {
        self.resources.remove_session(session_id);
    }

    pub fn clear(&self) {
        self.resources.clear();
    }

    pub fn viewers_for_session(
        &self,
        session_id: &str,
        project_id: Option<&str>,
    ) -> HashSet<String> {
        self.resources
            .devices_for(REMOTE_RESOURCE_TERMINALS, project_id, Some(session_id))
    }
}

fn clean_subscription_part(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub fn host_capabilities() -> Value {
    json!({
        "domains": {
            "project": true,
            "terminal": true,
            "worktree": true,
            "file": true,
            "git": true,
            "aiStats": true,
        },
        "terminalBuffer": {
            "chunking": true,
            "maxChars": REMOTE_TERMINAL_BUFFER_MAX_CHARS,
            "chunkChars": REMOTE_TERMINAL_BUFFER_CHUNK_CHARS,
            "requestId": true,
            "screenData": true,
        },
        "terminalOutput": {
            "sequence": true,
            "screenData": true,
        },
        "terminalViewport": {
            "ownership": true,
            "state": true,
            "scroll": true,
        },
    })
}

pub fn terminal_buffer_payloads(
    window: &RemoteTerminalBufferWindow,
    output_seq: i64,
    chunk_chars: Option<usize>,
) -> Vec<Value> {
    let max_chunk_chars = chunk_chars.unwrap_or(REMOTE_TERMINAL_BUFFER_CHUNK_CHARS);
    let total_chars = window.data.chars().count();
    if total_chars <= max_chunk_chars {
        return vec![terminal_buffer_payload(
            window,
            output_seq,
            window.data.clone(),
            window.offset,
            None,
        )];
    }

    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let chunks = split_text_chunks(&window.data, max_chunk_chars);
    let chunk_count = chunks.len();
    let mut offset = window.offset;
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, data)| {
            let payload = terminal_buffer_payload(
                window,
                output_seq,
                data.clone(),
                offset,
                Some((&snapshot_id, index, chunk_count)),
            );
            offset += data.chars().count();
            payload
        })
        .collect()
}

fn terminal_buffer_payload(
    window: &RemoteTerminalBufferWindow,
    output_seq: i64,
    data: String,
    offset: usize,
    chunk: Option<(&str, usize, usize)>,
) -> Value {
    let mut payload = json!({
        "data": data,
        "buffer": true,
        "offset": offset,
        "startOffset": window.offset,
        "bufferLength": window.total_characters,
        "truncated": window.truncated,
        "outputSeq": output_seq,
        "tail": window.tail,
        "hasPrevious": window.has_previous,
    });
    if let Some(request_id) = window.request_id.as_deref() {
        payload["requestId"] = json!(request_id);
    }
    if chunk
        .map(|(_, chunk_index, _)| chunk_index == 0)
        .unwrap_or(true)
    {
        if let Some(screen_data) = window.screen_data.as_deref() {
            payload["screenData"] = json!(screen_data);
        }
    }
    if let Some((snapshot_id, chunk_index, chunk_count)) = chunk {
        payload["snapshotId"] = json!(snapshot_id);
        payload["chunkIndex"] = json!(chunk_index);
        payload["chunkCount"] = json!(chunk_count);
        payload["chunked"] = json!(true);
    }
    payload
}

pub fn terminal_live_output_payload(data: String, buffer_length: usize, output_seq: i64) -> Value {
    json!({
        "data": data,
        "bufferLength": buffer_length,
        "outputSeq": output_seq,
    })
}

fn split_text_chunks(text: &str, chunk_chars: usize) -> Vec<String> {
    let chunk_chars = chunk_chars.max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0;
    for ch in text.chars() {
        current.push(ch);
        current_chars += 1;
        if current_chars >= chunk_chars {
            chunks.push(std::mem::take(&mut current));
            current_chars = 0;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_buffer_payloads_are_chunked_on_character_boundaries() {
        let window = RemoteTerminalBufferWindow {
            data: "ab你好cd".to_string(),
            screen_data: Some("\x1b[Hscreen".to_string()),
            offset: 10,
            total_characters: 16,
            truncated: true,
            output_seq: None,
            request_id: Some("request-1".to_string()),
            tail: true,
            has_previous: true,
        };

        let payloads = terminal_buffer_payloads(&window, 7, Some(2));

        assert_eq!(payloads.len(), 3);
        let snapshot_id = payloads[0]["snapshotId"]
            .as_str()
            .expect("snapshot id")
            .to_string();
        assert!(!snapshot_id.is_empty());
        let data = payloads
            .iter()
            .map(|payload| payload["data"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(data, vec!["ab", "你好", "cd"]);
        assert_eq!(payloads[0]["offset"], 10);
        assert_eq!(payloads[1]["offset"], 12);
        assert_eq!(payloads[2]["offset"], 14);
        for (index, payload) in payloads.iter().enumerate() {
            assert_eq!(payload["snapshotId"], snapshot_id);
            assert_eq!(payload["chunkIndex"], index);
            assert_eq!(payload["chunkCount"], 3);
            assert_eq!(payload["startOffset"], 10);
            assert_eq!(payload["bufferLength"], 16);
            assert_eq!(payload["outputSeq"], 7);
            assert_eq!(payload["truncated"], true);
            assert_eq!(payload["requestId"], "request-1");
            assert_eq!(payload["tail"], true);
            assert_eq!(payload["hasPrevious"], true);
        }
        assert_eq!(payloads[0]["screenData"], "\x1b[Hscreen");
        assert!(payloads[1].get("screenData").is_none());
        assert!(payloads[2].get("screenData").is_none());
    }

    #[test]
    fn host_capabilities_advertise_runtime_domains() {
        let capabilities = host_capabilities();
        assert_eq!(capabilities["domains"]["project"], true);
        assert_eq!(capabilities["domains"]["terminal"], true);
        assert_eq!(capabilities["domains"]["worktree"], true);
        assert_eq!(capabilities["domains"]["file"], true);
        assert_eq!(capabilities["domains"]["git"], true);
        assert_eq!(capabilities["domains"]["aiStats"], true);
        assert_eq!(capabilities["terminalBuffer"]["chunking"], true);
        assert_eq!(capabilities["terminalBuffer"]["requestId"], true);
        assert_eq!(capabilities["terminalBuffer"]["screenData"], true);
        assert_eq!(capabilities["terminalOutput"]["sequence"], true);
        assert_eq!(capabilities["terminalOutput"]["screenData"], true);
        assert_eq!(capabilities["terminalViewport"]["ownership"], true);
    }

    #[test]
    fn terminal_live_output_payload_carries_data_only() {
        // Live output is a pure byte stream now — no screen keyframe. The desktop
        // emulator keeps its own scrollback from `data`; replaying a whole-screen
        // keyframe on top of it duplicated the screen (notably on resize bursts).
        let payload = terminal_live_output_payload("raw".to_string(), 128, 9);

        assert_eq!(payload["data"], "raw");
        assert_eq!(payload["bufferLength"], 128);
        assert_eq!(payload["outputSeq"], 9);
        assert!(payload.get("screenData").is_none());
        assert!(payload.get("buffer").is_none());
    }

    #[test]
    fn relay_envelope_uses_camel_case_transport_shape() {
        let envelope =
            relay_hello_envelope("host-1", "device-1", json!({ "role": "client" }), Some(42));
        let value = serde_json::to_value(envelope).expect("serialize relay envelope");

        assert_eq!(value["type"], REMOTE_HELLO);
        assert_eq!(value["hostId"], "host-1");
        assert_eq!(value["deviceId"], "device-1");
        assert_eq!(value["payload"]["role"], "client");
        assert_eq!(value["at"], 42);
        assert!(value.get("error").is_none());
    }

    #[test]
    fn relay_policy_validates_tickets_and_messages() {
        let policy = RemoteRelayPolicy {
            max_message_bytes: 16,
            burst_limit: 2,
            rate_window_secs: 10,
            ticket_ttl_secs: 60,
            ticket_max_entries: 1,
            ticket_payload_max_bytes: 8,
        };
        assert_eq!(
            policy.validate_ticket_payload_size(0),
            RemoteRelayDecision::Reject("ticket_payload_too_large")
        );
        assert_eq!(
            policy.validate_ticket_payload_size(8),
            RemoteRelayDecision::Allow
        );
        assert_eq!(
            policy.validate_ticket_capacity(1),
            RemoteRelayDecision::Reject("too_many_active_tickets")
        );

        let mut window = RemoteRelayPeerWindow::default();
        let envelope = RemoteRelayEnvelope {
            kind: REMOTE_TERMINAL_OUTPUT.to_string(),
            ..RemoteRelayEnvelope::default()
        };
        assert_eq!(
            policy.validate_message(&envelope, 8, &mut window, 1_000),
            RemoteRelayDecision::Allow
        );
        assert_eq!(
            policy.validate_message(&envelope, 8, &mut window, 1_001),
            RemoteRelayDecision::Allow
        );
        assert_eq!(
            policy.validate_message(&envelope, 8, &mut window, 1_002),
            RemoteRelayDecision::Reject("rate_limited")
        );
        assert_eq!(
            policy.validate_message(&envelope, 17, &mut RemoteRelayPeerWindow::default(), 2_000),
            RemoteRelayDecision::Reject("message_too_large")
        );
        let write = RemoteRelayEnvelope {
            kind: REMOTE_FILE_WRITE.to_string(),
            ..RemoteRelayEnvelope::default()
        };
        assert_eq!(
            policy.validate_message(&write, 8, &mut RemoteRelayPeerWindow::default(), 2_000),
            RemoteRelayDecision::Reject("upload_requires_p2p")
        );
    }

    #[test]
    fn relay_policy_resets_rate_window_after_timeout() {
        let policy = RemoteRelayPolicy {
            max_message_bytes: 64,
            burst_limit: 1,
            rate_window_secs: 1,
            ticket_ttl_secs: 60,
            ticket_max_entries: 16,
            ticket_payload_max_bytes: 1024,
        };
        let envelope = RemoteRelayEnvelope {
            kind: REMOTE_TERMINAL_OUTPUT.to_string(),
            ..RemoteRelayEnvelope::default()
        };
        let mut window = RemoteRelayPeerWindow::default();

        assert_eq!(
            policy.validate_message(&envelope, 8, &mut window, 1_000),
            RemoteRelayDecision::Allow
        );
        assert_eq!(
            policy.validate_message(&envelope, 8, &mut window, 1_100),
            RemoteRelayDecision::Reject("rate_limited")
        );
        assert_eq!(
            policy.validate_message(&envelope, 8, &mut window, 2_100),
            RemoteRelayDecision::Allow
        );
    }

    #[test]
    fn resource_subscription_target_trims_and_rejects_empty_resource() {
        let target = RemoteResourceSubscriptionTarget::from_payload(
            Some(" session-1 "),
            &json!({
                "resource": " git.status ",
                "projectId": " project-1 ",
                "baseline": true,
            }),
        )
        .unwrap();

        assert_eq!(target.resource, REMOTE_RESOURCE_GIT_STATUS);
        assert_eq!(target.project_id.as_deref(), Some("project-1"));
        assert_eq!(target.session_id.as_deref(), Some("session-1"));
        assert!(target.baseline);

        assert!(
            RemoteResourceSubscriptionTarget::from_payload(None, &json!({ "resource": " " }),)
                .is_err()
        );
    }

    #[test]
    fn terminal_subscription_target_parses_project_scope() {
        let target = RemoteTerminalSubscriptionTarget::from_payload(
            None,
            &json!({ "scope": "project", "projectId": "project-1", "baseline": true }),
        )
        .unwrap();

        assert_eq!(
            target,
            RemoteTerminalSubscriptionTarget::Project {
                project_id: "project-1".to_string()
            }
        );
        assert!(RemoteTerminalSubscriptionTarget::baseline_requested(
            &json!({ "baseline": true })
        ));
    }

    #[test]
    fn terminal_subscriptions_merge_session_and_project_viewers() {
        let subscriptions = RemoteTerminalSubscriptions::default();
        subscriptions.add_session_viewer("session-1", "device-a");
        subscriptions.add_project_subscriber("project-1", "device-b");

        let viewers = subscriptions.viewers_for_session("session-1", Some("project-1"));

        assert!(viewers.contains("device-a"));
        assert!(viewers.contains("device-b"));
    }

    #[test]
    fn resource_subscriptions_match_project_scoped_resources() {
        let subscriptions = RemoteResourceSubscriptions::default();
        subscriptions.subscribe(
            REMOTE_RESOURCE_GIT_STATUS,
            Some("project-1"),
            None,
            "device-a",
        );
        subscriptions.subscribe(REMOTE_RESOURCE_GIT_STATUS, None, None, "device-b");
        subscriptions.subscribe(
            REMOTE_RESOURCE_WORKTREES,
            Some("project-1"),
            None,
            "device-c",
        );

        let git_devices =
            subscriptions.devices_for(REMOTE_RESOURCE_GIT_STATUS, Some("project-1"), None);
        assert!(git_devices.contains("device-a"));
        assert!(git_devices.contains("device-b"));
        assert!(!git_devices.contains("device-c"));

        let other_git_devices =
            subscriptions.devices_for(REMOTE_RESOURCE_GIT_STATUS, Some("project-2"), None);
        assert!(!other_git_devices.contains("device-a"));
        assert!(other_git_devices.contains("device-b"));

        subscriptions.unsubscribe(REMOTE_RESOURCE_GIT_STATUS, None, None, "device-b");
        let git_devices =
            subscriptions.devices_for(REMOTE_RESOURCE_GIT_STATUS, Some("project-1"), None);
        assert!(git_devices.contains("device-a"));
        assert!(!git_devices.contains("device-b"));
    }
}
