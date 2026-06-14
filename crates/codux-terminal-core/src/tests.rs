use super::*;

fn key_input<'a>(
    key: &'a str,
    key_char: Option<&'a str>,
    shift: bool,
    alt: bool,
    control: bool,
    platform: bool,
    application_cursor: bool,
) -> TerminalKeyInput<'a> {
    TerminalKeyInput {
        key,
        key_char,
        modifiers: TerminalKeyInputModifiers {
            shift,
            alt,
            control,
            platform,
        },
        mode: TerminalInputMode {
            application_cursor,
            ..Default::default()
        },
    }
}

#[test]
fn terminal_text_input_normalizes_committed_ime_text() {
    assert_eq!(terminal_text_input("abc"), "abc");
    assert_eq!(terminal_text_input("你好かな한글"), "你好かな한글");
    assert_eq!(terminal_text_input("\u{8}"), "\u{7f}");
    assert_eq!(terminal_text_input("\n\r"), "\r\r");
    assert_eq!(terminal_text_input("a\u{f700}b"), "ab");
}

#[test]
fn terminal_insert_and_paste_input_use_bracketed_paste_rules() {
    assert_eq!(terminal_insert_input("\r"), "\r");
    assert_eq!(terminal_insert_input("\u{7f}"), "\u{7f}");
    assert_eq!(
        terminal_insert_input("BREW。"),
        "\u{1b}[200~BREW。\u{1b}[201~"
    );
    assert_eq!(terminal_insert_input("a\nb"), "\u{1b}[200~a\nb\u{1b}[201~");
    assert_eq!(
        terminal_paste_input_bytes("a\r\nb\rc", true),
        b"\x1b[200~a\nb\nc\x1b[201~"
    );
    assert_eq!(terminal_paste_input_bytes("raw\r\n", false), b"raw\r\n");
}

#[test]
fn terminal_key_input_maps_control_and_navigation_sequences() {
    assert_eq!(
        terminal_key_input(key_input(
            "backspace",
            None,
            false,
            false,
            false,
            false,
            false
        )),
        Some("\u{7f}".to_string())
    );
    assert_eq!(
        terminal_key_input(key_input("enter", None, false, false, false, false, false)),
        Some("\r".to_string())
    );
    assert_eq!(
        terminal_key_input(key_input("up", None, false, false, false, false, false)),
        Some("\u{1b}[A".to_string())
    );
    assert_eq!(
        terminal_key_input(key_input("up", None, false, false, false, false, true)),
        Some("\u{1b}OA".to_string())
    );
    assert_eq!(
        terminal_key_input_bytes(key_input("space", None, false, false, true, false, false)),
        Some(vec![0])
    );
    assert_eq!(
        terminal_key_input(key_input("q", None, false, false, true, false, false)),
        Some("\u{11}".to_string())
    );
}

#[test]
fn terminal_selector_input_maps_platform_edit_commands() {
    let normal = TerminalInputMode {
        application_cursor: false,
        ..Default::default()
    };
    let app_cursor = TerminalInputMode {
        application_cursor: true,
        ..Default::default()
    };

    assert_eq!(
        terminal_selector_input("deleteBackward:", normal),
        Some("\u{7f}".to_string())
    );
    assert_eq!(
        terminal_selector_input("moveLeft:", normal),
        Some("\u{1b}[D".to_string())
    );
    assert_eq!(
        terminal_selector_input("moveLeft:", app_cursor),
        Some("\u{1b}OD".to_string())
    );
    assert_eq!(terminal_selector_input("unknown:", normal), None);
}

#[test]
fn terminal_key_input_keeps_app_shortcuts_out_of_terminal() {
    assert!(terminal_key_input(key_input("q", None, false, false, false, true, false)).is_none());
    assert!(terminal_is_copy_shortcut(key_input(
        "c", None, false, false, false, true, false
    )));
    assert!(terminal_is_paste_shortcut(key_input(
        "v", None, false, false, false, true, false
    )));
}

fn mouse_input(
    action: TerminalMouseAction,
    button: Option<TerminalMouseButton>,
    row: usize,
    col: usize,
    modifiers: TerminalKeyInputModifiers,
    mode: TerminalInputMode,
) -> TerminalMouseInput {
    TerminalMouseInput {
        action,
        button,
        row,
        col,
        modifiers,
        mode,
    }
}

fn mouse_mode(
    sgr_mouse: bool,
    utf8_mouse: bool,
    mouse_drag: bool,
    mouse_motion: bool,
) -> TerminalInputMode {
    TerminalInputMode {
        mouse_tracking: true,
        sgr_mouse,
        utf8_mouse,
        mouse_drag,
        mouse_motion,
        ..Default::default()
    }
}

#[test]
fn terminal_mouse_input_maps_sgr_reports() {
    let mode = mouse_mode(true, false, false, false);
    assert_eq!(
        terminal_mouse_input_bytes(mouse_input(
            TerminalMouseAction::Press,
            Some(TerminalMouseButton::Left),
            1,
            2,
            TerminalKeyInputModifiers::default(),
            mode,
        ))
        .unwrap(),
        b"\x1b[<0;3;2M"
    );
    assert_eq!(
        terminal_mouse_input_bytes(mouse_input(
            TerminalMouseAction::Release,
            Some(TerminalMouseButton::Left),
            1,
            2,
            TerminalKeyInputModifiers::default(),
            mode,
        ))
        .unwrap(),
        b"\x1b[<0;3;2m"
    );
    assert_eq!(
        terminal_mouse_input_bytes(mouse_input(
            TerminalMouseAction::Press,
            Some(TerminalMouseButton::WheelUp),
            1,
            2,
            TerminalKeyInputModifiers::default(),
            mode,
        ))
        .unwrap(),
        b"\x1b[<64;3;2M"
    );
}

