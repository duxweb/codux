//! Consumer-side terminal output orchestration.
//!
//! This is the state machine that decides, for each incoming `terminal.output`
//! envelope, whether it is a baseline, a duplicate, a sequence gap, or live
//! output to append, and which effects the UI must run (ack, loading,
//! buffer-received, session-updated, baseline-resync). It owns the per-session
//! remote PTY state, the output sequencer, the chunk assembler, and the buffer-
//! request bookkeeping.
//!
//! Previously this lived in the mobile app (Dart); it is ported here so the
//! shared core is the single home for remote PTY restore logic. Held live
//! frames are stored directly as the raw envelope JSON (`RemotePtySession<String>`),
//! so there is no token indirection.

use std::collections::{HashMap, HashSet};

use base64::{Engine as _, engine::general_purpose};
use serde_json::{Value, json};

use crate::{RemotePtySession, TerminalBufferAssembler, TerminalOutputSequencer, TerminalSequence};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteTerminalBufferPhase {
    Idle,
    Requesting,
    Receiving,
    Rendering,
}

impl RemoteTerminalBufferPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Requesting => "requesting",
            Self::Receiving => "receiving",
            Self::Rendering => "rendering",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteTerminalOutputEffectKind {
    Loading,
    Ack,
    MarkBufferReceived,
    SessionUpdated,
    RequestBaselineResync,
}

impl RemoteTerminalOutputEffectKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Ack => "ack",
            Self::MarkBufferReceived => "markBufferReceived",
            Self::SessionUpdated => "sessionUpdated",
            Self::RequestBaselineResync => "requestBaselineResync",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteTerminalOutputEffect {
    pub kind: RemoteTerminalOutputEffectKind,
    pub session_id: Option<String>,
    pub output_seq: Option<TerminalSequence>,
    pub buffer_length: Option<i64>,
    pub progress: Option<f64>,
    pub phase: Option<RemoteTerminalBufferPhase>,
    pub loading: bool,
}

impl RemoteTerminalOutputEffect {
    fn loading(loading: bool, phase: RemoteTerminalBufferPhase, progress: Option<f64>) -> Self {
        Self {
            kind: RemoteTerminalOutputEffectKind::Loading,
            session_id: None,
            output_seq: None,
            buffer_length: None,
            progress,
            phase: Some(phase),
            loading,
        }
    }

    fn ack(
        session_id: &str,
        output_seq: Option<TerminalSequence>,
        buffer_length: Option<i64>,
    ) -> Self {
        Self {
            kind: RemoteTerminalOutputEffectKind::Ack,
            session_id: Some(session_id.to_string()),
            output_seq,
            buffer_length,
            progress: None,
            phase: None,
            loading: false,
        }
    }

    fn simple(kind: RemoteTerminalOutputEffectKind, session_id: &str) -> Self {
        Self {
            kind,
            session_id: Some(session_id.to_string()),
            output_seq: None,
            buffer_length: None,
            progress: None,
            phase: None,
            loading: false,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "sessionId": self.session_id,
            "outputSeq": self.output_seq,
            "bufferLength": self.buffer_length,
            "progress": self.progress,
            "phase": self.phase.map(RemoteTerminalBufferPhase::as_str),
            "loading": self.loading,
        })
    }
}

struct DecodedPayload {
    data: String,
    is_buffer: bool,
    screen_data: Option<String>,
    offset: Option<i64>,
    buffer_length: Option<i64>,
    tail: bool,
}

pub struct RemoteTerminalOutputRouter {
    sessions: HashMap<String, RemotePtySession<String>>,
    /// Most-recently-bound session id is last; used for LRU eviction.
    order: Vec<String>,
    max_cached_chars: usize,
    sequencer: TerminalOutputSequencer,
    assembler: TerminalBufferAssembler,
    active_buffer_request_by_session: HashMap<String, String>,
    restore_buffer_request_ids: HashSet<String>,
    gap_sessions: HashSet<String>,
    /// Per-session render generation, bumped whenever the screen could change.
    /// The UI caches the decoded screen snapshot keyed by this so it only
    /// re-decodes after a real mutation.
    render_gen: HashMap<String, u64>,
}

