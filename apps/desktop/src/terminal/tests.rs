#[cfg(test)]
mod tests {
    use super::*;

    fn keystroke(key: &str) -> Keystroke {
        Keystroke {
            key: key.to_string(),
            key_char: None,
            modifiers: Modifiers::default(),
        }
    }

    fn modified_key(key: &str, shift: bool, alt: bool, control: bool, platform: bool) -> Keystroke {
        modified_key_with_function(key, shift, alt, control, platform, false)
    }

    fn modified_key_with_function(
        key: &str,
        shift: bool,
        alt: bool,
        control: bool,
        platform: bool,
        function: bool,
    ) -> Keystroke {
        Keystroke {
            key: key.to_string(),
            key_char: None,
            modifiers: Modifiers {
                shift,
                alt,
                control,
                platform,
                function,
            },
        }
    }

    fn key_char(key: &str, key_char: &str) -> Keystroke {
        Keystroke {
            key: key.to_string(),
            key_char: Some(key_char.to_string()),
            modifiers: Modifiers::default(),
        }
    }

    fn modified_key_with_char(
        key: &str,
        key_char: &str,
        shift: bool,
        alt: bool,
        control: bool,
        platform: bool,
    ) -> Keystroke {
        let mut keystroke = modified_key(key, shift, alt, control, platform);
        keystroke.key_char = Some(key_char.to_string());
        keystroke
    }

    fn normal_mode() -> TerminalInputMode {
        TerminalInputMode::default()
    }

    fn app_cursor_mode() -> TerminalInputMode {
        TerminalInputMode {
            application_cursor: true,
            ..TerminalInputMode::default()
        }
    }

    fn alternate_scroll_mode() -> TerminalInputMode {
        TerminalInputMode {
            alternate_screen: true,
            alternate_scroll: true,
            ..TerminalInputMode::default()
        }
    }

    fn bytes(keystroke: Keystroke, mode: TerminalInputMode) -> Vec<u8> {
        keystroke_to_bytes(&keystroke, mode).expect("keystroke should map to terminal bytes")
    }

    fn row_text(content: &TerminalContent, line: i32) -> String {
        content
            .cells
            .iter()
            .filter(|cell| cell.point.line == line)
            .map(|cell| cell.cell.text.as_str())
            .collect()
    }

    fn test_cell(
        fg: TerminalScreenColor,
        bg: TerminalScreenColor,
        bold: bool,
        inverse: bool,
    ) -> TerminalScreenCellSnapshot {
        TerminalScreenCellSnapshot {
            row: 0,
            col: 0,
            text: "x".to_string(),
            width: 1,
            fg,
            bg,
            bold,
            dim: false,
            italic: false,
            underline: false,
            inverse,
            hidden: false,
            strikeout: false,
        }
    }

    #[test]
    fn maps_plain_text_and_basic_control_keys() {
        assert_eq!(bytes(keystroke("enter"), normal_mode()), b"\r");
        assert_eq!(bytes(keystroke("Return"), normal_mode()), b"\r");
        assert_eq!(bytes(keystroke("kp_enter"), normal_mode()), b"\r");
        assert_eq!(bytes(keystroke("tab"), normal_mode()), b"\t");
        assert_eq!(bytes(keystroke("Tab"), normal_mode()), b"\t");
        assert_eq!(bytes(keystroke("escape"), normal_mode()), b"\x1b");
        assert_eq!(bytes(keystroke("Esc"), normal_mode()), b"\x1b");
        assert_eq!(bytes(keystroke("backspace"), normal_mode()), b"\x7f");
    }

    #[test]
    fn plain_character_without_text_input_is_not_lowercased() {
        assert!(keystroke_to_bytes(&keystroke("a"), normal_mode()).is_none());
    }

    #[test]
    fn printable_key_chars_are_committed_by_text_input() {
        assert!(keystroke_to_bytes(&key_char("a", "a"), normal_mode()).is_none());
        assert!(keystroke_to_bytes(&key_char("a", "A"), normal_mode()).is_none());
        assert!(keystroke_to_bytes(&key_char("semicolon", ";"), normal_mode()).is_none());
    }

    #[test]
    fn maps_terminal_interrupt_shortcut_to_etx() {
        assert_eq!(
            bytes(modified_key("c", false, false, true, false), normal_mode()),
            b"\x03"
        );
        assert_eq!(
            bytes(
                modified_key_with_char("c", "c", false, false, true, false),
                normal_mode()
            ),
            b"\x03"
        );
        assert_eq!(
            bytes(
                modified_key_with_char("c", "\x03", false, false, true, false),
                normal_mode()
            ),
            b"\x03"
        );
    }

