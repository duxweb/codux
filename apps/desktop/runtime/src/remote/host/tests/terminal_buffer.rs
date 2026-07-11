use super::*;

#[test]
fn remote_terminal_output_sequence_is_session_scoped() {
    let support_dir = temp_support_dir("codux-remote-terminal-output-seq");
    let runtime = RemoteHostRuntime::new(support_dir.clone());

    assert_eq!(runtime.current_terminal_output_seq("terminal-a"), 0);
    assert_eq!(runtime.next_terminal_output_seq("terminal-a"), 1);
    assert_eq!(runtime.next_terminal_output_seq("terminal-a"), 2);
    assert_eq!(runtime.next_terminal_output_seq("terminal-b"), 1);
    assert_eq!(runtime.current_terminal_output_seq("terminal-a"), 2);

    runtime.clear_terminal_output_seq("terminal-a");

    assert_eq!(runtime.current_terminal_output_seq("terminal-a"), 0);
    assert_eq!(runtime.current_terminal_output_seq("terminal-b"), 1);

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn remote_terminal_buffer_window_returns_retained_history_window() {
    let support_dir = temp_support_dir("codux-remote-terminal-buffer-window");
    let terminals = Arc::new(TerminalManager::new());
    let runtime = RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    );
    let session_id = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf abcdef".to_string()),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");

    let mut window = None;
    for _ in 0..20 {
        let current = runtime
            .terminal_buffer_window(&session_id, 0, buffer_options(3, None, false))
            .expect("terminal buffer window");
        if current.total_characters >= 6 {
            window = Some(current);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let window = window.expect("terminal output");

    assert_eq!(window.data, "abc");
    assert_eq!(window.offset, 0);
    assert_eq!(window.total_characters, 6);
    assert!(window.truncated);
    assert!(!window.has_previous);

    let next = runtime
        .terminal_buffer_window(&session_id, 3, buffer_options(3, None, false))
        .expect("next terminal buffer window");
    assert_eq!(next.data, "def");
    assert_eq!(next.offset, 3);
    assert_eq!(next.total_characters, 6);
    assert!(!next.truncated);

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn remote_terminal_buffer_window_freezes_pages_for_request_id() {
    let support_dir = temp_support_dir("codux-remote-terminal-buffer-frozen-pages");
    let terminals = Arc::new(TerminalManager::new());
    let runtime = RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    );
    let session_id = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("cat".to_string()),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");
    terminals
        .write(&session_id, b"abcdef")
        .expect("write initial");

    let mut first = None;
    for _ in 0..20 {
        let current = runtime
            .terminal_buffer_window(
                &session_id,
                0,
                buffer_options(3, Some("request-freeze"), false),
            )
            .expect("first terminal buffer window");
        if current.total_characters >= 6 {
            first = Some(current);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let first = first.expect("terminal output");
    assert_eq!(first.data, "abc");
    assert_eq!(first.total_characters, 6);
    assert!(first.truncated);

    terminals
        .write(&session_id, b"XYZ")
        .expect("write appended");
    std::thread::sleep(std::time::Duration::from_millis(25));

    let second = runtime
        .terminal_buffer_window(
            &session_id,
            3,
            buffer_options(3, Some("request-freeze"), false),
        )
        .expect("second terminal buffer window");
    assert_eq!(second.data, "def");
    assert_eq!(second.offset, 3);
    assert_eq!(second.total_characters, 6);
    assert_eq!(second.output_seq, first.output_seq);
    assert!(!second.truncated);

    let live = runtime
        .terminal_buffer_window(
            &session_id,
            0,
            buffer_options(16, Some("request-live"), false),
        )
        .expect("live terminal buffer window");
    assert!(live.total_characters >= 9);
    assert!(live.data.contains("XYZ"));

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn remote_terminal_buffer_window_tail_returns_history_tail() {
    let support_dir = temp_support_dir("codux-remote-terminal-buffer-tail-window");
    let terminals = Arc::new(TerminalManager::new());
    let runtime = RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    );
    let session_id = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf abcdef".to_string()),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");

    let mut window = None;
    for _ in 0..20 {
        let current = runtime
            .terminal_buffer_window(&session_id, 0, buffer_options(3, Some("request-1"), true))
            .expect("terminal buffer window");
        if current.data.contains("def") {
            window = Some(current);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let window = window.expect("terminal output");

    assert!(window.data.contains("def"));
    assert_eq!(window.offset, 3);
    assert_eq!(window.total_characters, 6);
    assert!(!window.truncated);
    assert_eq!(window.request_id.as_deref(), Some("request-1"));
    assert!(window.tail);
    assert!(window.has_previous);

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn remote_terminal_buffer_window_tail_ships_keyframe_for_normal_screen() {
    // Raw history replay cannot reconstruct a primary-screen TUI that redraws
    // in place (codex spinner): the tail starts mid-stream and replays into
    // stacked garbage lines. Every tail baseline therefore ships the screen
    // keyframe; its home+2J wipe replaces the tail's on-screen result, and
    // since the single-owner grid model viewers render the host grid 1:1, so
    // the keyframe cannot land at reflowed rows (the old ghost-prompt bug).
    let support_dir = temp_support_dir("codux-remote-terminal-buffer-screen-baseline");
    let terminals = Arc::new(TerminalManager::new());
    let runtime = RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    );
    let session_id = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf 'old line\\n\\033[2J\\033[Hvisible tui'".to_string()),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");

    let mut window = None;
    for _ in 0..20 {
        let current = runtime
            .terminal_buffer_window(&session_id, 0, buffer_options(64, Some("request-1"), true))
            .expect("terminal buffer window");
        if current.data.contains("visible tui") {
            window = Some(current);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let window = window.expect("terminal buffer window");

    assert!(window.data.contains("visible tui"));
    let screen_data = window
        .screen_data
        .as_deref()
        .expect("normal-screen tail baseline must ship the screen keyframe");
    assert!(
        screen_data.contains("visible tui"),
        "keyframe must carry the current screen: {screen_data:?}"
    );
    assert!(window.tail);

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn remote_terminal_buffer_window_tail_includes_target_viewport_keyframe() {
    let support_dir = temp_support_dir("codux-remote-terminal-buffer-viewport-keyframe");
    write_paired_remote_settings(&support_dir);
    let terminals = Arc::new(TerminalManager::new());
    let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    ));
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }
    let session_id = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some(
                    "printf 'wide normal screen\\n\\033[2J\\033[Hmobile keyframe'".to_string(),
                ),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");

    for _ in 0..20 {
        if terminals
            .screen_snapshot(&session_id)
            .map(|snapshot| snapshot.data.contains("mobile keyframe"))
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    terminals
        .claim_viewport(&session_id, "remote:phone-a")
        .expect("phone owns viewport");
    runtime.send_terminal_buffer(
        &session_id,
        Some("phone-a"),
        0,
        viewport_buffer_options(128, Some("request-1"), true, 72, 18),
    );

    let (_, data) = transport
        .wait_for_message(|(_, data)| {
            let Ok(text) = std::str::from_utf8(data) else {
                return false;
            };
            let Ok(envelope) = runtime.service().parse_incoming_envelope(text) else {
                return false;
            };
            envelope.kind == REMOTE_TERMINAL_OUTPUT
                && envelope.payload.get("buffer").and_then(Value::as_bool) == Some(true)
        })
        .expect("terminal baseline");
    let text = String::from_utf8(data).expect("utf8 transport");
    let envelope = runtime
        .service()
        .parse_incoming_envelope(&text)
        .expect("parse outgoing envelope");
    let baseline = envelope.payload;

    assert_eq!(baseline["tail"], true);
    let screen_data = baseline["screenData"]
        .as_str()
        .expect("target viewport baseline must ship keyframe");
    assert!(screen_data.contains("mobile keyframe"));
    let snapshot = terminals
        .screen_snapshot(&session_id)
        .expect("screen snapshot after viewport baseline");
    assert_eq!(snapshot.cols, 72);
    assert_eq!(snapshot.rows, 18);

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_resource_subscribe_baseline_keyframe_for_alt_screen() {
    // The alternate buffer has no scrollback, so a re-attaching viewer cannot
    // reconstruct an alt-screen TUI from the raw history alone -- the baseline
    // MUST carry the screen keyframe. (Normal screens ship it too; see
    // remote_terminal_buffer_window_tail_ships_keyframe_for_normal_screen.)
    let support_dir = temp_support_dir("codux-resource-subscribe-terminal-screen-baseline");
    write_paired_remote_settings(&support_dir);
    let terminals = Arc::new(TerminalManager::new());
    let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    ));
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }
    let session_id = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf '\\033[?1049h\\033[2J\\033[HALT_UI'; sleep 30".to_string()),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");

    // Wait for the alternate screen to be active and painted.
    for _ in 0..40 {
        if terminals
            .screen_snapshot(&session_id)
            .map(|snapshot| {
                snapshot.input_mode.alternate_screen && snapshot.data.contains("ALT_UI")
            })
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    runtime.handle_resource_subscribe(&RemoteEnvelope {
        kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
        device_id: Some("phone-a".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({
            "resource": REMOTE_RESOURCE_TERMINALS,
            "sessionId": session_id,
            "baseline": true,
            "maxChars": 64,
            "requestId": "request-1",
        }),
    });

    let (_, data) = transport
        .wait_for_message(|(_, data)| {
            let Ok(text) = std::str::from_utf8(data) else {
                return false;
            };
            let Ok(envelope) = runtime.service().parse_incoming_envelope(text) else {
                return false;
            };
            envelope.kind == REMOTE_TERMINAL_OUTPUT
                && envelope.payload.get("buffer").and_then(Value::as_bool) == Some(true)
        })
        .expect("terminal baseline");
    let text = String::from_utf8(data).expect("utf8 transport");
    let envelope = runtime
        .service()
        .parse_incoming_envelope(&text)
        .expect("parse outgoing envelope");
    let baseline = envelope.payload;

    assert_eq!(baseline["requestId"], "request-1");
    assert_eq!(baseline["tail"], true);
    // The keyframe is the only way to restore the alt-screen TUI, so it must
    // be present and carry the alt UI.
    assert!(
        baseline["screenData"]
            .as_str()
            .unwrap_or_default()
            .contains("ALT_UI"),
        "alt-screen baseline must ship the screen keyframe"
    );

    terminals
        .kill_and_wait(&session_id, Duration::from_secs(2))
        .expect("stop terminal");

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn remote_terminal_live_output_is_data_only_without_screen_keyframe() {
    let support_dir = temp_support_dir("codux-remote-terminal-live-screen-keyframe");
    write_paired_remote_settings(&support_dir);
    let terminals = Arc::new(TerminalManager::new());
    let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    ));
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }
    let session_id = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some(
                    "printf '\\033[2J\\033[Hrestored tui\\n\\033[3;1Hinput box'".to_string(),
                ),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");

    for _ in 0..20 {
        if terminals
            .screen_snapshot(&session_id)
            .map(|snapshot| snapshot.data.contains("restored tui"))
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    runtime.register_terminal_viewer(&session_id, Some("device-1"));
    transport.take_messages();

    runtime.queue_terminal_output_batch(session_id.clone(), "partial live raw".to_string());
    runtime.flush_terminal_output_batch(&session_id);

    let mut live = None;
    for (_, data) in transport.take_messages() {
        let text = String::from_utf8(data).expect("utf8 transport");
        let envelope = runtime
            .service()
            .parse_incoming_envelope(&text)
            .expect("parse outgoing envelope");
        if envelope.kind == "terminal.output" && envelope.session_id.as_deref() == Some(&session_id)
        {
            live = Some(envelope.payload);
            break;
        }
    }
    let live = live.expect("live terminal output");

    assert_eq!(live["data"], "partial live raw");
    assert_eq!(live["outputSeq"], 1);
    // Live output is a pure byte stream now — NO screen keyframe. Replaying a
    // whole-screen keyframe on top of the emulator's own scrollback duplicated
    // the screen (badly on resize bursts), so the host no longer sends one.
    assert!(
        live.get("screenData").is_none(),
        "live terminal output must not carry a screen keyframe"
    );

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_buffer_request_does_not_resize_remote_pty() {
    let support_dir = temp_support_dir("codux-remote-terminal-buffer-readonly");
    let terminals = Arc::new(TerminalManager::new());
    let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    ));
    let session_id = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf ready".to_string()),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");

    runtime.handle_terminal_buffer(&RemoteEnvelope {
        kind: "terminal.buffer".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({
            "offset": 0,
            "cols": 44,
            "rows": 12,
        }),
    });

    let info = terminals
        .list()
        .into_iter()
        .find(|terminal| terminal.id == session_id)
        .expect("terminal");
    assert_eq!(info.cols, 100);
    assert_eq!(info.rows, 32);

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_subscribe_does_not_push_screen_keyframe() {
    let support_dir = temp_support_dir("codux-remote-terminal-subscribe-keyframe");
    write_paired_remote_settings(&support_dir);
    let terminals = Arc::new(TerminalManager::new());
    let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    ));
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }
    let session_id = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf ready".to_string()),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");

    for _ in 0..20 {
        if terminals
            .screen_snapshot(&session_id)
            .map(|snapshot| snapshot.data.contains("ready"))
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    transport.take_messages();
    runtime.handle_terminal_subscribe(&RemoteEnvelope {
        kind: "terminal.subscribe".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({}),
    });

    let mut keyframe = None;
    for (device_id, data) in transport.take_messages() {
        let text = String::from_utf8(data).expect("utf8 transport");
        let envelope = runtime
            .service()
            .parse_incoming_envelope(&text)
            .expect("parse outgoing envelope");
        if device_id.as_deref() == Some("device-1")
            && envelope.kind == "terminal.output"
            && envelope.session_id.as_deref() == Some(&session_id)
        {
            keyframe = Some(envelope.payload);
            break;
        }
    }
    // A plain subscribe (no baseline requested) pushes viewport state only —
    // no screen keyframe. The keyframe duplicated the screen in the desktop's
    // own scrollback; the re-attach seed rides the baseline buffer instead.
    assert!(
        keyframe.is_none(),
        "subscribe must not push a screen keyframe (it duplicated the screen)"
    );

    fs::remove_dir_all(support_dir).ok();
}
