fn git2_remotes(repo: &GitRepository) -> Vec<GitRemoteSummary> {
    let mut remotes = Vec::new();
    let Ok(names) = repo.remotes() else {
        return remotes;
    };
    for name in names.iter().flatten().flatten() {
        if let Ok(remote) = repo.find_remote(name) {
            remotes.push(GitRemoteSummary {
                name: name.to_string(),
                url: remote.url().unwrap_or("").to_string(),
            });
        }
    }
    remotes.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    remotes
}
