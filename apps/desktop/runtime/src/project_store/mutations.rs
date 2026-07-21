use super::{
    ProjectCreateRequest, ProjectDefaultPushRemoteRequest, ProjectListSnapshot,
    ProjectMoveDirection, ProjectRecord, ProjectReorderRequest, ProjectRuntimeTarget,
    ProjectSelectWorktreeRequest, ProjectStore, ProjectUpdateRequest, ProjectWorktreeRecord,
    WorktreeTaskRecord,
};
use crate::project_store::helpers::{
    normalized_existing_path, normalized_project_name, normalized_project_path,
    optional_string_value, project_uuid, workspace_paths_equal,
};
use crate::project_store::raw_state::{
    ensure_array, project_index, project_record, prune_project_state, select_project_after_removal,
    update_default_worktree_record,
};
use codux_runtime_core::project::is_reserved_project_environment_key;
use codux_runtime_core::worktree::selected_runtime_worktree_id;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

impl ProjectStore {
    pub fn replace_project_worktree_state(
        &self,
        project_id: &str,
        worktrees: Vec<ProjectWorktreeRecord>,
        tasks: Vec<WorktreeTaskRecord>,
        preferred_worktree_id: Option<&str>,
    ) -> Result<(), String> {
        if worktrees
            .iter()
            .any(|worktree| worktree.project_id != project_id)
        {
            return Err("Worktree belongs to a different project.".to_string());
        }
        let snapshot = self.snapshot();
        if !snapshot
            .projects
            .iter()
            .any(|project| project.id == project_id)
        {
            return Err("Project not found.".to_string());
        }
        let current_worktrees = snapshot
            .worktrees
            .iter()
            .filter(|worktree| worktree.project_id == project_id)
            .cloned()
            .collect::<Vec<_>>();
        let current_worktree_ids = current_worktrees
            .iter()
            .map(|worktree| worktree.id.clone())
            .collect::<HashSet<_>>();
        let next_worktree_ids = worktrees
            .iter()
            .map(|worktree| worktree.id.clone())
            .collect::<HashSet<_>>();
        let tasks = tasks
            .into_iter()
            .filter(|task| {
                task.worktree_id == project_id || next_worktree_ids.contains(&task.worktree_id)
            })
            .collect::<Vec<_>>();
        let current_tasks = snapshot
            .worktree_tasks
            .iter()
            .filter(|task| current_worktree_ids.contains(&task.worktree_id))
            .cloned()
            .collect::<Vec<_>>();
        let is_valid_selection = |worktree_id: &str| {
            worktree_id == project_id || next_worktree_ids.contains(worktree_id)
        };
        let selected_worktree_id = preferred_worktree_id
            .filter(|worktree_id| is_valid_selection(worktree_id))
            .map(str::to_string)
            .or_else(|| {
                snapshot
                    .selected_worktree_id_by_project
                    .get(project_id)
                    .filter(|worktree_id| is_valid_selection(worktree_id))
                    .cloned()
            })
            .unwrap_or_else(|| project_id.to_string());
        if current_worktrees == worktrees
            && current_tasks == tasks
            && snapshot
                .selected_worktree_id_by_project
                .get(project_id)
                .is_some_and(|current| current == &selected_worktree_id)
        {
            return Ok(());
        }
        let mut raw = self.raw_snapshot();
        let records = ensure_array(&mut raw, "worktrees")?;
        records
            .retain(|record| record.get("projectId").and_then(Value::as_str) != Some(project_id));
        records.extend(
            worktrees
                .into_iter()
                .map(serde_json::to_value)
                .collect::<Result<Vec<_>, serde_json::Error>>()
                .map_err(|error| error.to_string())?,
        );
        let replaced_task_ids = current_worktree_ids
            .into_iter()
            .chain(next_worktree_ids)
            .collect::<HashSet<_>>();
        let task_records = ensure_array(&mut raw, "worktreeTasks")?;
        task_records.retain(|task| {
            task.get("worktreeId")
                .and_then(Value::as_str)
                .is_none_or(|worktree_id| !replaced_task_ids.contains(worktree_id))
        });
        task_records.extend(
            tasks
                .into_iter()
                .map(serde_json::to_value)
                .collect::<Result<Vec<_>, serde_json::Error>>()
                .map_err(|error| error.to_string())?,
        );
        let selected = raw
            .entry("selectedWorktreeIdByProject".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| "selectedWorktreeIdByProject is not an object.".to_string())?;
        selected.insert(project_id.to_string(), Value::String(selected_worktree_id));
        self.save_raw_snapshot(&raw)
    }

