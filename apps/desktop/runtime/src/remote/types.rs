use serde::{Deserialize, Serialize};

pub use codux_protocol::{RemoteEnvelope, RemoteOutgoingEnvelope};
pub(crate) use codux_protocol::{RemoteTransportCandidate, RemoteTransportPairingRequest};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSummary {
    pub enabled: bool,
    pub relay: String,
    pub devices: usize,
    pub encryption: String,
    pub status: String,
    pub message: String,
    pub host_id: String,
    pub pairing: Option<RemotePairingInfo>,
    pub device_list: Vec<RemoteDeviceSummary>,
    pub online_devices: usize,
    pub pending_pairings: usize,
    pub pending_pairing_list: Vec<RemotePendingPairing>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RemoteHostEvent {
    Summary(Box<RemoteSummary>),
    TerminalLayoutChanged(RemoteTerminalLayoutChanged),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteTerminalLayoutChanged {
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairingPollResult {
    pub summary: RemoteSummary,
    pub finished: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDeviceSummary {
    pub id: String,
    pub host_id: String,
    pub name: String,
    pub public_key: String,
    pub created_at: String,
    pub last_seen: String,
    pub revoked_at: Option<String>,
    pub online: Option<bool>,
    /// The device's OS, if it reported one at pairing. Empty for devices paired
    /// before this was recorded (or clients that don't send it yet).
    #[serde(default)]
    pub platform: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairingInfo {
    pub pairing_id: String,
    pub code: String,
    pub secret: String,
    pub expires_at: String,
    pub qr_payload: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePendingPairing {
    pub id: String,
    pub device_name: String,
    pub device_id: String,
    pub code: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteSettings {
    #[serde(default, rename = "isEnabled")]
    pub(crate) is_enabled: bool,
    #[serde(default)]
    pub(crate) relay_preset: String,
    #[serde(default)]
    pub(crate) relay_url: String,
    #[serde(default, alias = "hostId", rename = "hostID")]
    pub(crate) host_id: String,
    #[serde(default)]
    pub(crate) relay_authentication: String,
    #[serde(default)]
    pub(crate) host_token: String,
    #[serde(default)]
    pub(crate) cached_devices: Vec<RemoteDeviceSettings>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteDeviceSettings {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default, rename = "token", alias = "deviceToken")]
    pub(crate) device_token: String,
    #[serde(default)]
    pub(crate) host_id: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) public_key: String,
    #[serde(default)]
    pub(crate) created_at: String,
    #[serde(default)]
    pub(crate) last_seen: String,
    #[serde(default)]
    pub(crate) revoked_at: Option<String>,
    #[serde(default)]
    pub(crate) online: Option<bool>,
    #[serde(default)]
    pub(crate) platform: String,
}

impl From<RemoteDeviceSettings> for RemoteDeviceSummary {
    fn from(device: RemoteDeviceSettings) -> Self {
        Self {
            id: device.id,
            host_id: device.host_id,
            name: device.name,
            public_key: device.public_key,
            created_at: device.created_at,
            last_seen: device.last_seen,
            revoked_at: device.revoked_at,
            online: device.online,
            platform: device.platform,
        }
    }
}
