use crate::ai_history_normalized::AIHistorySnapshot;
use crate::git::{GitReviewSummary, GitSummary};
use crate::project_store::ProjectSummary;
use crate::worktree::WorktreeSummary;
use serde::Serialize;
use std::time::Instant;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectActivitySnapshot {
    pub tracked_count: usize,
    pub active_project_id: Option<String>,
    pub visible: bool,
    pub focused: bool,
    pub activated_git_count: usize,
    pub activated_ai_count: usize,
    pub queued_activation_count: usize,
    pub tracked_projects: Vec<ProjectActivityTrackedProject>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectActivityTrackedProject {
    pub id: String,
    pub name: String,
    pub path: String,
    pub has_git_refresh: bool,
    pub has_ai_refresh: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusEvent {
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub snapshot: GitSummary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSnapshotEvent {
    pub project_id: String,
    pub project_path: String,
    pub snapshot: WorktreeSummary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitProjectChangedEvent {
    pub project_path: String,
    pub repository_path: String,
    pub changed_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProjectActivityEvent {
    GitChanged {
        project_path: String,
        repository_path: String,
        changed_paths: Vec<String>,
    },
    GitStatus {
        project_id: String,
        project_name: String,
        project_path: String,
        snapshot: GitSummary,
        review: GitReviewSummary,
    },
    WorktreeSnapshot {
        project_id: String,
        project_path: String,
        snapshot: WorktreeSummary,
    },
    AIHistory {
        project_id: String,
        project_name: String,
        project_path: String,
        snapshot: AIHistorySnapshot,
    },
}

impl ProjectActivityEvent {
    pub fn git_status_event(&self) -> Option<GitStatusEvent> {
        match self {
            Self::GitStatus {
                project_id,
                project_name,
                project_path,
                snapshot,
                ..
            } => Some(GitStatusEvent {
                project_id: project_id.clone(),
                project_name: project_name.clone(),
                project_path: project_path.clone(),
                snapshot: snapshot.clone(),
            }),
            _ => None,
        }
    }

    pub fn worktree_snapshot_event(&self) -> Option<WorktreeSnapshotEvent> {
        match self {
            Self::WorktreeSnapshot {
                project_id,
                project_path,
                snapshot,
            } => Some(WorktreeSnapshotEvent {
                project_id: project_id.clone(),
                project_path: project_path.clone(),
                snapshot: snapshot.clone(),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TrackedProject {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) path: String,
    pub(super) runtime_target: crate::project_store::ProjectRuntimeTarget,
    pub(super) last_git_refresh: Option<Instant>,
    pub(super) last_remote_git_refresh: Option<Instant>,
    pub(super) last_git_changed_refresh: Option<Instant>,
    pub(super) last_ai_refresh: Option<Instant>,
}

impl From<ProjectSummary> for TrackedProject {
    fn from(project: ProjectSummary) -> Self {
        Self {
            id: project.id,
            name: project.name,
            path: project.path,
            runtime_target: project.runtime_target,
            last_git_refresh: None,
            last_remote_git_refresh: None,
            last_git_changed_refresh: None,
            last_ai_refresh: Some(Instant::now()),
        }
    }
}

impl From<TrackedProject> for ProjectSummary {
    fn from(project: TrackedProject) -> Self {
        Self {
            id: project.id,
            name: project.name,
            path: project.path,
            badge: String::new(),
            status: "active".to_string(),
            branch: "master".to_string(),
            changes: 0,
            badge_symbol: None,
            badge_color_hex: None,
            git_default_push_remote_name: None,
            environment_variables: Default::default(),
            runtime_target: project.runtime_target,
        }
    }
}
