use super::*;

#[test]
fn output_capture_keeps_limited_tail_and_total_bytes() {
    let mut capture = TerminalOutputCapture::new(5);
    capture.push(b"hello");
    capture.push(b" world");
    let snapshot = capture.snapshot();
    assert_eq!(snapshot.bytes, 11);
    assert_eq!(snapshot.tail, "world");
}

#[test]
fn output_replay_uses_terminal_history_not_limited_tail() {
    let mut history = RingHistory::new(1024);
    history.push_text("hello");
    history.push_text(" world");
    assert_eq!(history.to_text(), "hello world");

    let mut capture = TerminalOutputCapture::new(5);
    capture.push(b"hello world");
    assert_eq!(capture.snapshot().tail, "world");
}

#[test]
fn terminal_history_tail_returns_recent_window_and_offset() {
    let mut history = RingHistory::new(1024);
    history.push_text("hello");
    history.push_text(" world");

    assert_eq!(history.tail_text(5), ("world".to_string(), 6));
    assert_eq!(history.tail_text(20), ("hello world".to_string(), 0));
}

#[test]
fn terminal_history_tail_starts_after_partial_csi_sequence() {
    let mut history = RingHistory::new(1024);
    history.push_text("line 1\n");
    history.push_text("\x1b[12;27Hprompt");

    let (tail, offset) = history.tail_text(9);

    assert_eq!(tail, "prompt");
    assert_eq!(offset, "line 1\n\x1b[12;27H".chars().count());
}

#[test]
fn terminal_history_tail_starts_after_partial_osc_sequence() {
    let mut history = RingHistory::new(1024);
    history.push_text("line 1\n");
    history.push_text("\x1b]0;Codux\x07prompt");

    let (tail, offset) = history.tail_text(10);

    assert_eq!(tail, "prompt");
    assert_eq!(offset, "line 1\n\x1b]0;Codux\x07".chars().count());
}

#[test]
fn headless_screen_snapshot_replays_current_screen_not_raw_tail() {
    let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
    screen.process(b"old line\n\x1b[2J\x1b[Htop\x1b[3;5Hbottom");

    let snapshot = screen.snapshot();

    assert!(snapshot.data.contains("\x1b[H\x1b[2J"));
    assert!(snapshot.data.contains("top"));
    assert!(snapshot.data.contains("bottom"));
    assert!(!snapshot.data.contains("old line"));
    assert_eq!(snapshot.cols, 20);
    assert_eq!(snapshot.rows, 4);
}

#[test]
fn headless_screen_snapshot_tracks_resize() {
    let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
    screen.resize(30, 10);
    screen.process(b"ready");

    let snapshot = screen.snapshot();

    assert!(snapshot.data.contains("ready"));
    assert_eq!(snapshot.cols, 30);
    assert_eq!(snapshot.rows, 10);
}

#[test]
fn headless_screen_snapshot_does_not_insert_spaces_after_wide_chars() {
    let mut screen = HeadlessTerminalScreen::new(40, 4, 100);
    screen.process("第 2003行 测 试 文 本".as_bytes());

    let snapshot = screen.snapshot();

    assert!(
        snapshot.data.contains("第 2003行 测 试 文 本"),
        "{}",
        snapshot.data.escape_debug()
    );
    assert!(!snapshot.data.contains("第  2003"));
    assert!(!snapshot.data.contains("测  试"));
}

#[test]
fn input_capture_keeps_limited_history_and_total_bytes() {
    let mut capture = TerminalInputCapture::new(2);
    capture.push(b"ls\n");
    capture.push(b" ");
    capture.push(b"pwd\n");
    capture.push(b"echo ok\n");
    let snapshot = capture.snapshot();
    assert_eq!(snapshot.bytes, 16);
    assert_eq!(snapshot.history.len(), 2);
    assert_eq!(snapshot.history[0].text, "pwd\n");
    assert_eq!(snapshot.history[1].text, "echo ok\n");
}

