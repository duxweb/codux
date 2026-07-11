//! Persisted record of devices that have paired with this host (`devices.json`).
//! The running host upserts a device on pairing; the `device` CLI commands and
//! `status` read it.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use crate::paths;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PairedDevice {
    pub id: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub paired_at: String,
    #[serde(default)]
    pub last_seen: String,
}

const AUTHORIZATION_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileFingerprint {
    modified: Option<SystemTime>,
    len: u64,
}

struct AuthorizationState {
    credentials: HashMap<String, String>,
    fingerprint: Option<FileFingerprint>,
    last_check: Instant,
}

pub(crate) struct DeviceAuthorizationCache {
    path: PathBuf,
    refresh_interval: Duration,
    state: Mutex<AuthorizationState>,
}

impl DeviceAuthorizationCache {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self::with_refresh_interval(path, AUTHORIZATION_REFRESH_INTERVAL)
    }

    fn with_refresh_interval(path: PathBuf, refresh_interval: Duration) -> Self {
        let initial = load_from(&path).ok();
        Self {
            state: Mutex::new(AuthorizationState {
                credentials: initial
                    .as_deref()
                    .map(authorization_map)
                    .unwrap_or_default(),
                fingerprint: None,
                last_check: Instant::now(),
            }),
            path,
            refresh_interval,
        }
    }

    pub(crate) fn replace_after_write(&self, devices: &[PairedDevice]) {
        if let Ok(mut state) = self.state.lock() {
            state.credentials = authorization_map(devices);
            state.fingerprint = None;
            state.last_check = Instant::now();
        }
    }

    pub(crate) fn is_authorized(&self, id: &str, token: &str) -> bool {
        let id = id.trim();
        let token = token.trim();
        if id.is_empty() || token.is_empty() {
            return false;
        }
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if state.last_check.elapsed() >= self.refresh_interval {
            state.last_check = Instant::now();
            match file_fingerprint(&self.path) {
                Ok(fingerprint) if fingerprint == state.fingerprint => {}
                Ok(fingerprint) => match load_from(&self.path) {
                    Ok(devices) => {
                        state.credentials = authorization_map(&devices);
                        state.fingerprint = fingerprint;
                    }
                    Err(_) => {
                        state.credentials.clear();
                        state.fingerprint = None;
                    }
                },
                Err(_) => {
                    state.credentials.clear();
                    state.fingerprint = None;
                }
            }
        }
        state
            .credentials
            .get(id)
            .is_some_and(|value| value == token)
    }
}

fn authorization_map(devices: &[PairedDevice]) -> HashMap<String, String> {
    devices
        .iter()
        .filter_map(|device| {
            let id = device.id.trim();
            let token = device.token.trim();
            if id.is_empty() || token.is_empty() {
                None
            } else {
                Some((id.to_string(), token.to_string()))
            }
        })
        .collect()
}

fn file_fingerprint(path: &Path) -> Result<Option<FileFingerprint>, String> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(Some(FileFingerprint {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        })),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn load_from(path: &Path) -> Result<Vec<PairedDevice>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.to_string()),
    }
}

fn save_to(path: &Path, devices: &[PairedDevice]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(devices).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

fn load() -> Vec<PairedDevice> {
    load_from(&paths::devices_path()).unwrap_or_default()
}

fn save(devices: &[PairedDevice]) -> Result<(), String> {
    save_to(&paths::devices_path(), devices)
}

pub fn list() -> Vec<PairedDevice> {
    load()
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Upsert a device on (re)pairing: refresh token/name/platform/last_seen,
/// preserving the original pairing time.
pub(crate) fn record_at(
    path: &Path,
    id: &str,
    token: &str,
    name: &str,
    platform: &str,
) -> Result<Vec<PairedDevice>, String> {
    let mut devices = load_from(path)?;
    let timestamp = now();
    if let Some(existing) = devices.iter_mut().find(|device| device.id == id) {
        if !token.trim().is_empty() {
            existing.token = token.to_string();
        }
        if !name.trim().is_empty() {
            existing.name = name.to_string();
        }
        if !platform.trim().is_empty() {
            existing.platform = platform.to_string();
        }
        existing.last_seen = timestamp;
    } else {
        devices.push(PairedDevice {
            id: id.to_string(),
            token: token.to_string(),
            name: name.to_string(),
            platform: platform.to_string(),
            paired_at: timestamp.clone(),
            last_seen: timestamp,
        });
    }
    save_to(path, &devices)?;
    Ok(devices)
}

/// Remove a device by id. Returns whether it existed.
pub fn remove(id: &str) -> Result<bool, String> {
    let mut devices = load();
    let before = devices.len();
    devices.retain(|device| device.id != id);
    let removed = devices.len() != before;
    save(&devices)?;
    Ok(removed)
}

/// Rename a device by id. Returns whether it existed.
pub fn rename(id: &str, name: &str) -> Result<bool, String> {
    let mut devices = load();
    let found = if let Some(device) = devices.iter_mut().find(|device| device.id == id) {
        device.name = name.to_string();
        true
    } else {
        false
    };
    save(&devices)?;
    Ok(found)
}

pub fn clear() -> Result<usize, String> {
    let count = load().len();
    save(&[])?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codux-agent-device-{name}-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    #[test]
    fn authorization_cache_observes_external_revocation() {
        let path = path("revoke");
        let devices = record_at(&path, "phone", "token", "Phone", "ios").unwrap();
        let cache = DeviceAuthorizationCache::with_refresh_interval(path.clone(), Duration::ZERO);
        cache.replace_after_write(&devices);
        assert!(cache.is_authorized("phone", "token"));

        save_to(&path, &[]).unwrap();

        assert!(!cache.is_authorized("phone", "token"));
        std::fs::remove_file(path).ok();
    }
}
