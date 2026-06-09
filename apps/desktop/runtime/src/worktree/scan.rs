use serde::{Deserialize, Serialize};

use super::git_ops::{
    commit_hash, current_branch, list_git_worktrees, normalize_path, repository_root, short_hash,
    worktree_display_name, worktree_uuid,
};

#[derive(Clone, Debug)]
pub(super) struct ScannedWorktreeSnapshot {
    pub selected_worktree_id: String,
    pub worktrees: Vec<ScannedWorktree>,
    pub tasks: Vec<ScannedTask>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ScannedWorktree {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub status: String,
    pub is_default: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ScannedTask {
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

pub(super) fn scan_git_worktrees(
    project_id: &str,
    project_path: &str,
) -> Result<ScannedWorktreeSnapshot, String> {
    let root_path = repository_root(project_path).unwrap_or_else(|| normalize_path(project_path));
    let default_branch = current_branch(&root_path).unwrap_or_else(|| "main".to_string());
    let now = super::git_ops::now_seconds();
    let mut worktrees = vec![ScannedWorktree {
        id: project_id.to_string(),
        project_id: project_id.to_string(),
        name: default_branch.clone(),
        branch: default_branch.clone(),
        path: root_path.clone(),
        status: "todo".to_string(),
        is_default: true,
        created_at: now,
        updated_at: now,
    }];
    let mut tasks = Vec::new();

    for entry in list_git_worktrees(&root_path)? {
        let path = normalize_path(&entry.path);
        if entry.bare || path == root_path {
            continue;
        }
        let branch = if entry.branch.trim().is_empty() {
            if entry.detached && !entry.head.trim().is_empty() {
                format!("detached {}", short_hash(&entry.head))
            } else {
                "detached HEAD".to_string()
            }
        } else {
            entry.branch
        };
        let id = worktree_uuid(project_id, &path);
        let name = worktree_display_name(&branch, &path);
        worktrees.push(ScannedWorktree {
            id: id.clone(),
            project_id: project_id.to_string(),
            name: name.clone(),
            branch,
            path,
            status: "todo".to_string(),
            is_default: false,
            created_at: now,
            updated_at: now,
        });
        tasks.push(ScannedTask {
            worktree_id: id,
            title: name,
            base_branch: default_branch.clone(),
            base_commit: commit_hash(&root_path, &default_branch),
            status: "todo".to_string(),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
        });
    }

    Ok(ScannedWorktreeSnapshot {
        selected_worktree_id: worktrees
            .first()
            .map(|worktree| worktree.id.clone())
            .unwrap_or_else(|| project_id.to_string()),
        worktrees,
        tasks,
    })
}
