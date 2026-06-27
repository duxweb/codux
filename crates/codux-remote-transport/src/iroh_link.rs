use crate::{
    RemoteControllerTransportConfig, RemoteHostTransportConfig, RemoteTransport,
    RemoteTransportCandidate, RemoteTransportLogHandler, RemoteTransportMessageHandler,
    RemoteTransportPairingHandler, RemoteTransportStateHandler, RemoteTransportUpload,
    RemoteTransportUploadHandler,
};
use async_trait::async_trait;
use codux_protocol::{REMOTE_TERMINAL_UPLOAD_BLOB, RemoteEnvelope, RemoteTransportKind};
use futures_util::StreamExt;
use iroh::{
    Endpoint, EndpointAddr, RelayMap, RelayMode, RelayUrl, SecretKey, TransportAddr,
    endpoint::{Connection, PathEvent, RecvStream, SendStream, presets},
    protocol::{AcceptError, ProtocolHandler, Router},
};
use iroh_blobs::{BlobsProtocol, store::mem::MemStore, ticket::BlobTicket};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use iroh_tickets::endpoint::EndpointTicket;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, mpsc};

pub const CODUX_REMOTE_ALPN: &[u8] = b"/codux/remote/1";
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_UPLOAD_BYTES: usize = 20 * 1024 * 1024;
const STREAM_KIND_CONTROL: u8 = 0;
const STREAM_KIND_TERMINAL: u8 = 1;
const IROH_RELAY_URL_ENV: &str = "CODUX_IROH_RELAY_URL";
/// Set (non-empty) to disable LAN mDNS address lookup. Direct LAN discovery is
/// best-effort and additive to the relay path, so disabling it only forgoes
/// same-network direct connections — useful on networks that block multicast.
const IROH_DISABLE_LAN_MDNS_ENV: &str = "CODUX_IROH_DISABLE_LAN_MDNS";
const IROH_ONLINE_TIMEOUT: Duration = Duration::from_secs(12);
/// Upper bound on a SINGLE controller→host dial attempt. Kept short on purpose:
/// the first relay-routed dial to a peer routinely stalls while the relay primes
/// the cross-path between the two nodes (measured: first dial times out, the very
/// next one connects in ~18ms), so we want to give up on a stalled dial quickly
/// and re-dial the same already-online endpoint rather than wait out a long hang.
const IROH_DIAL_BASE_TIMEOUT: Duration = Duration::from_secs(4);
/// How many times to re-dial on the same warm endpoint before surfacing an error
/// to the reconnect loop (which then cold-starts a fresh endpoint). Each attempt
/// gets `BASE + 2s * attempt`, so 4s + 6s + 8s = 18s of dialling on one online().
const IROH_DIAL_ATTEMPTS: usize = 3;

type IrohSender = mpsc::UnboundedSender<Vec<u8>>;

#[derive(Clone)]
struct PeerSenders {
    control: IrohSender,
    terminal: IrohSender,
}

impl PeerSenders {
    fn same_channels(&self, other: &Self) -> bool {
        self.control.same_channel(&other.control) && self.terminal.same_channel(&other.terminal)
    }
}

pub struct RemoteIrohHostTransport {
    endpoint: Endpoint,
    blob_store: MemStore,
    node_id: String,
    relay_url: String,
    peers: Mutex<HashMap<String, PeerSenders>>,
    on_message: RemoteTransportMessageHandler,
    on_upload: RemoteTransportUploadHandler,
    on_state: RemoteTransportStateHandler,
    on_pairing: RemoteTransportPairingHandler,
    on_log: Option<RemoteTransportLogHandler>,
    /// The iroh `Router` driving our accept loop. Stored so it stays alive (it
    /// aborts on drop); set once right after construction.
    router: OnceLock<Router>,
}

/// Adapts the host transport to iroh's `ProtocolHandler` so the `Router` drives
/// incoming `CODUX_REMOTE_ALPN` connections. Holds a `Weak` back-ref so the
/// router (owned by the transport) doesn't keep the transport alive in a cycle.
#[derive(Debug, Clone)]
struct CoduxRemoteProtocol {
    transport: Weak<RemoteIrohHostTransport>,
}

impl ProtocolHandler for CoduxRemoteProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        if let Some(transport) = self.transport.upgrade() {
            transport.handle_connection(connection).await;
        }
        Ok(())
    }
}

