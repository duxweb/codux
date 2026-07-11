use crate::config::ConfigStore;
use crate::remote::remote_settings_from_raw;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

const FILE_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileFingerprint {
    modified: Option<SystemTime>,
    len: u64,
}

struct AuthorizationState {
    configured_credentials: HashMap<String, String>,
    persisted_credentials: HashMap<String, String>,
    config_revision: u64,
    file_fingerprint: Option<FileFingerprint>,
    last_file_check: Instant,
}

pub(super) struct RemoteAuthorizationCache {
    settings_path: PathBuf,
    store: Arc<ConfigStore>,
    refresh_interval: Duration,
    state: Mutex<AuthorizationState>,
}

impl RemoteAuthorizationCache {
    pub(super) fn new(settings_path: PathBuf) -> Self {
        Self::with_refresh_interval(settings_path, FILE_REFRESH_INTERVAL)
    }

    fn with_refresh_interval(settings_path: PathBuf, refresh_interval: Duration) -> Self {
        let store = ConfigStore::for_file(settings_path.clone());
        let config_revision = store.revision();
        let configured_credentials = credentials_from_raw(&store.snapshot());
        let persisted_credentials = read_settings(&settings_path)
            .ok()
            .map(|raw| credentials_from_raw(&raw))
            .unwrap_or_default();
        Self {
            settings_path,
            store,
            refresh_interval,
            state: Mutex::new(AuthorizationState {
                configured_credentials,
                persisted_credentials,
                config_revision,
                file_fingerprint: None,
                last_file_check: Instant::now(),
            }),
        }
    }

    pub(super) fn is_authorized(&self, device_id: &str, device_token: &str) -> bool {
        let device_id = device_id.trim();
        let device_token = device_token.trim();
        if device_id.is_empty() || device_token.is_empty() {
            return false;
        }
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        self.refresh_if_needed(&mut state);
        state
            .configured_credentials
            .get(device_id)
            .is_some_and(|token| token == device_token)
            && state
                .persisted_credentials
                .get(device_id)
                .is_some_and(|token| token == device_token)
    }

    pub(super) fn refresh_after_persisted_write(&self) {
        if let Ok(mut state) = self.state.lock() {
            let credentials = credentials_from_raw(&self.store.snapshot());
            state.configured_credentials = credentials.clone();
            state.persisted_credentials = credentials;
            state.config_revision = self.store.revision();
            state.file_fingerprint = None;
            state.last_file_check = Instant::now();
        }
    }

    fn refresh_if_needed(&self, state: &mut AuthorizationState) {
        let config_revision = self.store.revision();
        if config_revision != state.config_revision {
            state.configured_credentials = credentials_from_raw(&self.store.snapshot());
            state.config_revision = config_revision;
            state.file_fingerprint = None;
            state.last_file_check = Instant::now();
            return;
        }
        if state.last_file_check.elapsed() < self.refresh_interval {
            return;
        }
        state.last_file_check = Instant::now();
        let Ok(fingerprint) = file_fingerprint(&self.settings_path) else {
            state.persisted_credentials.clear();
            state.file_fingerprint = None;
            return;
        };
        if fingerprint == state.file_fingerprint {
            return;
        }
        match read_settings(&self.settings_path) {
            Ok(raw) => {
                state.persisted_credentials = credentials_from_raw(&raw);
                state.file_fingerprint = fingerprint;
            }
            Err(_) => {
                state.persisted_credentials.clear();
                state.file_fingerprint = None;
            }
        }
    }
}

fn credentials_from_raw(raw: &Map<String, Value>) -> HashMap<String, String> {
    remote_settings_from_raw(raw)
        .cached_devices
        .into_iter()
        .filter(|device| {
            device
                .revoked_at
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
        })
        .filter_map(|device| {
            let device_id = device.id.trim();
            let device_token = device.device_token.trim();
            if device_id.is_empty() || device_token.is_empty() {
                None
            } else {
                Some((device_id.to_string(), device_token.to_string()))
            }
        })
        .collect()
}

fn read_settings(path: &Path) -> Result<Map<String, Value>, String> {
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str::<Value>(&content)
            .map_err(|error| error.to_string())?
            .as_object()
            .cloned()
            .ok_or_else(|| "settings root must be an object".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(error.to_string()),
    }
}

fn file_fingerprint(path: &Path) -> Result<Option<FileFingerprint>, String> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(FileFingerprint {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        })),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn settings_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codux-auth-{name}-{}.json", Uuid::new_v4()))
    }

    fn settings(token: &str) -> Map<String, Value> {
        json!({
            "remote": {
                "cachedDevices": [{ "id": "device-1", "token": token }]
            }
        })
        .as_object()
        .cloned()
        .unwrap()
    }

    #[test]
    fn config_revision_revokes_credentials_immediately() {
        let path = settings_path("revision");
        fs::write(&path, serde_json::to_vec(&settings("token-1")).unwrap()).unwrap();
        let cache = RemoteAuthorizationCache::new(path.clone());
        assert!(cache.is_authorized("device-1", "token-1"));

        ConfigStore::for_file(path.clone())
            .save_snapshot(&Map::new())
            .unwrap();

        assert!(!cache.is_authorized("device-1", "token-1"));
        fs::remove_file(path).ok();
    }

    #[test]
    fn external_file_revocation_invalidates_cached_credentials() {
        let path = settings_path("external-revoke");
        fs::write(&path, serde_json::to_vec(&settings("token-1")).unwrap()).unwrap();
        let cache = RemoteAuthorizationCache::with_refresh_interval(path.clone(), Duration::ZERO);
        assert!(cache.is_authorized("device-1", "token-1"));

        fs::write(&path, br#"{"remote":{"cachedDevices":[]}}"#).unwrap();

        assert!(!cache.is_authorized("device-1", "token-1"));
        fs::remove_file(path).ok();
    }
}
