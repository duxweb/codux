use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSummary {
    pub available: bool,
    pub selected_worktree_id: Option<String>,
    pub worktrees: Vec<WorktreeInfo>,
    pub tasks: Vec<WorktreeTaskInfo>,
    pub active_git: crate::git::GitSummary,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub status: String,
    pub is_default: bool,
    pub exists: bool,
    pub git_summary: ProjectWorktreeGitSummary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeTaskInfo {
    pub worktree_id: String,
    pub title: String,
    pub base_branch: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSnapshot {
    pub project_id: String,
    pub selected_worktree_id: String,
    pub worktrees: Vec<ProjectWorktreeSnapshot>,
    pub tasks: Vec<WorktreeTaskSnapshot>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorktreeSnapshot {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub status: String,
    pub is_default: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub git_summary: ProjectWorktreeGitSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorktreeGitSummary {
    pub changes: usize,
    pub incoming: i64,
    pub outgoing: i64,
    pub additions: i64,
    pub deletions: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeTaskSnapshot {
    pub worktree_id: String,
    pub title: String,
    pub base_branch: String,
    pub base_commit: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreateRequest {
    pub project_id: String,
    pub project_path: String,
    pub base_branch: Option<String>,
    pub branch_name: String,
    pub task_title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemoveRequest {
    pub project_id: String,
    pub project_path: String,
    pub worktree_path: String,
    #[serde(default)]
    pub remove_branch: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeMergeRequest {
    pub project_id: String,
    pub project_path: String,
    pub worktree_path: String,
    pub base_branch: Option<String>,
    pub remove_branch: Option<bool>,
}
