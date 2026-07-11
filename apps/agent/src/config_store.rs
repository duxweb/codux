//! The headless host's persisted configuration (`codux.toml`).
//!
//! `host_id` + `host_token` seed the iroh node identity (see
//! `host_secret_key`), so they must stay stable across restarts — otherwise the
//! node id (and every saved desktop's reconnect target) changes. They are
//! generated once on first `config` and preserved thereafter.

use serde::{Deserialize, Serialize};

use crate::paths;

pub const RELAY_PRESET_CUSTOM: &str = "custom";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct CoduxConfig {
    /// The name shown for this host on paired desktops.
    pub device_name: String,
    /// Stable logical host id (part of the node-identity seed).
    pub host_id: String,
    /// Stable secret seeding the iroh node identity. Treat as sensitive.
    pub host_token: String,
    /// Relay preset key (e.g. "global", "china", or "custom").
    pub relay_preset: String,
    /// Relay URL when `relay_preset` is "custom".
    pub relay_url: String,
    /// Optional bearer token for a custom relay.
    pub relay_authentication: String,
}

impl Default for CoduxConfig {
    fn default() -> Self {
        Self {
            device_name: default_device_name(),
            host_id: String::new(),
            host_token: String::new(),
            relay_preset: "global".to_string(),
            relay_url: String::new(),
            relay_authentication: String::new(),
        }
    }
}

impl CoduxConfig {
    /// Load the config, or a default if none exists yet.
    pub fn load() -> Self {
        std::fs::read_to_string(paths::config_path())
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// True if a config file exists on disk.
    pub fn exists() -> bool {
        paths::config_path().exists()
    }

    /// Fill in a useful display name and stable identity if absent. Returns
    /// whether anything was generated, so the caller can persist.
    pub fn ensure_identity(&mut self) -> Result<bool, String> {
        let host_id = self
            .host_id
            .trim()
            .is_empty()
            .then(|| random_hex(6))
            .transpose()?;
        let host_token = self
            .host_token
            .trim()
            .is_empty()
            .then(|| random_hex(32))
            .transpose()?;
        let mut generated = false;
        if is_placeholder_device_name(&self.device_name) {
            self.device_name = default_device_name();
            generated = true;
        }
        if let Some(host_id) = host_id {
            self.host_id = format!("codux-{host_id}");
            generated = true;
        }
        if let Some(host_token) = host_token {
            self.host_token = host_token;
            generated = true;
        }
        Ok(generated)
    }

    pub fn save(&self) -> Result<(), String> {
        paths::ensure_data_dir();
        let text = toml::to_string_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(paths::config_path(), text).map_err(|error| error.to_string())
    }
}

/// A lowercase hex string of `bytes` random bytes (2 chars per byte).
pub fn random_hex(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    getrandom::getrandom(&mut buf)
        .map_err(|error| format!("operating system random source unavailable: {error}"))?;
    Ok(buf.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// A sensible default device name (the machine's hostname).
pub fn default_device_name() -> String {
    display_host_name(platform_host_name(), platform_user_name())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "codux-agent".to_string())
}

fn is_placeholder_device_name(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value.eq_ignore_ascii_case("codux-agent")
}

fn display_host_name(host_name: Option<String>, user_name: Option<String>) -> Option<String> {
    let host_name = host_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    if !is_generic_apple_host_name(&host_name) {
        return Some(host_name);
    }
    user_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "root")
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
    command_output("/usr/sbin/scutil", &["--get", "ComputerName"])
        .or_else(|| command_output("/usr/sbin/scutil", &["--get", "LocalHostName"]))
        .or_else(|| command_output("hostname", &[]))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_host_name() -> Option<String> {
    command_output("hostname", &[])
}

#[cfg(windows)]
fn platform_host_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(command)
        .args(args)
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
