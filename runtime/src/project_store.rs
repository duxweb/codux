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
use std::{fs, path::PathBuf};

pub struct ProjectStore {
    state_file: PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub enum ProjectMoveDirection {
    Up,
    Down,
}

impl ProjectStore {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            state_file: support_dir.join("state.json"),
        }
    }

    pub(super) fn raw_snapshot(&self) -> Map<String, Value> {
        fs::read_to_string(&self.state_file)
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    }

    pub(super) fn save_raw_snapshot(&self, snapshot: &Map<String, Value>) -> Result<(), String> {
        if let Some(parent) = self.state_file.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let content = serde_json::to_string_pretty(snapshot).map_err(|error| error.to_string())?;
        fs::write(&self.state_file, format!("{content}\n")).map_err(|error| error.to_string())
    }
}
