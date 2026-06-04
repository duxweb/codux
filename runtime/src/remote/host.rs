use super::crypto::{remote_base64_url_decode, remote_host_name, remote_pairing_match_code};
use super::http::{remote_error_message, remote_server_url, remote_url};
use super::types::{
    RemoteEnvelope, RemoteOutgoingEnvelope, RemotePendingPairing, RemoteSettings, RemoteSummary,
};
use super::RemoteService;
use base64::Engine;
use crate::ai_history_indexer::{AIHistoryIndexer, AIHistoryProjectState};
use crate::ai_history_normalized::AIHistoryProjectRequest;
use crate::project_store::{ProjectCreateRequest, ProjectStore, ProjectUpdateRequest};
use crate::remote_p2p::{RemoteP2PHostTransport, RemoteP2PLane, RemoteP2PSignal};
use crate::terminal_layout::{
    TerminalLayoutService, TerminalPaneSummary, terminal_layout_storage_key,
};
use crate::terminal_pty::{
    TerminalEvent, TerminalManager, TerminalPtyConfig, TerminalSessionSnapshot,
};
use futures_util::{SinkExt, StreamExt};
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
    time::Duration,
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;

pub const REMOTE_PROTOCOL_VERSION: &str = "v1.0";

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
}

pub struct RemoteHostRuntime {
    support_dir: PathBuf,
    ai_history: AIHistoryIndexer,
    terminals: Arc<TerminalManager>,
    terminal_viewers_by_session: Mutex<HashMap<String, HashSet<String>>>,
    remote_project_scope_by_device: Mutex<HashMap<String, String>>,
    terminal_event_subscriptions: Mutex<HashSet<String>>,
    terminal_upload_sessions: Mutex<HashMap<String, RemoteTerminalUploadSession>>,
    p2p: Mutex<Option<Arc<RemoteP2PHostTransport>>>,
    events: Mutex<VecDeque<RemoteSummary>>,
    snapshot: Mutex<RemoteSummary>,
    socket_tx: Mutex<Option<mpsc::UnboundedSender<String>>>,
    connection_generation: AtomicU64,
    send_seq_by_device: Mutex<HashMap<String, i64>>,
    receive_seq_by_device: Mutex<HashMap<String, i64>>,
}

impl RemoteHostRuntime {
    pub fn new(support_dir: PathBuf) -> Self {
        Self::new_with_ai_history(support_dir, AIHistoryIndexer::new())
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
            support_dir,
            ai_history,
            terminals,
            terminal_viewers_by_session: Mutex::new(HashMap::new()),
            remote_project_scope_by_device: Mutex::new(HashMap::new()),
            terminal_event_subscriptions: Mutex::new(HashSet::new()),
            terminal_upload_sessions: Mutex::new(HashMap::new()),
            p2p: Mutex::new(None),
            events: Mutex::new(VecDeque::new()),
            snapshot: Mutex::new(snapshot),
            socket_tx: Mutex::new(None),
            connection_generation: AtomicU64::new(0),
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
        self.ensure_p2p_transport();
        let summary = self.service().summary();
        if !summary.enabled || summary.relay.trim().is_empty() {
            self.stop_with_message("Remote Host stopped.");
            return self.snapshot();
        }
        let current = self.snapshot();
        let has_started_connect_loop = self.connection_generation.load(Ordering::SeqCst) > 0;
        if has_started_connect_loop
            && current.enabled
            && current.relay == summary.relay
            && matches!(current.status.as_str(), "connected" | "connecting")
        {
            return current;
        }
        self.update_snapshot(summary);
        self.spawn_connect_loop(0);
        self.snapshot()
    }

