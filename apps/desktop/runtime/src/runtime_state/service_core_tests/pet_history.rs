fn write_pet_test_projects(
    support_dir: &Path,
    projects: &[(&str, &str, &Path)],
) {
    let projects = projects
        .iter()
        .map(|(id, name, path)| {
            json!({
                "id": id,
                "name": name,
                "path": path.to_string_lossy()
            })
        })
        .collect::<Vec<_>>();
    let mut snapshot = serde_json::Map::new();
    snapshot.insert("projects".to_string(), json!(projects));
    snapshot.insert(
        "selectedProjectId".to_string(),
        json!(projects.first().and_then(|project| project["id"].as_str())),
    );
    // state.json is served by a process-global in-memory ConfigStore; writes
    // must go through it or a running service never sees the change.
    crate::config::save_raw_state_snapshot(
        &crate::config::state_file_path(support_dir.to_path_buf()),
        &snapshot,
    )
    .expect("write project state");
}

fn replace_usage_event_total(
    support_dir: &Path,
    project_dir: &Path,
    session_key: &str,
    total_tokens: i64,
) {
    let store = crate::ai_usage_store::AIUsageStore::at_path(support_dir.join("ai-usage.sqlite3"));
    let conn = store.connect().expect("connect ai usage store");
    conn.execute(
        r#"
        UPDATE ai_history_file_usage_event
        SET total_tokens = ?1
        WHERE project_path = ?2 AND session_key = ?3
        "#,
        rusqlite::params![
            total_tokens,
            codux_runtime_core::path::normalize_local_path(project_dir),
            session_key
        ],
    )
    .expect("replace usage event total");
}

#[test]
fn pet_history_rebuild_uses_claim_time_and_can_correct_experience_downward() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-pet-history-rebuild-{}",
        uuid::Uuid::new_v4()
    ));
    let project_dir = support_dir.join("project");
    fs::create_dir_all(&project_dir).expect("create project dir");
    write_pet_test_projects(
        &support_dir,
        &[("project-1", "Project", project_dir.as_path())],
    );

    let today_start = crate::ai_history_normalized::local_day_start_seconds(
        crate::ai_history_normalized::now_seconds(),
    ) as i64;
    let claimed_at = today_start + 10;
    let pet_snapshot = crate::pet::PetSnapshot {
        state_version: crate::pet::PetSnapshot::default().state_version - 1,
        claimed_at: Some(claimed_at),
        species: "voidcat".to_string(),
        current_experience_tokens: 9_999_999,
        ..crate::pet::PetSnapshot::default()
    };
    fs::write(
        support_dir.join("pet-state.json"),
        serde_json::to_vec(&pet_snapshot).expect("encode pet state"),
    )
    .expect("write pet state");
    write_usage_bucket(
        &support_dir,
        &project_dir,
        "project-1",
        "Project",
        "before-claim",
        100,
        (claimed_at - 1) as f64,
    );
    write_usage_bucket(
        &support_dir,
        &project_dir,
        "project-1",
        "Project",
        "after-claim",
        30,
        (claimed_at + 1) as f64,
    );

    let service = RuntimeService::new(support_dir.clone());
    let rebuilt = service
        .refresh_pet_from_indexed_history()
        .expect("rebuild pet experience");
    assert_eq!(rebuilt.total_xp, 30);
    assert_eq!(rebuilt.daily_xp, 30);
    let snapshot = service.pet_snapshot().expect("rebuilt pet snapshot");
    assert_eq!(
        snapshot.state_version,
        crate::pet::PetSnapshot::default().state_version
    );
    assert_eq!(snapshot.memberships.len(), 1);
    assert_eq!(snapshot.memberships[0].included_at, claimed_at);

    replace_usage_event_total(&support_dir, &project_dir, "after-claim", 12);
    let corrected = service
        .refresh_pet_from_indexed_history()
        .expect("correct pet experience after rebuild");
    assert_eq!(corrected.total_xp, 12);
    assert_eq!(corrected.daily_xp, 12);
    assert_eq!(
        service
            .refresh_pet_from_indexed_history()
            .expect("repeat corrected refresh")
            .total_xp,
        12
    );

    let _ = fs::remove_dir_all(support_dir);
}

