use super::{
    ProjectCreateRequest, ProjectDefaultPushRemoteRequest, ProjectListSnapshot,
    ProjectMoveDirection, ProjectReorderRequest, ProjectSelectWorktreeRequest, ProjectStore,
    ProjectUpdateRequest,
};
use crate::project_store::helpers::{
    normalize_path, normalized_existing_path, normalized_project_name, optional_string_value,
    project_uuid,
};
use crate::project_store::raw_state::{
    ensure_array, project_index, project_record, prune_project_state, select_project_after_removal,
    update_default_worktree_record,
};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

impl ProjectStore {
    pub fn create_project(
        &self,
        request: ProjectCreateRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let project_id = self.create_or_select_project(&request.name, &request.path)?;
        let mut raw = self.raw_snapshot();
        if let Some(projects) = raw.get_mut("projects").and_then(Value::as_array_mut)
            && let Some(project) = projects.iter_mut().find_map(|value| {
                let project = value.as_object_mut()?;
                (project.get("id").and_then(Value::as_str) == Some(project_id.as_str()))
                    .then_some(project)
            })
        {
            project.insert(
                "badgeText".to_string(),
                optional_string_value(request.badge_text.as_deref()),
            );
            project.insert(
                "badgeSymbol".to_string(),
                optional_string_value(request.badge_symbol.as_deref()),
            );
            project.insert(
                "badgeColorHex".to_string(),
                optional_string_value(request.badge_color_hex.as_deref()),
            );
            self.save_raw_snapshot(&raw)?;
        }
        Ok(self.list_snapshot())
    }

    pub fn update_project_from_request(
        &self,
        request: ProjectUpdateRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let project_path = normalized_existing_path(&request.path)?;
        let project_name = normalized_project_name(&request.name, &project_path);
        let mut raw = self.raw_snapshot();
        {
            let projects = ensure_array(&mut raw, "projects")?;
            let index = project_index(projects, &request.project_id)
                .ok_or_else(|| "Project not found.".to_string())?;
            let Some(project) = projects.get_mut(index).and_then(Value::as_object_mut) else {
                return Err("Project record is invalid.".to_string());
            };
            project.insert("name".to_string(), Value::String(project_name.clone()));
            project.insert("path".to_string(), Value::String(project_path.clone()));
            project.insert(
                "badgeText".to_string(),
                optional_string_value(request.badge_text.as_deref()),
            );
            project.insert(
                "badgeSymbol".to_string(),
                optional_string_value(request.badge_symbol.as_deref()),
            );
            project.insert(
                "badgeColorHex".to_string(),
                optional_string_value(request.badge_color_hex.as_deref()),
            );
        }
        update_default_worktree_record(&mut raw, &request.project_id, &project_name, &project_path);
        raw.insert(
            "selectedProjectId".to_string(),
            Value::String(request.project_id.clone()),
        );
        self.save_raw_snapshot(&raw)?;
        Ok(self.list_snapshot())
    }

    pub fn reorder_projects(
        &self,
        request: ProjectReorderRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let mut raw = self.raw_snapshot();
        let projects = ensure_array(&mut raw, "projects")?;
        let ordered_project_ids = request.project_ids;
        let project_ids = ordered_project_ids.iter().cloned().collect::<HashSet<_>>();
        if ordered_project_ids.len() != projects.len()
            || project_ids.len() != projects.len()
            || projects.iter().any(|project| {
                project
                    .as_object()
                    .and_then(|project| project.get("id"))
                    .and_then(Value::as_str)
                    .map(|id| !project_ids.contains(id))
                    .unwrap_or(true)
            })
        {
            return Err("Project order does not match current projects.".to_string());
        }
        let mut by_id = projects
            .drain(..)
            .filter_map(|project| {
                let id = project
                    .as_object()
                    .and_then(|project| project.get("id"))
                    .and_then(Value::as_str)?
                    .to_string();
                Some((id, project))
            })
            .collect::<HashMap<_, _>>();
        projects.extend(
            ordered_project_ids
                .iter()
                .filter_map(|id| by_id.remove(id))
                .collect::<Vec<_>>(),
        );
        self.save_raw_snapshot(&raw)?;
        Ok(self.list_snapshot())
    }

    pub fn close_project_snapshot(
        &self,
        project_id: String,
    ) -> Result<ProjectListSnapshot, String> {
        self.close_project(&project_id)?;
        Ok(self.list_snapshot())
    }

