use super::crypto::{
    ensure_remote_host_identity, remote_base64_url_encode, remote_e2e_decrypt, remote_e2e_encrypt,
    remote_e2e_symmetric_key, remote_pairing_match_code, remote_pairing_payload,
};
use super::host::{
    quote_terminal_path, remote_ai_stats_payload, remote_file_list, remote_file_read,
    remote_file_rename, remote_file_write, remote_terminal_upload_directory,
    remote_terminal_upload_kind, sanitized_remote_upload_name, terminal_upload_path_input,
    unique_remote_upload_path,
};
use super::pairing::remote_summary_show_pending_pairing;
use super::summary::remote_summary_from_settings;
use super::types::{
    RemoteDeviceSettings, RemoteOutgoingEnvelope, RemotePairingInfo, RemoteSettings,
    RemoteTransportCandidate,
};
use super::{RemoteHostRuntime, RemoteService};
use crate::ai_history_indexer::AIHistoryProjectState;
use crate::config::flush_all_config_writes;
use crate::terminal_pty::{TerminalManager, TerminalSessionSnapshot};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

#[test]
fn summary_matches_tauri_remote_status_shape_from_settings() {
    let summary = remote_summary_from_settings(RemoteSettings {
        is_enabled: true,
        relay_preset: "custom".to_string(),
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
fn disabled_remote_keeps_empty_relay_and_disabled_encryption() {
    let summary = remote_summary_from_settings(RemoteSettings {
        is_enabled: false,
        relay_preset: String::new(),
        server_url: String::new(),
        ..Default::default()
    });

    assert!(!summary.enabled);
    assert_eq!(summary.relay, super::relay::GLOBAL_RELAY_SERVER_URL);
    assert_eq!(summary.status, "stopped");
    assert_eq!(summary.encryption, "disabled");
}

#[test]
fn remote_identity_and_pairing_payload_match_tauri_shape() {
    let mut settings = RemoteSettings {
        is_enabled: true,
        relay_preset: "custom".to_string(),
        server_url: "http://relay.example".to_string(),
        host_id: "host-1".to_string(),
        ..Default::default()
    };
    ensure_remote_host_identity(&mut settings);
    assert!(!settings.host_private_key.is_empty());
    assert!(!settings.host_public_key.is_empty());

    assert!(remote_pairing_match_code(&settings, "123456", "secret", "device-public").is_some());
}

#[test]
fn remote_pairing_payload_contains_v3_transport_candidates() {
    let settings = RemoteSettings {
        is_enabled: true,
        relay_preset: "custom".to_string(),
        server_url: "http://relay.example".to_string(),
        host_id: "host-1".to_string(),
        host_token: "token".to_string(),
        host_private_key: "host-private".to_string(),
        host_public_key: "host-public".to_string(),
        cached_devices: Vec::new(),
    };
    let pairing = RemotePairingInfo {
        pairing_id: "pair-1".to_string(),
        code: "123456".to_string(),
        secret: "secret".to_string(),
        host_public_key: Some("host-public".to_string()),
        crypto_version: Some(1),
        expires_at: "later".to_string(),
        qr_payload: String::new(),
    };
    let value = remote_pairing_payload(
        &settings,
        &pairing,
        vec![
            RemoteTransportCandidate {
                kind: "websocketRelay".to_string(),
                role: Some("host".to_string()),
                url: Some("https://relay.example".to_string()),
                ice_servers: Vec::new(),
            },
            RemoteTransportCandidate {
                kind: "webRtc".to_string(),
                role: Some("host".to_string()),
                url: Some("https://relay.example".to_string()),
                ice_servers: vec![super::types::RemoteIceServer {
                    urls: vec![
                        "stun:stun.miwifi.com:3478".to_string(),
                        "stun:stun.l.google.com:19302".to_string(),
                    ],
                }],
            },
        ],
    );
    assert_eq!(value["code"], "123456");
    assert_eq!(value["secret"], "secret");
    assert_eq!(value["pairingId"], "pair-1");
    assert_eq!(value["hostPublicKey"], "host-public");
    assert_eq!(value["cryptoVersion"], 1);
    assert_eq!(
        value["protocolVersion"],
        super::protocol::REMOTE_PROTOCOL_VERSION
    );
    assert!(value.get("transport").is_none());
    assert!(value.get("iroh").is_none());
    assert_eq!(value["transports"][0]["kind"], "websocketRelay");
    assert_eq!(value["transports"][0]["role"], "host");
    assert_eq!(value["transports"][0]["url"], "https://relay.example");
    assert_eq!(value["transports"][1]["kind"], "webRtc");
    assert_eq!(value["transports"][1]["url"], "https://relay.example");
}

#[test]
fn remote_pairing_ticket_payload_is_the_only_qr_shape() {
    let payload =
        super::relay::remote_pairing_ticket_payload("https://relay.example/v3", "ticket-1")
            .expect("ticket payload");
    assert_eq!(
        payload,
        "codux://pair?server=https%3A%2F%2Frelay.example%2Fv3&ticket=ticket-1"
    );
}

#[test]
fn remote_relay_urls_use_v3_prefix_for_plain_domains() {
    let relay = super::relay::remote_server_url("https://codux-service.dux.plus");
    assert_eq!(relay, "https://codux-service.dux.plus/v3");
    assert_eq!(
        super::relay::remote_url(&relay, "/api/hosts/register", &[], false).unwrap(),
        "https://codux-service.dux.plus/v3/api/hosts/register"
    );
    assert_eq!(
        super::relay::remote_url(
            &relay,
            "/ws/host",
            &[("hostId", "host-1"), ("token", "token-1")],
            true,
        )
        .unwrap(),
        "wss://codux-service.dux.plus/v3/ws/host?hostId=host-1&token=token-1"
    );
    assert_eq!(
        super::relay::remote_server_url("https://codux-service.dux.plus/v3"),
        "https://codux-service.dux.plus/v3"
    );
    assert_eq!(super::relay::remote_relay_preset_for_url(""), "global");
    assert_eq!(
        super::relay::remote_relay_preset_for_url("https://codux-service.dux.plus"),
        "china"
    );
    assert_eq!(
        super::relay::remote_relay_preset_for_url("https://codux-node.dux.plus"),
        "global"
    );
    assert_eq!(
        super::relay::remote_relay_preset_for_url("https://relay.example"),
        "custom"
    );
}

#[test]
fn pending_pairing_summary_matches_tauri_claimed_status_shape() {
    let settings = RemoteSettings {
        is_enabled: true,
        relay_preset: "custom".to_string(),
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

    assert_eq!(
        encrypted.get("v").and_then(serde_json::Value::as_i64),
        Some(1)
    );
    assert_eq!(
        encrypted.get("alg").and_then(serde_json::Value::as_str),
        Some("X25519-HKDF-SHA256-AES-256-GCM")
    );
    assert!(
        encrypted
            .get("nonce")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
    assert!(
        encrypted
            .get("ciphertext")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
    assert!(
        encrypted
            .get("tag")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );

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
                "serverUrl": "http://relay.example",
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
                "serverUrl": "http://relay.example",
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
    let parsed = service
        .parse_incoming_envelope(&text)
        .expect("parse secure envelope");
    let mut receive_seq = HashMap::new();
    let decrypted = service
        .decrypt_envelope_if_needed(parsed.clone(), &mut receive_seq)
        .expect("decrypt secure envelope")
        .expect("inner envelope");

    assert_eq!(decrypted.kind, "terminal.output");
    assert_eq!(decrypted.device_id.as_deref(), Some("device-1"));
    assert_eq!(decrypted.session_id.as_deref(), Some("term-1"));
    assert_eq!(decrypted.seq, Some(1));
    assert!(receive_seq.contains_key("device-1"));

    let replay = service
        .decrypt_envelope_if_needed(parsed, &mut receive_seq)
        .expect("decrypt replay");
    assert!(replay.is_none());

    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_service_accepts_out_of_order_secure_envelopes_across_channels() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-remote-envelope-out-of-order-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");

    let host_secret = StaticSecret::from([13_u8; 32]);
    let host_public = X25519PublicKey::from(&host_secret);
    let device_secret = StaticSecret::from([14_u8; 32]);
    let device_public = X25519PublicKey::from(&device_secret);
    let host_private_key = remote_base64_url_encode(host_secret.to_bytes().as_slice());
    let host_public_key = remote_base64_url_encode(host_public.as_bytes());
    let device_public_key = remote_base64_url_encode(device_public.as_bytes());

    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "remote": {
                "isEnabled": true,
                "serverUrl": "http://relay.example",
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
    let mut receive_seq = HashMap::new();
    let next_secure = |kind: &str, seq: i64| {
        let payload = service
            .encrypt_device_payload(
                "device-1",
                serde_json::to_vec(&RemoteOutgoingEnvelope {
                    kind: kind.to_string(),
                    device_id: Some("device-1".to_string()),
                    session_id: None,
                    seq: Some(seq),
                    payload: json!({ "ok": true }),
                })
                .expect("serialize")
                .as_slice(),
            )
            .expect("encrypt");
        super::types::RemoteEnvelope {
            kind: "secure.message".to_string(),
            device_id: Some("device-1".to_string()),
            session_id: None,
            seq: None,
            payload,
        }
    };

    let second = service
        .decrypt_envelope_if_needed(next_secure("terminal.list", 2), &mut receive_seq)
        .expect("decrypt")
        .expect("terminal list");
    let first = service
        .decrypt_envelope_if_needed(next_secure("project.select", 1), &mut receive_seq)
        .expect("decrypt")
        .expect("project select");
    let duplicate = service
        .decrypt_envelope_if_needed(next_secure("project.select", 1), &mut receive_seq)
        .expect("decrypt");

    assert_eq!(second.kind, "terminal.list");
    assert_eq!(first.kind, "project.select");
    assert!(duplicate.is_none());

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
                "serverUrl": "http://relay.example"
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
        },
        "tab",
    );

    assert_eq!(
        payload.get("id").and_then(serde_json::Value::as_str),
        Some("term-1")
    );
    assert_eq!(
        payload
            .get("layoutKind")
            .and_then(serde_json::Value::as_str),
        Some("tab")
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
                "serverUrl": "http://relay.example",
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
                "serverUrl": "http://relay.example"
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
                "serverUrl": "http://relay.example",
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
                "serverUrl": "http://127.0.0.1:8088",
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
                "serverUrl": "",
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
                "serverUrl": "",
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
