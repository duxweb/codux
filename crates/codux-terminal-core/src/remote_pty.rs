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

    /// The byte stream fed to the native terminal emulator: the raw PTY
    /// history only. The screen `screenData` keyframe is deliberately NOT
    /// spliced in here -- it is a synthetic full-screen repaint (it begins
    /// with `ESC[H ESC[2J` and paints by absolute cursor positioning), so
    /// replaying it mid-stream erases the scrollback rendered above it. The
    /// keyframe updates only the cell-grid `keyframe_screen` used by the
    /// scroll/snapshot path.
    pub fn native_render_content(&self) -> &str {
        &self.content
    }

    pub fn buffer_length(&self) -> usize {
        self.buffer_length
    }

    pub fn sequence(&self) -> TerminalSequence {
        self.sequence
    }

    pub fn is_restoring_baseline(&self) -> bool {
        self.awaiting_baseline
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
    /// `margin_rows_below` rows at the bottom are context below it. `rows`
    /// is the full host-rendered snapshot height, already including those
    /// margins.
    pub fn apply_host_scroll_snapshot(
        &mut self,
        screen_data: &str,
        cols: usize,
        rows: usize,
        display_offset: usize,
        total_lines: usize,
        margin_rows: usize,
        margin_rows_below: usize,
    ) {
        if screen_data.is_empty() {
            return;
        }
        let cols = cols.max(1);
        let rows = rows.max(1);
        self.host_total_lines = Some(total_lines);
        if display_offset == 0 && margin_rows == 0 && margin_rows_below == 0 {
            // At the live bottom the keyframe screen stays in charge so
            // live output keeps rendering.
            self.keyframe_screen.resize(cols, rows);
            self.keyframe_screen
                .replace_with_keyframe(screen_data.as_bytes());
            self.has_keyframe_screen = true;
            self.screen_view = RemotePtyScreenView::Keyframe;
            self.host_scroll = None;
            return;
        }
        self.scroll_screen.resize(cols, rows);
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
        self.held_sequenced_live.clear();
        self.held_unsequenced_live.clear();
    }

    pub fn reset_transient(&mut self, reset_sequence: bool) {
        self.awaiting_baseline = false;
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
        self.content.clear();
        self.content.push_str(content);
        trim_cache_buffer(&mut self.content, self.max_cached_chars);
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
            self.apply_keyframe_screen(screen_data);
        } else {
            self.has_keyframe_screen = false;
            self.screen_view = RemotePtyScreenView::History;
        }
        let base_sequence = sequence.unwrap_or(self.sequence);
        self.sequence = base_sequence;
        self.awaiting_baseline = false;

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
        if matches!(self.host_scroll, Some((0, _, _, _))) {
            self.host_scroll = None;
            self.scroll_screen.clear();
            self.screen_view = if self.has_keyframe_screen {
                RemotePtyScreenView::Keyframe
            } else {
                RemotePtyScreenView::History
            };
        }
        if !data.is_empty() {
            // The native render content is the raw PTY history; live output
            // only ever appends its bytes. The screen keyframe is never
            // spliced in (see `native_render_content`), so the stream stays a
            // clean scrollback that the emulator extends, and the consumer
            // feeds only the delta instead of re-rendering the whole buffer.
            push_cache_buffer(&mut self.content, data, self.max_cached_chars);
            self.history_screen.process(data.as_bytes());
            if self.screen_view == RemotePtyScreenView::Keyframe && !self.has_keyframe_screen {
                self.history_screen.scroll_to_bottom();
            }
        }
        if let Some(screen_data) = screen_data.filter(|data| !data.is_empty()) {
            // Refresh only the cell-grid keyframe screen (consumed by the
            // scroll / snapshot path); the raw native render stream above is
            // left untouched.
            self.apply_keyframe_screen(screen_data);
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

    /// Apply a screen keyframe to the cell-grid `keyframe_screen` used by the
    /// scroll/snapshot path. It is never spliced into `native_render_content`
    /// (the raw history stream), so it cannot erase the emulator's scrollback.
    fn apply_keyframe_screen(&mut self, screen_data: &str) {
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

/// Trailing line budget for the cached raw history and native ANSI replay.
///
/// The native terminal emulator (iOS SwiftTerm / Android) keeps its own
/// ~500-line scrollback, so caching far more than it can hold only makes the
/// full re-feed on a session switch needlessly large (the emulator parses it
/// all and then discards everything past its scrollback). Bounding the cache
/// a little above that scrollback keeps a switch's `replace` small while still
/// fully repopulating the emulator. Deeper history is served by the host on
/// demand (`apply_host_scroll`), not from this cache.
const MAX_CACHED_LINES: usize = 600;

/// Append `data` to `buffer`, then trim the front to the cache budget. Appends
/// in place (no per-frame reallocation of the whole buffer).
fn push_cache_buffer(buffer: &mut String, data: &str, max_chars: usize) {
    buffer.push_str(data);
    trim_cache_buffer(buffer, max_chars);
}

/// Trim the front of `buffer` in place so it keeps at most [`MAX_CACHED_LINES`]
/// trailing newline-delimited lines and at most `max_chars` characters.
///
/// The line budget is the primary bound -- it matches the native emulator's
/// scrollback so a restore re-feeds only what the emulator can hold.
/// `max_chars` is a safety ceiling that also bounds pathologically long lines.
/// Both scans are bounded by the size of the retained window (~600 lines), not
/// the whole buffer, so the steady-state live path stays amortized O(appended
/// bytes) rather than O(buffer length) per frame.
fn trim_cache_buffer(buffer: &mut String, max_chars: usize) {
    if max_chars == 0 {
        buffer.clear();
        return;
    }
    let bytes = buffer.as_bytes();
    let len = bytes.len();

    // Line budget: walk back from the end until we have passed
    // MAX_CACHED_LINES newlines, then cut just after that newline (always a
    // UTF-8 and line boundary, so the kept stream starts at a clean line).
    let mut cut = 0usize;
    let mut seen = 0usize;
    let mut i = len;
    while i > 0 {
        i -= 1;
        if bytes[i] == b'\n' {
            seen += 1;
            if seen > MAX_CACHED_LINES {
                cut = i + 1;
                break;
            }
        }
    }

    // Char ceiling: only scan when the retained window is still over the byte
    // ceiling (bytes >= chars), then drop to 7/8 of the ceiling so the
    // pathological long-line case re-trims rarely rather than every frame.
    if len - cut > max_chars {
        let remaining = &buffer[cut..];
        let total = remaining.chars().count();
        if total > max_chars {
            let target = max_chars.saturating_sub(max_chars / 8).max(1);
            let drop = total - target;
            let extra = remaining
                .char_indices()
                .nth(drop)
                .map(|(index, _)| index)
                .unwrap_or(remaining.len());
            cut += extra;
        }
    }

    if cut > 0 {
        buffer.drain(..cut);
    }
}

