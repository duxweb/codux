fn open_git_repository(path: &str) -> Result<GitRepository, String> {
    let path = Path::new(path.trim());
    if path.as_os_str().is_empty() {
        return Err("Project path cannot be empty.".to_string());
    }
    GitRepository::discover(path).map_err(|error| error.message().to_string())
}

fn repo_root(repo: &GitRepository) -> &Path {
    repo.workdir()
        .or_else(|| repo.path().parent())
        .unwrap_or_else(|| Path::new(""))
}

const MAX_GIT_STATUS_FILES: usize = 1200;
const MAX_GIT_PATH_STATUS_FILES: usize = 1200;

fn git_status_from_repo(repo: &GitRepository) -> GitSummary {
    let branch = current_branch_name(repo);
    let upstream = upstream_branch_name(repo);
    let (ahead, behind) = ahead_behind(repo).unwrap_or((0, 0));
    let head_pushed = head_commit_pushed_from_repo(repo);
    let raw_changed_files = flatten_status_files(repo);
    let changed_files = collapse_path_status_files(raw_changed_files.clone(), "");
    let branches = git2_branches(repo, git2::BranchType::Local, &branch);
    let remote_branches = git2_branches(repo, git2::BranchType::Remote, &branch)
        .into_iter()
        .filter(|branch| !branch.name.ends_with("/HEAD"))
        .map(|branch| branch.name)
        .collect();
    let remotes = git2_remotes(repo);
    let commits = git2_commit_log(repo, 20);

    let staged = raw_changed_files
        .iter()
        .filter(|file| {
            let index = file.index_status.trim();
            !index.is_empty() && index != "?"
        })
        .count();
    let unstaged = raw_changed_files
        .iter()
        .filter(|file| !is_untracked_status(file) && !file.worktree_status.trim().is_empty())
        .count();
    let untracked = raw_changed_files
        .iter()
        .filter(|file| is_untracked_status(file))
        .count();

    GitSummary {
        branch,
        upstream,
        ahead,
        behind,
        head_pushed,
        staged,
        unstaged,
        untracked,
        is_repository: true,
        error: None,
        changed_files,
        branches,
        remote_branches,
        remotes,
        commits,
    }
}

fn head_commit_pushed_from_repo(repo: &GitRepository) -> bool {
    let Some(head) = repo.head().ok().and_then(|head| head.target()) else {
        return false;
    };
    let Some(upstream) = upstream_branch_name(repo) else {
        return false;
    };
    let upstream_ref = format!("refs/remotes/{upstream}");
    let Some(upstream_target) = repo
        .find_reference(&upstream_ref)
        .ok()
        .and_then(|reference| reference.target())
    else {
        return false;
    };
    repo.graph_descendant_of(upstream_target, head)
        .unwrap_or(false)
}

fn git_status_snapshot_from_repo(repo: &GitRepository) -> GitStatusSnapshot {
    let branch = current_branch_name(repo);
    let upstream = upstream_branch_name(repo);
    let (ahead, behind) = ahead_behind(repo).unwrap_or((0, 0));
    let (staged, unstaged, untracked) = git2_status_files(repo);
    let branches = git2_branches(repo, git2::BranchType::Local, &branch);
    let remote_branch_summaries = git2_branches(repo, git2::BranchType::Remote, &branch)
        .into_iter()
        .filter(|branch| !branch.name.ends_with("/HEAD"))
        .collect::<Vec<_>>();
    GitStatusSnapshot {
        branch,
        upstream,
        ahead,
        behind,
        staged,
        unstaged,
        untracked,
        commits: git2_commit_log(repo, 24),
        branches,
        remote_branches: remote_branch_summaries
            .into_iter()
            .map(|branch| branch.name)
            .collect(),
        remotes: git2_remotes(repo),
        is_repository: true,
        error: None,
    }
}

fn git_review_from_repo(repo: &GitRepository, base_branch: Option<&str>) -> GitReviewSummary {
    let root = repo_root(repo);
    let base = base_branch
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "current branch")
        .map(str::to_string);

    if let Some(base) = base {
        let files = git2_commit_review_files(repo, &base).unwrap_or_default();
        let diff_stat = review_diff_stat(&files);
        return GitReviewSummary {
            mode: "taskBranch".to_string(),
            title: "Worktree Review".to_string(),
            base_branch: Some(base),
            diff_stat,
            files,
            is_repository: true,
            error: None,
        };
    }

    let changed_files = flatten_status_files(repo);
    let stats = working_tree_review_stats_git2(repo);
    let mut seen_paths = HashSet::new();
    let mut files = Vec::new();
    for file in changed_files.iter().filter(|file| {
        let index = file.index_status.trim();
        !index.is_empty() && index != "?"
    }) {
        push_review_file_from_status(&mut files, &mut seen_paths, file, "staged", &stats, root);
    }
    for file in changed_files
        .iter()
        .filter(|file| !is_untracked_status(file) && !file.worktree_status.trim().is_empty())
    {
        push_review_file_from_status(&mut files, &mut seen_paths, file, "modified", &stats, root);
    }
    for file in changed_files
        .iter()
        .filter(|file| is_untracked_status(file))
    {
        push_review_file_from_status(&mut files, &mut seen_paths, file, "added", &stats, root);
    }

    GitReviewSummary {
        mode: "workingTreeAudit".to_string(),
        title: "Uncommitted Audit".to_string(),
        base_branch: None,
        diff_stat: if files.is_empty() {
            String::new()
        } else {
            format!("{} changed files", files.len())
        },
        files,
        is_repository: true,
        error: None,
    }
}