#[test]
fn terminal_mouse_input_maps_drag_and_utf8_reports() {
    let drag_mode = mouse_mode(true, false, true, false);
    assert_eq!(
        terminal_mouse_input_bytes(mouse_input(
            TerminalMouseAction::Move,
            Some(TerminalMouseButton::Left),
            1,
            2,
            TerminalKeyInputModifiers {
                shift: true,
                alt: true,
                control: true,
                platform: false,
            },
            drag_mode,
        ))
        .unwrap(),
        b"\x1b[<60;3;2M"
    );

    assert!(
        terminal_mouse_input_bytes(mouse_input(
            TerminalMouseAction::Move,
            None,
            1,
            2,
            TerminalKeyInputModifiers::default(),
            drag_mode,
        ))
        .is_none()
    );

    let normal = mouse_mode(false, false, false, false);
    assert_eq!(
        terminal_mouse_input_bytes(mouse_input(
            TerminalMouseAction::Press,
            Some(TerminalMouseButton::Left),
            1,
            2,
            TerminalKeyInputModifiers::default(),
            normal,
        ))
        .unwrap(),
        vec![b'\x1b', b'[', b'M', 32, 35, 34]
    );

    let utf8 = mouse_mode(false, true, false, false);
    let report = terminal_mouse_input_bytes(mouse_input(
        TerminalMouseAction::Press,
        Some(TerminalMouseButton::Left),
        100,
        100,
        TerminalKeyInputModifiers::default(),
        utf8,
    ))
    .unwrap();
    assert_eq!(&report[..4], &[b'\x1b', b'[', b'M', 32]);
    assert!(report.len() > 6);
}

fn scrollable_history(prefix: &str) -> String {
    (1..=12)
        .map(|index| format!("{prefix} {index:02}"))
        .collect::<Vec<_>>()
        .join("\r\n")
}

#[test]
fn restores_baseline_before_replaying_held_live_output() {
    let mut session = RemotePtySession::new("session-1", 64);
    session.require_baseline();

    assert!(session.hold_live(Some(11), "stale"));
    assert!(session.hold_live(Some(12), "new"));

    let page = session.accept_baseline_page("abcd", 0, Some(8), true);
    assert!(page.accepted);
    assert!(!page.ready);
    assert_eq!(page.next_offset, 4);

    let page = session.accept_baseline_page("efgh", 4, Some(8), false);
    assert!(page.ready);

    let replay = session.replace_from_baseline(&page.data, Some(8), Some(11));
    assert_eq!(session.content(), "abcdefgh");
    assert_eq!(replay, vec!["new"]);
}

#[test]
fn restores_visible_screen_from_screen_baseline_without_mixing_raw_history() {
    let mut session = RemotePtySession::<String>::new("session-1", 256);
    session.resize_screen(20, 8);
    let history = scrollable_history("raw history");

    session.replace_from_baseline_screen(
        &history,
        Some("\x1b[2J\x1b[Hvisible tui"),
        Some(83),
        Some(7),
    );

    let screen = session.screen_snapshot();
    assert_eq!(session.content(), history);
    assert_eq!(session.buffer_length(), 83);
    assert_eq!(session.sequence(), 7);
    assert!(screen.data.contains("visible tui"));
    assert!(!screen.data.contains("raw history 01"));

    session.scroll_screen_lines(8);
    let scrolled = session.screen_snapshot();
    assert!(scrolled.display_offset > 0);
    assert!(scrolled.data.contains("raw history 01") || scrolled.data.contains("raw history 02"));
    assert!(!scrolled.data.contains("visible tui"));

    session.scroll_screen_to_bottom();
    let bottom = session.screen_snapshot();
    assert_eq!(bottom.display_offset, 0);
    assert!(bottom.data.contains("visible tui"));
    assert!(!bottom.data.contains("raw history 01"));
}

#[test]
fn live_output_can_update_visible_screen_from_screen_keyframe() {
    let mut session = RemotePtySession::<String>::new("session-1", 512);
    session.resize_screen(20, 8);
    let history = scrollable_history("cached raw history");
    session.replace_from_baseline_screen(
        &history,
        Some("\x1b[2J\x1b[Hold screen"),
        Some(18),
        Some(3),
    );

    session.append_live_screen(
        "partial live raw",
        Some("\x1b[2J\x1b[Hrestored tui\n\x1b[3;1Hinput box"),
        Some(32),
        Some(4),
    );

    let screen = session.screen_snapshot();
    assert_eq!(session.content(), format!("{history}partial live raw"));
    assert_eq!(session.buffer_length(), 32);
    assert_eq!(session.sequence(), 4);
    assert!(screen.data.contains("restored tui"));
    assert!(screen.data.contains("input box"));
    assert!(!screen.data.contains("old screen"));

    session.scroll_screen_lines(8);
    let scrolled = session.screen_snapshot();
    assert!(scrolled.display_offset > 0);
    assert!(scrolled.data.contains("cached raw history"));
    assert!(!scrolled.data.contains("restored tui"));
}

#[test]
fn host_scroll_snapshot_overrides_view_and_offsets() {
    let mut session = RemotePtySession::<String>::new("session-1", 512);
    session.resize_screen(20, 8);
    session.replace_from_baseline_screen(
        "history",
        Some("\x1b[2J\x1b[Hlive bottom"),
        Some(7),
        Some(1),
    );

    // Host-rendered scrolled viewport replaces the display and reports the
    // host's offsets through the snapshot.
    session.apply_host_scroll_snapshot("\x1b[2J\x1b[Hhost history view", 12, 200, 0, 0);
    let scrolled = session.screen_snapshot();
    assert!(scrolled.data.contains("host history view"));
    assert_eq!(scrolled.display_offset, 12);
    assert_eq!(scrolled.total_lines, 200);

    // Returning to the bottom clears the host scroll state.
    session.apply_host_scroll_snapshot("\x1b[2J\x1b[Hback to bottom", 0, 200, 0, 0);
    let bottom = session.screen_snapshot();
    assert!(bottom.data.contains("back to bottom"));
    assert_eq!(bottom.display_offset, 0);

    // A new baseline resets any lingering host scroll override.
    session.apply_host_scroll_snapshot("\x1b[2J\x1b[Hscrolled again", 5, 50, 0, 0);
    session.replace_from_baseline_screen(
        "fresh",
        Some("\x1b[2J\x1b[Hfresh keyframe"),
        Some(5),
        Some(2),
    );
    assert_eq!(session.screen_snapshot().display_offset, 0);
}

#[test]
fn scroll_to_bottom_clears_host_scroll_override() {
    let mut session = RemotePtySession::<String>::new("session-1", 512);
    session.resize_screen(20, 8);
    session.replace_from_baseline_screen("history", Some("\x1b[2J\x1b[Hlive"), Some(7), Some(1));
    session.apply_host_scroll_snapshot("\x1b[2J\x1b[Hhost view", 9, 90, 0, 0);
    assert_eq!(session.screen_snapshot().display_offset, 9);

    session.scroll_screen_to_bottom();
    assert_eq!(session.screen_snapshot().display_offset, 0);
}

