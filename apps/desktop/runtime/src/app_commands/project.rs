use super::*;

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
pub fn project_list(service: &RuntimeService) -> ProjectListSnapshot {
    service.project_list()
}
pub fn project_create(
    service: &RuntimeService,
    request: ProjectCreateRequest,
) -> Result<ProjectListSnapshot, String> {
    service.project_create(request)
}
pub fn project_update(
    service: &RuntimeService,
    request: ProjectUpdateRequest,
) -> Result<ProjectListSnapshot, String> {
    service.project_update(request)
}
pub fn project_reorder(
    service: &RuntimeService,
    request: ProjectReorderRequest,
) -> Result<ProjectListSnapshot, String> {
    service.project_reorder(request)
}
pub fn project_close(
    service: &RuntimeService,
    request: ProjectCloseRequest,
) -> Result<ProjectListSnapshot, String> {
    service.project_close(request)
}
pub fn project_select_worktree(
    service: &RuntimeService,
    request: ProjectSelectWorktreeRequest,
) -> Result<(), String> {
    service.project_select_worktree(request)
}
pub fn project_set_default_push_remote(
    service: &RuntimeService,
    request: ProjectDefaultPushRemoteRequest,
) -> Result<ProjectListSnapshot, String> {
    service.project_set_default_push_remote(request)
}
pub fn project_open_applications(service: &RuntimeService) -> Vec<ProjectOpenApplicationSummary> {
    service.project_open_applications()
}
pub fn project_open_in_application(
    service: &RuntimeService,
    request: ProjectOpenApplicationRequest,
) -> Result<(), String> {
    service.project_open_in_application(request.project_path, request.application_id)
}
pub fn project_reveal_in_file_manager(
    service: &RuntimeService,
    project_path: String,
) -> Result<(), String> {
    service.project_reveal_in_file_manager(&project_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_store::ProjectRuntimeTarget;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn project_commands_select_and_mark_active_project() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-projects-{}", Uuid::new_v4()));
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
    fn project_management_commands_match_tauri_facade_shape() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-project-management-{}",
            Uuid::new_v4()
        ));
        let first = support_dir.join("first");
        let second = support_dir.join("second");
        std::fs::create_dir_all(&first).expect("first project dir");
        std::fs::create_dir_all(&second).expect("second project dir");
        let service = RuntimeService::new(support_dir.clone());

        let created = project_create(
            &service,
            ProjectCreateRequest {
                name: "First".to_string(),
                path: first.display().to_string(),
                badge_text: None,
                badge_symbol: Some("folder".to_string()),
                badge_color_hex: Some("#2F80ED".to_string()),
                runtime_target: ProjectRuntimeTarget::Local,
            },
        )
        .expect("create project");
        assert_eq!(created.projects.len(), 1);
        let first_id = created.projects[0].id.clone();

        let created = project_create(
            &service,
            ProjectCreateRequest {
                name: "Second".to_string(),
                path: second.display().to_string(),
                badge_text: None,
                badge_symbol: None,
                badge_color_hex: None,
                runtime_target: ProjectRuntimeTarget::Local,
            },
        )
        .expect("create second project");
        assert_eq!(created.projects.len(), 2);
        let second_id = created.projects[1].id.clone();

        let listed = project_list(&service);
        assert_eq!(listed.projects.len(), 2);

        let updated = project_update(
            &service,
            ProjectUpdateRequest {
                project_id: first_id.clone(),
                name: "First Renamed".to_string(),
                path: first.display().to_string(),
                badge_text: None,
                badge_symbol: Some("book".to_string()),
                badge_color_hex: Some("#78D891".to_string()),
                runtime_target: ProjectRuntimeTarget::Local,
            },
        )
        .expect("update project");
        assert!(
            updated
                .projects
                .iter()
                .any(|project| project.id == first_id && project.name == "First Renamed")
        );

        let reordered = project_reorder(
            &service,
            ProjectReorderRequest {
                project_ids: vec![second_id.clone(), first_id.clone()],
            },
        )
        .expect("reorder projects");
        assert_eq!(reordered.projects[0].id, second_id);

        let remotes = project_set_default_push_remote(
            &service,
            ProjectDefaultPushRemoteRequest {
                project_id: first_id.clone(),
                remote_name: Some("origin/main".to_string()),
            },
        )
        .expect("set default remote");
        assert_eq!(
            remotes
                .projects
                .iter()
                .find(|project| project.id == first_id)
                .and_then(|project| project.git_default_push_remote_name.as_deref()),
            Some("origin/main")
        );

        let applications = project_open_applications(&service);
        assert!(applications.iter().any(|app| app.id == "vscode"));

        let closed = project_close(
            &service,
            ProjectCloseRequest {
                project_id: first_id.clone(),
            },
        )
        .expect("close project");
        assert!(!closed.projects.iter().any(|project| project.id == first_id));

        let _ = std::fs::remove_dir_all(support_dir);
    }
}
