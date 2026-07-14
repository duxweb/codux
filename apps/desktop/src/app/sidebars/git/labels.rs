use super::*;

#[derive(Clone)]
pub(in crate::app) struct GitSidebarLabels {
    pub(super) remote_url: String,
    pub(super) cancel: String,
    pub(super) confirm: String,
    pub(super) credentials_message: String,
    pub(in crate::app) credentials_title: String,
    pub(super) credential_username: String,
    pub(super) credential_password_or_token: String,
    pub(in crate::app) auth_credentials_required: String,
    pub(super) commit_message: String,
    pub(super) commit: String,
    pub(super) commit_push: String,
    pub(super) commit_sync: String,
    pub(super) load_last_commit_message: String,
    pub(super) amend_last_commit: String,
    pub(super) undo_last_commit: String,
    pub(super) no_branch: String,
    pub(super) no_repository: String,
    pub(super) no_repository_description: String,
    pub(super) init_repository: String,
    pub(super) trust_directory_title: String,
    pub(super) trust_directory_description: String,
    pub(super) trust_directory_action: String,
    pub(in crate::app) clone_repository: String,
    pub(super) clone_preparing: String,
    pub(super) staged: String,
    pub(super) staged_empty: String,
    pub(super) changed: String,
    pub(super) changed_empty: String,
    pub(super) untracked: String,
    pub(super) untracked_empty: String,
    pub(super) history: String,
    pub(super) history_empty: String,
    pub(super) tree_limit: String,
    pub(super) stage: String,
    pub(super) unstage: String,
    pub(super) open_diff: String,
    pub(super) discard_changes: String,
    pub(super) add_gitignore: String,
    pub(super) no_selected_file: String,
    pub(super) empty_diff: String,
    pub(super) diff_current: String,
    pub(super) checkout_commit: String,
    pub(super) revert_commit: String,
    pub(super) restore_commit: String,
    pub(super) review_changed_files: String,
    pub(super) review_original: String,
    pub(super) review_select_file: String,
    pub(super) review_empty: String,
    pub(super) review_no_repository: String,
    pub(super) review_tree_limit: String,
}
impl GitSidebarLabels {
    pub(in crate::app) fn load(language: &str) -> Self {
        let locale = locale_from_language_setting(language);
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        Self {
            remote_url: tr("git.remote.add.url_message", "Remote Repository URL"),
            cancel: tr("common.cancel", "Cancel"),
            confirm: tr("common.confirm", "Confirm"),
            credentials_message: tr(
                "git.credentials.message",
                "Remote access requires authentication. Enter your username and password or token to retry.",
            ),
            credentials_title: tr("git.credentials.title", "Git Credentials Required"),
            credential_username: tr("git.credential.username", "Username"),
            credential_password_or_token: tr(
                "git.credential.password_or_token",
                "Password or Token",
            ),
            auth_credentials_required: tr(
                "git.auth.credentials_required",
                "Username and password or token cannot be empty.",
            ),
            commit_message: tr("git.commit.message.placeholder", "Enter Commit Message"),
            commit: tr("git.commit.action", "Commit"),
            commit_push: tr("git.commit.action_push", "Commit and Push"),
            commit_sync: tr("git.commit.action_sync", "Commit and Sync"),
            load_last_commit_message: tr(
                "git.history.edit_last_commit_message",
                "Load Last Commit Message",
            ),
            amend_last_commit: tr(
                "git.history.edit_last_commit_message",
                "Edit Last Commit Message",
            ),
            undo_last_commit: tr("git.history.undo_last_commit", "Undo Last Commit"),
            no_branch: tr("git.branch.none", "No Branch"),
            no_repository: tr("git.empty.no_repository", "No Repository"),
            no_repository_description: tr(
                "git.empty.description",
                "Initialize Git or clone from a remote URL.",
            ),
            init_repository: tr("git.empty.initialize_repository", "Initialize Repository"),
            trust_directory_title: tr("git.trust.title", "Trust Project Directory"),
            trust_directory_description: tr(
                "git.trust.description",
                "Git blocked this repository because the directory owner is different. Trust this project directory to enable Git features.",
            ),
            trust_directory_action: tr("git.trust.action", "Trust Directory"),
            clone_repository: tr(
                "git.empty.clone_remote_repository",
                "Clone Remote Repository",
            ),
            clone_preparing: tr("git.clone.preparing", "Preparing to clone the repository"),
            staged: tr("git.files.staged", "Staged"),
            staged_empty: tr("git.files.staged.empty", "No staged changes"),
            changed: tr("git.files.changes", "Changes"),
            changed_empty: tr("git.files.changes.empty", "No worktree changes"),
            untracked: tr("git.files.untracked", "Untracked"),
            untracked_empty: tr("git.files.untracked.empty", "No untracked files"),
            history: tr("git.history.title", "Git History"),
            history_empty: tr("git.history.empty", "No commit history"),
            tree_limit: tr(
                "git.files.tree_limit_format",
                "Showing the first %@ items. Expand a directory to view its children.",
            ),
            stage: tr("git.files.stage", "Stage"),
            unstage: tr("git.files.unstage", "Unstage"),
            open_diff: tr("git.diff.open", "Open Diff"),
            discard_changes: tr("git.files.discard_changes", "Discard Changes"),
            add_gitignore: tr("git.ignore.add", "Add to .gitignore"),
            no_selected_file: tr("git.diff.select_file", "Select a file to view its diff."),
            empty_diff: tr("git.diff.empty", "No Diff to Display"),
            diff_current: tr("git.diff.column.current", "Current File"),
            checkout_commit: tr("git.history.checkout_commit", "Checkout This Commit"),
            revert_commit: tr("git.history.revert_commit", "Revert This Commit"),
            restore_commit: tr("git.history.restore_local", "Restore to This Commit"),
            review_changed_files: tr("worktree.review.changed_files", "Changed Files"),
            review_original: tr("worktree.review.column.original", "Original"),
            review_select_file: tr(
                "worktree.review.select_file",
                "Select a changed file to compare.",
            ),
            review_empty: tr(
                "worktree.review.no_changes",
                "No changes relative to the base branch.",
            ),
            review_no_repository: tr("worktree.review.no_repository", "No Git repository."),
            review_tree_limit: tr(
                "worktree.review.tree_limit_format",
                "Showing first %@ rows of %@ changed files",
            ),
        }
    }
}

