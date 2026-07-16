#[test]
fn project_close_closes_pet_membership() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-project-close-pet-baseline-{}",
        uuid::Uuid::new_v4()
    ));
    let first_dir = support_dir.join("first");
    let second_dir = support_dir.join("second");
    fs::create_dir_all(&first_dir).expect("create first project dir");
    fs::create_dir_all(&second_dir).expect("create second project dir");
    fs::write(
        support_dir.join("state.json"),
        json!({
            "projects": [
                {
                    "id": "project-1",
                    "name": "First",
                    "path": first_dir.to_string_lossy()
                },
                {
                    "id": "project-2",
                    "name": "Second",
                    "path": second_dir.to_string_lossy()
                }
            ],
            "selectedProjectId": "project-1"
        })
        .to_string(),
    )
    .expect("write state");
    let pet_snapshot = crate::pet::PetSnapshot {
        claimed_at: Some(1),
        species: "codux".to_string(),
        memberships: vec![
            crate::pet::PetProjectMembership {
                project_path: codux_runtime_core::path::normalize_local_path(&first_dir),
                included_at: 1,
                excluded_at: None,
            },
            crate::pet::PetProjectMembership {
                project_path: codux_runtime_core::path::normalize_local_path(&second_dir),
                included_at: 1,
                excluded_at: None,
            },
        ],
        ..crate::pet::PetSnapshot::default()
    };
    fs::write(
        support_dir.join("pet-state.json"),
        serde_json::to_vec(&pet_snapshot).expect("encode pet"),
    )
    .expect("write pet state");

    let service = RuntimeService::new(PathBuf::from(&support_dir));

    service
        .project_close(crate::project_store::ProjectCloseRequest {
            project_id: "project-1".to_string(),
        })
        .expect("close first project");
    let pet = service.pet_snapshot().expect("pet snapshot after close");
    assert!(pet.memberships.iter().any(|membership| {
        membership.project_path == codux_runtime_core::path::normalize_local_path(&first_dir)
            && membership.excluded_at.is_some()
    }));
    assert!(pet.memberships.iter().any(|membership| {
        membership.project_path == codux_runtime_core::path::normalize_local_path(&second_dir)
            && membership.excluded_at.is_none()
    }));

    let _ = fs::remove_dir_all(support_dir);
}

