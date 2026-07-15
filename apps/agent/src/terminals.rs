//! Terminal domain for the headless host: spawn real PTYs with the same
//! `TerminalManager` the desktop host uses, so AI runtime tracking and terminal
//! protocol behavior stay aligned.
//!
//! Multi-client: several devices can watch the same terminal at once. The
//! viewer set, baseline catch-up, and viewport lease all reuse the shared crate
//! pieces (`RemoteTerminalSubscriptions`, atomic baseline snapshots +
//! `terminal_buffer_payloads`,
//! the `TerminalManager` lease + viewport-owner resolver) rather than a private
//! copy of the desktop host's batching/baseline machinery.

use codux_protocol::{
    REMOTE_ERROR, REMOTE_RESOURCE_SUBSCRIBE, REMOTE_RESOURCE_TERMINALS,
    REMOTE_RESOURCE_UNSUBSCRIBE, REMOTE_TERMINAL_BUFFER_MAX_CHARS, REMOTE_TERMINAL_CLOSED,
    REMOTE_TERMINAL_CREATED, REMOTE_TERMINAL_INPUT_ACK, REMOTE_TERMINAL_LIST,
    REMOTE_TERMINAL_OUTPUT, REMOTE_TERMINAL_VIEWPORT_STATE, RemoteTerminalBufferWindow,
    RemoteTerminalSubscriptions, terminal_buffer_payloads, terminal_live_output_payload,
};
use codux_remote_transport::RemoteTransport;
use codux_runtime_core::terminal::terminal_snapshot_payload;
use codux_runtime_live::remote_terminal_dispatch::{
    self, RemoteTerminalDispatch, TerminalMessage, apply_terminal_osc_color_env,
    finish_terminal_create_viewer_lifecycle, prepare_terminal_create_lifecycle,
    rollback_terminal_create_viewer_lifecycle,
};
use codux_runtime_live::terminal_pty::{TerminalManager, TerminalPtyConfig};
use codux_runtime_live::terminal_pty::{
    TerminalViewportState, terminal_viewport_local_owner, terminal_viewport_remote_owner,
};
use codux_terminal_core::TerminalEvent;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type TransportSlot = Arc<Mutex<Option<Arc<dyn RemoteTransport>>>>;
const STALE_OUTPUT_SEQ_LAG: i64 = 8;

#[derive(Clone, Copy, Debug)]
struct BaselineViewport {
    cols: u16,
    rows: u16,
}

/// Shared multi-client state for headless terminals: which devices view each
/// session (`subscriptions`) and the per-session output sequence (`output_seq`,
/// shared across viewers so every device sees the same `outputSeq`).
#[derive(Clone, Default)]
pub struct TerminalFanout {
    subscriptions: Arc<RemoteTerminalSubscriptions>,
    project_id_by_session: Arc<Mutex<HashMap<String, String>>>,
    output_seq: Arc<Mutex<HashMap<String, i64>>>,
    output_ack: Arc<Mutex<HashMap<(String, String), i64>>>,
}

impl TerminalFanout {
    pub fn new() -> Self {
        Self::default()
    }

    fn next_seq(&self, session_id: &str) -> i64 {
        let mut map = self
            .output_seq
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let next = map.get(session_id).copied().unwrap_or(0) + 1;
        map.insert(session_id.to_string(), next);
        next
    }

    fn current_seq(&self, session_id: &str) -> i64 {
        self.output_seq
            .lock()
            .map(|map| map.get(session_id).copied().unwrap_or(0))
            .unwrap_or(0)
    }

    fn add_viewer(&self, session_id: &str, device_id: &str) {
        self.subscriptions.add_session_viewer(session_id, device_id);
    }

    fn add_project_subscriber(&self, project_id: &str, device_id: &str) {
        self.subscriptions
            .add_project_subscriber(project_id, device_id);
    }

    fn remove_project_subscriber(&self, project_id: &str, device_id: &str) {
        self.subscriptions
            .remove_project_subscriber(project_id, device_id);
    }

    fn project_subscribers(&self, project_id: &str) -> Vec<String> {
        self.subscriptions
            .project_subscribers(project_id)
            .into_iter()
            .collect()
    }

    fn set_session_project(&self, session_id: &str, project_id: &str) {
        let session_id = session_id.trim();
        let project_id = project_id.trim();
        if session_id.is_empty() || project_id.is_empty() {
            return;
        }
        if let Ok(mut projects) = self.project_id_by_session.lock() {
            projects.insert(session_id.to_string(), project_id.to_string());
        }
    }

    fn project_for_session(&self, session_id: &str) -> Option<String> {
        self.project_id_by_session
            .lock()
            .ok()
            .and_then(|projects| projects.get(session_id).cloned())
    }

    fn remove_viewer(&self, session_id: &str, device_id: &str) {
        self.subscriptions
            .remove_session_viewer(session_id, device_id);
        self.clear_ack(session_id, device_id);
    }

    pub(crate) fn viewers(&self, session_id: &str) -> Vec<String> {
        self.subscriptions
            .viewers_for_session(session_id)
            .into_iter()
            .collect()
    }