impl RemoteIrohHostTransport {
    pub async fn connect(
        config: &RemoteHostTransportConfig,
        on_message: RemoteTransportMessageHandler,
        on_upload: RemoteTransportUploadHandler,
        on_state: RemoteTransportStateHandler,
        on_pairing: RemoteTransportPairingHandler,
        on_log: Option<RemoteTransportLogHandler>,
    ) -> Result<Arc<Self>, String> {
        let configured_relay_url = iroh_relay_url(config)?;
        let relay_authentication = config.iroh_relay_authentication.trim().to_string();
        let endpoint = endpoint_builder(
            configured_relay_url.as_ref(),
            &relay_authentication,
            host_secret_key(config).as_ref(),
        )
        .alpns(vec![CODUX_REMOTE_ALPN.to_vec(), iroh_blobs::ALPN.to_vec()])
        .bind()
        .await
        .map_err(|error| format!("iroh host bind failed: {error}"))?;
        // Advertise on the LAN so co-located controllers reach us directly.
        enable_local_discovery(&endpoint, &on_log);
        if tokio::time::timeout(IROH_ONLINE_TIMEOUT, endpoint.online())
            .await
            .is_err()
        {
            endpoint.close().await;
            return Err("iroh host relay online timeout".to_string());
        }
        let relay_url = endpoint
            .addr()
            .relay_urls()
            .next()
            .cloned()
            .or(configured_relay_url)
            .ok_or_else(|| "iroh host has no relay url".to_string())?;
        let blob_store = MemStore::new();
        let transport = Arc::new(Self {
            node_id: endpoint.id().to_string(),
            endpoint,
            blob_store: blob_store.clone(),
            relay_url: relay_url.to_string(),
            peers: Mutex::new(HashMap::new()),
            on_message,
            on_upload,
            on_state,
            on_pairing,
            on_log,
            router: OnceLock::new(),
        });
        // Run our star protocol as a standard iroh custom protocol: the Router
        // owns the accept loop and dispatches by ALPN (our control/terminal
        // streams vs iroh-blobs), replacing the hand-rolled accept+match. The
        // handler holds a Weak back-ref so transport<->router don't form an Arc
        // cycle; closing the transport shuts the Router down.
        let blob_protocol = BlobsProtocol::new(blob_store.as_ref(), None);
        let router = Router::builder(transport.endpoint.clone())
            .accept(
                CODUX_REMOTE_ALPN,
                CoduxRemoteProtocol {
                    transport: Arc::downgrade(&transport),
                },
            )
            .accept(iroh_blobs::ALPN, blob_protocol)
            .spawn();
        let _ = transport.router.set(router);
        Ok(transport)
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn relay_url(&self) -> &str {
        &self.relay_url
    }

    async fn handle_connection(self: Arc<Self>, connection: Connection) {
        let peer_key = connection.remote_id().to_string();
        let (control_tx, control_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (terminal_tx, terminal_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let peer_senders = PeerSenders {
            control: control_tx,
            terminal: terminal_tx,
        };
        let mut control_rx = Some(control_rx);
        let mut terminal_rx = Some(terminal_rx);
        self.insert_peer(peer_key.clone(), peer_senders.clone());
        (self.on_state)(peer_key.clone(), "connected".to_string());
        loop {
            let (send, mut recv) = match connection.accept_bi().await {
                Ok(streams) => streams,
                Err(error) => {
                    self.log(format!(
                        "iroh_host_accept_stream failed peer={peer_key} error={error}"
                    ));
                    break;
                }
            };
            let kind = match read_stream_kind(&mut recv).await {
                Ok(kind) => kind,
                Err(error) => {
                    self.log(format!(
                        "iroh_host_stream_kind failed peer={peer_key} error={error}"
                    ));
                    continue;
                }
            };
            match kind {
                STREAM_KIND_CONTROL => {
                    let Some(rx) = control_rx.take() else {
                        self.log(format!(
                            "iroh_host_duplicate_control_stream peer={peer_key}"
                        ));
                        continue;
                    };
                    let mut send = send;
                    if let Err(error) = write_stream_kind(&mut send, STREAM_KIND_CONTROL).await {
                        self.log(format!(
                            "iroh_host_control_init failed peer={peer_key} error={error}"
                        ));
                        continue;
                    }
                    spawn_writer(send, rx, self.on_log.clone());
                    self.spawn_peer_reader(recv, peer_key.clone(), peer_senders.clone(), "control");
                }
                STREAM_KIND_TERMINAL => {
                    let Some(rx) = terminal_rx.take() else {
                        self.log(format!(
                            "iroh_host_duplicate_terminal_stream peer={peer_key}"
                        ));
                        continue;
                    };
                    let mut send = send;
                    if let Err(error) = write_stream_kind(&mut send, STREAM_KIND_TERMINAL).await {
                        self.log(format!(
                            "iroh_host_terminal_init failed peer={peer_key} error={error}"
                        ));
                        continue;
                    }
                    spawn_writer(send, rx, self.on_log.clone());
                    self.spawn_peer_reader(
                        recv,
                        peer_key.clone(),
                        peer_senders.clone(),
                        "terminal",
                    );
                }
                _ => self.log(format!(
                    "unsupported iroh stream kind {kind} peer={peer_key}"
                )),
            }
        }
        self.remove_peer_aliases(&peer_senders);
        (self.on_state)(peer_key, "closed".to_string());
    }

    fn spawn_peer_reader(
        self: &Arc<Self>,
        recv: RecvStream,
        peer_key: String,
        senders: PeerSenders,
        label: &'static str,
    ) {
        let transport = Arc::clone(self);
        tokio::spawn(async move {
            let transport_for_frame = Arc::clone(&transport);
            let peer_key_for_frame = peer_key.clone();
            let read_result = read_loop(
                recv,
                peer_key.clone(),
                Arc::new(move |device_id, data| {
                    transport_for_frame.handle_frame(
                        &peer_key_for_frame,
                        &senders,
                        device_id,
                        data,
                    );
                }),
            )
            .await;
            if let Err(error) = read_result {
                transport.log(format!(
                    "iroh_host_{label}_read failed peer={} error={error}",
                    peer_key
                ));
            }
        });
    }

    fn handle_frame(
        &self,
        peer_key: &str,
        senders: &PeerSenders,
        fallback_device_id: String,
        data: Vec<u8>,
    ) {
        let Ok(raw) = serde_json::from_slice::<RemoteEnvelope>(&data) else {
            self.log("iroh_host_recv drop reason=decode".to_string());
            return;
        };
        if raw.kind == codux_protocol::REMOTE_PAIRING_REQUEST {
            if let Some(handshake) = crate::control_messages::pairing_handshake_from_envelope(&raw)
            {
                (self.on_pairing)(handshake);
            }
        }
        let device_id = raw
            .device_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(fallback_device_id);
        if !device_id.trim().is_empty() {
            self.bind_peer_alias(peer_key, &device_id, senders);
            (self.on_state)(device_id.clone(), "connected".to_string());
        }
        if let Some(pong) = crate::control_messages::transport_pong_for_ping(&raw, Some(&device_id))
        {
            let _ = self.send(pong.into_bytes(), Some(&device_id));
            return;
        }
        if raw.kind == REMOTE_TERMINAL_UPLOAD_BLOB {
            let upload = upload_metadata_from_blob_envelope(&raw, &device_id);
            let endpoint = self.endpoint.clone();
            let blob_store = self.blob_store.clone();
            let on_upload = Arc::clone(&self.on_upload);
            let on_log = self.on_log.clone();
            tokio::spawn(async move {
                match download_terminal_upload_blob(&blob_store, &endpoint, upload).await {
                    Ok(upload) => {
                        if let Err(error) = on_upload(upload) {
                            if let Some(on_log) = on_log.as_ref() {
                                on_log(format!("iroh_host_blob_upload failed error={error}"));
                            }
                        }
                    }
                    Err(error) => {
                        if let Some(on_log) = on_log.as_ref() {
                            on_log(format!("iroh_host_blob_download failed error={error}"));
                        }
                    }
                }
            });
            return;
        }
        (self.on_message)(device_id, data);
    }

    fn insert_peer(&self, device_id: String, senders: PeerSenders) {
        if let Ok(mut peers) = self.peers.lock() {
            peers.insert(device_id, senders);
        }
    }

    fn bind_peer_alias(&self, peer_key: &str, device_id: &str, senders: &PeerSenders) {
        if peer_key == device_id {
            return;
        }
        if let Ok(mut peers) = self.peers.lock() {
            peers.insert(device_id.to_string(), senders.clone());
        }
    }

    fn remove_peer_aliases(&self, senders: &PeerSenders) {
        if let Ok(mut peers) = self.peers.lock() {
            peers.retain(|_, current| !current.same_channels(senders));
        }
    }

    fn send_to_peers(
        &self,
        data: Vec<u8>,
        device_id: Option<&str>,
        select: impl Fn(&PeerSenders) -> &IrohSender,
    ) -> bool {
        let Ok(peers) = self.peers.lock() else {
            return false;
        };
        if let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) {
            return peers
                .get(device_id)
                .map(|peer| select(peer).send(data).is_ok())
                .unwrap_or(false);
        }
        let mut sent = false;
        let unique = unique_senders(peers.values().map(select));
        for tx in unique {
            sent |= tx.send(data.clone()).is_ok();
        }
        sent
    }

    fn log(&self, message: String) {
        if let Some(on_log) = self.on_log.as_ref() {
            on_log(message);
        }
    }
}

pub(crate) fn unique_senders<'a>(
    senders: impl IntoIterator<Item = &'a IrohSender>,
) -> Vec<IrohSender> {
    let mut unique = Vec::<IrohSender>::new();
    for tx in senders {
        if unique.iter().any(|current| current.same_channel(tx)) {
            continue;
        }
        unique.push(tx.clone());
    }
    unique
}

#[async_trait]
impl RemoteTransport for RemoteIrohHostTransport {
    fn kind(&self) -> RemoteTransportKind {
        RemoteTransportKind::Iroh
    }