#[test]
fn live_screen_keyframe_replaces_current_screen_without_polluting_history() {
    let mut session = RemotePtySession::<String>::new("session-1", 512);
    session.resize_screen(20, 8);
    let history = scrollable_history("history");
    session.replace_from_baseline_screen(
        &history,
        Some("\x1b[2J\x1b[Hold screen\r\nold input"),
        Some(50),
        Some(3),
    );

    session.append_live_screen(
        "live raw",
        Some("\x1b[2J\x1b[Hnew screen\r\n\x1b[3;1Hnew input"),
        Some(58),
        Some(4),
    );

    let screen = session.screen_snapshot();
    assert!(screen.data.contains("new screen"));
    assert!(screen.data.contains("new input"));
    assert!(!screen.data.contains("old screen"));
    assert!(!screen.data.contains("history 01"));

    session.scroll_screen_lines(8);
    let scrolled = session.screen_snapshot();
    assert!(scrolled.display_offset > 0);
    assert!(scrolled.data.contains("history 01") || scrolled.data.contains("history 02"));
    assert!(!scrolled.data.contains("old screen"));
    assert!(!scrolled.data.contains("new screen"));

    session.scroll_screen_to_bottom();
    let bottom = session.screen_snapshot();
    assert!(bottom.data.contains("new screen"));
    assert!(bottom.data.contains("new input"));
    assert!(!bottom.data.contains("history 01"));
}

#[test]
fn rejects_out_of_order_baseline_pages() {
    let mut session = RemotePtySession::<String>::new("session-1", 64);
    session.require_baseline();

    let page = session.accept_baseline_page("abcd", 0, Some(8), true);
    assert!(page.accepted);

    let page = session.accept_baseline_page("gh", 6, Some(8), false);
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

#[test]
fn output_sequencer_drops_duplicates_and_tracks_buffers() {
    let mut sequencer = TerminalOutputSequencer::new();

    let first = sequencer.observe("term-1", false, Some(1), None, false);
    let second = sequencer.observe("term-1", false, Some(2), None, false);
    let duplicate = sequencer.observe("term-1", false, Some(2), None, false);
    let baseline = sequencer.observe("term-1", true, Some(2), Some(0), false);
    let next = sequencer.observe("term-1", false, Some(3), None, false);

    assert_eq!(first.action, TerminalOutputSequenceAction::Accept);
    assert_eq!(second.action, TerminalOutputSequenceAction::Accept);
    assert_eq!(duplicate.action, TerminalOutputSequenceAction::Duplicate);
    assert_eq!(duplicate.previous_seq, 2);
    assert_eq!(baseline.action, TerminalOutputSequenceAction::Baseline);
    assert_eq!(next.action, TerminalOutputSequenceAction::Accept);
    assert_eq!(sequencer.sequence_for("term-1"), 3);
}

#[test]
fn output_sequencer_reports_live_sequence_gaps() {
    let mut sequencer = TerminalOutputSequencer::new();

    let first = sequencer.observe("term-1", false, Some(1), None, false);
    let contiguous = sequencer.observe("term-1", false, Some(2), None, false);
    let gapped = sequencer.observe("term-1", false, Some(5), None, false);
    let duplicate = sequencer.observe("term-1", false, Some(5), None, false);
    let after_gap = sequencer.observe("term-1", false, Some(6), None, false);

    assert!(!first.gap);
    assert!(!contiguous.gap);
    assert_eq!(gapped.action, TerminalOutputSequenceAction::Accept);
    assert!(gapped.gap);
    assert_eq!(gapped.previous_seq, 2);
    assert_eq!(duplicate.action, TerminalOutputSequenceAction::Duplicate);
    assert!(!duplicate.gap);
    assert!(!after_gap.gap);
    assert_eq!(sequencer.sequence_for("term-1"), 6);
}

#[test]
fn output_sequencer_does_not_report_gap_after_baseline_rebase() {
    let mut sequencer = TerminalOutputSequencer::new();
    sequencer.observe("term-1", false, Some(2), None, false);

    let baseline = sequencer.observe("term-1", true, Some(2), Some(0), false);
    let rebased = sequencer.observe("term-1", false, Some(9), None, false);

    assert_eq!(baseline.action, TerminalOutputSequenceAction::Baseline);
    assert!(!baseline.gap);
    assert!(!rebased.gap);
}

#[test]
fn output_sequencer_allows_host_restart_sequence_reset() {
    let mut sequencer = TerminalOutputSequencer::new();
    sequencer.observe("term-1", false, Some(8), None, false);

    let baseline = sequencer.observe("term-1", true, Some(0), Some(0), false);
    let next = sequencer.observe("term-1", false, Some(1), None, false);

    assert_eq!(baseline.action, TerminalOutputSequenceAction::Baseline);
    assert_eq!(next.action, TerminalOutputSequenceAction::Accept);
    assert_eq!(sequencer.sequence_for("term-1"), 1);
}

#[test]
fn terminal_buffer_assembler_assembles_out_of_order_chunks() {
    let mut assembler = TerminalBufferAssembler::new(200_000);

    assert!(
        !assembler
            .accept(
                "term-1",
                chunk_payload("snapshot-1", "request-1", 2, "cd", 3)
            )
            .ready
    );
    assert!(
        !assembler
            .accept(
                "term-1",
                chunk_payload("snapshot-1", "request-1", 0, "ab", 3)
            )
            .ready
    );
    let result = assembler.accept(
        "term-1",
        chunk_payload("snapshot-1", "request-1", 1, "你好", 3),
    );

    assert!(result.ready);
    let payload = result.payload.unwrap();
    assert_eq!(payload["data"], "ab你好cd");
    assert_eq!(payload["offset"], 10);
    assert_eq!(payload["chunked"], false);
    assert_eq!(payload["assembled"], true);
    assert!(payload.get("chunkIndex").is_none());
    assert!(payload.get("chunkCount").is_none());
}

#[test]
fn terminal_buffer_assembler_preserves_screen_baseline_metadata() {
    let mut assembler = TerminalBufferAssembler::new(200_000);

    assert!(
        !assembler
            .accept(
                "term-1",
                chunk_payload("snapshot-1", "request-1", 1, "history-tail", 2)
            )
            .ready
    );
    let result = assembler.accept(
        "term-1",
        chunk_payload("snapshot-1", "request-1", 0, "raw-", 2),
    );

    assert!(result.ready);
    let payload = result.payload.unwrap();
    assert_eq!(payload["data"], "raw-history-tail");
    assert_eq!(payload["screenData"], "\u{1b}[2J\u{1b}[Hvisible screen");
}

#[test]
fn terminal_buffer_assembler_ignores_duplicate_chunks_and_limits_size() {
    let mut assembler = TerminalBufferAssembler::new(4);

    let first = assembler.accept(
        "term-1",
        chunk_payload("snapshot-1", "request-1", 0, "abcd", 2),
    );
    let duplicate = assembler.accept(
        "term-1",
        chunk_payload("snapshot-1", "request-1", 0, "abcd", 2),
    );
    let too_large = assembler.accept(
        "term-1",
        chunk_payload("snapshot-1", "request-1", 1, "ef", 2),
    );

    assert_eq!(first.progress, Some(0.5));
    assert!(!duplicate.ready);
    assert_eq!(duplicate.progress, Some(0.5));
    assert!(!too_large.ready);
    assert_eq!(too_large.progress, Some(0.5));
}

#[test]
fn terminal_buffer_assembler_replaces_stale_snapshot_per_request() {
    let mut assembler = TerminalBufferAssembler::new(200_000);

    assembler.accept("term-1", chunk_payload("old", "request-1", 0, "old-", 2));
    assembler.accept("term-1", chunk_payload("new", "request-1", 0, "new-", 2));
    let result = assembler.accept("term-1", chunk_payload("new", "request-1", 1, "data", 2));

    assert!(result.ready);
    assert_eq!(result.payload.unwrap()["data"], "new-data");
}

#[test]
fn remote_sequence_guard_keeps_state_channels_and_terminal_sessions_independent() {
    let mut guard = RemoteSequenceGuard::new(128);

    assert!(guard.accept("terminal.list", None, Some(34)));
    assert!(guard.accept("project.list", None, Some(33)));
    assert!(!guard.accept("project.list", None, Some(33)));
    assert!(guard.accept("terminal.output", Some("a"), Some(10)));
    assert!(guard.accept("terminal.output", Some("b"), Some(10)));
    assert!(!guard.accept("terminal.output", Some("a"), Some(10)));
}

#[test]
fn remote_sequence_guard_applies_monotonic_state_but_allows_output_reordering() {
    let mut guard = RemoteSequenceGuard::new(3);

    assert!(guard.accept("project.list", None, Some(4)));
    assert!(!guard.accept("project.list", None, Some(1)));
    assert!(guard.accept("terminal.output", Some("session-1"), Some(40)));
    assert!(guard.accept("terminal.output", Some("session-1"), Some(39)));
}

fn chunk_payload(
    snapshot_id: &str,
    request_id: &str,
    index: usize,
    data: &str,
    chunk_count: usize,
) -> serde_json::Value {
    serde_json::json!({
        "buffer": true,
        "chunked": true,
        "snapshotId": snapshot_id,
        "chunkIndex": index,
        "chunkCount": chunk_count,
        "data": data,
        "offset": 10 + index * 2,
        "startOffset": 10,
        "bufferLength": 16,
        "truncated": true,
        "outputSeq": 7,
        "requestId": request_id,
        "screenData": "\u{1b}[2J\u{1b}[Hvisible screen",
    })
}

#[test]
fn runtime_model_does_not_repeat_project_select_after_ack() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);

    let select = runtime.user_select_project(projects()[1].clone(), true);
    assert_eq!(
        select.request_project_select_id.as_deref(),
        Some("project-2")
    );
    runtime.mark_project_select_sent("project-2");

    let confirmed = runtime.project_selected(Some("project-2".to_string()), None);
    assert!(confirmed.request_terminal_list);
    assert_eq!(runtime.pending_project_select(true), None);

    let retry = runtime.ensure_terminal_for_selected_project(true, true);
    assert_eq!(retry.request_project_select_id, None);
    assert!(!retry.request_terminal_list);
}

