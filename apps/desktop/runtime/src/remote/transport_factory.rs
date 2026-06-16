use super::types::RemoteSettings;
use codux_remote_transport::{
    RemoteHostTransportConfig, RemoteTransport, RemoteTransportFactory as SharedTransportFactory,
    RemoteTransportMessageHandler, RemoteTransportPairingHandler, RemoteTransportStateHandler,
    RemoteTransportUploadHandler,
};
use std::sync::Arc;

pub(crate) struct RemoteTransportFactory;

impl RemoteTransportFactory {
    pub(crate) async fn connect_host(
        settings: &RemoteSettings,
        on_message: RemoteTransportMessageHandler,
        on_upload: RemoteTransportUploadHandler,
        on_state: RemoteTransportStateHandler,
        on_pairing: RemoteTransportPairingHandler,
    ) -> Result<Arc<dyn RemoteTransport>, String> {
        SharedTransportFactory::connect_host(
            &host_transport_config(settings),
            on_message,
            on_upload,
            on_state,
            on_pairing,
            Some(Arc::new(|message| {
                crate::runtime_trace::runtime_trace("remote", &message);
            })),
        )
        .await
    }
}

pub(crate) fn host_transport_config(settings: &RemoteSettings) -> RemoteHostTransportConfig {
    RemoteHostTransportConfig {
        relay_url: settings.relay_url.clone(),
        relay_preset: settings.relay_preset.clone(),
        iroh_relay_url: codux_remote_transport::iroh_relay_url_for_preset(
            &settings.relay_preset,
            &settings.relay_url,
        ),
        iroh_relay_authentication: settings.relay_authentication.clone(),
        host_id: settings.host_id.clone(),
        host_token: settings.host_token.clone(),
    }
}