    pub fn stop_with_message(&self, message: &str) {
        self.connection_generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut tx) = self.socket_tx.lock() {
            *tx = None;
        }
        let mut summary = self.service().summary();
        summary.status = "stopped".to_string();
        summary.message = message.to_string();
        self.update_snapshot(summary);
    }

    pub fn shutdown(&self) {
        self.stop_with_message("Remote Host stopped.");
        if let Ok(mut p2p) = self.p2p.lock() {
            *p2p = None;
        }
        if let Ok(mut viewers) = self.terminal_viewers_by_session.lock() {
            viewers.clear();
        }
        if let Ok(mut uploads) = self.terminal_upload_sessions.lock() {
            uploads.clear();
        }
        for terminal in self.terminals.list() {
            let _ = self.terminals.kill(&terminal.id);
        }
    }

    pub fn reconnect(self: &Arc<Self>) -> RemoteSummary {
        crate::runtime_trace::runtime_trace("remote", "host_reconnect requested");
        if let Ok(mut tx) = self.socket_tx.lock() {
            *tx = None;
        }
        let mut summary = self.service().summary();
        summary.status = "connecting".to_string();
        summary.message = "Connecting relay...".to_string();
        summary.pairing = self.snapshot().pairing;
        self.update_snapshot(summary);
        self.spawn_connect_loop(0);
        self.snapshot()
    }

    pub fn send_relay(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) -> bool {
        let Some(text) = self.outgoing_relay_text(kind, device_id, session_id, payload) else {
            return false;
        };
        self.socket_tx
            .lock()
            .ok()
            .and_then(|tx| tx.as_ref().cloned())
            .map(|tx| tx.send(text).is_ok())
            .unwrap_or(false)
    }

    fn spawn_connect_loop(self: &Arc<Self>, initial_delay_ms: u64) {
        let generation = self.connection_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let runtime = Arc::clone(self);
        crate::async_runtime::spawn(async move {
            if initial_delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(initial_delay_ms)).await;
            }
            runtime.connect_loop(generation).await;
        });
    }

    async fn connect_loop(self: Arc<Self>, generation: u64) {
        let mut delay = 1_u64;
        loop {
            if generation != self.connection_generation.load(Ordering::SeqCst) {
                return;
            }
            let summary = self.service().summary();
            if !summary.enabled {
                return;
            }
            if let Err(error) = self.connect_once(generation).await {
                let mut status = self.service().summary();
                status.status = "failed".to_string();
                status.message = error;
                status.pairing = self.snapshot().pairing;
                self.update_snapshot(status);
            }
            if generation != self.connection_generation.load(Ordering::SeqCst) {
                return;
            }
            tokio::time::sleep(Duration::from_secs(delay)).await;
            delay = (delay * 2).min(30);
        }
    }

    async fn connect_once(self: &Arc<Self>, generation: u64) -> Result<(), String> {
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("connect_once start generation={generation}"),
        );
        self.service().register_host_async().await?;
        if generation != self.connection_generation.load(Ordering::SeqCst) {
            return Ok(());
        }
        let snapshot = self.snapshot();
        if snapshot.status == "connecting" && self.socket_tx.lock().ok().and_then(|tx| tx.clone()).is_none() {
            let mut connecting = self.service().summary();
            connecting.status = "connecting".to_string();
            connecting.message = "Connecting relay...".to_string();
            connecting.pairing = snapshot.pairing;
            self.update_snapshot(connecting);
        }
        if let Err(error) = self.service().refresh_devices_async().await {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!("connect_once refresh_devices failed error={error}"),
            );
        }
        let settings = super::remote_settings_from_raw(&self.service().raw_settings());
        let ws_url = remote_url(
            &remote_server_url(&settings),
            "/ws/host",
            &[
                ("hostId", settings.host_id.as_str()),
                ("token", settings.host_token.as_str()),
            ],
            true,
        )?;
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("connect_once websocket_connect generation={generation}"),
        );
        let (socket, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(remote_error_message)?;
        let (mut write, mut read) = socket.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        if let Ok(mut current) = self.socket_tx.lock() {
            *current = Some(tx);
        }

        let mut connected = self.service().summary();
        connected.status = "connected".to_string();
        connected.message = "Remote Host connected.".to_string();
        connected.pairing = self.snapshot().pairing;
        self.update_snapshot(connected);
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!("connect_once connected generation={generation}"),
        );

        let writer = crate::async_runtime::spawn(async move {
            while let Some(message) = rx.recv().await {
                if write
                    .send(WebSocketMessage::Text(message.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        while let Some(message) = read.next().await {
            if generation != self.connection_generation.load(Ordering::SeqCst) {
                writer.abort();
                return Ok(());
            }
            match message {
                Ok(WebSocketMessage::Text(text)) => {
                    self.handle_socket_text(text.to_string());
                }
                Ok(WebSocketMessage::Close(_)) => break,
                Ok(_) => {}
                Err(error) => {
                    writer.abort();
                    return Err(error.to_string());
                }
            }
        }
        writer.abort();
        if let Ok(mut current) = self.socket_tx.lock() {
            *current = None;
        }
        Err("Remote Host disconnected.".to_string())
    }

    fn handle_socket_text(self: &Arc<Self>, text: String) {
        let Ok(raw) = self.service().parse_incoming_envelope(&text) else {
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
        self.handle_remote_envelope(envelope);
    }

    fn handle_remote_envelope(self: &Arc<Self>, envelope: RemoteEnvelope) {
        match envelope.kind.as_str() {
            "pairing.request" => self.handle_pairing_request(&envelope),
            "host.info" => self.send(
                "host.info",
                envelope.device_id.as_deref(),
                None,
                json!({
                    "hostId": self.snapshot().host_id,
                    "name": remote_host_name(),
                    "platform": std::env::consts::OS,
                    "app": "Codux",
                    "protocolVersion": REMOTE_PROTOCOL_VERSION,
                }),
            ),
            "device.connected" => {
                self.update_device_online(envelope.device_id.as_deref(), true);
                self.send_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            "device.disconnected" => {
                self.update_device_online(envelope.device_id.as_deref(), false);
                self.clear_remote_project_scope(envelope.device_id.as_deref());
                self.remove_terminal_viewer(envelope.device_id.as_deref());
                self.close_p2p(envelope.device_id.as_deref());
            }
            "project.list" => self.send_project_list(envelope.device_id.as_deref()),
            "project.select" => self.handle_project_select(&envelope),
            "terminal.list" => self.send_terminal_list(envelope.device_id.as_deref()),
            "terminal.create" => self.handle_terminal_create(&envelope),
            "terminal.buffer" => self.handle_terminal_buffer(&envelope),
            "terminal.input" => self.handle_terminal_input(&envelope),
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
            "project.add" => self.handle_project_add(&envelope),
            "project.edit" => self.handle_project_edit(&envelope),
            "project.remove" => self.handle_project_remove(&envelope),
            "ai.stats" => self.handle_ai_stats(&envelope),
            "p2p.ping" => self.send_terminal_data(
                "p2p.pong",
                envelope.device_id.as_deref(),
                None,
                envelope.payload,
            ),
            "p2p.offer" => self.handle_p2p_offer(&envelope),
            "p2p.candidate" => self.handle_p2p_candidate(&envelope),
            _ => {}
        }
    }

    fn ensure_p2p_transport(self: &Arc<Self>) {
        if self.p2p.lock().ok().and_then(|value| value.clone()).is_some() {
            return;
        }
        let weak_for_signal = Arc::downgrade(self);
        let weak_for_message = Arc::downgrade(self);
        let weak_for_state = Arc::downgrade(self);
        let Ok(transport) = RemoteP2PHostTransport::new(
            Arc::new(move |signal: RemoteP2PSignal| {
                if let Some(runtime) = weak_for_signal.upgrade() {
                    runtime.send_relay(&signal.kind, Some(&signal.device_id), None, signal.payload);
                }
            }),
            Arc::new(move |device_id: String, data: Vec<u8>| {
                if let Some(runtime) = weak_for_message.upgrade() {
                    crate::async_runtime::spawn(async move {
                        runtime.handle_p2p_message(device_id, data);
                    });
                }
            }),
            Arc::new(move |device_id: String, state: String| {
                if let Some(runtime) = weak_for_state.upgrade() {
                    if matches!(state.as_str(), "closed" | "failed" | "disconnected") {
                        runtime.remove_terminal_viewer(Some(&device_id));
                    }
                }
            }),
        ) else {
            return;
        };
        if let Ok(mut current) = self.p2p.lock() {
            *current = Some(transport);
        }
    }

    fn handle_p2p_offer(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        self.ensure_p2p_transport();
        let Some(device_id) = envelope.device_id.clone() else {
            return;
        };
        let Some(p2p) = self.p2p.lock().ok().and_then(|value| value.clone()) else {
            return;
        };
        let payload = envelope.payload.clone();
        crate::async_runtime::spawn(async move {
            p2p.handle_offer(device_id, payload).await;
        });
    }

    fn handle_p2p_candidate(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        self.ensure_p2p_transport();
        let Some(device_id) = envelope.device_id.clone() else {
            return;
        };
        let Some(p2p) = self.p2p.lock().ok().and_then(|value| value.clone()) else {
            return;
        };
        let payload = envelope.payload.clone();
        crate::async_runtime::spawn(async move {
            p2p.handle_candidate(device_id, payload).await;
        });
    }

    fn close_p2p(&self, device_id: Option<&str>) {
        let Some(device_id) = device_id.map(str::to_string) else {
            return;
        };
        let Some(p2p) = self.p2p.lock().ok().and_then(|value| value.clone()) else {
            return;
        };
        crate::async_runtime::spawn(async move {
            p2p.close(&device_id).await;
        });
    }

    fn handle_p2p_message(self: Arc<Self>, device_id: String, data: Vec<u8>) {
        let Ok(raw) = serde_json::from_slice::<RemoteEnvelope>(&data) else {
            return;
        };
        let envelope = {
            let Ok(mut received) = self.receive_seq_by_device.lock() else {
                return;
            };
            self.service()
                .decrypt_envelope_if_needed(raw.with_device_id(device_id), &mut received)
                .ok()
                .flatten()
        };
        let Some(envelope) = envelope else {
            return;
        };
        self.handle_remote_envelope(envelope);
    }

    fn handle_pairing_request(&self, envelope: &RemoteEnvelope) {
        let settings = super::remote_settings_from_raw(&self.service().raw_settings());
        let Some(status) =
            remote_pending_pairing_summary(self.snapshot(), settings, &envelope.payload)
        else {
            return;
        };
        self.update_snapshot(status);
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

    fn handle_project_select(&self, envelope: &RemoteEnvelope) {
        let Some(project_id) = envelope.payload.get("projectId").and_then(Value::as_str) else {
            self.send_error(envelope, "Project id is required.");
            return;
        };
        match self.remote_project_scope(project_id) {
            Ok(scope) => {
                self.set_remote_project_scope(envelope.device_id.as_deref(), &scope.project_id);
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
                self.persist_remote_terminal_layout(&plan.scope.layout_key, &session_id, &plan.title);
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
                self.send_terminal_buffer(&session_id, envelope.device_id.as_deref(), 0);
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
        if let Err(error) = self.ensure_remote_terminal_started(session_id, envelope) {
            self.send_error(envelope, &error);
            return;
        }
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        self.apply_terminal_viewport(session_id, envelope);
        self.send_terminal_buffer(session_id, envelope.device_id.as_deref(), offset);
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
        if self.ensure_remote_terminal_started(session_id, envelope).is_err() {
            return;
        }
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        let _ = self.terminals.resize(session_id, cols, rows);
    }

    fn apply_terminal_viewport(&self, session_id: &str, envelope: &RemoteEnvelope) {
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
        let _ = self.terminals.resize(session_id, cols, rows);
    }

    fn handle_terminal_close(&self, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        match self.terminals.kill(session_id) {
            Ok(()) => {
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
        let projects = ProjectStore::new(self.support_dir.clone())
            .projects_snapshot()
            .into_iter()
            .map(|project| {
                json!({
                    "id": project.id,
                    "name": project.name,
                    "path": project.path,
                })
            })
            .collect::<Vec<_>>();
        self.send("project.list", device_id, None, json!({ "projects": projects }));
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

    fn send(&self, kind: &str, device_id: Option<&str>, session_id: Option<&str>, payload: Value) {
        self.send_relay(kind, device_id, session_id, payload);
    }

    fn send_terminal_data(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) {
        let inner = RemoteOutgoingEnvelope {
            kind: kind.to_string(),
            device_id: device_id.map(str::to_string),
            session_id: session_id.map(str::to_string),
            seq: None,
            payload: payload.clone(),
        };
        let Some(p2p) = self.p2p.lock().ok().and_then(|value| value.clone()) else {
            self.send_relay(kind, device_id, session_id, payload);
            return;
        };
        let Ok(data) = serde_json::to_vec(&inner) else {
            self.send_relay(kind, device_id, session_id, payload);
            return;
        };
        let lane = remote_p2p_lane(kind);
        let relay_text = self.outgoing_relay_text(kind, device_id, session_id, payload);
        let socket_tx = self.socket_tx.lock().ok().and_then(|value| value.clone());
        let device_id = device_id.map(str::to_string);
        crate::async_runtime::spawn(async move {
            if p2p.send(data, device_id.as_deref(), lane).await {
                return;
            }
            if let (Some(tx), Some(text)) = (socket_tx, relay_text) {
                let _ = tx.send(text);
            }
        });
    }

    fn send_error(&self, envelope: &RemoteEnvelope, message: &str) {
        self.send_relay(
            "error",
            envelope.device_id.as_deref(),
            envelope.session_id.as_deref(),
            json!({ "message": message }),
        );
    }

    fn outgoing_relay_text(
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
            .outgoing_relay_text(kind, device_id, session_id, payload, &mut send_seq)
    }

    fn update_device_online(&self, device_id: Option<&str>, online: bool) {
        let Some(device_id) = device_id else {
            return;
        };
        let mut status = self.snapshot();
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

    fn send_terminal_buffer(self: &Arc<Self>, session_id: &str, device_id: Option<&str>, offset: usize) {
        self.register_terminal_viewer(session_id, device_id);
        match self.terminals.snapshot(session_id) {
            Ok(data) => {
                let total_characters = data.chars().count();
                let clamped = offset.min(total_characters);
                let chunk = data.chars().skip(clamped).collect::<String>();
                self.send_terminal_data(
                    "terminal.output",
                    device_id,
                    Some(session_id),
                    json!({
                        "data": chunk,
                        "buffer": true,
                        "offset": clamped,
                        "bufferLength": total_characters,
                    }),
                );
            }
            Err(error) => self.send(
                "error",
                device_id,
                Some(session_id),
                json!({ "message": error.to_string() }),
            ),
        }
    }

    fn remote_terminal_payload(&self, session_id: &str) -> Option<Value> {
        self.remote_terminals()
            .into_iter()
            .find(|value| value.get("id").and_then(Value::as_str) == Some(session_id))
    }

    fn remote_terminals(&self) -> Vec<Value> {
        let mut terminals = self
            .terminals
            .list()
            .into_iter()
            .map(remote_terminal_snapshot_payload)
            .collect::<Vec<_>>();
        terminals.sort_by_key(remote_terminal_order_key);
        terminals
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
        self.persist_remote_terminal_layout(&plan.scope.layout_key, session_id, &plan.title);
        self.mark_terminal_event_subscription(session_id);
        self.send_terminal_list(envelope.device_id.as_deref());
        Ok(())
    }

    fn saved_remote_terminal_id(&self, layout_key: &str) -> Option<String> {
        let layout = TerminalLayoutService::new(self.support_dir.clone()).load(Some(layout_key));
        let active = layout.active_terminal_id.trim();
        if !active.is_empty()
            && (layout.top_panes.iter().any(|pane| pane.terminal_id == active)
                || layout.tabs.iter().any(|tab| tab.terminal_id == active))
        {
            return Some(active.to_string());
        }
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
    ) {
        if layout_key.trim().is_empty() {
            return;
        }
        let service = TerminalLayoutService::new(self.support_dir.clone());
        let layout = service.load(Some(layout_key));
        if layout.top_panes.iter().any(|pane| pane.terminal_id == terminal_id)
            || layout.tabs.iter().any(|tab| tab.terminal_id == terminal_id)
        {
            return;
        }
        let title = if title.trim().is_empty() {
            "Terminal"
        } else {
            title.trim()
        };
        let _ = service.save_from_gpui(
            layout_key,
            Vec::new(),
            terminal_id.to_string(),
            vec![TerminalPaneSummary {
                title: title.to_string(),
                terminal_id: terminal_id.to_string(),
            }],
        );
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
        let emit = Arc::new(move |event| runtime.handle_terminal_event(event));
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

    fn register_terminal_viewer(self: &Arc<Self>, session_id: &str, device_id: Option<&str>) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        if let Ok(mut viewers) = self.terminal_viewers_by_session.lock() {
            viewers
                .entry(session_id.to_string())
                .or_default()
                .insert(device_id.to_string());
        }
        self.ensure_terminal_event_subscription(session_id);
    }

    fn remove_terminal_viewer(&self, device_id: Option<&str>) {
        let Some(device_id) = device_id else {
            return;
        };
        if let Ok(mut viewers) = self.terminal_viewers_by_session.lock() {
            for session_viewers in viewers.values_mut() {
                session_viewers.remove(device_id);
            }
            viewers.retain(|_, value| !value.is_empty());
        }
    }

    fn handle_terminal_event(&self, event: TerminalEvent) {
        match event {
            TerminalEvent::Output {
                session_id, text, ..
            } => {
                let viewers = self
                    .terminal_viewers_by_session
                    .lock()
                    .ok()
                    .and_then(|viewers| viewers.get(&session_id).cloned())
                    .unwrap_or_default();
                if viewers.is_empty() {
                    return;
                }
                let buffer_length = self.terminals.buffer_characters(&session_id).unwrap_or(0);
                for device_id in viewers {
                    self.send_terminal_data(
                        "terminal.output",
                        Some(&device_id),
                        Some(&session_id),
                        json!({
                            "data": text,
                            "bufferLength": buffer_length,
                        }),
                    );
                }
            }
            TerminalEvent::Exit { session_id, .. } => {
                if let Ok(mut subscriptions) = self.terminal_event_subscriptions.lock() {
                    subscriptions.remove(&session_id);
                }
                if let Ok(mut viewers) = self.terminal_viewers_by_session.lock() {
                    viewers.remove(&session_id);
                }
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

pub(crate) fn remote_pending_pairing_summary(
    mut status: RemoteSummary,
    settings: RemoteSettings,
    payload: &Value,
) -> Option<RemoteSummary> {
    let pairing_id = payload
        .get("pairingId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if pairing_id.is_empty() {
        return None;
    }
    let device_name = payload
        .get("deviceName")
        .and_then(Value::as_str)
        .unwrap_or("Mobile Device")
        .to_string();
    let device_public_key = payload
        .get("devicePublicKey")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let active_pairing = status
        .pairing
        .as_ref()
        .filter(|pairing| pairing.pairing_id == pairing_id)
        .cloned();
    let pairing_code = payload
        .get("code")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| active_pairing.as_ref().map(|pairing| pairing.code.clone()))
        .unwrap_or_default();
    let pairing_secret = active_pairing
        .as_ref()
        .map(|pairing| pairing.secret.clone())
        .unwrap_or_default();
    let match_code =
        remote_pairing_match_code(&settings, &pairing_code, &pairing_secret, &device_public_key)
            .unwrap_or_else(|| pairing_code.clone());

    if active_pairing.is_some() {
        status.pairing = None;
    }
    if let Some(existing) = status
        .pending_pairing_list
        .iter_mut()
        .find(|pairing| pairing.id == pairing_id)
    {
        existing.device_name = device_name;
        existing.device_public_key = device_public_key;
        existing.code = match_code;
    } else {
        status.pending_pairing_list.push(RemotePendingPairing {
            id: pairing_id,
            device_name,
            device_public_key,
            code: match_code,
        });
    }
    status.pending_pairings = status.pending_pairing_list.len();
    status.message = "Confirm device pairing.".to_string();
    Some(status)
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
            .map_err(remote_error_message)
    })
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

pub(crate) fn remote_p2p_lane(kind: &str) -> RemoteP2PLane {
    match kind {
        "terminal.upload.ack" | "terminal.uploaded" => RemoteP2PLane::Upload,
        _ => RemoteP2PLane::Terminal,
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

pub(crate) fn remote_terminal_snapshot_payload(terminal: TerminalSessionSnapshot) -> Value {
    json!({
        "id": terminal.id,
        "title": terminal.title,
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
        let runtime = RemoteHostRuntime::new(support_dir.clone());

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

        runtime.persist_remote_terminal_layout(&layout_key, "terminal-mobile-b", "Mobile");

        let layout = TerminalLayoutService::new(support_dir.clone()).load(Some(&layout_key));
        assert_eq!(layout.active_terminal_id, "terminal-mobile-b");
        assert_eq!(layout.top_panes.len(), 1);
        assert_eq!(layout.top_panes[0].terminal_id, "terminal-mobile-b");

        fs::remove_dir_all(support_dir).ok();
    }
}
