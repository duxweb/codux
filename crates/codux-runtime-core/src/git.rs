use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusSummary {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    #[serde(default)]
    pub additions: i64,
    #[serde(default)]
    pub deletions: i64,
    pub is_repository: bool,
    pub error: Option<String>,
    pub changed_files: Vec<Value>,
    pub branches: Vec<GitBranchSummary>,
    pub remote_branches: Vec<String>,
    pub remotes: Vec<Value>,
    pub commits: Vec<Value>,
    #[serde(default)]
    pub stashes: Vec<Value>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranchSummary {
    pub name: String,
    pub is_current: bool,
}

pub fn git_status_payload(
    project_id: impl Into<String>,
    project_path: impl Into<String>,
    summary: GitStatusSummary,
) -> Value {
    json!({
        "projectId": project_id.into(),
        "projectPath": project_path.into(),
        "branch": summary.branch,
        "upstream": summary.upstream,
        "ahead": summary.ahead,
        "behind": summary.behind,
        "staged": summary.staged,
        "unstaged": summary.unstaged,
        "untracked": summary.untracked,
        "changes": summary.staged + summary.unstaged + summary.untracked,
        "additions": summary.additions,
        "deletions": summary.deletions,
        "isRepository": summary.is_repository,
        "error": summary.error,
        "changedFiles": summary.changed_files,
        "branches": summary.branches,
        "remoteBranches": summary.remote_branches,
        "remotes": summary.remotes,
        "commits": summary.commits,
        "stashes": summary.stashes,
        "tags": summary.tags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_status_payload_counts_changes() {
        let payload = git_status_payload(
            "project-1",
            "/tmp/project-1",
            GitStatusSummary {
                branch: "main".to_string(),
                staged: 1,
                unstaged: 2,
                untracked: 3,
                is_repository: true,
                ..Default::default()
            },
        );

        assert_eq!(payload["changes"], 6);
        assert_eq!(payload["additions"], 0);
        assert_eq!(payload["deletions"], 0);
        assert_eq!(payload["branch"], "main");
        assert_eq!(payload["isRepository"], true);
    }
}
