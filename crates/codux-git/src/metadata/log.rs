fn git2_commit_log(repo: &GitRepository, limit: usize) -> Vec<GitCommitSummary> {
    let mut revwalk = match repo.revwalk() {
        Ok(revwalk) => revwalk,
        Err(_) => return Vec::new(),
    };
    let _ = revwalk.set_sorting(git2::Sort::TIME);
    if revwalk.push_head().is_err() {
        return Vec::new();
    }
    let tags_by_commit = git2_tags_by_commit(repo);
    revwalk
        .take(limit)
        .filter_map(Result::ok)
        .filter_map(|oid| {
            let commit = repo.find_commit(oid).ok()?;
            Some(GitCommitSummary {
                hash: short_oid(oid),
                title: commit.summary().ok().flatten().unwrap_or("").to_string(),
                relative_time: relative_git_time(commit.time().seconds()),
                decorations: tags_by_commit.get(&oid).map(|names| names.join(",")),
                graph_prefix: String::new(),
                author: commit.author().name().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Tag names per target commit (annotated tags peeled to the commit).
fn git2_tags_by_commit(repo: &GitRepository) -> HashMap<git2::Oid, Vec<String>> {
    let mut map: HashMap<git2::Oid, Vec<String>> = HashMap::new();
    let Ok(references) = repo.references_glob("refs/tags/*") else {
        return map;
    };
    for reference in references.filter_map(Result::ok) {
        let Ok(name) = reference.shorthand().map(str::to_string) else {
            continue;
        };
        let Ok(commit) = reference.peel_to_commit() else {
            continue;
        };
        map.entry(commit.id()).or_default().push(name);
    }
    map
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
