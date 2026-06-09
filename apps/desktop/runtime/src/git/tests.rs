#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn git_file(path: &str, index_status: &str, worktree_status: &str) -> GitFileStatus {
        GitFileStatus {
            path: path.to_string(),
            index_status: index_status.to_string(),
            worktree_status: worktree_status.to_string(),
        }
    }

    #[test]
    fn reads_modified_and_untracked_file_diffs() {
        let repo = temp_dir("git-diff");
        GitService::init(repo.to_str().expect("repo")).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        let mut config = repository.config().expect("config");
        config
            .set_str("user.email", "codux@example.test")
            .expect("email");
        config.set_str("user.name", "Codux").expect("name");
        fs::write(repo.join("tracked.txt"), "old\n").expect("tracked file");
        GitService::stage_file(repo.to_str().expect("repo"), "tracked.txt").expect("stage file");
        GitService::commit_staged(repo.to_str().expect("repo"), "initial").expect("commit");

        fs::write(repo.join("tracked.txt"), "new\n").expect("modify tracked");
        fs::write(repo.join("untracked.txt"), "hello\n").expect("untracked file");

        let modified = GitService::file_diff(repo.to_str().expect("repo"), "tracked.txt")
            .expect("modified diff");
        assert!(modified.contains("--- unstaged ---"));
        assert!(modified.contains("-old"));
        assert!(modified.contains("+new"));

        let untracked = GitService::file_diff(repo.to_str().expect("repo"), "untracked.txt")
            .expect("untracked diff");
        assert!(untracked.contains("--- untracked ---"));
        assert!(untracked.contains("hello"));

        GitService::stage_file(repo.to_str().expect("repo"), "tracked.txt").expect("stage file");
        let staged_status = GitService::status(repo.to_str().expect("repo"));
        let tracked = staged_status
            .changed_files
            .iter()
            .find(|file| file.path == "tracked.txt")
            .expect("tracked staged status");
        assert_eq!(tracked.index_status, "M");
        assert_eq!(tracked.worktree_status, " ");
    }

    #[test]
    fn status_collapses_untracked_directories_until_expanded() {
        let repo = temp_dir("git-untracked-dir");
        GitService::init(repo.to_str().expect("repo")).expect("init repo");
        fs::create_dir_all(repo.join("bulk/nested")).expect("bulk dir");
        fs::write(repo.join("bulk/nested/a.txt"), "a\n").expect("a");
        fs::write(repo.join("bulk/nested/b.txt"), "b\n").expect("b");

        let status = GitService::status(repo.to_str().expect("repo"));
        assert!(
            status.changed_files.iter().any(|file| file.path == "bulk/"),
            "initial status should show the untracked directory"
        );
        assert!(
            status
                .changed_files
                .iter()
                .all(|file| !file.path.starts_with("bulk/nested/")),
            "initial status should not recurse into untracked directories"
        );

        let expanded =
            GitService::path_status(repo.to_str().expect("repo"), "bulk").expect("path status");
        assert!(
            expanded.iter().any(|file| file.path == "bulk/nested/"),
            "directory expansion should expose only the next child directory"
        );
        assert!(
            expanded
                .iter()
                .all(|file| !file.path.starts_with("bulk/nested/a.txt")),
            "directory expansion should not recurse into nested children"
        );

        let nested =
            GitService::path_status(repo.to_str().expect("repo"), "bulk/nested").expect("nested");
        assert!(
            nested.iter().any(|file| file.path == "bulk/nested/a.txt"),
            "nested directory expansion should load its direct files"
        );
    }

    #[test]
    fn status_collapses_nested_tracked_changes_until_expanded() {
        let repo = temp_dir("git-tracked-dir");
        GitService::init(repo.to_str().expect("repo")).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        let mut config = repository.config().expect("config");
        config
            .set_str("user.email", "codux@example.test")
            .expect("email");
        config.set_str("user.name", "Codux").expect("name");
        fs::create_dir_all(repo.join("src/nested")).expect("nested dir");
        fs::write(repo.join("src/nested/lib.rs"), "old\n").expect("tracked file");
        GitService::stage_file(repo.to_str().expect("repo"), "src/nested/lib.rs")
            .expect("stage file");
        GitService::commit_staged(repo.to_str().expect("repo"), "initial").expect("commit");

        fs::write(repo.join("src/nested/lib.rs"), "new\n").expect("modify tracked");

        let status = GitService::status(repo.to_str().expect("repo"));
        assert!(
            status.changed_files.iter().any(|file| file.path == "src/"),
            "initial status should expose the root directory marker"
        );
        assert!(
            status
                .changed_files
                .iter()
                .all(|file| file.path != "src/nested/lib.rs"),
            "initial status should not expose nested tracked files"
        );

        let src = GitService::path_status(repo.to_str().expect("repo"), "src").expect("src");
        assert!(
            src.iter().any(|file| file.path == "src/nested/"),
            "expanding src should expose the next child directory"
        );

        let nested =
            GitService::path_status(repo.to_str().expect("repo"), "src/nested").expect("nested");
        assert!(
            nested.iter().any(|file| file.path == "src/nested/lib.rs"),
            "expanding nested should expose the changed file"
        );
    }

    #[test]
    fn path_status_keeps_directory_markers_per_status_kind() {
        let files = vec![
            git_file("src/shared/tracked.rs", "M", " "),
            git_file("src/shared/new.rs", " ", "?"),
        ];

        let collapsed = collapse_path_status_files(files, "src");
        assert!(collapsed.iter().any(|file| {
            file.path == "src/shared/"
                && file.index_status == "M"
                && file.worktree_status.trim().is_empty()
        }));
        assert!(collapsed.iter().any(|file| {
            file.path == "src/shared/" && file.index_status == "?" && file.worktree_status == "?"
        }));
    }

    #[test]
    fn review_collapses_untracked_directories_but_file_diff_still_loads_nested_file() {
        let repo = temp_dir("git-review-untracked-dir");
        GitService::init(repo.to_str().expect("repo")).expect("init repo");
        fs::create_dir_all(repo.join("bulk/nested")).expect("bulk dir");
        fs::write(repo.join("bulk/nested/a.txt"), "a\n").expect("a");

        let review = GitService::review(repo.to_str().expect("repo"), None);
        assert!(
            review.files.iter().any(|file| file.path == "bulk/"),
            "review should show the untracked directory"
        );
        assert!(
            review
                .files
                .iter()
                .all(|file| !file.path.starts_with("bulk/nested/")),
            "review should not recurse into untracked directories"
        );

        let diff = GitService::review_file_diff(
            repo.to_str().expect("repo"),
            "bulk/nested/a.txt",
            None,
        )
        .expect("nested untracked file diff");
        assert!(diff.contains("--- untracked ---"));
        assert!(diff.contains("a"));
    }

    #[test]
    fn review_file_content_marks_untracked_file_lines_as_added() {
        let repo = temp_dir("git-review-untracked-content");
        GitService::init(repo.to_str().expect("repo")).expect("init repo");
        fs::write(repo.join("new.txt"), "one\ntwo\n").expect("new file");

        let content = GitService::review_file_content(
            repo.to_str().expect("repo"),
            "new.txt",
            None,
        );

        assert_eq!(content.path, "new.txt");
        assert_eq!(content.head_content, "");
        assert_eq!(content.index_content, None);
        assert_eq!(content.worktree_content, "one\ntwo\n");
        assert_eq!(content.deleted_lines, Vec::<usize>::new());
        assert_eq!(content.added_lines, vec![1, 2]);
        assert_eq!(content.error, None);
    }

    #[test]
    fn git_watch_filter_allows_worktree_and_known_metadata() {
        let repository = "/repo/app";
        let git_dirs = vec!["/repo/app/.git".to_string()];

        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/src/main.rs"
        ));
        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/HEAD"
        ));
        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/index"
        ));
        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/refs/heads/main"
        ));
        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/logs/HEAD"
        ));
        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/FETCH_HEAD"
        ));
    }

    #[test]
    fn git_watch_filter_ignores_git_object_churn() {
        let repository = "/repo/app";
        let git_dirs = vec!["/repo/app/.git".to_string()];

        assert!(!should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git"
        ));
        assert!(!should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/objects/ab/cdef"
        ));
        assert!(!should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/modules/dependency/config"
        ));
    }

    #[test]
    fn git_watcher_path_set_keeps_other_worktrees_when_one_is_removed() {
        let mut paths = HashSet::from([
            "/repo/app".to_string(),
            "/repo/app/.codux/worktrees/task-a".to_string(),
        ]);

        let empty = remove_watched_project_path(
            &mut paths,
            &normalized_path_key(Path::new("/repo/app/.codux/worktrees/task-a")),
        );

        assert!(!empty);
        assert_eq!(paths, HashSet::from(["/repo/app".to_string()]));
    }

    #[test]
    fn git_watcher_path_set_reports_empty_after_last_path_is_removed() {
        let mut paths = HashSet::from(["/repo/app".to_string()]);

        let empty =
            remove_watched_project_path(&mut paths, &normalized_path_key(Path::new("/repo/app")));

        assert!(empty);
        assert!(paths.is_empty());
    }

    #[test]
    fn cancellable_git_commands_pass_cancel_token_to_git2_operations() {
        let repo = temp_dir("git-cancel");
        GitService::init(repo.to_str().expect("repo")).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        repository
            .remote("origin", "https://example.invalid/codux.git")
            .expect("remote");
        let cancelled = Arc::new(AtomicBool::new(true));
        let repo_path = repo.to_string_lossy().to_string();

        assert_cancelled(git_fetch_with_cancel(
            repo_path.clone(),
            Some(Arc::clone(&cancelled)),
        ));
        assert_cancelled(git_pull_with_cancel(
            repo_path.clone(),
            Some(Arc::clone(&cancelled)),
        ));
        assert_cancelled(git_push_with_cancel(
            repo_path.clone(),
            Some(Arc::clone(&cancelled)),
        ));
        assert_cancelled(git_force_push_with_cancel(
            repo_path.clone(),
            Some(Arc::clone(&cancelled)),
        ));
        assert_cancelled(git_sync_with_cancel(
            repo_path.clone(),
            Some(Arc::clone(&cancelled)),
        ));
        assert_cancelled(git_push_remote_with_cancel(
            GitPushRemoteRequest {
                project_path: repo_path.clone(),
                remote: "origin".to_string(),
            },
            Some(Arc::clone(&cancelled)),
        ));
        assert_cancelled(git_push_remote_branch_with_cancel(
            GitPushRemoteBranchRequest {
                project_path: repo_path,
                remote_branch: "origin/main".to_string(),
                local_branch: Some("main".to_string()),
            },
            Some(cancelled),
        ));
    }

    fn assert_cancelled(result: Result<GitStatusSnapshot, String>) {
        assert_eq!(result.expect_err("operation should cancel"), "Git operation cancelled.");
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("codux-gpui-{label}-{nanos}"));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }
}
