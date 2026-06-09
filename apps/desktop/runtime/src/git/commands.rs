pub fn git_brief_status(project_path: String) -> GitBriefStatus {
    let status = git_status(project_path);
    GitBriefStatus {
        branch: status.branch,
        ahead: status.ahead,
        behind: status.behind,
        changes: status.staged.len() + status.unstaged.len() + status.untracked.len(),
        is_repository: status.is_repository,
        error: status.error,
    }
}

pub fn git_status(project_path: String) -> GitStatusSnapshot {
    let repo = match open_git_repository(&project_path) {
        Ok(repo) => repo,
        Err(error) => {
            return GitStatusSnapshot {
                branch: "uninitialized".to_string(),
                upstream: None,
                ahead: 0,
                behind: 0,
                staged: Vec::new(),
                unstaged: Vec::new(),
                untracked: Vec::new(),
                commits: Vec::new(),
                branches: Vec::new(),
                remote_branches: Vec::new(),
                remotes: Vec::new(),
                is_repository: false,
                error: Some(error),
            };
        }
    };
    git_status_snapshot_from_repo(&repo)
}

pub fn git_stage(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    stage_paths_git2(&repo, &request.paths)?;
    Ok(git_status(root))
}

pub fn git_unstage(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    unstage_paths_git2(&repo, &request.paths)?;
    Ok(git_status(root))
}

pub fn git_commit(request: GitCommitRequest) -> Result<GitStatusSnapshot, String> {
    GitService::commit_staged(&request.project_path, &request.message)?;
    Ok(git_status(request.project_path))
}

pub fn git_commit_action(request: GitCommitActionRequest) -> Result<GitStatusSnapshot, String> {
    GitService::commit_action(&request.project_path, &request.message, &request.action)?;
    Ok(git_status(request.project_path))
}

pub fn git_amend_last_commit_message(
    request: GitCommitRequest,
) -> Result<GitStatusSnapshot, String> {
    GitService::amend_last_commit_message(&request.project_path, &request.message)?;
    Ok(git_status(request.project_path))
}

pub fn git_last_commit_message(project_path: String) -> Result<String, String> {
    GitService::last_commit_message(&project_path)
}

pub fn git_undo_last_commit(project_path: String) -> Result<GitStatusSnapshot, String> {
    GitService::undo_last_commit(&project_path)?;
    Ok(git_status(project_path))
}

pub fn git_head_commit_pushed(project_path: String) -> Result<bool, String> {
    GitService::head_commit_pushed(&project_path)
}

pub fn git_init(project_path: String) -> Result<GitStatusSnapshot, String> {
    GitService::init(&project_path)?;
    Ok(git_status(project_path))
}

pub fn git_clone(request: GitCloneRequest) -> Result<GitStatusSnapshot, String> {
    GitService::clone_repository(&request.project_path, &request.remote_url)?;
    Ok(git_status(request.project_path))
}

pub fn git_discard(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    discard_paths_git2(&repo, &request.paths)?;
    Ok(git_status(root))
}

pub fn git_branches(project_path: String) -> GitBranchesSnapshot {
    let repo = match open_git_repository(&project_path) {
        Ok(repo) => repo,
        Err(error) => {
            return GitBranchesSnapshot {
                current: String::new(),
                local: Vec::new(),
                remote: Vec::new(),
                is_repository: false,
                error: Some(error),
            };
        }
    };
    let current = current_branch_name(&repo);
    let local = git2_branches(&repo, git2::BranchType::Local, &current);
    let remote = git2_branches(&repo, git2::BranchType::Remote, &current)
        .into_iter()
        .filter(|branch| !branch.name.ends_with("/HEAD"))
        .collect();
    GitBranchesSnapshot {
        current,
        local,
        remote,
        is_repository: true,
        error: None,
    }
}

pub fn git_checkout_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    GitService::checkout_branch(&request.project_path, &request.branch)?;
    Ok(git_status(request.project_path))
}

pub fn git_create_branch(request: GitCreateBranchRequest) -> Result<GitStatusSnapshot, String> {
    GitService::create_branch(
        &request.project_path,
        &request.branch,
        request.from.as_deref(),
        request.checkout,
    )?;
    Ok(git_status(request.project_path))
}

pub fn git_checkout_remote_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    GitService::checkout_remote_branch(&request.project_path, &request.branch)?;
    Ok(git_status(request.project_path))
}

pub fn git_merge_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    GitService::merge_branch(&request.project_path, &request.branch, false)?;
    Ok(git_status(request.project_path))
}

pub fn git_squash_merge_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    GitService::merge_branch(&request.project_path, &request.branch, true)?;
    Ok(git_status(request.project_path))
}

pub fn git_delete_branch(request: GitDeleteBranchRequest) -> Result<GitStatusSnapshot, String> {
    GitService::delete_branch(&request.project_path, &request.branch, request.force)?;
    Ok(git_status(request.project_path))
}

pub fn git_checkout_commit(request: GitCommitRefRequest) -> Result<GitStatusSnapshot, String> {
    GitService::checkout_commit(&request.project_path, &request.commit)?;
    Ok(git_status(request.project_path))
}