    pub fn select_worktree(&self, request: ProjectSelectWorktreeRequest) -> Result<(), String> {
        let snapshot = self.snapshot();
        if !snapshot
            .projects
            .iter()
            .any(|project| project.id == request.project_id)
        {
            return Err("Project not found.".to_string());
        }
        let mut raw = self.raw_snapshot();
        if !matches!(
            raw.get("selectedWorktreeIdByProject"),
            Some(Value::Object(_))
        ) {
            raw.insert(
                "selectedWorktreeIdByProject".to_string(),
                Value::Object(Map::new()),
            );
        }
        raw.get_mut("selectedWorktreeIdByProject")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "selectedWorktreeIdByProject is not an object.".to_string())?
            .insert(request.project_id, Value::String(request.worktree_id));
        self.save_raw_snapshot(&raw)
    }

    pub fn set_default_push_remote(
        &self,
        request: ProjectDefaultPushRemoteRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let mut raw = self.raw_snapshot();
        let projects = ensure_array(&mut raw, "projects")?;
        let index = project_index(projects, &request.project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        let Some(project) = projects.get_mut(index).and_then(Value::as_object_mut) else {
            return Err("Project record is invalid.".to_string());
        };
        project.insert(
            "gitDefaultPushRemoteName".to_string(),
            optional_string_value(request.remote_name.as_deref()),
        );
        self.save_raw_snapshot(&raw)?;
        Ok(self.list_snapshot())
    }

    pub fn select_project(&self, project_id: &str) -> Result<(), String> {
        let snapshot = self.snapshot();
        if !snapshot
            .projects
            .iter()
            .any(|project| project.id == project_id)
        {
            return Err("Project does not exist in state.json.".to_string());
        }
        let mut raw = self.raw_snapshot();
        raw.insert(
            "selectedProjectId".to_string(),
            Value::String(project_id.to_string()),
        );
        self.save_raw_snapshot(&raw)
    }

    pub fn create_or_select_project(&self, name: &str, path: &str) -> Result<String, String> {
        let project_path = normalized_existing_path(path)?;
        let project_name = normalized_project_name(name, &project_path);
        let mut raw = self.raw_snapshot();
        let projects = ensure_array(&mut raw, "projects")?;

        if let Some(existing_id) = projects.iter().find_map(|project| {
            let project = project.as_object()?;
            let existing_path = project.get("path")?.as_str()?;
            if normalize_path(existing_path) == project_path {
                project.get("id")?.as_str().map(str::to_string)
            } else {
                None
            }
        }) {
            raw.insert(
                "selectedProjectId".to_string(),
                Value::String(existing_id.clone()),
            );
            self.save_raw_snapshot(&raw)?;
            return Ok(existing_id);
        }

        let project_id = project_uuid(&project_name, &project_path);
        projects.push(Value::Object(project_record(
            &project_id,
            &project_name,
            &project_path,
        )));
        raw.insert(
            "selectedProjectId".to_string(),
            Value::String(project_id.clone()),
        );
        self.save_raw_snapshot(&raw)?;
        Ok(project_id)
    }

    pub fn move_project(
        &self,
        project_id: &str,
        direction: ProjectMoveDirection,
    ) -> Result<(), String> {
        let mut raw = self.raw_snapshot();
        let projects = ensure_array(&mut raw, "projects")?;
        let Some(index) = project_index(projects, project_id) else {
            return Err("Project not found.".to_string());
        };
        let next_index = match direction {
            ProjectMoveDirection::Up => index.checked_sub(1),
            ProjectMoveDirection::Down => {
                if index + 1 < projects.len() {
                    Some(index + 1)
                } else {
                    None
                }
            }
        };
        let Some(next_index) = next_index else {
            return Ok(());
        };
        projects.swap(index, next_index);
        raw.insert(
            "selectedProjectId".to_string(),
            Value::String(project_id.to_string()),
        );
        self.save_raw_snapshot(&raw)
    }

    pub fn close_project(&self, project_id: &str) -> Result<Option<String>, String> {
        let mut raw = self.raw_snapshot();
        let selected_project_id = raw
            .get("selectedProjectId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let next_project_id = {
            let projects = ensure_array(&mut raw, "projects")?;
            let Some(index) = project_index(projects, project_id) else {
                return Err("Project not found.".to_string());
            };
            projects.remove(index);
            if selected_project_id.as_deref() == Some(project_id) {
                select_project_after_removal(projects, index)
            } else {
                selected_project_id.filter(|selected| project_index(projects, selected).is_some())
            }
        };
        prune_project_state(&mut raw, project_id);
        if let Some(next_project_id) = &next_project_id {
            raw.insert(
                "selectedProjectId".to_string(),
                Value::String(next_project_id.clone()),
            );
        } else {
            raw.remove("selectedProjectId");
        }
        self.save_raw_snapshot(&raw)?;
        Ok(next_project_id)
    }

    pub fn update_project(&self, project_id: &str, name: &str, path: &str) -> Result<(), String> {
        let project_path = normalized_existing_path(path)?;
        let project_name = normalized_project_name(name, &project_path);
        let mut raw = self.raw_snapshot();
        {
            let projects = ensure_array(&mut raw, "projects")?;
            let index = project_index(projects, project_id)
                .ok_or_else(|| "Project not found.".to_string())?;
            let Some(project) = projects.get_mut(index).and_then(Value::as_object_mut) else {
                return Err("Project record is invalid.".to_string());
            };
            project.insert("name".to_string(), Value::String(project_name.clone()));
            project.insert("path".to_string(), Value::String(project_path.clone()));
        }
        update_default_worktree_record(&mut raw, project_id, &project_name, &project_path);
        raw.insert(
            "selectedProjectId".to_string(),
            Value::String(project_id.to_string()),
        );
        self.save_raw_snapshot(&raw)
    }
}