#[test]
fn runtime_model_binds_terminal_when_delayed_list_arrives() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.user_select_project(projects()[1].clone(), true);
    runtime.mark_project_select_sent("project-2");
    runtime.project_selected(Some("project-2".to_string()), None);

    let before_list =
        runtime.apply_project_list(projects(), Some("project-2".to_string()), None, true, true);
    assert_eq!(before_list.request_project_select_id, None);

    let list = runtime.apply_terminal_list(vec![terminal("session-2", "project-2")], true, true);
    assert_eq!(list.bind_session_id.as_deref(), Some("session-2"));
    assert_eq!(
        runtime.snapshot().active_session_id.as_deref(),
        Some("session-2")
    );
}

#[test]
fn runtime_model_latest_project_selection_wins_over_stale_host_list() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.user_select_project(projects()[1].clone(), true);

    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);

    assert_eq!(
        runtime.snapshot().selected_project_id.as_deref(),
        Some("project-2")
    );
}

#[test]
fn runtime_model_terminal_list_does_not_ack_pending_project_select() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.user_select_project(projects()[1].clone(), true);
    runtime.mark_project_select_sent("project-2");

    let list = runtime.apply_terminal_list(vec![terminal("session-2", "project-2")], true, true);

    assert_eq!(list.bind_session_id.as_deref(), Some("session-2"));
    assert_eq!(
        runtime.pending_project_select(true).as_deref(),
        Some("project-2")
    );
}

#[test]
fn runtime_model_project_list_remote_selected_acks_pending_project_select() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.user_select_project(projects()[1].clone(), true);
    runtime.mark_project_select_sent("project-2");

    runtime.apply_project_list(projects(), Some("project-2".to_string()), None, true, true);

    assert_eq!(runtime.pending_project_select(true), None);
    assert_eq!(
        runtime.snapshot().project_select_acknowledged_id.as_deref(),
        Some("project-2")
    );
}

