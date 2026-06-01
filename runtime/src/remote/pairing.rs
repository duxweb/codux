use super::crypto::{remote_pairing_match_code, remote_pairing_qr_payload};
use super::http::{remote_post, remote_post_blocking, remote_server_url};
use super::summary::remote_summary_from_settings;
use super::types::{
    RemotePairingInfo, RemotePairingPollResult, RemotePairingStatusResponse,
    RemotePendingPairing, RemoteSettings, RemoteSummary,
};
use super::{RemoteService, remote_settings_from_raw};
use crate::runtime_trace::{runtime_trace, runtime_trace_elapsed};
use serde_json::{Value, json};
use std::time::Instant;

impl RemoteService {
    pub fn create_pairing(&self) -> Result<RemoteSummary, String> {
        crate::async_runtime::block_on(self.create_pairing_async())
    }

    pub async fn create_pairing_async(&self) -> Result<RemoteSummary, String> {
        let started_at = Instant::now();
        runtime_trace("remote", "create_pairing service_start");
        let mut raw = self.raw_settings();
        let settings = self.register_host_in_raw_async(&mut raw).await?;
        if settings.host_id.trim().is_empty() || settings.host_token.trim().is_empty() {
            runtime_trace("remote", "create_pairing failed reason=host_not_registered");
            return Err("Remote Host is not registered.".to_string());
        }
        let body = json!({
            "hostId": settings.host_id,
            "token": settings.host_token,
        });
        runtime_trace(
            "remote",
            &format!("create_pairing request relay={}", remote_server_url(&settings)),
        );
        let mut pairing =
            remote_post::<RemotePairingInfo>(&remote_server_url(&settings), "/api/pairings", body)
                .await?;
        pairing.host_public_key =
            (!settings.host_public_key.trim().is_empty()).then(|| settings.host_public_key.clone());
        pairing.crypto_version = Some(1);
        pairing.qr_payload = remote_pairing_qr_payload(&settings, &pairing);
        runtime_trace(
            "remote",
            &format!(
                "create_pairing payload_ready pairing_id={} code={} qr_bytes={}",
                pairing.pairing_id,
                pairing.code,
                pairing.qr_payload.len()
            ),
        );
        self.save_raw_settings(&raw)?;

        let mut summary = remote_summary_from_settings(settings);
        summary.pairing = Some(pairing.clone());
        summary.status = "connected".to_string();
        summary.message = format!("Pairing code: {}", pairing.code);
        runtime_trace_elapsed(
            "remote",
            "create_pairing ok",
            started_at,
            &format!("pairing_id={}", pairing.pairing_id),
        );
        Ok(summary)
    }

    pub fn cancel_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        self.reject_pairing(pairing_id)
    }

    pub fn poll_pairing_status(
        &self,
        pairing: &RemotePairingInfo,
    ) -> Result<RemotePairingPollResult, String> {
        let settings = remote_settings_from_raw(&self.raw_settings());
        let response = remote_post_blocking::<RemotePairingStatusResponse>(
            &remote_server_url(&settings),
            "/api/pairings/status",
            json!({
                "code": pairing.code,
                "secret": pairing.secret,
            }),
        );
        let Ok(response) = response else {
            let mut summary = remote_summary_from_settings(settings);
            summary.status = "connected".to_string();
            summary.message = "Pairing failed.".to_string();
            return Ok(RemotePairingPollResult {
                summary,
                finished: true,
            });
        };

        match response.status.as_str() {
            "claimed" => {
                let summary = remote_summary_show_pending_pairing(
                    settings,
                    pairing,
                    response
                        .pairing_id
                        .unwrap_or_else(|| pairing.pairing_id.clone()),
                    response
                        .device_name
                        .unwrap_or_else(|| "Mobile Device".to_string()),
                    response.device_public_key.unwrap_or_default(),
                    response.code.unwrap_or_else(|| pairing.code.clone()),
                    pairing.secret.clone(),
                );
                Ok(RemotePairingPollResult {
                    summary,
                    finished: true,
                })
            }
            "confirmed" | "rejected" => {
                let mut summary = remote_summary_from_settings(settings);
                summary.status = "connected".to_string();
                Ok(RemotePairingPollResult {
                    summary,
                    finished: true,
                })
            }
            _ => {
                let mut summary = remote_summary_from_settings(settings);
                summary.pairing = Some(pairing.clone());
                summary.status = "connected".to_string();
                summary.message = format!("Pairing code: {}", pairing.code);
                Ok(RemotePairingPollResult {
                    summary,
                    finished: false,
                })
            }
        }
    }

    pub fn confirm_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        self.pairing_decision("/api/pairings/confirm", pairing_id, "Pairing confirmed.")
    }

    pub fn reject_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        self.pairing_decision("/api/pairings/reject", pairing_id, "Pairing rejected.")
    }

    fn pairing_decision(
        &self,
        path: &str,
        pairing_id: &str,
        message: &str,
    ) -> Result<RemoteSummary, String> {
        let pairing_id = pairing_id.trim();
        if pairing_id.is_empty() {
            return Err("Missing pairing id.".to_string());
        }
        let settings = remote_settings_from_raw(&self.raw_settings());
        if settings.host_id.trim().is_empty() || settings.host_token.trim().is_empty() {
            return Err("Remote Host is not registered.".to_string());
        }
        remote_post_blocking::<Value>(
            &remote_server_url(&settings),
            path,
            json!({
                "hostId": settings.host_id,
                "token": settings.host_token,
                "pairingId": pairing_id,
            }),
        )?;
        let mut summary = if path.ends_with("/confirm") {
            self.refresh_devices()?
        } else {
            remote_summary_from_settings(settings)
        };
        summary.status = "connected".to_string();
        summary.message = message.to_string();
        Ok(summary)
    }
}

pub(crate) fn remote_summary_show_pending_pairing(
    settings: RemoteSettings,
    active_pairing: &RemotePairingInfo,
    pairing_id: String,
    device_name: String,
    device_public_key: String,
    pairing_code: String,
    pairing_secret: String,
) -> RemoteSummary {
    let mut summary = remote_summary_from_settings(settings.clone());
    if pairing_id.trim().is_empty() {
        summary.pairing = Some(active_pairing.clone());
        return summary;
    }

    let match_code = remote_pairing_match_code(
        &settings,
        &pairing_code,
        &pairing_secret,
        &device_public_key,
    )
    .unwrap_or(pairing_code);

    summary.status = "connected".to_string();
    summary.message = "Confirm device pairing.".to_string();
    summary.pending_pairing_list.push(RemotePendingPairing {
        id: pairing_id,
        device_name,
        device_public_key,
        code: match_code,
    });
    summary.pending_pairings = summary.pending_pairing_list.len();
    summary
}
