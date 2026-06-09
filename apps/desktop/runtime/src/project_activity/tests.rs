use super::*;

#[test]
fn ai_refresh_uses_foreground_and_background_intervals() {
    let coordinator =
        ProjectActivityCoordinator::new(std::env::temp_dir(), AIHistoryIndexer::new());
    let now = Instant::now();
    {
        let mut projects = coordinator.projects.lock().unwrap();
        projects.insert(
            "active".to_string(),
            TrackedProject {
                id: "active".to_string(),
                name: "Active".to_string(),
                path: "/tmp/active".to_string(),
                last_git_refresh: None,
                last_remote_git_refresh: None,
                last_git_changed_refresh: None,
                last_ai_refresh: Some(now - Duration::from_secs(180)),
            },
        );
        projects.insert(
            "background".to_string(),
            TrackedProject {
                id: "background".to_string(),
                name: "Background".to_string(),
                path: "/tmp/background".to_string(),
                last_git_refresh: None,
                last_remote_git_refresh: None,
                last_git_changed_refresh: None,
                last_ai_refresh: Some(now - Duration::from_secs(180)),
            },
        );
    }
    *coordinator.active_project_id.lock().unwrap() = Some("active".to_string());
    coordinator.mark_main_window_visible(true);

    let due = coordinator.projects_due_for_ai(Duration::from_secs(120), Duration::from_secs(600));

    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, "active");
}

#[test]
fn ai_background_refresh_is_skipped_during_idle_tick() {
    let coordinator =
        ProjectActivityCoordinator::new(std::env::temp_dir(), AIHistoryIndexer::new());
    let now = Instant::now();
    {
        let mut projects = coordinator.projects.lock().unwrap();
        projects.insert(
            "active".to_string(),
            TrackedProject {
                id: "active".to_string(),
                name: "Active".to_string(),
                path: "/tmp/active".to_string(),
                last_git_refresh: None,
                last_remote_git_refresh: None,
                last_git_changed_refresh: None,
                last_ai_refresh: Some(now - Duration::from_secs(700)),
            },
        );
        projects.insert(
            "background".to_string(),
            TrackedProject {
                id: "background".to_string(),
                name: "Background".to_string(),
                path: "/tmp/background".to_string(),
                last_git_refresh: None,
                last_remote_git_refresh: None,
                last_git_changed_refresh: None,
                last_ai_refresh: Some(now - Duration::from_secs(700)),
            },
        );
    }
    *coordinator.active_project_id.lock().unwrap() = Some("active".to_string());
    coordinator.mark_main_window_visible(true);

    let due = coordinator.projects_due_for_ai(Duration::from_secs(120), Duration::from_secs(600));
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, "active");
}

#[test]
fn git_background_refresh_is_limited_per_tick() {
    let projects = Mutex::new(HashMap::new());
    let now = Instant::now();
    {
        let mut guard = projects.lock().unwrap();
        for index in 0..5 {
            guard.insert(
                format!("background-{index}"),
                TrackedProject {
                    id: format!("background-{index}"),
                    name: format!("Background {index}"),
                    path: format!("/tmp/background-{index}"),
                    last_git_refresh: Some(now - Duration::from_secs(700)),
                    last_remote_git_refresh: None,
                    last_git_changed_refresh: None,
                    last_ai_refresh: None,
                },
            );
        }
        guard.insert(
            "active".to_string(),
            TrackedProject {
                id: "active".to_string(),
                name: "Active".to_string(),
                path: "/tmp/active".to_string(),
                last_git_refresh: Some(now - Duration::from_secs(30)),
                last_remote_git_refresh: None,
                last_git_changed_refresh: None,
                last_ai_refresh: None,
            },
        );
    }

    let due = projects_due_for_git_interval(
        &projects,
        Some("active"),
        true,
        Duration::from_secs(15),
        Duration::from_secs(600),
        0,
    );
    let active_count = due.iter().filter(|project| project.id == "active").count();
    let background_count = due.iter().filter(|project| project.id != "active").count();

    assert_eq!(active_count, 1);
    assert_eq!(background_count, 0);
    assert_eq!(due.len(), 1);
}

#[test]
fn git_changed_refresh_is_debounced_per_project() {
    let support_dir =
        std::env::temp_dir().join(format!("codux-project-activity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&support_dir);
    std::fs::create_dir_all(&support_dir).expect("create temp support dir");
    std::fs::write(
        support_dir.join("state.json"),
        r#"{"projects":[{"id":"p1","name":"Project","path":"/tmp/project"}]}"#,
    )
    .expect("write state");

    let coordinator = ProjectActivityCoordinator::new(support_dir.clone(), AIHistoryIndexer::new());
    let store = ProjectStore::new(support_dir.clone());

    coordinator.refresh_git_changed(
        &store,
        "/tmp/project".to_string(),
        "/tmp/project".to_string(),
        vec!["a".to_string()],
    );
    let first = coordinator
        .projects
        .lock()
        .unwrap()
        .get("p1")
        .and_then(|project| project.last_git_changed_refresh)
        .expect("first changed refresh");

    coordinator.refresh_git_changed(
        &store,
        "/tmp/project".to_string(),
        "/tmp/project".to_string(),
        vec!["b".to_string()],
    );
    let second = coordinator
        .projects
        .lock()
        .unwrap()
        .get("p1")
        .and_then(|project| project.last_git_changed_refresh)
        .expect("second changed refresh");

    assert_eq!(first, second);
    assert_eq!(coordinator.drain_events().len(), 2);
    let _ = std::fs::remove_dir_all(support_dir);
}

#[test]
fn remote_git_refresh_is_throttled_per_project() {
    let coordinator =
        ProjectActivityCoordinator::new(std::env::temp_dir(), AIHistoryIndexer::new());
    let project = ProjectSummary {
        id: "p1".to_string(),
        name: "Project".to_string(),
        path: "/tmp/project".to_string(),
        badge: String::new(),
        status: "active".to_string(),
        branch: "main".to_string(),
        changes: 0,
        badge_symbol: None,
        badge_color_hex: None,
        git_default_push_remote_name: None,
    };

    coordinator.mark_project_summary(&project);
    coordinator.refresh_git_once(&project);
    let first = coordinator
        .projects
        .lock()
        .unwrap()
        .get("p1")
        .and_then(|project| project.last_remote_git_refresh)
        .expect("first remote refresh");
    coordinator.refresh_git_once(&project);
    let second = coordinator
        .projects
        .lock()
        .unwrap()
        .get("p1")
        .and_then(|project| project.last_remote_git_refresh)
        .expect("second remote refresh");

    assert_eq!(first, second);
}