    fn record_ack(&self, session_id: &str, device_id: Option<&str>, output_seq: Option<i64>) {
        let Some(device_id) = device_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };
        let Some(output_seq) = output_seq else {
            return;
        };
        if let Ok(mut acks) = self.output_ack.lock() {
            let key = (session_id.to_string(), device_id.to_string());
            let current = acks.get(&key).copied().unwrap_or(0);
            if output_seq > current {
                acks.insert(key, output_seq);
            }
        }
    }

    fn ack_seq(&self, session_id: &str, device_id: Option<&str>) -> i64 {
        let Some(device_id) = device_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return 0;
        };
        self.output_ack
            .lock()
            .ok()
            .and_then(|acks| {
                acks.get(&(session_id.to_string(), device_id.to_string()))
                    .copied()
            })
            .unwrap_or(0)
    }

    fn is_stale(&self, session_id: &str, device_id: Option<&str>) -> bool {
        let current = self.current_seq(session_id);
        current > 0
            && current.saturating_sub(self.ack_seq(session_id, device_id)) > STALE_OUTPUT_SEQ_LAG
    }

    fn clear_ack(&self, session_id: &str, device_id: &str) {
        if let Ok(mut acks) = self.output_ack.lock() {
            acks.remove(&(session_id.to_string(), device_id.to_string()));
        }
    }

    pub(crate) fn remove_device(&self, device_id: &str) {
        self.subscriptions.remove_device(device_id);
        if let Ok(mut acks) = self.output_ack.lock() {
            acks.retain(|(_, device), _| device != device_id);
        }
    }

    pub(crate) fn remove_project(&self, project_id: &str) {
        self.subscriptions.remove_project(project_id);
    }

    fn clear_session(&self, session_id: &str) {
        self.subscriptions.remove_session(session_id);
        if let Ok(mut projects) = self.project_id_by_session.lock() {
            projects.remove(session_id);
        }
        if let Ok(mut output_seq) = self.output_seq.lock() {
            output_seq.remove(session_id);
        }
        if let Ok(mut acks) = self.output_ack.lock() {
            acks.retain(|(session, _), _| session != session_id);
        }
    }
}

/// True for every message this module routes: the shared terminal-protocol set
/// ([`remote_terminal_dispatch::is_terminal_kind`]) plus the resource-level
/// subscribe/unsubscribe the headless host folds into its terminal handling.
pub fn is_terminal_kind(kind: &str) -> bool {
    remote_terminal_dispatch::is_terminal_kind(kind)
        || matches!(
            kind,
            REMOTE_RESOURCE_SUBSCRIBE | REMOTE_RESOURCE_UNSUBSCRIBE
        )
}

fn send(
    transport: &TransportSlot,
    device_id: Option<&str>,
    envelope: Value,
    terminal_stream: bool,
) {
    let Ok(bytes) = serde_json::to_vec(&envelope) else {
        return;
    };
    if let Ok(guard) = transport.lock()
        && let Some(t) = guard.as_ref()
    {
        if terminal_stream {
            t.send_terminal(bytes, device_id);
        } else {
            t.send(bytes, device_id);
        }
    }
}

/// Serialize one frame and fan it out: unicast to each viewer (the transport
/// routes by the device arg, not the envelope's `deviceId`). Output with no
/// subscribers is discarded so terminal contents never cross device boundaries.
fn fanout(transport: &TransportSlot, viewers: &[String], envelope: Value, terminal_stream: bool) {
    let Ok(bytes) = serde_json::to_vec(&envelope) else {
        return;
    };
    let Ok(guard) = transport.lock() else {
        return;
    };
    let Some(t) = guard.as_ref() else {
        return;
    };
    if viewers.is_empty() {
        return;
    }
    for device in viewers {
        let copy = bytes.clone();
        if terminal_stream {
            t.send_terminal(copy, Some(device));
        } else {
            t.send(copy, Some(device));
        }
    }
}

fn send_to_viewers(
    transport: &TransportSlot,
    viewers: &[String],
    envelope: Value,
    terminal_stream: bool,
) {
    if viewers.is_empty() {
        return;
    }
    let Ok(bytes) = serde_json::to_vec(&envelope) else {
        return;
    };
    let Ok(guard) = transport.lock() else {
        return;
    };
    let Some(transport) = guard.as_ref() else {
        return;
    };
    for device_id in viewers {
        if terminal_stream {
            transport.send_terminal(bytes.clone(), Some(device_id));
        } else {
            transport.send(bytes.clone(), Some(device_id));
        }
    }
}

fn reply(
    transport: &TransportSlot,
    device_id: Option<&str>,
    session_id: Option<&str>,
    request_id: Option<&str>,
    kind: &str,
    payload: Value,
) {
    let mut envelope = json!({ "type": kind, "payload": payload });
    if let Some(device_id) = device_id {
        envelope["deviceId"] = json!(device_id);
    }
    if let Some(session_id) = session_id {
        envelope["sessionId"] = json!(session_id);
    }
    if let Some(request_id) = request_id {
        envelope["requestId"] = json!(request_id);
    }
    send(transport, device_id, envelope, false);
}

fn list_payload(driver: &TerminalManager, fanout_state: &TerminalFanout) -> Value {
    let terminals = driver
        .list()
        .into_iter()
        .filter(|terminal| terminal.is_running)
        .map(|terminal| terminal_payload(terminal, fanout_state))
        .collect::<Vec<_>>();
    json!({ "terminals": terminals })
}

fn terminal_payload(
    terminal: codux_terminal_core::TerminalSessionSnapshot,
    fanout_state: &TerminalFanout,
) -> Value {
    let session_id = terminal.id.clone();
    let worktree_id = terminal.worktree_id.clone();
    let mut payload = terminal_snapshot_payload(terminal);
    if let Some(project_id) = fanout_state.project_for_session(&session_id) {
        payload["projectId"] = json!(project_id);
    }
    if let Some(worktree_id) = worktree_id.filter(|value| !value.trim().is_empty()) {
        payload["worktreeId"] = json!(worktree_id);
    }
    payload
}

fn baseline_viewport(payload: &Value) -> Option<BaselineViewport> {
    let cols = payload
        .get("viewportCols")
        .and_then(Value::as_u64)
        .map(|value| value as u16)
        .filter(|value| *value > 0)?;
    let rows = payload
        .get("viewportRows")
        .and_then(Value::as_u64)
        .map(|value| value as u16)
        .filter(|value| *value > 0)?;
    Some(BaselineViewport { cols, rows })
}

fn apply_baseline_viewport(
    driver: &TerminalManager,
    session_id: &str,
    device_id: &str,
    viewport: Option<BaselineViewport>,
) -> bool {
    let Some(viewport) = viewport else {
        return false;
    };
    let owner = terminal_viewport_remote_owner(device_id);
    let Ok(state) = driver.claim_viewport_auto(session_id, &owner) else {
        return false;
    };
    if state.owner != owner {
        return false;
    }
    driver.touch_viewport_lease(session_id, &owner);
    driver
        .resize_viewport(session_id, &owner, viewport.cols, viewport.rows)
        .ok()
        .flatten()
        .is_some()
}

