impl GitService {
    pub fn review(project_path: &str, base_branch: Option<&str>) -> GitReviewSummary {
        let repo = match open_git_repository(project_path) {
            Ok(repo) => repo,
            Err(error) => {
                return GitReviewSummary {
                    mode: "workingTreeAudit".to_string(),
                    title: "Uncommitted Audit".to_string(),
                    base_branch: base_branch.map(str::to_string),
                    error: Some(error),
                    ..Default::default()
                };
            }
        };
        git_review_from_repo(&repo, base_branch)
    }

    pub fn review_file_diff(
        project_path: &str,
        file_path: &str,
        base_branch: Option<&str>,
    ) -> Result<String, String> {
        let repo = open_git_repository(project_path)?;
        let file_path = safe_git_path(file_path)?;
        let base = base_branch
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "current branch");
        if let Some(base) = base {
            return git2_commit_diff_to_string(&repo, base, Some(&file_path), 3);
        }

        let staged =
            git2_diff_to_string(&repo, DiffTarget::Index, Some(&file_path), 3).unwrap_or_default();
        let unstaged = git2_diff_to_string(&repo, DiffTarget::Worktree, Some(&file_path), 3)
            .unwrap_or_default();
        Ok(
            match (staged.trim().is_empty(), unstaged.trim().is_empty()) {
                (true, true) if is_untracked_path_git2(&repo, &file_path) => {
                    untracked_file_preview(repo_root(&repo), &file_path)?
                }
                (true, _) => truncate_diff(unstaged),
                (_, true) => truncate_diff(staged),
                _ => truncate_diff(format!("{staged}\n{unstaged}")),
            },
        )
    }

    pub fn review_file_content(
        project_path: &str,
        file_path: &str,
        base_branch: Option<&str>,
    ) -> GitReviewContentSummary {
        let repo = match open_git_repository(project_path) {
            Ok(repo) => repo,
            Err(error) => {
                return GitReviewContentSummary {
                    path: file_path.to_string(),
                    is_repository: false,
                    error: Some(error),
                    ..Default::default()
                };
            }
        };
        let file_path = match safe_git_path(file_path) {
            Ok(path) if !path.is_empty() => path,
            Ok(_) => {
                return GitReviewContentSummary {
                    is_repository: true,
                    error: Some("File path cannot be empty.".to_string()),
                    ..Default::default()
                };
            }
            Err(error) => {
                return GitReviewContentSummary {
                    path: file_path.to_string(),
                    is_repository: true,
                    error: Some(error),
                    ..Default::default()
                };
            }
        };
        let base = base_branch
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "current branch");
        let head_content = git2_blob_or_empty(&repo, "HEAD", &file_path);
        let base_content = base.map(|reference| git2_blob_or_empty(&repo, reference, &file_path));
        let index_content = git2_index_blob(&repo, &file_path).ok();
        let worktree_content = read_worktree_file(repo_root(&repo), &file_path).unwrap_or_default();
        let is_untracked = base.is_none() && is_untracked_path_git2(&repo, &file_path);
        let diff = if is_untracked {
            String::new()
        } else if let Some(base) = base {
            git2_commit_diff_to_string(&repo, base, Some(&file_path), 0).unwrap_or_default()
        } else {
            let unstaged = git2_diff_to_string(&repo, DiffTarget::Worktree, Some(&file_path), 0)
                .unwrap_or_default();
            let staged = git2_diff_to_string(&repo, DiffTarget::Index, Some(&file_path), 0)
                .unwrap_or_default();
            match (staged.trim().is_empty(), unstaged.trim().is_empty()) {
                (true, _) => unstaged,
                (_, true) => staged,
                _ => format!("{staged}\n{unstaged}"),
            }
        };
        let (deleted_lines, added_lines) = if is_untracked {
            (
                Vec::new(),
                (1..=worktree_content.lines().count()).collect::<Vec<_>>(),
            )
        } else {
            parse_diff_line_numbers(&diff)
        };

        GitReviewContentSummary {
            path: file_path,
            head_content,
            base_content,
            index_content,
            worktree_content,
            added_lines,
            deleted_lines,
            is_repository: true,
            error: None,
        }
    }
}
