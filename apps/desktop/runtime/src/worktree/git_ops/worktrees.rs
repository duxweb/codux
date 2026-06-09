pub(super) fn removable_worktree_branch(root_path: &str, worktree_path: &str) -> Option<String> {
    let default_branch = current_branch(root_path);
    let branch = current_branch(worktree_path)?;
    if default_branch.as_deref() == Some(branch.as_str()) {
        return None;
    }
    Some(branch)
}

pub(super) fn remove_worktree_with_git2(
    root_path: &str,
    worktree_path: &str,
) -> Result<(), String> {
    let repo = GitRepository::discover(root_path).map_err(|error| error.message().to_string())?;
    let target_path = normalize_path(worktree_path);
    let names = repo
        .worktrees()
        .map_err(|error| error.message().to_string())?;
    for name in names.iter().flatten().flatten() {
        let worktree = repo
            .find_worktree(name)
            .map_err(|error| error.message().to_string())?;
        if normalize_path(&worktree.path().to_string_lossy()) != target_path {
            continue;
        }
        if Path::new(&target_path).exists() {
            fs::remove_dir_all(&target_path).map_err(|error| error.to_string())?;
        }
        let mut options = git2::WorktreePruneOptions::new();
        options.valid(true);
        return worktree
            .prune(Some(&mut options))
            .map_err(|error| error.message().to_string());
    }
    Err("Worktree not found.".to_string())
}

pub(super) fn create_worktree_with_git2(
    root_path: &str,
    branch: &str,
    destination: &Path,
    base: Option<&str>,
) -> Result<(), String> {
    let repo = GitRepository::discover(root_path).map_err(|error| error.message().to_string())?;
    let base_commit = match base {
        Some(base) => repo
            .revparse_single(base)
            .and_then(|object| object.peel_to_commit())
            .map_err(|error| error.message().to_string())?,
        None => repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|error| error.message().to_string())?,
    };
    let mut created_branch = false;
    match repo.find_branch(branch, git2::BranchType::Local) {
        Ok(_) => {}
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            repo.branch(branch, &base_commit, false)
                .map_err(|error| error.message().to_string())?;
            created_branch = true;
        }
        Err(error) => return Err(error.message().to_string()),
    }
    let reference_name = format!("refs/heads/{branch}");
    let reference = repo
        .find_reference(&reference_name)
        .map_err(|error| error.message().to_string())?;
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    match repo.worktree(&worktree_slug(branch), destination, Some(&options)) {
        Ok(_) => Ok(()),
        Err(error) => {
            if created_branch {
                if let Ok(mut local_branch) = repo.find_branch(branch, git2::BranchType::Local) {
                    let _ = local_branch.delete();
                }
            }
            Err(error.message().to_string())
        }
    }
}

pub(super) fn managed_worktree_path(root_path: &str, branch: &str) -> PathBuf {
    PathBuf::from(root_path)
        .join(".codux")
        .join("worktrees")
        .join(worktree_slug(branch))
}

pub(super) fn worktree_slug(branch_name: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in branch_name.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        format!("worktree-{}", now_seconds())
    } else {
        slug
    }
}

pub(super) fn worktree_uuid(project_id: &str, path: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("codux:worktree:{project_id}:{path}").as_bytes(),
    )
    .to_string()
}

pub(super) fn worktree_display_name(branch: &str, path: &str) -> String {
    let branch = branch.trim();
    if !branch.is_empty() && branch != "detached HEAD" {
        return branch
            .split('/')
            .next_back()
            .filter(|value| !value.is_empty())
            .unwrap_or(branch)
            .to_string();
    }
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Worktree")
        .to_string()
}
