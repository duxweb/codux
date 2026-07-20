#[test]
fn launch_artifacts_include_tool_context_when_memory_is_disabled() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-runtime-tool-context-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&support_dir).unwrap();
    fs::write(
        support_dir.join("settings.json"),
        serde_json::json!({
            "ai": {
                "globalPrompt": "",
                "memory": {
                    "enabled": false,
                    "automaticInjectionEnabled": false
                }
            }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        support_dir.join("ssh_profiles.json"),
        serde_json::json!([{
            "id": "profile-1",
            "name": "Production",
            "host": "example.com",
            "port": 22,
            "username": "root",
            "credentialKind": "password",
            "privateKeyPath": "",
            "password": "secret-password",
            "keyPassphrase": "secret-passphrase",
            "updatedAt": 1
        }])
        .to_string(),
    )
    .unwrap();
    fs::write(
        support_dir.join("db_profiles.json"),
        serde_json::json!([{
            "id": "db-1",
            "projectId": "project-a",
            "name": "Production DB",
            "engine": "postgres",
            "host": "db.example.com",
            "port": 5432,
            "database": "app",
            "username": "app_user",
            "password": "db-secret",
            "sslMode": "require",
            "readOnly": true,
            "updatedAt": 1
        }])
        .to_string(),
    )
    .unwrap();

    let service = RuntimeService::new(support_dir.clone());
    let workspace_id = format!("project-a-{}", uuid::Uuid::new_v4());
    let artifacts = service
        .prepare_workspace_memory_launch_artifacts(
            "project-a",
            &workspace_id,
            "Project A",
            "/workspace/project-a",
        )
        .expect("tool launch context should create artifacts");
    let agents = fs::read_to_string(artifacts.workspace_root.join("AGENTS.md")).unwrap();

    assert!(agents.starts_with("# Codux Environment Directive"));
    assert!(agents.contains("codux-ssh list"));
    assert!(agents.contains("codux-ssh <profile-id> -- '<remote-command>'"));
    assert!(agents.contains("Do not grep the repository"));
    assert!(!agents.contains("profile-1"));
    assert!(!agents.contains("root@example.com:22"));
    assert!(!agents.contains("secret-password"));
    assert!(!agents.contains("secret-passphrase"));
    assert!(agents.contains("codux-db list"));
    assert!(agents.contains("codux-db <profile-id> -- '<SQL>'"));
    assert!(agents.contains("codux-worktree create"));
    assert!(agents.contains("waits for the child agent to complete"));
    assert!(agents.contains("cast them to text"));
    assert!(!agents.contains("db-1"));
    assert!(!agents.contains("db.example.com:5432 / app"));
    assert!(!agents.contains("db-secret"));
    assert!(!agents.contains("app_user"));
    assert!(!agents.contains("project active entry"));

    fs::remove_dir_all(support_dir).ok();
    fs::remove_dir_all(artifacts.workspace_root).ok();
}

#[test]
fn launch_artifacts_include_environment_directive_without_profiles() {
    let support_dir = std::env::temp_dir().join(format!(
        "codux-runtime-environment-directive-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&support_dir).unwrap();
    fs::write(
        support_dir.join("settings.json"),
        serde_json::json!({
            "ai": {
                "globalPrompt": "",
                "memory": {
                    "enabled": false,
                    "automaticInjectionEnabled": false
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    let service = RuntimeService::new(support_dir.clone());
    let workspace_id = format!("project-a-{}", uuid::Uuid::new_v4());
    let artifacts = service
        .prepare_workspace_memory_launch_artifacts(
            "project-a",
            &workspace_id,
            "Project A",
            "/workspace/project-a",
        )
        .expect("environment directive should create artifacts");
    let agents = fs::read_to_string(artifacts.workspace_root.join("AGENTS.md")).unwrap();

    assert!(agents.starts_with("# Codux Environment Directive"));
    assert!(agents.contains("codux-ssh list"));
    assert!(agents.contains("codux-db list"));
    assert!(agents.contains("codux-worktree create"));
    assert!(agents.contains("# Codux Memory"));
    assert!(!agents.contains("project active entry"));

    fs::remove_dir_all(support_dir).ok();
    fs::remove_dir_all(artifacts.workspace_root).ok();
}