fn flatten_status_files(repo: &GitRepository) -> Vec<GitFileStatus> {
    let (staged, unstaged, untracked) = git2_status_files(repo);
    flatten_unique_status_files(staged, unstaged, untracked)
}

fn flatten_path_status_files(repo: &GitRepository, directory_path: &str) -> Vec<GitFileStatus> {
    let (staged, unstaged, untracked) = git2_path_status_files(repo, directory_path);
    flatten_unique_status_files(staged, unstaged, untracked)
}

fn collapse_path_status_files(
    files: Vec<GitFileStatus>,
    directory_path: &str,
) -> Vec<GitFileStatus> {
    let base_path = directory_path.trim_matches('/');
    let mut collapsed = Vec::new();
    let mut directory_markers = HashMap::<String, GitFileStatus>::new();

    for file in files {
        let Some(relative_path) = relative_git_path(base_path, &file.path) else {
            continue;
        };
        let relative_path = relative_path.trim_end_matches('/');
        if relative_path.is_empty() {
            continue;
        }

        if let Some((directory_name, _rest)) = relative_path.split_once('/') {
            let path = join_git_path(base_path, directory_name);
            let marker_path = format!("{}/", path.trim_end_matches('/'));
            for marker in git_status_directory_markers(marker_path, &file) {
                directory_markers.entry(git_status_marker_key(&marker)).or_insert(marker);
            }
        } else {
            collapsed.push(file);
        }
    }

    collapsed.extend(directory_markers.into_values());
    collapsed.sort_by(|left, right| {
        left.path
            .to_lowercase()
            .cmp(&right.path.to_lowercase())
            .then_with(|| left.index_status.cmp(&right.index_status))
            .then_with(|| left.worktree_status.cmp(&right.worktree_status))
    });
    collapsed
}

fn relative_git_path<'a>(base_path: &str, file_path: &'a str) -> Option<&'a str> {
    if base_path.is_empty() {
        return Some(file_path);
    }
    file_path
        .strip_prefix(base_path)
        .and_then(|path| path.strip_prefix('/'))
}

fn join_git_path(base_path: &str, name: &str) -> String {
    if base_path.is_empty() {
        name.to_string()
    } else {
        format!("{base_path}/{name}")
    }
}

fn git_status_directory_markers(path: String, file: &GitFileStatus) -> Vec<GitFileStatus> {
    if is_untracked_status(file) {
        return vec![GitFileStatus {
            path,
            index_status: "?".to_string(),
            worktree_status: "?".to_string(),
        }];
    }

    vec![GitFileStatus {
        path,
        index_status: file.index_status.clone(),
        worktree_status: file.worktree_status.clone(),
    }]
}

fn git_status_marker_key(file: &GitFileStatus) -> String {
    format!(
        "{}\0{}\0{}",
        file.path, file.index_status, file.worktree_status
    )
}

fn flatten_unique_status_files(
    staged: Vec<GitFileStatus>,
    unstaged: Vec<GitFileStatus>,
    untracked: Vec<GitFileStatus>,
) -> Vec<GitFileStatus> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for file in staged.into_iter().chain(unstaged).chain(untracked) {
        if seen.insert(file.path.clone()) {
            files.push(file);
        }
    }
    files.sort_by(|left, right| left.path.to_lowercase().cmp(&right.path.to_lowercase()));
    files
}

fn git2_status_files(
    repo: &GitRepository,
) -> (Vec<GitFileStatus>, Vec<GitFileStatus>, Vec<GitFileStatus>) {
    git2_status_files_with_options(repo, false, None, MAX_GIT_STATUS_FILES)
}

fn git2_path_status_files(
    repo: &GitRepository,
    directory_path: &str,
) -> (Vec<GitFileStatus>, Vec<GitFileStatus>, Vec<GitFileStatus>) {
    git2_status_files_with_options(
        repo,
        true,
        Some(directory_path),
        MAX_GIT_PATH_STATUS_FILES,
    )
}

fn git2_status_files_with_options(
    repo: &GitRepository,
    recurse_untracked_dirs: bool,
    directory_path: Option<&str>,
    max_files: usize,
) -> (Vec<GitFileStatus>, Vec<GitFileStatus>, Vec<GitFileStatus>) {
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(recurse_untracked_dirs)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    if let Some(directory_path) = directory_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        options.pathspec(directory_path);
        options.pathspec(format!("{}/**", directory_path.trim_end_matches('/')));
    }

    let statuses = match repo.statuses(Some(&mut options)) {
        Ok(statuses) => statuses,
        Err(_) => return (Vec::new(), Vec::new(), Vec::new()),
    };

    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();
    for entry in statuses.iter() {
        let status = entry.status();
        let Ok(path) = entry.path().map(normalize_git_path) else {
            continue;
        };
        if is_codux_managed_memory_entrypoint(repo, &path) {
            continue;
        }
        let index_status = git2_index_status_code(status);
        let worktree_status = git2_worktree_status_code(status);
        let file = GitFileStatus {
            path,
            index_status: index_status.clone(),
            worktree_status: worktree_status.clone(),
        };
        if status.contains(git2::Status::WT_NEW) && index_status.trim().is_empty() {
            untracked.push(file);
            continue;
        }
        if !index_status.trim().is_empty() {
            staged.push(file.clone());
        }
        if !worktree_status.trim().is_empty() {
            unstaged.push(file);
        }
        if staged.len() + unstaged.len() + untracked.len() >= max_files {
            break;
        }
    }
    (staged, unstaged, untracked)
}
