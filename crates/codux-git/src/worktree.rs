//! Shared worktree mutations (create / remove / merge) for both remote hosts.
//!
//! The desktop host and the headless agent used to implement these three
//! operations separately — the desktop via git2 under a `.codux/worktrees/<slug>`
//! convention, the agent via raw `git worktree` CLI shell-outs under a different
//! `.worktrees/<branch>` path. Same operation, two backends that could (and did)
//! diverge on path layout, branch resolution and force semantics.
//!
//! This module is the single git2-backed implementation both hosts call. The
//! host-specific bookkeeping (the desktop's `state.json` tasks/selection, each
//! host's wire payload) stays in the host; the git work — including the managed
//! path convention — lives here so the two ends can never drift again.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

type GitRepository = git2::Repository;

/// Create a worktree for `branch` (creating the branch from `base`, or the
/// repo's current branch when `base` is `None`) at the managed
/// `.codux/worktrees/<slug>` path, and return the created directory.
pub fn create_worktree(
    repo_path: &str,
    branch: &str,
    base: Option<&str>,
) -> Result<PathBuf, String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let root = repository_root(repo_path).ok_or_else(|| "Not a Git repository.".to_string())?;
    if !has_head_commit(&root) {
        return Err(
            "Repository has no commits yet. Create an initial commit before adding a worktree."
                .to_string(),
        );
    }
    let destination = managed_worktree_path(&root, branch);
    if destination.exists() {
        return Err(format!(
            "Worktree path already exists: {}",
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let base = base
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| current_branch(&root));
    create_worktree_with_git2(&root, branch, &destination, base.as_deref())?;
    Ok(destination)
}

/// Remove the worktree at `worktree_path`, optionally deleting its branch (only
/// when that branch differs from the main worktree's checked-out branch).
pub fn remove_worktree(
    repo_path: &str,
    worktree_path: &str,
    remove_branch: bool,
) -> Result<(), String> {
    let root = repository_root(repo_path).ok_or_else(|| "Not a Git repository.".to_string())?;
    let branch_to_delete = if remove_branch {
        removable_worktree_branch(&root, worktree_path)
    } else {
        None
    };
    remove_worktree_with_git2(&root, worktree_path)?;
    if let Some(branch) = branch_to_delete.as_deref() {
        delete_local_branch(&root, branch)?;
    }
    Ok(())
}

/// Merge the worktree's branch into `base` (or the repo's current branch when
/// `base` is `None`) in the main worktree, optionally removing the worktree and
/// deleting its branch afterwards.
pub fn merge_worktree(
    repo_path: &str,
    worktree_path: &str,
    base: Option<&str>,
    remove_after: bool,
) -> Result<(), String> {
    let root = repository_root(repo_path).ok_or_else(|| "Not a Git repository.".to_string())?;
    let branch = current_branch(worktree_path)
        .ok_or_else(|| "Worktree branch cannot be resolved.".to_string())?;
    let base_branch = base
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| current_branch(&root))
        .ok_or_else(|| "Base branch cannot be resolved.".to_string())?;
    if branch == base_branch {
        return Err("The worktree branch is already the base branch.".to_string());
    }
    let repo = crate::discover_repository(&root).map_err(|error| error.message().to_string())?;
    if current_branch_from_repo(&repo).as_deref() != Some(base_branch.as_str()) {
        checkout_branch_git2(&repo, &base_branch)?;
    }
    merge_branch_git2(&repo, &branch)?;
    if remove_after {
        remove_worktree_with_git2(&root, worktree_path)?;
        delete_local_branch(&root, &branch)?;
    }
    Ok(())
}

// ---- managed path convention ----

fn managed_worktree_path(root_path: &str, branch: &str) -> PathBuf {
    crate::git_path(root_path)
        .join(".codux")
        .join("worktrees")
        .join(worktree_slug(branch))
}

fn worktree_slug(branch_name: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in branch_name.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        format!("worktree-{}", now_seconds())
    } else {
        slug
    }
}

// ---- git2 operations ----

