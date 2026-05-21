use crate::git::{git_brief_status, git_command_output};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

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
    let root_path = repository_root(&project_path).unwrap_or_else(|| normalize_path(&project_path));
    let default_branch = current_branch(&root_path).unwrap_or_default();
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
    let destination_text = destination.display().to_string();
    let mut args = vec!["worktree", "add", "-b", branch, destination_text.as_str()];
    if let Some(base) = base.as_deref() {
        args.push(base);
    }
    git_output(Path::new(&root), &args).map(|_| ())?;
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
    git_output(
        Path::new(&root),
        &["worktree", "remove", request.worktree_path.as_str()],
    )?;
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
    let output = git_output(Path::new(path), &["worktree", "list", "--porcelain"])?;
    let mut entries = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_branch = String::new();
    let mut current_head = String::new();
    let mut is_bare = false;
    let mut is_detached = false;

    let flush = |entries: &mut Vec<GitWorktreeEntry>,
                 current_path: &mut Option<String>,
                 current_branch: &mut String,
                 current_head: &mut String,
                 is_bare: &mut bool,
                 is_detached: &mut bool| {
        if let Some(path) = current_path.take() {
            entries.push(GitWorktreeEntry {
                path,
                branch: std::mem::take(current_branch),
                head: std::mem::take(current_head),
                is_bare: *is_bare,
                is_detached: *is_detached,
            });
        }
        *is_bare = false;
        *is_detached = false;
    };

    for raw_line in output.lines().chain(std::iter::once("")) {
        let line = raw_line.trim();
        if line.is_empty() {
            flush(
                &mut entries,
                &mut current_path,
                &mut current_branch,
                &mut current_head,
                &mut is_bare,
                &mut is_detached,
            );
            continue;
        }
        if let Some(value) = line.strip_prefix("worktree ") {
            current_path = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("HEAD ") {
            current_head = value.to_string();
        } else if let Some(value) = line.strip_prefix("branch ") {
            current_branch = value
                .strip_prefix("refs/heads/")
                .unwrap_or(value)
                .to_string();
        } else if line == "bare" {
            is_bare = true;
        } else if line == "detached" {
            is_detached = true;
        }
    }
    Ok(entries)
}

fn repository_root(path: &str) -> Option<String> {
    git_output(Path::new(path), &["rev-parse", "--show-toplevel"])
        .ok()
        .map(|value| normalize_path(value.trim()))
        .filter(|value| !value.is_empty())
}

fn current_branch(path: &str) -> Option<String> {
    git_output(Path::new(path), &["branch", "--show-current"])
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn commit_hash(ref_name: &str, path: &str) -> Option<String> {
    let ref_name = ref_name.trim();
    if ref_name.is_empty() {
        return None;
    }
    git_output(Path::new(path), &["rev-parse", "--verify", ref_name])
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn worktree_line_stats(path: &str) -> (i64, i64) {
    let root = Path::new(path);
    let staged = git_output(root, &["diff", "--cached", "--numstat"]).unwrap_or_default();
    let unstaged = git_output(root, &["diff", "--numstat"]).unwrap_or_default();
    merge_numstat(&staged, &unstaged)
}

fn merge_numstat(left: &str, right: &str) -> (i64, i64) {
    let mut additions = 0;
    let mut deletions = 0;
    for value in [left, right] {
        for line in value.lines() {
            let mut parts = line.split('\t');
            additions += parts
                .next()
                .and_then(|part| part.parse::<i64>().ok())
                .unwrap_or(0);
            deletions += parts
                .next()
                .and_then(|part| part.parse::<i64>().ok())
                .unwrap_or(0);
        }
    }
    (additions, deletions)
}

fn has_head_commit(path: &str) -> bool {
    git_output(Path::new(path), &["rev-parse", "--verify", "HEAD^{commit}"]).is_ok()
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<String, String> {
    git_command_output(cwd, args)
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

    #[test]
    fn managed_worktree_path_lives_under_project_directory() {
        let path = managed_worktree_path("/tmp/example-project", "task/fix cli hooks");
        let text = path.display().to_string();
        assert!(text.ends_with("/tmp/example-project/.codux/worktrees/task-fix-cli-hooks"));
    }
}
