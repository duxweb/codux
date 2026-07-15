use super::*;

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
) -> Result<AIGlobalHistorySnapshot, String> {
    service.indexed_global_ai_history_summary()
}
pub fn ai_history_refresh_global(service: &RuntimeService) -> Result<(), String> {
    service.refresh_indexed_global_ai_history()
}
pub fn ai_history_global_state(
    service: &RuntimeService,
) -> Result<Option<AIGlobalHistorySnapshot>, String> {
    service.indexed_global_ai_history_state()
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn ai_history_commands_delegate_to_indexed_runtime_layer() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-ai-history-{}", Uuid::new_v4()));
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
        assert!(!summary.is_loading);

        ai_history_refresh_project(&service, project.clone()).expect("refresh project");

        let global_state = ai_history_global_state(&service).expect("global state");
        assert!(global_state.is_some());

        let global_summary = ai_history_global_summary(&service).expect("global summary");
        assert_eq!(global_summary.project_count, 0);

        ai_history_refresh_global(&service).expect("refresh global");

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
}
