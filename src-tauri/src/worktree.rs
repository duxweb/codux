use crate::git::{git_brief_status, git_checkout_branch, git_merge_branch, GitBranchRequest};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

type GitRepository = git2::Repository;

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

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone)]
struct GitWorktreeEntry {
    path: String,
    branch: String,
    head: String,
    is_bare: bool,
    is_detached: bool,
}

pub fn worktree_snapshot(project_id: String, project_path: String) -> WorktreeSnapshot {
    let now = Utc::now().timestamp();
    let repository_root_path = repository_root(&project_path);
    let is_repository = repository_root_path.is_some();
    let root_path = repository_root_path.unwrap_or_else(|| normalize_path(&project_path));
    let default_branch = current_branch(&root_path).unwrap_or_else(|| "main".to_string());
    let mut error = None;
    let mut worktrees = Vec::new();
    let mut tasks = Vec::new();

    let default = project_worktree(
        project_id.clone(),
        project_id.clone(),
        default_branch.clone(),
        default_branch.clone(),
        root_path.clone(),
        "todo".to_string(),
        true,
        now,
    );
    worktrees.push(default);

    if is_repository {
        match list_worktrees(&root_path) {
            Ok(entries) => {
                let default_path = normalize_path(&root_path);
                for entry in entries {
                    let entry_path = normalize_path(&entry.path);
                    if entry.is_bare || entry_path == default_path {
                        continue;
                    }
                    let branch = if entry.branch.trim().is_empty() {
                        if entry.is_detached && !entry.head.trim().is_empty() {
                            format!("detached {}", short_hash(&entry.head))
                        } else {
                            "detached HEAD".to_string()
                        }
                    } else {
                        entry.branch
                    };
                    let id = worktree_uuid(&project_id, &entry_path);
                    let name = worktree_display_name(&branch, &entry_path);
                    worktrees.push(project_worktree(
                        id.clone(),
                        project_id.clone(),
                        name.clone(),
                        branch,
                        entry_path,
                        "todo".to_string(),
                        false,
                        now,
                    ));
                    tasks.push(WorktreeTaskSnapshot {
                        worktree_id: id,
                        title: name,
                        base_branch: default_branch.clone(),
                        base_commit: commit_hash(&default_branch, &root_path),
                        status: "todo".to_string(),
                        created_at: now,
                        updated_at: now,
                        started_at: None,
                        completed_at: None,
                    });
                }
            }
            Err(next_error) => {
                error = Some(next_error);
            }
        }
    } else {
        error = Some("non_git_repository".to_string());
    }

    WorktreeSnapshot {
        project_id,
        selected_worktree_id: worktrees
            .first()
            .map(|worktree| worktree.id.clone())
            .unwrap_or_default(),
        worktrees,
        tasks,
        error,
    }
}

pub fn create_worktree(request: WorktreeCreateRequest) -> Result<WorktreeSnapshot, String> {
    let branch = request.branch_name.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)
        .ok_or_else(|| "Not a Git repository.".to_string())?;
    if !has_head_commit(&root) {
        return Err("当前仓库还没有任何提交。请先创建初始提交后再创建 Worktree。".to_string());
    }
    let destination = managed_worktree_path(&request.project_path, branch);
    if destination.exists() {
        return Err(format!(
            "Worktree path already exists: {}",
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let base = request
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| current_branch(&root));
    create_worktree_with_git2(&root, branch, &destination, base.as_deref())?;
    let destination_text = destination.display().to_string();
    let created_path = normalize_path(&destination_text);
    let mut snapshot = worktree_snapshot(request.project_id, request.project_path);
    if let Some(created) = snapshot
        .worktrees
        .iter()
        .find(|worktree| normalize_path(&worktree.path) == created_path)
    {
        snapshot.selected_worktree_id = created.id.clone();
        if let Some(task_title) = request
            .task_title
            .and_then(|value| normalized_string(&value))
        {
            if let Some(task) = snapshot
                .tasks
                .iter_mut()
                .find(|task| task.worktree_id == created.id)
            {
                task.title = task_title;
            }
        }
    }
    Ok(snapshot)
}

pub fn remove_worktree(request: WorktreeRemoveRequest) -> Result<WorktreeSnapshot, String> {
    let root = repository_root(&request.project_path)
        .ok_or_else(|| "Not a Git repository.".to_string())?;
    let branch_to_delete = if request.remove_branch {
        removable_worktree_branch(&root, &request.worktree_path)
    } else {
        None
    };
    remove_worktree_with_git2(&root, &request.worktree_path)?;
    if let Some(branch) = branch_to_delete.as_deref() {
        delete_local_branch(&root, branch)?;
    }
    Ok(worktree_snapshot(request.project_id, request.project_path))
}

pub fn merge_worktree(request: WorktreeMergeRequest) -> Result<WorktreeSnapshot, String> {
    let root = repository_root(&request.project_path)
        .ok_or_else(|| "Not a Git repository.".to_string())?;
    let branch = current_branch(&request.worktree_path)
        .ok_or_else(|| "Worktree branch cannot be resolved.".to_string())?;
    let base_branch = request
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| current_branch(&root))
        .ok_or_else(|| "Base branch cannot be resolved.".to_string())?;
    if branch == base_branch {
        return Err("The default worktree cannot be merged into itself.".to_string());
    }
    if current_branch(&root).as_deref() != Some(base_branch.as_str()) {
        git_checkout_branch(GitBranchRequest {
            project_path: root.clone(),
            branch: base_branch.clone(),
        })?;
    }
    git_merge_branch(GitBranchRequest {
        project_path: root.clone(),
        branch: branch.clone(),
    })?;
    if request.remove_branch.unwrap_or(false) {
        remove_worktree_with_git2(&root, &request.worktree_path)?;
        delete_local_branch(&root, &branch)?;
    }
    Ok(worktree_snapshot(request.project_id, request.project_path))
}

