use crate::ai_history_normalized::{
    AIGlobalHistorySnapshot, AIHistoryProjectRequest, AIHistorySnapshot, AIHistorySourceFingerprint,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;

pub(super) enum AIHistoryJob {
    Global {
        projects: Vec<AIHistoryProjectRequest>,
        database_path: PathBuf,
        reply: SyncSender<Result<AIGlobalHistorySnapshot, String>>,
    },
    RefreshProject {
        project: AIHistoryProjectRequest,
        database_path: PathBuf,
    },
    RefreshGlobal {
        projects: Vec<AIHistoryProjectRequest>,
        database_path: PathBuf,
    },
}

#[derive(Default)]
pub(super) struct AIHistoryIndexerState {
    pub(super) projects: HashMap<String, AIHistoryProjectState>,
    pub(super) queued_or_running_projects: HashSet<String>,
    pub(super) project_source_fingerprints: HashMap<String, AIHistorySourceFingerprint>,
    pub(super) next_version: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryProjectState {
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub snapshot: Option<AIHistorySnapshot>,
    pub is_loading: bool,
    pub queued: bool,
    pub progress: Option<f64>,
    pub detail: String,
    pub error: Option<String>,
    pub version: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AIHistoryEvent {
    Project {
        snapshot: AIHistorySnapshot,
    },
    ProjectState {
        state: AIHistoryProjectState,
    },
    Global {
        snapshot: AIGlobalHistorySnapshot,
    },
    Status {
        scope: String,
        project_id: Option<String>,
        is_loading: bool,
        detail: String,
    },
}
