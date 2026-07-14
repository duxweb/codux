use crate::git::GitService;

use super::{
    scan::{ScannedTask, ScannedWorktree},
    types::{ProjectWorktreeGitSummary, ProjectWorktreeSnapshot, WorktreeTaskSnapshot},
};

pub(super) fn scanned_worktree_to_snapshot(worktree: ScannedWorktree) -> ProjectWorktreeSnapshot {
    project_worktree_snapshot(worktree)
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

pub(super) fn project_worktree_snapshot(worktree: ScannedWorktree) -> ProjectWorktreeSnapshot {
    let git_summary = project_worktree_git_summary(&worktree.path);
    ProjectWorktreeSnapshot {
        id: worktree.id,
        project_id: worktree.project_id,
        name: worktree.name,
        branch: worktree.branch,
        path: worktree.path,
        status: worktree.status,
        is_default: worktree.is_default,
        created_at: worktree.created_at,
        updated_at: worktree.updated_at,
        git_summary,
    }
}

pub(super) fn project_worktree_git_summary(path: &str) -> ProjectWorktreeGitSummary {
    let status_snapshot = GitService::status(path);
    ProjectWorktreeGitSummary {
        changes: status_snapshot.staged + status_snapshot.unstaged + status_snapshot.untracked,
        incoming: status_snapshot.behind,
        outgoing: status_snapshot.ahead,
        additions: status_snapshot.additions,
        deletions: status_snapshot.deletions,
    }
}
