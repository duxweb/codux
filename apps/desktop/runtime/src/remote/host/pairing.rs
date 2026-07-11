use super::*;

impl RemoteHostRuntime {
    pub(super) fn handle_transport_pairing_request(
        self: &Arc<Self>,
        handshake: RemoteTransportPairingRequest,
    ) -> Option<Value> {
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!(
                "pairing_request received device={} pair={} code_present={} secret_present={}",
                handshake.device_id,
                handshake.pairing_id.as_deref().unwrap_or(""),
                handshake
                    .pairing_code
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
                handshake
                    .pairing_secret
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
            ),
        );
        let Ok(mut active_pairing) = self.active_pairing.lock() else {
            return None;
        };
        let Some(active) = active_pairing.as_ref() else {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "pairing_request reject reason=no_active_pairing pair={}",
                    handshake.pairing_id.as_deref().unwrap_or("")
                ),
            );
            return None;
        };
        if let Err(reason) = codux_protocol::pairing_request_matches(
            &active.pairing_id,
            &active.code,
            &active.secret,
            remote_pairing_expired(active),
            &handshake,
        ) {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "pairing_request reject reason={reason} pair={}",
                    active.pairing_id
                ),
            );
            return None;
        }
        // The request carried this session's pairing_id + code + secret, which
        // proves the device scanned our QR — so confirm immediately rather than
        // prompting an operator. The headless agent runs the same shared
        // match-then-confirm path, so the two hosts pair identically.
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!(
                "pairing_request auto_confirm device={} pair={}",
                handshake.device_id, active.pairing_id
            ),
        );
        let device_token = match self.persist_paired_device(&handshake) {
            Ok(device_token) => device_token,
            Err(error) => {
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!("pairing_request reject reason=persist_failed error={error}"),
                );
                return None;
            }
        };
        *active_pairing = None;
        drop(active_pairing);
        self.clear_pending_pairings();
        let settings = super::remote_settings_from_raw(&self.service().raw_settings());
        let transports = self
            .transport_candidates_snapshot()
            .iter()
            .map(codux_protocol::confirmed_transport_entry)
            .collect::<Vec<_>>();
        let mut summary = self.service().summary();
        summary.status = "connected".to_string();
        summary.message = "Pairing confirmed.".to_string();
        self.update_snapshot(summary);
        Some(json!({
            "hostId": settings.host_id,
            "deviceId": handshake.device_id,
            "token": device_token,
            "hostName": remote_host_name(),
            "platform": std::env::consts::OS,
            "transports": transports,
        }))
    }

    pub fn create_pairing(self: &Arc<Self>) -> Result<RemoteSummary, String> {
        crate::async_runtime::block_on(self.create_pairing_async())
    }

    pub async fn create_pairing_async(self: &Arc<Self>) -> Result<RemoteSummary, String> {
        let started_at = Instant::now();
        crate::runtime_trace::runtime_trace("remote", "pairing_create start");
        if !self.snapshot().enabled {
            return Err("Remote Host is disabled.".to_string());
        }
        // Reuse the already-connected transport instead of tearing it down and
        // rebuilding it. A fresh endpoint gets new direct addresses and must
        // re-establish its home relay, so a mobile peer that scans the QR and
        // dials in that settling window hits a QUIC handshake timeout
        // (iroh_host_connect timed out) and its pairing.request never lands. The
        // NodeId + relay URL are stable across restarts, so a live endpoint
        // already advertises everything the QR needs — only spin up a transport
        // when the host has none (e.g. it was just enabled).
        let has_live_transport = self
            .transport
            .lock()
            .ok()
            .map(|guard| guard.is_some())
            .unwrap_or(false);
        if !has_live_transport {
            let (transport, generation) = self.prepare_transport_for_pairing()?;
            if let Some(transport) = transport {
                transport.shutdown().await;
            }
            self.start_remote_transport(generation).await?;
        }
        let raw = self.service().raw_settings();
        let settings = super::remote_settings_from_raw(&raw);
        let mut pairing = RemotePairingInfo {
            pairing_id: uuid::Uuid::new_v4().to_string(),
            code: remote_pairing_code(),
            secret: super::crypto::remote_random_token(),
            expires_at: (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339(),
            qr_payload: String::new(),
        };
        let transports = self.transport_candidates().await;
        let payload =
            super::crypto::remote_pairing_payload(&settings, &pairing, transports.clone());
        pairing.qr_payload = self.create_pairing_ticket_payload(payload)?;
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!(
                "pairing_qr relay={} transports={}",
                super::relay::remote_relay_url(&settings.relay_url),
                transports.len()
            ),
        );
        if let Ok(mut active) = self.active_pairing.lock() {
            *active = Some(pairing.clone());
        }
        if let Ok(mut pending) = self.pending_pairings.lock() {
            pending.clear();
        }
        let mut summary = self.service().summary();
        summary.status = "connected".to_string();
        summary.message = format!("Pairing code: {}", pairing.code);
        summary.pairing = Some(pairing.clone());
        self.update_snapshot(summary.clone());
        crate::runtime_trace::runtime_trace_elapsed(
            "remote",
            "pairing_create ok",
            started_at,
            &format!("pairing_id={}", pairing.pairing_id),
        );
        Ok(summary)
    }

    pub(super) fn create_pairing_ticket_payload(&self, payload: Value) -> Result<String, String> {
        remote_pairing_payload_url(&payload)
    }

    pub fn poll_pairing_status(
        &self,
        pairing: &RemotePairingInfo,
    ) -> Result<RemotePairingPollResult, String> {
        let mut active_pairing = self
            .active_pairing
            .lock()
            .map_err(|_| "Remote pairing state is unavailable.".to_string())?;
        let Some(active) = active_pairing
            .as_ref()
            .filter(|active| active.pairing_id == pairing.pairing_id)
            .cloned()
        else {
            let summary = self.snapshot();
            let finished = summary
                .pairing
                .as_ref()
                .is_none_or(|current| current.pairing_id != pairing.pairing_id);
            return Ok(RemotePairingPollResult { summary, finished });
        };
        if remote_pairing_expired(&active) {
            *active_pairing = None;
            drop(active_pairing);
            if let Ok(mut pending) = self.pending_pairings.lock() {
                pending.remove(&active.pairing_id);
            }
            let mut summary = self.service().summary();
            summary.status = "connected".to_string();
            summary.message = "Pairing expired.".to_string();
            self.update_snapshot(summary.clone());
            return Ok(RemotePairingPollResult {
                summary,
                finished: true,
            });
        }
        let mut summary = self.snapshot();
        summary.pairing = Some(active.clone());
        summary.status = "connected".to_string();
        summary.message = format!("Pairing code: {}", active.code);
        Ok(RemotePairingPollResult {
            summary,
            finished: false,
        })
    }

    pub fn cancel_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        let pairing_id = pairing_id.trim();
        if pairing_id.is_empty() {
            return Err("Missing pairing id.".to_string());
        }
        if let Ok(mut active) = self.active_pairing.lock()
            && active.as_ref().map(|pairing| pairing.pairing_id.as_str()) == Some(pairing_id)
        {
            *active = None;
        }
        if let Ok(mut pending) = self.pending_pairings.lock() {
            pending.remove(pairing_id);
        }
        let mut summary = self.service().summary();
        summary.status = "connected".to_string();
        summary.message = "Pairing cancelled.".to_string();
        self.update_snapshot(summary.clone());
        Ok(summary)
    }

    pub fn reject_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        let pairing_id = pairing_id.trim();
        if pairing_id.is_empty() {
            return Err("Missing pairing id.".to_string());
        }
        let handshake = self
            .pending_pairings
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(pairing_id));
        if let Some(handshake) = handshake.as_ref() {
            self.send_plain(
                REMOTE_PAIRING_REJECTED,
                Some(&handshake.device_id),
                None,
                json!({ "pairingId": pairing_id }),
            );
        }
        if let Ok(mut active) = self.active_pairing.lock()
            && active.as_ref().map(|pairing| pairing.pairing_id.as_str()) == Some(pairing_id)
        {
            *active = None;
        }
        let mut summary = self.service().summary();
        summary.status = "connected".to_string();
        summary.message = "Pairing rejected.".to_string();
        self.update_snapshot(summary.clone());
        Ok(summary)
    }

    /// Persist the paired device into `cached_devices`. The caller owns pairing
    /// credential consumption so validation, persistence, and one-time use stay
    /// serialized under the active-pairing lock.
    pub(super) fn persist_paired_device(
        &self,
        handshake: &RemoteTransportPairingRequest,
    ) -> Result<String, String> {
        let device_id = handshake.device_id.clone();
        let device_token = super::crypto::remote_random_token();
        let now = chrono::Utc::now().to_rfc3339();
        self.service().update_raw_settings_sync(|raw| {
            let mut settings = super::remote_settings_from_raw(raw);
            settings
                .cached_devices
                .retain(|device| device.id != device_id);
            settings.cached_devices.push(RemoteDeviceSettings {
                id: device_id.clone(),
                device_token: device_token.clone(),
                host_id: settings.host_id.clone(),
                name: handshake.device_name.clone(),
                public_key: String::new(),
                created_at: now.clone(),
                last_seen: now.clone(),
                revoked_at: None,
                online: Some(false),
                platform: handshake.platform.clone().unwrap_or_default(),
            });
            let value = serde_json::to_value(&settings).map_err(|error| error.to_string())?;
            raw.insert("remote".to_string(), value);
            Ok(())
        })?;
        self.authorization.refresh_after_persisted_write();
        Ok(device_token)
    }

    fn clear_pending_pairings(&self) {
        if let Ok(mut pending) = self.pending_pairings.lock() {
            pending.clear();
        }
    }

    /// Send the confirmed transports back to the controller. This snapshots the
    /// live iroh candidate and calls `send`, so it re-enters the transport and
    /// MUST run off the transport's receive/pairing callback (the auto-confirm
    /// path defers it for exactly this reason).
    pub(super) fn send_pairing_confirmed(
        &self,
        handshake: &RemoteTransportPairingRequest,
        device_token: String,
    ) -> RemoteSummary {
        let settings = super::remote_settings_from_raw(&self.service().raw_settings());
        let device_id = handshake.device_id.clone();
        let transports = self
            .transport_candidates_snapshot()
            .iter()
            .map(codux_protocol::confirmed_transport_entry)
            .collect::<Vec<_>>();
        self.send_plain(
            REMOTE_PAIRING_CONFIRMED,
            Some(&device_id),
            None,
            json!({
                "hostId": settings.host_id,
                "deviceId": device_id,
                "token": device_token,
                "hostName": remote_host_name(),
                "platform": std::env::consts::OS,
                "transports": transports,
            }),
        );
        let mut summary = self.service().summary();
        summary.status = "connected".to_string();
        summary.message = "Pairing confirmed.".to_string();
        summary
    }

    /// Record the paired device, then send the confirmed transports back. Shared
    /// by the auto-confirm path (a request whose code/secret matched the QR) and
    /// the legacy operator confirm — on a match the QR's secret already proves
    /// trust, so no dialog gates this. The auto-confirm path calls the two halves
    /// separately so it can authorize synchronously and defer only the send.
    pub(super) fn finalize_remote_pairing(
        &self,
        handshake: &RemoteTransportPairingRequest,
    ) -> Result<RemoteSummary, String> {
        let device_token = self.persist_paired_device(handshake)?;
        if let Ok(mut active) = self.active_pairing.lock() {
            *active = None;
        }
        self.clear_pending_pairings();
        Ok(self.send_pairing_confirmed(handshake, device_token))
    }

    pub fn confirm_pairing(&self, pairing_id: &str) -> Result<RemoteSummary, String> {
        let pairing_id = pairing_id.trim();
        if pairing_id.is_empty() {
            return Err("Missing pairing id.".to_string());
        }
        let handshake = self
            .pending_pairings
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(pairing_id))
            .ok_or_else(|| "Remote pairing request not found.".to_string())?;
        let summary = self.finalize_remote_pairing(&handshake)?;
        self.update_snapshot(summary.clone());
        Ok(summary)
    }
}

pub(super) fn remote_pairing_code() -> String {
    let value = uuid::Uuid::new_v4().as_u128() % 1_000_000;
    format!("{value:06}")
}

fn remote_pairing_expired(pairing: &RemotePairingInfo) -> bool {
    chrono::DateTime::parse_from_rfc3339(&pairing.expires_at)
        .map(|expires_at| expires_at <= chrono::Utc::now())
        .unwrap_or(true)
}
