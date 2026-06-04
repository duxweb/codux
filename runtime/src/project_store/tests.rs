use super::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn create_move_and_close_preserve_unknown_fields_and_prune_related_state() {
    let dir = temp_dir("project-store");
    fs::create_dir_all(&dir).unwrap();
    let project_dir = dir.join("added-project");
    fs::create_dir_all(&project_dir).unwrap();
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "projects": [
                {"id": "p1", "name": "One", "path": "/tmp/one", "custom": "keep"},
                {"id": "p2", "name": "Two", "path": "/tmp/two"}
            ],
            "worktrees": [
                {"id": "w1", "projectId": "p1"},
                {"id": "w2", "projectId": "p2"}
            ],
            "worktreeTasks": [
                {"worktreeId": "w1", "title": "remove"},
                {"worktreeId": "w2", "title": "keep"}
            ],
            "terminalLayouts": {
                "p1": {},
                "w1": {},
                "p2": {}
            },
            "selectedProjectId": "p1",
            "selectedWorktreeIdByProject": {
                "p1": "w1",
                "p2": "w2"
            },
            "unknownTopLevel": true
        }))
        .unwrap(),
    )
    .unwrap();

    let store = ProjectStore::new(support_dir.clone());
    let added_id = store
        .create_or_select_project("Added", project_dir.to_str().unwrap())
        .unwrap();
    store
        .move_project(&added_id, ProjectMoveDirection::Up)
        .unwrap();
    store.close_project("p1").unwrap();

    let state = state_value(&support_dir);
    assert_eq!(state["unknownTopLevel"], true);
    assert_eq!(state["projects"][0]["id"], added_id);
    assert_eq!(state["projects"][0]["name"], "Added");
    assert_eq!(state["projects"][1]["id"], "p2");
    assert_eq!(state["selectedProjectId"], added_id);
    assert_eq!(state["worktrees"].as_array().unwrap().len(), 1);
    assert_eq!(state["worktrees"][0]["id"], "w2");
    assert_eq!(state["worktreeTasks"].as_array().unwrap().len(), 1);
    assert!(state.get("terminalLayouts").is_none());
    assert_eq!(state["selectedWorktreeIdByProject"]["p2"], "w2");
    assert!(state["selectedWorktreeIdByProject"].get("p1").is_none());
}

#[test]
fn update_project_preserves_unknown_fields_and_updates_default_worktree() {
    let dir = temp_dir("project-store-update");
    fs::create_dir_all(&dir).unwrap();
    let project_dir = dir.join("renamed-project");
    fs::create_dir_all(&project_dir).unwrap();
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "projects": [
                {
                    "id": "p1",
                    "name": "Old",
                    "path": "/tmp/old",
                    "custom": "keep",
                    "badgeText": "A"
                }
            ],
            "worktrees": [
                {
                    "id": "w-default",
                    "projectId": "p1",
                    "name": "Old",
                    "path": "/tmp/old",
                    "isDefault": true,
                    "customWorktree": "keep"
                },
                {
                    "id": "w-task",
                    "projectId": "p1",
                    "name": "Task",
                    "path": "/tmp/task",
                    "isDefault": false
                }
            ],
            "selectedProjectId": "p1",
            "unknownTopLevel": true
        }))
        .unwrap(),
    )
    .unwrap();

    ProjectStore::new(support_dir.clone())
        .update_project("p1", "Renamed", project_dir.to_str().unwrap())
        .unwrap();

    let state = state_value(&support_dir);
    let normalized_path = project_dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert_eq!(state["unknownTopLevel"], true);
    assert_eq!(state["selectedProjectId"], "p1");
    assert_eq!(state["projects"][0]["name"], "Renamed");
    assert_eq!(state["projects"][0]["custom"], "keep");
    assert_eq!(state["projects"][0]["path"], normalized_path);
    assert_eq!(state["worktrees"][0]["name"], "Renamed");
    assert_eq!(state["worktrees"][0]["path"], normalized_path);
    assert_eq!(state["worktrees"][0]["customWorktree"], "keep");
    assert!(state["worktrees"][0]["updatedAt"].as_i64().unwrap_or(0) > 0);
    assert_eq!(state["worktrees"][1]["name"], "Task");

    fs::remove_dir_all(dir).ok();
}

#[test]
fn close_non_selected_project_keeps_current_selection() {
    let dir = temp_dir("project-store-close-non-selected");
    fs::create_dir_all(&dir).unwrap();
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "projects": [
                {"id": "p1", "name": "One", "path": "/tmp/one"},
                {"id": "p2", "name": "Two", "path": "/tmp/two"},
                {"id": "p3", "name": "Three", "path": "/tmp/three"}
            ],
            "selectedProjectId": "p2"
        }))
        .unwrap(),
    )
    .unwrap();

    let next = ProjectStore::new(support_dir.clone())
        .close_project("p1")
        .unwrap();

    let state = state_value(&support_dir);
    assert_eq!(next.as_deref(), Some("p2"));
    assert_eq!(state["selectedProjectId"], "p2");
    assert_eq!(state["projects"][0]["id"], "p2");
    assert_eq!(state["projects"][1]["id"], "p3");

    fs::remove_dir_all(dir).ok();
}

#[test]
fn close_selected_project_moves_selection_to_neighbor() {
    let dir = temp_dir("project-store-close-selected");
    fs::create_dir_all(&dir).unwrap();
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "projects": [
                {"id": "p1", "name": "One", "path": "/tmp/one"},
                {"id": "p2", "name": "Two", "path": "/tmp/two"},
                {"id": "p3", "name": "Three", "path": "/tmp/three"}
            ],
            "selectedProjectId": "p2"
        }))
        .unwrap(),
    )
    .unwrap();

    let next = ProjectStore::new(support_dir.clone())
        .close_project("p2")
        .unwrap();

    let state = state_value(&support_dir);
    assert_eq!(next.as_deref(), Some("p3"));
    assert_eq!(state["selectedProjectId"], "p3");
    assert_eq!(state["projects"][0]["id"], "p1");
    assert_eq!(state["projects"][1]["id"], "p3");

    fs::remove_dir_all(dir).ok();
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("codux-gpui-{label}-{nanos}"))
}

fn state_value(support_dir: &PathBuf) -> Value {
    Value::Object(crate::config::raw_state_snapshot(
        &support_dir.join("state.json"),
    ))
}
