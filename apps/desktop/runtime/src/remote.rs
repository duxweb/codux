mod controller;
mod controller_manager;
mod controller_store;
mod crypto;
mod devices;
mod envelope;
mod host;
mod pairing;
mod protocol;
mod registration;
mod relay;
mod settings;
mod summary;
mod sync;
mod transport;
mod transport_factory;
mod types;

use std::path::PathBuf;

pub use controller::{
    parse_pairing_ticket, PairingTicket, RemoteController, RemoteControllerTarget,
    RemoteDirectoryEntry, RemoteDirectoryListing,
};
pub use controller_manager::{ControllerLinkState, RemoteControllerManager};
pub use controller_store::{RemoteControllerStore, SavedRemoteHost, SavedRemoteTransport};
pub use crypto::remote_host_name;
pub use host::RemoteHostRuntime;
pub use protocol::REMOTE_PROTOCOL_VERSION;
pub use relay::{
    GLOBAL_RELAY_SERVER_URL, normalize_remote_relay_preset, remote_relay_preset_for_url,
    remote_relay_presets, remote_relay_url_for_preset,
};
pub(crate) use settings::{remote_settings_from_raw, remote_settings_mut};
pub use types::{
    RemoteDeviceSummary, RemoteEnvelope, RemoteHostEvent, RemoteOutgoingEnvelope,
    RemotePairingInfo, RemotePairingPollResult, RemotePendingPairing, RemoteSummary,
    RemoteTerminalLayoutChanged,
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
