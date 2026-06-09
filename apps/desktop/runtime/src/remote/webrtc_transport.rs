use super::transport::{
    RemoteTransport, RemoteTransportKind, RemoteTransportMessageHandler,
    RemoteTransportPairingHandler, RemoteTransportStateHandler,
};
use super::types::{RemoteEnvelope, RemoteSettings};
use super::websocket_transport::RemoteWebSocketHostTransport;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use webrtc::api::APIBuilder;
use webrtc::api::media_engine::MediaEngine;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

pub(crate) struct RemoteWebRtcHostTransport {
    relay: Mutex<Option<Arc<RemoteWebSocketHostTransport>>>,
    peers: Mutex<HashMap<String, Arc<WebRtcPeer>>>,
    ice_servers: Vec<String>,
    on_message: RemoteTransportMessageHandler,
    on_state: RemoteTransportStateHandler,
}

struct WebRtcPeer {
    pc: Arc<RTCPeerConnection>,
    dc: Mutex<Option<Arc<RTCDataChannel>>>,
}

impl RemoteWebRtcHostTransport {
    pub(crate) async fn connect(
        settings: &RemoteSettings,
        ws_url: String,
        on_message: RemoteTransportMessageHandler,
        on_state: RemoteTransportStateHandler,
        on_pairing: RemoteTransportPairingHandler,
    ) -> Result<Arc<Self>, String> {
        let transport = Arc::new(Self {
            relay: Mutex::new(None),
            peers: Mutex::new(HashMap::new()),
            ice_servers: super::relay::remote_stun_urls(),
            on_message: Arc::clone(&on_message),
            on_state: Arc::clone(&on_state),
        });
        let weak = Arc::downgrade(&transport);
        let relay = RemoteWebSocketHostTransport::connect(
            ws_url,
            on_message,
            on_state,
            on_pairing,
            Some(Arc::new(move |device_id, envelope| {
                if !envelope.kind.starts_with("webrtc.") {
                    return false;
                }
                if let Some(transport) = weak.upgrade() {
                    crate::async_runtime::spawn(async move {
                        transport.handle_signal(device_id, envelope).await;
                    });
                }
                true
            })),
        )
        .await?;
        if let Ok(mut current) = transport.relay.lock() {
            *current = Some(relay);
        }
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("webrtc_transport ready host={}", settings.host_id),
        );
        Ok(transport)
    }

    async fn handle_signal(self: Arc<Self>, device_id: String, envelope: RemoteEnvelope) {
        if device_id.trim().is_empty() {
            return;
        }
        match envelope.kind.as_str() {
            "webrtc.offer" => {
                if let Err(error) = self.handle_offer(&device_id, envelope.payload).await {
                    crate::runtime_trace::runtime_trace(
                        "remote",
                        &format!("webrtc_offer failed device={device_id} error={error}"),
                    );
                    (self.on_state)(device_id, "path=relay".to_string());
                }
            }
            "webrtc.ice" => {
                if let Err(error) = self.handle_ice(&device_id, envelope.payload).await {
                    crate::runtime_trace::runtime_trace(
                        "remote",
                        &format!("webrtc_ice failed device={device_id} error={error}"),
                    );
                }
            }
            _ => {}
        }
    }

    async fn handle_offer(&self, device_id: &str, payload: Value) -> Result<(), String> {
        let description = payload
            .get("description")
            .cloned()
            .ok_or_else(|| "Missing WebRTC offer description.".to_string())
            .and_then(session_description_from_value)?;
        let peer = self.create_peer(device_id.to_string()).await?;
        peer.pc
            .set_remote_description(description)
            .await
            .map_err(|error| error.to_string())?;
        let answer = peer
            .pc
            .create_answer(None)
            .await
            .map_err(|error| error.to_string())?;
        let mut gathering_complete = peer.pc.gathering_complete_promise().await;
        peer.pc
            .set_local_description(answer)
            .await
            .map_err(|error| error.to_string())?;
        let _ = gathering_complete.recv().await;
        let description = peer
            .pc
            .local_description()
            .await
            .ok_or_else(|| "Missing WebRTC local answer.".to_string())?;
        self.send_signal(
            "webrtc.answer",
            Some(device_id),
            json!({ "description": description }),
        );
        Ok(())
    }

    async fn handle_ice(&self, device_id: &str, payload: Value) -> Result<(), String> {
        let candidate = payload
            .get("candidate")
            .cloned()
            .ok_or_else(|| "Missing WebRTC ICE candidate.".to_string())
            .and_then(|value| {
                serde_json::from_value::<RTCIceCandidateInit>(value)
                    .map_err(|error| error.to_string())
            })?;
        let peer = self
            .peers
            .lock()
            .ok()
            .and_then(|peers| peers.get(device_id).cloned())
            .ok_or_else(|| "Missing WebRTC peer.".to_string())?;
        peer.pc
            .add_ice_candidate(candidate)
            .await
            .map_err(|error| error.to_string())
    }

    async fn create_peer(&self, device_id: String) -> Result<Arc<WebRtcPeer>, String> {
        if let Some(peer) = self
            .peers
            .lock()
            .ok()
            .and_then(|peers| peers.get(&device_id).cloned())
        {
            let _ = peer.pc.close().await;
        }

        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .map_err(|error| error.to_string())?;
        let api = APIBuilder::new().with_media_engine(media_engine).build();
        let pc = Arc::new(
            api.new_peer_connection(RTCConfiguration {
                ice_servers: vec![RTCIceServer {
                    urls: self.ice_servers.clone(),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())?,
        );
        let peer = Arc::new(WebRtcPeer {
            pc: Arc::clone(&pc),
            dc: Mutex::new(None),
        });
        let weak_peer = Arc::downgrade(&peer);
        let message_handler = Arc::clone(&self.on_message);
        let state_handler = Arc::clone(&self.on_state);
        let channel_device_id = device_id.clone();
        pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
            let weak_peer = weak_peer.clone();
            let message_handler = Arc::clone(&message_handler);
            let state_handler = Arc::clone(&state_handler);
            let channel_device_id = channel_device_id.clone();
            Box::pin(async move {
                install_data_channel(
                    weak_peer,
                    dc,
                    channel_device_id,
                    message_handler,
                    state_handler,
                );
            })
        }));

        let state_device_id = device_id.clone();
        let state_handler = Arc::clone(&self.on_state);
        pc.on_peer_connection_state_change(Box::new(move |state| {
            let state_handler = Arc::clone(&state_handler);
            let state_device_id = state_device_id.clone();
            Box::pin(async move {
                if matches!(
                    state,
                    RTCPeerConnectionState::Failed
                        | RTCPeerConnectionState::Disconnected
                        | RTCPeerConnectionState::Closed
                ) {
                    state_handler(state_device_id, "path=relay".to_string());
                }
            })
        }));

        if let Ok(mut peers) = self.peers.lock() {
            peers.insert(device_id, Arc::clone(&peer));
        }
        Ok(peer)
    }

    fn send_signal(&self, kind: &str, device_id: Option<&str>, payload: Value) -> bool {
        let envelope = super::types::RemoteOutgoingEnvelope {
            kind: kind.to_string(),
            device_id: device_id.map(str::to_string),
            session_id: None,
            seq: None,
            payload,
        };
        let Ok(data) = serde_json::to_vec(&envelope) else {
            return false;
        };
        self.send_relay(data)
    }

    fn send_relay(&self, data: Vec<u8>) -> bool {
        let relay = self.relay.lock().ok().and_then(|value| value.clone());
        relay.map(|relay| relay.send(data, None)).unwrap_or(false)
    }
}

