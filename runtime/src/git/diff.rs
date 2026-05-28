#[derive(Clone, Copy)]
enum DiffTarget {
    Index,
    Worktree,
}

fn git2_diff_to_string(
    repo: &GitRepository,
    target: DiffTarget,
    path: Option<&str>,
    context_lines: u32,
) -> Result<String, String> {
    let tree = head_tree(repo).ok();
    let mut options = git2_diff_options(path, context_lines);
    let diff = match target {
        DiffTarget::Index => repo.diff_tree_to_index(tree.as_ref(), None, Some(&mut options)),
        DiffTarget::Worktree => repo.diff_index_to_workdir(None, Some(&mut options)),
    }
    .map_err(|error| error.message().to_string())?;
    diff_to_string(&diff)
}

fn git2_commit_diff_to_string(
    repo: &GitRepository,
    base: &str,
    path: Option<&str>,
    context_lines: u32,
) -> Result<String, String> {
    let base_tree = resolve_commit_tree(repo, base)?;
    let head_tree = head_tree(repo)?;
    let mut options = git2_diff_options(path, context_lines);
    let diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut options))
        .map_err(|error| error.message().to_string())?;
    diff_to_string(&diff)
}

fn git2_commit_review_files(
    repo: &GitRepository,
    base: &str,
) -> Result<Vec<GitReviewFile>, String> {
    let base_tree = resolve_commit_tree(repo, base)?;
    let head_tree = head_tree(repo)?;
    let mut diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)
        .map_err(|error| error.message().to_string())?;
    let _ = diff.find_similar(None);
    review_files_from_diff(&diff)
}

fn working_tree_review_stats_git2(repo: &GitRepository) -> HashMap<String, (i64, i64)> {
    let mut stats = HashMap::new();
    if let Ok(diff) = diff_for_review_stats(repo, DiffTarget::Index) {
        merge_review_stats_from_diff(&mut stats, &diff);
    }
    if let Ok(diff) = diff_for_review_stats(repo, DiffTarget::Worktree) {
        merge_review_stats_from_diff(&mut stats, &diff);
    }
    stats
}

fn diff_for_review_stats(
    repo: &GitRepository,
    target: DiffTarget,
) -> Result<git2::Diff<'_>, String> {
    let tree = head_tree(repo).ok();
    let diff = match target {
        DiffTarget::Index => repo.diff_tree_to_index(tree.as_ref(), None, None),
        DiffTarget::Worktree => repo.diff_index_to_workdir(None, None),
    }
    .map_err(|error| error.message().to_string())?;
    Ok(diff)
}

fn merge_review_stats_from_diff(target: &mut HashMap<String, (i64, i64)>, diff: &git2::Diff<'_>) {
    for file in review_files_from_diff(diff).unwrap_or_default() {
        let entry = target.entry(file.path).or_insert((0, 0));
        entry.0 += file.additions;
        entry.1 += file.deletions;
    }
}

fn review_files_from_diff(diff: &git2::Diff<'_>) -> Result<Vec<GitReviewFile>, String> {
    let mut files = Vec::new();
    for index in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(index) else {
            continue;
        };
        let Some(path) = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(normalize_git_path_path)
        else {
            continue;
        };
        let (additions, deletions) = patch_line_stats(diff, index);
        files.push(GitReviewFile {
            path,
            status: review_status_from_delta(delta.status()),
            additions,
            deletions,
        });
    }
    Ok(files)
}

fn patch_line_stats(diff: &git2::Diff<'_>, index: usize) -> (i64, i64) {
    let Some(delta) = diff.get_delta(index) else {
        return (0, 0);
    };
    let Ok(Some(patch)) = git2::Patch::from_diff(diff, index) else {
        return (0, 0);
    };
    let mut additions = 0;
    let mut deletions = 0;
    for hunk_index in 0..patch.num_hunks() {
        let Ok((_hunk, line_count)) = patch.hunk(hunk_index) else {
            continue;
        };
        for line_index in 0..line_count {
            let Ok(line) = patch.line_in_hunk(hunk_index, line_index) else {
                continue;
            };
            match line.origin() {
                '+' => additions += 1,
                '-' => deletions += 1,
                _ => {}
            }
        }
    }
    if additions == 0 && deletions == 0 {
        match delta.status() {
            git2::Delta::Added => additions = 1,
            git2::Delta::Deleted => deletions = 1,
            _ => {}
        }
    }
    (additions, deletions)
}

fn git2_diff_options(path: Option<&str>, context_lines: u32) -> git2::DiffOptions {
    let mut options = git2::DiffOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(path.is_some_and(|path| !path.trim().is_empty()))
        .context_lines(context_lines);
    if let Some(path) = path.filter(|path| !path.trim().is_empty()) {
        options.pathspec(path);
    }
    options
}

fn diff_to_string(diff: &git2::Diff<'_>) -> Result<String, String> {
    let mut output = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        match line.origin() {
            '+' | '-' | ' ' => output.push(line.origin() as u8),
            _ => {}
        }
        output.extend_from_slice(line.content());
        true
    })
    .map_err(|error| error.message().to_string())?;
    Ok(String::from_utf8_lossy(&output).to_string())
}

