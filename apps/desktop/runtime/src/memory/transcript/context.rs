pub(super) fn memory_project_context(
    projects: &[ProjectWorkspaceRecord],
    session: &AISessionSnapshot,
) -> Option<MemoryProjectContext> {
    projects
        .iter()
        .find(|project| project.id == session.project_id || project.root_project_id == session.project_id)
        .or_else(|| {
            session.project_path.as_ref().and_then(|path| {
                projects.iter().find(|project| {
                    paths_equivalent(Some(project.workspace_path.as_str()), path)
                        || paths_equivalent(Some(project.root_project_path.as_str()), path)
                })
            })
        })
        .map(|project| MemoryProjectContext {
            project_id: project.root_project_id.clone(),
            project_name: project.root_project_name.clone(),
            workspace_path: project.workspace_path.clone(),
        })
}

pub(super) fn memory_project_context_for_task(
    projects: &[ProjectWorkspaceRecord],
    task: &MemoryExtractionTask,
) -> Option<MemoryProjectContext> {
    projects
        .iter()
        .find(|project| {
            project.id == task.project_id
                || project.root_project_id == task.project_id
                || task
                    .workspace_path
                    .as_deref()
                    .and_then(|path| normalized_string(Some(path)))
                    .map(|path| paths_equivalent(Some(project.workspace_path.as_str()), &path))
                    .unwrap_or(false)
        })
        .map(|project| MemoryProjectContext {
            project_id: project.root_project_id.clone(),
            project_name: project.root_project_name.clone(),
            workspace_path: task
                .workspace_path
                .as_deref()
                .and_then(|value| normalized_string(Some(value)))
                .unwrap_or_else(|| project.workspace_path.clone()),
        })
        .or_else(|| {
            task.workspace_path
                .as_deref()
                .and_then(|value| normalized_string(Some(value)))
                .map(|workspace_path| MemoryProjectContext {
                    project_id: task.project_id.clone(),
                    project_name: task.project_id.clone(),
                    workspace_path,
                })
        })
}