#[test]
fn project_close_cleans_workspace_cache_for_root_and_worktrees() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-project-close-workspace-cache-{}",
        uuid::Uuid::new_v4()
    ));
    let project_dir = support_dir.join("project");
    let worktree_dir = support_dir.join("worktree");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::create_dir_all(&worktree_dir).expect("create worktree dir");
    fs::write(
        support_dir.join("state.json"),
        json!({
            "projects": [
                {
                    "id": "project-1",
                    "name": "Project",
                    "path": project_dir.to_string_lossy()
                }
            ],
            "worktrees": [
                {
                    "id": "worktree-1",
                    "projectId": "project-1",
                    "name": "Task",
                    "branch": "task",
                    "path": worktree_dir.to_string_lossy(),
                    "status": "active",
                    "isDefault": false,
                    "createdAt": 1,
                    "updatedAt": 1
                }
            ],
            "worktreeTasks": [
                {
                    "worktreeId": "worktree-1",
                    "title": "Task",
                    "baseBranch": "main",
                    "status": "active",
                    "createdAt": 1,
                    "updatedAt": 1
                }
            ],
            "selectedProjectId": "project-1",
            "selectedWorktreeIdByProject": {
                "project-1": "worktree-1"
            }
        })
        .to_string(),
    )
    .expect("write state");

    let service = RuntimeService::new(PathBuf::from(&support_dir));
    service
        .save_terminal_layout(
            "project-1",
            Vec::new(),
            "terminal-1".to_string(),
            vec![TerminalPaneSummary {
                title: "Shell".to_string(),
                terminal_id: "terminal-1".to_string(),
            }],
            vec![1.0],
            0.24,
        )
        .expect("save project terminal layout");
    service
        .save_file_editor_layout(
            "worktree-1",
            vec![FileEditorTabSummary {
                path: "src/main.rs".to_string(),
                label: "main.rs".to_string(),
                language: "rust".to_string(),
            }],
            Some("src/main.rs".to_string()),
        )
        .expect("save worktree file editor layout");
    let obsolete_cache =
        crate::persistent_cache::PersistentCacheStore::for_support_dir(support_dir.clone())
            .expect("obsolete cache");
    obsolete_cache
        .put_json(
            "file-tree-state",
            "worktree-1",
            &serde_json::json!({
                "fileDirectory": "src",
                "selectedFileEntry": "src/main.rs"
            }),
        )
        .expect("save obsolete file tree state");
    obsolete_cache
        .put_json(
            "git-ui-state",
            "worktree-1",
            &serde_json::json!({
                "selectedGitFile": "src/main.rs"
            }),
        )
        .expect("save obsolete git ui state");

    let pet_snapshot = crate::pet::PetSnapshot {
        claimed_at: Some(1),
        species: "codux".to_string(),
        memberships: vec![
            crate::pet::PetProjectMembership {
                project_path: codux_runtime_core::path::normalize_local_path(&project_dir),
                included_at: 1,
                excluded_at: None,
            },
            crate::pet::PetProjectMembership {
                project_path: codux_runtime_core::path::normalize_local_path(&worktree_dir),
                included_at: 1,
                excluded_at: None,
            },
        ],
        ..crate::pet::PetSnapshot::default()
    };
    fs::write(
        support_dir.join("pet-state.json"),
        serde_json::to_vec(&pet_snapshot).expect("encode pet"),
    )
    .expect("write pet state");

    service
        .project_close(ProjectCloseRequest {
            project_id: "project-1".to_string(),
        })
        .expect("close project");

    assert!(service.project_list().projects.is_empty());
    assert!(
        service
            .project_list()
            .selected_worktree_id_by_project
            .is_empty()
    );
    assert!(service.terminal_layout_record("project-1").is_none());
    assert!(
        service
            .reload_file_editor_layout(Some("worktree-1"))
            .tabs
            .is_empty()
    );
    assert_eq!(
        obsolete_cache
            .get_json::<serde_json::Value>("file-tree-state", "worktree-1")
            .expect("load obsolete file tree state"),
        None
    );
    assert_eq!(
        obsolete_cache
            .get_json::<serde_json::Value>("git-ui-state", "worktree-1")
            .expect("load obsolete git ui state"),
        None
    );
    let pet = service.pet_snapshot().expect("pet snapshot");
    assert!(pet.memberships.iter().any(|membership| {
        membership.project_path == codux_runtime_core::path::normalize_local_path(&project_dir)
            && membership.excluded_at.is_some()
    }));
    assert!(pet.memberships.iter().any(|membership| {
        membership.project_path == codux_runtime_core::path::normalize_local_path(&worktree_dir)
            && membership.excluded_at.is_some()
    }));

    let _ = fs::remove_dir_all(support_dir);
}

#[test]
fn project_close_cleans_remote_project_state() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-project-close-remote-state-{}",
        uuid::Uuid::new_v4()
    ));
    let project_dir = support_dir.join("project");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::write(
        support_dir.join("state.json"),
        json!({
            "projects": [{
                "id": "project-1",
                "name": "Project",
                "path": project_dir.to_string_lossy()
            }],
            "selectedProjectId": "project-1"
        })
        .to_string(),
    )
    .expect("write state");
    let service = RuntimeService::new(support_dir.clone());
    let resource = codux_protocol::REMOTE_RESOURCE_AI_STATS;
    service.remote_host.resource_subscriptions.subscribe(
        resource,
        Some("project-1"),
        None,
        "device-1",
    );
    service.remote_host.resource_subscriptions.next_version(
        resource,
        Some("project-1"),
        None,
    );
    service
        .remote_host
        .remote_project_scope_by_device
        .lock()
        .expect("project scopes")
        .insert("device-1".to_string(), "project-1".to_string());
    service
        .remote_host
        .ai_stats_watchers
        .lock()
        .expect("ai stats watchers")
        .entry("project-1".to_string())
        .or_default()
        .insert("device-1".to_string(), "project-1".to_string());

    service
        .project_close(ProjectCloseRequest {
            project_id: "project-1".to_string(),
        })
        .expect("close project");

    assert!(
        service
            .remote_host
            .resource_subscriptions
            .devices_for(resource, Some("project-1"), None)
            .is_empty()
    );
    assert_eq!(
        service
            .remote_host
            .resource_subscriptions
            .current_version(resource, Some("project-1"), None),
        0
    );
    assert!(
        service
            .remote_host
            .remote_project_scope_by_device
            .lock()
            .expect("project scopes")
            .is_empty()
    );
    assert!(
        service
            .remote_host
            .ai_stats_watchers
            .lock()
            .expect("ai stats watchers")
            .is_empty()
    );

    let _ = fs::remove_dir_all(support_dir);
}
