use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::TerminalSequence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalBaselineRequest {
    pub session_id: String,
    pub request_id: Option<String>,
    pub offset: usize,
    pub max_chars: usize,
    pub chunk_chars: Option<usize>,
    pub tail: bool,
    pub resume_from_seq: Option<TerminalSequence>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLaunchConfig {
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub command: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub scrollback_lines: Option<usize>,
    pub env: Option<HashMap<String, String>>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub terminal_id: Option<String>,
    pub slot_id: Option<String>,
    pub session_key: Option<String>,
    pub worktree_id: Option<String>,
    pub title: Option<String>,
    pub tool: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TerminalEvent {
    Output {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        text: String,
        #[serde(skip)]
        bytes: Vec<u8>,
        #[serde(rename = "bufferLength")]
        buffer_length: usize,
        #[serde(rename = "bufferEnd")]
        buffer_end: usize,
    },
    Exit {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "exitCode")]
        exit_code: Option<i32>,
    },
    Error {
        #[serde(rename = "sessionId")]
        session_id: String,
        message: String,
    },
    Viewport {
        #[serde(rename = "sessionId")]
        session_id: String,
        owner: String,
        cols: u16,
        rows: u16,
        generation: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalViewportState {
    pub owner: String,
    pub cols: u16,
    pub rows: u16,
    pub generation: u64,
    /// Friendly name of the current REMOTE owner (for the desktop "handed off"
    /// placeholder). None when the local host owns it or the name is unknown.
    pub owner_label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionSnapshot {
    pub id: String,
    pub title: String,
    pub slot_id: String,
    pub session_key: Option<String>,
    pub project_id: String,
    #[serde(
        default,
        rename = "worktreeId",
        skip_serializing_if = "Option::is_none"
    )]
    pub worktree_id: Option<String>,
    pub project_name: String,
    pub cwd: String,
    pub shell: String,
    pub command: String,
    pub cols: u16,
    pub rows: u16,
    pub status: String,
    pub is_running: bool,
    pub created_at: String,
    pub last_active_at: String,
    pub buffer_characters: usize,
    pub has_buffer: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

pub type TerminalEventSink = Box<dyn Fn(TerminalEvent) -> bool + Send + Sync + 'static>;

pub trait TerminalSessionHandle: Send + Sync {
    fn id(&self) -> &str;
    fn info(&self) -> TerminalSessionSnapshot;
    fn write(&self, data: &[u8]) -> Result<(), String>;
    fn resize(&self, cols: u16, rows: u16) -> Result<(), String>;
    fn claim_viewport(&self, owner: &str) -> Result<TerminalViewportState, String>;
    fn release_viewport(&self, owner: &str) -> Result<Option<TerminalViewportState>, String>;
    fn resize_viewport(
        &self,
        owner: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Option<TerminalViewportState>, String>;
    fn viewport_state(&self) -> TerminalViewportState;
    fn snapshot(&self) -> String;
    fn snapshot_tail(&self, max_chars: usize) -> (String, usize);
    fn buffer_characters(&self) -> usize;
    fn clear_history(&self);
    fn kill(&self) -> Result<(), String>;
}

pub trait TerminalDriver: Send + Sync {
    type Session: TerminalSessionHandle + Clone + 'static;

    fn list(&self) -> Vec<TerminalSessionSnapshot>;
    fn create(
        &self,
        config: TerminalLaunchConfig,
        emit: TerminalEventSink,
    ) -> Result<Self::Session, String>;
    fn session(&self, session_id: &str) -> Result<Self::Session, String>;
    fn remove(&self, session_id: &str) -> Result<(), String>;
    fn subscribe_events(&self, session_id: &str, emit: TerminalEventSink) -> Result<(), String>;
}
