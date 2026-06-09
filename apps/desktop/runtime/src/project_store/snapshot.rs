use super::{
    AppSnapshot, ProjectListSnapshot, ProjectRecord, ProjectStore, ProjectSummary,
    ProjectWorkspaceRecord,
};
use crate::project_store::helpers::{normalize_path, project_summary, worktree_summary};
use serde_json::Value;

impl ProjectStore {
    pub fn snapshot(&self) -> AppSnapshot {
        serde_json::from_value::<AppSnapshot>(Value::Object(self.raw_snapshot()))
            .unwrap_or_default()
    }

    pub fn projects_snapshot(&self) -> Vec<ProjectRecord> {
        self.snapshot().projects
    }

    pub fn project_summaries(&self) -> Vec<ProjectSummary> {
        self.snapshot()
            .projects
            .iter()
            .map(project_summary)
            .collect()
    }

    pub fn list_snapshot(&self) -> ProjectListSnapshot {
        let snapshot = self.snapshot();
        let selected_project_id = snapshot
            .selected_project_id
            .clone()
            .filter(|id| snapshot.projects.iter().any(|project| &project.id == id))
            .or_else(|| snapshot.projects.first().map(|project| project.id.clone()));
        ProjectListSnapshot {
            projects: snapshot
                .projects
                .iter()
                .map(project_summary)
                .collect::<Vec<_>>(),
            selected_project_id,
            selected_worktree_id_by_project: snapshot.selected_worktree_id_by_project,
        }
    }

    pub fn project_workspaces_snapshot(&self) -> Vec<ProjectWorkspaceRecord> {
        let snapshot = self.snapshot();
        let mut rows = Vec::new();
        for project in &snapshot.projects {
            rows.push(ProjectWorkspaceRecord {
                id: project.id.clone(),
                root_project_id: project.id.clone(),
                root_project_name: project.name.clone(),
                root_project_path: project.path.clone(),
                workspace_path: project.path.clone(),
                git_default_push_remote_name: project.git_default_push_remote_name.clone(),
            });
            for worktree in snapshot
                .worktrees
                .iter()
                .filter(|worktree| worktree.project_id == project.id)
            {
                rows.push(ProjectWorkspaceRecord {
                    id: worktree.id.clone(),
                    root_project_id: project.id.clone(),
                    root_project_name: project.name.clone(),
                    root_project_path: project.path.clone(),
                    workspace_path: worktree.path.clone(),
                    git_default_push_remote_name: project.git_default_push_remote_name.clone(),
                });
            }
        }
        rows
    }

    pub fn workspace_summary_by_path(&self, path: &str) -> Option<ProjectSummary> {
        let normalized = normalize_path(path);
        let snapshot = self.snapshot();
        snapshot
            .worktrees
            .iter()
            .find(|worktree| normalize_path(&worktree.path) == normalized)
            .map(worktree_summary)
            .or_else(|| {
                snapshot
                    .projects
                    .iter()
                    .find(|project| normalize_path(&project.path) == normalized)
                    .map(project_summary)
            })
    }

    pub fn root_project_summary_for_workspace_id(&self, id: &str) -> Option<ProjectSummary> {
        let snapshot = self.snapshot();
        if let Some(project) = snapshot.projects.iter().find(|project| project.id == id) {
            return Some(project_summary(project));
        }
        let worktree = snapshot
            .worktrees
            .iter()
            .find(|worktree| worktree.id == id)?;
        snapshot
            .projects
            .iter()
            .find(|project| project.id == worktree.project_id)
            .map(project_summary)
    }

    pub fn active_workspace_path_for_project(&self, project_id: &str) -> Option<String> {
        let snapshot = self.snapshot();
        let project = snapshot
            .projects
            .iter()
            .find(|project| project.id == project_id)?;
        let selected_worktree_id = snapshot
            .selected_worktree_id_by_project
            .get(project_id)
            .map(String::as_str)
            .unwrap_or(project_id);
        if selected_worktree_id != project_id
            && let Some(worktree) = snapshot
                .worktrees
                .iter()
                .find(|worktree| worktree.id == selected_worktree_id)
        {
            return Some(worktree.path.clone());
        }
        Some(project.path.clone())
    }
}
