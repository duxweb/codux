#[test]
fn file_watch_events_are_queued_and_drained_for_gpui() {
    let support_dir =
        std::env::temp_dir().join(format!("codux-file-watch-events-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&support_dir).expect("create support dir");
    let service = RuntimeService::new(PathBuf::from(&support_dir));

    service
        .file_watch_events
        .lock()
        .expect("file event queue")
        .push_back(FileChangeEvent {
            project_path: "/tmp/project".to_string(),
            changed_paths: vec!["/tmp/project/src/main.rs".to_string()],
        });

    let events = service.drain_file_change_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].project_path, "/tmp/project");
    assert!(service.drain_file_change_events().is_empty());

    let _ = fs::remove_dir_all(support_dir);
}

#[test]
fn revoke_remote_device_preserves_connected_host_snapshot() {
    let support_dir =
        std::env::temp_dir().join(format!("codux-revoke-remote-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&support_dir).expect("create support dir");
    fs::write(
        support_dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": true,
                "relayUrl": crate::remote::remote_relay_url_for_preset("china-tencent", ""),
                "hostID": "host-1",
                "hostToken": "secret-token",
                "cachedDevices": [
                    {"id": "device-1", "hostId": "host-1", "name": "Phone", "online": true}
                ]
            }
        }))
        .expect("settings json"),
    )
    .expect("write settings");

    let service = RuntimeService::new(PathBuf::from(&support_dir));
    let mut connected = service.remote_host.reload_snapshot_from_settings();
    connected.status = "connected".to_string();
    connected.message = "Remote transport connected.".to_string();
    service.remote_host.apply_snapshot(connected);

    let summary = service
        .revoke_remote_device("device-1")
        .expect("revoke device");

    assert_eq!(summary.status, "connected");
    assert_eq!(summary.message, "Remote transport connected.");
    assert_eq!(summary.devices, 0);
    assert!(summary.device_list.is_empty());

    let _ = fs::remove_dir_all(support_dir);
}

#[test]
fn runtime_dock_badge_count_matches_tauri_attention_semantics() {
    let mut snapshot = AIRuntimeStateSnapshot::default();

    assert_eq!(runtime_dock_badge_count(true, &snapshot), None);

    snapshot.needs_input_count = 2;
    snapshot.completion_count = 3;

    assert_eq!(runtime_dock_badge_count(true, &snapshot), Some(5));
    assert_eq!(runtime_dock_badge_count(false, &snapshot), None);
}

#[test]
fn ai_history_global_scope_includes_project_roots_and_worktrees() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-ai-history-workspaces-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&support_dir).expect("create support dir");
    fs::write(
        support_dir.join("state.json"),
        serde_json::json!({
            "projects": [
                { "id": "project-a", "name": "Project A", "path": "/tmp/project-a" },
                { "id": "project-b", "name": "Project B", "path": "/tmp/project-b" },
                { "id": "project-windows", "name": "Project Windows", "path": "C:\\Users\\test\\project" },
                {
                    "id": "project-wsl",
                    "name": "Project WSL",
                    "path": "/home/test/project",
                    "runtimeTarget": { "kind": "wsl", "distribution": "Ubuntu" }
                },
                {
                    "id": "project-remote",
                    "name": "Project Remote",
                    "path": "/srv/project",
                    "runtimeTarget": { "kind": "remote", "deviceId": "device-1" }
                }
            ],
            "worktrees": [
                {
                    "id": "worktree-a",
                    "projectId": "project-a",
                    "name": "Feature A",
                    "branch": "feature/a",
                    "path": "/tmp/project-a-worktree",
                    "status": "todo",
                    "isDefault": false,
                    "createdAt": 1,
                    "updatedAt": 1
                },
                {
                    "id": "duplicate-root",
                    "projectId": "project-a",
                    "name": "Duplicate Root",
                    "branch": "main",
                    "path": "/tmp/project-a",
                    "status": "todo",
                    "isDefault": true,
                    "createdAt": 1,
                    "updatedAt": 1
                },
                {
                    "id": "duplicate-windows-root",
                    "projectId": "project-windows",
                    "name": "Duplicate Windows Root",
                    "branch": "main",
                    "path": "\\\\?\\c:\\users\\test\\project\\",
                    "status": "todo",
                    "isDefault": true,
                    "createdAt": 1,
                    "updatedAt": 1
                },
                {
                    "id": "worktree-wsl",
                    "projectId": "project-wsl",
                    "name": "WSL Feature",
                    "branch": "feature/wsl",
                    "path": "/home/test/project-feature",
                    "status": "todo",
                    "isDefault": false,
                    "createdAt": 1,
                    "updatedAt": 1
                },
                {
                    "id": "worktree-remote",
                    "projectId": "project-remote",
                    "name": "Remote Feature",
                    "branch": "feature/remote",
                    "path": "/srv/project-feature",
                    "status": "todo",
                    "isDefault": false,
                    "createdAt": 1,
                    "updatedAt": 1
                }
            ]
        })
        .to_string(),
    )
    .expect("write state");
    let service = RuntimeService::new(support_dir.clone());

    let requests = service.ai_history_workspace_requests();

    assert_eq!(requests.len(), 4);
    assert!(requests.iter().any(|request| request.id == "project-a"));
    assert!(requests.iter().any(|request| request.id == "project-b"));
    assert!(requests.iter().any(|request| request.id == "worktree-a"));
    assert!(requests.iter().any(|request| request.id == "project-windows"));
    assert!(!requests.iter().any(|request| request.id == "project-wsl"));
    assert!(!requests.iter().any(|request| request.id == "worktree-wsl"));
    assert!(!requests.iter().any(|request| request.id == "project-remote"));
    assert!(!requests.iter().any(|request| request.id == "worktree-remote"));
    assert!(!requests.iter().any(|request| request.id == "duplicate-root"));
    assert!(
        !requests
            .iter()
            .any(|request| request.id == "duplicate-windows-root")
    );
    let _ = fs::remove_dir_all(support_dir);
}

#[test]
fn applying_global_history_updates_daily_level_from_the_same_snapshot() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-ai-history-level-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&support_dir).expect("create support dir");
    let mut state = RuntimeState::load_from_support_dir(support_dir.clone());
    let history = AIGlobalHistorySummary {
        today_total_tokens: 6_000_000,
        ..Default::default()
    };

    state.set_ai_global_history(history);

    assert_eq!(state.ai_global_history.today_total_tokens, 6_000_000);
    assert_eq!(state.daily_level.tokens, 6_000_000);
    assert_eq!(state.daily_level.current_tier.id, "gold");
    let _ = fs::remove_dir_all(support_dir);
}