impl RemoteTerminalOutputRouter {
    pub fn new(max_buffer_chars: usize, max_cached_chars: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            order: Vec::new(),
            max_cached_chars,
            sequencer: TerminalOutputSequencer::new(),
            assembler: TerminalBufferAssembler::new(max_buffer_chars),
            active_buffer_request_by_session: HashMap::new(),
            restore_buffer_request_ids: HashSet::new(),
            gap_sessions: HashSet::new(),
            render_gen: HashMap::new(),
        }
    }

    fn bump_render(&mut self, session_id: &str) {
        *self.render_gen.entry(session_id.to_string()).or_insert(0) += 1;
    }

    pub fn render_generation(&self, session_id: &str) -> u64 {
        self.render_gen.get(session_id).copied().unwrap_or(0)
    }

    // ---- render path (screen operations on the owned sessions) ----------

    pub fn screen_snapshot_json(&self, session_id: &str) -> Option<String> {
        let session = self.sessions.get(session_id)?;
        serde_json::to_string(&session.screen_snapshot()).ok()
    }

    pub fn resize_screen(&mut self, session_id: &str, cols: usize, rows: usize) {
        self.ensure_session(session_id).resize_screen(cols, rows);
        self.bump_render(session_id);
    }

    pub fn scroll_screen_lines(&mut self, session_id: &str, lines: i32) {
        self.ensure_session(session_id).scroll_screen_lines(lines);
        self.bump_render(session_id);
    }

    pub fn scroll_screen_pixels(&mut self, session_id: &str, pixels: f64, cell_height: f64) {
        self.ensure_session(session_id)
            .scroll_screen_pixels(pixels, cell_height);
        self.bump_render(session_id);
    }

    pub fn settle_screen_pixel_scroll(&mut self, session_id: &str) {
        self.ensure_session(session_id).settle_screen_pixel_scroll();
        self.bump_render(session_id);
    }

    pub fn scroll_screen_to_bottom(&mut self, session_id: &str) {
        self.ensure_session(session_id).scroll_screen_to_bottom();
        self.bump_render(session_id);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_host_scroll(
        &mut self,
        session_id: &str,
        screen_data: &str,
        cols: usize,
        rows: usize,
        display_offset: usize,
        total_lines: usize,
        margin_rows: usize,
        margin_rows_below: usize,
    ) {
        self.ensure_session(session_id).apply_host_scroll_snapshot(
            screen_data,
            cols,
            rows,
            display_offset,
            total_lines,
            margin_rows,
            margin_rows_below,
        );
        self.bump_render(session_id);
    }

    // ---- session access -------------------------------------------------

    fn touch(&mut self, session_id: &str) {
        self.order.retain(|id| id != session_id);
        self.order.push(session_id.to_string());
    }

    fn session(&mut self, session_id: &str) -> &mut RemotePtySession<String> {
        if !self.sessions.contains_key(session_id) {
            self.sessions.insert(
                session_id.to_string(),
                RemotePtySession::new(session_id, self.max_cached_chars),
            );
            self.order.push(session_id.to_string());
        }
        self.sessions.get_mut(session_id).expect("session exists")
    }

    pub fn content(&self, session_id: &str) -> Option<&str> {
        self.sessions
            .get(session_id)
            .map(|session| session.content())
            .filter(|content| !content.is_empty())
    }

    pub fn native_render_content(&self, session_id: &str) -> Option<&str> {
        self.sessions
            .get(session_id)
            .map(|session| session.native_render_content())
            .filter(|content| !content.is_empty())
    }

    pub fn has_cached_output(&self, session_id: &str) -> bool {
        self.content(session_id).is_some()
    }

    pub fn buffer_offset(&self, session_id: &str) -> usize {
        self.sessions
            .get(session_id)
            .map(|session| session.buffer_length())
            .unwrap_or(0)
    }

    pub fn sequence_for(&self, session_id: &str) -> TerminalSequence {
        self.sessions
            .get(session_id)
            .map(|session| session.sequence())
            .unwrap_or(0)
    }

    pub fn has_sequence_gap(&self, session_id: &str) -> bool {
        self.gap_sessions.contains(session_id)
    }

    pub fn session_ref(&self, session_id: &str) -> Option<&RemotePtySession<String>> {
        self.sessions.get(session_id)
    }

    pub fn session_mut(&mut self, session_id: &str) -> Option<&mut RemotePtySession<String>> {
        self.sessions.get_mut(session_id)
    }

    /// Create-if-absent and return a mutable handle (for the render path:
    /// resize/scroll/snapshot on the active session).
    pub fn ensure_session(&mut self, session_id: &str) -> &mut RemotePtySession<String> {
        self.touch(session_id);
        self.session(session_id)
    }

    // ---- lifecycle ------------------------------------------------------

    pub fn start_buffer_request(
        &mut self,
        session_id: &str,
        request_id: &str,
        require_baseline: bool,
        reset_assembler: bool,
        replace_active: bool,
    ) -> bool {
        if session_id.trim().is_empty() || request_id.trim().is_empty() {
            return false;
        }
        if let Some(active) = self
            .active_buffer_request_by_session
            .get(session_id)
            .cloned()
        {
            if active != request_id {
                if !replace_active {
                    return false;
                }
                self.restore_buffer_request_ids
                    .remove(&buffer_request_key(session_id, &active));
            }
        }
        self.active_buffer_request_by_session
            .insert(session_id.to_string(), request_id.to_string());
        let key = buffer_request_key(session_id, request_id);
        if require_baseline {
            self.restore_buffer_request_ids.insert(key);
        } else {
            self.restore_buffer_request_ids.remove(&key);
        }
        if reset_assembler {
            self.assembler.remove(session_id);
        }
        if require_baseline {
            self.session(session_id).require_baseline();
        }
        true
    }

    pub fn bind_session(&mut self, session_id: &str, require_baseline: bool) {
        if session_id.trim().is_empty() {
            return;
        }
        self.sequencer.remove(session_id);
        self.gap_sessions.remove(session_id);
        self.assembler.remove(session_id);
        if let Some(stale) = self.active_buffer_request_by_session.remove(session_id) {
            self.restore_buffer_request_ids
                .remove(&buffer_request_key(session_id, &stale));
        }
        let session = self.session(session_id);
        if require_baseline {
            session.require_baseline();
        } else {
            session.reset_transient(false);
        }
        self.touch(session_id);
    }

    pub fn remove_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
        self.order.retain(|id| id != session_id);
        self.active_buffer_request_by_session.remove(session_id);
        self.restore_buffer_request_ids
            .retain(|key| !key.starts_with(&format!("{session_id}:")));
        self.assembler.remove(session_id);
        self.sequencer.remove(session_id);
        self.gap_sessions.remove(session_id);
        self.render_gen.remove(session_id);
    }

    /// Evict least-recently-bound inactive sessions beyond `max_sessions`,
    /// never evicting `active_session_id`. Returns the evicted ids.
    pub fn evict_inactive_sessions(
        &mut self,
        active_session_id: &str,
        max_sessions: usize,
    ) -> Vec<String> {
        if active_session_id.trim().is_empty() {
            return Vec::new();
        }
        self.touch(active_session_id);
        let max_sessions = max_sessions.max(1);
        let mut evicted = Vec::new();
        while self.sessions.len() > max_sessions {
            let victim = self
                .order
                .iter()
                .find(|id| id.as_str() != active_session_id)
                .cloned();
            let Some(victim) = victim else { break };
            self.remove_session(&victim);
            evicted.push(victim);
        }
        evicted
    }

    pub fn reset_transient(&mut self) {
        self.assembler.reset();
        self.active_buffer_request_by_session.clear();
        self.restore_buffer_request_ids.clear();
    }

    pub fn reset_session_transient(&mut self, session_id: &str, reset_sequence: bool) {
        self.assembler.remove(session_id);
        if reset_sequence {
            self.sequencer.remove(session_id);
            self.gap_sessions.remove(session_id);
        }
        self.session(session_id).reset_transient(reset_sequence);
    }

    pub fn reset_all(&mut self) {
        self.sessions.clear();
        self.order.clear();
        self.active_buffer_request_by_session.clear();
        self.restore_buffer_request_ids.clear();
        self.gap_sessions.clear();
        self.assembler.reset();
        self.sequencer.reset();
        self.render_gen.clear();
    }

    pub fn active_buffer_request_id(&self, session_id: &str) -> Option<&str> {
        self.active_buffer_request_by_session
            .get(session_id)
            .map(String::as_str)
    }

    pub fn has_active_buffer_request(&self, session_id: &str) -> bool {
        self.active_buffer_request_by_session
            .contains_key(session_id)
    }

    // ---- accept ---------------------------------------------------------

    pub fn accept(
        &mut self,
        message: &Value,
        active_session_id: Option<&str>,
    ) -> Vec<RemoteTerminalOutputEffect> {
        self.accept_inner(message, active_session_id, false)
    }

    fn accept_inner(
        &mut self,
        message: &Value,
        active_session_id: Option<&str>,
        replaying_held_live: bool,
    ) -> Vec<RemoteTerminalOutputEffect> {
        let mut payload = match message.get("payload") {
            Some(payload)
                if payload.is_object()
                    && payload.get("data").is_some_and(|value| !value.is_null()) =>
            {
                payload.clone()
            }
            _ => return Vec::new(),
        };
        let session_id = match message.get("sessionId").and_then(Value::as_str) {
            Some(session_id) if !session_id.trim().is_empty() => session_id.to_string(),
            _ => return Vec::new(),
        };
        let session_id = session_id.as_str();
        let is_active_session = active_session_id == Some(session_id);
        let had_cached_output_at_start = self.has_cached_output(session_id);
        let incoming_request_id = payload_string(&payload, "requestId");
        let active_request_id = self
            .active_buffer_request_by_session
            .get(session_id)
            .cloned();

        let is_buffer_flag = payload.get("buffer").and_then(Value::as_bool) == Some(true);

        if is_buffer_flag
            && active_request_id.is_some()
            && (had_cached_output_at_start || incoming_request_id.is_some())
            && incoming_request_id != active_request_id
        {
            return vec![RemoteTerminalOutputEffect::ack(
                session_id,
                payload_int(&payload, "outputSeq"),
                payload_int(&payload, "bufferLength"),
            )];
        }
        if is_buffer_flag
            && active_request_id.is_none()
            && incoming_request_id.as_deref().map(str::is_empty) == Some(false)
            && self.has_cached_output(session_id)
        {
            return vec![RemoteTerminalOutputEffect::ack(
                session_id,
                payload_int(&payload, "outputSeq"),
                payload_int(&payload, "bufferLength"),
            )];
        }
        if is_buffer_flag
            && active_request_id.is_none()
            && incoming_request_id.as_deref().map(str::is_empty) == Some(false)
        {
            let request_id = incoming_request_id.clone().unwrap();
            self.active_buffer_request_by_session
                .insert(session_id.to_string(), request_id.clone());
            if self.content(session_id).is_none() {
                self.restore_buffer_request_ids
                    .insert(buffer_request_key(session_id, &request_id));
            }
        }
        if is_buffer_flag
            && had_cached_output_at_start
            && self
                .active_buffer_request_by_session
                .get(session_id)
                .is_none()
        {
            let payload_offset =
                payload_int(&payload, "startOffset").or_else(|| payload_int(&payload, "offset"));
            let payload_buffer_length = payload_int(&payload, "bufferLength");
            let payload_output_seq = payload_int(&payload, "outputSeq");
            let known_output_seq = self.sequencer.sequence_for(session_id);
            if payload_offset == Some(0)
                && payload_output_seq.is_some()
                && payload_output_seq.unwrap() <= known_output_seq
            {
                self.assembler.remove(session_id);
                return vec![RemoteTerminalOutputEffect::ack(
                    session_id,
                    payload_output_seq,
                    payload_buffer_length,
                )];
            }
            let buffer_offset = self.buffer_offset(session_id) as i64;
            if (payload_offset == Some(0)
                && payload_buffer_length.is_some()
                && payload_buffer_length.unwrap() > buffer_offset)
                || payload_offset.map(|offset| offset > 0) == Some(true)
            {
                self.assembler.remove(session_id);
                return vec![RemoteTerminalOutputEffect::ack(
                    session_id,
                    payload_int(&payload, "outputSeq"),
                    payload_buffer_length,
                )];
            }
        }

        let assembly = self.assembler.accept(session_id, payload.clone());
        if !assembly.ready {
            if assembly.progress.is_none() {
                return Vec::new();
            }
            if !is_active_session {
                return Vec::new();
            }
            return vec![RemoteTerminalOutputEffect::loading(
                true,
                RemoteTerminalBufferPhase::Receiving,
                assembly.progress,
            )];
        }
        payload = assembly.payload.unwrap_or(payload);
        let decoded = decode_terminal_output_payload(&payload);
        let raw = decoded.data.clone();
        let is_buffer = decoded.is_buffer;
        let output_seq = payload_int(&payload, "outputSeq");

        let active_request_id_after_assembly = self
            .active_buffer_request_by_session
            .get(session_id)
            .cloned();
        if is_buffer
            && had_cached_output_at_start
            && active_request_id_after_assembly.is_none()
            && decoded.offset == Some(0)
            && decoded.buffer_length.is_some()
            && decoded.buffer_length.unwrap() > self.buffer_offset(session_id) as i64
        {
            self.assembler.remove(session_id);
            return vec![RemoteTerminalOutputEffect::ack(
                session_id,
                output_seq,
                decoded.buffer_length,
            )];
        }
        if is_buffer
            && had_cached_output_at_start
            && active_request_id_after_assembly.is_none()
            && decoded.offset.map(|offset| offset > 0) == Some(true)
        {
            self.assembler.remove(session_id);
            return vec![RemoteTerminalOutputEffect::ack(
                session_id,
                output_seq,
                decoded.buffer_length,
            )];
        }

        // Hold live output that arrives before the baseline is restored.
        if !replaying_held_live
            && !is_buffer
            && decoded.screen_data.is_none()
            && self
                .session(session_id)
                .hold_live(output_seq, message.to_string())
        {
            return vec![RemoteTerminalOutputEffect::ack(
                session_id,
                output_seq,
                decoded.buffer_length,
            )];
        }

        if !is_active_session && !is_buffer {
            let resync = self
                .sequencer
                .observe(session_id, false, output_seq, None, false);
            let mut held_live: Vec<String> = Vec::new();
            if resync.should_render() && (!raw.is_empty() || decoded.screen_data.is_some()) {
                held_live = self.apply_live_to_session(
                    session_id,
                    &raw,
                    decoded.screen_data.as_deref(),
                    decoded.buffer_length,
                    output_seq,
                );
                if decoded.screen_data.is_some() {
                    self.active_buffer_request_by_session.remove(session_id);
                    self.remove_restore_request(session_id);
                    self.assembler.remove(session_id);
                }
            }
            let mut effects = Vec::new();
            if resync.gap && self.gap_sessions.insert(session_id.to_string()) {
                effects.push(RemoteTerminalOutputEffect::simple(
                    RemoteTerminalOutputEffectKind::RequestBaselineResync,
                    session_id,
                ));
            }
            effects.push(RemoteTerminalOutputEffect::ack(
                session_id,
                output_seq,
                decoded.buffer_length,
            ));
            self.replay_held(held_live, active_session_id, &mut effects);
            return effects;
        }

        let resync = self.sequencer.observe(
            session_id,
            is_buffer,
            output_seq,
            decoded
                .offset
                .and_then(|offset| usize::try_from(offset).ok()),
            decoded.tail,
        );
        if !resync.should_render() {
            return vec![RemoteTerminalOutputEffect::ack(
                session_id,
                output_seq,
                decoded.buffer_length,
            )];
        }

        let mut effects = Vec::new();
        if resync.gap && self.gap_sessions.insert(session_id.to_string()) {
            effects.push(RemoteTerminalOutputEffect::simple(
                RemoteTerminalOutputEffectKind::RequestBaselineResync,
                session_id,
            ));
        }
        let mut held_live: Vec<String> = Vec::new();

        if is_buffer {
            let active_request_id = self
                .active_buffer_request_by_session
                .get(session_id)
                .cloned();
            let local_cache_empty = self.content(session_id).is_none();
            let is_restore_request = active_request_id
                .as_ref()
                .map(|id| {
                    self.restore_buffer_request_ids
                        .contains(&buffer_request_key(session_id, id))
                })
                .unwrap_or(false);
            let is_baseline_restore = decoded.tail
                || self.session(session_id).is_restoring_baseline()
                || is_restore_request
                || local_cache_empty;
            let baseline_has_renderable_data = !raw.is_empty() || decoded.screen_data.is_some();
            if is_baseline_restore && !baseline_has_renderable_data {
                if is_active_session && active_request_id.is_some() {
                    effects.push(RemoteTerminalOutputEffect::loading(
                        true,
                        RemoteTerminalBufferPhase::Requesting,
                        None,
                    ));
                }
                effects.push(RemoteTerminalOutputEffect::ack(
                    session_id,
                    output_seq,
                    decoded.buffer_length,
                ));
                return effects;
            }
            if is_baseline_restore {
                held_live = self.replace_session_from_baseline(
                    session_id,
                    &raw,
                    decoded.screen_data.as_deref(),
                    decoded.buffer_length,
                    output_seq,
                );
            }

            if is_baseline_restore || self.content(session_id).is_none() {
                if !is_baseline_restore {
                    self.replace_session_from_baseline(
                        session_id,
                        &raw,
                        decoded.screen_data.as_deref(),
                        decoded.buffer_length,
                        output_seq,
                    );
                }
                self.active_buffer_request_by_session.remove(session_id);
                self.remove_restore_request(session_id);
                if is_active_session {
                    effects.push(RemoteTerminalOutputEffect::simple(
                        RemoteTerminalOutputEffectKind::SessionUpdated,
                        session_id,
                    ));
                    effects.push(RemoteTerminalOutputEffect::simple(
                        RemoteTerminalOutputEffectKind::MarkBufferReceived,
                        session_id,
                    ));
                }
            } else {
                held_live = self.apply_live_to_session(
                    session_id,
                    &raw,
                    decoded.screen_data.as_deref(),
                    decoded.buffer_length,
                    output_seq,
                );
                self.active_buffer_request_by_session.remove(session_id);
                self.remove_restore_request(session_id);
                if is_active_session {
                    effects.push(RemoteTerminalOutputEffect::simple(
                        RemoteTerminalOutputEffectKind::SessionUpdated,
                        session_id,
                    ));
                    effects.push(RemoteTerminalOutputEffect::simple(
                        RemoteTerminalOutputEffectKind::MarkBufferReceived,
                        session_id,
                    ));
                }
            }
        } else if (!raw.is_empty() || decoded.screen_data.is_some()) && is_active_session {
            effects.push(RemoteTerminalOutputEffect::loading(
                false,
                RemoteTerminalBufferPhase::Requesting,
                None,
            ));
        }

        if !is_buffer && (!raw.is_empty() || decoded.screen_data.is_some()) {
            held_live = self.apply_live_to_session(
                session_id,
                &raw,
                decoded.screen_data.as_deref(),
                decoded.buffer_length,
                output_seq,
            );
            if decoded.screen_data.is_some() {
                self.active_buffer_request_by_session.remove(session_id);
                self.remove_restore_request(session_id);
                self.assembler.remove(session_id);
            }
            if is_active_session {
                effects.push(RemoteTerminalOutputEffect::simple(
                    RemoteTerminalOutputEffectKind::SessionUpdated,
                    session_id,
                ));
            }
        }

        effects.push(RemoteTerminalOutputEffect::ack(
            session_id,
            output_seq,
            decoded.buffer_length,
        ));

        self.replay_held(held_live, active_session_id, &mut effects);
        effects
    }

    fn replay_held(
        &mut self,
        held_live: Vec<String>,
        active_session_id: Option<&str>,
        effects: &mut Vec<RemoteTerminalOutputEffect>,
    ) {
        for held in held_live {
            if let Ok(message) = serde_json::from_str::<Value>(&held) {
                let mut more = self.accept_inner(&message, active_session_id, true);
                effects.append(&mut more);
            }
        }
    }

    fn remove_restore_request(&mut self, session_id: &str) {
        let prefix = format!("{session_id}:");
        self.restore_buffer_request_ids
            .retain(|key| !key.starts_with(&prefix));
    }

    fn replace_session_from_baseline(
        &mut self,
        session_id: &str,
        data: &str,
        screen_data: Option<&str>,
        buffer_length: Option<i64>,
        output_seq: Option<TerminalSequence>,
    ) -> Vec<String> {
        self.gap_sessions.remove(session_id);
        let replay = self.session(session_id).replace_from_baseline_screen(
            data,
            screen_data,
            buffer_length.and_then(|value| usize::try_from(value).ok()),
            output_seq,
        );
        self.bump_render(session_id);
        replay
    }

    fn apply_live_to_session(
        &mut self,
        session_id: &str,
        data: &str,
        screen_data: Option<&str>,
        buffer_length: Option<i64>,
        output_seq: Option<TerminalSequence>,
    ) -> Vec<String> {
        let buffer_len = buffer_length.and_then(|value| usize::try_from(value).ok());
        let awaiting = self.session(session_id).is_restoring_baseline();
        if screen_data.is_some() && awaiting {
            let existing = self
                .content(session_id)
                .map(str::to_string)
                .unwrap_or_default();
            let combined = format!("{existing}{data}");
            let replay = self.session(session_id).replace_from_baseline_screen(
                &combined,
                screen_data,
                buffer_len,
                output_seq,
            );
            self.bump_render(session_id);
            return replay;
        }
        self.session(session_id)
            .append_live_screen(data, screen_data, buffer_len, output_seq);
        self.bump_render(session_id);
        Vec::new()
    }
}

