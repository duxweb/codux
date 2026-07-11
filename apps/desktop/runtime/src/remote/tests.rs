use super::crypto::{display_host_name, remote_pairing_payload};
use super::host::{
    remote_file_list, remote_file_read, remote_file_rename, remote_file_write,
    remote_terminal_upload_directory, sanitized_remote_upload_name, terminal_upload_path_input,
    unique_remote_upload_path,
};
use super::summary::remote_summary_from_settings;
use super::transport_factory::host_transport_config;
use super::types::{RemoteDeviceSettings, RemoteHostEvent, RemotePairingInfo, RemoteSettings};
use super::{RemoteHostRuntime, RemoteService};
use crate::ai_history_indexer::AIHistoryProjectState;
use crate::config::flush_all_config_writes;
use crate::terminal_pty::{TerminalManager, TerminalSessionSnapshot};
use codux_runtime_core::upload::quote_terminal_path;
use serde_json::json;
use std::fs;
use std::sync::Arc;

#[test]
fn summary_matches_tauri_remote_status_shape_from_settings() {
    let summary = remote_summary_from_settings(RemoteSettings {
        is_enabled: true,
        relay_preset: "custom".to_string(),
        relay_url: " http://relay.example ".to_string(),
        host_id: "host-1".to_string(),
        cached_devices: vec![
            RemoteDeviceSettings {
                id: "device-1".to_string(),
                host_id: "host-1".to_string(),
                name: "Phone".to_string(),
                online: Some(true),
                ..Default::default()
            },
            RemoteDeviceSettings {
                id: "device-2".to_string(),
                host_id: "other-host".to_string(),
                name: "Other".to_string(),
                online: Some(true),
                ..Default::default()
            },
            RemoteDeviceSettings {
                id: "device-3".to_string(),
                host_id: "host-1".to_string(),
                revoked_at: Some("2026-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
        ],
        ..Default::default()
    });

    assert!(summary.enabled);
    assert_eq!(summary.relay, "http://relay.example");
    assert_eq!(summary.status, "connecting");
    assert_eq!(summary.encryption, "configured");
    assert_eq!(summary.devices, 1);
    assert_eq!(summary.online_devices, 1);
    assert_eq!(summary.device_list[0].name, "Phone");
}

#[test]
fn host_transport_config_uses_iroh_relay_preset_mapping() {
    let custom = host_transport_config(&RemoteSettings {
        relay_preset: "custom".to_string(),
        relay_url: "https://relay.example".to_string(),
        relay_authentication: " relay-token ".to_string(),
        host_id: "host-1".to_string(),
        host_token: "token-1".to_string(),
        ..Default::default()
    });
    assert_eq!(custom.relay_preset, "custom");
    assert_eq!(custom.iroh_relay_url, "https://relay.example");
    assert_eq!(custom.iroh_relay_authentication, " relay-token ");

    let china = host_transport_config(&RemoteSettings {
        relay_preset: "china".to_string(),
        relay_url: "https://ignored.example".to_string(),
        ..Default::default()
    });
    let tencent_preset_url = super::relay::remote_relay_presets()
        .iter()
        .find(|preset| preset.key == "china-tencent")
        .map(|preset| preset.url.as_str())
        .expect("china-tencent preset");
    assert_eq!(china.relay_preset, "china-tencent");
    assert_eq!(china.iroh_relay_url, tencent_preset_url);

    let aliyun = host_transport_config(&RemoteSettings {
        relay_preset: "china-aliyun".to_string(),
        relay_url: "https://ignored.example".to_string(),
        ..Default::default()
    });
    let aliyun_preset_url = super::relay::remote_relay_presets()
        .iter()
        .find(|preset| preset.key == "china-aliyun")
        .map(|preset| preset.url.as_str())
        .expect("china-aliyun preset");
    assert_eq!(aliyun.iroh_relay_url, aliyun_preset_url);

    let global = host_transport_config(&RemoteSettings {
        relay_preset: "global".to_string(),
        relay_url: "https://ignored.example".to_string(),
        ..Default::default()
    });
    assert_eq!(global.iroh_relay_url, "");
}

#[test]
fn disabled_remote_keeps_empty_relay_and_disabled_encryption() {
    let summary = remote_summary_from_settings(RemoteSettings {
        is_enabled: false,
        relay_preset: String::new(),
        relay_url: String::new(),
        ..Default::default()
    });

    assert!(!summary.enabled);
    assert_eq!(summary.relay, super::relay::GLOBAL_RELAY_SERVER_URL);
    assert_eq!(summary.status, "stopped");
    assert_eq!(summary.encryption, "disabled");
}

#[test]
fn display_host_name_replaces_generic_apple_name_with_user_name() {
    assert_eq!(
        display_host_name(
            Some("Apple 的 Apple 电脑".to_string()),
            Some("developer".to_string())
        ),
        Some("developer的Apple电脑".to_string())
    );
    assert_eq!(
        display_host_name(
            Some("工作室 MacBook Pro".to_string()),
            Some("developer".to_string())
        ),
        Some("工作室 MacBook Pro".to_string())
    );
}

#[test]
fn remote_pairing_payload_contains_iroh_transport_candidates() {
    let settings = RemoteSettings {
        is_enabled: true,
        relay_preset: "custom".to_string(),
        relay_url: "http://relay.example".to_string(),
        relay_authentication: "relay-token".to_string(),
        host_id: "host-1".to_string(),
        host_token: "token".to_string(),
        cached_devices: Vec::new(),
    };
    let pairing = RemotePairingInfo {
        pairing_id: "pair-1".to_string(),
        code: "123456".to_string(),
        secret: "secret".to_string(),
        expires_at: "later".to_string(),
        qr_payload: String::new(),
    };
    let value = remote_pairing_payload(
        &settings,
        &pairing,
        vec![
            codux_protocol::iroh_transport_candidate_with_ticket_and_authentication(
                "https://relay.example",
                "node-1",
                "https://relay.example",
                "endpoint-ticket",
                settings.relay_authentication.clone(),
            ),
        ],
    );
    assert_eq!(value["code"], "123456");
    assert_eq!(value["secret"], "secret");
    assert_eq!(value["pairingId"], "pair-1");
    assert!(value.get("hostId").is_none());
    assert!(value.get("hostName").is_none());
    assert!(value.get("protocolVersion").is_none());
    assert!(value.get("transport").is_none());
    assert!(value.get("iroh").is_none());
    assert_eq!(value["transports"][0]["kind"], "iroh");
    // The QR omits the bulky iroh endpoint ticket (it ~doubles QR density and
    // hurts scan reliability) and carries the minimum needed to dial — nodeId +
    // relayUrl. The host re-sends the full transport set on `pairing.confirmed`.
    assert!(value["transports"][0].get("ticket").is_none());
    assert_eq!(value["transports"][0]["nodeId"], "node-1");
    assert_eq!(value["transports"][0]["relayUrl"], "https://relay.example");
    assert_eq!(value["transports"][0]["relayAuthentication"], "relay-token");
    assert!(value["transports"][0].get("role").is_none());
    assert!(value["transports"][0].get("url").is_none());
}

#[test]
fn remote_pairing_payload_url_embeds_pairing_payload_without_http_ticket_service() {
    let value = json!({
        "code": "123456",
        "transports": [{
            "kind": "iroh",
            "ticket": "endpointabc"
        }]
    });
    let payload = super::relay::remote_pairing_payload_url(&value).expect("payload url");
    let url = url::Url::parse(&payload).expect("qr url");
    assert_eq!(url.scheme(), "codux");
    assert_eq!(url.host_str(), Some("pair"));
    assert!(url.query_pairs().any(|(key, _)| key == "payload"));
    assert!(!url.query_pairs().any(|(key, _)| key == "ticket"));
}

#[test]
fn remote_relay_presets_use_iroh_relay_urls() {
    let tencent = super::relay::remote_relay_presets()
        .iter()
        .find(|preset| preset.key == "china-tencent")
        .expect("china-tencent preset");
    let aliyun_preset_url = super::relay::remote_relay_presets()
        .iter()
        .find(|preset| preset.key == "china-aliyun")
        .map(|preset| preset.url.as_str())
        .expect("china-aliyun preset");

    assert_eq!(super::relay::remote_relay_preset_for_url(""), "global");
    assert_eq!(
        super::relay::remote_relay_preset_for_url(&tencent.url),
        "china-tencent"
    );
    assert_eq!(super::relay::remote_relay_url_for_preset("global", ""), "");
    assert_eq!(
        super::relay::remote_relay_url_for_preset("china", ""),
        tencent.url
    );
    assert_eq!(
        super::relay::remote_relay_url_for_preset("china-aliyun", ""),
        aliyun_preset_url
    );
    assert_eq!(
        super::relay::remote_relay_preset_for_url("https://relay.example"),
        "custom"
    );
}

#[test]
fn remote_host_runtime_stops_without_enabled_remote_settings() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-host-disabled-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");
    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": false,
                "relayUrl": "http://relay.example"
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let host = std::sync::Arc::new(RemoteHostRuntime::new(dir.clone()));
    let summary = host.start();

    assert!(!summary.enabled);
    assert_eq!(summary.status, "stopped");
    assert_eq!(summary.message, "Remote Host stopped.");
    assert!(!host.send_transport("host.info", None, None, json!({})));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_host_runtime_queues_status_events_for_gpui() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-event-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");

    let host = std::sync::Arc::new(RemoteHostRuntime::new(dir.clone()));
    assert!(host.drain_events().is_empty());

    host.stop_with_message("Remote Host stopped for test.");
    let events = host.drain_events();
    assert_eq!(events.len(), 1);
    let RemoteHostEvent::Summary(summary) = &events[0] else {
        panic!("expected remote summary event");
    };
    assert_eq!(summary.status, "stopped");
    assert_eq!(summary.message, "Remote Host stopped for test.");
    assert!(host.drain_events().is_empty());

    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_host_runtime_shutdown_stops_and_queues_gpui_event() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-shutdown-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");

    let host = RemoteHostRuntime::new(dir.clone());
    host.shutdown();

    let snapshot = host.snapshot();
    assert_eq!(snapshot.status, "stopped");
    assert_eq!(snapshot.message, "Remote Host stopped.");
    let events = host.drain_events();
    assert_eq!(events.len(), 1);
    let RemoteHostEvent::Summary(summary) = &events[0] else {
        panic!("expected remote summary event");
    };
    assert_eq!(summary.status, "stopped");
    assert_eq!(summary.message, "Remote Host stopped.");

    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_file_list_matches_tauri_mobile_shape_and_sorting() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-file-list-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(dir.join("zeta")).expect("create zeta");
    fs::create_dir_all(dir.join("Alpha")).expect("create alpha");
    fs::write(dir.join("beta.txt"), "beta").expect("write beta");
    fs::write(dir.join(".hidden"), "hidden").expect("write hidden");

    let payload = remote_file_list(Some(dir.to_str().unwrap()), Some("project-picker"));
    assert_eq!(
        payload.get("path").and_then(serde_json::Value::as_str),
        Some(dir.to_str().unwrap())
    );
    assert_eq!(
        payload.get("purpose").and_then(serde_json::Value::as_str),
        Some("project-picker")
    );
    let names = payload
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .expect("entries")
        .iter()
        .filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["Alpha", "zeta", "beta.txt"]);

    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_file_read_write_and_rename_match_tauri_mobile_limits() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-file-mutate-test-{}",
        uuid::Uuid::new_v4()
    ));
    let other = dir.join("other");
    fs::create_dir_all(&dir).expect("create temp support");
    fs::create_dir_all(&other).expect("create other");
    let source = dir.join("note.txt");
    let renamed = dir.join("renamed.txt");

    remote_file_write(source.to_str().unwrap(), "hello").expect("write");
    let payload = remote_file_read(source.to_str().unwrap()).expect("read");
    assert_eq!(
        payload.get("name").and_then(serde_json::Value::as_str),
        Some("note.txt")
    );
    assert_eq!(
        payload.get("content").and_then(serde_json::Value::as_str),
        Some("hello")
    );
    assert!(remote_file_read(dir.to_str().unwrap()).is_err());

    assert!(
        remote_file_rename(
            source.to_str().unwrap(),
            other.join("note.txt").to_str().unwrap()
        )
        .is_err()
    );
    remote_file_rename(source.to_str().unwrap(), renamed.to_str().unwrap()).expect("rename");
    assert!(renamed.exists());

    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_ai_stats_payload_matches_tauri_empty_snapshot_shape() {
    let payload = codux_runtime_core::ai_stats::ai_stats_payload_from_state(
        "project-1".to_string(),
        "Project One".to_string(),
        AIHistoryProjectState {
            project_id: "project-1".to_string(),
            project_name: "Project One".to_string(),
            project_path: "/tmp/project-one".to_string(),
            snapshot: None,
            is_loading: false,
            queued: false,
            progress: None,
            detail: "idle".to_string(),
            error: None,
            version: 1,
        },
        Vec::new(),
    )
    .expect("payload");

    assert_eq!(
        payload.get("projectId").and_then(serde_json::Value::as_str),
        Some("project-1")
    );
    assert_eq!(
        payload
            .get("projectName")
            .and_then(serde_json::Value::as_str),
        Some("Project One")
    );
    assert!(
        payload
            .get("projectSummary")
            .and_then(serde_json::Value::as_object)
            .is_some()
    );
    assert_eq!(
        payload
            .get("sessions")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(0)
    );
    assert!(
        payload
            .get("updatedAt")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
}

#[test]
fn remote_git_status_payload_matches_domain_shape() {
    let payload = super::host::remote_git_status_payload(
        "project-1".to_string(),
        "/tmp/project-1".to_string(),
        crate::git::GitSummary {
            branch: "main".to_string(),
            upstream: Some("origin/main".to_string()),
            ahead: 2,
            behind: 1,
            head_pushed: true,
            staged: 1,
            unstaged: 2,
            untracked: 3,
            is_repository: true,
            error: None,
            changed_files: vec![crate::git::GitFileStatus {
                path: "src/main.rs".to_string(),
                index_status: "modified".to_string(),
                worktree_status: "modified".to_string(),
            }],
            branches: vec![crate::git::GitBranchSummary {
                name: "main".to_string(),
                is_current: true,
            }],
            remote_branches: vec!["origin/main".to_string()],
            stashes: vec![],
            tags: vec![],
            remotes: vec![crate::git::GitRemoteSummary {
                name: "origin".to_string(),
                url: "https://example.test/repo.git".to_string(),
            }],
            commits: vec![],
        },
    );

    assert_eq!(
        payload.get("projectId").and_then(serde_json::Value::as_str),
        Some("project-1")
    );
    assert_eq!(
        payload.get("branch").and_then(serde_json::Value::as_str),
        Some("main")
    );
    assert_eq!(
        payload.get("changes").and_then(serde_json::Value::as_u64),
        Some(6)
    );
    assert_eq!(
        payload
            .get("changedFiles")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(1)
    );
}

#[test]
fn remote_terminal_snapshot_payload_uses_compact_terminal_identity_shape() {
    let payload = super::host::remote_terminal_snapshot_payload(
        TerminalSessionSnapshot {
            id: "term-1".to_string(),
            title: "Shell".to_string(),
            slot_id: "slot-1".to_string(),
            session_key: Some("session-key-1".to_string()),
            project_id: "project-1".to_string(),
            worktree_id: Some("worktree-1".to_string()),
            project_name: "Codux".to_string(),
            cwd: "/workspace/codux".to_string(),
            shell: "zsh".to_string(),
            command: String::new(),
            cols: 120,
            rows: 36,
            status: "running".to_string(),
            is_running: true,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_active_at: "2026-01-01T00:00:01Z".to_string(),
            buffer_characters: 42,
            has_buffer: true,
            tool: None,
        },
        Some("worktree-1"),
        Some(3),
    );

    assert_eq!(
        payload.get("id").and_then(serde_json::Value::as_str),
        Some("term-1")
    );
    assert_eq!(
        payload
            .get("worktreeId")
            .and_then(serde_json::Value::as_str),
        Some("worktree-1")
    );
    assert_eq!(
        payload
            .get("layoutOrder")
            .and_then(serde_json::Value::as_u64),
        Some(3)
    );
    assert_eq!(
        payload
            .get("displayTitle")
            .and_then(serde_json::Value::as_str),
        Some("Codux · Shell")
    );
    assert_eq!(
        payload.get("cols").and_then(serde_json::Value::as_u64),
        Some(120)
    );
    assert_eq!(
        payload.get("rows").and_then(serde_json::Value::as_u64),
        Some(36)
    );
    assert!(payload.get("kind").is_none());
    assert!(payload.get("slotId").is_none());
    assert!(payload.get("sessionKey").is_none());
    assert!(payload.get("paneIndex").is_none());
    assert!(payload.get("sortOrder").is_none());
    assert_eq!(
        payload
            .get("isRunning")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload
            .get("bufferCharacters")
            .and_then(serde_json::Value::as_u64),
        Some(42)
    );
}

#[test]
fn remote_terminal_order_uses_runtime_order_before_id() {
    let mut terminals = [
        json!({
            "id": "term-2",
            "createdAt": "2026-01-01T00:00:02Z",
        }),
        json!({
            "id": "term-1",
            "createdAt": "2026-01-01T00:00:01Z",
        }),
    ];

    terminals.sort_by_key(super::host::remote_terminal_order_key);

    assert_eq!(
        terminals
            .iter()
            .filter_map(|terminal| terminal.get("id").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>(),
        vec!["term-1", "term-2"]
    );
}

#[test]
fn remote_terminal_upload_helpers_match_tauri_shape() {
    assert_eq!(
        sanitized_remote_upload_name("../unsafe path/$image.png"),
        "_image.png"
    );
    assert_eq!(sanitized_remote_upload_name("..."), "upload.png");
    assert_eq!(
        remote_terminal_upload_directory("../term id")
            .file_name()
            .and_then(|value| value.to_str()),
        Some("term_id")
    );
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-upload-path-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");
    fs::write(dir.join("asset.png"), "existing").expect("write existing");
    let unique = unique_remote_upload_path(&dir, "asset.png");
    assert_eq!(
        unique.file_name().and_then(|value| value.to_str()),
        Some("asset-1.png")
    );
    fs::remove_dir_all(dir).ok();
}

#[test]
fn terminal_upload_path_input_quotes_shell_sensitive_paths() {
    assert_eq!(
        quote_terminal_path("/tmp/CoduxUploads/file.txt"),
        "/tmp/CoduxUploads/file.txt"
    );

    #[cfg(not(windows))]
    assert_eq!(
        terminal_upload_path_input(std::path::Path::new("/tmp/Codux Uploads/file name.txt")),
        "'/tmp/Codux Uploads/file name.txt'"
    );

    #[cfg(windows)]
    assert_eq!(
        terminal_upload_path_input(std::path::Path::new(
            r"C:\Users\Codux User\AppData\Local\Temp\file name.txt"
        )),
        r#""C:\Users\Codux User\AppData\Local\Temp\file name.txt""#
    );
}

#[test]
fn refresh_devices_disabled_remote_is_noop() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-refresh-noop-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");
    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": false,
                "relayUrl": "http://relay.example",
                "hostID": "",
                "hostToken": "secret-token"
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let summary = RemoteService::new(dir.clone())
        .refresh_devices()
        .expect("refresh noop");
    assert!(!summary.enabled);
    assert_eq!(summary.devices, 0);
    fs::remove_dir_all(dir).ok();
}

#[test]
fn register_host_disabled_is_noop() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-register-noop-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");
    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": false,
                "relayUrl": "http://relay.example"
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let summary = RemoteService::new(dir.clone())
        .register_host()
        .expect("register noop");
    assert!(!summary.enabled);
    assert_eq!(summary.status, "stopped");
    fs::remove_dir_all(dir).ok();
}

