use super::RemoteService;
use super::crypto::remote_host_name;
use super::pairing::remote_summary_show_pending_pairing;
use super::protocol::{
    REMOTE_AI_SESSION, REMOTE_AI_SESSION_RESULT, REMOTE_AI_STATE, REMOTE_AI_STATS,
    REMOTE_DEVICE_CONNECTED, REMOTE_DEVICE_DISCONNECTED, REMOTE_ERROR, REMOTE_FILE_BYTES_WRITTEN,
    REMOTE_FILE_COPIED, REMOTE_FILE_COPY, REMOTE_FILE_CREATE_DIRECTORY, REMOTE_FILE_DELETE,
    REMOTE_FILE_DELETED, REMOTE_FILE_DIRECTORY_CREATED, REMOTE_FILE_LIST, REMOTE_FILE_MOVE,
    REMOTE_FILE_MOVED, REMOTE_FILE_READ, REMOTE_FILE_RENAME, REMOTE_FILE_RENAMED,
    REMOTE_FILE_WRITE, REMOTE_FILE_WRITE_BYTES, REMOTE_FILE_WRITTEN, REMOTE_GIT_INVOKE,
    REMOTE_GIT_READ, REMOTE_GIT_STATUS, REMOTE_HOST_INFO, REMOTE_HOST_OFFLINE,
    REMOTE_PAIRING_CONFIRMED, REMOTE_PAIRING_REJECTED, REMOTE_PROJECT_ADD, REMOTE_PROJECT_EDIT,
    REMOTE_PROJECT_LIST, REMOTE_PROJECT_REMOVE, REMOTE_PROJECT_SELECT, REMOTE_PROJECT_SELECTED,
    REMOTE_PROJECT_UPDATED, REMOTE_RESOURCE_AI_STATS, REMOTE_RESOURCE_GIT_STATUS,
    REMOTE_RESOURCE_PROJECTS, REMOTE_RESOURCE_SUBSCRIBE, REMOTE_RESOURCE_TERMINALS,
    REMOTE_RESOURCE_UNSUBSCRIBE, REMOTE_RESOURCE_WORKTREES, REMOTE_SSH_LIST,
    REMOTE_SSH_LIST_RESULT, REMOTE_SSH_REMOVE, REMOTE_SSH_UPSERT, REMOTE_TERMINAL_BUFFER,
    REMOTE_TERMINAL_BUFFER_MAX_CHARS, REMOTE_TERMINAL_CLOSED, REMOTE_TERMINAL_CREATED,
    REMOTE_TERMINAL_INPUT, REMOTE_TERMINAL_INPUT_ACK, REMOTE_TERMINAL_LIST, REMOTE_TERMINAL_OUTPUT,
    REMOTE_TERMINAL_OUTPUT_ACK, REMOTE_TERMINAL_SIGNAL, REMOTE_TERMINAL_UPLOADED,
    REMOTE_TERMINAL_VIEWPORT_STATE, REMOTE_TRANSPORT_PING,
    REMOTE_TRANSPORT_PONG, REMOTE_WORKTREE_CREATE, REMOTE_WORKTREE_DELETE, REMOTE_WORKTREE_LIST,
    REMOTE_WORKTREE_MERGE, REMOTE_WORKTREE_REMOVE, REMOTE_WORKTREE_SELECT, REMOTE_WORKTREE_UPDATED,
    RemoteTerminalBufferWindow, RemoteTerminalSubscriptionTarget, RemoteTerminalSubscriptions,
    terminal_buffer_payloads, terminal_live_output_payload,
};
use super::relay::{remote_pairing_payload_url, remote_relay_url};
use super::transport::RemoteTransport;
use super::transport_factory::RemoteTransportFactory;
use super::types::{
    RemoteDeviceSettings, RemoteEnvelope, RemoteHostEvent, RemotePairingInfo,
    RemotePairingPollResult, RemoteSummary, RemoteTerminalLayoutChanged, RemoteTransportCandidate,
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
use codux_remote_transport::RemoteTransportUpload;
use codux_runtime_core::{
    ai_stats as runtime_ai_stats, file as runtime_file, git as runtime_git, host as runtime_host,
    project as runtime_project, subscription::RuntimeSubscriptionRouter,
    terminal as runtime_terminal, upload as runtime_upload, worktree as runtime_worktree,
};
use codux_runtime_live::remote_terminal_dispatch::{
    RemoteTerminalDispatch, TerminalMessage, is_terminal_kind,
};
use codux_terminal_core::{
    RemoteSequenceGuard, TerminalDriver, TerminalSequence, TerminalSessionHandle,
    runtime_scope_parts,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const REMOTE_TERMINAL_OUTPUT_BATCH_MS: u64 = 32;
const REMOTE_TERMINAL_BUFFER_BASELINE_TTL: Duration = Duration::from_secs(60);

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

struct RemoteTerminalLayoutScope {
    layout_key: String,
    project_id: String,
    layout_kind: String,
    worktree_id: String,
    layout_order: usize,
}

struct RemoteTerminalOutputBatch {
    data: String,
    buffer_length: usize,
    viewers: HashSet<String>,
}

struct RemoteTerminalBufferBaseline {
    data: String,
    start_offset: usize,
    total_characters: usize,
    output_seq: TerminalSequence,
    created_at: Instant,
}

pub struct RemoteHostRuntime {
    runtime_instance_id: String,
    support_dir: PathBuf,
    ai_history: AIHistoryIndexer,
    ai_current_sessions: Option<Arc<dyn runtime_ai_stats::RemoteAICurrentSessionProvider>>,
    terminals: Arc<TerminalManager>,
    resource_subscriptions: RuntimeSubscriptionRouter,
    // Arc so the viewport-owner resolver (registered on the TerminalManager) can
    // read the live viewer set when a remote lease expires.
    terminal_subscriptions: Arc<RemoteTerminalSubscriptions>,
    terminal_output_seq_by_session: Mutex<HashMap<String, TerminalSequence>>,
    terminal_output_batches: Mutex<HashMap<String, RemoteTerminalOutputBatch>>,
    terminal_buffer_baselines: Mutex<HashMap<String, RemoteTerminalBufferBaseline>>,
    remote_project_scope_by_device: Mutex<HashMap<String, String>>,
    terminal_event_subscriptions: Mutex<HashSet<String>>,
    transport: Mutex<Option<Arc<dyn RemoteTransport>>>,
    transport_start_lock: tokio::sync::Mutex<()>,
    active_pairing: Mutex<Option<RemotePairingInfo>>,
    pending_pairings: Mutex<HashMap<String, RemoteTransportPairingRequest>>,
    events: Mutex<VecDeque<RemoteHostEvent>>,
    snapshot: Mutex<RemoteSummary>,
    connection_generation: AtomicU64,
    // Sequence for terminal layout changes emitted by remote clients.
    remote_terminal_layout_generation: AtomicU64,
    resolved_relay: Mutex<Option<String>>,
    send_seq_by_device: Mutex<HashMap<String, i64>>,
    receive_seq_by_device: Mutex<HashMap<String, RemoteSequenceGuard>>,
    // Devices currently watching a project's `ai.stats` (project_id -> device_id
    // -> runtime session scope). A device registers by requesting `ai.stats` and
    // watches at most one project at a time. We re-push fresh stats to these
    // devices when the live AI runtime changes (so remote views tick like the
    // desktop's local view) and when a cold-on-request index finishes refreshing.
    ai_stats_watchers: Mutex<HashMap<String, HashMap<String, String>>>,
}

impl RemoteHostRuntime {
    pub fn new(support_dir: PathBuf) -> Self {
        codux_ai_history::trace::set_trace_sink(crate::runtime_trace::runtime_trace);
        let ai_history = AIHistoryIndexer::with_database_path(support_dir.join("ai-usage.sqlite3"));
        Self::new_with_ai_history(support_dir, ai_history)
    }

    pub fn new_with_ai_history(support_dir: PathBuf, ai_history: AIHistoryIndexer) -> Self {
        Self::new_with_ai_history_current_sessions_option_and_terminals(
            support_dir,
            ai_history,
            None,
            Arc::new(TerminalManager::new()),
        )
    }

    pub fn new_with_ai_history_current_sessions_and_terminals(
        support_dir: PathBuf,
        ai_history: AIHistoryIndexer,
        ai_current_sessions: Arc<dyn runtime_ai_stats::RemoteAICurrentSessionProvider>,
        terminals: Arc<TerminalManager>,
    ) -> Self {
        Self::new_with_ai_history_current_sessions_option_and_terminals(
            support_dir,
            ai_history,
            Some(ai_current_sessions),
            terminals,
        )
    }

    pub fn new_with_ai_history_and_terminals(
        support_dir: PathBuf,
        ai_history: AIHistoryIndexer,
        terminals: Arc<TerminalManager>,
    ) -> Self {
        Self::new_with_ai_history_current_sessions_option_and_terminals(
            support_dir,
            ai_history,
            None,
            terminals,
        )
    }

    fn new_with_ai_history_current_sessions_option_and_terminals(
        support_dir: PathBuf,
        ai_history: AIHistoryIndexer,
        ai_current_sessions: Option<Arc<dyn runtime_ai_stats::RemoteAICurrentSessionProvider>>,
        terminals: Arc<TerminalManager>,
    ) -> Self {
        let snapshot = RemoteService::new(support_dir.clone()).summary();
        let terminal_subscriptions = Arc::new(RemoteTerminalSubscriptions::default());
        // When a remote viewport lease expires, hand it to another phone still
        // viewing the same terminal (if any) instead of snapping back to the
        // desktop; only fall back to the desktop when no other viewer remains.
        {
            let subscriptions = Arc::clone(&terminal_subscriptions);
            terminals.set_viewport_owner_resolver(Arc::new(
                move |session_id: &str, expired_owner: &str| {
                    subscriptions
                        .viewers_for_session(session_id, None)
                        .into_iter()
                        .map(|device| terminal_viewport_remote_owner(&device))
                        .find(|owner| owner != expired_owner)
                },
            ));
        }
        Self {
            runtime_instance_id: uuid::Uuid::new_v4().to_string(),
            support_dir,
            ai_history,
            ai_current_sessions,
            terminals,
            resource_subscriptions: RuntimeSubscriptionRouter::default(),
            terminal_subscriptions,
            terminal_output_seq_by_session: Mutex::new(HashMap::new()),
            terminal_output_batches: Mutex::new(HashMap::new()),
            terminal_buffer_baselines: Mutex::new(HashMap::new()),
            remote_project_scope_by_device: Mutex::new(HashMap::new()),
            terminal_event_subscriptions: Mutex::new(HashSet::new()),
            transport: Mutex::new(None),
            transport_start_lock: tokio::sync::Mutex::new(()),
            active_pairing: Mutex::new(None),
            pending_pairings: Mutex::new(HashMap::new()),
            events: Mutex::new(VecDeque::new()),
            snapshot: Mutex::new(snapshot),
            connection_generation: AtomicU64::new(0),
            remote_terminal_layout_generation: AtomicU64::new(0),
            resolved_relay: Mutex::new(None),
            send_seq_by_device: Mutex::new(HashMap::new()),
            receive_seq_by_device: Mutex::new(HashMap::new()),
            ai_stats_watchers: Mutex::new(HashMap::new()),
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

    pub fn drain_events(&self) -> Vec<RemoteHostEvent> {
        self.events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn terminal_manager(&self) -> Arc<TerminalManager> {
        Arc::clone(&self.terminals)
    }

    /// Push the current terminal list to subscribed devices. Called when the
    /// desktop closes a terminal so connected mobile clients reconcile their
    /// view (mobile already drops sessions no longer in the list) instead of
    /// showing the closed session's stale content.
    pub fn broadcast_terminal_list_change(&self) {
        self.broadcast_terminal_list(None);
    }

    fn publish_remote_terminal_layout_changed(&self) {
        let generation = self
            .remote_terminal_layout_generation
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        self.push_event(RemoteHostEvent::TerminalLayoutChanged(
            RemoteTerminalLayoutChanged { generation },
        ));
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

    /// Tell connected clients the host is going offline before we tear down the
    /// transport, so a clean quit reflects on mobile immediately instead of
    /// waiting out the relay's disconnect grace period. Best-effort: if the
    /// message does not flush before the socket closes, the relay grace still
    /// catches it.
    fn broadcast_host_offline(&self, message: &str) {
        let device_ids =
            self.resource_subscriptions
                .devices_for(REMOTE_RESOURCE_TERMINALS, None, None);
        let payload = json!({ "message": message });
        for device_id in device_ids {
            self.send_plain(REMOTE_HOST_OFFLINE, Some(&device_id), None, payload.clone());
        }
    }

    pub fn shutdown(&self) {
        self.broadcast_host_offline("Remote Host stopped.");
        self.stop_with_message("Remote Host stopped.");
        self.resource_subscriptions.clear();
        self.terminal_subscriptions.clear();
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
        summary.message = "Connecting remote transport...".to_string();
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
        let ok = if is_terminal_stream_kind(kind) {
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

    fn prepare_transport_reconnect_after_disconnect(
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

    fn prepare_transport_for_pairing(
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

    fn handle_transport_state(
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
        summary.message = "Connecting remote transport...".to_string();
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

    async fn transport_candidates(&self) -> Vec<RemoteTransportCandidate> {
        self.transport_candidates_snapshot()
    }

    async fn start_remote_transport(self: &Arc<Self>, generation: u64) -> Result<(), String> {
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
        connected.message = "Remote transport connected.".to_string();
        connected.pairing = self.snapshot().pairing;
        self.update_snapshot(connected);
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("transport_connected kind={transport_kind}"),
        );
        Ok(())
    }

    fn handle_transport_message(self: Arc<Self>, device_id: String, data: Vec<u8>) {
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
        if !self.is_authorized_device(envelope.device_id.as_deref()) {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "drop unauthorized device={}",
                    envelope.device_id.as_deref().unwrap_or("")
                ),
            );
            self.send_device_unauthorized(envelope.device_id.as_deref());
            return;
        }
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

    fn handle_transport_upload(&self, upload: RemoteTransportUpload) -> Result<(), String> {
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
        if !self.is_authorized_device(Some(device_id)) {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!("drop unauthorized upload device={device_id}"),
            );
            self.send_device_unauthorized(Some(device_id));
            return Err("Device is not authorized.".to_string());
        }
        if upload.session_id.trim().is_empty() {
            return Err("Terminal session is required.".to_string());
        }
        if upload.bytes.is_empty() || upload.bytes.len() > 20 * 1024 * 1024 {
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

    fn handle_remote_envelope(self: &Arc<Self>, envelope: RemoteEnvelope) {
        match envelope.kind.as_str() {
            REMOTE_HOST_INFO => self.send_host_info(envelope.device_id.as_deref()),
            REMOTE_DEVICE_CONNECTED => {
                self.update_device_online(envelope.device_id.as_deref(), true);
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            REMOTE_DEVICE_DISCONNECTED => {
                self.update_device_online(envelope.device_id.as_deref(), false);
                self.clear_remote_project_scope(envelope.device_id.as_deref());
                self.remove_terminal_viewer(envelope.device_id.as_deref());
            }
            REMOTE_PROJECT_LIST => self.send_project_list(envelope.device_id.as_deref()),
            REMOTE_PROJECT_SELECT => self.handle_project_select(&envelope),
            REMOTE_RESOURCE_SUBSCRIBE => self.handle_resource_subscribe(&envelope),
            REMOTE_RESOURCE_UNSUBSCRIBE => self.handle_resource_unsubscribe(&envelope),
            REMOTE_WORKTREE_LIST => self.handle_worktree_list(&envelope),
            REMOTE_WORKTREE_SELECT => self.handle_worktree_select(&envelope),
            REMOTE_WORKTREE_CREATE => self.handle_worktree_create(&envelope),
            REMOTE_WORKTREE_MERGE => self.handle_worktree_merge(&envelope),
            REMOTE_WORKTREE_DELETE | REMOTE_WORKTREE_REMOVE => {
                self.handle_worktree_remove(&envelope)
            }
            // Every terminal-protocol message routes through the shared dispatch
            // in `codux-runtime-live`, so the desktop and the headless agent
            // enumerate the protocol surface in ONE place and cannot drift apart.
            // The desktop's host-specific arms (create/list/input/subscribe/...
            // which touch its terminal layout, output batching and baseline
            // cache) are supplied by the `DesktopTerminalCtx` impl below; the
            // arms that are identical across hosts (signal, output-ack, viewport
            // release/scroll) come from the trait's default methods.
            kind if is_terminal_kind(kind) => {
                let ctx = DesktopTerminalCtx {
                    host: Arc::clone(self),
                    envelope: &envelope,
                };
                ctx.dispatch_terminal(&TerminalMessage {
                    kind,
                    device_id: envelope.device_id.as_deref(),
                    session_id: envelope.session_id.as_deref(),
                    payload: &envelope.payload,
                });
            }
            REMOTE_FILE_LIST => {
                let path = envelope.payload.get("path").and_then(Value::as_str);
                let purpose = envelope.payload.get("purpose").and_then(Value::as_str);
                self.send(
                    REMOTE_FILE_LIST,
                    envelope.device_id.as_deref(),
                    None,
                    remote_file_list(path, purpose),
                );
            }
            REMOTE_FILE_READ => self.handle_file_read(&envelope),
            REMOTE_FILE_WRITE => self.handle_file_write(&envelope),
            REMOTE_FILE_RENAME => self.handle_file_rename(&envelope),
            REMOTE_FILE_DELETE => self.handle_file_delete(&envelope),
            REMOTE_FILE_CREATE_DIRECTORY => self.handle_file_create_directory(&envelope),
            REMOTE_FILE_COPY => self.handle_file_copy(&envelope),
            REMOTE_FILE_MOVE => self.handle_file_move(&envelope),
            REMOTE_FILE_WRITE_BYTES => self.handle_file_write_bytes(&envelope),
            REMOTE_GIT_STATUS => self.handle_git_status(&envelope),
            REMOTE_GIT_INVOKE => self.handle_git_invoke(&envelope),
            REMOTE_GIT_READ => self.handle_git_read(&envelope),
            REMOTE_PROJECT_ADD => self.handle_project_add(&envelope),
            REMOTE_PROJECT_EDIT => self.handle_project_edit(&envelope),
            REMOTE_PROJECT_REMOVE => self.handle_project_remove(&envelope),
            REMOTE_AI_STATS => self.handle_ai_stats(&envelope),
            REMOTE_AI_STATE => self.handle_ai_state(&envelope),
            REMOTE_AI_SESSION => self.handle_ai_session(&envelope),
            REMOTE_SSH_LIST => self.handle_ssh_list(&envelope),
            REMOTE_SSH_UPSERT => self.handle_ssh_upsert(&envelope),
            REMOTE_SSH_REMOVE => self.handle_ssh_remove(&envelope),
            REMOTE_TRANSPORT_PING => {
                self.send_plain(
                    REMOTE_TRANSPORT_PONG,
                    envelope.device_id.as_deref(),
                    None,
                    envelope.payload,
                );
            }
            _ => {}
        }
    }

    fn send_host_info(self: &Arc<Self>, device_id: Option<&str>) {
        let transports = self.transport_candidates_snapshot();
        self.send(
            REMOTE_HOST_INFO,
            device_id,
            None,
            runtime_host::host_info_payload(runtime_host::HostInfoPayload {
                host_id: self.snapshot().host_id,
                runtime_instance_id: self.runtime_instance_id.clone(),
                name: remote_host_name(),
                platform: std::env::consts::OS.to_string(),
                app: "Codux".to_string(),
                transports,
            }),
        );
    }

    fn handle_transport_pairing_request(&self, handshake: RemoteTransportPairingRequest) {
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
        let active_pairing = self
            .active_pairing
            .lock()
            .ok()
            .and_then(|value| value.clone());
        let Some(active_pairing) = active_pairing else {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "pairing_request reject reason=no_active_pairing pair={}",
                    handshake.pairing_id.as_deref().unwrap_or("")
                ),
            );
            return;
        };
        if handshake.pairing_id.as_deref() != Some(active_pairing.pairing_id.as_str()) {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "pairing_request reject reason=pairing_id_mismatch expected={} received={}",
                    active_pairing.pairing_id,
                    handshake.pairing_id.as_deref().unwrap_or("")
                ),
            );
            return;
        }
        if handshake.pairing_code.as_deref() != Some(active_pairing.code.as_str()) {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "pairing_request reject reason=code_mismatch pair={}",
                    active_pairing.pairing_id
                ),
            );
            return;
        }
        if handshake.pairing_secret.as_deref() != Some(active_pairing.secret.as_str()) {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!(
                    "pairing_request reject reason=secret_mismatch pair={}",
                    active_pairing.pairing_id
                ),
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
            handshake.device_id.clone(),
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
        let (transport, generation) = self.prepare_transport_for_pairing()?;
        if let Some(transport) = transport {
            transport.shutdown().await;
        }
        self.start_remote_transport(generation).await?;
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

    fn create_pairing_ticket_payload(&self, payload: Value) -> Result<String, String> {
        remote_pairing_payload_url(&payload)
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
                handshake.device_id.clone(),
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
                REMOTE_PAIRING_REJECTED,
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
            public_key: String::new(),
            created_at: now.clone(),
            last_seen: now,
            revoked_at: None,
            online: Some(false),
            platform: handshake.platform.clone().unwrap_or_default(),
        });
        raw.insert(
            "remote".to_string(),
            serde_json::to_value(&settings).map_err(|error| error.to_string())?,
        );
        self.service().save_raw_settings(&raw)?;
        if let Ok(mut active) = self.active_pairing.lock() {
            *active = None;
        }
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
                "token": "",
                "hostName": remote_host_name(),
                "platform": std::env::consts::OS,
                "transports": transports,
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
            Ok(payload) => self.send(
                REMOTE_FILE_READ,
                envelope.device_id.as_deref(),
                None,
                payload,
            ),
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
                REMOTE_FILE_WRITTEN,
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
                REMOTE_FILE_RENAMED,
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
                REMOTE_FILE_DELETED,
                envelope.device_id.as_deref(),
                None,
                json!({ "path": path }),
            ),
            Err(error) => self.send_error(envelope, &error.to_string()),
        }
    }

    fn handle_file_create_directory(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "Directory path is required.");
            return;
        };
        match runtime_file::file_make_directory(path) {
            Ok(()) => self.send(
                REMOTE_FILE_DIRECTORY_CREATED,
                envelope.device_id.as_deref(),
                None,
                json!({ "path": path }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_file_copy(&self, envelope: &RemoteEnvelope) {
        let path = envelope
            .payload
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let target = envelope
            .payload
            .get("targetDir")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match runtime_file::file_copy(path, target) {
            Ok(new_path) => self.send(
                REMOTE_FILE_COPIED,
                envelope.device_id.as_deref(),
                None,
                json!({ "path": new_path }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_file_move(&self, envelope: &RemoteEnvelope) {
        let path = envelope
            .payload
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let target = envelope
            .payload
            .get("targetDir")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let overwrite = envelope
            .payload
            .get("overwrite")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        match runtime_file::file_move(path, target, overwrite) {
            Ok(new_path) => self.send(
                REMOTE_FILE_MOVED,
                envelope.device_id.as_deref(),
                None,
                json!({ "path": new_path }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_file_write_bytes(&self, envelope: &RemoteEnvelope) {
        use base64::Engine;
        let directory = envelope
            .payload
            .get("directory")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let name = envelope
            .payload
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let bytes = envelope
            .payload
            .get("bytes")
            .and_then(Value::as_str)
            .and_then(|encoded| {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .ok()
            })
            .unwrap_or_default();
        match runtime_file::file_write_bytes(directory, name, &bytes) {
            Ok(new_path) => self.send(
                REMOTE_FILE_BYTES_WRITTEN,
                envelope.device_id.as_deref(),
                None,
                json!({ "path": new_path }),
            ),
            Err(error) => self.send_error(envelope, &error),
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
            host_device_id: None,
        }) {
            Ok(baseline) => {
                let project_id = baseline.selected_project_id.unwrap_or_default();
                self.send(
                    REMOTE_PROJECT_UPDATED,
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
                host_device_id: None,
            },
        ) {
            Ok(_) => {
                self.send(
                    REMOTE_PROJECT_UPDATED,
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
                    REMOTE_PROJECT_UPDATED,
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
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!(
                "project_select start device={} project={project_id}",
                envelope.device_id.as_deref().unwrap_or("")
            ),
        );
        let preferred_worktree_id = envelope
            .payload
            .get("worktreeId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        match self.remote_project_scope_with_worktree(project_id, preferred_worktree_id) {
            Ok(scope) => {
                self.set_remote_project_scope(envelope.device_id.as_deref(), &scope.project_id);
                if let Err(error) =
                    self.ensure_remote_project_terminal(&scope, envelope.device_id.as_deref())
                {
                    crate::runtime_trace::runtime_trace(
                        "remote",
                        &format!(
                            "project_select error device={} project={project_id} error={error}",
                            envelope.device_id.as_deref().unwrap_or("")
                        ),
                    );
                    self.send_error(envelope, &error);
                    return;
                }
                self.register_project_terminal_viewers(
                    &scope.project_id,
                    envelope.device_id.as_deref(),
                );
                self.send(
                    REMOTE_PROJECT_SELECTED,
                    envelope.device_id.as_deref(),
                    None,
                    json!({ "projectId": scope.project_id, "worktreeId": scope.worktree_id }),
                );
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!(
                        "project_select ok device={} project={}",
                        envelope.device_id.as_deref().unwrap_or(""),
                        scope.project_id
                    ),
                );
            }
            Err(error) => {
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!(
                        "project_select error device={} project={project_id} error={error}",
                        envelope.device_id.as_deref().unwrap_or("")
                    ),
                );
                self.send_error(envelope, &error)
            }
        }
    }

    fn handle_terminal_subscribe(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(device_id) = envelope.device_id.as_deref() else {
            return;
        };
        match RemoteTerminalSubscriptionTarget::from_payload(
            envelope.session_id.as_deref(),
            &envelope.payload,
        ) {
            Ok(RemoteTerminalSubscriptionTarget::Project { project_id }) => {
                self.register_project_terminal_viewers(&project_id, Some(device_id));
                if RemoteTerminalSubscriptionTarget::baseline_requested(&envelope.payload) {
                    self.send_project_terminal_baselines(&project_id, Some(device_id), envelope);
                }
            }
            Ok(RemoteTerminalSubscriptionTarget::Session { session_id }) => {
                self.register_terminal_viewer(&session_id, Some(device_id));
                self.send_terminal_viewport_state(&session_id, Some(device_id));
                if RemoteTerminalSubscriptionTarget::baseline_requested(&envelope.payload) {
                    self.send_terminal_baseline(&session_id, Some(device_id), envelope);
                }
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_terminal_unsubscribe(&self, envelope: &RemoteEnvelope) {
        let Some(device_id) = envelope.device_id.as_deref() else {
            return;
        };
        match RemoteTerminalSubscriptionTarget::from_payload(
            envelope.session_id.as_deref(),
            &envelope.payload,
        ) {
            Ok(RemoteTerminalSubscriptionTarget::Project { project_id }) => {
                self.remove_project_terminal_viewers(&project_id, Some(device_id));
            }
            Ok(RemoteTerminalSubscriptionTarget::Session { session_id }) => {
                self.remove_terminal_viewer_for_session(&session_id, Some(device_id));
            }
            Err(_) => {}
        }
    }

    fn handle_resource_subscribe(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let change = match self.resource_subscriptions.subscribe_envelope(envelope) {
            Ok(change) => change,
            Err(error) => {
                self.send_error(envelope, &error);
                return;
            }
        };
        match change.resource.as_str() {
            REMOTE_RESOURCE_PROJECTS => self.send_project_list(envelope.device_id.as_deref()),
            REMOTE_RESOURCE_TERMINALS => {
                if let Some(project_id) = change.project_id.as_deref() {
                    self.register_project_terminal_viewers(
                        project_id,
                        envelope.device_id.as_deref(),
                    );
                    if change.baseline {
                        self.send_project_terminal_baselines(
                            project_id,
                            envelope.device_id.as_deref(),
                            envelope,
                        );
                    }
                } else if let Some(session_id) = change.session_id.as_deref() {
                    self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
                    self.send_terminal_viewport_state(session_id, envelope.device_id.as_deref());
                    if change.baseline {
                        self.send_terminal_baseline(
                            session_id,
                            envelope.device_id.as_deref(),
                            envelope,
                        );
                    }
                } else {
                    self.send_terminal_list(envelope.device_id.as_deref());
                }
            }
            REMOTE_RESOURCE_WORKTREES => self.handle_worktree_list(envelope),
            REMOTE_RESOURCE_GIT_STATUS => self.handle_git_status(envelope),
            REMOTE_RESOURCE_AI_STATS => self.handle_ai_stats(envelope),
            _ => self.send_error(envelope, "Unsupported resource subscription."),
        }
    }

    fn handle_resource_unsubscribe(&self, envelope: &RemoteEnvelope) {
        let Ok(change) = self.resource_subscriptions.unsubscribe_envelope(envelope) else {
            return;
        };
        if change.resource.as_str() != REMOTE_RESOURCE_TERMINALS {
            return;
        }
        if let Some(project_id) = change.project_id.as_deref() {
            self.remove_project_terminal_viewers(project_id, Some(&change.device_id));
        }
        if let Some(session_id) = change.session_id.as_deref() {
            self.remove_terminal_viewer_for_session(session_id, Some(&change.device_id));
        }
    }

    fn handle_worktree_list(&self, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        self.send_worktree_summary(
            REMOTE_WORKTREE_LIST,
            envelope.device_id.as_deref(),
            &project_id,
            &project_path,
        );
    }

    fn handle_worktree_select(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        let Some(worktree_id) = envelope.payload.get("worktreeId").and_then(Value::as_str) else {
            self.send_error(envelope, "Worktree id is required.");
            return;
        };
        let service = WorktreeService::new(self.support_dir.clone());
        let mut summary = service.summary(Some(&project_id), Some(&project_path));
        let exists = summary
            .worktrees
            .iter()
            .any(|worktree| worktree.id == worktree_id);
        if !exists {
            self.send_error(envelope, "Worktree not found.");
            return;
        }
        summary.selected_worktree_id = Some(worktree_id.to_string());
        self.set_remote_project_scope(envelope.device_id.as_deref(), &project_id);
        if let Ok(scope) = self.remote_project_scope_for_envelope(envelope, Some(&project_id)) {
            if let Err(error) =
                self.ensure_remote_project_terminal(&scope, envelope.device_id.as_deref())
            {
                self.send_error(envelope, &error);
                return;
            }
        }
        self.send(
            REMOTE_WORKTREE_UPDATED,
            envelope.device_id.as_deref(),
            None,
            remote_worktree_summary_payload(&project_id, summary),
        );
        self.send_project_and_terminal_lists(envelope.device_id.as_deref());
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
            Ok(baseline) => {
                let git = crate::git::GitService::status(&project_path);
                self.broadcast_worktree_update(
                    REMOTE_WORKTREE_UPDATED,
                    envelope.device_id.as_deref(),
                    &project_id,
                    remote_worktree_update_payload(project_id.clone(), baseline, git),
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
            Ok(baseline) => {
                let git = crate::git::GitService::status(&project_path);
                self.broadcast_worktree_update(
                    REMOTE_WORKTREE_UPDATED,
                    envelope.device_id.as_deref(),
                    &project_id,
                    remote_worktree_update_payload(project_id.clone(), baseline, git),
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
            Ok(baseline) => {
                let git = crate::git::GitService::status(&project_path);
                self.broadcast_worktree_update(
                    REMOTE_WORKTREE_UPDATED,
                    envelope.device_id.as_deref(),
                    &project_id,
                    remote_worktree_update_payload(project_id.clone(), baseline, git),
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
        let current_session_scope_id = envelope
            .payload
            .get("worktreeId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&project.id)
            .to_string();
        let request = AIHistoryProjectRequest {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
        };
        match self.ai_history.project_state(request) {
            Ok(state) => {
                // Register the requesting device as a watcher of this project so
                // we re-push fresh stats when the live AI runtime changes (and,
                // for a cold-on-request index, once the refresh completes).
                if let Some(device_id) = envelope
                    .device_id
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                {
                    self.register_ai_stats_watcher(
                        &state.project_id,
                        device_id,
                        &current_session_scope_id,
                    );
                }
                match self.remote_ai_stats_payload(
                    project.id,
                    project.name,
                    state,
                    &current_session_scope_id,
                ) {
                    Ok(payload) => {
                        let payload_project_id = payload
                            .get("projectId")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        self.broadcast_resource_payload(
                            REMOTE_AI_STATS,
                            REMOTE_RESOURCE_AI_STATS,
                            envelope.device_id.as_deref(),
                            payload_project_id.as_deref(),
                            None,
                            payload,
                        );
                    }
                    Err(error) => self.send_error(envelope, &error),
                }
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    /// Record that `device_id` is watching `project_id`'s `ai.stats` (a device
    /// watches at most one project, so drop its entries under any other project).
    fn register_ai_stats_watcher(&self, project_id: &str, device_id: &str, scope_id: &str) {
        let Ok(mut watchers) = self.ai_stats_watchers.lock() else {
            return;
        };
        for (id, devices) in watchers.iter_mut() {
            if id != project_id {
                devices.remove(device_id);
            }
        }
        watchers.retain(|_, devices| !devices.is_empty());
        watchers
            .entry(project_id.to_string())
            .or_default()
            .insert(device_id.to_string(), scope_id.to_string());
    }

    /// Drop a disconnected device from every project's watcher set.
    fn clear_ai_stats_watcher_device(&self, device_id: &str) {
        if let Ok(mut watchers) = self.ai_stats_watchers.lock() {
            for devices in watchers.values_mut() {
                devices.remove(device_id);
            }
            watchers.retain(|_, devices| !devices.is_empty());
        }
    }

    /// Re-push fresh `ai.stats` to every watcher. Called when the live AI runtime
    /// state changes so remote views tick like the desktop's local view.
    pub fn push_ai_stats_to_watchers(&self) {
        let snapshot = match self.ai_stats_watchers.lock() {
            Ok(watchers) => watchers.clone(),
            Err(_) => return,
        };
        for project_id in snapshot.keys() {
            self.push_ai_stats_for_project(project_id, &snapshot);
        }
    }

    /// Push freshly-indexed `ai.stats` to watchers of a project once its cold
    /// index refresh completes. No-op until the state is ready.
    pub fn flush_pending_ai_stats(&self, state: &AIHistoryProjectState) {
        if state.is_loading || state.queued {
            return;
        }
        let snapshot = match self.ai_stats_watchers.lock() {
            Ok(watchers) => watchers.clone(),
            Err(_) => return,
        };
        self.push_ai_stats_for_project(&state.project_id, &snapshot);
    }

    /// Build and send `ai.stats` to each device watching `project_id`, using each
    /// device's stored runtime session scope.
    fn push_ai_stats_for_project(
        &self,
        project_id: &str,
        watchers: &HashMap<String, HashMap<String, String>>,
    ) {
        let Some(devices) = watchers.get(project_id).filter(|devices| !devices.is_empty()) else {
            return;
        };
        let project_store = ProjectStore::new(self.support_dir.clone());
        let Some(project) = project_store
            .projects_snapshot()
            .into_iter()
            .find(|project| project.id == project_id)
        else {
            return;
        };
        let request = AIHistoryProjectRequest {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
        };
        let Ok(state) = self.ai_history.project_state(request) else {
            return;
        };
        for (device_id, scope_id) in devices {
            let payload = match self.remote_ai_stats_payload(
                project.id.clone(),
                project.name.clone(),
                state.clone(),
                scope_id,
            ) {
                Ok(payload) => payload,
                Err(_) => continue,
            };
            self.send(REMOTE_AI_STATS, Some(device_id), None, payload);
        }
    }

    fn remote_ai_stats_payload(
        &self,
        project_id: String,
        project_name: String,
        state: AIHistoryProjectState,
        current_session_scope_id: &str,
    ) -> Result<Value, String> {
        let current_sessions = self
            .ai_current_sessions
            .as_ref()
            .map(|provider| provider.current_sessions(current_session_scope_id))
            .unwrap_or_default();
        runtime_ai_stats::ai_stats_payload_from_state(
            project_id,
            project_name,
            state,
            current_sessions,
        )
    }

    /// Serve the full `AIHistoryProjectState` for a desktop controller, indexed
    /// from the path the controller sends (it owns the project record).
    fn handle_ai_state(&self, envelope: &RemoteEnvelope) {
        let request = AIHistoryProjectRequest {
            id: envelope
                .payload
                .get("projectId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            name: envelope
                .payload
                .get("projectName")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            path: envelope
                .payload
                .get("projectPath")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        };
        match self.ai_history.project_state(request) {
            Ok(state) => match serde_json::to_value(state) {
                Ok(payload) => self.send(
                    REMOTE_AI_STATE,
                    envelope.device_id.as_deref(),
                    None,
                    payload,
                ),
                Err(error) => self.send_error(envelope, &error.to_string()),
            },
            Err(error) => self.send_error(envelope, &error),
        }
    }

    /// Serve `ai.session` for a remote controller. Same channel + DTO the agent
    /// uses; the host owns the AI history, so it sends the lean session list.
    fn handle_ai_session(&self, envelope: &RemoteEnvelope) {
        let payload = &envelope.payload;
        let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
        let project_path = payload
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                let project_id = payload
                    .get("projectId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let store = ProjectStore::new(self.support_dir.clone());
                store
                    .projects_snapshot()
                    .into_iter()
                    .find(|project| project.id == project_id)
                    .or_else(|| store.projects_snapshot().into_iter().next())
                    .map(|project| project.path)
            })
            .unwrap_or_default();
        let service = codux_ai_sessions::AIHistoryService::new(self.support_dir.clone());
        let result = codux_ai_sessions::session_op_result(&service, &project_path, payload);
        self.send(
            REMOTE_AI_SESSION_RESULT,
            envelope.device_id.as_deref(),
            None,
            json!({ "op": op, "result": result }),
        );
    }

    /// Serve the host's saved SSH profiles (lean, no secrets). The host owns the
    /// profiles, so it just sends its own list as the shared DTO.
    fn handle_ssh_list(&self, envelope: &RemoteEnvelope) {
        self.send_ssh_list(envelope.device_id.as_deref());
    }

    /// Reply with the saved SSH profiles as secret-free summaries.
    fn send_ssh_list(&self, device_id: Option<&str>) {
        let service =
            crate::ssh::SSHService::new(self.support_dir.clone(), std::path::PathBuf::new());
        let profiles: Vec<codux_protocol::RemoteSshProfileSummary> = service
            .summary()
            .profiles
            .into_iter()
            .map(|profile| codux_protocol::RemoteSshProfileSummary {
                id: profile.id,
                name: profile.name,
                endpoint: profile.endpoint,
                credential: profile.credential_kind,
            })
            .collect();
        self.send(
            REMOTE_SSH_LIST_RESULT,
            device_id,
            None,
            json!({ "profiles": profiles }),
        );
    }

    /// Add or update a saved SSH profile, then reply with the refreshed list.
    fn handle_ssh_upsert(&self, envelope: &RemoteEnvelope) {
        let request: crate::ssh::SSHProfileUpsertRequest =
            match serde_json::from_value(envelope.payload.clone()) {
                Ok(request) => request,
                Err(error) => {
                    self.send_error(envelope, &format!("Invalid SSH profile: {error}"));
                    return;
                }
            };
        let store = crate::ssh::SSHStore::from_support_dir(self.support_dir.clone());
        match store.upsert(request) {
            Ok(_) => self.send_ssh_list(envelope.device_id.as_deref()),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    /// Remove a saved SSH profile by id, then reply with the refreshed list.
    fn handle_ssh_remove(&self, envelope: &RemoteEnvelope) {
        let id = envelope
            .payload
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if id.trim().is_empty() {
            self.send_error(envelope, "SSH profile id is required.");
            return;
        }
        let store = crate::ssh::SSHStore::from_support_dir(self.support_dir.clone());
        match store.delete(id) {
            Ok(_) => self.send_ssh_list(envelope.device_id.as_deref()),
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
        self.broadcast_resource_payload(
            REMOTE_GIT_STATUS,
            REMOTE_RESOURCE_GIT_STATUS,
            envelope.device_id.as_deref(),
            Some(&project_id),
            None,
            remote_git_status_payload(project_id.clone(), project_path, summary),
        );
    }

    /// Generic git mutation (`git.invoke`) → GitService, then reply with
    /// refreshed status (the controller maps it back into a GitSummary).
    fn handle_git_invoke(&self, envelope: &RemoteEnvelope) {
        let project_path = envelope
            .payload
            .get("projectPath")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if project_path.trim().is_empty() {
            self.send_error(envelope, "Project path is required.");
            return;
        }
        let op = envelope
            .payload
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = envelope.payload.get("args").cloned().unwrap_or(Value::Null);
        match crate::git::wire::invoke(project_path.as_str(), op, &args) {
            Ok(_) => {
                let summary = crate::git::GitService::status(project_path.as_str());
                self.send(
                    REMOTE_GIT_STATUS,
                    envelope.device_id.as_deref(),
                    None,
                    remote_git_status_payload(String::new(), project_path, summary),
                );
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    /// Generic git read (`git.read`) → `{op, result}`.
    fn handle_git_read(&self, envelope: &RemoteEnvelope) {
        let path = envelope
            .payload
            .get("projectPath")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let op = envelope
            .payload
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = envelope.payload.get("args").cloned().unwrap_or(Value::Null);
        // `stored_state` is a full status payload (needs the project envelope),
        // so it stays host-side; every other read op shares the engine table.
        let result: Result<Value, String> = if op == "stored_state" {
            Ok(remote_git_status_payload(
                String::new(),
                path.to_string(),
                crate::git::GitService::status(path),
            ))
        } else {
            crate::git::wire::read(path, op, &args)
        };
        match result {
            Ok(result) => self.send(
                REMOTE_GIT_READ,
                envelope.device_id.as_deref(),
                None,
                json!({ "op": op, "result": result }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn handle_terminal_create(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let runtime = Arc::clone(self);
        let emit = move |event| {
            runtime.handle_terminal_event(event);
        };
        // A controller passes its stable `terminalId` so we key the session by it
        // and `attach_or_create` RE-ATTACHES to the still-running shell on a later
        // open (persistent remote terminals) instead of spawning a new one.
        let requested_terminal_id = envelope
            .payload
            .get("terminalId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let plan = match self.remote_terminal_plan_from_envelope(envelope, requested_terminal_id, false)
        {
            Ok(plan) => plan,
            Err(error) => {
                self.send_error(envelope, &error);
                return;
            }
        };
        self.set_remote_project_scope(envelope.device_id.as_deref(), &plan.scope.project_id);
        // Subscribe the creating device to this project's terminal output BEFORE
        // the shell starts. `queue_terminal_output_batch` drops output while a
        // session has no viewers, and the per-session viewer is only registered
        // after `create` + the layout persist below — a window during which the
        // freshly spawned shell prints its prompt. The desktop renders only the
        // live byte stream (it ignores the screen keyframe and the host's initial
        // buffer push), so a prompt dropped in that window is lost forever and the
        // terminal looks blank until the user types. A project-scoped subscription
        // added up front covers the new session the instant it first emits.
        if let Some(device_id) = envelope
            .device_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
        {
            // Subscribe BEFORE the shell starts so its first prompt isn't dropped
            // (`queue_terminal_output_batch` drops output with no viewers). The
            // terminal is stored with `project_id = scope.worktree_id` (see
            // `remote_terminal_pty_config`), and `terminal_output_viewers` resolves
            // viewers by THAT id — so subscribing only under `scope.project_id`
            // misses the window when project_id != worktree_id. Cover both.
            for id in [
                plan.scope.project_id.as_str(),
                plan.scope.worktree_id.as_str(),
            ] {
                if !id.trim().is_empty() {
                    self.terminal_subscriptions
                        .add_project_subscriber(id, device_id);
                }
            }
        }
        // Detect re-attach BEFORE create (which reuses the session by terminal_id).
        // Only a re-attach needs the seed buffer: a freshly spawned shell prints
        // its own prompt as live output, so sending the buffer too could duplicate
        // it — whereas a re-attached (idle) shell emits nothing, so its screen has
        // to be replayed from the buffer or the pane stays blank.
        let reattaching = plan
            .config
            .terminal_id
            .as_deref()
            .map(|id| self.terminals.snapshot(id).is_ok())
            .unwrap_or(false);
        match self.terminals.create(plan.config, emit) {
            Ok(session_id) => {
                self.persist_remote_terminal_layout(
                    &plan.scope.layout_key,
                    &session_id,
                    &plan.title,
                    &plan.layout_kind,
                );
                self.publish_remote_terminal_layout_changed();
                self.mark_terminal_event_subscription(&session_id);
                self.register_terminal_viewer(&session_id, envelope.device_id.as_deref());
                self.send_terminal_data(
                    REMOTE_TERMINAL_CREATED,
                    envelope.device_id.as_deref(),
                    Some(&session_id),
                    self.remote_terminal_payload(&session_id)
                        .unwrap_or_else(|| json!({ "id": session_id })),
                );
                self.send_terminal_list(envelope.device_id.as_deref());
                self.send_terminal_viewport_state(&session_id, envelope.device_id.as_deref());
                if reattaching {
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
        self.terminals
            .touch_viewport_lease(session_id, &self.remote_viewport_owner(envelope));
        if let Some(input_id) = envelope.payload.get("inputId").and_then(Value::as_str) {
            self.send_terminal_data(
                REMOTE_TERMINAL_INPUT_ACK,
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
        let owner = self.remote_viewport_owner(envelope);
        let _ = self.terminals.claim_viewport(session_id, &owner);
        self.resize_terminal_viewport_from_envelope(session_id, envelope, cols, rows);
    }

    fn resize_terminal_viewport_from_envelope(
        self: &Arc<Self>,
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
            Ok(Some(state)) => {
                self.send_terminal_viewport_state_payload(
                    session_id,
                    envelope.device_id.as_deref(),
                    &state,
                );
            }
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
        if !self.remove_remote_terminal_from_layout(session_id) {
            self.send_terminal_list(envelope.device_id.as_deref());
            return;
        }
        match self.terminals.kill(session_id) {
            Ok(()) => {
                self.clear_terminal_output_seq(session_id);
                self.publish_remote_terminal_layout_changed();
                self.send_terminal_data(
                    REMOTE_TERMINAL_CLOSED,
                    envelope.device_id.as_deref(),
                    Some(session_id),
                    json!({ "id": session_id }),
                );
                self.send_terminal_list(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error.to_string()),
        }
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
            REMOTE_TERMINAL_UPLOADED,
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

    fn send_project_and_terminal_lists(&self, device_id: Option<&str>) {
        self.broadcast_project_list(device_id);
        self.broadcast_terminal_list(device_id);
    }

    fn send_project_list(&self, device_id: Option<&str>) {
        let payload = self.remote_project_list_payload(device_id);
        self.send(REMOTE_PROJECT_LIST, device_id, None, payload);
    }

    fn remote_project_list_payload(&self, device_id: Option<&str>) -> Value {
        let store = ProjectStore::new(self.support_dir.clone());
        let baseline = store.list_snapshot();
        // Only advertise LOCAL projects to a controller. A project backed by
        // ANOTHER host (host_device_id set) lives on that host; this host can't
        // serve its terminal — it would spawn a wrong local shell because the
        // remote path doesn't exist here. Chained host→host forwarding isn't
        // supported, so hiding them keeps the controller from opening a project
        // it can never actually use. (host_device_id is on the full record, not
        // the list summary, so derive the local set from `projects_snapshot`.)
        let local_ids: HashSet<String> = store
            .projects_snapshot()
            .into_iter()
            .filter(|project| {
                project
                    .host_device_id
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
            })
            .map(|project| project.id)
            .collect();
        let selected_project_id = self
            .remote_project_scope_id(device_id)
            .filter(|id| local_ids.contains(id))
            .or_else(|| baseline.selected_project_id.filter(|id| local_ids.contains(id)))
            .or_else(|| {
                baseline
                    .projects
                    .iter()
                    .find(|project| local_ids.contains(&project.id))
                    .map(|project| project.id.clone())
            });
        runtime_project::project_list_payload_with_worktrees(
            baseline
                .projects
                .into_iter()
                .filter(|project| local_ids.contains(&project.id))
                .map(|project| runtime_project::ProjectListItem {
                    id: project.id,
                    name: project.name,
                    path: project.path,
                }),
            selected_project_id,
            None,
            store
                .snapshot()
                .worktrees
                .into_iter()
                .filter(|worktree| local_ids.contains(&worktree.project_id))
                .map(|worktree| runtime_project::ProjectWorktreeListItem {
                    id: worktree.id,
                    project_id: worktree.project_id,
                    name: worktree.name,
                    branch: worktree.branch,
                    path: worktree.path,
                    status: worktree.status,
                    is_default: worktree.is_default,
                    exists: true,
                }),
        )
    }

    fn send_terminal_list(&self, device_id: Option<&str>) {
        let terminals = self.remote_terminals();
        self.send(
            REMOTE_TERMINAL_LIST,
            device_id,
            None,
            json!({ "terminals": terminals }),
        );
    }

    fn broadcast_project_list(&self, source_device_id: Option<&str>) {
        let mut device_ids =
            self.resource_subscriptions
                .devices_for(REMOTE_RESOURCE_PROJECTS, None, None);
        if let Some(source_device_id) = source_device_id.filter(|value| !value.trim().is_empty()) {
            device_ids.insert(source_device_id.to_string());
        }
        if device_ids.is_empty() {
            self.send_project_list(source_device_id);
            return;
        }
        for device_id in device_ids {
            let payload = self.remote_project_list_payload(Some(&device_id));
            self.send(REMOTE_PROJECT_LIST, Some(&device_id), None, payload);
        }
    }

    fn broadcast_terminal_list(&self, source_device_id: Option<&str>) {
        let mut device_ids =
            self.resource_subscriptions
                .devices_for(REMOTE_RESOURCE_TERMINALS, None, None);
        if let Some(source_device_id) = source_device_id.filter(|value| !value.trim().is_empty()) {
            device_ids.insert(source_device_id.to_string());
        }
        if device_ids.is_empty() {
            self.send_terminal_list(source_device_id);
            return;
        }
        let payload = json!({ "terminals": self.remote_terminals() });
        for device_id in device_ids {
            self.send(
                REMOTE_TERMINAL_LIST,
                Some(&device_id),
                None,
                payload.clone(),
            );
        }
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
        self.send(
            kind,
            device_id,
            None,
            remote_worktree_summary_payload(project_id, summary),
        );
    }

    /// Push the current worktree list to subscribed devices after a
    /// desktop-initiated worktree mutation (create/remove/merge), so mobile
    /// reconciles its view instead of showing a stale list. A no-op when no
    /// device is subscribed. Mirrors the `worktree.list` request reply.
    pub fn broadcast_worktree_list_change(&self, project_id: &str, project_path: &str) {
        if project_id.trim().is_empty() {
            return;
        }
        let summary = WorktreeService::new(self.support_dir.clone())
            .summary(Some(project_id), Some(project_path));
        self.broadcast_worktree_update(
            REMOTE_WORKTREE_LIST,
            None,
            project_id,
            remote_worktree_summary_payload(project_id, summary),
        );
    }

    fn broadcast_worktree_update(
        &self,
        kind: &str,
        source_device_id: Option<&str>,
        project_id: &str,
        payload: Value,
    ) {
        self.broadcast_resource_payload(
            kind,
            REMOTE_RESOURCE_WORKTREES,
            source_device_id,
            Some(project_id),
            None,
            payload,
        );
    }

    fn broadcast_resource_payload(
        &self,
        kind: &str,
        resource: &str,
        source_device_id: Option<&str>,
        project_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) {
        let mut device_ids = self
            .resource_subscriptions
            .devices_for(resource, project_id, session_id);
        if let Some(source_device_id) = source_device_id.filter(|value| !value.trim().is_empty()) {
            device_ids.insert(source_device_id.to_string());
        }
        if device_ids.is_empty() {
            self.send(kind, source_device_id, session_id, payload);
            return;
        }
        for device_id in device_ids {
            self.send(kind, Some(&device_id), session_id, payload.clone());
        }
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

    fn send_device_unauthorized(&self, device_id: Option<&str>) -> bool {
        self.send_plain(
            REMOTE_ERROR,
            device_id,
            None,
            json!({
                "code": "device_unauthorized",
                "message": "This mobile device is no longer authorized. Please pair it again."
            }),
        )
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

    /// Fan-out helper for terminal output: sends a frame whose payload was
    /// already serialized once (see [`RemoteService::outgoing_transport_text_raw`]),
    /// so broadcasting one batch to N subscribers does not clone + re-serialize
    /// the payload per device. Only the small per-device `seq` wrapper differs.
    fn send_terminal_output_raw(
        &self,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: &serde_json::value::RawValue,
    ) -> bool {
        let text = {
            let Ok(mut send_seq) = self.send_seq_by_device.lock() else {
                return false;
            };
            self.service().outgoing_transport_text_raw(
                REMOTE_TERMINAL_OUTPUT,
                device_id,
                session_id,
                payload,
                &mut send_seq,
            )
        };
        let Some(text) = text else {
            return false;
        };
        let transport = self.transport.lock().ok().and_then(|value| value.clone());
        let Some(transport) = transport else {
            return false;
        };
        transport.send_terminal(text.into_bytes(), device_id)
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
            self.push_event(RemoteHostEvent::Summary(current.clone()));
        }
    }

    fn push_event(&self, event: RemoteHostEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push_back(event);
            while events.len() > 128 {
                events.pop_front();
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
                let output_seq = window
                    .output_seq
                    .unwrap_or_else(|| self.current_terminal_output_seq(session_id));
                for payload in terminal_buffer_payloads(&window, output_seq, chunk_chars) {
                    self.send_terminal_data(
                        REMOTE_TERMINAL_OUTPUT,
                        device_id,
                        Some(session_id),
                        payload,
                    );
                }
            }
            Err(error) => {
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!("terminal_buffer baseline_failed session={session_id} error={error}"),
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
            REMOTE_TERMINAL_VIEWPORT_STATE,
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
            // ROOT FIX for the stale remote screen (e.g. Claude's classic-mode
            // input box frozen at its old row while "working").
            //
            // The per-frame sequence the viewer uses to order/dedup output is the
            // host's flush counter (one ++ per forwarded batch), but the screen
            // and history advance continuously as the PTY is processed. So at any
            // instant the screen/history already reflect a pending un-flushed
            // batch, while the sequence still reads the previous flushed frame —
            // a baseline built from "current screen + current sequence" is
            // therefore inconsistent (its screen is ahead of its label). The
            // viewer then either drops the in-between frames (stale, what was
            // reported) or re-appends them (double).
            //
            // Flush the pending batch FIRST so its data is assigned a sequence
            // and forwarded; then the baseline's history, keyframe AND sequence
            // all reflect that same flushed state, captured together under the
            // sequence lock so a concurrent flush can't advance it between the
            // reads. The viewer dedupes the just-flushed frame instead.
            self.flush_terminal_output_batch(session_id);
            let seq_guard = self.terminal_output_seq_by_session.lock();
            let output_seq = seq_guard
                .as_ref()
                .ok()
                .and_then(|sequences| sequences.get(session_id).copied())
                .unwrap_or(0);
            let (data, start_offset) = self.terminals.snapshot_tail(session_id, max_chars)?;
            let total_characters = self
                .terminals
                .buffer_characters(session_id)
                .unwrap_or_else(|_| start_offset + data.chars().count());
            let screen_data = self
                .terminals
                .screen_snapshot(session_id)
                .ok()
                .map(|snapshot| snapshot.data)
                .filter(|data| !data.is_empty());
            drop(seq_guard);
            return Ok(RemoteTerminalBufferWindow {
                data,
                screen_data,
                offset: start_offset,
                total_characters,
                truncated: false,
                output_seq: Some(output_seq),
                request_id,
                tail: true,
                has_previous: start_offset > 0,
            });
        }

        let request_id_for_window = request_id.clone();
        let frozen = request_id
            .as_deref()
            .and_then(|request_id| {
                self.terminal_buffer_baseline(session_id, request_id, offset, max_chars)
                    .transpose()
            })
            .transpose()?;
        let (data, start_offset, total_characters, output_seq) = match frozen {
            Some(baseline) => (
                baseline.data,
                baseline.start_offset,
                baseline.total_characters,
                Some(baseline.output_seq),
            ),
            None => {
                let data = self.terminals.snapshot(session_id)?;
                let total_characters = data.chars().count();
                (data, 0, total_characters, None)
            }
        };
        let clamped = offset.max(start_offset).min(total_characters);
        let chunk = data
            .chars()
            .skip(clamped.saturating_sub(start_offset))
            .take(max_chars)
            .collect::<String>();
        let truncated = clamped + chunk.chars().count() < total_characters;
        if !truncated {
            if let Some(request_id) = request_id_for_window.as_deref() {
                self.remove_terminal_buffer_baseline(session_id, request_id);
            }
        }
        Ok(RemoteTerminalBufferWindow {
            data: chunk,
            screen_data: None,
            offset: clamped,
            total_characters,
            truncated,
            output_seq,
            request_id: request_id_for_window,
            tail: false,
            has_previous: clamped > 0,
        })
    }

    fn terminal_buffer_baseline(
        &self,
        session_id: &str,
        request_id: &str,
        offset: usize,
        max_chars: usize,
    ) -> Result<Option<RemoteTerminalBufferBaseline>, anyhow::Error> {
        let key = terminal_buffer_baseline_key(session_id, request_id);
        let now = Instant::now();
        if let Ok(mut baselines) = self.terminal_buffer_baselines.lock() {
            baselines.retain(|_, baseline| {
                now.duration_since(baseline.created_at) <= REMOTE_TERMINAL_BUFFER_BASELINE_TTL
            });
            if let Some(baseline) = baselines.get(&key) {
                return Ok(Some(RemoteTerminalBufferBaseline {
                    data: baseline.data.clone(),
                    start_offset: baseline.start_offset,
                    total_characters: baseline.total_characters,
                    output_seq: baseline.output_seq,
                    created_at: baseline.created_at,
                }));
            }
        }
        if offset != 0 {
            return Ok(None);
        }

        let data = self.terminals.snapshot(session_id)?;
        let total_characters = data.chars().count();
        let baseline = RemoteTerminalBufferBaseline {
            data,
            start_offset: 0,
            total_characters,
            output_seq: self.current_terminal_output_seq(session_id),
            created_at: now,
        };
        let returned = RemoteTerminalBufferBaseline {
            data: baseline.data.clone(),
            start_offset: baseline.start_offset,
            total_characters: baseline.total_characters,
            output_seq: baseline.output_seq,
            created_at: baseline.created_at,
        };
        if max_chars < total_characters {
            if let Ok(mut baselines) = self.terminal_buffer_baselines.lock() {
                baselines.insert(key, baseline);
            }
        }
        Ok(Some(returned))
    }

    fn remove_terminal_buffer_baseline(&self, session_id: &str, request_id: &str) {
        if let Ok(mut baselines) = self.terminal_buffer_baselines.lock() {
            baselines.remove(&terminal_buffer_baseline_key(session_id, request_id));
        }
    }

    fn remote_terminal_payload(&self, session_id: &str) -> Option<Value> {
        self.remote_terminals()
            .into_iter()
            .find(|value| value.get("id").and_then(Value::as_str) == Some(session_id))
    }

    fn remote_terminals(&self) -> Vec<Value> {
        let baseline = ProjectStore::new(self.support_dir.clone()).snapshot();
        let mut workspace_scopes = HashMap::new();
        for project in &baseline.projects {
            workspace_scopes.insert(project.id.clone(), (project.id.clone(), project.id.clone()));
        }
        for worktree in &baseline.worktrees {
            workspace_scopes.insert(
                worktree.id.clone(),
                (worktree.project_id.clone(), worktree.id.clone()),
            );
        }
        let scopes = self.remote_terminal_layout_scopes();
        let mut terminals = self
            .terminals
            .list()
            .into_iter()
            .map(|terminal| {
                let fallback_worktree_id = terminal.project_id.clone();
                let workspace_scope = workspace_scopes.get(&terminal.project_id);
                let layout_scope = scopes.get(&terminal.id);
                let layout_kind = layout_scope
                    .map(|scope| scope.layout_kind.as_str())
                    .unwrap_or("split");
                let project_id = layout_scope
                    .map(|scope| scope.project_id.as_str())
                    .or_else(|| workspace_scope.map(|(project_id, _)| project_id.as_str()));
                let worktree_id = layout_scope
                    .map(|scope| scope.worktree_id.as_str())
                    .or_else(|| workspace_scope.map(|(_, worktree_id)| worktree_id.as_str()))
                    .or_else(|| {
                        (!fallback_worktree_id.trim().is_empty())
                            .then_some(fallback_worktree_id.as_str())
                    });
                let layout_order = layout_scope.map(|scope| scope.layout_order);
                let mut payload = remote_terminal_snapshot_payload(
                    terminal,
                    layout_kind,
                    worktree_id,
                    layout_order,
                );
                if let Some(project_id) = project_id.filter(|value| !value.trim().is_empty()) {
                    payload["projectId"] = json!(project_id);
                }
                payload
            })
            .collect::<Vec<_>>();
        terminals.sort_by_key(remote_terminal_order_key);
        terminals
    }

    fn remote_terminal_layout_scopes(&self) -> HashMap<String, RemoteTerminalLayoutScope> {
        let project_store = ProjectStore::new(self.support_dir.clone());
        let baseline = project_store.snapshot();
        let mut keys = Vec::new();
        let mut seen = HashSet::new();
        for project in &baseline.projects {
            let default_key = terminal_layout_storage_key(&project.id, &project.id);
            if seen.insert(default_key.clone()) {
                keys.push(default_key);
            }
            for worktree in baseline
                .worktrees
                .iter()
                .filter(|worktree| worktree.project_id == project.id)
            {
                let worktree_key = terminal_layout_storage_key(&project.id, &worktree.id);
                if seen.insert(worktree_key.clone()) {
                    keys.push(worktree_key);
                }
            }
        }
        let layouts = TerminalLayoutService::new(self.support_dir.clone())
            .load_many(keys.iter().map(String::as_str));
        let mut result = HashMap::new();
        for layout_key in keys {
            let Some(layout) = layouts.get(&layout_key) else {
                continue;
            };
            let Some((project_id, worktree_id)) = runtime_scope_parts(&layout_key) else {
                continue;
            };
            let project_id = project_id.to_string();
            let worktree_id = worktree_id.to_string();
            let mut layout_order = 0;
            for pane in &layout.top_panes {
                result.insert(
                    pane.terminal_id.clone(),
                    RemoteTerminalLayoutScope {
                        layout_key: layout_key.clone(),
                        project_id: project_id.clone(),
                        layout_kind: "split".to_string(),
                        worktree_id: worktree_id.clone(),
                        layout_order,
                    },
                );
                layout_order += 1;
            }
            for tab in &layout.tabs {
                result.insert(
                    tab.terminal_id.clone(),
                    RemoteTerminalLayoutScope {
                        layout_key: layout_key.clone(),
                        project_id: project_id.clone(),
                        layout_kind: "tab".to_string(),
                        worktree_id: worktree_id.clone(),
                        layout_order,
                    },
                );
                layout_order += 1;
            }
        }
        result
    }

    fn remove_remote_terminal_from_layout(&self, terminal_id: &str) -> bool {
        let scopes = self.remote_terminal_layout_scopes();
        let Some(scope) = scopes.get(terminal_id) else {
            return true;
        };
        let service = TerminalLayoutService::new(self.support_dir.clone());
        let mut layout = service.load(Some(&scope.layout_key));
        let before_top = layout.top_panes.len();
        let before_tabs = layout.tabs.len();
        let before_total = before_top + before_tabs;
        if before_total <= 1 {
            return false;
        }
        layout
            .top_panes
            .retain(|pane| pane.terminal_id != terminal_id);
        layout.tabs.retain(|tab| tab.terminal_id != terminal_id);
        let after_total = layout.top_panes.len() + layout.tabs.len();
        if after_total == before_total || after_total == 0 {
            return false;
        }
        let _ = service.save_summary(&scope.layout_key, layout);
        true
    }

    fn remote_terminal_plan_from_envelope(
        &self,
        envelope: &RemoteEnvelope,
        terminal_id: Option<&str>,
        reuse_saved_terminal: bool,
    ) -> Result<RemoteTerminalPlan, String> {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let cwd = envelope
            .payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty());
        // A controller that added this project by browsing a path holds its OWN
        // project id, which this host's project store doesn't have — each side
        // keeps its own projects; only the host *code* is shared, not the data.
        // Git/worktree route to the host by path and work regardless; the terminal
        // scope normally resolves by id, so when the project isn't registered here
        // fall back to a scope synthesized from the cwd the controller sent (the
        // host path) — i.e. just open a terminal in that directory.
        let scope = match self.remote_project_scope_for_envelope(envelope, project_id) {
            Ok(scope) => scope,
            Err(error) => {
                let cwd = cwd.clone().ok_or(error)?;
                self.remote_terminal_scope_from_path(envelope, project_id, &cwd)
            }
        };
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
        let terminal_id = terminal_id.map(str::to_string).or_else(|| {
            if reuse_saved_terminal {
                self.saved_remote_terminal_id(&scope.layout_key)
            } else {
                None
            }
        });
        let cols = envelope
            .payload
            .get("cols")
            .and_then(Value::as_u64)
            .map(|value| value as u16);
        let rows = envelope
            .payload
            .get("rows")
            .and_then(Value::as_u64)
            .map(|value| value as u16);
        let config =
            remote_terminal_pty_config(&scope, terminal_id, &title, command, cwd, cols, rows);
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
        let plan = self.remote_terminal_plan_from_envelope(envelope, Some(session_id), true)?;
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
        self.publish_remote_terminal_layout_changed();
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
                    && terminal.get("worktreeId").and_then(Value::as_str)
                        == Some(scope.worktree_id.as_str())
            })
            .and_then(|terminal| {
                terminal
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.trim().is_empty())
                    .map(str::to_string)
            });
        if let Some(session_id) = existing {
            if self.remote_terminal_session_matches_scope(&session_id, scope) {
                self.ensure_terminal_event_subscription(&session_id);
                return Ok(session_id);
            }
        }

        let terminal_id = self
            .saved_remote_terminal_id(&scope.layout_key)
            .or_else(|| Some(remote_terminal_id_for_scope(scope)));
        let title = "Terminal".to_string();
        let config =
            remote_terminal_pty_config(scope, terminal_id.clone(), &title, None, None, None, None);
        let runtime = Arc::clone(self);
        let emit = move |event| {
            runtime.handle_terminal_event(event);
        };
        let session_id = self
            .terminals
            .create(config, emit)
            .map_err(|error| error.to_string())?;
        self.persist_remote_terminal_layout(&scope.layout_key, &session_id, &title, "split");
        self.publish_remote_terminal_layout_changed();
        self.mark_terminal_event_subscription(&session_id);
        self.register_terminal_viewer(&session_id, device_id);
        Ok(session_id)
    }

    fn remote_terminal_session_matches_scope(
        &self,
        session_id: &str,
        scope: &RemoteProjectScope,
    ) -> bool {
        self.terminals
            .session(session_id)
            .ok()
            .map(|session| {
                let info = session.info();
                info.project_id == scope.worktree_id
                    && normalize_remote_path(&info.cwd)
                        == normalize_remote_path(&scope.project_path)
            })
            .unwrap_or(false)
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

    fn remote_project_scope_with_worktree(
        &self,
        project_id: &str,
        preferred_worktree_id: Option<&str>,
    ) -> Result<RemoteProjectScope, String> {
        let baseline = ProjectStore::new(self.support_dir.clone()).snapshot();
        let project = baseline
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        let preferred_worktree_id = preferred_worktree_id
            .filter(|worktree_id| {
                worktree_id.trim() == project.id
                    || baseline.worktrees.iter().any(|worktree| {
                        worktree.project_id == project.id && worktree.id == worktree_id.trim()
                    })
            })
            .map(str::to_string);
        let worktree_id = preferred_worktree_id
            .or_else(|| {
                baseline
                    .selected_worktree_id_by_project
                    .get(&project.id)
                    .cloned()
                    .filter(|worktree_id| {
                        worktree_id == &project.id
                            || baseline.worktrees.iter().any(|worktree| {
                                worktree.project_id == project.id && &worktree.id == worktree_id
                            })
                    })
            })
            .unwrap_or_else(|| project.id.clone());
        let worktree_path = baseline
            .worktrees
            .iter()
            .find(|worktree| worktree.project_id == project.id && worktree.id == worktree_id)
            .map(|worktree| worktree.path.clone())
            .filter(|path| !path.trim().is_empty())
            .unwrap_or_else(|| project.path.clone());
        Ok(RemoteProjectScope {
            project_id: project.id.clone(),
            project_name: project.name.clone(),
            project_path: worktree_path,
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
        let worktree_id = envelope
            .payload
            .get("worktreeId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        self.remote_project_scope_with_worktree(&scoped_project_id, worktree_id.as_deref())
    }

    /// Build a terminal scope from a host path for a project this host doesn't
    /// have registered (the controller added it by browsing, so it holds an id
    /// we don't know). Keeps the controller's project id for stable layout
    /// keying, and uses the path as the worktree/cwd — the host runs the shell
    /// there just as it would for a local project at that path.
    fn remote_terminal_scope_from_path(
        &self,
        envelope: &RemoteEnvelope,
        project_id: Option<&str>,
        path: &str,
    ) -> RemoteProjectScope {
        let project_id = project_id
            .map(str::to_string)
            .or_else(|| self.remote_project_scope_id(envelope.device_id.as_deref()))
            .unwrap_or_else(|| path.to_string());
        let worktree_id = envelope
            .payload
            .get("worktreeId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| project_id.clone());
        RemoteProjectScope {
            project_id: project_id.clone(),
            project_name: default_project_name(path),
            project_path: path.to_string(),
            worktree_id: worktree_id.clone(),
            layout_key: terminal_layout_storage_key(&project_id, &worktree_id),
        }
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

    fn next_terminal_output_seq(&self, session_id: &str) -> TerminalSequence {
        self.terminal_output_seq_by_session
            .lock()
            .map(|mut sequences| {
                let next = sequences.get(session_id).copied().unwrap_or(0) + 1;
                sequences.insert(session_id.to_string(), next);
                next
            })
            .unwrap_or(0)
    }

    fn current_terminal_output_seq(&self, session_id: &str) -> TerminalSequence {
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

    fn send_project_terminal_baselines(
        self: &Arc<Self>,
        project_id: &str,
        device_id: Option<&str>,
        envelope: &RemoteEnvelope,
    ) {
        let sessions = self
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
        for session_id in sessions {
            self.send_terminal_baseline(&session_id, device_id, envelope);
        }
    }

    fn send_terminal_baseline(
        self: &Arc<Self>,
        session_id: &str,
        device_id: Option<&str>,
        envelope: &RemoteEnvelope,
    ) {
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
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("subscribe-{}-{session_id}", uuid::Uuid::new_v4()));
        self.send_terminal_buffer(
            session_id,
            device_id,
            0,
            max_chars,
            chunk_chars,
            Some(request_id),
            true,
        );
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
        self.resource_subscriptions.remove_device(device_id);
        self.terminal_subscriptions.remove_device(device_id);
        self.clear_ai_stats_watcher_device(device_id);
    }

    fn release_all_remote_viewports(&self) {
        for terminal in self.terminals.list() {
            let Ok(state) = self.terminals.viewport_state(&terminal.id) else {
                continue;
            };
            if state.owner == crate::terminal_pty::terminal_viewport_local_owner() {
                continue;
            }
            let _ = self.terminals.release_viewport(&terminal.id, &state.owner);
        }
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
                    REMOTE_TERMINAL_CLOSED,
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
                    REMOTE_TERMINAL_VIEWPORT_STATE,
                    None,
                    Some(&session_id),
                    json!({
                        "owner": owner,
                        "cols": cols,
                        "rows": rows,
                        "generation": generation,
                    }),
                );
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
        let payload = terminal_live_output_payload(batch.data, batch.buffer_length, output_seq);
        // Serialize the payload once and fan it out raw, so N subscribers of the
        // same terminal don't each clone + re-serialize the whole batch. Falls
        // back to the per-device path if the payload can't be pre-serialized.
        match serde_json::value::to_raw_value(&payload) {
            Ok(payload_raw) => {
                for device_id in batch.viewers {
                    self.send_terminal_output_raw(
                        Some(&device_id),
                        Some(session_id),
                        &payload_raw,
                    );
                }
            }
            Err(_) => {
                for device_id in batch.viewers {
                    self.send_terminal_data(
                        REMOTE_TERMINAL_OUTPUT,
                        Some(&device_id),
                        Some(session_id),
                        payload.clone(),
                    );
                }
            }
        }
    }
}

/// Adapts the desktop host to the shared remote-terminal router
/// ([`RemoteTerminalDispatch`]). It holds the host (so the create arm can clone
/// an `Arc` for its output-event closure) and the inbound envelope (so each
/// host-specific arm keeps reading its existing `RemoteEnvelope` fields,
/// unchanged from before the router was introduced). The arms that are
/// identical across hosts -- signal, output-ack, viewport release/scroll -- are
/// served by the trait's default methods against the two primitives below.
struct DesktopTerminalCtx<'a> {
    host: Arc<RemoteHostRuntime>,
    envelope: &'a RemoteEnvelope,
}

impl RemoteTerminalDispatch for DesktopTerminalCtx<'_> {
    fn terminal_manager(&self) -> &TerminalManager {
        self.host.terminals.as_ref()
    }

    fn reply_terminal(
        &self,
        device_id: Option<&str>,
        session_id: Option<&str>,
        kind: &str,
        payload: Value,
    ) {
        self.host
            .send_terminal_data(kind, device_id, session_id, payload);
    }

    fn handle_terminal_list_msg(&self, _msg: &TerminalMessage) {
        self.host
            .send_terminal_list(self.envelope.device_id.as_deref());
    }

    fn handle_terminal_subscribe_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_subscribe(self.envelope);
    }

    fn handle_terminal_unsubscribe_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_unsubscribe(self.envelope);
    }

    fn handle_terminal_create_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_create(self.envelope);
    }

    fn handle_terminal_buffer_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_buffer(self.envelope);
    }

    fn handle_terminal_input_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_input(self.envelope);
    }

    fn handle_terminal_resize_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_resize(self.envelope);
    }

    fn handle_terminal_close_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_close(self.envelope);
    }

    fn handle_terminal_viewport_claim_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_viewport_claim(self.envelope);
    }

    fn handle_terminal_viewport_resize_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_viewport_resize(self.envelope);
    }
}

fn default_project_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Project")
        .to_string()
}

pub(crate) fn remote_file_list(path: Option<&str>, purpose: Option<&str>) -> Value {
    runtime_file::file_list_payload(path, purpose)
}

pub(crate) fn remote_file_read(path: &str) -> Result<Value, String> {
    runtime_file::file_read_payload(path)
}

pub(crate) fn remote_file_write(path: &str, content: &str) -> Result<(), String> {
    runtime_file::file_write(path, content)
}

pub(crate) fn remote_file_rename(path: &str, new_path: &str) -> Result<(), String> {
    runtime_file::file_rename(path, new_path)
}

fn remote_pairing_code() -> String {
    let value = uuid::Uuid::new_v4().as_u128() % 1_000_000;
    format!("{value:06}")
}

pub(crate) fn remote_terminal_upload_directory(session_id: &str) -> PathBuf {
    runtime_upload::terminal_upload_directory(session_id)
}

pub(crate) fn sanitized_remote_upload_name(value: &str) -> String {
    runtime_upload::sanitized_upload_name(value)
}

pub(crate) fn terminal_upload_path_input(path: &Path) -> String {
    runtime_upload::terminal_upload_path_input(path)
}

pub(crate) fn unique_remote_upload_path(directory: &Path, file_name: &str) -> PathBuf {
    runtime_upload::unique_upload_path(directory, file_name)
}

fn is_terminal_stream_kind(kind: &str) -> bool {
    matches!(
        kind,
        REMOTE_TERMINAL_OUTPUT
            | REMOTE_TERMINAL_OUTPUT_ACK
            | REMOTE_TERMINAL_INPUT
            | REMOTE_TERMINAL_INPUT_ACK
            | REMOTE_TERMINAL_SIGNAL
            | REMOTE_TERMINAL_BUFFER
    )
}

pub(crate) fn remote_git_status_payload(
    project_id: String,
    project_path: String,
    summary: crate::git::GitSummary,
) -> Value {
    runtime_git::git_status_payload(
        project_id,
        project_path,
        crate::git::wire::wire_status_summary(summary),
    )
}

fn remote_worktree_summary_payload(
    project_id: &str,
    summary: crate::worktree::WorktreeSummary,
) -> Value {
    let base_branches = remote_worktree_base_branches(&summary.active_git);
    let default_base_branch = remote_default_worktree_base_branch(&summary.active_git);
    runtime_worktree::worktree_summary_payload(
        project_id,
        summary.selected_worktree_id,
        serde_json::to_value(summary.worktrees).unwrap_or_else(|_| json!([])),
        serde_json::to_value(summary.tasks).unwrap_or_else(|_| json!([])),
        summary.available,
        base_branches,
        default_base_branch,
        summary.error,
    )
}

fn remote_worktree_update_payload(
    project_id: String,
    baseline: crate::worktree::WorktreeSnapshot,
    git: crate::git::GitSummary,
) -> Value {
    runtime_worktree::worktree_update_payload(
        project_id,
        baseline.selected_worktree_id,
        serde_json::to_value(baseline.worktrees).unwrap_or_else(|_| json!([])),
        serde_json::to_value(baseline.tasks).unwrap_or_else(|_| json!([])),
        remote_worktree_base_branches(&git),
        remote_default_worktree_base_branch(&git),
        baseline.error,
    )
}

pub(crate) fn remote_terminal_order_key(value: &Value) -> (String, String) {
    runtime_terminal::terminal_order_key(value)
}

fn terminal_buffer_baseline_key(session_id: &str, request_id: &str) -> String {
    format!("{session_id}:{request_id}")
}

pub(crate) fn remote_terminal_snapshot_payload(
    terminal: TerminalSessionSnapshot,
    layout_kind: &str,
    worktree_id: Option<&str>,
    layout_order: Option<usize>,
) -> Value {
    let mut payload = runtime_terminal::terminal_snapshot_payload(terminal, layout_kind);
    if let Some(worktree_id) = worktree_id.filter(|value| !value.trim().is_empty()) {
        payload["worktreeId"] = json!(worktree_id);
    }
    if let Some(layout_order) = layout_order {
        payload["layoutOrder"] = json!(layout_order);
    }
    payload
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

fn remote_terminal_pty_config(
    scope: &RemoteProjectScope,
    terminal_id: Option<String>,
    title: &str,
    command: Option<String>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> TerminalPtyConfig {
    let cwd = cwd
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| scope.project_path.clone());
    let terminal_id = terminal_id.filter(|value| !value.trim().is_empty());
    let session_key = terminal_id
        .as_ref()
        .map(|terminal_id| format!("gpui:{}:{terminal_id}", scope.worktree_id));
    let session_instance_id = session_key.as_ref().map(|session_key| {
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, session_key.as_bytes()).to_string()
    });
    TerminalPtyConfig {
        cwd: Some(cwd),
        command,
        cols,
        rows,
        project_id: Some(scope.worktree_id.clone()),
        worktree_id: Some(scope.worktree_id.clone()),
        project_name: Some(scope.project_name.clone()),
        terminal_id,
        session_key,
        session_instance_id,
        title: Some(title.to_string()),
        ..Default::default()
    }
}

fn remote_terminal_id_for_scope(scope: &RemoteProjectScope) -> String {
    format!("gpui-term-{}-{}", scope.worktree_id, uuid::Uuid::new_v4())
}

#[cfg(windows)]
fn normalize_remote_path(path: &str) -> String {
    let path = path.trim();
    let path = if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    };
    trim_remote_path_tail(&path)
}

#[cfg(not(windows))]
fn normalize_remote_path(path: &str) -> String {
    trim_remote_path_tail(path.trim())
}

fn trim_remote_path_tail(path: &str) -> String {
    if path == "/" {
        path.to_string()
    } else {
        path.trim_end_matches(['/', '\\']).to_string()
    }
}

fn remote_worktree_base_branches(git: &crate::git::GitSummary) -> Vec<String> {
    runtime_worktree::worktree_base_branches(&git.branch, &crate::git::wire::wire_branches(&git.branches))
}

fn remote_default_worktree_base_branch(git: &crate::git::GitSummary) -> String {
    runtime_worktree::default_worktree_base_branch(
        &git.branch,
        &crate::git::wire::wire_branches(&git.branches),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::transport::RemoteTransport;
    use crate::remote::types::RemoteOutgoingEnvelope;
    use crate::terminal_layout::TerminalPaneSummary;
    use async_trait::async_trait;
    use codux_remote_transport::RemoteTransportKind;

    fn temp_support_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("create temp support dir");
        dir
    }

    #[derive(Default)]
    struct CapturingTransport {
        messages: Mutex<Vec<(Option<String>, Vec<u8>)>>,
    }

    impl CapturingTransport {
        fn take_messages(&self) -> Vec<(Option<String>, Vec<u8>)> {
            self.messages
                .lock()
                .map(|mut messages| messages.drain(..).collect())
                .unwrap_or_default()
        }
    }

    #[async_trait]
    impl RemoteTransport for CapturingTransport {
        fn kind(&self) -> RemoteTransportKind {
            RemoteTransportKind::Iroh
        }

        fn send(&self, data: Vec<u8>, device_id: Option<&str>) -> bool {
            if let Ok(mut messages) = self.messages.lock() {
                messages.push((device_id.map(str::to_string), data));
            }
            true
        }

        async fn shutdown(&self) {}
    }

    #[test]
    fn host_transport_disconnect_clears_stale_transport_and_enters_reconnect() {
        let support_dir = temp_support_dir("codux-remote-host-reconnect");
        write_paired_remote_settings(&support_dir);
        let runtime = RemoteHostRuntime::new(support_dir.clone());
        runtime.connection_generation.store(7, Ordering::SeqCst);
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(Arc::new(CapturingTransport::default()));
        }

        let restart = runtime.prepare_transport_reconnect_after_disconnect(7);

        assert!(restart.is_some());
        let (_, restart_generation) = restart.expect("restart generation");
        assert_eq!(restart_generation, 8);
        assert_eq!(runtime.connection_generation.load(Ordering::SeqCst), 8);
        assert!(runtime.transport.lock().expect("transport lock").is_none());
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.status, "connecting");
        assert_eq!(
            snapshot.message,
            "Remote transport disconnected. Reconnecting..."
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn stale_host_transport_disconnect_does_not_clear_current_transport() {
        let support_dir = temp_support_dir("codux-remote-host-reconnect-stale");
        write_paired_remote_settings(&support_dir);
        let runtime = RemoteHostRuntime::new(support_dir.clone());
        runtime.connection_generation.store(8, Ordering::SeqCst);
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(Arc::new(CapturingTransport::default()));
        }

        let restart = runtime.prepare_transport_reconnect_after_disconnect(7);

        assert!(restart.is_none());
        assert_eq!(runtime.connection_generation.load(Ordering::SeqCst), 8);
        assert!(runtime.transport.lock().expect("transport lock").is_some());

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn pairing_preparation_restarts_host_transport() {
        let support_dir = temp_support_dir("codux-remote-host-pairing-restart");
        write_paired_remote_settings(&support_dir);
        let runtime = RemoteHostRuntime::new(support_dir.clone());
        runtime.connection_generation.store(11, Ordering::SeqCst);
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(Arc::new(CapturingTransport::default()));
        }

        let (transport, generation) = runtime
            .prepare_transport_for_pairing()
            .expect("prepare pairing transport");

        assert!(transport.is_some());
        assert_eq!(generation, 12);
        assert_eq!(runtime.connection_generation.load(Ordering::SeqCst), 12);
        assert!(runtime.transport.lock().expect("transport lock").is_none());
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.status, "connecting");
        assert_eq!(snapshot.message, "Connecting remote transport...");
        assert!(snapshot.pairing.is_none());

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn unauthorized_remote_message_gets_repair_response() {
        let support_dir = temp_support_dir("codux-remote-unauthorized-repair");
        write_paired_remote_settings(&support_dir);
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }

        let raw = RemoteOutgoingEnvelope {
            kind: REMOTE_HOST_INFO.to_string(),
            device_id: Some("unknown-device".to_string()),
            session_id: None,
            seq: None,
            payload: json!({}),
        };
        runtime.clone().handle_transport_message(
            "unknown-device".to_string(),
            serde_json::to_vec(&raw).unwrap(),
        );

        let messages = transport.take_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0.as_deref(), Some("unknown-device"));
        let envelope: RemoteEnvelope =
            serde_json::from_slice(&messages[0].1).expect("unauthorized envelope");
        assert_eq!(envelope.kind, REMOTE_ERROR);
        assert_eq!(envelope.device_id.as_deref(), Some("unknown-device"));
        assert_eq!(envelope.payload["code"], "device_unauthorized");

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn unauthorized_message_without_envelope_device_uses_transport_device_for_repair() {
        let support_dir = temp_support_dir("codux-remote-unauthorized-transport-device-repair");
        write_paired_remote_settings(&support_dir);
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }

        let raw = RemoteOutgoingEnvelope {
            kind: REMOTE_HOST_INFO.to_string(),
            device_id: None,
            session_id: None,
            seq: None,
            payload: json!({}),
        };
        runtime.clone().handle_transport_message(
            "unknown-device".to_string(),
            serde_json::to_vec(&raw).unwrap(),
        );

        let messages = transport.take_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0.as_deref(), Some("unknown-device"));
        let envelope: RemoteEnvelope =
            serde_json::from_slice(&messages[0].1).expect("unauthorized envelope");
        assert_eq!(envelope.kind, REMOTE_ERROR);
        assert_eq!(envelope.device_id.as_deref(), Some("unknown-device"));
        assert_eq!(envelope.payload["code"], "device_unauthorized");

        fs::remove_dir_all(support_dir).ok();
    }

    fn write_paired_remote_settings(support_dir: &Path) {
        fs::write(
            support_dir.join("settings.json"),
            serde_json::to_string_pretty(&json!({
                "remote": {
                    "isEnabled": true,
                    "relayUrl": "http://relay.example",
                    "hostID": "host-1",
                    "cachedDevices": [
                        {
                            "id": "device-1",
                            "hostId": "host-1",
                            "name": "Phone"
                        }
                    ]
                }
            }))
            .expect("serialize settings"),
        )
        .expect("write settings");
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
    fn secure_project_select_keeps_decrypted_device_id_for_scope_and_replies() {
        let support_dir = temp_support_dir("codux-remote-secure-scope-select");
        write_paired_remote_settings(&support_dir);
        write_two_project_state(&support_dir);
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }
        let encrypted = {
            let mut send_seq = HashMap::new();
            runtime
                .service()
                .outgoing_transport_text(
                    "project.select",
                    Some("device-1"),
                    None,
                    json!({ "projectId": "project-b" }),
                    &mut send_seq,
                )
                .expect("secure envelope")
                .into_bytes()
        };

        Arc::clone(&runtime).handle_transport_message("relay-device".to_string(), encrypted);

        assert_eq!(
            runtime.remote_project_scope_id(Some("device-1")).as_deref(),
            Some("project-b")
        );
        assert_eq!(runtime.remote_project_scope_id(Some("relay-device")), None);
        let replies = transport.take_messages();
        assert!(
            replies
                .iter()
                .any(|(device_id, _)| device_id.as_deref() == Some("device-1")),
            "expected reply to decrypted device id"
        );
        assert!(
            replies
                .iter()
                .all(|(device_id, _)| device_id.as_deref() != Some("relay-device")),
            "must not reply to transport device id"
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn transport_ping_runtime_fallback_replies_plain_pong() {
        let support_dir = temp_support_dir("codux-remote-transport-ping-pong");
        write_paired_remote_settings(&support_dir);
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }

        Arc::clone(&runtime).handle_transport_message(
            "device-1".to_string(),
            json!({
                "type": REMOTE_TRANSPORT_PING,
                "deviceId": "device-1",
                "payload": { "id": "ping-1" },
            })
            .to_string()
            .into_bytes(),
        );

        let messages = transport.take_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0.as_deref(), Some("device-1"));
        let reply: Value = serde_json::from_slice(&messages[0].1).expect("plain pong json");
        assert_eq!(
            reply.get("type").and_then(Value::as_str),
            Some(REMOTE_TRANSPORT_PONG)
        );
        assert_eq!(
            reply.get("deviceId").and_then(Value::as_str),
            Some("device-1")
        );
        assert_eq!(reply["payload"]["id"], "ping-1");

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn viewport_events_do_not_broadcast_terminal_list() {
        let support_dir = temp_support_dir("codux-remote-viewport-no-terminal-list");
        write_paired_remote_settings(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }
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

        transport.take_messages();
        runtime.handle_terminal_event(TerminalEvent::Viewport {
            session_id: session_id.clone(),
            owner: "desktop".to_string(),
            cols: 100,
            rows: 32,
            generation: 1,
        });

        let mut kinds = Vec::new();
        for (_, data) in transport.take_messages() {
            let text = String::from_utf8(data).expect("utf8 transport");
            let envelope = runtime
                .service()
                .parse_incoming_envelope(&text)
                .expect("parse outgoing envelope");
            kinds.push(envelope.kind);
        }

        assert_eq!(kinds, vec!["terminal.viewport.state"]);

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
        assert!(payload["selectedWorktreeId"].is_null());
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
        let (_, project_b) = write_two_project_state(&support_dir);
        let worktree_b_path = support_dir.join("project-b-worktree");
        fs::create_dir_all(&worktree_b_path).expect("create worktree b");
        let mut state: Value = serde_json::from_str(
            &fs::read_to_string(support_dir.join("state.json")).expect("read state"),
        )
        .expect("parse state");
        state["worktrees"][0]["path"] = json!(worktree_b_path.to_string_lossy());
        fs::write(
            support_dir.join("state.json"),
            serde_json::to_string_pretty(&state).expect("serialize state"),
        )
        .expect("write state");
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));

        runtime.handle_project_select(&RemoteEnvelope {
            kind: "project.select".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            seq: None,
            payload: json!({ "projectId": "project-b", "worktreeId": "worktree-b" }),
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
        let session = runtime
            .terminals
            .session(session_id)
            .expect("terminal session");
        let expected_session_key = format!("gpui:worktree-b:{session_id}");
        assert_eq!(session.info().project_id, "worktree-b");
        assert_eq!(
            session.info().cwd,
            worktree_b_path.to_string_lossy().as_ref()
        );
        assert_eq!(
            session.info().session_key.as_deref(),
            Some(expected_session_key.as_str())
        );
        assert_eq!(project_terminal["projectId"], "project-b");
        assert_eq!(project_terminal["worktreeId"], "worktree-b");
        assert_eq!(
            project_terminal["cwd"].as_str(),
            Some(worktree_b_path.to_string_lossy().as_ref())
        );
        assert_ne!(
            project_b.to_string_lossy(),
            worktree_b_path.to_string_lossy()
        );
        assert!(
            runtime
                .drain_events()
                .iter()
                .any(|event| matches!(event, RemoteHostEvent::TerminalLayoutChanged(_)))
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_worktree_select_is_device_scoped_and_does_not_mutate_desktop_selection() {
        let support_dir = temp_support_dir("codux-remote-worktree-device-scope");
        let (_, project_b) = write_two_project_state(&support_dir);
        let mut state: Value = serde_json::from_str(
            &fs::read_to_string(support_dir.join("state.json")).expect("read state"),
        )
        .expect("parse state");
        state["worktrees"]
            .as_array_mut()
            .expect("worktrees")
            .push(json!({
                "id": "worktree-c",
                "projectId": "project-b",
                "name": "Task C",
                "branch": "task-c",
                "path": project_b.to_string_lossy(),
                "status": "active",
                "isDefault": false,
                "createdAt": 2,
                "updatedAt": 2
            }));
        state["selectedWorktreeIdByProject"]["project-b"] = json!("worktree-c");
        fs::write(
            support_dir.join("state.json"),
            serde_json::to_string_pretty(&state).expect("serialize state"),
        )
        .expect("write state");
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));

        runtime.handle_worktree_select(&RemoteEnvelope {
            kind: "worktree.select".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            seq: None,
            payload: json!({
                "projectId": "project-b",
                "worktreeId": "worktree-b",
            }),
        });

        let state = fs::read_to_string(support_dir.join("state.json")).expect("read state");
        let state: Value = serde_json::from_str(&state).expect("parse state");
        assert_eq!(state["selectedProjectId"], "project-a");
        assert_eq!(
            state["selectedWorktreeIdByProject"]["project-b"],
            "worktree-c"
        );
        assert_eq!(
            runtime.remote_project_scope_id(Some("device-1")).as_deref(),
            Some("project-b")
        );
        assert!(
            runtime.remote_terminals().iter().any(|terminal| terminal
                .get("projectId")
                .and_then(Value::as_str)
                == Some("project-b")
                && terminal.get("worktreeId").and_then(Value::as_str) == Some("worktree-b"))
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_worktree_select_replaces_saved_terminal_with_wrong_cwd() {
        let support_dir = temp_support_dir("codux-remote-worktree-wrong-cwd");
        let (_, project_b) = write_two_project_state(&support_dir);
        let worktree_b_path = support_dir.join("project-b-worktree");
        fs::create_dir_all(&worktree_b_path).expect("create worktree b");
        let mut state: Value = serde_json::from_str(
            &fs::read_to_string(support_dir.join("state.json")).expect("read state"),
        )
        .expect("parse state");
        state["worktrees"][0]["path"] = json!(worktree_b_path.to_string_lossy());
        fs::write(
            support_dir.join("state.json"),
            serde_json::to_string_pretty(&state).expect("serialize state"),
        )
        .expect("write state");
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let stale_terminal_id = "terminal-stale-worktree-b";
        terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf stale".to_string()),
                    cwd: Some(project_b.to_string_lossy().to_string()),
                    project_id: Some("project-b".to_string()),
                    terminal_id: Some(stale_terminal_id.to_string()),
                    session_key: Some(format!("gpui:project-b:{stale_terminal_id}")),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create stale terminal");
        TerminalLayoutService::new(support_dir.clone())
            .save_from_gpui(
                &terminal_layout_storage_key("project-b", "worktree-b"),
                Vec::new(),
                stale_terminal_id.to_string(),
                vec![TerminalPaneSummary {
                    title: "Stale".to_string(),
                    terminal_id: stale_terminal_id.to_string(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save stale layout");

        runtime.handle_worktree_select(&RemoteEnvelope {
            kind: "worktree.select".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            seq: None,
            payload: json!({
                "projectId": "project-b",
                "worktreeId": "worktree-b",
            }),
        });

        let session = runtime
            .terminals
            .session(stale_terminal_id)
            .expect("recreated terminal session");
        let info = session.info();
        let expected_session_key = format!("gpui:worktree-b:{stale_terminal_id}");
        assert_eq!(info.project_id, "worktree-b");
        assert_eq!(info.cwd, worktree_b_path.to_string_lossy().as_ref());
        assert_eq!(
            info.session_key.as_deref(),
            Some(expected_session_key.as_str())
        );

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
    fn resource_subscriptions_broadcast_project_scoped_git_status() {
        let support_dir = temp_support_dir("codux-remote-resource-subscriptions");
        let (project_a, _) = write_two_project_state(&support_dir);
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }

        runtime.handle_resource_subscribe(&RemoteEnvelope {
            kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
            device_id: Some("phone-a".to_string()),
            session_id: None,
            seq: None,
            payload: json!({
                "resource": REMOTE_RESOURCE_GIT_STATUS,
                "projectId": "project-a",
                "projectPath": project_a.to_string_lossy(),
            }),
        });
        transport.take_messages();

        runtime.handle_git_status(&RemoteEnvelope {
            kind: REMOTE_GIT_STATUS.to_string(),
            device_id: Some("phone-b".to_string()),
            session_id: None,
            seq: None,
            payload: json!({
                "projectId": "project-a",
                "projectPath": project_a.to_string_lossy(),
            }),
        });

        let messages = transport.take_messages();
        let target_devices = messages
            .iter()
            .filter_map(|(device_id, data)| {
                let value: Value = serde_json::from_slice(data).ok()?;
                let kind = value.get("type").and_then(Value::as_str);
                (kind == Some(REMOTE_GIT_STATUS)).then(|| device_id.clone())
            })
            .collect::<Vec<_>>();

        assert!(target_devices.contains(&Some("phone-a".to_string())));
        assert!(target_devices.contains(&Some("phone-b".to_string())));

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn terminal_resource_subscription_sends_tail_raw_baseline() {
        let support_dir = temp_support_dir("codux-remote-resource-terminal-tail-baseline");
        write_paired_remote_settings(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }
        let session_id = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf abcdef".to_string()),
                    cwd: Some(support_dir.to_string_lossy().to_string()),
                    project_id: Some("project-a".to_string()),
                    terminal_id: Some("terminal-a".to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");
        TerminalLayoutService::new(support_dir.clone())
            .save_from_gpui(
                &terminal_layout_storage_key("project-a", "project-a"),
                Vec::new(),
                session_id.clone(),
                vec![TerminalPaneSummary {
                    title: "Main".to_string(),
                    terminal_id: session_id.clone(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save layout");

        let mut baseline = None;
        for _ in 0..20 {
            runtime.handle_resource_subscribe(&RemoteEnvelope {
                kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
                device_id: Some("phone-a".to_string()),
                session_id: None,
                seq: None,
                payload: json!({
                    "resource": REMOTE_RESOURCE_TERMINALS,
                    "projectId": "project-a",
                    "baseline": true,
                    "maxChars": 3,
                    "requestId": "request-1",
                }),
            });
            for (_, data) in transport.take_messages() {
                let value: Value = serde_json::from_slice(&data).expect("json");
                if value.get("type").and_then(Value::as_str) == Some(REMOTE_TERMINAL_OUTPUT)
                    && value.get("sessionId").and_then(Value::as_str) == Some(&session_id)
                {
                    baseline = value.get("payload").cloned();
                    break;
                }
            }
            if baseline.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let baseline = baseline.expect("terminal baseline");
        // Baseline re-attach sends the newest `maxChars` (tail window); the mobile
        // consumer treats `tail: true` as a full keyframe replacement.
        assert_eq!(baseline["data"], "def");
        assert_eq!(baseline["offset"], 3);
        assert_eq!(baseline["tail"], true);
        assert_eq!(baseline["hasPrevious"], true);
        assert_eq!(baseline["truncated"], false);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn project_list_broadcast_preserves_per_device_project_scope() {
        let support_dir = temp_support_dir("codux-remote-project-list-subscriptions");
        write_two_project_state(&support_dir);
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));

        runtime
            .resource_subscriptions
            .subscribe_envelope(&RemoteEnvelope {
                kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
                device_id: Some("phone-a".to_string()),
                session_id: None,
                seq: None,
                payload: json!({ "resource": REMOTE_RESOURCE_PROJECTS }),
            })
            .unwrap();
        runtime
            .resource_subscriptions
            .subscribe_envelope(&RemoteEnvelope {
                kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
                device_id: Some("phone-b".to_string()),
                session_id: None,
                seq: None,
                payload: json!({ "resource": REMOTE_RESOURCE_PROJECTS }),
            })
            .unwrap();
        runtime.set_remote_project_scope(Some("phone-a"), "project-a");
        runtime.set_remote_project_scope(Some("phone-b"), "project-b");

        assert_eq!(
            runtime.remote_project_list_payload(Some("phone-a"))["selectedProjectId"],
            "project-a"
        );
        assert_eq!(
            runtime.remote_project_list_payload(Some("phone-b"))["selectedProjectId"],
            "project-b"
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn terminal_project_subscribe_with_baseline_sends_buffer_baseline() {
        let support_dir = temp_support_dir("codux-remote-terminal-subscribe-baseline");
        let (project_a, _) = write_two_project_state(&support_dir);
        write_paired_remote_settings(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }
        let session_id = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf baseline-data".to_string()),
                    cwd: Some(project_a.to_string_lossy().to_string()),
                    project_id: Some("project-a".to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");

        for _ in 0..20 {
            if terminals
                .snapshot(&session_id)
                .map(|snapshot| snapshot.contains("baseline-data"))
                .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        runtime.handle_terminal_subscribe(&RemoteEnvelope {
            kind: "terminal.subscribe".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            seq: None,
            payload: json!({
                "scope": "project",
                "projectId": "project-a",
                "baseline": true,
                "maxChars": 64,
                "chunkChars": 16
            }),
        });

        let mut baseline = None;
        for (_, data) in transport.take_messages() {
            let text = String::from_utf8(data).expect("utf8 transport");
            let envelope = runtime
                .service()
                .parse_incoming_envelope(&text)
                .expect("parse outgoing envelope");
            if envelope.kind == "terminal.output"
                && envelope.session_id.as_deref() == Some(&session_id)
            {
                baseline = Some(envelope.payload);
                break;
            }
        }
        let baseline = baseline.expect("baseline terminal output");
        assert_eq!(baseline["buffer"], true);
        assert_eq!(baseline["offset"], 0);
        assert_eq!(baseline["requestId"].as_str().is_some(), true);
        assert!(
            baseline["data"]
                .as_str()
                .unwrap_or_default()
                .contains("baseline-data")
        );

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
                true,
            )
            .expect("terminal plan");

        assert_eq!(plan.scope.project_id, "project-b");
        assert_eq!(plan.scope.worktree_id, "worktree-b");
        assert_eq!(plan.config.project_id.as_deref(), Some("worktree-b"));
        assert_eq!(
            plan.config.session_key.as_deref(),
            Some("gpui:worktree-b:terminal-b")
        );
        assert_eq!(plan.config.terminal_id.as_deref(), Some("terminal-b"));

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_list_indexes_all_project_worktree_layouts() {
        let support_dir = temp_support_dir("codux-remote-terminal-all-worktrees");
        write_two_project_state(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        );
        let default_session = terminals
            .create(
                TerminalPtyConfig {
                    command: Some("printf default".to_string()),
                    project_id: Some("project-b".to_string()),
                    terminal_id: Some("terminal-default".to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create default terminal");
        let worktree_session = terminals
            .create(
                TerminalPtyConfig {
                    command: Some("printf worktree".to_string()),
                    project_id: Some("project-b".to_string()),
                    terminal_id: Some("terminal-worktree".to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create worktree terminal");
        TerminalLayoutService::new(support_dir.clone())
            .save_from_gpui(
                &terminal_layout_storage_key("project-b", "project-b"),
                Vec::new(),
                default_session.clone(),
                vec![TerminalPaneSummary {
                    title: "Default".to_string(),
                    terminal_id: default_session.clone(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save default layout");
        TerminalLayoutService::new(support_dir.clone())
            .save_from_gpui(
                &terminal_layout_storage_key("project-b", "worktree-b"),
                Vec::new(),
                worktree_session.clone(),
                vec![TerminalPaneSummary {
                    title: "Worktree".to_string(),
                    terminal_id: worktree_session.clone(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save worktree layout");

        let terminal_worktrees = runtime
            .remote_terminals()
            .into_iter()
            .filter_map(|terminal| {
                Some((
                    terminal.get("id")?.as_str()?.to_string(),
                    terminal.get("worktreeId")?.as_str()?.to_string(),
                ))
            })
            .collect::<HashMap<_, _>>();

        assert_eq!(
            terminal_worktrees.get(&default_session).map(String::as_str),
            Some("project-b")
        );
        assert_eq!(
            terminal_worktrees
                .get(&worktree_session)
                .map(String::as_str),
            Some("worktree-b")
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_list_reports_all_worktree_splits_under_root_project() {
        let support_dir = temp_support_dir("codux-remote-terminal-worktree-splits");
        write_two_project_state(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        );
        let sessions = (0..3)
            .map(|index| {
                terminals
                    .create(
                        TerminalPtyConfig {
                            command: Some(format!("printf split-{index}")),
                            project_id: Some("worktree-b".to_string()),
                            terminal_id: Some(format!("terminal-worktree-{index}")),
                            ..Default::default()
                        },
                        |_| {},
                    )
                    .expect("create worktree terminal")
            })
            .collect::<Vec<_>>();
        TerminalLayoutService::new(support_dir.clone())
            .save_from_gpui(
                &terminal_layout_storage_key("project-b", "project-b"),
                Vec::new(),
                sessions[0].clone(),
                vec![TerminalPaneSummary {
                    title: "Stale".to_string(),
                    terminal_id: sessions[0].clone(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save stale default layout");
        TerminalLayoutService::new(support_dir.clone())
            .save_from_gpui(
                &terminal_layout_storage_key("project-b", "worktree-b"),
                Vec::new(),
                sessions[0].clone(),
                sessions
                    .iter()
                    .enumerate()
                    .map(|(index, session)| TerminalPaneSummary {
                        title: format!("Split {}", index + 1),
                        terminal_id: session.clone(),
                    })
                    .collect(),
                vec![0.33, 0.34, 0.33],
                0.24,
            )
            .expect("save worktree split layout");

        let mut worktree_terminals = runtime
            .remote_terminals()
            .into_iter()
            .filter(|terminal| {
                terminal.get("projectId").and_then(Value::as_str) == Some("project-b")
            })
            .filter(|terminal| {
                terminal.get("worktreeId").and_then(Value::as_str) == Some("worktree-b")
            })
            .collect::<Vec<_>>();
        worktree_terminals.sort_by_key(|terminal| {
            terminal
                .get("layoutOrder")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX)
        });

        assert_eq!(worktree_terminals.len(), 3);
        assert_eq!(
            worktree_terminals
                .iter()
                .filter_map(|terminal| terminal.get("id").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            sessions.iter().map(String::as_str).collect::<Vec<_>>()
        );
        assert_eq!(
            worktree_terminals
                .iter()
                .filter_map(|terminal| terminal.get("layoutOrder").and_then(Value::as_u64))
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_create_plan_does_not_reuse_saved_layout_terminal() {
        let support_dir = temp_support_dir("codux-remote-create-new-terminal");
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

        let create_plan = runtime
            .remote_terminal_plan_from_envelope(
                &RemoteEnvelope {
                    kind: "terminal.create".to_string(),
                    device_id: Some("device-1".to_string()),
                    session_id: None,
                    seq: None,
                    payload: json!({"layoutKind": "tab"}),
                },
                None,
                false,
            )
            .expect("create terminal plan");
        assert_eq!(create_plan.config.terminal_id, None);
        assert_eq!(create_plan.config.project_id.as_deref(), Some("worktree-b"));
        let expected_worktree_path = support_dir.join("project-b");
        let expected_worktree_path = expected_worktree_path.to_string_lossy();
        assert_eq!(
            create_plan.config.cwd.as_deref(),
            Some(expected_worktree_path.as_ref())
        );
        assert_eq!(create_plan.layout_kind, "tab");

        let restore_plan = runtime
            .remote_terminal_plan_from_envelope(
                &RemoteEnvelope {
                    kind: "terminal.buffer".to_string(),
                    device_id: Some("device-1".to_string()),
                    session_id: None,
                    seq: None,
                    payload: json!({}),
                },
                None,
                true,
            )
            .expect("restore terminal plan");
        assert_eq!(
            restore_plan.config.terminal_id.as_deref(),
            Some("terminal-b")
        );
        assert_eq!(
            restore_plan.config.project_id.as_deref(),
            Some("worktree-b")
        );
        assert_eq!(
            restore_plan.config.session_key.as_deref(),
            Some("gpui:worktree-b:terminal-b")
        );

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
    fn remote_terminal_create_emits_layout_changed_event() {
        let support_dir = temp_support_dir("codux-remote-create-layout-event");
        write_two_project_state(&support_dir);
        let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
        runtime.set_remote_project_scope(Some("device-1"), "project-b");
        runtime.drain_events();

        runtime.handle_terminal_create(&RemoteEnvelope {
            kind: "terminal.create".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            seq: None,
            payload: json!({
                "projectId": "project-b",
                "worktreeId": "worktree-b",
                "layoutKind": "tab",
            }),
        });

        let layout_key = terminal_layout_storage_key("project-b", "worktree-b");
        let layout = TerminalLayoutService::new(support_dir.clone()).load(Some(&layout_key));
        assert_eq!(layout.tabs.len(), 1);
        assert!(
            runtime
                .drain_events()
                .iter()
                .any(|event| matches!(event, RemoteHostEvent::TerminalLayoutChanged(_)))
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_close_removes_layout_entry_but_keeps_last_terminal() {
        let support_dir = temp_support_dir("codux-remote-close-layout-entry");
        write_two_project_state(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let layout_key = terminal_layout_storage_key("project-b", "worktree-b");
        let session_a = terminals
            .create(
                TerminalPtyConfig {
                    command: Some("printf a".to_string()),
                    project_id: Some("worktree-b".to_string()),
                    terminal_id: Some("terminal-a".to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal a");
        let session_b = terminals
            .create(
                TerminalPtyConfig {
                    command: Some("printf b".to_string()),
                    project_id: Some("worktree-b".to_string()),
                    terminal_id: Some("terminal-b".to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal b");
        TerminalLayoutService::new(support_dir.clone())
            .save_from_gpui(
                &layout_key,
                Vec::new(),
                session_a.clone(),
                vec![
                    TerminalPaneSummary {
                        title: "A".to_string(),
                        terminal_id: session_a.clone(),
                    },
                    TerminalPaneSummary {
                        title: "B".to_string(),
                        terminal_id: session_b.clone(),
                    },
                ],
                vec![0.5, 0.5],
                0.24,
            )
            .expect("save layout");
        runtime.drain_events();

        runtime.handle_terminal_close(&RemoteEnvelope {
            kind: "terminal.close".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: Some(session_b.clone()),
            seq: None,
            payload: json!({ "projectId": "project-b", "worktreeId": "worktree-b" }),
        });

        let layout = TerminalLayoutService::new(support_dir.clone()).load(Some(&layout_key));
        assert_eq!(layout.top_panes.len(), 1);
        assert_eq!(layout.top_panes[0].terminal_id, session_a);
        assert!(terminals.snapshot(&session_b).is_err());
        assert!(
            runtime
                .drain_events()
                .iter()
                .any(|event| matches!(event, RemoteHostEvent::TerminalLayoutChanged(_)))
        );

        runtime.handle_terminal_close(&RemoteEnvelope {
            kind: "terminal.close".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: Some(session_a.clone()),
            seq: None,
            payload: json!({ "projectId": "project-b", "worktreeId": "worktree-b" }),
        });

        let layout = TerminalLayoutService::new(support_dir.clone()).load(Some(&layout_key));
        assert_eq!(layout.top_panes.len(), 1);
        assert_eq!(layout.top_panes[0].terminal_id, session_a);
        assert!(terminals.snapshot(&session_a).is_ok());

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
    fn remote_terminal_buffer_window_returns_retained_history_window() {
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
        assert!(!window.has_previous);

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
    fn remote_terminal_buffer_window_freezes_pages_for_request_id() {
        let support_dir = temp_support_dir("codux-remote-terminal-buffer-frozen-pages");
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
                    command: Some("cat".to_string()),
                    cwd: Some(support_dir.to_string_lossy().to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");
        terminals
            .write(&session_id, b"abcdef")
            .expect("write initial");

        let mut first = None;
        for _ in 0..20 {
            let current = runtime
                .terminal_buffer_window(
                    &session_id,
                    0,
                    3,
                    Some("request-freeze".to_string()),
                    false,
                )
                .expect("first terminal buffer window");
            if current.total_characters >= 6 {
                first = Some(current);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let first = first.expect("terminal output");
        assert_eq!(first.data, "abc");
        assert_eq!(first.total_characters, 6);
        assert!(first.truncated);

        terminals
            .write(&session_id, b"XYZ")
            .expect("write appended");
        std::thread::sleep(std::time::Duration::from_millis(25));

        let second = runtime
            .terminal_buffer_window(&session_id, 3, 3, Some("request-freeze".to_string()), false)
            .expect("second terminal buffer window");
        assert_eq!(second.data, "def");
        assert_eq!(second.offset, 3);
        assert_eq!(second.total_characters, 6);
        assert_eq!(second.output_seq, first.output_seq);
        assert!(!second.truncated);

        let live = runtime
            .terminal_buffer_window(&session_id, 0, 16, Some("request-live".to_string()), false)
            .expect("live terminal buffer window");
        assert!(live.total_characters >= 9);
        assert!(live.data.contains("XYZ"));

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_buffer_window_tail_returns_history_tail() {
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
            if current.data.contains("def") {
                window = Some(current);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let window = window.expect("terminal output");

        assert!(window.data.contains("def"));
        assert_eq!(window.offset, 3);
        assert_eq!(window.total_characters, 6);
        assert!(!window.truncated);
        assert_eq!(window.request_id.as_deref(), Some("request-1"));
        assert!(window.tail);
        assert!(window.has_previous);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_buffer_window_tail_includes_headless_screen_baseline() {
        let support_dir = temp_support_dir("codux-remote-terminal-buffer-screen-baseline");
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
                    command: Some("printf 'old line\\n\\033[2J\\033[Hvisible tui'".to_string()),
                    cwd: Some(support_dir.to_string_lossy().to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");

        let mut window = None;
        for _ in 0..20 {
            let current = runtime
                .terminal_buffer_window(&session_id, 0, 16, Some("request-1".to_string()), true)
                .expect("terminal buffer window");
            if current
                .screen_data
                .as_deref()
                .is_some_and(|data| data.contains("visible tui"))
            {
                window = Some(current);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let window = window.expect("terminal screen baseline");
        let screen_data = window.screen_data.expect("screen data");

        assert!(window.data.contains("visible tui"));
        assert!(screen_data.contains("visible tui"));
        assert!(!screen_data.contains("old line"));
        assert!(window.tail);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn terminal_resource_subscribe_baseline_includes_tail_screen_keyframe() {
        let support_dir = temp_support_dir("codux-resource-subscribe-terminal-screen-baseline");
        write_paired_remote_settings(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }
        let session_id = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some("printf 'old line\\n\\033[2J\\033[Hvisible tui'".to_string()),
                    cwd: Some(support_dir.to_string_lossy().to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");

        for _ in 0..20 {
            if terminals
                .screen_snapshot(&session_id)
                .map(|snapshot| snapshot.data.contains("visible tui"))
                .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        runtime.handle_resource_subscribe(&RemoteEnvelope {
            kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
            device_id: Some("phone-a".to_string()),
            session_id: None,
            seq: None,
            payload: json!({
                "resource": REMOTE_RESOURCE_TERMINALS,
                "sessionId": session_id,
                "baseline": true,
                "maxChars": 16,
                "requestId": "request-1",
            }),
        });

        let mut baseline = None;
        for (_, data) in transport.take_messages() {
            let text = String::from_utf8(data).expect("utf8 transport");
            let envelope = runtime
                .service()
                .parse_incoming_envelope(&text)
                .expect("parse outgoing envelope");
            if envelope.kind == REMOTE_TERMINAL_OUTPUT
                && envelope.payload.get("buffer").and_then(Value::as_bool) == Some(true)
            {
                baseline = Some(envelope.payload);
                break;
            }
        }
        let baseline = baseline.expect("terminal baseline");

        assert_eq!(baseline["requestId"], "request-1");
        assert_eq!(baseline["tail"], true);
        assert!(
            baseline["data"]
                .as_str()
                .unwrap_or_default()
                .contains("visible tui")
        );
        assert!(
            baseline["screenData"]
                .as_str()
                .unwrap_or_default()
                .contains("visible tui")
        );
        assert!(
            !baseline["screenData"]
                .as_str()
                .unwrap_or_default()
                .contains("old line")
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn remote_terminal_live_output_is_data_only_without_screen_keyframe() {
        let support_dir = temp_support_dir("codux-remote-terminal-live-screen-keyframe");
        write_paired_remote_settings(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }
        let session_id = terminals
            .create(
                TerminalPtyConfig {
                    shell: Some("sh".to_string()),
                    command: Some(
                        "printf '\\033[2J\\033[Hrestored tui\\n\\033[3;1Hinput box'".to_string(),
                    ),
                    cwd: Some(support_dir.to_string_lossy().to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");

        for _ in 0..20 {
            if terminals
                .screen_snapshot(&session_id)
                .map(|snapshot| snapshot.data.contains("restored tui"))
                .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        runtime.register_terminal_viewer(&session_id, Some("device-1"));
        transport.take_messages();

        runtime.queue_terminal_output_batch(session_id.clone(), "partial live raw".to_string());
        runtime.flush_terminal_output_batch(&session_id);

        let mut live = None;
        for (_, data) in transport.take_messages() {
            let text = String::from_utf8(data).expect("utf8 transport");
            let envelope = runtime
                .service()
                .parse_incoming_envelope(&text)
                .expect("parse outgoing envelope");
            if envelope.kind == "terminal.output"
                && envelope.session_id.as_deref() == Some(&session_id)
            {
                live = Some(envelope.payload);
                break;
            }
        }
        let live = live.expect("live terminal output");

        assert_eq!(live["data"], "partial live raw");
        assert_eq!(live["outputSeq"], 1);
        // Live output is a pure byte stream now — NO screen keyframe. Replaying a
        // whole-screen keyframe on top of the emulator's own scrollback duplicated
        // the screen (badly on resize bursts), so the host no longer sends one.
        assert!(
            live.get("screenData").is_none(),
            "live terminal output must not carry a screen keyframe"
        );

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
        assert_eq!(state.rows, 32);

        let ignored = terminals
            .resize_viewport(&session_id, "remote:device-2", 120, 40)
            .expect("resize from non-owner");
        assert!(ignored.is_none());
        let state = terminals
            .viewport_state(&session_id)
            .expect("viewport state");
        assert_eq!(state.owner, "remote:device-1");
        assert_eq!(state.cols, 72);
        assert_eq!(state.rows, 32);

        let ignored = terminals
            .resize_viewport(&session_id, "desktop", 100, 32)
            .expect("resize from desktop while remote owns");
        assert!(ignored.is_none());
        let state = terminals
            .viewport_state(&session_id)
            .expect("viewport state");
        assert_eq!(state.owner, "remote:device-1");
        assert_eq!(state.cols, 72);
        assert_eq!(state.rows, 32);

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
        assert_eq!(state.rows, 32);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn terminal_viewport_resize_pushes_state_without_screen_keyframe() {
        let support_dir = temp_support_dir("codux-remote-terminal-viewport-keyframe");
        write_paired_remote_settings(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }
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

        for _ in 0..20 {
            if terminals
                .screen_snapshot(&session_id)
                .map(|snapshot| snapshot.data.contains("ready"))
                .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        transport.take_messages();
        runtime.handle_terminal_viewport_claim(&RemoteEnvelope {
            kind: "terminal.viewport.claim".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: Some(session_id.clone()),
            seq: None,
            payload: json!({}),
        });
        transport.take_messages();
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

        let mut saw_state = false;
        let mut keyframe = None;
        for (device_id, data) in transport.take_messages() {
            let text = String::from_utf8(data).expect("utf8 transport");
            let envelope = runtime
                .service()
                .parse_incoming_envelope(&text)
                .expect("parse outgoing envelope");
            match envelope.kind.as_str() {
                REMOTE_TERMINAL_VIEWPORT_STATE
                    if device_id.as_deref() == Some("device-1")
                        && envelope.session_id.as_deref() == Some(&session_id) =>
                {
                    saw_state = true
                }
                REMOTE_TERMINAL_OUTPUT => {
                    if device_id.as_deref() == Some("device-1")
                        && envelope.session_id.as_deref() == Some(&session_id)
                    {
                        keyframe = Some(envelope.payload);
                    }
                }
                _ => {}
            }
        }

        assert!(saw_state, "resize must still push viewport state");
        // No screen keyframe: the desktop emulator handles resize via the shell's
        // own repaint in the live byte stream (like a local terminal). Pushing a
        // whole-screen keyframe duplicated the screen on every resize event.
        assert!(
            keyframe.is_none(),
            "resize must not push a screen keyframe (it duplicated on resize)"
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn terminal_subscribe_does_not_push_screen_keyframe() {
        let support_dir = temp_support_dir("codux-remote-terminal-subscribe-keyframe");
        write_paired_remote_settings(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        let transport = Arc::new(CapturingTransport::default());
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(transport.clone());
        }
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

        for _ in 0..20 {
            if terminals
                .screen_snapshot(&session_id)
                .map(|snapshot| snapshot.data.contains("ready"))
                .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        transport.take_messages();
        runtime.handle_terminal_subscribe(&RemoteEnvelope {
            kind: "terminal.subscribe".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: Some(session_id.clone()),
            seq: None,
            payload: json!({}),
        });

        let mut keyframe = None;
        for (device_id, data) in transport.take_messages() {
            let text = String::from_utf8(data).expect("utf8 transport");
            let envelope = runtime
                .service()
                .parse_incoming_envelope(&text)
                .expect("parse outgoing envelope");
            if device_id.as_deref() == Some("device-1")
                && envelope.kind == "terminal.output"
                && envelope.session_id.as_deref() == Some(&session_id)
            {
                keyframe = Some(envelope.payload);
                break;
            }
        }
        // A plain subscribe (no baseline requested) pushes viewport state only —
        // no screen keyframe. The keyframe duplicated the screen in the desktop's
        // own scrollback; the re-attach seed rides the baseline buffer instead.
        assert!(
            keyframe.is_none(),
            "subscribe must not push a screen keyframe (it duplicated the screen)"
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn device_disconnect_releases_owned_terminal_viewport() {
        let support_dir = temp_support_dir("codux-remote-terminal-viewport-disconnect");
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
        assert_eq!(
            terminals
                .viewport_state(&session_id)
                .expect("viewport state")
                .owner,
            "remote:device-1"
        );

        runtime.handle_remote_envelope(RemoteEnvelope {
            kind: "device.disconnected".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            seq: None,
            payload: json!({}),
        });

        let state = terminals
            .viewport_state(&session_id)
            .expect("viewport state");
        assert_eq!(state.owner, "remote:device-1");
        assert_eq!((state.cols, state.rows), (72, 32));

        let expired = terminals
            .expire_viewport_lease_for_test(&session_id)
            .expect("expire viewport lease")
            .expect("expired viewport state");
        assert_eq!(expired.owner, "desktop");
        assert_eq!((expired.cols, expired.rows), (72, 32));

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn device_transport_disconnect_keeps_viewport_until_lease_expires() {
        let support_dir = temp_support_dir("codux-remote-terminal-viewport-transport-disconnect");
        write_paired_remote_settings(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        runtime.connection_generation.store(7, Ordering::SeqCst);
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(Arc::new(CapturingTransport::default()));
        }
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
        terminals
            .claim_viewport(&session_id, "remote:device-1")
            .expect("remote claim");

        runtime.handle_transport_state(7, "device-1".to_string(), "disconnected".to_string());

        let state = terminals
            .viewport_state(&session_id)
            .expect("viewport state");
        assert_eq!(state.owner, "remote:device-1");

        let expired = terminals
            .expire_viewport_lease_for_test(&session_id)
            .expect("expire viewport lease")
            .expect("expired viewport state");
        assert_eq!(expired.owner, "desktop");

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn host_transport_disconnect_releases_remote_terminal_viewports() {
        let support_dir = temp_support_dir("codux-remote-terminal-viewport-host-disconnect");
        write_paired_remote_settings(&support_dir);
        let terminals = Arc::new(TerminalManager::new());
        let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
            support_dir.clone(),
            Default::default(),
            Arc::clone(&terminals),
        ));
        runtime.connection_generation.store(7, Ordering::SeqCst);
        if let Ok(mut current) = runtime.transport.lock() {
            *current = Some(Arc::new(CapturingTransport::default()));
        }
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
        terminals
            .claim_viewport(&session_id, "remote:device-1")
            .expect("remote claim");

        runtime.handle_transport_state(7, String::new(), "closed".to_string());

        let state = terminals
            .viewport_state(&session_id)
            .expect("viewport state");
        assert_eq!(state.owner, "desktop");

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn terminal_resize_without_owner_claims_remote_viewport_for_compatibility() {
        let support_dir = temp_support_dir("codux-remote-terminal-resize-without-owner");
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
        assert_eq!(state.rows, 32);

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn ai_stats_watcher_tracks_one_project_per_device_and_clears_on_disconnect() {
        let support_dir = temp_support_dir("codux-remote-ai-stats-watcher");
        let runtime = RemoteHostRuntime::new(support_dir.clone());

        runtime.register_ai_stats_watcher("project-a", "device-1", "project-a");
        runtime.register_ai_stats_watcher("project-a", "device-2", "worktree-x");
        {
            let watchers = runtime.ai_stats_watchers.lock().unwrap();
            assert_eq!(watchers["project-a"].len(), 2);
            assert_eq!(watchers["project-a"]["device-2"], "worktree-x");
        }

        // Switching a device to another project drops its old-project entry.
        runtime.register_ai_stats_watcher("project-b", "device-1", "project-b");
        {
            let watchers = runtime.ai_stats_watchers.lock().unwrap();
            assert!(!watchers["project-a"].contains_key("device-1"));
            assert!(watchers["project-b"].contains_key("device-1"));
            assert!(watchers["project-a"].contains_key("device-2"));
        }

        // Disconnect drops the device from every project, pruning empties.
        runtime.clear_ai_stats_watcher_device("device-1");
        runtime.clear_ai_stats_watcher_device("device-2");
        assert!(runtime.ai_stats_watchers.lock().unwrap().is_empty());

        fs::remove_dir_all(support_dir).ok();
    }
}
