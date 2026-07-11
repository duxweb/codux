use super::terminal_dispatch::sanitized_remote_upload_name;
use super::*;

impl RemoteHostRuntime {
    pub fn send_transport(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) -> bool {
        self.send_transport_with_request_id(kind, device_id, session_id, None, payload)
    }

    pub(super) fn send_transport_with_request_id(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        request_id: Option<&str>,
        payload: Value,
    ) -> bool {
        let Some(data) =
            self.outgoing_transport_text(kind, device_id, session_id, request_id, payload)
        else {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "send drop kind={kind} device={} reason=encode",
                    device_id.unwrap_or("")
                ),
            );
            return false;
        };
        let transport = self.transport.lock().ok().and_then(|value| value.clone());
        let Some(transport) = transport else {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "send drop kind={kind} device={} reason=no_transport",
                    device_id.unwrap_or("")
                ),
            );
            return false;
        };
        let bytes = data.into_bytes();
        let ok = if codux_protocol::is_terminal_stream_message(kind) {
            transport.send_terminal(bytes, device_id)
        } else {
            transport.send(bytes, device_id)
        };
        if matches!(
            kind,
            REMOTE_PROJECT_SELECTED | REMOTE_PROJECT_LIST | REMOTE_TERMINAL_LIST | REMOTE_ERROR
        ) {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "send kind={kind} device={} session={} ok={ok}",
                    device_id.unwrap_or(""),
                    session_id.unwrap_or("")
                ),
            );
        }
        ok
    }

    pub(super) fn spawn_transport_start(self: &Arc<Self>, generation: u64) {
        let runtime = Arc::clone(self);
        crate::async_runtime::spawn(async move {
            if let Err(error) = runtime.ensure_transport_ready(generation).await {
                if generation != runtime.connection_generation.load(Ordering::SeqCst) {
                    return;
                }
                let mut status = runtime.service().summary();
                status.status = "failed".to_string();
                status.message = error;
                status.pairing = runtime.snapshot().pairing;
                runtime.update_snapshot(status);
            }
        });
    }

    pub(super) fn spawn_transport_restart(
        self: &Arc<Self>,
        transport: Option<Arc<dyn RemoteTransport>>,
        generation: u64,
    ) {
        let runtime = Arc::clone(self);
        crate::async_runtime::spawn(async move {
            if let Some(transport) = transport {
                transport.shutdown().await;
            }
            if let Err(error) = runtime.ensure_transport_ready(generation).await {
                if generation != runtime.connection_generation.load(Ordering::SeqCst) {
                    return;
                }
                let mut status = runtime.service().summary();
                status.status = "failed".to_string();
                status.message = error;
                status.pairing = runtime.snapshot().pairing;
                runtime.update_snapshot(status);
            }
        });
    }

    pub(super) fn prepare_transport_reconnect_after_disconnect(
        &self,
        state_generation: u64,
    ) -> Option<(Option<Arc<dyn RemoteTransport>>, u64)> {
        let restart_generation = state_generation.checked_add(1)?;
        if self
            .connection_generation
            .compare_exchange(
                state_generation,
                restart_generation,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_err()
        {
            return None;
        }

        let transport = self.take_transport();
        let mut status = self.service().summary();
        status.pairing = self.snapshot().pairing;
        if !status.enabled {
            status.status = "stopped".to_string();
            status.message = "Remote Host stopped.".to_string();
            self.update_snapshot(status);
            return None;
        }
        status.status = "connecting".to_string();
        status.message = "Remote transport disconnected. Reconnecting...".to_string();
        self.update_snapshot(status);
        Some((transport, restart_generation))
    }

    pub(super) fn prepare_transport_for_pairing(
        &self,
    ) -> Result<(Option<Arc<dyn RemoteTransport>>, u64), String> {
        let mut status = self.service().summary();
        if !status.enabled {
            return Err("Remote Host is disabled.".to_string());
        }
        let generation = self.connection_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let transport = self.take_transport();
        status.status = "connecting".to_string();
        status.message = "Connecting remote transport...".to_string();
        status.pairing = None;
        status.pending_pairing_list.clear();
        status.pending_pairings = 0;
        self.update_snapshot(status);
        Ok((transport, generation))
    }

    pub(super) fn handle_transport_state(
        self: &Arc<Self>,
        state_generation: u64,
        device_id: String,
        state: String,
    ) {
        if state_generation != self.connection_generation.load(Ordering::SeqCst) {
            return;
        }
        if !device_id.trim().is_empty() {
            if state == "connected" {
                self.update_device_online(Some(&device_id), true);
            } else if matches!(state.as_str(), "closed" | "failed" | "disconnected") {
                self.update_device_online(Some(&device_id), false);
                self.clear_remote_project_scope(Some(&device_id));
                self.remove_terminal_viewer(Some(&device_id));
            }
            return;
        }
        if matches!(state.as_str(), "closed" | "failed" | "disconnected") {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!("host_transport_disconnected state={state} generation={state_generation}"),
            );
            self.release_all_remote_viewports();
            if let Some((transport, restart_generation)) =
                self.prepare_transport_reconnect_after_disconnect(state_generation)
            {
                self.spawn_transport_restart(transport, restart_generation);
            }
        }
    }

    pub(super) fn take_transport(&self) -> Option<Arc<dyn RemoteTransport>> {
        self.transport
            .lock()
            .ok()
            .and_then(|mut value| value.take())
    }

    async fn ensure_transport_ready(self: &Arc<Self>, generation: u64) -> Result<(), String> {
        if self
            .transport
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .is_some()
        {
            return Ok(());
        }

        let _guard = self.transport_start_lock.lock().await;
        if self
            .transport
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .is_some()
        {
            return Ok(());
        }

        let mut summary = self.service().summary();
        if !summary.enabled {
            return Err("Remote Host is disabled.".to_string());
        }
        summary.status = "connecting".to_string();
        summary.message = "Connecting remote transport...".to_string();
        summary.pairing = self.snapshot().pairing;
        self.update_snapshot(summary);

        self.start_remote_transport(generation).await
    }

    pub(super) fn transport_candidates_snapshot(&self) -> Vec<RemoteTransportCandidate> {
        let settings = super::remote_settings_from_raw(&self.service().raw_settings());
        let relay = self
            .resolved_relay
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .unwrap_or_else(|| remote_relay_url(&settings.relay_url));
        let transport = self.transport.lock().ok().and_then(|value| value.clone());
        transport
            .as_ref()
            .and_then(|transport| {
                let ticket = transport.iroh_endpoint_ticket().unwrap_or_default();
                transport
                    .iroh_candidate()
                    .map(|(node_id, relay_url)| (node_id, relay_url, ticket))
            })
            .map(|(node_id, relay_url, ticket)| {
                vec![
                    codux_protocol::iroh_transport_candidate_with_ticket_and_authentication(
                        relay,
                        node_id,
                        relay_url,
                        ticket,
                        settings.relay_authentication.trim(),
                    ),
                ]
            })
            .unwrap_or_default()
    }

    pub(super) async fn transport_candidates(&self) -> Vec<RemoteTransportCandidate> {
        self.transport_candidates_snapshot()
    }

    pub(super) async fn start_remote_transport(
        self: &Arc<Self>,
        generation: u64,
    ) -> Result<(), String> {
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("transport_start kind=iroh generation={generation}"),
        );
        let mut raw = self.service().raw_settings();
        let settings = self.service().register_host_in_raw_async(&mut raw).await?;
        self.service().save_raw_settings(&raw)?;
        if let Ok(mut resolved) = self.resolved_relay.lock() {
            *resolved = Some(settings.relay_url.clone());
        }
        let _ = self.service().refresh_devices_async().await;
        if generation != self.connection_generation.load(Ordering::SeqCst) {
            return Ok(());
        }
        let weak_for_message = Arc::downgrade(self);
        let weak_for_upload = Arc::downgrade(self);
        let weak_for_state = Arc::downgrade(self);
        let weak_for_pairing = Arc::downgrade(self);
        let weak_for_authorize = Arc::downgrade(self);
        let weak_for_web_tunnel = Arc::downgrade(self);
        let state_generation = generation;
        let transport = RemoteTransportFactory::connect_host(
            &settings,
            Arc::new(move |device_id, data| {
                if let Some(runtime) = weak_for_message.upgrade() {
                    crate::async_runtime::spawn(async move {
                        runtime.handle_transport_message(device_id, data);
                    });
                }
            }),
            Arc::new(move |upload| {
                let Some(runtime) = weak_for_upload.upgrade() else {
                    return Err("remote runtime is not available".to_string());
                };
                runtime.handle_transport_upload(upload)
            }),
            Arc::new(move |device_id, state| {
                if let Some(runtime) = weak_for_state.upgrade() {
                    runtime.handle_transport_state(state_generation, device_id, state);
                }
            }),
            Arc::new(move |handshake| {
                weak_for_pairing
                    .upgrade()
                    .and_then(|runtime| runtime.handle_transport_pairing_request(handshake))
            }),
            Arc::new(move |device_id, device_token| {
                weak_for_authorize.upgrade().is_some_and(|runtime| {
                    runtime.is_authorized_device_token(Some(device_id), Some(device_token))
                })
            }),
            Some(Arc::new(move |request| {
                if let Some(runtime) = weak_for_web_tunnel.upgrade() {
                    runtime.authorize_web_tunnel_tcp_connect(request)
                } else {
                    Err("remote runtime is not available".to_string())
                }
            })),
        )
        .await?;
        if generation != self.connection_generation.load(Ordering::SeqCst) {
            transport.shutdown().await;
            return Ok(());
        }
        let transport_kind = transport.kind().as_str();
        if let Ok(mut current) = self.transport.lock() {
            *current = Some(transport);
        }
        let mut connected = self.service().summary();
        connected.status = "connected".to_string();
        connected.message = "Remote transport connected.".to_string();
        connected.pairing = self.snapshot().pairing;
        self.update_snapshot(connected);
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("transport_connected kind={transport_kind}"),
        );
        Ok(())
    }

    pub(super) fn handle_transport_message(self: Arc<Self>, device_id: String, data: Vec<u8>) {
        let Ok(mut raw) = serde_json::from_slice::<RemoteEnvelope>(&data) else {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "drop incoming reason=decode device={device_id} bytes={}",
                    data.len()
                ),
            );
            return;
        };
        if raw
            .device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
            && !device_id.trim().is_empty()
        {
            raw.device_id = Some(device_id.clone());
        }
        let envelope = {
            if let Some(seq) = raw.seq {
                let Ok(mut received) = self.receive_seq_by_device.lock() else {
                    crate::runtime_trace::runtime_trace(
                        "remote",
                        &format!(
                            "drop incoming reason=sequence_lock device={device_id} kind={}",
                            raw.kind
                        ),
                    );
                    return;
                };
                let guard = received
                    .entry(device_id.clone())
                    .or_insert_with(|| RemoteSequenceGuard::new(128));
                if !guard.accept(&raw.kind, raw.session_id.as_deref(), Some(seq)) {
                    crate::runtime_trace::runtime_trace(
                        "remote",
                        &format!(
                            "drop incoming reason=duplicate_seq device={device_id} kind={} seq={seq}",
                            raw.kind
                        ),
                    );
                    return;
                }
            }
            raw
        };
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!(
                "recv kind={} transport_device={} envelope_device={} session={}",
                envelope.kind,
                device_id,
                envelope.device_id.as_deref().unwrap_or(""),
                envelope.session_id.as_deref().unwrap_or("")
            ),
        );
        self.update_device_online(envelope.device_id.as_deref(), true);
        self.handle_remote_envelope(envelope);
    }

    pub(super) fn handle_transport_upload(
        &self,
        upload: RemoteTransportUpload,
    ) -> Result<(), String> {
        let device_id = upload.device_id.trim();
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!(
                "upload recv device={} session={} name={} bytes={}",
                device_id,
                upload.session_id,
                upload.name,
                upload.bytes.len()
            ),
        );
        if upload.session_id.trim().is_empty() {
            return Err("Terminal session is required.".to_string());
        }
        if upload.bytes.is_empty() || upload.bytes.len() > codux_protocol::REMOTE_BLOB_MAX_BYTES {
            return Err("Upload size is not supported.".to_string());
        }
        let name = sanitized_remote_upload_name(&upload.name);
        let kind = if upload.kind.trim().eq_ignore_ascii_case("image") {
            "image"
        } else {
            "file"
        };
        let path = self.write_terminal_upload_file(&upload.session_id, &name, &upload.bytes)?;
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!(
                "upload stored device={} session={} path={}",
                device_id,
                upload.session_id,
                path.to_string_lossy()
            ),
        );
        self.finish_terminal_upload(Some(device_id), &upload.session_id, path, kind);
        Ok(())
    }

    pub(super) fn authorize_web_tunnel_tcp_connect(
        &self,
        request: WebTunnelTcpConnectRequest,
    ) -> Result<(), String> {
        if self.is_authorized_device_token(Some(&request.device_id), Some(&request.device_token)) {
            Ok(())
        } else {
            Err("device is not authorized".to_string())
        }
    }
}
