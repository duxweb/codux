use super::*;

fn watcher_fixture(
    tag: &str,
) -> (
    std::path::PathBuf,
    Arc<AIRuntimeBridge>,
    String,
    AIRuntimeTerminalOutputWatcher,
) {
    let dir = std::env::temp_dir().join(format!("codux-{tag}-{}", Uuid::new_v4()));
    let bridge = Arc::new(AIRuntimeBridge::with_paths(
        dir.join("root"),
        dir.join("temp"),
        dir.join("home"),
    ));
    bridge.ensure_started().expect("runtime should start");
    let terminal_id = format!("test-{tag}-{}", Uuid::new_v4());
    let binding = AIRuntimeTerminalBinding {
        terminal_id: terminal_id.clone(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Terminal".to_string(),
        cwd: "/tmp/project".to_string(),
        tool: None,
        is_active: false,
        session_key: Some("session-1".to_string()),
        terminal_instance_id: Some("terminal-instance-1".to_string()),
    };
    let watcher = AIRuntimeTerminalOutputWatcher::new(binding, Arc::clone(&bridge));
    (dir, bridge, terminal_id, watcher)
}

fn push_output(watcher: &mut AIRuntimeTerminalOutputWatcher, terminal_id: &str, bytes: &[u8]) {
    watcher.handle_terminal_event(&TerminalEvent::Output {
        session_id: terminal_id.to_string(),
        text: String::new(),
        bytes: bytes.to_vec(),
    });
}

#[test]
fn terminal_progress_osc_emits_working_status_without_session_mutation() {
    let (dir, bridge, terminal_id, mut watcher) = watcher_fixture("terminal-progress-start");

    push_output(&mut watcher, &terminal_id, b"\x1b]9;4;3\x07");

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);
    assert_eq!(status.source, "terminal-progress-osc");
    assert!(bridge.runtime_state_snapshot().sessions.is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_progress_osc_emits_completed_status() {
    let (dir, bridge, terminal_id, mut watcher) = watcher_fixture("terminal-progress-idle");

    push_output(&mut watcher, &terminal_id, b"\x1b]9;4;0\x07");

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Completed);
    assert_eq!(status.source, "terminal-progress-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_notification_osc_emits_waiting_status() {
    let (dir, bridge, terminal_id, mut watcher) = watcher_fixture("terminal-notification");

    push_output(
        &mut watcher,
        &terminal_id,
        b"\x1b]9;Approval requested: npm install\x07",
    );

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Waiting);
    assert_eq!(status.source, "terminal-notification-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_notification_osc_survives_chunk_split() {
    let (dir, bridge, terminal_id, mut watcher) = watcher_fixture("terminal-osc-split");

    // Split beyond the old 32-byte tail so the prefix must survive the read gap.
    let sequence = b"\x1b]9;Approval requested: allow codex to run cargo build --release\x07";
    let (first, second) = sequence.split_at(48);
    for chunk in [first, second] {
        push_output(&mut watcher, &terminal_id, chunk);
    }

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Waiting);
    assert_eq!(status.source, "terminal-notification-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn oversized_unterminated_osc_is_dropped_and_parser_recovers() {
    let (dir, bridge, terminal_id, mut watcher) = watcher_fixture("terminal-osc-oversized");

    let mut oversized = b"\x1b]9;".to_vec();
    oversized.extend(std::iter::repeat(b'x').take(2048));
    push_output(&mut watcher, &terminal_id, &oversized);
    push_output(&mut watcher, &terminal_id, b"\x1b]9;4;3\x07");

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);
    assert_eq!(status.source, "terminal-progress-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_command_osc_drives_working_then_clears() {
    let (dir, bridge, terminal_id, mut watcher) = watcher_fixture("terminal-osc-command");

    // A (prompt) is ignored, C starts the command, D;exit ends it.
    push_output(&mut watcher, &terminal_id, b"\x1b]133;A\x07\x1b]133;C\x07");
    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);
    assert_eq!(status.source, "terminal-command-osc");

    push_output(&mut watcher, &terminal_id, b"\x1b]133;D;0\x07\x1b]133;A\x07");
    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Idle);
    assert_eq!(status.source, "terminal-command-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_command_osc_survives_chunk_split() {
    let (dir, bridge, terminal_id, mut watcher) = watcher_fixture("terminal-osc-command-split");

    // Split inside the "133;" prefix so the tail must carry the partial OSC.
    for chunk in [b"\x1b]13".as_slice(), b"3;C\x07".as_slice()] {
        push_output(&mut watcher, &terminal_id, chunk);
    }

    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);
    assert_eq!(status.source, "terminal-command-osc");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_title_spinner_drives_turn_level_status() {
    let (dir, bridge, terminal_id, mut watcher) = watcher_fixture("terminal-osc-title");

    // Plain startup title emits nothing; the braille spinner starts the turn.
    push_output(&mut watcher, &terminal_id, "\x1b]0;Data\x07".as_bytes());
    push_output(
        &mut watcher,
        &terminal_id,
        "\x1b]0;⠋ | Data\x07\x1b]0;⠙ | Data\x07".as_bytes(),
    );
    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);
    assert_eq!(status.source, "terminal-title-osc");

    // Blocked on approval: both blink phases map to one Waiting.
    push_output(
        &mut watcher,
        &terminal_id,
        "\x1b]0;[ ! ] Action Required | Data\x07\x1b]0;[ . ] Action Required | Data\x07"
            .as_bytes(),
    );
    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Waiting);
    assert_eq!(status.source, "terminal-title-osc");

    // Approved: spinner returns, then the turn finishes back to a plain title.
    push_output(&mut watcher, &terminal_id, "\x1b]0;⠹ | Data\x07".as_bytes());
    wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);
    push_output(&mut watcher, &terminal_id, "\x1b]0;Data\x07".as_bytes());
    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Completed);
    assert_eq!(status.source, "terminal-title-osc");

    // Reordered title items: the spinner counts anywhere, not just leading.
    push_output(&mut watcher, &terminal_id, "\x1b]0;Data | ⠼\x07".as_bytes());
    wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Working);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn dismissed_action_required_title_clears_instead_of_completing() {
    let (dir, bridge, terminal_id, mut watcher) = watcher_fixture("terminal-osc-title-dismiss");

    push_output(
        &mut watcher,
        &terminal_id,
        "\x1b]0;[ ! ] Action Required | Data\x07".as_bytes(),
    );
    wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Waiting);

    push_output(&mut watcher, &terminal_id, "\x1b]0;Data\x07".as_bytes());
    let status = wait_for_terminal_status(&bridge, &terminal_id, TerminalStatusState::Idle);
    assert_eq!(status.source, "terminal-title-osc");

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
