//! Crate-owned configuration types.
//!
//! The memory engine must run on the desktop *and* on the headless host (the
//! controller forwards a provider config so a remote-hosted project's memory is
//! generated where its AI sessions live). So the engine cannot depend on the
//! desktop's `MemoryConfig`/`MemoryProjectInfo` schema. It owns these narrow types
//! instead; the desktop converts its richer settings into them at the boundary,
//! and the controller forwards them as JSON. Field names/shape mirror the
//! desktop types so the moved engine code reads them unchanged.

use serde::{Deserialize, Serialize};

/// Memory's view of the AI settings (the subset the engine reads). Mirrors the
/// desktop `MemoryConfig` fields memory uses.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryConfig {
    #[serde(default)]
    pub global_prompt: String,
    #[serde(default)]
    pub memory: MemorySettings,
    #[serde(default)]
    pub providers: Vec<MemoryProvider>,
}

/// Mirrors the desktop `MemorySettings`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub automatic_injection_enabled: bool,
    #[serde(default = "default_true")]
    pub automatic_extraction_enabled: bool,
    #[serde(default = "default_true")]
    pub allow_cross_project_user_recall: bool,
    #[serde(default)]
    pub default_extractor_provider_id: String,
    #[serde(default = "default_memory_user_recall")]
    pub max_injected_user_working_memories: i32,
    #[serde(default = "default_memory_project_recall")]
    pub max_injected_project_working_memories: i32,
    #[serde(default = "default_memory_max_active_working_entries")]
    pub max_active_working_entries: i32,
    #[serde(default = "default_memory_max_summary_versions")]
    pub max_summary_versions: i32,
    #[serde(default = "default_memory_summary_target_token_budget")]
    pub summary_target_token_budget: i32,
    #[serde(default = "default_memory_max_injected_summary_tokens")]
    pub max_injected_summary_tokens: i32,
    #[serde(default = "default_memory_extraction_idle_delay_seconds")]
    pub extraction_idle_delay_seconds: i32,
    #[serde(default = "default_memory_session_extraction_cooldown_seconds")]
    pub session_extraction_cooldown_seconds: i32,
    #[serde(default = "default_memory_max_index_sessions")]
    pub max_index_sessions: i32,
    #[serde(default = "default_memory_max_extraction_transcript_lines")]
    pub max_extraction_transcript_lines: i32,
    #[serde(default = "default_memory_max_extraction_transcript_tokens")]
    pub max_extraction_transcript_tokens: i32,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            automatic_injection_enabled: true,
            automatic_extraction_enabled: true,
            allow_cross_project_user_recall: true,
            default_extractor_provider_id: String::new(),
            max_injected_user_working_memories: default_memory_user_recall(),
            max_injected_project_working_memories: default_memory_project_recall(),
            max_active_working_entries: default_memory_max_active_working_entries(),
            max_summary_versions: default_memory_max_summary_versions(),
            summary_target_token_budget: default_memory_summary_target_token_budget(),
            max_injected_summary_tokens: default_memory_max_injected_summary_tokens(),
            extraction_idle_delay_seconds: default_memory_extraction_idle_delay_seconds(),
            session_extraction_cooldown_seconds:
                default_memory_session_extraction_cooldown_seconds(),
            max_index_sessions: default_memory_max_index_sessions(),
            max_extraction_transcript_lines: default_memory_max_extraction_transcript_lines(),
            max_extraction_transcript_tokens: default_memory_max_extraction_transcript_tokens(),
        }
    }
}

/// Mirrors the desktop `MemoryProvider`. Converted to [`codux_llm::LlmProvider`]
/// for the actual completion call.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProvider {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_true")]
    pub use_for_memory_extraction: bool,
    #[serde(default)]
    pub priority: i32,
}

impl MemoryProvider {
    pub fn to_llm_provider(&self) -> codux_llm::LlmProvider {
        codux_llm::LlmProvider {
            id: self.id.clone(),
            kind: self.kind.clone(),
            display_name: self.display_name.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
        }
    }
}

