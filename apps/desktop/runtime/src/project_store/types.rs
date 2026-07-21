pub use codux_runtime_core::runtime_target::RuntimeTarget as ProjectRuntimeTarget;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    #[serde(default)]
    pub projects: Vec<ProjectRecord>,
    #[serde(default)]
    pub worktrees: Vec<ProjectWorktreeRecord>,
    #[serde(default)]
    pub worktree_tasks: Vec<WorktreeTaskRecord>,
    pub selected_project_id: Option<String>,
    #[serde(default)]
    pub selected_worktree_id_by_project: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub badge_text: Option<String>,
    #[serde(default)]
    pub badge_symbol: Option<String>,
    #[serde(default)]
    pub badge_color_hex: Option<String>,
    #[serde(default)]
    pub git_default_push_remote_name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment_variables: BTreeMap<String, String>,
    pub runtime_target: ProjectRuntimeTarget,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRecordWire {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    badge_text: Option<String>,
    #[serde(default)]
    badge_symbol: Option<String>,
    #[serde(default)]
    badge_color_hex: Option<String>,
    #[serde(default)]
    git_default_push_remote_name: Option<String>,
    #[serde(default)]
    environment_variables: BTreeMap<String, String>,
    #[serde(default)]
    runtime_target: Option<ProjectRuntimeTarget>,
    #[serde(default)]
    host_device_id: Option<String>,
}

impl<'de> Deserialize<'de> for ProjectRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ProjectRecordWire::deserialize(deserializer)?;
        let runtime_target = wire.runtime_target.unwrap_or_else(|| {
            wire.host_device_id
                .map(|device_id| ProjectRuntimeTarget::Remote { device_id })
                .unwrap_or_default()
        });
        Ok(Self {
            id: wire.id,
            name: wire.name,
            path: wire.path,
            badge_text: wire.badge_text,
            badge_symbol: wire.badge_symbol,
            badge_color_hex: wire.badge_color_hex,
            git_default_push_remote_name: wire.git_default_push_remote_name,
            environment_variables: wire.environment_variables,
            runtime_target,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorktreeRecord {
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeTaskRecord {
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListSnapshot {
    pub projects: Vec<ProjectSummary>,
    pub selected_project_id: Option<String>,
    pub selected_worktree_id_by_project: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct ProjectWorkspaceRecord {
    pub id: String,
    pub root_project_id: String,
    pub root_project_name: String,
    pub root_project_path: String,
    pub workspace_path: String,
    pub git_default_push_remote_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCreateRequest {
    pub name: String,
    pub path: String,
    pub badge_text: Option<String>,
    pub badge_symbol: Option<String>,
    pub badge_color_hex: Option<String>,
    #[serde(default)]
    pub environment_variables: BTreeMap<String, String>,
    #[serde(default)]
    pub runtime_target: ProjectRuntimeTarget,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdateRequest {
    pub project_id: String,
    pub name: String,
    pub path: String,
    pub badge_text: Option<String>,
    pub badge_symbol: Option<String>,
    pub badge_color_hex: Option<String>,
    #[serde(default)]
    pub environment_variables: BTreeMap<String, String>,
    #[serde(default)]
    pub runtime_target: ProjectRuntimeTarget,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCloseRequest {
    pub project_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSelectWorktreeRequest {
    pub project_id: String,
    pub worktree_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReorderRequest {
    pub project_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDefaultPushRemoteRequest {
    pub project_id: String,
    pub remote_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub badge: String,
    pub status: String,
    pub branch: String,
    pub changes: usize,
    pub badge_symbol: Option<String>,
    pub badge_color_hex: Option<String>,
    pub git_default_push_remote_name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment_variables: BTreeMap<String, String>,
    #[serde(default)]
    pub runtime_target: ProjectRuntimeTarget,
}
