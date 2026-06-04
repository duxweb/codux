use super::crypto::{
    ensure_remote_host_identity, remote_base64_url_encode, remote_e2e_decrypt,
    remote_e2e_encrypt, remote_e2e_symmetric_key, remote_pairing_match_code,
    remote_pairing_qr_payload,
};
use super::http::{default_remote_server_url, remote_url};
use super::host::{
    quote_terminal_path, remote_ai_stats_payload, remote_file_list, remote_file_read,
    remote_file_rename, remote_file_write, remote_p2p_lane, remote_pending_pairing_summary,
    remote_terminal_upload_directory, remote_terminal_upload_kind, sanitized_remote_upload_name,
    terminal_upload_path_input, unique_remote_upload_path,
};
use super::pairing::remote_summary_show_pending_pairing;
use super::types::{RemoteDeviceSettings, RemoteOutgoingEnvelope, RemotePairingInfo, RemoteSettings};
use crate::ai_history_indexer::AIHistoryProjectState;
use crate::config::flush_all_config_writes;
use crate::remote_p2p::RemoteP2PLane;
use crate::terminal_pty::{TerminalManager, TerminalSessionSnapshot};
use super::{RemoteHostRuntime, RemoteService, remote_summary_from_settings};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

fn remote_test_server_sequence(
    response_bodies: Vec<&'static str>,
) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for response_body in response_bodies {
            let (mut stream, _) = listener.accept().expect("accept request");
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let Ok(read) = stream.read(&mut buffer) else {
                    break;
                };
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(key, value)| {
                            key.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                    })
                    .unwrap_or(0);
                if request.len() >= header_end + 4 + content_length {
                    break;
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&request).to_string());
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (format!("http://{address}"), rx)
}

