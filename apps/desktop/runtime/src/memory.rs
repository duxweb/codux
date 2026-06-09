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

use crate::runtime_state::ProjectInfo;
use rusqlite::{Connection, OptionalExtension, params};
use std::{fs, path::PathBuf};

impl MemoryService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            database_path: support_dir.join("memory.sqlite3"),
        }
    }

    pub(super) fn open_connection(&self) -> Result<Connection, String> {
        if !self.database_path.is_file() {
            return Err("memory.sqlite3 not found".to_string());
        }
        Connection::open(&self.database_path).map_err(|error| error.to_string())
    }

    pub(super) fn open_or_create_connection(&self) -> Result<Connection, String> {
        if let Some(parent) = self.database_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        Connection::open(&self.database_path).map_err(|error| error.to_string())
    }
}

include!("memory/service_summary.rs");
include!("memory/service_management.rs");
include!("memory/service_launch.rs");

pub(super) fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests;