fn buffer_request_key(session_id: &str, request_id: &str) -> String {
    format!("{session_id}:{request_id}")
}

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    let text = payload.get(key)?;
    let text = match text {
        Value::String(value) => value.trim().to_string(),
        Value::Null => return None,
        other => other.to_string(),
    };
    if text.is_empty() { None } else { Some(text) }
}

fn payload_int(payload: &Value, key: &str) -> Option<i64> {
    let value = payload.get(key)?;
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_f64().map(|value| value as i64))
        .or_else(|| {
            value
                .as_str()
                .and_then(|value| value.trim().parse::<i64>().ok())
        })
}

fn decode_terminal_output_payload(payload: &Value) -> DecodedPayload {
    DecodedPayload {
        data: decode_data(payload),
        is_buffer: payload.get("buffer").and_then(Value::as_bool) == Some(true),
        screen_data: payload_string(payload, "screenData"),
        offset: payload_int(payload, "offset"),
        buffer_length: payload_int(payload, "bufferLength"),
        tail: payload.get("tail").and_then(Value::as_bool) == Some(true),
    }
}

fn decode_data(payload: &Value) -> String {
    let value = payload
        .get("data")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if payload.get("compressed").and_then(Value::as_bool) != Some(true) {
        return value;
    }
    if payload.get("encoding").and_then(Value::as_str) != Some("base64+deflate+utf8") {
        return value;
    }
    let Ok(compressed) = general_purpose::URL_SAFE_NO_PAD.decode(value.trim_end_matches('='))
    else {
        return String::new();
    };
    use std::io::Read;
    let mut decoder = flate2::read::DeflateDecoder::new(compressed.as_slice());
    let mut output = String::new();
    if decoder.read_to_string(&mut output).is_err() {
        return String::new();
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(effects: &[RemoteTerminalOutputEffect]) -> Vec<&'static str> {
        effects.iter().map(|effect| effect.kind.as_str()).collect()
    }

    fn buffer(
        session: &str,
        data: &str,
        offset: i64,
        buffer_length: i64,
        truncated: bool,
        output_seq: i64,
        request_id: Option<&str>,
    ) -> Value {
        let mut payload = json!({
            "data": data,
            "buffer": true,
            "offset": offset,
            "bufferLength": buffer_length,
            "truncated": truncated,
            "outputSeq": output_seq,
        });
        if let Some(request_id) = request_id {
            payload["requestId"] = json!(request_id);
        }
        json!({ "type": "terminal.output", "sessionId": session, "payload": payload })
    }

    fn live(session: &str, data: &str, output_seq: i64) -> Value {
        json!({
            "type": "terminal.output",
            "sessionId": session,
            "payload": { "data": data, "outputSeq": output_seq },
        })
    }

    fn buffer_with_screen_data(
        session: &str,
        data: &str,
        screen_data: &str,
        output_seq: i64,
    ) -> Value {
        json!({
            "type": "terminal.output",
            "sessionId": session,
            "payload": {
                "data": data,
                "screenData": screen_data,
                "buffer": true,
                "offset": 0,
                "bufferLength": data.len(),
                "tail": true,
                "outputSeq": output_seq,
            },
        })
    }

    #[test]
    fn truncated_baseline_renders_retained_window() {
        let mut router = RemoteTerminalOutputRouter::new(4, 65536);
        router.bind_session("session-1", true);
        let result = router.accept(
            &buffer("session-1", "abcd", 0, 8, true, 10, None),
            Some("session-1"),
        );
        assert_eq!(
            kinds(&result),
            ["sessionUpdated", "markBufferReceived", "ack"]
        );
        assert_eq!(router.content("session-1"), Some("abcd"));
        assert_eq!(router.buffer_offset("session-1"), 8);
    }

    #[test]
    fn live_output_held_until_baseline() {
        let mut router = RemoteTerminalOutputRouter::new(4, 65536);
        router.bind_session("session-1", true);
        let held = router.accept(&live("session-1", "new", 11), Some("session-1"));
        assert_eq!(kinds(&held), ["ack"]);
        assert_eq!(router.content("session-1"), None);

        let first = router.accept(
            &buffer("session-1", "old-", 0, 8, true, 10, None),
            Some("session-1"),
        );
        assert_eq!(
            kinds(&first),
            [
                "sessionUpdated",
                "markBufferReceived",
                "ack",
                "loading",
                "sessionUpdated",
                "ack"
            ]
        );
        assert_eq!(router.content("session-1"), Some("old-new"));
    }

    #[test]
    fn native_render_content_is_raw_history_without_keyframe() {
        let mut router = RemoteTerminalOutputRouter::new(65536, 65536);
        router.bind_session("session-1", true);

        // The baseline keyframe updates only the cell-grid screen; the native
        // render content stays the raw history so the emulator can rebuild its
        // scrollback (splicing the keyframe's ESC[2J would erase it).
        router.accept(
            &buffer_with_screen_data("session-1", "raw-history", "\x1b[2J\x1b[Hkeyframe", 10),
            Some("session-1"),
        );

        assert_eq!(router.content("session-1"), Some("raw-history"));
        assert_eq!(
            router.native_render_content("session-1"),
            Some("raw-history")
        );

        // Live output appends its raw bytes; native render content tracks the
        // raw history exactly.
        router.accept(&live("session-1", "\nlive", 11), Some("session-1"));

        assert_eq!(router.content("session-1"), Some("raw-history\nlive"));
        assert_eq!(
            router.native_render_content("session-1"),
            Some("raw-history\nlive")
        );

        // A live frame that also carries a fresh screen keyframe still only
        // appends its raw bytes -- the keyframe never enters the stream.
        router.accept(
            &json!({
                "type": "terminal.output",
                "sessionId": "session-1",
                "payload": {
                    "data": "\nworking",
                    "screenData": "\x1b[2J\x1b[Hupdated keyframe",
                    "outputSeq": 12,
                },
            }),
            Some("session-1"),
        );

        assert_eq!(
            router.content("session-1"),
            Some("raw-history\nlive\nworking")
        );
        assert_eq!(
            router.native_render_content("session-1"),
            Some("raw-history\nlive\nworking")
        );
    }

    #[test]
    fn empty_live_screen_keyframe_leaves_native_render_content_untouched() {
        let mut router = RemoteTerminalOutputRouter::new(65536, 65536);
        router.bind_session("session-1", true);

        router.accept(
            &buffer_with_screen_data("session-1", "raw-history", "\x1b[2J\x1b[Hold", 10),
            Some("session-1"),
        );

        // An empty-data live keyframe refreshes only the cell-grid screen used
        // by the scroll path; the raw native render content is left as-is.
        let effects = router.accept(
            &json!({
                "type": "terminal.output",
                "sessionId": "session-1",
                "payload": {
                    "data": "",
                    "screenData": "\x1b[2J\x1b[Hnew",
                    "bufferLength": 11,
                    "outputSeq": 11,
                },
            }),
            Some("session-1"),
        );

        assert_eq!(kinds(&effects), ["loading", "sessionUpdated", "ack"]);
        assert_eq!(router.content("session-1"), Some("raw-history"));
        assert_eq!(
            router.native_render_content("session-1"),
            Some("raw-history")
        );
    }

    #[test]
    fn empty_refresh_baseline_preserves_cached_native_render_content() {
        let mut router = RemoteTerminalOutputRouter::new(65536, 65536);
        router.bind_session("session-1", false);
        router.accept(
            &buffer_with_screen_data("session-1", "history", "\x1b[2J\x1b[Hscreen", 10),
            Some("session-1"),
        );
        assert_eq!(
            router.native_render_content("session-1"),
            Some("history")
        );

        assert!(router.start_buffer_request("session-1", "refresh-empty", true, true, true));
        let empty = router.accept(
            &buffer("session-1", "", 0, 0, false, 11, Some("refresh-empty")),
            Some("session-1"),
        );

        assert_eq!(kinds(&empty), ["loading", "ack"]);
        assert_eq!(
            router.native_render_content("session-1"),
            Some("history")
        );
        assert_eq!(
            router.active_buffer_request_id("session-1"),
            Some("refresh-empty")
        );
    }

    #[test]
    fn tail_refresh_replaces_raw_native_render_content_for_cached_session() {
        let mut router = RemoteTerminalOutputRouter::new(65536, 65536);
        router.bind_session("session-1", true);
        router.accept(
            &buffer_with_screen_data("session-1", "raw-history", "\x1b[2J\x1b[Hold", 10),
            Some("session-1"),
        );

        assert!(router.start_buffer_request("session-1", "tail-refresh", false, true, true));
        let refresh = router.accept(
            &json!({
                "type": "terminal.output",
                "sessionId": "session-1",
                "payload": {
                    "data": "raw-history\nnew",
                    "screenData": "\x1b[2J\x1b[Hnew",
                    "buffer": true,
                    "offset": 0,
                    "bufferLength": 15,
                    "tail": true,
                    "outputSeq": 10,
                    "requestId": "tail-refresh",
                },
            }),
            Some("session-1"),
        );

        assert_eq!(
            kinds(&refresh),
            ["sessionUpdated", "markBufferReceived", "ack"]
        );
        assert_eq!(router.content("session-1"), Some("raw-history\nnew"));
        assert_eq!(
            router.native_render_content("session-1"),
            Some("raw-history\nnew")
        );
    }

    #[test]
    fn live_sequence_gap_requests_resync_once() {
        let mut router = RemoteTerminalOutputRouter::new(4, 65536);
        router.bind_session("session-1", false);
        router.accept(&live("session-1", "one", 1), Some("session-1"));
        assert!(!router.has_sequence_gap("session-1"));

        let skipped = router.accept(&live("session-1", "three", 3), Some("session-1"));
        assert_eq!(
            kinds(&skipped),
            ["requestBaselineResync", "loading", "sessionUpdated", "ack"]
        );
        assert_eq!(router.content("session-1"), Some("onethree"));
        assert!(router.has_sequence_gap("session-1"));

        let next = router.accept(&live("session-1", "six", 6), Some("session-1"));
        assert!(!kinds(&next).contains(&"requestBaselineResync"));
    }

    #[test]
    fn stale_request_id_baseline_cannot_replace() {
        let mut router = RemoteTerminalOutputRouter::new(4, 65536);
        router.bind_session("session-1", true);
        router.start_buffer_request("session-1", "request-new", false, true, false);

        let stale = router.accept(
            &buffer("session-1", "old", 0, 3, false, 10, Some("request-old")),
            Some("session-1"),
        );
        assert_eq!(kinds(&stale), ["ack"]);
        assert_eq!(router.content("session-1"), None);

        let current = router.accept(
            &buffer("session-1", "new", 0, 3, false, 10, Some("request-new")),
            Some("session-1"),
        );
        assert_eq!(
            kinds(&current),
            ["sessionUpdated", "markBufferReceived", "ack"]
        );
        assert_eq!(router.content("session-1"), Some("new"));
    }

    #[test]
    fn duplicate_baseline_is_acked() {
        let mut router = RemoteTerminalOutputRouter::new(65536, 65536);
        router.bind_session("session-1", false);
        router.accept(
            &buffer("session-1", "cached", 0, 6, false, 10, None),
            Some("session-1"),
        );
        let result = router.accept(
            &buffer("session-1", "old-prefix", 0, 400000, true, 697, None),
            Some("session-1"),
        );
        assert_eq!(kinds(&result), ["ack"]);
        assert_eq!(router.content("session-1"), Some("cached"));
        assert_eq!(router.buffer_offset("session-1"), 6);
    }

    #[test]
    fn inactive_live_output_updates_cache_without_render() {
        let mut router = RemoteTerminalOutputRouter::new(4, 65536);
        let result = router.accept(&live("session-2", "background", 1), Some("session-1"));
        assert_eq!(kinds(&result), ["ack"]);
        assert_eq!(router.content("session-2"), Some("background"));
        assert_eq!(router.content("session-1"), None);
    }

    #[test]
    fn evicts_inactive_sessions_keeping_active() {
        let mut router = RemoteTerminalOutputRouter::new(4, 65536);
        for id in ["s1", "s2", "s3"] {
            router.accept(&live(id, "x", 1), Some(id));
        }
        let evicted = router.evict_inactive_sessions("s3", 2);
        assert_eq!(evicted.len(), 1);
        assert!(router.session_ref("s3").is_some());
        assert!(!evicted.contains(&"s3".to_string()));
    }
}
