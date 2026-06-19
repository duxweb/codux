mod apply;
mod compat;
pub mod extraction;
mod launch;
mod management;
mod manual;
mod profile;
mod queries;
mod queue;
mod transcript;
mod types;

pub use types::*;

pub use apply::{
    MemoryDecisionLog, MemoryEntryStatus, MemoryWriteDecisionKind, StoredMemoryEntry,
    StoredMemorySummary,
};
pub use extraction::{MemoryKind, MemoryScope, MemoryTier};
pub use launch::launch_artifact_paths;
use launch::{render_launch_memory_index, render_recent_memory};
pub use manual::MemoryExtractionEnqueueResult;
use queries::*;
pub use queue::{
    MemoryEnqueueResult, MemoryExtractionStatus, MemoryExtractionStatusSnapshot,
    MemoryExtractionTask,
};

use rusqlite::{Connection, OptionalExtension, params};
use std::{fs, path::PathBuf};

impl MemoryService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            database_path: support_dir.join("memory.sqlite3"),
        }
    }

    pub(crate) fn open_connection(&self) -> Result<Connection, String> {
        if !self.database_path.is_file() {
            return Err("memory.sqlite3 not found".to_string());
        }
        let connection =
            Connection::open(&self.database_path).map_err(|error| error.to_string())?;
        initialize_memory_connection(&connection)?;
        Ok(connection)
    }

    pub(crate) fn open_or_create_connection(&self) -> Result<Connection, String> {
        if let Some(parent) = self.database_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let connection =
            Connection::open(&self.database_path).map_err(|error| error.to_string())?;
        initialize_memory_connection(&connection)?;
        Ok(connection)
    }
}

/// Concurrency-safe connection setup, mirroring the AI-usage store
/// (`ai_usage_store/connection.rs`). The memory DB has several in-process
/// accessors at once -- the 300ms status poller (reader), the queue worker
/// (writer), enqueue-on-completion (writer), and the profile refresh (writer).
/// With the SQLite defaults (rollback journal + `busy_timeout = 0`) any
/// contention returns `database is locked` immediately. WAL lets a reader and a
/// writer proceed without blocking each other, and the busy_timeout makes a
/// contended writer wait briefly instead of failing.
fn initialize_memory_connection(connection: &Connection) -> Result<(), String> {
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| error.to_string())?;
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|error| error.to_string())?;
    Ok(())
}

include!("service_summary.rs");
include!("service_management.rs");
include!("service_launch.rs");

pub(crate) fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests;