fn compact_commit_message_diff(diff: &str) -> (String, bool) {
    let mut output = String::new();
    let mut truncated = false;
    let mut file_count = 0usize;
    let mut file_line_count = 0usize;
    let mut include_current_file = true;

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            file_count += 1;
            if file_count > COMMIT_CONTEXT_MAX_FILES {
                truncated = true;
                break;
            }
            file_line_count = 0;
            include_current_file = true;
        }

        let is_header = line.starts_with("diff --git ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("@@ ")
            || line.starts_with("new file mode ")
            || line.starts_with("deleted file mode ")
            || line.starts_with("rename from ")
            || line.starts_with("rename to ")
            || line.starts_with("Binary files ");

        if !is_header {
            file_line_count += 1;
            if file_line_count > COMMIT_CONTEXT_MAX_LINES_PER_FILE {
                if include_current_file {
                    push_commit_context_line(&mut output, "... file diff truncated ...");
                    include_current_file = false;
                    truncated = true;
                }
                continue;
            }
        }

        if output.len() + line.len() + 1 > COMMIT_CONTEXT_MAX_CHARS {
            truncated = true;
            break;
        }
        push_commit_context_line(&mut output, line);
    }

    if truncated {
        push_commit_context_line(&mut output, "... diff truncated for token budget ...");
    }
    (output, truncated)
}

fn push_commit_context_line(output: &mut String, line: &str) {
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str(line);
}

fn head_tree(repo: &GitRepository) -> Result<git2::Tree<'_>, String> {
    let head = repo.head().map_err(|error| error.message().to_string())?;
    let commit = head
        .peel_to_commit()
        .map_err(|error| error.message().to_string())?;
    commit.tree().map_err(|error| error.message().to_string())
}

fn resolve_commit_tree<'repo>(
    repo: &'repo GitRepository,
    reference: &str,
) -> Result<git2::Tree<'repo>, String> {
    let object = repo
        .revparse_single(reference)
        .map_err(|_| format!("Cannot resolve git reference: {reference}"))?;
    let commit = object
        .peel_to_commit()
        .map_err(|error| error.message().to_string())?;
    commit.tree().map_err(|error| error.message().to_string())
}

fn git2_blob_or_empty(repo: &GitRepository, reference: &str, path: &str) -> String {
    git2_blob(repo, reference, path).unwrap_or_default()
}

fn git2_blob(repo: &GitRepository, reference: &str, path: &str) -> Result<String, String> {
    let tree = resolve_commit_tree(repo, reference)?;
    let entry = tree
        .get_path(Path::new(path))
        .map_err(|error| error.message().to_string())?;
    let object = entry
        .to_object(repo)
        .map_err(|error| error.message().to_string())?;
    let blob = object
        .as_blob()
        .ok_or_else(|| "Git object is not a blob.".to_string())?;
    Ok(String::from_utf8_lossy(blob.content()).to_string())
}

fn git2_index_blob(repo: &GitRepository, path: &str) -> Result<String, String> {
    let index = repo.index().map_err(|error| error.message().to_string())?;
    let Some(entry) = index.get_path(Path::new(path), 0) else {
        return Err("File is not in the index.".to_string());
    };
    let blob = repo
        .find_blob(entry.id)
        .map_err(|error| error.message().to_string())?;
    Ok(String::from_utf8_lossy(blob.content()).to_string())
}

fn read_worktree_file(root: &Path, path: &str) -> Result<String, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let full_path = root.join(path);
    let full_path = full_path.canonicalize().unwrap_or(full_path);
    if !full_path.starts_with(&root) {
        return Err("File path escapes repository root.".to_string());
    }
    fs::read_to_string(full_path).map_err(|error| error.to_string())
}

fn parse_diff_line_numbers(diff: &str) -> (Vec<usize>, Vec<usize>) {
    let mut deleted = Vec::new();
    let mut added = Vec::new();
    let mut old_line = 0usize;
    let mut new_line = 0usize;
    for line in diff.lines() {
        if let Some((old_start, new_start)) = parse_hunk_header(line) {
            old_line = old_start;
            new_line = new_start;
            continue;
        }
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            added.push(new_line);
            new_line += 1;
        } else if line.starts_with('-') {
            deleted.push(old_line);
            old_line += 1;
        } else if line.starts_with(' ') || line.is_empty() {
            old_line += 1;
            new_line += 1;
        }
    }
    (deleted, added)
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    if !line.starts_with("@@ ") {
        return None;
    }
    let mut parts = line.split_whitespace();
    parts.next()?;
    let old = parts.next()?.trim_start_matches('-');
    let new = parts.next()?.trim_start_matches('+');
    Some((parse_hunk_start(old)?, parse_hunk_start(new)?))
}

fn parse_hunk_start(value: &str) -> Option<usize> {
    value
        .split(',')
        .next()
        .and_then(|value| value.parse::<usize>().ok())
}
