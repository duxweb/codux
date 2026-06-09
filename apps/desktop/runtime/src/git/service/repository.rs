impl GitService {
    pub fn status(project_path: &str) -> GitSummary {
        let repo = match open_git_repository(project_path) {
            Ok(repo) => repo,
            Err(error) => {
                return GitSummary {
                    branch: "uninitialized".to_string(),
                    error: Some(error),
                    ..Default::default()
                };
            }
        };
        git_status_from_repo(&repo)
    }

    pub fn init(project_path: &str) -> Result<(), String> {
        let path = Path::new(project_path.trim());
        if !path.exists() {
            return Err(format!("Project path does not exist: {}", path.display()));
        }
        GitRepository::init(path)
            .map(|_| ())
            .map_err(|error| error.message().to_string())
    }

    pub fn clone_repository(project_path: &str, remote_url: &str) -> Result<(), String> {
        let remote_url = remote_url.trim();
        if remote_url.is_empty() {
            return Err("Remote URL cannot be empty.".to_string());
        }
        let project_path = Path::new(project_path.trim());
        clone_repository_git2(remote_url, project_path)
    }

    pub fn clone_repository_with_credentials(
        project_path: &str,
        remote_url: &str,
        credentials: GitCredentials,
    ) -> Result<(), String> {
        let remote_url = remote_url.trim();
        if remote_url.is_empty() {
            return Err("Remote URL cannot be empty.".to_string());
        }
        if credentials.username.trim().is_empty()
            || credentials.password_or_token.trim().is_empty()
        {
            return Err("Username and password or token cannot be empty.".to_string());
        }
        let project_path = Path::new(project_path.trim());
        clone_repository_git2_with_credentials(remote_url, project_path, credentials)
    }

    pub fn path_status(
        project_path: &str,
        directory_path: &str,
    ) -> Result<Vec<GitFileStatus>, String> {
        let repo = open_git_repository(project_path)?;
        let directory_path = safe_git_path(directory_path)?;
        let prefix = if directory_path.is_empty() {
            String::new()
        } else {
            format!("{}/", directory_path.trim_end_matches('/'))
        };
        Ok(collapse_path_status_files(
            flatten_path_status_files(&repo, &directory_path),
            &directory_path,
        )
            .into_iter()
            .filter(|file| file.path == directory_path || file.path.starts_with(&prefix))
            .collect())
    }

    pub fn file_diff(project_path: &str, file_path: &str) -> Result<String, String> {
        let repo = open_git_repository(project_path)?;
        let file_path = safe_git_path(file_path)?;
        if is_untracked_path_git2(&repo, &file_path) {
            return untracked_file_preview(repo_root(&repo), &file_path);
        }
        let mut parts = Vec::new();

        let staged = truncate_diff(git2_diff_to_string(
            &repo,
            DiffTarget::Index,
            Some(&file_path),
            3,
        )?);
        if !staged.trim().is_empty() {
            parts.push(format!("--- staged ---\n{staged}"));
        }

        let unstaged = truncate_diff(git2_diff_to_string(
            &repo,
            DiffTarget::Worktree,
            Some(&file_path),
            3,
        )?);
        if !unstaged.trim().is_empty() {
            parts.push(format!("--- unstaged ---\n{unstaged}"));
        }

        if parts.is_empty() {
            Ok("No diff for selected file.".to_string())
        } else {
            Ok(parts.join("\n\n"))
        }
    }

    pub fn stage_file(project_path: &str, file_path: &str) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        stage_paths_git2(&repo, &[safe_git_path(file_path)?])
    }

    pub fn stage_paths(project_path: &str, file_paths: &[String]) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        stage_paths_git2(&repo, &safe_git_paths(file_paths)?)
    }

    pub fn unstage_file(project_path: &str, file_path: &str) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        unstage_paths_git2(&repo, &[safe_git_path(file_path)?])
    }

    pub fn unstage_paths(project_path: &str, file_paths: &[String]) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        unstage_paths_git2(&repo, &safe_git_paths(file_paths)?)
    }

    pub fn discard_file(project_path: &str, file_path: &str) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        discard_paths_git2(&repo, &[safe_git_path(file_path)?])
    }

    pub fn discard_paths(project_path: &str, file_paths: &[String]) -> Result<(), String> {
        let repo = open_git_repository(project_path)?;
        discard_paths_git2(&repo, &safe_git_paths(file_paths)?)
    }
}

fn safe_git_paths(file_paths: &[String]) -> Result<Vec<String>, String> {
    file_paths
        .iter()
        .map(|path| safe_git_path(path))
        .collect::<Result<Vec<_>, _>>()
}
