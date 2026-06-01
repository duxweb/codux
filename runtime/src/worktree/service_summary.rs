impl WorktreeService {
    pub fn summary(&self, project_id: Option<&str>, project_path: Option<&str>) -> WorktreeSummary {
        let Some(project_id) = project_id else {
            return WorktreeSummary {
                error: Some("no selected project".to_string()),
                ..Default::default()
            };
        };

        let content = match fs::read_to_string(&self.state_file) {
            Ok(content) => content,
            Err(error) => {
                if let Some(project_path) = project_path {
                    return fallback_project_worktree_summary(
                        project_id,
                        project_path,
                        true,
                        Some(error.to_string()),
                    );
                }
                return WorktreeSummary {
                    error: Some(error.to_string()),
                    ..Default::default()
                };
            }
        };
        let state = match serde_json::from_str::<StateFile>(&content) {
            Ok(state) => state,
            Err(error) => {
                if let Some(project_path) = project_path {
                    return fallback_project_worktree_summary(
                        project_id,
                        project_path,
                        true,
                        Some(error.to_string()),
                    );
                }
                return WorktreeSummary {
                    error: Some(error.to_string()),
                    ..Default::default()
                };
            }
        };

        let mut worktrees = state
            .worktrees
            .into_iter()
            .filter(|worktree| worktree.project_id == project_id)
            .map(|worktree| WorktreeInfo {
                exists: Path::new(&worktree.path).exists(),
                git_summary: project_worktree_git_summary(&worktree.path),
                id: worktree.id,
                project_id: worktree.project_id,
                name: worktree.name,
                branch: worktree.branch,
                path: worktree.path,
                status: worktree.status,
                is_default: worktree.is_default,
            })
            .collect::<Vec<_>>();

        if worktrees.is_empty()
            && let Some(project_path) = project_path
        {
            worktrees.push(default_project_worktree(project_id, project_path, true));
        }

        let selected_worktree_id = state
            .selected_worktree_id_by_project
            .get(project_id)
            .cloned()
            .filter(|id| worktrees.iter().any(|worktree| &worktree.id == id))
            .or_else(|| {
                worktrees
                    .iter()
                    .find(|worktree| worktree.is_default)
                    .or_else(|| worktrees.first())
                    .map(|worktree| worktree.id.clone())
            });

        let tasks = state
            .worktree_tasks
            .into_iter()
            .filter(|task| {
                worktrees
                    .iter()
                    .any(|worktree| worktree.id == task.worktree_id)
            })
            .map(|task| WorktreeTaskInfo {
                worktree_id: task.worktree_id,
                title: task.title,
                base_branch: task.base_branch,
                status: task.status,
            })
            .collect::<Vec<_>>();

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

        let content = match fs::read_to_string(&self.state_file) {
            Ok(content) => content,
            Err(error) => {
                if let Some(project_path) = project_path {
                    return fallback_project_worktree_summary(
                        project_id,
                        project_path,
                        false,
                        Some(error.to_string()),
                    );
                }
                return WorktreeSummary {
                    error: Some(error.to_string()),
                    ..Default::default()
                };
            }
        };
        let state = match serde_json::from_str::<StateFile>(&content) {
            Ok(state) => state,
            Err(error) => {
                if let Some(project_path) = project_path {
                    return fallback_project_worktree_summary(
                        project_id,
                        project_path,
                        false,
                        Some(error.to_string()),
                    );
                }
                return WorktreeSummary {
                    error: Some(error.to_string()),
                    ..Default::default()
                };
            }
        };

        let mut worktrees = state
            .worktrees
            .into_iter()
            .filter(|worktree| worktree.project_id == project_id)
            .map(|worktree| WorktreeInfo {
                exists: Path::new(&worktree.path).exists(),
                git_summary: ProjectWorktreeGitSummary::default(),
                id: worktree.id,
                project_id: worktree.project_id,
                name: worktree.name,
                branch: worktree.branch,
                path: worktree.path,
                status: worktree.status,
                is_default: worktree.is_default,
            })
            .collect::<Vec<_>>();

        if worktrees.is_empty()
            && let Some(project_path) = project_path
        {
            worktrees.push(default_project_worktree(project_id, project_path, false));
        }

        let selected_worktree_id = state
            .selected_worktree_id_by_project
            .get(project_id)
            .cloned()
            .filter(|id| worktrees.iter().any(|worktree| &worktree.id == id))
            .or_else(|| {
                worktrees
                    .iter()
                    .find(|worktree| worktree.is_default)
                    .or_else(|| worktrees.first())
                    .map(|worktree| worktree.id.clone())
            });

        let tasks = state
            .worktree_tasks
            .into_iter()
            .filter(|task| {
                worktrees
                    .iter()
                    .any(|worktree| worktree.id == task.worktree_id)
            })
            .map(|task| WorktreeTaskInfo {
                worktree_id: task.worktree_id,
                title: task.title,
                base_branch: task.base_branch,
                status: task.status,
            })
            .collect::<Vec<_>>();

        WorktreeSummary {
            available: true,
            selected_worktree_id,
            worktrees,
            tasks,
            active_git: crate::git::GitSummary::default(),
            error: None,
        }
    }
}

fn fallback_project_worktree_summary(
    project_id: &str,
    project_path: &str,
    include_git_stats: bool,
    error: Option<String>,
) -> WorktreeSummary {
    WorktreeSummary {
        available: true,
        selected_worktree_id: Some(project_id.to_string()),
        worktrees: vec![default_project_worktree(
            project_id,
            project_path,
            include_git_stats,
        )],
        tasks: Vec::new(),
        active_git: if include_git_stats {
            GitService::status(project_path)
        } else {
            crate::git::GitSummary::default()
        },
        error,
    }
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
