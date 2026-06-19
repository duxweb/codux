//! Lean git status reader for the headless host. The desktop has a richer
//! GitService; the agent only needs to report status (branch, change counts,
//! changed files, local branches) so a controller's Git panel can show the
//! repo state. git2 lives on the agent binary only — not the shared crates — so
//! the mobile FFI is not pulled into libgit2.

use codux_runtime_core::git::{GitBranchSummary, GitStatusSummary};
use git2::build::CheckoutBuilder;
use git2::{BranchType, DiffFormat, DiffOptions, IndexAddOption, Repository, Signature, Status, StatusOptions};
use serde_json::json;
use std::path::Path;

pub fn git_status_summary(path: &str) -> GitStatusSummary {
    let repo = match Repository::open(path) {
        Ok(repo) => repo,
        Err(_) => {
            return GitStatusSummary {
                is_repository: false,
                ..Default::default()
            };
        }
    };
    let mut summary = GitStatusSummary {
        is_repository: true,
        ..Default::default()
    };

    if let Ok(head) = repo.head() {
        if let Ok(name) = head.shorthand() {
            summary.branch = name.to_string();
        }
    }

    let staged_mask = Status::INDEX_NEW
        | Status::INDEX_MODIFIED
        | Status::INDEX_DELETED
        | Status::INDEX_RENAMED
        | Status::INDEX_TYPECHANGE;
    let unstaged_mask = Status::WT_MODIFIED
        | Status::WT_DELETED
        | Status::WT_RENAMED
        | Status::WT_TYPECHANGE;

    let mut options = StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    if let Ok(statuses) = repo.statuses(Some(&mut options)) {
        for entry in statuses.iter() {
            let status = entry.status();
            let staged = status.intersects(staged_mask);
            let untracked = status.contains(Status::WT_NEW);
            let unstaged = status.intersects(unstaged_mask);
            if staged {
                summary.staged += 1;
            }
            if untracked {
                summary.untracked += 1;
            } else if unstaged {
                summary.unstaged += 1;
            }
            // Match the desktop host's GitFileStatus shape so a controller maps
            // both hosts uniformly.
            let index_status = if status.contains(Status::INDEX_NEW) {
                "A"
            } else if status.contains(Status::INDEX_MODIFIED) {
                "M"
            } else if status.contains(Status::INDEX_DELETED) {
                "D"
            } else if status.contains(Status::INDEX_RENAMED) {
                "R"
            } else if status.contains(Status::INDEX_TYPECHANGE) {
                "T"
            } else {
                ""
            };
            let worktree_status = if status.contains(Status::WT_NEW) {
                "?"
            } else if status.contains(Status::WT_MODIFIED) {
                "M"
            } else if status.contains(Status::WT_DELETED) {
                "D"
            } else if status.contains(Status::WT_RENAMED) {
                "R"
            } else if status.contains(Status::WT_TYPECHANGE) {
                "T"
            } else {
                ""
            };
            summary.changed_files.push(json!({
                "path": entry.path().unwrap_or_default(),
                "indexStatus": index_status,
                "worktreeStatus": worktree_status,
            }));
        }
    }

    if let Ok(branches) = repo.branches(Some(BranchType::Local)) {
        for branch in branches.flatten() {
            if let Ok(Some(name)) = branch.0.name() {
                summary.branches.push(GitBranchSummary {
                    name: name.to_string(),
                    is_current: branch.0.is_head(),
                });
            }
        }
    }

    summary
}

/// Stage the given project-relative paths (adds, modifies, deletes).
pub fn stage(repo_path: &str, paths: &[String]) -> Result<(), String> {
    let repo = Repository::open(repo_path).map_err(|error| error.to_string())?;
    let mut index = repo.index().map_err(|error| error.to_string())?;
    index
        .add_all(paths.iter(), IndexAddOption::DEFAULT, None)
        .map_err(|error| error.to_string())?;
    index.write().map_err(|error| error.to_string())
}

/// Unstage the given paths (reset their index entries to HEAD).
pub fn unstage(repo_path: &str, paths: &[String]) -> Result<(), String> {
    let repo = Repository::open(repo_path).map_err(|error| error.to_string())?;
    match repo.head().ok().and_then(|head| head.peel_to_commit().ok()) {
        Some(commit) => repo
            .reset_default(Some(commit.as_object()), paths.iter())
            .map_err(|error| error.to_string()),
        None => {
            // No commits yet: drop the entries from the index.
            let mut index = repo.index().map_err(|error| error.to_string())?;
            for path in paths {
                let _ = index.remove_path(Path::new(path));
            }
            index.write().map_err(|error| error.to_string())
        }
    }
}

/// Commit the staged index with `message`.
pub fn commit(repo_path: &str, message: &str) -> Result<(), String> {
    let repo = Repository::open(repo_path).map_err(|error| error.to_string())?;
    let mut index = repo.index().map_err(|error| error.to_string())?;
    let tree_oid = index.write_tree().map_err(|error| error.to_string())?;
    let tree = repo.find_tree(tree_oid).map_err(|error| error.to_string())?;
    let signature = repo
        .signature()
        .or_else(|_| Signature::now("Codux", "codux@local"))
        .map_err(|error| error.to_string())?;
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &parents)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Discard worktree changes for the given tracked paths (checkout from HEAD).
pub fn discard(repo_path: &str, paths: &[String]) -> Result<(), String> {
    let repo = Repository::open(repo_path).map_err(|error| error.to_string())?;
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    for path in paths {
        checkout.path(path);
    }
    repo.checkout_head(Some(&mut checkout))
        .map_err(|error| error.to_string())
}

/// A unified diff (HEAD → working tree, including the index) for one path.
pub fn diff(repo_path: &str, path: &str) -> Result<String, String> {
    let repo = Repository::open(repo_path).map_err(|error| error.to_string())?;
    let head_tree = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_tree().ok());
    let mut options = DiffOptions::new();
    options
        .pathspec(path)
        .include_untracked(true)
        .recurse_untracked_dirs(true);
    let diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut options))
        .map_err(|error| error.to_string())?;
    let mut out = String::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        if matches!(line.origin(), '+' | '-' | ' ') {
            out.push(line.origin());
        }
        out.push_str(std::str::from_utf8(line.content()).unwrap_or_default());
        true
    })
    .map_err(|error| error.to_string())?;
    Ok(out)
}
