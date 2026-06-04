use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRuntimeSummary {
    pub path: String,
    pub active_terminal_id: String,
    pub active_slot_id: String,
    pub open_count: usize,
    pub closed_count: usize,
    pub sessions: Vec<TerminalRuntimeSessionSummary>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRuntimeSessionSummary {
    pub terminal_id: String,
    pub slot_id: String,
    pub tab_id: String,
    pub pane_index: usize,
    pub title: String,
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub cwd: String,
    pub status: String,
    pub is_running: bool,
    pub created_at: f64,
    pub last_active_at: f64,
    pub has_buffer: bool,
    pub buffer_characters: usize,
    #[serde(default)]
    pub input_bytes: usize,
    #[serde(default)]
    pub last_input_at: Option<f64>,
    #[serde(default)]
    pub input_history: Vec<TerminalInputSummary>,
    #[serde(default)]
    pub output_bytes: usize,
    #[serde(default)]
    pub output_tail: String,
    pub source: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInputSummary {
    pub text: String,
    pub bytes: usize,
    pub timestamp: f64,
}

#[derive(Clone, Debug)]
pub struct TerminalRuntimeSessionInput {
    pub terminal_id: String,
    pub slot_id: String,
    pub tab_id: String,
    pub pane_index: usize,
    pub title: String,
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub cwd: String,
    pub input_bytes: usize,
    pub input_history: Vec<TerminalInputSummary>,
    pub output_bytes: usize,
    pub output_tail: String,
}