    fn send(&self, data: Vec<u8>, device_id: Option<&str>) -> bool {
        self.send_to_peers(data, device_id, |peer| &peer.control)
    }

    fn send_terminal(&self, data: Vec<u8>, device_id: Option<&str>) -> bool {
        self.send_to_peers(data, device_id, |peer| &peer.terminal)
    }

    fn iroh_candidate(&self) -> Option<(String, String)> {
        Some((self.node_id.clone(), self.relay_url.clone()))
    }

    fn iroh_endpoint_ticket(&self) -> Option<String> {
        Some(EndpointTicket::from(self.endpoint.addr()).to_string())
    }

    async fn publish_blob(&self, bytes: Vec<u8>) -> Result<String, String> {
        publish_blob_bytes(&self.blob_store, &self.endpoint, bytes).await
    }

    async fn fetch_blob(&self, ticket: &str) -> Result<Vec<u8>, String> {
        fetch_blob_bytes(&self.blob_store, &self.endpoint, ticket).await
    }

    async fn shutdown(&self) {
        if let Ok(mut peers) = self.peers.lock() {
            peers.clear();
        }
        if let Some(router) = self.router.get() {
            let _ = router.shutdown().await;
        }
        self.endpoint.close().await;
    }
}

/// One controller endpoint, shared across every controller connection in the
/// process — pairing, the control link, and reconnects. The first connection
/// pays bind + `online()` (relay registration, ~3s) AND primes the relay's route
/// to the host (the costly part: the first dial to a peer stalls while the relay
/// establishes that route, the next one connects in ~milliseconds). Pooling lets
/// every later connection skip both, so the control link comes up instantly right
/// after pairing instead of cold-starting a fresh endpoint. Keyed by relay so a
/// relay change rebuilds; evicted when a dial keeps failing on it so a wedged
/// endpoint can't poison every retry.
struct SharedControllerEndpoint {
    relay_key: String,
    endpoint: Endpoint,
    // One blob store + Router shared by every controller transport on this
    // pooled endpoint: a single accept loop (the Router) serves iroh-blobs for
    // all of them from one store, so an incoming fetch always hits the store the
    // blob was published to. (The old per-transport accept loops raced on the
    // shared endpoint and could serve from the wrong store.) Held here so they
    // live as long as the pooled endpoint; dropping the entry aborts the Router.
    blob_store: MemStore,
    #[allow(dead_code)]
    router: Router,
    // Identity of THIS pooled build. A connect that fails its dials carries the
    // generation it dialed on and only retires the pool entry when it still
    // matches — so a late evict from an ABANDONED reconnect (the Dart layer gave
    // up and started a newer connect) can't clobber the build a concurrent
    // connect already reused or swapped in.
    generation: u64,
}

static SHARED_CONTROLLER_ENDPOINT: OnceLock<AsyncMutex<Option<SharedControllerEndpoint>>> =
    OnceLock::new();
static SHARED_CONTROLLER_ENDPOINT_GENERATION: AtomicU64 = AtomicU64::new(0);

fn controller_endpoint_key(relay_url: Option<&RelayUrl>, relay_authentication: &str) -> String {
    format!(
        "{}|{}",
        relay_url.map(|url| url.to_string()).unwrap_or_default(),
        relay_authentication.trim()
    )
}

async fn acquire_controller_endpoint(
    relay_url: Option<&RelayUrl>,
    relay_authentication: &str,
    on_log: &Option<RemoteTransportLogHandler>,
) -> Result<(Endpoint, MemStore, u64), String> {
    let key = controller_endpoint_key(relay_url, relay_authentication);
    let mut guard = SHARED_CONTROLLER_ENDPOINT
        .get_or_init(|| AsyncMutex::new(None))
        .lock()
        .await;
    if let Some(shared) = guard.as_ref() {
        if shared.relay_key == key {
            log_transport(on_log, "iroh_controller endpoint reused".to_string());
            return Ok((
                shared.endpoint.clone(),
                shared.blob_store.clone(),
                shared.generation,
            ));
        }
        // Relay changed under us — drop the old entry and build for the new one.
        // Don't close(): a concurrent connect may still hold a clone of it, and
        // closing a shared handle would wedge that connect mid-dial. It's
        // reclaimed when the last clone drops.
        *guard = None;
    }
    let online_started = std::time::Instant::now();
    let endpoint = endpoint_builder(relay_url, relay_authentication, None)
        .alpns(vec![iroh_blobs::ALPN.to_vec()])
        .bind()
        .await
        .map_err(|error| format!("iroh controller bind failed: {error}"))?;
    // Discover co-located hosts directly on the LAN, not just via the relay.
    enable_local_discovery(&endpoint, on_log);
    // Wait for our endpoint to register with its relay before we hand it out: a
    // relay-routed dial cannot complete without a relay home.
    if tokio::time::timeout(IROH_ONLINE_TIMEOUT, endpoint.online())
        .await
        .is_err()
    {
        endpoint.close().await;
        return Err("iroh controller relay online timeout".to_string());
    }
    log_transport(
        on_log,
        format!(
            "iroh_controller online elapsed_ms={}",
            online_started.elapsed().as_millis()
        ),
    );
    // One blob store + Router for every transport that shares this endpoint, so
    // iroh-blobs is served by a single accept loop instead of per-transport
    // loops racing on the shared endpoint.
    let blob_store = MemStore::new();
    let router = Router::builder(endpoint.clone())
        .accept(iroh_blobs::ALPN, BlobsProtocol::new(blob_store.as_ref(), None))
        .spawn();
    let generation = SHARED_CONTROLLER_ENDPOINT_GENERATION.fetch_add(1, Ordering::Relaxed);
    *guard = Some(SharedControllerEndpoint {
        relay_key: key,
        endpoint: endpoint.clone(),
        blob_store: blob_store.clone(),
        router,
        generation,
    });
    Ok((endpoint, blob_store, generation))
}

/// Retire the pooled endpoint so the next connection rebuilds a fresh one — used
/// when every dial on it failed (its relay home may have gone stale).
///
/// Two guards keep this safe under overlapping reconnects (the Dart layer can
/// abandon a slow connect and start a newer one while the old one is still
/// dialing the SAME pooled endpoint):
/// - Only retire the entry when it still matches `generation`. A late evict from
///   the abandoned connect must not clobber a build a newer connect already
///   reused or swapped in.
/// - Do NOT `close()` the endpoint. `iroh::Endpoint` is a shared handle, so
///   closing tears down the magicsock for EVERY clone — including the newer
///   connect that reused this same instance, which then fails its in-flight dial
///   with "Internal consistency error". Dropping the pool's reference is enough;
///   the OS resources are reclaimed once the last clone drops. The per-connection
///   blob-accept loops watch their own close notifier, so this doesn't strand
///   them either.
async fn evict_controller_endpoint(generation: u64) {
    if let Some(cell) = SHARED_CONTROLLER_ENDPOINT.get() {
        let mut guard = cell.lock().await;
        if let Some(shared) = guard.as_ref() {
            if shared.generation == generation {
                *guard = None;
            }
        }
    }
}

pub struct RemoteIrohControllerTransport {
    endpoint: Endpoint,
    blob_store: MemStore,
    connection: Mutex<Option<Connection>>,
    tx: Mutex<Option<IrohSender>>,
    terminal_tx: Mutex<Option<IrohSender>>,
    upload_tx: Mutex<Option<mpsc::UnboundedSender<RemoteTransportUpload>>>,
    closed: Arc<AtomicBool>,
}

impl RemoteIrohControllerTransport {
    pub async fn connect(
        config: &RemoteControllerTransportConfig,
        on_message: RemoteTransportMessageHandler,
        on_state: RemoteTransportStateHandler,
        on_log: Option<RemoteTransportLogHandler>,
    ) -> Result<Arc<Self>, String> {
        let candidate = config
            .transports
            .iter()
            .find(|candidate| {
                candidate.kind == codux_protocol::REMOTE_TRANSPORT_IROH
                    && (!candidate.ticket.trim().is_empty()
                        || (!candidate.node_id.trim().is_empty()
                            && !candidate.relay_url.trim().is_empty()))
            })
            .ok_or_else(|| "missing iroh transport candidate".to_string())?;
        let endpoint_addr = candidate_endpoint_addr(candidate)?;
        let relay_url = endpoint_addr
            .relay_urls()
            .next()
            .cloned()
            .or_else(|| parse_relay_url(&candidate.relay_url).ok());
        let relay_authentication = candidate.relay_authentication.trim().to_string();
        on_state(String::new(), "connecting".to_string());
        // Reuse the process-shared, already-online controller endpoint: the first
        // connection warms it (relay registration + route priming), every later
        // one — the control link after pairing, reconnects — skips straight to the
        // dial on the warmed path.
        let (endpoint, blob_store, endpoint_generation) =
            acquire_controller_endpoint(relay_url.as_ref(), &relay_authentication, &on_log).await?;
        // Dial with a short timeout and retry on the SAME, already-online
        // endpoint. The first relay-routed dial routinely stalls while the relay
        // primes the cross-path (the next dial then connects in ~milliseconds);
        // re-dialling here keeps the relay-registration cost (online() above) paid
        // once, instead of letting the Dart/reconnect layer tear the endpoint down
        // and cold-start a fresh one — which pays online() all over again — for
        // every attempt. Only after exhausting these does the reconnect loop get
        // an error and fall back to a fresh endpoint.
        let mut connection = None;
        let mut last_dial_error = "iroh controller connect timeout".to_string();
        for attempt in 0..IROH_DIAL_ATTEMPTS {
            let dial_timeout = IROH_DIAL_BASE_TIMEOUT + Duration::from_secs(2 * attempt as u64);
            let dial_started = std::time::Instant::now();
            match tokio::time::timeout(
                dial_timeout,
                endpoint.connect(endpoint_addr.clone(), CODUX_REMOTE_ALPN),
            )
            .await
            {
                Ok(Ok(established)) => {
                    log_transport(
                        &on_log,
                        format!(
                            "iroh_controller dial elapsed_ms={} attempt={}",
                            dial_started.elapsed().as_millis(),
                            attempt + 1
                        ),
                    );
                    connection = Some(established);
                    break;
                }
                Ok(Err(error)) => {
                    last_dial_error = format!("iroh controller connect failed: {error}");
                }
                Err(_) => {
                    last_dial_error = "iroh controller connect timeout".to_string();
                }
            }
            log_transport(
                &on_log,
                format!(
                    "iroh_controller dial retry attempt={} elapsed_ms={} reason={last_dial_error}",
                    attempt + 1,
                    dial_started.elapsed().as_millis()
                ),
            );
        }
        let Some(connection) = connection else {
            // Every dial on the pooled endpoint failed — retire it so the next
            // attempt rebuilds a fresh one rather than re-dialling a wedged path.
            // Generation-guarded + no-close so this late evict can't pull the rug
            // out from a newer reconnect that already reused this endpoint.
            evict_controller_endpoint(endpoint_generation).await;
            return Err(last_dial_error);
        };
        let (send, recv) = connection
            .open_bi()
            .await
            .map_err(|error| format!("iroh controller open stream failed: {error}"))?;
        let (terminal_send, terminal_recv) = connection
            .open_bi()
            .await
            .map_err(|error| format!("iroh controller open terminal stream failed: {error}"))?;
        let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (terminal_tx, terminal_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (upload_tx, upload_rx) = mpsc::unbounded_channel::<RemoteTransportUpload>();
        let upload_control_tx = tx.clone();
        let transport = Arc::new(Self {
            endpoint,
            blob_store: blob_store.clone(),
            connection: Mutex::new(Some(connection.clone())),
            tx: Mutex::new(Some(tx)),
            terminal_tx: Mutex::new(Some(terminal_tx)),
            upload_tx: Mutex::new(Some(upload_tx)),
            closed: Arc::new(AtomicBool::new(false)),
        });
        publish_path_state(&connection, &on_state);
        spawn_path_watcher(connection.clone(), Arc::clone(&on_state));
        // Blobs are now served by the pool-level Router (see
        // acquire_controller_endpoint), so there's no per-transport accept loop.
        spawn_upload_writer(
            blob_store,
            transport.endpoint.clone(),
            upload_rx,
            upload_control_tx,
            on_log.clone(),
        );
        spawn_terminal_stream(
            terminal_send,
            terminal_recv,
            terminal_rx,
            Arc::clone(&on_message),
            on_log.clone(),
        );
        spawn_control_stream(send, recv, rx, on_message, on_log.clone(), {
            let reader_transport = Arc::clone(&transport);
            let on_state = Arc::clone(&on_state);
            move || {
                reader_transport.close_sender();
                on_state(String::new(), "closed".to_string());
            }
        });
        Ok(transport)
    }

    fn close_sender(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(mut tx) = self.tx.lock() {
            *tx = None;
        }
        if let Ok(mut tx) = self.terminal_tx.lock() {
            *tx = None;
        }
        if let Ok(mut tx) = self.upload_tx.lock() {
            *tx = None;
        }
        if let Ok(mut connection) = self.connection.lock() {
            *connection = None;
        }
    }
}

#[async_trait]
impl RemoteTransport for RemoteIrohControllerTransport {
    fn kind(&self) -> RemoteTransportKind {
        RemoteTransportKind::Iroh
    }

