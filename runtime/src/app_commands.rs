use crate::{
    app_info::{
        AppAboutMetadata, AppDiagnosticsSnapshot, DiagnosticsExportRequest, DiagnosticsExportResult,
        UpdateInstallResult, UpdateStatus,
    },
    ai_history_indexer::AIHistoryProjectState,
    ai_history_normalized::{AIGlobalHistorySnapshot, AIHistoryProjectRequest},
    git::{GitPushRemoteBranchRequest, GitPushRemoteRequest, GitSummary},
    memory::{
        MemoryExtractionStatusSnapshot, MemoryProjectMigrationRequest, MemorySummaryRow,
        MemorySummaryUpdateRequest,
    },
    notification::{NotificationDispatchRequest, NotificationDispatchResult},
    performance::{PerformanceMonitor, PerformanceSnapshot},
    pet::{
        PetClaimRequest, PetCustomPet, PetCustomPetInstallPreview, PetCustomPetInstallRequest,
        PetRefreshRequest, PetRenameRequest, PetRestoreRequest, PetSnapshot,
    },
    power::PowerManager,
    project_activity::ProjectActivitySnapshot,
    project_store::{ProjectListSnapshot, ProjectSummary},
    remote::RemoteSummary,
    runtime_state::RuntimeService,
    settings::{AppSettings, AppSettingsStore, sync_process_locale_preference},
    ssh::{SSHProfileTestResult, SSHProfileUpsertRequest, SSHProfilesSnapshot},
    worktree::{WorktreeCreateRequest, WorktreeMergeRequest, WorktreeRemoveRequest, WorktreeSnapshot},
};
use std::path::PathBuf;

pub fn app_about_metadata(
    version: impl Into<String>,
    identifier: impl Into<String>,
) -> AppAboutMetadata {
    crate::app_info::about_metadata(version, identifier)
}

pub fn app_update_status(
    settings: &AppSettings,
    repo_root: PathBuf,
    current_version: &str,
) -> Result<UpdateStatus, String> {
    Ok(crate::app_info::update_status(
        settings,
        repo_root,
        current_version,
    ))
}

pub fn app_update_install(
    settings: &AppSettings,
    repo_root: PathBuf,
    current_version: &str,
) -> Result<UpdateInstallResult, String> {
    crate::app_info::install_update(settings, repo_root, current_version)
}

pub fn diagnostics_export(
    request: DiagnosticsExportRequest,
    about: AppAboutMetadata,
    update: UpdateStatus,
    snapshot: AppDiagnosticsSnapshot,
) -> Result<DiagnosticsExportResult, String> {
    crate::app_info::export_diagnostics(request, about, update, snapshot)
}

pub fn app_open_runtime_log() -> Result<(), String> {
    crate::app_info::open_runtime_log()
}

pub fn app_open_live_log() -> Result<(), String> {
    crate::app_info::open_live_log()
}

pub fn app_open_url(url: String) -> Result<(), String> {
    crate::app_info::open_url(&url)
}

pub fn app_request_restart() -> Result<(), String> {
    crate::app_info::request_restart()
}

pub fn app_toggle_devtools() -> bool {
    cfg!(debug_assertions)
}

pub fn app_window_close() -> bool {
    true
}

pub fn app_settings_get(store: &AppSettingsStore) -> AppSettings {
    store.snapshot()
}

pub fn app_settings_set(
    store: &AppSettingsStore,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let next = store.replace(settings)?;
    sync_process_locale_preference(&next);
    Ok(next)
}

pub fn i18n_bundle_get() -> crate::i18n::I18nBundle {
    crate::i18n::i18n_bundle()
}

pub fn performance_snapshot(monitor: &PerformanceMonitor) -> PerformanceSnapshot {
    monitor.snapshot()
}

pub fn project_mark_active(
    service: &RuntimeService,
    project: ProjectSummary,
) -> Result<ProjectActivitySnapshot, String> {
    service.mark_project_active_with_watch(&project.id)
}

pub fn project_select(
    service: &RuntimeService,
    project_id: String,
) -> Result<ProjectListSnapshot, String> {
    service.select_project(&project_id)?;
    Ok(service.project_list())
}