#[test]
fn runtime_model_ignores_stale_project_selected_during_fast_switch() {
    let mut runtime = RemoteRuntimeModel::new();
    let mut three_projects = projects();
    three_projects.push(RemoteRuntimeProject {
        id: "project-3".to_string(),
        name: "Project 3".to_string(),
        path: Some("/tmp/project-3".to_string()),
    });
    runtime.apply_project_list(
        three_projects.clone(),
        Some("project-1".to_string()),
        None,
        true,
        false,
    );
    runtime.apply_terminal_list(
        vec![
            terminal("session-1", "project-1"),
            terminal("session-2", "project-2"),
            terminal("session-3", "project-3"),
        ],
        true,
        true,
    );

    runtime.user_select_project(three_projects[1].clone(), true);
    runtime.mark_project_select_sent("project-2");
    runtime.user_select_project(three_projects[2].clone(), true);
    runtime.mark_project_select_sent("project-3");

    let stale = runtime.project_selected(Some("project-2".to_string()), None);

    assert_eq!(stale, RemoteRuntimePlan::default());
    assert_eq!(
        runtime.snapshot().selected_project_id.as_deref(),
        Some("project-3")
    );
    assert_eq!(
        runtime.pending_project_select(true).as_deref(),
        Some("project-3")
    );
}

#[test]
fn runtime_model_remembers_last_terminal_per_project() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.apply_terminal_list(
        vec![
            terminal("session-1", "project-1"),
            terminal("session-2", "project-2"),
            terminal("session-3", "project-2"),
        ],
        true,
        true,
    );
    runtime.select_terminal(terminal("session-3", "project-2"));
    runtime.user_select_project(projects()[0].clone(), true);

    let select = runtime.user_select_project(projects()[1].clone(), true);

    assert_eq!(select.bind_session_id.as_deref(), Some("session-3"));
}

#[test]
fn runtime_model_preserves_terminal_worktree_scope() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(
        projects(),
        Some("project-2".to_string()),
        Some("worktree-2".to_string()),
        true,
        false,
    );

    runtime.apply_terminal_list(
        vec![RemoteRuntimeTerminal {
            worktree_id: Some("worktree-2".to_string()),
            layout_order: Some(0),
            ..terminal("session-2", "project-2")
        }],
        true,
        true,
    );

    let terminals = runtime.current_project_terminals();
    assert_eq!(terminals.len(), 1);
    assert_eq!(terminals[0].worktree_id.as_deref(), Some("worktree-2"));
}

#[test]
fn runtime_model_keeps_all_selected_worktree_split_terminals() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(
        projects(),
        Some("project-2".to_string()),
        Some("worktree-2".to_string()),
        true,
        false,
    );
    runtime.apply_terminal_list(
        vec![
            RemoteRuntimeTerminal {
                worktree_id: Some("worktree-2".to_string()),
                layout_order: Some(2),
                ..terminal("worktree-split-3", "project-2")
            },
            RemoteRuntimeTerminal {
                worktree_id: Some("project-2".to_string()),
                layout_order: Some(0),
                ..terminal("default-split", "project-2")
            },
            RemoteRuntimeTerminal {
                worktree_id: Some("worktree-2".to_string()),
                layout_order: Some(0),
                ..terminal("worktree-split-1", "project-2")
            },
            RemoteRuntimeTerminal {
                worktree_id: Some("worktree-2".to_string()),
                layout_order: Some(1),
                ..terminal("worktree-split-2", "project-2")
            },
        ],
        true,
        true,
    );

    assert_eq!(
        runtime
            .current_project_terminals()
            .into_iter()
            .map(|terminal| terminal.id)
            .collect::<Vec<_>>(),
        vec![
            "worktree-split-1".to_string(),
            "worktree-split-2".to_string(),
            "worktree-split-3".to_string()
        ]
    );
}

#[test]
fn runtime_model_binds_selected_worktree_active_terminal_without_project_select() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-2".to_string()), None, true, false);
    runtime.apply_terminal_list(
        vec![
            RemoteRuntimeTerminal {
                worktree_id: Some("project-2".to_string()),
                layout_order: Some(0),
                ..terminal("default-session", "project-2")
            },
            RemoteRuntimeTerminal {
                worktree_id: Some("worktree-2".to_string()),
                layout_order: Some(0),
                ..terminal("worktree-split", "project-2")
            },
            RemoteRuntimeTerminal {
                worktree_id: Some("worktree-2".to_string()),
                layout_kind: "tab".to_string(),
                layout_order: Some(1),
                ..terminal("worktree-tab", "project-2")
            },
        ],
        true,
        true,
    );

    let plan = runtime.apply_worktree_selected(
        Some("project-2".to_string()),
        Some("worktree-2".to_string()),
        true,
        true,
    );

    assert_eq!(plan.bind_session_id.as_deref(), Some("worktree-split"));
    assert_eq!(plan.request_project_select_id, None);
    assert_eq!(
        runtime.snapshot().active_session_id.as_deref(),
        Some("worktree-split")
    );
    assert_eq!(
        runtime.snapshot().selected_worktree_id.as_deref(),
        Some("worktree-2")
    );
    assert_eq!(
        runtime
            .current_project_terminals()
            .into_iter()
            .map(|terminal| terminal.id)
            .collect::<Vec<_>>(),
        vec!["worktree-split".to_string(), "worktree-tab".to_string()]
    );

    runtime.select_terminal(RemoteRuntimeTerminal {
        worktree_id: Some("worktree-2".to_string()),
        layout_kind: "tab".to_string(),
        layout_order: Some(1),
        ..terminal("worktree-tab", "project-2")
    });
    runtime.apply_worktree_selected(
        Some("project-2".to_string()),
        Some("project-2".to_string()),
        true,
        true,
    );
    let back = runtime.apply_worktree_selected(
        Some("project-2".to_string()),
        Some("worktree-2".to_string()),
        true,
        true,
    );

    assert_eq!(back.bind_session_id.as_deref(), Some("worktree-tab"));
    assert_eq!(
        runtime.snapshot().active_session_id.as_deref(),
        Some("worktree-tab")
    );
}

#[test]
fn runtime_model_uses_project_list_worktree_scope_for_controller_selection() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(
        projects(),
        Some("project-2".to_string()),
        Some("worktree-2".to_string()),
        true,
        false,
    );

    let plan = runtime.apply_terminal_list(
        vec![
            RemoteRuntimeTerminal {
                worktree_id: Some("project-2".to_string()),
                layout_order: Some(0),
                ..terminal("default-session", "project-2")
            },
            RemoteRuntimeTerminal {
                worktree_id: Some("worktree-2".to_string()),
                layout_order: Some(0),
                ..terminal("worktree-session", "project-2")
            },
        ],
        true,
        true,
    );

    assert_eq!(plan.bind_session_id.as_deref(), Some("worktree-session"));
    assert_eq!(
        runtime.snapshot().selected_worktree_id.as_deref(),
        Some("worktree-2")
    );
    assert_eq!(
        runtime.snapshot().active_session_id.as_deref(),
        Some("worktree-session")
    );
}