    fn send(&self, data: Vec<u8>, _device_id: Option<&str>) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        self.tx
            .lock()
            .ok()
            .and_then(|tx| tx.clone())
            .map(|tx| tx.send(data).is_ok())
            .unwrap_or(false)
    }

    fn send_terminal(&self, data: Vec<u8>, _device_id: Option<&str>) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        self.terminal_tx
            .lock()
            .ok()
            .and_then(|tx| tx.clone())
            .map(|tx| tx.send(data).is_ok())
            .unwrap_or(false)
    }

    fn send_terminal_upload(&self, upload: RemoteTransportUpload) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        self.upload_tx
            .lock()
            .ok()
            .and_then(|tx| tx.clone())
            .map(|tx| tx.send(upload).is_ok())
            .unwrap_or(false)
    }

    async fn publish_blob(&self, bytes: Vec<u8>) -> Result<String, String> {
        publish_blob_bytes(&self.blob_store, &self.endpoint, bytes).await
    }

    async fn fetch_blob(&self, ticket: &str) -> Result<Vec<u8>, String> {
        fetch_blob_bytes(&self.blob_store, &self.endpoint, ticket).await
    }

    async fn shutdown(&self) {
        // Do NOT close the endpoint: it is process-shared (see
        // acquire_controller_endpoint), so closing it here would kill every other
        // controller connection and force the next one to cold-start. Dropping our
        // connection (close_sender) is enough; the pool keeps the endpoint warm.
        self.close_sender();
    }
}

