use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalInputSnapshot {
    pub bytes: usize,
    pub history: Vec<TerminalCapturedInput>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalCapturedInput {
    pub text: String,
    pub bytes: usize,
    pub timestamp: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalOutputSnapshot {
    pub bytes: usize,
    pub tail: String,
}

pub(super) struct CaptureReader {
    session_id: String,
    inner: Box<dyn Read + Send>,
    output_capture: Arc<parking_lot::Mutex<TerminalOutputCapture>>,
    history: Arc<parking_lot::Mutex<RingHistory>>,
    screen: Arc<parking_lot::Mutex<HeadlessTerminalScreen>>,
    output_subscribers: Arc<parking_lot::Mutex<Vec<flume::Sender<Vec<u8>>>>>,
    event_subscribers: Arc<parking_lot::Mutex<Vec<EventSubscriber>>>,
    info: Arc<parking_lot::Mutex<TerminalSessionSnapshot>>,
    pending_utf8: Vec<u8>,
    raw_capture: Option<std::fs::File>,
}

// Debug lever: CODUX_TERMINAL_CAPTURE=<dir|1> appends each session's raw PTY
// output to terminal-<session>.ans for offline replay of rendering bugs.
fn raw_capture_file(session_id: &str) -> Option<std::fs::File> {
    let value = std::env::var("CODUX_TERMINAL_CAPTURE").ok()?;
    let value = value.trim();
    if value.is_empty() || value == "0" || value.eq_ignore_ascii_case("false") {
        return None;
    }
    let candidate = std::path::PathBuf::from(value);
    let dir = if candidate.is_dir() {
        candidate
    } else {
        std::env::temp_dir().join("codux-terminal-capture")
    };
    std::fs::create_dir_all(&dir).ok()?;
    let name: String = session_id
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' })
        .collect();
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("terminal-{name}.ans")))
        .ok()
}

impl CaptureReader {
    pub(super) fn new(
        session_id: String,
        inner: Box<dyn Read + Send>,
        output_capture: Arc<parking_lot::Mutex<TerminalOutputCapture>>,
        history: Arc<parking_lot::Mutex<RingHistory>>,
        screen: Arc<parking_lot::Mutex<HeadlessTerminalScreen>>,
        output_subscribers: Arc<parking_lot::Mutex<Vec<flume::Sender<Vec<u8>>>>>,
        event_subscribers: Arc<parking_lot::Mutex<Vec<EventSubscriber>>>,
        info: Arc<parking_lot::Mutex<TerminalSessionSnapshot>>,
    ) -> Self {
        let raw_capture = raw_capture_file(&session_id);
        Self {
            session_id,
            inner,
            output_capture,
            history,
            screen,
            output_subscribers,
            event_subscribers,
            info,
            pending_utf8: Vec::new(),
            raw_capture,
        }
    }
}

impl Read for CaptureReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        if read == 0 {
            self.flush_pending_utf8();
            return Ok(0);
        }
        if read > 0 {
            let bytes = &buf[..read];
            if let Some(file) = self.raw_capture.as_mut() {
                let _ = file.write_all(bytes);
            }
            self.output_capture.lock().push(bytes);
            self.screen.lock().process(bytes);
            self.broadcast_output(bytes);
            let text = decode_utf8_output(bytes, &mut self.pending_utf8);
            if !text.is_empty() {
                let mut history = self.history.lock();
                history.push_text(&text);
                let chars = history.len_chars();
                let mut info = self.info.lock();
                info.last_active_at = rfc3339_now();
                info.buffer_characters = chars;
                info.has_buffer = chars > 0;
            }
            emit_terminal_event(
                &self.event_subscribers,
                TerminalEvent::Output {
                    session_id: self.session_id.clone(),
                    text,
                    bytes: bytes.to_vec(),
                },
            );
        }
        Ok(read)
    }
}

impl CaptureReader {
    pub(super) fn flush_pending_utf8(&mut self) {
        let text = flush_utf8_decoder(&mut self.pending_utf8);
        if text.is_empty() {
            return;
        }
        let bytes = text.as_bytes().to_vec();
        {
            let mut history = self.history.lock();
            history.push_text(&text);
            let chars = history.len_chars();
            let mut info = self.info.lock();
            info.last_active_at = rfc3339_now();
            info.buffer_characters = chars;
            info.has_buffer = chars > 0;
        }
        self.broadcast_output(&bytes);
        emit_terminal_event(
            &self.event_subscribers,
            TerminalEvent::Output {
                session_id: self.session_id.clone(),
                text,
                bytes,
            },
        );
    }

