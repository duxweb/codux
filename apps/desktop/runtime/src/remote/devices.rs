use super::types::RemoteSummary;
use super::{RemoteService, remote_settings_from_raw};

impl RemoteService {
    pub fn revoke_device(&self, device_id: &str) -> Result<RemoteSummary, String> {
        let device_id = device_id.trim();
        if device_id.is_empty() {
            return Err("Missing device id.".to_string());
        }
        let mut raw = self.raw_settings();
        let mut settings = remote_settings_from_raw(&raw);
        let before_len = settings.cached_devices.len();
        settings
            .cached_devices
            .retain(|device| device.id != device_id);
        if settings.cached_devices.len() == before_len {
            return Err("Remote device not found.".to_string());
        }
        raw.insert(
            "remote".to_string(),
            serde_json::to_value(&settings).map_err(|error| error.to_string())?,
        );
        self.save_raw_settings(&raw)?;
        let mut summary = self.summary();
        summary.message = "Device removed.".to_string();
        Ok(summary)
    }

    pub fn refresh_devices(&self) -> Result<RemoteSummary, String> {
        self.refresh_devices_local()
    }

    pub async fn refresh_devices_async(&self) -> Result<RemoteSummary, String> {
        self.refresh_devices_local()
    }

    fn refresh_devices_local(&self) -> Result<RemoteSummary, String> {
        let mut raw = self.raw_settings();
        let mut settings = remote_settings_from_raw(&raw);
        if !settings.is_enabled {
            return Ok(super::summary::remote_summary_from_settings(settings));
        }
        for device in &mut settings.cached_devices {
            device.online = Some(false);
        }
        raw.insert(
            "remote".to_string(),
            serde_json::to_value(&settings).map_err(|error| error.to_string())?,
        );
        self.save_raw_settings(&raw)?;
        Ok(super::summary::remote_summary_from_settings(settings))
    }
}
