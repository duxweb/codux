use super::*;

#[cfg(unix)]
#[test]
fn terminal_manager_reuses_session_and_broadcasts_to_subscribers() {
    let manager = TerminalManager::new();
    let emit: EventSink = Arc::new(|_| true);
    let config = TerminalPtyConfig {
        terminal_id: Some(format!("test-terminal-{}", Uuid::new_v4())),
        shell: Some("/bin/cat".to_string()),
        cols: Some(80),
        rows: Some(24),
        scrollback_lines: Some(100),
        ..Default::default()
    };

    let (first_session, first_rx) = manager
        .attach_or_create_with_context(config.clone(), None, emit.clone())
        .expect("terminal should start");
    first_session
        .write(b"first-shared-output\n")
        .expect("write should succeed");
    assert!(
        recv_until_contains(&first_rx, "first-shared-output", Duration::from_secs(2))
            .contains("first-shared-output")
    );

    let (second_session, second_rx) = manager
        .attach_or_create_with_context(config, None, emit)
        .expect("terminal should attach");
    assert!(Arc::ptr_eq(&first_session, &second_session));

    first_session
        .write(b"second-shared-output\n")
        .expect("write should succeed");
    assert!(
        recv_until_contains(&first_rx, "second-shared-output", Duration::from_secs(2))
            .contains("second-shared-output")
    );
    assert!(
        recv_until_contains(&second_rx, "second-shared-output", Duration::from_secs(2))
            .contains("second-shared-output")
    );

    let _ = first_session.kill();
}
#[cfg(unix)]
#[test]
fn reattach_appends_keyframe_only_for_alt_screen_session() {
    let manager = TerminalManager::new();
    let emit: EventSink = Arc::new(|_| true);
    let config = TerminalPtyConfig {
        terminal_id: Some(format!("test-altscreen-{}", Uuid::new_v4())),
        shell: Some("/bin/cat".to_string()),
        cols: Some(80),
        rows: Some(24),
        scrollback_lines: Some(100),
        ..Default::default()
    };

    let (session, first_rx) = manager
        .attach_or_create_with_context(config.clone(), None, emit.clone())
        .expect("terminal should start");

    // Normal screen: a re-attach replays only the raw history; it never
    // appends the keyframe (identified by its cursor-hide repaint prefix).
    session
        .write(b"normal-line\n")
        .expect("write should succeed");
    assert!(
        recv_until_contains(&first_rx, "normal-line", Duration::from_secs(2))
            .contains("normal-line")
    );
    let (_normal_session, normal_rx) = manager
        .attach_or_create_with_context(config.clone(), None, emit.clone())
        .expect("terminal should attach");
    let normal_replay = recv_until_contains(&normal_rx, "normal-line", Duration::from_secs(2));
    assert!(normal_replay.contains("normal-line"));
    assert!(!normal_replay.contains("\x1b[?25l"));

    // Enter the alternate screen and let it apply to the live screen.
    session
        .write(b"\x1b[?1049h\x1b[2J\x1b[HALT_SCREEN_MARKER\n")
        .expect("write should succeed");
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && !session.screen_snapshot().input_mode.alternate_screen {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(session.screen_snapshot().input_mode.alternate_screen);

    // Alt screen: the re-attach replay now carries the live keyframe, so the
    // current screen and its alt-screen mode are reconstructed even though
    // the alternate buffer never reached the raw history.
    let (_alt_session, alt_rx) = manager
        .attach_or_create_with_context(config, None, emit)
        .expect("terminal should attach");
    let alt_replay = recv_until_contains(&alt_rx, "\x1b[?25l", Duration::from_secs(2));
    assert!(alt_replay.contains("\x1b[?25l"));
    assert!(alt_replay.contains("\x1b[?1049h"));

    let _ = session.kill();
}
#[cfg(unix)]
#[test]
fn terminal_manager_ensures_session_before_ui_attach() {
    let manager = TerminalManager::new();
    let terminal_id = format!("test-prewarm-terminal-{}", Uuid::new_v4());
    let config = TerminalPtyConfig {
        terminal_id: Some(terminal_id.clone()),
        shell: Some("/bin/cat".to_string()),
        cols: Some(80),
        rows: Some(24),
        scrollback_lines: Some(100),
        ..Default::default()
    };

    let ensured_id = manager
        .ensure_session_with_context(config.clone(), None)
        .expect("terminal should prewarm");
    assert_eq!(ensured_id, terminal_id);

    let emit: EventSink = Arc::new(|_| true);
    let (session, rx) = manager
        .attach_or_create_with_context(config, None, emit)
        .expect("terminal should attach");
    assert_eq!(session.id(), ensured_id);
    session
        .write(b"prewarm-shared-output\n")
        .expect("write should succeed");
    assert!(
        recv_until_contains(&rx, "prewarm-shared-output", Duration::from_secs(2))
            .contains("prewarm-shared-output")
    );

    let _ = session.kill();
}

#[cfg(unix)]
#[test]
fn terminal_manager_kill_and_wait_waits_for_exit() {
    let manager = TerminalManager::new();
    let terminal_id = format!("test-kill-wait-terminal-{}", Uuid::new_v4());
    let config = TerminalPtyConfig {
        terminal_id: Some(terminal_id.clone()),
        shell: Some("/bin/cat".to_string()),
        cols: Some(80),
        rows: Some(24),
        scrollback_lines: Some(100),
        ..Default::default()
    };

    manager
        .ensure_session_with_context(config, None)
        .expect("terminal should start");
    manager
        .kill_and_wait(&terminal_id, Duration::from_secs(2))
        .expect("terminal should exit after kill");

    assert!(
        manager.session(&terminal_id).is_err(),
        "killed session should be removed from the manager"
    );
}

#[cfg(windows)]
#[test]
fn terminal_manager_kill_and_wait_releases_spawn_cwd() {
    let manager = TerminalManager::new();
    let terminal_id = format!("test-kill-cwd-terminal-{}", Uuid::new_v4());
    let cwd = std::env::temp_dir().join(format!("codux-pty-cwd-{}", Uuid::new_v4()));
    fs::create_dir_all(&cwd).expect("create cwd");
    let config = TerminalPtyConfig {
        terminal_id: Some(terminal_id.clone()),
        cwd: Some(cwd.display().to_string()),
        cols: Some(80),
        rows: Some(24),
        scrollback_lines: Some(100),
        ..Default::default()
    };

    manager
        .ensure_session_with_context(config, None)
        .expect("terminal should start in cwd");
    manager
        .kill_and_wait(&terminal_id, Duration::from_secs(5))
        .expect("terminal should exit after kill");

    fs::remove_dir_all(&cwd).expect("spawn cwd should be removable after terminal exit");
}

#[cfg(unix)]
#[test]
fn terminal_manager_replaces_same_terminal_id_when_identity_changes() {
    let manager = TerminalManager::new();
    let emit: EventSink = Arc::new(|_| true);
    let terminal_id = format!("test-scoped-terminal-{}", Uuid::new_v4());
    let first_cwd = std::env::temp_dir().join(format!("codux-pty-first-{}", Uuid::new_v4()));
    let second_cwd = std::env::temp_dir().join(format!("codux-pty-second-{}", Uuid::new_v4()));
    fs::create_dir_all(&first_cwd).unwrap();
    fs::create_dir_all(&second_cwd).unwrap();

    let first_config = TerminalPtyConfig {
        terminal_id: Some(terminal_id.clone()),
        shell: Some("/bin/cat".to_string()),
        cwd: Some(first_cwd.display().to_string()),
        project_id: Some("worktree-a".to_string()),
        session_key: Some(format!("gpui:worktree-a:{terminal_id}")),
        cols: Some(80),
        rows: Some(24),
        scrollback_lines: Some(100),
        ..Default::default()
    };
    let second_config = TerminalPtyConfig {
        cwd: Some(second_cwd.display().to_string()),
        project_id: Some("worktree-b".to_string()),
        session_key: Some(format!("gpui:worktree-b:{terminal_id}")),
        ..first_config.clone()
    };

    let (first_session, _) = manager
        .attach_or_create_with_context(first_config, None, emit.clone())
        .expect("first terminal should start");
    assert_eq!(first_session.info().cwd, first_cwd.display().to_string());
    assert_eq!(first_session.info().project_id, "worktree-a");

    let (second_session, _) = manager
        .attach_or_create_with_context(second_config, None, emit)
        .expect("second terminal should replace incompatible session");
    assert!(!Arc::ptr_eq(&first_session, &second_session));
    assert_eq!(second_session.id(), terminal_id);
    assert_eq!(second_session.info().cwd, second_cwd.display().to_string());
    assert_eq!(second_session.info().project_id, "worktree-b");

    let _ = second_session.kill();
    let _ = fs::remove_dir_all(first_cwd);
    let _ = fs::remove_dir_all(second_cwd);
}

#[cfg(unix)]
#[test]
fn terminal_manager_uses_context_session_cwd_for_identity() {
    let manager = TerminalManager::new();
    let emit: EventSink = Arc::new(|_| true);
    let terminal_id = format!("test-context-cwd-terminal-{}", Uuid::new_v4());
    let project_cwd = std::env::temp_dir().join(format!("codux-project-{}", Uuid::new_v4()));
    let worktree_cwd = std::env::temp_dir().join(format!("codux-worktree-{}", Uuid::new_v4()));
    fs::create_dir_all(&project_cwd).unwrap();
    fs::create_dir_all(&worktree_cwd).unwrap();
    let context = TerminalLaunchContext {
        root_project_id: "project-1".to_string(),
        project_id: "worktree-context".to_string(),
        project_name: "Context Worktree".to_string(),
        project_path: project_cwd.clone(),
        support_dir: std::env::temp_dir(),
        runtime_root: std::env::temp_dir(),
        terminal_id: Some(terminal_id.clone()),
        slot_id: None,
        session_key: Some(format!("gpui:worktree-context:{terminal_id}")),
        session_title: None,
        session_cwd: Some(worktree_cwd.clone()),
        session_instance_id: None,
        tool_permissions_file: None,
        memory_workspace_root: None,
        memory_prompt_file: None,
        memory_index_file: None,
        host_device_id: None,
    };
    let config = TerminalPtyConfig {
        terminal_id: Some(terminal_id),
        shell: Some("/bin/cat".to_string()),
        cols: Some(80),
        rows: Some(24),
        scrollback_lines: Some(100),
        ..Default::default()
    };

    let (session, _) = manager
        .attach_or_create_with_context(config, Some(&context), emit)
        .expect("terminal should use context session cwd");

    assert_eq!(session.info().cwd, worktree_cwd.display().to_string());
    assert_eq!(session.info().project_id, "worktree-context");

    let _ = session.kill();
    let _ = fs::remove_dir_all(project_cwd);
    let _ = fs::remove_dir_all(worktree_cwd);
}

#[test]
fn terminal_event_subscribers_are_pruned_when_sink_is_closed() {
    let subscribers: Arc<parking_lot::Mutex<Vec<EventSubscriber>>> =
        Arc::new(parking_lot::Mutex::new(Vec::new()));
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    subscribers
        .lock()
        .push(EventSubscriber::anonymous(Arc::new(move |_| {
            tx.send(()).is_ok()
        })));

    emit_terminal_event(
        &subscribers,
        TerminalEvent::Exit {
            session_id: "session-a".to_string(),
            exit_code: None,
        },
    );
    assert_eq!(subscribers.lock().len(), 1);
    drop(rx);

    emit_terminal_event(
        &subscribers,
        TerminalEvent::Exit {
            session_id: "session-a".to_string(),
            exit_code: None,
        },
    );
    assert!(subscribers.lock().is_empty());
}

#[cfg(unix)]
#[test]
fn exit_event_observes_session_as_exited() {
    let manager = Arc::new(TerminalManager::new());
    let (tx, rx) = std::sync::mpsc::channel();
    let manager_for_event = Arc::clone(&manager);
    let session_id = manager
        .create(
            TerminalPtyConfig {
                command: Some("exit 0".to_string()),
                ..Default::default()
            },
            move |event| {
                if matches!(event, TerminalEvent::Exit { .. }) {
                    let _ = tx.send(manager_for_event.list());
                }
            },
        )
        .expect("create terminal");

    let terminals = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("exit event");
    let terminal = terminals
        .into_iter()
        .find(|terminal| terminal.id == session_id)
        .expect("exited terminal remains queryable");
    assert_eq!(terminal.status, "exited");
    assert!(!terminal.is_running);
}

#[cfg(unix)]
#[test]
fn terminal_manager_recreates_exited_session_with_same_id() {
    let manager = TerminalManager::new();
    let terminal_id = format!("test-restart-terminal-{}", Uuid::new_v4());
    let first = manager
        .attach_or_create_with_context(
            TerminalPtyConfig {
                terminal_id: Some(terminal_id.clone()),
                shell: Some("/bin/sh".to_string()),
                command: Some("exit 0".to_string()),
                ..Default::default()
            },
            None,
            Arc::new(|_| true),
        )
        .expect("first terminal should start")
        .0;

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && !first.has_exited() {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(first.has_exited());

    let second = manager
        .attach_or_create_with_context(
            TerminalPtyConfig {
                terminal_id: Some(terminal_id.clone()),
                shell: Some("/bin/cat".to_string()),
                ..Default::default()
            },
            None,
            Arc::new(|_| true),
        )
        .expect("exited terminal should restart")
        .0;

    assert_eq!(second.id(), terminal_id);
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(second.info().is_running);
    let _ = second.kill();
}

#[test]
fn keyed_terminal_event_subscribers_replace_stale_sinks() {
    let subscribers: Arc<parking_lot::Mutex<Vec<EventSubscriber>>> =
        Arc::new(parking_lot::Mutex::new(Vec::new()));
    let anonymous_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let stale_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let latest_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let other_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    {
        let anonymous_hits = anonymous_hits.clone();
        subscribers
            .lock()
            .push(EventSubscriber::anonymous(Arc::new(move |_| {
                anonymous_hits.fetch_add(1, Ordering::SeqCst);
                true
            })));
    }
    {
        let stale_hits = stale_hits.clone();
        insert_keyed_event_subscriber(
            &subscribers,
            "remote-terminal:session-1".to_string(),
            Arc::new(move |_| {
                stale_hits.fetch_add(1, Ordering::SeqCst);
                true
            }),
        );
    }
    {
        let latest_hits = latest_hits.clone();
        insert_keyed_event_subscriber(
            &subscribers,
            "remote-terminal:session-1".to_string(),
            Arc::new(move |_| {
                latest_hits.fetch_add(1, Ordering::SeqCst);
                true
            }),
        );
    }
    {
        let other_hits = other_hits.clone();
        insert_keyed_event_subscriber(
            &subscribers,
            "remote-terminal:session-2".to_string(),
            Arc::new(move |_| {
                other_hits.fetch_add(1, Ordering::SeqCst);
                true
            }),
        );
    }

    emit_terminal_event(
        &subscribers,
        TerminalEvent::Output {
            session_id: "session-1".to_string(),
            text: "hello".to_string(),
            bytes: b"hello".to_vec(),
        },
    );

    assert_eq!(anonymous_hits.load(Ordering::SeqCst), 1);
    assert_eq!(stale_hits.load(Ordering::SeqCst), 0);
    assert_eq!(latest_hits.load(Ordering::SeqCst), 1);
    assert_eq!(other_hits.load(Ordering::SeqCst), 1);
    assert_eq!(subscribers.lock().len(), 3);
}

#[cfg(unix)]
#[test]
fn terminal_manager_registers_ai_runtime_terminal_lifecycle() {
    let dir = std::env::temp_dir().join(format!("codux-terminal-bridge-{}", Uuid::new_v4()));
    let bridge = Arc::new(AIRuntimeBridge::with_paths(
        dir.join("root"),
        dir.join("temp"),
        dir.join("home"),
    ));
    let manager = TerminalManager::with_ai_runtime(Arc::clone(&bridge));
    let terminal_id = format!("test-ai-terminal-{}", Uuid::new_v4());
    let config = TerminalPtyConfig {
        terminal_id: Some(terminal_id.clone()),
        root_project_id: Some("project-1".to_string()),
        project_id: Some("worktree-1".to_string()),
        worktree_id: Some("worktree-1".to_string()),
        slot_id: Some("slot-1".to_string()),
        session_key: Some("session-key-1".to_string()),
        title: Some("Codex".to_string()),
        tool: Some("codex".to_string()),
        shell: Some("/bin/cat".to_string()),
        cols: Some(80),
        rows: Some(24),
        scrollback_lines: Some(100),
        ..Default::default()
    };
    let emit: EventSink = Arc::new(|_| true);

    let (session, _) = manager
        .attach_or_create_with_context(config, None, emit)
        .expect("terminal should start");

    let terminals = bridge.registry().snapshot();
    assert_eq!(terminals.len(), 1);
    assert_eq!(terminals[0].terminal_id, terminal_id);
    assert_eq!(terminals[0].project_id, "worktree-1");
    assert_eq!(terminals[0].slot_id, "slot-1");
    assert_eq!(terminals[0].tool.as_deref(), Some("codex"));

    manager.kill(session.id()).expect("terminal should stop");
    assert!(bridge.registry().snapshot().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
