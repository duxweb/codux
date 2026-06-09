use crate::git::GitService;

use super::{
    scan::{ScannedTask, ScannedWorktree},
    types::{ProjectWorktreeGitSummary, ProjectWorktreeSnapshot, WorktreeTaskSnapshot},
};

pub(super) fn scanned_worktree_to_snapshot(worktree: ScannedWorktree) -> ProjectWorktreeSnapshot {
    project_worktree_snapshot(
        worktree.id,
        worktree.project_id,
        worktree.name,
        worktree.branch,
        worktree.path,
        worktree.status,
        worktree.is_default,
        worktree.created_at,
    )
}

pub(super) fn scanned_task_to_snapshot(task: ScannedTask) -> WorktreeTaskSnapshot {
    WorktreeTaskSnapshot {
        worktree_id: task.worktree_id,
        title: task.title,
        base_branch: task.base_branch,
        base_commit: task.base_commit,
        status: task.status,
        created_at: task.created_at,
        updated_at: task.updated_at,
        started_at: task.started_at,
        completed_at: task.completed_at,
    }
}

pub(super) fn project_worktree_snapshot(
    id: String,
    project_id: String,
    name: String,
    branch: String,
    path: String,
    status: String,
    is_default: bool,
    now: i64,
) -> ProjectWorktreeSnapshot {
    let git_summary = project_worktree_git_summary(&path);
    ProjectWorktreeSnapshot {
        id,
        project_id,
        name,
        branch,
        path,
        status,
        is_default,
        created_at: now,
        updated_at: now,
        git_summary,
    }
}

pub(super) fn project_worktree_git_summary(path: &str) -> ProjectWorktreeGitSummary {
    let status_snapshot = GitService::status(path);
    let review_snapshot = GitService::review(path, None);
    let additions = review_snapshot
        .files
        .iter()
        .map(|file| file.additions)
        .sum();
    let deletions = review_snapshot
        .files
        .iter()
        .map(|file| file.deletions)
        .sum();
    ProjectWorktreeGitSummary {
        changes: status_snapshot.staged + status_snapshot.unstaged + status_snapshot.untracked,
        incoming: status_snapshot.behind,
        outgoing: status_snapshot.ahead,
        additions,
        deletions,
    }
}
