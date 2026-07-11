use super::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
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
fn create_project_persists_host_device_id_round_trip() {
    let dir = temp_dir("project-store-host-device");
    fs::create_dir_all(&dir).unwrap();
    let project_dir = dir.join("remote-project");
    fs::create_dir_all(&project_dir).unwrap();
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();

    ProjectStore::new(support_dir.clone())
        .create_project(ProjectCreateRequest {
            name: "Remote".to_string(),
            path: project_dir.to_str().unwrap().to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
            host_device_id: Some("device-xyz".to_string()),
        })
        .unwrap();

    let state = state_value(&support_dir);
    assert_eq!(state["projects"][0]["hostDeviceId"], "device-xyz");

    // The typed record round-trips the field rather than dropping it.
    let reloaded: Vec<ProjectRecord> = serde_json::from_value(state["projects"].clone()).unwrap();
    assert_eq!(reloaded[0].host_device_id.as_deref(), Some("device-xyz"));

    fs::remove_dir_all(dir).ok();
}

#[test]
fn create_remote_project_keeps_host_path_without_local_existence_check() {
    // The host path lives on the paired machine, not this one, so it must NOT be
    // validated against (or canonicalized on) the local filesystem. A Windows
    // `F:\…` path browsed from macOS would otherwise fail `Path::exists()` and
    // the project would silently refuse to save.
    let dir = temp_dir("project-store-remote-path");
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();

    let host_path = r"F:\test\does-not-exist-locally";
    ProjectStore::new(support_dir.clone())
        .create_project(ProjectCreateRequest {
            name: "Win".to_string(),
            path: host_path.to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
            host_device_id: Some("device-win".to_string()),
        })
        .expect("creating a remote project must not require the path to exist locally");

    let state = state_value(&support_dir);
    // The host path is stored verbatim (not canonicalized away), and tagged with
    // its host so the project routes over the controller.
    assert_eq!(state["projects"][0]["path"], host_path);
    assert_eq!(state["projects"][0]["hostDeviceId"], "device-win");

    fs::remove_dir_all(dir).ok();
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

#[test]
fn worktree_selection_rejects_missing_non_default_workspace() {
    let dir = temp_dir("project-store-missing-worktree");
    fs::create_dir_all(&dir).unwrap();
    let support_dir = dir.join("support");
    let project_dir = dir.join("project");
    let missing_dir = dir.join("missing-worktree");
    fs::create_dir_all(&support_dir).unwrap();
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "projects": [
                {"id": "p1", "name": "One", "path": project_dir.to_string_lossy()}
            ],
            "worktrees": [
                {
                    "id": "w-missing",
                    "projectId": "p1",
                    "name": "Missing",
                    "branch": "missing",
                    "path": missing_dir.to_string_lossy(),
                    "status": "todo",
                    "isDefault": false,
                    "createdAt": 1,
                    "updatedAt": 1
                }
            ],
            "selectedProjectId": "p1",
            "selectedWorktreeIdByProject": {"p1": "w-missing"}
        }))
        .unwrap(),
    )
    .unwrap();

    let store = ProjectStore::new(support_dir.clone());
    assert!(
        store
            .select_worktree(ProjectSelectWorktreeRequest {
                project_id: "p1".to_string(),
                worktree_id: "w-missing".to_string(),
            })
            .is_err()
    );
    assert_eq!(
        store
            .list_snapshot()
            .selected_worktree_id_by_project
            .get("p1")
            .map(String::as_str),
        Some("p1")
    );
    assert_eq!(
        store.active_workspace_path_for_project("p1").as_deref(),
        Some(project_dir.to_string_lossy().as_ref())
    );

    fs::remove_dir_all(dir).ok();
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("codux-gpui-{label}-{nanos}"))
}

fn state_value(support_dir: &Path) -> Value {
    Value::Object(crate::config::raw_state_snapshot(
        &support_dir.join("state.json"),
    ))
}
