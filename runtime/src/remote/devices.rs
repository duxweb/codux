use super::http::{
    remote_error_message, remote_http_client, remote_parse_response, remote_post_blocking,
    remote_server_url, remote_url,
};
use super::types::{RemoteDeviceSettings, RemoteSummary};
use super::{RemoteService, remote_settings_from_raw, remote_settings_mut};
use crate::runtime_trace::{runtime_trace, runtime_trace_elapsed};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Instant;

impl RemoteService {
    pub fn revoke_device(&self, device_id: &str) -> Result<RemoteSummary, String> {
        let device_id = device_id.trim();
        if device_id.is_empty() {
            return Err("Missing device id.".to_string());
        }
        let mut raw = self.raw_settings();
        let settings = remote_settings_from_raw(&raw);
        if settings.host_id.trim().is_empty() || settings.host_token.trim().is_empty() {
            return Err("Remote Host is not registered.".to_string());
        }
        remote_post_blocking::<Value>(
            &remote_server_url(&settings),
            "/api/devices/revoke",
            json!({
                "hostId": settings.host_id,
                "token": settings.host_token,
                "deviceId": device_id,
            }),
        )?;

        let remote = remote_settings_mut(&mut raw)?;
        let devices = remote
            .get_mut("cachedDevices")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "Remote cached devices are not configured.".to_string())?;
        let before_len = devices.len();
        devices.retain(|device| {
            device
                .get("id")
                .and_then(Value::as_str)
                .map(|id| id != device_id)
                .unwrap_or(true)
        });
        if devices.len() == before_len {
            return Err("Remote device not found.".to_string());
        }
        self.save_raw_settings(&raw)?;
        let mut summary = self.summary();
        summary.status = "connected".to_string();
        summary.message = "Device removed.".to_string();
        if let Ok(mut refreshed) = self.refresh_devices() {
            refreshed.status = summary.status;
            refreshed.message = summary.message;
            summary = refreshed;
        }
        Ok(summary)
    }

    pub fn refresh_devices(&self) -> Result<RemoteSummary, String> {
        crate::async_runtime::block_on(self.refresh_devices_async())
    }

    pub async fn refresh_devices_async(&self) -> Result<RemoteSummary, String> {
        let started_at = Instant::now();
        let mut raw = self.raw_settings();
        let mut settings = remote_settings_from_raw(&raw);
        if settings.host_id.trim().is_empty() {
            settings = self.register_host_in_raw_async(&mut raw).await?;
            self.save_raw_settings(&raw)?;
        }
        let relay = remote_server_url(&settings);
        if relay.trim().is_empty()
            || settings.host_id.trim().is_empty()
            || settings.host_token.trim().is_empty()
        {
            return Ok(super::remote_summary_from_settings(settings));
        }

        #[derive(Deserialize)]
        struct DeviceList {
            devices: Vec<RemoteDeviceSettings>,
        }

        let path = format!("/api/hosts/{}/devices", settings.host_id);
        runtime_trace(
            "remote",
            &format!("refresh_devices request relay={relay} host_id={}", settings.host_id),
        );
        let url = remote_url(
            &relay,
            &path,
            &[("token", settings.host_token.as_str())],
            false,
        )?;
        let client = remote_http_client()?;
        let response = match client.get(url).send().await {
            Ok(response) => response,
            Err(error) => {
                let error = remote_error_message(error);
                runtime_trace_elapsed(
                    "remote",
                    "refresh_devices failed",
                    started_at,
                    &format!("error={error}"),
                );
                return Err(error);
            }
        };
        let mut list = remote_parse_response::<DeviceList>(response).await?;
        list.devices.retain(|device| device.revoked_at.is_none());
        let device_count = list.devices.len();
        let devices = list
            .devices
            .into_iter()
            .map(|mut device| {
                device.online = Some(false);
                device
            })
            .collect::<Vec<_>>();
        let remote = remote_settings_mut(&mut raw)?;
        remote.insert(
            "cachedDevices".to_string(),
            serde_json::to_value(&devices).map_err(|error| error.to_string())?,
        );
        self.save_raw_settings(&raw)?;
        runtime_trace_elapsed(
            "remote",
            "refresh_devices ok",
            started_at,
            &format!("devices={device_count}"),
        );
        Ok(self.summary())
    }
}
