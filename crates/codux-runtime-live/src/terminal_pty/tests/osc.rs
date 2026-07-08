use super::*;

#[test]
fn terminal_progress_osc_emits_working_status_without_session_mutation() {
    let dir =
        std::env::temp_dir().join(format!("codux-terminal-progress-start-{}", Uuid::new_v4()));
    let bridge = Arc::new(AIRuntimeBridge::with_paths(
        dir.join("root"),
        dir.join("temp"),
        dir.join("home"),
    ));
    bridge.ensure_started().expect("runtime should start");
    let terminal_id = format!("test-terminal-progress-start-{}", Uuid::new_v4());
    let binding = AIRuntimeTerminalBinding {
        terminal_id: terminal_id.clone(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Terminal".to_string(),
        cwd: "/tmp/project".to_string(),
        tool: None,
        is_active: false,
        session_key: Some("codewhale-session-1".to_string()),
        terminal_instance_id: Some("terminal-instance-1".to_string()),
    };
    let mut watcher = AIRuntimeTerminalOutputWatcher::new(binding.clone(), Arc::clone(&bridge));

    watcher.handle_terminal_event(&TerminalEvent::Output {
        session_id: terminal_id.clone(),
        text: String::new(),
        bytes: b"\x1b]9;4;3\x07".to_vec(),
    });

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);
    assert_eq!(status.source, "terminal-progress-osc");
    assert!(bridge.runtime_state_snapshot().sessions.is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_progress_osc_emits_completed_status() {
    let dir = std::env::temp_dir().join(format!("codux-terminal-progress-idle-{}", Uuid::new_v4()));
    let bridge = Arc::new(AIRuntimeBridge::with_paths(
        dir.join("root"),
        dir.join("temp"),
        dir.join("home"),
    ));
    bridge.ensure_started().expect("runtime should start");
    let terminal_id = format!("test-terminal-progress-idle-{}", Uuid::new_v4());
    let binding = AIRuntimeTerminalBinding {
        terminal_id: terminal_id.clone(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Terminal".to_string(),
        cwd: "/tmp/project".to_string(),
        tool: None,
        is_active: false,
        session_key: Some("codewhale-session-1".to_string()),
        terminal_instance_id: Some("terminal-instance-1".to_string()),
    };
    let mut watcher = AIRuntimeTerminalOutputWatcher::new(binding.clone(), Arc::clone(&bridge));

    watcher.handle_terminal_event(&TerminalEvent::Output {
        session_id: terminal_id.clone(),
        text: String::new(),
        bytes: b"\x1b]9;4;0\x07".to_vec(),
    });

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Completed);
    assert_eq!(status.source, "terminal-progress-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_notification_osc_emits_waiting_status() {
    let dir = std::env::temp_dir().join(format!(
        "codux-terminal-notification-waiting-{}",
        Uuid::new_v4()
    ));
    let bridge = Arc::new(AIRuntimeBridge::with_paths(
        dir.join("root"),
        dir.join("temp"),
        dir.join("home"),
    ));
    bridge.ensure_started().expect("runtime should start");
    let terminal_id = format!("test-terminal-notification-{}", Uuid::new_v4());
    let binding = AIRuntimeTerminalBinding {
        terminal_id: terminal_id.clone(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Terminal".to_string(),
        cwd: "/tmp/project".to_string(),
        tool: None,
        is_active: false,
        session_key: Some("codex-session-1".to_string()),
        terminal_instance_id: Some("terminal-instance-1".to_string()),
    };
    let mut watcher = AIRuntimeTerminalOutputWatcher::new(binding.clone(), Arc::clone(&bridge));

    watcher.handle_terminal_event(&TerminalEvent::Output {
        session_id: terminal_id.clone(),
        text: String::new(),
        bytes: b"\x1b]9;Approval requested: npm install\x07".to_vec(),
    });

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Waiting);
    assert_eq!(status.source, "terminal-notification-osc");

    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn terminal_notification_osc_survives_chunk_split() {
    let dir = std::env::temp_dir().join(format!("codux-terminal-osc-split-{}", Uuid::new_v4()));
    let bridge = Arc::new(AIRuntimeBridge::with_paths(
        dir.join("root"),
        dir.join("temp"),
        dir.join("home"),
    ));
    bridge.ensure_started().expect("runtime should start");
    let terminal_id = format!("test-terminal-osc-split-{}", Uuid::new_v4());
    let binding = AIRuntimeTerminalBinding {
        terminal_id: terminal_id.clone(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Terminal".to_string(),
        cwd: "/tmp/project".to_string(),
        tool: None,
        is_active: false,
        session_key: Some("codex-session-1".to_string()),
        terminal_instance_id: Some("terminal-instance-1".to_string()),
    };
    let mut watcher = AIRuntimeTerminalOutputWatcher::new(binding.clone(), Arc::clone(&bridge));

    // Split beyond the old 32-byte tail so the prefix must survive the read gap.
    let sequence = b"\x1b]9;Approval requested: allow codex to run cargo build --release\x07";
    let (first, second) = sequence.split_at(48);
    for chunk in [first, second] {
        watcher.handle_terminal_event(&TerminalEvent::Output {
            session_id: terminal_id.clone(),
            text: String::new(),
            bytes: chunk.to_vec(),
        });
    }

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Waiting);
    assert_eq!(status.source, "terminal-notification-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn oversized_unterminated_osc_is_dropped_and_parser_recovers() {
    let dir = std::env::temp_dir().join(format!("codux-terminal-osc-oversized-{}", Uuid::new_v4()));
    let bridge = Arc::new(AIRuntimeBridge::with_paths(
        dir.join("root"),
        dir.join("temp"),
        dir.join("home"),
    ));
    bridge.ensure_started().expect("runtime should start");
    let terminal_id = format!("test-terminal-osc-oversized-{}", Uuid::new_v4());
    let binding = AIRuntimeTerminalBinding {
        terminal_id: terminal_id.clone(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Terminal".to_string(),
        cwd: "/tmp/project".to_string(),
        tool: None,
        is_active: false,
        session_key: Some("codex-session-1".to_string()),
        terminal_instance_id: Some("terminal-instance-1".to_string()),
    };
    let mut watcher = AIRuntimeTerminalOutputWatcher::new(binding.clone(), Arc::clone(&bridge));

    let mut oversized = b"\x1b]9;".to_vec();
    oversized.extend(std::iter::repeat(b'x').take(2048));
    watcher.handle_terminal_event(&TerminalEvent::Output {
        session_id: terminal_id.clone(),
        text: String::new(),
        bytes: oversized,
    });
    watcher.handle_terminal_event(&TerminalEvent::Output {
        session_id: terminal_id.clone(),
        text: String::new(),
        bytes: b"\x1b]9;4;3\x07".to_vec(),
    });

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);
    assert_eq!(status.source, "terminal-progress-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_command_osc_drives_working_then_clears() {
    let dir = std::env::temp_dir().join(format!("codux-terminal-osc-command-{}", Uuid::new_v4()));
    let bridge = Arc::new(AIRuntimeBridge::with_paths(
        dir.join("root"),
        dir.join("temp"),
        dir.join("home"),
    ));
    bridge.ensure_started().expect("runtime should start");
    let terminal_id = format!("test-terminal-osc-command-{}", Uuid::new_v4());
    let binding = AIRuntimeTerminalBinding {
        terminal_id: terminal_id.clone(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Terminal".to_string(),
        cwd: "/tmp/project".to_string(),
        tool: None,
        is_active: false,
        session_key: None,
        terminal_instance_id: Some("terminal-instance-1".to_string()),
    };
    let mut watcher = AIRuntimeTerminalOutputWatcher::new(binding.clone(), Arc::clone(&bridge));

    // A (prompt) is ignored, C starts the command, D;exit ends it.
    watcher.handle_terminal_event(&TerminalEvent::Output {
        session_id: terminal_id.clone(),
        text: String::new(),
        bytes: b"\x1b]133;A\x07\x1b]133;C\x07".to_vec(),
    });
    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);
    assert_eq!(status.source, "terminal-command-osc");

    watcher.handle_terminal_event(&TerminalEvent::Output {
        session_id: terminal_id.clone(),
        text: String::new(),
        bytes: b"\x1b]133;D;0\x07\x1b]133;A\x07".to_vec(),
    });
    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Idle);
    assert_eq!(status.source, "terminal-command-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_command_osc_survives_chunk_split() {
    let dir = std::env::temp_dir().join(format!(
        "codux-terminal-osc-command-split-{}",
        Uuid::new_v4()
    ));
    let bridge = Arc::new(AIRuntimeBridge::with_paths(
        dir.join("root"),
        dir.join("temp"),
        dir.join("home"),
    ));
    bridge.ensure_started().expect("runtime should start");
    let terminal_id = format!("test-terminal-osc-command-split-{}", Uuid::new_v4());
    let binding = AIRuntimeTerminalBinding {
        terminal_id: terminal_id.clone(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Terminal".to_string(),
        cwd: "/tmp/project".to_string(),
        tool: None,
        is_active: false,
        session_key: None,
        terminal_instance_id: Some("terminal-instance-1".to_string()),
    };
    let mut watcher = AIRuntimeTerminalOutputWatcher::new(binding.clone(), Arc::clone(&bridge));

    // Split inside the "133;" prefix so the tail must carry the partial OSC.
    for chunk in [b"\x1b]13".as_slice(), b"3;C\x07".as_slice()] {
        watcher.handle_terminal_event(&TerminalEvent::Output {
            session_id: terminal_id.clone(),
            text: String::new(),
            bytes: chunk.to_vec(),
        });
    }

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);
    assert_eq!(status.source, "terminal-command-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn terminal_output_refreshes_kiro_screen_signal_without_poll() {
    let dir = std::env::temp_dir().join(format!(
        "codux-kiro-terminal-screen-signal-{}",
        Uuid::new_v4()
    ));
    let bridge = Arc::new(AIRuntimeBridge::with_paths(
        dir.join("root"),
        dir.join("temp"),
        dir.join("home"),
    ));
    bridge.ensure_started().expect("runtime should start");
    let terminal_id = format!("test-kiro-terminal-{}", Uuid::new_v4());
    let binding = AIRuntimeTerminalBinding {
        terminal_id: terminal_id.clone(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Kiro".to_string(),
        cwd: "/tmp/project".to_string(),
        tool: Some("kiro".to_string()),
        is_active: false,
        session_key: Some("kiro-session-1".to_string()),
        terminal_instance_id: Some("terminal-instance-1".to_string()),
    };
    bridge.registry().upsert(binding.clone());
    let screen = Arc::new(parking_lot::Mutex::new(HeadlessTerminalScreen::new(
        80, 24, 100,
    )));
    bridge
        .registry()
        .register_screen(&terminal_id, Arc::downgrade(&screen));
    let mut watcher = AIRuntimeTerminalOutputWatcher::new(binding.clone(), Arc::clone(&bridge));
    bridge
        .submit_hook_event(AIHookEventPayload {
            kind: "sessionStarted".to_string(),
            terminal_id: terminal_id.clone(),
            terminal_instance_id: binding.terminal_instance_id.clone(),
            project_id: "project-1".to_string(),
            project_name: "Codux".to_string(),
            project_path: Some("/tmp/project".to_string()),
            session_title: "Kiro".to_string(),
            tool: "kiro".to_string(),
            ai_session_id: Some("kiro-session-1".to_string()),
            model: None,
            input_tokens: None,
            output_tokens: None,
            cached_input_tokens: None,
            total_tokens: None,
            updated_at: now_seconds(),
            metadata: None,
        })
        .expect("session hook should submit");
    wait_for_session_state(&bridge, &terminal_id, "idle", Duration::from_secs(2));

    let output = "kiro_default · auto\nKiro is working · Type to steer · Ctrl+S to queue";
    screen.lock().process(output.as_bytes());
    watcher.handle_terminal_event(&TerminalEvent::Output {
        session_id: terminal_id.clone(),
        text: output.to_string(),
        bytes: output.as_bytes().to_vec(),
    });

    wait_for_session_state(&bridge, &terminal_id, "responding", Duration::from_secs(2));

    let _ = std::fs::remove_dir_all(dir);
}
