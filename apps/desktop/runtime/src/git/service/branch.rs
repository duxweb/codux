impl GitService {
    pub fn checkout_branch(project_path: &str, branch: &str) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        checkout_branch_git2(&repo, safe_branch_name(branch)?.as_str())
    }

    pub fn checkout_remote_branch(project_path: &str, remote_branch: &str) -> Result<(), String> {
        let remote_branch = remote_branch.trim();
        if remote_branch.is_empty() {
            return Err("Remote branch name cannot be empty.".to_string());
        }
        let local_name = remote_branch
            .split_once('/')
            .map(|(_, branch)| branch)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(remote_branch);
        let repo = open_git_repository(project_path)?;
        checkout_remote_branch_git2(&repo, remote_branch, local_name)
    }

    pub fn create_branch(
        project_path: &str,
        branch: &str,
        from: Option<&str>,
        checkout: bool,
    ) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        create_branch_git2(&repo, safe_branch_name(branch)?.as_str(), from, checkout)
    }

    pub fn merge_branch(project_path: &str, branch: &str, squash: bool) -> Result<(), String> {
        let branch = branch.trim();
        if branch.is_empty() {
            return Err("Branch name cannot be empty.".to_string());
        }
        let repo = open_git_repository(project_path)?;
        merge_branch_git2(&repo, branch, squash)
    }

    pub fn delete_branch(project_path: &str, branch: &str, force: bool) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        delete_branch_git2(&repo, safe_branch_name(branch)?.as_str(), force)
    }

    pub fn checkout_commit(project_path: &str, commit: &str) -> Result<(), String> {
        let commit = commit.trim();
        if commit.is_empty() {
            return Err("Commit cannot be empty.".to_string());
        }
        let repo = open_git_repository(project_path)?;
        checkout_commit_git2(&repo, commit)
    }

    pub fn revert_commit(project_path: &str, commit: &str) -> Result<(), String> {
        let commit = commit.trim();
        if commit.is_empty() {
            return Err("Commit cannot be empty.".to_string());
        }
        let repo = open_git_repository(project_path)?;
        revert_commit_git2(&repo, commit)
    }

    pub fn restore_commit(
        project_path: &str,
        commit: &str,
        force_remote: bool,
    ) -> Result<(), String> {
        let commit = commit.trim();
        if commit.is_empty() {
            return Err("Commit cannot be empty.".to_string());
        }
        let repo = open_git_repository(project_path)?;
        hard_reset_git2(&repo, commit)?;
        if force_remote {
            push_current_branch_git2(&repo, None, true, None)?;
        }
        Ok(())
    }
}