    pub fn create_project(
        &self,
        request: ProjectCreateRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let runtime_target = normalized_runtime_target(request.runtime_target)?;
        let project_id = self.create_or_select_project_with_target(
            &request.name,
            &request.path,
            &runtime_target,
        )?;
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
            project.insert(
                "runtimeTarget".to_string(),
                runtime_target_value(&runtime_target)?,
            );
            project.insert(
                "environmentVariables".to_string(),
                environment_variables_value(request.environment_variables)?,
            );
            project.remove("hostDeviceId");
            self.save_raw_snapshot(&raw)?;
        }
        Ok(self.list_snapshot())
    }

    pub fn update_project_from_request(
        &self,
        request: ProjectUpdateRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let runtime_target = normalized_runtime_target(request.runtime_target)?;
        let project_path = normalized_project_path(&request.path, runtime_target.is_hosted())?;
        let project_name =
            normalized_project_name(&request.name, &project_path, runtime_target.is_hosted());
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
            project.insert(
                "runtimeTarget".to_string(),
                runtime_target_value(&runtime_target)?,
            );
            project.insert(
                "environmentVariables".to_string(),
                environment_variables_value(request.environment_variables)?,
            );
            project.remove("hostDeviceId");
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
        if request.worktree_id != request.project_id {
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == request.project_id)
                .ok_or_else(|| "Project not found.".to_string())?;
            let is_runnable = selected_runtime_worktree_id(
                &request.project_id,
                Some(&request.worktree_id),
                super::snapshot::project_runtime_worktrees(&snapshot, project),
            )
            .as_deref()
                == Some(request.worktree_id.as_str());
            if !is_runnable {
                return Err("Worktree not found.".to_string());
            }
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
        // A bare create/select is always a local project (the add-project flow
        // routes remote projects through `create_project` with a host id).
        self.create_or_select_project_with_target(name, path, &ProjectRuntimeTarget::Local)
    }

    /// Hosted projects keep their runtime-side paths verbatim instead of
    /// validating them against the desktop filesystem. A path such as
    /// existence, so a Windows `F:\test` browsed on a paired host can be saved
    /// can therefore be saved from another operating system.
    pub(super) fn create_or_select_project_with_target(
        &self,
        name: &str,
        path: &str,
        runtime_target: &ProjectRuntimeTarget,
    ) -> Result<String, String> {
        let project_path = normalized_project_path(path, runtime_target.is_hosted())?;
        let project_name = normalized_project_name(name, &project_path, runtime_target.is_hosted());
        let mut raw = self.raw_snapshot();
        let projects = ensure_array(&mut raw, "projects")?;

        if let Some(existing_id) = projects.iter().find_map(|project| {
            let project = serde_json::from_value::<ProjectRecord>(project.clone()).ok()?;
            (workspace_paths_equal(&project.path, &project_path, &project.runtime_target)
                && project.runtime_target == *runtime_target)
                .then_some(project.id)
        }) {
            raw.insert(
                "selectedProjectId".to_string(),
                Value::String(existing_id.clone()),
            );
            self.save_raw_snapshot(&raw)?;
            return Ok(existing_id);
        }

        let target_identity = runtime_target.identity();
        let project_id = project_uuid(&project_name, &project_path, target_identity.as_deref());
        let mut record = project_record(&project_id, &project_name, &project_path);
        record.insert(
            "runtimeTarget".to_string(),
            runtime_target_value(runtime_target)?,
        );
        projects.push(Value::Object(record));
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
        let project_name = normalized_project_name(name, &project_path, false);
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

fn normalized_runtime_target(
    runtime_target: ProjectRuntimeTarget,
) -> Result<ProjectRuntimeTarget, String> {
    match runtime_target {
        ProjectRuntimeTarget::Local => Ok(ProjectRuntimeTarget::Local),
        ProjectRuntimeTarget::Wsl { distribution } => {
            let distribution = distribution.trim();
            if distribution.is_empty() {
                return Err("WSL distribution cannot be empty.".to_string());
            }
            Ok(ProjectRuntimeTarget::Wsl {
                distribution: distribution.to_string(),
            })
        }
        ProjectRuntimeTarget::Remote { device_id } => {
            let device_id = device_id.trim();
            if device_id.is_empty() {
                return Err("Remote device id cannot be empty.".to_string());
            }
            Ok(ProjectRuntimeTarget::Remote {
                device_id: device_id.to_string(),
            })
        }
    }
}

fn runtime_target_value(runtime_target: &ProjectRuntimeTarget) -> Result<Value, String> {
    serde_json::to_value(runtime_target)
        .map_err(|error| format!("Failed to serialize project runtime target: {error}"))
}

fn environment_variables_value(variables: BTreeMap<String, String>) -> Result<Value, String> {
    let mut normalized = BTreeMap::new();
    for (key, value) in variables {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        if is_reserved_project_environment_key(key) {
            return Err(format!(
                "Environment variable {key} is reserved for Codux runtime."
            ));
        }
        normalized.insert(key.to_string(), value);
    }
    serde_json::to_value(normalized)
        .map_err(|error| format!("Failed to serialize project environment variables: {error}"))
}