fn viewport_state_payload(
    fanout_state: &TerminalFanout,
    session_id: &str,
    device_id: Option<&str>,
    state: &TerminalViewportState,
) -> Value {
    json!({
        "sessionId": session_id,
        "owner": state.owner,
        "cols": state.cols,
        "rows": state.rows,
        "generation": state.generation,
        "staleOutput": device_id.is_some() && fanout_state.is_stale(session_id, device_id),
        "outputSeq": fanout_state.current_seq(session_id),
    })
}

fn handle_agent_terminal_event(
    driver: &TerminalManager,
    transport: &TransportSlot,
    fanout_state: &TerminalFanout,
    project_id: Option<&str>,
    event: TerminalEvent,
) -> bool {
    let keep_subscription = !matches!(event, TerminalEvent::Exit { .. });
    let session_id = match &event {
        TerminalEvent::Output { session_id, .. }
        | TerminalEvent::Exit { session_id, .. }
        | TerminalEvent::Error { session_id, .. }
        | TerminalEvent::Viewport { session_id, .. } => session_id,
    };
    if let Some(project_id) = project_id {
        fanout_state.set_session_project(session_id, project_id);
    }
    match event {
        TerminalEvent::Output {
            session_id,
            text,
            buffer_length,
            buffer_end,
            ..
        } => {
            if text.is_empty() {
                return true;
            }
            let next = fanout_state.next_seq(&session_id);
            let envelope = json!({
                "type": REMOTE_TERMINAL_OUTPUT,
                "sessionId": session_id,
                "payload": terminal_live_output_payload(text, buffer_length, buffer_end, next),
            });
            fanout(
                transport,
                &fanout_state.viewers(&session_id),
                envelope,
                true,
            );
        }
        TerminalEvent::Viewport {
            session_id,
            owner,
            cols,
            rows,
            generation,
        } => {
            let state = TerminalViewportState {
                owner,
                cols,
                rows,
                generation,
                owner_label: None,
            };
            for viewer in fanout_state.viewers(&session_id) {
                let mut envelope = json!({
                    "type": REMOTE_TERMINAL_VIEWPORT_STATE,
                    "sessionId": session_id,
                    "payload": viewport_state_payload(
                        fanout_state,
                        &session_id,
                        Some(&viewer),
                        &state,
                    ),
                });
                envelope["deviceId"] = json!(viewer);
                send(transport, Some(&viewer), envelope, false);
            }
        }
        TerminalEvent::Exit {
            session_id,
            exit_code,
        } => {
            let viewers = fanout_state.viewers(&session_id);
            let project_subscribers = fanout_state
                .project_for_session(&session_id)
                .as_deref()
                .map(|project_id| fanout_state.project_subscribers(project_id))
                .unwrap_or_default();
            send_to_viewers(
                transport,
                &viewers,
                json!({
                    "type": REMOTE_TERMINAL_CLOSED,
                    "sessionId": session_id,
                    "payload": {
                        "sessionId": session_id,
                        "exitCode": exit_code,
                    },
                }),
                false,
            );
            fanout_state.clear_session(&session_id);
            send_to_viewers(
                transport,
                &project_subscribers,
                json!({
                    "type": REMOTE_TERMINAL_LIST,
                    "payload": list_payload(driver, fanout_state),
                }),
                false,
            );
        }
        TerminalEvent::Error {
            session_id,
            message,
        } => {
            let viewers = fanout_state.viewers(&session_id);
            send_to_viewers(
                transport,
                &viewers,
                json!({
                    "type": REMOTE_ERROR,
                    "sessionId": session_id,
                    "payload": { "message": message },
                }),
                false,
            );
        }
    }
    keep_subscription
}

/// Send a session's catch-up baseline to a newly-subscribed device, reusing the
/// shared `snapshot_tail` + `terminal_buffer_payloads` helpers so the wire shape
/// matches the desktop host exactly.
fn send_terminal_baseline(
    driver: &TerminalManager,
    transport: &TransportSlot,
    fanout_state: &TerminalFanout,
    device_id: &str,
    session_id: &str,
    payload: &Value,
) {
    let max_chars = payload
        .get("maxChars")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(REMOTE_TERMINAL_BUFFER_MAX_CHARS);
    let chunk_chars = payload
        .get("chunkChars")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let request_id = payload
        .get("requestId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let viewport = baseline_viewport(payload);
    let use_viewport = apply_baseline_viewport(driver, session_id, device_id, viewport);
    let output_seq = fanout_state.current_seq(session_id);
    let viewport_max_lines = use_viewport.then(|| {
        viewport
            .map(|viewport| viewport.rows.max(8) as usize)
            .unwrap_or(8)
    });
    let baseline = driver
        .baseline_snapshot(session_id, max_chars, viewport_max_lines)
        .ok();
    let baseline_failed = baseline.is_none();
    let (data, offset, buffer_length, buffer_end, screen_data, screen_wrapped_rows) = baseline
        .map(|baseline| {
            let (screen_data, screen_wrapped_rows) = (!baseline.screen.data.is_empty())
                .then(|| {
                    (
                        Some(baseline.screen.data),
                        Some(baseline.screen.wrapped_rows),
                    )
                })
                .unwrap_or_default();
            (
                baseline.data,
                baseline.offset,
                baseline.buffer_length,
                baseline.buffer_end,
                screen_data,
                screen_wrapped_rows,
            )
        })
        .unwrap_or_default();
    let window = RemoteTerminalBufferWindow {
        data,
        screen_data,
        screen_wrapped_rows,
        offset,
        total_characters: buffer_length,
        buffer_end: Some(buffer_end),
        truncated: false,
        output_seq: Some(output_seq),
        request_id,
        tail: true,
        has_previous: offset > 0,
        baseline_failed,
    };
    for payload in terminal_buffer_payloads(&window, output_seq, chunk_chars) {
        let mut envelope =
            json!({ "type": REMOTE_TERMINAL_OUTPUT, "sessionId": session_id, "payload": payload });
        envelope["deviceId"] = json!(device_id);
        send(transport, Some(device_id), envelope, true);
    }
}

/// Adapts the headless agent to the shared remote-terminal router
/// ([`RemoteTerminalDispatch`]). It borrows the agent's PTY manager, transport
/// slot and multi-client fan-out state. The arms that are identical to the
/// desktop host -- signal, output-ack, viewport release/scroll -- are served by
/// the trait's default methods; the arms below stay agent-shaped (a leaner
/// per-session model with no terminal-layout persistence or output batching).
struct AgentTerminalCtx<'a> {
    driver: &'a Arc<TerminalManager>,
    transport: &'a TransportSlot,
    fanout: &'a TerminalFanout,
}