pub fn git_revert_commit(request: GitCommitRefRequest) -> Result<GitStatusSnapshot, String> {
    GitService::revert_commit(&request.project_path, &request.commit)?;
    Ok(git_status(request.project_path))
}

pub fn git_restore_commit(request: GitRestoreCommitRequest) -> Result<GitStatusSnapshot, String> {
    GitService::restore_commit(&request.project_path, &request.commit, request.force_remote)?;
    Ok(git_status(request.project_path))
}

pub fn git_add_remote(request: GitRemoteRequest) -> Result<GitStatusSnapshot, String> {
    let url = request.url.as_deref().unwrap_or("");
    GitService::add_remote(&request.project_path, &request.name, url)?;
    Ok(git_status(request.project_path))
}

pub fn git_remove_remote(request: GitRemoteRequest) -> Result<GitStatusSnapshot, String> {
    GitService::remove_remote(&request.project_path, &request.name)?;
    Ok(git_status(request.project_path))
}

pub fn git_append_gitignore(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    GitService::append_gitignore(&request.project_path, &request.paths)?;
    Ok(git_status(request.project_path))
}

pub fn git_fetch_with_cancel(
    project_path: String,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    fetch_all_remotes_git2(&repo, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_sync_with_cancel(
    project_path: String,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    pull_current_branch_git2(&repo, cancel.as_ref())?;
    push_current_branch_git2(&repo, None, false, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_push_remote_with_cancel(
    request: GitPushRemoteRequest,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let remote = request.remote.trim();
    if remote.is_empty() {
        return Err("Remote name cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    push_current_branch_git2(&repo, Some(remote), false, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_push_remote_branch_with_cancel(
    request: GitPushRemoteBranchRequest,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let remote_branch = request.remote_branch.trim();
    if remote_branch.is_empty() {
        return Err("Remote branch cannot be empty.".to_string());
    }
    let (remote, branch_name) = remote_branch
        .split_once('/')
        .ok_or_else(|| "Remote branch must include a remote name.".to_string())?;
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    let branch = request
        .local_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| current_branch_name(&repo));
    if branch == "HEAD" || branch == "uninitialized" {
        return Err("Cannot push detached HEAD to a remote branch.".to_string());
    }
    let refspec = format!("{branch}:{branch_name}");
    push_refspec_git2(&repo, remote, &refspec, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_pull_with_cancel(
    project_path: String,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    pull_current_branch_git2(&repo, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_push_with_cancel(
    project_path: String,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    push_current_branch_git2(&repo, None, false, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_force_push_with_cancel(
    project_path: String,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    push_current_branch_git2(&repo, None, true, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_diff_file(request: GitDiffRequest) -> GitDiffSnapshot {
    let repo = match open_git_repository(&request.project_path) {
        Ok(repo) => repo,
        Err(error) => {
            return GitDiffSnapshot {
                path: request.path,
                diff: String::new(),
                is_repository: false,
                error: Some(error),
            };
        }
    };
    let path = match safe_git_path(&request.path) {
        Ok(path) if !path.is_empty() => path,
        Ok(_) => {
            return GitDiffSnapshot {
                path: String::new(),
                diff: String::new(),
                is_repository: true,
                error: Some("File path cannot be empty.".to_string()),
            };
        }
        Err(error) => {
            return GitDiffSnapshot {
                path: request.path,
                diff: String::new(),
                is_repository: true,
                error: Some(error),
            };
        }
    };
    let diff = if request.staged {
        git2_diff_to_string(&repo, DiffTarget::Index, Some(&path), 3)
    } else {
        git2_diff_to_string(&repo, DiffTarget::Worktree, Some(&path), 3)
    }
    .unwrap_or_default();
    let diff = if diff.trim().is_empty() && !request.staged && is_untracked_path_git2(&repo, &path)
    {
        format!("Untracked file: {path}\n\nStage the file to include it in the next commit.")
    } else {
        diff
    };
    GitDiffSnapshot {
        path,
        diff,
        is_repository: true,
        error: None,
    }
}

pub fn git_commit_message_context(project_path: String) -> GitCommitMessageContextSnapshot {
    GitService::commit_message_context(&project_path)
}

pub fn git_review_diff_file(request: GitReviewDiffRequest) -> GitDiffSnapshot {
    let diff = GitService::review_file_diff(
        &request.project_path,
        &request.path,
        request.base_branch.as_deref(),
    );
    match diff {
        Ok(diff) => GitDiffSnapshot {
            path: request.path,
            diff,
            is_repository: true,
            error: None,
        },
        Err(error) => GitDiffSnapshot {
            path: request.path,
            diff: String::new(),
            is_repository: false,
            error: Some(error),
        },
    }
}

pub fn git_review_file_content(request: GitReviewContentRequest) -> GitReviewContentSnapshot {
    GitService::review_file_content(
        &request.project_path,
        &request.path,
        request.base_branch.as_deref(),
    )
}

pub fn git_review(project_path: String, base_branch: Option<String>) -> GitReviewSnapshot {
    GitService::review(&project_path, base_branch.as_deref())
}
