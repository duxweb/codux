use async_trait::async_trait;
use bytes::BytesMut;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelState};
use webrtc::peer_connection::{
    register_default_interceptors, MediaEngine, PeerConnection, PeerConnectionBuilder,
    PeerConnectionEventHandler, RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceServer,
    RTCPeerConnectionIceEvent, RTCPeerConnectionState, RTCSessionDescription, Registry,
};
use webrtc::runtime::{default_runtime, Runtime};

const UPLOAD_CHANNEL_LABEL: &str = "codux-upload";
const TERMINAL_BUFFERED_AMOUNT_HIGH_WATERMARK: u32 = 192 * 1024;
const UPLOAD_BUFFERED_AMOUNT_HIGH_WATERMARK: u32 = 512 * 1024;

#[derive(Clone, Copy)]
pub enum RemoteP2PLane {
    Terminal,
    Upload,
}

pub struct RemoteP2PSignal {
    pub device_id: String,
    pub kind: String,
    pub payload: Value,
}

type SignalHandler = Arc<dyn Fn(RemoteP2PSignal) + Send + Sync + 'static>;
type MessageHandler = Arc<dyn Fn(String, Vec<u8>) + Send + Sync + 'static>;
type StateHandler = Arc<dyn Fn(String, String) + Send + Sync + 'static>;

pub struct RemoteP2PHostTransport {
    peers: Mutex<HashMap<String, Arc<RemoteP2PPeer>>>,
    on_signal: SignalHandler,
    on_message: MessageHandler,
    on_state: StateHandler,
    runtime: Arc<dyn Runtime>,
}

impl RemoteP2PHostTransport {
    pub fn new(
        on_signal: SignalHandler,
        on_message: MessageHandler,
        on_state: StateHandler,
    ) -> Result<Arc<Self>, String> {
        let runtime = default_runtime().ok_or_else(|| "WebRTC runtime unavailable.".to_string())?;
        Ok(Arc::new(Self {
            peers: Mutex::new(HashMap::new()),
            on_signal,
            on_message,
            on_state,
            runtime,
        }))
    }

    pub async fn close(&self, device_id: &str) {
        let peer = self
            .peers
            .lock()
            .ok()
            .and_then(|mut peers| peers.remove(device_id));
        if let Some(peer) = peer {
            peer.close().await;
        }
        self.notify_state(device_id, "closed", None);
    }

    pub async fn send(&self, data: Vec<u8>, device_id: Option<&str>, lane: RemoteP2PLane) -> bool {
        let Some(device_id) = device_id else {
            return false;
        };
        let peer = self
            .peers
            .lock()
            .ok()
            .and_then(|peers| peers.get(device_id).cloned());
        match peer {
            Some(peer) if peer.is_open().await => peer.send(data, lane).await,
            _ => false,
        }
    }

    pub async fn handle_offer(self: &Arc<Self>, device_id: String, payload: Value) {
        let Some(sdp) = payload.get("sdp").and_then(Value::as_str) else {
            return;
        };
        if sdp.trim().is_empty() {
            return;
        }
        let peer = match self.make_peer(&device_id).await {
            Ok(peer) => peer,
            Err(error) => {
                self.notify_state(&device_id, "failed", Some(error));
                return;
            }
        };
        let offer = match RTCSessionDescription::offer(sdp.to_string()) {
            Ok(offer) => offer,
            Err(error) => {
                self.notify_state(&device_id, "failed", Some(error.to_string()));
                return;
            }
        };
        if let Err(error) = peer.pc.set_remote_description(offer).await {
            self.notify_state(&device_id, "failed", Some(error.to_string()));
            return;
        }
        let answer = match peer.pc.create_answer(None).await {
            Ok(answer) => answer,
            Err(error) => {
                self.notify_state(&device_id, "failed", Some(error.to_string()));
                return;
            }
        };
        if let Err(error) = peer.pc.set_local_description(answer).await {
            self.notify_state(&device_id, "failed", Some(error.to_string()));
            return;
        }
        let Some(answer) = peer.pc.local_description().await else {
            self.notify_state(
                &device_id,
                "failed",
                Some("Missing local SDP answer.".to_string()),
            );
            return;
        };
        (self.on_signal)(RemoteP2PSignal {
            device_id,
            kind: "p2p.answer".to_string(),
            payload: json!({
                "type": "answer",
                "sdp": answer.sdp,
            }),
        });
    }

