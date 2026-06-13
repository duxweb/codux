use crate::{
    config::ServerConfig,
    stats::StatsRecorder,
    store::{Device, Host, Store, StoreError, pairing_code, token_url},
};
use anyhow::Context;
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use codux_protocol::{
    REMOTE_DEVICE_CONNECTED, REMOTE_DEVICE_DISCONNECTED, REMOTE_DEVICE_INFO, REMOTE_HOST_OFFLINE,
    REMOTE_PAIRING_CONFIRMED, REMOTE_PAIRING_REJECTED, REMOTE_PAIRING_REQUEST,
    REMOTE_PROTOCOL_VERSION, REMOTE_TRANSPORT_ROLE_HOST, REMOTE_TRANSPORT_WEBRTC,
    REMOTE_TRANSPORT_WEBSOCKET_RELAY, RemoteRelayDecision, RemoteRelayEnvelope,
    RemoteRelayPeerWindow, RemoteRelayPolicy, relay_error_envelope, relay_hello_envelope,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

#[derive(Debug)]
pub struct Hub {
    store: Mutex<Store>,
    peers: tokio::sync::Mutex<PeerRegistry>,
    stats: Option<StatsRecorder>,
    config: ServerConfig,
    // Per-host generation, bumped on every (dis)connect. A host disconnect
    // schedules a grace-delayed `host.offline` to that host's clients; if the
    // host reconnects within the grace (a transient blip) the generation moves
    // and the pending notification is skipped, so blips never reset clients.
    host_offline_gen: tokio::sync::Mutex<HashMap<String, u64>>,
}

/// How long the relay waits after a host's socket drops before telling that
/// host's clients it is offline. Long enough to ride out a relay blip /
/// reconnect, short enough that a genuine quit/crash is reflected promptly.
const HOST_OFFLINE_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Default, Debug)]
struct PeerRegistry {
    hosts: HashMap<String, PeerSender>,
    clients: HashMap<String, PeerSender>,
    tickets: HashMap<String, TicketEntry>,
    tickets_by_code: HashMap<String, String>,
}

#[derive(Clone, Debug)]
struct PeerSender {
    host_id: String,
    tx: mpsc::UnboundedSender<RemoteRelayEnvelope>,
}

