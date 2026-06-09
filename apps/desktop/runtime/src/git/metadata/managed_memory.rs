fn remove_codux_managed_memory_entrypoint_from_index(
    repo: &GitRepository,
    index: &mut git2::Index,
) {
    let path = "AGENTS.md";
    if is_codux_managed_memory_entrypoint(repo, path) {
        let _ = index.remove_path(Path::new(path));
    }
}

fn is_codux_managed_memory_entrypoint_path(repo: &GitRepository, path: &Path) -> bool {
    path.to_str()
        .map(normalize_git_path)
        .is_some_and(|path| is_codux_managed_memory_entrypoint(repo, &path))
}

fn is_codux_managed_memory_entrypoint(repo: &GitRepository, path: &str) -> bool {
    if path != "AGENTS.md" {
        return false;
    }
    if head_contains_path(repo, path) {
        return false;
    }
    let full_path = repo_root(repo).join(path);
    is_codux_managed_memory_entrypoint_file(&full_path)
}

fn head_contains_path(repo: &GitRepository, path: &str) -> bool {
    head_tree(repo)
        .ok()
        .and_then(|tree| tree.get_path(Path::new(path)).ok())
        .is_some()
}

fn is_codux_managed_memory_entrypoint_file(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::read_link(path)
            .ok()
            .and_then(|target| target.to_str().map(str::to_string))
            .is_some_and(|target| {
                let normalized = target.replace('\\', "/");
                normalized.ends_with("/AGENTS.md")
                    && normalized.contains("/runtime-root/memory-workspaces/")
            }),
        Ok(metadata) if metadata.is_file() => fs::read_to_string(path)
            .ok()
            .and_then(|text| text.lines().next().map(str::to_string))
            .is_some_and(|line| line.trim() == CODUX_MANAGED_MEMORY_ENTRYPOINT_MARKER),
        _ => false,
    }
}
