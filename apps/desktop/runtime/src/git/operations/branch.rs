fn checkout_branch_git2(repo: &GitRepository, branch: &str) -> Result<(), String> {
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

fn create_branch_git2(
    repo: &GitRepository,
    branch: &str,
    from: Option<&str>,
    checkout: bool,
) -> Result<(), String> {
    let commit = match from.map(str::trim).filter(|value| !value.is_empty()) {
        Some(from) => repo
            .revparse_single(from)
            .and_then(|object| object.peel_to_commit())
            .map_err(|error| error.message().to_string())?,
        None => repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|error| error.message().to_string())?,
    };
    repo.branch(branch, &commit, false)
        .map_err(|error| error.message().to_string())?;
    if checkout {
        checkout_branch_git2(repo, branch)?;
    }
    Ok(())
}

fn checkout_remote_branch_git2(
    repo: &GitRepository,
    remote_branch: &str,
    local_name: &str,
) -> Result<(), String> {
    let remote_ref = format!("refs/remotes/{remote_branch}");
    let commit = repo
        .find_reference(&remote_ref)
        .and_then(|reference| reference.peel_to_commit())
        .map_err(|error| error.message().to_string())?;
    let mut branch = repo
        .branch(local_name, &commit, false)
        .map_err(|error| error.message().to_string())?;
    branch
        .set_upstream(Some(remote_branch))
        .map_err(|error| error.message().to_string())?;
    checkout_branch_git2(repo, local_name)
}

fn checkout_commit_git2(repo: &GitRepository, reference: &str) -> Result<(), String> {
    let object = repo
        .revparse_single(reference)
        .map_err(|error| error.message().to_string())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(&object, Some(&mut checkout))
        .map_err(|error| error.message().to_string())?;
    repo.set_head_detached(object.id())
        .map_err(|error| error.message().to_string())
}

fn hard_reset_git2(repo: &GitRepository, reference: &str) -> Result<(), String> {
    let object = repo
        .revparse_single(reference)
        .map_err(|error| error.message().to_string())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force().remove_untracked(true);
    repo.reset(&object, git2::ResetType::Hard, Some(&mut checkout))
        .map_err(|error| error.message().to_string())
}

fn delete_branch_git2(repo: &GitRepository, branch: &str, force: bool) -> Result<(), String> {
    if current_branch_name(repo) == branch {
        return Err(format!("Cannot delete the checked out branch: {branch}"));
    }
    let mut local_branch = repo
        .find_branch(branch, git2::BranchType::Local)
        .map_err(|error| error.message().to_string())?;
    if !force {
        let head = repo.head().ok().and_then(|head| head.target());
        let target = local_branch.get().target();
        if let (Some(head), Some(target)) = (head, target) {
            if !repo.graph_descendant_of(head, target).unwrap_or(false) {
                return Err(format!("Branch {branch} is not fully merged."));
            }
        }
    }
    local_branch
        .delete()
        .map_err(|error| error.message().to_string())
}

fn revert_commit_git2(repo: &GitRepository, reference: &str) -> Result<(), String> {
    let commit = repo
        .revparse_single(reference)
        .and_then(|object| object.peel_to_commit())
        .map_err(|error| error.message().to_string())?;
    repo.revert(&commit, None)
        .map_err(|error| error.message().to_string())?;
    if repo
        .index()
        .map_err(|error| error.message().to_string())?
        .has_conflicts()
    {
        return Err("Revert produced conflicts. Resolve them manually.".to_string());
    }
    let summary = commit.summary().ok().flatten().unwrap_or(reference);
    create_commit_git2(repo, &format!("Revert \"{summary}\""), false)?;
    repo.cleanup_state()
        .map_err(|error| error.message().to_string())
}

fn merge_branch_git2(repo: &GitRepository, branch: &str, squash: bool) -> Result<(), String> {
    let annotated = annotated_commit_for_branch(repo, branch)?;
    let (analysis, _) = repo
        .merge_analysis(&[&annotated])
        .map_err(|error| error.message().to_string())?;
    if analysis.is_up_to_date() {
        return Ok(());
    }
    if analysis.is_fast_forward() && !squash {
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
    if !squash {
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
    }
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
