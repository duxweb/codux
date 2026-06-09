use super::relay::{remote_server_url, remote_url};
use super::transport::{
    RemoteTransport, RemoteTransportMessageHandler, RemoteTransportPairingHandler,
    RemoteTransportStateHandler,
};
use super::types::RemoteSettings;
use super::webrtc_transport::RemoteWebRtcHostTransport;
use std::sync::Arc;

pub(crate) struct RemoteTransportFactory;

impl RemoteTransportFactory {
    pub(crate) async fn connect_host(
        settings: &RemoteSettings,
        on_message: RemoteTransportMessageHandler,
        on_state: RemoteTransportStateHandler,
        on_pairing: RemoteTransportPairingHandler,
    ) -> Result<Arc<dyn RemoteTransport>, String> {
        let relay = remote_server_url(&settings.server_url);
        let ws_url = remote_url(
            &relay,
            "/ws/host",
            &[
                ("hostId", settings.host_id.as_str()),
                ("token", settings.host_token.as_str()),
            ],
            true,
        )?;
        let transport =
            RemoteWebRtcHostTransport::connect(settings, ws_url, on_message, on_state, on_pairing)
                .await?;
        Ok(transport)
    }
}
