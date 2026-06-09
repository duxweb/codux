use super::crypto::{ensure_remote_host_identity, remote_random_token};
use super::relay::remote_server_url;
use super::summary::remote_summary_from_settings;
use super::types::{RemoteSettings, RemoteSummary};
use super::{RemoteService, remote_settings_from_raw, remote_settings_mut};
use crate::runtime_trace::{runtime_trace, runtime_trace_elapsed};
use serde_json::{Map, Value};
use std::time::Instant;

impl RemoteService {
    pub fn set_enabled(&self, enabled: bool) -> Result<RemoteSummary, String> {
        let mut raw = self.raw_settings();
        let remote = remote_settings_mut(&mut raw)?;
        remote.insert("isEnabled".to_string(), Value::Bool(enabled));
        self.save_raw_settings(&raw)?;
        Ok(self.summary())
    }

    pub fn register_host(&self) -> Result<RemoteSummary, String> {
        crate::async_runtime::block_on(self.register_host_async())
    }

    pub async fn register_host_async(&self) -> Result<RemoteSummary, String> {
        let mut raw = self.raw_settings();
        let settings = self.register_host_in_raw_async(&mut raw).await?;
        self.save_raw_settings(&raw)?;
        Ok(remote_summary_from_settings(settings))
    }

    pub fn reconnect(&self) -> Result<RemoteSummary, String> {
        self.register_host()?;
        self.refresh_devices()
    }

    pub(super) async fn register_host_in_raw_async(
        &self,
        raw: &mut Map<String, Value>,
    ) -> Result<RemoteSettings, String> {
        let started_at = Instant::now();
        let mut settings = remote_settings_from_raw(raw);
        if !settings.is_enabled {
            runtime_trace("remote", "register_host skipped enabled=false");
            return Ok(settings);
        }
        if settings.host_id.trim().is_empty() {
            settings.host_id = uuid::Uuid::new_v4().to_string();
        }
        if settings.host_token.trim().is_empty() {
            settings.host_token = remote_random_token();
        }
        ensure_remote_host_identity(&mut settings);
        let configured_server_url = settings.server_url.clone();
        let resolved_relay = remote_server_url(&settings.server_url);
        runtime_trace(
            "remote",
            &format!(
                "register_host relay={} has_host={} has_token={}",
                resolved_relay,
                !settings.host_id.trim().is_empty(),
                !settings.host_token.trim().is_empty()
            ),
        );
        settings.server_url = resolved_relay;
        let mut saved_settings = settings.clone();
        saved_settings.server_url = configured_server_url;
        raw.insert(
            "remote".to_string(),
            serde_json::to_value(&saved_settings).map_err(|error| error.to_string())?,
        );
        runtime_trace_elapsed(
            "remote",
            "register_host ok",
            started_at,
            &format!("host_id={}", settings.host_id),
        );
        Ok(settings)
    }
}