fn log_transport(on_log: &Option<RemoteTransportLogHandler>, message: String) {
    if let Some(on_log) = on_log.as_ref() {
        on_log(message);
    }
}

pub(crate) async fn write_frame(send: &mut SendStream, data: &[u8]) -> Result<(), String> {
    if data.len() > MAX_FRAME_BYTES {
        return Err("frame too large".to_string());
    }
    let len = (data.len() as u32).to_be_bytes();
    send.write_all(&len)
        .await
        .map_err(|error| error.to_string())?;
    send.write_all(data)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn write_stream_kind(send: &mut SendStream, kind: u8) -> Result<(), String> {
    send.write_all(&[kind])
        .await
        .map_err(|error| error.to_string())
}

async fn read_stream_kind(recv: &mut RecvStream) -> Result<u8, String> {
    let mut kind = [0u8; 1];
    recv.read_exact(&mut kind)
        .await
        .map_err(|error| error.to_string())?;
    Ok(kind[0])
}

pub(crate) async fn read_frame(recv: &mut RecvStream) -> Result<Option<Vec<u8>>, String> {
    let mut len = [0u8; 4];
    match recv.read_exact(&mut len).await {
        Ok(_) => {}
        Err(error) if error.to_string().contains("closed") => return Ok(None),
        Err(error) if error.to_string().contains("finished") => return Ok(None),
        Err(error) => return Err(error.to_string()),
    }
    let size = u32::from_be_bytes(len) as usize;
    if size > MAX_FRAME_BYTES {
        return Err("frame too large".to_string());
    }
    let mut buf = vec![0u8; size];
    recv.read_exact(&mut buf)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(buf))
}