pub fn ssh_profile_upsert(
    service: &RuntimeService,
    request: SSHProfileUpsertRequest,
) -> Result<SSHProfilesSnapshot, String> {
    service.upsert_ssh_profile(request)
}

pub fn ssh_profile_delete(
    service: &RuntimeService,
    profile_id: String,
) -> Result<SSHProfilesSnapshot, String> {
    service.delete_ssh_profile(profile_id)
}

pub fn ssh_profile_test(
    service: &RuntimeService,
    request: SSHProfileUpsertRequest,
    runtime_assets: PathBuf,
) -> Result<SSHProfileTestResult, String> {
    service.test_ssh_profile(request, runtime_assets)
}

pub fn remote_status(service: &RuntimeService) -> RemoteSummary {
    service.reload_remote()
}

pub fn remote_snapshot_emit(service: &RuntimeService) -> RemoteSummary {
    service.reload_remote()
}

pub fn remote_reconnect(service: &RuntimeService) -> Result<RemoteSummary, String> {
    service.reconnect_remote()
}

pub fn remote_devices_refresh(service: &RuntimeService) -> Result<RemoteSummary, String> {
    service.refresh_remote_devices()
}

pub fn remote_device_revoke(
    service: &RuntimeService,
    device_id: String,
) -> Result<RemoteSummary, String> {
    service.revoke_remote_device(&device_id)
}

pub fn remote_pairing_create(service: &RuntimeService) -> Result<RemoteSummary, String> {
    service.create_remote_pairing()
}

pub fn remote_pairing_cancel(
    service: &RuntimeService,
    pairing_id: String,
) -> Result<RemoteSummary, String> {
    service.cancel_remote_pairing(&pairing_id)
}

pub fn remote_pairing_confirm(
    service: &RuntimeService,
    pairing_id: String,
) -> Result<RemoteSummary, String> {
    service.confirm_remote_pairing(&pairing_id)
}

pub fn remote_pairing_reject(
    service: &RuntimeService,
    pairing_id: String,
) -> Result<RemoteSummary, String> {
    service.reject_remote_pairing(&pairing_id)
}

pub fn pet_refresh(
    service: &RuntimeService,
    _request: PetRefreshRequest,
) -> Result<PetSnapshot, String> {
    service.refresh_pet_from_indexed_history()?;
    service.pet_snapshot()
}

pub async fn pet_custom_install_preview(
    service: &RuntimeService,
    request: PetCustomPetInstallRequest,
) -> Result<PetCustomPetInstallPreview, String> {
    service.resolve_custom_pet_install(request).await
}

pub async fn pet_custom_install(
    service: &RuntimeService,
    request: PetCustomPetInstallRequest,
) -> Result<PetCustomPet, String> {
    service.install_custom_pet(request).await
}

pub fn pet_custom_sprite(service: &RuntimeService, pet: PetCustomPet) -> Result<PetCustomPet, String> {
    Ok(service.custom_pet_sprite(pet))
}

pub fn pet_claim(
    service: &RuntimeService,
    request: PetClaimRequest,
) -> Result<PetSnapshot, String> {
    service.claim_pet_from_indexed_history(request)
}

pub fn pet_rename(
    service: &RuntimeService,
    request: PetRenameRequest,
) -> Result<PetSnapshot, String> {
    service.rename_pet(request)
}

pub fn pet_archive_current(service: &RuntimeService) -> Result<PetSnapshot, String> {
    service.archive_current_pet()
}

pub fn pet_restore_archived(
    service: &RuntimeService,
    request: PetRestoreRequest,
) -> Result<PetSnapshot, String> {
    service.restore_archived_pet(request)
}

pub fn worktree_create(
    service: &RuntimeService,
    request: WorktreeCreateRequest,
) -> Result<WorktreeSnapshot, String> {
    service.create_worktree_from_request(request)
}

pub fn worktree_remove(
    service: &RuntimeService,
    request: WorktreeRemoveRequest,
) -> Result<WorktreeSnapshot, String> {
    service.remove_worktree_from_request(request)
}

pub fn worktree_merge(
    service: &RuntimeService,
    request: WorktreeMergeRequest,
) -> Result<WorktreeSnapshot, String> {
    service.merge_worktree_from_request(request)
}