impl AgentTerminalCtx<'_> {
    fn add_viewer(&self, session_id: &str, device_id: &str) {
        self.fanout.add_viewer(session_id, device_id);
        self.driver.restore_remote_screen_scrollback(session_id);
    }

    fn remove_viewer(&self, session_id: &str, device_id: &str) {
        self.fanout.remove_viewer(session_id, device_id);
        if self.fanout.viewers(session_id).is_empty() {
            self.driver.shrink_remote_screen_scrollback(session_id);
        }
    }

    /// Register a viewer for `session_id` and, when asked, push its catch-up
    /// baseline. Shared by `terminal.subscribe`, `terminal.buffer` and the
    /// resource-level `resource.subscribe` so every subscription path behaves
    /// identically.
    fn subscribe_session(
        &self,
        session_id: &str,
        device_id: &str,
        baseline: bool,
        payload: &Value,
    ) {
        self.add_viewer(session_id, device_id);
        if baseline {
            send_terminal_baseline(
                self.driver,
                self.transport,
                self.fanout,
                device_id,
                session_id,
                payload,
            );
        }
    }

    /// `resource.subscribe` for the terminals resource: the headless host folds
    /// it into terminal handling (the desktop routes it through a generic
    /// resource router instead).
    fn resource_subscribe(&self, msg: &TerminalMessage) {
        if msg.payload.get("resource").and_then(Value::as_str) != Some(REMOTE_RESOURCE_TERMINALS) {
            return;
        }
        let baseline = msg
            .payload
            .get("baseline")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let (Some(project_id), Some(device_id)) = (
            msg.payload.get("projectId").and_then(Value::as_str),
            msg.device_id,
        ) {
            self.fanout.add_project_subscriber(project_id, device_id);
            reply(
                self.transport,
                Some(device_id),
                None,
                msg.request_id,
                REMOTE_TERMINAL_LIST,
                list_payload(self.driver, self.fanout),
            );
        } else if let (Some(id), Some(device_id)) = (msg.session_id, msg.device_id) {
            self.subscribe_session(id, device_id, baseline, msg.payload);
        }
    }

    fn resource_unsubscribe(&self, msg: &TerminalMessage) {
        if msg.payload.get("resource").and_then(Value::as_str) != Some(REMOTE_RESOURCE_TERMINALS) {
            return;
        }
        if let (Some(project_id), Some(device_id)) = (
            msg.payload.get("projectId").and_then(Value::as_str),
            msg.device_id,
        ) {
            self.fanout.remove_project_subscriber(project_id, device_id);
        } else if let (Some(id), Some(device_id)) = (msg.session_id, msg.device_id) {
            self.remove_viewer(id, device_id);
        }
    }
}