fn project_worktree(
    id: String,
    project_id: String,
    name: String,
    branch: String,
    path: String,
    status: String,
    is_default: bool,
    now: i64,
) -> ProjectWorktreeSnapshot {
    let status_snapshot = git_brief_status(path.clone());
    let (additions, deletions) = worktree_line_stats(&path);
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
        git_summary: ProjectWorktreeGitSummary {
            changes: status_snapshot.changes,
            incoming: status_snapshot.behind,
            outgoing: status_snapshot.ahead,
            additions,
            deletions,
        },
    }
}

fn list_worktrees(path: &str) -> Result<Vec<GitWorktreeEntry>, String> {
    let mut entries = Vec::new();
    let repo = GitRepository::discover(path).map_err(|error| error.message().to_string())?;
    let names = repo
        .worktrees()
        .map_err(|error| error.message().to_string())?;
    for name in names.iter().flatten().flatten() {
        let Ok(worktree) = repo.find_worktree(name) else {
            continue;
        };
        let path = normalize_path(&worktree.path().to_string_lossy());
        let worktree_repo = GitRepository::open(worktree.path()).ok();
        let branch = worktree_repo
            .as_ref()
            .and_then(current_branch_from_repo)
            .unwrap_or_default();
        let head = worktree_repo
            .as_ref()
            .and_then(head_oid_from_repo)
            .unwrap_or_default();
        let is_detached = worktree_repo
            .as_ref()
            .map(|repo| repo.head().map(|head| !head.is_branch()).unwrap_or(false))
            .unwrap_or(false);
        let is_bare = worktree_repo
            .as_ref()
            .map(|repo| repo.is_bare())
            .unwrap_or(false);
        entries.push(GitWorktreeEntry {
            path,
            branch,
            head,
            is_bare,
            is_detached,
        });
    }
    Ok(entries)
}

fn remove_worktree_with_git2(root: &str, worktree_path: &str) -> Result<(), String> {
    let repo = GitRepository::discover(root).map_err(|error| error.message().to_string())?;
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
            std::fs::remove_dir_all(&target_path).map_err(|error| error.to_string())?;
        }
        let mut options = git2::WorktreePruneOptions::new();
        options.valid(true);
        return worktree
            .prune(Some(&mut options))
            .map_err(|error| error.message().to_string());
    }
    Err("Worktree not found.".to_string())
}

fn create_worktree_with_git2(
    root: &str,
    branch: &str,
    destination: &Path,
    base: Option<&str>,
) -> Result<(), String> {
    let repo = GitRepository::discover(root).map_err(|error| error.message().to_string())?;
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
            if created_branch {
                if let Ok(mut local_branch) = repo.find_branch(branch, git2::BranchType::Local) {
                    let _ = local_branch.delete();
                }
            }
            Err(error.message().to_string())
        }
    }
}

fn repository_root(path: &str) -> Option<String> {
    GitRepository::discover(path)
        .ok()
        .and_then(|repo| repo_root(&repo).map(|path| normalize_path(&path.to_string_lossy())))
}

fn current_branch(path: &str) -> Option<String> {
    GitRepository::discover(path)
        .ok()
        .as_ref()
        .and_then(current_branch_from_repo)
}

fn removable_worktree_branch(root: &str, worktree_path: &str) -> Option<String> {
    let default_branch = current_branch(root);
    let branch = current_branch(worktree_path)?;
    if default_branch.as_deref() == Some(branch.as_str()) {
        return None;
    }
    Some(branch)
}

