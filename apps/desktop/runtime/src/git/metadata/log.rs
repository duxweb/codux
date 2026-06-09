fn git2_commit_log(repo: &GitRepository, limit: usize) -> Vec<GitCommitSummary> {
    let mut revwalk = match repo.revwalk() {
        Ok(revwalk) => revwalk,
        Err(_) => return Vec::new(),
    };
    let _ = revwalk.set_sorting(git2::Sort::TIME);
    if revwalk.push_head().is_err() {
        return Vec::new();
    }
    revwalk
        .take(limit)
        .filter_map(Result::ok)
        .filter_map(|oid| {
            let commit = repo.find_commit(oid).ok()?;
            Some(GitCommitSummary {
                hash: short_oid(oid),
                title: commit.summary().ok().flatten().unwrap_or("").to_string(),
                relative_time: relative_git_time(commit.time().seconds()),
                decorations: None,
                graph_prefix: String::new(),
                author: commit.author().name().unwrap_or("").to_string(),
            })
        })
        .collect()
}

fn relative_git_time(seconds: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let delta = (now - seconds).max(0);
    if delta < 60 {
        "just now".to_string()
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86_400 {
        format!("{}h ago", delta / 3600)
    } else if delta < 2_592_000 {
        format!("{}d ago", delta / 86_400)
    } else if delta < 31_536_000 {
        format!("{}mo ago", delta / 2_592_000)
    } else {
        format!("{}y ago", delta / 31_536_000)
    }
}

fn short_oid(oid: git2::Oid) -> String {
    oid.to_string().chars().take(7).collect()
}
