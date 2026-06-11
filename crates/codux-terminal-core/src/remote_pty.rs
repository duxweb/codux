use std::collections::BTreeMap;

use crate::{HeadlessTerminalScreen, TerminalScreenSnapshot, TerminalSequence};

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
    screen: HeadlessTerminalScreen,
    awaiting_baseline: bool,
    page_buffer: Option<RemotePtyPageBuffer>,
    held_sequenced_live: BTreeMap<TerminalSequence, T>,
    held_unsequenced_live: Vec<T>,
}

impl<T> RemotePtySession<T> {
    pub fn new(session_id: impl Into<String>, max_cached_chars: usize) -> Self {
        Self {
            session_id: session_id.into(),
            max_cached_chars,
            content: String::new(),
            buffer_length: 0,
            sequence: 0,
            screen: HeadlessTerminalScreen::new(80, 24, 2_000),
            awaiting_baseline: false,
            page_buffer: None,
            held_sequenced_live: BTreeMap::new(),
            held_unsequenced_live: Vec::new(),
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
        self.screen.snapshot()
    }

    pub fn resize_screen(&mut self, cols: usize, rows: usize) {
        self.screen.resize(cols, rows);
    }

    pub fn scroll_screen_lines(&mut self, lines: i32) {
        self.screen.scroll_lines(lines);
    }

    pub fn scroll_screen_to_bottom(&mut self) {
        self.screen.scroll_to_bottom();
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
        } else {
            self.held_unsequenced_live.push(output);
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
        self.screen.clear();
        if !content.is_empty() {
            self.screen.process(content.as_bytes());
        }
        if let Some(screen_data) = screen_data.filter(|data| !data.is_empty()) {
            self.screen.process(screen_data.as_bytes());
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
            self.screen.process(data.as_bytes());
        }
        if let Some(screen_data) = screen_data.filter(|data| !data.is_empty()) {
            self.screen.process(screen_data.as_bytes());
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
        self.screen.clear();
        self.reset_transient(false);
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
