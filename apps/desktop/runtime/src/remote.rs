mod crypto;
mod devices;
mod envelope;
mod host;
mod pairing;
mod protocol;
mod registration;
mod relay;
mod sequence;
mod settings;
mod summary;
mod sync;
mod terminal_subscriptions;
mod transport;
mod transport_factory;
mod types;
mod webrtc_transport;
mod websocket_transport;

use std::path::PathBuf;

pub use host::RemoteHostRuntime;
pub use protocol::REMOTE_PROTOCOL_VERSION;
pub use relay::{
    CHINA_RELAY_SERVER_URL, GLOBAL_RELAY_SERVER_URL, remote_relay_preset_for_url,
    remote_relay_url_for_preset,
};
pub(crate) use settings::{remote_settings_from_raw, remote_settings_mut};
pub use types::{
    RemoteDeviceSummary, RemoteEnvelope, RemoteOutgoingEnvelope, RemotePairingInfo,
    RemotePairingPollResult, RemotePendingPairing, RemoteSummary,
};

pub struct RemoteService {
    settings_path: PathBuf,
}

impl RemoteService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            settings_path: crate::config::settings_file_path(support_dir),
        }
    }
}

#[cfg(test)]
mod tests;
