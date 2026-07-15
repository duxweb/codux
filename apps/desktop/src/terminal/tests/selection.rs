use super::super::*;

#[test]
fn search_buffer_finds_case_insensitive_matches_across_scrollback() {
    let mut state = TerminalModel::new_for_test(10, 3, 100);
    state.process_bytes(b"alpha\r\nbeta\r\nALPHA x\r\ngamma\r\nalphabet\r\n1\r\n2\r\n3");
    state.handle.publish_snapshot();

    let matches = state.search_buffer("alpha", 100);

    assert_eq!(matches.len(), 3);
    assert_eq!(matches[0].start.line, 0);
    assert_eq!(matches[0].start.col, 0);
    assert_eq!(matches[0].end.col, 5);
    assert!(
        matches
            .windows(2)
            .all(|pair| pair[0].start.line < pair[1].start.line)
    );

    assert!(state.search_buffer("nothing-here", 100).is_empty());
    assert_eq!(state.search_buffer("alpha", 2).len(), 2);
}
#[test]
fn select_all_covers_scrollback_and_viewport() {
    let mut state = TerminalModel::new_for_test(10, 3, 100);
    state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour");
    state.handle.publish_snapshot();

    let range = state.select_all();
    let content = state.snapshot();

    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.col, 0);
    assert_eq!(range.end.line, content.total_lines as i32 - 1);

    let text = state.selected_text().expect("select-all yields text");
    assert!(text.contains("one"));
    assert!(text.contains("four"));
}
#[test]
fn selects_text_from_terminal_grid() {
    let mut state = TerminalModel::new_for_test(10, 4, 100);
    state.process_bytes(b"hello\r\nworld");
    state.handle.publish_snapshot();

    assert_eq!(
        state.handle.selected_text_for_range(SelectionRange {
            start: TerminalSelectionPoint { line: 0, col: 0 },
            end: TerminalSelectionPoint { line: 1, col: 5 },
        }),
        "hello\nworld"
    );
}
#[test]
fn selects_text_across_scrollback_outside_visible_snapshot() {
    let mut state = TerminalModel::new_for_test(10, 3, 100);
    state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
    state.handle.publish_snapshot();

    assert_eq!(
        state.handle.selected_text_for_range(SelectionRange {
            start: TerminalSelectionPoint { line: 1, col: 0 },
            end: TerminalSelectionPoint { line: 5, col: 3 },
        }),
        "two\nthree\nfour\nfive\nsix"
    );
}
#[test]
fn wrapped_line_copies_without_seam_newline() {
    // 15 chars into a 10-col grid soft-wraps onto a second row; copying the
    // whole visual line must not inject a newline at the wrap.
    let mut state = TerminalModel::new_for_test(10, 4, 100);
    state.process_bytes(b"abcdefghijklmno");
    state.handle.publish_snapshot();

    assert_eq!(
        state.handle.selected_text_for_range(SelectionRange {
            start: TerminalSelectionPoint { line: 0, col: 0 },
            end: TerminalSelectionPoint { line: 1, col: 5 },
        }),
        "abcdefghijklmno"
    );
}
#[test]
fn wrapped_and_hard_lines_copy_with_only_real_newline() {
    let mut state = TerminalModel::new_for_test(10, 4, 100);
    state.process_bytes(b"abcdefghijklmno\r\nsecond");
    state.handle.publish_snapshot();

    assert_eq!(
        state.handle.selected_text_for_range(SelectionRange {
            start: TerminalSelectionPoint { line: 0, col: 0 },
            end: TerminalSelectionPoint { line: 2, col: 6 },
        }),
        "abcdefghijklmno\nsecond"
    );
}
#[test]
fn double_click_selects_word_under_cell() {
    let mut state = TerminalModel::new_for_test(20, 4, 100);
    state.process_bytes(b"foo bar baz");
    state.handle.publish_snapshot();

    let range = state
        .select_word_at(TerminalSelectionPoint { line: 0, col: 5 })
        .expect("double-click resolves a word");
    assert_eq!(range.start.col, 4);
    assert_eq!(state.selected_text(), Some("bar".to_string()));
}
#[test]
fn double_click_on_blank_selects_nothing() {
    let mut state = TerminalModel::new_for_test(20, 4, 100);
    state.process_bytes(b"foo");
    state.handle.publish_snapshot();

    assert!(
        state
            .select_word_at(TerminalSelectionPoint { line: 0, col: 10 })
            .is_none()
    );
}
#[test]
fn triple_click_selects_whole_wrapped_line() {
    // Clicking the wrapped continuation still selects the entire logical line.
    let mut state = TerminalModel::new_for_test(10, 4, 100);
    state.process_bytes(b"abcdefghijklmno");
    state.handle.publish_snapshot();

    state
        .select_line_at(TerminalSelectionPoint { line: 1, col: 2 })
        .expect("triple-click resolves a line");
    assert_eq!(state.selected_text(), Some("abcdefghijklmno".to_string()));
}
#[test]
fn selection_autoscroll_updates_head_against_scrolled_viewport() {
    let mut state = TerminalModel::new_for_test(10, 3, 100);
    state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
    state.handle.publish_snapshot();
    state.start_selection(TerminalSelectionPoint { line: 6, col: 5 });

    assert!(state.scroll_display(2));
    let (did_scroll, content) = state.apply_pending_scroll_for_selection();
    assert!(did_scroll);
    assert_eq!(content.display_offset, 2);

    let scrolled_top = selection_point_from_cell(TerminalCellPoint { row: 0, col: 0 }, &content);
    state.update_selection(scrolled_top);

    assert_eq!(
        state.selected_text(),
        Some("three\nfour\nfive\nsix\nseven".to_string())
    );
}
#[test]
fn keeps_utf8_cjk_output_in_terminal_grid() {
    let mut state = TerminalModel::new_for_test(20, 4, 100);
    state.process_bytes("中文恢复记录".as_bytes());
    state.handle.publish_snapshot();

    assert_eq!(
        state.handle.selected_text_for_range(SelectionRange {
            start: TerminalSelectionPoint { line: 0, col: 0 },
            end: TerminalSelectionPoint { line: 0, col: 11 },
        }),
        "中文恢复记录"
    );
}
#[test]
fn selection_tracks_output_scrollback_rotation() {
    let mut state = TerminalModel::new_for_test(10, 3, 100);
    state.process_bytes(b"one\r\ntwo\r\nthree");
    state.handle.publish_snapshot();
    state.start_selection(TerminalSelectionPoint { line: 1, col: 0 });
    state.update_selection(TerminalSelectionPoint { line: 1, col: 3 });
    assert_eq!(state.selected_text(), Some("two".to_string()));

    state.process_bytes(b"\r\nfour");
    state.handle.publish_snapshot();

    assert_eq!(state.selected_text(), Some("two".to_string()));
    assert_eq!(
        state.selection_range(),
        Some(SelectionRange {
            start: TerminalSelectionPoint { line: 1, col: 0 },
            end: TerminalSelectionPoint { line: 1, col: 3 },
        })
    );
}
#[test]
fn shift_click_extends_existing_terminal_selection_anchor() {
    let mut selection = SelectionState::default();
    selection.start(TerminalSelectionPoint { line: 2, col: 4 });
    selection.finish(TerminalSelectionPoint { line: 2, col: 8 });
    selection.extend(TerminalSelectionPoint { line: 4, col: 3 });

    assert_eq!(
        selection.anchor,
        Some(TerminalSelectionPoint { line: 2, col: 4 })
    );
    assert_eq!(
        selection.head,
        Some(TerminalSelectionPoint { line: 4, col: 3 })
    );
    assert!(selection.dragging);
}
