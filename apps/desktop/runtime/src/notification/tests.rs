use super::*;
use std::fs;

#[test]
fn summary_redacts_tokens_and_reports_known_channels() {
    let raw = serde_json::json!({
        "notificationChannels": {
            "ntfy": {"enabled": true, "endpoint": " https://ntfy.sh/topic ", "token": "secret"},
            "custom": {"enabled": true, "endpoint": "https://example.test", "token": ""}
        }
    })
    .as_object()
    .expect("object")
    .clone();

    let summary = summary_from_raw(&raw);

    assert!(summary.channel_count >= KNOWN_CHANNELS.len());
    assert_eq!(summary.enabled_count, 2);
    assert_eq!(summary.configured_count, 2);
    let ntfy = summary
        .channels
        .iter()
        .find(|channel| channel.id == "ntfy")
        .expect("ntfy channel");
    assert!(ntfy.enabled);
    assert!(ntfy.has_token);
    assert_eq!(ntfy.endpoint, "https://ntfy.sh/topic");
    let serialized = serde_json::to_string(&summary).expect("serialize");
    assert!(!serialized.contains("secret"));
}

#[test]
fn toggle_channel_preserves_endpoint_and_token() {
    let dir = std::env::temp_dir().join(format!(
        "codux-gpui-notification-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp support");
    fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "notificationChannels": {
                "bark": {"enabled": false, "endpoint": "https://bark.example", "token": "device-key"}
            }
        }))
        .unwrap(),
    )
    .expect("write settings");

    let summary = NotificationService::new(dir.clone())
        .toggle_channel("bark")
        .expect("toggle channel");

    let bark = summary
        .channels
        .iter()
        .find(|channel| channel.id == "bark")
        .expect("bark channel");
    assert!(bark.enabled);
    assert_eq!(bark.endpoint, "https://bark.example");
    assert!(bark.has_token);
    let raw = fs::read_to_string(dir.join("settings.json")).expect("settings");
    assert!(raw.contains("device-key"));
    fs::remove_dir_all(dir).ok();
}
