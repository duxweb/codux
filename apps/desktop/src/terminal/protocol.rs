struct TerminalBlinkManager {
    blink_interval: Duration,
    blink_epoch: usize,
    blinking_paused: bool,
    visible: bool,
    enabled: bool,
}

impl TerminalBlinkManager {
    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            blink_interval: Duration::from_millis(500),
            blink_epoch: 0,
            blinking_paused: false,
            visible: true,
            enabled: false,
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
        if epoch != self.blink_epoch || !self.enabled || self.blinking_paused {
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
            cx.notify();
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
        index += 1;
    }

    let tail_len = scan.len().min(MAX_PATTERN_LEN.saturating_sub(1));
    state.scan_tail.clear();
    state
        .scan_tail
        .extend_from_slice(&scan[scan.len().saturating_sub(tail_len)..]);

    update
}

fn terminal_color_scheme_report(colors: &ColorPalette) -> &'static [u8] {
    if colors.is_dark() {
        b"\x1b[?997;1n"
    } else {
        b"\x1b[?997;2n"
    }
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
        "paint cursor_visible={} show_cursor={} cursor_hidden={} cursor_row={} cursor_col={} cursor_shape={:?} display_offset={} cells={} cols={} rows={}",
        cursor_visible,
        content.mode.contains(TermMode::SHOW_CURSOR),
        content.cursor.shape == CursorShape::Hidden,
        content.cursor.point.line.0,
        content.cursor.point.column.0,
        content.cursor.shape,
        content.display_offset,
        content.cells.len(),
        content.columns,
        content.screen_lines,
    ));
}
