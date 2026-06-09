pub(super) fn mergeable_branch(current: Option<&str>, fallback: &str) -> Option<String> {
    let branch = current
        .map(str::trim)
        .filter(|branch| !branch.is_empty() && *branch != "HEAD")
        .unwrap_or_else(|| fallback.trim());
    if branch.is_empty() || branch == "detached HEAD" || branch.starts_with("detached ") {
        None
    } else {
        Some(branch.to_string())
    }
}

pub(super) fn delete_local_branch(root_path: &str, branch: &str) -> Result<(), String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Ok(());
    }
    let repo = GitRepository::discover(root_path).map_err(|error| error.message().to_string())?;
    if current_branch_from_repo(&repo).as_deref() == Some(branch) {
        return Err(format!("Cannot delete the checked out branch: {branch}"));
    }
    match repo.find_branch(branch, git2::BranchType::Local) {
        Ok(mut local_branch) => local_branch
            .delete()
            .map_err(|error| error.message().to_string()),
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(()),
        Err(error) => Err(error.message().to_string()),
    }
}

pub(super) fn checkout_branch_git2(repo: &GitRepository, branch: &str) -> Result<(), String> {
    let reference_name = if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    };
    let reference = repo
        .find_reference(&reference_name)
        .map_err(|error| error.message().to_string())?;
    let object = reference
        .peel(git2::ObjectType::Commit)
        .map_err(|error| error.message().to_string())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(&object, Some(&mut checkout))
        .map_err(|error| error.message().to_string())?;
    repo.set_head(&reference_name)
        .map_err(|error| error.message().to_string())
}

pub(super) fn merge_branch_git2(repo: &GitRepository, branch: &str) -> Result<(), String> {
    let annotated = annotated_commit_for_branch(repo, branch)?;
    let (analysis, _) = repo
        .merge_analysis(&[&annotated])
        .map_err(|error| error.message().to_string())?;
    if analysis.is_up_to_date() {
        return Ok(());
    }
    if analysis.is_fast_forward() {
        fast_forward_head(repo, annotated.id())?;
        return Ok(());
    }
    let head_commit = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(|error| error.message().to_string())?;
    let their_commit = repo
        .find_commit(annotated.id())
        .map_err(|error| error.message().to_string())?;
    let mut index = repo
        .merge_commits(&head_commit, &their_commit, None)
        .map_err(|error| error.message().to_string())?;
    if index.has_conflicts() {
        repo.checkout_index(Some(&mut index), None)
            .map_err(|error| error.message().to_string())?;
        return Err("Merge produced conflicts. Resolve them manually.".to_string());
    }
    let tree_id = index
        .write_tree_to(repo)
        .map_err(|error| error.message().to_string())?;
    let tree = repo
        .find_tree(tree_id)
        .map_err(|error| error.message().to_string())?;
    repo.checkout_tree(
        tree.as_object(),
        Some(git2::build::CheckoutBuilder::new().safe()),
    )
    .map_err(|error| error.message().to_string())?;
    repo.index()
        .and_then(|mut repo_index| {
            repo_index.read_tree(&tree)?;
            repo_index.write()
        })
        .map_err(|error| error.message().to_string())?;
    let signature = repo_signature(repo)?;
    let message = format!("Merge branch '{branch}'");
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &message,
        &tree,
        &[&head_commit, &their_commit],
    )
    .map_err(|error| error.message().to_string())?;
    repo.cleanup_state()
        .map_err(|error| error.message().to_string())
}

fn annotated_commit_for_branch<'repo>(
    repo: &'repo GitRepository,
    branch: &str,
) -> Result<git2::AnnotatedCommit<'repo>, String> {
    let object = repo
        .revparse_single(branch)
        .or_else(|_| repo.revparse_single(&format!("refs/heads/{branch}")))
        .map_err(|error| error.message().to_string())?;
    repo.find_annotated_commit(object.id())
        .map_err(|error| error.message().to_string())
}

fn fast_forward_head(repo: &GitRepository, target: git2::Oid) -> Result<(), String> {
    let head_name = repo
        .head()
        .ok()
        .and_then(|head| head.name().ok().map(str::to_string))
        .ok_or_else(|| "Cannot fast-forward detached HEAD.".to_string())?;
    let mut reference = repo
        .find_reference(&head_name)
        .map_err(|error| error.message().to_string())?;
    reference
        .set_target(target, "Fast-forward")
        .map_err(|error| error.message().to_string())?;
    repo.set_head(&head_name)
        .map_err(|error| error.message().to_string())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))
        .map_err(|error| error.message().to_string())
}

fn repo_signature(repo: &GitRepository) -> Result<git2::Signature<'_>, String> {
    repo.signature()
        .or_else(|_| git2::Signature::now("Codux", "codux@example.local"))
        .map_err(|error| error.message().to_string())
}
