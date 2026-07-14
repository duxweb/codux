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
    fn workspace_snapshot_excludes_committed_changes_from_uncommitted_stats() {
        let repo = temp_dir("git-workspace-snapshot");
        GitService::init(repo.to_str().expect("repo")).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        let mut config = repository.config().expect("config");
        config
            .set_str("user.email", "codux@example.test")
            .expect("email");
        config.set_str("user.name", "Codux").expect("name");
        fs::write(repo.join("tracked.txt"), "first\n").expect("initial file");
        GitService::stage_file(repo.to_str().expect("repo"), "tracked.txt")
            .expect("stage initial file");
        GitService::commit_staged(repo.to_str().expect("repo"), "initial")
            .expect("initial commit");

        fs::write(repo.join("tracked.txt"), "second\nline\n").expect("committed update");
        GitService::stage_file(repo.to_str().expect("repo"), "tracked.txt")
            .expect("stage update");
        GitService::commit_staged(repo.to_str().expect("repo"), "update")
            .expect("update commit");

        let clean = GitService::workspace_snapshot(repo.to_str().expect("repo"));
        assert_eq!(clean.status.staged, 0);
        assert_eq!(clean.status.unstaged, 0);
        assert_eq!(clean.status.untracked, 0);
        assert_eq!(clean.status.additions, 0);
        assert_eq!(clean.status.deletions, 0);
        assert!(clean.review.files.is_empty());

        fs::write(repo.join("tracked.txt"), "second\nchanged\n").expect("working update");
        let dirty = GitService::workspace_snapshot(repo.to_str().expect("repo"));
        let review_additions = dirty
            .review
            .files
            .iter()
            .map(|file| file.additions)
            .sum::<i64>();
        let review_deletions = dirty
            .review
            .files
            .iter()
            .map(|file| file.deletions)
            .sum::<i64>();
        assert_eq!(dirty.status.unstaged, 1);
        assert_eq!(dirty.status.additions, review_additions);
        assert_eq!(dirty.status.deletions, review_deletions);
        assert_eq!(dirty.status.additions, 1);
        assert_eq!(dirty.status.deletions, 1);

        fs::remove_dir_all(repo).ok();
    }

    #[test]
    fn workspace_snapshot_counts_staged_then_modified_file_once() {
        let repo = temp_dir("git-workspace-staged-modified");
        GitService::init(repo.to_str().expect("repo")).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        let mut config = repository.config().expect("config");
        config
            .set_str("user.email", "codux@example.test")
            .expect("email");
        config.set_str("user.name", "Codux").expect("name");
        fs::write(repo.join("tracked.txt"), "base\n").expect("initial file");
        GitService::stage_file(repo.to_str().expect("repo"), "tracked.txt")
            .expect("stage initial file");
        GitService::commit_staged(repo.to_str().expect("repo"), "initial")
            .expect("initial commit");

        fs::write(repo.join("tracked.txt"), "staged\n").expect("staged content");
        GitService::stage_file(repo.to_str().expect("repo"), "tracked.txt")
            .expect("stage content");
        fs::write(repo.join("tracked.txt"), "final\n").expect("final content");

        let snapshot = GitService::workspace_snapshot(repo.to_str().expect("repo"));
        assert_eq!(snapshot.status.staged, 1);
        assert_eq!(snapshot.status.unstaged, 1);
        assert_eq!(snapshot.status.additions, 1);
        assert_eq!(snapshot.status.deletions, 1);
        assert_eq!(snapshot.review.files.len(), 1);
        assert_eq!(snapshot.review.files[0].additions, 1);
        assert_eq!(snapshot.review.files[0].deletions, 1);

        let content = GitService::review_file_content(
            repo.to_str().expect("repo"),
            "tracked.txt",
            None,
        );
        assert_eq!(content.deleted_lines, vec![1]);
        assert_eq!(content.added_lines, vec![1]);

        fs::remove_dir_all(repo).ok();
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
    fn stage_directory_path_recurses_into_tracked_and_untracked_children() {
        let repo = temp_dir("git-stage-dir");
        let p = repo.to_str().expect("repo").to_string();
        GitService::init(&p).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        let mut config = repository.config().expect("config");
        config
            .set_str("user.email", "codux@example.test")
            .expect("email");
        config.set_str("user.name", "Codux").expect("name");
        fs::create_dir_all(repo.join("src/nested")).expect("nested dir");
        fs::write(repo.join("src/nested/lib.rs"), "old\n").expect("tracked file");
        GitService::stage_file(&p, "src/nested/lib.rs").expect("stage nested");
        GitService::commit_staged(&p, "initial").expect("commit");

        // A tracked modification plus a brand-new untracked file, both under src/.
        fs::write(repo.join("src/nested/lib.rs"), "new\n").expect("modify tracked");
        fs::write(repo.join("src/added.rs"), "added\n").expect("untracked");

        // Staging the directory marker must recurse, not error with
        // "it is a directory" the way `index.add_path` did before.
        GitService::stage_paths(&p, &["src".to_string()]).expect("stage directory");

        let status = GitService::status(&p);
        // The collapsed src/ marker now reports a staged (index) change and no
        // remaining worktree-only change.
        assert!(
            status
                .changed_files
                .iter()
                .any(|file| file.path == "src/" && !file.index_status.trim().is_empty()),
            "directory stage should leave src/ with a staged index status: {:?}",
            status.changed_files
        );
        // Drilling in, both the tracked modification and the untracked file are staged.
        let nested =
            GitService::path_status(&p, "src/nested").expect("nested status after staging");
        assert!(
            nested
                .iter()
                .any(|file| file.path == "src/nested/lib.rs"
                    && file.index_status.trim() == "M"),
            "tracked modification should be staged: {nested:?}"
        );
        let src = GitService::path_status(&p, "src").expect("src status after staging");
        assert!(
            src.iter()
                .any(|file| file.path == "src/added.rs" && file.index_status.trim() == "A"),
            "untracked child should be staged as added: {src:?}"
        );
    }

    #[test]
    fn stage_untracked_directory_marker_with_trailing_slash() {
        let repo = temp_dir("git-stage-untracked-dir");
        let p = repo.to_str().expect("repo").to_string();
        GitService::init(&p).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        let mut config = repository.config().expect("config");
        config
            .set_str("user.email", "codux@example.test")
            .expect("email");
        config.set_str("user.name", "Codux").expect("name");
        // Need a commit so HEAD exists; the untracked dir is separate from it.
        fs::write(repo.join("seed.txt"), "seed\n").expect("seed");
        GitService::stage_file(&p, "seed.txt").expect("stage seed");
        GitService::commit_staged(&p, "seed").expect("commit seed");
        fs::create_dir_all(repo.join("bulk/inner")).expect("bulk dir");
        fs::write(repo.join("bulk/inner/a.txt"), "a\n").expect("untracked a");

        // The sidebar hands an untracked-directory marker through with its
        // trailing slash; staging it must still recurse.
        GitService::stage_paths(&p, &["bulk/".to_string()]).expect("stage untracked dir");

        let bulk = GitService::path_status(&p, "bulk/inner").expect("bulk status");
        assert!(
            bulk.iter()
                .any(|file| file.path == "bulk/inner/a.txt" && file.index_status.trim() == "A"),
            "untracked directory contents should be staged: {bulk:?}"
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

    #[test]
    fn git_watch_changed_paths_dedup_and_cap() {
        let mut changed = HashSet::new();
        push_unique_strings(
            &mut changed,
            vec!["a".to_string(), "b".to_string(), "a".to_string()],
        );
        assert_eq!(changed.len(), 2);
        let flood = (0..GIT_WATCH_MAX_CHANGED_PATHS * 2)
            .map(|index| format!("path-{index}"))
            .collect::<Vec<_>>();
        push_unique_strings(&mut changed, flood);
        assert_eq!(changed.len(), GIT_WATCH_MAX_CHANGED_PATHS);
    }

    #[test]
    fn stash_tag_and_rename_branch_round_trip() {
        let repo = temp_dir("git-stash-tag");
        let path = repo.to_str().expect("repo");
        GitService::init(path).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        let mut config = repository.config().expect("config");
        config.set_str("user.email", "codux@example.test").expect("email");
        config.set_str("user.name", "Codux").expect("name");
        fs::write(repo.join("a.txt"), "one\n").expect("file");
        GitService::stage_file(path, "a.txt").expect("stage");
        GitService::commit_staged(path, "initial").expect("commit");

        // Stash push / list / pop.
        fs::write(repo.join("a.txt"), "two\n").expect("modify");
        GitService::stash_push(path, Some("wip"), false).expect("stash push");
        let status = GitService::status(path);
        assert_eq!(status.stashes.len(), 1);
        assert!(status.stashes[0].message.contains("wip"));
        GitService::stash_pop(path, 0).expect("stash pop");
        assert!(GitService::status(path).stashes.is_empty());
        assert_eq!(fs::read_to_string(repo.join("a.txt")).expect("read"), "two\n");

        // Tag create / list / delete.
        GitService::create_tag(path, "v1.0.0", None).expect("create tag");
        let tagged = GitService::status(path);
        assert_eq!(tagged.tags, vec!["v1.0.0".to_string()]);
        assert_eq!(
            tagged.commits.first().and_then(|c| c.decorations.clone()),
            Some("v1.0.0".to_string())
        );
        GitService::delete_tag(path, "v1.0.0").expect("delete tag");
        assert!(GitService::status(path).tags.is_empty());

        // Rename branch.
        let current = GitService::status(path).branch;
        GitService::rename_branch(path, &current, "renamed-branch").expect("rename");
        assert_eq!(GitService::status(path).branch, "renamed-branch");
    }

    #[test]
    fn amend_last_commit_message_works_without_staged_changes() {
        let repo = temp_dir("git-amend-message-only");
        let path = repo.to_str().expect("repo");
        GitService::init(path).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        let mut config = repository.config().expect("config");
        config
            .set_str("user.email", "codux@example.test")
            .expect("email");
        config.set_str("user.name", "Codux").expect("name");
        fs::write(repo.join("a.txt"), "one\n").expect("file");
        GitService::stage_file(path, "a.txt").expect("stage");
        GitService::commit_staged(path, "initial").expect("commit");

        GitService::amend_last_commit_message(path, "renamed commit").expect("amend message");

        assert_eq!(
            GitService::last_commit_message(path).expect("last message"),
            "renamed commit"
        );
        assert!(GitService::status(path).changed_files.is_empty());
    }

    #[test]
    fn init_existing_repository_is_ok() {
        let repo = temp_dir("git-init-existing");
        let path = repo.to_str().expect("repo");
        GitService::init(path).expect("init repo");

        GitService::init(path).expect("init existing repo");

        assert!(GitService::status(path).is_repository);
    }

    #[test]
    fn rebase_replays_branch_commits_onto_target() {
        let repo = temp_dir("git-rebase");
        let path = repo.to_str().expect("repo");
        GitService::init(path).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        let mut config = repository.config().expect("config");
        config.set_str("user.email", "codux@example.test").expect("email");
        config.set_str("user.name", "Codux").expect("name");
        fs::write(repo.join("base.txt"), "base\n").expect("file");
        GitService::stage_file(path, "base.txt").expect("stage");
        GitService::commit_staged(path, "base").expect("commit");
        let main = GitService::status(path).branch;

        GitService::create_branch(path, "feature", None, true).expect("create feature");
        fs::write(repo.join("feature.txt"), "feature\n").expect("file");
        GitService::stage_file(path, "feature.txt").expect("stage");
        GitService::commit_staged(path, "feature work").expect("commit");

        GitService::checkout_branch(path, &main).expect("checkout main");
        fs::write(repo.join("main.txt"), "main\n").expect("file");
        GitService::stage_file(path, "main.txt").expect("stage");
        GitService::commit_staged(path, "main work").expect("commit");

        GitService::checkout_branch(path, "feature").expect("checkout feature");
        GitService::rebase_branch(path, &main).expect("rebase");

        // Rebased feature has main's tip as ancestor and both files present.
        let feature_tip = repository.revparse_single("feature").expect("feature").id();
        let main_tip = repository.revparse_single(&main).expect("main").id();
        assert!(
            repository
                .graph_descendant_of(feature_tip, main_tip)
                .expect("graph")
        );
        assert!(repo.join("feature.txt").exists());
        assert!(repo.join("main.txt").exists());
    }

    #[test]
    fn remote_branch_and_tag_ops_round_trip_with_local_bare_remote() {
        let remote_dir = temp_dir("git-remote-bare");
        GitRepository::init_bare(&remote_dir).expect("bare remote");
        let repo = temp_dir("git-remote-work");
        let path = repo.to_str().expect("repo");
        GitService::init(path).expect("init repo");
        let repository = GitRepository::open(&repo).expect("open repo");
        let mut config = repository.config().expect("config");
        config.set_str("user.email", "codux@example.test").expect("email");
        config.set_str("user.name", "Codux").expect("name");
        fs::write(repo.join("a.txt"), "one\n").expect("file");
        GitService::stage_file(path, "a.txt").expect("stage");
        GitService::commit_staged(path, "initial").expect("commit");
        let main = GitService::status(path).branch;

        let remote_url = remote_dir.to_str().expect("remote path");
        GitService::add_remote(path, "origin", remote_url).expect("add remote");
        GitService::push_remote_branch(path, &format!("origin/{main}"), None).expect("push main");
        GitService::push_remote_branch(path, "origin/feature", Some(&main)).expect("push feature");
        GitService::fetch(path).expect("fetch");
        let feature_ref = "origin/feature".to_string();
        assert!(GitService::status(path).remote_branches.contains(&feature_ref));

        // Delete the remote branch, then prune the stale tracking ref.
        GitService::delete_remote_branch(path, "origin/feature").expect("delete remote branch");
        GitService::fetch_prune(path).expect("fetch prune");
        assert!(!GitService::status(path).remote_branches.contains(&feature_ref));

        // Tags: push to the bare remote, then delete there.
        let bare_tags = || {
            GitRepository::open(&remote_dir)
                .expect("open bare")
                .tag_names(None)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(|name| name.ok().flatten().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        GitService::create_tag(path, "v1.0.0", Some("release")).expect("create tag");
        GitService::push_tags(path, Some("origin")).expect("push tags");
        assert_eq!(bare_tags(), vec!["v1.0.0".to_string()]);
        GitService::delete_remote_tag(path, Some("origin"), "v1.0.0").expect("delete remote tag");
        assert!(bare_tags().is_empty());
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