fn create_worktree_with_git2(
    root_path: &str,
    branch: &str,
    destination: &Path,
    base: Option<&str>,
) -> Result<(), String> {
    let repo =
        crate::discover_repository(root_path).map_err(|error| error.message().to_string())?;
    let base_commit = match base {
        Some(base) => repo
            .revparse_single(base)
            .and_then(|object| object.peel_to_commit())
            .map_err(|error| error.message().to_string())?,
        None => repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|error| error.message().to_string())?,
    };
    let mut created_branch = false;
    match repo.find_branch(branch, git2::BranchType::Local) {
        Ok(_) => {}
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            repo.branch(branch, &base_commit, false)
                .map_err(|error| error.message().to_string())?;
            created_branch = true;
        }
        Err(error) => return Err(error.message().to_string()),
    }
    let reference_name = format!("refs/heads/{branch}");
    let reference = repo
        .find_reference(&reference_name)
        .map_err(|error| error.message().to_string())?;
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    match repo.worktree(&worktree_slug(branch), destination, Some(&options)) {
        Ok(_) => Ok(()),
        Err(error) => {
            if created_branch
                && let Ok(mut local_branch) = repo.find_branch(branch, git2::BranchType::Local)
            {
                let _ = local_branch.delete();
            }
            Err(error.message().to_string())
        }
    }
}

fn remove_worktree_with_git2(root_path: &str, worktree_path: &str) -> Result<(), String> {
    let repo =
        crate::discover_repository(root_path).map_err(|error| error.message().to_string())?;
    let target_path = normalize_path(worktree_path);
    let names = repo
        .worktrees()
        .map_err(|error| error.message().to_string())?;
    for name in names.iter().flatten().flatten() {
        let worktree = repo
            .find_worktree(name)
            .map_err(|error| error.message().to_string())?;
        if normalize_path(&worktree.path().to_string_lossy()) != target_path {
            continue;
        }
        if Path::new(&target_path).exists() {
            fs::remove_dir_all(&target_path).map_err(|error| error.to_string())?;
        }
        let mut options = git2::WorktreePruneOptions::new();
        options.valid(true);
        return worktree
            .prune(Some(&mut options))
            .map_err(|error| error.message().to_string());
    }
    Err("Worktree not found.".to_string())
}

fn checkout_branch_git2(repo: &GitRepository, branch: &str) -> Result<(), String> {
    let reference_name = if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    };
    let reference = repo
        .find_reference(&reference_name)
        .map_err(|error| error.message().to_string())?;
    let object = reference
        .peel(git2::ObjectType::Commit)
        .map_err(|error| error.message().to_string())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(&object, Some(&mut checkout))
        .map_err(|error| error.message().to_string())?;
    repo.set_head(&reference_name)
        .map_err(|error| error.message().to_string())
}

fn merge_branch_git2(repo: &GitRepository, branch: &str) -> Result<(), String> {
    let annotated = annotated_commit_for_branch(repo, branch)?;
    let (analysis, _) = repo
        .merge_analysis(&[&annotated])
        .map_err(|error| error.message().to_string())?;
    if analysis.is_up_to_date() {
        return Ok(());
    }
    if analysis.is_fast_forward() {
        fast_forward_head(repo, annotated.id())?;
        return Ok(());
    }
    let head_commit = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(|error| error.message().to_string())?;
    let their_commit = repo
        .find_commit(annotated.id())
        .map_err(|error| error.message().to_string())?;
    let mut index = repo
        .merge_commits(&head_commit, &their_commit, None)
        .map_err(|error| error.message().to_string())?;
    if index.has_conflicts() {
        repo.checkout_index(Some(&mut index), None)
            .map_err(|error| error.message().to_string())?;
        return Err("Merge produced conflicts. Resolve them manually.".to_string());
    }
    let tree_id = index
        .write_tree_to(repo)
        .map_err(|error| error.message().to_string())?;
    let tree = repo
        .find_tree(tree_id)
        .map_err(|error| error.message().to_string())?;
    repo.checkout_tree(
        tree.as_object(),
        Some(git2::build::CheckoutBuilder::new().safe()),
    )
    .map_err(|error| error.message().to_string())?;
    repo.index()
        .and_then(|mut repo_index| {
            repo_index.read_tree(&tree)?;
            repo_index.write()
        })
        .map_err(|error| error.message().to_string())?;
    let signature = repo_signature(repo)?;
    let message = format!("Merge branch '{branch}'");
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &message,
        &tree,
        &[&head_commit, &their_commit],
    )
    .map_err(|error| error.message().to_string())?;
    repo.cleanup_state()
        .map_err(|error| error.message().to_string())
}