#[test]
fn runtime_model_scope_key_matches_desktop_layout_owner_rules() {
    let mut runtime = RuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.apply_worktree_state(
        RuntimeWorktreeState {
            project_id: Some("project-1".to_string()),
            selected_worktree_id: None,
            worktrees: vec![
                RemoteRuntimeWorktree {
                    is_default: false,
                    ..worktree("worktree-1", "project-1")
                },
                worktree("project-1", "project-1"),
            ],
            base_branches: Vec::new(),
            default_base_branch: None,
        },
        false,
        true,
        true,
    );

    assert_eq!(
        runtime.snapshot().selected_worktree_id.as_deref(),
        Some("project-1")
    );
    assert_eq!(
        runtime.selected_scope_key().as_deref(),
        Some("project-1::project-1")
    );

    runtime.apply_worktree_selected(
        Some("project-1".to_string()),
        Some("worktree-1".to_string()),
        true,
        true,
    );

    assert_eq!(
        runtime.snapshot().selected_worktree_id.as_deref(),
        Some("worktree-1")
    );
    assert_eq!(
        runtime.selected_scope_key().as_deref(),
        Some("project-1::worktree-1")
    );
    assert_eq!(
        runtime_scope_key("project-1", Some("worktree-1")).as_str(),
        "project-1::worktree-1"
    );
    assert_eq!(
        super::runtime_scope_parts("project-1::worktree-1"),
        Some(("project-1", "worktree-1"))
    );
    assert_eq!(super::runtime_scope_parts("project-1::"), None);
}

#[test]
fn runtime_model_terminal_scope_queries_use_core_state() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.apply_worktree_selected(
        Some("project-1".to_string()),
        Some("worktree-1".to_string()),
        true,
        false,
    );
    runtime.apply_terminal_list(
        vec![
            RemoteRuntimeTerminal {
                worktree_id: Some("worktree-1".to_string()),
                ..terminal("session-1", "project-1")
            },
            terminal("session-2", "project-2"),
        ],
        true,
        true,
    );

    let project_scope = runtime
        .terminal_scope_for_project("project-1")
        .expect("project scope");
    assert_eq!(project_scope.project_id, "project-1");
    assert_eq!(
        project_scope.project_path.as_deref(),
        Some("/tmp/project-1")
    );
    assert_eq!(project_scope.worktree_id.as_deref(), Some("worktree-1"));

    let session_scope = runtime
        .terminal_scope_for_session("session-2", None)
        .expect("session scope");
    assert_eq!(session_scope.project_id, "project-2");
    assert_eq!(
        session_scope.project_path.as_deref(),
        Some("/tmp/project-2")
    );
    assert_eq!(session_scope.worktree_id.as_deref(), Some("project-2"));
}

#[test]
fn runtime_model_terminal_scope_uses_explicit_terminal_after_list_removal() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);

    let scope = runtime
        .terminal_scope_for_session(
            "session-2",
            Some(RemoteRuntimeTerminal {
                worktree_id: Some("worktree-2".to_string()),
                ..terminal("session-2", "project-2")
            }),
        )
        .expect("explicit terminal scope");

    assert_eq!(scope.project_id, "project-2");
    assert_eq!(scope.project_path.as_deref(), Some("/tmp/project-2"));
    assert_eq!(scope.worktree_id.as_deref(), Some("worktree-2"));
}

#[test]
fn runtime_model_restores_project_worktree_scope_after_switching_projects() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.apply_worktree_state(
        RemoteRuntimeWorktreeState {
            project_id: Some("project-1".to_string()),
            selected_worktree_id: Some("project-1".to_string()),
            worktrees: vec![
                worktree("project-1", "project-1"),
                worktree("worktree-1", "project-1"),
            ],
            base_branches: vec!["main".to_string()],
            default_base_branch: Some("main".to_string()),
        },
        false,
        true,
        false,
    );
    runtime.apply_terminal_list(
        vec![
            RemoteRuntimeTerminal {
                worktree_id: Some("project-1".to_string()),
                layout_order: Some(0),
                ..terminal("project-1-main", "project-1")
            },
            RemoteRuntimeTerminal {
                worktree_id: Some("worktree-1".to_string()),
                layout_order: Some(0),
                ..terminal("project-1-task", "project-1")
            },
            RemoteRuntimeTerminal {
                worktree_id: Some("project-2".to_string()),
                layout_order: Some(0),
                ..terminal("project-2-main", "project-2")
            },
        ],
        true,
        true,
    );
    let select_worktree = runtime.apply_worktree_selected(
        Some("project-1".to_string()),
        Some("worktree-1".to_string()),
        true,
        true,
    );
    assert_eq!(
        select_worktree.bind_session_id.as_deref(),
        Some("project-1-task")
    );

    let select_project_2 = runtime.user_select_project(projects()[1].clone(), true);
    assert_eq!(
        select_project_2.bind_session_id.as_deref(),
        Some("project-2-main")
    );
    assert_eq!(
        runtime.snapshot().selected_worktree_id.as_deref(),
        Some("project-2")
    );

    let back_to_project_1 = runtime.user_select_project(projects()[0].clone(), true);

    assert_eq!(
        runtime.snapshot().selected_worktree_id.as_deref(),
        Some("worktree-1")
    );
    assert_eq!(
        back_to_project_1.bind_session_id.as_deref(),
        Some("project-1-task")
    );
    assert_eq!(
        runtime.snapshot().active_session_id.as_deref(),
        Some("project-1-task")
    );
}

#[test]
fn runtime_model_waits_for_worktree_terminal_list_without_project_select() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-2".to_string()), None, true, false);
    runtime.apply_terminal_list(
        vec![RemoteRuntimeTerminal {
            worktree_id: Some("project-2".to_string()),
            ..terminal("default-session", "project-2")
        }],
        true,
        true,
    );

    let plan = runtime.apply_worktree_selected(
        Some("project-2".to_string()),
        Some("worktree-2".to_string()),
        true,
        true,
    );

    assert_eq!(plan.bind_session_id, None);
    assert_eq!(plan.request_project_select_id, None);
    assert!(plan.request_terminal_list);
    assert_eq!(runtime.snapshot().active_session_id, None);

    let repeated = runtime.apply_terminal_list(
        vec![RemoteRuntimeTerminal {
            worktree_id: Some("project-2".to_string()),
            ..terminal("default-session", "project-2")
        }],
        true,
        true,
    );

    assert_eq!(repeated.bind_session_id, None);
    assert_eq!(repeated.request_project_select_id, None);
    assert!(!repeated.request_terminal_list);
    assert_eq!(runtime.snapshot().active_session_id, None);
}

