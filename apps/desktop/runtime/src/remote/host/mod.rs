use super::crypto::remote_host_name;
use super::protocol::{
    REMOTE_AI_SESSION, REMOTE_AI_SESSION_RESULT, REMOTE_AI_STATE, REMOTE_AI_STATS,
    REMOTE_DEVICE_CONNECTED, REMOTE_DEVICE_DISCONNECTED, REMOTE_ERROR, REMOTE_FILE_BYTES_WRITTEN,
    REMOTE_FILE_COPIED, REMOTE_FILE_COPY, REMOTE_FILE_CREATE_DIRECTORY, REMOTE_FILE_DELETE,
    REMOTE_FILE_DELETED, REMOTE_FILE_DIRECTORY_CREATED, REMOTE_FILE_LIST, REMOTE_FILE_MOVE,
    REMOTE_FILE_MOVED, REMOTE_FILE_READ, REMOTE_FILE_RENAME, REMOTE_FILE_RENAMED,
    REMOTE_FILE_WRITE, REMOTE_FILE_WRITE_BYTES, REMOTE_FILE_WRITTEN, REMOTE_GIT_INVOKE,
    REMOTE_GIT_READ, REMOTE_GIT_STATUS, REMOTE_HOST_INFO, REMOTE_HOST_METRICS, REMOTE_HOST_OFFLINE,
    REMOTE_PAIRING_CONFIRMED, REMOTE_PAIRING_REJECTED, REMOTE_PROJECT_ADD, REMOTE_PROJECT_EDIT,
    REMOTE_PROJECT_LIST, REMOTE_PROJECT_REMOVE, REMOTE_PROJECT_SELECT, REMOTE_PROJECT_SELECTED,
    REMOTE_PROJECT_UPDATED, REMOTE_RESOURCE_AI_STATS, REMOTE_RESOURCE_GIT_STATUS,
    REMOTE_RESOURCE_PROJECTS, REMOTE_RESOURCE_SUBSCRIBE, REMOTE_RESOURCE_TERMINALS,
    REMOTE_RESOURCE_UNSUBSCRIBE, REMOTE_RESOURCE_WORKTREES, REMOTE_SSH_LIST,
    REMOTE_SSH_LIST_RESULT, REMOTE_SSH_REMOVE, REMOTE_SSH_UPSERT, REMOTE_TERMINAL_BUFFER_MAX_CHARS,
    REMOTE_TERMINAL_CLOSED, REMOTE_TERMINAL_CREATED, REMOTE_TERMINAL_INPUT_ACK,
    REMOTE_TERMINAL_LIST, REMOTE_TERMINAL_OUTPUT, REMOTE_TERMINAL_STATUS, REMOTE_TERMINAL_UPLOADED,
    REMOTE_TERMINAL_VIEWPORT_STATE, REMOTE_TRANSPORT_PING, REMOTE_TRANSPORT_PONG,
    REMOTE_WORKTREE_CREATE, REMOTE_WORKTREE_DELETE, REMOTE_WORKTREE_LIST, REMOTE_WORKTREE_MERGE,
    REMOTE_WORKTREE_REMOVE, REMOTE_WORKTREE_SELECT, REMOTE_WORKTREE_UPDATED,
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
use super::{RemoteService, crypto, relay, remote_settings_from_raw, types};
use crate::ai_history_indexer::{AIHistoryIndexer, AIHistoryProjectState};
use crate::ai_history_normalized::AIHistoryProjectRequest;
use crate::project_store::{ProjectCreateRequest, ProjectStore, ProjectUpdateRequest};
use crate::terminal_layout::{
    TerminalLayoutService, TerminalPaneSummary, terminal_layout_storage_key,
};
use crate::terminal_pty::{
    TerminalEvent, TerminalManager, TerminalPtyConfig, TerminalSessionSnapshot,
    TerminalViewportState, terminal_viewport_remote_owner,
};
use crate::worktree::{
    WorktreeCreateRequest, WorktreeMergeRequest, WorktreeRemoveRequest, WorktreeService,
};
use codux_remote_transport::{RemoteTransportUpload, WebTunnelTcpConnectRequest};
use codux_runtime_core::{
    ai_stats as runtime_ai_stats, file as runtime_file, git as runtime_git, host as runtime_host,
    project as runtime_project, subscription::RuntimeSubscriptionRouter,
    terminal as runtime_terminal, upload as runtime_upload, worktree as runtime_worktree,
};
use codux_runtime_live::{
    host_metrics::sample_host_metrics,
    remote_terminal_dispatch::{
        RemoteTerminalDispatch, TerminalMessage, apply_terminal_osc_color_env,
        finish_terminal_create_viewer_lifecycle, is_terminal_kind,
        prepare_terminal_create_lifecycle,
    },
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
const REMOTE_TERMINAL_STALE_OUTPUT_SEQ_LAG: TerminalSequence = 8;
/// How often the live-output flush re-asserts the authoritative viewport owner
/// to non-owner viewers (self-healing handoff, design 3). One in every N flushes;
/// at the 32ms batch cadence that is ~4/s during continuous output, 0 when idle.
const REMOTE_TERMINAL_OWNER_REASSERT_EVERY: i64 = 8;

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

struct RemoteTerminalLayoutScope {
    layout_key: String,
    project_id: String,
    worktree_id: String,
    layout_order: usize,
}

pub(crate) struct RemoteTerminalOutputBatch {
    data: String,
    buffer_length: usize,
    viewers: HashSet<String>,
}

pub(crate) struct RemoteTerminalBufferBaseline {
    data: String,
    start_offset: usize,
    total_characters: usize,
    output_seq: TerminalSequence,
    created_at: Instant,
}

#[derive(Clone, Copy, Debug)]
struct BaselineViewport {
    cols: u16,
    rows: u16,
}

#[derive(Clone, Debug)]
struct TerminalBaselineOptions {
    max_chars: usize,
    chunk_chars: Option<usize>,
    request_id: Option<String>,
    tail: bool,
    viewport: Option<BaselineViewport>,
}

pub struct RemoteHostRuntime {
    pub(crate) runtime_instance_id: String,
    pub(crate) support_dir: PathBuf,
    pub(crate) ai_history: AIHistoryIndexer,
    pub(crate) ai_current_sessions:
        Option<Arc<dyn runtime_ai_stats::RemoteAICurrentSessionProvider>>,
    pub(crate) terminals: Arc<TerminalManager>,
    pub(crate) resource_subscriptions: RuntimeSubscriptionRouter,
    // Arc so the viewport-owner resolver (registered on the TerminalManager) can
    // read the live viewer set when a remote lease expires.
    pub(crate) terminal_subscriptions: Arc<RemoteTerminalSubscriptions>,
    pub(crate) terminal_output_seq_by_session: Mutex<HashMap<String, TerminalSequence>>,
    pub(crate) terminal_output_ack_by_viewer: Mutex<HashMap<(String, String), TerminalSequence>>,
    pub(crate) terminal_output_batches: Mutex<HashMap<String, RemoteTerminalOutputBatch>>,
    pub(crate) terminal_buffer_baselines: Mutex<HashMap<String, RemoteTerminalBufferBaseline>>,
    pub(crate) remote_project_scope_by_device: Mutex<HashMap<String, String>>,
    pub(crate) terminal_event_subscriptions: Mutex<HashSet<String>>,
    pub(crate) transport: Mutex<Option<Arc<dyn RemoteTransport>>>,
    pub(crate) transport_start_lock: tokio::sync::Mutex<()>,
    pub(crate) active_pairing: Mutex<Option<RemotePairingInfo>>,
    pub(crate) pending_pairings: Mutex<HashMap<String, RemoteTransportPairingRequest>>,
    pub(crate) events: Mutex<VecDeque<RemoteHostEvent>>,
    pub(crate) snapshot: Mutex<RemoteSummary>,
    pub(crate) connection_generation: AtomicU64,
    // Sequence for terminal layout changes emitted by remote clients.
    pub(crate) remote_terminal_layout_generation: AtomicU64,
    pub(crate) resolved_relay: Mutex<Option<String>>,
    pub(crate) send_seq_by_device: Mutex<HashMap<String, i64>>,
    pub(crate) receive_seq_by_device: Mutex<HashMap<String, RemoteSequenceGuard>>,
    // Devices currently watching a project's `ai.stats` (project_id -> device_id
    // -> runtime session scope). A device registers by requesting `ai.stats` and
    // watches at most one project at a time. We re-push fresh stats to these
    // devices when the live AI runtime changes (so remote views tick like the
    // desktop's local view) and when a cold-on-request index finishes refreshing.
    pub(crate) ai_stats_watchers: Mutex<HashMap<String, HashMap<String, String>>>,
    // Host theme's OSC 10/11 payloads; the fallback for remote spawn paths whose
    // envelope carries no viewer colors (e.g. lazy respawn after a host restart).
    pub(crate) terminal_osc_colors: Mutex<Option<(String, String)>>,
}

mod ai_stats;
mod files;
mod git;
mod pairing;
mod projects;
mod send;
mod ssh;
mod terminal_buffer;
mod terminal_dispatch;
mod terminal_state;
#[cfg(test)]
mod tests;
mod transport;
mod worktrees;

#[cfg(test)]
pub(crate) use files::{remote_file_list, remote_file_read, remote_file_rename, remote_file_write};
#[cfg(test)]
pub(crate) use git::remote_git_status_payload;
#[cfg(test)]
pub(crate) use terminal_buffer::{remote_terminal_order_key, remote_terminal_snapshot_payload};
#[cfg(test)]
pub(crate) use terminal_dispatch::{
    remote_terminal_upload_directory, sanitized_remote_upload_name, terminal_upload_path_input,
    unique_remote_upload_path,
};

use projects::*;
use terminal_dispatch::DesktopTerminalCtx;
use worktrees::*;

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
            terminal_output_ack_by_viewer: Mutex::new(HashMap::new()),
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
            terminal_osc_colors: Mutex::new(None),
        }
    }

    pub fn set_terminal_osc_colors(&self, foreground: String, background: String) {
        if let Ok(mut colors) = self.terminal_osc_colors.lock() {
            *colors = Some((foreground, background));
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

    /// Push the project list to subscribed controllers after a desktop-initiated
    /// project mutation (create / rename / reorder / close), so a controller's
    /// list updates live instead of only on reconnect or pull-to-refresh.
    pub fn broadcast_project_list_change(&self) {
        self.broadcast_project_list(None);
    }

    /// Push refreshed git status to controllers subscribed to this project after
    /// a desktop-initiated git mutation (stage/commit/discard/branch/...), so the
    /// controller's git view reconciles instead of showing stale status. Mirrors
    /// the `git.status` request reply; a no-op when nothing is subscribed.
    pub fn broadcast_git_status_change(&self, project_id: &str, project_path: &str) {
        if project_id.trim().is_empty() || project_path.trim().is_empty() {
            return;
        }
        let summary = crate::git::GitService::status(project_path);
        self.broadcast_resource_payload(
            REMOTE_GIT_STATUS,
            REMOTE_RESOURCE_GIT_STATUS,
            None,
            Some(project_id),
            None,
            git::remote_git_status_payload(
                project_id.to_string(),
                project_path.to_string(),
                summary,
            ),
        );
    }

    /// Tell controllers an AI history session changed on the desktop (e.g. the
    /// user deleted one). We broadcast the lean `remove` signal -- NOT a session
    /// list -- so each controller re-requests its OWN project/worktree scope and
    /// drops the stale row, instead of adopting the desktop's list (which may be
    /// a different project or worktree). Mirrors the reply a controller gets after
    /// its own `ai.session` remove, which the mobile client already handles by
    /// re-listing.
    pub fn broadcast_ai_session_changed(&self) {
        self.send(
            REMOTE_AI_SESSION_RESULT,
            None,
            None,
            json!({ "op": "remove", "result": Value::Null }),
        );
    }

    /// Push one live terminal status event (loading/waiting/completed dots) to
    /// connected controllers. These are transient one-shots — no resource
    /// versioning; a viewer that missed one self-heals via the next event or
    /// its own stale sweep.
    pub fn broadcast_terminal_status(&self, payload: Value) {
        self.send(REMOTE_TERMINAL_STATUS, None, None, payload);
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

    fn handle_remote_envelope(self: &Arc<Self>, envelope: RemoteEnvelope) {
        match envelope.kind.as_str() {
            REMOTE_HOST_INFO => self.send_host_info(envelope.device_id.as_deref()),
            REMOTE_HOST_METRICS => self.handle_host_metrics(envelope.device_id.as_deref()),
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
                    files::remote_file_list(path, purpose),
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

    fn handle_host_metrics(self: &Arc<Self>, device_id: Option<&str>) {
        let runtime = Arc::clone(self);
        let device_id = device_id.map(str::to_string);
        crate::async_runtime::spawn(async move {
            match crate::async_runtime::spawn_blocking(sample_host_metrics).await {
                Ok(metrics) => {
                    let payload = serde_json::to_value(metrics).unwrap_or(Value::Null);
                    runtime.send(REMOTE_HOST_METRICS, device_id.as_deref(), None, payload);
                }
                Err(error) => {
                    runtime.send(
                        REMOTE_ERROR,
                        device_id.as_deref(),
                        None,
                        json!({ "message": format!("Unable to sample host metrics: {error}") }),
                    );
                }
            }
        });
    }
}