#[test]
fn utf8_decoder_keeps_split_multibyte_characters() {
    let mut pending = Vec::new();
    assert_eq!(decode_utf8_output(&[0xe6, 0x8e], &mut pending), "");
    assert_eq!(decode_utf8_output(&[0xa8], &mut pending), "推");
    assert!(pending.is_empty());
}

#[test]
fn utf8_decoder_flushes_incomplete_tail_on_eof() {
    let mut pending = Vec::new();
    assert_eq!(decode_utf8_output(&[0xe6, 0x8e], &mut pending), "");
    assert_eq!(flush_utf8_decoder(&mut pending), "�");
    assert!(pending.is_empty());
}

#[test]
fn terminal_progress_osc_parser_detects_split_start_and_completion() {
    let mut parser = TerminalOscParser::default();

    assert!(parser.push(b"noise\x1b]9;").is_empty());
    assert_eq!(
        parser.push(b"4;3\x07"),
        vec![TerminalOscEvent::Progress(
            TerminalProgressOscState::Working
        )]
    );
    assert_eq!(
        parser.push(b"\x1b]9;4;0\x1b\\"),
        vec![TerminalOscEvent::Progress(
            TerminalProgressOscState::Completed
        )]
    );
}

#[test]
fn terminal_progress_osc_parser_ignores_incomplete_sequence() {
    let mut parser = TerminalOscParser::default();

    assert!(parser.push(b"\x1b]9;4;0").is_empty());
    assert_eq!(
        parser.push(b"\x07"),
        vec![TerminalOscEvent::Progress(
            TerminalProgressOscState::Completed
        )]
    );
}

#[test]
fn terminal_progress_osc_parser_accepts_percent_error_and_warning() {
    let mut parser = TerminalOscParser::default();

    assert_eq!(
        parser.push(b"\x1b]9;4;1;50\x07\x1b]9;4;2\x07\x1b]9;4;4\x1b\\"),
        vec![
            TerminalOscEvent::Progress(TerminalProgressOscState::Working),
            TerminalOscEvent::Progress(TerminalProgressOscState::Error),
            TerminalOscEvent::Progress(TerminalProgressOscState::Warning),
        ]
    );
}

#[test]
fn terminal_osc_parser_detects_codex_wait_notifications() {
    let mut parser = TerminalOscParser::default();

    assert_eq!(
        parser
            .push(b"\x1b]9;Approval requested: npm install\x07\x1b]9;Plan mode prompt: Review\x07"),
        vec![
            TerminalOscEvent::Notification(TerminalNotificationKind::ApprovalRequested),
            TerminalOscEvent::Notification(TerminalNotificationKind::PlanModePrompt),
        ]
    );
}

#[test]
fn terminal_history_bytes_respects_configured_scrollback() {
    assert_eq!(terminal_history_bytes(Some(10_000), 100), 4 * 100 * 10_000);
    assert_eq!(terminal_history_bytes(Some(1), 100), MIN_HISTORY_BYTES);
}

#[test]
fn remote_screen_scrollback_is_capped() {
    assert_eq!(
        remote_screen_scrollback_lines(None),
        REMOTE_SCREEN_SCROLLBACK_CAP
    );
    assert_eq!(
        remote_screen_scrollback_lines(Some(10_000)),
        REMOTE_SCREEN_SCROLLBACK_CAP
    );
    assert_eq!(remote_screen_scrollback_lines(Some(1200)), 1200);
}

#[test]
fn initial_remote_screen_scrollback_starts_idle() {
    assert_eq!(
        initial_remote_screen_scrollback_lines(REMOTE_SCREEN_SCROLLBACK_CAP),
        REMOTE_SCREEN_IDLE_SCROLLBACK
    );
    assert_eq!(initial_remote_screen_scrollback_lines(1200), 500);
    assert_eq!(initial_remote_screen_scrollback_lines(300), 300);
}
