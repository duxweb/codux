use super::*;

#[test]
fn sleep_mode_cycles_match_tauri_settings_values() {
    assert_eq!(next_sleep_mode("off"), "always");
    assert_eq!(next_sleep_mode("always"), "powerAdapterOnly");
    assert_eq!(next_sleep_mode("powerAdapterOnly"), "off");
    assert_eq!(next_sleep_mode("unknown"), "always");
}

#[test]
fn summary_normalizes_unknown_sleep_mode_without_creating_assertion() {
    let service = PowerService::new();
    let summary = service.summary("invalid");
    assert_eq!(summary.mode, "off");
    assert!(!summary.effective_enabled);
    assert!(!summary.assertion_active);
}

#[test]
fn manager_summary_reflects_shared_sleep_assertion_state() {
    let manager = PowerManager::default();

    manager
        .set_sleep_prevention("off".to_string())
        .expect("disable sleep prevention");

    let summary = manager.summary("off");
    assert_eq!(summary.mode, "off");
    assert!(!summary.effective_enabled);
    assert!(!summary.assertion_active);
}
