fn review_status_from_delta(delta: git2::Delta) -> String {
    match delta {
        git2::Delta::Added => "added",
        git2::Delta::Deleted => "deleted",
        git2::Delta::Renamed => "renamed",
        git2::Delta::Copied => "copied",
        git2::Delta::Typechange => "typeChanged",
        _ => "modified",
    }
    .to_string()
}

fn review_diff_stat(files: &[GitReviewFile]) -> String {
    if files.is_empty() {
        return String::new();
    }
    let additions: i64 = files.iter().map(|file| file.additions).sum();
    let deletions: i64 = files.iter().map(|file| file.deletions).sum();
    format!(
        "{} changed files, {} insertions(+), {} deletions(-)",
        files.len(),
        additions,
        deletions
    )
}

fn normalize_git_path_path(path: &Path) -> String {
    normalize_git_path(&path.to_string_lossy())
}

fn push_review_file_from_status(
    files: &mut Vec<GitReviewFile>,
    seen_paths: &mut HashSet<String>,
    file: &GitFileStatus,
    fallback: &str,
    stats: &HashMap<String, (i64, i64)>,
    root: &Path,
) {
    if !seen_paths.insert(file.path.clone()) {
        return;
    }
    let mut review_file = review_file_from_status(file, fallback, stats);
    if is_untracked_status(file) && review_file.additions == 0 {
        review_file.additions = count_untracked_file_lines(root, &file.path).unwrap_or(0);
    }
    files.push(review_file);
}

fn review_file_from_status(
    file: &GitFileStatus,
    fallback: &str,
    stats: &HashMap<String, (i64, i64)>,
) -> GitReviewFile {
    let status = if is_untracked_status(file) {
        "added".to_string()
    } else {
        review_status(
            file.worktree_status
                .trim()
                .chars()
                .next()
                .or_else(|| file.index_status.trim().chars().next())
                .map(|value| value.to_string())
                .as_deref()
                .unwrap_or(fallback),
        )
    };
    let (additions, deletions) = stats.get(&file.path).copied().unwrap_or((0, 0));
    GitReviewFile {
        path: file.path.clone(),
        status,
        additions,
        deletions,
    }
}

fn review_status(value: &str) -> String {
    match value.chars().next().unwrap_or('M') {
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        'T' => "typeChanged",
        '?' => "added",
        _ => "modified",
    }
    .to_string()
}

fn count_untracked_file_lines(root: &Path, path: &str) -> Option<i64> {
    let root = root.canonicalize().ok()?;
    let full_path = root.join(path).canonicalize().ok()?;
    if !full_path.starts_with(&root) || !full_path.is_file() {
        return None;
    }
    let metadata = fs::metadata(&full_path).ok()?;
    if metadata.len() > REVIEW_UNTRACKED_LINE_COUNT_LIMIT_BYTES {
        return None;
    }
    let data = fs::read(full_path).ok()?;
    if data.contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(&data);
    Some(text.lines().count() as i64)
}
