use super::types::{RemotePairingInfo, RemoteSettings, RemoteTransportCandidate};
// E2E crypto (key derivation, encrypt/decrypt, base64url) lives in the shared
// codux-remote-crypto crate so the host and the mobile device (via FFI) stay
// byte-compatible. Re-exported here so existing call sites are unchanged.
pub(crate) use codux_remote_crypto::{
    remote_base64_url_decode, remote_base64_url_encode, remote_e2e_decrypt, remote_e2e_encrypt,
    remote_e2e_symmetric_key,
};
use serde_json::Value;
use serde_json::json;
#[cfg(unix)]
use std::ffi::CStr;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

pub(crate) fn remote_host_name() -> String {
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

pub(crate) fn ensure_remote_host_identity(settings: &mut RemoteSettings) {
    if let Some(private_key) = remote_e2e_private_key(&settings.host_private_key) {
        let public_key = X25519PublicKey::from(&private_key);
        let derived_public = remote_base64_url_encode(public_key.as_bytes());
        if settings.host_public_key.trim().is_empty() || settings.host_public_key == derived_public
        {
            settings.host_public_key = derived_public;
            return;
        }
    }
    let mut bytes = [0_u8; 32];
    bytes[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    let private_key = StaticSecret::from(bytes);
    let public_key = X25519PublicKey::from(&private_key);
    settings.host_private_key = remote_base64_url_encode(private_key.to_bytes().as_slice());
    settings.host_public_key = remote_base64_url_encode(public_key.as_bytes());
}

pub(crate) fn remote_pairing_payload(
    settings: &RemoteSettings,
    pairing: &RemotePairingInfo,
    transports: Vec<RemoteTransportCandidate>,
) -> Value {
    json!({
        "code": pairing.code,
        "secret": pairing.secret,
        "pairingId": pairing.pairing_id,
        "hostId": settings.host_id,
        "hostName": remote_host_name(),
        "hostPublicKey": settings.host_public_key,
        "cryptoVersion": 1,
        "protocolVersion": super::protocol::REMOTE_PROTOCOL_VERSION,
        "transports": transports,
    })
}

fn remote_e2e_private_key(value: &str) -> Option<StaticSecret> {
    let bytes = remote_base64_url_decode(value).ok()?;
    let array: [u8; 32] = bytes.as_slice().try_into().ok()?;
    Some(StaticSecret::from(array))
}
