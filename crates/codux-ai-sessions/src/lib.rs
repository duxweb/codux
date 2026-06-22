//! The AI-session history layer (project/global summaries, session detail, fork,
//! rename, remove), extracted from the desktop runtime so the headless host can
//! serve a remote-hosted project's sessions. Operates on the shared
//! `ai-usage.sqlite3` the indexer populates.
//!
//! The desktop keeps its live-stats view (`stats_view`, which merges
//! `AIRuntimeStateSummary`) on top of this crate; only the DB/session layer moves.

mod dispatch;
mod helpers;
mod queries;
mod restore;
mod session_fork;
mod sessions;
mod summary;
#[cfg(test)]
mod tests;
mod types;

pub use dispatch::{session_op_payload, session_op_result};
pub use restore::session_restore_command;
pub use types::*;

use rusqlite::Connection;
use std::path::PathBuf;

pub struct AIHistoryService {
    database_path: PathBuf,
}

impl AIHistoryService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            database_path: support_dir.join("ai-usage.sqlite3"),
        }
    }

    fn open_connection(&self) -> Result<Connection, String> {
        if !self.database_path.is_file() {
            return Err("ai-usage.sqlite3 not found".to_string());
        }
        Connection::open(&self.database_path).map_err(|error| error.to_string())
    }
}
