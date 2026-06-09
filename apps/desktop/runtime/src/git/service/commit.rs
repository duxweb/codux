impl GitService {
    pub fn commit_message_context(project_path: &str) -> GitCommitMessageContextSummary {
        let repo = match open_git_repository(project_path) {
            Ok(repo) => repo,
            Err(error) => {
                return GitCommitMessageContextSummary {
                    is_repository: false,
                    error: Some(error),
                    ..Default::default()
                };
            }
        };
        match git2_diff_to_string(&repo, DiffTarget::Index, None, 1) {
            Ok(diff) => {
                let (diff, truncated) = compact_commit_message_diff(&diff);
                GitCommitMessageContextSummary {
                    diff,
                    truncated,
                    is_repository: true,
                    error: None,
                }
            }
            Err(error) => GitCommitMessageContextSummary {
                is_repository: true,
                error: Some(error),
                ..Default::default()
            },
        }
    }

    pub fn commit_staged(project_path: &str, message: &str) -> Result<(), String> {
        let message = message.trim();
        if message.is_empty() {
            return Err("Commit message cannot be empty.".to_string());
        }
        let repo = open_git_repository(project_path)?;
        create_commit_git2(&repo, message, false).map(|_| ())
    }

    pub fn commit_action(project_path: &str, message: &str, action: &str) -> Result<(), String> {
        let message = message.trim();
        if message.is_empty() {
            return Err("Commit message cannot be empty.".to_string());
        }
        let repo = open_git_repository(project_path)?;
        create_commit_git2(&repo, message, false)?;
        match action {
            "commit" => Ok(()),
            "commitAndPush" => push_current_branch_git2(&repo, None, false, None),
            "commitAndSync" => {
                pull_current_branch_git2(&repo, None)?;
                push_current_branch_git2(&repo, None, false, None)
            }
            _ => Err(format!("Unknown commit action: {action}")),
        }
    }

    pub fn amend_last_commit_message(project_path: &str, message: &str) -> Result<(), String> {
        let message = message.trim();
        if message.is_empty() {
            return Err("Commit message cannot be empty.".to_string());
        }
        let repo = open_git_repository(project_path)?;
        create_commit_git2(&repo, message, true).map(|_| ())
    }

    pub fn last_commit_message(project_path: &str) -> Result<String, String> {
        let repo = open_git_repository(project_path)?;
        let commit = repo
            .head()
            .map_err(|error| error.message().to_string())?
            .peel_to_commit()
            .map_err(|error| error.message().to_string())?;
        Ok(commit.summary().ok().flatten().unwrap_or("").to_string())
    }

    pub fn undo_last_commit(project_path: &str) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        soft_reset_to_parent_git2(&repo)
    }

    pub fn head_commit_pushed(project_path: &str) -> Result<bool, String> {
        let repo = open_git_repository(project_path)?;
        let Some(head) = repo.head().ok().and_then(|head| head.target()) else {
            return Ok(false);
        };
        let Some(upstream) = upstream_branch_name(&repo) else {
            return Ok(false);
        };
        let upstream_ref = format!("refs/remotes/{upstream}");
        let Some(upstream_target) = repo
            .find_reference(&upstream_ref)
            .ok()
            .and_then(|reference| reference.target())
        else {
            return Ok(false);
        };
        Ok(repo.graph_descendant_of(upstream_target, head).unwrap_or(false))
    }
}
