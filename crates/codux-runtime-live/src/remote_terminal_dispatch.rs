//! Shared dispatch for the remote terminal protocol, used by BOTH the desktop
//! host (`codux-runtime`) and the headless agent host (`codux-agent`).
//!
//! The two hosts differ only in the GUI-adjacent bits: the desktop tracks a
//! terminal *layout* (which panes hold which sessions) and batches PTY output
//! for its renderer, while the headless agent keeps a leaner per-session model.
//! Everything else -- the protocol surface, the message router, and the arms
//! that are pure [`TerminalManager`] calls -- lives here so neither host can
//! silently fall behind the other.
//!
//! How a host plugs in: wrap the host's state in a small context type and
//! implement [`RemoteTerminalDispatch`]. The trait owns the ONE
//! `match kind { .. }` router ([`RemoteTerminalDispatch::dispatch_terminal`]);
//! adding a new terminal message means adding one arm there, and any host that
//! does not implement the corresponding method fails to compile -- drift becomes
//! a build error instead of a silently-missing feature. Arms that are identical
//! across hosts (signal, output-ack, viewport release/scroll) ship as default
//! methods here, so a fix lands once for both.

use codux_protocol::{
    REMOTE_TERMINAL_BUFFER, REMOTE_TERMINAL_CLOSE, REMOTE_TERMINAL_CREATE, REMOTE_TERMINAL_INPUT,
    REMOTE_TERMINAL_LIST, REMOTE_TERMINAL_OUTPUT_ACK, REMOTE_TERMINAL_RESIZE, REMOTE_TERMINAL_SIGNAL,
    REMOTE_TERMINAL_SUBSCRIBE, REMOTE_TERMINAL_UNSUBSCRIBE, REMOTE_TERMINAL_VIEWPORT_CLAIM,
    REMOTE_TERMINAL_VIEWPORT_RELEASE, REMOTE_TERMINAL_VIEWPORT_RESIZE,
    REMOTE_TERMINAL_VIEWPORT_SCROLL, REMOTE_TERMINAL_VIEWPORT_SCROLLED,
    REMOTE_TERMINAL_VIEWPORT_STATE,
};
use codux_terminal_core::TerminalViewportState;
use serde_json::{Value, json};

use crate::terminal_pty::{TerminalManager, terminal_viewport_remote_owner};

/// One inbound terminal message, normalized so both hosts present the same view
/// to the shared router regardless of how each stores its envelopes.
///
/// `session_id` is pre-resolved by the host: the desktop reads its envelope's
/// dedicated field, the agent falls back from `payload.sessionId` to the
/// envelope top level. The router and every arm below see only the resolved id.
pub struct TerminalMessage<'a> {
    pub kind: &'a str,
    pub device_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub payload: &'a Value,
}

/// True for every terminal-protocol message the shared router handles. This is
/// the canonical set; hosts must not keep private copies that can fall behind.
///
/// Resource-level subscribe/unsubscribe are intentionally excluded: they are a
/// generic resource concern (projects/worktrees/terminals), routed by each host
/// outside this terminal dispatch.
pub fn is_terminal_kind(kind: &str) -> bool {
    matches!(
        kind,
        REMOTE_TERMINAL_LIST
            | REMOTE_TERMINAL_SUBSCRIBE
            | REMOTE_TERMINAL_UNSUBSCRIBE
            | REMOTE_TERMINAL_CREATE
            | REMOTE_TERMINAL_BUFFER
            | REMOTE_TERMINAL_OUTPUT_ACK
            | REMOTE_TERMINAL_INPUT
            | REMOTE_TERMINAL_SIGNAL
            | REMOTE_TERMINAL_RESIZE
            | REMOTE_TERMINAL_CLOSE
            | REMOTE_TERMINAL_VIEWPORT_CLAIM
            | REMOTE_TERMINAL_VIEWPORT_RESIZE
            | REMOTE_TERMINAL_VIEWPORT_RELEASE
            | REMOTE_TERMINAL_VIEWPORT_SCROLL
    )
}

/// The shared remote-terminal protocol handler. A host implements the few
/// host-specific arms (create/list/input/... which touch the host's layout,
/// batching or baseline machinery) plus two primitives ([`terminal_manager`]
/// and [`reply_terminal`]); the router and the identical arms come for free.
///
/// [`terminal_manager`]: RemoteTerminalDispatch::terminal_manager
/// [`reply_terminal`]: RemoteTerminalDispatch::reply_terminal
pub trait RemoteTerminalDispatch {
    // ---- primitives every host provides ----