pub fn git_cancel(service: &RuntimeService, project_path: String) -> Result<(), String> {
    service.cancel_project_git(&project_path)
}

pub fn git_refresh_project(service: &RuntimeService, project_path: String) -> GitSummary {
    service.reload_project_git(&project_path)
}

pub fn git_fetch(service: &RuntimeService, project_path: String) -> Result<GitSummary, String> {
    service.fetch_project_git(&project_path)
}

pub fn git_pull(service: &RuntimeService, project_path: String) -> Result<GitSummary, String> {
    service.pull_project_git(&project_path)
}

pub fn git_push(service: &RuntimeService, project_path: String) -> Result<GitSummary, String> {
    service.push_project_git(&project_path)
}

pub fn git_sync(service: &RuntimeService, project_path: String) -> Result<GitSummary, String> {
    service.sync_project_git(&project_path)
}

pub fn git_force_push(
    service: &RuntimeService,
    project_path: String,
) -> Result<GitSummary, String> {
    service.force_push_project_git(&project_path)
}

pub fn git_push_remote(
    service: &RuntimeService,
    request: GitPushRemoteRequest,
) -> Result<GitSummary, String> {
    service.push_project_git_remote(&request.project_path, &request.remote)
}

pub fn git_push_remote_branch(
    service: &RuntimeService,
    request: GitPushRemoteBranchRequest,
) -> Result<GitSummary, String> {
    service.push_project_git_remote_branch(
        &request.project_path,
        &request.remote_branch,
        request.local_branch.as_deref(),
    )
}

pub fn ai_history_project_summary(
    service: &RuntimeService,
    project: AIHistoryProjectRequest,
) -> Result<AIHistoryProjectState, String> {
    service.indexed_project_ai_history_summary(project)
}

pub fn ai_history_refresh_project(
    service: &RuntimeService,
    project: AIHistoryProjectRequest,
) -> Result<(), String> {
    service.refresh_indexed_project_ai_history(project)
}

pub fn ai_history_project_state(
    service: &RuntimeService,
    project: AIHistoryProjectRequest,
) -> Result<AIHistoryProjectState, String> {
    service.indexed_project_ai_history_state(project)
}

