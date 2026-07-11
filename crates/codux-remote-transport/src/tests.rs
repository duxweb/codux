use super::*;
use codux_protocol::{REMOTE_TRANSPORT_PING, REMOTE_TRANSPORT_PONG, RemoteEnvelope};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::mpsc;

#[test]
fn remote_url_preserves_existing_base_path_and_escapes_query() {
    let url = remote_url(
        "https://relay.example/custom",
        "api/resource/item 1",
        &[("hostId", "host 1"), ("deviceId", "device+1")],
    )
    .unwrap();

    assert_eq!(
        url,
        "https://relay.example/custom/api/resource/item%201?hostId=host+1&deviceId=device%2B1"
    );
}

#[test]
fn preferred_transport_helpers_only_accept_iroh() {
    let candidates = [("iroh", "https://relay.example")];
    assert_eq!(preferred_pairing_transport_kind(candidates), "iroh");
    assert_eq!(preferred_controller_transport_kind(candidates), "iroh");
    assert_eq!(preferred_controller_transport_kind([]), "");
    assert_eq!(preferred_pairing_transport_kind([]), "");
}

#[tokio::test]
async fn controller_factory_rejects_missing_iroh_candidate_before_network_dial() {
    let config = RemoteControllerTransportConfig {
        relay_url: "https://relay.example".to_string(),
        host_id: "host-1".to_string(),
        device_id: "device-1".to_string(),
        device_token: "token-1".to_string(),
        transports: Vec::new(),
    };

    let error = match RemoteTransportFactory::connect_controller(
        &config,
        Arc::new(|_, _| {}),
        Arc::new(|_, _| {}),
        None,
    )
    .await
    {
        Ok(_) => panic!("non-iroh-only config must not dial"),
        Err(error) => error,
    };

    assert!(error.contains("missing iroh"));
}

#[test]
fn controller_config_prefers_iroh_candidate() {
    let config = RemoteControllerTransportConfig {
        relay_url: "https://relay.example".to_string(),
        host_id: "host-1".to_string(),
        device_id: "device-1".to_string(),
        device_token: "token-1".to_string(),
        transports: vec![RemoteTransportCandidate {
            kind: "iroh".to_string(),
            url: "https://relay.example".to_string(),
            node_id: "node-1".to_string(),
            relay_url: "https://relay.example".to_string(),
            ticket: String::new(),
            relay_authentication: String::new(),
        }],
    };

    assert_eq!(
        preferred_controller_transport_kind(
            config
                .transports
                .iter()
                .map(|candidate| (candidate.kind.as_str(), candidate.url.as_str()))
        ),
        "iroh"
    );
}

#[test]
fn host_secret_key_keeps_iroh_endpoint_id_stable() {
    let config = RemoteHostTransportConfig {
        host_id: "host-1".to_string(),
        host_token: "host-token-1".to_string(),
        ..Default::default()
    };
    let first = crate::iroh_link::host_secret_key(&config)
        .expect("host secret")
        .public()
        .to_string();
    let second = crate::iroh_link::host_secret_key(&config)
        .expect("host secret")
        .public()
        .to_string();

    assert_eq!(first, second);

    let changed = crate::iroh_link::host_secret_key(&RemoteHostTransportConfig {
        host_id: "host-1".to_string(),
        host_token: "host-token-2".to_string(),
        ..Default::default()
    })
    .expect("changed host secret")
    .public()
    .to_string();

    assert_ne!(first, changed);
}

#[tokio::test]
async fn local_memory_transport_broadcasts_and_targets_messages() {
    let hub = LocalMemoryTransportHub::new();
    let received_a = Arc::new(StdMutex::new(Vec::<String>::new()));
    let received_b = Arc::new(StdMutex::new(Vec::<String>::new()));
    let state_b = Arc::new(StdMutex::new(Vec::<String>::new()));

    let a = hub.connect(
        "a",
        RemoteTransportKind::Iroh,
        {
            let received = Arc::clone(&received_a);
            Arc::new(move |source, data| {
                received
                    .lock()
                    .unwrap()
                    .push(format!("{source}:{}", String::from_utf8(data).unwrap()));
            })
        },
        Arc::new(|_, _| {}),
    );
    let b = hub.connect(
        "b",
        RemoteTransportKind::Iroh,
        {
            let received = Arc::clone(&received_b);
            Arc::new(move |source, data| {
                received
                    .lock()
                    .unwrap()
                    .push(format!("{source}:{}", String::from_utf8(data).unwrap()));
            })
        },
        {
            let state = Arc::clone(&state_b);
            Arc::new(move |peer, status| {
                state.lock().unwrap().push(format!("{peer}:{status}"));
            })
        },
    );

    assert!(a.send(b"hello".to_vec(), None));
    assert_eq!(received_b.lock().unwrap().as_slice(), ["a:hello"]);
    assert!(b.send(b"direct".to_vec(), Some("a")));
    assert_eq!(received_a.lock().unwrap().as_slice(), ["b:direct"]);
    assert!(!a.send(b"missing".to_vec(), Some("missing")));
    b.shutdown().await;
    assert_eq!(
        state_b.lock().unwrap().last().map(String::as_str),
        Some("b:closed")
    );
}