    /// The shared PTY/session manager both hosts drive.
    fn terminal_manager(&self) -> &TerminalManager;

    /// Send one frame back to a device. `session_id`, when set, is stamped on
    /// the envelope by hosts that carry it there (the desktop); the shared
    /// replies below ALSO embed it in the payload, so a host whose transport
    /// envelope omits it (the agent) still delivers a session-tagged frame.
    fn reply_terminal(
        &self,
        device_id: Option<&str>,
        session_id: Option<&str>,
        kind: &str,
        payload: Value,
    );

    // ---- host-specific arms (no shared default) ----
    //
    // These touch state the headless agent does not share with the desktop:
    // the terminal layout (create/close/list), output batching, or the baseline
    // cache (buffer). Each host keeps its own body; the router still forces both
    // to provide one.

    fn handle_terminal_list_msg(&self, msg: &TerminalMessage);
    fn handle_terminal_subscribe_msg(&self, msg: &TerminalMessage);
    fn handle_terminal_unsubscribe_msg(&self, msg: &TerminalMessage);
    fn handle_terminal_create_msg(&self, msg: &TerminalMessage);
    fn handle_terminal_buffer_msg(&self, msg: &TerminalMessage);
    fn handle_terminal_input_msg(&self, msg: &TerminalMessage);
    fn handle_terminal_resize_msg(&self, msg: &TerminalMessage);
    fn handle_terminal_close_msg(&self, msg: &TerminalMessage);
    fn handle_terminal_viewport_claim_msg(&self, msg: &TerminalMessage);
    fn handle_terminal_viewport_resize_msg(&self, msg: &TerminalMessage);

    // ---- shared defaults (identical across hosts) ----

    /// Map a device id to its viewport-lease owner string, computed the same way
    /// everywhere so a lease taken on one host is recognized by the resolver.
    fn viewport_owner_for(&self, device_id: Option<&str>) -> String {
        device_id
            .map(terminal_viewport_remote_owner)
            .unwrap_or_else(|| "remote".to_string())
    }

    /// Forward a control signal to the PTY: interrupt = Ctrl-C (`0x03`),
    /// escape = ESC (`0x1b`).
    fn handle_terminal_signal_msg(&self, msg: &TerminalMessage) {
        let Some(session_id) = msg.session_id else {
            return;
        };
        let byte: &[u8] = match msg.payload.get("signal").and_then(Value::as_str) {
            Some("interrupt") => &[0x03],
            Some("escape") => &[0x1b],
            _ => &[],
        };
        if !byte.is_empty() {
            let _ = self.terminal_manager().write(session_id, byte);
        }
    }

    /// A steady output-ack proves the client is still actively viewing, so keep
    /// its viewport lease alive -- otherwise a passive local claim could reclaim
    /// the viewport mid-session.
    fn handle_terminal_output_ack_msg(&self, msg: &TerminalMessage) {
        let Some(session_id) = msg.session_id else {
            return;
        };
        let owner = self.viewport_owner_for(msg.device_id);
        self.terminal_manager()
            .touch_viewport_lease(session_id, &owner);
    }

    /// Release this device's viewport lease and, when ownership actually
    /// changes hands, broadcast the new viewport state.
    fn handle_terminal_viewport_release_msg(&self, msg: &TerminalMessage) {
        let Some(session_id) = msg.session_id else {
            return;
        };
        let owner = self.viewport_owner_for(msg.device_id);
        if let Ok(Some(state)) = self.terminal_manager().release_viewport(session_id, &owner) {
            self.send_terminal_viewport_state(session_id, msg.device_id, &state);
        }
    }

