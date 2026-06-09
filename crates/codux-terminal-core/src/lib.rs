use std::collections::BTreeMap;

pub type TerminalSequence = i64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalBaselineRequest {
    pub session_id: String,
    pub request_id: Option<String>,
    pub offset: usize,
    pub max_chars: usize,
    pub chunk_chars: Option<usize>,
    pub tail: bool,
    pub resume_from_seq: Option<TerminalSequence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePtySnapshot {
    pub session_id: String,
    pub content: String,
    pub buffer_length: usize,
    pub sequence: TerminalSequence,
}

#[derive(Debug, Clone)]
pub struct RemotePtySession<T> {
    session_id: String,
    max_cached_chars: usize,
    content: String,
    buffer_length: usize,
    sequence: TerminalSequence,
    awaiting_snapshot: bool,
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
            awaiting_snapshot: false,
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

    pub fn is_restoring_snapshot(&self) -> bool {
        self.awaiting_snapshot || self.page_buffer.is_some()
    }

    pub fn snapshot(&self) -> RemotePtySnapshot {
        RemotePtySnapshot {
            session_id: self.session_id.clone(),
            content: self.content.clone(),
            buffer_length: self.buffer_length,
            sequence: self.sequence,
        }
    }

    pub fn require_snapshot(&mut self) {
        self.awaiting_snapshot = true;
        self.page_buffer = None;
        self.held_sequenced_live.clear();
        self.held_unsequenced_live.clear();
    }

    pub fn reset_transient(&mut self, reset_sequence: bool) {
        self.awaiting_snapshot = false;
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
        if !self.awaiting_snapshot {
            return false;
        }
        if let Some(sequence) = sequence {
            self.held_sequenced_live.entry(sequence).or_insert(output);
        } else {
            self.held_unsequenced_live.push(output);
        }
        true
    }

    pub fn accept_snapshot_page(
        &mut self,
        data: &str,
        offset: usize,
        buffer_length: Option<usize>,
        truncated: bool,
    ) -> RemotePtySnapshotPageResult {
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

    pub fn replace_from_snapshot(
        &mut self,
        content: &str,
        buffer_length: Option<usize>,
        sequence: Option<TerminalSequence>,
    ) -> Vec<T> {
        self.content = trim_to_char_limit(content, self.max_cached_chars);
        if let Some(buffer_length) = buffer_length {
            self.buffer_length = buffer_length;
        }
        let base_sequence = sequence.unwrap_or(self.sequence);
        self.sequence = base_sequence;
        self.awaiting_snapshot = false;
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
        if !data.is_empty() {
            self.content =
                trim_to_char_limit(&format!("{}{}", self.content, data), self.max_cached_chars);
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
        self.reset_transient(false);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemotePtySnapshotPageResult {
    pub accepted: bool,
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
    ) -> RemotePtySnapshotPageResult {
        if self.buffer_length.is_none() {
            self.buffer_length = buffer_length;
        }
        if offset != self.next_offset {
            return RemotePtySnapshotPageResult {
                accepted: false,
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
        RemotePtySnapshotPageResult {
            accepted: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_snapshot_before_replaying_held_live_output() {
        let mut session = RemotePtySession::new("session-1", 64);
        session.require_snapshot();

        assert!(session.hold_live(Some(11), "stale"));
        assert!(session.hold_live(Some(12), "new"));

        let page = session.accept_snapshot_page("abcd", 0, Some(8), true);
        assert!(page.accepted);
        assert!(!page.ready);
        assert_eq!(page.next_offset, 4);

        let page = session.accept_snapshot_page("efgh", 4, Some(8), false);
        assert!(page.ready);

        let replay = session.replace_from_snapshot(&page.data, Some(8), Some(11));
        assert_eq!(session.content(), "abcdefgh");
        assert_eq!(replay, vec!["new"]);
    }

    #[test]
    fn rejects_out_of_order_snapshot_pages() {
        let mut session = RemotePtySession::<String>::new("session-1", 64);
        session.require_snapshot();

        let page = session.accept_snapshot_page("abcd", 0, Some(8), true);
        assert!(page.accepted);

        let page = session.accept_snapshot_page("gh", 6, Some(8), false);
        assert!(!page.accepted);
        assert_eq!(page.next_offset, 4);
    }

    #[test]
    fn trims_cache_on_character_boundaries() {
        let mut session = RemotePtySession::<String>::new("session-1", 4);

        session.append_live("a你好bcd", Some(7), Some(2));

        assert_eq!(session.content(), "好bcd");
        assert_eq!(session.buffer_length(), 7);
        assert_eq!(session.sequence(), 2);
    }
}
