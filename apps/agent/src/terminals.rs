//! Terminal domain for the headless host: spawn real PTYs with the same
//! `TerminalManager` the desktop host uses, so AI runtime tracking and terminal
//! protocol behavior stay aligned.
//!
//! Multi-client: several devices can watch the same terminal at once. The
//! viewer set, baseline catch-up, and viewport lease all reuse the shared crate
//! pieces (`RemoteTerminalSubscriptions`, `snapshot_tail` + `terminal_buffer_payloads`,
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
};
use codux_runtime_live::terminal_pty::{TerminalManager, TerminalPtyConfig};
use codux_runtime_live::terminal_pty::{
    TerminalViewportState, terminal_viewport_local_owner, terminal_viewport_remote_owner,
};
use codux_terminal_core::TerminalEvent;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    output_seq: Arc<Mutex<HashMap<String, i64>>>,
    output_ack: Arc<Mutex<HashMap<(String, String), i64>>>,
}

impl TerminalFanout {
    pub fn new() -> Self {
        Self::default()
    }

    /// The shared viewer registry, e.g. for the viewport-owner resolver.
    pub fn subscriptions(&self) -> Arc<RemoteTerminalSubscriptions> {
        Arc::clone(&self.subscriptions)
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

    fn remove_viewer(&self, session_id: &str, device_id: &str) {
        self.subscriptions
            .remove_session_viewer(session_id, device_id);
        self.clear_ack(session_id, device_id);
    }

    fn viewers(&self, session_id: &str) -> Vec<String> {
        self.subscriptions
            .viewers_for_session(session_id, None)
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

    fn clear_session(&self, session_id: &str) {
        self.subscriptions.remove_session(session_id);
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
    if let Ok(guard) = transport.lock() {
        if let Some(t) = guard.as_ref() {
            if terminal_stream {
                t.send_terminal(bytes, device_id);
            } else {
                t.send(bytes, device_id);
            }
        }
    }
}

/// Serialize one frame and fan it out: unicast to each viewer (the transport
/// routes by the device arg, not the envelope's `deviceId`), or broadcast when
/// no device has explicitly subscribed -- which preserves the original
/// single-device / no-device behavior.
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
        if terminal_stream {
            t.send_terminal(bytes, None);
        } else {
            t.send(bytes, None);
        }
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

fn reply(transport: &TransportSlot, device_id: Option<&str>, kind: &str, payload: Value) {
    let mut envelope = json!({ "type": kind, "payload": payload });
    if let Some(device_id) = device_id {
        envelope["deviceId"] = json!(device_id);
    }
    send(transport, device_id, envelope, false);
}

fn list_payload(driver: &TerminalManager) -> Value {
    let terminals = driver
        .list()
        .into_iter()
        .map(terminal_snapshot_payload)
        .collect::<Vec<_>>();
    json!({ "terminals": terminals })
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
    let Ok(state) = driver.viewport_state(session_id) else {
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

fn send_terminal_viewport_state(
    driver: &TerminalManager,
    transport: &TransportSlot,
    fanout_state: &TerminalFanout,
    session_id: &str,
    device_id: &str,
) {
    let Ok(state) = driver.viewport_state(session_id) else {
        return;
    };
    let mut envelope = json!({
        "type": REMOTE_TERMINAL_VIEWPORT_STATE,
        "sessionId": session_id,
        "payload": viewport_state_payload(fanout_state, session_id, Some(device_id), &state),
    });
    envelope["deviceId"] = json!(device_id);
    send(transport, Some(device_id), envelope, false);
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
    let (data, offset, baseline_failed) = match driver.snapshot_tail(session_id, max_chars) {
        Ok((data, offset)) => (data, offset, false),
        Err(_) => (String::new(), 0, true),
    };
    let total_characters = driver
        .buffer_characters(session_id)
        .unwrap_or_else(|_| offset + data.chars().count());
    let screen_data = if baseline_failed {
        None
    } else if use_viewport {
        let max_lines = viewport
            .map(|viewport| viewport.rows.max(8) as usize)
            .unwrap_or(8);
        driver
            .remote_viewport_snapshot(session_id, 0, 0, max_lines)
            .ok()
            .map(|snapshot| snapshot.data)
    } else {
        driver
            .screen_snapshot(session_id)
            .ok()
            .filter(|snapshot| snapshot.input_mode.alternate_screen)
            .map(|snapshot| snapshot.data)
    }
    .filter(|data| !data.is_empty());
    let output_seq = fanout_state.current_seq(session_id);
    let window = RemoteTerminalBufferWindow {
        data,
        screen_data,
        offset,
        total_characters,
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
            .unwrap_or(true);
        if let (Some(project_id), Some(device_id)) = (
            msg.payload.get("projectId").and_then(Value::as_str),
            msg.device_id,
        ) {
            let target_session_id = msg
                .payload
                .get("baselineSessionId")
                .or_else(|| msg.payload.get("sessionId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            for terminal in self.driver.list() {
                if terminal.project_id != project_id {
                    continue;
                }
                self.add_viewer(&terminal.id, device_id);
                send_terminal_viewport_state(
                    self.driver,
                    self.transport,
                    self.fanout,
                    &terminal.id,
                    device_id,
                );
                if baseline {
                    let mut payload = msg.payload.clone();
                    if target_session_id != Some(terminal.id.as_str()) {
                        if let Some(payload) = payload.as_object_mut() {
                            payload.remove("viewportCols");
                            payload.remove("viewportRows");
                        }
                    }
                    send_terminal_baseline(
                        self.driver,
                        self.transport,
                        self.fanout,
                        device_id,
                        &terminal.id,
                        &payload,
                    );
                }
            }
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
            for terminal in self.driver.list() {
                if terminal.project_id == project_id {
                    self.remove_viewer(&terminal.id, device_id);
                }
            }
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
        _session_id: Option<&str>,
        kind: &str,
        payload: Value,
    ) {
        // The agent envelope is {type, payload, deviceId}; the shared replies
        // embed sessionId in the payload, so the session id is already carried
        // and the explicit envelope-level arg is not needed here.
        reply(self.transport, device_id, kind, payload);
    }

    fn handle_terminal_list_msg(&self, msg: &TerminalMessage) {
        reply(
            self.transport,
            msg.device_id,
            REMOTE_TERMINAL_LIST,
            list_payload(self.driver),
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
            project_id: project_id.clone(),
            worktree_id: payload
                .get("worktreeId")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| project_id.clone()),
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
        let lifecycle = prepare_terminal_create_lifecycle(
            self.driver,
            &config,
            device_id,
            |session_id, device_id| self.add_viewer(session_id, device_id),
        );
        // Stream this session's output to ALL of its viewers (fan-out), and
        // forward viewport-state changes (lease claim/handoff) too.
        let driver_for_emit = Arc::clone(self.driver);
        let transport_for_emit = Arc::clone(self.transport);
        let fanout_for_emit = self.fanout.clone();
        let emit = move |event: TerminalEvent| match event {
            TerminalEvent::Output {
                session_id, bytes, ..
            } => {
                let data = String::from_utf8_lossy(&bytes).to_string();
                let next = fanout_for_emit.next_seq(&session_id);
                // Live output is a pure byte stream — no per-output screen
                // keyframe. Replaying a whole-screen keyframe on top of the
                // viewer's own scrollback duplicated the screen (badly on
                // resize); the snapshot was also serialized on every chunk.
                let buffer_len = driver_for_emit
                    .buffer_characters(&session_id)
                    .ok()
                    .unwrap_or(0);
                let envelope = json!({
                    "type": REMOTE_TERMINAL_OUTPUT,
                    "sessionId": session_id,
                    "payload": terminal_live_output_payload(data, buffer_len, next),
                });
                fanout(
                    &transport_for_emit,
                    &fanout_for_emit.viewers(&session_id),
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
                for viewer in fanout_for_emit.viewers(&session_id) {
                    let mut envelope = json!({
                        "type": REMOTE_TERMINAL_VIEWPORT_STATE,
                        "sessionId": session_id,
                        "payload": viewport_state_payload(
                            &fanout_for_emit,
                            &session_id,
                            Some(&viewer),
                            &state,
                        ),
                    });
                    envelope["deviceId"] = json!(viewer);
                    send(&transport_for_emit, Some(&viewer), envelope, false);
                }
            }
            _ => {}
        };
        let event_key = config
            .terminal_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|terminal_id| format!("remote-terminal:{terminal_id}"));
        let create_result = if let Some(event_key) = event_key {
            self.driver.create_with_event_key(
                config,
                event_key,
                Arc::new(move |event| {
                    emit(event);
                    true
                }),
            )
        } else {
            self.driver.create(config, emit)
        };
        match create_result {
            Ok(session_id) => {
                finish_terminal_create_viewer_lifecycle(
                    &session_id,
                    device_id,
                    |session_id, device_id| self.add_viewer(session_id, device_id),
                );
                reply(
                    self.transport,
                    device_id,
                    REMOTE_TERMINAL_CREATED,
                    json!({ "sessionId": session_id }),
                );
                reply(
                    self.transport,
                    device_id,
                    REMOTE_TERMINAL_LIST,
                    list_payload(self.driver),
                );
                if lifecycle.reattaching {
                    if let Some(device_id) =
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
            }
            Err(error) => reply(
                self.transport,
                device_id,
                REMOTE_ERROR,
                json!({ "message": error.to_string() }),
            ),
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
                // Same handoff guard as the desktop host: a non-owner's input is
                // dropped — except when the lease expired back to the host-local
                // placeholder (nobody driving), where the first remote input
                // re-claims instead of leaving the pane permanently deaf.
                let is_owner = match self.driver.viewport_state(id) {
                    Ok(state) if state.owner == owner => true,
                    Ok(state) if state.owner == terminal_viewport_local_owner() => {
                        self.driver.claim_viewport(id, &owner).is_ok()
                    }
                    Ok(_) => false,
                    Err(_) => true,
                };
                if !is_owner {
                    if let Some(input_id) = msg.payload.get("inputId").and_then(Value::as_str) {
                        reply(
                            self.transport,
                            msg.device_id,
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
                            REMOTE_TERMINAL_INPUT_ACK,
                            payload,
                        );
                    }
                    Err(error) => {
                        reply(
                            self.transport,
                            msg.device_id,
                            REMOTE_ERROR,
                            json!({ "message": error.to_string() }),
                        );
                    }
                }
            }
            None => reply(
                self.transport,
                msg.device_id,
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
                REMOTE_ERROR,
                json!({ "message": "terminal.resize requires positive rows." }),
            );
            return;
        };
        if let Some(id) = msg.session_id {
            let _ = self.driver.resize(id, cols, rows);
        }
    }

    fn handle_terminal_close_msg(&self, msg: &TerminalMessage) {
        if let Some(id) = msg.session_id {
            let _ = self.driver.kill(id);
            self.fanout.clear_session(id);
            reply(
                self.transport,
                msg.device_id,
                REMOTE_TERMINAL_CLOSED,
                json!({ "sessionId": id }),
            );
            reply(
                self.transport,
                msg.device_id,
                REMOTE_TERMINAL_LIST,
                list_payload(self.driver),
            );
        }
    }

    fn handle_terminal_viewport_claim_msg(&self, msg: &TerminalMessage) {
        if let (Some(id), Some(device_id)) = (msg.session_id, msg.device_id) {
            self.add_viewer(id, device_id);
            let owner = self.viewport_owner_for(Some(device_id));
            let renew_only = msg
                .payload
                .get("renewOnly")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if renew_only {
                self.driver.touch_viewport_lease(id, &owner);
                if let Ok(state) = self.driver.viewport_state(id) {
                    if state.owner != owner {
                        self.send_terminal_viewport_state(id, Some(device_id), &state);
                    }
                }
                return;
            }
            if let Ok(state) = self.driver.claim_viewport(id, &owner) {
                self.send_terminal_viewport_state(id, Some(device_id), &state);
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
            let _ = self.driver.claim_viewport(id, &owner);
            match self.driver.resize_viewport(id, &owner, cols, rows) {
                Ok(Some(state)) => self.send_terminal_viewport_state(id, Some(device_id), &state),
                Ok(None) => {
                    if let Ok(state) = self.driver.viewport_state(id) {
                        self.send_terminal_viewport_state(id, Some(device_id), &state);
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
        state: &TerminalViewportState,
    ) {
        let Some(device_id) = device_id else {
            return;
        };
        self.reply_terminal(
            Some(device_id),
            Some(session_id),
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
