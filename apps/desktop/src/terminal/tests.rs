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

    fn bytes(keystroke: Keystroke, mode: TermMode) -> Vec<u8> {
        keystroke_to_bytes(&keystroke, mode).expect("keystroke should map to terminal bytes")
    }

    #[test]
    fn maps_plain_text_and_basic_control_keys() {
        assert_eq!(bytes(keystroke("enter"), TermMode::NONE), b"\r");
        assert_eq!(bytes(keystroke("Return"), TermMode::NONE), b"\r");
        assert_eq!(bytes(keystroke("kp_enter"), TermMode::NONE), b"\r");
        assert_eq!(bytes(keystroke("tab"), TermMode::NONE), b"\t");
        assert_eq!(bytes(keystroke("Tab"), TermMode::NONE), b"\t");
        assert_eq!(bytes(keystroke("escape"), TermMode::NONE), b"\x1b");
        assert_eq!(bytes(keystroke("Esc"), TermMode::NONE), b"\x1b");
        assert_eq!(bytes(keystroke("backspace"), TermMode::NONE), b"\x7f");
    }

    #[test]
    fn plain_character_without_text_input_is_not_lowercased() {
        assert!(keystroke_to_bytes(&keystroke("a"), TermMode::NONE).is_none());
    }

    #[test]
    fn printable_key_chars_are_committed_by_text_input() {
        assert!(keystroke_to_bytes(&key_char("a", "a"), TermMode::NONE).is_none());
        assert!(keystroke_to_bytes(&key_char("a", "A"), TermMode::NONE).is_none());
        assert!(keystroke_to_bytes(&key_char("semicolon", ";"), TermMode::NONE).is_none());
    }

    #[test]
    fn maps_terminal_interrupt_shortcut_to_etx() {
        assert_eq!(
            bytes(modified_key("c", false, false, true, false), TermMode::NONE),
            b"\x03"
        );
        assert_eq!(
            bytes(
                modified_key_with_char("c", "c", false, false, true, false),
                TermMode::NONE
            ),
            b"\x03"
        );
        assert_eq!(
            bytes(
                modified_key_with_char("c", "\x03", false, false, true, false),
                TermMode::NONE
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
        let mode = TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL;
        assert!(should_send_alternate_scroll(mode, false));
        assert!(!should_send_alternate_scroll(mode, true));
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
        let paint_text: String = paint_snapshot
            .cells
            .iter()
            .filter(|cell| cell.point.line.0 == 0)
            .map(|cell| cell.c)
            .collect();
        assert!(!paint_text.contains("during"));

        let live = state.live_snapshot();

        let live_text: String = live
            .cells
            .iter()
            .filter(|cell| cell.point.line.0 == 0)
            .map(|cell| cell.c)
            .collect();

        assert!(live_text.contains("during"));

        state.handle.publish_snapshot();
        let published = state.handle.snapshot();
        let published_text: String = published
            .cells
            .iter()
            .filter(|cell| cell.point.line.0 == 0)
            .map(|cell| cell.c)
            .collect();

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
            }
        );
        assert!(state.updates_enabled);

        assert_eq!(
            update_terminal_color_scheme_state(b"\x1b[?2031l", &mut state),
            TerminalColorSchemeUpdate {
                enabled: false,
                disabled: true,
                query_count: 0,
            }
        );
        assert!(!state.updates_enabled);
    }

    #[test]
    fn color_scheme_report_matches_xterm_codes() {
        assert_eq!(
            terminal_color_scheme_report(&ColorPalette::default()),
            b"\x1b[?997;1n"
        );

        let light = ColorPalette::builder()
            .background(0xee, 0xee, 0xee)
            .foreground(0x11, 0x11, 0x11)
            .build();
        assert_eq!(terminal_color_scheme_report(&light), b"\x1b[?997;2n");
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
        assert_eq!(bytes(keystroke("up"), TermMode::NONE), b"\x1b[A");
        assert_eq!(bytes(keystroke("down"), TermMode::NONE), b"\x1b[B");
        assert_eq!(bytes(keystroke("right"), TermMode::NONE), b"\x1b[C");
        assert_eq!(bytes(keystroke("left"), TermMode::NONE), b"\x1b[D");
        assert_eq!(bytes(keystroke("arrow_up"), TermMode::NONE), b"\x1b[A");
        assert_eq!(bytes(keystroke("down_arrow"), TermMode::NONE), b"\x1b[B");
        assert_eq!(bytes(keystroke("home"), TermMode::NONE), b"\x1b[H");
        assert_eq!(bytes(keystroke("end"), TermMode::NONE), b"\x1b[F");

        assert_eq!(bytes(keystroke("up"), TermMode::APP_CURSOR), b"\x1bOA");
        assert_eq!(bytes(keystroke("down"), TermMode::APP_CURSOR), b"\x1bOB");
        assert_eq!(bytes(keystroke("right"), TermMode::APP_CURSOR), b"\x1bOC");
        assert_eq!(bytes(keystroke("left"), TermMode::APP_CURSOR), b"\x1bOD");
        assert_eq!(bytes(keystroke("home"), TermMode::APP_CURSOR), b"\x1bOH");
        assert_eq!(bytes(keystroke("end"), TermMode::APP_CURSOR), b"\x1bOF");
    }

    #[test]
    fn maps_modified_navigation_and_function_keys() {
        assert_eq!(
            bytes(
                modified_key("up", true, false, false, false),
                TermMode::NONE
            ),
            b"\x1b[1;2A"
        );
        assert_eq!(
            bytes(
                modified_key("left", false, true, true, false),
                TermMode::NONE
            ),
            b"\x1b[1;7D"
        );
        assert_eq!(
            bytes(
                modified_key("home", true, false, false, false),
                TermMode::NONE
            ),
            b"\x1b[1;2H"
        );
        assert_eq!(bytes(keystroke("f12"), TermMode::NONE), b"\x1b[24~");
        assert_eq!(bytes(keystroke("f20"), TermMode::NONE), b"\x1b[34~");
        assert_eq!(
            bytes(
                modified_key("f5", false, false, true, false),
                TermMode::NONE
            ),
            b"\x1b[15;5~"
        );
        assert_eq!(
            bytes(
                modified_key("delete", true, false, false, false),
                TermMode::NONE
            ),
            b"\x1b[3;2~"
        );
    }

    #[test]
    fn maps_macos_terminal_navigation_shortcuts() {
        assert_eq!(
            bytes(
                modified_key("left", false, true, false, false),
                TermMode::NONE
            ),
            b"\x1bb"
        );
        assert_eq!(
            bytes(
                modified_key("right", false, true, false, false),
                TermMode::NONE
            ),
            b"\x1bf"
        );
        assert_eq!(
            bytes(
                modified_key("left", false, false, false, true),
                TermMode::NONE
            ),
            b"\x01"
        );
        assert_eq!(
            bytes(
                modified_key("right", false, false, false, true),
                TermMode::NONE
            ),
            b"\x05"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("left", false, false, false, true, true),
                TermMode::NONE
            ),
            b"\x01"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("right", false, false, false, true, true),
                TermMode::NONE
            ),
            b"\x05"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("left", false, true, false, false, true),
                TermMode::NONE
            ),
            b"\x1bb"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("right", false, true, false, false, true),
                TermMode::NONE
            ),
            b"\x1bf"
        );
        assert_eq!(
            bytes(
                modified_key("home", false, false, false, true),
                TermMode::NONE
            ),
            b"\x01"
        );
        assert_eq!(
            bytes(
                modified_key("end", false, false, false, true),
                TermMode::NONE
            ),
            b"\x05"
        );
        assert_eq!(
            bytes(
                modified_key("delete", false, true, false, false),
                TermMode::NONE
            ),
            b"\x1bd"
        );
        assert_eq!(
            bytes(
                modified_key("backspace", false, false, false, true),
                TermMode::NONE
            ),
            b"\x15"
        );
        assert_eq!(
            bytes(
                modified_key("back", false, false, false, true),
                TermMode::NONE
            ),
            b"\x15"
        );
        assert_eq!(
            bytes(
                modified_key("delete", false, false, false, true),
                TermMode::NONE
            ),
            b"\x0b"
        );
    }

    #[test]
    fn maps_ctrl_alt_and_shift_enter_sequences() {
        assert_eq!(
            bytes(modified_key("a", false, false, true, false), TermMode::NONE),
            b"\x01"
        );
        assert_eq!(
            bytes(modified_key("C", true, false, true, false), TermMode::NONE),
            b"\x03"
        );
        assert_eq!(
            bytes(modified_key("[", false, false, true, false), TermMode::NONE),
            b"\x1b"
        );
        assert_eq!(
            bytes(
                modified_key("enter", true, false, false, false),
                TermMode::NONE
            ),
            b"\n"
        );
        assert_eq!(
            bytes(
                modified_key("Tab", true, false, false, false),
                TermMode::NONE
            ),
            b"\x1b[Z"
        );
        assert_eq!(
            bytes(
                modified_key("BackTab", true, false, false, false),
                TermMode::NONE
            ),
            b"\x1b[Z"
        );
        assert_eq!(
            bytes(
                modified_key("enter", false, true, false, false),
                TermMode::NONE
            ),
            b"\x1b\r"
        );
        assert_eq!(
            bytes(modified_key("x", false, true, false, false), TermMode::NONE),
            b"\x1bx"
        );
    }

    #[test]
    fn maps_mouse_reports() {
        let point = TerminalCellPoint { row: 1, col: 2 };
        assert_eq!(
            mouse_report_sequence(
                Some(MouseButton::Left),
                point,
                MouseReportKind::Press,
                Modifiers::default(),
                TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE
            )
            .unwrap(),
            b"\x1b[<0;3;2M"
        );
        assert_eq!(
            mouse_report_sequence(
                Some(MouseButton::Left),
                point,
                MouseReportKind::Release,
                Modifiers::default(),
                TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE
            )
            .unwrap(),
            b"\x1b[<0;3;2m"
        );
        assert_eq!(
            mouse_report_sequence(
                Some(MouseButton::Left),
                point,
                MouseReportKind::Move,
                Modifiers {
                    shift: true,
                    alt: true,
                    control: true,
                    platform: false,
                    function: false,
                },
                TermMode::MOUSE_DRAG | TermMode::SGR_MOUSE
            )
            .unwrap(),
            b"\x1b[<60;3;2M"
        );
        assert_eq!(
            mouse_report_sequence(
                Some(MouseButton::Navigate(NavigationDirection::Back)),
                point,
                MouseReportKind::Wheel,
                Modifiers::default(),
                TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE
            )
            .unwrap(),
            b"\x1b[<64;3;2M"
        );
    }

    #[test]
    fn maps_normal_and_utf8_mouse_reports() {
        let point = TerminalCellPoint { row: 1, col: 2 };
        assert_eq!(
            mouse_report_sequence(
                Some(MouseButton::Left),
                point,
                MouseReportKind::Press,
                Modifiers::default(),
                TermMode::MOUSE_MODE
            )
            .unwrap(),
            vec![b'\x1b', b'[', b'M', 32, 35, 34]
        );

        let utf8_point = TerminalCellPoint { row: 100, col: 100 };
        let report = mouse_report_sequence(
            Some(MouseButton::Left),
            utf8_point,
            MouseReportKind::Press,
            Modifiers::default(),
            TermMode::MOUSE_REPORT_CLICK | TermMode::UTF8_MOUSE,
        )
        .unwrap();
        assert_eq!(&report[..4], &[b'\x1b', b'[', b'M', 32]);
        assert!(report.len() > 6);
    }

    #[test]
    fn selects_text_from_terminal_grid() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"hello\r\nworld");

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

        assert_eq!(
            state.handle.selected_text_for_range(SelectionRange {
                start: TerminalSelectionPoint { line: 0, col: 0 },
                end: TerminalSelectionPoint { line: 0, col: 11 },
            }),
            "中文恢复记录"
        );
    }

    #[test]
    fn alacritty_selection_tracks_output_scrollback_rotation() {
        let mut state = TerminalModel::new_for_test(10, 3, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree");
        state.start_selection(
            TerminalSelectionPoint { line: 1, col: 0 },
            TerminalSide::Left,
        );
        state.update_selection(
            TerminalSelectionPoint { line: 1, col: 3 },
            TerminalSide::Right,
        );
        assert_eq!(state.selected_text(), Some("two".to_string()));

        state.process_bytes(b"\r\nfour");

        assert_eq!(state.selected_text(), Some("two".to_string()));
        assert_eq!(
            state.selection_range(),
            Some(SelectionRange {
                start: TerminalSelectionPoint { line: 0, col: 0 },
                end: TerminalSelectionPoint { line: 0, col: 3 },
            })
        );
    }

    #[test]
    fn updates_render_snapshot_after_output() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"hello");

        let pending_snapshot = state.handle.snapshot();
        let pending_text: String = pending_snapshot
            .cells
            .iter()
            .filter(|cell| cell.point.line.0 == 0)
            .map(|cell| cell.c)
            .collect();
        assert!(!pending_text.contains("hello"));

        state.handle.publish_snapshot();

        let snapshot = state.handle.snapshot();
        let text: String = snapshot
            .cells
            .iter()
            .filter(|cell| cell.point.line.0 == 0)
            .map(|cell| cell.c)
            .collect();

        assert!(text.contains("hello"));
        assert_eq!(snapshot.columns, 10);
        assert_eq!(snapshot.screen_lines, 4);
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
    fn display_cursor_tracks_scroll_offset_like_zed() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
        state.handle.publish_snapshot();
        assert!(state.scroll_display(2));
        let snapshot = state.sync_for_test();

        let display_cursor = DisplayCursor::from(snapshot.cursor.point, snapshot.display_offset);

        assert_eq!(snapshot.display_offset, 2);
        assert_eq!(
            display_cursor,
            DisplayCursor {
                row: snapshot.cursor.point.line.0 + 2,
                col: snapshot.cursor.point.column.0,
            }
        );
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
        let row = DisplayCursor::from(bottom.cursor.point, bottom.display_offset).row;

        assert_eq!(bottom.display_offset, 0);
        assert!(bottom.scrolled_to_bottom);
        assert!(row >= 0);
        assert_eq!(
            bounds.origin.x,
            px(15.0) + px(10.0) * bottom.cursor.point.column.0 as f32
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
    fn pending_session_reports_initial_layout_once() {
        let (binding, rx) = TerminalSessionBinding::pending();

        binding.record_layout(120, 36);

        assert_eq!(
            rx.recv_timeout(Duration::from_millis(10)).unwrap(),
            (120, 36)
        );
        binding.record_layout(121, 37);
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
        let colors = Colors::default();
        let mut cell = Cell::default();
        cell.fg = Color::Named(NamedColor::Foreground);
        cell.bg = Color::Named(NamedColor::Background);

        let normal = renderer.cell_render_colors(&cell, &colors);
        cell.flags.insert(Flags::INVERSE);
        let inverse = renderer.cell_render_colors(&cell, &colors);

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
        let colors = Colors::default();
        let mut cell = Cell::default();
        cell.fg = Color::Named(NamedColor::Blue);
        cell.bg = Color::Named(NamedColor::Background);
        cell.flags.insert(Flags::BOLD);

        let (fg, _) = renderer.cell_render_colors(&cell, &colors);
        assert_eq!(
            fg,
            renderer
                .palette
                .resolve(Color::Named(NamedColor::BrightBlue), &colors)
        );
    }

    #[test]
    fn default_named_colors_ignore_stale_dynamic_terminal_colors() {
        let palette = ColorPalette::builder()
            .background(0xee, 0xee, 0xee)
            .foreground(0x11, 0x11, 0x11)
            .cursor(0x22, 0x22, 0x22)
            .build();
        let mut colors = Colors::default();
        colors[NamedColor::Background] = Some(Rgb {
            r: 0x11,
            g: 0x14,
            b: 0x1a,
        });
        colors[NamedColor::Foreground] = Some(Rgb {
            r: 0xd6,
            g: 0xda,
            b: 0xe2,
        });

        assert_eq!(
            palette.resolve(Color::Named(NamedColor::Background), &colors),
            palette.background
        );
        assert_eq!(
            palette.resolve(Color::Named(NamedColor::Foreground), &colors),
            palette.foreground
        );
    }

    #[test]
    fn color_requests_use_current_palette_for_default_colors() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.colors = ColorPalette::builder()
            .background(0xee, 0xee, 0xee)
            .foreground(0x11, 0x11, 0x11)
            .cursor(0x22, 0x22, 0x22)
            .build();

        assert_eq!(
            state.color_request(NamedColor::Background as usize),
            Rgb {
                r: 0xee,
                g: 0xee,
                b: 0xee
            }
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
        let colors = Colors::default();
        let mut cell = Cell::default();
        cell.fg = Color::Named(NamedColor::Blue);
        cell.bg = Color::Named(NamedColor::Red);
        cell.flags.insert(Flags::BOLD | Flags::INVERSE);

        let (fg, bg) = renderer.cell_render_colors(&cell, &colors);
        assert_eq!(
            fg,
            renderer
                .palette
                .resolve(Color::Named(NamedColor::BrightRed), &colors)
        );
        assert_eq!(
            bg,
            renderer
                .palette
                .resolve(Color::Named(NamedColor::Blue), &colors)
        );
    }

    #[test]
    fn color_requests_use_configured_palette() {
        let palette = ColorPalette::builder()
            .background(0x28, 0x2A, 0x36)
            .foreground(0xF8, 0xF8, 0xF2)
            .cursor(0xF8, 0xF8, 0xF2)
            .selection(0x44, 0x47, 0x5A)
            .black(0x21, 0x22, 0x2C)
            .bright_black(0x62, 0x72, 0xA4)
            .build();

        assert_eq!(
            palette.color_request(NamedColor::Background as usize),
            Rgb {
                r: 0x28,
                g: 0x2A,
                b: 0x36
            }
        );
        assert_eq!(
            palette.color_request(NamedColor::Foreground as usize),
            Rgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            }
        );
        assert_eq!(
            palette.color_request(NamedColor::Black as usize),
            Rgb {
                r: 0x21,
                g: 0x22,
                b: 0x2C
            }
        );
        assert_eq!(
            palette.color_request(NamedColor::BrightBlack as usize),
            Rgb {
                r: 0x62,
                g: 0x72,
                b: 0xA4
            }
        );
        assert_eq!(
            palette.color_request(NamedColor::BrightForeground as usize),
            hsla_to_rgb(brighten_color(rgb_to_hsla(Rgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            })))
        );
        assert_eq!(
            palette.color_request(NamedColor::DimForeground as usize),
            hsla_to_rgb(dim_color(rgb_to_hsla(Rgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            })))
        );
        assert_eq!(
            palette.color_request(999),
            Rgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            }
        );
    }
}
