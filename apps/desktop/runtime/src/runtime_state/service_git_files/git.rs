impl RuntimeService {
    pub fn init_project_git(&self, project_path: &str) -> Result<git::GitSummary, String> {
        if let Some(result) =
            self.hosted_git_invoke_blocking(project_path, "init", serde_json::json!({}))
        {
            return result;
        }
        git::GitService::init(project_path)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn clone_project_git(
        &self,
        project_path: &str,
        remote_url: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke_blocking(
            project_path,
            "clone",
            serde_json::json!({ "remoteUrl": remote_url }),
        ) {
            return result;
        }
        git::GitService::clone_repository(project_path, remote_url)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn clone_project_git_with_credentials(
        &self,
        project_path: &str,
        remote_url: &str,
        credentials: git::GitCredentials,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke_blocking(
            project_path,
            "clone",
            serde_json::json!({ "remoteUrl": remote_url, "credentials": credentials }),
        ) {
            return result;
        }
        git::GitService::clone_repository_with_credentials(project_path, remote_url, credentials)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn trust_project_git_directory(&self, project_path: &str) -> Result<git::GitSummary, String> {
        if let Some(result) =
            self.hosted_git_invoke_blocking(project_path, "trust_directory", serde_json::json!({}))
        {
            return result;
        }
        git::GitService::trust_project_directory(project_path)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn read_project_git_diff(
        &self,
        project_path: &str,
        file_path: &str,
    ) -> Result<String, String> {
        if let Some(result) =
            self.hosted_git_read(project_path, "diff", json!({ "filePath": file_path }))
        {
            return result.map(|value| hosted_git_string(&value, "diff"));
        }
        git::GitService::file_diff(project_path, file_path)
    }

    pub fn read_project_git_review_diff(
        &self,
        project_path: &str,
        file_path: &str,
        base_branch: Option<&str>,
    ) -> Result<String, String> {
        if let Some(result) = self.hosted_git_read(project_path, "review_diff", serde_json::json!({ "filePath": file_path, "baseBranch": base_branch })) {
            return result.map(|value| hosted_git_string(&value, "diff"));
        }
        git::GitService::review_file_diff(project_path, file_path, base_branch)
    }

    pub fn read_project_git_commit_context(
        &self,
        project_path: &str,
    ) -> git::GitCommitMessageContextSummary {
        if let Some(result) =
            self.hosted_git_read(project_path, "commit_context", serde_json::json!({}))
        {
            return result
                .and_then(|value| serde_json::from_value(value).map_err(|error| error.to_string()))
                .unwrap_or_else(|error| git::GitCommitMessageContextSummary {
                    is_repository: false,
                    error: Some(error),
                    ..Default::default()
                });
        }
        git::GitService::commit_message_context(project_path)
    }

    pub fn read_project_git_review_file_content(
        &self,
        project_path: &str,
        file_path: &str,
        base_branch: Option<&str>,
    ) -> git::GitReviewContentSummary {
        if let Some(result) = self.hosted_git_read(project_path, "review_file_content", serde_json::json!({ "filePath": file_path, "baseBranch": base_branch })) {
            return result
                .and_then(|value| serde_json::from_value(value).map_err(|error| error.to_string()))
                .unwrap_or_else(|error| git::GitReviewContentSummary {
                    path: file_path.to_string(),
                    is_repository: false,
                    error: Some(error),
                    ..Default::default()
                });
        }
        git::GitService::review_file_content(project_path, file_path, base_branch)
    }

    pub fn read_project_git_path_status(
        &self,
        project_path: &str,
        directory_path: &str,
    ) -> Result<Vec<git::GitFileStatus>, String> {
        if let Some(result) = self.hosted_git_read(project_path, "path_status", serde_json::json!({ "directoryPath": directory_path })) {
            return result.and_then(|value| serde_json::from_value(value.get("entries").cloned().unwrap_or_default()).map_err(|error| error.to_string()));
        }
        git::GitService::path_status(project_path, directory_path)
    }

    pub fn stage_project_git_file(
        &self,
        project_path: &str,
        file_path: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "stage", json!({ "paths": [file_path] })) {
            return result;
        }
        git::GitService::stage_file(project_path, file_path)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn stage_project_git_paths(
        &self,
        project_path: &str,
        file_paths: &[String],
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "stage", json!({ "paths": file_paths })) {
            return result;
        }
        git::GitService::stage_paths(project_path, file_paths)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn unstage_project_git_file(
        &self,
        project_path: &str,
        file_path: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "unstage", json!({ "paths": [file_path] })) {
            return result;
        }
        git::GitService::unstage_file(project_path, file_path)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn unstage_project_git_paths(
        &self,
        project_path: &str,
        file_paths: &[String],
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "unstage", json!({ "paths": file_paths })) {
            return result;
        }
        git::GitService::unstage_paths(project_path, file_paths)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn commit_project_git(
        &self,
        project_path: &str,
        message: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "commit", json!({ "message": message })) {
            return result;
        }
        git::GitService::commit_staged(project_path, message)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn commit_project_git_action(
        &self,
        project_path: &str,
        message: &str,
        action: &str,
    ) -> Result<git::GitSummary, String> {
        let remote_op = match action {
            "commitAndPush" => "commit_push",
            "commitAndSync" => "commit_sync",
            _ => "commit",
        };
        if let Some(result) =
            self.hosted_git_invoke(project_path, remote_op, serde_json::json!({ "message": message }))
        {
            return result;
        }
        git::GitService::commit_action(project_path, message, action)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn amend_project_git_last_commit(
        &self,
        project_path: &str,
        message: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "amend", serde_json::json!({ "message": message })) {
            return result;
        }
        git::GitService::amend_last_commit_message(project_path, message)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn read_project_git_last_commit_message(
        &self,
        project_path: &str,
    ) -> Result<String, String> {
        if let Some(result) = self.hosted_git_read(project_path, "last_commit_message", serde_json::json!({})) {
            return result.map(|value| hosted_git_string(&value, "message"));
        }
        git::GitService::last_commit_message(project_path)
    }

    pub fn undo_project_git_last_commit(
        &self,
        project_path: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "undo_last_commit", serde_json::json!({})) {
            return result;
        }
        git::GitService::undo_last_commit(project_path)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn project_git_head_commit_pushed(&self, project_path: &str) -> Result<bool, String> {
        if let Some(result) = self.hosted_git_read(project_path, "head_pushed", serde_json::json!({})) {
            return result.map(|value| value.get("pushed").and_then(|v| v.as_bool()).unwrap_or(false));
        }
        git::GitService::head_commit_pushed(project_path)
    }

    pub fn fetch_project_git(&self, project_path: &str) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "fetch", serde_json::json!({})) {
            return result;
        }
        self.run_cancellable_project_git(project_path, |path, cancel| {
            git::git_fetch_with_cancel(path, Some(cancel)).map(|_| ())
        })
    }

    pub fn pull_project_git(&self, project_path: &str) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "pull", serde_json::json!({})) {
            return result;
        }
        self.run_cancellable_project_git(project_path, |path, cancel| {
            git::git_pull_with_cancel(path, Some(cancel)).map(|_| ())
        })
    }

    pub fn push_project_git(&self, project_path: &str) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "push", serde_json::json!({})) {
            return result;
        }
        self.run_cancellable_project_git(project_path, |path, cancel| {
            git::git_push_with_cancel(path, Some(cancel)).map(|_| ())
        })
    }

    pub fn sync_project_git(&self, project_path: &str) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "sync", serde_json::json!({})) {
            return result;
        }
        self.run_cancellable_project_git(project_path, |path, cancel| {
            git::git_sync_with_cancel(path, Some(cancel)).map(|_| ())
        })
    }

    pub fn force_push_project_git(&self, project_path: &str) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "force_push", serde_json::json!({})) {
            return result;
        }
        self.run_cancellable_project_git(project_path, |path, cancel| {
            git::git_force_push_with_cancel(path, Some(cancel)).map(|_| ())
        })
    }

    pub fn push_project_git_remote(
        &self,
        project_path: &str,
        remote: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "push_remote", serde_json::json!({ "remote": remote })) {
            return result;
        }
        let remote = remote.to_string();
        self.run_cancellable_project_git(project_path, move |path, cancel| {
            git::git_push_remote_with_cancel(
                git::GitPushRemoteRequest {
                    project_path: path,
                    remote,
                },
                Some(cancel),
            )
            .map(|_| ())
        })
    }

    pub fn push_project_git_remote_branch(
        &self,
        project_path: &str,
        remote_branch: &str,
        local_branch: Option<&str>,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "push_remote_branch", serde_json::json!({ "remoteBranch": remote_branch, "localBranch": local_branch })) {
            return result;
        }
        let remote_branch = remote_branch.to_string();
        let local_branch = local_branch.map(str::to_string);
        self.run_cancellable_project_git(project_path, move |path, cancel| {
            git::git_push_remote_branch_with_cancel(
                git::GitPushRemoteBranchRequest {
                    project_path: path,
                    remote_branch,
                    local_branch,
                },
                Some(cancel),
            )
            .map(|_| ())
        })
    }

    pub fn cancel_project_git(&self, project_path: &str) -> Result<(), String> {
        let key = git_cancel_key(project_path);
        let Some(token) = self
            .git_cancels
            .lock()
            .map_err(|_| "Git cancel lock is poisoned.".to_string())?
            .get(&key)
            .cloned()
        else {
            return Ok(());
        };
        token.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub fn checkout_project_git_branch(
        &self,
        project_path: &str,
        branch: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "checkout_branch", serde_json::json!({ "branch": branch })) {
            return result;
        }
        git::GitService::checkout_branch(project_path, branch)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn checkout_project_git_remote_branch(
        &self,
        project_path: &str,
        remote_branch: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "checkout_remote_branch", serde_json::json!({ "remoteBranch": remote_branch })) {
            return result;
        }
        git::GitService::checkout_remote_branch(project_path, remote_branch)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn create_project_git_branch(
        &self,
        project_path: &str,
        branch: &str,
        checkout: bool,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "create_branch", serde_json::json!({ "branch": branch, "checkout": checkout })) {
            return result;
        }
        git::GitService::create_branch(project_path, branch, None, checkout)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn create_project_git_branch_from(
        &self,
        project_path: &str,
        branch: &str,
        from: Option<&str>,
        checkout: bool,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "create_branch_from", serde_json::json!({ "branch": branch, "from": from, "checkout": checkout })) {
            return result;
        }
        git::GitService::create_branch(project_path, branch, from, checkout)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn merge_project_git_branch(
        &self,
        project_path: &str,
        branch: &str,
        squash: bool,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "merge_branch", serde_json::json!({ "branch": branch, "squash": squash })) {
            return result;
        }
        git::GitService::merge_branch(project_path, branch, squash)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn delete_project_git_branch(
        &self,
        project_path: &str,
        branch: &str,
        force: bool,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "delete_branch", serde_json::json!({ "branch": branch, "force": force })) {
            return result;
        }
        git::GitService::delete_branch(project_path, branch, force)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn discard_project_git_file(
        &self,
        project_path: &str,
        file_path: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "discard", json!({ "paths": [file_path] })) {
            return result;
        }
        git::GitService::discard_file(project_path, file_path)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn discard_project_git_paths(
        &self,
        project_path: &str,
        file_paths: &[String],
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "discard", json!({ "paths": file_paths })) {
            return result;
        }
        git::GitService::discard_paths(project_path, file_paths)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn checkout_project_git_commit(
        &self,
        project_path: &str,
        commit: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "checkout_commit", serde_json::json!({ "commit": commit })) {
            return result;
        }
        git::GitService::checkout_commit(project_path, commit)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn revert_project_git_commit(
        &self,
        project_path: &str,
        commit: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "revert_commit", serde_json::json!({ "commit": commit })) {
            return result;
        }
        git::GitService::revert_commit(project_path, commit)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn restore_project_git_commit(
        &self,
        project_path: &str,
        commit: &str,
        force_remote: bool,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "restore_commit", serde_json::json!({ "commit": commit, "forceRemote": force_remote })) {
            return result;
        }
        git::GitService::restore_commit(project_path, commit, force_remote)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn add_project_git_remote(
        &self,
        project_path: &str,
        name: &str,
        url: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "add_remote", serde_json::json!({ "name": name, "url": url })) {
            return result;
        }
        git::GitService::add_remote(project_path, name, url)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn remove_project_git_remote(
        &self,
        project_path: &str,
        name: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "remove_remote", serde_json::json!({ "name": name })) {
            return result;
        }
        git::GitService::remove_remote(project_path, name)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn append_project_gitignore(
        &self,
        project_path: &str,
        paths: &[String],
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "append_gitignore", serde_json::json!({ "paths": paths })) {
            return result;
        }
        git::GitService::append_gitignore(project_path, paths)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn rename_project_git_branch(
        &self,
        project_path: &str,
        branch: &str,
        new_name: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "rename_branch", serde_json::json!({ "branch": branch, "newName": new_name })) {
            return result;
        }
        git::GitService::rename_branch(project_path, branch, new_name)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn rebase_project_git_branch(
        &self,
        project_path: &str,
        branch: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "rebase_branch", serde_json::json!({ "branch": branch })) {
            return result;
        }
        git::GitService::rebase_branch(project_path, branch)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn fetch_prune_project_git(&self, project_path: &str) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "fetch_prune", serde_json::json!({})) {
            return result;
        }
        self.run_cancellable_project_git(project_path, |path, cancel| {
            git::git_fetch_prune_with_cancel(path, Some(cancel)).map(|_| ())
        })
    }

    pub fn delete_project_git_remote_branch(
        &self,
        project_path: &str,
        remote_branch: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "delete_remote_branch", serde_json::json!({ "remoteBranch": remote_branch })) {
            return result;
        }
        let remote_branch = remote_branch.to_string();
        self.run_cancellable_project_git(project_path, move |path, cancel| {
            git::git_delete_remote_branch_with_cancel(path, remote_branch, Some(cancel)).map(|_| ())
        })
    }

    pub fn stash_project_git(
        &self,
        project_path: &str,
        message: Option<&str>,
        include_untracked: bool,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "stash_push", serde_json::json!({ "message": message.unwrap_or_default(), "includeUntracked": include_untracked })) {
            return result;
        }
        git::GitService::stash_push(project_path, message, include_untracked)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn apply_project_git_stash(
        &self,
        project_path: &str,
        index: usize,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "stash_apply", serde_json::json!({ "index": index })) {
            return result;
        }
        git::GitService::stash_apply(project_path, index)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn pop_project_git_stash(
        &self,
        project_path: &str,
        index: usize,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "stash_pop", serde_json::json!({ "index": index })) {
            return result;
        }
        git::GitService::stash_pop(project_path, index)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn drop_project_git_stash(
        &self,
        project_path: &str,
        index: usize,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "stash_drop", serde_json::json!({ "index": index })) {
            return result;
        }
        git::GitService::stash_drop(project_path, index)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn drop_all_project_git_stashes(
        &self,
        project_path: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "stash_drop_all", serde_json::json!({})) {
            return result;
        }
        git::GitService::stash_drop_all(project_path)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn create_project_git_tag(
        &self,
        project_path: &str,
        name: &str,
        message: Option<&str>,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "create_tag", serde_json::json!({ "name": name, "message": message.unwrap_or_default() })) {
            return result;
        }
        git::GitService::create_tag(project_path, name, message)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn delete_project_git_tag(
        &self,
        project_path: &str,
        name: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "delete_tag", serde_json::json!({ "name": name })) {
            return result;
        }
        git::GitService::delete_tag(project_path, name)?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    pub fn push_project_git_tags(
        &self,
        project_path: &str,
        remote: Option<&str>,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "push_tags", serde_json::json!({ "remote": remote.unwrap_or_default() })) {
            return result;
        }
        let remote = remote.map(str::to_string);
        self.run_cancellable_project_git(project_path, move |path, cancel| {
            git::git_push_tags_with_cancel(path, remote, Some(cancel)).map(|_| ())
        })
    }

    pub fn delete_project_git_remote_tag(
        &self,
        project_path: &str,
        remote: Option<&str>,
        name: &str,
    ) -> Result<git::GitSummary, String> {
        if let Some(result) = self.hosted_git_invoke(project_path, "delete_remote_tag", serde_json::json!({ "remote": remote.unwrap_or_default(), "name": name })) {
            return result;
        }
        let remote = remote.map(str::to_string);
        let name = name.to_string();
        self.run_cancellable_project_git(project_path, move |path, cancel| {
            git::git_delete_remote_tag_with_cancel(path, remote, name, Some(cancel)).map(|_| ())
        })
    }

    fn run_cancellable_project_git(
        &self,
        project_path: &str,
        action: impl FnOnce(String, git::GitCancelToken) -> Result<(), String>,
    ) -> Result<git::GitSummary, String> {
        let token = self.create_git_cancel_token(project_path);
        let result = action(project_path.to_string(), Arc::clone(&token));
        self.clear_git_cancel_token(project_path, &token);
        result?;
        Ok(refresh_git_summary(&self.support_dir, project_path))
    }

    fn create_git_cancel_token(&self, project_path: &str) -> git::GitCancelToken {
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Ok(mut cancels) = self.git_cancels.lock() {
            cancels.insert(git_cancel_key(project_path), Arc::clone(&token));
        }
        token
    }

    fn clear_git_cancel_token(&self, project_path: &str, token: &git::GitCancelToken) {
        if let Ok(mut cancels) = self.git_cancels.lock() {
            let key = git_cancel_key(project_path);
            if cancels
                .get(&key)
                .is_some_and(|current| Arc::ptr_eq(current, token))
            {
                cancels.remove(&key);
            }
        }
    }
}

fn git_cancel_key(project_path: &str) -> String {
    let normalized = Path::new(project_path.trim())
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(project_path.trim()));
    let mut key = normalized.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    #[cfg(windows)]
    {
        key = key.to_ascii_lowercase();
    }
    key
}

#[cfg(test)]
mod git_cancel_tests {
    use super::*;

    #[test]
    fn project_git_cancel_marks_active_token() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-runtime-service-git-cancel-{}",
            uuid::Uuid::new_v4()
        ));
        let service = RuntimeService::new(support_dir);
        let project_path = "/tmp/codux-runtime-service-git-cancel-project/";
        let token = service.create_git_cancel_token(project_path);

        service.cancel_project_git(project_path).unwrap();

        assert!(token.load(std::sync::atomic::Ordering::Relaxed));
        service.clear_git_cancel_token(project_path, &token);
        service.cancel_project_git(project_path).unwrap();
    }

    #[test]
    fn git_cancel_key_matches_tauri_normalization() {
        assert_eq!(
            git_cancel_key("/tmp/codux-project///"),
            "/tmp/codux-project"
        );
    }
}
