#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalOscEvent {
    Progress(TerminalProgressOscState),
    Notification(TerminalNotificationKind),
    Command(TerminalCommandOscState),
    Title(TerminalTitleAgentSignal),
}

// Agent state readable from OSC 0 titles: codex renders a leading braille
// spinner frame while a turn runs and a fixed "Action Required" prefix while
// blocked on input (title_setup defaults ["activity", "project-name"]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalTitleAgentSignal {
    Working,
    Waiting,
    Plain,
}

// OSC 133 semantic marks emitted by Codux's staged shell integration:
// A = prompt, C = command started, D = command finished (B is ignored).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalCommandOscState {
    Prompt,
    Started,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalProgressOscState {
    Completed,
    Working,
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerminalNotificationKind {
    ApprovalRequested,
    PlanModePrompt,
}

// Notification payloads ("Approval requested: …") can span PTY reads; anything
// longer than this without a terminator is not one of ours and gets dropped.
const MAX_UNTERMINATED_OSC: usize = 512;

const TERMINAL_PROGRESS_OSC_PREFIX: &[u8] = b"\x1b]9;";
const TERMINAL_COMMAND_OSC_PREFIX: &[u8] = b"\x1b]133;";
const TERMINAL_TITLE_OSC_PREFIX: &[u8] = b"\x1b]0;";

#[derive(Debug, Default)]
pub(super) struct TerminalOscParser {
    scan_tail: Vec<u8>,
}

impl TerminalOscParser {
    pub(super) fn push(&mut self, bytes: &[u8]) -> Vec<TerminalOscEvent> {
        if bytes.is_empty() {
            return Vec::new();
        }
        if self.scan_tail.is_empty() && !bytes.contains(&0x1b) {
            return Vec::new();
        }
        let mut scan = Vec::with_capacity(self.scan_tail.len() + bytes.len());
        scan.extend_from_slice(&self.scan_tail);
        scan.extend_from_slice(bytes);

        let mut events = Vec::new();
        let mut index = 0;
        let mut consumed_until = 0;
        while index < scan.len() {
            let Some(relative) = scan[index..].iter().position(|byte| *byte == 0x1b) else {
                consumed_until = scan.len();
                break;
            };
            index += relative;
            let Some(rest) = scan.get(index..) else {
                break;
            };
            if TERMINAL_PROGRESS_OSC_PREFIX.starts_with(rest)
                || TERMINAL_COMMAND_OSC_PREFIX.starts_with(rest)
                || TERMINAL_TITLE_OSC_PREFIX.starts_with(rest)
            {
                consumed_until = index;
                break;
            }
            let (prefix_len, kind) = if rest.starts_with(TERMINAL_PROGRESS_OSC_PREFIX) {
                (TERMINAL_PROGRESS_OSC_PREFIX.len(), OscPrefixKind::Progress)
            } else if rest.starts_with(TERMINAL_COMMAND_OSC_PREFIX) {
                (TERMINAL_COMMAND_OSC_PREFIX.len(), OscPrefixKind::Command)
            } else if rest.starts_with(TERMINAL_TITLE_OSC_PREFIX) {
                (TERMINAL_TITLE_OSC_PREFIX.len(), OscPrefixKind::Title)
            } else {
                index += 1;
                consumed_until = index;
                continue;
            };
            let Some((payload, terminator_len)) = terminal_osc_payload(&rest[prefix_len..]) else {
                consumed_until = index;
                break;
            };
            match kind {
                OscPrefixKind::Command => {
                    if let Some(state) = terminal_command_osc_state(payload) {
                        events.push(TerminalOscEvent::Command(state));
                    }
                }
                OscPrefixKind::Title => {
                    events.push(TerminalOscEvent::Title(terminal_title_agent_signal(
                        payload,
                    )));
                }
                OscPrefixKind::Progress => {
                    if let Some(state) = terminal_progress_osc_state(payload) {
                        events.push(TerminalOscEvent::Progress(state));
                    } else if let Some(kind) = terminal_notification_kind(payload) {
                        events.push(TerminalOscEvent::Notification(kind));
                    }
                }
            }
            index += prefix_len + payload.len() + terminator_len;
            consumed_until = index;
        }

        // The tail is only ever a partial prefix or an unterminated OSC; keep it
        // whole (truncating would chop the ESC prefix and lose the event) or,
        // past the cap, drop it entirely so the buffer stays bounded.
        let tail = &scan[consumed_until.min(scan.len())..];
        self.scan_tail.clear();
        if tail.len() <= MAX_UNTERMINATED_OSC {
            self.scan_tail.extend_from_slice(tail);
        }
        events
    }
}

fn terminal_osc_payload(body: &[u8]) -> Option<(&[u8], usize)> {
    if let Some(end) = body.iter().position(|byte| *byte == 0x07) {
        return Some((&body[..end], 1));
    }
    if let Some(end) = body.windows(2).position(|bytes| bytes == b"\x1b\\") {
        return Some((&body[..end], 2));
    }
    None
}

fn terminal_progress_osc_state(payload: &[u8]) -> Option<TerminalProgressOscState> {
    let progress = payload.strip_prefix(b"4;")?;
    let code_end = progress
        .iter()
        .position(|byte| *byte == b';')
        .unwrap_or(progress.len());
    let code = std::str::from_utf8(&progress[..code_end]).ok()?.trim();
    match code {
        "0" => Some(TerminalProgressOscState::Completed),
        "1" | "3" => Some(TerminalProgressOscState::Working),
        "2" => Some(TerminalProgressOscState::Error),
        "4" => Some(TerminalProgressOscState::Warning),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum OscPrefixKind {
    Progress,
    Command,
    Title,
}

fn terminal_title_agent_signal(payload: &[u8]) -> TerminalTitleAgentSignal {
    // Blink phases toggle "!" and "." but keep the prefix text stable.
    if payload.starts_with(b"[ ! ] Action Required")
        || payload.starts_with(b"[ . ] Action Required")
    {
        return TerminalTitleAgentSignal::Waiting;
    }
    // A braille spinner frame (U+2800–U+28FF, encoded E2 A0..A3 xx) anywhere in
    // the title counts as busy: the spinner item's position is user-configurable
    // and E2 never appears as a continuation byte, so the pair is unambiguous.
    if payload
        .windows(2)
        .any(|pair| pair[0] == 0xE2 && (0xA0..=0xA3).contains(&pair[1]))
    {
        return TerminalTitleAgentSignal::Working;
    }
    TerminalTitleAgentSignal::Plain
}

fn terminal_command_osc_state(payload: &[u8]) -> Option<TerminalCommandOscState> {
    let kind_end = payload
        .iter()
        .position(|byte| *byte == b';')
        .unwrap_or(payload.len());
    match &payload[..kind_end] {
        b"A" => Some(TerminalCommandOscState::Prompt),
        b"C" => Some(TerminalCommandOscState::Started),
        b"D" => Some(TerminalCommandOscState::Finished),
        _ => None,
    }
}

fn terminal_notification_kind(payload: &[u8]) -> Option<TerminalNotificationKind> {
    let message = std::str::from_utf8(payload)
        .ok()?
        .trim()
        .to_ascii_lowercase();
    if message.starts_with("approval requested:")
        || message.starts_with("approval requested by ")
        || message.starts_with("codex wants to edit ")
    {
        return Some(TerminalNotificationKind::ApprovalRequested);
    }
    if message.starts_with("plan mode prompt:") {
        return Some(TerminalNotificationKind::PlanModePrompt);
    }
    None
}