async fn read_loop(
    mut recv: RecvStream,
    fallback_device_id: String,
    on_message: RemoteTransportMessageHandler,
) -> Result<(), String> {
    while let Some(data) = read_frame(&mut recv).await? {
        on_message(fallback_device_id.clone(), data);
    }
    Ok(())
}

fn spawn_writer(
    mut send: SendStream,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
    on_log: Option<RemoteTransportLogHandler>,
) {
    tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if let Err(error) = write_frame(&mut send, &data).await {
                if let Some(on_log) = on_log.as_ref() {
                    on_log(format!("iroh_write failed error={error}"));
                }
                break;
            }
        }
    });
}

fn spawn_control_stream(
    mut send: SendStream,
    mut recv: RecvStream,
    rx: mpsc::UnboundedReceiver<Vec<u8>>,
    on_message: RemoteTransportMessageHandler,
    on_log: Option<RemoteTransportLogHandler>,
    on_close: impl FnOnce() + Send + 'static,
) {
    tokio::spawn(async move {
        let write_result = write_stream_kind(&mut send, STREAM_KIND_CONTROL).await;
        if let Err(error) = write_result {
            if let Some(on_log) = on_log.as_ref() {
                on_log(format!("iroh_controller_control_init failed error={error}"));
            }
            on_close();
            return;
        }
        spawn_writer(send, rx, on_log.clone());
        let result = match read_stream_kind(&mut recv).await {
            Ok(STREAM_KIND_CONTROL) => read_loop(recv, String::new(), on_message).await,
            Ok(kind) => Err(format!("unexpected controller stream kind {kind}")),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            if let Some(on_log) = on_log.as_ref() {
                on_log(format!("iroh_controller_read failed error={error}"));
            }
        }
        on_close();
    });
}

fn spawn_terminal_stream(
    mut send: SendStream,
    mut recv: RecvStream,
    rx: mpsc::UnboundedReceiver<Vec<u8>>,
    on_message: RemoteTransportMessageHandler,
    on_log: Option<RemoteTransportLogHandler>,
) {
    tokio::spawn(async move {
        if let Err(error) = write_stream_kind(&mut send, STREAM_KIND_TERMINAL).await {
            if let Some(on_log) = on_log.as_ref() {
                on_log(format!(
                    "iroh_controller_terminal_init failed error={error}"
                ));
            }
            return;
        }
        spawn_writer(send, rx, on_log.clone());
        let result = match read_stream_kind(&mut recv).await {
            Ok(STREAM_KIND_TERMINAL) => read_loop(recv, String::new(), on_message).await,
            Ok(kind) => Err(format!("unexpected terminal stream kind {kind}")),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            if let Some(on_log) = on_log.as_ref() {
                on_log(format!(
                    "iroh_controller_terminal_read failed error={error}"
                ));
            }
        }
    });
}

