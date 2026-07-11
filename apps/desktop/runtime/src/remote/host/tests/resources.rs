use super::*;

#[test]
fn terminal_project_subscriptions_do_not_create_session_viewers() {
    let support_dir = temp_support_dir("codux-remote-terminal-subscriptions");
    let (project_a, project_b) = write_two_project_state(&support_dir);
    let terminals = Arc::new(TerminalManager::new());
    let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    ));
    let session_a = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf a".to_string()),
                cwd: Some(project_a.to_string_lossy().to_string()),
                project_id: Some("project-a".to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal a");
    let session_b = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf b".to_string()),
                cwd: Some(project_b.to_string_lossy().to_string()),
                project_id: Some("project-b".to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal b");

    runtime.handle_terminal_subscribe(&RemoteEnvelope {
        kind: "terminal.subscribe".to_string(),
        device_id: Some("mac".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({ "scope": "project", "projectId": "project-a" }),
    });
    runtime.handle_terminal_subscribe(&RemoteEnvelope {
        kind: "terminal.subscribe".to_string(),
        device_id: Some("windows".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({ "scope": "project", "projectId": "project-b" }),
    });

    let viewers_a = runtime.terminal_output_viewers(&session_a);
    let viewers_b = runtime.terminal_output_viewers(&session_b);

    assert!(viewers_a.is_empty());
    assert!(viewers_b.is_empty());
    assert!(
        runtime
            .resource_subscriptions
            .devices_for(REMOTE_RESOURCE_TERMINALS, Some("project-a"), None)
            .contains("mac")
    );
    assert!(
        runtime
            .resource_subscriptions
            .devices_for(REMOTE_RESOURCE_TERMINALS, Some("project-b"), None)
            .contains("windows")
    );

    runtime.handle_terminal_unsubscribe(&RemoteEnvelope {
        kind: "terminal.unsubscribe".to_string(),
        device_id: Some("mac".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({ "scope": "project", "projectId": "project-a" }),
    });

    let viewers_a = runtime.terminal_output_viewers(&session_a);
    assert!(!viewers_a.contains("mac"));

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn project_select_does_not_change_session_viewers() {
    let support_dir = temp_support_dir("codux-project-select-terminal-viewers");
    let (project_a, project_b) = write_two_project_state(&support_dir);
    let terminals = Arc::new(TerminalManager::new());
    let runtime = Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    ));
    let session_a = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf a".to_string()),
                cwd: Some(project_a.to_string_lossy().to_string()),
                project_id: Some("project-a".to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal a");
    let session_b = terminals
        .create(
            TerminalPtyConfig {
                shell: Some("sh".to_string()),
                command: Some("printf b".to_string()),
                cwd: Some(project_b.to_string_lossy().to_string()),
                project_id: Some("project-b".to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal b");

    runtime.handle_project_select(&RemoteEnvelope {
        kind: "project.select".to_string(),
        device_id: Some("phone".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({ "projectId": "project-a" }),
    });
    assert!(runtime.terminal_output_viewers(&session_a).is_empty());

    runtime.handle_project_select(&RemoteEnvelope {
        kind: "project.select".to_string(),
        device_id: Some("phone".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({ "projectId": "project-b" }),
    });

    assert!(runtime.terminal_output_viewers(&session_a).is_empty());
    assert!(runtime.terminal_output_viewers(&session_b).is_empty());

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn git_status_snapshot_replies_only_to_requesting_device() {
    let support_dir = temp_support_dir("codux-remote-resource-subscriptions");
    let (project_a, _) = write_two_project_state(&support_dir);
    let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }

    runtime.handle_resource_subscribe(&RemoteEnvelope {
        kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
        device_id: Some("phone-a".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({
            "resource": REMOTE_RESOURCE_GIT_STATUS,
            "projectId": "project-a",
            "projectPath": project_a.to_string_lossy(),
        }),
    });
    transport.take_messages();

    runtime.handle_git_status(&RemoteEnvelope {
        kind: REMOTE_GIT_STATUS.to_string(),
        device_id: Some("phone-b".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({
            "projectId": "project-a",
            "projectPath": project_a.to_string_lossy(),
        }),
    });

    let messages = transport.take_messages();
    let target_devices = messages
        .iter()
        .filter_map(|(device_id, data)| {
            let value: Value = serde_json::from_slice(data).ok()?;
            let kind = value.get("type").and_then(Value::as_str);
            (kind == Some(REMOTE_GIT_STATUS)).then(|| device_id.clone())
        })
        .collect::<Vec<_>>();

    assert!(!target_devices.contains(&Some("phone-a".to_string())));
    assert_eq!(target_devices, vec![Some("phone-b".to_string())]);

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn resource_change_without_subscribers_does_not_broadcast() {
    let support_dir = temp_support_dir("codux-remote-resource-no-subscribers");
    let runtime = RemoteHostRuntime::new(support_dir.clone());
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }

    runtime.broadcast_resource_payload(
        REMOTE_GIT_STATUS,
        REMOTE_RESOURCE_GIT_STATUS,
        None,
        Some("project-a"),
        None,
        json!({ "projectId": "project-a" }),
    );

    assert!(transport.take_messages().is_empty());

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn unsupported_resource_subscription_is_not_retained() {
    let support_dir = temp_support_dir("codux-remote-resource-unsupported");
    let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }

    runtime.handle_resource_subscribe(&RemoteEnvelope {
        kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
        device_id: Some("phone-a".to_string()),
        session_id: None,
        request_id: Some("request-unsupported".to_string()),
        seq: None,
        payload: json!({ "resource": "unsupported" }),
    });

    assert!(
        runtime
            .resource_subscriptions
            .devices_for_resource("unsupported")
            .is_empty()
    );
    let messages = transport.take_messages();
    assert!(messages.iter().any(|(_, data)| {
        let value: Value = serde_json::from_slice(data).expect("json");
        value.get("type").and_then(Value::as_str) == Some(REMOTE_ERROR)
            && value.get("requestId").and_then(Value::as_str) == Some("request-unsupported")
    }));

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn invalid_resource_scopes_are_rejected_before_subscription() {
    let support_dir = temp_support_dir("codux-remote-resource-invalid-scopes");
    write_two_project_state(&support_dir);
    let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }

    for (request_id, payload) in [
        (
            "scoped-projects",
            json!({
                "resource": REMOTE_RESOURCE_PROJECTS,
                "projectId": "project-a",
            }),
        ),
        (
            "missing-project",
            json!({
                "resource": REMOTE_RESOURCE_GIT_STATUS,
                "projectId": "missing",
            }),
        ),
        (
            "missing-session",
            json!({
                "resource": REMOTE_RESOURCE_TERMINALS,
                "sessionId": "missing",
            }),
        ),
    ] {
        runtime.handle_resource_subscribe(&RemoteEnvelope {
            kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
            device_id: Some("phone-a".to_string()),
            session_id: None,
            request_id: Some(request_id.to_string()),
            seq: None,
            payload,
        });
    }

    assert!(
        runtime
            .resource_subscriptions
            .devices_for_resource(REMOTE_RESOURCE_PROJECTS)
            .is_empty()
    );
    assert!(
        runtime
            .resource_subscriptions
            .devices_for_resource(REMOTE_RESOURCE_GIT_STATUS)
            .is_empty()
    );
    assert!(
        runtime
            .resource_subscriptions
            .devices_for_resource(REMOTE_RESOURCE_TERMINALS)
            .is_empty()
    );
    let errors = transport
        .take_messages()
        .into_iter()
        .filter_map(|(_, data)| serde_json::from_slice::<Value>(&data).ok())
        .filter(|value| value.get("type").and_then(Value::as_str) == Some(REMOTE_ERROR))
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 3);
    assert!(errors.iter().all(|value| value.get("requestId").is_some()));

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn resource_worktree_subscription_uses_registered_project_path() {
    let support_dir = temp_support_dir("codux-remote-resource-worktree-path");
    let (project_a, _) = write_two_project_state(&support_dir);
    let unrelated = support_dir.join("unrelated");
    fs::create_dir_all(&unrelated).expect("create unrelated dir");
    let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));
    let transport = Arc::new(CapturingTransport::default());
    if let Ok(mut current) = runtime.transport.lock() {
        *current = Some(transport.clone());
    }

    runtime.handle_resource_subscribe(&RemoteEnvelope {
        kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
        device_id: Some("phone-a".to_string()),
        session_id: None,
        request_id: Some("worktrees".to_string()),
        seq: None,
        payload: json!({
            "resource": REMOTE_RESOURCE_WORKTREES,
            "projectId": "project-a",
            "projectPath": unrelated.to_string_lossy(),
        }),
    });

    let message = transport
        .take_messages()
        .into_iter()
        .filter_map(|(_, data)| serde_json::from_slice::<Value>(&data).ok())
        .find(|value| value.get("type").and_then(Value::as_str) == Some(REMOTE_WORKTREE_LIST))
        .expect("worktree reply");
    assert_eq!(message["payload"]["projectId"], "project-a");
    assert_eq!(
        runtime.worktree_request_scope(&RemoteEnvelope {
            kind: REMOTE_WORKTREE_LIST.to_string(),
            device_id: Some("phone-a".to_string()),
            session_id: None,
            request_id: None,
            seq: None,
            payload: json!({
                "projectId": "project-a",
                "projectPath": unrelated.to_string_lossy(),
            }),
        }),
        Ok((
            "project-a".to_string(),
            project_a.to_string_lossy().to_string(),
        ))
    );

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_project_resource_subscription_sends_list_without_baseline() {
    let support_dir = temp_support_dir("codux-remote-resource-terminal-tail-baseline");
    write_paired_remote_settings(&support_dir);
    write_two_project_state(&support_dir);
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
                command: Some("printf abcdef".to_string()),
                cwd: Some(support_dir.to_string_lossy().to_string()),
                project_id: Some("project-a".to_string()),
                terminal_id: Some("terminal-a".to_string()),
                ..Default::default()
            },
            |_| {},
        )
        .expect("create terminal");
    TerminalLayoutService::new(support_dir.clone())
        .save_from_gpui(
            &terminal_layout_storage_key("project-a", "project-a"),
            Vec::new(),
            vec![TerminalPaneSummary {
                title: "Main".to_string(),
                terminal_id: session_id.clone(),
            }],
            vec![1.0],
            0.24,
        )
        .expect("save layout");

    runtime.handle_resource_subscribe(&RemoteEnvelope {
        kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
        device_id: Some("phone-a".to_string()),
        session_id: None,
        request_id: Some("request-1".to_string()),
        seq: None,
        payload: json!({
            "resource": REMOTE_RESOURCE_TERMINALS,
            "projectId": "project-a",
            "baseline": true,
            "maxChars": 3,
        }),
    });

    let messages = transport.take_messages();
    assert!(messages.iter().any(|(_, data)| {
        let value: Value = serde_json::from_slice(data).expect("json");
        value.get("type").and_then(Value::as_str) == Some(REMOTE_TERMINAL_LIST)
            && value.get("requestId").and_then(Value::as_str) == Some("request-1")
    }));
    assert!(messages.iter().all(|(_, data)| {
        let value: Value = serde_json::from_slice(data).expect("json");
        value.get("type").and_then(Value::as_str) != Some(REMOTE_TERMINAL_OUTPUT)
    }));
    assert!(runtime.terminal_output_viewers(&session_id).is_empty());

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn terminal_list_subscription_does_not_receive_session_output() {
    let support_dir = temp_support_dir("codux-remote-terminal-list-only");
    let runtime = Arc::new(RemoteHostRuntime::new(support_dir.clone()));

    runtime.handle_resource_subscribe(&RemoteEnvelope {
        kind: REMOTE_RESOURCE_SUBSCRIBE.to_string(),
        device_id: Some("phone-a".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({ "resource": REMOTE_RESOURCE_TERMINALS }),
    });

    assert!(runtime.terminal_output_viewers("session-1").is_empty());
    fs::remove_dir_all(support_dir).ok();
}