#[test]
fn runtime_model_rejects_unknown_worktree_when_worktree_list_is_loaded() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.apply_worktree_state(
        RemoteRuntimeWorktreeState {
            project_id: Some("project-1".to_string()),
            selected_worktree_id: Some("project-1".to_string()),
            worktrees: vec![
                worktree("project-1", "project-1"),
                worktree("worktree-1", "project-1"),
            ],
            base_branches: vec!["main".to_string()],
            default_base_branch: Some("main".to_string()),
        },
        false,
        true,
        false,
    );
    runtime.apply_terminal_list(
        vec![
            RemoteRuntimeTerminal {
                worktree_id: Some("project-1".to_string()),
                layout_order: Some(0),
                ..terminal("main-session", "project-1")
            },
            RemoteRuntimeTerminal {
                worktree_id: Some("worktree-1".to_string()),
                layout_order: Some(0),
                ..terminal("worktree-session", "project-1")
            },
        ],
        true,
        true,
    );
    runtime.apply_worktree_selected(
        Some("project-1".to_string()),
        Some("worktree-1".to_string()),
        true,
        true,
    );

    let plan = runtime.apply_worktree_selected(
        Some("project-1".to_string()),
        Some("missing-worktree".to_string()),
        true,
        true,
    );

    assert_eq!(
        runtime.snapshot().selected_worktree_id.as_deref(),
        Some("worktree-1")
    );
    assert_eq!(
        runtime.snapshot().active_session_id.as_deref(),
        Some("worktree-session")
    );
    assert_eq!(plan.bind_session_id.as_deref(), Some("worktree-session"));
    assert!(!plan.request_terminal_list);
    assert_eq!(plan.request_project_select_id, None);
}

#[test]
fn runtime_model_caches_worktrees_by_project() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);

    runtime.apply_worktree_state(
        RemoteRuntimeWorktreeState {
            project_id: Some("project-1".to_string()),
            selected_worktree_id: Some("project-1".to_string()),
            worktrees: vec![worktree("project-1", "project-1")],
            base_branches: vec!["main".to_string()],
            default_base_branch: Some("main".to_string()),
        },
        false,
        true,
        true,
    );
    runtime.apply_worktree_state(
        RemoteRuntimeWorktreeState {
            project_id: Some("project-2".to_string()),
            selected_worktree_id: Some("worktree-2".to_string()),
            worktrees: vec![worktree("worktree-2", "project-2")],
            base_branches: vec!["develop".to_string()],
            default_base_branch: Some("develop".to_string()),
        },
        false,
        true,
        true,
    );

    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.worktrees.len(), 2);
    assert!(
        snapshot
            .worktrees
            .iter()
            .any(|item| item.project_id == "project-1")
    );
    assert!(
        snapshot
            .worktrees
            .iter()
            .any(|item| item.project_id == "project-2")
    );
    assert_eq!(
        snapshot
            .base_branches_by_project
            .get("project-1")
            .cloned()
            .unwrap_or_default(),
        vec!["main".to_string()]
    );
    assert_eq!(
        snapshot
            .default_base_branch_by_project
            .get("project-2")
            .map(String::as_str),
        Some("develop")
    );
}

#[test]
fn runtime_model_caches_full_worktree_snapshot_from_project_list_payload() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);

    runtime.apply_worktree_state(
        RemoteRuntimeWorktreeState {
            project_id: None,
            selected_worktree_id: None,
            worktrees: vec![
                worktree("project-1", "project-1"),
                worktree("worktree-1", "project-1"),
                worktree("project-2", "project-2"),
                worktree("worktree-2", "project-2"),
            ],
            base_branches: Vec::new(),
            default_base_branch: None,
        },
        false,
        true,
        false,
    );

    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.worktrees.len(), 4);
    assert_eq!(snapshot.selected_worktree_id.as_deref(), Some("project-1"));
    assert!(
        snapshot
            .worktrees
            .iter()
            .any(|worktree| worktree.project_id == "project-2" && worktree.id == "worktree-2")
    );
}

#[test]
fn runtime_model_initializes_selected_worktree_from_list() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);

    runtime.apply_worktree_state(
        RemoteRuntimeWorktreeState {
            project_id: Some("project-1".to_string()),
            selected_worktree_id: None,
            worktrees: vec![
                worktree("worktree-1", "project-1"),
                worktree("project-1", "project-1"),
            ],
            base_branches: vec!["main".to_string()],
            default_base_branch: Some("main".to_string()),
        },
        false,
        true,
        true,
    );

    assert_eq!(
        runtime.snapshot().selected_worktree_id.as_deref(),
        Some("project-1")
    );
}

#[test]
fn runtime_model_preserves_local_worktree_selection_on_list_refresh() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.apply_worktree_state(
        RemoteRuntimeWorktreeState {
            project_id: Some("project-1".to_string()),
            selected_worktree_id: Some("project-1".to_string()),
            worktrees: vec![
                worktree("project-1", "project-1"),
                worktree("worktree-1", "project-1"),
            ],
            base_branches: vec!["main".to_string()],
            default_base_branch: Some("main".to_string()),
        },
        false,
        true,
        true,
    );
    runtime.apply_worktree_selected(
        Some("project-1".to_string()),
        Some("worktree-1".to_string()),
        true,
        true,
    );

    runtime.apply_worktree_state(
        RemoteRuntimeWorktreeState {
            project_id: Some("project-1".to_string()),
            selected_worktree_id: Some("project-1".to_string()),
            worktrees: vec![
                worktree("project-1", "project-1"),
                worktree("worktree-1", "project-1"),
            ],
            base_branches: vec!["main".to_string()],
            default_base_branch: Some("main".to_string()),
        },
        false,
        true,
        true,
    );

    assert_eq!(
        runtime.snapshot().selected_worktree_id.as_deref(),
        Some("worktree-1")
    );
}

