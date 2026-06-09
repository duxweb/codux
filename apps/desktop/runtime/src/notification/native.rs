pub fn show_native_notification_blocking(
    title: &str,
    body: &str,
    group: &str,
) -> Result<(), String> {
    show_native_notification_impl(title, body, group)
}

fn show_native_notification_impl(title: &str, body: &str, group: &str) -> Result<(), String> {
    let _ = group;
    let title = native_notification_text(title, "Codux");
    let body = native_notification_text(body, "");
    ensure_native_notification_application();
    let mut notification = notify_rust::Notification::new();
    notification.summary(&title).body(&body);

    #[cfg(all(unix, not(target_os = "macos")))]
    notification.appname("Codux").icon("codux");

    let result = notification
        .show()
        .map(|_| ())
        .map_err(|error| error.to_string());
    if result.is_ok() {
        crate::runtime_trace::runtime_trace("notification", "native notification sent");
    }
    result
}

#[cfg(target_os = "macos")]
fn ensure_native_notification_application() {
    use std::sync::Once;

    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let identifier = native_notification_application_identifier();
        match notify_rust::set_application(&identifier) {
            Ok(()) => crate::runtime_trace::runtime_trace(
                "notification",
                &format!("native notification application={identifier}"),
            ),
            Err(error) => crate::runtime_trace::runtime_trace(
                "notification",
                &format!("native notification application failed id={identifier} error={error}"),
            ),
        }
    });
}

#[cfg(target_os = "macos")]
fn native_notification_application_identifier() -> String {
    if cfg!(debug_assertions) {
        let installed = notify_rust::get_bundle_identifier_or_default("Codux");
        if installed != "com.apple.Finder" {
            return installed;
        }
    }
    "com.duxweb.codux".to_string()
}

#[cfg(not(target_os = "macos"))]
fn ensure_native_notification_application() {}

fn native_notification_text(value: &str, fallback: &str) -> String {
    let value = value.trim();
    let text = if value.is_empty() { fallback } else { value };
    text.chars().take(512).collect()
}