#[test]
fn transport_pong_for_ping_uses_fallback_device_id() {
    let ping = codux_protocol::RemoteEnvelope {
        kind: REMOTE_TRANSPORT_PING.to_string(),
        device_id: None,
        session_id: None,
        request_id: Some("request-2".to_string()),
        seq: None,
        payload: json!({ "id": "ping-2" }),
    };

    let pong =
        crate::control_messages::transport_pong_for_ping(&ping, Some("device-2")).expect("pong");
    let envelope: Value = serde_json::from_str(&pong).unwrap();
    assert_eq!(
        envelope.get("type").and_then(Value::as_str),
        Some(REMOTE_TRANSPORT_PONG)
    );
    assert_eq!(
        envelope.get("deviceId").and_then(Value::as_str),
        Some("device-2")
    );
    assert_eq!(envelope["payload"]["id"], "ping-2");
    assert_eq!(envelope["requestId"], "request-2");
}

#[test]
fn pairing_handshake_rejects_conflicting_device_ids() {
    let envelope = RemoteEnvelope {
        kind: codux_protocol::REMOTE_PAIRING_REQUEST.to_string(),
        device_id: Some("device-a".to_string()),
        session_id: None,
        request_id: None,
        seq: None,
        payload: json!({
            "deviceId": "device-b",
            "deviceName": "Phone",
            "pairingId": "pair-1",
            "code": "123456",
            "secret": "secret",
        }),
    };

    assert!(super::control_messages::pairing_handshake_from_envelope(&envelope).is_none());
}

#[test]
fn iroh_broadcast_deduplicates_peer_alias_senders() {
    let (tx, _rx) = mpsc::channel::<Vec<u8>>(1);
    let other = mpsc::channel::<Vec<u8>>(1).0;

    let unique = crate::iroh_link::unique_senders([&tx, &tx, &other]);

    assert_eq!(unique.len(), 2);
    assert!(unique[0].same_channel(&tx));
    assert!(unique[1].same_channel(&other));
}

#[test]
fn relay_preset_round_trip_matches_default_servers() {
    let tencent = remote_relay_presets()
        .iter()
        .find(|preset| preset.key == "china-tencent")
        .expect("china-tencent preset");
    assert_eq!(remote_relay_preset_for_url(&tencent.url), "china-tencent");
    assert_eq!(
        remote_relay_url_for_preset("global", ""),
        GLOBAL_RELAY_SERVER_URL
    );
}

#[test]
fn iroh_relay_presets_are_separate_from_pairing_servers() {
    let tencent = remote_relay_presets()
        .iter()
        .find(|preset| preset.key == "china-tencent")
        .expect("china-tencent preset");
    let aliyun = remote_relay_presets()
        .iter()
        .find(|preset| preset.key == "china-aliyun")
        .expect("china-aliyun preset");

    assert_eq!(iroh_relay_url_for_preset("global", ""), "");
    assert_eq!(iroh_relay_url_for_preset("china", ""), tencent.url);
    assert_eq!(iroh_relay_url_for_preset("china-tencent", ""), tencent.url);
    assert_eq!(iroh_relay_url_for_preset("china-aliyun", ""), aliyun.url);
    assert_eq!(
        iroh_relay_url_for_preset("custom", "https://relay.example"),
        "https://relay.example"
    );
    assert_eq!(iroh_relay_preset_for_url(""), "global");
    assert_eq!(
        iroh_relay_preset_for_url(&format!("{}/", tencent.url)),
        "china-tencent"
    );
    assert_eq!(iroh_relay_preset_for_url(&aliyun.url), "china-aliyun");
    assert_eq!(iroh_relay_preset_for_url("https://relay.example"), "custom");
    assert_eq!(normalize_remote_relay_preset("china", ""), "china-tencent");
    assert!(!aliyun.url.is_empty());
}