    pub(super) fn broadcast_output(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut subscribers = self.output_subscribers.lock();
        subscribers.retain(|subscriber| subscriber.send(bytes.to_vec()).is_ok());
    }
}

pub(super) struct CaptureWriter {
    inner: Arc<parking_lot::Mutex<Box<dyn Write + Send>>>,
    capture: Arc<parking_lot::Mutex<TerminalInputCapture>>,
}

impl CaptureWriter {
    pub(super) fn new(
        inner: Arc<parking_lot::Mutex<Box<dyn Write + Send>>>,
        capture: Arc<parking_lot::Mutex<TerminalInputCapture>>,
    ) -> Self {
        Self { inner, capture }
    }
}

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.lock().write(buf)?;
        if written > 0 {
            self.capture.lock().push(&buf[..written]);
        }
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.lock().flush()
    }
}

pub(super) struct TerminalOutputCapture {
    total_bytes: usize,
    limit: usize,
    tail: VecDeque<u8>,
}

impl TerminalOutputCapture {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            total_bytes: 0,
            limit,
            tail: VecDeque::with_capacity(limit.min(4096)),
        }
    }

    pub(super) fn push(&mut self, bytes: &[u8]) {
        self.total_bytes = self.total_bytes.saturating_add(bytes.len());
        if self.limit == 0 {
            return;
        }
        for byte in bytes {
            self.tail.push_back(*byte);
            while self.tail.len() > self.limit {
                self.tail.pop_front();
            }
        }
    }

    pub(super) fn snapshot(&self) -> TerminalOutputSnapshot {
        let bytes = self.tail.iter().copied().collect::<Vec<_>>();
        TerminalOutputSnapshot {
            bytes: self.total_bytes,
            tail: String::from_utf8_lossy(&bytes).to_string(),
        }
    }
}

pub(super) struct TerminalInputCapture {
    total_bytes: usize,
    limit: usize,
    history: VecDeque<TerminalCapturedInput>,
}

impl TerminalInputCapture {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            total_bytes: 0,
            limit,
            history: VecDeque::with_capacity(limit.min(8)),
        }
    }

    pub(super) fn push(&mut self, bytes: &[u8]) {
        self.total_bytes = self.total_bytes.saturating_add(bytes.len());
        if self.limit == 0 {
            return;
        }
        let text = String::from_utf8_lossy(bytes).to_string();
        if text.trim().is_empty() {
            return;
        }
        self.history.push_back(TerminalCapturedInput {
            text,
            bytes: bytes.len(),
            timestamp: now_seconds(),
        });
        while self.history.len() > self.limit {
            self.history.pop_front();
        }
    }

    pub(super) fn snapshot(&self) -> TerminalInputSnapshot {
        TerminalInputSnapshot {
            bytes: self.total_bytes,
            history: self.history.iter().cloned().collect(),
        }
    }
}

pub(super) struct RingHistory {
    max_bytes: usize,
    len_bytes: usize,
    len_chars: usize,
    chunks: VecDeque<String>,
}

impl RingHistory {
    pub(super) fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            len_bytes: 0,
            len_chars: 0,
            chunks: VecDeque::new(),
        }
    }

    pub(super) fn clear(&mut self) {
        self.len_bytes = 0;
        self.len_chars = 0;
        self.chunks.clear();
    }

    pub(super) fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let chunk = text.to_string();
        self.len_bytes += chunk.len();
        self.len_chars += chunk.chars().count();
        self.chunks.push_back(chunk);

        while self.len_bytes > self.max_bytes {
            if let Some(chunk) = self.chunks.pop_front() {
                self.len_bytes = self.len_bytes.saturating_sub(chunk.len());
                self.len_chars = self.len_chars.saturating_sub(chunk.chars().count());
            } else {
                break;
            }
        }
    }

    pub(super) fn to_text(&self) -> String {
        let mut text = String::with_capacity(self.len_bytes);
        for chunk in &self.chunks {
            text.push_str(chunk);
        }
        text
    }

    pub(super) fn tail_text(&self, max_chars: usize) -> (String, usize) {
        if max_chars == 0 || self.len_chars <= max_chars {
            return (self.to_text(), 0);
        }
        let text = self.to_text();
        let start_chars = self.len_chars.saturating_sub(max_chars);
        let start_byte = byte_index_for_char_offset(&text, start_chars);
        let safe_start_byte = ansi_safe_snapshot_start(&text, start_byte);
        let safe_start_chars = text[..safe_start_byte].chars().count();
        (text[safe_start_byte..].to_string(), safe_start_chars)
    }

    pub(super) fn len_chars(&self) -> usize {
        self.len_chars
    }
}

