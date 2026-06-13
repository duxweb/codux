use std::collections::BTreeMap;

use crate::{HeadlessTerminalScreen, TerminalScreenSnapshot, TerminalSequence};

/// Upper bound on live frames held while awaiting a baseline. A baseline that
/// never arrives (host torn down mid-request) would otherwise let the held
/// buffers grow without limit; past the cap we drop the oldest held frames.
const MAX_HELD_LIVE: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePtySnapshot {
    pub session_id: String,
    pub content: String,
    pub buffer_length: usize,
    pub sequence: TerminalSequence,
}

pub struct RemotePtySession<T> {
    session_id: String,
    max_cached_chars: usize,
    content: String,
    buffer_length: usize,
    sequence: TerminalSequence,
    history_screen: HeadlessTerminalScreen,
    keyframe_screen: HeadlessTerminalScreen,
    has_keyframe_screen: bool,
    screen_view: RemotePtyScreenView,
    awaiting_baseline: bool,
    page_buffer: Option<RemotePtyPageBuffer>,
    held_sequenced_live: BTreeMap<TerminalSequence, T>,
    held_unsequenced_live: Vec<T>,
    // Scrollback state served by the host screen (display offset / total
    // lines / overscan margins above and below of the last host-scrolled
    // snapshot). When set, the scroll screen shows a host-rendered history
    // viewport instead of the local raw-byte replay.
    host_scroll: Option<(usize, usize, usize, usize)>,
    // Authoritative scrollback length reported by the host; the local
    // keyframe screen has no scrollback of its own, so this drives the
    // client scroll range even at the live bottom.
    host_total_lines: Option<usize>,
    // Dedicated screen for host-served scroll snapshots: they can be taller
    // than the viewport (overscan margin above) and must not disturb the
    // live keyframe screen.
    scroll_screen: HeadlessTerminalScreen,
    screen_cols: usize,
    screen_rows: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RemotePtyScreenView {
    Keyframe,
    History,
}

impl<T> RemotePtySession<T> {
    pub fn new(session_id: impl Into<String>, max_cached_chars: usize) -> Self {
        Self {
            session_id: session_id.into(),
            max_cached_chars,
            content: String::new(),
            buffer_length: 0,
            sequence: 0,
            history_screen: HeadlessTerminalScreen::new(80, 24, 2_000),
            keyframe_screen: HeadlessTerminalScreen::new(80, 24, 2_000),
            has_keyframe_screen: false,
            screen_view: RemotePtyScreenView::History,
            awaiting_baseline: false,
            page_buffer: None,
            held_sequenced_live: BTreeMap::new(),
            held_unsequenced_live: Vec::new(),
            host_scroll: None,
            host_total_lines: None,
            scroll_screen: HeadlessTerminalScreen::new(80, 24, 0),
            screen_cols: 80,
            screen_rows: 24,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn buffer_length(&self) -> usize {
        self.buffer_length
    }

    pub fn sequence(&self) -> TerminalSequence {
        self.sequence
    }

    pub fn is_restoring_baseline(&self) -> bool {
        self.awaiting_baseline || self.page_buffer.is_some()
    }

    pub fn snapshot(&self) -> RemotePtySnapshot {
        RemotePtySnapshot {
            session_id: self.session_id.clone(),
            content: self.content.clone(),
            buffer_length: self.buffer_length,
            sequence: self.sequence,
        }
    }

    pub fn screen_snapshot(&self) -> TerminalScreenSnapshot {
        if let Some((display_offset, total_lines, margin_rows, margin_rows_below)) =
            self.host_scroll
        {
            let mut snapshot = self.scroll_screen.snapshot();
            snapshot.display_offset = display_offset;
            snapshot.total_lines = total_lines.max(snapshot.rows);
            snapshot.margin_rows = margin_rows;
            snapshot.margin_rows_below = margin_rows_below;
            return snapshot;
        }
        if self.screen_view == RemotePtyScreenView::Keyframe && self.has_keyframe_screen {
            let mut snapshot = self.keyframe_screen.snapshot();
            if let Some(total) = self.host_total_lines {
                snapshot.total_lines = total.max(snapshot.total_lines);
            }
            snapshot
        } else {
            self.history_screen.snapshot()
        }
    }

    /// Apply a host-rendered scrolled viewport (terminal.viewport.scrolled).
    /// The host screen owns the authoritative scrollback at the live grid
    /// size; rendering it replaces the fragile local raw-byte history
    /// replay entirely. `margin_rows` rows at the top of the data are
    /// pre-rendered overscan context above the viewport;
    /// `margin_rows_below` rows at the bottom are context below it.
    pub fn apply_host_scroll_snapshot(
        &mut self,
        screen_data: &str,
        display_offset: usize,
        total_lines: usize,
        margin_rows: usize,
        margin_rows_below: usize,
    ) {
        if screen_data.is_empty() {
            return;
        }
        self.host_total_lines = Some(total_lines);
        if display_offset == 0 {
            // At the live bottom the keyframe screen stays in charge so
            // live output keeps rendering; sync replies (margin > 0) only
            // seed the scroll range recorded above.
            if margin_rows == 0 {
                self.keyframe_screen
                    .replace_with_keyframe(screen_data.as_bytes());
                self.has_keyframe_screen = true;
            }
            self.screen_view = if self.has_keyframe_screen {
                RemotePtyScreenView::Keyframe
            } else {
                self.screen_view
            };
            self.host_scroll = None;
            return;
        }
        self.scroll_screen.resize(
            self.screen_cols,
            self.screen_rows + margin_rows + margin_rows_below,
        );
        self.scroll_screen
            .replace_with_keyframe(screen_data.as_bytes());
        self.host_scroll = Some((display_offset, total_lines, margin_rows, margin_rows_below));
    }

    pub fn resize_screen(&mut self, cols: usize, rows: usize) {
        self.screen_cols = cols;
        self.screen_rows = rows;
        self.history_screen.resize(cols, rows);
        self.keyframe_screen.resize(cols, rows);
    }

    pub fn scroll_screen_lines(&mut self, lines: i32) {
        if lines == 0 {
            return;
        }
        if lines > 0 {
            self.ensure_history_view_at_bottom();
        }
        self.history_screen.scroll_lines(lines);
        self.sync_view_after_history_scroll();
    }

    pub fn scroll_screen_pixels(&mut self, pixels: f64, cell_height: f64) {
        if !pixels.is_finite() || pixels == 0.0 || !cell_height.is_finite() || cell_height <= 0.0 {
            return;
        }
        if pixels > 0.0 {
            self.ensure_history_view_at_bottom();
        }
        self.history_screen.scroll_pixels(pixels, cell_height);
        self.sync_view_after_history_scroll();
    }

    pub fn settle_screen_pixel_scroll(&mut self) {
        self.history_screen.settle_pixel_scroll();
        self.keyframe_screen.settle_pixel_scroll();
        self.sync_view_after_history_scroll();
    }

    pub fn scroll_screen_to_bottom(&mut self) {
        self.history_screen.scroll_to_bottom();
        self.keyframe_screen.scroll_to_bottom();
        self.scroll_screen.clear();
        self.host_scroll = None;
        self.screen_view = if self.has_keyframe_screen {
            RemotePtyScreenView::Keyframe
        } else {
            RemotePtyScreenView::History
        };
    }

    pub fn require_baseline(&mut self) {
        self.awaiting_baseline = true;
        self.page_buffer = None;
        self.held_sequenced_live.clear();
        self.held_unsequenced_live.clear();
    }

    pub fn reset_transient(&mut self, reset_sequence: bool) {
        self.awaiting_baseline = false;
        self.page_buffer = None;
        self.held_sequenced_live.clear();
        self.held_unsequenced_live.clear();
        if reset_sequence {
            self.sequence = 0;
        }
    }

    pub fn set_sequence(&mut self, sequence: TerminalSequence) {
        self.sequence = sequence;
    }

    pub fn hold_live(&mut self, sequence: Option<TerminalSequence>, output: T) -> bool {
        if !self.awaiting_baseline {
            return false;
        }
        if let Some(sequence) = sequence {
            self.held_sequenced_live.entry(sequence).or_insert(output);
            // Drop the oldest held frames past the cap. The baseline replay and
            // sequence-gap resync repair any resulting hole.
            while self.held_sequenced_live.len() > MAX_HELD_LIVE {
                let oldest = *self
                    .held_sequenced_live
                    .keys()
                    .next()
                    .expect("non-empty held buffer");
                self.held_sequenced_live.remove(&oldest);
            }
        } else {
            self.held_unsequenced_live.push(output);
            if self.held_unsequenced_live.len() > MAX_HELD_LIVE {
                let overflow = self.held_unsequenced_live.len() - MAX_HELD_LIVE;
                self.held_unsequenced_live.drain(0..overflow);
            }
        }
        true
    }

    pub fn accept_baseline_page(
        &mut self,
        data: &str,
        offset: usize,
        buffer_length: Option<usize>,
        truncated: bool,
    ) -> RemotePtyBaselinePageResult {
        let mut page_buffer = if offset == 0 || self.page_buffer.is_none() {
            RemotePtyPageBuffer::new(buffer_length, offset)
        } else {
            self.page_buffer.take().expect("page buffer exists")
        };
        let accepted = page_buffer.accept(data, offset, buffer_length, truncated);
        if !accepted.accepted {
            self.page_buffer = None;
            return accepted;
        }
        if accepted.ready {
            self.page_buffer = None;
        } else {
            self.buffer_length = accepted.next_offset;
            self.page_buffer = Some(page_buffer);
        }
        accepted
    }

    pub fn replace_from_baseline(
        &mut self,
        content: &str,
        buffer_length: Option<usize>,
        sequence: Option<TerminalSequence>,
    ) -> Vec<T> {
        self.replace_from_baseline_screen(content, None, buffer_length, sequence)
    }

    pub fn replace_from_baseline_screen(
        &mut self,
        content: &str,
        screen_data: Option<&str>,
        buffer_length: Option<usize>,
        sequence: Option<TerminalSequence>,
    ) -> Vec<T> {
        self.content = trim_to_char_limit(content, self.max_cached_chars);
        if let Some(buffer_length) = buffer_length {
            self.buffer_length = buffer_length;
        }
        self.history_screen.clear();
        if !content.is_empty() {
            self.history_screen.process(content.as_bytes());
            self.history_screen.scroll_to_bottom();
        }
        self.host_scroll = None;
        if let Some(screen_data) = screen_data.filter(|data| !data.is_empty()) {
            self.replace_keyframe_screen(screen_data);
        } else {
            self.has_keyframe_screen = false;
            self.screen_view = RemotePtyScreenView::History;
        }
        let base_sequence = sequence.unwrap_or(self.sequence);
        self.sequence = base_sequence;
        self.awaiting_baseline = false;
        self.page_buffer = None;

        let mut replay = Vec::new();
        let held_sequenced_live = std::mem::take(&mut self.held_sequenced_live);
        for (sequence, output) in held_sequenced_live {
            if sequence > base_sequence {
                replay.push(output);
            }
        }
        replay.append(&mut self.held_unsequenced_live);
        replay
    }

    pub fn append_live(
        &mut self,
        data: &str,
        buffer_length: Option<usize>,
        sequence: Option<TerminalSequence>,
    ) {
        self.append_live_screen(data, None, buffer_length, sequence);
    }

    pub fn append_live_screen(
        &mut self,
        data: &str,
        screen_data: Option<&str>,
        buffer_length: Option<usize>,
        sequence: Option<TerminalSequence>,
    ) {
        if !data.is_empty() {
            self.content =
                trim_to_char_limit(&format!("{}{}", self.content, data), self.max_cached_chars);
            self.history_screen.process(data.as_bytes());
            if self.screen_view == RemotePtyScreenView::Keyframe && !self.has_keyframe_screen {
                self.history_screen.scroll_to_bottom();
            }
        }
        if let Some(screen_data) = screen_data.filter(|data| !data.is_empty()) {
            self.replace_keyframe_screen(screen_data);
        } else if !data.is_empty() && self.has_keyframe_screen {
            self.keyframe_screen.process(data.as_bytes());
            self.keyframe_screen.scroll_to_bottom();
        }
        if let Some(buffer_length) = buffer_length {
            self.buffer_length = buffer_length;
        }
        if let Some(sequence) = sequence {
            self.sequence = sequence;
        }
    }

    pub fn clear(&mut self) {
        self.content.clear();
        self.buffer_length = 0;
        self.sequence = 0;
        self.history_screen.clear();
        self.keyframe_screen.clear();
        self.scroll_screen.clear();
        self.has_keyframe_screen = false;
        self.host_scroll = None;
        self.screen_view = RemotePtyScreenView::History;
        self.reset_transient(false);
    }

    fn replace_keyframe_screen(&mut self, screen_data: &str) {
        self.keyframe_screen
            .replace_with_keyframe(screen_data.as_bytes());
        self.has_keyframe_screen = true;
        if self.screen_view == RemotePtyScreenView::Keyframe
            || self.history_screen.display_offset() == 0
        {
            self.screen_view = RemotePtyScreenView::Keyframe;
        }
    }

    fn ensure_history_view_at_bottom(&mut self) {
        if self.screen_view == RemotePtyScreenView::Keyframe {
            self.history_screen.scroll_to_bottom();
            self.screen_view = RemotePtyScreenView::History;
        }
    }

    fn sync_view_after_history_scroll(&mut self) {
        if self.history_screen.display_offset() == 0 && self.has_keyframe_screen {
            self.screen_view = RemotePtyScreenView::Keyframe;
        } else {
            self.screen_view = RemotePtyScreenView::History;
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemotePtyBaselinePageResult {
    pub accepted: bool,
    pub duplicate: bool,
    pub ready: bool,
    pub data: String,
    pub next_offset: usize,
    pub progress: Option<f64>,
}

#[derive(Debug, Clone)]
struct RemotePtyPageBuffer {
    buffer: String,
    next_offset: usize,
    buffer_length: Option<usize>,
}

impl RemotePtyPageBuffer {
    fn new(buffer_length: Option<usize>, next_offset: usize) -> Self {
        Self {
            buffer: String::new(),
            next_offset,
            buffer_length,
        }
    }

    fn accept(
        &mut self,
        data: &str,
        offset: usize,
        buffer_length: Option<usize>,
        truncated: bool,
    ) -> RemotePtyBaselinePageResult {
        if self.buffer_length.is_none() {
            self.buffer_length = buffer_length;
        }
        if offset != self.next_offset {
            let data_chars = data.chars().count();
            let duplicate = offset.saturating_add(data_chars) <= self.next_offset;
            return RemotePtyBaselinePageResult {
                accepted: false,
                duplicate,
                ready: false,
                data: String::new(),
                next_offset: self.next_offset,
                progress: None,
            };
        }

        self.buffer.push_str(data);
        self.next_offset += data.chars().count();
        let expected_length = buffer_length.or(self.buffer_length);
        let complete_by_length = expected_length
            .map(|length| self.next_offset >= length)
            .unwrap_or(false);
        let ready = !truncated || complete_by_length;
        RemotePtyBaselinePageResult {
            accepted: true,
            duplicate: false,
            ready,
            data: if ready {
                self.buffer.clone()
            } else {
                String::new()
            },
            next_offset: self.next_offset,
            progress: expected_length
                .filter(|length| *length > 0)
                .map(|length| (self.next_offset as f64 / length as f64).clamp(0.0, 1.0)),
        }
    }
}

fn trim_to_char_limit(value: &str, max_chars: usize) -> String {
    let total = value.chars().count();
    if total <= max_chars {
        return value.to_string();
    }
    value.chars().skip(total - max_chars).collect()
}