    /// Serve remote scrollback from the host's authoritative screen. The
    /// scrollback lives here at the current grid size, so the client never has
    /// to rebuild history by replaying raw bytes recorded at other grid sizes
    /// (which corrupts TUI repaints). `displayOffset` = a precise viewport
    /// fetch; `toBottom` = jump to live; otherwise scroll by `lines` (0 = a sync
    /// request that just reports the current viewport + `totalLines`).
    fn handle_terminal_viewport_scroll_msg(&self, msg: &TerminalMessage) {
        let Some(session_id) = msg.session_id else {
            return;
        };
        let owner = self.viewport_owner_for(msg.device_id);
        self.terminal_manager()
            .touch_viewport_lease(session_id, &owner);
        let viewport_request_id = msg.payload.get("viewportRequestId").and_then(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_u64().map(|number| number.to_string()))
        });
        let max_lines = msg
            .payload
            .get("maxLines")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let overscan_rows = msg
            .payload
            .get("overscanRows")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let snapshot = if let Some(display_offset) = msg
            .payload
            .get("displayOffset")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
        {
            self.terminal_manager()
                .remote_viewport_snapshot(session_id, display_offset, overscan_rows, max_lines)
        } else if msg
            .payload
            .get("toBottom")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            self.terminal_manager().scroll_screen_to_bottom(session_id)
        } else {
            let lines = msg
                .payload
                .get("lines")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
            self.terminal_manager().scroll_screen_lines(session_id, lines)
        };
        let Ok(snapshot) = snapshot else {
            return;
        };
        let mut payload = json!({
            "sessionId": session_id,
            "displayOffset": snapshot.display_offset,
            "totalLines": snapshot.total_lines,
            "cols": snapshot.cols,
            "rows": snapshot.rows,
            "marginRows": snapshot.margin_rows,
            "marginRowsBelow": snapshot.margin_rows_below,
            "screenData": snapshot.data,
        });
        if let Some(request_id) = viewport_request_id {
            payload["viewportRequestId"] = Value::String(request_id);
        }
        self.reply_terminal(
            msg.device_id,
            Some(session_id),
            REMOTE_TERMINAL_VIEWPORT_SCROLLED,
            payload,
        );
    }

    /// The shared VIEWPORT_STATE reply shape. `sessionId` is embedded in the
    /// payload so a host that does not stamp it on the transport envelope still
    /// delivers a session-tagged frame.
    fn send_terminal_viewport_state(
        &self,
        session_id: &str,
        device_id: Option<&str>,
        state: &TerminalViewportState,
    ) {
        self.reply_terminal(
            device_id,
            Some(session_id),
            REMOTE_TERMINAL_VIEWPORT_STATE,
            json!({
                "sessionId": session_id,
                "owner": state.owner,
                "cols": state.cols,
                "rows": state.rows,
                "generation": state.generation,
            }),
        );
    }

    // ---- the ONE router ----

    /// Route a terminal message to its handler. This is the single place the
    /// protocol surface is enumerated; keep [`is_terminal_kind`] in sync with
    /// the arms below.
    fn dispatch_terminal(&self, msg: &TerminalMessage) {
        match msg.kind {
            REMOTE_TERMINAL_LIST => self.handle_terminal_list_msg(msg),
            REMOTE_TERMINAL_SUBSCRIBE => self.handle_terminal_subscribe_msg(msg),
            REMOTE_TERMINAL_UNSUBSCRIBE => self.handle_terminal_unsubscribe_msg(msg),
            REMOTE_TERMINAL_CREATE => self.handle_terminal_create_msg(msg),
            REMOTE_TERMINAL_BUFFER => self.handle_terminal_buffer_msg(msg),
            REMOTE_TERMINAL_OUTPUT_ACK => self.handle_terminal_output_ack_msg(msg),
            REMOTE_TERMINAL_INPUT => self.handle_terminal_input_msg(msg),
            REMOTE_TERMINAL_SIGNAL => self.handle_terminal_signal_msg(msg),
            REMOTE_TERMINAL_RESIZE => self.handle_terminal_resize_msg(msg),
            REMOTE_TERMINAL_CLOSE => self.handle_terminal_close_msg(msg),
            REMOTE_TERMINAL_VIEWPORT_CLAIM => self.handle_terminal_viewport_claim_msg(msg),
            REMOTE_TERMINAL_VIEWPORT_RESIZE => self.handle_terminal_viewport_resize_msg(msg),
            REMOTE_TERMINAL_VIEWPORT_RELEASE => self.handle_terminal_viewport_release_msg(msg),
            REMOTE_TERMINAL_VIEWPORT_SCROLL => self.handle_terminal_viewport_scroll_msg(msg),
            _ => {}
        }
    }
}
