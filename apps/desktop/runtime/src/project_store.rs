mod helpers;
mod mutations;
mod raw_state;
mod snapshot;
mod terminal_layout;
mod terminal_layout_store;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use helpers::badge_from_name;
pub use terminal_layout::{
    TerminalBottomTabRecord, TerminalLayoutRecord, TerminalLayoutsSnapshot, TerminalTopPaneRecord,
};
pub use types::*;

use serde_json::{Map, Value};
use std::path::PathBuf;

pub struct ProjectStore {
    pub(super) support_dir: PathBuf,
    pub(super) state_file: PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub enum ProjectMoveDirection {
    Up,
    Down,
}

impl ProjectStore {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            state_file: crate::config::state_file_path(&support_dir),
            support_dir,
        }
    }

    pub(super) fn raw_snapshot(&self) -> Map<String, Value> {
        crate::config::raw_state_snapshot(&self.state_file)
    }

    pub(super) fn save_raw_snapshot(&self, snapshot: &Map<String, Value>) -> Result<(), String> {
        crate::config::save_raw_state_snapshot(&self.state_file, snapshot)
    }

    pub(super) fn state_cache_file(&self) -> PathBuf {
        self.support_dir.join("state-cache.redb")
    }
}
