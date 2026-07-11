use super::types::RemoteSettings;
use codux_remote_transport::{
    RemoteControllerTransportConfig, RemoteHostTransportConfig, RemoteHostTransportHandlers,
    RemoteTransport, RemoteTransportAuthorizationHandler,
    RemoteTransportFactory as SharedTransportFactory, RemoteTransportMessageHandler,
    RemoteTransportPairingHandler, RemoteTransportStateHandler, RemoteTransportUploadHandler,
    WebTunnelTcpConnectHandler,
};
use std::sync::Arc;

pub(crate) struct RemoteTransportFactory;

impl RemoteTransportFactory {
    /// Dial OUT to a remote host as a controller (the inverse of `connect_host`).
    /// The desktop uses this to drive another device's domains.
    pub(crate) async fn connect_controller(
        config: &RemoteControllerTransportConfig,
        on_message: RemoteTransportMessageHandler,
        on_state: RemoteTransportStateHandler,
    ) -> Result<Arc<dyn RemoteTransport>, String> {
        SharedTransportFactory::connect_controller(
            config,
            on_message,
            on_state,
            Some(Arc::new(|message| {
                crate::runtime_trace::runtime_trace("remote-controller", &message);
            })),
        )
        .await
    }

    pub(crate) async fn connect_host(
        settings: &RemoteSettings,
        on_message: RemoteTransportMessageHandler,
        on_upload: RemoteTransportUploadHandler,
        on_state: RemoteTransportStateHandler,
        on_pairing: RemoteTransportPairingHandler,
        on_authorize: RemoteTransportAuthorizationHandler,
        on_web_tunnel_tcp_connect: Option<WebTunnelTcpConnectHandler>,
    ) -> Result<Arc<dyn RemoteTransport>, String> {
        SharedTransportFactory::connect_host(
            &host_transport_config(settings),
            RemoteHostTransportHandlers {
                on_message,
                on_upload,
                on_state,
                on_pairing,
                on_authorize,
                on_web_tunnel_tcp_connect,
                on_log: Some(Arc::new(|message| {
                    crate::runtime_trace::runtime_trace("remote", &message);
                })),
            },
        )
        .await
    }
}

pub(crate) fn host_transport_config(settings: &RemoteSettings) -> RemoteHostTransportConfig {
    let relay_preset = codux_remote_transport::normalize_remote_relay_preset(
        &settings.relay_preset,
        &settings.relay_url,
    );
    RemoteHostTransportConfig {
        relay_url: settings.relay_url.clone(),
        relay_preset: relay_preset.clone(),
        iroh_relay_url: codux_remote_transport::iroh_relay_url_for_preset(
            &relay_preset,
            &settings.relay_url,
        ),
        iroh_relay_authentication: settings.relay_authentication.clone(),
        host_id: settings.host_id.clone(),
        host_token: settings.host_token.clone(),
    }
}