pub(in crate::app) struct GitSectionInput<'a> {
    pub(in crate::app) git: &'a GitSummary,
    pub(in crate::app) default_push_remote: Option<&'a str>,
    pub(in crate::app) language: &'a str,
    pub(in crate::app) running_operation: Option<&'a GitRunningOperation>,
    pub(in crate::app) commit_message: &'a str,
    pub(in crate::app) commit_message_revision: u64,
    pub(in crate::app) files_panel_view: gpui::Entity<GitFilesPanelView>,
    pub(in crate::app) history_panel_view: gpui::Entity<GitHistoryPanelView>,
}

pub(in crate::app) fn git_section(
    input: GitSectionInput<'_>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let GitSectionInput {
        git,
        default_push_remote,
        language,
        running_operation,
        commit_message,
        commit_message_revision,
        files_panel_view,
        history_panel_view,
    } = input;
    let labels = Rc::new(GitSidebarLabels::load(language));
    let branch = if git.branch.trim().is_empty() {
        labels.no_branch.as_str()
    } else {
        git.branch.as_str()
    };

    div()
        .flex()
        .flex_1()
        .h_full()
        .min_h_0()
        .flex_col()
        .when(git.is_repository, |this| {
            this.child(git_panel_header(
                git,
                branch,
                default_push_remote,
                language,
                running_operation,
                cx,
            ))
        })
        .child(if git.is_repository {
            git_repository_panel(
                labels.clone(),
                commit_message,
                commit_message_revision,
                files_panel_view,
                history_panel_view,
                window,
                cx,
            )
            .into_any_element()
        } else {
            git_empty_repository_panel(git, labels, running_operation, cx).into_any_element()
        })
        .into_any_element()
}
