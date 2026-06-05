mod crypto;
mod devices;
mod envelope;
mod host;
mod iroh_transport;
mod pairing;
mod registration;
mod settings;
mod summary;
mod sync;
mod types;

use std::path::PathBuf;

pub use host::RemoteHostRuntime;
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
