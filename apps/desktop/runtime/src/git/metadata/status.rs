fn git2_index_status_code(status: git2::Status) -> String {
    if status.contains(git2::Status::INDEX_NEW) {
        "A"
    } else if status.contains(git2::Status::INDEX_MODIFIED) {
        "M"
    } else if status.contains(git2::Status::INDEX_DELETED) {
        "D"
    } else if status.contains(git2::Status::INDEX_RENAMED) {
        "R"
    } else if status.contains(git2::Status::INDEX_TYPECHANGE) {
        "T"
    } else {
        " "
    }
    .to_string()
}

fn git2_worktree_status_code(status: git2::Status) -> String {
    if status.contains(git2::Status::WT_NEW) {
        "?"
    } else if status.contains(git2::Status::WT_MODIFIED) {
        "M"
    } else if status.contains(git2::Status::WT_DELETED) {
        "D"
    } else if status.contains(git2::Status::WT_RENAMED) {
        "R"
    } else if status.contains(git2::Status::WT_TYPECHANGE) {
        "T"
    } else {
        " "
    }
    .to_string()
}

fn is_untracked_status(file: &GitFileStatus) -> bool {
    file.worktree_status == "?" && (file.index_status == "?" || file.index_status.trim().is_empty())
}

fn is_untracked_path_git2(repo: &GitRepository, path: &str) -> bool {
    if let Ok(status) = repo.status_file(Path::new(path)) {
        return status.contains(git2::Status::WT_NEW);
    }
    flatten_path_status_files(
        repo,
        path.rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or(""),
    )
    .iter()
    .any(|file| file.path == path && is_untracked_status(file))
}
