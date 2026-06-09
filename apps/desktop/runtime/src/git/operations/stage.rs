fn stage_paths_git2(repo: &GitRepository, paths: &[String]) -> Result<(), String> {
    let mut index = repo.index().map_err(|error| error.message().to_string())?;
    if paths.is_empty() {
        index
            .add_all(
                ["*"].iter(),
                git2::IndexAddOption::DEFAULT,
                Some(&mut |path, _| {
                    if is_codux_managed_memory_entrypoint_path(repo, path) {
                        1
                    } else {
                        0
                    }
                }),
            )
            .map_err(|error| error.message().to_string())?;
        remove_codux_managed_memory_entrypoint_from_index(repo, &mut index);
    } else {
        for path in normalized_pathspecs(paths) {
            if is_codux_managed_memory_entrypoint(repo, &path) {
                let _ = index.remove_path(Path::new(&path));
                continue;
            }
            if repo_root(repo).join(&path).exists() {
                index
                    .add_path(Path::new(&path))
                    .map_err(|error| error.message().to_string())?;
            } else {
                let _ = index.remove_path(Path::new(&path));
            }
        }
    }
    index.write().map_err(|error| error.message().to_string())
}

fn unstage_paths_git2(repo: &GitRepository, paths: &[String]) -> Result<(), String> {
    if let Ok(head) = repo.head() {
        let target = head
            .peel(git2::ObjectType::Commit)
            .map_err(|error| error.message().to_string())?;
        let pathspecs = if paths.is_empty() {
            vec![".".to_string()]
        } else {
            normalized_pathspecs(paths)
        };
        repo.reset_default(Some(&target), pathspecs.iter().map(String::as_str))
            .map_err(|error| error.message().to_string())
    } else {
        let mut index = repo.index().map_err(|error| error.message().to_string())?;
        if paths.is_empty() {
            index.clear().map_err(|error| error.message().to_string())?;
        } else {
            for path in normalized_pathspecs(paths) {
                let _ = index.remove_path(Path::new(&path));
            }
        }
        index.write().map_err(|error| error.message().to_string())
    }
}

fn discard_paths_git2(repo: &GitRepository, paths: &[String]) -> Result<(), String> {
    unstage_paths_git2(repo, paths)?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout
        .force()
        .remove_untracked(true)
        .recreate_missing(true);
    if !paths.is_empty() {
        checkout.disable_pathspec_match(true);
        for path in normalized_pathspecs(paths) {
            checkout.path(path);
        }
    }
    repo.checkout_head(Some(&mut checkout))
        .map_err(|error| error.message().to_string())
}
