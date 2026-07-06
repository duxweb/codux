struct TerminalBlinkManager {
    blink_interval: Duration,
    blink_epoch: usize,
    blinking_paused: bool,
    visible: bool,
    enabled: bool,
    render_visible: bool,
}

impl TerminalBlinkManager {
    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            blink_interval: Duration::from_millis(500),
            blink_epoch: 0,
            blinking_paused: false,
            visible: true,
            enabled: false,
            render_visible: false,
        }
    }

    fn next_blink_epoch(&mut self) -> usize {
        self.blink_epoch += 1;
        self.blink_epoch
    }

    fn pause_blinking(&mut self, cx: &mut Context<Self>) {
        self.show_cursor(cx);
        self.blinking_paused = true;
        let epoch = self.next_blink_epoch();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = this.update(cx, |this, cx| this.resume_blinking(epoch, cx));
        })
        .detach();
    }

    fn resume_blinking(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if epoch == self.blink_epoch {
            self.blinking_paused = false;
            self.blink_cursors(epoch, cx);
        }
    }

    fn blink_cursors(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if epoch != self.blink_epoch
            || !self.enabled
            || !self.render_visible
            || self.blinking_paused
        {
            return;
        }
        self.visible = !self.visible;
        cx.notify();

        let epoch = self.next_blink_epoch();
        let interval = self.blink_interval;
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(interval).await;
            let _ = this.update(cx, |this, cx| this.blink_cursors(epoch, cx));
        })
        .detach();
    }

    fn show_cursor(&mut self, cx: &mut Context<Self>) {
        if !self.visible {
            self.visible = true;
            if self.render_visible {
                cx.notify();
            }
        }
    }

    fn enable(&mut self, cx: &mut Context<Self>) {
        if self.enabled {
            return;
        }
        self.enabled = true;
        self.visible = false;
        self.blink_cursors(self.blink_epoch, cx);
    }

    fn disable(&mut self, cx: &mut Context<Self>) {
        self.enabled = false;
        self.show_cursor(cx);
    }

    fn set_render_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.render_visible == visible {
            return;
        }
        self.render_visible = visible;
        self.visible = true;
        let epoch = self.next_blink_epoch();
        if visible {
            cx.notify();
            self.blink_cursors(epoch, cx);
        }
    }

    fn visible(&self) -> bool {
        self.visible
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SyncOutputUpdate {
    entered_from_idle: bool,
    exited_to_idle: bool,
    should_notify: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalColorSchemeUpdate {
    enabled: bool,
    disabled: bool,
    query_count: usize,
    osc_foreground_queries: usize,
    osc_background_queries: usize,
}

#[derive(Debug, Default)]
struct TerminalColorSchemeState {
    updates_enabled: bool,
    scan_tail: Vec<u8>,
}

fn update_synchronized_output_state(
    bytes: &[u8],
    depth: &mut usize,
    scan_tail: &mut Vec<u8>,
) -> SyncOutputUpdate {
    const START: &[u8] = b"\x1b[?2026h";
    const END: &[u8] = b"\x1b[?2026l";
    const MAX_PATTERN_LEN: usize = START.len();

    let mut update = SyncOutputUpdate::default();
    let mut scan = Vec::with_capacity(scan_tail.len() + bytes.len());
    scan.extend_from_slice(scan_tail);
    scan.extend_from_slice(bytes);

    let mut index = 0;
    while index < scan.len() {
        if scan[index..].starts_with(START) {
            if *depth == 0 {
                update.entered_from_idle = true;
            }
            *depth = depth.saturating_add(1);
            index += START.len();
            continue;
        }
        if scan[index..].starts_with(END) {
            let was_syncing = *depth > 0;
            *depth = depth.saturating_sub(1);
            if was_syncing {
                update.should_notify = true;
                if *depth == 0 {
                    update.exited_to_idle = true;
                }
            }
            index += END.len();
            continue;
        }
        index += 1;
    }

    let tail_len = scan.len().min(MAX_PATTERN_LEN.saturating_sub(1));
    scan_tail.clear();
    scan_tail.extend_from_slice(&scan[scan.len().saturating_sub(tail_len)..]);

    update
}

fn update_terminal_color_scheme_state(
    bytes: &[u8],
    state: &mut TerminalColorSchemeState,
) -> TerminalColorSchemeUpdate {
    const ENABLE: &[u8] = b"\x1b[?2031h";
    const DISABLE: &[u8] = b"\x1b[?2031l";
    const QUERY: &[u8] = b"\x1b[?996n";
    // xterm dynamic color queries; TUIs use these to read the real
    // fg/bg RGB (e.g. to derive elevated panel backgrounds).
    const OSC_FG_QUERY: &[u8] = b"\x1b]10;?";
    const OSC_BG_QUERY: &[u8] = b"\x1b]11;?";
    const MAX_PATTERN_LEN: usize = ENABLE.len();

    let mut update = TerminalColorSchemeUpdate::default();
    let old_tail_len = state.scan_tail.len();
    let mut scan = Vec::with_capacity(state.scan_tail.len() + bytes.len());
    scan.extend_from_slice(&state.scan_tail);
    scan.extend_from_slice(bytes);

    let mut index = 0;
    while index < scan.len() {
        if scan[index..].starts_with(ENABLE) {
            if index + ENABLE.len() > old_tail_len {
                state.updates_enabled = true;
                update.enabled = true;
            }
            index += ENABLE.len();
            continue;
        }
        if scan[index..].starts_with(DISABLE) {
            if index + DISABLE.len() > old_tail_len {
                state.updates_enabled = false;
                update.disabled = true;
            }
            index += DISABLE.len();
            continue;
        }
        if scan[index..].starts_with(QUERY) {
            if index + QUERY.len() > old_tail_len {
                update.query_count += 1;
            }
            index += QUERY.len();
            continue;
        }
        if scan[index..].starts_with(OSC_FG_QUERY) {
            if index + OSC_FG_QUERY.len() > old_tail_len {
                update.osc_foreground_queries += 1;
            }
            index += OSC_FG_QUERY.len();
            continue;
        }
        if scan[index..].starts_with(OSC_BG_QUERY) {
            if index + OSC_BG_QUERY.len() > old_tail_len {
                update.osc_background_queries += 1;
            }
            index += OSC_BG_QUERY.len();
            continue;
        }
        index += 1;
    }

    let tail_len = scan.len().min(MAX_PATTERN_LEN.saturating_sub(1));
    state.scan_tail.clear();
    state
        .scan_tail
        .extend_from_slice(&scan[scan.len().saturating_sub(tail_len)..]);

    update
}

struct TerminalOscNotification {
    title: Option<String>,
    body: String,
}

/// OSC 9 (iTerm2-style) and OSC 777;notify (urxvt/foot) desktop
/// notifications, scanned from raw output like the OSC color queries above
/// (alacritty ignores both sequences). `scan_tail` carries unfinished
/// sequences across reads.
fn scan_terminal_osc_notifications(
    bytes: &[u8],
    scan_tail: &mut Vec<u8>,
) -> Vec<TerminalOscNotification> {
    const PREFIX_9: &[u8] = b"\x1b]9;";
    const PREFIX_777: &[u8] = b"\x1b]777;notify;";
    // Notifications are short; anything longer is some other payload stream.
    const MAX_PAYLOAD: usize = 1024;

    let old_tail_len = scan_tail.len();
    let mut scan = Vec::with_capacity(old_tail_len + bytes.len());
    scan.extend_from_slice(scan_tail);
    scan.extend_from_slice(bytes);

    let mut notifications = Vec::new();
    let mut unterminated_start = None;
    let mut index = 0;
    while index < scan.len() {
        let (prefix, with_title) = if scan[index..].starts_with(PREFIX_777) {
            (PREFIX_777, true)
        } else if scan[index..].starts_with(PREFIX_9) {
            (PREFIX_9, false)
        } else {
            index += 1;
            continue;
        };
        let payload_start = index + prefix.len();
        let mut cursor = payload_start;
        let mut terminator = None;
        while cursor < scan.len() && cursor - payload_start <= MAX_PAYLOAD {
            match scan[cursor] {
                0x07 => {
                    terminator = Some((cursor, cursor + 1));
                    break;
                }
                0x1b if scan.get(cursor + 1) == Some(&b'\\') => {
                    terminator = Some((cursor, cursor + 2));
                    break;
                }
                _ => cursor += 1,
            }
        }
        let Some((payload_end, sequence_end)) = terminator else {
            if cursor - payload_start > MAX_PAYLOAD {
                index = cursor;
                continue;
            }
            // Sequence split across reads: keep it in the tail and retry.
            unterminated_start = Some(index);
            break;
        };
        // Sequences fully inside the carried tail were already reported.
        if sequence_end > old_tail_len {
            let payload = String::from_utf8_lossy(&scan[payload_start..payload_end]);
            let notification = if with_title {
                let (title, body) = payload.split_once(';').unwrap_or(("", payload.as_ref()));
                TerminalOscNotification {
                    title: (!title.is_empty()).then(|| title.to_string()),
                    body: body.to_string(),
                }
            } else {
                TerminalOscNotification {
                    title: None,
                    body: payload.to_string(),
                }
            };
            // OSC 9;<digit>;... is ConEmu-style progress, not a notification.
            let progress = !with_title
                && notification
                    .body
                    .split_once(';')
                    .is_some_and(|(kind, _)| kind.chars().all(|ch| ch.is_ascii_digit()));
            if !notification.body.is_empty() && !progress {
                notifications.push(notification);
            }
        }
        index = sequence_end;
    }

    let keep_from = unterminated_start
        .unwrap_or_else(|| scan.len().saturating_sub(PREFIX_777.len().saturating_sub(1)));
    scan_tail.clear();
    scan_tail.extend_from_slice(&scan[keep_from..]);

    notifications
}

#[cfg(target_os = "macos")]
fn terminal_bell_beep() {
    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {
        fn NSBeep();
    }
    unsafe { NSBeep() }
}

#[cfg(target_os = "windows")]
fn terminal_bell_beep() {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBeep(u_type: u32) -> i32;
    }
    // 0 = MB_OK, the default system alert sound.
    unsafe {
        MessageBeep(0);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn terminal_bell_beep() {}

fn terminal_color_scheme_report_for(dark: bool) -> &'static [u8] {
    if dark {
        b"\x1b[?997;1n"
    } else {
        b"\x1b[?997;2n"
    }
}

fn scheme_is_dark(remote_viewer: bool, colors: &ColorPalette) -> bool {
    // The mobile client renders with a fixed dark theme.
    remote_viewer || colors.is_dark()
}

// Mobile app theme (apps/mobile/lib/theme/app_theme.dart):
// bgBase #0D1117, textPrimary #E6EDF3.
const REMOTE_VIEWER_BACKGROUND: (u8, u8, u8) = (0x0d, 0x11, 0x17);
const REMOTE_VIEWER_FOREGROUND: (u8, u8, u8) = (0xe6, 0xed, 0xf3);

fn terminal_osc_rgb_report(code: u8, (r, g, b): (u8, u8, u8)) -> Vec<u8> {
    format!(
        "\x1b]{};rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}\x07",
        code, r, r, g, g, b, b
    )
    .into_bytes()
}

// xterm dynamic color reply (OSC 10 = foreground, OSC 11 = background),
// 16-bit per channel as rrrr/gggg/bbbb.
fn terminal_osc_color_report(code: u8, color: Hsla) -> Vec<u8> {
    format!("\x1b]{};{}\x07", code, osc_color_payload(color)).into_bytes()
}

fn terminal_trace_enabled() -> bool {
    *TERMINAL_TRACE_ENABLED.get_or_init(|| {
        env::var("CODUX_TERMINAL_TRACE")
            .map(|value| {
                let value = value.trim();
                !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
            })
            .unwrap_or(false)
    })
}

fn terminal_trace(message: &str) {
    if terminal_trace_enabled() {
        codux_runtime::runtime_trace::runtime_trace("terminal-pty", message);
    }
}

fn trace_terminal_protocol_bytes(
    bytes: &[u8],
    sync_update: SyncOutputUpdate,
    sync_depth: usize,
    color_scheme_update: TerminalColorSchemeUpdate,
    color_scheme_updates_enabled: bool,
) {
    if !terminal_trace_enabled() {
        return;
    }
    let flags = terminal_protocol_flags(bytes);

    if sync_update != SyncOutputUpdate::default()
        || flags.show_cursor
        || flags.hide_cursor
        || flags.osc_10_request
        || flags.osc_11_request
        || color_scheme_update != TerminalColorSchemeUpdate::default()
    {
        terminal_trace(&format!(
            "protocol bytes={} sync_depth={} sync_enter={} sync_exit={} notify={} show_cursor={} hide_cursor={} osc10_request={} osc11_request={} color_scheme_enabled={} color_scheme_enable={} color_scheme_disable={} color_scheme_queries={}",
            bytes.len(),
            sync_depth,
            sync_update.entered_from_idle,
            sync_update.exited_to_idle,
            sync_update.should_notify,
            flags.show_cursor,
            flags.hide_cursor,
            flags.osc_10_request,
            flags.osc_11_request,
            color_scheme_updates_enabled,
            color_scheme_update.enabled,
            color_scheme_update.disabled,
            color_scheme_update.query_count,
        ));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalProtocolFlags {
    show_cursor: bool,
    hide_cursor: bool,
    osc_10_request: bool,
    osc_11_request: bool,
}

fn terminal_protocol_flags(bytes: &[u8]) -> TerminalProtocolFlags {
    TerminalProtocolFlags {
        show_cursor: bytes
            .windows(b"\x1b[?25h".len())
            .any(|part| part == b"\x1b[?25h"),
        hide_cursor: bytes
            .windows(b"\x1b[?25l".len())
            .any(|part| part == b"\x1b[?25l"),
        osc_10_request: bytes
            .windows(b"\x1b]10;?".len())
            .any(|part| part == b"\x1b]10;?"),
        osc_11_request: bytes
            .windows(b"\x1b]11;?".len())
            .any(|part| part == b"\x1b]11;?"),
    }
}

fn trace_terminal_paint_snapshot(content: &TerminalContent, cursor_visible: bool) {
    if !terminal_trace_enabled() {
        return;
    }
    terminal_trace(&format!(
        "paint cursor_visible={} cursor_model_visible={} cursor_row={} cursor_col={} cursor_shape={:?} display_offset={} cells={} cols={} rows={} visible_rows={} row_shift={}",
        cursor_visible,
        content.cursor.visible,
        content.cursor.row,
        content.cursor.col,
        content.cursor.shape,
        content.display_offset,
        content.cells.len(),
        content.columns,
        content.screen_lines,
        content.visible_rows(),
        content.visible_row_shift,
    ));
}
