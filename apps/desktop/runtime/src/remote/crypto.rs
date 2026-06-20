use super::types::{RemotePairingInfo, RemoteSettings, RemoteTransportCandidate};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Map, Value, json};
#[cfg(unix)]
use std::ffi::CStr;

pub fn remote_host_name() -> String {
    display_host_name(platform_host_name(), platform_user_name())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| crate::runtime_paths::app_display_name().to_string())
}

pub(crate) fn display_host_name(
    host_name: Option<String>,
    user_name: Option<String>,
) -> Option<String> {
    let host_name = host_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    if !is_generic_apple_host_name(&host_name) {
        return Some(host_name);
    }
    let user_name = user_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    user_name
        .map(|user| format!("{user}的Apple电脑"))
        .or(Some(host_name))
}

fn is_generic_apple_host_name(value: &str) -> bool {
    let normalized: String = value
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '\'' && *ch != '’')
        .flat_map(char::to_lowercase)
        .collect();
    matches!(
        normalized.as_str(),
        "apple的apple电脑" | "apple的mac" | "applemac" | "applecomputer" | "apple的电脑"
    )
}

fn platform_user_name() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "root")
}

#[cfg(target_os = "macos")]
fn platform_host_name() -> Option<String> {
    macos_scutil_value("ComputerName")
        .or_else(|| macos_scutil_value("LocalHostName"))
        .or_else(unix_host_name)
}

#[cfg(target_os = "macos")]
fn macos_scutil_value(key: &str) -> Option<String> {
    let output = std::process::Command::new("/usr/sbin/scutil")
        .args(["--get", key])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_host_name() -> Option<String> {
    unix_host_name()
}

#[cfg(unix)]
fn unix_host_name() -> Option<String> {
    let mut buffer = [0_i8; 256];
    let result = unsafe { libc::gethostname(buffer.as_mut_ptr(), buffer.len()) };
    if result != 0 {
        return None;
    }
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_str()
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "windows")]
fn platform_host_name() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(not(any(unix, target_os = "windows")))]
fn platform_host_name() -> Option<String> {
    None
}

pub(crate) fn remote_random_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

pub(crate) fn remote_base64_url_encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn remote_pairing_payload(
    _settings: &RemoteSettings,
    pairing: &RemotePairingInfo,
    transports: Vec<RemoteTransportCandidate>,
) -> Value {
    let transports = transports
        .into_iter()
        .map(|transport| {
            let mut item = Map::new();
            item.insert("kind".to_string(), json!(transport.kind));
            if let Some(ticket) = transport.ticket.filter(|value| !value.trim().is_empty()) {
                item.insert("ticket".to_string(), json!(ticket));
            }
            if let Some(authentication) = transport
                .relay_authentication
                .filter(|value| !value.trim().is_empty())
            {
                item.insert("relayAuthentication".to_string(), json!(authentication));
            }
            Value::Object(item)
        })
        .collect::<Vec<_>>();

    json!({
        "code": pairing.code,
        "secret": pairing.secret,
        "pairingId": pairing.pairing_id,
        "transports": transports,
    })
}
