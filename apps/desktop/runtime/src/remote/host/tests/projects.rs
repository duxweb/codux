use super::*;

#[test]
fn remote_project_select_keeps_desktop_selected_project() {
    let support_dir = temp_support_dir("codux-remote-scope-select");
    write_two_project_state(&support_dir);
    let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));

    runtime.handle_project_select(&RemoteEnvelope {
        kind: "project.select".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({ "projectId": "project-b" }),
    });

    let state = fs::read_to_string(support_dir.join("state.json")).expect("read state");
    let state: Value = serde_json::from_str(&state).expect("parse state");
    assert_eq!(state["selectedProjectId"], "project-a");
    assert_eq!(
        runtime.remote_project_scope_id(Some("device-1")).as_deref(),
        Some("project-b")
    );

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn secure_project_select_keeps_decrypted_device_id_for_scope_and_replies() {
    let support_dir = temp_support_dir("codux-remote-secure-scope-select");
    write_paired_remote_settings(&support_dir);
    write_two_project_state(&support_dir);
    let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }
    let encrypted = {
        let mut send_seq = HashMap::new();
        runtime
            .service()
            .outgoing_transport_text(
                "project.select",
                Some("device-1"),
                None,
                None,
                json!({ "projectId": "project-b" }),
                &mut send_seq,
            )
            .expect("secure envelope")
            .into_bytes()
    };

    Arc::clone(&runtime).handle_transport_message("relay-device".to_string(), encrypted);

    assert_eq!(
        runtime.remote_project_scope_id(Some("device-1")).as_deref(),
        Some("project-b")
    );
    assert_eq!(runtime.remote_project_scope_id(Some("relay-device")), None);
    let replies = transport.take_messages();
    assert!(
        replies
            .iter()
            .any(|(device_id, _)| device_id.as_deref() == Some("device-1")),
        "expected reply to decrypted device id"
    );
    assert!(
        replies
            .iter()
            .all(|(device_id, _)| device_id.as_deref() != Some("relay-device")),
        "must not reply to transport device id"
    );

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn remote_project_list_reports_device_selected_project_scope() {
    let support_dir = temp_support_dir("codux-remote-project-list-scope");
    write_two_project_state(&support_dir);
    let runtime = RemoteHostRuntime::new(support_dir.clone());
    runtime.set_remote_project_scope(Some("device-1"), "project-b");

    let payload = runtime.remote_project_list_payload(Some("device-1"));

    assert_eq!(payload["selectedProjectId"], "project-b");
    assert!(payload["selectedWorktreeId"].is_null());
    assert_eq!(
        payload["projects"]
            .as_array()
            .expect("projects")
            .iter()
            .filter_map(|project| project.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec!["project-a", "project-b"],
    );

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn remote_project_select_starts_project_terminal_on_host() {
    let support_dir = temp_support_dir("codux-remote-project-terminal");
    let (_, project_b) = write_two_project_state(&support_dir);
    let worktree_b_path = support_dir.join("project-b-worktree");
    fs::create_dir_all(&worktree_b_path).expect("create worktree b");
    let mut state: Value = serde_json::from_str(
        &fs::read_to_string(support_dir.join("state.json")).expect("read state"),
    )
    .expect("parse state");
    state["worktrees"][0]["path"] = json!(worktree_b_path.to_string_lossy());
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&state).expect("serialize state"),
    )
    .expect("write state");
    let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));

    runtime.handle_project_select(&RemoteEnvelope {
        kind: "project.select".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({ "projectId": "project-b", "worktreeId": "worktree-b" }),
    });

    let terminals = runtime.remote_terminals();
    let project_terminal = terminals
        .iter()
        .find(|terminal| terminal.get("projectId").and_then(Value::as_str) == Some("project-b"))
        .expect("project terminal");
    let session_id = project_terminal
        .get("id")
        .and_then(Value::as_str)
        .expect("session id");
    assert!(!session_id.trim().is_empty());

    let layout_key = terminal_layout_storage_key("project-b", "worktree-b");
    let layout = TerminalLayoutService::new(support_dir.clone()).load(Some(&layout_key));
    assert_eq!(layout.top_panes.len(), 1);
    assert_eq!(layout.top_panes[0].terminal_id, session_id);
    let session = runtime
        .terminals
        .session(session_id)
        .expect("terminal session");
    let expected_session_key = format!("gpui:worktree-b:{session_id}");
    assert_eq!(session.info().project_id, "worktree-b");
    assert_eq!(
        session.info().cwd,
        worktree_b_path.to_string_lossy().as_ref()
    );
    assert_eq!(
        session.info().session_key.as_deref(),
        Some(expected_session_key.as_str())
    );
    assert_eq!(project_terminal["projectId"], "project-b");
    assert_eq!(project_terminal["worktreeId"], "worktree-b");
    assert_eq!(
        project_terminal["cwd"].as_str(),
        Some(worktree_b_path.to_string_lossy().as_ref())
    );
    assert_ne!(
        project_b.to_string_lossy(),
        worktree_b_path.to_string_lossy()
    );
    assert!(
        runtime
            .drain_events()
            .iter()
            .any(|event| matches!(event, RemoteHostEvent::TerminalLayoutChanged(_)))
    );

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn remote_worktree_select_is_device_scoped_and_does_not_mutate_desktop_selection() {
    let support_dir = temp_support_dir("codux-remote-worktree-device-scope");
    let (_, project_b) = write_two_project_state(&support_dir);
    let mut state: Value = serde_json::from_str(
        &fs::read_to_string(support_dir.join("state.json")).expect("read state"),
    )
    .expect("parse state");
    state["worktrees"]
        .as_array_mut()
        .expect("worktrees")
        .push(json!({
            "id": "worktree-c",
            "projectId": "project-b",
            "name": "Task C",
            "branch": "task-c",
            "path": project_b.to_string_lossy(),
            "status": "active",
            "isDefault": false,
            "createdAt": 2,
            "updatedAt": 2
        }));
    state["selectedWorktreeIdByProject"]["project-b"] = json!("worktree-c");
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&state).expect("serialize state"),
    )
    .expect("write state");
    let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));

    runtime.handle_worktree_select(&RemoteEnvelope {
        kind: "worktree.select".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({
            "projectId": "project-b",
            "worktreeId": "worktree-b",
        }),
    });

    let state = fs::read_to_string(support_dir.join("state.json")).expect("read state");
    let state: Value = serde_json::from_str(&state).expect("parse state");
    assert_eq!(state["selectedProjectId"], "project-a");
    assert_eq!(
        state["selectedWorktreeIdByProject"]["project-b"],
        "worktree-c"
    );
    assert_eq!(
        runtime.remote_project_scope_id(Some("device-1")).as_deref(),
        Some("project-b")
    );
    assert!(runtime.remote_terminals().iter().any(|terminal| {
        terminal.get("projectId").and_then(Value::as_str) == Some("project-b")
            && terminal.get("worktreeId").and_then(Value::as_str) == Some("worktree-b")
    }));

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn remote_worktree_select_replaces_saved_terminal_with_wrong_cwd() {
    let support_dir = temp_support_dir("codux-remote-worktree-wrong-cwd");
    let (_, project_b) = write_two_project_state(&support_dir);
    let worktree_b_path = support_dir.join("project-b-worktree");
    fs::create_dir_all(&worktree_b_path).expect("create worktree b");
    let mut state: Value = serde_json::from_str(
        &fs::read_to_string(support_dir.join("state.json")).expect("read state"),
    )
    .expect("parse state");
    state["worktrees"][0]["path"] = json!(worktree_b_path.to_string_lossy());
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&state).expect("serialize state"),
    )
    .expect("write state");
    let terminals = Arc::new(TerminalManager::new());
    let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    ));
    let stale_terminal_id = "terminal-stale-worktree-b";
    terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf stale".to_string()),
                cwd: Some(project_b.to_string_lossy().to_string()),
                project_id: Some("project-b".to_string()),
                terminal_id: Some(stale_terminal_id.to_string()),
                session_key: Some(format!("gpui:project-b:{stale_terminal_id}")),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create stale terminal");
    TerminalLayoutService::new(support_dir.clone())
        .save_from_gpui(
            &terminal_layout_storage_key("project-b", "worktree-b"),
            Vec::new(),
            vec![TerminalPaneSummary {
                title: "Stale".to_string(),
                terminal_id: stale_terminal_id.to_string(),
            }],
            vec![1.0],
            0.24,
        )
        .expect("save stale layout");

    runtime.handle_worktree_select(&RemoteEnvelope {
        kind: "worktree.select".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({
            "projectId": "project-b",
            "worktreeId": "worktree-b",
        }),
    });

    let session = runtime
        .terminals
        .session(stale_terminal_id)
        .expect("recreated terminal session");
    let info = session.info();
    let expected_session_key = format!("gpui:worktree-b:{stale_terminal_id}");
    assert_eq!(info.project_id, "worktree-b");
    assert_eq!(info.cwd, worktree_b_path.to_string_lossy().as_ref());
    assert_eq!(
        info.session_key.as_deref(),
        Some(expected_session_key.as_str())
    );

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn project_list_broadcast_preserves_per_device_project_scope() {
    let support_dir = temp_support_dir("codux-remote-project-list-subscriptions");
    write_two_project_state(&support_dir);
    let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));

    runtime
        .resource_subscriptions
        .subscribe_envelope(&RemoteEnvelope {
            kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
            device_id: Some("phone-a".to_string()),
            session_id: None,
            request_id: None,
            seq: None,
            payload: json!({ "resource": REMOTE_RESOURCE_PROJECTS }),
        })
        .unwrap();
    runtime
        .resource_subscriptions
        .subscribe_envelope(&RemoteEnvelope {
            kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
            device_id: Some("phone-b".to_string()),
            session_id: None,
            request_id: None,
            seq: None,
            payload: json!({ "resource": REMOTE_RESOURCE_PROJECTS }),
        })
        .unwrap();
    runtime.set_remote_project_scope(Some("phone-a"), "project-a");
    runtime.set_remote_project_scope(Some("phone-b"), "project-b");

    assert_eq!(
        runtime.remote_project_list_payload(Some("phone-a"))["selectedProjectId"],
        "project-a"
    );
    assert_eq!(
        runtime.remote_project_list_payload(Some("phone-b"))["selectedProjectId"],
        "project-b"
    );

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_project_subscribe_returns_list_without_buffer_baseline() {
    let support_dir = temp_support_dir("codux-remote-terminal-subscribe-baseline");
    let (project_a, _) = write_two_project_state(&support_dir);
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
                command: Some("printf baseline-data".to_string()),
                cwd: Some(project_a.to_string_lossy().to_string()),
                project_id: Some("project-a".to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");

    for _ in 0..20 {
        if terminals
            .snapshot(&session_id)
            .map(|snapshot| snapshot.contains("baseline-data"))
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    runtime.handle_terminal_subscribe(&RemoteEnvelope {
        kind: "terminal.subscribe".to_string(),
        device_id: Some("device-1".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({
            "scope": "project",
            "projectId": "project-a",
            "baseline": true,
            "maxChars": 64,
            "chunkChars": 16
        }),
    });

    let messages = transport.take_messages();
    assert!(messages.iter().any(|(_, data)| {
        let text = std::str::from_utf8(data).expect("utf8 transport");
        runtime
            .service()
            .parse_incoming_envelope(text)
            .is_ok_and(|envelope| envelope.kind == REMOTE_TERMINAL_LIST)
    }));
    assert!(messages.iter().all(|(_, data)| {
        let text = std::str::from_utf8(data).expect("utf8 transport");
        runtime
            .service()
            .parse_incoming_envelope(text)
            .is_ok_and(|envelope| envelope.kind != REMOTE_TERMINAL_OUTPUT)
    }));
    assert!(runtime.terminal_output_viewers(&session_id).is_empty());

    fs::remove_dir_all(support_dir).ok();
}
