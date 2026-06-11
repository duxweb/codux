use super::*;

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
fn restores_visible_screen_from_screen_baseline_while_retaining_history_content() {
    let mut session = RemotePtySession::<String>::new("session-1", 256);

    session.replace_from_baseline_screen(
        "raw history that should stay in scrollback cache",
        Some("\x1b[2J\x1b[Hvisible tui"),
        Some(43),
        Some(7),
    );

    let screen = session.screen_snapshot();
    assert_eq!(
        session.content(),
        "raw history that should stay in scrollback cache"
    );
    assert_eq!(session.buffer_length(), 43);
    assert_eq!(session.sequence(), 7);
    assert!(screen.data.contains("visible tui"));
    session.scroll_screen_lines(8);
    let scrolled = session.screen_snapshot();
    assert!(scrolled.data.contains("raw history"));
}

#[test]
fn live_output_can_update_visible_screen_from_screen_keyframe() {
    let mut session = RemotePtySession::<String>::new("session-1", 256);
    session.replace_from_baseline_screen(
        "cached raw history",
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
    assert_eq!(session.content(), "cached raw historypartial live raw");
    assert_eq!(session.buffer_length(), 32);
    assert_eq!(session.sequence(), 4);
    assert!(screen.data.contains("restored tui"));
    assert!(screen.data.contains("input box"));
    assert!(!screen.data.contains("old screen"));

    session.scroll_screen_lines(8);
    let scrolled = session.screen_snapshot();
    assert!(scrolled.data.contains("cached raw history"));
    assert!(scrolled.data.contains("partial live raw"));
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
    runtime.apply_project_list(projects(), Some("project-1".to_string()), true, false);

    let select = runtime.user_select_project(projects()[1].clone(), true);
    assert_eq!(
        select.request_project_select_id.as_deref(),
        Some("project-2")
    );
    runtime.mark_project_select_sent("project-2");

    let confirmed = runtime.project_selected(Some("project-2".to_string()));
    assert!(confirmed.request_terminal_list);
    assert_eq!(runtime.pending_project_select(true), None);

    let retry = runtime.ensure_terminal_for_selected_project(true, true);
    assert_eq!(retry.request_project_select_id, None);
    assert!(!retry.request_terminal_list);
}

#[test]
fn runtime_model_binds_terminal_when_delayed_list_arrives() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), true, false);
    runtime.user_select_project(projects()[1].clone(), true);
    runtime.mark_project_select_sent("project-2");
    runtime.project_selected(Some("project-2".to_string()));

    let before_list =
        runtime.apply_project_list(projects(), Some("project-2".to_string()), true, true);
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
    runtime.apply_project_list(projects(), Some("project-1".to_string()), true, false);
    runtime.user_select_project(projects()[1].clone(), true);

    runtime.apply_project_list(projects(), Some("project-1".to_string()), true, false);

    assert_eq!(
        runtime.snapshot().selected_project_id.as_deref(),
        Some("project-2")
    );
}

#[test]
fn runtime_model_terminal_list_does_not_ack_pending_project_select() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), true, false);
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
    runtime.apply_project_list(projects(), Some("project-1".to_string()), true, false);
    runtime.user_select_project(projects()[1].clone(), true);
    runtime.mark_project_select_sent("project-2");

    runtime.apply_project_list(projects(), Some("project-2".to_string()), true, true);

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

    let stale = runtime.project_selected(Some("project-2".to_string()));

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
    runtime.apply_project_list(projects(), Some("project-1".to_string()), true, false);
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
fn runtime_model_keeps_bound_local_selection_over_stale_host_project_list() {
    let mut runtime = RemoteRuntimeModel::new();
    runtime.apply_project_list(projects(), Some("project-1".to_string()), true, false);
    runtime.apply_terminal_list(
        vec![
            terminal("session-1", "project-1"),
            terminal("session-2", "project-2"),
        ],
        true,
        true,
    );
    runtime.user_select_project(projects()[1].clone(), true);

    let stale = runtime.apply_project_list(projects(), Some("project-1".to_string()), true, true);

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
        layout_kind: "split".to_string(),
        cols: None,
        rows: None,
        status: None,
        created_at: Some(id.to_string()),
        buffer_characters: None,
    }
}