#[async_trait]
impl RemoteTransport for RemoteWebRtcHostTransport {
    fn kind(&self) -> RemoteTransportKind {
        RemoteTransportKind::WebRtc
    }

    fn send(&self, data: Vec<u8>, device_id: Option<&str>) -> bool {
        if let Some(device_id) = device_id {
            let channel = self
                .peers
                .lock()
                .ok()
                .and_then(|peers| peers.get(device_id).cloned())
                .and_then(|peer| peer.dc.lock().ok().and_then(|dc| dc.clone()));
            if let Some(channel) = channel {
                if let Ok(text) = String::from_utf8(data.clone()) {
                    let channel = Arc::clone(&channel);
                    crate::async_runtime::spawn(async move {
                        let _ = channel.send_text(text).await;
                    });
                    return true;
                }
            }
        }
        self.send_relay(data)
    }

    async fn shutdown(&self) {
        let relay = self.relay.lock().ok().and_then(|mut value| value.take());
        if let Some(relay) = relay {
            relay.shutdown().await;
        }
        let peers = self
            .peers
            .lock()
            .map(|mut peers| peers.drain().map(|(_, peer)| peer).collect::<Vec<_>>())
            .unwrap_or_default();
        for peer in peers {
            let _ = peer.pc.close().await;
        }
    }
}

fn install_data_channel(
    weak_peer: std::sync::Weak<WebRtcPeer>,
    dc: Arc<RTCDataChannel>,
    device_id: String,
    on_message: RemoteTransportMessageHandler,
    on_state: RemoteTransportStateHandler,
) {
    if let Some(peer) = weak_peer.upgrade() {
        if let Ok(mut current) = peer.dc.lock() {
            *current = Some(Arc::clone(&dc));
        }
    }
    let open_device_id = device_id.clone();
    let open_state = Arc::clone(&on_state);
    dc.on_open(Box::new(move || {
        let open_state = Arc::clone(&open_state);
        let open_device_id = open_device_id.clone();
        Box::pin(async move {
            open_state(open_device_id, "path=direct".to_string());
        })
    }));
    let close_device_id = device_id.clone();
    let close_state = Arc::clone(&on_state);
    dc.on_close(Box::new(move || {
        let close_state = Arc::clone(&close_state);
        let close_device_id = close_device_id.clone();
        Box::pin(async move {
            close_state(close_device_id, "path=relay".to_string());
        })
    }));
    dc.on_message(Box::new(move |message: DataChannelMessage| {
        let on_message = Arc::clone(&on_message);
        let device_id = device_id.clone();
        Box::pin(async move {
            on_message(device_id, message.data.to_vec());
        })
    }));
}

fn session_description_from_value(value: Value) -> Result<RTCSessionDescription, String> {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let sdp = value
        .get("sdp")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    match kind.as_str() {
        "offer" => RTCSessionDescription::offer(sdp).map_err(|error| error.to_string()),
        "answer" => RTCSessionDescription::answer(sdp).map_err(|error| error.to_string()),
        "pranswer" => RTCSessionDescription::pranswer(sdp).map_err(|error| error.to_string()),
        _ => Err("Unsupported WebRTC session description.".to_string()),
    }
}
