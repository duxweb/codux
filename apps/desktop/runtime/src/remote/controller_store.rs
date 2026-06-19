//! Persisted dial targets for the desktop-as-controller: hosts this desktop has
//! paired with and can reconnect to without re-pairing. This is the OUTBOUND
//! counterpart to `RemoteDeviceSummary` (which records INBOUND devices paired
//! into this host). Stored as `remote-controllers.json` under the support dir.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// One reconnectable transport candidate for a saved host. The iroh pairing
/// ticket is single-use, so reconnect relies on `node_id` + `relay_url`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SavedRemoteTransport {
    pub kind: String,
    #[serde(default)]
    pub node_id: String,
    #[serde(default)]
    pub relay_url: String,
    #[serde(default)]
    pub relay_authentication: String,
}

/// A host this desktop has paired with, keyed by the device id we minted during
/// pairing (the host caches that id, so reconnect needs no fresh handshake).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedRemoteHost {
    pub device_id: String,
    pub host_id: String,
    #[serde(default)]
    pub host_name: String,
    #[serde(default)]
    pub device_token: String,
    #[serde(default)]
    pub transports: Vec<SavedRemoteTransport>,
}

pub struct RemoteControllerStore {
    path: PathBuf,
}

impl RemoteControllerStore {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            path: support_dir.join("remote-controllers.json"),
        }
    }

    pub fn list(&self) -> Vec<SavedRemoteHost> {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<SavedRemoteHost>>(&text).ok())
            .unwrap_or_default()
    }

    pub fn find(&self, device_id: &str) -> Option<SavedRemoteHost> {
        self.list()
            .into_iter()
            .find(|host| host.device_id == device_id)
    }

    /// Insert or replace a saved host (keyed by `device_id`).
    pub fn upsert(&self, host: SavedRemoteHost) -> Result<Vec<SavedRemoteHost>, String> {
        let mut hosts = self.list();
        if let Some(existing) = hosts
            .iter_mut()
            .find(|existing| existing.device_id == host.device_id)
        {
            *existing = host;
        } else {
            hosts.push(host);
        }
        self.save(&hosts)?;
        Ok(hosts)
    }

    pub fn remove(&self, device_id: &str) -> Result<Vec<SavedRemoteHost>, String> {
        let mut hosts = self.list();
        hosts.retain(|host| host.device_id != device_id);
        self.save(&hosts)?;
        Ok(hosts)
    }

    fn save(&self, hosts: &[SavedRemoteHost]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let text = serde_json::to_string_pretty(hosts).map_err(|error| error.to_string())?;
        fs::write(&self.path, text).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_support() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "codux-controller-store-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn upsert_find_remove_round_trip() {
        let dir = temp_support();
        let store = RemoteControllerStore::new(dir.clone());
        assert!(store.list().is_empty());

        let host = SavedRemoteHost {
            device_id: "dev-1".to_string(),
            host_id: "host-1".to_string(),
            host_name: "Studio".to_string(),
            device_token: String::new(),
            transports: vec![SavedRemoteTransport {
                kind: "iroh".to_string(),
                node_id: "node-abc".to_string(),
                relay_url: "https://relay.example".to_string(),
                relay_authentication: String::new(),
            }],
        };
        store.upsert(host.clone()).unwrap();

        // Reloaded from disk by a fresh store instance.
        let reloaded = RemoteControllerStore::new(dir.clone());
        assert_eq!(reloaded.find("dev-1"), Some(host.clone()));

        // Upsert replaces in place (no duplicate).
        let mut renamed = host.clone();
        renamed.host_name = "Studio Renamed".to_string();
        let hosts = reloaded.upsert(renamed.clone()).unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].host_name, "Studio Renamed");

        reloaded.remove("dev-1").unwrap();
        assert!(reloaded.list().is_empty());

        std::fs::remove_dir_all(dir).ok();
    }
}
