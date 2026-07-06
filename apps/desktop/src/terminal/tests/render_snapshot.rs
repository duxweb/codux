use super::super::*;
use super::fixtures::*;

#[test]
fn nerd_font_private_use_cells_use_symbol_font_and_stay_grid_anchored() {
    let renderer = TerminalRenderer::new(
        default_terminal_font_family().to_string(),
        px(14.0),
        DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
        ColorPalette::default(),
    );
    let mut clock = test_cell(
        TerminalScreenColor::Default,
        TerminalScreenColor::Default,
        false,
        false,
    );
    clock.text = "\u{f017}".to_string();
    let mut plain = clock.clone();
    plain.text = "a".to_string();
    let cells = vec![
        TerminalIndexedCell {
            point: TerminalPoint { line: 0, column: 0 },
            cell: clock,
        },
        TerminalIndexedCell {
            point: TerminalPoint { line: 0, column: 1 },
            cell: plain,
        },
    ];

    let mut text_runs = Vec::new();
    let mut graphics = Vec::new();
    renderer.prepare_row_text(0, &cells, &mut text_runs, &mut graphics, None);

    assert_eq!(text_runs.len(), 2);
    assert_eq!(
        text_runs[0].style.font.family.as_ref(),
        TERMINAL_SYMBOL_FONT_FAMILY
    );
    assert_eq!(
        text_runs[1].style.font.family.as_ref(),
        default_terminal_font_family()
    );
    assert!(renderer.combine_row_runs(&text_runs).is_none());

    assert!(terminal_cell_is_private_use("\u{f0954}"));
    assert!(!terminal_cell_is_private_use("a"));
    assert!(!terminal_cell_is_private_use("拼"));
}
#[test]
fn processed_output_updates_live_terminal_before_paint_snapshot() {
    let mut state = TerminalModel::new_for_test(10, 4, 100);
    state.process_bytes(b"before");

    state.process_bytes(b"\r\x1b[2Kduring");
    let paint_snapshot = state.handle.snapshot();
    let paint_text = row_text(&paint_snapshot, 0);
    assert!(!paint_text.contains("during"));

    let live = state.live_snapshot();

    let live_text = row_text(&live, 0);

    assert!(live_text.contains("during"));

    state.handle.publish_snapshot();
    let published = state.handle.snapshot();
    let published_text = row_text(&published, 0);

    assert!(published_text.contains("during"));
}
#[test]
fn updates_render_snapshot_after_output() {
    let mut state = TerminalModel::new_for_test(10, 4, 100);
    state.process_bytes(b"hello");

    let pending_snapshot = state.handle.snapshot();
    let pending_text = row_text(&pending_snapshot, 0);
    assert!(!pending_text.contains("hello"));

    state.handle.publish_snapshot();

    let snapshot = state.handle.snapshot();
    let text = row_text(&snapshot, 0);

    assert!(text.contains("hello"));
    assert_eq!(snapshot.columns, 10);
    assert_eq!(snapshot.screen_lines, 4);
}
#[test]
fn first_real_output_replaces_restored_bootstrap() {
    let mut state = TerminalModel::new_for_test_with_restored_output(
        20,
        4,
        100,
        TerminalOutputSnapshot {
            bytes: 8,
            tail: "restored".to_string(),
        },
    );
    let bootstrapped = state.handle.snapshot();
    assert!(row_text(&bootstrapped, 0).contains("restored"));

    state.process_output_bytes_for_test(b"live");
    let snapshot = state.sync_for_test();

    assert!(!row_text(&snapshot, 0).contains("restored"));
    assert!(row_text(&snapshot, 0).contains("live"));
}
#[test]
fn stale_snapshot_does_not_replace_published_terminal_content() {
    let mut state = TerminalModel::new_for_test(20, 4, 100);
    state.process_bytes(b"stable");
    state.handle.publish_snapshot();
    let stale_empty = TerminalScreenSnapshot {
        cols: 20,
        rows: 4,
        total_lines: 4,
        ..TerminalScreenSnapshot::default()
    };

    state.snapshot_dirty = true;
    let published = state.publish_completed_snapshot(stale_empty, TERMINAL_SNAPSHOT_PUBLISH_SLOW);
    let snapshot = state.handle.snapshot();

    assert!(!published);
    assert!(state.snapshot_dirty);
    assert!(row_text(&snapshot, 0).contains("stable"));
}
#[test]
fn stale_resized_snapshot_does_not_replace_current_dimensions() {
    let mut state = TerminalModel::new_for_test(20, 4, 100);
    state.process_bytes(b"stable");
    state.handle.publish_snapshot();
    state.resize(
        30,
        6,
        TerminalWindowSize {
            num_lines: 6,
            num_cols: 30,
            cell_width: 1,
            cell_height: 1,
        },
    );
    assert!(state.apply_model_events());
    let stale_old_size = TerminalScreenSnapshot {
        cols: 20,
        rows: 4,
        total_lines: 4,
        cells: vec![TerminalScreenCellSnapshot {
            row: 0,
            col: 0,
            text: "old".to_string(),
            width: 1,
            fg: TerminalScreenColor::Default,
            bg: TerminalScreenColor::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            inverse: false,
            hidden: false,
            strikeout: false,
        }],
        ..TerminalScreenSnapshot::default()
    };

    let published =
        state.publish_completed_snapshot(stale_old_size, TERMINAL_SNAPSHOT_PUBLISH_SLOW);
    let snapshot = state.handle.snapshot();

    assert!(!published);
    assert!(state.snapshot_dirty);
    assert_eq!(state.dimensions(), (30, 6));
    assert_eq!(snapshot.columns, 20);
    assert!(row_text(&snapshot, 0).contains("stable"));
}
#[test]
fn output_batching_keeps_non_empty_snapshot_publishing() {
    let mut state = TerminalModel::new_for_test(20, 4, 100);
    state.process_bytes(b"stable");
    state.handle.publish_snapshot();
    let next = TerminalScreenSnapshot {
        cols: 20,
        rows: 4,
        total_lines: 4,
        cells: vec![TerminalScreenCellSnapshot {
            row: 0,
            col: 0,
            text: "next".to_string(),
            width: 1,
            fg: TerminalScreenColor::Default,
            bg: TerminalScreenColor::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            inverse: false,
            hidden: false,
            strikeout: false,
        }],
        ..TerminalScreenSnapshot::default()
    };

    state.output_flush_pending = true;
    let published = state.publish_completed_snapshot(next, TERMINAL_SNAPSHOT_PUBLISH_SLOW);
    let snapshot = state.handle.snapshot();

    assert!(published);
    assert!(row_text(&snapshot, 0).contains("next"));
}
#[test]
fn updates_render_snapshot_after_resize() {
    let state = TerminalModel::new_for_test(10, 4, 100);
    let handle = state.handle.clone();
    assert!(handle.resize(20, 8));
    handle.publish_snapshot();

    let snapshot = handle.snapshot();
    assert_eq!(snapshot.columns, 20);
    assert_eq!(snapshot.screen_lines, 8);
    assert!(!handle.resize(20, 8));
}
#[test]
fn engine_resize_throttles_drag_bursts_but_applies_final_target() {
    let window_size = |cols: u16, rows: u16| TerminalWindowSize {
        num_lines: rows,
        num_cols: cols,
        cell_width: 1,
        cell_height: 1,
    };
    let mut state = TerminalModel::new_for_test(20, 4, 100);

    state.resize(30, 6, window_size(30, 6));
    state.apply_model_events();
    assert_eq!(*state.handle.engine_dims.lock(), (30, 6));

    // A second resize inside the throttle window stays queued.
    state.resize(40, 8, window_size(40, 8));
    state.apply_model_events();
    assert_eq!(*state.handle.engine_dims.lock(), (30, 6));
    assert!(
        state
            .events
            .iter()
            .any(|event| matches!(event, TerminalInternalEvent::Resize { .. }))
    );

    // After the throttle window the deferred target applies.
    state.last_engine_resize_at = Some(Instant::now() - TERMINAL_ENGINE_RESIZE_THROTTLE);
    state.apply_model_events();
    assert_eq!(*state.handle.engine_dims.lock(), (40, 8));
    assert!(state.events.is_empty());
}
#[test]
fn resize_back_applies_even_when_published_snapshot_lags() {
    let state = TerminalModel::new_for_test(20, 4, 100);
    let handle = state.handle.clone();
    // Engine resized away while the published snapshot still shows 20x4.
    assert!(handle.resize(30, 10));
    // Resizing back must reach the engine; deduping against the lagging
    // published snapshot would leave the engine stuck at 30x10.
    assert!(handle.resize(20, 4));

    handle.publish_snapshot();
    let snapshot = handle.snapshot();
    assert_eq!(snapshot.columns, 20);
    assert_eq!(snapshot.screen_lines, 4);
}