fn delete_local_branch(root: &str, branch: &str) -> Result<(), String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Ok(());
    }
    let repo = GitRepository::discover(root).map_err(|error| error.message().to_string())?;
    if current_branch_from_repo(&repo).as_deref() == Some(branch) {
        return Err(format!("Cannot delete the checked out branch: {branch}"));
    }
    let result = match repo.find_branch(branch, git2::BranchType::Local) {
        Ok(mut local_branch) => local_branch
            .delete()
            .map_err(|error| error.message().to_string()),
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(()),
        Err(error) => Err(error.message().to_string()),
    };
    result
}

fn commit_hash(ref_name: &str, path: &str) -> Option<String> {
    let ref_name = ref_name.trim();
    if ref_name.is_empty() {
        return None;
    }
    GitRepository::discover(path).ok().and_then(|repo| {
        repo.revparse_single(ref_name)
            .ok()?
            .peel_to_commit()
            .ok()
            .map(|commit| commit.id().to_string())
    })
}

fn worktree_line_stats(path: &str) -> (i64, i64) {
    let Ok(repo) = GitRepository::discover(path) else {
        return (0, 0);
    };
    let mut total = (0, 0);
    if let Ok(diff) = diff_for_line_stats(&repo, true) {
        merge_diff_line_stats(&mut total, &diff);
    }
    if let Ok(diff) = diff_for_line_stats(&repo, false) {
        merge_diff_line_stats(&mut total, &diff);
    }
    total
}

fn diff_for_line_stats(repo: &GitRepository, staged: bool) -> Result<git2::Diff<'_>, git2::Error> {
    let tree = head_tree(repo).ok();
    if staged {
        repo.diff_tree_to_index(tree.as_ref(), None, None)
    } else {
        repo.diff_index_to_workdir(None, None)
    }
}

fn merge_diff_line_stats(total: &mut (i64, i64), diff: &git2::Diff<'_>) {
    for index in 0..diff.deltas().len() {
        let (additions, deletions) = patch_line_stats(diff, index);
        total.0 += additions;
        total.1 += deletions;
    }
}

fn patch_line_stats(diff: &git2::Diff<'_>, index: usize) -> (i64, i64) {
    let Ok(Some(patch)) = git2::Patch::from_diff(diff, index) else {
        return (0, 0);
    };
    let mut additions = 0;
    let mut deletions = 0;
    for hunk_index in 0..patch.num_hunks() {
        let Ok((_hunk, line_count)) = patch.hunk(hunk_index) else {
            continue;
        };
        for line_index in 0..line_count {
            let Ok(line) = patch.line_in_hunk(hunk_index, line_index) else {
                continue;
            };
            match line.origin() {
                '+' => additions += 1,
                '-' => deletions += 1,
                _ => {}
            }
        }
    }
    (additions, deletions)
}

fn has_head_commit(path: &str) -> bool {
    GitRepository::discover(path)
        .ok()
        .map(|repo| {
            repo.head()
                .ok()
                .and_then(|head| head.peel_to_commit().ok())
                .is_some()
        })
        .unwrap_or(false)
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

fn head_oid_from_repo(repo: &GitRepository) -> Option<String> {
    repo.head()
        .ok()
        .and_then(|head| head.target())
        .map(|oid| oid.to_string())
}

fn head_tree(repo: &GitRepository) -> Result<git2::Tree<'_>, git2::Error> {
    repo.head()?.peel_to_commit()?.tree()
}

fn managed_worktree_path(project_path: &str, branch_name: &str) -> PathBuf {
    let root = repository_root(project_path).unwrap_or_else(|| normalize_path(project_path));
    PathBuf::from(root)
        .join(".codux")
        .join("worktrees")
        .join(worktree_slug(branch_name))
}

fn worktree_uuid(project_id: &str, path: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("codux:worktree:{project_id}:{path}").as_bytes(),
    )
    .to_string()
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
        format!("worktree-{}", Uuid::new_v4().to_string()[..8].to_string())
    } else {
        slug
    }
}

fn worktree_display_name(branch: &str, path: &str) -> String {
    let branch = branch.trim();
    if !branch.is_empty() && branch != "detached HEAD" {
        return branch
            .split('/')
            .next_back()
            .filter(|value| !value.is_empty())
            .unwrap_or(branch)
            .to_string();
    }
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Worktree")
        .to_string()
}

