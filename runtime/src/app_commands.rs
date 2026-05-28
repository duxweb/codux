use crate::{
    app_info::{
        AppAboutMetadata, AppDiagnosticsSnapshot, DiagnosticsExportRequest, DiagnosticsExportResult,
        UpdateInstallResult, UpdateStatus,
    },
    notification::{NotificationDispatchRequest, NotificationDispatchResult},
    performance::{PerformanceMonitor, PerformanceSnapshot},
    power::PowerManager,
    project_activity::ProjectActivitySnapshot,
    project_store::{ProjectListSnapshot, ProjectSummary},
    remote::RemoteSummary,
    runtime_state::RuntimeService,
    settings::{AppSettings, AppSettingsStore, sync_process_locale_preference},
    ssh::{SSHProfileTestResult, SSHProfileUpsertRequest, SSHProfilesSnapshot},
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