pub(super) fn byte_index_for_char_offset(text: &str, char_offset: usize) -> usize {
    if char_offset == 0 {
        return 0;
    }
    text.char_indices()
        .nth(char_offset)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AnsiSequenceState {
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
    String,
    StringEscape,
}

pub(super) fn ansi_safe_snapshot_start(text: &str, start_byte: usize) -> usize {
    let bytes = text.as_bytes();
    let mut state = AnsiSequenceState::Ground;
    let mut index = 0;
    while index < start_byte {
        state = ansi_sequence_next_state(state, bytes[index]);
        index += 1;
    }
    if state == AnsiSequenceState::Ground {
        return start_byte;
    }
    while index < bytes.len() {
        state = ansi_sequence_next_state(state, bytes[index]);
        index += 1;
        if state == AnsiSequenceState::Ground {
            return index;
        }
    }
    bytes.len()
}

pub(super) fn ansi_sequence_next_state(state: AnsiSequenceState, byte: u8) -> AnsiSequenceState {
    match state {
        AnsiSequenceState::Ground => match byte {
            0x1b => AnsiSequenceState::Escape,
            0x9b => AnsiSequenceState::Csi,
            0x9d => AnsiSequenceState::Osc,
            0x90 | 0x98 | 0x9e | 0x9f => AnsiSequenceState::String,
            _ => AnsiSequenceState::Ground,
        },
        AnsiSequenceState::Escape => match byte {
            b'[' => AnsiSequenceState::Csi,
            b']' => AnsiSequenceState::Osc,
            b'P' | b'X' | b'^' | b'_' => AnsiSequenceState::String,
            0x20..=0x2f => AnsiSequenceState::Escape,
            _ => AnsiSequenceState::Ground,
        },
        AnsiSequenceState::Csi => {
            if (0x40..=0x7e).contains(&byte) {
                AnsiSequenceState::Ground
            } else {
                AnsiSequenceState::Csi
            }
        }
        AnsiSequenceState::Osc => match byte {
            0x07 => AnsiSequenceState::Ground,
            0x1b => AnsiSequenceState::OscEscape,
            _ => AnsiSequenceState::Osc,
        },
        AnsiSequenceState::OscEscape => {
            if byte == b'\\' {
                AnsiSequenceState::Ground
            } else if byte == 0x1b {
                AnsiSequenceState::OscEscape
            } else {
                AnsiSequenceState::Osc
            }
        }
        AnsiSequenceState::String => match byte {
            0x07 => AnsiSequenceState::Ground,
            0x1b => AnsiSequenceState::StringEscape,
            _ => AnsiSequenceState::String,
        },
        AnsiSequenceState::StringEscape => {
            if byte == b'\\' {
                AnsiSequenceState::Ground
            } else if byte == 0x1b {
                AnsiSequenceState::StringEscape
            } else {
                AnsiSequenceState::String
            }
        }
    }
}

pub(super) fn terminal_history_bytes(scrollback_lines: Option<usize>, cols: u16) -> usize {
    let lines = scrollback_lines.unwrap_or(500).clamp(200, 10_000);
    usize::from(cols.max(20))
        .saturating_mul(lines)
        .saturating_mul(4)
        .clamp(MIN_HISTORY_BYTES, MAX_CONFIGURED_HISTORY_BYTES)
}

pub(super) fn remote_screen_scrollback_lines(scrollback_lines: Option<usize>) -> usize {
    scrollback_lines
        .unwrap_or(5000)
        .min(REMOTE_SCREEN_SCROLLBACK_CAP)
}

pub(super) fn initial_remote_screen_scrollback_lines(active_scrollback: usize) -> usize {
    REMOTE_SCREEN_IDLE_SCROLLBACK.min(active_scrollback)
}

pub(super) fn decode_utf8_output(bytes: &[u8], pending: &mut Vec<u8>) -> String {
    if pending.is_empty() {
        return decode_utf8_complete_prefix(bytes, pending);
    }
    pending.extend_from_slice(bytes);
    let combined = std::mem::take(pending);
    decode_utf8_complete_prefix(&combined, pending)
}

pub(super) fn decode_utf8_complete_prefix(bytes: &[u8], pending: &mut Vec<u8>) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(error) => {
            let valid_up_to = error.valid_up_to();
            let (valid, rest) = bytes.split_at(valid_up_to);
            if error.error_len().is_none() {
                pending.extend_from_slice(rest);
                return String::from_utf8_lossy(valid).to_string();
            }
            String::from_utf8_lossy(bytes).to_string()
        }
    }
}

pub(super) fn flush_utf8_decoder(pending: &mut Vec<u8>) -> String {
    if pending.is_empty() {
        String::new()
    } else {
        String::from_utf8_lossy(&std::mem::take(pending)).to_string()
    }
}
