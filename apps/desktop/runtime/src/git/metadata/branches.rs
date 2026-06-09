fn current_branch_name(repo: &GitRepository) -> String {
    repo.head()
        .ok()
        .and_then(|head| {
            if head.is_branch() {
                head.shorthand().ok().map(str::to_string)
            } else {
                head.target().map(short_oid)
            }
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "HEAD".to_string())
}

fn repository_root(project_path: &str) -> Result<String, String> {
    let repo = open_git_repository(project_path)?;
    Ok(repo_root(&repo).display().to_string())
}

fn upstream_branch_name(repo: &GitRepository) -> Option<String> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    let name = head.shorthand().ok()?;
    repo.find_branch(name, git2::BranchType::Local)
        .ok()
        .and_then(|branch| branch.upstream().ok())
        .and_then(|branch| branch.name().ok().flatten().map(str::to_string))
}

fn ahead_behind(repo: &GitRepository) -> Option<(i64, i64)> {
    let head = repo.head().ok()?.target()?;
    let upstream = {
        let head_ref = repo.head().ok()?;
        if !head_ref.is_branch() {
            return Some((0, 0));
        }
        let name = head_ref.shorthand().ok()?;
        repo.find_branch(name, git2::BranchType::Local)
            .ok()?
            .upstream()
            .ok()?
            .get()
            .target()?
    };
    repo.graph_ahead_behind(head, upstream)
        .ok()
        .map(|(ahead, behind)| (ahead as i64, behind as i64))
}

fn git2_branches(
    repo: &GitRepository,
    branch_type: git2::BranchType,
    current: &str,
) -> Vec<GitBranchSummary> {
    let mut branches = Vec::new();
    let Ok(iter) = repo.branches(Some(branch_type)) else {
        return branches;
    };
    for item in iter.filter_map(Result::ok) {
        let branch = item.0;
        let name = branch
            .name()
            .ok()
            .flatten()
            .map(str::to_string)
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        branches.push(GitBranchSummary {
            is_current: branch_type == git2::BranchType::Local && name == current,
            name,
        });
    }
    branches.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    if branch_type == git2::BranchType::Local {
        ensure_current_local_branch(branches, current)
    } else {
        branches
    }
}

fn ensure_current_local_branch(
    mut branches: Vec<GitBranchSummary>,
    current: &str,
) -> Vec<GitBranchSummary> {
    if !current.is_empty()
        && current != "HEAD"
        && !branches.iter().any(|branch| branch.name == current)
    {
        branches.insert(
            0,
            GitBranchSummary {
                name: current.to_string(),
                is_current: true,
            },
        );
    }
    branches
}