#[test]
fn pet_experience_survives_project_id_change_and_reindex() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-pet-project-id-change-{}",
        uuid::Uuid::new_v4()
    ));
    let project_dir = support_dir.join("project");
    fs::create_dir_all(&project_dir).expect("create project dir");
    write_pet_test_projects(
        &support_dir,
        &[("project-old", "Old Name", project_dir.as_path())],
    );

    let now = crate::ai_history_normalized::now_seconds() as i64;
    let pet_snapshot = crate::pet::PetSnapshot {
        state_version: crate::pet::PetSnapshot::default().state_version - 1,
        claimed_at: Some(now - 100),
        species: "voidcat".to_string(),
        ..crate::pet::PetSnapshot::default()
    };
    fs::write(
        support_dir.join("pet-state.json"),
        serde_json::to_vec(&pet_snapshot).expect("encode pet state"),
    )
    .expect("write pet state");
    write_usage_bucket(
        &support_dir,
        &project_dir,
        "project-old",
        "Old Name",
        "session",
        42,
        (now - 10) as f64,
    );

    let service = RuntimeService::new(support_dir.clone());
    assert_eq!(
        service
            .refresh_pet_from_indexed_history()
            .expect("initial pet refresh")
            .total_xp,
        42
    );

    write_pet_test_projects(
        &support_dir,
        &[("project-new", "New Name", project_dir.as_path())],
    );
    let usage_store =
        crate::ai_usage_store::AIUsageStore::at_path(support_dir.join("ai-usage.sqlite3"));
    let conn = usage_store.connect().expect("connect usage store");
    conn.execute(
        "UPDATE ai_history_file_usage_event SET project_id = 'project-new';",
        [],
    )
    .expect("simulate reindex with new project id");

    assert_eq!(
        service
            .refresh_pet_from_indexed_history()
            .expect("refresh after id change")
            .total_xp,
        42
    );
    let snapshot = service.pet_snapshot().expect("pet snapshot");
    assert_eq!(snapshot.memberships.len(), 1);
    assert_eq!(
        snapshot.memberships[0].project_path,
        codux_runtime_core::path::normalize_local_path(&project_dir)
    );

    let _ = fs::remove_dir_all(support_dir);
}

#[test]
fn pet_membership_lifecycle_counts_only_included_history() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-pet-membership-lifecycle-{}",
        uuid::Uuid::new_v4()
    ));
    let first_dir = support_dir.join("first");
    let second_dir = support_dir.join("second");
    fs::create_dir_all(&first_dir).expect("create first project dir");
    fs::create_dir_all(&second_dir).expect("create second project dir");
    write_pet_test_projects(
        &support_dir,
        &[("project-1", "First", first_dir.as_path())],
    );

    let now = crate::ai_history_normalized::now_seconds() as i64;
    let pet_snapshot = crate::pet::PetSnapshot {
        state_version: crate::pet::PetSnapshot::default().state_version - 1,
        claimed_at: Some(now - 1_000),
        species: "voidcat".to_string(),
        ..crate::pet::PetSnapshot::default()
    };
    fs::write(
        support_dir.join("pet-state.json"),
        serde_json::to_vec(&pet_snapshot).expect("encode pet state"),
    )
    .expect("write pet state");
    write_usage_bucket(
        &support_dir,
        &first_dir,
        "project-1",
        "First",
        "first-active",
        20,
        (now - 500) as f64,
    );

    let service = RuntimeService::new(support_dir.clone());
    assert_eq!(
        service
            .refresh_pet_from_indexed_history()
            .expect("initial pet refresh")
            .total_xp,
        20
    );

    write_usage_bucket(
        &support_dir,
        &second_dir,
        "project-2",
        "Second",
        "second-before-add",
        10_000,
        (now - 500) as f64,
    );
    write_pet_test_projects(
        &support_dir,
        &[
            ("project-1", "First", first_dir.as_path()),
            ("project-2", "Second", second_dir.as_path()),
        ],
    );
    assert_eq!(
        service
            .refresh_pet_from_indexed_history()
            .expect("refresh after adding project")
            .total_xp,
        20
    );
    let second_included_at = service
        .pet_snapshot()
        .expect("pet snapshot after add")
        .memberships
        .iter()
        .find(|membership| {
            membership.project_path
                == codux_runtime_core::path::normalize_local_path(&second_dir)
                && membership.excluded_at.is_none()
        })
        .expect("active second membership")
        .included_at;
    write_usage_bucket(
        &support_dir,
        &second_dir,
        "project-2",
        "Second",
        "second-after-add",
        5,
        (second_included_at + 1) as f64,
    );
    assert_eq!(
        service
            .refresh_pet_from_indexed_history()
            .expect("refresh with second project usage")
            .total_xp,
        25
    );

    service
        .project_close(ProjectCloseRequest {
            project_id: "project-1".to_string(),
        })
        .expect("close first project");
    let first_excluded_at = service
        .pet_snapshot()
        .expect("pet snapshot after remove")
        .memberships
        .iter()
        .find(|membership| {
            membership.project_path
                == codux_runtime_core::path::normalize_local_path(&first_dir)
        })
        .and_then(|membership| membership.excluded_at)
        .expect("closed first membership");
    write_usage_bucket(
        &support_dir,
        &first_dir,
        "project-1",
        "First",
        "first-after-remove",
        1_000,
        (first_excluded_at + 1) as f64,
    );
    assert_eq!(
        service
            .refresh_pet_from_indexed_history()
            .expect("refresh after removed project usage")
            .total_xp,
        25
    );

    write_pet_test_projects(
        &support_dir,
        &[
            ("project-1", "First", first_dir.as_path()),
            ("project-2", "Second", second_dir.as_path()),
        ],
    );
    // Seconds-level timestamps cannot order remove → usage → re-add within one
    // real second, so re-add the membership with an explicit later clock.
    let first_reincluded_at = first_excluded_at + 10;
    crate::pet::PetStore::load_or_seed(support_dir.clone())
        .sync_memberships_at(
            vec![
                crate::pet::PetWorkspace {
                    project_path: first_dir.to_string_lossy().into_owned(),
                },
                crate::pet::PetWorkspace {
                    project_path: second_dir.to_string_lossy().into_owned(),
                },
            ],
            Vec::new(),
            first_reincluded_at,
        )
        .expect("readd first project membership");
    write_usage_bucket(
        &support_dir,
        &first_dir,
        "project-1",
        "First",
        "first-after-readd",
        7,
        (first_reincluded_at + 1) as f64,
    );
    assert_eq!(
        service
            .refresh_pet_from_indexed_history()
            .expect("refresh after readded usage")
            .total_xp,
        32
    );

    let _ = fs::remove_dir_all(support_dir);
}

