//! Daemon runtime state shared between the running host and the CLI: a
//! single-instance advisory lock, the published status, and the pairing ticket.

use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::path::Path;

use crate::paths;

/// Held for the daemon's lifetime; dropping it releases the single-instance lock.
pub struct InstanceLock {
    _file: File,
}

/// Acquire the single-instance lock, or report that the host is already running.
pub fn acquire_instance_lock() -> Result<InstanceLock, String> {
    paths::ensure_data_dir();
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(paths::lock_path())
        .map_err(|error| format!("failed to open lock file: {error}"))?;
    match FileExt::try_lock_exclusive(&file) {
        Ok(true) => Ok(InstanceLock { _file: file }),
        Ok(false) => Err("the Codux host is already running".to_string()),
        Err(error) => Err(format!("failed to acquire lock: {error}")),
    }
}

/// Whether a host instance currently holds the lock.
pub fn is_running() -> bool {
    let Ok(file) = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(paths::lock_path())
    else {
        return false;
    };
    match FileExt::try_lock_exclusive(&file) {
        // We acquired it — nobody else holds it, so nothing is running. Release.
        Ok(true) => {
            let _ = FileExt::unlock(&file);
            false
        }
        // Could not acquire — a daemon holds it.
        Ok(false) => true,
        Err(_) => false,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub pid: u32,
    pub started_at: String,
    pub host_id: String,
    pub device_name: String,
    pub node_id: String,
    pub relay: String,
    #[serde(default)]
    pub web_test_url: String,
}

pub fn write_status(status: &DaemonStatus) {
    if let Ok(text) = serde_json::to_string_pretty(status) {
        let _ = std::fs::write(paths::status_path(), text);
    }
}

pub fn read_status() -> Option<DaemonStatus> {
    let text = std::fs::read_to_string(paths::status_path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn clear_status() {
    let _ = std::fs::remove_file(paths::status_path());
}

/// Publish the pasteable `codux://pair` ticket for `link`/`qrcode`.
pub fn write_ticket(ticket: &str) -> Result<(), String> {
    write_ticket_at(&paths::ticket_path(), ticket)
}

pub(crate) fn write_ticket_at(path: &Path, ticket: &str) -> Result<(), String> {
    std::fs::write(path, ticket).map_err(|error| format!("failed to write pairing ticket: {error}"))
}

pub fn read_ticket() -> Option<String> {
    std::fs::read_to_string(paths::ticket_path())
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

pub fn clear_ticket() {
    let _ = std::fs::remove_file(paths::ticket_path());
}
