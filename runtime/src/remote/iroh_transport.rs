use super::types::RemoteEnvelope;
#[cfg(test)]
use iroh::EndpointId;
use iroh::{
    Endpoint, EndpointAddr, RelayMode, RelayUrl, SecretKey,
    endpoint::{Connection, presets},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
#[cfg(test)]
use std::net::SocketAddr;
use std::str::FromStr;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
    time::{Duration, timeout},
};

pub(crate) const CODUX_REMOTE_ALPN: &[u8] = b"codux/remote/iroh/v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteIrohNodeAddr {
    pub(crate) node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) relay_url: Option<String>,
    #[serde(default)]
    pub(crate) direct_addresses: Vec<String>,
}

impl RemoteIrohNodeAddr {
    pub(crate) fn from_endpoint_addr(addr: EndpointAddr) -> Self {
        Self {
            node_id: addr.id.to_string(),
            relay_url: addr.relay_urls().next().map(|url| url.to_string()),
            direct_addresses: addr.ip_addrs().map(|addr| addr.to_string()).collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn to_endpoint_addr(&self) -> Result<EndpointAddr, String> {
        let node_id =
            EndpointId::from_str(self.node_id.trim()).map_err(|error| error.to_string())?;
        let relay_url = self
            .relay_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(RelayUrl::from_str)
            .transpose()
            .map_err(|error| error.to_string())?;
        let mut addr = EndpointAddr::new(node_id);
        if let Some(relay_url) = relay_url {
            addr = addr.with_relay_url(relay_url);
        }
        for direct in self
            .direct_addresses
            .iter()
            .filter_map(|value| SocketAddr::from_str(value.trim()).ok())
        {
            addr = addr.with_ip_addr(direct);
        }
        Ok(addr)
    }
}

pub(crate) fn iroh_relay_mode_from_url(value: &str) -> Result<RelayMode, String> {
    let value = value.trim();
    if value.is_empty() || value == "iroh://default" {
        return Ok(RelayMode::Default);
    }
    let relay_url = RelayUrl::from_str(value).map_err(|error| error.to_string())?;
    Ok(RelayMode::custom([relay_url]))
}

#[derive(Clone, Debug)]
pub(crate) struct RemoteIrohHandshake {
    pub(crate) device_id: String,
    pub(crate) device_name: String,
    pub(crate) device_public_key: String,
    pub(crate) pairing_id: Option<String>,
    pub(crate) pairing_code: Option<String>,
    pub(crate) pairing_secret: Option<String>,
}

type MessageHandler = Arc<dyn Fn(String, Vec<u8>) + Send + Sync + 'static>;
type StateHandler = Arc<dyn Fn(String, String) + Send + Sync + 'static>;
type PairingHandler = Arc<dyn Fn(RemoteIrohHandshake) + Send + Sync + 'static>;

pub(crate) struct RemoteIrohHostTransport {
    endpoint: Endpoint,
    peers: Mutex<HashMap<String, mpsc::UnboundedSender<Vec<u8>>>>,
    on_message: MessageHandler,
    on_state: StateHandler,
    on_pairing: PairingHandler,
}

pub(crate) fn iroh_secret_key_from_settings(value: &str) -> (SecretKey, String) {
    let decoded = super::crypto::remote_base64_url_decode(value)
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok());
    let secret_key = decoded
        .map(|bytes| SecretKey::from_bytes(&bytes))
        .unwrap_or_else(SecretKey::generate);
    let encoded = super::crypto::remote_base64_url_encode(&secret_key.to_bytes());
    (secret_key, encoded)
}

impl RemoteIrohHostTransport {
    pub(crate) async fn bind(
        secret_key: SecretKey,
        relay_url: &str,
        on_message: MessageHandler,
        on_state: StateHandler,
        on_pairing: PairingHandler,
    ) -> Result<Arc<Self>, String> {
        let relay_mode = iroh_relay_mode_from_url(relay_url)?;
        let endpoint = Endpoint::builder(presets::N0)
            .secret_key(secret_key)
            .alpns(vec![CODUX_REMOTE_ALPN.to_vec()])
            .relay_mode(relay_mode)
            .bind()
            .await
            .map_err(|error| error.to_string())?;
        let transport = Arc::new(Self {
            endpoint,
            peers: Mutex::new(HashMap::new()),
            on_message,
            on_state,
            on_pairing,
        });
        transport.spawn_accept_loop();
        Ok(transport)
    }