#[test]
fn pet_archive_and_restore_preserve_experience() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-pet-archive-restore-{}",
        uuid::Uuid::new_v4()
    ));
    let project_dir = support_dir.join("project");
    fs::create_dir_all(&project_dir).expect("create project dir");
    write_pet_test_projects(
        &support_dir,
        &[("project-1", "Project", project_dir.as_path())],
    );

    let now = crate::ai_history_normalized::now_seconds() as i64;
    let pet_snapshot = crate::pet::PetSnapshot {
        state_version: crate::pet::PetSnapshot::default().state_version - 1,
        claimed_at: Some(now - 1_000),
        species: "voidcat".to_string(),
        ..crate::pet::PetSnapshot::default()
    };
    fs::write(
        support_dir.join("pet-state.json"),
        serde_json::to_vec(&pet_snapshot).expect("encode pet state"),
    )
    .expect("write pet state");
    write_usage_bucket(
        &support_dir,
        &project_dir,
        "project-1",
        "Project",
        "active",
        20,
        (now - 500) as f64,
    );

    let service = RuntimeService::new(support_dir.clone());
    assert_eq!(
        service
            .refresh_pet_from_indexed_history()
            .expect("initial pet refresh")
            .total_xp,
        20
    );

    let archived = service.archive_current_pet().expect("archive current pet");
    let legacy_id = archived.legacy[0].id.clone();
    assert!(archived.legacy[0]
        .memberships
        .iter()
        .all(|membership| membership.excluded_at.is_some()));
    let restored = service
        .restore_archived_pet(PetRestoreRequest { legacy_id })
        .expect("restore archived pet");
    assert_eq!(restored.current_experience_tokens, 20);
    assert!(restored.memberships.iter().any(|membership| {
        membership.project_path == codux_runtime_core::path::normalize_local_path(&project_dir)
            && membership.excluded_at.is_none()
    }));
    assert_eq!(
        service
            .refresh_pet_from_indexed_history()
            .expect("refresh after restore")
            .total_xp,
        20
    );

    let _ = fs::remove_dir_all(support_dir);
}