    #[test]
    fn maps_copy_and_paste_shortcuts_as_ui_commands() {
        assert!(is_copy_keystroke(&modified_key(
            "C", false, false, false, true
        )));
        assert!(is_paste_keystroke(&modified_key(
            "V", false, false, false, true
        )));
        assert!(!is_copy_keystroke(&modified_key(
            "c", false, false, true, false
        )));
        assert!(!is_paste_keystroke(&modified_key(
            "v", false, false, true, false
        )));
    }

    #[test]
    fn shift_scroll_keeps_terminal_history_available_in_alternate_screen() {
        assert!(should_send_alternate_scroll(alternate_scroll_mode(), false));
        assert!(!should_send_alternate_scroll(alternate_scroll_mode(), true));
    }

    #[test]
    fn tracks_synchronized_output_across_chunks() {
        let mut depth = 0;
        let mut tail = Vec::new();

        assert_eq!(
            update_synchronized_output_state(b"\x1b[?202", &mut depth, &mut tail),
            SyncOutputUpdate::default()
        );
        assert_eq!(depth, 0);

        assert_eq!(
            update_synchronized_output_state(b"6hpartial frame", &mut depth, &mut tail),
            SyncOutputUpdate {
                entered_from_idle: true,
                exited_to_idle: false,
                should_notify: false,
            }
        );
        assert_eq!(depth, 1);

        assert_eq!(
            update_synchronized_output_state(b"done\x1b[?2026l", &mut depth, &mut tail),
            SyncOutputUpdate {
                entered_from_idle: false,
                exited_to_idle: true,
                should_notify: true,
            }
        );
        assert_eq!(depth, 0);
    }

    #[test]
    fn reports_notify_when_synchronized_output_ends() {
        let mut depth = 0;
        let mut tail = Vec::new();

        assert_eq!(
            update_synchronized_output_state(b"\x1b[?2026hframe\x1b[?2026l", &mut depth, &mut tail),
            SyncOutputUpdate {
                entered_from_idle: true,
                exited_to_idle: true,
                should_notify: true,
            }
        );
        assert_eq!(depth, 0);
    }

    #[test]
    fn tracks_nested_synchronized_output() {
        let mut depth = 0;
        let mut tail = Vec::new();

        assert_eq!(
            update_synchronized_output_state(
                b"\x1b[?2026houter\x1b[?2026hinner",
                &mut depth,
                &mut tail,
            ),
            SyncOutputUpdate {
                entered_from_idle: true,
                exited_to_idle: false,
                should_notify: false,
            }
        );
        assert_eq!(depth, 2);

        assert_eq!(
            update_synchronized_output_state(b"\x1b[?2026l", &mut depth, &mut tail),
            SyncOutputUpdate {
                entered_from_idle: false,
                exited_to_idle: false,
                should_notify: true,
            }
        );
        assert_eq!(depth, 1);

        assert_eq!(
            update_synchronized_output_state(b"\x1b[?2026l", &mut depth, &mut tail),
            SyncOutputUpdate {
                entered_from_idle: false,
                exited_to_idle: true,
                should_notify: true,
            }
        );
        assert_eq!(depth, 0);
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
    fn protocol_flags_detect_cursor_and_color_requests() {
        assert_eq!(
            terminal_protocol_flags(b"\x1b[?25lhello\x1b[?25h\x1b]10;?\x07\x1b]11;?\x07"),
            TerminalProtocolFlags {
                show_cursor: true,
                hide_cursor: true,
                osc_10_request: true,
                osc_11_request: true,
            }
        );
    }

    #[test]
    fn color_scheme_protocol_tracks_subscription_and_queries_across_chunks() {
        let mut state = TerminalColorSchemeState::default();

        assert_eq!(
            update_terminal_color_scheme_state(b"\x1b[?203", &mut state),
            TerminalColorSchemeUpdate::default()
        );
        assert!(!state.updates_enabled);

        assert_eq!(
            update_terminal_color_scheme_state(b"1h\x1b[?996n", &mut state),
            TerminalColorSchemeUpdate {
                enabled: true,
                disabled: false,
                query_count: 1,
                ..TerminalColorSchemeUpdate::default()
            }
        );
        assert!(state.updates_enabled);

        assert_eq!(
            update_terminal_color_scheme_state(b"\x1b[?2031l", &mut state),
            TerminalColorSchemeUpdate {
                enabled: false,
                disabled: true,
                query_count: 0,
                ..TerminalColorSchemeUpdate::default()
            }
        );
        assert!(!state.updates_enabled);
    }

    #[test]
    fn osc_color_queries_tracked_across_chunks() {
        let mut state = TerminalColorSchemeState::default();

        assert_eq!(
            update_terminal_color_scheme_state(b"\x1b]1", &mut state),
            TerminalColorSchemeUpdate::default()
        );
        assert_eq!(
            update_terminal_color_scheme_state(b"1;?\x07\x1b]10;?\x1b\\", &mut state),
            TerminalColorSchemeUpdate {
                osc_foreground_queries: 1,
                osc_background_queries: 1,
                ..TerminalColorSchemeUpdate::default()
            }
        );
    }

    #[test]
    fn osc_color_queries_reply_with_palette_colors() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.colors = ColorPalette::builder()
            .background(0x1e, 0x22, 0x2b)
            .foreground(0xee, 0xee, 0xee)
            .build();

        state.respond_to_osc_color_queries(&TerminalColorSchemeUpdate {
            osc_foreground_queries: 1,
            osc_background_queries: 1,
            ..TerminalColorSchemeUpdate::default()
        });

        let written = String::from_utf8(state.written_bytes_for_test()).unwrap();
        assert!(written.contains("\x1b]10;rgb:eeee/eeee/eeee\x07"));
        assert!(written.contains("\x1b]11;rgb:1e1e/2222/2b2b\x07"));
    }

