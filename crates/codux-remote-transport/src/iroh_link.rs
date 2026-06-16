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
    protocol::ProtocolHandler,
};
use iroh_blobs::{BlobsProtocol, store::mem::MemStore, ticket::BlobTicket};
use iroh_tickets::endpoint::EndpointTicket;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

pub const CODUX_REMOTE_ALPN: &[u8] = b"/codux/remote/1";
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_UPLOAD_BYTES: usize = 20 * 1024 * 1024;
const STREAM_KIND_CONTROL: u8 = 0;
const STREAM_KIND_TERMINAL: u8 = 1;
const IROH_RELAY_URL_ENV: &str = "CODUX_IROH_RELAY_URL";
const IROH_ONLINE_TIMEOUT: Duration = Duration::from_secs(12);

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
    closed: AtomicBool,
    on_message: RemoteTransportMessageHandler,
    on_upload: RemoteTransportUploadHandler,
    on_state: RemoteTransportStateHandler,
    on_pairing: RemoteTransportPairingHandler,
    on_log: Option<RemoteTransportLogHandler>,
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
            closed: AtomicBool::new(false),
            on_message,
            on_upload,
            on_state,
            on_pairing,
            on_log,
        });
        let accept_transport = Arc::clone(&transport);
        let blob_protocol = BlobsProtocol::new(blob_store.as_ref(), None);
        tokio::spawn(async move {
            accept_transport.accept_loop(blob_protocol).await;
        });
        Ok(transport)
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn relay_url(&self) -> &str {
        &self.relay_url
    }

    async fn accept_loop(self: Arc<Self>, blobs: BlobsProtocol) {
        while !self.closed.load(Ordering::SeqCst) {
            let Some(incoming) = self.endpoint.accept().await else {
                break;
            };
            let transport = Arc::clone(&self);
            let blobs = blobs.clone();
            tokio::spawn(async move {
                let mut accepting = match incoming.accept() {
                    Ok(accepting) => accepting,
                    Err(error) => {
                        transport.log(format!("iroh_host_accept failed error={error}"));
                        return;
                    }
                };
                let alpn = match accepting.alpn().await {
                    Ok(alpn) => alpn,
                    Err(error) => {
                        transport.log(format!("iroh_host_alpn failed error={error}"));
                        return;
                    }
                };
                let connection = match accepting.await {
                    Ok(connection) => connection,
                    Err(error) => {
                        transport.log(format!("iroh_host_connect failed error={error}"));
                        return;
                    }
                };
                if alpn == CODUX_REMOTE_ALPN {
                    transport.handle_connection(connection).await;
                } else if alpn == iroh_blobs::ALPN {
                    if let Err(error) = blobs.accept(connection).await {
                        transport.log(format!("iroh_host_blobs failed error={error}"));
                    }
                } else {
                    transport.log(format!(
                        "iroh_host_unsupported_alpn alpn={}",
                        String::from_utf8_lossy(&alpn)
                    ));
                }
            });
        }
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

    async fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(mut peers) = self.peers.lock() {
            peers.clear();
        }
        self.endpoint.close().await;
    }
}

pub struct RemoteIrohControllerTransport {
    endpoint: Endpoint,
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
        let endpoint = endpoint_builder(relay_url.as_ref(), &relay_authentication, None)
            .alpns(vec![iroh_blobs::ALPN.to_vec()])
            .bind()
            .await
            .map_err(|error| format!("iroh controller bind failed: {error}"))?;
        on_state(String::new(), "connecting".to_string());
        let connection = endpoint
            .connect(endpoint_addr, CODUX_REMOTE_ALPN)
            .await
            .map_err(|error| format!("iroh controller connect failed: {error}"))?;
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
        let blob_store = MemStore::new();
        let upload_control_tx = tx.clone();
        let transport = Arc::new(Self {
            endpoint,
            connection: Mutex::new(Some(connection.clone())),
            tx: Mutex::new(Some(tx)),
            terminal_tx: Mutex::new(Some(terminal_tx)),
            upload_tx: Mutex::new(Some(upload_tx)),
            closed: Arc::new(AtomicBool::new(false)),
        });
        publish_path_state(&connection, &on_state);
        spawn_path_watcher(connection.clone(), Arc::clone(&on_state));
        spawn_blob_accept_loop(
            transport.endpoint.clone(),
            BlobsProtocol::new(blob_store.as_ref(), None),
            Arc::clone(&transport.closed),
            on_log.clone(),
        );
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

    async fn shutdown(&self) {
        self.close_sender();
        self.endpoint.close().await;
    }
}

fn spawn_blob_accept_loop(
    endpoint: Endpoint,
    blobs: BlobsProtocol,
    closed: Arc<AtomicBool>,
    on_log: Option<RemoteTransportLogHandler>,
) {
    tokio::spawn(async move {
        while !closed.load(Ordering::SeqCst) {
            let Some(incoming) = endpoint.accept().await else {
                break;
            };
            let blobs = blobs.clone();
            let on_log = on_log.clone();
            tokio::spawn(async move {
                let mut accepting = match incoming.accept() {
                    Ok(accepting) => accepting,
                    Err(error) => {
                        log_transport(&on_log, format!("iroh_blob_accept failed error={error}"));
                        return;
                    }
                };
                let alpn = match accepting.alpn().await {
                    Ok(alpn) => alpn,
                    Err(error) => {
                        log_transport(&on_log, format!("iroh_blob_alpn failed error={error}"));
                        return;
                    }
                };
                let connection = match accepting.await {
                    Ok(connection) => connection,
                    Err(error) => {
                        log_transport(&on_log, format!("iroh_blob_connect failed error={error}"));
                        return;
                    }
                };
                if alpn == iroh_blobs::ALPN {
                    if let Err(error) = blobs.accept(connection).await {
                        log_transport(&on_log, format!("iroh_blob_accept failed error={error}"));
                    }
                } else {
                    log_transport(
                        &on_log,
                        format!(
                            "iroh_blob_unsupported_alpn alpn={}",
                            String::from_utf8_lossy(&alpn)
                        ),
                    );
                }
            });
        }
    });
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
