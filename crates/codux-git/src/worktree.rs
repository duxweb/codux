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
    let base = base
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| current_branch(repo_path));
    let root = main_worktree_root(repo_path)?;
    if !has_head_commit(&root) {
        return Err(
            "Repository has no commits yet. Create an initial commit before adding a worktree."
                .to_string(),
        );
    }
    let destination = managed_worktree_path_from_root(&root, branch);
    if destination.exists() {
        return Err(format!(
            "Worktree path already exists: {}",
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    ensure_managed_worktrees_excluded(&root)?;
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
    let root = main_worktree_root(repo_path)?;
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
    let root = main_worktree_root(repo_path)?;
    ensure_worktree_clean(worktree_path)?;
    ensure_worktree_clean(&root)?;
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

/// Merge a child worktree branch back into the worktree that created it.
pub fn merge_worktree_into_source(
    source_worktree_path: &str,
    child_worktree_path: &str,
    expected_source_branch: Option<&str>,
) -> Result<(), String> {
    ensure_worktree_status_clean(
        child_worktree_path,
        true,
        "Child worktree has uncommitted changes. Commit the reviewed changes before merging.",
    )?;
    ensure_worktree_status_clean(
        source_worktree_path,
        false,
        "Source worktree has tracked uncommitted changes. Commit or discard them before merging.",
    )?;
    let branch = current_branch(child_worktree_path)
        .ok_or_else(|| "Worktree branch cannot be resolved.".to_string())?;
    let source_branch = current_branch(source_worktree_path)
        .ok_or_else(|| "Source worktree branch cannot be resolved.".to_string())?;
    if let Some(expected) = expected_source_branch
        .map(str::trim)
        .filter(|value| !value.is_empty())
        && source_branch != expected
    {
        return Err(format!(
            "Source worktree branch changed from {expected} to {source_branch}."
        ));
    }
    if branch == source_branch {
        return Err("The child worktree is already on the source branch.".to_string());
    }
    let repo = crate::discover_repository(source_worktree_path)
        .map_err(|error| error.message().to_string())?;
    merge_branch_git2(&repo, &branch)
}

pub fn ensure_merged_worktree_removable(
    source_worktree_path: &str,
    child_worktree_path: &str,
    child_branch: &str,
) -> Result<(), String> {
    let child_branch = child_branch.trim();
    if child_branch.is_empty() {
        return Err("Child worktree branch cannot be empty.".to_string());
    }
    let root = main_worktree_root(source_worktree_path)?;
    let expected_path = managed_worktree_path_from_root(&root, child_branch);
    if crate::repository_path_key(child_worktree_path)
        != crate::repository_path_key(&expected_path.to_string_lossy())
    {
        return Err("Child worktree path does not match its managed branch.".to_string());
    }
    let source_repo = crate::discover_repository(source_worktree_path)
        .map_err(|error| error.message().to_string())?;
    let root_repo =
        crate::discover_repository(&root).map_err(|error| error.message().to_string())?;
    let has_metadata = find_worktree_by_path(&root_repo, child_worktree_path)?.is_some();
    let child_exists = Path::new(child_worktree_path).exists();
    if child_exists && !has_metadata {
        return Err("Child worktree metadata is missing.".to_string());
    }
    if child_exists {
        ensure_worktree_clean(child_worktree_path)?;
        let actual_branch = current_branch(child_worktree_path)
            .ok_or_else(|| "Child worktree branch cannot be resolved.".to_string())?;
        if actual_branch != child_branch {
            return Err(format!(
                "Child worktree branch changed from {child_branch} to {actual_branch}."
            ));
        }
    }
    let child_commit = match root_repo.find_branch(child_branch, git2::BranchType::Local) {
        Ok(branch) => Some(
            branch
                .get()
                .peel_to_commit()
                .map_err(|error| error.message().to_string())?
                .id(),
        ),
        Err(error) if error.code() == git2::ErrorCode::NotFound => None,
        Err(error) => return Err(error.message().to_string()),
    };
    let Some(child_head) = child_commit else {
        return if has_metadata {
            Err("Child worktree branch is missing.".to_string())
        } else {
            Ok(())
        };
    };
    let source_head = source_repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(|error| error.message().to_string())?
        .id();
    if source_head == child_head
        || source_repo
            .graph_descendant_of(source_head, child_head)
            .map_err(|error| error.message().to_string())?
    {
        Ok(())
    } else {
        Err(
            "The child branch has commits that are not merged into the source worktree."
                .to_string(),
        )
    }
}

pub fn remove_merged_worktree(
    source_worktree_path: &str,
    child_worktree_path: &str,
    child_branch: &str,
) -> Result<(), String> {
    ensure_merged_worktree_removable(source_worktree_path, child_worktree_path, child_branch)?;
    let root = main_worktree_root(source_worktree_path)?;
    let repo = crate::discover_repository(&root).map_err(|error| error.message().to_string())?;
    if let Some(worktree) = find_worktree_by_path(&repo, child_worktree_path)? {
        prune_worktree(worktree, child_worktree_path)?;
    }
    delete_local_branch(&root, child_branch)
}

pub fn current_branch(project_path: &str) -> Option<String> {
    crate::discover_repository(project_path)
        .ok()
        .as_ref()
        .and_then(current_branch_from_repo)
}

fn ensure_worktree_clean(worktree_path: &str) -> Result<(), String> {
    ensure_worktree_status_clean(
        worktree_path,
        true,
        "Worktree has uncommitted changes. Commit the reviewed changes before merging.",
    )
}

fn ensure_worktree_status_clean(
    worktree_path: &str,
    include_untracked: bool,
    message: &str,
) -> Result<(), String> {
    let repo =
        crate::discover_repository(worktree_path).map_err(|error| error.message().to_string())?;
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(include_untracked)
        .recurse_untracked_dirs(include_untracked);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|error| error.message().to_string())?;
    if statuses.is_empty() {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

// ---- managed path convention ----

pub fn managed_worktree_path(repo_path: &str, branch: &str) -> Result<PathBuf, String> {
    let root = main_worktree_root(repo_path)?;
    Ok(managed_worktree_path_from_root(&root, branch))
}

fn managed_worktree_path_from_root(root_path: &str, branch: &str) -> PathBuf {
    crate::git_path(root_path)
        .join(".codux")
        .join("worktrees")
        .join(worktree_slug(branch))
}

pub fn ensure_managed_worktrees_excluded(root_path: &str) -> Result<(), String> {
    let repository =
        crate::discover_repository(root_path).map_err(|error| error.message().to_string())?;
    let exclude_path = repository.path().join("info").join("exclude");
    let Some(parent) = exclude_path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let entry = ".codux/worktrees/";
    let existing = match fs::read_to_string(&exclude_path) {
        Ok(existing) => existing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    if existing.lines().map(str::trim).any(|line| line == entry) {
        return Ok(());
    }
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(entry);
    content.push('\n');
    fs::write(exclude_path, content).map_err(|error| error.to_string())
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
    let worktree = find_worktree_by_path(&repo, worktree_path)?
        .ok_or_else(|| "Worktree not found.".to_string())?;
    prune_worktree(worktree, worktree_path)
}

fn prune_worktree(worktree: git2::Worktree, worktree_path: &str) -> Result<(), String> {
    let target_path = normalize_path(worktree_path);
    if Path::new(&target_path).exists() {
        fs::remove_dir_all(&target_path).map_err(|error| error.to_string())?;
    }
    let mut options = git2::WorktreePruneOptions::new();
    options.valid(true);
    worktree
        .prune(Some(&mut options))
        .map_err(|error| error.message().to_string())
}

fn find_worktree_by_path(
    repo: &GitRepository,
    worktree_path: &str,
) -> Result<Option<git2::Worktree>, String> {
    let target_path = normalize_path(worktree_path);
    let names = repo
        .worktrees()
        .map_err(|error| error.message().to_string())?;
    for name in names.iter().flatten().flatten() {
        let worktree = repo
            .find_worktree(name)
            .map_err(|error| error.message().to_string())?;
        if normalize_path(&worktree.path().to_string_lossy()) == target_path {
            return Ok(Some(worktree));
        }
    }
    Ok(None)
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
    let target_commit = repo
        .find_commit(target)
        .map_err(|error| error.message().to_string())?;
    let target_tree = target_commit
        .tree()
        .map_err(|error| error.message().to_string())?;
    repo.checkout_tree(
        target_tree.as_object(),
        Some(git2::build::CheckoutBuilder::new().safe()),
    )
    .map_err(|error| error.message().to_string())?;
    reference
        .set_target(target, "Fast-forward")
        .map_err(|error| error.message().to_string())?;
    repo.set_head(&head_name)
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

pub fn main_worktree_root(project_path: &str) -> Result<String, String> {
    let repo = crate::discover_repository(project_path)
        .map_err(|_| "Not a Git repository.".to_string())?;
    if repo.is_bare() {
        return Err("Repository has no working tree.".to_string());
    }
    let root = if repo.is_worktree() {
        GitRepository::open(repo.commondir())
            .ok()
            .and_then(|common| common.workdir().map(Path::to_path_buf))
    } else {
        repo_root(&repo).map(Path::to_path_buf)
    }
    .ok_or_else(|| "Repository has no working tree.".to_string())?;
    Ok(normalize_path(&root.to_string_lossy()))
}

fn repo_root(repo: &GitRepository) -> Option<&Path> {
    repo.workdir().or_else(|| repo.path().parent())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_worktree_creation_uses_main_managed_root_and_source_branch() {
        let root = temp_dir("linked-source");
        init_repo(&root);
        let source = create_worktree(root.to_string_lossy().as_ref(), "feature/source", None)
            .expect("create source worktree");
        fs::write(source.join("source.txt"), "source\n").expect("write source file");
        commit_all(&source, "source commit");

        let child = create_worktree(source.to_string_lossy().as_ref(), "feature/child", None)
            .expect("create child worktree");

        assert_eq!(
            normalize_path(&child.to_string_lossy()),
            normalize_path(
                &root
                    .join(".codux/worktrees/feature-child")
                    .to_string_lossy()
            )
        );
        assert!(!source.join(".codux/worktrees/feature-child").exists());
        let child_repo = GitRepository::open(&child).expect("open child worktree");
        let source_repo = GitRepository::open(&source).expect("open source worktree");
        let source_head = source_repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .expect("source head");
        assert_eq!(
            child_repo.head().unwrap().target(),
            Some(source_head.id()),
            "the new branch must start at the calling worktree head"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn managed_path_and_main_root_are_stable_from_linked_worktree() {
        let root = temp_dir("linked-root");
        init_repo(&root);
        let canonical_root = fs::canonicalize(&root).expect("canonical repository root");
        let linked = create_worktree(root.to_string_lossy().as_ref(), "task/one", None)
            .expect("create linked worktree");

        assert_eq!(
            main_worktree_root(linked.to_string_lossy().as_ref()).unwrap(),
            normalize_path(&root.to_string_lossy())
        );
        assert_eq!(
            crate::repository_path_key(
                &managed_worktree_path(linked.to_string_lossy().as_ref(), "task/two")
                    .unwrap()
                    .to_string_lossy()
            ),
            crate::repository_path_key(
                &canonical_root
                    .join(".codux/worktrees/task-two")
                    .to_string_lossy()
            )
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn agent_worktree_merge_targets_its_source_worktree_and_can_be_removed() {
        let root = temp_dir("delivery");
        init_repo(&root);
        let source = create_worktree(root.to_string_lossy().as_ref(), "feature/source", None)
            .expect("create source worktree");
        fs::write(source.join("source.txt"), "source\n").expect("write source file");
        commit_all(&source, "source commit");
        let child = create_worktree(source.to_string_lossy().as_ref(), "feature/child", None)
            .expect("create child worktree");
        fs::write(child.join("child.txt"), "child\n").expect("write child file");
        commit_all(&child, "child commit");

        merge_worktree_into_source(
            source.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            Some("feature/source"),
        )
        .expect("merge child into source");

        assert!(source.join("child.txt").exists());
        assert!(!root.join("child.txt").exists());
        remove_merged_worktree(
            source.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            "feature/child",
        )
        .expect("remove merged child");
        assert!(!child.exists());
        remove_worktree(
            root.to_string_lossy().as_ref(),
            source.to_string_lossy().as_ref(),
            true,
        )
        .expect("remove source worktree");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn agent_worktree_merge_requires_clean_child_and_tracked_source() {
        let root = temp_dir("delivery-clean");
        init_repo(&root);
        let source = create_worktree(root.to_string_lossy().as_ref(), "feature/source", None)
            .expect("create source worktree");
        let child = create_worktree(source.to_string_lossy().as_ref(), "feature/child", None)
            .expect("create child worktree");
        fs::write(child.join("child.txt"), "uncommitted\n").expect("dirty child");

        let child_error = merge_worktree_into_source(
            source.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            Some("feature/source"),
        )
        .unwrap_err();
        assert!(child_error.contains("uncommitted changes"));

        fs::remove_file(child.join("child.txt")).expect("clean child");
        fs::write(child.join("child.txt"), "committed\n").expect("write child");
        commit_all(&child, "child commit");
        fs::write(source.join("README.md"), "tracked source change\n").expect("dirty source");
        let source_error = merge_worktree_into_source(
            source.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            Some("feature/source"),
        )
        .unwrap_err();
        assert!(source_error.contains("Source worktree has tracked uncommitted changes"));

        fs::write(source.join("README.md"), "test\n").expect("clean source");
        remove_worktree(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            true,
        )
        .expect("remove child worktree");
        remove_worktree(
            root.to_string_lossy().as_ref(),
            source.to_string_lossy().as_ref(),
            true,
        )
        .expect("remove source worktree");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn agent_worktree_merge_preserves_non_conflicting_source_untracked_files() {
        let root = temp_dir("delivery-source-untracked");
        init_repo(&root);
        let source_branch = current_branch(root.to_string_lossy().as_ref()).unwrap();
        let child = create_worktree(root.to_string_lossy().as_ref(), "feature/child", None)
            .expect("create child worktree");
        fs::write(child.join("child.txt"), "child\n").expect("write child file");
        commit_all(&child, "child commit");
        fs::write(root.join("local.txt"), "keep me\n").expect("write source untracked file");

        merge_worktree_into_source(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            Some(&source_branch),
        )
        .expect("merge with non-conflicting source untracked file");

        assert_eq!(
            fs::read_to_string(root.join("local.txt")).unwrap(),
            "keep me\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("child.txt")).unwrap(),
            "child\n"
        );
        fs::remove_file(root.join("local.txt")).expect("remove source untracked file");
        remove_merged_worktree(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            "feature/child",
        )
        .expect("remove merged child");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn agent_worktree_merge_rejects_conflicting_source_untracked_file_without_data_loss() {
        let root = temp_dir("delivery-source-untracked-conflict");
        init_repo(&root);
        let source_branch = current_branch(root.to_string_lossy().as_ref()).unwrap();
        let source_head = GitRepository::open(&root)
            .unwrap()
            .head()
            .unwrap()
            .target()
            .unwrap();
        let child = create_worktree(root.to_string_lossy().as_ref(), "feature/child", None)
            .expect("create child worktree");
        fs::write(child.join("shared.txt"), "child\n").expect("write child file");
        commit_all(&child, "child commit");
        fs::write(root.join("shared.txt"), "local\n").expect("write conflicting source file");

        let error = merge_worktree_into_source(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            Some(&source_branch),
        )
        .unwrap_err();

        assert!(!error.is_empty());
        assert_eq!(
            fs::read_to_string(root.join("shared.txt")).unwrap(),
            "local\n"
        );
        assert_eq!(
            GitRepository::open(&root).unwrap().head().unwrap().target(),
            Some(source_head)
        );
        fs::remove_file(root.join("shared.txt")).expect("remove conflicting source file");
        remove_worktree(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            true,
        )
        .expect("remove child worktree");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn agent_worktree_removal_rejects_new_unmerged_commits() {
        let root = temp_dir("delivery-new-commit");
        init_repo(&root);
        let source_branch = current_branch(root.to_string_lossy().as_ref()).unwrap();
        let child = create_worktree(root.to_string_lossy().as_ref(), "feature/child", None)
            .expect("create child worktree");
        fs::write(child.join("first.txt"), "first\n").expect("write first child file");
        commit_all(&child, "first child commit");
        merge_worktree_into_source(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            Some(&source_branch),
        )
        .expect("merge first child commit");
        fs::write(child.join("second.txt"), "second\n").expect("write second child file");
        commit_all(&child, "second child commit");

        let error = remove_merged_worktree(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            "feature/child",
        )
        .unwrap_err();
        assert!(error.contains("not merged"));
        assert!(child.exists());

        merge_worktree_into_source(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            Some(&source_branch),
        )
        .expect("merge second child commit");
        remove_merged_worktree(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            "feature/child",
        )
        .expect("remove child worktree");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn agent_worktree_removal_is_idempotent_after_git_cleanup() {
        let root = temp_dir("delivery-retry");
        init_repo(&root);
        let source_branch = current_branch(root.to_string_lossy().as_ref()).unwrap();
        let child = create_worktree(root.to_string_lossy().as_ref(), "feature/child", None)
            .expect("create child worktree");
        fs::write(child.join("child.txt"), "child\n").expect("write child file");
        commit_all(&child, "child commit");
        merge_worktree_into_source(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            Some(&source_branch),
        )
        .expect("merge child");

        remove_merged_worktree(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            "feature/child",
        )
        .expect("remove child");
        remove_merged_worktree(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            "feature/child",
        )
        .expect("retry removed child");

        assert!(!child.exists());
        assert!(
            GitRepository::open(&root)
                .unwrap()
                .find_branch("feature/child", git2::BranchType::Local)
                .is_err()
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn agent_worktree_conflict_does_not_modify_source_worktree() {
        let root = temp_dir("delivery-conflict");
        init_repo(&root);
        let source_branch = current_branch(root.to_string_lossy().as_ref()).unwrap();
        let child = create_worktree(root.to_string_lossy().as_ref(), "feature/child", None)
            .expect("create child worktree");
        fs::write(child.join("README.md"), "child\n").expect("write child conflict");
        commit_all(&child, "child conflict");
        fs::write(root.join("README.md"), "source\n").expect("write source conflict");
        commit_all(&root, "source conflict");

        let error = merge_worktree_into_source(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            Some(&source_branch),
        )
        .unwrap_err();

        assert!(error.contains("conflicts"));
        assert_eq!(
            fs::read_to_string(root.join("README.md")).unwrap(),
            "source\n"
        );
        ensure_worktree_clean(root.to_string_lossy().as_ref())
            .expect("source worktree must remain clean");
        remove_worktree(
            root.to_string_lossy().as_ref(),
            child.to_string_lossy().as_ref(),
            true,
        )
        .expect("remove child worktree");
        fs::remove_dir_all(root).ok();
    }

    fn init_repo(path: &Path) {
        fs::create_dir_all(path).expect("create repository directory");
        let repo = GitRepository::init(path).expect("init repository");
        let mut config = repo.config().expect("repository config");
        config
            .set_str("user.email", "codux@example.test")
            .expect("set email");
        config.set_str("user.name", "Codux").expect("set name");
        fs::write(path.join("README.md"), "test\n").expect("write repository file");
        commit_all(path, "initial");
    }

    fn commit_all(path: &Path, message: &str) {
        let repo = GitRepository::open(path).expect("open repository");
        let mut index = repo.index().expect("open index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("stage files");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let signature = repo.signature().expect("signature");
        let parents = repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok())
            .into_iter()
            .collect::<Vec<_>>();
        let parent_refs = parents.iter().collect::<Vec<_>>();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .expect("commit");
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("codux-worktree-{label}-{nanos}"))
    }
}
