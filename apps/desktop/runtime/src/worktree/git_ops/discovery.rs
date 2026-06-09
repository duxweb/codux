pub(super) fn current_branch(project_path: &str) -> Option<String> {
    GitRepository::discover(project_path)
        .ok()
        .as_ref()
        .and_then(current_branch_from_repo)
}

pub(super) fn repository_root(project_path: &str) -> Option<String> {
    GitRepository::discover(project_path)
        .ok()
        .and_then(|repo| repo_root(&repo).map(|path| normalize_path(&path.to_string_lossy())))
}

pub(super) fn list_git_worktrees(root_path: &str) -> Result<Vec<GitWorktreeEntry>, String> {
    let mut entries = Vec::new();
    let repo = GitRepository::discover(root_path).map_err(|error| error.message().to_string())?;
    let names = repo
        .worktrees()
        .map_err(|error| error.message().to_string())?;
    for name in names.iter().flatten().flatten() {
        let Ok(worktree) = repo.find_worktree(name) else {
            continue;
        };
        let path = normalize_path(&worktree.path().to_string_lossy());
        let worktree_repo = GitRepository::open(worktree.path()).ok();
        let branch = worktree_repo
            .as_ref()
            .and_then(current_branch_from_repo)
            .unwrap_or_default();
        let head = worktree_repo
            .as_ref()
            .and_then(head_oid_from_repo)
            .unwrap_or_default();
        let detached = worktree_repo
            .as_ref()
            .map(|repo| repo.head().map(|head| !head.is_branch()).unwrap_or(false))
            .unwrap_or(false);
        let bare = worktree_repo
            .as_ref()
            .map(|repo| repo.is_bare())
            .unwrap_or(false);
        entries.push(GitWorktreeEntry {
            path,
            branch,
            head,
            detached,
            bare,
        });
    }
    Ok(entries)
}

pub(super) fn has_head_commit(root_path: &str) -> bool {
    GitRepository::discover(root_path)
        .ok()
        .map(|repo| {
            repo.head()
                .ok()
                .and_then(|head| head.peel_to_commit().ok())
                .is_some()
        })
        .unwrap_or(false)
}

pub(super) fn commit_hash(root_path: &str, ref_name: &str) -> Option<String> {
    let ref_name = ref_name.trim();
    if ref_name.is_empty() {
        return None;
    }
    GitRepository::discover(root_path).ok().and_then(|repo| {
        repo.revparse_single(ref_name)
            .ok()?
            .peel_to_commit()
            .ok()
            .map(|commit| commit.id().to_string())
    })
}

pub(super) fn current_branch_from_repo(repo: &GitRepository) -> Option<String> {
    repo.head()
        .ok()
        .and_then(|head| {
            if head.is_branch() {
                head.shorthand().ok().map(str::to_string)
            } else {
                None
            }
        })
        .filter(|value| !value.trim().is_empty())
}

fn repo_root(repo: &GitRepository) -> Option<&Path> {
    repo.workdir().or_else(|| repo.path().parent())
}

fn head_oid_from_repo(repo: &GitRepository) -> Option<String> {
    repo.head()
        .ok()
        .and_then(|head| head.target())
        .map(|oid| oid.to_string())
}