    pub async fn handle_candidate(self: &Arc<Self>, device_id: String, payload: Value) {
        let candidate = payload
            .get("candidate")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if candidate.trim().is_empty() {
            return;
        }
        let peer = match self.make_peer(&device_id).await {
            Ok(peer) => peer,
            Err(error) => {
                self.notify_state(&device_id, "failed", Some(error));
                return;
            }
        };
        let sdp_mline_index = payload
            .get("sdpMLineIndex")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .or_else(|| {
                payload
                    .get("sdpMLineIndex")
                    .and_then(Value::as_i64)
                    .and_then(|value| u16::try_from(value).ok())
            });
        let ice = RTCIceCandidateInit {
            candidate: candidate.to_string(),
            sdp_mid: payload
                .get("sdpMid")
                .and_then(Value::as_str)
                .map(str::to_string),
            sdp_mline_index,
            username_fragment: None,
            url: None,
        };
        if let Err(error) = peer.pc.add_ice_candidate(ice).await {
            self.notify_state(&device_id, "failed", Some(error.to_string()));
        }
    }

    async fn make_peer(self: &Arc<Self>, device_id: &str) -> Result<Arc<RemoteP2PPeer>, String> {
        if let Some(existing) = self
            .peers
            .lock()
            .ok()
            .and_then(|peers| peers.get(device_id).cloned())
        {
            return Ok(existing);
        }

        let mut media = MediaEngine::default();
        media
            .register_default_codecs()
            .map_err(|error| error.to_string())?;
        let registry = register_default_interceptors(Registry::new(), &mut media)
            .map_err(|error| format!("WebRTC interceptor registration failed: {error}"))?;
        let handler = Arc::new(RemoteP2PEventHandler {
            device_id: device_id.to_string(),
            owner: Arc::downgrade(self),
        });
        let pc = PeerConnectionBuilder::new()
            .with_configuration(
                RTCConfigurationBuilder::new()
                    .with_ice_servers(vec![RTCIceServer {
                        urls: remote_p2p_ice_server_urls(),
                        ..Default::default()
                    }])
                    .build(),
            )
            .with_media_engine(media)
            .with_interceptor_registry(registry)
            .with_handler(handler)
            .with_runtime(self.runtime.clone())
            .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
            .build()
            .await
            .map_err(|error| error.to_string())?;
        let peer = Arc::new(RemoteP2PPeer {
            pc: Arc::new(pc),
            terminal_channel: Mutex::new(None),
            upload_channel: Mutex::new(None),
        });
        let mut duplicate: Option<Arc<RemoteP2PPeer>> = None;
        if let Ok(mut peers) = self.peers.lock() {
            if let Some(existing) = peers.get(device_id).cloned() {
                duplicate = Some(existing);
            } else {
                peers.insert(device_id.to_string(), peer.clone());
            }
        }
        if let Some(existing) = duplicate {
            peer.close().await;
            return Ok(existing);
        }
        self.notify_state(device_id, "connecting", None);
        Ok(peer)
    }

    fn notify_state(&self, device_id: &str, state: &str, error: Option<String>) {
        let mut payload = json!({ "state": state });
        if let Some(error) = error {
            payload["error"] = json!(error);
        }
        (self.on_state)(device_id.to_string(), state.to_string());
        (self.on_signal)(RemoteP2PSignal {
            device_id: device_id.to_string(),
            kind: "p2p.state".to_string(),
            payload,
        });
    }

    fn handle_data_channel(self: &Arc<Self>, device_id: String, dc: Arc<dyn DataChannel>) {
        let owner = Arc::clone(self);
        let runtime = self.runtime.clone();
        runtime.spawn(Box::pin(async move {
            let label = dc.label().await.unwrap_or_default();
            let is_upload = label == UPLOAD_CHANNEL_LABEL;
            let peer = owner
                .peers
                .lock()
                .ok()
                .and_then(|peers| peers.get(&device_id).cloned());
            if let Some(peer) = peer.as_ref() {
                peer.set_channel(is_upload, dc.clone());
            }
            let high_watermark = if is_upload {
                UPLOAD_BUFFERED_AMOUNT_HIGH_WATERMARK
            } else {
                TERMINAL_BUFFERED_AMOUNT_HIGH_WATERMARK
            };
            let _ = dc.set_buffered_amount_high_threshold(high_watermark).await;
            let _ = dc
                .set_buffered_amount_low_threshold(high_watermark / 2)
                .await;

            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        if !is_upload {
                            owner.notify_state(&device_id, "connected", None);
                        }
                    }
                    DataChannelEvent::OnMessage(message) => {
                        (owner.on_message)(device_id.clone(), message.data.to_vec());
                    }
                    DataChannelEvent::OnClose => {
                        if let Some(peer) = peer.as_ref() {
                            peer.clear_channel(is_upload);
                        }
                        if !is_upload {
                            owner.notify_state(&device_id, "closed", None);
                        }
                        break;
                    }
                    DataChannelEvent::OnError => {
                        owner.notify_state(
                            &device_id,
                            "failed",
                            Some("WebRTC data channel error.".to_string()),
                        );
                    }
                    _ => {}
                }
            }
        }));
    }

    fn handle_local_candidate(&self, device_id: String, event: RTCPeerConnectionIceEvent) {
        if let Ok(candidate) = event.candidate.to_json() {
            if candidate.candidate.trim().is_empty() {
                return;
            }
            (self.on_signal)(RemoteP2PSignal {
                device_id,
                kind: "p2p.candidate".to_string(),
                payload: json!({
                    "candidate": candidate.candidate,
                    "sdpMid": candidate.sdp_mid,
                    "sdpMLineIndex": candidate.sdp_mline_index.unwrap_or(0),
                }),
            });
        }
    }

    fn handle_connection_state(&self, device_id: String, state: RTCPeerConnectionState) {
        match state {
            RTCPeerConnectionState::Connected => {}
            RTCPeerConnectionState::Failed => self.notify_state(&device_id, "failed", None),
            RTCPeerConnectionState::Disconnected => {
                self.notify_state(&device_id, "disconnected", None)
            }
            RTCPeerConnectionState::Closed => self.notify_state(&device_id, "closed", None),
            _ => {}
        }
    }
}

