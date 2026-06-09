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
        .map(|project| ProjectInfo {
            exists: Path::new(&project.path).exists(),
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
        })
        .collect::<Vec<_>>();

    let selected_project = state
        .selected_project_id
        .and_then(|id| projects.iter().find(|project| project.id == id).cloned())
        .or_else(|| projects.first().cloned());

    (projects, selected_project)
}

fn load_settings(support_dir: &Path) -> SettingsSummary {
    SettingsService::new(support_dir.to_path_buf()).summary()
}

fn load_git_summary(support_dir: &Path, project_path: &str) -> git::GitSummary {
    crate::runtime_cache::cached_git_summary(support_dir, project_path).unwrap_or_else(|| {
        let summary = git::GitService::status(project_path);
        crate::runtime_cache::save_git_summary(support_dir, project_path, &summary);
        summary
    })
}

fn load_git_review(
    support_dir: &Path,
    project_path: &str,
    base_branch: Option<&str>,
) -> git::GitReviewSummary {
    crate::runtime_cache::cached_git_review(support_dir, project_path, base_branch).unwrap_or_else(
        || {
            let review = git::GitService::review(project_path, base_branch);
            crate::runtime_cache::save_git_review(support_dir, project_path, base_branch, &review);
            review
        },
    )
}

fn refresh_git_summary(support_dir: &Path, project_path: &str) -> git::GitSummary {
    let summary = git::GitService::status(project_path);
    crate::runtime_cache::save_git_summary(support_dir, project_path, &summary);
    summary
}

fn refresh_git_review(
    support_dir: &Path,
    project_path: &str,
    base_branch: Option<&str>,
) -> git::GitReviewSummary {
    let review = git::GitService::review(project_path, base_branch);
    crate::runtime_cache::save_git_review(support_dir, project_path, base_branch, &review);
    review
}

fn load_file_entries(project_path: &str, directory_path: Option<&str>) -> Vec<FileEntry> {
    FilesService::list_children(project_path, directory_path)
        .map(|mut entries| {
            entries.truncate(80);
            entries
        })
        .unwrap_or_default()
        .into_iter()
        .map(|entry| FileEntry {
            name: entry.name,
            relative_path: entry.relative_path,
            size: entry.size,
            kind: match entry.kind {
                crate::files::FileKind::Directory => FileKind::Directory,
                crate::files::FileKind::File | crate::files::FileKind::Symlink => FileKind::File,
            },
        })
        .collect()
}

fn load_ai_history(support_dir: &Path, project_path: &str) -> AIHistorySummary {
    AIHistoryService::new(support_dir.to_path_buf()).project_summary(project_path)
}

fn load_global_ai_history(support_dir: &Path) -> AIGlobalHistorySummary {
    let mut summary = AIHistoryService::new(support_dir.to_path_buf()).global_summary();
    summary.today_total_tokens =
        normalized_global_today_tokens(support_dir).unwrap_or(summary.today_total_tokens);
    summary
}

fn normalized_global_today_tokens(support_dir: &Path) -> Result<i64, String> {
    crate::ai_history_normalized::global_today_normalized_tokens_at(
        support_dir.join("ai-usage.sqlite3"),
    )
    .map(|tokens| tokens.max(0))
    .map_err(|error| error.to_string())
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
    MemoryService::new(support_dir.to_path_buf())
        .manager_snapshot(projects, scope, project_id, tab, 500)
}

fn load_notifications(support_dir: &Path) -> NotificationSummary {
    NotificationService::new(support_dir.to_path_buf()).summary()
}

fn load_ssh(support_dir: &Path, runtime_assets: PathBuf) -> SSHSummary {
    SSHService::new(support_dir.to_path_buf(), runtime_assets).summary()
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
) -> WorktreeSummary {
    WorktreeService::new(support_dir.to_path_buf()).state_summary(project_id, project_path)
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
    AIRuntimeStateService::new(support_dir.to_path_buf()).summary()
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
    ToolPermissionsService::new(support_dir.to_path_buf()).summary()
}

fn read_json_or_default(path: PathBuf) -> Value {
    Value::Object(crate::config::ConfigStore::for_file(path).snapshot())
}

fn app_support_dir() -> PathBuf {
    runtime_paths::app_support_dir()
}