#[test]
fn sync_settings_background_is_noop_when_disabled() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-sync-disabled-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");
    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": false,
                "relayUrl": "http://relay.example",
                "hostID": "host-1",
                "hostToken": "secret-token"
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let summary = RemoteService::new(dir.clone()).sync_settings_background();

    assert!(!summary.enabled);
    assert_eq!(summary.status, "stopped");
    let raw = fs::read_to_string(dir.join("settings.json")).expect("settings");
    assert!(raw.contains("secret-token"));
    fs::remove_dir_all(dir).ok();
}

#[test]
fn reads_settings_json_without_exposing_tokens() {
    let dir = std::env::temp_dir().join(format!("codux-gpui-remote-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create temp support");
    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": true,
                "relayUrl": "http://127.0.0.1:8088",
                "hostID": "host-1",
                "hostToken": "secret-token",
                "cachedDevices": [
                    {"id": "device-1", "hostId": "host-1", "name": "Tablet", "online": false}
                ]
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let summary = RemoteService::new(dir.clone()).summary();

    assert!(summary.enabled);
    assert_eq!(summary.host_id, "host-1");
    assert_eq!(summary.devices, 1);
    assert_eq!(summary.encryption, "configured");
    assert!(!format!("{summary:?}").contains("secret-token"));
    fs::remove_dir_all(dir).ok();
}

#[test]
fn toggles_remote_host_and_revokes_cached_device_preserving_secrets() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-mutate-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");
    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": false,
                "relayUrl": "",
                "hostID": "host-1",
                "hostToken": "secret-token",
                "cachedDevices": [
                    {"id": "device-1", "hostId": "host-1", "name": "Phone", "revokedAt": null},
                    {"id": "device-2", "hostId": "host-1", "name": "Tablet", "revokedAt": null}
                ]
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let service = RemoteService::new(dir.clone());
    let enabled = service.set_enabled(true).expect("enable remote");
    assert!(enabled.enabled);
    assert_eq!(enabled.relay, super::relay::GLOBAL_RELAY_SERVER_URL);

    let revoked = service
        .revoke_device("device-1")
        .expect("revoke cached device");
    assert_eq!(revoked.status, "connecting");
    assert_eq!(revoked.message, "Device removed.");
    assert_eq!(revoked.devices, 1);
    assert_eq!(revoked.device_list[0].id, "device-2");
    flush_all_config_writes();
    let raw = fs::read_to_string(dir.join("settings.json")).expect("settings");
    assert!(raw.contains("secret-token"));
    assert!(!raw.contains("\"id\": \"device-1\""));
    assert!(raw.contains("\"id\": \"device-2\""));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn refresh_devices_without_host_token_returns_local_cached_devices() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-refresh-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");
    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": true,
                "relayUrl": "",
                "hostID": "host-1",
                "cachedDevices": [
                    {"id":"device-1","hostId":"host-1","name":"Phone","online":true}
                ]
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let summary = RemoteService::new(dir.clone())
        .refresh_devices()
        .expect("refresh devices");

    assert_eq!(summary.host_id, "host-1");
    assert_eq!(summary.status, "connecting");
    assert_eq!(summary.devices, 1);
    assert_eq!(summary.device_list[0].id, "device-1");
    assert_eq!(summary.device_list[0].online, Some(false));
    flush_all_config_writes();
    let raw = fs::read_to_string(dir.join("settings.json")).expect("settings");
    assert!(raw.contains("host-1"));
    assert!(raw.contains("device-1"));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_host_runtime_apply_snapshot_queues_gpui_event() {
    let dir = std::env::temp_dir().join(format!("codux-gpui-remote-host-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create temp support");
    let runtime = RemoteHostRuntime::new(dir.clone());
    let summary = remote_summary_from_settings(RemoteSettings {
        is_enabled: true,
        relay_preset: "custom".to_string(),
        relay_url: "http://relay.example".to_string(),
        host_id: "host-1".to_string(),
        ..Default::default()
    });

    let applied = runtime.apply_snapshot(summary.clone());

    assert_eq!(applied.host_id, "host-1");
    assert_eq!(runtime.snapshot().host_id, "host-1");
    let events = runtime.drain_events();
    assert_eq!(events.len(), 1);
    let RemoteHostEvent::Summary(summary) = &events[0] else {
        panic!("expected remote summary event");
    };
    assert_eq!(summary.host_id, "host-1");
    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_host_runtime_uses_injected_terminal_manager() {
    let dir = std::env::temp_dir().join(format!("codux-gpui-remote-host-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create temp support");
    let terminals = Arc::new(TerminalManager::new());
    let runtime = RemoteHostRuntime::new_with_ai_history_and_terminals(
        dir.clone(),
        Default::default(),
        Arc::clone(&terminals),
    );

    assert!(Arc::ptr_eq(&terminals, &runtime.terminal_manager()));

    fs::remove_dir_all(dir).ok();
}