    pub(crate) async fn node_addr(&self) -> Result<RemoteIrohNodeAddr, String> {
        let _ = timeout(Duration::from_secs(3), self.endpoint.online()).await;
        let addr = RemoteIrohNodeAddr::from_endpoint_addr(self.endpoint.addr());
        if addr.relay_url.is_none() && addr.direct_addresses.is_empty() {
            return Err("Iroh Remote Host address has no relay or direct addresses.".to_string());
        }
        Ok(addr)
    }

    pub(crate) async fn shutdown(&self) {
        if let Ok(mut peers) = self.peers.lock() {
            peers.clear();
        }
        self.endpoint.close().await;
    }

    pub(crate) fn send(&self, data: Vec<u8>, device_id: Option<&str>) -> bool {
        let Some(device_id) = device_id else {
            crate::runtime_trace::runtime_trace("remote", "iroh_send drop reason=missing_device");
            return false;
        };
        let sent = self
            .peers
            .lock()
            .ok()
            .and_then(|peers| peers.get(device_id).cloned())
            .map(|tx| tx.send(data).is_ok())
            .unwrap_or(false);
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("iroh_send device={device_id} sent={sent}"),
        );
        sent
    }

    fn spawn_accept_loop(self: &Arc<Self>) {
        let owner = Arc::clone(self);
        crate::async_runtime::spawn(async move {
            while let Some(incoming) = owner.endpoint.accept().await {
                let owner = Arc::clone(&owner);
                crate::async_runtime::spawn(async move {
                    let Ok(connecting) = incoming.accept() else {
                        return;
                    };
                    let Ok(connection) = connecting.await else {
                        return;
                    };
                    owner.handle_connection(connection).await;
                });
            }
        });
    }