#[test]
fn runtime_model_worktree_update_confirmed_selection_wins_over_previous_selection() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.apply_terminal_list(
        vec![
            RemoteRuntimeTerminal {
                worktree_id: Some("project-1".to_string()),
                ..terminal("main-session", "project-1")
            },
            RemoteRuntimeTerminal {
                worktree_id: Some("worktree-1".to_string()),
                ..terminal("worktree-session", "project-1")
            },
        ],
        true,
        true,
    );
    runtime.apply_worktree_state(
        RemoteRuntimeWorktreeState {
            project_id: Some("project-1".to_string()),
            selected_worktree_id: Some("project-1".to_string()),
            worktrees: vec![
                worktree("project-1", "project-1"),
                worktree("worktree-1", "project-1"),
            ],
            base_branches: vec!["main".to_string()],
            default_base_branch: Some("main".to_string()),
        },
        false,
        true,
        true,
    );

    let plan = runtime.apply_worktree_state(
        RemoteRuntimeWorktreeState {
            project_id: Some("project-1".to_string()),
            selected_worktree_id: Some("worktree-1".to_string()),
            worktrees: vec![
                worktree("project-1", "project-1"),
                worktree("worktree-1", "project-1"),
            ],
            base_branches: vec!["main".to_string()],
            default_base_branch: Some("main".to_string()),
        },
        true,
        true,
        true,
    );

    assert_eq!(plan.bind_session_id.as_deref(), Some("worktree-session"));
    assert_eq!(
        runtime.snapshot().selected_worktree_id.as_deref(),
        Some("worktree-1")
    );
    assert_eq!(
        runtime.snapshot().active_session_id.as_deref(),
        Some("worktree-session")
    );
}

#[test]
fn runtime_model_keeps_bound_local_selection_over_stale_host_project_list() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, false);
    runtime.apply_terminal_list(
        vec![
            terminal("session-1", "project-1"),
            terminal("session-2", "project-2"),
        ],
        true,
        true,
    );
    runtime.user_select_project(projects()[1].clone(), true);

    let stale =
        runtime.apply_project_list(projects(), Some("project-1".to_string()), None, true, true);

    assert_eq!(stale.bind_session_id, None);
    assert_eq!(
        runtime.snapshot().selected_project_id.as_deref(),
        Some("project-2")
    );
    assert_eq!(
        runtime.snapshot().active_session_id.as_deref(),
        Some("session-2")
    );
}

#[test]
fn runtime_model_host_selection_replaces_unbound_cached_project() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.restore_cached_projects(projects());

    let plan =
        runtime.apply_project_list(projects(), Some("project-2".to_string()), None, true, true);

    assert!(plan.clear_terminal);
    assert_eq!(
        runtime.snapshot().selected_project_id.as_deref(),
        Some("project-2")
    );
    assert_eq!(runtime.snapshot().active_session_id, None);
}

#[test]
fn apply_git_status_stores_projection_by_project() {
    let mut runtime = RemoteRuntimeModel::new();
    let status = serde_json::json!({
        "projectId": "project-1",
        "branch": "main",
        "changes": 3,
    });
    let plan = runtime.apply_git_status(status.clone());
    assert!(plan.state_changed);
    let snapshot = runtime.snapshot();
    assert_eq!(
        snapshot.git_status_by_project.get("project-1"),
        Some(&status)
    );

    // A status without a project id is ignored and changes nothing.
    let ignored = runtime.apply_git_status(serde_json::json!({ "branch": "x" }));
    assert_eq!(ignored, RemoteRuntimePlan::default());
    assert_eq!(runtime.snapshot().git_status_by_project.len(), 1);

    // A full reset drops git status; keep_projects retains it.
    runtime.reset(true);
    assert_eq!(runtime.snapshot().git_status_by_project.len(), 1);
    runtime.reset(false);
    assert!(runtime.snapshot().git_status_by_project.is_empty());
}

/// Drift tripwire: the JSON keys of a fully-populated `RemoteRuntimePlan` must
/// match the set the mobile FFI binding (`RemoteRuntimeCorePlan.fromJson` /
/// `_planFromCore` in remote_runtime_store.dart) reads. If this fails because a
/// field was added/renamed here, update the Dart binding to match — otherwise
/// the new field is silently dropped at the FFI boundary.
#[test]
fn runtime_plan_json_keys_match_dart_binding() {
    let plan = RemoteRuntimePlan {
        state_changed: true,
        clear_terminal: true,
        reset_terminal_input: true,
        reset_terminal_buffer: true,
        request_terminal_list: true,
        request_project_select_id: Some("p".to_string()),
        bind_session_id: Some("s".to_string()),
        bind_full_buffer: true,
        flush_terminal_input: true,
        removed_session_id: Some("r".to_string()),
    };
    let value = serde_json::to_value(&plan).expect("plan serializes");
    let mut keys: Vec<String> = value
        .as_object()
        .expect("plan is a JSON object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    let expected = vec![
        "bindFullBuffer",
        "bindSessionId",
        "clearTerminal",
        "flushTerminalInput",
        "removedSessionId",
        "requestProjectSelectId",
        "requestTerminalList",
        "resetTerminalBuffer",
        "resetTerminalInput",
        "stateChanged",
    ];
    assert_eq!(
        keys, expected,
        "RemoteRuntimePlan fields changed — update the Dart RemoteRuntimeCorePlan binding and _planFromCore mapping to match"
    );
}

fn worktree(id: &str, project_id: &str) -> RemoteRuntimeWorktree {
    RemoteRuntimeWorktree {
        id: id.to_string(),
        project_id: project_id.to_string(),
        name: id.to_string(),
        branch: "main".to_string(),
        path: format!("/tmp/{id}"),
        status: "clean".to_string(),
        is_default: id == project_id,
        exists: true,
        base_branch: None,
        changes: 0,
        incoming: 0,
        outgoing: 0,
        additions: 0,
        deletions: 0,
    }
}

fn projects() -> Vec<RemoteRuntimeProject> {
    vec![
        RemoteRuntimeProject {
            id: "project-1".to_string(),
            name: "Project 1".to_string(),
            path: Some("/tmp/project-1".to_string()),
        },
        RemoteRuntimeProject {
            id: "project-2".to_string(),
            name: "Project 2".to_string(),
            path: Some("/tmp/project-2".to_string()),
        },
    ]
}

fn terminal(id: &str, project_id: &str) -> RemoteRuntimeTerminal {
    RemoteRuntimeTerminal {
        id: id.to_string(),
        title: id.to_string(),
        project_id: project_id.to_string(),
        worktree_id: None,
        layout_kind: "split".to_string(),
        layout_order: None,
        cols: None,
        rows: None,
        status: None,
        created_at: Some(id.to_string()),
        buffer_characters: None,
    }
}
