use super::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn project_root_missing_only_reports_deleted_local_projects() {
    let dir = temp_dir("project-root-missing");
    let local_dir = dir.join("local-project");
    let support_dir = dir.join("support");
    fs::create_dir_all(&local_dir).unwrap();
    fs::create_dir_all(&support_dir).unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "projects": [
                {"id": "local", "path": local_dir},
                {
                    "id": "remote",
                    "path": "/srv/project",
                    "runtimeTarget": {"kind": "remote", "deviceId": "device-1"}
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let store = ProjectStore::new(support_dir);

    assert!(!store.project_root_missing("local"));
    fs::remove_dir_all(&local_dir).unwrap();
    assert!(store.project_root_missing("local"));
    assert!(!store.project_root_missing("remote"));
    assert!(!store.project_root_missing("unknown"));

    fs::remove_dir_all(dir).ok();
}

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
fn legacy_host_device_id_loads_as_remote_runtime_target() {
    let dir = temp_dir("project-store-host-device");
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "projects": [{
                "id": "remote-project",
                "name": "Remote",
                "path": "/srv/project",
                "hostDeviceId": "device-xyz"
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let state = state_value(&support_dir);
    let reloaded: Vec<ProjectRecord> = serde_json::from_value(state["projects"].clone()).unwrap();
    assert_eq!(
        reloaded[0].runtime_target,
        ProjectRuntimeTarget::Remote {
            device_id: "device-xyz".to_string()
        }
    );

    fs::remove_dir_all(dir).ok();
}

#[test]
fn create_project_persists_runtime_target_without_legacy_field() {
    let dir = temp_dir("project-store-runtime-target");
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();

    ProjectStore::new(support_dir.clone())
        .create_project(ProjectCreateRequest {
            name: "Remote".to_string(),
            path: "/srv/project".to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
            environment_variables: Default::default(),
            runtime_target: ProjectRuntimeTarget::Remote {
                device_id: "device-xyz".to_string(),
            },
        })
        .unwrap();

    let state = state_value(&support_dir);
    assert_eq!(state["projects"][0]["runtimeTarget"]["kind"], "remote");
    assert_eq!(
        state["projects"][0]["runtimeTarget"]["deviceId"],
        "device-xyz"
    );
    assert!(state["projects"][0].get("hostDeviceId").is_none());

    fs::remove_dir_all(dir).ok();
}

#[test]
fn project_environment_variables_are_persisted_and_summarized() {
    let dir = temp_dir("project-store-environment");
    let project_dir = dir.join("project");
    let support_dir = dir.join("support");
    fs::create_dir_all(&project_dir).unwrap();
    fs::create_dir_all(&support_dir).unwrap();
    let mut variables = std::collections::BTreeMap::new();
    variables.insert(" API_BASE ".to_string(), "https://example.test".to_string());
    variables.insert("".to_string(), "ignored".to_string());
    variables.insert("CODUX_PROJECT_ID".to_string(), "reserved".to_string());
    variables.insert("dmux_session_id".to_string(), "reserved".to_string());

    let error = ProjectStore::new(support_dir.clone())
        .create_project(ProjectCreateRequest {
            name: "Project".to_string(),
            path: project_dir.display().to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
            environment_variables: variables,
            runtime_target: ProjectRuntimeTarget::Local,
        })
        .expect_err("reserved environment key should be rejected");
    assert!(error.contains("CODUX_PROJECT_ID"));

    let mut variables = std::collections::BTreeMap::new();
    variables.insert(" API_BASE ".to_string(), "https://example.test".to_string());
    variables.insert("".to_string(), "ignored".to_string());
    let snapshot = ProjectStore::new(support_dir.clone())
        .create_project(ProjectCreateRequest {
            name: "Project".to_string(),
            path: project_dir.display().to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
            environment_variables: variables,
            runtime_target: ProjectRuntimeTarget::Local,
        })
        .unwrap();

    let project = snapshot.projects.first().expect("project");
    assert_eq!(
        project
            .environment_variables
            .get("API_BASE")
            .map(String::as_str),
        Some("https://example.test")
    );
    assert!(!project.environment_variables.contains_key(""));
    let state = state_value(&support_dir);
    assert_eq!(
        state["projects"][0]["environmentVariables"]["API_BASE"],
        "https://example.test"
    );
    assert!(
        state["projects"][0]["environmentVariables"]
            .get("")
            .is_none()
    );

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
            environment_variables: Default::default(),
            runtime_target: ProjectRuntimeTarget::Remote {
                device_id: "device-win".to_string(),
            },
        })
        .expect("creating a remote project must not require the path to exist locally");

    let state = state_value(&support_dir);
    // The host path is stored verbatim (not canonicalized away), and tagged with
    // its host so the project routes over the controller.
    assert_eq!(state["projects"][0]["path"], host_path);
    assert_eq!(
        state["projects"][0]["runtimeTarget"]["deviceId"],
        "device-win"
    );

    fs::remove_dir_all(dir).ok();
}

#[test]
fn remote_windows_workspace_routes_equivalent_path_forms() {
    let dir = temp_dir("project-store-remote-windows-path");
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();
    let store = ProjectStore::new(support_dir.clone());
    store
        .create_project(ProjectCreateRequest {
            name: String::new(),
            path: r"F:\Projects\Codux\".to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
            environment_variables: Default::default(),
            runtime_target: ProjectRuntimeTarget::Remote {
                device_id: "device-win".to_string(),
            },
        })
        .unwrap();

    assert_eq!(
        store
            .runtime_target_for_workspace_path(r"\\?\f:\projects\codux")
            .unwrap(),
        ProjectRuntimeTarget::Remote {
            device_id: "device-win".to_string()
        }
    );
    let summary = store
        .workspace_summary_by_path("f:/PROJECTS/codux")
        .expect("equivalent remote workspace");
    assert_eq!(summary.name, "Codux");

    fs::remove_dir_all(dir).ok();
}

#[test]
fn same_wsl_path_in_different_distributions_creates_distinct_projects() {
    let dir = temp_dir("project-store-wsl-identity");
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();
    let store = ProjectStore::new(support_dir.clone());

    for distribution in ["Ubuntu", "Debian"] {
        store
            .create_project(ProjectCreateRequest {
                name: "Project".to_string(),
                path: "/home/user/project".to_string(),
                badge_text: None,
                badge_symbol: None,
                badge_color_hex: None,
                environment_variables: Default::default(),
                runtime_target: ProjectRuntimeTarget::Wsl {
                    distribution: distribution.to_string(),
                },
            })
            .unwrap();
    }

    let projects = store.projects_snapshot();
    assert_eq!(projects.len(), 2);
    assert_ne!(projects[0].id, projects[1].id);
    assert_eq!(
        store
            .runtime_target_for_workspace_path("/home/user/project")
            .unwrap(),
        ProjectRuntimeTarget::Wsl {
            distribution: "Debian".to_string()
        }
    );

    fs::remove_dir_all(dir).ok();
}

#[test]
fn duplicate_workspace_path_without_selection_is_rejected() {
    let dir = temp_dir("project-store-wsl-ambiguous");
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "projects": [
                {
                    "id": "ubuntu",
                    "name": "Project",
                    "path": "/home/user/project",
                    "runtimeTarget": { "kind": "wsl", "distribution": "Ubuntu" }
                },
                {
                    "id": "debian",
                    "name": "Project",
                    "path": "/home/user/project",
                    "runtimeTarget": { "kind": "wsl", "distribution": "Debian" }
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let error = ProjectStore::new(support_dir.clone())
        .runtime_target_for_workspace_path("/home/user/project")
        .unwrap_err();
    assert!(error.contains("multiple runtime targets"));

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

#[test]
fn hosted_worktree_sync_preserves_other_projects_and_allows_selection() {
    let dir = temp_dir("project-store-hosted-worktree-sync");
    let support_dir = dir.join("support");
    fs::create_dir_all(&support_dir).unwrap();
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "projects": [
                {
                    "id": "wsl-project",
                    "name": "WSL",
                    "path": "/root/project",
                    "runtimeTarget": { "kind": "wsl", "distribution": "Ubuntu" }
                },
                { "id": "local-project", "name": "Local", "path": "/tmp/local" }
            ],
            "worktrees": [
                {
                    "id": "old-wsl",
                    "projectId": "wsl-project",
                    "name": "Old",
                    "branch": "old",
                    "path": "/root/old",
                    "status": "active",
                    "isDefault": false,
                    "createdAt": 1,
                    "updatedAt": 1
                },
                {
                    "id": "local-worktree",
                    "projectId": "local-project",
                    "name": "Local",
                    "branch": "local",
                    "path": "/tmp/local-worktree",
                    "status": "active",
                    "isDefault": false,
                    "createdAt": 1,
                    "updatedAt": 1
                }
            ],
            "worktreeTasks": [
                {
                    "worktreeId": "old-wsl",
                    "title": "Old task",
                    "baseBranch": "main",
                    "baseCommit": null,
                    "status": "active",
                    "createdAt": 1,
                    "updatedAt": 1,
                    "startedAt": null,
                    "completedAt": null
                },
                {
                    "worktreeId": "local-worktree",
                    "title": "Local task",
                    "baseBranch": "main",
                    "baseCommit": null,
                    "status": "active",
                    "createdAt": 1,
                    "updatedAt": 1,
                    "startedAt": null,
                    "completedAt": null
                }
            ],
            "selectedWorktreeIdByProject": {
                "wsl-project": "old-wsl",
                "local-project": "local-worktree"
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let store = ProjectStore::new(support_dir.clone());
    store
        .replace_project_worktree_state(
            "wsl-project",
            vec![ProjectWorktreeRecord {
                id: "new-wsl".to_string(),
                project_id: "wsl-project".to_string(),
                name: "New".to_string(),
                branch: "new".to_string(),
                path: "/root/new".to_string(),
                status: "active".to_string(),
                is_default: false,
                created_at: 2,
                updated_at: 2,
            }],
            vec![WorktreeTaskRecord {
                worktree_id: "new-wsl".to_string(),
                title: "New task".to_string(),
                base_branch: "main".to_string(),
                base_commit: None,
                status: "active".to_string(),
                created_at: 2,
                updated_at: 2,
                started_at: None,
                completed_at: None,
            }],
            Some("new-wsl"),
        )
        .unwrap();

    let state = state_value(&support_dir);
    let worktrees = state["worktrees"].as_array().unwrap();
    assert!(worktrees.iter().any(|worktree| worktree["id"] == "new-wsl"));
    assert!(
        worktrees
            .iter()
            .any(|worktree| worktree["id"] == "local-worktree")
    );
    assert!(!worktrees.iter().any(|worktree| worktree["id"] == "old-wsl"));
    let tasks = state["worktreeTasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().any(|task| task["worktreeId"] == "new-wsl"));
    assert!(
        tasks
            .iter()
            .any(|task| task["worktreeId"] == "local-worktree")
    );
    assert!(!tasks.iter().any(|task| task["worktreeId"] == "old-wsl"));
    assert_eq!(
        state["selectedWorktreeIdByProject"]["wsl-project"],
        "new-wsl"
    );
    assert_eq!(
        state["selectedWorktreeIdByProject"]["local-project"],
        "local-worktree"
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
