use std::collections::BTreeMap;

use crate::{HeadlessTerminalScreen, TerminalScreenSnapshot, TerminalSequence};

/// Upper bound on live frames held while awaiting a baseline. A baseline that
/// never arrives (host torn down mid-request) would otherwise let the held
/// buffers grow without limit; past the cap we drop the oldest held frames.
const MAX_HELD_LIVE: usize = 2048;

pub struct RemotePtySession<T> {
    max_cached_chars: usize,
    content: String,
    buffer_length: usize,
    sequence: TerminalSequence,
    history_screen: HeadlessTerminalScreen,
    awaiting_baseline: bool,
    held_sequenced_live: BTreeMap<TerminalSequence, T>,
    held_unsequenced_live: Vec<T>,
}

impl<T> RemotePtySession<T> {
    pub fn new(max_cached_chars: usize) -> Self {
        Self {
            max_cached_chars,
            content: String::new(),
            buffer_length: 0,
            sequence: 0,
            history_screen: HeadlessTerminalScreen::new(80, 24, 2_000),
            awaiting_baseline: false,
            held_sequenced_live: BTreeMap::new(),
            held_unsequenced_live: Vec::new(),
        }
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
        self.awaiting_baseline
    }

    pub fn screen_snapshot(&self) -> TerminalScreenSnapshot {
        // The live view renders the raw-byte history screen, which is reflowed
        // to the consumer's own grid size. The screen `screenData` keyframe is
        // rendered at the *host's* grid size and, when the host viewport differs
        // (e.g. the consumer hasn't claimed/resized the host yet), would paint
        // into only the top rows and leave the rest blank. Rendering the raw
        // history avoids that and matches what the native emulator did.
        self.history_screen.snapshot()
    }

    pub fn resize_screen(&mut self, cols: usize, rows: usize) {
        self.history_screen.resize(cols, rows);
    }

    pub fn scroll_screen_pixels(&mut self, pixels: f64, cell_height: f64) {
        if !pixels.is_finite() || pixels == 0.0 || !cell_height.is_finite() || cell_height <= 0.0 {
            return;
        }
        self.history_screen.scroll_pixels(pixels, cell_height);
    }

    pub fn settle_screen_pixel_scroll(&mut self) {
        self.history_screen.settle_pixel_scroll();
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
        screen_data: Option<&str>,
        screen_wrapped_rows: Option<&[bool]>,
        buffer_length: Option<usize>,
        sequence: Option<TerminalSequence>,
    ) -> Vec<T> {
        // Preserve the user's scroll position across a baseline replace: a
        // resync (e.g. after a dropped frame) rebuilds the buffer, and snapping
        // back to the bottom mid-scroll is jarring. If the user was scrolled up
        // by N lines, restore that distance from the new bottom.
        let prev_offset = self.history_screen.display_offset();
        self.content.clear();
        self.content.push_str(content);
        trim_cache_buffer(&mut self.content, self.max_cached_chars);
        if let Some(buffer_length) = buffer_length {
            self.buffer_length = buffer_length;
        }
        self.history_screen.clear();
        let mut rendered = false;
        if !content.is_empty() {
            self.history_screen.process(content.as_bytes());
            rendered = true;
        }
        // Reconstruct the current screen from the host keyframe. An alt-screen
        // TUI (e.g. Claude) keeps its UI in the alternate buffer, which has no
        // scrollback and is therefore absent from the raw history above; without
        // the keyframe a fresh restore renders blank until the host happens to
        // emit a full repaint. The keyframe carries the active DEC modes and its
        // \x1b[2J clears only the visible screen (alacritty keeps scrollback),
        // so the raw history above stays scrollable.
        if let Some(screen_data) = screen_data
            && !screen_data.is_empty()
        {
            self.history_screen.process_replay(screen_data.as_bytes());
            if let Some(wrapped_rows) = screen_wrapped_rows {
                self.history_screen
                    .restore_visible_wrapped_rows(wrapped_rows);
            }
            rendered = true;
        }
        if rendered {
            self.history_screen.scroll_to_bottom();
            if prev_offset > 0 {
                self.history_screen.scroll_to_offset(prev_offset);
                // A rebuilt buffer can be shorter than the old scroll distance;
                // a clamped restore would strand the view at the very top, so
                // fall back to the bottom when the exact spot no longer exists.
                if self.history_screen.display_offset() != prev_offset {
                    self.history_screen.scroll_to_bottom();
                }
            }
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

    pub fn complete_empty_baseline(&mut self, sequence: Option<TerminalSequence>) -> Vec<T> {
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
        if !data.is_empty() {
            // The live view is the raw PTY history reflowed to the consumer's
            // grid; live output only appends bytes. Follow the bottom only if
            // we were already there, so a user scrolled up into history stays
            // put instead of snapping down.
            let was_at_bottom = self.history_screen.display_offset() == 0;
            push_cache_buffer(&mut self.content, data, self.max_cached_chars);
            self.history_screen.process(data.as_bytes());
            if was_at_bottom {
                self.history_screen.scroll_to_bottom();
            }
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
        self.reset_transient(false);
    }
}

/// Trailing line budget for the cached raw history and native ANSI replay.
///
/// The native terminal emulator (iOS SwiftTerm / Android) keeps its own
/// ~500-line scrollback, so caching far more than it can hold only makes the
/// full re-feed on a session switch needlessly large (the emulator parses it
/// all and then discards everything past its scrollback). Bounding the cache
/// a little above that scrollback keeps a switch's `replace` small while still
/// fully repopulating the emulator.
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