#[derive(Debug)]
struct TicketEntry {
    payload: Value,
    expires_at: i64,
    code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerRole {
    Host,
    Client,
}

impl PeerRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Client => "client",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PeerSnapshot {
    pub role: PeerRole,
    pub host_id: String,
    pub device_id: String,
    pub stateless: bool,
}

#[derive(Debug)]
struct Peer {
    snapshot: PeerSnapshot,
    relay_window: RemoteRelayPeerWindow,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterHostRequest {
    host_id: Option<String>,
    name: Option<String>,
    token: Option<String>,
    public_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePairingRequest {
    host_id: String,
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaimPairingRequest {
    code: String,
    secret: String,
    name: Option<String>,
    public_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairingStatusRequest {
    code: String,
    secret: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmPairingRequest {
    host_id: String,
    token: String,
    pairing_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RejectPairingRequest {
    host_id: String,
    token: String,
    pairing_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevokeDeviceRequest {
    host_id: String,
    token: String,
    device_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostQuery {
    host_id: String,
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientQuery {
    host_id: Option<String>,
    device_id: String,
    token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TicketResponse {
    ticket: String,
    expires_at: i64,
}

impl Hub {
    pub fn open(config: ServerConfig) -> anyhow::Result<Self> {
        let store = Store::open(&config.db_path)?;
        let stats = if config.stats_enabled {
            Some(
                StatsRecorder::open(&config.stats_path, config.stats_flush_interval)
                    .with_context(|| format!("open stats {}", config.stats_path.display()))?,
            )
        } else {
            None
        };
        Ok(Self {
            store: Mutex::new(store),
            peers: tokio::sync::Mutex::new(PeerRegistry::default()),
            stats,
            config,
            host_offline_gen: tokio::sync::Mutex::new(HashMap::new()),
        })
    }

    #[cfg(test)]
    fn in_memory() -> anyhow::Result<Self> {
        Ok(Self {
            store: Mutex::new(Store::in_memory()?),
            peers: tokio::sync::Mutex::new(PeerRegistry::default()),
            stats: None,
            config: ServerConfig {
                addr: "127.0.0.1:0".parse().unwrap(),
                db_path: "memory".into(),
                stats_enabled: false,
                stats_path: "memory.stats".into(),
                stats_flush_interval: std::time::Duration::from_secs(10),
                pairing_ttl: std::time::Duration::from_secs(300),
                shutdown_timeout: std::time::Duration::from_secs(3),
                read_header_timeout: std::time::Duration::from_secs(10),
                config_loaded_from: None,
            },
            host_offline_gen: tokio::sync::Mutex::new(HashMap::new()),
        })
    }

    pub fn router(self: Arc<Self>) -> Router {
        Router::new()
            .route("/healthz", get(health))
            .route("/api/hosts/register", post(register_host))
            .route("/api/pairings", post(create_pairing))
            .route("/api/pairings/claim", post(claim_pairing))
            .route("/api/pairings/status", post(pairing_status))
            .route("/api/pairings/confirm", post(confirm_pairing))
            .route("/api/pairings/reject", post(reject_pairing))
            .route("/api/hosts/{host_id}/devices", get(list_devices))
            .route("/api/devices/revoke", post(revoke_device))
            .route("/ws/host", get(legacy_host_socket))
            .route("/ws/client", get(legacy_client_socket))
            .route("/v3/healthz", get(v3_health))
            .route("/v3/api/tickets", post(create_ticket))
            .route("/v3/api/tickets/{ticket}", get(get_ticket))
            .route("/v3/api/pairings/code/{code}", get(get_pairing_code))
            .route("/v3/ws/host", get(v3_host_socket))
            .route("/v3/ws/client", get(v3_client_socket))
            .layer(CorsLayer::permissive())
            .with_state(self)
    }

    pub async fn close(&self) {
        if let Some(stats) = &self.stats {
            stats.close();
        }
        let mut peers = self.peers.lock().await;
        peers.hosts.clear();
        peers.clients.clear();
    }

    fn with_store<T>(&self, f: impl FnOnce(&mut Store) -> anyhow::Result<T>) -> anyhow::Result<T> {
        let mut store = self.store.lock().expect("store lock");
        f(&mut store)
    }

    fn authenticate_host(&self, host_id: &str, token: &str) -> Option<Host> {
        if host_id.trim().is_empty() || token.trim().is_empty() {
            return None;
        }
        self.with_store(|store| Ok(store.host_by_token(token)?))
            .ok()
            .filter(|host| host.id == host_id)
    }

    fn authenticate_device(&self, device_id: &str, token: &str) -> Option<Device> {
        if device_id.trim().is_empty() || token.trim().is_empty() {
            return None;
        }
        self.with_store(|store| Ok(store.device_by_token(token)?))
            .ok()
            .filter(|device| device.id == device_id && device.revoked_at.is_none())
    }
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true }))
}

async fn v3_health() -> impl IntoResponse {
    Json(json!({ "ok": true, "protocolVersion": REMOTE_PROTOCOL_VERSION }))
}

async fn register_host(
    State(hub): State<Arc<Hub>>,
    Json(request): Json<RegisterHostRequest>,
) -> Response {
    match hub.with_store(|store| {
        Ok(store.upsert_host(
            request.host_id,
            request.name,
            request.token,
            request.public_key,
        )?)
    }) {
        Ok(host) => json_ok(json!({ "hostId": host.id, "token": host.token })),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn create_pairing(
    State(hub): State<Arc<Hub>>,
    headers: HeaderMap,
    Json(request): Json<CreatePairingRequest>,
) -> Response {
    if hub
        .authenticate_host(&request.host_id, &request.token)
        .is_none()
    {
        return json_error(StatusCode::UNAUTHORIZED, "invalid host token");
    }

    match hub.with_store(|store| {
        let secret = token_url(24);
        let code = pairing_code();
        let ttl_ms = hub.config.pairing_ttl.as_millis() as i64;
        let pairing = store.create_pairing(request.host_id.clone(), code, secret, ttl_ms)?;
        let host = store.host_by_id(&pairing.host_id)?;
        Ok((pairing, host))
    }) {
        Ok((pairing, host)) => {
            let crypto_version = if host.public_key.is_empty() { 0 } else { 1 };
            let server_url = public_base_url(&headers);
            let payload = json!({
                "code": pairing.code,
                "secret": pairing.secret,
                "pairingId": pairing.id,
                "hostName": host.name,
                "hostPublicKey": host.public_key,
                "cryptoVersion": crypto_version,
                "protocolVersion": REMOTE_PROTOCOL_VERSION,
                "transports": [
                    { "kind": REMOTE_TRANSPORT_WEBSOCKET_RELAY, "role": REMOTE_TRANSPORT_ROLE_HOST, "url": server_url },
                    { "kind": REMOTE_TRANSPORT_WEBRTC, "role": REMOTE_TRANSPORT_ROLE_HOST, "url": server_url, "iceServers": [
                        { "urls": ["stun:stun.miwifi.com:3478", "stun:stun.l.google.com:19302"] }
                    ] }
                ]
            });
            let qr_payload =
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap_or_default());
            json_ok(json!({
                "pairingId": pairing.id,
                "code": pairing.code,
                "secret": pairing.secret,
                "hostName": host.name,
                "hostPublicKey": host.public_key,
                "cryptoVersion": crypto_version,
                "expiresAt": pairing.expires_at,
                "qrPayload": qr_payload,
            }))
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn claim_pairing(
    State(hub): State<Arc<Hub>>,
    Json(request): Json<ClaimPairingRequest>,
) -> Response {
    let code = request.code.trim().to_uppercase();
    match hub.with_store(|store| {
        let pairing = store.pairing_by_code(&code)?;
        if pairing.secret != request.secret || pairing.expires_at < crate::store::now_millis() {
            return Err(StoreError::NotFound.into());
        }
        let name = request.name.unwrap_or_else(|| "Mobile Device".into());
        let public_key = request.public_key.unwrap_or_default();
        store.claim_pairing(&pairing.id, &name, &public_key)?;
        Ok(store.pairing_by_id(&pairing.id)?)
    }) {
        Ok(pairing) => {
            hub.send_to_host(
                &pairing.host_id,
                RemoteRelayEnvelope {
                    kind: REMOTE_PAIRING_REQUEST.into(),
                    host_id: pairing.host_id.clone(),
                    payload: Some(json!({
                        "pairingId": pairing.id,
                        "code": pairing.code,
                        "deviceName": pairing.device_name,
                        "devicePublicKey": pairing.device_public_key,
                    })),
                    at: Some(crate::store::now_millis()),
                    ..RemoteRelayEnvelope::default()
                },
            )
            .await;
            json_ok(json!({
                "pairingId": pairing.id,
                "hostId": pairing.host_id,
                "status": "claimed"
            }))
        }
        Err(_) => json_error(StatusCode::NOT_FOUND, "pairing not found or expired"),
    }
}

async fn pairing_status(
    State(hub): State<Arc<Hub>>,
    Json(request): Json<PairingStatusRequest>,
) -> Response {
    match hub.with_store(|store| {
        let pairing = store.pairing_by_code(&request.code.trim().to_uppercase())?;
        if pairing.secret != request.secret || pairing.expires_at < crate::store::now_millis() {
            return Err(StoreError::NotFound.into());
        }
        let host = store.host_by_id(&pairing.host_id)?;
        let device = pairing
            .device_id
            .as_deref()
            .and_then(|id| store.device_by_id(id).ok())
            .filter(|device| device.revoked_at.is_none());
        Ok((pairing, host, device))
    }) {
        Ok((pairing, host, device)) => json_ok(json!({
            "status": pairing.status,
            "pairingId": pairing.id,
            "hostId": pairing.host_id,
            "hostName": host.name,
            "hostPublicKey": host.public_key,
            "cryptoVersion": if host.public_key.is_empty() { 0 } else { 1 },
            "code": pairing.code,
            "deviceName": pairing.device_name,
            "devicePublicKey": pairing.device_public_key,
            "deviceId": device.as_ref().map(|device| device.id.clone()).unwrap_or_default(),
            "token": device.as_ref().map(|device| device.token.clone()).unwrap_or_default(),
        })),
        Err(_) => json_error(StatusCode::NOT_FOUND, "pairing not found or expired"),
    }
}

async fn confirm_pairing(
    State(hub): State<Arc<Hub>>,
    Json(request): Json<ConfirmPairingRequest>,
) -> Response {
    if hub
        .authenticate_host(&request.host_id, &request.token)
        .is_none()
    {
        return json_error(StatusCode::UNAUTHORIZED, "invalid host token");
    }
    match hub.with_store(|store| {
        let pairing = store.pairing_by_id(&request.pairing_id)?;
        if pairing.host_id != request.host_id || pairing.status != "claimed" {
            return Err(StoreError::NotFound.into());
        }
        Ok(store.confirm_pairing(&pairing)?)
    }) {
        Ok(device) => {
            let host_id = request.host_id.clone();
            hub.send_to_host(
                &host_id,
                RemoteRelayEnvelope {
                    kind: REMOTE_PAIRING_CONFIRMED.into(),
                    host_id: host_id.clone(),
                    device_id: device.id.clone(),
                    payload: Some(json!({ "deviceId": device.id, "deviceName": device.name })),
                    at: Some(crate::store::now_millis()),
                    ..RemoteRelayEnvelope::default()
                },
            )
            .await;
            json_ok(
                json!({ "deviceId": device.id, "hostId": device.host_id, "token": device.token }),
            )
        }
        Err(_) => json_error(StatusCode::CONFLICT, "pairing is not claimed"),
    }
}

async fn reject_pairing(
    State(hub): State<Arc<Hub>>,
    Json(request): Json<RejectPairingRequest>,
) -> Response {
    if hub
        .authenticate_host(&request.host_id, &request.token)
        .is_none()
    {
        return json_error(StatusCode::UNAUTHORIZED, "invalid host token");
    }
    match hub.with_store(|store| {
        let pairing = store.pairing_by_id(&request.pairing_id)?;
        if pairing.host_id != request.host_id {
            return Err(StoreError::NotFound.into());
        }
        store.reject_pairing(&request.host_id, &request.pairing_id)?;
        Ok(pairing)
    }) {
        Ok(pairing) => {
            let host_id = request.host_id.clone();
            hub.send_to_host(
                &host_id,
                RemoteRelayEnvelope {
                    kind: REMOTE_PAIRING_REJECTED.into(),
                    host_id: host_id.clone(),
                    payload: Some(
                        json!({ "pairingId": pairing.id, "deviceName": pairing.device_name }),
                    ),
                    at: Some(crate::store::now_millis()),
                    ..RemoteRelayEnvelope::default()
                },
            )
            .await;
            json_ok(json!({ "ok": true }))
        }
        Err(_) => json_error(StatusCode::CONFLICT, "pairing is not pending or claimed"),
    }
}

async fn list_devices(
    State(hub): State<Arc<Hub>>,
    Path(host_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let token = query
        .get("token")
        .cloned()
        .or_else(|| bearer_token(&headers))
        .unwrap_or_default();
    if hub.authenticate_host(&host_id, &token).is_none() {
        return json_error(StatusCode::UNAUTHORIZED, "invalid host token");
    }
    match hub.with_store(|store| Ok(store.devices_for_host(&host_id)?)) {
        Ok(mut devices) => {
            let online = hub.online_device_ids().await;
            for device in &mut devices {
                device.online = online.contains_key(&device.id);
            }
            json_ok(json!({ "devices": devices }))
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn revoke_device(
    State(hub): State<Arc<Hub>>,
    Json(request): Json<RevokeDeviceRequest>,
) -> Response {
    if hub
        .authenticate_host(&request.host_id, &request.token)
        .is_none()
    {
        return json_error(StatusCode::UNAUTHORIZED, "invalid host token");
    }
    match hub.with_store(|store| Ok(store.revoke_device(&request.host_id, &request.device_id)?)) {
        Ok(()) | Err(_) => {
            hub.send_to_client(
                &request.device_id,
                relay_error_envelope(
                    &request.host_id,
                    &request.device_id,
                    "device revoked",
                    Some(crate::store::now_millis()),
                ),
            )
            .await;
            json_ok(json!({ "ok": true }))
        }
    }
}

async fn create_ticket(State(hub): State<Arc<Hub>>, Json(payload): Json<Value>) -> Response {
    let Ok(data) = serde_json::to_vec(&payload) else {
        return json_error(StatusCode::BAD_REQUEST, "invalid_json");
    };
    let policy = RemoteRelayPolicy::default();
    let RemoteRelayDecision::Allow = policy.validate_ticket_payload_size(data.len()) else {
        return json_error(StatusCode::PAYLOAD_TOO_LARGE, "ticket_payload_too_large");
    };
    let mut peers = hub.peers.lock().await;
    peers.prune_tickets();
    if matches!(
        policy.validate_ticket_capacity(peers.tickets.len()),
        RemoteRelayDecision::Reject(_)
    ) {
        return json_error(StatusCode::TOO_MANY_REQUESTS, "too_many_active_tickets");
    }
    let ticket = token_url(12);
    let expires_at =
        crate::store::now_millis() + (policy.ticket_ttl_secs as i64).saturating_mul(1000);
    peers.insert_ticket(ticket.clone(), payload, expires_at);
    json_ok(json!(TicketResponse { ticket, expires_at }))
}

async fn get_ticket(State(hub): State<Arc<Hub>>, Path(ticket): Path<String>) -> Response {
    let mut peers = hub.peers.lock().await;
    peers.prune_tickets();
    match peers.take_ticket(ticket.trim()) {
        Some(entry) => json_ok(entry.payload),
        None => json_error(StatusCode::NOT_FOUND, "ticket_not_found_or_expired"),
    }
}

async fn get_pairing_code(State(hub): State<Arc<Hub>>, Path(code): Path<String>) -> Response {
    let Some(code) = normalize_pairing_code(&code) else {
        return json_error(StatusCode::BAD_REQUEST, "invalid_pairing_code");
    };
    let mut peers = hub.peers.lock().await;
    peers.prune_tickets();
    match peers.take_pairing_code(&code) {
        Some(entry) => json_ok(entry.payload),
        None => json_error(StatusCode::NOT_FOUND, "pairing_code_not_found_or_expired"),
    }
}

async fn legacy_host_socket(
    State(hub): State<Arc<Hub>>,
    Query(query): Query<HostQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let token = query.token.unwrap_or_default();
    if hub.authenticate_host(&query.host_id, &token).is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _ = hub.with_store(|store| Ok(store.touch_host(&query.host_id)?));
    ws.on_upgrade(move |socket| {
        run_peer(
            socket,
            hub,
            PeerRole::Host,
            query.host_id,
            String::new(),
            false,
        )
    })
    .into_response()
}

async fn legacy_client_socket(
    State(hub): State<Arc<Hub>>,
    Query(query): Query<ClientQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let token = query.token.unwrap_or_default();
    let Some(device) = hub.authenticate_device(&query.device_id, &token) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let _ = hub.with_store(|store| Ok(store.touch_device(&device.id)?));
    ws.on_upgrade(move |socket| {
        run_peer(
            socket,
            hub,
            PeerRole::Client,
            device.host_id,
            device.id,
            false,
        )
    })
    .into_response()
}

async fn v3_host_socket(
    State(hub): State<Arc<Hub>>,
    Query(query): Query<HostQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    if query.host_id.trim().is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    ws.on_upgrade(move |socket| {
        run_peer(
            socket,
            hub,
            PeerRole::Host,
            query.host_id,
            String::new(),
            true,
        )
    })
    .into_response()
}

async fn v3_client_socket(
    State(hub): State<Arc<Hub>>,
    Query(query): Query<ClientQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let host_id = query.host_id.unwrap_or_default();
    if host_id.trim().is_empty() || query.device_id.trim().is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    ws.on_upgrade(move |socket| {
        run_peer(
            socket,
            hub,
            PeerRole::Client,
            host_id,
            query.device_id,
            true,
        )
    })
    .into_response()
}

async fn run_peer(
    socket: WebSocket,
    hub: Arc<Hub>,
    role: PeerRole,
    host_id: String,
    device_id: String,
    stateless: bool,
) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<RemoteRelayEnvelope>();
    let mut peer = Peer {
        snapshot: PeerSnapshot {
            role,
            host_id: host_id.clone(),
            device_id: device_id.clone(),
            stateless,
        },
        relay_window: RemoteRelayPeerWindow::default(),
    };
    hub.register_peer(peer.snapshot.clone(), tx.clone()).await;

    let _ = tx.send(relay_hello_envelope(
        host_id.clone(),
        device_id.clone(),
        json!({ "role": role.as_str() }),
        Some(crate::store::now_millis()),
    ));

    let writer = tokio::spawn(async move {
        while let Some(envelope) = rx.recv().await {
            let Ok(text) = serde_json::to_string(&envelope) else {
                continue;
            };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(message) = stream.next().await {
        let Ok(Message::Text(text)) = message else {
            continue;
        };
        let Ok(mut envelope) = serde_json::from_str::<RemoteRelayEnvelope>(&text) else {
            continue;
        };
        envelope.at = Some(crate::store::now_millis());
        let size = serde_json::to_vec(&envelope)
            .map(|data| data.len())
            .unwrap_or(usize::MAX);
        if let Some(stats) = &hub.stats {
            stats.record_message(size);
        }
        if !hub.allow_relay_message(&mut peer, &envelope, size, &tx) {
            continue;
        }
        if !stateless && role == PeerRole::Client && envelope.kind == REMOTE_DEVICE_INFO {
            if let Some(name) = envelope
                .payload
                .as_ref()
                .and_then(|payload| payload.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                let _ = hub.with_store(|store| Ok(store.update_device_name(&device_id, name)?));
            }
        }
        hub.forward_envelope(&peer.snapshot, envelope).await;
    }

    writer.abort();
    hub.unregister_peer(&peer.snapshot, &tx).await;
    if peer.snapshot.role == PeerRole::Host {
        // The host's socket dropped; after a grace period (to ride out blips)
        // tell its clients it is offline so they stop showing stale content.
        hub.schedule_host_offline(peer.snapshot.host_id.clone());
    }
}

impl Hub {
    async fn register_peer(
        &self,
        peer: PeerSnapshot,
        tx: mpsc::UnboundedSender<RemoteRelayEnvelope>,
    ) {
        let mut peers = self.peers.lock().await;
        match peer.role {
            PeerRole::Host => {
                if let Some(old) = peers.hosts.insert(
                    peer.host_id.clone(),
                    PeerSender {
                        host_id: peer.host_id.clone(),
                        tx: tx.clone(),
                    },
                ) {
                    let _ = old.tx.send(relay_error_envelope(
                        &peer.host_id,
                        "",
                        "replaced",
                        Some(crate::store::now_millis()),
                    ));
                }
                // A reconnecting host cancels any pending grace-delayed offline:
                // dropping the generation entry makes the in-flight grace task
                // see `None` and skip. Removing (rather than bumping) also keeps
                // the map from accumulating an entry per ever-connected host.
                self.host_offline_gen.lock().await.remove(&peer.host_id);
            }
            PeerRole::Client => {
                if let Some(old) = peers.clients.insert(
                    peer.device_id.clone(),
                    PeerSender {
                        host_id: peer.host_id.clone(),
                        tx: tx.clone(),
                    },
                ) {
                    let _ = old.tx.send(relay_error_envelope(
                        &peer.host_id,
                        &peer.device_id,
                        "replaced",
                        Some(crate::store::now_millis()),
                    ));
                }
            }
        }
        drop(peers);
        info!(
            role = peer.role.as_str(),
            host = peer.host_id,
            device = peer.device_id,
            protocol = peer_protocol(&peer),
            "peer connected"
        );
        if let Some(stats) = &self.stats {
            stats.record_connect(&peer);
        }
        if peer.role == PeerRole::Client {
            self.send_to_host(
                &peer.host_id,
                RemoteRelayEnvelope {
                    kind: REMOTE_DEVICE_CONNECTED.into(),
                    host_id: peer.host_id.clone(),
                    device_id: peer.device_id.clone(),
                    payload: Some(json!({ "deviceId": peer.device_id })),
                    at: Some(crate::store::now_millis()),
                    ..RemoteRelayEnvelope::default()
                },
            )
            .await;
        }
    }

    /// Bump and return a host's generation. Any in-flight grace task that
    /// captured an older generation will skip its `host.offline`.
    async fn bump_host_offline_gen(&self, host_id: &str) -> u64 {
        let mut gens = self.host_offline_gen.lock().await;
        let generation = gens.entry(host_id.to_string()).or_insert(0);
        *generation += 1;
        *generation
    }

    /// After a host's socket drops, wait out the grace period and — only if the
    /// host has not reconnected — tell its clients it is offline. A reconnect
    /// (which bumps the generation and re-adds the host) cancels this, so a
    /// transient blip never resets clients.
    fn schedule_host_offline(self: &Arc<Self>, host_id: String) {
        let hub = Arc::clone(self);
        tokio::spawn(async move {
            let generation = hub.bump_host_offline_gen(&host_id).await;
            tokio::time::sleep(HOST_OFFLINE_GRACE).await;
            // Check presence first, then the generation — never holding both
            // locks at once (register takes peers→gen; nesting them the other
            // way here would deadlock).
            let host_back = hub.peers.lock().await.hosts.contains_key(&host_id);
            let mut gens = hub.host_offline_gen.lock().await;
            if gens.get(&host_id).copied() != Some(generation) {
                return; // superseded by a newer disconnect/reconnect; it owns the entry.
            }
            // Our generation is the latest, so we own cleanup on every path —
            // remove the entry whether the host came back or is truly gone.
            gens.remove(&host_id);
            drop(gens);
            if host_back {
                return; // host reconnected within grace; not gone.
            }
            hub.broadcast_host_offline(&host_id).await;
        });
    }

    async fn broadcast_host_offline(&self, host_id: &str) {
        let targets: Vec<mpsc::UnboundedSender<RemoteRelayEnvelope>> = {
            let peers = self.peers.lock().await;
            peers
                .clients
                .values()
                .filter(|client| client.host_id == host_id)
                .map(|client| client.tx.clone())
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        info!(host = host_id, clients = targets.len(), "host offline notified");
        let envelope = RemoteRelayEnvelope {
            kind: REMOTE_HOST_OFFLINE.into(),
            host_id: host_id.to_string(),
            payload: Some(json!({ "message": "Desktop host disconnected." })),
            at: Some(crate::store::now_millis()),
            ..RemoteRelayEnvelope::default()
        };
        for tx in targets {
            let _ = tx.send(envelope.clone());
        }
    }

    async fn unregister_peer(
        &self,
        peer: &PeerSnapshot,
        tx: &mpsc::UnboundedSender<RemoteRelayEnvelope>,
    ) {
        let mut peers = self.peers.lock().await;
        let removed = match peer.role {
            PeerRole::Host => remove_same_channel(&mut peers.hosts, &peer.host_id, tx),
            PeerRole::Client => remove_same_channel(&mut peers.clients, &peer.device_id, tx),
        };
        drop(peers);
        if !removed {
            return;
        }
        info!(
            role = peer.role.as_str(),
            host = peer.host_id,
            device = peer.device_id,
            protocol = peer_protocol(peer),
            "peer disconnected"
        );
        if let Some(stats) = &self.stats {
            stats.record_disconnect(peer);
        }
        if peer.role == PeerRole::Client {
            self.send_to_host(
                &peer.host_id,
                RemoteRelayEnvelope {
                    kind: REMOTE_DEVICE_DISCONNECTED.into(),
                    host_id: peer.host_id.clone(),
                    device_id: peer.device_id.clone(),
                    payload: Some(json!({ "deviceId": peer.device_id })),
                    at: Some(crate::store::now_millis()),
                    ..RemoteRelayEnvelope::default()
                },
            )
            .await;
        }
    }

    async fn forward_envelope(&self, peer: &PeerSnapshot, mut envelope: RemoteRelayEnvelope) {
        let peers = self.peers.lock().await;
        let mut forwarded = 0;
        match peer.role {
            PeerRole::Host => {
                if !envelope.device_id.is_empty() {
                    if let Some(client) = peers.clients.get(&envelope.device_id) {
                        let _ = client.tx.send(envelope);
                        forwarded = 1;
                    }
                } else {
                    for client in peers
                        .clients
                        .values()
                        .filter(|client| client.host_id == peer.host_id)
                    {
                        let _ = client.tx.send(envelope.clone());
                        forwarded += 1;
                    }
                }
            }
            PeerRole::Client => {
                envelope.host_id = peer.host_id.clone();
                envelope.device_id = peer.device_id.clone();
                if let Some(host) = peers.hosts.get(&peer.host_id) {
                    let _ = host.tx.send(envelope);
                    forwarded = 1;
                }
            }
        }
        drop(peers);
        if let Some(stats) = &self.stats {
            stats.record_forwarded(forwarded);
        }
    }

    fn allow_relay_message(
        &self,
        peer: &mut Peer,
        envelope: &RemoteRelayEnvelope,
        size: usize,
        tx: &mpsc::UnboundedSender<RemoteRelayEnvelope>,
    ) -> bool {
        let policy = RemoteRelayPolicy::default();
        match policy.validate_message(
            envelope,
            size,
            &mut peer.relay_window,
            crate::store::now_millis(),
        ) {
            RemoteRelayDecision::Allow => true,
            RemoteRelayDecision::Reject(error) => {
                warn!(
                    role = peer.snapshot.role.as_str(),
                    host = peer.snapshot.host_id,
                    device = peer.snapshot.device_id,
                    kind = envelope.kind,
                    error,
                    "relay message rejected"
                );
                if let Some(stats) = &self.stats {
                    stats.record_dropped(&peer.snapshot, envelope, error, size);
                }
                let _ = tx.send(relay_error_envelope(
                    &peer.snapshot.host_id,
                    &peer.snapshot.device_id,
                    error,
                    Some(crate::store::now_millis()),
                ));
                false
            }
        }
    }

    async fn send_to_host(&self, host_id: &str, envelope: RemoteRelayEnvelope) -> bool {
        let peers = self.peers.lock().await;
        peers
            .hosts
            .get(host_id)
            .map(|peer| peer.tx.send(envelope).is_ok())
            .unwrap_or(false)
    }

    async fn send_to_client(&self, device_id: &str, envelope: RemoteRelayEnvelope) -> bool {
        let peers = self.peers.lock().await;
        peers
            .clients
            .get(device_id)
            .map(|peer| peer.tx.send(envelope).is_ok())
            .unwrap_or(false)
    }

    async fn online_device_ids(&self) -> HashMap<String, bool> {
        let peers = self.peers.lock().await;
        peers.clients.keys().map(|id| (id.clone(), true)).collect()
    }
}

impl PeerRegistry {
    fn insert_ticket(&mut self, ticket: String, payload: Value, expires_at: i64) {
        let code = pairing_code_from_payload(&payload);
        if let Some(code) = &code {
            if let Some(old_ticket) = self.tickets_by_code.insert(code.clone(), ticket.clone()) {
                self.tickets.remove(&old_ticket);
            }
        }
        self.tickets.insert(
            ticket,
            TicketEntry {
                payload,
                expires_at,
                code,
            },
        );
    }

    fn take_ticket(&mut self, ticket: &str) -> Option<TicketEntry> {
        let entry = self.tickets.remove(ticket)?;
        if let Some(code) = &entry.code {
            if self
                .tickets_by_code
                .get(code)
                .is_some_and(|mapped| mapped == ticket)
            {
                self.tickets_by_code.remove(code);
            }
        }
        Some(entry)
    }

    fn take_pairing_code(&mut self, code: &str) -> Option<TicketEntry> {
        let ticket = self.tickets_by_code.remove(code)?;
        self.tickets.remove(&ticket)
    }

    fn prune_tickets(&mut self) {
        let now = crate::store::now_millis();
        self.tickets.retain(|_, ticket| ticket.expires_at > now);
        self.tickets_by_code
            .retain(|_, ticket| self.tickets.contains_key(ticket));
    }
}

pub fn peer_protocol(peer: &PeerSnapshot) -> &'static str {
    if peer.stateless { "v3" } else { "legacy" }
}

fn remove_same_channel(
    peers: &mut HashMap<String, PeerSender>,
    id: &str,
    tx: &mpsc::UnboundedSender<RemoteRelayEnvelope>,
) -> bool {
    if peers
        .get(id)
        .map(|peer| peer.tx.same_channel(tx))
        .unwrap_or(false)
    {
        peers.remove(id);
        true
    } else {
        false
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(ToOwned::to_owned)
}

fn public_base_url(headers: &HeaderMap) -> String {
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("http");
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(axum::http::header::HOST))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("localhost:8088");
    format!("{proto}://{host}")
}

fn pairing_code_from_payload(payload: &Value) -> Option<String> {
    normalize_pairing_code(payload.get("code")?.as_str()?)
}

fn normalize_pairing_code(code: &str) -> Option<String> {
    let value: String = code.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if value.len() == 6 { Some(value) } else { None }
}

fn json_ok(value: Value) -> Response {
    (StatusCode::OK, Json(value)).into_response()
}

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn v3_ticket_indexes_pairing_code_once() {
        let hub = Hub::in_memory().unwrap();
        let mut peers = hub.peers.lock().await;
        peers.insert_ticket(
            "ticket-1".into(),
            json!({ "code": "123456", "hostId": "host-1" }),
            crate::store::now_millis() + 60_000,
        );

        let entry = peers.take_pairing_code("123456").expect("pairing code");
        assert_eq!(entry.payload["hostId"], "host-1");
        assert!(peers.take_pairing_code("123456").is_none());
        assert!(peers.take_ticket("ticket-1").is_none());
    }

    #[test]
    fn normalize_pairing_code_accepts_spaced_digits() {
        assert_eq!(normalize_pairing_code("12 34-56"), Some("123456".into()));
        assert_eq!(normalize_pairing_code("12345"), None);
    }

    #[test]
    fn public_base_url_uses_forwarded_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        headers.insert("x-forwarded-host", "relay.example.com".parse().unwrap());
        assert_eq!(public_base_url(&headers), "https://relay.example.com");
    }

    #[test]
    fn relay_policy_rejects_upload_messages() {
        let hub = Hub::in_memory().unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut peer = Peer {
            snapshot: PeerSnapshot {
                role: PeerRole::Client,
                host_id: "host".into(),
                device_id: "device".into(),
                stateless: true,
            },
            relay_window: RemoteRelayPeerWindow::default(),
        };
        let envelope = RemoteRelayEnvelope {
            kind: "terminal.upload.start".into(),
            ..RemoteRelayEnvelope::default()
        };

        assert!(!hub.allow_relay_message(&mut peer, &envelope, 64, &tx));
    }
}