/// Memory's view of a project (the engine reads only id/name/path). Mirrors the
/// desktop `MemoryProjectInfo` subset.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProjectInfo {
    pub id: String,
    pub name: String,
    pub path: String,
}

/// Memory's view of a live AI session snapshot. Mirrors the desktop
/// `AISessionSnapshot` field-for-field (the engine constructs synthetic
/// snapshots from history, so it needs the full shape). The desktop converts
/// its snapshot into this at the enqueue boundary; the controller forwards it as
/// JSON for a remote-hosted project.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MemorySessionSnapshot {
    pub terminal_id: String,
    pub terminal_instance_id: Option<String>,
    pub project_id: String,
    pub project_name: String,
    pub project_path: Option<String>,
    pub session_title: String,
    pub tool: String,
    pub ai_session_id: Option<String>,
    pub model: Option<String>,
    pub state: String,
    pub status: String,
    pub is_running: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub total_tokens: i64,
    pub baseline_total_tokens: i64,
    pub baseline_cached_input_tokens: i64,
    pub baseline_resolved: bool,
    pub started_at: Option<f64>,
    pub updated_at: f64,
    pub active_turn_started_at: Option<f64>,
    pub runtime_turn_started_at: Option<f64>,
    pub completed_turn_started_at: Option<f64>,
    pub has_completed_turn: bool,
    pub was_interrupted: bool,
    pub transcript_path: Option<String>,
    pub notification_type: Option<String>,
    pub target_tool_name: Option<String>,
    pub message: Option<String>,
    pub latest_assistant_preview: Option<String>,
    pub plan: Option<MemoryPlanSnapshot>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MemoryPlanSnapshot {
    pub source: String,
    pub session_id: String,
    pub updated_at: f64,
    pub items: Vec<MemoryPlanItem>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MemoryPlanItem {
    pub text: String,
    pub status: String,
    pub priority: Option<String>,
}

/// Memory's view of a workspace record. Mirrors the desktop
/// `MemoryProjectRecord`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProjectRecord {
    pub id: String,
    pub root_project_id: String,
    pub root_project_name: String,
    pub root_project_path: String,
    pub workspace_path: String,
    pub git_default_push_remote_name: Option<String>,
}

/// Trim a value and drop it when empty. Mirrors `ai_runtime::state::normalized_string`.
pub fn normalized_string(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// The user's home directory. Mirrors `runtime_paths::home_dir`; the transcript
/// subsystem joins it to locate AI tool logs (~/.claude, ~/.codex, …), which is
/// identical on the desktop and the host.
pub fn home_dir() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(windows_user_profile)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

#[cfg(target_os = "windows")]
fn windows_user_profile() -> Option<std::path::PathBuf> {
    std::env::var_os("USERPROFILE")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let drive = std::env::var_os("HOMEDRIVE")?;
            let path = std::env::var_os("HOMEPATH")?;
            let mut home = std::path::PathBuf::from(drive);
            home.push(path);
            Some(home)
        })
}

#[cfg(not(target_os = "windows"))]
fn windows_user_profile() -> Option<std::path::PathBuf> {
    None
}

fn default_true() -> bool {
    true
}
fn default_memory_user_recall() -> i32 {
    4
}
fn default_memory_project_recall() -> i32 {
    6
}
fn default_memory_max_active_working_entries() -> i32 {
    50
}
fn default_memory_max_summary_versions() -> i32 {
    10
}
fn default_memory_summary_target_token_budget() -> i32 {
    900
}
fn default_memory_max_injected_summary_tokens() -> i32 {
    900
}
fn default_memory_extraction_idle_delay_seconds() -> i32 {
    300
}
fn default_memory_session_extraction_cooldown_seconds() -> i32 {
    900
}
fn default_memory_max_index_sessions() -> i32 {
    20
}
fn default_memory_max_extraction_transcript_lines() -> i32 {
    80
}
fn default_memory_max_extraction_transcript_tokens() -> i32 {
    8000
}