struct RemoteP2PPeer {
    pc: Arc<dyn PeerConnection>,
    terminal_channel: Mutex<Option<Arc<dyn DataChannel>>>,
    upload_channel: Mutex<Option<Arc<dyn DataChannel>>>,
}

impl RemoteP2PPeer {
    async fn close(&self) {
        let terminal = self
            .terminal_channel
            .lock()
            .ok()
            .and_then(|mut value| value.take());
        let upload = self
            .upload_channel
            .lock()
            .ok()
            .and_then(|mut value| value.take());
        if let Some(channel) = terminal {
            let _ = channel.close().await;
        }
        if let Some(channel) = upload {
            let _ = channel.close().await;
        }
        let _ = self.pc.close().await;
    }

    async fn is_open(&self) -> bool {
        let channel = self
            .terminal_channel
            .lock()
            .ok()
            .and_then(|value| value.clone());
        match channel {
            Some(channel) => channel.ready_state().await.ok() == Some(RTCDataChannelState::Open),
            None => false,
        }
    }

    async fn send(&self, data: Vec<u8>, lane: RemoteP2PLane) -> bool {
        let channel = self.channel(lane);
        let Some(channel) = channel else {
            return false;
        };
        if channel.ready_state().await.ok() != Some(RTCDataChannelState::Open) {
            return false;
        }
        tokio::time::timeout(
            Duration::from_millis(250),
            channel.send(BytesMut::from(data.as_slice())),
        )
        .await
        .map(|result| result.is_ok())
        .unwrap_or(false)
    }

    fn set_channel(&self, upload: bool, channel: Arc<dyn DataChannel>) {
        let target = if upload {
            &self.upload_channel
        } else {
            &self.terminal_channel
        };
        if let Ok(mut current) = target.lock() {
            *current = Some(channel);
        }
    }

    fn clear_channel(&self, upload: bool) {
        let target = if upload {
            &self.upload_channel
        } else {
            &self.terminal_channel
        };
        if let Ok(mut current) = target.lock() {
            *current = None;
        }
    }

    fn channel(&self, lane: RemoteP2PLane) -> Option<Arc<dyn DataChannel>> {
        if matches!(lane, RemoteP2PLane::Upload) {
            return self
                .upload_channel
                .lock()
                .ok()
                .and_then(|value| value.clone());
        }
        self.terminal_channel
            .lock()
            .ok()
            .and_then(|value| value.clone())
    }
}

struct RemoteP2PEventHandler {
    device_id: String,
    owner: Weak<RemoteP2PHostTransport>,
}

#[async_trait]
impl PeerConnectionEventHandler for RemoteP2PEventHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Some(owner) = self.owner.upgrade() {
            owner.handle_local_candidate(self.device_id.clone(), event);
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if let Some(owner) = self.owner.upgrade() {
            owner.handle_connection_state(self.device_id.clone(), state);
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        if let Some(owner) = self.owner.upgrade() {
            owner.handle_data_channel(self.device_id.clone(), dc);
        }
    }
}

fn remote_p2p_ice_server_urls() -> Vec<String> {
    let domestic = vec!["stun:stun.miwifi.com:3478".to_string()];
    let global = vec![
        "stun:stun.l.google.com:19302".to_string(),
        "stun:global.stun.twilio.com:3478".to_string(),
    ];
    if prefers_domestic_stun() {
        domestic.into_iter().chain(global).collect()
    } else {
        global.into_iter().chain(domestic).collect()
    }
}

fn prefers_domestic_stun() -> bool {
    std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .map(|value| value.to_ascii_lowercase().starts_with("zh"))
        .unwrap_or(false)
}
