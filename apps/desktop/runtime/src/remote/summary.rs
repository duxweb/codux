use super::types::{RemoteDeviceSummary, RemoteSettings, RemoteSummary};
use super::{RemoteService, remote_settings_from_raw};

impl RemoteService {
    pub fn summary(&self) -> RemoteSummary {
        let settings = remote_settings_from_raw(&self.raw_settings());
        remote_summary_from_settings(settings)
    }
}

pub(crate) fn remote_summary_from_settings(mut settings: RemoteSettings) -> RemoteSummary {
    settings.server_url = if settings.server_url.trim().is_empty() {
        super::relay::remote_relay_url_for_preset("", "")
    } else {
        settings.server_url.trim().to_string()
    };
    settings.cached_devices.retain(|device| {
        !device.id.trim().is_empty()
            && device
                .revoked_at
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
    });
    if !settings.host_id.trim().is_empty() {
        let host_id = settings.host_id.trim().to_string();
        settings
            .cached_devices
            .retain(|device| device.host_id.trim().is_empty() || device.host_id.trim() == host_id);
    }

    let enabled = settings.is_enabled;
    let device_list = settings
        .cached_devices
        .into_iter()
        .map(RemoteDeviceSummary::from)
        .collect::<Vec<_>>();
    let online_devices = device_list
        .iter()
        .filter(|device| device.online.unwrap_or(false))
        .count();
    RemoteSummary {
        enabled,
        relay: settings.server_url,
        devices: device_list.len(),
        encryption: if enabled && !settings.host_public_key.trim().is_empty() {
            "configured".to_string()
        } else if enabled {
            "pending".to_string()
        } else {
            "disabled".to_string()
        },
        status: if enabled { "connecting" } else { "stopped" }.to_string(),
        message: if enabled {
            "Connecting relay...".to_string()
        } else {
            "Remote Host stopped.".to_string()
        },
        host_id: settings.host_id,
        pairing: None,
        device_list,
        online_devices,
        pending_pairings: 0,
        pending_pairing_list: Vec::new(),
        error: None,
    }
}