impl RemoteTerminalDispatch for AgentTerminalCtx<'_> {
    fn terminal_manager(&self) -> &TerminalManager {
        self.driver.as_ref()
    }

    fn reply_terminal(
        &self,
        device_id: Option<&str>,
        session_id: Option<&str>,
        request_id: Option<&str>,
        kind: &str,
        payload: Value,
    ) {
        reply(
            self.transport,
            device_id,
            session_id,
            request_id,
            kind,
            payload,
        );
    }

    fn handle_terminal_list_msg(&self, msg: &TerminalMessage) {
        reply(
            self.transport,
            msg.device_id,
            None,
            msg.request_id,
            REMOTE_TERMINAL_LIST,
            list_payload(self.driver, self.fanout),
        );
    }

    fn handle_terminal_subscribe_msg(&self, msg: &TerminalMessage) {
        // Parity with the desktop's `terminal.subscribe`: register the viewer and
        // (by default) serve a catch-up baseline. The agent keys viewers by
        // session, so it honors a session target directly.
        let baseline = msg
            .payload
            .get("baseline")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if let (Some(id), Some(device_id)) = (msg.session_id, msg.device_id) {
            self.subscribe_session(id, device_id, baseline, msg.payload);
        }
    }

    fn handle_terminal_unsubscribe_msg(&self, msg: &TerminalMessage) {
        if let (Some(id), Some(device_id)) = (msg.session_id, msg.device_id) {
            self.remove_viewer(id, device_id);
        }
    }

    fn handle_terminal_buffer_msg(&self, msg: &TerminalMessage) {
        // Parity with the desktop's `terminal.buffer`: register the viewer and
        // serve the session's tail baseline (the agent keeps a single tail
        // window rather than the desktop's offset-addressable cache).
        if let (Some(id), Some(device_id)) = (msg.session_id, msg.device_id) {
            self.subscribe_session(id, device_id, true, msg.payload);
        }
    }

    fn handle_terminal_create_msg(&self, msg: &TerminalMessage) {
        let payload = msg.payload;
        let device_id = msg.device_id;
        let project_id = payload
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let worktree_id = payload
            .get("worktreeId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| project_id.clone());
        let mut config = TerminalPtyConfig {
            cwd: payload
                .get("cwd")
                .or_else(|| payload.get("projectPath"))
                .and_then(Value::as_str)
                .map(str::to_string),
            command: payload
                .get("command")
                .and_then(Value::as_str)
                .map(str::to_string),
            cols: payload
                .get("cols")
                .and_then(Value::as_u64)
                .map(|v| v as u16),
            rows: payload
                .get("rows")
                .and_then(Value::as_u64)
                .map(|v| v as u16),
            root_project_id: project_id.clone(),
            project_id: worktree_id.clone(),
            worktree_id,
            project_name: payload
                .get("projectName")
                .and_then(Value::as_str)
                .map(str::to_string),
            terminal_id: payload
                .get("terminalId")
                .or_else(|| payload.get("sessionId"))
                .and_then(Value::as_str)
                .map(str::to_string),
            slot_id: payload
                .get("slotId")
                .and_then(Value::as_str)
                .map(str::to_string),
            session_key: payload
                .get("sessionKey")
                .and_then(Value::as_str)
                .map(str::to_string),
            title: payload
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string),
            tool: payload
                .get("tool")
                .and_then(Value::as_str)
                .map(str::to_string),
            support_dir: Some(crate::projects::agent_data_dir()),
            runtime_root: Some(codux_runtime_live::runtime_paths::runtime_root_dir()),
            tool_permissions_file: Some(
                crate::projects::agent_data_dir().join("tool_permissions.json"),
            ),
            memory_workspace_root: Some(crate::projects::agent_data_dir().join("memory")),
            memory_prompt_file: Some(
                crate::projects::agent_data_dir()
                    .join("memory")
                    .join("AI_MEMORY.md"),
            ),
            memory_index_file: Some(
                crate::projects::agent_data_dir()
                    .join("memory")
                    .join("memory-index.json"),
            ),
            ..Default::default()
        };
        apply_terminal_osc_color_env(&mut config, payload);
        if config.terminal_id.is_none() {
            config.terminal_id = Some(uuid::Uuid::new_v4().to_string());
        }
        let lifecycle = prepare_terminal_create_lifecycle(
            self.driver,
            &config,
            device_id,
            |session_id, device_id| self.add_viewer(session_id, device_id),
        );
        if let (Some(session_id), Some(project_id)) = (
            lifecycle.requested_terminal_id.as_deref(),
            project_id.as_deref(),
        ) {
            self.fanout.set_session_project(session_id, project_id);
        }
        // Stream this session's output to ALL of its viewers (fan-out), and
        // forward viewport-state changes (lease claim/handoff) too.
        let driver_for_emit = Arc::clone(self.driver);
        let transport_for_emit = Arc::clone(self.transport);
        let fanout_for_emit = self.fanout.clone();
        let project_id_for_emit = project_id.clone();
        let event_order = Arc::new(Mutex::new(()));
        let event_order_for_emit = Arc::clone(&event_order);
        let emit = Arc::new(move |event: TerminalEvent| {
            let _guard = event_order_for_emit
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            handle_agent_terminal_event(
                &driver_for_emit,
                &transport_for_emit,
                &fanout_for_emit,
                project_id_for_emit.as_deref(),
                event,
            )
        });
        let event_key = config
            .terminal_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|terminal_id| format!("remote-terminal:{terminal_id}"));
        let create_result = if let Some(event_key) = event_key {
            self.driver.create_with_event_key(config, event_key, emit)
        } else {
            self.driver.create_with_sink(config, emit)
        };
        match create_result {
            Ok(session_id) => {
                let _guard = event_order
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let created_terminal = self
                    .driver
                    .list()
                    .into_iter()
                    .find(|terminal| terminal.id == session_id && terminal.is_running);
                let Some(created_terminal) = created_terminal else {
                    self.fanout.clear_session(&session_id);
                    reply(
                        self.transport,
                        device_id,
                        Some(&session_id),
                        msg.request_id,
                        REMOTE_TERMINAL_CLOSED,
                        json!({ "sessionId": session_id }),
                    );
                    return;
                };
                if let Some(project_id) = project_id.as_deref() {
                    self.fanout.set_session_project(&session_id, project_id);
                }
                finish_terminal_create_viewer_lifecycle(
                    &session_id,
                    device_id,
                    |session_id, device_id| self.add_viewer(session_id, device_id),
                );
                let mut subscribers = project_id
                    .as_deref()
                    .map(|project_id| self.fanout.project_subscribers(project_id))
                    .unwrap_or_default();
                let created_payload = terminal_payload(created_terminal, self.fanout);
                reply(
                    self.transport,
                    device_id,
                    Some(&session_id),
                    msg.request_id,
                    REMOTE_TERMINAL_CREATED,
                    created_payload.clone(),
                );
                if let Some(device_id) = device_id {
                    subscribers.retain(|subscriber| subscriber != device_id);
                }
                send_to_viewers(
                    self.transport,
                    &subscribers,
                    json!({
                        "type": REMOTE_TERMINAL_LIST,
                        "payload": list_payload(self.driver, self.fanout),
                    }),
                    false,
                );
                if lifecycle.reattaching
                    && let Some(device_id) =
                        device_id.map(str::trim).filter(|value| !value.is_empty())
                {
                    send_terminal_baseline(
                        self.driver,
                        self.transport,
                        self.fanout,
                        device_id,
                        &session_id,
                        payload,
                    );
                }
            }
            Err(error) => {
                rollback_terminal_create_viewer_lifecycle(
                    &lifecycle,
                    device_id,
                    |session_id, device_id| self.remove_viewer(session_id, device_id),
                );
                if let Some(session_id) = lifecycle.requested_terminal_id.as_deref() {
                    reply(
                        self.transport,
                        device_id,
                        Some(session_id),
                        msg.request_id,
                        REMOTE_ERROR,
                        json!({ "message": error.to_string() }),
                    );
                } else {
                    reply(
                        self.transport,
                        device_id,
                        None,
                        msg.request_id,
                        REMOTE_ERROR,
                        json!({ "message": error.to_string() }),
                    );
                }
            }
        }
    }

    fn handle_terminal_input_msg(&self, msg: &TerminalMessage) {
        let data = msg
            .payload
            .get("data")
            .and_then(Value::as_str)
            .unwrap_or("");
        match msg.session_id {
            Some(id) => {
                let owner = self.viewport_owner_for(msg.device_id);
                let is_owner = match self.driver.viewport_state(id) {
                    Ok(state) if state.owner == owner => true,
                    Ok(state) if state.owner == terminal_viewport_local_owner() => self
                        .driver
                        .claim_viewport_auto(id, &owner)
                        .is_ok_and(|state| state.owner == owner),
                    Ok(_) => false,
                    Err(_) => true,
                };
                if !is_owner {
                    if let Some(input_id) = msg.payload.get("inputId").and_then(Value::as_str) {
                        reply(
                            self.transport,
                            msg.device_id,
                            Some(id),
                            msg.request_id,
                            REMOTE_TERMINAL_INPUT_ACK,
                            json!({
                                "sessionId": id,
                                "inputId": input_id,
                                "ok": false,
                                "accepted": false,
                            }),
                        );
                    }
                    return;
                }
                self.driver.touch_viewport_lease(id, &owner);
                match self.driver.write(id, data.as_bytes()) {
                    Ok(()) => {
                        let mut payload = json!({ "sessionId": id });
                        if let Some(input_id) = msg.payload.get("inputId").and_then(Value::as_str) {
                            payload["inputId"] = json!(input_id);
                            payload["ok"] = json!(true);
                            payload["accepted"] = json!(true);
                        }
                        reply(
                            self.transport,
                            msg.device_id,
                            Some(id),
                            msg.request_id,
                            REMOTE_TERMINAL_INPUT_ACK,
                            payload,
                        );
                    }
                    Err(error) => {
                        reply(
                            self.transport,
                            msg.device_id,
                            Some(id),
                            msg.request_id,
                            REMOTE_ERROR,
                            json!({ "message": error.to_string() }),
                        );
                    }
                }
            }
            None => reply(
                self.transport,
                msg.device_id,
                None,
                msg.request_id,
                REMOTE_ERROR,
                json!({ "message": "sessionId is required." }),
            ),
        }
    }

    fn handle_terminal_resize_msg(&self, msg: &TerminalMessage) {
        let Some(cols) = msg
            .payload
            .get("cols")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .filter(|value| *value > 0)
        else {
            reply(
                self.transport,
                msg.device_id,
                msg.session_id,
                msg.request_id,
                REMOTE_ERROR,
                json!({ "message": "terminal.resize requires positive cols." }),
            );
            return;
        };
        let Some(rows) = msg
            .payload
            .get("rows")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .filter(|value| *value > 0)
        else {
            reply(
                self.transport,
                msg.device_id,
                msg.session_id,
                msg.request_id,
                REMOTE_ERROR,
                json!({ "message": "terminal.resize requires positive rows." }),
            );
            return;
        };
        if let Some(id) = msg.session_id {
            let owner = self.viewport_owner_for(msg.device_id);
            let _ = self.driver.claim_viewport_auto(id, &owner);
            let _ = self.driver.resize_viewport(id, &owner, cols, rows);
        }
    }

    fn handle_terminal_close_msg(&self, msg: &TerminalMessage) {
        if let Some(id) = msg.session_id {
            let mut project_subscribers = self
                .fanout
                .project_for_session(id)
                .as_deref()
                .map(|project_id| self.fanout.project_subscribers(project_id))
                .unwrap_or_default();
            let requesting_device = msg
                .device_id
                .filter(|device_id| !device_id.trim().is_empty());
            let restore_requesting_viewer = requesting_device.is_some_and(|device_id| {
                self.fanout
                    .viewers(id)
                    .iter()
                    .any(|viewer| viewer == device_id)
            });
            if let Some(device_id) = requesting_device {
                self.remove_viewer(id, device_id);
            }
            let existed = match self
                .driver
                .kill_and_wait_if_present(id, Duration::from_secs(10))
            {
                Ok(existed) => existed,
                Err(error) => {
                    if restore_requesting_viewer && let Some(device_id) = requesting_device {
                        self.add_viewer(id, device_id);
                    }
                    reply(
                        self.transport,
                        msg.device_id,
                        Some(id),
                        msg.request_id,
                        REMOTE_ERROR,
                        json!({ "message": error.to_string() }),
                    );
                    return;
                }
            };
            reply(
                self.transport,
                msg.device_id,
                Some(id),
                msg.request_id,
                REMOTE_TERMINAL_CLOSED,
                json!({ "sessionId": id }),
            );
            if !existed {
                if let Some(device_id) = requesting_device
                    && !project_subscribers
                        .iter()
                        .any(|subscriber| subscriber == device_id)
                {
                    project_subscribers.push(device_id.to_string());
                }
                send_to_viewers(
                    self.transport,
                    &project_subscribers,
                    json!({
                        "type": REMOTE_TERMINAL_LIST,
                        "payload": list_payload(self.driver, self.fanout),
                    }),
                    false,
                );
            }
        }
    }

    fn handle_terminal_viewport_claim_msg(&self, msg: &TerminalMessage) {
        if let (Some(id), Some(device_id)) = (msg.session_id, msg.device_id) {
            self.add_viewer(id, device_id);
            let owner = self.viewport_owner_for(Some(device_id));
            let intent = remote_terminal_dispatch::terminal_viewport_claim_intent(msg.payload);
            if intent == remote_terminal_dispatch::TerminalViewportClaimIntent::Renew {
                self.driver.touch_viewport_lease(id, &owner);
                if let Ok(state) = self.driver.viewport_state(id)
                    && state.owner != owner
                {
                    self.send_terminal_viewport_state(id, Some(device_id), msg.request_id, &state);
                }
                return;
            }
            let state = match intent {
                remote_terminal_dispatch::TerminalViewportClaimIntent::Auto => {
                    self.driver.claim_viewport_auto(id, &owner)
                }
                remote_terminal_dispatch::TerminalViewportClaimIntent::Force => {
                    self.driver.claim_viewport(id, &owner)
                }
                remote_terminal_dispatch::TerminalViewportClaimIntent::Renew => unreachable!(),
            };
            if let Ok(state) = state {
                self.send_terminal_viewport_state(id, Some(device_id), msg.request_id, &state);
            }
        }
    }

    fn handle_terminal_viewport_resize_msg(&self, msg: &TerminalMessage) {
        if let (Some(id), Some(device_id)) = (msg.session_id, msg.device_id) {
            self.add_viewer(id, device_id);
            let owner = self.viewport_owner_for(Some(device_id));
            let cols = msg
                .payload
                .get("cols")
                .and_then(Value::as_u64)
                .unwrap_or(80) as u16;
            let rows = msg
                .payload
                .get("rows")
                .and_then(Value::as_u64)
                .unwrap_or(24) as u16;
            let _ = self.driver.claim_viewport_auto(id, &owner);
            match self.driver.resize_viewport(id, &owner, cols, rows) {
                Ok(Some(state)) => {
                    self.send_terminal_viewport_state(id, Some(device_id), msg.request_id, &state)
                }
                Ok(None) => {
                    if let Ok(state) = self.driver.viewport_state(id) {
                        self.send_terminal_viewport_state(
                            id,
                            Some(device_id),
                            msg.request_id,
                            &state,
                        );
                    }
                }
                Err(_) => {}
            }
        }
    }

    fn handle_terminal_output_ack_msg(&self, msg: &TerminalMessage) {
        if let Some(session_id) = msg.session_id {
            self.fanout.record_ack(
                session_id,
                msg.device_id,
                msg.payload.get("outputSeq").and_then(Value::as_i64),
            );
        }
        let Some(session_id) = msg.session_id else {
            return;
        };
        let owner = self.viewport_owner_for(msg.device_id);
        self.driver.touch_viewport_lease(session_id, &owner);
    }

    fn send_terminal_viewport_state(
        &self,
        session_id: &str,
        device_id: Option<&str>,
        request_id: Option<&str>,
        state: &TerminalViewportState,
    ) {
        let Some(device_id) = device_id else {
            return;
        };
        self.reply_terminal(
            Some(device_id),
            Some(session_id),
            request_id,
            REMOTE_TERMINAL_VIEWPORT_STATE,
            viewport_state_payload(self.fanout, session_id, Some(device_id), state),
        );
    }
}