pub fn ai_history_global_summary(
    service: &RuntimeService,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<AIGlobalHistorySnapshot, String> {
    service.indexed_global_ai_history_summary(projects)
}

pub fn ai_history_refresh_global(
    service: &RuntimeService,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<(), String> {
    service.refresh_indexed_global_ai_history(projects)
}

pub fn ai_history_global_state(
    service: &RuntimeService,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<Option<AIGlobalHistorySnapshot>, String> {
    service.indexed_global_ai_history_state(projects)
}

pub fn ai_history_global_today_normalized_tokens(service: &RuntimeService) -> Result<i64, String> {
    service.global_today_normalized_ai_tokens()
}

pub fn ai_history_session_rename(
    service: &RuntimeService,
    project: AIHistoryProjectRequest,
    session_id: String,
    title: String,
) -> Result<AIHistoryProjectState, String> {
    service.rename_indexed_ai_session(project, session_id, title)
}

pub fn ai_history_session_remove(
    service: &RuntimeService,
    project: AIHistoryProjectRequest,
    session_id: String,
) -> Result<AIHistoryProjectState, String> {
    service.remove_indexed_ai_session(project, session_id)
}

pub fn memory_extraction_cancel(
    service: &RuntimeService,
) -> Result<MemoryExtractionStatusSnapshot, String> {
    service.cancel_memory_extraction_queue()
}

pub fn memory_archive_entry(service: &RuntimeService, entry_id: String) -> Result<(), String> {
    service.archive_memory_entry(None, &entry_id).map(|_| ())
}

pub fn memory_delete_entry(service: &RuntimeService, entry_id: String) -> Result<(), String> {
    service.delete_memory_entry(None, &entry_id).map(|_| ())
}

pub fn memory_delete_summary(service: &RuntimeService, summary_id: String) -> Result<(), String> {
    service.delete_memory_summary(None, &summary_id).map(|_| ())
}

pub fn memory_delete_project_profile(
    service: &RuntimeService,
    project_id: String,
) -> Result<(), String> {
    service.delete_memory_project_profile(&project_id).map(|_| ())
}

pub fn memory_delete_project(service: &RuntimeService, project_id: String) -> Result<(), String> {
    service.delete_memory_project(&project_id).map(|_| ())
}

pub fn memory_migrate_project(
    service: &RuntimeService,
    request: MemoryProjectMigrationRequest,
) -> Result<(), String> {
    service.migrate_memory_project(request).map(|_| ())
}

pub fn memory_update_summary(
    service: &RuntimeService,
    request: MemorySummaryUpdateRequest,
) -> Result<MemorySummaryRow, String> {
    service.update_memory_summary(request)
}

pub async fn memory_index_now(
    service: &RuntimeService,
) -> Result<MemoryExtractionStatusSnapshot, String> {
    service.process_memory_sessions_now().await
}

pub fn power_set_sleep_prevention(
    manager: &PowerManager,
    mode: String,
) -> Result<bool, String> {
    manager.set_sleep_prevention(mode)
}

pub fn notification_dispatch_channels(
    request: NotificationDispatchRequest,
) -> NotificationDispatchResult {
    crate::notification::dispatch_notification_channels(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::{PetProjectTokenTotal, PetRefreshInput};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn app_command_names_delegate_to_runtime_system_logic() {
        let mut settings = AppSettings::default();
        settings.update.enabled = false;
        let status = app_update_status(&settings, PathBuf::new(), "1.2.3")
            .expect("update status");
        assert_eq!(status.current_version, "1.2.3");
        assert_eq!(status.installation_mode, "disabled");

        let about = app_about_metadata("1.2.3", "com.duxweb.codux.test");
        assert_eq!(about.version, "1.2.3");
        assert_eq!(about.identifier, "com.duxweb.codux.test");

        let manager = PowerManager::default();
        assert!(!power_set_sleep_prevention(&manager, "off".to_string()).expect("power off"));
        assert!(app_window_close());
    }

    #[test]
    fn settings_i18n_and_performance_commands_match_tauri_facade_shape() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-settings-{}",
            Uuid::new_v4()
        ));
        let store = AppSettingsStore::from_support_dir(support_dir.clone());
        let mut settings = app_settings_get(&store);
        settings.language = "en".to_string();
        settings.theme = "dark".to_string();

        let saved = app_settings_set(&store, settings).expect("save settings");
        assert_eq!(saved.language, "en");
        assert_eq!(store.reload_snapshot().theme, "dark");

        let bundle = i18n_bundle_get();
        assert_eq!(bundle.source_language, "en");
        assert!(bundle.locales.iter().any(|locale| locale == "zh-Hans"));
        assert!(bundle.locales.iter().any(|locale| locale == "en"));

        let snapshot = performance_snapshot(&PerformanceMonitor::default());
        assert!(snapshot.cpu_percent >= 0.0);
        assert!(snapshot.memory_bytes >= snapshot.memory.main_bytes);

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn project_commands_select_and_mark_active_project() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-projects-{}",
            Uuid::new_v4()
        ));
        let first = support_dir.join("first");
        let second = support_dir.join("second");
        std::fs::create_dir_all(&first).expect("first project dir");
        std::fs::create_dir_all(&second).expect("second project dir");
        std::fs::write(
            support_dir.join("state.json"),
            serde_json::to_string_pretty(&json!({
                "projects": [
                    {
                        "id": "project-a",
                        "name": "Project A",
                        "path": first.display().to_string()
                    },
                    {
                        "id": "project-b",
                        "name": "Project B",
                        "path": second.display().to_string()
                    }
                ],
                "selectedProjectId": "project-a"
            }))
            .expect("state json"),
        )
        .expect("write state");

        let service = RuntimeService::new(support_dir.clone());
        let selected = project_select(&service, "project-b".to_string()).expect("select project");
        assert_eq!(selected.selected_project_id.as_deref(), Some("project-b"));

        let project = selected
            .projects
            .iter()
            .find(|project| project.id == "project-b")
            .expect("selected project")
            .clone();
        let activity = project_mark_active(&service, project).expect("mark active");
        assert_eq!(activity.active_project_id.as_deref(), Some("project-b"));
        assert!(
            activity
                .tracked_projects
                .iter()
                .any(|project| project.id == "project-b")
        );

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn ssh_profile_commands_upsert_delete_and_test_without_real_connection() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-ssh-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        let service = RuntimeService::new(support_dir.clone());
        let request = SSHProfileUpsertRequest {
            id: Some("profile-1".to_string()),
            name: "Production".to_string(),
            host: "example.com".to_string(),
            port: 2222,
            username: "root".to_string(),
            credential_kind: "password".to_string(),
            private_key_path: None,
            password: Some("secret".to_string()),
            key_passphrase: None,
        };

        let snapshot = ssh_profile_upsert(&service, request.clone()).expect("upsert profile");
        assert_eq!(snapshot.profiles.len(), 1);
        assert_eq!(snapshot.profiles[0].id, "profile-1");
        assert_eq!(snapshot.profiles[0].host, "example.com");

        let test_error = ssh_profile_test(&service, request, support_dir.join("missing-bin"))
            .expect_err("missing wrapper should fail before connecting");
        assert!(test_error.contains("codux-ssh wrapper is not ready"));

        let snapshot =
            ssh_profile_delete(&service, "profile-1".to_string()).expect("delete profile");
        assert!(snapshot.profiles.is_empty());

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn remote_commands_delegate_to_runtime_service_without_network_when_disabled() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-remote-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        std::fs::write(
            support_dir.join("settings.json"),
            serde_json::to_string_pretty(&json!({
                "remote": {
                    "isEnabled": false,
                    "serverURL": "http://relay.example"
                }
            }))
            .expect("settings json"),
        )
        .expect("write settings");

        let service = RuntimeService::new(support_dir.clone());
        let status = remote_status(&service);
        assert!(!status.enabled);
        assert_eq!(status.status, "stopped");

        let emitted = remote_snapshot_emit(&service);
        assert_eq!(emitted.status, "stopped");

        let reconnected = remote_reconnect(&service).expect("disabled reconnect");
        assert!(!reconnected.enabled);

        let refreshed = remote_devices_refresh(&service).expect("disabled refresh");
        assert_eq!(refreshed.devices, 0);

        assert!(
            remote_device_revoke(&service, String::new())
                .expect_err("missing device id")
                .contains("Missing device id")
        );
        assert!(
            remote_pairing_create(&service)
                .expect_err("disabled pairing")
                .contains("Remote Host is not registered")
        );
        assert!(
            remote_pairing_cancel(&service, String::new())
                .expect_err("missing cancel id")
                .contains("Missing pairing id")
        );
        assert!(
            remote_pairing_confirm(&service, String::new())
                .expect_err("missing confirm id")
                .contains("Missing pairing id")
        );
        assert!(
            remote_pairing_reject(&service, String::new())
                .expect_err("missing reject id")
                .contains("Missing pairing id")
        );

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn pet_commands_delegate_to_runtime_pet_store() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-pet-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        let service = RuntimeService::new(support_dir.clone());

        if service
            .pet_snapshot()
            .map(|snapshot| snapshot.claimed_at.is_none())
            .unwrap_or(true)
        {
            service
                .claim_pet(crate::pet::PetClaimInput {
                    species: "dragon".to_string(),
                    custom_name: " Spark ".to_string(),
                    custom_pet: None,
                    project_totals: vec![PetProjectTokenTotal {
                        project_id: "project-a".to_string(),
                        total_tokens: 100,
                    }],
                    fallback_total_tokens: 100,
                })
                .expect("seed claimed pet");
        }

        let renamed = pet_rename(
            &service,
            PetRenameRequest {
                custom_name: " Ember ".to_string(),
            },
        )
        .expect("rename pet");
        assert_eq!(renamed.custom_name, "Ember");

        assert!(
            pet_restore_archived(
                &service,
                PetRestoreRequest {
                    legacy_id: "missing".to_string(),
                },
            )
            .expect_err("missing archived pet")
            .contains("Archived pet not found")
        );

        let refreshed = service
            .refresh_pet(PetRefreshInput {
                project_totals: vec![PetProjectTokenTotal {
                    project_id: "project-a".to_string(),
                    total_tokens: 250,
                }],
                fallback_total_tokens: 250,
                computed_stats: Default::default(),
            })
            .expect("refresh pet directly");
        assert!(refreshed.updated_at > 0);

        let archived = pet_archive_current(&service).expect("archive pet");
        assert!(archived.claimed_at.is_none());
        assert!(!archived.legacy.is_empty());

        let custom = pet_custom_sprite(
            &service,
            PetCustomPet {
                id: "demo".to_string(),
                display_name: "Demo".to_string(),
                description: String::new(),
                spritesheet_path: "sprite.png".to_string(),
                directory_name: "demo".to_string(),
                spritesheet_data_url: None,
                source_page_url: None,
                source_zip_url: None,
                installed_at: None,
            },
        )
        .expect("custom sprite hydrate");
        assert_eq!(custom.id, "demo");

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn worktree_commands_delegate_to_runtime_validation() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-worktree-{}",
            Uuid::new_v4()
        ));
        let project_dir = support_dir.join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let service = RuntimeService::new(support_dir.clone());
        let project_path = project_dir.display().to_string();

        let create_error = worktree_create(
            &service,
            WorktreeCreateRequest {
                project_id: "project".to_string(),
                project_path: project_path.clone(),
                base_branch: None,
                branch_name: "feature/demo".to_string(),
                task_title: Some("Demo".to_string()),
            },
        )
        .expect_err("non git create should fail");
        assert!(create_error.contains("Not a Git repository"));

        let remove_error = worktree_remove(
            &service,
            WorktreeRemoveRequest {
                project_id: "project".to_string(),
                project_path: project_path.clone(),
                worktree_path: project_path.clone(),
                remove_branch: false,
            },
        )
        .expect_err("non git remove should fail");
        assert!(remove_error.contains("Not a Git repository"));

        let merge_error = worktree_merge(
            &service,
            WorktreeMergeRequest {
                project_id: "project".to_string(),
                project_path,
                worktree_path: project_dir.display().to_string(),
                base_branch: None,
                remove_branch: Some(false),
            },
        )
        .expect_err("non git merge should fail");
        assert!(merge_error.contains("Not a Git repository"));

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn git_remote_commands_delegate_to_runtime_git2_layer() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-git-{}",
            Uuid::new_v4()
        ));
        let project_dir = support_dir.join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let service = RuntimeService::new(support_dir.clone());
        let project_path = project_dir.display().to_string();

        git_cancel(&service, project_path.clone()).expect("cancel without token is ok");
        let snapshot = git_refresh_project(&service, project_path.clone());
        assert!(!snapshot.is_repository);

        for result in [
            git_fetch(&service, project_path.clone()),
            git_pull(&service, project_path.clone()),
            git_push(&service, project_path.clone()),
            git_sync(&service, project_path.clone()),
            git_force_push(&service, project_path.clone()),
            git_push_remote(
                &service,
                GitPushRemoteRequest {
                    project_path: project_path.clone(),
                    remote: "origin".to_string(),
                },
            ),
            git_push_remote_branch(
                &service,
                GitPushRemoteBranchRequest {
                    project_path: project_path.clone(),
                    remote_branch: "origin/main".to_string(),
                    local_branch: Some("main".to_string()),
                },
            ),
        ] {
            assert!(result.is_err());
        }

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn ai_history_commands_delegate_to_indexed_runtime_layer() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-ai-history-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        let service = RuntimeService::new(support_dir.clone());
        let project = AIHistoryProjectRequest {
            id: "project-a".to_string(),
            name: "Project A".to_string(),
            path: String::new(),
        };

        let state = ai_history_project_state(&service, project.clone()).expect("project state");
        assert_eq!(state.project_id, "project-a");
        assert_eq!(state.detail, "idle");

        let summary =
            ai_history_project_summary(&service, project.clone()).expect("project summary");
        assert_eq!(summary.project_id, "project-a");
        assert!(summary.is_loading);

        ai_history_refresh_project(&service, project.clone()).expect("refresh project");

        let global_state =
            ai_history_global_state(&service, vec![project.clone()]).expect("global state");
        assert!(global_state.is_some());

        let global_summary =
            ai_history_global_summary(&service, Vec::new()).expect("global summary");
        assert_eq!(global_summary.project_count, 0);

        ai_history_refresh_global(&service, Vec::new()).expect("refresh global");
        assert!(ai_history_global_today_normalized_tokens(&service).expect("today tokens") >= 0);

        assert!(
            ai_history_session_rename(
                &service,
                project.clone(),
                "missing-session".to_string(),
                "Renamed".to_string(),
            )
            .expect_err("missing session rename")
            .contains("Matching session record was not found")
        );
        assert!(
            ai_history_session_remove(&service, project, "missing-session".to_string())
                .expect_err("missing session remove")
                .contains("Matching session record was not found")
        );

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn memory_commands_delegate_to_runtime_memory_store() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-memory-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        let service = RuntimeService::new(support_dir.clone());

        let canceled = memory_extraction_cancel(&service).expect("cancel queue");
        assert_eq!(canceled.pending_count, 0);

        assert!(
            memory_archive_entry(&service, String::new())
                .expect_err("empty archive id")
                .contains("Memory entry id is empty")
        );
        assert!(
            memory_delete_entry(&service, String::new())
                .expect_err("empty delete id")
                .contains("Memory entry id is empty")
        );
        assert!(
            memory_delete_summary(&service, String::new())
                .expect_err("empty summary id")
                .contains("Memory summary id is empty")
        );
        assert!(
            memory_delete_project_profile(&service, String::new())
                .expect_err("empty project profile id")
                .contains("Project id is empty")
        );
        assert!(
            memory_delete_project(&service, String::new())
                .expect_err("empty project id")
                .contains("Project id is empty")
        );
        assert!(
            memory_migrate_project(
                &service,
                MemoryProjectMigrationRequest {
                    from_project_id: String::new(),
                    to_project_id: "project-b".to_string(),
                    overwrite: false,
                },
            )
            .expect_err("empty migrate project id")
            .contains("project id cannot be empty")
        );
        assert!(
            memory_update_summary(
                &service,
                MemorySummaryUpdateRequest {
                    summary_id: String::new(),
                    content: String::new(),
                    max_versions: None,
                },
            )
            .expect_err("empty summary content")
            .contains("summary content cannot be empty")
        );

        let indexed =
            crate::async_runtime::block_on(memory_index_now(&service)).expect("memory index now");
        assert_eq!(indexed.running_count, 0);

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn diagnostics_export_command_writes_redacted_runtime_report() {
        let destination = std::env::temp_dir().join(format!(
            "codux-app-command-diagnostics-{}.json",
            Uuid::new_v4()
        ));
        let request = DiagnosticsExportRequest {
            destination_path: destination.display().to_string(),
        };
        let about = app_about_metadata("1.0.0", "com.duxweb.codux.test");
        let update = UpdateStatus {
            current_version: "1.0.0".to_string(),
            installation_mode: "disabled".to_string(),
            message: "disabled".to_string(),
            ..Default::default()
        };
        let result = diagnostics_export(
            request,
            about,
            update,
            AppDiagnosticsSnapshot {
                settings: json!({
                    "ai": {
                        "providers": [
                            {
                                "apiKey": "secret",
                                "api_key": "secret",
                                "token": "secret"
                            }
                        ]
                    }
                }),
                projects: json!([]),
                ai_runtime: json!({}),
                ai_state: json!({}),
                performance: json!({}),
                ssh: json!({
                    "profiles": [
                        {
                            "password": "secret",
                            "privateKeyPath": "/tmp/key"
                        }
                    ]
                }),
            },
        )
        .expect("export diagnostics");

        assert!(result.bytes > 0);
        let content = std::fs::read_to_string(&destination).expect("diagnostics file");
        assert!(content.contains("\"apiKey\": \"******\""));
        assert!(content.contains("\"api_key\": \"******\""));
        assert!(content.contains("\"password\": \"******\""));
        assert!(content.contains("\"privateKeyPath\": \"******\""));
        let _ = std::fs::remove_file(destination);
    }
}
