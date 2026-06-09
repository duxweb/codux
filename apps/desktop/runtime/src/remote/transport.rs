use async_trait::async_trait;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RemoteTransportKind {
    WebSocketRelay,
    WebRtc,
}

impl RemoteTransportKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::WebSocketRelay => "websocketRelay",
            Self::WebRtc => "webRtc",
        }
    }
}

pub(crate) type RemoteTransportMessageHandler =
    Arc<dyn Fn(String, Vec<u8>) + Send + Sync + 'static>;
pub(crate) type RemoteTransportStateHandler = Arc<dyn Fn(String, String) + Send + Sync + 'static>;
pub(crate) type RemoteTransportPairingHandler =
    Arc<dyn Fn(super::types::RemoteTransportPairingRequest) + Send + Sync + 'static>;
pub(crate) type RemoteTransportControlHandler =
    Arc<dyn Fn(String, super::types::RemoteEnvelope) -> bool + Send + Sync + 'static>;

#[async_trait]
pub(crate) trait RemoteTransport: Send + Sync {
    fn kind(&self) -> RemoteTransportKind;
    fn send(&self, data: Vec<u8>, device_id: Option<&str>) -> bool;
    async fn shutdown(&self);
}