/// Route one terminal envelope through the shared dispatch. Terminal output
/// streams asynchronously, so this domain is imperative (it sends its own
/// responses) rather than single-reply. The session id is carried in the
/// payload by create/input/resize/close and at the envelope top level by
/// subscribe/viewport/ack; accept either.
pub fn handle_terminal(
    driver: &Arc<TerminalManager>,
    transport: &TransportSlot,
    fanout_state: &TerminalFanout,
    device_id: Option<&str>,
    kind: &str,
    envelope: &Value,
    payload: &Value,
) {
    let session_id = payload
        .get("sessionId")
        .and_then(Value::as_str)
        .or_else(|| envelope.get("sessionId").and_then(Value::as_str));
    let ctx = AgentTerminalCtx {
        driver,
        transport,
        fanout: fanout_state,
    };
    let msg = TerminalMessage {
        kind,
        device_id,
        session_id,
        request_id: envelope.get("requestId").and_then(Value::as_str),
        payload,
    };
    if remote_terminal_dispatch::is_terminal_kind(kind) {
        ctx.dispatch_terminal(&msg);
        return;
    }
    // The headless host folds the resource-level terminal subscription into this
    // module (the desktop has a generic resource router for it instead).
    match kind {
        REMOTE_RESOURCE_SUBSCRIBE => ctx.resource_subscribe(&msg),
        REMOTE_RESOURCE_UNSUBSCRIBE => ctx.resource_unsubscribe(&msg),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codux_protocol::{REMOTE_TERMINAL_VIEWPORT_CLAIM, RemoteTransportKind};
    use codux_remote_transport::LocalMemoryTransportHub;

    type TestMessages = Arc<Mutex<HashMap<String, Vec<Value>>>>;

    fn test_transport(device_ids: &[&str]) -> (TransportSlot, TestMessages) {
        let hub = LocalMemoryTransportHub::new();
        let received = Arc::new(Mutex::new(HashMap::<String, Vec<Value>>::new()));
        let host = hub.connect(
            "host",
            RemoteTransportKind::Iroh,
            Arc::new(|_, _| {}),
            Arc::new(|_, _| {}),
        );
        for device_id in device_ids {
            let device_id = (*device_id).to_string();
            let received = Arc::clone(&received);
            hub.connect(
                device_id.clone(),
                RemoteTransportKind::Iroh,
                Arc::new(move |_, data| {
                    let Ok(message) = serde_json::from_slice::<Value>(&data) else {
                        return;
                    };
                    received
                        .lock()
                        .unwrap()
                        .entry(device_id.clone())
                        .or_default()
                        .push(message);
                }),
                Arc::new(|_, _| {}),
            );
        }
        (
            Arc::new(Mutex::new(Some(host as Arc<dyn RemoteTransport>))),
            received,
        )
    }

    fn message_types(messages: &TestMessages, device_id: &str) -> Vec<String> {
        messages
            .lock()
            .unwrap()
            .get(device_id)
            .into_iter()
            .flatten()
            .filter_map(|message| message.get("type").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn project_subscription_does_not_receive_future_session_errors() {
        let (transport, received) = test_transport(&["phone-a", "phone-b"]);
        let fanout = TerminalFanout::new();
        fanout.add_project_subscriber("project-1", "phone-a");
        fanout.add_project_subscriber("project-1", "phone-b");

        assert!(handle_agent_terminal_event(
            &TerminalManager::new(),
            &transport,
            &fanout,
            Some("project-1"),
            TerminalEvent::Error {
                session_id: "future-session".to_string(),
                message: "failed".to_string(),
            },
        ));

        assert!(message_types(&received, "phone-a").is_empty());
        assert!(message_types(&received, "phone-b").is_empty());
    }

    #[test]
    fn output_without_viewers_is_not_broadcast() {
        let (transport, received) = test_transport(&["phone-a", "phone-b"]);
        let fanout = TerminalFanout::new();

        assert!(handle_agent_terminal_event(
            &TerminalManager::new(),
            &transport,
            &fanout,
            Some("project-1"),
            TerminalEvent::Output {
                session_id: "session-1".to_string(),
                text: "private".to_string(),
                bytes: b"private".to_vec(),
                buffer_length: 7,
                buffer_end: 7,
            },
        ));

        assert!(message_types(&received, "phone-a").is_empty());
        assert!(message_types(&received, "phone-b").is_empty());
    }

    #[test]
    fn live_output_uses_the_event_history_watermark() {
        let (transport, received) = test_transport(&["phone-a"]);
        let fanout = TerminalFanout::new();
        fanout.add_viewer("session-1", "phone-a");

        assert!(handle_agent_terminal_event(
            &TerminalManager::new(),
            &transport,
            &fanout,
            Some("project-1"),
            TerminalEvent::Output {
                session_id: "session-1".to_string(),
                text: "output".to_string(),
                bytes: b"output".to_vec(),
                buffer_length: 7,
                buffer_end: 42,
            },
        ));

        let messages = received.lock().unwrap();
        let output = messages.get("phone-a").unwrap().first().unwrap();
        assert_eq!(output["payload"]["data"], "output");
        assert_eq!(output["payload"]["bufferLength"], 7);
        assert_eq!(output["payload"]["bufferEnd"], 42);
    }

    #[test]
    fn session_reply_carries_terminal_id_at_envelope_level() {
        let (transport, received) = test_transport(&["phone-a"]);

        reply(
            &transport,
            Some("phone-a"),
            Some("session-1"),
            Some("request-1"),
            REMOTE_ERROR,
            json!({ "message": "failed" }),
        );

        let messages = received.lock().unwrap();
        let message = messages.get("phone-a").unwrap().first().unwrap();
        assert_eq!(
            message.get("type").and_then(Value::as_str),
            Some(REMOTE_ERROR)
        );
        assert_eq!(
            message.get("sessionId").and_then(Value::as_str),
            Some("session-1")
        );
        assert_eq!(
            message.get("requestId").and_then(Value::as_str),
            Some("request-1")
        );
    }

    #[test]
    fn natural_exit_notifies_all_viewers_and_clears_session() {
        let (transport, received) = test_transport(&["phone-a", "phone-b"]);
        let fanout = TerminalFanout::new();
        fanout.add_project_subscriber("project-1", "phone-a");
        fanout.add_viewer("session-1", "phone-b");

        assert!(!handle_agent_terminal_event(
            &TerminalManager::new(),
            &transport,
            &fanout,
            Some("project-1"),
            TerminalEvent::Exit {
                session_id: "session-1".to_string(),
                exit_code: Some(0),
            },
        ));

        assert_eq!(
            message_types(&received, "phone-a"),
            vec![REMOTE_TERMINAL_LIST]
        );
        assert_eq!(
            message_types(&received, "phone-b"),
            vec![REMOTE_TERMINAL_CLOSED]
        );
        assert!(fanout.viewers("session-1").is_empty());
        assert_eq!(fanout.project_for_session("session-1"), None);
    }

    #[test]
    fn project_unsubscribe_preserves_direct_session_viewer() {
        let fanout = TerminalFanout::new();
        fanout.set_session_project("session-1", "project-1");
        fanout.add_project_subscriber("project-1", "phone-a");
        fanout.add_viewer("session-1", "phone-a");

        fanout.remove_project_subscriber("project-1", "phone-a");

        assert_eq!(fanout.viewers("session-1"), vec!["phone-a"]);
    }

    #[test]
    fn project_subscriber_is_not_a_session_viewer() {
        let fanout = TerminalFanout::new();
        fanout.add_project_subscriber("project-1", "phone-a");
        fanout.set_session_project("session-1", "project-1");

        assert!(fanout.viewers("session-1").is_empty());
    }

    #[test]
    fn remove_device_clears_all_subscriptions_and_output_ack() {
        let fanout = TerminalFanout::new();
        fanout.set_session_project("session-1", "project-1");
        fanout.add_project_subscriber("project-1", "phone-a");
        fanout.add_viewer("session-1", "phone-a");
        fanout.add_viewer("session-1", "phone-b");
        fanout.record_ack("session-1", Some("phone-a"), Some(9));
        fanout.record_ack("session-1", Some("phone-b"), Some(7));

        fanout.remove_device("phone-a");

        assert_eq!(fanout.viewers("session-1"), vec!["phone-b"]);
        assert_eq!(fanout.ack_seq("session-1", Some("phone-a")), 0);
        assert_eq!(fanout.ack_seq("session-1", Some("phone-b")), 7);
    }

    #[test]
    fn viewport_auto_claim_does_not_override_explicit_desktop_owner() {
        let driver = Arc::new(TerminalManager::new());
        let temp = std::env::temp_dir().join(format!(
            "codux-agent-viewport-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp).unwrap();
        let session_id = driver
            .create(
                TerminalPtyConfig {
                    cwd: Some(temp.to_string_lossy().to_string()),
                    ..Default::default()
                },
                |_| {},
            )
            .expect("create terminal");
        let (transport, _) = test_transport(&["phone-a"]);
        let fanout = TerminalFanout::new();
        let envelope = json!({ "sessionId": session_id });

        handle_terminal(
            &driver,
            &transport,
            &fanout,
            Some("phone-a"),
            REMOTE_TERMINAL_VIEWPORT_CLAIM,
            &envelope,
            &json!({ "intent": "auto" }),
        );
        assert_eq!(
            driver.viewport_state(&session_id).unwrap().owner,
            "remote:phone-a"
        );

        driver
            .claim_viewport(&session_id, terminal_viewport_local_owner())
            .expect("explicit desktop claim");
        handle_terminal(
            &driver,
            &transport,
            &fanout,
            Some("phone-a"),
            REMOTE_TERMINAL_VIEWPORT_CLAIM,
            &envelope,
            &json!({ "intent": "auto" }),
        );
        assert_eq!(
            driver.viewport_state(&session_id).unwrap().owner,
            terminal_viewport_local_owner()
        );

        handle_terminal(
            &driver,
            &transport,
            &fanout,
            Some("phone-a"),
            REMOTE_TERMINAL_VIEWPORT_CLAIM,
            &envelope,
            &json!({ "intent": "force" }),
        );
        assert_eq!(
            driver.viewport_state(&session_id).unwrap().owner,
            "remote:phone-a"
        );

        driver.kill(&session_id).ok();
        std::fs::remove_dir_all(temp).ok();
    }
}