fn spawn_upload_writer(
    blob_store: MemStore,
    endpoint: Endpoint,
    mut rx: mpsc::UnboundedReceiver<RemoteTransportUpload>,
    control_tx: IrohSender,
    on_log: Option<RemoteTransportLogHandler>,
) {
    tokio::spawn(async move {
        while let Some(upload) = rx.recv().await {
            if let Err(error) =
                send_terminal_upload_blob(&blob_store, &endpoint, &control_tx, upload).await
            {
                if let Some(on_log) = on_log.as_ref() {
                    on_log(format!("iroh_upload_blob failed error={error}"));
                }
            }
        }
    });
}

async fn send_terminal_upload_blob(
    blob_store: &MemStore,
    endpoint: &Endpoint,
    control_tx: &IrohSender,
    upload: RemoteTransportUpload,
) -> Result<(), String> {
    if upload.bytes.is_empty() || upload.bytes.len() > MAX_UPLOAD_BYTES {
        return Err("upload size is not supported".to_string());
    }
    let total_bytes = upload.bytes.len();
    let tag = blob_store
        .add_bytes(upload.bytes)
        .await
        .map_err(|error| error.to_string())?;
    let ticket = BlobTicket::new(endpoint.addr(), tag.hash, tag.format).to_string();
    let envelope = serde_json::json!({
        "type": REMOTE_TERMINAL_UPLOAD_BLOB,
        "deviceId": upload.device_id,
        "sessionId": upload.session_id,
        "payload": {
            "name": upload.name,
            "mime": upload.mime,
            "kind": upload.kind,
            "ticket": ticket,
            "totalBytes": total_bytes,
        },
    });
    let data = serde_json::to_vec(&envelope).map_err(|error| error.to_string())?;
    control_tx.send(data).map_err(|error| error.to_string())
}

fn upload_metadata_from_blob_envelope(
    envelope: &RemoteEnvelope,
    fallback_device_id: &str,
) -> RemoteTransportUpload {
    let payload = &envelope.payload;
    RemoteTransportUpload {
        device_id: envelope
            .device_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| fallback_device_id.to_string()),
        session_id: envelope.session_id.clone().unwrap_or_default(),
        name: payload
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("upload")
            .to_string(),
        mime: payload
            .get("mime")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind: payload
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("file")
            .to_string(),
        bytes: Vec::new(),
        ticket: payload
            .get("ticket")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

async fn download_terminal_upload_blob(
    blob_store: &MemStore,
    endpoint: &Endpoint,
    mut upload: RemoteTransportUpload,
) -> Result<RemoteTransportUpload, String> {
    let ticket = upload
        .ticket
        .parse::<BlobTicket>()
        .map_err(|error| error.to_string())?;
    let downloader = blob_store.downloader(endpoint);
    downloader
        .download(ticket.hash(), Some(ticket.addr().id))
        .await
        .map_err(|error| error.to_string())?;
    let bytes = blob_store
        .export_bao(ticket.hash(), iroh_blobs::protocol::ChunkRanges::all())
        .data_to_bytes()
        .await
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_UPLOAD_BYTES {
        return Err("upload size is not supported".to_string());
    }
    upload.bytes = bytes.to_vec();
    Ok(upload)
}

/// Generic blob publish: add bytes to the store, return a shareable ticket. The
/// terminal upload is a special case of this; the file domain reuses it directly.
async fn publish_blob_bytes(
    blob_store: &MemStore,
    endpoint: &Endpoint,
    bytes: Vec<u8>,
) -> Result<String, String> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err("blob size is not supported".to_string());
    }
    let tag = blob_store
        .add_bytes(bytes)
        .await
        .map_err(|error| error.to_string())?;
    Ok(BlobTicket::new(endpoint.addr(), tag.hash, tag.format).to_string())
}

/// Generic blob fetch: download the bytes a peer published under `ticket`.
async fn fetch_blob_bytes(
    blob_store: &MemStore,
    endpoint: &Endpoint,
    ticket: &str,
) -> Result<Vec<u8>, String> {
    let ticket = ticket
        .parse::<BlobTicket>()
        .map_err(|error| error.to_string())?;
    let downloader = blob_store.downloader(endpoint);
    downloader
        .download(ticket.hash(), Some(ticket.addr().id))
        .await
        .map_err(|error| error.to_string())?;
    let bytes = blob_store
        .export_bao(ticket.hash(), iroh_blobs::protocol::ChunkRanges::all())
        .data_to_bytes()
        .await
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err("blob size is not supported".to_string());
    }
    Ok(bytes.to_vec())
}

fn spawn_path_watcher(connection: Connection, on_state: RemoteTransportStateHandler) {
    tokio::spawn(async move {
        let mut events = connection.path_events();
        while let Some(event) = events.next().await {
            match event {
                PathEvent::Selected { remote_addr, .. } => {
                    publish_transport_state_for_addr(&on_state, &remote_addr, None);
                }
                PathEvent::Lagged { .. } => publish_path_state(&connection, &on_state),
                _ => {}
            }
        }
    });
}

fn publish_path_state(connection: &Connection, on_state: &RemoteTransportStateHandler) {
    let paths = connection.paths();
    for path in paths.iter() {
        if path.is_selected() {
            publish_transport_state_for_addr(on_state, path.remote_addr(), Some(path.rtt()));
            return;
        }
    }
    on_state(String::new(), "connected:path=unknown".to_string());
}

