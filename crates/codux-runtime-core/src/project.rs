use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub fn is_reserved_project_environment_key(key: &str) -> bool {
    let key = key.trim().to_ascii_uppercase();
    key.starts_with("CODUX_") || key.starts_with("DMUX_")
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListItem {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorktreeListItem {
    pub id: String,
    #[serde(rename = "projectId")]
    pub project_id: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub status: String,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    pub exists: bool,
}

pub fn project_list_payload(
    projects: impl IntoIterator<Item = ProjectListItem>,
    selected_project_id: Option<String>,
    selected_worktree_id: Option<String>,
) -> Value {
    project_list_payload_with_worktrees(
        projects,
        selected_project_id,
        selected_worktree_id,
        std::iter::empty::<ProjectWorktreeListItem>(),
    )
}

pub fn project_list_payload_with_worktrees(
    projects: impl IntoIterator<Item = ProjectListItem>,
    selected_project_id: Option<String>,
    selected_worktree_id: Option<String>,
    worktrees: impl IntoIterator<Item = ProjectWorktreeListItem>,
) -> Value {
    let projects = projects
        .into_iter()
        .map(|project| {
            json!({
                "id": project.id,
                "name": project.name,
                "path": project.path,
            })
        })
        .collect::<Vec<_>>();
    let worktrees = worktrees
        .into_iter()
        .map(|worktree| {
            json!({
                "id": worktree.id,
                "projectId": worktree.project_id,
                "name": worktree.name,
                "branch": worktree.branch,
                "path": worktree.path,
                "status": worktree.status,
                "isDefault": worktree.is_default,
                "exists": worktree.exists,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "projects": projects,
        "worktrees": worktrees,
        "selectedProjectId": selected_project_id,
        "selectedWorktreeId": selected_worktree_id
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_list_payload_keeps_mobile_shape() {
        let payload = project_list_payload(
            [ProjectListItem {
                id: "project-1".to_string(),
                name: "Codux".to_string(),
                path: "/tmp/codux".to_string(),
            }],
            Some("project-1".to_string()),
            Some("project-1".to_string()),
        );

        assert_eq!(payload["selectedProjectId"], "project-1");
        assert_eq!(payload["selectedWorktreeId"], "project-1");
        assert_eq!(payload["projects"][0]["name"], "Codux");
        assert_eq!(payload["worktrees"].as_array().unwrap().len(), 0);
        assert!(payload["projects"][0].get("badgeText").is_none());
    }

    #[test]
    fn project_list_payload_can_include_worktree_relationships() {
        let payload = project_list_payload_with_worktrees(
            [ProjectListItem {
                id: "project-1".to_string(),
                name: "Codux".to_string(),
                path: "/tmp/codux".to_string(),
            }],
            Some("project-1".to_string()),
            None,
            [ProjectWorktreeListItem {
                id: "worktree-1".to_string(),
                project_id: "project-1".to_string(),
                name: "Task".to_string(),
                branch: "task".to_string(),
                path: "/tmp/codux-task".to_string(),
                status: "active".to_string(),
                is_default: false,
                exists: true,
            }],
        );

        assert_eq!(payload["worktrees"][0]["projectId"], "project-1");
        assert_eq!(payload["worktrees"][0]["id"], "worktree-1");
    }
}