#[test]
fn summary_matches_tauri_remote_status_shape_from_settings() {
    let summary = remote_summary_from_settings(RemoteSettings {
        is_enabled: true,
        server_url: " http://relay.example ".to_string(),
        host_id: "host-1".to_string(),
        host_public_key: "pub".to_string(),
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
fn disabled_remote_uses_default_relay_and_disabled_encryption() {
    let summary = remote_summary_from_settings(RemoteSettings {
        is_enabled: false,
        server_url: String::new(),
        ..Default::default()
    });

    assert!(!summary.enabled);
    assert_eq!(summary.relay, default_remote_server_url());
    assert_eq!(summary.status, "stopped");
    assert_eq!(summary.encryption, "disabled");
}

#[test]
fn remote_url_matches_tauri_http_and_websocket_shape() {
    let http = remote_url(
        "https://relay.example/base",
        "/api/hosts/host-1/devices",
        &[("token", "secret token")],
        false,
    )
    .unwrap();
    assert_eq!(
        http,
        "https://relay.example/api/hosts/host-1/devices?token=secret+token"
    );

    let websocket = remote_url(
        "https://relay.example",
        "/ws/host",
        &[("hostId", "host-1"), ("token", "secret")],
        true,
    )
    .unwrap();
    assert_eq!(
        websocket,
        "wss://relay.example/ws/host?hostId=host-1&token=secret"
    );
}

#[test]
fn remote_identity_and_pairing_payload_match_tauri_shape() {
    let mut settings = RemoteSettings {
        is_enabled: true,
        server_url: "http://relay.example".to_string(),
        host_id: "host-1".to_string(),
        ..Default::default()
    };
    ensure_remote_host_identity(&mut settings);
    assert!(!settings.host_private_key.is_empty());
    assert!(!settings.host_public_key.is_empty());

    let pairing = RemotePairingInfo {
        pairing_id: "pair-1".to_string(),
        code: "123456".to_string(),
        secret: "secret".to_string(),
        host_public_key: Some(settings.host_public_key.clone()),
        crypto_version: Some(1),
        expires_at: "2026-01-01T00:00:00Z".to_string(),
        qr_payload: String::new(),
    };
    let payload = remote_pairing_qr_payload(&settings, &pairing);
    assert!(!payload.is_empty());
    assert!(remote_pairing_match_code(&settings, "123456", "secret", "device-public").is_some());
}

#[test]
fn pending_pairing_summary_matches_tauri_claimed_status_shape() {
    let settings = RemoteSettings {
        is_enabled: true,
        server_url: "http://relay.example".to_string(),
        host_id: "host-1".to_string(),
        host_token: "host-token".to_string(),
        host_public_key: "host-public".to_string(),
        ..Default::default()
    };
    let active_pairing = RemotePairingInfo {
        pairing_id: "pair-1".to_string(),
        code: "123456".to_string(),
        secret: "secret".to_string(),
        host_public_key: Some("host-public".to_string()),
        crypto_version: Some(1),
        expires_at: "2026-01-01T00:00:00Z".to_string(),
        qr_payload: "payload".to_string(),
    };

    let summary = remote_summary_show_pending_pairing(
        settings.clone(),
        &active_pairing,
        "pair-1".to_string(),
        "iPhone".to_string(),
        "device-public".to_string(),
        "654321".to_string(),
        "secret".to_string(),
    );

    assert_eq!(summary.status, "connected");
    assert_eq!(summary.message, "Confirm device pairing.");
    assert!(summary.pairing.is_none());
    assert_eq!(summary.pending_pairings, 1);
    assert_eq!(summary.pending_pairing_list[0].id, "pair-1");
    assert_eq!(summary.pending_pairing_list[0].device_name, "iPhone");
    assert_ne!(summary.pending_pairing_list[0].code, "654321");

    let without_device_key = remote_summary_show_pending_pairing(
        settings,
        &active_pairing,
        "pair-2".to_string(),
        "Mobile Device".to_string(),
        String::new(),
        "111222".to_string(),
        "secret".to_string(),
    );
    assert_eq!(without_device_key.pending_pairing_list[0].code, "111222");
}

#[test]
fn pairing_request_summary_matches_tauri_pending_status_shape() {
    let settings = RemoteSettings {
        is_enabled: true,
        server_url: "http://relay.example".to_string(),
        host_id: "host-1".to_string(),
        host_token: "host-token".to_string(),
        host_public_key: "host-public".to_string(),
        ..Default::default()
    };
    let active_pairing = RemotePairingInfo {
        pairing_id: "pair-1".to_string(),
        code: "123456".to_string(),
        secret: "secret".to_string(),
        host_public_key: Some("host-public".to_string()),
        crypto_version: Some(1),
        expires_at: "2026-01-01T00:00:00Z".to_string(),
        qr_payload: "payload".to_string(),
    };
    let mut current = remote_summary_from_settings(settings.clone());
    current.status = "connected".to_string();
    current.pairing = Some(active_pairing);

    let pending = remote_pending_pairing_summary(
        current,
        settings.clone(),
        &json!({
            "pairingId": "pair-1",
            "deviceName": "iPhone",
            "devicePublicKey": "device-public",
            "code": "654321"
        }),
    )
    .expect("pending pairing");

    assert_eq!(pending.status, "connected");
    assert_eq!(pending.message, "Confirm device pairing.");
    assert!(pending.pairing.is_none());
    assert_eq!(pending.pending_pairings, 1);
    assert_eq!(pending.pending_pairing_list[0].id, "pair-1");
    assert_eq!(pending.pending_pairing_list[0].device_name, "iPhone");
    assert_eq!(pending.pending_pairing_list[0].device_public_key, "device-public");
    assert_ne!(pending.pending_pairing_list[0].code, "654321");

    let updated = remote_pending_pairing_summary(
        pending,
        settings,
        &json!({
            "pairingId": "pair-1",
            "deviceName": "iPad",
            "devicePublicKey": "device-public-2",
            "code": "111222"
        }),
    )
    .expect("updated pending pairing");
    assert_eq!(updated.pending_pairings, 1);
    assert_eq!(updated.pending_pairing_list[0].device_name, "iPad");
    assert_eq!(
        updated.pending_pairing_list[0].device_public_key,
        "device-public-2"
    );
}

#[test]
fn remote_e2e_crypto_matches_tauri_envelope_shape() {
    let host_secret = StaticSecret::from([7_u8; 32]);
    let host_public = X25519PublicKey::from(&host_secret);
    let device_secret = StaticSecret::from([9_u8; 32]);
    let device_public = X25519PublicKey::from(&device_secret);

    let host_private_key = remote_base64_url_encode(host_secret.to_bytes().as_slice());
    let host_public_key = remote_base64_url_encode(host_public.as_bytes());
    let device_private_key = remote_base64_url_encode(device_secret.to_bytes().as_slice());
    let device_public_key = remote_base64_url_encode(device_public.as_bytes());

    let host_key =
        remote_e2e_symmetric_key(&host_private_key, &device_public_key, "host-1", "device-1")
            .expect("host key");
    let device_key =
        remote_e2e_symmetric_key(&device_private_key, &host_public_key, "host-1", "device-1")
            .expect("device key");
    assert_eq!(host_key, device_key);

    let plaintext = br#"{"type":"terminal.input","payload":{"data":"ls\n"}}"#;
    let encrypted =
        remote_e2e_encrypt(plaintext, &host_key, "host-1", "device-1").expect("encrypt");

    assert_eq!(encrypted.get("v").and_then(serde_json::Value::as_i64), Some(1));
    assert_eq!(
        encrypted.get("alg").and_then(serde_json::Value::as_str),
        Some("X25519-HKDF-SHA256-AES-256-GCM")
    );
    assert!(encrypted.get("nonce").and_then(serde_json::Value::as_str).is_some());
    assert!(
        encrypted
            .get("ciphertext")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
    assert!(encrypted.get("tag").and_then(serde_json::Value::as_str).is_some());

    let decrypted =
        remote_e2e_decrypt(&encrypted, &device_key, "host-1", "device-1").expect("decrypt");
    assert_eq!(decrypted, plaintext);
    assert!(remote_e2e_decrypt(&encrypted, &device_key, "host-1", "other-device").is_err());
}

#[test]
fn remote_service_encrypts_and_decrypts_cached_device_payloads() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-envelope-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");

    let host_secret = StaticSecret::from([3_u8; 32]);
    let host_public = X25519PublicKey::from(&host_secret);
    let device_secret = StaticSecret::from([4_u8; 32]);
    let device_public = X25519PublicKey::from(&device_secret);
    let host_private_key = remote_base64_url_encode(host_secret.to_bytes().as_slice());
    let host_public_key = remote_base64_url_encode(host_public.as_bytes());
    let device_public_key = remote_base64_url_encode(device_public.as_bytes());

    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": true,
                "serverURL": "http://relay.example",
                "hostID": "host-1",
                "hostPrivateKey": host_private_key,
                "hostPublicKey": host_public_key,
                "cachedDevices": [
                    {
                        "id": "device-1",
                        "hostId": "host-1",
                        "name": "Phone",
                        "publicKey": device_public_key
                    }
                ]
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let service = RemoteService::new(dir.clone());
    let plaintext = br#"{"type":"secure.message","payload":{"ok":true}}"#;
    let encrypted = service
        .encrypt_device_payload("device-1", plaintext)
        .expect("encrypt");
    let decrypted = service
        .decrypt_device_payload("device-1", &encrypted)
        .expect("decrypt");

    assert_eq!(decrypted, plaintext);
    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_service_wraps_and_unwraps_secure_envelopes_with_sequence_guard() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-envelope-seq-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");

    let host_secret = StaticSecret::from([11_u8; 32]);
    let host_public = X25519PublicKey::from(&host_secret);
    let device_secret = StaticSecret::from([12_u8; 32]);
    let device_public = X25519PublicKey::from(&device_secret);
    let host_private_key = remote_base64_url_encode(host_secret.to_bytes().as_slice());
    let host_public_key = remote_base64_url_encode(host_public.as_bytes());
    let device_public_key = remote_base64_url_encode(device_public.as_bytes());

    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": true,
                "serverURL": "http://relay.example",
                "hostID": "host-1",
                "hostPrivateKey": host_private_key,
                "hostPublicKey": host_public_key,
                "cachedDevices": [
                    {
                        "id": "device-1",
                        "hostId": "host-1",
                        "name": "Phone",
                        "publicKey": device_public_key
                    }
                ]
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let service = RemoteService::new(dir.clone());
    let mut send_seq = HashMap::new();
    let secure = service
        .encrypted_outgoing_envelope(
            RemoteOutgoingEnvelope {
                kind: "terminal.output".to_string(),
                device_id: Some("device-1".to_string()),
                session_id: Some("term-1".to_string()),
                seq: None,
                payload: json!({ "data": "hello" }),
            },
            &mut send_seq,
        )
        .expect("secure envelope");

    assert_eq!(secure.kind, "secure.message");
    assert_eq!(secure.device_id.as_deref(), Some("device-1"));
    assert_eq!(secure.session_id.as_deref(), Some("term-1"));
    assert_eq!(secure.seq, None);
    assert_eq!(send_seq.get("device-1"), Some(&1));

    let text = serde_json::to_string(&secure).expect("serialize secure envelope");
    let parsed = service.parse_incoming_envelope(&text).expect("parse secure envelope");
    let mut receive_seq = HashMap::new();
    let decrypted = service
        .decrypt_envelope_if_needed(parsed.clone(), &mut receive_seq)
        .expect("decrypt secure envelope")
        .expect("inner envelope");

    assert_eq!(decrypted.kind, "terminal.output");
    assert_eq!(decrypted.device_id.as_deref(), Some("device-1"));
    assert_eq!(decrypted.session_id.as_deref(), Some("term-1"));
    assert_eq!(decrypted.seq, Some(1));
    assert_eq!(receive_seq.get("device-1"), Some(&1));

    let replay = service
        .decrypt_envelope_if_needed(parsed, &mut receive_seq)
        .expect("decrypt replay");
    assert!(replay.is_none());

    fs::remove_dir_all(dir).ok();
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
                "serverURL": "http://relay.example"
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
    assert!(!host.send_relay("host.info", None, None, json!({})));

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
    assert_eq!(events[0].status, "stopped");
    assert_eq!(events[0].message, "Remote Host stopped for test.");
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
    assert_eq!(events[0].status, "stopped");
    assert_eq!(events[0].message, "Remote Host stopped.");

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
    assert_eq!(payload.get("path").and_then(serde_json::Value::as_str), Some(dir.to_str().unwrap()));
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
    assert_eq!(payload.get("name").and_then(serde_json::Value::as_str), Some("note.txt"));
    assert_eq!(payload.get("content").and_then(serde_json::Value::as_str), Some("hello"));
    assert!(remote_file_read(dir.to_str().unwrap()).is_err());

    assert!(remote_file_rename(source.to_str().unwrap(), other.join("note.txt").to_str().unwrap()).is_err());
    remote_file_rename(source.to_str().unwrap(), renamed.to_str().unwrap()).expect("rename");
    assert!(renamed.exists());

    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_ai_stats_payload_matches_tauri_empty_snapshot_shape() {
    let payload = remote_ai_stats_payload(
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
    )
    .expect("payload");

    assert_eq!(
        payload.get("projectId").and_then(serde_json::Value::as_str),
        Some("project-1")
    );
    assert_eq!(
        payload.get("projectName").and_then(serde_json::Value::as_str),
        Some("Project One")
    );
    assert!(payload.get("projectSummary").and_then(serde_json::Value::as_object).is_some());
    assert_eq!(
        payload.get("sessions").and_then(serde_json::Value::as_array).map(Vec::len),
        Some(0)
    );
    assert!(payload.get("updatedAt").and_then(serde_json::Value::as_str).is_some());
}

#[test]
fn remote_terminal_snapshot_payload_uses_compact_terminal_identity_shape() {
    let payload = super::host::remote_terminal_snapshot_payload(TerminalSessionSnapshot {
        id: "term-1".to_string(),
        title: "Shell".to_string(),
        slot_id: "slot-1".to_string(),
        session_key: Some("session-key-1".to_string()),
        project_id: "project-1".to_string(),
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
    });

    assert_eq!(payload.get("id").and_then(serde_json::Value::as_str), Some("term-1"));
    assert_eq!(
        payload.get("displayTitle").and_then(serde_json::Value::as_str),
        Some("Codux · Shell")
    );
    assert_eq!(payload.get("cols").and_then(serde_json::Value::as_u64), Some(120));
    assert_eq!(payload.get("rows").and_then(serde_json::Value::as_u64), Some(36));
    assert!(payload.get("kind").is_none());
    assert!(payload.get("slotId").is_none());
    assert!(payload.get("sessionKey").is_none());
    assert!(payload.get("paneIndex").is_none());
    assert!(payload.get("sortOrder").is_none());
    assert_eq!(
        payload.get("isRunning").and_then(serde_json::Value::as_bool),
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
    let mut terminals = vec![
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
    assert_eq!(
        remote_terminal_upload_kind(&json!({ "kind": "file" })),
        "file"
    );
    assert_eq!(
        remote_terminal_upload_kind(&json!({ "kind": "image/png" })),
        "image"
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
fn remote_p2p_lane_matches_tauri_routing() {
    assert_eq!(remote_p2p_lane("terminal.upload.ack"), RemoteP2PLane::Upload);
    assert_eq!(remote_p2p_lane("terminal.uploaded"), RemoteP2PLane::Upload);
    assert_eq!(remote_p2p_lane("terminal.output"), RemoteP2PLane::Terminal);
    assert_eq!(remote_p2p_lane("p2p.pong"), RemoteP2PLane::Terminal);
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
                "serverURL": "http://relay.example",
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
                "serverURL": "http://relay.example"
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
                "serverURL": "http://relay.example",
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
                "serverURL": "http://127.0.0.1:8088",
                "hostID": "host-1",
                "hostToken": "secret-token",
                "hostPublicKey": "",
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
    assert_eq!(summary.encryption, "pending");
    assert!(!format!("{summary:?}").contains("secret-token"));
    fs::remove_dir_all(dir).ok();
}

#[test]
fn toggles_remote_host_and_revokes_cached_device_preserving_secrets() {
    let (relay_url, request_rx) = remote_test_server_sequence(vec![
        "{}",
        r#"{"devices":[{"id":"device-2","hostId":"host-1","name":"Tablet","online":true},{"id":"device-3","hostId":"host-1","name":"Laptop","online":true}]}"#,
    ]);
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
                "serverURL": relay_url,
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
    assert_eq!(enabled.relay, relay_url);

    let revoked = service
        .revoke_device("device-1")
        .expect("revoke cached device");
    assert_eq!(revoked.status, "connected");
    assert_eq!(revoked.message, "Device removed.");
    assert_eq!(revoked.devices, 2);
    assert_eq!(revoked.device_list[0].id, "device-2");
    let request = request_rx.recv().expect("revoke request");
    assert!(request.contains("POST /api/devices/revoke"));
    assert!(request.contains("\"hostId\":\"host-1\""));
    assert!(request.contains("\"token\":\"secret-token\""));
    assert!(request.contains("\"deviceId\":\"device-1\""));
    let refresh_request = request_rx.recv().expect("refresh request");
    assert!(refresh_request.contains("GET /api/hosts/host-1/devices?token=secret-token"));
    flush_all_config_writes();
    let raw = fs::read_to_string(dir.join("settings.json")).expect("settings");
    assert!(raw.contains("secret-token"));
    assert!(!raw.contains("\"id\": \"device-1\""));
    assert!(raw.contains("\"id\": \"device-2\""));
    assert!(raw.contains("\"id\": \"device-3\""));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn refresh_devices_registers_host_first_when_missing_host_identity() {
    let (relay_url, request_rx) = remote_test_server_sequence(vec![
        r#"{"hostId":"registered-host","token":"registered-token"}"#,
        r#"{"devices":[{"id":"device-1","hostId":"registered-host","name":"Phone","online":true}]}"#,
    ]);
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
                "serverURL": relay_url,
                "hostID": "",
                "hostToken": "",
                "cachedDevices": []
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let summary = RemoteService::new(dir.clone())
        .refresh_devices()
        .expect("refresh devices");

    assert_eq!(summary.host_id, "registered-host");
    assert_eq!(summary.devices, 1);
    assert_eq!(summary.device_list[0].id, "device-1");
    assert_eq!(summary.device_list[0].online, Some(false));
    let register_request = request_rx.recv().expect("register request");
    assert!(register_request.contains("POST /api/hosts/register"));
    let devices_request = request_rx.recv().expect("devices request");
    assert!(devices_request.contains("GET /api/hosts/registered-host/devices?token=registered-token"));
    flush_all_config_writes();
    let raw = fs::read_to_string(dir.join("settings.json")).expect("settings");
    assert!(raw.contains("registered-host"));
    assert!(raw.contains("registered-token"));
    assert!(raw.contains("device-1"));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_host_runtime_apply_snapshot_queues_gpui_event() {
    let dir =
        std::env::temp_dir().join(format!("codux-gpui-remote-host-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create temp support");
    let runtime = RemoteHostRuntime::new(dir.clone());
    let summary = remote_summary_from_settings(RemoteSettings {
        is_enabled: true,
        server_url: "http://relay.example".to_string(),
        host_id: "host-1".to_string(),
        host_public_key: "host-public".to_string(),
        ..Default::default()
    });

    let applied = runtime.apply_snapshot(summary.clone());

    assert_eq!(applied.host_id, "host-1");
    assert_eq!(runtime.snapshot().host_id, "host-1");
    let events = runtime.drain_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].host_id, "host-1");
    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_host_runtime_uses_injected_terminal_manager() {
    let dir =
        std::env::temp_dir().join(format!("codux-gpui-remote-host-{}", uuid::Uuid::new_v4()));
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
