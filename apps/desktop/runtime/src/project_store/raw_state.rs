use serde_json::{Map, Value};
use std::collections::HashSet;

pub(super) fn ensure_array<'a>(
    snapshot: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Vec<Value>, String> {
    if !matches!(snapshot.get(key), Some(Value::Array(_))) {
        snapshot.insert(key.to_string(), Value::Array(Vec::new()));
    }
    snapshot
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("{key} is not an array."))
}

pub(super) fn project_index(projects: &[Value], project_id: &str) -> Option<usize> {
    projects.iter().position(|project| {
        project
            .as_object()
            .and_then(|project| project.get("id"))
            .and_then(Value::as_str)
            == Some(project_id)
    })
}

pub(super) fn select_project_after_removal(
    projects: &[Value],
    removed_index: usize,
) -> Option<String> {
    projects
        .get(removed_index)
        .or_else(|| {
            removed_index
                .checked_sub(1)
                .and_then(|index| projects.get(index))
        })
        .or_else(|| projects.first())
        .and_then(|project| project.as_object())
        .and_then(|project| project.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(super) fn prune_project_state(snapshot: &mut Map<String, Value>, project_id: &str) {
    let removed_worktree_ids = remove_worktrees(snapshot, project_id);
    remove_worktree_tasks(snapshot, &removed_worktree_ids);
    remove_selected_worktree(snapshot, project_id);
}

fn remove_worktrees(snapshot: &mut Map<String, Value>, project_id: &str) -> HashSet<String> {
    let Some(worktrees) = snapshot.get_mut("worktrees").and_then(Value::as_array_mut) else {
        return HashSet::new();
    };
    let mut removed = HashSet::new();
    worktrees.retain(|worktree| {
        let should_remove = worktree
            .as_object()
            .and_then(|worktree| worktree.get("projectId"))
            .and_then(Value::as_str)
            == Some(project_id);
        if should_remove {
            if let Some(id) = worktree
                .as_object()
                .and_then(|worktree| worktree.get("id"))
                .and_then(Value::as_str)
            {
                removed.insert(id.to_string());
            }
        }
        !should_remove
    });
    removed
}

fn remove_worktree_tasks(
    snapshot: &mut Map<String, Value>,
    removed_worktree_ids: &HashSet<String>,
) {
    if removed_worktree_ids.is_empty() {
        return;
    }
    let Some(tasks) = snapshot
        .get_mut("worktreeTasks")
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    tasks.retain(|task| {
        task.as_object()
            .and_then(|task| task.get("worktreeId"))
            .and_then(Value::as_str)
            .map(|id| !removed_worktree_ids.contains(id))
            .unwrap_or(true)
    });
}

fn remove_selected_worktree(snapshot: &mut Map<String, Value>, project_id: &str) {
    if let Some(selected) = snapshot
        .get_mut("selectedWorktreeIdByProject")
        .and_then(Value::as_object_mut)
    {
        selected.remove(project_id);
    }
}

pub(super) fn update_default_worktree_record(
    snapshot: &mut Map<String, Value>,
    project_id: &str,
    project_name: &str,
    project_path: &str,
) {
    let Some(worktrees) = snapshot.get_mut("worktrees").and_then(Value::as_array_mut) else {
        return;
    };
    for worktree in worktrees {
        let Some(worktree) = worktree.as_object_mut() else {
            continue;
        };
        let matches_project = worktree
            .get("projectId")
            .and_then(Value::as_str)
            .map(|id| id == project_id)
            .unwrap_or(false);
        let is_default = worktree
            .get("isDefault")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if matches_project && is_default {
            worktree.insert("name".to_string(), Value::String(project_name.to_string()));
            worktree.insert("path".to_string(), Value::String(project_path.to_string()));
            worktree.insert("updatedAt".to_string(), Value::from(now_millis()));
        }
    }
}

pub(super) fn project_record(id: &str, name: &str, path: &str) -> Map<String, Value> {
    let mut record = Map::new();
    record.insert("id".to_string(), Value::String(id.to_string()));
    record.insert("name".to_string(), Value::String(name.to_string()));
    record.insert("path".to_string(), Value::String(path.to_string()));
    record.insert("badgeText".to_string(), Value::Null);
    record.insert("badgeSymbol".to_string(), Value::Null);
    record.insert("badgeColorHex".to_string(), Value::Null);
    record.insert("gitDefaultPushRemoteName".to_string(), Value::Null);
    record
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}
