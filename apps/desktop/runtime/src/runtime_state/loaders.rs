fn load_projects(support_dir: &Path) -> (Vec<ProjectInfo>, Option<ProjectInfo>) {
    let state = serde_json::from_value::<StateFile>(Value::Object(
        crate::config::ConfigStore::for_support_dir(support_dir).snapshot(),
    ))
    .ok();

    let Some(state) = state else {
        return (Vec::new(), None);
    };

    let projects = state
        .projects
        .into_iter()
        .map(|project| {
            let runtime_target = project.resolved_runtime_target();
            ProjectInfo {
            exists: runtime_target.is_hosted() || Path::new(&project.path).exists(),
            id: project.id,
            badge: project
                .badge_text
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| crate::project_store::badge_from_name(&project.name)),
            badge_symbol: project.badge_symbol,
            badge_color_hex: project.badge_color_hex,
            name: project.name,
            path: project.path,
            git_default_push_remote_name: project.git_default_push_remote_name,
            environment_variables: project.environment_variables,
            runtime_target,
        }
        })
        .collect::<Vec<_>>();

    let selected_project = state
        .selected_project_id
        .and_then(|id| projects.iter().find(|project| project.id == id).cloned())
        .or_else(|| projects.first().cloned());

    (projects, selected_project)
}

fn load_settings(support_dir: &Path) -> SettingsSummary {
    let store = AppSettingsStore::from_support_dir(support_dir.to_path_buf());
    let settings = store.snapshot();
    sync_process_locale_preference(&settings);
    SettingsService::new(support_dir.to_path_buf()).summary()
}

fn load_git_workspace(support_dir: &Path, project_path: &str) -> git::GitWorkspaceSnapshot {
    crate::runtime_cache::cached_git_workspace(support_dir, project_path).unwrap_or_else(|| {
        let snapshot = git::GitService::workspace_snapshot(project_path);
        crate::runtime_cache::save_git_workspace(support_dir, project_path, &snapshot);
        snapshot
    })
}

fn refresh_git_summary(support_dir: &Path, project_path: &str) -> git::GitSummary {
    let snapshot = git::GitService::workspace_snapshot(project_path);
    let summary = snapshot.status.clone();
    crate::runtime_cache::save_git_workspace(support_dir, project_path, &snapshot);
    summary
}

fn refresh_git_review(
    support_dir: &Path,
    project_path: &str,
    base_branch: Option<&str>,
) -> git::GitReviewSummary {
    if base_branch.is_none() {
        let snapshot = git::GitService::workspace_snapshot(project_path);
        let review = snapshot.review.clone();
        crate::runtime_cache::save_git_workspace(support_dir, project_path, &snapshot);
        return review;
    }
    let review = git::GitService::review(project_path, base_branch);
    crate::runtime_cache::save_git_review(support_dir, project_path, base_branch, &review);
    review
}

fn load_file_entries(project_path: &str, directory_path: Option<&str>) -> Vec<FileEntry> {
    try_load_file_entries(project_path, directory_path).unwrap_or_default()
}

pub(super) fn try_load_file_entries(
    project_path: &str,
    directory_path: Option<&str>,
) -> Result<Vec<FileEntry>, String> {
    FilesService::list_children(project_path, directory_path).map(|mut entries| {
        entries.truncate(80);
        entries
            .into_iter()
            .map(|entry| FileEntry {
                name: entry.name,
                relative_path: entry.relative_path,
                size: entry.size,
                kind: match entry.kind {
                    crate::files::FileKind::Directory => FileKind::Directory,
                    crate::files::FileKind::File | crate::files::FileKind::Symlink => {
                        FileKind::File
                    }
                },
            })
            .collect()
    })
}

fn load_ai_history(support_dir: &Path, project_path: &str) -> AIHistorySummary {
    AIHistoryService::new(support_dir.to_path_buf()).project_summary(project_path)
}

