use super::RemoteService;
use super::crypto::{remote_base64_url_decode, remote_host_name};
use super::pairing::remote_summary_show_pending_pairing;
use super::protocol::{
    REMOTE_PROTOCOL_VERSION, REMOTE_TERMINAL_BUFFER_MAX_CHARS, RemoteTerminalBufferWindow,
    host_capabilities, terminal_buffer_payloads,
};
use super::relay::{remote_pairing_ticket_payload, remote_server_url, remote_stun_urls};
use super::sequence::RemoteSequenceGuard;
use super::terminal_subscriptions::RemoteTerminalSubscriptions;
use super::transport::RemoteTransport;
use super::transport_factory::RemoteTransportFactory;
use super::types::{
    RemoteDeviceSettings, RemoteEnvelope, RemoteIceServer, RemotePairingInfo,
    RemotePairingPollResult, RemoteSummary, RemoteTransportCandidate,
    RemoteTransportPairingRequest,
};
use crate::ai_history_indexer::{AIHistoryIndexer, AIHistoryProjectState};
use crate::ai_history_normalized::AIHistoryProjectRequest;
use crate::project_store::{ProjectCreateRequest, ProjectStore, ProjectUpdateRequest};
use crate::terminal_layout::{
    TerminalLayoutService, TerminalPaneSummary, TerminalTabSummary, terminal_layout_storage_key,
};
use crate::terminal_pty::{
    TerminalEvent, TerminalManager, TerminalPtyConfig, TerminalSessionSnapshot,
    TerminalViewportState, terminal_viewport_remote_owner,
};
use crate::worktree::{
    WorktreeCreateRequest, WorktreeMergeRequest, WorktreeRemoveRequest, WorktreeService,
};
use base64::Engine;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::{Seek, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const REMOTE_TERMINAL_OUTPUT_BATCH_MS: u64 = 32;

struct RemoteProjectScope {
    project_id: String,
    project_name: String,
    project_path: String,
    worktree_id: String,
    layout_key: String,
}

struct RemoteTerminalPlan {
    config: TerminalPtyConfig,
    scope: RemoteProjectScope,
    title: String,
    layout_kind: String,
}

struct RemoteTerminalOutputBatch {
    data: String,
    buffer_length: usize,
    viewers: HashSet<String>,
}

pub struct RemoteHostRuntime {
    runtime_instance_id: String,
    support_dir: PathBuf,
    ai_history: AIHistoryIndexer,
    terminals: Arc<TerminalManager>,
    terminal_subscriptions: RemoteTerminalSubscriptions,
    terminal_output_seq_by_session: Mutex<HashMap<String, i64>>,
    terminal_output_batches: Mutex<HashMap<String, RemoteTerminalOutputBatch>>,
    remote_project_scope_by_device: Mutex<HashMap<String, String>>,
    terminal_event_subscriptions: Mutex<HashSet<String>>,
    terminal_upload_sessions: Mutex<HashMap<String, RemoteTerminalUploadSession>>,
    transport: Mutex<Option<Arc<dyn RemoteTransport>>>,
    transport_start_lock: tokio::sync::Mutex<()>,
    active_pairing: Mutex<Option<RemotePairingInfo>>,
    pending_pairings: Mutex<HashMap<String, RemoteTransportPairingRequest>>,
    events: Mutex<VecDeque<RemoteSummary>>,
    snapshot: Mutex<RemoteSummary>,
    connection_generation: AtomicU64,
    resolved_relay: Mutex<Option<String>>,
    send_seq_by_device: Mutex<HashMap<String, i64>>,
    receive_seq_by_device: Mutex<HashMap<String, RemoteSequenceGuard>>,
}

impl RemoteHostRuntime {
    pub fn new(support_dir: PathBuf) -> Self {
        let ai_history = AIHistoryIndexer::with_database_path(support_dir.join("ai-usage.sqlite3"));
        Self::new_with_ai_history(support_dir, ai_history)
    }

    pub fn new_with_ai_history(support_dir: PathBuf, ai_history: AIHistoryIndexer) -> Self {
        Self::new_with_ai_history_and_terminals(
            support_dir,
            ai_history,
            Arc::new(TerminalManager::new()),
        )
    }

    pub fn new_with_ai_history_and_terminals(
        support_dir: PathBuf,
        ai_history: AIHistoryIndexer,
        terminals: Arc<TerminalManager>,
    ) -> Self {
        let snapshot = RemoteService::new(support_dir.clone()).summary();
        Self {
            runtime_instance_id: uuid::Uuid::new_v4().to_string(),
            support_dir,
            ai_history,
            terminals,
            terminal_subscriptions: RemoteTerminalSubscriptions::default(),
            terminal_output_seq_by_session: Mutex::new(HashMap::new()),
            terminal_output_batches: Mutex::new(HashMap::new()),
            remote_project_scope_by_device: Mutex::new(HashMap::new()),
            terminal_event_subscriptions: Mutex::new(HashSet::new()),
            terminal_upload_sessions: Mutex::new(HashMap::new()),
            transport: Mutex::new(None),
            transport_start_lock: tokio::sync::Mutex::new(()),
            active_pairing: Mutex::new(None),
            pending_pairings: Mutex::new(HashMap::new()),
            events: Mutex::new(VecDeque::new()),
            snapshot: Mutex::new(snapshot),
            connection_generation: AtomicU64::new(0),
            resolved_relay: Mutex::new(None),
            send_seq_by_device: Mutex::new(HashMap::new()),
            receive_seq_by_device: Mutex::new(HashMap::new()),
        }
    }

    pub fn snapshot(&self) -> RemoteSummary {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_else(|_| self.service().summary())
    }

    pub fn reload_snapshot_from_settings(&self) -> RemoteSummary {
        let summary = self.summary_from_settings_preserving_connection();
        self.update_snapshot(summary.clone());
        summary
    }

    pub fn clear_pairing_state(&self) {
        if let Ok(mut active) = self.active_pairing.lock() {
            *active = None;
        }
        if let Ok(mut pending) = self.pending_pairings.lock() {
            pending.clear();
        }
        if let Ok(mut scopes) = self.remote_project_scope_by_device.lock() {
            scopes.clear();
        }
        if let Ok(mut send_seq) = self.send_seq_by_device.lock() {
            send_seq.clear();
        }
        if let Ok(mut receive_seq) = self.receive_seq_by_device.lock() {
            receive_seq.clear();
        }
    }

    pub fn drain_events(&self) -> Vec<RemoteSummary> {
        self.events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn terminal_manager(&self) -> Arc<TerminalManager> {
        Arc::clone(&self.terminals)
    }

    pub fn apply_snapshot(&self, summary: RemoteSummary) -> RemoteSummary {
        self.update_snapshot(summary.clone());
        summary
    }

    pub fn start(self: &Arc<Self>) -> RemoteSummary {
        let summary = self.service().summary();
        if !summary.enabled {
            self.stop_with_message("Remote Host stopped.");
            return self.snapshot();
        }
        let current = self.snapshot();
        let has_transport = self
            .transport
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .is_some();
        let is_starting = current.enabled
            && current.status == "connecting"
            && self.connection_generation.load(Ordering::SeqCst) > 0;
        let is_connected = current.enabled && current.status == "connected" && has_transport;
        if is_starting || is_connected {
            return current;
        }
        let generation = self.connection_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.update_snapshot(summary);
        self.spawn_transport_start(generation);
        self.snapshot()
    }

    pub fn stop_with_message(&self, message: &str) {
        self.connection_generation.fetch_add(1, Ordering::SeqCst);
        let transport = self.take_transport();
        if let Some(transport) = transport {
            crate::async_runtime::spawn(async move {
                transport.shutdown().await;
            });
        }
        let mut summary = self.service().summary();
        summary.status = "stopped".to_string();
        summary.message = message.to_string();
        self.update_snapshot(summary);
    }

    pub fn shutdown(&self) {
        self.stop_with_message("Remote Host stopped.");
        self.terminal_subscriptions.clear();
        if let Ok(mut uploads) = self.terminal_upload_sessions.lock() {
            uploads.clear();
        }
        for terminal in self.terminals.list() {
            let _ = self.terminals.kill(&terminal.id);
        }
    }

    pub fn reconnect(self: &Arc<Self>) -> RemoteSummary {
        crate::runtime_trace::runtime_trace("remote", "host_reconnect requested");
        let generation = self.connection_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let transport = self.take_transport();
        let mut summary = self.service().summary();
        summary.status = "connecting".to_string();
        summary.message = "Connecting relay...".to_string();
        summary.pairing = self.snapshot().pairing;
        self.update_snapshot(summary);
        self.spawn_transport_restart(transport, generation);
        self.snapshot()
    }

    pub fn send_transport(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) -> bool {
        let Some(data) = self.outgoing_transport_text(kind, device_id, session_id, payload) else {
            return false;
        };
        let transport = self.transport.lock().ok().and_then(|value| value.clone());
        let Some(transport) = transport else {
            return false;
        };
        transport.send(data.into_bytes(), device_id)
    }

    fn spawn_transport_start(self: &Arc<Self>, generation: u64) {
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

    fn spawn_transport_restart(
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

    fn take_transport(&self) -> Option<Arc<dyn RemoteTransport>> {
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
        summary.message = "Connecting relay...".to_string();
        summary.pairing = self.snapshot().pairing;
        self.update_snapshot(summary);

        self.start_remote_transport(generation).await
    }

    fn transport_candidates_snapshot(&self) -> Vec<RemoteTransportCandidate> {
        let settings = super::remote_settings_from_raw(&self.service().raw_settings());
        let relay = self
            .resolved_relay
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .unwrap_or_else(|| remote_server_url(&settings.server_url));
        vec![
            RemoteTransportCandidate {
                kind: "websocketRelay".to_string(),
                role: Some("host".to_string()),
                url: Some(relay.clone()),
                ice_servers: Vec::new(),
            },
            RemoteTransportCandidate {
                kind: "webRtc".to_string(),
                role: Some("host".to_string()),
                url: Some(relay),
                ice_servers: vec![RemoteIceServer {
                    urls: remote_stun_urls(),
                }],
            },
        ]
    }

    async fn transport_candidates(&self) -> Vec<RemoteTransportCandidate> {
        self.transport_candidates_snapshot()
    }

    async fn start_remote_transport(self: &Arc<Self>, generation: u64) -> Result<(), String> {
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("transport_start kind=webRtc generation={generation}"),
        );
        let mut raw = self.service().raw_settings();
        let settings = self.service().register_host_in_raw_async(&mut raw).await?;
        self.service().save_raw_settings(&raw)?;
        if let Ok(mut resolved) = self.resolved_relay.lock() {
            *resolved = Some(settings.server_url.clone());
        }
        let _ = self.service().refresh_devices_async().await;
        if generation != self.connection_generation.load(Ordering::SeqCst) {
            return Ok(());
        }
        let weak_for_message = Arc::downgrade(self);
        let weak_for_state = Arc::downgrade(self);
        let weak_for_pairing = Arc::downgrade(self);
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
            Arc::new(move |device_id, state| {
                if let Some(runtime) = weak_for_state.upgrade() {
                    if state_generation != runtime.connection_generation.load(Ordering::SeqCst) {
                        return;
                    }
                    if !device_id.trim().is_empty() {
                        if state == "connected" {
                            runtime.update_device_online(Some(&device_id), true);
                        } else if matches!(state.as_str(), "closed" | "failed" | "disconnected") {
                            runtime.update_device_online(Some(&device_id), false);
                            runtime.clear_remote_project_scope(Some(&device_id));
                            runtime.remove_terminal_viewer(Some(&device_id));
                        }
                    } else if matches!(state.as_str(), "closed" | "failed" | "disconnected") {
                        let mut status = runtime.service().summary();
                        status.status = "failed".to_string();
                        status.message = "Relay disconnected.".to_string();
                        status.pairing = runtime.snapshot().pairing;
                        runtime.update_snapshot(status);
                    }
                }
            }),
            Arc::new(move |handshake| {
                if let Some(runtime) = weak_for_pairing.upgrade() {
                    runtime.handle_transport_pairing_request(handshake);
                }
            }),
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
        connected.message = "Relay connected.".to_string();
        connected.pairing = self.snapshot().pairing;
        self.update_snapshot(connected);
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("transport_connected kind={transport_kind}"),
        );
        Ok(())
    }

    fn handle_transport_message(self: Arc<Self>, device_id: String, data: Vec<u8>) {
        let Ok(raw) = serde_json::from_slice::<RemoteEnvelope>(&data) else {
            return;
        };
        let envelope = {
            let Ok(mut received) = self.receive_seq_by_device.lock() else {
                return;
            };
            self.service()
                .decrypt_envelope_if_needed(raw, &mut received)
                .ok()
                .flatten()
        };
        let Some(envelope) = envelope else {
            return;
        };
        if !self.is_authorized_device(envelope.device_id.as_deref()) {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "drop unauthorized device={}",
                    envelope.device_id.as_deref().unwrap_or("")
                ),
            );
            return;
        }
        self.update_device_online(envelope.device_id.as_deref(), true);
        self.handle_remote_envelope(envelope.with_device_id(device_id));
    }

    fn handle_remote_envelope(self: &Arc<Self>, envelope: RemoteEnvelope) {
        match envelope.kind.as_str() {
            "host.info" => self.send_host_info(envelope.device_id.as_deref()),
            "device.connected" => {
                self.update_device_online(envelope.device_id.as_deref(), true);
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            "device.disconnected" => {
                self.update_device_online(envelope.device_id.as_deref(), false);
                self.clear_remote_project_scope(envelope.device_id.as_deref());
                self.remove_terminal_viewer(envelope.device_id.as_deref());
            }
            "project.list" => self.send_project_list(envelope.device_id.as_deref()),
            "project.select" => self.handle_project_select(&envelope),
            "worktree.list" => self.handle_worktree_list(&envelope),
            "worktree.select" => self.handle_worktree_select(&envelope),
            "worktree.create" => self.handle_worktree_create(&envelope),
            "worktree.merge" => self.handle_worktree_merge(&envelope),
            "worktree.delete" | "worktree.remove" => self.handle_worktree_remove(&envelope),
            "terminal.list" => self.send_terminal_list(envelope.device_id.as_deref()),
            "terminal.subscribe" => self.handle_terminal_subscribe(&envelope),
            "terminal.unsubscribe" => self.handle_terminal_unsubscribe(&envelope),
            "terminal.create" => self.handle_terminal_create(&envelope),
            "terminal.buffer" => self.handle_terminal_buffer(&envelope),
            "terminal.output.ack" => {}
            "terminal.input" => self.handle_terminal_input(&envelope),
            "terminal.viewport.claim" => self.handle_terminal_viewport_claim(&envelope),
            "terminal.viewport.resize" => self.handle_terminal_viewport_resize(&envelope),
            "terminal.viewport.release" => self.handle_terminal_viewport_release(&envelope),
            "terminal.resize" => self.handle_terminal_resize(&envelope),
            "terminal.close" => self.handle_terminal_close(&envelope),
            "terminal.signal" => self.handle_terminal_signal(&envelope),
            "terminal.upload" => self.handle_terminal_upload(&envelope),
            "terminal.upload.start" => self.handle_terminal_upload_start(&envelope),
            "terminal.upload.chunk" => self.handle_terminal_upload_chunk(&envelope),
            "terminal.upload.finish" => self.handle_terminal_upload_finish(&envelope),
            "terminal.upload.cancel" => self.handle_terminal_upload_cancel(&envelope),
            "file.list" => {
                let path = envelope.payload.get("path").and_then(Value::as_str);
                let purpose = envelope.payload.get("purpose").and_then(Value::as_str);
                self.send(
                    "file.list",
                    envelope.device_id.as_deref(),
                    None,
                    remote_file_list(path, purpose),
                );
            }
            "file.read" => self.handle_file_read(&envelope),
            "file.write" => self.handle_file_write(&envelope),
            "file.rename" => self.handle_file_rename(&envelope),
            "file.delete" => self.handle_file_delete(&envelope),
            "git.status" => self.handle_git_status(&envelope),
            "project.add" => self.handle_project_add(&envelope),
            "project.edit" => self.handle_project_edit(&envelope),
            "project.remove" => self.handle_project_remove(&envelope),
            "ai.stats" => self.handle_ai_stats(&envelope),
            "transport.ping" => self.send_terminal_data(
                "transport.pong",
                envelope.device_id.as_deref(),
                None,
                envelope.payload,
            ),
            _ => {}
        }
    }

    fn send_host_info(self: &Arc<Self>, device_id: Option<&str>) {
        let transports = self.transport_candidates_snapshot();
        self.send(
            "host.info",
            device_id,
            None,
            json!({
                "hostId": self.snapshot().host_id,
                "runtimeInstanceId": self.runtime_instance_id,
                "name": remote_host_name(),
                "platform": std::env::consts::OS,
                "app": "Codux",
                "protocolVersion": REMOTE_PROTOCOL_VERSION,
                "capabilities": host_capabilities(),
                "transports": transports,
            }),
        );
    }

    fn handle_transport_pairing_request(&self, handshake: RemoteTransportPairingRequest) {
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!(
                "pairing_request received device={} pair={} code_present={} secret_present={} public_key_present={}",
                handshake.device_id,
                handshake.pairing_id.as_deref().unwrap_or(""),
                handshake
                    .pairing_code
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
                handshake
                    .pairing_secret
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
                !handshake.device_public_key.trim().is_empty()
            ),
        );
        let active_pairing = self
            .active_pairing
            .lock()
            .ok()
            .and_then(|value| value.clone());
        let Some(active_pairing) = active_pairing else {
            crate::runtime_trace::runtime_trace(
                "remote",
                "pairing_request reject reason=no_active_pairing",
            );
            return;
        };
        if handshake.pairing_id.as_deref() != Some(active_pairing.pairing_id.as_str()) {
            crate::runtime_trace::runtime_trace(
                "remote",
                "pairing_request reject reason=pairing_id_mismatch",
            );
            return;
        }
        if handshake.pairing_code.as_deref() != Some(active_pairing.code.as_str()) {
            crate::runtime_trace::runtime_trace(
                "remote",
                "pairing_request reject reason=code_mismatch",
            );
            return;
        }
        if handshake.pairing_secret.as_deref() != Some(active_pairing.secret.as_str()) {
            crate::runtime_trace::runtime_trace(
                "remote",
                "pairing_request reject reason=secret_mismatch",
            );
            return;
        }
        if handshake.device_public_key.trim().is_empty() {
            crate::runtime_trace::runtime_trace(
                "remote",
                "pairing_request reject reason=missing_device_public_key",
            );
            return;
        }
        if let Ok(mut pending) = self.pending_pairings.lock() {
            pending.insert(active_pairing.pairing_id.clone(), handshake.clone());
        }
        let settings = super::remote_settings_from_raw(&self.service().raw_settings());
        let summary = remote_summary_show_pending_pairing(
            settings,
            &active_pairing,
            active_pairing.pairing_id.clone(),
            handshake.device_name,
            handshake.device_public_key,
            active_pairing.code.clone(),
            active_pairing.secret.clone(),
        );
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!(
                "pairing_request pending device={} pair={}",
                handshake.device_id, active_pairing.pairing_id
            ),
        );
        self.update_snapshot(summary);
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
        let generation = match self.connection_generation.load(Ordering::SeqCst) {
            0 => self.connection_generation.fetch_add(1, Ordering::SeqCst) + 1,
            generation => generation,
        };
        self.ensure_transport_ready(generation).await?;
        let raw = self.service().raw_settings();
        let settings = super::remote_settings_from_raw(&raw);
        if settings.host_public_key.trim().is_empty() {
            return Err("Remote Host encryption identity is not ready.".to_string());
        }
        let mut pairing = RemotePairingInfo {
            pairing_id: uuid::Uuid::new_v4().to_string(),
            code: remote_pairing_code(),
            secret: super::crypto::remote_random_token(),
            host_public_key: (!settings.host_public_key.trim().is_empty())
                .then(|| settings.host_public_key.clone()),
            crypto_version: Some(1),
            expires_at: (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339(),
            qr_payload: String::new(),
        };
        let transports = self.transport_candidates().await;
        let payload =
            super::crypto::remote_pairing_payload(&settings, &pairing, transports.clone());
        pairing.qr_payload = self
            .create_pairing_ticket_payload(&settings.server_url, payload)
            .await?;
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("pairing_qr transports={}", transports.len()),
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

    async fn create_pairing_ticket_payload(
        &self,
        relay: &str,
        payload: Value,
    ) -> Result<String, String> {
        let relay = remote_server_url(relay);
        let url = super::relay::remote_url(&relay, "/api/tickets", &[], false)?;
        let response = reqwest::Client::new()
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "ticket request failed status={}",
                response.status()
            ));
        }
        let value = response
            .json::<Value>()
            .await
            .map_err(|error| error.to_string())?;
        let ticket = value
            .get("ticket")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "ticket response missing ticket".to_string())?;
        remote_pairing_ticket_payload(&relay, ticket)
    }

    pub fn poll_pairing_status(
        &self,
        pairing: &RemotePairingInfo,
    ) -> Result<RemotePairingPollResult, String> {
        let pending = self
            .pending_pairings
            .lock()
            .ok()
            .and_then(|pending| pending.get(&pairing.pairing_id).cloned());
        if let Some(handshake) = pending {
            let settings = super::remote_settings_from_raw(&self.service().raw_settings());
            let summary = remote_summary_show_pending_pairing(
                settings,
                pairing,
                pairing.pairing_id.clone(),
                handshake.device_name,
                handshake.device_public_key,
                pairing.code.clone(),
                pairing.secret.clone(),
            );
            self.update_snapshot(summary.clone());
            return Ok(RemotePairingPollResult {
                summary,
                finished: true,
            });
        }
        let mut summary = self.snapshot();
        summary.pairing = Some(pairing.clone());
        summary.status = "connected".to_string();
        summary.message = format!("Pairing code: {}", pairing.code);
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
        if let Ok(mut active) = self.active_pairing.lock() {
            if active.as_ref().map(|pairing| pairing.pairing_id.as_str()) == Some(pairing_id) {
                *active = None;
            }
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
                "pairing.rejected",
                Some(&handshake.device_id),
                None,
                json!({ "pairingId": pairing_id }),
            );
        }
        if let Ok(mut active) = self.active_pairing.lock() {
            if active.as_ref().map(|pairing| pairing.pairing_id.as_str()) == Some(pairing_id) {
                *active = None;
            }
        }
        let mut summary = self.service().summary();
        summary.status = "connected".to_string();
        summary.message = "Pairing rejected.".to_string();
        self.update_snapshot(summary.clone());
        Ok(summary)
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
        let mut raw = self.service().raw_settings();
        let mut settings = super::remote_settings_from_raw(&raw);
        let device_id = handshake.device_id.clone();
        let now = chrono::Utc::now().to_rfc3339();
        settings
            .cached_devices
            .retain(|device| device.id != device_id);
        settings.cached_devices.push(RemoteDeviceSettings {
            id: device_id.clone(),
            host_id: settings.host_id.clone(),
            name: handshake.device_name.clone(),
            public_key: handshake.device_public_key.clone(),
            created_at: now.clone(),
            last_seen: now,
            revoked_at: None,
            online: Some(false),
        });
        raw.insert(
            "remote".to_string(),
            serde_json::to_value(&settings).map_err(|error| error.to_string())?,
        );
        self.service().save_raw_settings(&raw)?;
        if let Ok(mut active) = self.active_pairing.lock() {
            *active = None;
        }
        self.send_plain(
            "pairing.confirmed",
            Some(&device_id),
            None,
            json!({
                "hostId": settings.host_id,
                "deviceId": device_id,
                "token": "",
                "hostName": remote_host_name(),
            }),
        );
        let mut summary = self.service().summary();
        summary.status = "connected".to_string();
        summary.message = "Pairing confirmed.".to_string();
        self.update_snapshot(summary.clone());
        Ok(summary)
    }

    fn handle_file_read(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        match remote_file_read(path) {
            Ok(payload) => self.send("file.read", envelope.device_id.as_deref(), None, payload),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_file_write(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        let Some(content) = envelope.payload.get("content").and_then(Value::as_str) else {
            self.send_error(envelope, "File content is required.");
            return;
        };
        match remote_file_write(path, content) {
            Ok(()) => self.send(
                "file.written",
                envelope.device_id.as_deref(),
                None,
                json!({ "path": path }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_file_rename(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        let Some(new_path) = envelope.payload.get("newPath").and_then(Value::as_str) else {
            self.send_error(envelope, "New file path is required.");
            return;
        };
        match remote_file_rename(path, new_path) {
            Ok(()) => self.send(
                "file.renamed",
                envelope.device_id.as_deref(),
                None,
                json!({ "path": path, "newPath": new_path }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_file_delete(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        match fs::remove_file(path).or_else(|_| fs::remove_dir_all(path)) {
            Ok(()) => self.send(
                "file.deleted",
                envelope.device_id.as_deref(),
                None,
                json!({ "path": path }),
            ),
            Err(error) => self.send_error(envelope, &error.to_string()),
        }
    }

    fn handle_project_add(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "Project path is required.");
            return;
        };
        let name = envelope
            .payload
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| default_project_name(path));
        match ProjectStore::new(self.support_dir.clone()).create_project(ProjectCreateRequest {
            name,
            path: path.to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
        }) {
            Ok(snapshot) => {
                let project_id = snapshot.selected_project_id.unwrap_or_default();
                self.send(
                    "project.updated",
                    envelope.device_id.as_deref(),
                    None,
                    json!({ "action": "add", "projectId": project_id }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_project_edit(&self, envelope: &RemoteEnvelope) {
        let Some(project_id) = envelope.payload.get("projectId").and_then(Value::as_str) else {
            self.send_error(envelope, "Project id is required.");
            return;
        };
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "Project path is required.");
            return;
        };
        let name = envelope
            .payload
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| default_project_name(path));
        match ProjectStore::new(self.support_dir.clone()).update_project_from_request(
            ProjectUpdateRequest {
                project_id: project_id.to_string(),
                name,
                path: path.to_string(),
                badge_text: None,
                badge_symbol: None,
                badge_color_hex: None,
            },
        ) {
            Ok(_) => {
                self.send(
                    "project.updated",
                    envelope.device_id.as_deref(),
                    None,
                    json!({ "action": "edit", "projectId": project_id }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_project_remove(&self, envelope: &RemoteEnvelope) {
        let Some(project_id) = envelope.payload.get("projectId").and_then(Value::as_str) else {
            self.send_error(envelope, "Project id is required.");
            return;
        };
        match ProjectStore::new(self.support_dir.clone()).close_project(project_id) {
            Ok(_) => {
                self.clear_remote_project_scope_for_project(project_id);
                self.send(
                    "project.updated",
                    envelope.device_id.as_deref(),
                    None,
                    json!({ "action": "remove", "projectId": project_id }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_project_select(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(project_id) = envelope.payload.get("projectId").and_then(Value::as_str) else {
            self.send_error(envelope, "Project id is required.");
            return;
        };
        match self.remote_project_scope(project_id) {
            Ok(scope) => {
                self.set_remote_project_scope(envelope.device_id.as_deref(), &scope.project_id);
                if let Err(error) =
                    self.ensure_remote_project_terminal(&scope, envelope.device_id.as_deref())
                {
                    self.send_error(envelope, &error);
                    return;
                }
                self.register_project_terminal_viewers(
                    &scope.project_id,
                    envelope.device_id.as_deref(),
                );
                self.send(
                    "project.selected",
                    envelope.device_id.as_deref(),
                    None,
                    json!({ "projectId": scope.project_id, "worktreeId": scope.worktree_id }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_terminal_subscribe(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(device_id) = envelope.device_id.as_deref() else {
            return;
        };
        let scope = envelope
            .payload
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("session");
        match scope {
            "project" => {
                let Some(project_id) = envelope
                    .payload
                    .get("projectId")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                else {
                    self.send_error(envelope, "Project id is required.");
                    return;
                };
                self.register_project_terminal_viewers(project_id, Some(device_id));
            }
            "session" => {
                let session_id = envelope
                    .session_id
                    .as_deref()
                    .or_else(|| envelope.payload.get("sessionId").and_then(Value::as_str));
                let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) else {
                    self.send_error(envelope, "Terminal session id is required.");
                    return;
                };
                self.register_terminal_viewer(session_id, Some(device_id));
                self.send_terminal_viewport_state(session_id, Some(device_id));
            }
            _ => self.send_error(envelope, "Unsupported terminal subscription scope."),
        }
    }

    fn handle_terminal_unsubscribe(&self, envelope: &RemoteEnvelope) {
        let Some(device_id) = envelope.device_id.as_deref() else {
            return;
        };
        let scope = envelope
            .payload
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("session");
        match scope {
            "project" => {
                let Some(project_id) = envelope
                    .payload
                    .get("projectId")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                else {
                    return;
                };
                self.remove_project_terminal_viewers(project_id, Some(device_id));
            }
            "session" => {
                let session_id = envelope
                    .session_id
                    .as_deref()
                    .or_else(|| envelope.payload.get("sessionId").and_then(Value::as_str));
                let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) else {
                    return;
                };
                self.remove_terminal_viewer_for_session(session_id, Some(device_id));
            }
            _ => {}
        }
    }

    fn handle_worktree_list(&self, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        self.send_worktree_summary(
            "worktree.list",
            envelope.device_id.as_deref(),
            &project_id,
            &project_path,
        );
    }

    fn handle_worktree_select(&self, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        let Some(worktree_id) = envelope.payload.get("worktreeId").and_then(Value::as_str) else {
            self.send_error(envelope, "Worktree id is required.");
            return;
        };
        let service = WorktreeService::new(self.support_dir.clone());
        match service.select_worktree(&project_id, worktree_id) {
            Ok(()) => {
                self.send_worktree_summary(
                    "worktree.updated",
                    envelope.device_id.as_deref(),
                    &project_id,
                    &project_path,
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_worktree_create(&self, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        let Some(branch_name) = envelope.payload.get("branchName").and_then(Value::as_str) else {
            self.send_error(envelope, "Branch name is required.");
            return;
        };
        let request = WorktreeCreateRequest {
            project_id: project_id.clone(),
            project_path: project_path.clone(),
            base_branch: envelope
                .payload
                .get("baseBranch")
                .and_then(Value::as_str)
                .map(str::to_string),
            branch_name: branch_name.to_string(),
            task_title: envelope
                .payload
                .get("taskTitle")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        match WorktreeService::new(self.support_dir.clone()).create_from_request(request) {
            Ok(snapshot) => {
                let git = crate::git::GitService::status(&project_path);
                self.send(
                    "worktree.updated",
                    envelope.device_id.as_deref(),
                    None,
                    json!({
                        "projectId": project_id,
                        "selectedWorktreeId": snapshot.selected_worktree_id,
                        "worktrees": snapshot.worktrees,
                        "tasks": snapshot.tasks,
                        "baseBranches": remote_worktree_base_branches(&git),
                        "defaultBaseBranch": remote_default_worktree_base_branch(&git),
                        "error": snapshot.error,
                    }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_worktree_merge(&self, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        let Some(worktree_path) = envelope.payload.get("worktreePath").and_then(Value::as_str)
        else {
            self.send_error(envelope, "Worktree path is required.");
            return;
        };
        let request = WorktreeMergeRequest {
            project_id: project_id.clone(),
            project_path: project_path.clone(),
            worktree_path: worktree_path.to_string(),
            base_branch: envelope
                .payload
                .get("baseBranch")
                .and_then(Value::as_str)
                .map(str::to_string),
            remove_branch: envelope
                .payload
                .get("removeBranch")
                .and_then(Value::as_bool),
        };
        match WorktreeService::new(self.support_dir.clone()).merge_from_request(request) {
            Ok(snapshot) => {
                let git = crate::git::GitService::status(&project_path);
                self.send(
                    "worktree.updated",
                    envelope.device_id.as_deref(),
                    None,
                    json!({
                        "projectId": project_id,
                        "selectedWorktreeId": snapshot.selected_worktree_id,
                        "worktrees": snapshot.worktrees,
                        "tasks": snapshot.tasks,
                        "baseBranches": remote_worktree_base_branches(&git),
                        "defaultBaseBranch": remote_default_worktree_base_branch(&git),
                        "error": snapshot.error,
                    }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_worktree_remove(&self, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        let Some(worktree_path) = envelope.payload.get("worktreePath").and_then(Value::as_str)
        else {
            self.send_error(envelope, "Worktree path is required.");
            return;
        };
        let request = WorktreeRemoveRequest {
            project_id: project_id.clone(),
            project_path: project_path.clone(),
            worktree_path: worktree_path.to_string(),
            remove_branch: envelope
                .payload
                .get("removeBranch")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
        match WorktreeService::new(self.support_dir.clone()).remove_from_request(request) {
            Ok(snapshot) => {
                let git = crate::git::GitService::status(&project_path);
                self.send(
                    "worktree.updated",
                    envelope.device_id.as_deref(),
                    None,
                    json!({
                        "projectId": project_id,
                        "selectedWorktreeId": snapshot.selected_worktree_id,
                        "worktrees": snapshot.worktrees,
                        "tasks": snapshot.tasks,
                        "baseBranches": remote_worktree_base_branches(&git),
                        "defaultBaseBranch": remote_default_worktree_base_branch(&git),
                        "error": snapshot.error,
                    }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_ai_stats(&self, envelope: &RemoteEnvelope) {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let project_store = ProjectStore::new(self.support_dir.clone());
        let project = project_store
            .projects_snapshot()
            .into_iter()
            .find(|project| project.id == project_id)
            .or_else(|| project_store.projects_snapshot().into_iter().next());
        let Some(project) = project else {
            self.send_error(envelope, "Unable to load AI stats.");
            return;
        };
        let request = AIHistoryProjectRequest {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
        };
        match self.ai_history.project_state(request) {
            Ok(state) => match remote_ai_stats_payload(project.id, project.name, state) {
                Ok(payload) => self.send("ai.stats", envelope.device_id.as_deref(), None, payload),
                Err(error) => self.send_error(envelope, &error),
            },
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_git_status(&self, envelope: &RemoteEnvelope) {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let project_path = envelope.payload.get("projectPath").and_then(Value::as_str);
        let project_store = ProjectStore::new(self.support_dir.clone());
        let project = project_path
            .filter(|path| !path.trim().is_empty())
            .map(|path| (project_id.to_string(), path.to_string()))
            .or_else(|| {
                project_store
                    .projects_snapshot()
                    .into_iter()
                    .find(|project| project.id == project_id)
                    .or_else(|| project_store.projects_snapshot().into_iter().next())
                    .map(|project| (project.id, project.path))
            });
        let Some((project_id, project_path)) = project else {
            self.send_error(envelope, "Unable to load Git status.");
            return;
        };
        let summary = crate::git::GitService::status(&project_path);
        self.send(
            "git.status",
            envelope.device_id.as_deref(),
            None,
            remote_git_status_payload(project_id, project_path, summary),
        );
    }

    fn handle_terminal_create(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let runtime = Arc::clone(self);
        let emit = move |event| {
            runtime.handle_terminal_event(event);
        };
        let plan = match self.remote_terminal_plan_from_envelope(envelope, None) {
            Ok(plan) => plan,
            Err(error) => {
                self.send_error(envelope, &error);
                return;
            }
        };
        self.set_remote_project_scope(envelope.device_id.as_deref(), &plan.scope.project_id);
        match self.terminals.create(plan.config, emit) {
            Ok(session_id) => {
                self.persist_remote_terminal_layout(
                    &plan.scope.layout_key,
                    &session_id,
                    &plan.title,
                    &plan.layout_kind,
                );
                self.mark_terminal_event_subscription(&session_id);
                self.register_terminal_viewer(&session_id, envelope.device_id.as_deref());
                self.send_terminal_data(
                    "terminal.created",
                    envelope.device_id.as_deref(),
                    Some(&session_id),
                    self.remote_terminal_payload(&session_id)
                        .unwrap_or_else(|| json!({ "id": session_id })),
                );
                self.send_terminal_list(envelope.device_id.as_deref());
                self.send_terminal_viewport_state(&session_id, envelope.device_id.as_deref());
                self.send_terminal_buffer(
                    &session_id,
                    envelope.device_id.as_deref(),
                    0,
                    REMOTE_TERMINAL_BUFFER_MAX_CHARS,
                    None,
                    None,
                    false,
                );
            }
            Err(error) => self.send_error(envelope, &error.to_string()),
        }
    }

    fn handle_terminal_buffer(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            self.send_error(envelope, "Terminal session is required.");
            return;
        };
        let offset = envelope
            .payload
            .get("offset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let max_chars = envelope
            .payload
            .get("maxChars")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .filter(|value| *value > 0)
            .unwrap_or(REMOTE_TERMINAL_BUFFER_MAX_CHARS);
        let chunk_chars = envelope
            .payload
            .get("chunkChars")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .filter(|value| *value > 0)
            .map(|value| value.clamp(4 * 1024, 64 * 1024));
        let request_id = envelope
            .payload
            .get("requestId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let tail = envelope
            .payload
            .get("tail")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Err(error) = self.ensure_remote_terminal_started(session_id, envelope) {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!("terminal_buffer start_failed session={session_id} error={error}"),
            );
            self.send_error(envelope, &error);
            return;
        }
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        self.send_terminal_buffer(
            session_id,
            envelope.device_id.as_deref(),
            offset,
            max_chars,
            chunk_chars,
            request_id,
            tail,
        );
    }

    fn handle_terminal_input(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            self.send_error(envelope, "Terminal session is required.");
            return;
        };
        let Some(data) = envelope.payload.get("data").and_then(Value::as_str) else {
            self.send_error(envelope, "Terminal input is required.");
            return;
        };
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        if let Some(input_id) = envelope.payload.get("inputId").and_then(Value::as_str) {
            self.send_terminal_data(
                "terminal.input.ack",
                envelope.device_id.as_deref(),
                Some(session_id),
                json!({ "inputId": input_id, "ok": true, "accepted": true }),
            );
        }
        if let Err(error) = self.ensure_remote_terminal_started(session_id, envelope) {
            self.send_error(envelope, &error);
            return;
        }
        if let Err(error) = self.terminals.write(session_id, data.as_bytes()) {
            self.send_error(envelope, &error.to_string());
        }
    }

    fn handle_terminal_resize(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        let cols = envelope
            .payload
            .get("cols")
            .and_then(Value::as_u64)
            .unwrap_or(100) as u16;
        let rows = envelope
            .payload
            .get("rows")
            .and_then(Value::as_u64)
            .unwrap_or(30) as u16;
        if self
            .ensure_remote_terminal_started(session_id, envelope)
            .is_err()
        {
            return;
        }
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        let owner = self.remote_viewport_owner(envelope);
        let _ = self.terminals.claim_viewport(session_id, &owner);
        self.resize_terminal_viewport_from_envelope(session_id, envelope, cols, rows);
    }

    fn handle_terminal_viewport_claim(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        if self
            .ensure_remote_terminal_started(session_id, envelope)
            .is_err()
        {
            return;
        }
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        let owner = self.remote_viewport_owner(envelope);
        if let Ok(state) = self.terminals.claim_viewport(session_id, &owner) {
            self.send_terminal_viewport_state_payload(
                session_id,
                envelope.device_id.as_deref(),
                &state,
            );
        }
    }

    fn handle_terminal_viewport_resize(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        let Some(cols) = envelope
            .payload
            .get("cols")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
        else {
            return;
        };
        let Some(rows) = envelope
            .payload
            .get("rows")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
        else {
            return;
        };
        if self
            .ensure_remote_terminal_started(session_id, envelope)
            .is_err()
        {
            return;
        }
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        self.resize_terminal_viewport_from_envelope(session_id, envelope, cols, rows);
    }

    fn handle_terminal_viewport_release(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        let owner = self.remote_viewport_owner(envelope);
        if let Ok(Some(state)) = self.terminals.release_viewport(session_id, &owner) {
            self.send_terminal_viewport_state_payload(
                session_id,
                envelope.device_id.as_deref(),
                &state,
            );
        }
    }

    fn resize_terminal_viewport_from_envelope(
        &self,
        session_id: &str,
        envelope: &RemoteEnvelope,
        cols: u16,
        rows: u16,
    ) {
        let owner = self.remote_viewport_owner(envelope);
        match self
            .terminals
            .resize_viewport(session_id, &owner, cols, rows)
        {
            Ok(Some(state)) => self.send_terminal_viewport_state_payload(
                session_id,
                envelope.device_id.as_deref(),
                &state,
            ),
            Ok(None) => {
                self.send_terminal_viewport_state(session_id, envelope.device_id.as_deref())
            }
            Err(error) => self.send(
                "error",
                envelope.device_id.as_deref(),
                Some(session_id),
                json!({ "message": error.to_string() }),
            ),
        }
    }

    fn remote_viewport_owner(&self, envelope: &RemoteEnvelope) -> String {
        envelope
            .device_id
            .as_deref()
            .map(terminal_viewport_remote_owner)
            .unwrap_or_else(|| "remote".to_string())
    }

    fn handle_terminal_close(&self, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        match self.terminals.kill(session_id) {
            Ok(()) => {
                self.clear_terminal_output_seq(session_id);
                self.send_terminal_data(
                    "terminal.closed",
                    envelope.device_id.as_deref(),
                    Some(session_id),
                    json!({ "id": session_id }),
                );
                self.send_terminal_list(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error.to_string()),
        }
    }

    fn handle_terminal_signal(&self, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        let signal = envelope
            .payload
            .get("signal")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match signal {
            "interrupt" => {
                let _ = self.terminals.write(session_id, &[0x03]);
            }
            "escape" => {
                let _ = self.terminals.write(session_id, &[0x1b]);
            }
            _ => {}
        }
    }

    fn handle_terminal_upload(&self, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            self.send_error(envelope, "Terminal session is required.");
            return;
        };
        let Some(data) = envelope.payload.get("data").and_then(Value::as_str) else {
            self.send_error(envelope, "Upload data is required.");
            return;
        };
        let bytes = match remote_upload_decode(data) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.send_error(envelope, &error);
                return;
            }
        };
        let name = sanitized_remote_upload_name(
            envelope
                .payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("upload.png"),
        );
        let kind = remote_terminal_upload_kind(&envelope.payload);
        match self.write_terminal_upload_file(session_id, &name, &bytes) {
            Ok(path) => {
                self.finish_terminal_upload(envelope.device_id.as_deref(), session_id, path, &kind)
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_terminal_upload_start(&self, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            self.send_terminal_upload_ack(
                envelope,
                "start",
                None,
                false,
                Some("Terminal session is required."),
            );
            return;
        };
        let Some(upload_id) = envelope.payload.get("uploadId").and_then(Value::as_str) else {
            self.send_terminal_upload_ack(
                envelope,
                "start",
                None,
                false,
                Some("Upload id is required."),
            );
            return;
        };
        let total_bytes = envelope
            .payload
            .get("totalBytes")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let total_chunks = envelope
            .payload
            .get("totalChunks")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if total_bytes == 0 || total_bytes > 20 * 1024 * 1024 || total_chunks == 0 {
            self.send_terminal_upload_ack(
                envelope,
                "start",
                None,
                false,
                Some("Upload size is not supported."),
            );
            return;
        }
        let name = sanitized_remote_upload_name(
            envelope
                .payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("upload.png"),
        );
        let kind = remote_terminal_upload_kind(&envelope.payload);
        let directory = remote_terminal_upload_directory(session_id);
        if let Err(error) = fs::create_dir_all(&directory) {
            self.send_terminal_upload_ack(envelope, "start", None, false, Some(&error.to_string()));
            return;
        }
        let final_path = unique_remote_upload_path(&directory, &name);
        let partial_path = final_path.with_extension(format!(
            "{}.part-{}",
            final_path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("upload"),
            upload_id
        ));
        if fs::File::create(&partial_path).is_err() {
            self.send_terminal_upload_ack(
                envelope,
                "start",
                None,
                false,
                Some("Unable to create upload file."),
            );
            return;
        }
        if let Ok(mut uploads) = self.terminal_upload_sessions.lock() {
            uploads.insert(
                upload_id.to_string(),
                RemoteTerminalUploadSession {
                    session_id: session_id.to_string(),
                    device_id: envelope.device_id.clone(),
                    total_bytes,
                    total_chunks,
                    partial_path,
                    final_path,
                    kind,
                    received_chunks: HashSet::new(),
                    received_bytes: 0,
                },
            );
        }
        self.send_terminal_upload_ack(envelope, "start", None, true, None);
    }

    fn handle_terminal_upload_chunk(&self, envelope: &RemoteEnvelope) {
        let Some(upload_id) = envelope.payload.get("uploadId").and_then(Value::as_str) else {
            self.send_terminal_upload_ack(
                envelope,
                "chunk",
                None,
                false,
                Some("Upload id is required."),
            );
            return;
        };
        let chunk_index = envelope
            .payload
            .get("chunkIndex")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let offset = envelope
            .payload
            .get("offset")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let Some(data) = envelope.payload.get("data").and_then(Value::as_str) else {
            self.send_terminal_upload_ack(
                envelope,
                "chunk",
                Some(chunk_index),
                false,
                Some("Upload data is required."),
            );
            return;
        };
        let bytes = match remote_upload_decode(data) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.send_terminal_upload_ack(
                    envelope,
                    "chunk",
                    Some(chunk_index),
                    false,
                    Some(&error),
                );
                return;
            }
        };
        let mut uploads = match self.terminal_upload_sessions.lock() {
            Ok(uploads) => uploads,
            Err(_) => {
                self.send_terminal_upload_ack(
                    envelope,
                    "chunk",
                    Some(chunk_index),
                    false,
                    Some("Upload lock failed."),
                );
                return;
            }
        };
        let Some(session) = uploads.get_mut(upload_id) else {
            self.send_terminal_upload_ack(
                envelope,
                "chunk",
                Some(chunk_index),
                false,
                Some("Upload not found."),
            );
            return;
        };
        if chunk_index >= session.total_chunks || offset + bytes.len() as u64 > session.total_bytes
        {
            self.send_terminal_upload_ack(
                envelope,
                "chunk",
                Some(chunk_index),
                false,
                Some("Invalid upload chunk."),
            );
            return;
        }
        match fs::OpenOptions::new()
            .write(true)
            .open(&session.partial_path)
        {
            Ok(mut file) => {
                if file.seek(std::io::SeekFrom::Start(offset)).is_err()
                    || file.write_all(&bytes).is_err()
                {
                    self.send_terminal_upload_ack(
                        envelope,
                        "chunk",
                        Some(chunk_index),
                        false,
                        Some("Upload write failed."),
                    );
                    return;
                }
                session.received_chunks.insert(chunk_index);
                session.received_bytes = session.received_bytes.saturating_add(bytes.len() as u64);
                self.send_terminal_upload_ack(envelope, "chunk", Some(chunk_index), true, None);
            }
            Err(error) => self.send_terminal_upload_ack(
                envelope,
                "chunk",
                Some(chunk_index),
                false,
                Some(&error.to_string()),
            ),
        }
    }

    fn handle_terminal_upload_finish(&self, envelope: &RemoteEnvelope) {
        let Some(upload_id) = envelope.payload.get("uploadId").and_then(Value::as_str) else {
            self.send_terminal_upload_ack(
                envelope,
                "finish",
                None,
                false,
                Some("Upload id is required."),
            );
            return;
        };
        let session = match self.terminal_upload_sessions.lock() {
            Ok(mut uploads) => uploads.remove(upload_id),
            Err(_) => None,
        };
        let Some(session) = session else {
            self.send_terminal_upload_ack(
                envelope,
                "finish",
                None,
                false,
                Some("Upload not found."),
            );
            return;
        };
        if session.received_chunks.len() != session.total_chunks {
            self.send_terminal_upload_ack(
                envelope,
                "finish",
                None,
                false,
                Some("Upload is missing chunks."),
            );
            return;
        }
        if fs::rename(&session.partial_path, &session.final_path).is_err() {
            self.send_terminal_upload_ack(
                envelope,
                "finish",
                None,
                false,
                Some("Upload finish failed."),
            );
            return;
        }
        self.send_terminal_upload_ack(envelope, "finish", None, true, None);
        self.finish_terminal_upload(
            session.device_id.as_deref(),
            &session.session_id,
            session.final_path,
            &session.kind,
        );
    }

    fn handle_terminal_upload_cancel(&self, envelope: &RemoteEnvelope) {
        let Some(upload_id) = envelope.payload.get("uploadId").and_then(Value::as_str) else {
            return;
        };
        let session = self
            .terminal_upload_sessions
            .lock()
            .ok()
            .and_then(|mut uploads| uploads.remove(upload_id));
        if let Some(session) = session {
            let _ = fs::remove_file(session.partial_path);
        }
        self.send_terminal_upload_ack(envelope, "cancel", None, true, None);
    }

    fn write_terminal_upload_file(
        &self,
        session_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, String> {
        let directory = remote_terminal_upload_directory(session_id);
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let path = unique_remote_upload_path(&directory, name);
        fs::write(&path, bytes).map_err(|error| error.to_string())?;
        Ok(path)
    }

    fn finish_terminal_upload(
        &self,
        device_id: Option<&str>,
        session_id: &str,
        path: PathBuf,
        kind: &str,
    ) {
        let text = format!("{} ", terminal_upload_path_input(&path));
        let _ = self.terminals.write(session_id, text.as_bytes());
        self.send_terminal_data(
            "terminal.uploaded",
            device_id,
            Some(session_id),
            json!({
                "path": path.to_string_lossy().to_string(),
                "name": path.file_name().and_then(|value| value.to_str()).unwrap_or_default(),
                "kind": kind,
                "mode": "path",
                "tool": Value::Null,
                "inserted": true,
            }),
        );
    }

    fn send_terminal_upload_ack(
        &self,
        envelope: &RemoteEnvelope,
        stage: &str,
        chunk_index: Option<usize>,
        ok: bool,
        message: Option<&str>,
    ) {
        let mut payload = json!({
            "uploadId": envelope.payload.get("uploadId").cloned().unwrap_or(Value::Null),
            "stage": stage,
            "ok": ok,
        });
        if let Some(chunk_index) = chunk_index {
            payload["chunkIndex"] = json!(chunk_index);
        } else if let Some(value) = envelope.payload.get("chunkIndex") {
            payload["chunkIndex"] = value.clone();
        }
        if let Some(message) = message {
            payload["message"] = json!(message);
        }
        self.send_terminal_data(
            "terminal.upload.ack",
            envelope.device_id.as_deref(),
            envelope.session_id.as_deref(),
            payload,
        );
    }

    fn send_project_and_terminal_lists(&self, device_id: Option<&str>) {
        self.send_project_list(device_id);
        self.send_terminal_list(device_id);
    }

    fn send_project_list(&self, device_id: Option<&str>) {
        let payload = self.remote_project_list_payload(device_id);
        self.send("project.list", device_id, None, payload);
    }

    fn remote_project_list_payload(&self, device_id: Option<&str>) -> Value {
        let snapshot = ProjectStore::new(self.support_dir.clone()).list_snapshot();
        let selected_project_id = self
            .remote_project_scope_id(device_id)
            .filter(|id| snapshot.projects.iter().any(|project| &project.id == id))
            .or(snapshot.selected_project_id);
        let projects = snapshot
            .projects
            .into_iter()
            .map(|project| {
                json!({
                    "id": project.id,
                    "name": project.name,
                    "path": project.path,
                })
            })
            .collect::<Vec<_>>();
        json!({ "projects": projects, "selectedProjectId": selected_project_id })
    }

    fn send_terminal_list(&self, device_id: Option<&str>) {
        let terminals = self.remote_terminals();
        self.send(
            "terminal.list",
            device_id,
            None,
            json!({ "terminals": terminals }),
        );
    }

    fn send_worktree_summary(
        &self,
        kind: &str,
        device_id: Option<&str>,
        project_id: &str,
        project_path: &str,
    ) {
        let summary = WorktreeService::new(self.support_dir.clone())
            .summary(Some(project_id), Some(project_path));
        let base_branches = remote_worktree_base_branches(&summary.active_git);
        let default_base_branch = remote_default_worktree_base_branch(&summary.active_git);
        self.send(
            kind,
            device_id,
            None,
            json!({
                "projectId": project_id,
                "selectedWorktreeId": summary.selected_worktree_id,
                "worktrees": summary.worktrees,
                "tasks": summary.tasks,
                "available": summary.available,
                "baseBranches": base_branches,
                "defaultBaseBranch": default_base_branch,
                "error": summary.error,
            }),
        );
    }

    fn send(&self, kind: &str, device_id: Option<&str>, session_id: Option<&str>, payload: Value) {
        self.send_transport(kind, device_id, session_id, payload);
    }

    fn send_plain(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) -> bool {
        let envelope = super::types::RemoteOutgoingEnvelope {
            kind: kind.to_string(),
            device_id: device_id.map(str::to_string),
            session_id: session_id.map(str::to_string),
            seq: None,
            payload,
        };
        let Ok(data) = serde_json::to_vec(&envelope) else {
            return false;
        };
        let transport = self.transport.lock().ok().and_then(|value| value.clone());
        let Some(transport) = transport else {
            return false;
        };
        transport.send(data, device_id)
    }

    fn send_terminal_data(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) {
        self.send_transport(kind, device_id, session_id, payload);
    }

    fn send_error(&self, envelope: &RemoteEnvelope, message: &str) {
        self.send_transport(
            "error",
            envelope.device_id.as_deref(),
            envelope.session_id.as_deref(),
            json!({ "message": message }),
        );
    }

    fn outgoing_transport_text(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) -> Option<String> {
        let Ok(mut send_seq) = self.send_seq_by_device.lock() else {
            return None;
        };
        self.service()
            .outgoing_transport_text(kind, device_id, session_id, payload, &mut send_seq)
    }

    fn update_device_online(&self, device_id: Option<&str>, online: bool) {
        let Some(device_id) = device_id else {
            return;
        };
        let mut status = self.snapshot();
        if !status
            .device_list
            .iter()
            .any(|device| device.id == device_id)
        {
            status = self.summary_from_settings_preserving_connection();
        }
        if let Some(device) = status
            .device_list
            .iter_mut()
            .find(|device| device.id == device_id)
        {
            device.online = Some(online);
            if online {
                device.last_seen = chrono::Utc::now().to_rfc3339();
            }
        }
        status.online_devices = status
            .device_list
            .iter()
            .filter(|device| device.online.unwrap_or(false))
            .count();
        self.update_snapshot(status);
    }

    fn is_authorized_device(&self, device_id: Option<&str>) -> bool {
        let Some(device_id) = device_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return false;
        };
        super::remote_settings_from_raw(&self.service().raw_settings())
            .cached_devices
            .iter()
            .any(|device| {
                device.id == device_id
                    && device
                        .revoked_at
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or("")
                        .is_empty()
            })
    }

    fn update_snapshot(&self, summary: RemoteSummary) {
        if let Ok(mut current) = self.snapshot.lock() {
            *current = summary;
            if let Ok(mut events) = self.events.lock() {
                events.push_back(current.clone());
                while events.len() > 128 {
                    events.pop_front();
                }
            }
        }
    }

    fn summary_from_settings_preserving_connection(&self) -> RemoteSummary {
        let mut summary = self.service().summary();
        let current = self.snapshot();
        if summary.enabled && current.enabled && summary.relay == current.relay {
            summary.status = current.status;
            summary.message = current.message;
            summary.pairing = current.pairing;
            summary.pending_pairing_list = current.pending_pairing_list;
            summary.pending_pairings = summary.pending_pairing_list.len();
        }
        summary
    }

    fn service(&self) -> RemoteService {
        RemoteService::new(self.support_dir.clone())
    }

    fn send_terminal_buffer(
        self: &Arc<Self>,
        session_id: &str,
        device_id: Option<&str>,
        offset: usize,
        max_chars: usize,
        chunk_chars: Option<usize>,
        request_id: Option<String>,
        tail: bool,
    ) {
        self.register_terminal_viewer(session_id, device_id);
        match self.terminal_buffer_window(session_id, offset, max_chars, request_id, tail) {
            Ok(window) => {
                let output_seq = self.current_terminal_output_seq(session_id);
                for payload in terminal_buffer_payloads(&window, output_seq, chunk_chars) {
                    self.send_terminal_data(
                        "terminal.output",
                        device_id,
                        Some(session_id),
                        payload,
                    );
                }
            }
            Err(error) => {
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!("terminal_buffer snapshot_failed session={session_id} error={error}"),
                );
                self.send(
                    "error",
                    device_id,
                    Some(session_id),
                    json!({ "message": error.to_string() }),
                );
            }
        }
    }

    fn send_terminal_viewport_state(&self, session_id: &str, device_id: Option<&str>) {
        if let Ok(state) = self.terminals.viewport_state(session_id) {
            self.send_terminal_viewport_state_payload(session_id, device_id, &state);
        }
    }

    fn send_terminal_viewport_state_payload(
        &self,
        session_id: &str,
        device_id: Option<&str>,
        state: &TerminalViewportState,
    ) {
        self.send_terminal_data(
            "terminal.viewport.state",
            device_id,
            Some(session_id),
            json!({
                "owner": state.owner,
                "cols": state.cols,
                "rows": state.rows,
                "generation": state.generation,
            }),
        );
    }

    fn terminal_buffer_window(
        &self,
        session_id: &str,
        offset: usize,
        max_chars: usize,
        request_id: Option<String>,
        tail: bool,
    ) -> Result<RemoteTerminalBufferWindow, anyhow::Error> {
        let max_chars = max_chars.max(1);
        if tail {
            let snapshot = self.terminals.screen_snapshot(session_id)?;
            let total_characters = snapshot.data.chars().count();
            return Ok(RemoteTerminalBufferWindow {
                data: snapshot.data,
                offset: 0,
                total_characters,
                truncated: false,
                request_id,
                tail: true,
                screen_snapshot: true,
                has_previous: false,
            });
        }

        let total_characters = self.terminals.buffer_characters(session_id)?;
        let (chunk, clamped) = {
            let data = self.terminals.snapshot(session_id)?;
            let clamped = offset.min(total_characters);
            let chunk = data
                .chars()
                .skip(clamped)
                .take(max_chars)
                .collect::<String>();
            (chunk, clamped)
        };
        let truncated = clamped + chunk.chars().count() < total_characters;
        Ok(RemoteTerminalBufferWindow {
            data: chunk,
            offset: clamped,
            total_characters,
            truncated,
            request_id,
            tail: false,
            screen_snapshot: false,
            has_previous: clamped > 0,
        })
    }

    fn remote_terminal_payload(&self, session_id: &str) -> Option<Value> {
        self.remote_terminals()
            .into_iter()
            .find(|value| value.get("id").and_then(Value::as_str) == Some(session_id))
    }

    fn remote_terminals(&self) -> Vec<Value> {
        let layouts = self.remote_terminal_layout_kinds();
        let mut terminals = self
            .terminals
            .list()
            .into_iter()
            .map(|terminal| {
                let layout_kind = layouts
                    .get(&terminal.id)
                    .map(String::as_str)
                    .unwrap_or("split");
                remote_terminal_snapshot_payload(terminal, layout_kind)
            })
            .collect::<Vec<_>>();
        terminals.sort_by_key(remote_terminal_order_key);
        terminals
    }

    fn remote_terminal_layout_kinds(&self) -> HashMap<String, String> {
        let project_store = ProjectStore::new(self.support_dir.clone());
        let snapshot = project_store.snapshot();
        let keys = snapshot
            .projects
            .iter()
            .map(|project| {
                let worktree_id = snapshot
                    .selected_worktree_id_by_project
                    .get(&project.id)
                    .map(String::as_str)
                    .unwrap_or(&project.id);
                terminal_layout_storage_key(&project.id, worktree_id)
            })
            .collect::<Vec<_>>();
        let layouts = TerminalLayoutService::new(self.support_dir.clone())
            .load_many(keys.iter().map(String::as_str));
        let mut result = HashMap::new();
        for layout in layouts.values() {
            for pane in &layout.top_panes {
                result.insert(pane.terminal_id.clone(), "split".to_string());
            }
            for tab in &layout.tabs {
                result.insert(tab.terminal_id.clone(), "tab".to_string());
            }
        }
        result
    }

    fn remote_terminal_plan_from_envelope(
        &self,
        envelope: &RemoteEnvelope,
        terminal_id: Option<&str>,
    ) -> Result<RemoteTerminalPlan, String> {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let scope = self.remote_project_scope_for_envelope(envelope, project_id)?;
        let command = envelope
            .payload
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty());
        let title = envelope
            .payload
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty())
            .or_else(|| command.clone())
            .unwrap_or_else(|| "Terminal".to_string());
        let cwd = envelope
            .payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| scope.project_path.clone());
        let terminal_id = terminal_id
            .map(str::to_string)
            .or_else(|| self.saved_remote_terminal_id(&scope.layout_key));
        let config = TerminalPtyConfig {
            cwd: Some(cwd),
            command,
            cols: envelope
                .payload
                .get("cols")
                .and_then(Value::as_u64)
                .map(|value| value as u16),
            rows: envelope
                .payload
                .get("rows")
                .and_then(Value::as_u64)
                .map(|value| value as u16),
            project_id: Some(scope.project_id.clone()),
            project_name: Some(scope.project_name.clone()),
            terminal_id,
            title: Some(title.clone()),
            ..Default::default()
        };
        Ok(RemoteTerminalPlan {
            config,
            scope,
            title,
            layout_kind: remote_terminal_layout_kind(&envelope.payload),
        })
    }

    fn ensure_remote_terminal_started(
        self: &Arc<Self>,
        session_id: &str,
        envelope: &RemoteEnvelope,
    ) -> Result<(), String> {
        if self.terminals.snapshot(session_id).is_ok() {
            self.ensure_terminal_event_subscription(session_id);
            return Ok(());
        }
        let runtime = Arc::clone(self);
        let emit = move |event| {
            runtime.handle_terminal_event(event);
        };
        let plan = self.remote_terminal_plan_from_envelope(envelope, Some(session_id))?;
        self.set_remote_project_scope(envelope.device_id.as_deref(), &plan.scope.project_id);
        self.terminals
            .create(plan.config, emit)
            .map_err(|error| error.to_string())?;
        self.persist_remote_terminal_layout(
            &plan.scope.layout_key,
            session_id,
            &plan.title,
            &plan.layout_kind,
        );
        self.mark_terminal_event_subscription(session_id);
        self.send_terminal_list(envelope.device_id.as_deref());
        Ok(())
    }

    fn ensure_remote_project_terminal(
        self: &Arc<Self>,
        scope: &RemoteProjectScope,
        device_id: Option<&str>,
    ) -> Result<String, String> {
        let existing = self
            .remote_terminals()
            .into_iter()
            .find(|terminal| {
                terminal.get("projectId").and_then(Value::as_str) == Some(scope.project_id.as_str())
            })
            .and_then(|terminal| {
                terminal
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.trim().is_empty())
                    .map(str::to_string)
            });
        if let Some(session_id) = existing {
            self.ensure_terminal_event_subscription(&session_id);
            return Ok(session_id);
        }

        let terminal_id = self.saved_remote_terminal_id(&scope.layout_key);
        let title = "Terminal".to_string();
        let config = TerminalPtyConfig {
            cwd: Some(scope.project_path.clone()),
            project_id: Some(scope.project_id.clone()),
            project_name: Some(scope.project_name.clone()),
            terminal_id: terminal_id.clone(),
            title: Some(title.clone()),
            ..Default::default()
        };
        let runtime = Arc::clone(self);
        let emit = move |event| {
            runtime.handle_terminal_event(event);
        };
        let session_id = self
            .terminals
            .create(config, emit)
            .map_err(|error| error.to_string())?;
        self.persist_remote_terminal_layout(&scope.layout_key, &session_id, &title, "split");
        self.mark_terminal_event_subscription(&session_id);
        self.register_terminal_viewer(&session_id, device_id);
        Ok(session_id)
    }

    fn saved_remote_terminal_id(&self, layout_key: &str) -> Option<String> {
        let layout = TerminalLayoutService::new(self.support_dir.clone()).load(Some(layout_key));
        layout
            .top_panes
            .first()
            .map(|pane| pane.terminal_id.clone())
            .or_else(|| layout.tabs.first().map(|tab| tab.terminal_id.clone()))
            .filter(|id| !id.trim().is_empty())
    }

    fn persist_remote_terminal_layout(
        &self,
        layout_key: &str,
        terminal_id: &str,
        title: &str,
        layout_kind: &str,
    ) {
        if layout_key.trim().is_empty() {
            return;
        }
        let service = TerminalLayoutService::new(self.support_dir.clone());
        let layout = service.load(Some(layout_key));
        if layout
            .top_panes
            .iter()
            .any(|pane| pane.terminal_id == terminal_id)
            || layout.tabs.iter().any(|tab| tab.terminal_id == terminal_id)
        {
            return;
        }
        let title = if title.trim().is_empty() {
            "Terminal"
        } else {
            title.trim()
        };
        let mut layout = layout;
        if layout_kind == "tab" {
            layout.tabs.push(TerminalTabSummary {
                label: title.to_string(),
                terminal_id: terminal_id.to_string(),
            });
        } else {
            layout.top_panes.push(TerminalPaneSummary {
                title: title.to_string(),
                terminal_id: terminal_id.to_string(),
            });
        }
        let _ = service.save_summary(layout_key, layout);
    }

    fn remote_project_scope(&self, project_id: &str) -> Result<RemoteProjectScope, String> {
        let snapshot = ProjectStore::new(self.support_dir.clone()).snapshot();
        let project = snapshot
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        let worktree_id = snapshot
            .selected_worktree_id_by_project
            .get(&project.id)
            .cloned()
            .unwrap_or_else(|| project.id.clone());
        Ok(RemoteProjectScope {
            project_id: project.id.clone(),
            project_name: project.name.clone(),
            project_path: project.path.clone(),
            worktree_id: worktree_id.clone(),
            layout_key: terminal_layout_storage_key(&project.id, &worktree_id),
        })
    }

    fn remote_project_scope_for_envelope(
        &self,
        envelope: &RemoteEnvelope,
        project_id: Option<&str>,
    ) -> Result<RemoteProjectScope, String> {
        let Some(scoped_project_id) = project_id
            .map(str::to_string)
            .or_else(|| self.remote_project_scope_id(envelope.device_id.as_deref()))
        else {
            return Err("Project id is required.".to_string());
        };
        self.remote_project_scope(&scoped_project_id)
    }

    fn worktree_request_scope(
        &self,
        envelope: &RemoteEnvelope,
    ) -> Result<(String, String), String> {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Project id is required.".to_string())?;
        let project_path = envelope
            .payload
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| {
                ProjectStore::new(self.support_dir.clone())
                    .projects_snapshot()
                    .into_iter()
                    .find(|project| project.id == project_id)
                    .map(|project| project.path)
            })
            .ok_or_else(|| "Project path is required.".to_string())?;
        Ok((project_id.to_string(), project_path))
    }

    fn set_remote_project_scope(&self, device_id: Option<&str>, project_id: &str) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        if let Ok(mut scopes) = self.remote_project_scope_by_device.lock() {
            scopes.insert(device_id.to_string(), project_id.to_string());
        }
    }

    fn remote_project_scope_id(&self, device_id: Option<&str>) -> Option<String> {
        let device_id = device_id.filter(|value| !value.trim().is_empty())?;
        self.remote_project_scope_by_device
            .lock()
            .ok()
            .and_then(|scopes| scopes.get(device_id).cloned())
    }

    fn clear_remote_project_scope(&self, device_id: Option<&str>) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        if let Ok(mut scopes) = self.remote_project_scope_by_device.lock() {
            scopes.remove(device_id);
        }
    }

    fn clear_remote_project_scope_for_project(&self, project_id: &str) {
        if let Ok(mut scopes) = self.remote_project_scope_by_device.lock() {
            scopes.retain(|_, scoped_project_id| scoped_project_id != project_id);
        }
    }

    fn ensure_terminal_event_subscription(self: &Arc<Self>, session_id: &str) {
        let should_subscribe = self.mark_terminal_event_subscription(session_id);
        if !should_subscribe {
            return;
        }
        let runtime = Arc::clone(self);
        let emit = Arc::new(move |event| {
            runtime.handle_terminal_event(event);
            true
        });
        if self.terminals.subscribe_events(session_id, emit).is_err() {
            if let Ok(mut subscriptions) = self.terminal_event_subscriptions.lock() {
                subscriptions.remove(session_id);
            }
        }
    }

    fn mark_terminal_event_subscription(&self, session_id: &str) -> bool {
        self.terminal_event_subscriptions
            .lock()
            .map(|mut subscriptions| subscriptions.insert(session_id.to_string()))
            .unwrap_or(false)
    }

    fn next_terminal_output_seq(&self, session_id: &str) -> i64 {
        self.terminal_output_seq_by_session
            .lock()
            .map(|mut sequences| {
                let next = sequences.get(session_id).copied().unwrap_or(0) + 1;
                sequences.insert(session_id.to_string(), next);
                next
            })
            .unwrap_or(0)
    }

    fn current_terminal_output_seq(&self, session_id: &str) -> i64 {
        self.terminal_output_seq_by_session
            .lock()
            .ok()
            .and_then(|sequences| sequences.get(session_id).copied())
            .unwrap_or(0)
    }

    fn clear_terminal_output_seq(&self, session_id: &str) {
        if let Ok(mut sequences) = self.terminal_output_seq_by_session.lock() {
            sequences.remove(session_id);
        }
    }

    fn register_terminal_viewer(self: &Arc<Self>, session_id: &str, device_id: Option<&str>) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        self.terminal_subscriptions
            .add_session_viewer(session_id, device_id);
        self.ensure_terminal_event_subscription(session_id);
    }

    fn register_project_terminal_viewers(
        self: &Arc<Self>,
        project_id: &str,
        device_id: Option<&str>,
    ) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        self.terminal_subscriptions
            .add_project_subscriber(project_id, device_id);
        for terminal in self.remote_terminals() {
            let Some(terminal_project_id) = terminal.get("projectId").and_then(Value::as_str)
            else {
                continue;
            };
            if terminal_project_id != project_id {
                continue;
            }
            let Some(session_id) = terminal.get("id").and_then(Value::as_str) else {
                continue;
            };
            self.register_terminal_viewer(session_id, Some(device_id));
        }
    }

    fn remove_terminal_viewer_for_session(&self, session_id: &str, device_id: Option<&str>) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        self.terminal_subscriptions
            .remove_session_viewer(session_id, device_id);
    }

    fn remove_project_terminal_viewers(&self, project_id: &str, device_id: Option<&str>) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        self.terminal_subscriptions
            .remove_project_subscriber(project_id, device_id);
        let session_ids = self
            .remote_terminals()
            .into_iter()
            .filter(|terminal| {
                terminal.get("projectId").and_then(Value::as_str) == Some(project_id)
            })
            .filter_map(|terminal| {
                terminal
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        self.terminal_subscriptions
            .remove_project_session_viewers(session_ids.iter().map(String::as_str), device_id);
    }

    fn remove_terminal_viewer(&self, device_id: Option<&str>) {
        let Some(device_id) = device_id else {
            return;
        };
        self.terminal_subscriptions.remove_device(device_id);
    }

    fn handle_terminal_event(self: &Arc<Self>, event: TerminalEvent) {
        match event {
            TerminalEvent::Output {
                session_id, text, ..
            } => {
                self.queue_terminal_output_batch(session_id, text);
            }
            TerminalEvent::Exit { session_id, .. } => {
                if let Ok(mut subscriptions) = self.terminal_event_subscriptions.lock() {
                    subscriptions.remove(&session_id);
                }
                self.terminal_subscriptions.remove_session(&session_id);
                self.clear_terminal_output_seq(&session_id);
                self.send_terminal_data(
                    "terminal.closed",
                    None,
                    Some(&session_id),
                    json!({ "id": session_id }),
                );
            }
            TerminalEvent::Error {
                session_id,
                message,
            } => {
                self.send(
                    "error",
                    None,
                    Some(&session_id),
                    json!({ "message": message }),
                );
            }
            TerminalEvent::Viewport {
                session_id,
                owner,
                cols,
                rows,
                generation,
            } => {
                self.send_terminal_data(
                    "terminal.viewport.state",
                    None,
                    Some(&session_id),
                    json!({
                        "owner": owner,
                        "cols": cols,
                        "rows": rows,
                        "generation": generation,
                    }),
                );
                self.send_terminal_list(None);
            }
        }
    }

    fn queue_terminal_output_batch(self: &Arc<Self>, session_id: String, text: String) {
        if text.is_empty() {
            return;
        }
        let viewers = self.terminal_output_viewers(&session_id);
        if viewers.is_empty() {
            return;
        }
        let buffer_length = self.terminals.buffer_characters(&session_id).unwrap_or(0);
        let should_spawn = {
            let Ok(mut batches) = self.terminal_output_batches.lock() else {
                return;
            };
            let batch =
                batches
                    .entry(session_id.clone())
                    .or_insert_with(|| RemoteTerminalOutputBatch {
                        data: String::new(),
                        buffer_length,
                        viewers: HashSet::new(),
                    });
            let was_empty = batch.data.is_empty();
            batch.data.push_str(&text);
            batch.buffer_length = buffer_length;
            batch.viewers.extend(viewers);
            was_empty
        };
        if should_spawn {
            let runtime = Arc::clone(self);
            crate::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(REMOTE_TERMINAL_OUTPUT_BATCH_MS)).await;
                runtime.flush_terminal_output_batch(&session_id);
            });
        }
    }

    fn terminal_output_viewers(&self, session_id: &str) -> HashSet<String> {
        let project_id = self
            .terminals
            .list()
            .into_iter()
            .find(|terminal| terminal.id == session_id)
            .map(|terminal| terminal.project_id)
            .filter(|value| !value.trim().is_empty());
        self.terminal_subscriptions
            .viewers_for_session(session_id, project_id.as_deref())
    }

    fn flush_terminal_output_batch(&self, session_id: &str) {
        let batch = self
            .terminal_output_batches
            .lock()
            .ok()
            .and_then(|mut batches| batches.remove(session_id));
        let Some(batch) = batch else {
            return;
        };
        if batch.data.is_empty() || batch.viewers.is_empty() {
            return;
        }
        let output_seq = self.next_terminal_output_seq(session_id);
        for device_id in batch.viewers {
            self.send_terminal_data(
                "terminal.output",
                Some(&device_id),
                Some(session_id),
                json!({
                    "data": batch.data.clone(),
                    "bufferLength": batch.buffer_length,
                    "outputSeq": output_seq,
                }),
            );
        }
    }
}

struct RemoteTerminalUploadSession {
    session_id: String,
    device_id: Option<String>,
    total_bytes: u64,
    total_chunks: usize,
    partial_path: PathBuf,
    final_path: PathBuf,
    kind: String,
    received_chunks: HashSet<usize>,
    received_bytes: u64,
}

fn default_project_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Project")
        .to_string()
}

pub(crate) fn remote_file_list(path: Option<&str>, purpose: Option<&str>) -> Value {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let requested = path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&home);
    let requested_path = PathBuf::from(requested);
    let directory = if requested_path.is_dir() {
        requested_path
    } else {
        requested_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(&home))
    };
    let mut entries = fs::read_dir(&directory)
        .ok()
        .into_iter()
        .flat_map(|read_dir| read_dir.filter_map(Result::ok))
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            Some(json!({
                "name": name,
                "path": path.to_string_lossy().to_string(),
                "isDirectory": path.is_dir(),
            }))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        let left_dir = left
            .get("isDirectory")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let right_dir = right
            .get("isDirectory")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        right_dir.cmp(&left_dir).then_with(|| {
            left.get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_lowercase()
                .cmp(
                    &right
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_lowercase(),
                )
        })
    });
    let mut payload = json!({
        "path": directory.to_string_lossy().to_string(),
        "parent": directory.parent().map(|path| path.to_string_lossy().to_string()).unwrap_or_default(),
        "entries": entries,
    });
    if let Some(purpose) = purpose {
        payload["purpose"] = Value::String(purpose.to_string());
    }
    payload
}

pub(crate) fn remote_file_read(path: &str) -> Result<Value, String> {
    let path = PathBuf::from(path);
    if path.is_dir() {
        return Err("Cannot open a directory as a file.".to_string());
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > 2 * 1024 * 1024 {
        return Err("File is larger than 2MB and cannot be opened on mobile yet.".to_string());
    }
    let content = fs::read_to_string(&path)
        .map_err(|_| "Only UTF-8 text files can be edited on mobile.".to_string())?;
    Ok(json!({
        "path": path.to_string_lossy().to_string(),
        "name": path.file_name().and_then(|value| value.to_str()).unwrap_or_default(),
        "content": content,
        "size": content.len(),
    }))
}

pub(crate) fn remote_file_write(path: &str, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|error| error.to_string())
}

pub(crate) fn remote_file_rename(path: &str, new_path: &str) -> Result<(), String> {
    let source = PathBuf::from(path);
    let destination = PathBuf::from(new_path);
    if source.parent() != destination.parent() {
        return Err("Rename must stay in the same directory.".to_string());
    }
    if destination.exists() {
        return Err("A file with this name already exists.".to_string());
    }
    fs::rename(source, destination).map_err(|error| error.to_string())
}

fn remote_upload_decode(data: &str) -> Result<Vec<u8>, String> {
    remote_base64_url_decode(data).or_else(|_| {
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|error| error.to_string())
    })
}

fn remote_pairing_code() -> String {
    let value = uuid::Uuid::new_v4().as_u128() % 1_000_000;
    format!("{value:06}")
}

pub(crate) fn remote_terminal_upload_directory(session_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join("CoduxUploads")
        .join(sanitized_remote_upload_name(session_id))
}

pub(crate) fn sanitized_remote_upload_name(value: &str) -> String {
    let name = Path::new(value)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("upload.png");
    let cleaned = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .to_string();
    if cleaned.is_empty() {
        "upload.png".to_string()
    } else {
        cleaned
    }
}

pub(crate) fn remote_terminal_upload_kind(payload: &Value) -> String {
    let kind = payload
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("image")
        .trim()
        .to_ascii_lowercase();
    if kind == "file" {
        "file".to_string()
    } else {
        "image".to_string()
    }
}

pub(crate) fn terminal_upload_path_input(path: &Path) -> String {
    quote_terminal_path(&path.to_string_lossy())
}

#[cfg(windows)]
pub(crate) fn quote_terminal_path(value: &str) -> String {
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '&' | '(' | ')' | '[' | ']' | '{' | '}'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(not(windows))]
pub(crate) fn quote_terminal_path(value: &str) -> String {
    if value.chars().any(|ch| {
        ch.is_whitespace()
            || matches!(
                ch,
                '\'' | '"' | '\\' | '$' | '`' | '!' | '&' | '(' | ')' | ';' | '<' | '>' | '|'
            )
    }) {
        format!("'{}'", value.replace('\'', "'\\''"))
    } else {
        value.to_string()
    }
}

pub(crate) fn unique_remote_upload_path(directory: &Path, file_name: &str) -> PathBuf {
    let file_name = sanitized_remote_upload_name(file_name);
    let path = PathBuf::from(&file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("upload");
    let extension = path.extension().and_then(|value| value.to_str());
    let mut candidate = directory.join(&file_name);
    let mut index = 1;
    while candidate.exists() {
        let next = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem}-{index}.{extension}"),
            _ => format!("{stem}-{index}"),
        };
        candidate = directory.join(next);
        index += 1;
    }
    candidate
}

pub(crate) fn remote_ai_stats_payload(
    project_id: String,
    project_name: String,
    state: AIHistoryProjectState,
) -> Result<Value, String> {
    let mut value = serde_json::to_value(state).map_err(|error| error.to_string())?;
    let snapshot = value
        .get_mut("snapshot")
        .map(Value::take)
        .filter(|value| !value.is_null());
    let mut payload = snapshot.unwrap_or_else(|| {
        json!({
            "projectId": project_id,
            "projectName": project_name,
            "projectSummary": {},
            "sessions": [],
            "heatmap": [],
            "todayTimeBuckets": [],
            "toolBreakdown": [],
            "modelBreakdown": [],
        })
    });
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "updatedAt".to_string(),
            json!(chrono::Utc::now().to_rfc3339()),
        );
    }
    Ok(payload)
}

pub(crate) fn remote_git_status_payload(
    project_id: String,
    project_path: String,
    summary: crate::git::GitSummary,
) -> Value {
    json!({
        "projectId": project_id,
        "projectPath": project_path,
        "branch": summary.branch,
        "upstream": summary.upstream,
        "ahead": summary.ahead,
        "behind": summary.behind,
        "staged": summary.staged,
        "unstaged": summary.unstaged,
        "untracked": summary.untracked,
        "changes": summary.staged + summary.unstaged + summary.untracked,
        "isRepository": summary.is_repository,
        "error": summary.error,
        "changedFiles": summary.changed_files,
        "branches": summary.branches,
        "remoteBranches": summary.remote_branches,
        "remotes": summary.remotes,
        "commits": summary.commits,
    })
}

pub(crate) fn remote_terminal_order_key(value: &Value) -> (String, String) {
    let created_at = value
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    (created_at, id)
}

pub(crate) fn remote_terminal_snapshot_payload(
    terminal: TerminalSessionSnapshot,
    layout_kind: &str,
) -> Value {
    json!({
        "id": terminal.id,
        "title": terminal.title,
        "layoutKind": layout_kind,
        "displayTitle": if terminal.project_name.trim().is_empty() {
            terminal.title.clone()
        } else {
            format!("{} · {}", terminal.project_name, terminal.title)
        },
        "projectId": terminal.project_id,
        "projectName": terminal.project_name,
        "projectPath": terminal.cwd,
        "cwd": terminal.cwd,
        "shell": terminal.shell,
        "command": terminal.command,
        "cols": terminal.cols,
        "rows": terminal.rows,
        "status": terminal.status,
        "isRunning": terminal.is_running,
        "createdAt": terminal.created_at,
        "lastActiveAt": terminal.last_active_at,
        "bufferCharacters": terminal.buffer_characters,
        "hasBuffer": terminal.has_buffer,
    })
}

fn remote_terminal_layout_kind(payload: &Value) -> String {
    match payload
        .get("layoutKind")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("tab") => "tab".to_string(),
        _ => "split".to_string(),
    }
}

fn remote_worktree_base_branches(git: &crate::git::GitSummary) -> Vec<String> {
    let mut values = Vec::new();
    remote_push_unique_branch(&mut values, git.branch.as_str());
    for branch in &git.branches {
        remote_push_unique_branch(&mut values, branch.name.as_str());
    }
    values
}

fn remote_default_worktree_base_branch(git: &crate::git::GitSummary) -> String {
    git.branches
        .iter()
        .find(|branch| branch.is_current)
        .or_else(|| git.branches.first())
        .map(|branch| branch.name.clone())
        .filter(|branch| !branch.trim().is_empty())
        .unwrap_or_else(|| git.branch.clone())
}

fn remote_push_unique_branch(values: &mut Vec<String>, value: &str) {
    let branch = value.trim();
    if branch.is_empty() || values.iter().any(|item| item == branch) {
        return;
    }
    values.push(branch.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_layout::TerminalPaneSummary;

    fn temp_support_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("create temp support dir");
        dir
    }

    fn write_two_project_state(support_dir: &Path) -> (PathBuf, PathBuf) {
        let project_a = support_dir.join("project-a");
        let project_b = support_dir.join("project-b");
        fs::create_dir_all(&project_a).expect("create project a");
        fs::create_dir_all(&project_b).expect("create project b");
        fs::write(
            support_dir.join("state.json"),
            serde_json::to_string_pretty(&json!({
                "projects": [
                    {"id": "project-a", "name": "Project A", "path": project_a.to_string_lossy()},
                    {"id": "project-b", "name": "Project B", "path": project_b.to_string_lossy()}
                ],
                "worktrees": [
                    {
                        "id": "worktree-b",
                        "projectId": "project-b",
                        "name": "Task B",
                        "branch": "task-b",
                        "path": project_b.to_string_lossy(),
                        "status": "active",
                        "isDefault": true,
                        "createdAt": 1,
                        "updatedAt": 1
                    }
                ],
                "selectedProjectId": "project-a",
                "selectedWorktreeIdByProject": {
                    "project-b": "worktree-b"
                }
            }))
            .expect("serialize state"),
        )
        .expect("write state");
        (project_a, project_b)
    }

    #[test]
    fn remote_project_select_keeps_desktop_selected_project() {
        let support_dir = temp_support_dir("codux-remote-scope-select");
        write_two_project_state(&support_dir);
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));

        runtime.handle_project_select(&RemoteEnvelope {
            kind: "project.select".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            seq: None,
            payload: json!({ "projectId": "project-b" }),
        });

        let state = fs::read_to_string(support_dir.join("state.json")).expect("read state");
        let state: Value = serde_json::from_str(&state).expect("parse state");
        assert_eq!(state["selectedProjectId"], "project-a");
        assert_eq!(
            runtime.remote_project_scope_id(Some("device-1")).as_deref(),
            Some("project-b")
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_project_list_reports_device_selected_project_scope() {
        let support_dir = temp_support_dir("codux-remote-project-list-scope");
        write_two_project_state(&support_dir);
        let runtime = RemoteHostRuntime::new(support_dir.clone());
        runtime.set_remote_project_scope(Some("device-1"), "project-b");

        let payload = runtime.remote_project_list_payload(Some("device-1"));

        assert_eq!(payload["selectedProjectId"], "project-b");
        assert_eq!(
            payload["projects"]
                .as_array()
                .expect("projects")
                .iter()
                .filter_map(|project| project.get("id").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["project-a", "project-b"],
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_project_select_starts_project_terminal_on_host() {
        let support_dir = temp_support_dir("codux-remote-project-terminal");
        write_two_project_state(&support_dir);
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));

        runtime.handle_project_select(&RemoteEnvelope {
            kind: "project.select".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            seq: None,
            payload: json!({ "projectId": "project-b" }),
        });

        let terminals = runtime.remote_terminals();
        let project_terminal = terminals
            .iter()
            .find(|terminal| terminal.get("projectId").and_then(Value::as_str) == Some("project-b"))
            .expect("project terminal");
        let session_id = project_terminal
            .get("id")
            .and_then(Value::as_str)
            .expect("session id");
        assert!(!session_id.trim().is_empty());

        let layout_key = terminal_layout_storage_key("project-b", "worktree-b");
        let layout = TerminalLayoutService::new(support_dir.clone()).load(Some(&layout_key));
        assert_eq!(layout.top_panes.len(), 1);
        assert_eq!(layout.top_panes[0].terminal_id, session_id);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn terminal_project_subscriptions_keep_devices_scoped_to_their_projects() {
        let support_dir = temp_support_dir("codux-remote-terminal-subscriptions");
        let (project_a, project_b) = write_two_project_state(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let session_a = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf a".to_string()),
                    cwd: Some(project_a.to_string_lossy().to_string()),
                    project_id: Some("project-a".to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal a");
        let session_b = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf b".to_string()),
                    cwd: Some(project_b.to_string_lossy().to_string()),
                    project_id: Some("project-b".to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal b");

        runtime.handle_terminal_subscribe(&RemoteEnvelope {
            kind: "terminal.subscribe".to_string(),
            device_id: Some("mac".to_string()),
            session_id: None,
            seq: None,
            payload: json!({ "scope": "project", "projectId": "project-a" }),
        });
        runtime.handle_terminal_subscribe(&RemoteEnvelope {
            kind: "terminal.subscribe".to_string(),
            device_id: Some("windows".to_string()),
            session_id: None,
            seq: None,
            payload: json!({ "scope": "project", "projectId": "project-b" }),
        });

        let viewers_a = runtime.terminal_output_viewers(&session_a);
        let viewers_b = runtime.terminal_output_viewers(&session_b);

        assert!(viewers_a.contains("mac"));
        assert!(!viewers_a.contains("windows"));
        assert!(viewers_b.contains("windows"));
        assert!(!viewers_b.contains("mac"));

        runtime.handle_terminal_unsubscribe(&RemoteEnvelope {
            kind: "terminal.unsubscribe".to_string(),
            device_id: Some("mac".to_string()),
            session_id: None,
            seq: None,
            payload: json!({ "scope": "project", "projectId": "project-a" }),
        });

        let viewers_a = runtime.terminal_output_viewers(&session_a);
        assert!(!viewers_a.contains("mac"));

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_plan_uses_device_project_scope_without_desktop_ui_selection() {
        let support_dir = temp_support_dir("codux-remote-scope-terminal");
        write_two_project_state(&support_dir);
        let runtime = RemoteHostRuntime::new(support_dir.clone());
        runtime.set_remote_project_scope(Some("device-1"), "project-b");
        let layout_key = terminal_layout_storage_key("project-b", "worktree-b");
        TerminalLayoutService::new(support_dir.clone())
            .save_from_gpui(
                &layout_key,
                Vec::new(),
                "terminal-b".to_string(),
                vec![TerminalPaneSummary {
                    title: "Mobile".to_string(),
                    terminal_id: "terminal-b".to_string(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save layout");

        let plan = runtime
            .remote_terminal_plan_from_envelope(
                &RemoteEnvelope {
                    kind: "terminal.buffer".to_string(),
                    device_id: Some("device-1".to_string()),
                    session_id: Some("terminal-b".to_string()),
                    seq: None,
                    payload: json!({}),
                },
                None,
            )
            .expect("terminal plan");

        assert_eq!(plan.scope.project_id, "project-b");
        assert_eq!(plan.scope.worktree_id, "worktree-b");
        assert_eq!(plan.config.project_id.as_deref(), Some("project-b"));
        assert_eq!(plan.config.terminal_id.as_deref(), Some("terminal-b"));

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_layout_is_persisted_to_project_worktree_scope() {
        let support_dir = temp_support_dir("codux-remote-layout-persist");
        write_two_project_state(&support_dir);
        let runtime = RemoteHostRuntime::new(support_dir.clone());
        let layout_key = terminal_layout_storage_key("project-b", "worktree-b");

        runtime.persist_remote_terminal_layout(&layout_key, "terminal-mobile-b", "Mobile", "split");

        let layout = TerminalLayoutService::new(support_dir.clone()).load(Some(&layout_key));
        assert_eq!(layout.active_terminal_id, "");
        assert_eq!(layout.top_panes.len(), 1);
        assert_eq!(layout.top_panes[0].terminal_id, "terminal-mobile-b");

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_output_sequence_is_session_scoped() {
        let support_dir = temp_support_dir("codux-remote-terminal-output-seq");
        let runtime = RemoteHostRuntime::new(support_dir.clone());

        assert_eq!(runtime.current_terminal_output_seq("terminal-a"), 0);
        assert_eq!(runtime.next_terminal_output_seq("terminal-a"), 1);
        assert_eq!(runtime.next_terminal_output_seq("terminal-a"), 2);
        assert_eq!(runtime.next_terminal_output_seq("terminal-b"), 1);
        assert_eq!(runtime.current_terminal_output_seq("terminal-a"), 2);

        runtime.clear_terminal_output_seq("terminal-a");

        assert_eq!(runtime.current_terminal_output_seq("terminal-a"), 0);
        assert_eq!(runtime.current_terminal_output_seq("terminal-b"), 1);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_buffer_window_limits_full_history() {
        let support_dir = temp_support_dir("codux-remote-terminal-buffer-window");
        let terminals = Arc::new(TerminalManager::new());
        let runtime = RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        );
        let session_id = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf abcdef".to_string()),
                    cwd: Some(support_dir.to_string_lossy().to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");

        let mut window = None;
        for _ in 0..20 {
            let current = runtime
                .terminal_buffer_window(&session_id, 0, 3, None, false)
                .expect("terminal buffer window");
            if current.total_characters >= 6 {
                window = Some(current);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let window = window.expect("terminal output");

        assert_eq!(window.data, "abc");
        assert_eq!(window.offset, 0);
        assert_eq!(window.total_characters, 6);
        assert!(window.truncated);

        let next = runtime
            .terminal_buffer_window(&session_id, 3, 3, None, false)
            .expect("next terminal buffer window");
        assert_eq!(next.data, "def");
        assert_eq!(next.offset, 3);
        assert_eq!(next.total_characters, 6);
        assert!(!next.truncated);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_buffer_window_tail_snapshot_returns_screen_snapshot() {
        let support_dir = temp_support_dir("codux-remote-terminal-buffer-tail-window");
        let terminals = Arc::new(TerminalManager::new());
        let runtime = RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        );
        let session_id = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf abcdef".to_string()),
                    cwd: Some(support_dir.to_string_lossy().to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");

        let mut window = None;
        for _ in 0..20 {
            let current = runtime
                .terminal_buffer_window(&session_id, 0, 3, Some("request-1".to_string()), true)
                .expect("terminal buffer window");
            if current.data.contains("abcdef") {
                window = Some(current);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let window = window.expect("terminal output");

        assert!(window.data.contains("\x1b[H\x1b[2J"));
        assert!(window.data.contains("abcdef"));
        assert_eq!(window.offset, 0);
        assert!(!window.truncated);
        assert_eq!(window.request_id.as_deref(), Some("request-1"));
        assert!(window.tail);
        assert!(window.screen_snapshot);
        assert!(!window.has_previous);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn terminal_buffer_request_does_not_resize_remote_pty() {
        let support_dir = temp_support_dir("codux-remote-terminal-buffer-readonly");
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let session_id = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf ready".to_string()),
                    cwd: Some(support_dir.to_string_lossy().to_string()),
                    cols: Some(100),
                    rows: Some(32),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");

        runtime.handle_terminal_buffer(&RemoteEnvelope {
            kind: "terminal.buffer".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: Some(session_id.clone()),
            seq: None,
            payload: json!({
                "offset": 0,
                "cols": 44,
                "rows": 12,
            }),
        });

        let info = terminals
            .list()
            .into_iter()
            .find(|terminal| terminal.id == session_id)
            .expect("terminal");
        assert_eq!(info.cols, 100);
        assert_eq!(info.rows, 32);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn terminal_viewport_resize_uses_remote_owner() {
        let support_dir = temp_support_dir("codux-remote-terminal-viewport-owner");
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let session_id = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf ready".to_string()),
                    cwd: Some(support_dir.to_string_lossy().to_string()),
                    cols: Some(100),
                    rows: Some(32),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");

        runtime.handle_terminal_viewport_claim(&RemoteEnvelope {
            kind: "terminal.viewport.claim".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: Some(session_id.clone()),
            seq: None,
            payload: json!({}),
        });
        runtime.handle_terminal_viewport_resize(&RemoteEnvelope {
            kind: "terminal.viewport.resize".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: Some(session_id.clone()),
            seq: None,
            payload: json!({
                "cols": 72,
                "rows": 18,
            }),
        });

        let state = terminals
            .viewport_state(&session_id)
            .expect("viewport state");
        assert_eq!(state.owner, "remote:device-1");
        assert_eq!(state.cols, 72);
        assert_eq!(state.rows, 18);

        let ignored = terminals
            .resize_viewport(&session_id, "remote:device-2", 120, 40)
            .expect("resize from non-owner");
        assert!(ignored.is_none());
        let state = terminals
            .viewport_state(&session_id)
            .expect("viewport state");
        assert_eq!(state.owner, "remote:device-1");
        assert_eq!(state.cols, 72);
        assert_eq!(state.rows, 18);

        let ignored = terminals
            .resize_viewport(&session_id, "desktop", 100, 32)
            .expect("resize from desktop while remote owns");
        assert!(ignored.is_none());
        let state = terminals
            .viewport_state(&session_id)
            .expect("viewport state");
        assert_eq!(state.owner, "remote:device-1");
        assert_eq!(state.cols, 72);
        assert_eq!(state.rows, 18);

        terminals
            .claim_viewport(&session_id, "desktop")
            .expect("desktop claim");
        let accepted = terminals
            .resize_viewport(&session_id, "desktop", 100, 32)
            .expect("desktop resize")
            .expect("accepted desktop resize");
        assert_eq!(accepted.owner, "desktop");
        assert_eq!(accepted.cols, 100);
        assert_eq!(accepted.rows, 32);

        let ignored = terminals
            .resize_viewport(&session_id, "remote:device-1", 72, 18)
            .expect("old remote resize after desktop claim");
        assert!(ignored.is_none());
        let state = terminals
            .viewport_state(&session_id)
            .expect("viewport state");
        assert_eq!(state.owner, "desktop");
        assert_eq!(state.cols, 100);
        assert_eq!(state.rows, 32);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn legacy_terminal_resize_claims_remote_viewport_for_compatibility() {
        let support_dir = temp_support_dir("codux-remote-terminal-legacy-resize");
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let session_id = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf ready".to_string()),
                    cwd: Some(support_dir.to_string_lossy().to_string()),
                    cols: Some(100),
                    rows: Some(32),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");

        runtime.handle_terminal_resize(&RemoteEnvelope {
            kind: "terminal.resize".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: Some(session_id.clone()),
            seq: None,
            payload: json!({
                "cols": 80,
                "rows": 24,
            }),
        });

        let state = terminals
            .viewport_state(&session_id)
            .expect("viewport state");
        assert_eq!(state.owner, "remote:device-1");
        assert_eq!(state.cols, 80);
        assert_eq!(state.rows, 24);

        fs::remove_dir_all(support_dir).ok();
    }
}