    #[test]
    fn color_scheme_report_matches_xterm_codes() {
        assert_eq!(
            terminal_color_scheme_report_for(ColorPalette::default().is_dark()),
            b"\x1b[?997;1n"
        );

        let light = ColorPalette::builder()
            .background(0xee, 0xee, 0xee)
            .foreground(0x11, 0x11, 0x11)
            .build();
        assert_eq!(
            terminal_color_scheme_report_for(light.is_dark()),
            b"\x1b[?997;2n"
        );
    }

    #[test]
    fn color_scheme_queries_write_current_scheme() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.colors = ColorPalette::builder()
            .background(0xee, 0xee, 0xee)
            .foreground(0x11, 0x11, 0x11)
            .build();

        state.respond_to_color_scheme_queries(2);

        assert_eq!(state.written_bytes_for_test(), b"\x1b[?997;2n\x1b[?997;2n");
    }

    #[test]
    fn color_scheme_update_reports_theme_change_when_subscribed() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.color_scheme_state.updates_enabled = true;

        state.update_colors(
            ColorPalette::builder()
                .background(0xee, 0xee, 0xee)
                .foreground(0x11, 0x11, 0x11)
                .build(),
        );
        assert_eq!(state.written_bytes_for_test(), b"\x1b[?997;2n");

        state.update_colors(
            ColorPalette::builder()
                .background(0xdd, 0xdd, 0xdd)
                .foreground(0x22, 0x22, 0x22)
                .build(),
        );
        assert_eq!(state.written_bytes_for_test(), b"\x1b[?997;2n");

        state.update_colors(ColorPalette::default());
        assert_eq!(state.written_bytes_for_test(), b"\x1b[?997;2n\x1b[?997;1n");
    }

    #[test]
    fn maps_app_cursor_mode() {
        assert_eq!(bytes(keystroke("up"), normal_mode()), b"\x1b[A");
        assert_eq!(bytes(keystroke("down"), normal_mode()), b"\x1b[B");
        assert_eq!(bytes(keystroke("right"), normal_mode()), b"\x1b[C");
        assert_eq!(bytes(keystroke("left"), normal_mode()), b"\x1b[D");
        assert_eq!(bytes(keystroke("arrow_up"), normal_mode()), b"\x1b[A");
        assert_eq!(bytes(keystroke("down_arrow"), normal_mode()), b"\x1b[B");
        assert_eq!(bytes(keystroke("home"), normal_mode()), b"\x1b[H");
        assert_eq!(bytes(keystroke("end"), normal_mode()), b"\x1b[F");

        assert_eq!(bytes(keystroke("up"), app_cursor_mode()), b"\x1bOA");
        assert_eq!(bytes(keystroke("down"), app_cursor_mode()), b"\x1bOB");
        assert_eq!(bytes(keystroke("right"), app_cursor_mode()), b"\x1bOC");
        assert_eq!(bytes(keystroke("left"), app_cursor_mode()), b"\x1bOD");
        assert_eq!(bytes(keystroke("home"), app_cursor_mode()), b"\x1bOH");
        assert_eq!(bytes(keystroke("end"), app_cursor_mode()), b"\x1bOF");
    }

    #[test]
    fn maps_modified_navigation_and_function_keys() {
        assert_eq!(
            bytes(
                modified_key("up", true, false, false, false),
                normal_mode()
            ),
            b"\x1b[1;2A"
        );
        assert_eq!(
            bytes(
                modified_key("left", false, true, true, false),
                normal_mode()
            ),
            b"\x1b[1;7D"
        );
        assert_eq!(
            bytes(
                modified_key("home", true, false, false, false),
                normal_mode()
            ),
            b"\x1b[1;2H"
        );
        assert_eq!(bytes(keystroke("f12"), normal_mode()), b"\x1b[24~");
        assert_eq!(bytes(keystroke("f20"), normal_mode()), b"\x1b[34~");
        assert_eq!(
            bytes(
                modified_key("f5", false, false, true, false),
                normal_mode()
            ),
            b"\x1b[15;5~"
        );
        assert_eq!(
            bytes(
                modified_key("delete", true, false, false, false),
                normal_mode()
            ),
            b"\x1b[3;2~"
        );
    }

    #[test]
    fn maps_macos_terminal_navigation_shortcuts() {
        assert_eq!(
            bytes(
                modified_key("left", false, true, false, false),
                normal_mode()
            ),
            b"\x1bb"
        );
        assert_eq!(
            bytes(
                modified_key("right", false, true, false, false),
                normal_mode()
            ),
            b"\x1bf"
        );
        assert_eq!(
            bytes(
                modified_key("left", false, false, false, true),
                normal_mode()
            ),
            b"\x01"
        );
        assert_eq!(
            bytes(
                modified_key("right", false, false, false, true),
                normal_mode()
            ),
            b"\x05"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("left", false, false, false, true, true),
                normal_mode()
            ),
            b"\x01"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("right", false, false, false, true, true),
                normal_mode()
            ),
            b"\x05"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("left", false, true, false, false, true),
                normal_mode()
            ),
            b"\x1bb"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("right", false, true, false, false, true),
                normal_mode()
            ),
            b"\x1bf"
        );
        assert_eq!(
            bytes(
                modified_key("home", false, false, false, true),
                normal_mode()
            ),
            b"\x01"
        );
        assert_eq!(
            bytes(
                modified_key("end", false, false, false, true),
                normal_mode()
            ),
            b"\x05"
        );
        assert_eq!(
            bytes(
                modified_key("delete", false, true, false, false),
                normal_mode()
            ),
            b"\x1bd"
        );
        assert_eq!(
            bytes(
                modified_key("backspace", false, false, false, true),
                normal_mode()
            ),
            b"\x15"
        );
        assert_eq!(
            bytes(
                modified_key("back", false, false, false, true),
                normal_mode()
            ),
            b"\x15"
        );
        assert_eq!(
            bytes(
                modified_key("delete", false, false, false, true),
                normal_mode()
            ),
            b"\x0b"
        );
    }

    #[test]
    fn keeps_macos_app_shortcuts_out_of_terminal_input() {
        for key in ["q", "h", "m", "w", "tab", "`"] {
            assert!(
                keystroke_to_bytes(&modified_key(key, false, false, false, true), normal_mode())
                    .is_none(),
                "Cmd+{key} should remain an app shortcut"
            );
        }
        assert!(
            keystroke_to_bytes(&modified_key("h", false, true, false, true), normal_mode())
                .is_none()
        );
        assert!(
            keystroke_to_bytes(&modified_key("m", false, true, false, true), normal_mode())
                .is_none()
        );
        assert!(
            keystroke_to_bytes(&modified_key("tab", true, false, false, true), normal_mode())
                .is_none()
        );
    }

    #[test]
    fn preserves_control_q_for_terminal_flow_control() {
        assert_eq!(
            bytes(modified_key("q", false, false, true, false), normal_mode()),
            b"\x11"
        );
        assert_eq!(
            bytes(modified_key("Q", true, false, true, false), normal_mode()),
            b"\x11"
        );
    }

    #[test]
    fn maps_ctrl_alt_and_shift_enter_sequences() {
        assert_eq!(
            bytes(modified_key("a", false, false, true, false), normal_mode()),
            b"\x01"
        );
        assert_eq!(
            bytes(modified_key("C", true, false, true, false), normal_mode()),
            b"\x03"
        );
        assert_eq!(
            bytes(modified_key("[", false, false, true, false), normal_mode()),
            b"\x1b"
        );
        assert_eq!(
            bytes(
                modified_key("enter", true, false, false, false),
                normal_mode()
            ),
            b"\n"
        );
        assert_eq!(
            bytes(
                modified_key("Tab", true, false, false, false),
                normal_mode()
            ),
            b"\x1b[Z"
        );
        assert_eq!(
            bytes(
                modified_key("BackTab", true, false, false, false),
                normal_mode()
            ),
            b"\x1b[Z"
        );
        assert_eq!(
            bytes(
                modified_key("enter", false, true, false, false),
                normal_mode()
            ),
            b"\x1b\r"
        );
        assert_eq!(
            bytes(modified_key("x", false, true, false, false), normal_mode()),
            b"\x1bx"
        );
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
        let published =
            state.publish_completed_snapshot(stale_empty, TERMINAL_SNAPSHOT_PUBLISH_SLOW);
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
    fn absolute_scroll_targets_do_not_compound_when_published_offset_lags() {
        let mut state = TerminalModel::new_for_test(20, 4, 100);
        state.process_output_bytes_for_test(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");

        // Two drag frames target the same offset while no publish has
        // landed in between; the engine must end exactly at the target.
        state.scroll_to_display_offset(2);
        state.publish_snapshot_now();
        state.scroll_to_display_offset(2);
        let content = state.publish_snapshot_now();

        assert_eq!(content.display_offset, 2);
    }

    #[test]
    fn input_viewport_republishes_when_publish_is_in_flight() {
        let mut state = TerminalModel::new_for_test(20, 4, 100);
        state.process_output_bytes_for_test(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix");
        state.handle.publish_snapshot();
        state.snapshot_dirty = false;

        // A publish is in flight: the published offset can't be trusted.
        state.snapshot_publish_pending = true;
        assert!(state.prepare_input_viewport_snapshot());
        assert!(state.snapshot_dirty);
    }

    #[test]
    fn paste_uses_live_bracketed_paste_mode_before_snapshot_publish() {
        let mut state = TerminalModel::new_for_test(20, 4, 100);
        // Enable bracketed paste in the engine without publishing the
        // snapshot: the published input_mode is still stale.
        state.process_output_bytes_for_test(b"\x1b[?2004h");
        assert!(!state.handle.input_mode().bracketed_paste);

        state.paste_text("line1\nline2");

        let written = state.written_bytes_for_test();
        let text = String::from_utf8_lossy(&written);
        assert!(
            text.starts_with("\x1b[200~") && text.ends_with("\x1b[201~"),
            "paste was not bracketed: {text:?}"
        );
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
        state.last_engine_resize_at =
            Some(Instant::now() - TERMINAL_ENGINE_RESIZE_THROTTLE);
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

    #[test]
    fn display_cursor_tracks_ghostty_viewport_coordinates() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
        state.handle.publish_snapshot();
        assert!(state.scroll_display(2));
        let snapshot = state.sync_for_test();

        let display_cursor = snapshot.display_cursor();

        assert_eq!(snapshot.display_offset, 2);
        assert_eq!(
            display_cursor,
            DisplayCursor {
                row: snapshot.cursor.row as i32,
                col: snapshot.cursor.col,
            }
        );
    }

    #[test]
    fn local_visible_rows_map_bottom_slice_without_screen_gaps() {
        let mut state = TerminalModel::new_for_test(10, 6, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix");
        state.handle.publish_snapshot();

        let snapshot = state.sync_for_test().with_visible_row_shift(4);

        assert_eq!(snapshot.screen_lines, 6);
        assert_eq!(snapshot.visible_rows(), 4);
        assert_eq!(snapshot.visible_row_shift, 2);
        assert_eq!(snapshot.display_row_for_line(0), None);
        assert_eq!(snapshot.display_row_for_line(1), None);
        assert_eq!(snapshot.display_row_for_line(2), Some(0));
        assert_eq!(snapshot.display_row_for_line(3), Some(1));
        assert_eq!(snapshot.display_row_for_line(4), Some(2));
        assert_eq!(snapshot.display_row_for_line(5), Some(3));
        assert_eq!(snapshot.line_for_display_row(0), 2);
        assert_eq!(snapshot.line_for_display_row(3), 5);
    }

    #[test]
    fn scroll_to_bottom_restores_input_viewport() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
        state.handle.publish_snapshot();
        assert!(state.scroll_display(2));
        let scrolled = state.sync_for_test();
        assert_eq!(scrolled.display_offset, 2);
        assert!(!scrolled.scrolled_to_bottom);

        state.prepare_input_viewport_for_test();
        let bottom = state.live_snapshot();

        assert_eq!(bottom.display_offset, 0);
        assert!(bottom.scrolled_to_bottom);
    }

    #[test]
    fn input_viewport_preparation_discards_pending_history_scroll() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
        state.handle.publish_snapshot();
        assert!(state.scroll_display(2));
        assert_eq!(state.sync_for_test().display_offset, 2);

        assert!(state.scroll_display(1));
        state.prepare_input_viewport_for_test();
        let snapshot = state.sync_for_test();

        assert_eq!(snapshot.display_offset, 0);
        assert!(snapshot.scrolled_to_bottom);
    }

    #[test]
    fn input_viewport_preparation_keeps_bottom_stable_across_drift_events() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight");
        state.handle.publish_snapshot();

        for lines in [2, -1, 3, 1] {
            assert!(state.scroll_display(lines));
        }
        state.prepare_input_viewport_for_test();
        let snapshot = state.sync_for_test();

        assert_eq!(snapshot.display_offset, 0);
        assert!(snapshot.scrolled_to_bottom);
    }

    #[test]
    fn keyboard_input_suppresses_residual_precise_scroll_from_same_gesture() {
        let mut state = TerminalScrollInputState {
            pending_lines: 3,
            pending_pixels: 12.0,
            frame_pending: false,
            suppress_residual_precise_scroll: false,
        };
        state.prepare_for_keyboard_input();

        assert_eq!(state.pending_lines, 0);
        assert_eq!(state.pending_pixels, 0.0);
        assert!(state.should_suppress_residual_scroll(&ScrollWheelEvent {
            delta: gpui::ScrollDelta::Pixels(Point {
                x: px(0.0),
                y: px(8.0),
            }),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        }));
        assert_eq!(state.pending_pixels, 0.0);
    }

    #[test]
    fn new_scroll_gesture_after_keyboard_input_is_not_suppressed() {
        let mut state = TerminalScrollInputState::default();
        state.prepare_for_keyboard_input();

        assert!(!state.should_suppress_residual_scroll(&ScrollWheelEvent {
            delta: gpui::ScrollDelta::Pixels(Point {
                x: px(0.0),
                y: px(8.0),
            }),
            touch_phase: TouchPhase::Started,
            ..Default::default()
        }));
        assert!(!state.should_suppress_residual_scroll(&ScrollWheelEvent {
            delta: gpui::ScrollDelta::Pixels(Point {
                x: px(0.0),
                y: px(8.0),
            }),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        }));
    }

    #[test]
    fn keyboard_input_does_not_suppress_line_wheel_scroll() {
        let mut state = TerminalScrollInputState::default();
        state.prepare_for_keyboard_input();

        assert!(!state.should_suppress_residual_scroll(&ScrollWheelEvent {
            delta: gpui::ScrollDelta::Lines(Point { x: 0.0, y: 1.0 }),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        }));
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

    #[test]
    fn ime_cursor_bounds_follow_current_viewport_after_history_scroll() {
        let mut layout = TerminalLayoutMetrics::default();
        layout.update(
            Bounds {
                origin: Point {
                    x: px(10.0),
                    y: px(20.0),
                },
                size: Size {
                    width: px(100.0),
                    height: px(80.0),
                },
            },
            Edges {
                top: px(2.0),
                right: px(3.0),
                bottom: px(4.0),
                left: px(5.0),
            },
            px(10.0),
            px(20.0),
            10,
            4,
        );

        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
        state.handle.publish_snapshot();

        assert!(state.scroll_display(2));
        let scrolled = state.sync_for_test();
        assert_eq!(scrolled.display_offset, 2);
        assert!(state.current_ime_cursor_bounds(&layout).is_none());

        state.prepare_input_viewport_for_test();
        let bottom = state.sync_for_test();
        let bounds = state.current_ime_cursor_bounds(&layout).unwrap();
        let row = bottom.display_cursor().row;

        assert_eq!(bottom.display_offset, 0);
        assert!(bottom.scrolled_to_bottom);
        assert!(row >= 0);
        assert_eq!(
            bounds.origin.x,
            px(15.0) + px(10.0) * bottom.cursor.col as f32
        );
        assert_eq!(bounds.origin.y, px(22.0) + px(20.0) * row as f32);
        assert_eq!(bounds.size.width, px(10.0));
        assert_eq!(bounds.size.height, px(20.0));
    }

    #[test]
    fn ime_bounds_for_range_offsets_from_current_cursor_cell() {
        let mut layout = TerminalLayoutMetrics::default();
        layout.update(
            Bounds {
                origin: Point {
                    x: px(10.0),
                    y: px(20.0),
                },
                size: Size {
                    width: px(100.0),
                    height: px(80.0),
                },
            },
            Edges {
                top: px(2.0),
                right: px(0.0),
                bottom: px(0.0),
                left: px(5.0),
            },
            px(10.0),
            px(20.0),
            10,
            4,
        );
        let cursor = Bounds {
            origin: Point {
                x: px(25.0),
                y: px(42.0),
            },
            size: Size {
                width: px(10.0),
                height: px(20.0),
            },
        };

        let bounds = ime_bounds_for_range(Some(cursor), &layout, 2..4).unwrap();

        assert_eq!(bounds.origin.x, px(45.0));
        assert_eq!(bounds.origin.y, px(42.0));
    }

    #[test]
    fn pending_session_reports_initial_layout_once_without_resize_claim() {
        let (binding, rx) = TerminalSessionBinding::pending(TerminalPtyConfig::default());

        let initial = binding.record_layout(120, 36);
        assert!(initial.initialized);
        assert!(!initial.resized);

        assert_eq!(
            rx.recv_timeout(Duration::from_millis(10)).unwrap(),
            (120, 36)
        );
        let resize = binding.record_layout(121, 37);
        assert!(!resize.initialized);
        assert!(resize.resized);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn terminal_grid_dimension_tolerates_float_precision() {
        for cell in [8.1_f32, 9.7, 14.1, 20.1] {
            for count in 20..=200 {
                let available = count as f32 * cell;
                assert_eq!(terminal_grid_dimension(available, cell, 20), count);
            }
        }
        assert_eq!(terminal_grid_dimension(1.0, 8.0, 20), 20);
        assert_eq!(terminal_grid_dimension(1.0, 18.0, 1), 1);
    }

    #[test]
    fn detects_plain_terminal_urls_at_cell() {
        let mut state = TerminalModel::new_for_test(80, 4, 100);
        state.process_bytes(b"open https://example.com/path?x=1.\r\n");
        state.handle.publish_snapshot();
        let snapshot = state.handle.snapshot();

        let link = terminal_link_at_cell(&snapshot, TerminalCellPoint { row: 0, col: 12 })
            .expect("url under cursor");

        assert_eq!(link.url, "https://example.com/path?x=1");
        assert_eq!(link.line, 0);
        assert_eq!(link.range, 5..33);
        assert!(terminal_link_at_cell(&snapshot, TerminalCellPoint { row: 0, col: 2 }).is_none());
    }

    #[test]
    fn plain_url_detection_uses_terminal_columns() {
        let row_text = vec![
            (0, '中'),
            (2, ' '),
            (3, 'h'),
            (4, 't'),
            (5, 't'),
            (6, 'p'),
            (7, 's'),
            (8, ':'),
            (9, '/'),
            (10, '/'),
            (11, 'e'),
            (12, 'x'),
            (13, '.'),
            (14, 'c'),
            (15, 'o'),
            (16, 'm'),
            (17, ')'),
        ];

        let (url, range) = terminal_plain_url_at(&row_text, 12).expect("url under cursor");

        assert_eq!(url, "https://ex.com");
        assert_eq!(range, 3..17);
    }

    #[test]
    fn plain_url_detection_matches_xterm_style_boundaries() {
        let row_text: Vec<(usize, char)> =
            "(HTTPS://example.com/a?q=1),".chars().enumerate().collect();

        let (url, range) = terminal_plain_url_at(&row_text, 4).expect("url under cursor");

        assert_eq!(url, "HTTPS://example.com/a?q=1");
        assert_eq!(range, 1..26);
        assert!(terminal_plain_url_at(&row_text, 0).is_none());
        assert!(terminal_plain_url_at(&row_text, 26).is_none());
    }

    #[test]
    fn plain_url_detection_supports_file_urls() {
        let row_text: Vec<(usize, char)> = "open file:///tmp/codux-log.txt."
            .chars()
            .enumerate()
            .collect();

        let (url, range) = terminal_plain_url_at(&row_text, 12).expect("file url under cursor");

        assert_eq!(url, "file:///tmp/codux-log.txt");
        assert_eq!(range, 5..30);
    }

    #[test]
    fn inverse_cells_swap_foreground_and_background_colors() {
        let renderer = TerminalRenderer::new(
            default_terminal_font_family().to_string(),
            px(14.0),
            DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
            ColorPalette::default(),
        );
        let normal_cell = test_cell(
            TerminalScreenColor::Default,
            TerminalScreenColor::Default,
            false,
            false,
        );
        let inverse_cell = test_cell(
            TerminalScreenColor::Default,
            TerminalScreenColor::Default,
            false,
            true,
        );

        let normal = renderer.cell_render_colors(&normal_cell);
        let inverse = renderer.cell_render_colors(&inverse_cell);

        assert_eq!(inverse.0, normal.1);
        assert_eq!(inverse.1, normal.0);
    }

    #[test]
    fn default_terminal_line_height_matches_renderer_cell_height() {
        let config = terminal_config();
        assert_eq!(
            config.line_height_multiplier,
            DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER
        );

        let renderer = TerminalRenderer::new(
            config.font_family,
            config.font_size,
            config.line_height_multiplier,
            config.colors,
        );
        assert_eq!(
            renderer.cell_height,
            config.font_size * DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER
        );
        assert!(config.paste_images_as_paths);
    }

    #[test]
    fn terminal_clipboard_image_payload_detection_filters_data_and_html() {
        assert!(clipboard_text_looks_like_image_payload(
            "data:image/png;base64,abc"
        ));
        assert!(clipboard_text_looks_like_image_payload(
            "<img src=\"data:image/png;base64,abc\">"
        ));
        assert!(!clipboard_text_looks_like_image_payload("/tmp/image.png"));
    }

    #[test]
    fn terminal_path_input_quotes_spaces() {
        assert_eq!(
            terminal_path_input(Path::new("/tmp/codux image.png")),
            "'/tmp/codux image.png'"
        );
        assert_eq!(
            terminal_path_input(Path::new("/tmp/codux-image.png")),
            "/tmp/codux-image.png"
        );
        assert_eq!(terminal_clipboard_image_extension(ImageFormat::Jpeg), "jpg");
    }

    #[test]
    fn terminal_paths_input_joins_quoted_paths_with_trailing_space() {
        let paths = vec![
            PathBuf::from("/tmp/codux-image.png"),
            PathBuf::from("/tmp/codux image.png"),
        ];

        assert_eq!(
            terminal_paths_input(&paths),
            Some("/tmp/codux-image.png '/tmp/codux image.png' ".to_string())
        );
        assert_eq!(terminal_paths_input(&[]), None);
    }

    #[test]
    fn bold_ansi_foreground_uses_bright_color() {
        let renderer = TerminalRenderer::new(
            default_terminal_font_family().to_string(),
            px(14.0),
            DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
            ColorPalette::default(),
        );
        let cell = test_cell(
            TerminalScreenColor::Indexed { index: 4 },
            TerminalScreenColor::Default,
            true,
            false,
        );

        let (fg, _) = renderer.cell_render_colors(&cell);
        assert_eq!(
            fg,
            renderer
                .palette
                .resolve_fg(&TerminalScreenColor::Indexed { index: 12 }, false, false)
        );
    }

    #[test]
    fn default_colors_use_current_palette_values() {
        let palette = ColorPalette::builder()
            .background(0xee, 0xee, 0xee)
            .foreground(0x11, 0x11, 0x11)
            .cursor(0x22, 0x22, 0x22)
            .build();

        assert_eq!(
            palette.resolve_bg(&TerminalScreenColor::Default),
            palette.background()
        );
        assert_eq!(
            palette.resolve_fg(&TerminalScreenColor::Default, false, false),
            palette.foreground()
        );
    }

    #[test]
    fn inverse_bold_only_brightens_final_foreground() {
        let renderer = TerminalRenderer::new(
            default_terminal_font_family().to_string(),
            px(14.0),
            DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
            ColorPalette::default(),
        );
        let cell = test_cell(
            TerminalScreenColor::Indexed { index: 4 },
            TerminalScreenColor::Indexed { index: 1 },
            true,
            true,
        );

        let (fg, bg) = renderer.cell_render_colors(&cell);
        assert_eq!(
            fg,
            renderer
                .palette
                .resolve_fg(&TerminalScreenColor::Indexed { index: 9 }, false, false)
        );
        assert_eq!(
            bg,
            renderer
                .palette
                .resolve_fg(&TerminalScreenColor::Indexed { index: 4 }, false, false)
        );
    }

    #[test]
    fn palette_resolves_configured_colors() {
        let palette = ColorPalette::builder()
            .background(0x28, 0x2A, 0x36)
            .foreground(0xF8, 0xF8, 0xF2)
            .cursor(0xF8, 0xF8, 0xF2)
            .selection(0x44, 0x47, 0x5A)
            .black(0x21, 0x22, 0x2C)
            .bright_black(0x62, 0x72, 0xA4)
            .build();

        assert_eq!(
            hsla_to_rgb(palette.resolve_bg(&TerminalScreenColor::Default)),
            TerminalRgb {
                r: 0x28,
                g: 0x2A,
                b: 0x36
            }
        );
        assert_eq!(
            hsla_to_rgb(palette.resolve_fg(&TerminalScreenColor::Default, false, false)),
            TerminalRgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            }
        );
        assert_eq!(
            hsla_to_rgb(palette.resolve_fg(
                &TerminalScreenColor::Indexed { index: 0 },
                false,
                false
            )),
            TerminalRgb {
                r: 0x21,
                g: 0x22,
                b: 0x2C
            }
        );
        assert_eq!(
            hsla_to_rgb(palette.resolve_fg(
                &TerminalScreenColor::Indexed { index: 8 },
                false,
                false
            )),
            TerminalRgb {
                r: 0x62,
                g: 0x72,
                b: 0xA4
            }
        );
        assert_eq!(
            hsla_to_rgb(palette.resolve_fg(&TerminalScreenColor::Default, true, false)),
            TerminalRgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            }
        );
        assert_eq!(
            palette.resolve_fg(&TerminalScreenColor::Default, false, true),
            dim_color(rgb_to_hsla(TerminalRgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            }))
        );
        assert_eq!(
            hsla_to_rgb(palette.resolve_fg(
                &TerminalScreenColor::Indexed { index: 255 },
                false,
                false
            )),
            TerminalRgb {
                r: 0xEE,
                g: 0xEE,
                b: 0xEE
            }
        );
    }

    #[test]
    fn pending_terminal_binding_matches_requested_config_before_attach() {
        let config = terminal_pty_config_with_view(
            TerminalPtyConfig {
                cwd: Some("/tmp/project".to_string()),
                project_id: Some("project-1".to_string()),
                terminal_id: Some("terminal-1".to_string()),
                session_key: Some("gpui:project-1:terminal-1".to_string()),
                ..Default::default()
            },
            &terminal_config(),
        );

        let (binding, _initial_layout_rx) = TerminalSessionBinding::pending(config.clone());

        assert!(binding.matches_pty_config(&config));

        let mut different_terminal = config;
        different_terminal.terminal_id = Some("terminal-2".to_string());
        assert!(!binding.matches_pty_config(&different_terminal));
    }
}
