use async_trait::async_trait;
pub use codux_protocol::RemoteTransportKind;
use codux_protocol::{RemoteEnvelope, RemoteTransportPairingRequest};
use std::sync::Arc;
use std::sync::Once;

mod control_messages;
mod health;
mod local_memory;
mod url_rules;
mod webrtc;
mod websocket;

use health::{ControllerHealthState, ControllerHealthTransport};
pub use local_memory::{LocalMemoryTransport, LocalMemoryTransportHub};
pub use webrtc::RemoteWebRtcHostTransport;
use webrtc::{
    DirectRouteState, RelayRouteState, RemoteControllerCompositeTransport,
    controller_relay_state_handler,
};
pub use websocket::{RemoteWebSocketControllerTransport, RemoteWebSocketHostTransport};

pub use url_rules::{
    CHINA_RELAY_SERVER_URL, DEFAULT_RELAY_SERVER_URL, GLOBAL_RELAY_SERVER_URL, RemoteTurnConfig,
    preferred_controller_transport_kind, preferred_pairing_transport_kind,
    remote_client_websocket_url, remote_pairing_code_url, remote_pairing_ticket_url,
    remote_pairing_websocket_url, remote_relay_preset_for_url, remote_relay_url_for_preset,
    remote_server_url, remote_stun_urls, remote_turn_config_from_env, remote_url,
};

pub type RemoteTransportMessageHandler = Arc<dyn Fn(String, Vec<u8>) + Send + Sync + 'static>;
pub type RemoteTransportStateHandler = Arc<dyn Fn(String, String) + Send + Sync + 'static>;
pub type RemoteTransportPairingHandler =
    Arc<dyn Fn(RemoteTransportPairingRequest) + Send + Sync + 'static>;
pub type RemoteTransportControlHandler =
    Arc<dyn Fn(String, RemoteEnvelope) -> bool + Send + Sync + 'static>;
pub type RemoteTransportLogHandler = Arc<dyn Fn(String) + Send + Sync + 'static>;

#[async_trait]
pub trait RemoteTransport: Send + Sync {
    fn kind(&self) -> RemoteTransportKind;
    fn send(&self, data: Vec<u8>, device_id: Option<&str>) -> bool;
    fn mark_direct_unhealthy(&self) -> bool {
        false
    }
    fn probe_preferred_route(&self) -> bool {
        false
    }
    async fn shutdown(&self);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteControllerTransportConfig {
    pub server_url: String,
    pub host_id: String,
    pub device_id: String,
    pub device_token: String,
    pub transports: Vec<RemoteTransportCandidate>,
    pub stun_urls: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteTransportCandidate {
    pub kind: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteHostTransportConfig {
    pub server_url: String,
    pub host_id: String,
    pub host_token: String,
    pub stun_urls: Vec<String>,
}

pub struct RemoteTransportFactory;

impl RemoteTransportFactory {
    pub async fn connect_host(
        config: &RemoteHostTransportConfig,
        on_message: RemoteTransportMessageHandler,
        on_state: RemoteTransportStateHandler,
        on_pairing: RemoteTransportPairingHandler,
        on_log: Option<RemoteTransportLogHandler>,
    ) -> Result<Arc<dyn RemoteTransport>, String> {
        install_rustls_crypto_provider();
        let relay = remote_server_url(&config.server_url);
        let ws_url = remote_url(
            &relay,
            "/ws/host",
            &[
                ("hostId", config.host_id.as_str()),
                ("token", config.host_token.as_str()),
            ],
            true,
        )?;
        let transport = RemoteWebRtcHostTransport::connect(
            config, ws_url, on_message, on_state, on_pairing, on_log,
        )
        .await?;
        Ok(transport)
    }

    pub async fn connect_controller(
        config: &RemoteControllerTransportConfig,
        on_message: RemoteTransportMessageHandler,
        on_state: RemoteTransportStateHandler,
        on_log: Option<RemoteTransportLogHandler>,
    ) -> Result<Arc<dyn RemoteTransport>, String> {
        install_rustls_crypto_provider();
        let health_state = Arc::new(ControllerHealthState::new(
            config.device_id.clone(),
            Arc::clone(&on_message),
            Arc::clone(&on_state),
            on_log.clone(),
        ));
        let on_message = {
            let health_state = Arc::clone(&health_state);
            Arc::new(move |device_id, data| health_state.handle_message(device_id, data))
                as RemoteTransportMessageHandler
        };
        let on_state = {
            let health_state = Arc::clone(&health_state);
            Arc::new(move |device_id, state| health_state.handle_state(device_id, state))
                as RemoteTransportStateHandler
        };
        let kind = preferred_controller_transport_kind(
            config
                .transports
                .iter()
                .map(|candidate| (candidate.kind.as_str(), candidate.url.as_str())),
        );
        let relay = config
            .transports
            .iter()
            .find(|candidate| {
                candidate.kind == "websocketRelay" && !candidate.url.trim().is_empty()
            })
            .map(|candidate| candidate.url.as_str())
            .unwrap_or(config.server_url.as_str());
        let ws_url = remote_client_websocket_url(
            relay,
            &config.host_id,
            &config.device_id,
            Some(&config.device_token),
        )?;
        let transport: Arc<dyn RemoteTransport> = match kind {
            "webRtc" => {
                let direct_route = DirectRouteState::default();
                let relay_route = RelayRouteState::default();
                let relay_state = controller_relay_state_handler(
                    relay_route.clone(),
                    direct_route.clone(),
                    Arc::clone(&on_state),
                );
                let relay_transport = RemoteWebSocketControllerTransport::connect(
                    ws_url,
                    Arc::clone(&on_message),
                    relay_state,
                    on_log.clone(),
                )
                .await?;
                RemoteControllerCompositeTransport::connect(
                    config,
                    relay_transport,
                    direct_route,
                    relay_route,
                    on_message,
                    Arc::clone(&on_state),
                )
                .await
                .map(|transport| transport as Arc<dyn RemoteTransport>)
            }
            "websocketRelay" => RemoteWebSocketControllerTransport::connect(
                ws_url,
                Arc::clone(&on_message),
                Arc::clone(&on_state),
                on_log.clone(),
            )
            .await
            .map(|transport| transport as Arc<dyn RemoteTransport>),
            _ => Err("missing supported controller transport candidate".to_string()),
        }?;
        Ok(ControllerHealthTransport::start(transport, health_state))
    }
}

fn install_rustls_crypto_provider() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(test)]
mod tests;
