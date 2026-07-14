use crate::ai_history_indexer::AIHistoryIndexer;
use crate::ai_history_normalized::AIHistoryProjectRequest;
use crate::project_store::{ProjectRecord, ProjectStore, ProjectSummary};
use crate::runtime_trace::runtime_trace;
use crate::settings::SettingsSummary;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod git_jobs;
mod intervals;
#[cfg(test)]
mod tests;
mod types;

use git_jobs::{GitJob, GitJobQueue};
use intervals::{configured_interval_seconds, projects_due_for_git_interval, upsert_project};
use types::TrackedProject;
pub use types::{
    GitProjectChangedEvent, GitStatusEvent, ProjectActivityEvent, ProjectActivitySnapshot,
    ProjectActivityTrackedProject, WorktreeSnapshotEvent,
};

const MIN_GIT_REFRESH_SECONDS: u64 = 15;
const MIN_AI_REFRESH_SECONDS: u64 = 120;
const MAX_BACKGROUND_GIT_REFRESH_PER_TICK: usize = 0;
const MAX_AI_REFRESH_PER_TICK: usize = 1;

pub struct ProjectActivityCoordinator {
    support_dir: std::path::PathBuf,
    ai_history: AIHistoryIndexer,
    events: Arc<Mutex<VecDeque<ProjectActivityEvent>>>,
    projects: Mutex<HashMap<String, TrackedProject>>,
    active_project_id: Mutex<Option<String>>,
    main_window_visible: AtomicBool,
    main_window_focused: AtomicBool,
    activated_git_projects: Mutex<HashSet<String>>,
    activated_ai_projects: Mutex<HashSet<String>>,
    git_jobs: GitJobQueue,
}

impl fmt::Debug for ProjectActivityCoordinator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjectActivityCoordinator")
            .field("support_dir", &self.support_dir)
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

impl ProjectActivityCoordinator {
    pub fn new(support_dir: std::path::PathBuf, ai_history: AIHistoryIndexer) -> Self {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let git_jobs = GitJobQueue::new(support_dir.clone(), events.clone());
        Self {
            support_dir,
            ai_history,
            events: Arc::clone(&events),
            projects: Mutex::new(HashMap::new()),
            active_project_id: Mutex::new(None),
            main_window_visible: AtomicBool::new(false),
            main_window_focused: AtomicBool::new(false),
            activated_git_projects: Mutex::new(HashSet::new()),
            activated_ai_projects: Mutex::new(HashSet::new()),
            git_jobs,
        }
    }
}

include!("project_activity/lifecycle.rs");
include!("project_activity/refresh.rs");
include!("project_activity/conversions.rs");
