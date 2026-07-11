use crate::git::GitBranchSummary;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::borrow::Borrow;
use std::path::Path;
use uuid::Uuid;

/// Stable per-worktree id, shared by the desktop and headless hosts so the same
/// (project, path) always resolves to the same id regardless of transport. The
/// default/main worktree uses the project id itself; non-default worktrees use
/// this v5 UUID.
pub fn worktree_uuid(project_id: &str, path: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("codux:worktree:{project_id}:{path}").as_bytes(),
    )
    .to_string()
}

/// Human-readable worktree name: the branch leaf, else the directory name.
pub fn worktree_display_name(branch: &str, path: &str) -> String {
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeWorktreeItem {
    pub id: String,
    pub project_id: String,
    pub path: String,
    pub is_default: bool,
    pub exists: bool,
}

impl RuntimeWorktreeItem {
    pub fn runnable(&self) -> bool {
        self.is_default || self.exists
    }
}

pub fn selected_runtime_worktree_id<I>(
    project_id: &str,
    selected_worktree_id: Option<&str>,
    worktrees: I,
) -> Option<String>
where
    I: IntoIterator,
    I::Item: Borrow<RuntimeWorktreeItem>,
{
    let worktrees = worktrees
        .into_iter()
        .filter_map(|worktree| {
            let worktree = worktree.borrow();
            (worktree.project_id == project_id).then(|| worktree.clone())
        })
        .collect::<Vec<_>>();
    selected_worktree_id
        .and_then(|selected| {
            worktrees
                .iter()
                .find(|worktree| worktree.id == selected && worktree.runnable())
        })
        .or_else(|| {
            worktrees
                .iter()
                .find(|worktree| worktree.is_default && worktree.runnable())
        })
        .or_else(|| worktrees.iter().find(|worktree| worktree.runnable()))
        .map(|worktree| worktree.id.clone())
}

pub fn worktree_base_branches(branch: &str, branches: &[GitBranchSummary]) -> Vec<String> {
    let mut values = Vec::new();
    push_unique_branch(&mut values, branch);
    for branch in branches {
        push_unique_branch(&mut values, branch.name.as_str());
    }
    values
}

pub fn default_worktree_base_branch(branch: &str, branches: &[GitBranchSummary]) -> String {
    branches
        .iter()
        .find(|branch| branch.is_current)
        .or_else(|| branches.first())
        .map(|branch| branch.name.clone())
        .filter(|branch| !branch.trim().is_empty())
        .unwrap_or_else(|| branch.to_string())
}

pub struct WorktreeSummaryPayload {
    pub project_id: String,
    pub selected_worktree_id: Option<String>,
    pub worktrees: Value,
    pub tasks: Value,
    pub available: bool,
    pub base_branches: Vec<String>,
    pub default_base_branch: String,
    pub error: Option<String>,
}

pub fn worktree_summary_payload(payload: WorktreeSummaryPayload) -> Value {
    json!({
        "projectId": payload.project_id,
        "selectedWorktreeId": payload.selected_worktree_id,
        "worktrees": payload.worktrees,
        "tasks": payload.tasks,
        "available": payload.available,
        "baseBranches": payload.base_branches,
        "defaultBaseBranch": payload.default_base_branch,
        "error": payload.error,
    })
}

pub fn worktree_update_payload(
    project_id: impl Into<String>,
    selected_worktree_id: impl Into<String>,
    worktrees: Value,
    tasks: Value,
    base_branches: Vec<String>,
    default_base_branch: String,
    error: Option<String>,
) -> Value {
    json!({
        "projectId": project_id.into(),
        "selectedWorktreeId": selected_worktree_id.into(),
        "worktrees": worktrees,
        "tasks": tasks,
        "baseBranches": base_branches,
        "defaultBaseBranch": default_base_branch,
        "error": error,
    })
}

fn push_unique_branch(values: &mut Vec<String>, value: &str) {
    let branch = value.trim();
    if branch.is_empty() || values.iter().any(|item| item == branch) {
        return;
    }
    values.push(branch.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_base_branches_are_unique_and_current_first() {
        let branches = vec![
            GitBranchSummary {
                name: "main".to_string(),
                is_current: true,
            },
            GitBranchSummary {
                name: "feature".to_string(),
                is_current: false,
            },
        ];

        assert_eq!(
            worktree_base_branches("main", &branches),
            vec!["main".to_string(), "feature".to_string()]
        );
        assert_eq!(default_worktree_base_branch("fallback", &branches), "main");
    }

    #[test]
    fn selected_runtime_worktree_ignores_missing_non_default_selection() {
        let worktrees = vec![
            RuntimeWorktreeItem {
                id: "project-1".to_string(),
                project_id: "project-1".to_string(),
                path: "/repo".to_string(),
                is_default: true,
                exists: true,
            },
            RuntimeWorktreeItem {
                id: "worktree-missing".to_string(),
                project_id: "project-1".to_string(),
                path: "/repo/.codux/worktrees/missing".to_string(),
                is_default: false,
                exists: false,
            },
        ];

        assert_eq!(
            selected_runtime_worktree_id("project-1", Some("worktree-missing"), &worktrees)
                .as_deref(),
            Some("project-1")
        );
    }

    #[test]
    fn selected_runtime_worktree_keeps_existing_non_default_selection() {
        let worktrees = vec![
            RuntimeWorktreeItem {
                id: "project-1".to_string(),
                project_id: "project-1".to_string(),
                path: "/repo".to_string(),
                is_default: true,
                exists: true,
            },
            RuntimeWorktreeItem {
                id: "worktree-1".to_string(),
                project_id: "project-1".to_string(),
                path: "/repo/.codux/worktrees/task".to_string(),
                is_default: false,
                exists: true,
            },
        ];

        assert_eq!(
            selected_runtime_worktree_id("project-1", Some("worktree-1"), &worktrees).as_deref(),
            Some("worktree-1")
        );
    }
}
