impl WorktreeService {
    pub fn summary(&self, project_id: Option<&str>, project_path: Option<&str>) -> WorktreeSummary {
        let Some(project_id) = project_id else {
            return WorktreeSummary {
                error: Some("no selected project".to_string()),
                ..Default::default()
            };
        };

        let state = load_worktree_state(&self.state_file);

        let mut worktrees =
            state_worktree_rows(&state.worktrees, project_id, true, &self.support_dir);

        if worktrees.is_empty()
            && let Some(project_path) = project_path
        {
            worktrees.push(default_project_worktree(project_id, project_path, true));
        }
        persist_worktree_git_summaries(&self.support_dir, &worktrees);

        let selected_worktree_id = selected_worktree_id_for_project(
            &state.selected_worktree_id_by_project,
            project_id,
            &worktrees,
        );

        let tasks = task_rows_for_worktrees(&state.worktree_tasks, &worktrees);

        let active_path = selected_worktree_id
            .as_ref()
            .and_then(|id| worktrees.iter().find(|worktree| &worktree.id == id))
            .map(|worktree| worktree.path.clone())
            .or_else(|| project_path.map(str::to_string))
            .unwrap_or_default();

        WorktreeSummary {
            available: true,
            selected_worktree_id,
            worktrees,
            tasks,
            active_git: GitService::status(&active_path),
            error: None,
        }
    }

    pub fn state_summary(
        &self,
        project_id: Option<&str>,
        project_path: Option<&str>,
    ) -> WorktreeSummary {
        let Some(project_id) = project_id else {
            return WorktreeSummary {
                error: Some("no selected project".to_string()),
                ..Default::default()
            };
        };

        let state = load_worktree_state(&self.state_file);
        self.state_summary_from_state(&state, Some(project_id), project_path)
    }

    pub fn state_summaries<'a, I>(
        &self,
        projects: I,
    ) -> std::collections::HashMap<String, WorktreeSummary>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let state = load_worktree_state(&self.state_file);
        projects
            .into_iter()
            .map(|(project_id, project_path)| {
                (
                    project_id.to_string(),
                    self.state_summary_from_state(&state, Some(project_id), Some(project_path)),
                )
            })
            .collect()
    }

    fn state_summary_from_state(
        &self,
        state: &StateFile,
        project_id: Option<&str>,
        project_path: Option<&str>,
    ) -> WorktreeSummary {
        let Some(project_id) = project_id else {
            return WorktreeSummary {
                error: Some("no selected project".to_string()),
                ..Default::default()
            };
        };

        let mut worktrees =
            state_worktree_rows(&state.worktrees, project_id, false, &self.support_dir);

        if worktrees.is_empty()
            && let Some(project_path) = project_path
        {
            worktrees.push(default_project_worktree(project_id, project_path, false));
        }

        let selected_worktree_id = selected_worktree_id_for_project(
            &state.selected_worktree_id_by_project,
            project_id,
            &worktrees,
        );

        let tasks = task_rows_for_worktrees(&state.worktree_tasks, &worktrees);
        let active_git = selected_worktree_id
            .as_ref()
            .and_then(|id| worktrees.iter().find(|worktree| &worktree.id == id))
            .and_then(|worktree| {
                crate::runtime_cache::cached_git_summary(&self.support_dir, &worktree.path)
            })
            .unwrap_or_default();

        WorktreeSummary {
            available: true,
            selected_worktree_id,
            worktrees,
            tasks,
            active_git,
            error: None,
        }
    }
}

fn state_worktree_rows(
    records: &[WorktreeRecord],
    project_id: &str,
    refresh_git: bool,
    support_dir: &Path,
) -> Vec<WorktreeInfo> {
    records
        .iter()
        .filter(|worktree| worktree.project_id == project_id)
        .map(|worktree| {
            let git_summary = if refresh_git {
                project_worktree_git_summary(&worktree.path)
            } else {
                worktree_git_summary_from_cache(support_dir, &worktree.id).unwrap_or_default()
            };
            WorktreeInfo {
                exists: Path::new(&worktree.path).exists(),
                git_summary,
                id: worktree.id.clone(),
                project_id: worktree.project_id.clone(),
                name: worktree.name.clone(),
                branch: worktree.branch.clone(),
                path: worktree.path.clone(),
                status: worktree.status.clone(),
                is_default: worktree.is_default,
            }
        })
        .collect()
}

fn selected_worktree_id_for_project(
    selected_by_project: &std::collections::HashMap<String, String>,
    project_id: &str,
    worktrees: &[WorktreeInfo],
) -> Option<String> {
    selected_by_project
        .get(project_id)
        .cloned()
        .filter(|id| worktrees.iter().any(|worktree| &worktree.id == id))
        .or_else(|| {
            worktrees
                .iter()
                .find(|worktree| worktree.is_default)
                .or_else(|| worktrees.first())
                .map(|worktree| worktree.id.clone())
        })
}

fn task_rows_for_worktrees(
    tasks: &[WorktreeTaskRecord],
    worktrees: &[WorktreeInfo],
) -> Vec<WorktreeTaskInfo> {
    tasks
        .iter()
        .filter(|task| {
            worktrees
                .iter()
                .any(|worktree| worktree.id == task.worktree_id)
        })
        .map(|task| WorktreeTaskInfo {
            worktree_id: task.worktree_id.clone(),
            title: task.title.clone(),
            base_branch: task.base_branch.clone(),
            status: task.status.clone(),
        })
        .collect()
}

fn persist_worktree_git_summaries(support_dir: &Path, worktrees: &[WorktreeInfo]) {
    if worktrees.is_empty() {
        return;
    }
    let Ok(cache) =
        crate::persistent_cache::PersistentCacheStore::for_support_dir(support_dir.to_path_buf())
    else {
        return;
    };
    for worktree in worktrees {
        let _ = cache.put_json_debounced(
            WORKTREE_GIT_SUMMARY_NAMESPACE,
            &worktree.id,
            &worktree.git_summary,
        );
    }
}

fn worktree_git_summary_from_cache(
    support_dir: &Path,
    worktree_id: &str,
) -> Option<ProjectWorktreeGitSummary> {
    crate::persistent_cache::PersistentCacheStore::for_support_dir(support_dir.to_path_buf())
        .ok()?
        .get_json::<ProjectWorktreeGitSummary>(WORKTREE_GIT_SUMMARY_NAMESPACE, worktree_id)
        .ok()
        .flatten()
}

fn load_worktree_state(state_file: &Path) -> StateFile {
    serde_json::from_value::<StateFile>(Value::Object(raw_snapshot(state_file)))
        .unwrap_or_else(|_| StateFile::default())
}

fn default_project_worktree(
    project_id: &str,
    project_path: &str,
    include_git_stats: bool,
) -> WorktreeInfo {
    WorktreeInfo {
        git_summary: if include_git_stats {
            project_worktree_git_summary(project_path)
        } else {
            ProjectWorktreeGitSummary::default()
        },
        id: project_id.to_string(),
        project_id: project_id.to_string(),
        name: "main".to_string(),
        branch: if include_git_stats {
            current_branch(project_path).unwrap_or_else(|| "main".to_string())
        } else {
            "main".to_string()
        },
        path: project_path.to_string(),
        status: "todo".to_string(),
        is_default: true,
        exists: Path::new(project_path).exists(),
    }
}