fn publish_transport_state_for_addr(
    on_state: &RemoteTransportStateHandler,
    remote_addr: &TransportAddr,
    rtt: Option<std::time::Duration>,
) {
    let path = if remote_addr.is_relay() {
        "relay"
    } else if remote_addr.is_ip() {
        "direct"
    } else {
        "unknown"
    };
    let addr = transport_addr_label(remote_addr);
    let relay_url = match remote_addr {
        TransportAddr::Relay(url) => Some(url.to_string()),
        _ => None,
    };
    let relay_url_detail = relay_url
        .as_deref()
        .map(|url| format!(";relayUrl={url}"))
        .unwrap_or_default();
    if let Some(rtt) = rtt {
        on_state(
            String::new(),
            format!(
                "latency:rtt={};path={path};addr={}{}",
                rtt.as_millis(),
                addr,
                relay_url_detail
            ),
        );
    }
    on_state(
        String::new(),
        format!("connected:path={path};addr={}{}", addr, relay_url_detail),
    );
}

fn transport_addr_label(remote_addr: &TransportAddr) -> String {
    match remote_addr {
        TransportAddr::Ip(addr) => socket_addr_label(addr),
        TransportAddr::Relay(url) => url.to_string(),
        TransportAddr::Custom(addr) => addr.to_string(),
        _ => remote_addr.to_string(),
    }
}

fn socket_addr_label(addr: &SocketAddr) -> String {
    match addr {
        SocketAddr::V4(addr) => addr.to_string(),
        SocketAddr::V6(addr) => format!("[{}]:{}", addr.ip(), addr.port()),
    }
}

/// Best-effort: add LAN mDNS address lookup so two endpoints on the same local
/// network discover each other's direct addresses and connect WITHOUT routing
/// through the relay (lower latency, and it works even when the relay is the
/// slow path). This is purely additive to the N0 preset's relay + DNS discovery
/// — if mDNS can't initialize (no usable IPv4/IPv6, multicast blocked) the
/// endpoint keeps working over the relay, so errors are logged and swallowed.
///
/// Must be called from within a tokio runtime (after `bind().await`): the mDNS
/// service spawns onto the current runtime handle.
fn enable_local_discovery(endpoint: &Endpoint, on_log: &Option<RemoteTransportLogHandler>) {
    if std::env::var(IROH_DISABLE_LAN_MDNS_ENV)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        log_transport(on_log, "iroh_lan_mdns disabled via env".to_string());
        return;
    }
    let lookup = match endpoint.address_lookup() {
        Ok(lookup) => lookup,
        Err(error) => {
            log_transport(on_log, format!("iroh_lan_mdns unavailable error={error}"));
            return;
        }
    };
    match MdnsAddressLookup::builder().build(endpoint.id()) {
        Ok(mdns) => {
            lookup.add(mdns);
            log_transport(on_log, "iroh_lan_mdns enabled".to_string());
        }
        Err(error) => {
            log_transport(on_log, format!("iroh_lan_mdns init failed error={error}"));
        }
    }
}

fn endpoint_builder(
    relay_url: Option<&RelayUrl>,
    relay_authentication: &str,
    secret_key: Option<&SecretKey>,
) -> iroh::endpoint::Builder {
    let mut builder = Endpoint::builder(presets::N0);
    if let Some(secret_key) = secret_key {
        builder = builder.secret_key(secret_key.clone());
    }
    if let Some(relay_url) = relay_url {
        let mut relay_map = RelayMap::from(relay_url.clone());
        let relay_authentication = relay_authentication.trim();
        if !relay_authentication.is_empty() {
            relay_map = relay_map.with_auth_token(relay_authentication.to_string());
        }
        builder = builder.relay_mode(RelayMode::Custom(relay_map));
    }
    builder
}

pub(crate) fn host_secret_key(config: &RemoteHostTransportConfig) -> Option<SecretKey> {
    let host_token = config.host_token.trim();
    if host_token.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(b"codux-iroh-host-secret-key-v1");
    hasher.update(config.host_id.trim().as_bytes());
    hasher.update([0]);
    hasher.update(host_token.as_bytes());
    let bytes: [u8; 32] = hasher.finalize().into();
    Some(SecretKey::from_bytes(&bytes))
}

fn iroh_relay_url(config: &RemoteHostTransportConfig) -> Result<Option<RelayUrl>, String> {
    let configured = std::env::var(IROH_RELAY_URL_ENV).unwrap_or_default();
    let configured = configured.trim();
    let configured = if configured.is_empty() {
        config.iroh_relay_url.trim()
    } else {
        configured
    };
    let resolved = if configured.is_empty() {
        crate::iroh_relay_url_for_preset(&config.relay_preset, "")
    } else {
        configured.to_string()
    };
    let trimmed = resolved.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    parse_relay_url(trimmed).map(Some)
}

fn candidate_endpoint_addr(candidate: &RemoteTransportCandidate) -> Result<EndpointAddr, String> {
    let ticket = candidate.ticket.trim();
    if !ticket.is_empty() {
        return EndpointTicket::from_str(ticket)
            .map(EndpointAddr::from)
            .map_err(|error| format!("invalid iroh endpoint ticket: {error}"));
    }
    let relay_url = parse_relay_url(&candidate.relay_url)?;
    let endpoint_id = candidate
        .node_id
        .parse()
        .map_err(|error| format!("invalid iroh node id: {error}"))?;
    Ok(EndpointAddr::from_parts(
        endpoint_id,
        [TransportAddr::Relay(relay_url)],
    ))
}

fn parse_relay_url(value: &str) -> Result<RelayUrl, String> {
    value
        .parse::<RelayUrl>()
        .map_err(|error| format!("invalid iroh relay url `{value}`: {error}"))
}
