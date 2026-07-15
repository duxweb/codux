use super::*;

#[test]
fn terminal_baseline_viewport_does_not_steal_from_other_owner() {
    let support_dir = temp_support_dir("codux-remote-terminal-baseline-no-steal");
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
                command: Some("printf 'desktop owned'".to_string()),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");
    terminals
        .claim_viewport(&session_id, "desktop")
        .expect("desktop owns viewport");

    runtime.send_terminal_buffer(
        &session_id,
        Some("phone-a"),
        0,
        viewport_buffer_options(128, Some("request-1"), true, 72, 18),
    );

    let state = terminals
        .viewport_state(&session_id)
        .expect("viewport state");
    assert_eq!(state.owner, "desktop");
    assert_eq!(state.cols, 100);
    assert_eq!(state.rows, 32);

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
    // The non-owner still gets a host-grid keyframe (viewers render the owner's
    // grid 1:1); it just must not steal the lease or resize the PTY.
    assert!(
        envelope
            .payload
            .get("screenData")
            .and_then(Value::as_str)
            .is_some_and(|data| !data.is_empty())
    );

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn session_terminal_baseline_auto_claims_and_targets_only_subscribed_split() {
    let support_dir = temp_support_dir("codux-remote-project-baseline-active-viewport");
    write_paired_remote_settings(&support_dir);
    let project_dir = support_dir.join("project-a");
    fs::create_dir_all(&project_dir).expect("create project dir");
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
    let session_a = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf '\\033[2J\\033[Hactive split'; sleep 30".to_string()),
                cwd: Some(project_dir.to_string_lossy().to_string()),
                project_id: Some("project-a".to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal a");
    let session_b = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf '\\033[2J\\033[Hbackground split'; sleep 30".to_string()),
                cwd: Some(project_dir.to_string_lossy().to_string()),
                project_id: Some("project-a".to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal b");

    for _ in 0..20 {
        let ready_a = terminals
            .screen_snapshot(&session_a)
            .map(|snapshot| snapshot.data.contains("active split"))
            .unwrap_or(false);
        let ready_b = terminals
            .screen_snapshot(&session_b)
            .map(|snapshot| snapshot.data.contains("background split"))
            .unwrap_or(false);
        if ready_a && ready_b {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    transport.take_messages();
    runtime.handle_resource_subscribe(&RemoteEnvelope {
        kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
        device_id: Some("phone-a".to_string()),
        session_id: Some(session_a.clone()),
        request_id: None,
        seq: None,
        payload: json!({
            "resource": REMOTE_RESOURCE_TERMINALS,
            "baseline": true,
            "viewportCols": 72,
            "viewportRows": 18,
        }),
    });

    let mut active_baseline = None;
    let mut background_baseline = None;
    for _ in 0..40 {
        for (device_id, data) in transport.take_messages() {
            if device_id.as_deref() != Some("phone-a") {
                continue;
            }
            let text = String::from_utf8(data).expect("utf8 transport");
            let envelope = runtime
                .service()
                .parse_incoming_envelope(&text)
                .expect("parse outgoing envelope");
            if envelope.kind != REMOTE_TERMINAL_OUTPUT
                || envelope.payload.get("buffer").and_then(Value::as_bool) != Some(true)
            {
                continue;
            }
            if envelope.session_id.as_deref() == Some(session_a.as_str()) {
                active_baseline = Some(envelope.payload);
            } else if envelope.session_id.as_deref() == Some(session_b.as_str()) {
                background_baseline = Some(envelope.payload);
            }
        }
        if active_baseline.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    assert!(
        active_baseline
            .as_ref()
            .and_then(|payload| payload.get("screenData"))
            .and_then(Value::as_str)
            .map(|screen_data| screen_data.contains("active split"))
            .unwrap_or(false),
        "active split should receive a target-viewport keyframe"
    );
    assert!(background_baseline.is_none());
    let active_state = terminals
        .viewport_state(&session_a)
        .expect("active viewport state");
    let background_state = terminals
        .viewport_state(&session_b)
        .expect("background viewport state");
    assert_eq!(active_state.owner, "remote:phone-a");
    assert_eq!(active_state.cols, 72);
    assert_eq!(active_state.rows, 18);
    assert_eq!(background_state.cols, 100);
    assert_eq!(background_state.rows, 32);

    terminals
        .kill_and_wait(&session_a, Duration::from_secs(2))
        .expect("stop terminal a");
    terminals
        .kill_and_wait(&session_b, Duration::from_secs(2))
        .expect("stop terminal b");

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn viewport_state_marks_stale_output_per_viewer() {
    let support_dir = temp_support_dir("codux-remote-viewport-state-per-viewer-stale");
    let project_dir = support_dir.join("project-a");
    fs::create_dir_all(&project_dir).expect("create project dir");
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
                cwd: Some(project_dir.to_string_lossy().to_string()),
                project_id: Some("project-a".to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");

    runtime.register_terminal_viewer(&session_id, Some("phone-a"));
    runtime.register_terminal_viewer(&session_id, Some("phone-b"));
    runtime.record_terminal_output_ack(&session_id, Some("phone-a"), Some(20));
    runtime.record_terminal_output_ack(&session_id, Some("phone-b"), Some(11));
    if let Ok(mut sequences) = runtime.terminal_output_seq_by_session.lock() {
        sequences.insert(session_id.clone(), 20);
    }
    transport.take_messages();
    runtime.handle_terminal_event(TerminalEvent::Viewport {
        session_id: session_id.clone(),
        owner: "desktop".to_string(),
        cols: 100,
        rows: 32,
        generation: 1,
    });

    let mut stale_by_device = HashMap::new();
    for (device_id, data) in transport.take_messages() {
        let text = String::from_utf8(data).expect("utf8 transport");
        let envelope = runtime
            .service()
            .parse_incoming_envelope(&text)
            .expect("parse outgoing envelope");
        if envelope.kind == REMOTE_TERMINAL_VIEWPORT_STATE {
            stale_by_device.insert(
                device_id.expect("device id"),
                envelope.payload["staleOutput"].as_bool().unwrap_or(false),
            );
        }
    }

    assert_eq!(stale_by_device.get("phone-a"), Some(&false));
    assert_eq!(stale_by_device.get("phone-b"), Some(&true));

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_viewport_auto_claim_respects_explicit_desktop_owner() {
    let support_dir = temp_support_dir("codux-remote-terminal-viewport-owner");
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

    runtime.handle_terminal_viewport_claim(&RemoteEnvelope {
        kind: "terminal.viewport.claim".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({ "intent": "auto" }),
    });
    runtime.handle_terminal_viewport_resize(&RemoteEnvelope {
        kind: "terminal.viewport.resize".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({
            "cols": 72,
            "rows": 18,
        }),
    });

    let state = terminals
        .viewport_state(&session_id)
        .expect("viewport state");
    assert_eq!(state.owner, "remote:device-1");
    assert_eq!(state.cols, 72);
    assert_eq!(state.rows, 18);

    terminals
        .claim_viewport(&session_id, "desktop")
        .expect("desktop claim");
    let accepted = terminals
        .resize_viewport(&session_id, "desktop", 100, 32)
        .expect("desktop resize")
        .expect("accepted desktop resize");
    assert_eq!(accepted.owner, "desktop");
    assert_eq!(accepted.cols, 100);
    assert_eq!(accepted.rows, 32);

    runtime.handle_terminal_viewport_resize(&RemoteEnvelope {
        kind: "terminal.viewport.resize".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({
            "cols": 72,
            "rows": 18,
        }),
    });
    let state = terminals
        .viewport_state(&session_id)
        .expect("viewport state");
    assert_eq!(state.owner, "desktop");
    assert_eq!(state.cols, 100);
    assert_eq!(state.rows, 32);

    runtime.handle_terminal_viewport_claim(&RemoteEnvelope {
        kind: "terminal.viewport.claim".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({ "intent": "auto" }),
    });
    assert_eq!(
        terminals
            .viewport_state(&session_id)
            .expect("viewport state after automatic claim")
            .owner,
        "desktop"
    );

    runtime.handle_terminal_viewport_claim(&RemoteEnvelope {
        kind: "terminal.viewport.claim".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({ "intent": "force" }),
    });
    runtime.handle_terminal_viewport_resize(&RemoteEnvelope {
        kind: "terminal.viewport.resize".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({
            "cols": 72,
            "rows": 18,
        }),
    });
    let state = terminals
        .viewport_state(&session_id)
        .expect("viewport state after forced claim");
    assert_eq!(state.owner, "remote:device-1");
    assert_eq!(state.cols, 72);
    assert_eq!(state.rows, 18);

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_viewport_resize_pushes_state_without_screen_keyframe() {
    let support_dir = temp_support_dir("codux-remote-terminal-viewport-keyframe");
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
    runtime.handle_terminal_viewport_claim(&RemoteEnvelope {
        kind: "terminal.viewport.claim".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({}),
    });
    transport.take_messages();
    runtime.handle_terminal_viewport_resize(&RemoteEnvelope {
        kind: "terminal.viewport.resize".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({
            "cols": 72,
            "rows": 18,
        }),
    });

    let mut saw_state = false;
    let mut keyframe = None;
    for (device_id, data) in transport.take_messages() {
        let text = String::from_utf8(data).expect("utf8 transport");
        let envelope = runtime
            .service()
            .parse_incoming_envelope(&text)
            .expect("parse outgoing envelope");
        match envelope.kind.as_str() {
            REMOTE_TERMINAL_VIEWPORT_STATE
                if device_id.as_deref() == Some("device-1")
                    && envelope.session_id.as_deref() == Some(&session_id) =>
            {
                saw_state = true
            }
            REMOTE_TERMINAL_OUTPUT => {
                if device_id.as_deref() == Some("device-1")
                    && envelope.session_id.as_deref() == Some(&session_id)
                {
                    keyframe = Some(envelope.payload);
                }
            }
            _ => {}
        }
    }

    assert!(saw_state, "resize must still push viewport state");
    // No screen keyframe: the desktop emulator handles resize via the shell's
    // own repaint in the live byte stream (like a local terminal). Pushing a
    // whole-screen keyframe duplicated the screen on every resize event.
    assert!(
        keyframe.is_none(),
        "resize must not push a screen keyframe (it duplicated on resize)"
    );

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_input_reclaims_viewport_after_lease_expired_to_host() {
    let support_dir = temp_support_dir("codux-remote-terminal-input-reclaim");
    write_paired_remote_settings(&support_dir);
    let terminals = Arc::new(TerminalManager::new());
    let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    ));
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(Arc::new(CapturingTransport::default()));
    }
    let session_id = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("cat".to_string()),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                cols: Some(100),
                rows: Some(32),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");
    terminals
        .claim_viewport(&session_id, "remote:device-1")
        .expect("remote claim");
    let expired = terminals
        .expire_viewport_lease_for_test(&session_id)
        .expect("expire viewport lease")
        .expect("expired viewport state");
    assert_eq!(expired.owner, "desktop");

    // Nobody is driving: the first remote input re-claims and is accepted.
    runtime.handle_terminal_input(&RemoteEnvelope {
        kind: "terminal.input".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({ "data": "x" }),
    });
    let state = terminals
        .viewport_state(&session_id)
        .expect("viewport state");
    assert_eq!(state.owner, "remote:device-1");

    // A different device is still rejected while the lease is live.
    runtime.handle_terminal_input(&RemoteEnvelope {
        kind: "terminal.input".to_string(),
        device_id: Some("device-2".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({ "data": "y" }),
    });
    let state = terminals
        .viewport_state(&session_id)
        .expect("viewport state");
    assert_eq!(state.owner, "remote:device-1");

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_resize_without_owner_claims_remote_viewport_for_compatibility() {
    let support_dir = temp_support_dir("codux-remote-terminal-resize-without-owner");
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

    runtime.handle_terminal_resize(&RemoteEnvelope {
        kind: "terminal.resize".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({
            "cols": 80,
            "rows": 24,
        }),
    });

    let state = terminals
        .viewport_state(&session_id)
        .expect("viewport state");
    assert_eq!(state.owner, "remote:device-1");
    assert_eq!(state.cols, 80);
    assert_eq!(state.rows, 24);

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_resize_without_dimensions_is_rejected() {
    let support_dir = temp_support_dir("codux-remote-terminal-resize-reject");
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

    runtime.handle_terminal_resize(&RemoteEnvelope {
        kind: "terminal.resize".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: Some(session_id.clone()),
        request_id: None,
        seq: None,
        payload: json!({}),
    });

    let messages = transport.take_messages();
    assert_eq!(messages.len(), 1);
    let envelope: RemoteEnvelope = serde_json::from_slice(&messages[0].1).expect("error envelope");
    assert_eq!(envelope.kind, REMOTE_ERROR);
    assert_eq!(
        envelope.payload["message"],
        "terminal.resize requires positive cols."
    );
    let state = terminals
        .viewport_state(&session_id)
        .expect("viewport state");
    assert_ne!(state.owner, "remote:device-1");

    fs::remove_dir_all(support_dir).ok();
}