fn annotated_commit_for_branch<'repo>(
    repo: &'repo GitRepository,
    branch: &str,
) -> Result<git2::AnnotatedCommit<'repo>, String> {
    let object = repo
        .revparse_single(branch)
        .or_else(|_| repo.revparse_single(&format!("refs/heads/{branch}")))
        .map_err(|error| error.message().to_string())?;
    repo.find_annotated_commit(object.id())
        .map_err(|error| error.message().to_string())
}

fn fast_forward_head(repo: &GitRepository, target: git2::Oid) -> Result<(), String> {
    let head_name = repo
        .head()
        .ok()
        .and_then(|head| head.name().ok().map(str::to_string))
        .ok_or_else(|| "Cannot fast-forward detached HEAD.".to_string())?;
    let mut reference = repo
        .find_reference(&head_name)
        .map_err(|error| error.message().to_string())?;
    reference
        .set_target(target, "Fast-forward")
        .map_err(|error| error.message().to_string())?;
    repo.set_head(&head_name)
        .map_err(|error| error.message().to_string())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))
        .map_err(|error| error.message().to_string())
}

fn repo_signature(repo: &GitRepository) -> Result<git2::Signature<'_>, String> {
    repo.signature()
        .or_else(|_| git2::Signature::now("Codux", "codux@example.local"))
        .map_err(|error| error.message().to_string())
}

// ---- branch helpers ----

fn removable_worktree_branch(root_path: &str, worktree_path: &str) -> Option<String> {
    let default_branch = current_branch(root_path);
    let branch = current_branch(worktree_path)?;
    if default_branch.as_deref() == Some(branch.as_str()) {
        return None;
    }
    Some(branch)
}

fn delete_local_branch(root_path: &str, branch: &str) -> Result<(), String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Ok(());
    }
    let repo =
        crate::discover_repository(root_path).map_err(|error| error.message().to_string())?;
    if current_branch_from_repo(&repo).as_deref() == Some(branch) {
        return Err(format!("Cannot delete the checked out branch: {branch}"));
    }
    match repo.find_branch(branch, git2::BranchType::Local) {
        Ok(mut local_branch) => local_branch
            .delete()
            .map_err(|error| error.message().to_string()),
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(()),
        Err(error) => Err(error.message().to_string()),
    }
}

// ---- discovery helpers ----

fn repository_root(project_path: &str) -> Option<String> {
    crate::discover_repository(project_path)
        .ok()
        .and_then(|repo| repo_root(&repo).map(|path| normalize_path(&path.to_string_lossy())))
}

fn repo_root(repo: &GitRepository) -> Option<&Path> {
    repo.workdir().or_else(|| repo.path().parent())
}

fn current_branch(project_path: &str) -> Option<String> {
    crate::discover_repository(project_path)
        .ok()
        .as_ref()
        .and_then(current_branch_from_repo)
}

fn current_branch_from_repo(repo: &GitRepository) -> Option<String> {
    repo.head()
        .ok()
        .and_then(|head| {
            if head.is_branch() {
                head.shorthand().ok().map(str::to_string)
            } else {
                None
            }
        })
        .filter(|value| !value.trim().is_empty())
}

fn has_head_commit(root_path: &str) -> bool {
    crate::discover_repository(root_path)
        .ok()
        .map(|repo| {
            repo.head()
                .ok()
                .and_then(|head| head.peel_to_commit().ok())
                .is_some()
        })
        .unwrap_or(false)
}

fn normalize_path(path: &str) -> String {
    crate::normalize_repository_path(path)
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