fn load_global_ai_history(support_dir: &Path) -> AIGlobalHistorySummary {
    match load_indexed_global_history_at(
        support_dir.join("ai-usage.sqlite3"),
        ai_history_workspace_requests_from_support_dir(support_dir),
    ) {
        Ok(Some(snapshot)) => crate::ai_history::global_summary_from_normalized_snapshot(snapshot),
        Ok(None) => AIGlobalHistorySummary::default(),
        Err(error) => AIGlobalHistorySummary {
            error: Some(error.to_string()),
            ..Default::default()
        },
    }
}

fn load_ai_session_detail(
    support_dir: &Path,
    project_path: &str,
    session_id: &str,
) -> AISessionDetail {
    AIHistoryService::new(support_dir.to_path_buf())
        .project_session_detail(project_path, session_id)
        .unwrap_or_else(|error| AISessionDetail {
            id: session_id.to_string(),
            error: Some(error),
            ..Default::default()
        })
}

fn load_memory(support_dir: &Path, project_id: Option<&str>) -> MemorySummary {
    MemoryService::new(support_dir.to_path_buf()).summary(project_id)
}

fn load_memory_manager(
    support_dir: &Path,
    projects: &[ProjectInfo],
    scope: &str,
    project_id: Option<&str>,
    tab: &str,
) -> MemoryManagerSnapshot {
    MemoryService::new(support_dir.to_path_buf()).manager_snapshot(
        &crate::memory::memory_project_infos(projects),
        scope,
        project_id,
        tab,
        500,
    )
}

fn load_notifications(support_dir: &Path) -> NotificationSummary {
    NotificationService::new(support_dir.to_path_buf()).summary()
}

fn load_ssh(support_dir: &Path, runtime_assets: PathBuf) -> SSHSummary {
    SSHService::new(support_dir.to_path_buf(), runtime_assets).summary()
}

fn load_db(
    support_dir: &Path,
    runtime_assets: PathBuf,
    project_id: Option<&str>,
) -> DBSummary {
    DBService::new(
        support_dir.to_path_buf(),
        runtime_assets,
        project_id.map(str::to_string),
    )
    .summary()
}

fn load_terminal_layout(support_dir: &Path, project_id: Option<&str>) -> TerminalLayoutSummary {
    TerminalLayoutService::new(support_dir.to_path_buf()).load(project_id)
}

fn load_worktrees(
    support_dir: &Path,
    project_id: Option<&str>,
    project_path: Option<&str>,
) -> WorktreeSummary {
    WorktreeService::new(support_dir.to_path_buf()).summary(project_id, project_path)
}

fn load_worktrees_from_state(
    support_dir: &Path,
    project_id: Option<&str>,
    project_path: Option<&str>,
    hosted_paths: bool,
) -> WorktreeSummary {
    let service = WorktreeService::new(support_dir.to_path_buf());
    if hosted_paths {
        service.hosted_state_summary(project_id, project_path)
    } else {
        service.state_summary(project_id, project_path)
    }
}

fn load_update(support_dir: &Path, repo_root: PathBuf) -> UpdateSummary {
    UpdateService::new(support_dir.to_path_buf(), repo_root).summary()
}

fn load_runtime_activity(support_dir: &Path) -> RuntimeActivitySummary {
    RuntimeActivityService::new(support_dir.to_path_buf()).summary()
}

fn load_runtime_events() -> RuntimeEventSummary {
    RuntimeEventService::new().summary()
}

fn load_ai_runtime_state(
    support_dir: &Path,
    _runtime_events: &RuntimeEventSummary,
) -> AIRuntimeStateSummary {
    AIRuntimeStateService::new(support_dir).summary()
}

fn load_remote(support_dir: &Path) -> RemoteSummary {
    RemoteService::new(support_dir.to_path_buf()).summary()
}

fn load_pet(support_dir: &Path) -> PetSummary {
    PetService::new(support_dir.to_path_buf()).summary()
}

fn load_performance() -> PerformanceSummary {
    PerformanceService::summary()
}

fn load_tool_permissions(support_dir: &Path) -> ToolPermissionsSummary {
    ToolPermissionsService::new(support_dir.to_path_buf()).sync()
}

fn read_json_or_default(path: PathBuf) -> Value {
    Value::Object(crate::config::ConfigStore::for_file(path).snapshot())
}

fn app_support_dir() -> PathBuf {
    runtime_paths::app_support_dir()
}
