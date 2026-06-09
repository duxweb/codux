use std::{collections::HashMap, path::Path};

use serde::Deserialize;
use serde_json::{Map, Value};

use super::scan::{ScannedWorktree, ScannedWorktreeSnapshot};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StateFile {
    #[serde(default)]
    pub worktrees: Vec<WorktreeRecord>,
    #[serde(default)]
    pub worktree_tasks: Vec<WorktreeTaskRecord>,
    #[serde(default)]
    pub selected_worktree_id_by_project: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WorktreeRecord {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub status: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WorktreeTaskRecord {
    pub worktree_id: String,
    pub title: String,
    pub base_branch: String,
    pub status: String,
}

pub(super) fn enrich_scanned_snapshot_from_state(
    state_file: &Path,
    scanned: &mut ScannedWorktreeSnapshot,
) {
    let raw = raw_snapshot(state_file);
    let existing_worktrees = raw
        .get("worktrees")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let existing_worktrees_by_id = existing_worktrees
        .iter()
        .filter_map(|value| value.as_object())
        .filter_map(|worktree| Some((worktree.get("id")?.as_str()?.to_string(), worktree.clone())))
        .collect::<HashMap<_, _>>();
    for worktree in &mut scanned.worktrees {
        let Some(existing) = existing_worktrees_by_id.get(&worktree.id) else {
            continue;
        };
        if !worktree.is_default {
            if let Some(name) = existing.get("name").and_then(Value::as_str) {
                if !name.trim().is_empty() {
                    worktree.name = name.to_string();
                }
            }
        }
        worktree.status = existing
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or(&worktree.status)
            .to_string();
        worktree.created_at = existing
            .get("createdAt")
            .and_then(Value::as_i64)
            .unwrap_or(worktree.created_at);
    }

    let existing_tasks = raw
        .get("worktreeTasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let existing_tasks_by_id = existing_tasks
        .iter()
        .filter_map(|value| value.as_object())
        .filter_map(|task| Some((task.get("worktreeId")?.as_str()?.to_string(), task.clone())))
        .collect::<HashMap<_, _>>();
    for task in &mut scanned.tasks {
        let Some(existing) = existing_tasks_by_id.get(&task.worktree_id) else {
            continue;
        };
        task.title = existing
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(&task.title)
            .to_string();
        task.status = existing
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or(&task.status)
            .to_string();
        task.created_at = existing
            .get("createdAt")
            .and_then(Value::as_i64)
            .unwrap_or(task.created_at);
        task.started_at = existing.get("startedAt").and_then(Value::as_i64);
        task.completed_at = existing.get("completedAt").and_then(Value::as_i64);
    }
}

pub(super) fn merge_worktree_snapshot(
    raw: &mut Map<String, Value>,
    project_id: &str,
    mut snapshot: ScannedWorktreeSnapshot,
) -> Result<(), String> {
    let existing_worktrees = raw
        .get("worktrees")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let existing_by_id = existing_worktrees
        .iter()
        .filter_map(|value| value.as_object())
        .filter_map(|worktree| Some((worktree.get("id")?.as_str()?.to_string(), worktree.clone())))
        .collect::<HashMap<_, _>>();
    let existing_project_worktree_ids = existing_worktrees
        .iter()
        .filter_map(|value| value.as_object())
        .filter(|worktree| worktree.get("projectId").and_then(Value::as_str) == Some(project_id))
        .filter_map(|worktree| worktree.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<std::collections::HashSet<_>>();

    for worktree in &mut snapshot.worktrees {
        if let Some(existing) = existing_by_id.get(&worktree.id) {
            if !worktree.is_default {
                if let Some(name) = existing.get("name").and_then(Value::as_str) {
                    if !name.trim().is_empty() {
                        worktree.name = name.to_string();
                    }
                }
            }
            worktree.status = existing
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or(&worktree.status)
                .to_string();
            worktree.created_at = existing
                .get("createdAt")
                .and_then(Value::as_i64)
                .unwrap_or(worktree.created_at);
        }
    }

    let mut merged_worktrees = existing_worktrees
        .into_iter()
        .filter(|value| {
            value
                .as_object()
                .and_then(|worktree| worktree.get("projectId"))
                .and_then(Value::as_str)
                != Some(project_id)
        })
        .collect::<Vec<_>>();
    for worktree in snapshot.worktrees {
        merged_worktrees.push(worktree_state_value(worktree)?);
    }
    raw.insert("worktrees".to_string(), Value::Array(merged_worktrees));

    let existing_tasks = raw
        .get("worktreeTasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let scanned_ids = snapshot
        .tasks
        .iter()
        .map(|task| task.worktree_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let existing_tasks_by_id = existing_tasks
        .iter()
        .filter_map(|value| value.as_object())
        .filter_map(|task| Some((task.get("worktreeId")?.as_str()?.to_string(), task.clone())))
        .collect::<HashMap<_, _>>();
    let mut merged_tasks = existing_tasks
        .into_iter()
        .filter(|value| {
            value
                .as_object()
                .and_then(|task| task.get("worktreeId"))
                .and_then(Value::as_str)
                .map(|id| !scanned_ids.contains(id) && !existing_project_worktree_ids.contains(id))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    for mut task in snapshot.tasks {
        if let Some(existing) = existing_tasks_by_id.get(&task.worktree_id) {
            task.title = existing
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(&task.title)
                .to_string();
            task.status = existing
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or(&task.status)
                .to_string();
            task.created_at = existing
                .get("createdAt")
                .and_then(Value::as_i64)
                .unwrap_or(task.created_at);
            task.started_at = existing.get("startedAt").and_then(Value::as_i64);
            task.completed_at = existing.get("completedAt").and_then(Value::as_i64);
        }
        merged_tasks.push(serde_json::to_value(task).map_err(|error| error.to_string())?);
    }
    raw.insert("worktreeTasks".to_string(), Value::Array(merged_tasks));

    if !matches!(
        raw.get("selectedWorktreeIdByProject"),
        Some(Value::Object(_))
    ) {
        raw.insert(
            "selectedWorktreeIdByProject".to_string(),
            Value::Object(Map::new()),
        );
    }
    let selected_value = raw
        .get("selectedWorktreeIdByProject")
        .and_then(Value::as_object)
        .and_then(|selected| selected.get(project_id))
        .and_then(Value::as_str)
        .map(str::to_string);
    let selected_still_exists = selected_value
        .as_deref()
        .map(|selected| {
            raw.get("worktrees")
                .and_then(Value::as_array)
                .map(|worktrees| {
                    worktrees.iter().any(|worktree| {
                        worktree
                            .as_object()
                            .and_then(|worktree| worktree.get("id"))
                            .and_then(Value::as_str)
                            == Some(selected)
                    })
                })
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if !selected_still_exists {
        raw.get_mut("selectedWorktreeIdByProject")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "selectedWorktreeIdByProject is not an object.".to_string())?
            .insert(
                project_id.to_string(),
                Value::String(snapshot.selected_worktree_id),
            );
    }

    Ok(())
}

fn worktree_state_value(worktree: impl serde::Serialize) -> Result<Value, String> {
    let mut value = serde_json::to_value(worktree).map_err(|error| error.to_string())?;
    if let Some(worktree) = value.as_object_mut() {
        worktree.remove("gitSummary");
    }
    Ok(value)
}

pub(super) fn selected_worktree_id_from_state(
    state_file: &Path,
    project_id: &str,
    worktrees: &[ScannedWorktree],
) -> Option<String> {
    raw_snapshot(state_file)
        .get("selectedWorktreeIdByProject")
        .and_then(Value::as_object)
        .and_then(|selected| selected.get(project_id))
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|id| worktrees.iter().any(|worktree| &worktree.id == id))
}

pub(super) fn raw_snapshot(path: &Path) -> Map<String, Value> {
    crate::config::raw_state_snapshot(path)
}

pub(super) fn save_raw_snapshot(path: &Path, snapshot: &Map<String, Value>) -> Result<(), String> {
    crate::config::save_raw_state_snapshot(path, snapshot)
}
