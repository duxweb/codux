impl GitService {
    pub fn fetch(project_path: &str) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        fetch_all_remotes_git2(&repo, None)
    }

    pub fn pull(project_path: &str) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        pull_current_branch_git2(&repo, None)
    }

    pub fn push(project_path: &str) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        push_current_branch_git2(&repo, None, false, None)
    }

    pub fn force_push(project_path: &str) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        push_current_branch_git2(&repo, None, true, None)
    }

    pub fn sync(project_path: &str) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        pull_current_branch_git2(&repo, None)?;
        push_current_branch_git2(&repo, None, false, None)
    }

    pub fn push_remote(project_path: &str, remote: &str) -> Result<(), String> {
        let remote = remote.trim();
        if remote.is_empty() {
            return Err("Remote name cannot be empty.".to_string());
        }
        let repo = open_git_repository(project_path)?;
        push_current_branch_git2(&repo, Some(remote), false, None)
    }

    pub fn push_remote_branch(
        project_path: &str,
        remote_branch: &str,
        local_branch: Option<&str>,
    ) -> Result<(), String> {
        let remote_branch = remote_branch.trim();
        if remote_branch.is_empty() {
            return Err("Remote branch cannot be empty.".to_string());
        }
        let (remote, branch_name) = remote_branch
            .split_once('/')
            .ok_or_else(|| "Remote branch must include a remote name.".to_string())?;
        let repo = open_git_repository(project_path)?;
        let branch = local_branch
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| current_branch_name(&repo));
        if branch == "HEAD" || branch == "uninitialized" {
            return Err("Cannot push detached HEAD to a remote branch.".to_string());
        }
        let refspec = format!("{branch}:{branch_name}");
        push_refspec_git2(&repo, remote, &refspec, None)
    }

    pub fn add_remote(project_path: &str, name: &str, url: &str) -> Result<(), String> {
        let name = name.trim();
        let url = url.trim();
        if name.is_empty() {
            return Err("Remote name cannot be empty.".to_string());
        }
        if url.is_empty() {
            return Err("Remote URL cannot be empty.".to_string());
        }
        let repo = open_git_repository(project_path)?;
        repo.remote(name, url)
            .map(|_| ())
            .map_err(|error| error.message().to_string())
    }

    pub fn remove_remote(project_path: &str, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Remote name cannot be empty.".to_string());
        }
        let repo = open_git_repository(project_path)?;
        repo.remote_delete(name)
            .map_err(|error| error.message().to_string())
    }
}