fn normalize_path(path: &str) -> String {
    let path = Path::new(path);
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn short_hash(value: &str) -> String {
    value.chars().take(7).collect()
}

fn normalized_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("codux-worktree-test-{name}-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn managed_worktree_path_lives_under_project_directory() {
        let path = managed_worktree_path("/tmp/example-project", "task/fix cli hooks");
        let text = path.display().to_string();
        assert!(text.ends_with("/tmp/example-project/.codux/worktrees/task-fix-cli-hooks"));
    }

    #[test]
    fn remove_request_keeps_branch_by_default() {
        let request: WorktreeRemoveRequest = serde_json::from_value(serde_json::json!({
            "projectId": "project",
            "projectPath": "/tmp/project",
            "worktreePath": "/tmp/project/.codux/worktrees/topic"
        }))
        .expect("deserialize request");

        assert!(!request.remove_branch);
    }

    #[test]
    fn removable_worktree_branch_skips_current_default_branch() {
        let temp = TempDir::new("default-branch");
        let root = create_repo_with_commit(&temp.path);

        assert_eq!(removable_worktree_branch(&root, &root), None);
    }

    #[test]
    fn delete_local_branch_uses_git2_and_ignores_missing_branch() {
        let temp = TempDir::new("delete-branch");
        let root = create_repo_with_commit(&temp.path);
        let repo = GitRepository::discover(&root).expect("open repo");
        let head = repo.head().expect("head").peel_to_commit().expect("commit");
        repo.branch("topic/delete-me", &head, false)
            .expect("create branch");
        drop(head);
        drop(repo);

        delete_local_branch(&root, "topic/delete-me").expect("delete branch");
        delete_local_branch(&root, "topic/delete-me").expect("ignore missing branch");

        let repo = GitRepository::discover(&root).expect("open repo");
        assert!(repo
            .find_branch("topic/delete-me", git2::BranchType::Local)
            .is_err());
    }

    #[test]
    fn create_worktree_uses_git2_and_selects_created_worktree() {
        let temp = TempDir::new("create-worktree");
        let root = create_repo_with_commit(&temp.path);

        let snapshot = create_worktree(WorktreeCreateRequest {
            project_id: "project".to_string(),
            project_path: root.clone(),
            base_branch: current_branch(&root),
            branch_name: "feature/git2-create".to_string(),
            task_title: None,
        })
        .expect("create worktree");

        let created = snapshot
            .worktrees
            .iter()
            .find(|worktree| worktree.branch == "feature/git2-create")
            .expect("created worktree");
        assert_eq!(snapshot.selected_worktree_id, created.id);
        assert!(Path::new(&created.path).exists());
        let repo = GitRepository::discover(&root).expect("open repo");
        assert!(repo
            .find_branch("feature/git2-create", git2::BranchType::Local)
            .is_ok());
    }

    #[test]
    fn merge_worktree_merges_branch_into_default_worktree() {
        let temp = TempDir::new("merge-worktree");
        let root = create_repo_with_commit(&temp.path);
        let snapshot = create_worktree(WorktreeCreateRequest {
            project_id: "project".to_string(),
            project_path: root.clone(),
            base_branch: current_branch(&root),
            branch_name: "feature/test-merge".to_string(),
            task_title: None,
        })
        .expect("create worktree");
        let worktree_path = snapshot
            .worktrees
            .iter()
            .find(|worktree| worktree.branch == "feature/test-merge")
            .map(|worktree| PathBuf::from(&worktree.path))
            .expect("created worktree path");
        commit_file(&worktree_path, "README.md", "hello\nfeature\n", "feature");

        merge_worktree(WorktreeMergeRequest {
            project_id: "project".to_string(),
            project_path: root.clone(),
            worktree_path: worktree_path.to_string_lossy().to_string(),
            base_branch: current_branch(&root),
            remove_branch: Some(false),
        })
        .expect("merge worktree");

        let merged = fs::read_to_string(temp.path.join("README.md")).expect("read merged file");
        assert!(merged.contains("feature"));
        let repo = GitRepository::discover(&root).expect("open repo");
        assert!(repo
            .find_branch("feature/test-merge", git2::BranchType::Local)
            .is_ok());
    }

    fn create_repo_with_commit(path: &Path) -> String {
        let repo = GitRepository::init(path).expect("init repo");
        write_commit(&repo, path, "README.md", "hello\n", "initial");
        normalize_path(&path.to_string_lossy())
    }

    fn commit_file(repo_path: &Path, relative_path: &str, content: &str, message: &str) {
        let repo = GitRepository::discover(repo_path).expect("open repo");
        write_commit(&repo, repo_path, relative_path, content, message);
    }

    fn write_commit(
        repo: &GitRepository,
        repo_path: &Path,
        relative_path: &str,
        content: &str,
        message: &str,
    ) {
        let file_path = repo_path.join(relative_path);
        fs::write(&file_path, content).expect("write file");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new(relative_path)).expect("add file");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let signature = git2::Signature::now("Codux", "codux@example.test").expect("signature");
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
}
