fn create_commit_git2(
    repo: &GitRepository,
    message: &str,
    amend: bool,
) -> Result<git2::Oid, String> {
    let mut index = repo.index().map_err(|error| error.message().to_string())?;
    if index.has_conflicts() {
        return Err("Cannot commit while the index has conflicts.".to_string());
    }
    let tree_id = index
        .write_tree()
        .map_err(|error| error.message().to_string())?;
    let tree = repo
        .find_tree(tree_id)
        .map_err(|error| error.message().to_string())?;
    if !commit_tree_has_changes(repo, &tree) {
        return Err("No staged changes to commit.".to_string());
    }
    let signature = repo_signature(repo)?;
    let parents = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .into_iter()
        .collect::<Vec<_>>();
    if amend {
        let parent = parents
            .first()
            .ok_or_else(|| "No commit to amend.".to_string())?;
        let parent_refs = (0..parent.parent_count())
            .filter_map(|index| parent.parent(index).ok())
            .collect::<Vec<_>>();
        let parent_refs = parent_refs.iter().collect::<Vec<_>>();
        return repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parent_refs,
            )
            .map_err(|error| error.message().to_string());
    }
    let parent_refs = parents.iter().collect::<Vec<_>>();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parent_refs,
    )
    .map_err(|error| error.message().to_string())
}

fn soft_reset_to_parent_git2(repo: &GitRepository) -> Result<(), String> {
    let head = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(|error| error.message().to_string())?;
    let parent = head
        .parent(0)
        .map_err(|error| error.message().to_string())?;
    repo.reset(parent.as_object(), git2::ResetType::Soft, None)
        .map_err(|error| error.message().to_string())
}

fn commit_tree_has_changes(repo: &GitRepository, tree: &git2::Tree<'_>) -> bool {
    if let Ok(head) = repo.head().and_then(|head| head.peel_to_commit()) {
        return head.tree_id() != tree.id();
    }
    !tree.is_empty()
}

fn repo_signature(repo: &GitRepository) -> Result<git2::Signature<'_>, String> {
    repo.signature()
        .or_else(|_| git2::Signature::now("Codux", "codux@example.local"))
        .map_err(|error| error.message().to_string())
}