    async fn handle_connection(self: Arc<Self>, connection: Connection) {
        let peer = connection.remote_id().to_string();
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let mut device_id: Option<String> = None;
        let Ok((mut send, mut recv)) = connection.accept_bi().await else {
            if !peer.is_empty() {
                (self.on_state)(peer, "closed".to_string());
            }
            return;
        };
        loop {
            tokio::select! {
                inbound = read_frame(&mut recv) => {
                    let Ok(data) = inbound else {
                        break;
                    };
                    let Ok(raw) = serde_json::from_slice::<RemoteEnvelope>(&data) else {
                        let _ = write_frame(&mut send, br#"{"type":"error","payload":{"message":"Invalid remote envelope."}}"#).await;
                        continue;
                    };
                    if raw.kind == "pairing.request" {
                        if let Some(handshake) = pairing_handshake_from_envelope(&raw) {
                            device_id = Some(handshake.device_id.clone());
                            self.register_peer(&handshake.device_id, tx.clone());
                            (self.on_pairing)(handshake);
                        }
                    }
                    if device_id.is_none() {
                        device_id = raw
                            .device_id
                            .clone()
                            .filter(|value| !value.trim().is_empty());
                    }
                    if let Some(id) = device_id.clone() {
                        crate::runtime_trace::runtime_trace(
                            "remote",
                            &format!(
                                "iroh_recv raw_type={} device={} session={}",
                                raw.kind,
                                id,
                                raw.session_id.as_deref().unwrap_or("")
                            ),
                        );
                        self.register_peer(&id, tx.clone());
                        (self.on_message)(id, data);
                    }
                    let _ = write_frame(&mut send, br#"{"ok":true}"#).await;
                }
                outbound = rx.recv() => {
                    let Some(data) = outbound else {
                        break;
                    };
                    if write_frame(&mut send, &data).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = send.finish();
        if let Some(id) = device_id {
            let mut remove = false;
            if let Ok(peers) = self.peers.lock() {
                remove = peers
                    .get(&id)
                    .map(|peer_tx| peer_tx.same_channel(&tx))
                    .unwrap_or(false);
            }
            if remove {
                if let Ok(mut peers) = self.peers.lock() {
                    peers.remove(&id);
                }
                (self.on_state)(id, "closed".to_string());
            }
        } else if !peer.is_empty() {
            (self.on_state)(peer, "closed".to_string());
        }
    }

    fn register_peer(&self, device_id: &str, tx: mpsc::UnboundedSender<Vec<u8>>) {
        if let Ok(mut peers) = self.peers.lock() {
            peers.insert(device_id.to_string(), tx);
        }
        crate::runtime_trace::runtime_trace("remote", &format!("iroh_peer device={device_id}"));
        (self.on_state)(device_id.to_string(), "connected".to_string());
    }
}

#[cfg(test)]
pub(crate) async fn iroh_client_send(
    addr: RemoteIrohNodeAddr,
    message: Vec<u8>,
) -> Result<Vec<u8>, String> {
    iroh_client_send_with_hold(addr, message, None).await
}

#[cfg(test)]
pub(crate) async fn iroh_client_send_with_hold(
    addr: RemoteIrohNodeAddr,
    message: Vec<u8>,
    hold: Option<std::time::Duration>,
) -> Result<Vec<u8>, String> {
    let endpoint = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Default)
        .bind()
        .await
        .map_err(|error| error.to_string())?;
    let connection = endpoint
        .connect(addr.to_endpoint_addr()?, CODUX_REMOTE_ALPN)
        .await
        .map_err(|error| error.to_string())?;
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|error| error.to_string())?;
    write_frame(&mut send, &message).await?;
    let response = read_frame(&mut recv).await?;
    if let Some(hold) = hold {
        tokio::time::sleep(hold).await;
    }
    send.finish().map_err(|error| error.to_string())?;
    connection.close(0_u32.into(), b"done");
    endpoint.close().await;
    Ok(response)
}

async fn write_frame<W>(writer: &mut W, data: &[u8]) -> Result<(), String>
where
    W: AsyncWriteExt + Unpin,
{
    let len = u32::try_from(data.len()).map_err(|_| "Remote message is too large.".to_string())?;
    writer
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|error| error.to_string())?;
    writer
        .write_all(data)
        .await
        .map_err(|error| error.to_string())
}

async fn read_frame<R>(reader: &mut R) -> Result<Vec<u8>, String>
where
    R: AsyncReadExt + Unpin,
{
    let mut header = [0_u8; 4];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|error| error.to_string())?;
    let len = u32::from_be_bytes(header) as usize;
    if len > 8 * 1024 * 1024 {
        return Err("Remote message is too large.".to_string());
    }
    let mut data = vec![0_u8; len];
    reader
        .read_exact(&mut data)
        .await
        .map_err(|error| error.to_string())?;
    Ok(data)
}

fn pairing_handshake_from_envelope(envelope: &RemoteEnvelope) -> Option<RemoteIrohHandshake> {
    let device_id = envelope
        .device_id
        .clone()
        .filter(|value| !value.trim().is_empty())?;
    let device_name = envelope
        .payload
        .get("deviceName")
        .and_then(Value::as_str)
        .unwrap_or("Mobile Device")
        .to_string();
    let device_public_key = envelope
        .payload
        .get("devicePublicKey")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some(RemoteIrohHandshake {
        device_id,
        device_name,
        device_public_key,
        pairing_id: envelope
            .payload
            .get("pairingId")
            .and_then(Value::as_str)
            .map(str::to_string),
        pairing_code: envelope
            .payload
            .get("code")
            .and_then(Value::as_str)
            .map(str::to_string),
        pairing_secret: envelope
            .payload
            .get("secret")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::mpsc::UnboundedReceiver;

    #[test]
    fn iroh_long_stream_preserves_burst_frame_order() {
        crate::async_runtime::block_on(async {
            let received = Arc::new(Mutex::new(Vec::<usize>::new()));
            let received_for_handler = Arc::clone(&received);
            let (peer_tx, mut peer_rx) = mpsc::unbounded_channel::<String>();
            let transport = RemoteIrohHostTransport::bind(
                SecretKey::generate(),
                "iroh://default",
                Arc::new(move |_device_id, data| {
                    if let Ok(value) = serde_json::from_slice::<Value>(&data) {
                        if let Some(index) = value
                            .get("payload")
                            .and_then(|payload| payload.get("index"))
                            .and_then(Value::as_u64)
                        {
                            if let Ok(mut received) = received_for_handler.lock() {
                                received.push(index as usize);
                            }
                        }
                    }
                }),
                Arc::new(move |device_id, state| {
                    if state == "connected" {
                        let _ = peer_tx.send(device_id);
                    }
                }),
                Arc::new(|_| {}),
            )
            .await
            .expect("bind iroh host");
            let addr = transport.node_addr().await.expect("node addr");
            let outbound = run_burst_client(addr, 200, 5).await.expect("burst client");
            let device_id = recv_with_timeout(&mut peer_rx)
                .await
                .expect("peer connected");

            for index in 0..5 {
                let data = serde_json::to_vec(&json!({
                    "type": "terminal.output",
                    "deviceId": device_id,
                    "payload": { "index": index }
                }))
                .expect("outbound json");
                assert!(transport.send(data, Some(&device_id)));
            }

            let echoed = outbound
                .await
                .expect("join burst client")
                .expect("outbound");
            assert_eq!(echoed, (0..5).collect::<Vec<_>>());

            for _ in 0..40 {
                let current = received
                    .lock()
                    .map(|received| received.clone())
                    .unwrap_or_default();
                if current.len() == 200 {
                    assert_eq!(current, (0..200).collect::<Vec<_>>());
                    transport.shutdown().await;
                    return;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            panic!("host did not receive ordered burst");
        });
    }

    async fn recv_with_timeout(rx: &mut UnboundedReceiver<String>) -> Option<String> {
        tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .ok()
            .flatten()
    }

    #[test]
    fn iroh_relay_mode_accepts_default_and_custom_url() {
        assert!(matches!(
            iroh_relay_mode_from_url("").expect("empty relay"),
            RelayMode::Default
        ));
        assert!(matches!(
            iroh_relay_mode_from_url("iroh://default").expect("default relay"),
            RelayMode::Default
        ));
        assert!(matches!(
            iroh_relay_mode_from_url(" https://relay.example.com ").expect("custom relay"),
            RelayMode::Custom(_)
        ));
    }

    async fn run_burst_client(
        addr: RemoteIrohNodeAddr,
        inbound_frames: usize,
        outbound_frames: usize,
    ) -> Result<tokio::task::JoinHandle<Result<Vec<usize>, String>>, String> {
        let endpoint = Endpoint::builder(presets::N0)
            .relay_mode(RelayMode::Default)
            .bind()
            .await
            .map_err(|error| error.to_string())?;
        let connection = endpoint
            .connect(addr.to_endpoint_addr()?, CODUX_REMOTE_ALPN)
            .await
            .map_err(|error| error.to_string())?;
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| error.to_string())?;
        for index in 0..inbound_frames {
            let data = serde_json::to_vec(&json!({
                "type": "host.info",
                "deviceId": "burst-device",
                "payload": { "index": index }
            }))
            .map_err(|error| error.to_string())?;
            write_frame(&mut send, &data).await?;
            let _ = read_frame(&mut recv).await?;
        }
        Ok(tokio::spawn(async move {
            let mut received = Vec::new();
            while received.len() < outbound_frames {
                let data = read_frame(&mut recv).await?;
                let value =
                    serde_json::from_slice::<Value>(&data).map_err(|error| error.to_string())?;
                let Some(index) = value
                    .get("payload")
                    .and_then(|payload| payload.get("index"))
                    .and_then(Value::as_u64)
                else {
                    return Err("missing outbound index".to_string());
                };
                received.push(index as usize);
            }
            send.finish().map_err(|error| error.to_string())?;
            connection.close(0_u32.into(), b"done");
            endpoint.close().await;
            Ok(received)
        }))
    }
}

#[cfg(test)]
pub(crate) fn iroh_pairing_request_payload(
    pairing_id: &str,
    code: &str,
    secret: &str,
    name: &str,
    public_key: &str,
) -> Value {
    json!({
        "pairingId": pairing_id,
        "code": code,
        "secret": secret,
        "deviceName": name,
        "devicePublicKey": public_key,
    })
}
