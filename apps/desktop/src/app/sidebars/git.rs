use super::*;
use crate::app::ui_helpers::codux_tooltip_container;
use codux_runtime::git::GitReviewFile;
use gpui::{Animation, AnimationExt as _, ClickEvent, Div, ListSizingBehavior, Pixels, Stateful};
use gpui_component::input::{Input, InputEvent, InputState};
use std::{ops::Range, time::Duration};

#[derive(Clone)]
pub(in crate::app) struct GitSidebarLabels {
    remote_name: String,
    remote_url: String,
    add_remote: String,
    add: String,
    cancel: String,
    confirm: String,
    credentials_message: String,
    pub(in crate::app) credentials_title: String,
    credential_username: String,
    credential_password_or_token: String,
    pub(in crate::app) auth_credentials_required: String,
    commit_message: String,
    commit: String,
    commit_push: String,
    commit_sync: String,
    load_last_commit_message: String,
    amend_last_commit: String,
    undo_last_commit: String,
    no_branch: String,
    no_repository: String,
    no_repository_description: String,
    init_repository: String,
    pub(in crate::app) clone_repository: String,
    clone_preparing: String,
    staged: String,
    staged_empty: String,
    changed: String,
    changed_empty: String,
    untracked: String,
    untracked_empty: String,
    history: String,
    history_empty: String,
    tree_limit: String,
    stage: String,
    unstage: String,
    open_diff: String,
    discard_changes: String,
    add_gitignore: String,
    no_selected_file: String,
    empty_diff: String,
    diff_current: String,
    checkout_commit: String,
    revert_commit: String,
    restore_commit: String,
    review_changed_files: String,
    review_original: String,
    review_new_file: String,
    review_final_file: String,
    review_branch: String,
    review_select_file: String,
    review_empty: String,
    review_no_repository: String,
    review_tree_limit: String,
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct GitFilesPanelSnapshot {
    language: String,
    branch: String,
    changed_files: Vec<(String, String, String)>,
    expanded_sections: Vec<String>,
    expanded_dirs: Vec<String>,
    tree_children: Vec<(String, Vec<(String, String, String)>)>,
    selected_file: Option<String>,
    selected_files: Vec<String>,
}

pub(in crate::app) struct GitFilesPanelView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: GitFilesPanelSnapshot,
}

impl GitFilesPanelView {
    fn set_snapshot(&mut self, snapshot: GitFilesPanelSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for GitFilesPanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = self.app_entity.clone();
        app_entity.update(cx, |app, cx| {
            let labels = Rc::new(GitSidebarLabels::load(&app.state.settings.language));
            let staged = app
                .state
                .git
                .changed_files
                .iter()
                .filter(|file| is_git_staged_file(file))
                .cloned()
                .collect::<Vec<_>>();
            let changed = app
                .state
                .git
                .changed_files
                .iter()
                .filter(|file| is_git_worktree_file(file))
                .cloned()
                .collect::<Vec<_>>();
            let untracked = app
                .state
                .git
                .changed_files
                .iter()
                .filter(|file| is_git_untracked_file(file))
                .cloned()
                .collect::<Vec<_>>();

            git_files_panel(
                &staged,
                &changed,
                &untracked,
                &app.git_expanded_sections,
                &app.git_expanded_dirs,
                &app.git_tree_children,
                app.selected_git_file.as_deref(),
                &app.selected_git_files,
                labels,
                app.git_files_scroll_handle.clone(),
                cx,
            )
            .into_any_element()
        })
    }
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct GitHistoryPanelSnapshot {
    language: String,
    commits: Vec<(String, String, String, Option<String>, String, String)>,
}

pub(in crate::app) struct GitHistoryPanelView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: GitHistoryPanelSnapshot,
}

impl GitHistoryPanelView {
    fn set_snapshot(&mut self, snapshot: GitHistoryPanelSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for GitHistoryPanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = self.app_entity.clone();
        app_entity.update(cx, |app, cx| {
            let labels = Rc::new(GitSidebarLabels::load(&app.state.settings.language));
            git_history_panel(
                &app.state.git,
                labels,
                app.git_history_scroll_handle.clone(),
                cx,
            )
            .into_any_element()
        })
    }
}

impl CoduxApp {
    pub(in crate::app) fn git_files_panel_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<GitFilesPanelView> {
        let snapshot = self.git_files_panel_snapshot();
        if let Some(view) = self.git_files_panel_view.clone() {
            view.update(cx, |view: &mut GitFilesPanelView, cx| {
                view.set_snapshot(snapshot, cx)
            });
            return view;
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| GitFilesPanelView {
            app_entity,
            snapshot,
        });
        self.git_files_panel_view = Some(view.clone());
        view
    }

    pub(in crate::app) fn git_history_panel_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<GitHistoryPanelView> {
        let snapshot = self.git_history_panel_snapshot();
        if let Some(view) = self.git_history_panel_view.clone() {
            view.update(cx, |view: &mut GitHistoryPanelView, cx| {
                view.set_snapshot(snapshot, cx)
            });
            return view;
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| GitHistoryPanelView {
            app_entity,
            snapshot,
        });
        self.git_history_panel_view = Some(view.clone());
        view
    }

    fn git_files_panel_snapshot(&self) -> GitFilesPanelSnapshot {
        let mut expanded_sections = self
            .git_expanded_sections
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        expanded_sections.sort();
        let mut expanded_dirs = self.git_expanded_dirs.iter().cloned().collect::<Vec<_>>();
        expanded_dirs.sort();
        let mut selected_files = self.selected_git_files.iter().cloned().collect::<Vec<_>>();
        selected_files.sort();
        let mut tree_children = self
            .git_tree_children
            .iter()
            .map(|(path, files)| {
                (
                    path.clone(),
                    files.iter().map(git_file_status_tuple).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        tree_children.sort_by(|left, right| left.0.cmp(&right.0));

        GitFilesPanelSnapshot {
            language: self.state.settings.language.clone(),
            branch: self.state.git.branch.clone(),
            changed_files: self
                .state
                .git
                .changed_files
                .iter()
                .map(git_file_status_tuple)
                .collect(),
            expanded_sections,
            expanded_dirs,
            tree_children,
            selected_file: self.selected_git_file.clone(),
            selected_files,
        }
    }

    fn git_history_panel_snapshot(&self) -> GitHistoryPanelSnapshot {
        GitHistoryPanelSnapshot {
            language: self.state.settings.language.clone(),
            commits: self
                .state
                .git
                .commits
                .iter()
                .map(|commit| {
                    (
                        commit.hash.clone(),
                        commit.title.clone(),
                        commit.relative_time.clone(),
                        commit.decorations.clone(),
                        commit.graph_prefix.clone(),
                        commit.author.clone(),
                    )
                })
                .collect(),
        }
    }
}

fn git_file_status_tuple(file: &GitFileStatus) -> (String, String, String) {
    (
        file.path.clone(),
        file.index_status.clone(),
        file.worktree_status.clone(),
    )
}

impl GitSidebarLabels {
    pub(in crate::app) fn load(language: &str) -> Self {
        let locale = locale_from_language_setting(language);
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        Self {
            remote_name: tr("git.remote.name", "Remote Name"),
            remote_url: tr("git.remote.add.url_message", "Remote Repository URL"),
            add_remote: tr("git.remote.add", "Add Remote"),
            add: tr("common.add", "Add"),
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
            review_new_file: tr("worktree.review.column.new", "New File"),
            review_final_file: tr("worktree.review.column.final", "Final File"),
            review_branch: tr("worktree.review.column.branch", "Branch"),
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

pub(in crate::app) fn git_section(
    git: &GitSummary,
    selected_branch: Option<&str>,
    default_push_remote: Option<&str>,
    _clone_remote_url: &str,
    language: &str,
    remote_editor_open: bool,
    remote_name: &str,
    remote_url: &str,
    running_operation: Option<&GitRunningOperation>,
    commit_message: &str,
    commit_message_revision: u64,
    files_panel_view: gpui::Entity<GitFilesPanelView>,
    history_panel_view: gpui::Entity<GitHistoryPanelView>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
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
        .child(git_panel_header(
            git,
            branch,
            selected_branch,
            default_push_remote,
            language,
            running_operation,
            cx,
        ))
        .child(if git.is_repository {
            git_repository_panel(
                git,
                remote_editor_open,
                remote_name,
                remote_url,
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
            git_empty_repository_panel(labels, running_operation, cx).into_any_element()
        })
        .into_any_element()
}

fn git_panel_header(
    git: &GitSummary,
    branch: &str,
    _selected_branch: Option<&str>,
    default_push_remote: Option<&str>,
    language: &str,
    running_operation: Option<&GitRunningOperation>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let branches = git.branches.clone();
    let remote_branches = git.remote_branches.clone();
    let remotes = git.remotes.clone();
    let default_push_remote = default_push_remote.map(str::to_string);
    let language = language.to_string();
    let current_branch = branch.to_string();
    let upstream = git.upstream.clone();
    let has_commits = !git.commits.is_empty();
    let app_entity = cx.entity();

    div()
        .h(px(44.0))
        .px_3()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(color(theme::BORDER_SOFT))
        .child(
            div()
                .flex()
                .items_center()
                .min_w_0()
                .when(git.is_repository, |this| {
                    this.child(
                        Button::new("git-sidebar-branch-menu")
                            .compact()
                            .ghost()
                            .text_color(cx.theme().foreground)
                            .child(
                                div()
                                    .h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .max_w(px(132.0))
                                            .text_size(rems(0.875))
                                            .line_height(rems(1.125))
                                            .truncate()
                                            .child(branch.to_string()),
                                    )
                                    .child(
                                        Icon::new(HeroIconName::ChevronDown)
                                            .size_3()
                                            .text_color(color(theme::TEXT_DIM)),
                                    ),
                            )
                            .dropdown_menu(move |menu, window, cx| {
                                git_branch_dropdown_menu(
                                    menu,
                                    window,
                                    cx,
                                    branches.clone(),
                                    remote_branches.clone(),
                                    remotes.clone(),
                                    default_push_remote.clone(),
                                    current_branch.clone(),
                                    upstream.clone(),
                                    has_commits,
                                    language.clone(),
                                    app_entity.clone(),
                                )
                            }),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .when(git.is_repository, |this| {
                    this.child(assistant_header_icon_button(
                        "git-sidebar-ai",
                        HeroIconName::Sparkles,
                        cx,
                        |app, _event, window, cx| {
                            app.generate_git_commit_message_with_ai(window, cx)
                        },
                    ))
                })
                .when_some(running_operation, |this, operation| {
                    if operation.cancellable {
                        this.child(assistant_header_icon_button(
                            "git-sidebar-cancel",
                            HeroIconName::XCircle,
                            cx,
                            move |app, _event, window, cx| {
                                app.cancel_project_git(window, cx);
                            },
                        ))
                    } else {
                        this.child(
                            Button::new("git-sidebar-running")
                                .compact()
                                .ghost()
                                .text_color(cx.theme().secondary_foreground)
                                .icon(
                                    Icon::new(HeroIconName::ArrowPath)
                                        .size_3p5()
                                        .text_color(cx.theme().secondary_foreground),
                                ),
                        )
                    }
                }),
        )
}

fn git_branch_dropdown_menu(
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
    branches: Vec<GitBranchSummary>,
    remote_branches: Vec<String>,
    remotes: Vec<GitRemoteSummary>,
    default_push_remote: Option<String>,
    current_branch: String,
    upstream: Option<String>,
    has_commits: bool,
    language: String,
    app_entity: gpui::Entity<CoduxApp>,
) -> PopupMenu {
    let labels = Rc::new(GitBranchMenuLabels::load(&language));
    let create_entity = app_entity.clone();
    let menu = menu
        .item(
            PopupMenuItem::new(labels.new_branch.clone())
                .icon(HeroIconName::Plus)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&create_entity, |app, cx| {
                        app.create_git_branch(window, cx);
                    });
                }),
        )
        .separator();

    let local_branches = branches.clone();
    let local_entity = app_entity.clone();
    let local_labels = labels.clone();
    let menu = menu.submenu_with_icon(
        Some(Icon::new(HeroIconName::ArrowPathRoundedSquare)),
        labels.local_branches.clone(),
        window,
        cx,
        move |menu, window, cx| {
            if local_branches.is_empty() {
                return menu.item(
                    PopupMenuItem::new(local_labels.local_empty.clone())
                        .icon(HeroIconName::ArrowPathRoundedSquare)
                        .disabled(true),
                );
            }

            local_branches.iter().take(40).fold(menu, |menu, branch| {
                let branch_name = branch.name.clone();
                let is_current = branch.is_current;
                let submenu_entity = local_entity.clone();
                let submenu_labels = local_labels.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(if is_current {
                        HeroIconName::Check
                    } else {
                        HeroIconName::ArrowPathRoundedSquare
                    })),
                    branch.name.clone(),
                    window,
                    cx,
                    move |menu, _window, _cx| {
                        let switch_branch = branch_name.clone();
                        let switch_entity = submenu_entity.clone();
                        let merge_branch = branch_name.clone();
                        let merge_entity = submenu_entity.clone();
                        let squash_branch = branch_name.clone();
                        let squash_entity = submenu_entity.clone();
                        let delete_branch = branch_name.clone();
                        let delete_entity = submenu_entity.clone();

                        menu.item(
                            PopupMenuItem::new(submenu_labels.switch_branch.clone())
                                .icon(HeroIconName::Check)
                                .disabled(is_current)
                                .on_click(move |_, window, cx| {
                                    cx.update_entity(&switch_entity, |app, cx| {
                                        app.select_git_branch(switch_branch.clone(), window, cx);
                                        app.checkout_selected_git_branch(window, cx);
                                    });
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new(submenu_labels.merge_current.clone())
                                .icon(HeroIconName::ArrowUturnRight)
                                .disabled(is_current)
                                .on_click(move |_, window, cx| {
                                    cx.update_entity(&merge_entity, |app, cx| {
                                        app.merge_git_branch(merge_branch.clone(), window, cx);
                                    });
                                }),
                        )
                        .item(
                            PopupMenuItem::new(submenu_labels.squash_merge.clone())
                                .icon(HeroIconName::ArrowPath)
                                .disabled(is_current)
                                .on_click(move |_, window, cx| {
                                    cx.update_entity(&squash_entity, |app, cx| {
                                        app.squash_merge_git_branch(
                                            squash_branch.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new(submenu_labels.delete_local.clone())
                                .icon(HeroIconName::Trash)
                                .disabled(is_current)
                                .on_click(move |_, window, cx| {
                                    cx.update_entity(&delete_entity, |app, cx| {
                                        app.select_git_branch(delete_branch.clone(), window, cx);
                                        app.delete_selected_git_branch(window, cx);
                                    });
                                }),
                        )
                    },
                )
            })
        },
    );

    let merge_branches = branches.clone();
    let merge_entity = app_entity.clone();
    let merge_labels = labels.clone();
    let menu = menu.submenu(
        labels.merge_current.clone(),
        window,
        cx,
        move |menu, _window, _cx| {
            let candidates = merge_branches
                .iter()
                .filter(|branch| !branch.is_current)
                .take(40)
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return menu.item(
                    PopupMenuItem::new(merge_labels.merge_empty.clone())
                        .icon(HeroIconName::ArrowUturnRight)
                        .disabled(true),
                );
            }

            candidates.into_iter().fold(menu, |menu, branch| {
                let branch_name = branch.name.clone();
                let app_entity = merge_entity.clone();
                menu.item(
                    PopupMenuItem::new(branch.name.clone())
                        .icon(HeroIconName::ArrowUturnRight)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&app_entity, |app, cx| {
                                app.merge_git_branch(branch_name.clone(), window, cx);
                            });
                        }),
                )
            })
        },
    );

    let remote_items = remotes.clone();
    let remote_entity = app_entity.clone();
    let default_remote = default_push_remote.clone();
    let push_to_default_remote = default_push_remote.clone();
    let remote_labels = labels.clone();
    let menu = menu.submenu(
        labels.remotes.clone(),
        window,
        cx,
        move |menu, window, cx| {
            let add_entity = remote_entity.clone();
            let menu = menu.item(
                PopupMenuItem::new(remote_labels.add_remote.clone())
                    .icon(HeroIconName::Plus)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&add_entity, |app, cx| {
                            app.open_git_remote_editor(window, cx);
                        });
                    }),
            );

            if remote_items.is_empty() {
                return menu.separator().item(
                    PopupMenuItem::new(remote_labels.no_remotes.clone())
                        .icon(HeroIconName::GlobeAlt)
                        .disabled(true),
                );
            }

            remote_items.iter().fold(menu, |menu, remote| {
                let is_default = push_to_default_remote
                    .as_deref()
                    .map(|name| name == remote.name)
                    .unwrap_or(false);
                let remote_name = remote.name.clone();
                let remote_url = remote.url.clone();
                let set_entity = remote_entity.clone();
                let remove_entity = remote_entity.clone();
                let item_labels = remote_labels.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(if is_default {
                        HeroIconName::Check
                    } else {
                        HeroIconName::GlobeAlt
                    })),
                    remote.name.clone(),
                    window,
                    cx,
                    move |menu, _window, _cx| {
                        let set_remote = remote_name.clone();
                        let set_entity = set_entity.clone();
                        let remove_remote = remote_name.clone();
                        let remove_entity = remove_entity.clone();
                        let copy_url = remote_url.clone();

                        menu.item(
                            PopupMenuItem::new(item_labels.set_default.clone())
                                .icon(HeroIconName::Check)
                                .checked(is_default)
                                .on_click(move |_, window, cx| {
                                    let next_remote = if is_default {
                                        None
                                    } else {
                                        Some(set_remote.clone())
                                    };
                                    cx.update_entity(&set_entity, |app, cx| {
                                        app.set_project_default_push_remote(
                                            next_remote,
                                            window,
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new(item_labels.copy_url.clone())
                                .icon(HeroIconName::DocumentDuplicate)
                                .on_click(move |_, _window, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_url.clone(),
                                    ));
                                }),
                        )
                        .item(
                            PopupMenuItem::new(item_labels.remove_remote.clone())
                                .icon(HeroIconName::Trash)
                                .on_click(move |_, window, cx| {
                                    cx.update_entity(&remove_entity, |app, cx| {
                                        app.remove_project_git_remote(
                                            remove_remote.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                }),
                        )
                    },
                )
            })
        },
    );

    let remote_branch_items = remote_branches.clone();
    let remote_branch_groups = group_remote_branches(&remote_branch_items, upstream.as_deref());
    let remote_branch_entity = app_entity.clone();
    let has_remotes = !remotes.is_empty();
    let can_use_current_branch_remote = upstream.is_some()
        && current_branch != "HEAD"
        && current_branch != "uninitialized"
        && !current_branch.trim().is_empty();
    let remote_branch_labels = labels.clone();
    let menu = menu.submenu(
        labels.remote_branches.clone(),
        window,
        cx,
        move |menu, window, cx| {
            let fetch_entity = remote_branch_entity.clone();
            let menu = menu.item(
                PopupMenuItem::new(remote_branch_labels.refresh_remote_branches.clone())
                    .icon(HeroIconName::ArrowPath)
                    .disabled(!has_remotes)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&fetch_entity, |app, cx| {
                            app.fetch_project_git(window, cx);
                        });
                    }),
            );

            if remote_branch_groups.is_empty() {
                return menu.separator().item(
                    PopupMenuItem::new(remote_branch_labels.remote_branches_empty.clone())
                        .icon(HeroIconName::ArrowDown)
                        .disabled(true),
                );
            }

            remote_branch_groups
                .iter()
                .fold(menu.separator(), |menu, group| {
                    let group = group.clone();
                    let group_entity = remote_branch_entity.clone();
                    let group_labels = remote_branch_labels.clone();
                    menu.submenu(group.remote.clone(), window, cx, move |menu, window, cx| {
                        group.branches.iter().fold(menu, |menu, branch| {
                            let remote_branch = format!("{}/{}", group.remote, branch.name);
                            let checkout_branch = remote_branch.clone();
                            let checkout_entity = group_entity.clone();
                            let push_branch = remote_branch.clone();
                            let push_entity = group_entity.clone();
                            let branch_labels = group_labels.clone();
                            menu.submenu(
                                branch.name.clone(),
                                window,
                                cx,
                                move |menu, _window, _cx| {
                                    let checkout_branch = checkout_branch.clone();
                                    let checkout_entity = checkout_entity.clone();
                                    let push_branch = push_branch.clone();
                                    let push_entity = push_entity.clone();
                                    menu.item(
                                        PopupMenuItem::new(
                                            branch_labels.checkout_remote_branch.clone(),
                                        )
                                        .icon(HeroIconName::ArrowDown)
                                        .on_click(
                                            move |_, window, cx| {
                                                cx.update_entity(&checkout_entity, |app, cx| {
                                                    app.checkout_git_remote_branch(
                                                        checkout_branch.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            },
                                        ),
                                    )
                                    .item(
                                        PopupMenuItem::new(branch_labels.push_here.clone())
                                            .icon(HeroIconName::ArrowUp)
                                            .on_click(move |_, window, cx| {
                                                cx.update_entity(&push_entity, |app, cx| {
                                                    app.push_project_git_remote_branch(
                                                        push_branch.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            }),
                                    )
                                },
                            )
                        })
                    })
                })
        },
    );

    let fetch_entity = app_entity.clone();
    let pull_entity = app_entity.clone();
    let push_entity = app_entity.clone();
    let menu = menu
        .separator()
        .item(
            PopupMenuItem::new(labels.fetch.clone())
                .icon(HeroIconName::ArrowDown)
                .disabled(!has_remotes)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&fetch_entity, |app, cx| {
                        app.fetch_project_git(window, cx);
                    });
                }),
        )
        .item(
            PopupMenuItem::new(labels.pull.clone())
                .icon(HeroIconName::ArrowDown)
                .disabled(!can_use_current_branch_remote)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&pull_entity, |app, cx| {
                        app.pull_project_git(window, cx);
                    });
                }),
        )
        .item(
            PopupMenuItem::new(labels.push.clone())
                .icon(HeroIconName::ArrowUp)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&push_entity, |app, cx| {
                        app.push_project_git(window, cx);
                    });
                })
                .disabled(!can_use_current_branch_remote),
        );

    let push_remotes = remotes.clone();
    let push_remote_entity = app_entity.clone();
    let push_to_labels = labels.clone();
    let menu = menu.submenu(
        labels.push_to.clone(),
        window,
        cx,
        move |menu, _window, _cx| {
            if push_remotes.is_empty() {
                return menu.item(
                    PopupMenuItem::new(push_to_labels.no_remotes.clone())
                        .icon(HeroIconName::GlobeAlt)
                        .disabled(true),
                );
            }

            push_remotes.iter().fold(menu, |menu, remote| {
                let is_default = default_remote
                    .as_deref()
                    .map(|name| name == remote.name)
                    .unwrap_or(false);
                let remote_name = remote.name.clone();
                let label = if remote.url.trim().is_empty() {
                    remote.name.clone()
                } else {
                    format!("{}\n{}", remote.name, remote.url)
                };
                let app_entity = push_remote_entity.clone();
                menu.item(
                    PopupMenuItem::new(label)
                        .icon(if is_default {
                            HeroIconName::Check
                        } else {
                            HeroIconName::ArrowUp
                        })
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&app_entity, |app, cx| {
                                app.push_project_git_remote(remote_name.clone(), window, cx);
                            });
                        }),
                )
            })
        },
    );

    let force_push_entity = app_entity.clone();
    let undo_entity = app_entity.clone();
    let edit_entity = app_entity.clone();
    let reveal_entity = app_entity.clone();
    menu.separator()
        .item(
            PopupMenuItem::new(labels.force_push.clone())
                .icon(HeroIconName::ExclamationTriangle)
                .disabled(!can_use_current_branch_remote)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&force_push_entity, |app, cx| {
                        app.force_push_project_git(window, cx);
                    });
                }),
        )
        .item(
            PopupMenuItem::new(labels.undo_last_commit.clone())
                .icon(HeroIconName::ArrowUturnLeft)
                .disabled(!has_commits)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&undo_entity, |app, cx| {
                        app.undo_last_git_commit(window, cx);
                    });
                }),
        )
        .item(
            PopupMenuItem::new(labels.edit_last_commit_message.clone())
                .icon(HeroIconName::ArrowPath)
                .disabled(!has_commits)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&edit_entity, |app, cx| {
                        app.load_last_git_commit_message(window, cx);
                    });
                }),
        )
        .item(
            PopupMenuItem::new(labels.show_repository.clone())
                .icon(HeroIconName::FolderOpen)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&reveal_entity, |app, cx| {
                        app.reveal_selected_project_in_file_manager(window, cx);
                    });
                }),
        )
}

#[derive(Clone)]
struct GitBranchMenuLabels {
    new_branch: String,
    local_branches: String,
    local_empty: String,
    switch_branch: String,
    merge_current: String,
    squash_merge: String,
    delete_local: String,
    merge_empty: String,
    remote_branches: String,
    refresh_remote_branches: String,
    remote_branches_empty: String,
    checkout_remote_branch: String,
    push_here: String,
    remotes: String,
    add_remote: String,
    no_remotes: String,
    set_default: String,
    copy_url: String,
    remove_remote: String,
    fetch: String,
    pull: String,
    push: String,
    push_to: String,
    force_push: String,
    undo_last_commit: String,
    edit_last_commit_message: String,
    show_repository: String,
}

impl GitBranchMenuLabels {
    fn load(language: &str) -> Self {
        let locale = locale_from_language_setting(language);
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        Self {
            new_branch: tr("git.branch.create_and_switch", "New Branch"),
            local_branches: tr("git.branch.local", "Local Branches"),
            local_empty: tr("git.branch.local.empty", "No local branches"),
            switch_branch: tr("git.branch.switch", "Switch Branch"),
            merge_current: tr("git.branch.merge_current", "Merge into Current Branch"),
            squash_merge: tr(
                "git.branch.squash_merge",
                "Squash Merge into Current Branch",
            ),
            delete_local: tr("git.branch.delete_local", "Delete Local Branch"),
            merge_empty: tr("git.branch.merge.empty", "No branches to merge"),
            remote_branches: tr("git.remote.branches", "Remote Branches"),
            refresh_remote_branches: tr("git.remote.branches.refresh", "Refresh Remote Branches"),
            remote_branches_empty: tr("git.remote.branches.empty", "No remote branches"),
            checkout_remote_branch: tr(
                "git.remote.branch.checkout_local",
                "Checkout as Local Branch",
            ),
            push_here: tr("git.remote.branch.push_here", "Push to This Branch"),
            remotes: tr("git.remote.remotes", "Remotes"),
            add_remote: tr("git.remote.add", "Add Remote"),
            no_remotes: tr("git.remote.empty", "No remotes"),
            set_default: tr("git.remote.set_default", "Set as Default"),
            copy_url: tr("git.remote.copy_url", "Copy URL"),
            remove_remote: tr("git.remote.remove", "Remove Remote"),
            fetch: tr("git.remote.fetch", "Fetch"),
            pull: tr("git.remote.pull", "Pull"),
            push: tr("git.remote.push", "Push"),
            push_to: tr("git.remote.push_to", "Push To..."),
            force_push: tr("git.remote.force_push", "Force Push"),
            undo_last_commit: tr("git.history.undo_last_commit", "Undo Last Commit"),
            edit_last_commit_message: tr(
                "git.history.edit_last_commit_message",
                "Edit Last Commit Message",
            ),
            show_repository: tr(
                "git.repository.show_in_finder",
                "Show Repository in File Manager",
            ),
        }
    }
}

#[derive(Clone)]
struct RemoteBranchGroup {
    remote: String,
    branches: Vec<RemoteBranchItem>,
}

#[derive(Clone)]
struct RemoteBranchItem {
    name: String,
    is_upstream: bool,
}

fn group_remote_branches(values: &[String], upstream: Option<&str>) -> Vec<RemoteBranchGroup> {
    let mut groups: BTreeMap<String, Vec<RemoteBranchItem>> = BTreeMap::new();
    for value in values {
        let Some((remote, branch)) = value.split_once('/') else {
            continue;
        };
        if remote.is_empty() || branch.is_empty() || branch == "HEAD" {
            continue;
        }
        let branches = groups.entry(remote.to_string()).or_default();
        if branches.iter().any(|item| item.name == branch) {
            continue;
        }
        branches.push(RemoteBranchItem {
            name: branch.to_string(),
            is_upstream: upstream == Some(value.as_str()),
        });
    }

    groups
        .into_iter()
        .map(|(remote, mut branches)| {
            branches.sort_by(|left, right| {
                right
                    .is_upstream
                    .cmp(&left.is_upstream)
                    .then_with(|| left.name.cmp(&right.name))
            });
            RemoteBranchGroup { remote, branches }
        })
        .collect()
}

fn git_repository_panel(
    _git: &GitSummary,
    remote_editor_open: bool,
    remote_name: &str,
    remote_url: &str,
    labels: Rc<GitSidebarLabels>,
    commit_message: &str,
    commit_message_revision: u64,
    files_panel_view: gpui::Entity<GitFilesPanelView>,
    history_panel_view: gpui::Entity<GitHistoryPanelView>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .flex_col()
        .child(git_commit_panel(
            commit_message,
            commit_message_revision,
            labels.clone(),
            window,
            cx,
        ))
        .when(remote_editor_open, |this| {
            this.child(git_remote_editor_panel(
                remote_name,
                remote_url,
                labels.clone(),
                window,
                cx,
            ))
        })
        .child(
            v_resizable("git-sidebar-file-history-split")
                .child(
                    resizable_panel()
                        .size_range(px(160.0)..px(900.0))
                        .child(gpui::AnyView::from(files_panel_view)),
                )
                .child(
                    resizable_panel()
                        .size(px(260.0))
                        .size_range(px(180.0)..px(420.0))
                        .child(gpui::AnyView::from(history_panel_view)),
                ),
        )
}

fn git_remote_editor_panel(
    remote_name: &str,
    remote_url: &str,
    labels: Rc<GitSidebarLabels>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let name_value = remote_name.to_string();
    let name_state = window.use_keyed_state("git-remote-name", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(name_value.clone())
            .placeholder(labels.remote_name.clone())
    });
    name_state.update(cx, |state, cx| {
        if state.value().as_ref() != remote_name {
            state.set_value(remote_name.to_string(), window, cx);
        }
    });
    cx.subscribe_in(&name_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_git_remote_name(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    let url_value = remote_url.to_string();
    let url_state = window.use_keyed_state("git-remote-url", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(url_value.clone())
            .placeholder(labels.remote_url.clone())
    });
    url_state.update(cx, |state, cx| {
        if state.value().as_ref() != remote_url {
            state.set_value(remote_url.to_string(), window, cx);
        }
    });
    cx.subscribe_in(&url_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_git_remote_url(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .flex_shrink_0()
        .border_b_1()
        .border_color(color(theme::BORDER_SOFT))
        .p(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .child(labels.add_remote.clone()),
                )
                .child(
                    Button::new("git-remote-editor-close")
                        .compact()
                        .ghost()
                        .text_color(cx.theme().secondary_foreground)
                        .icon(Icon::new(HeroIconName::XMark).size_3p5())
                        .on_click(cx.listener(|app, _event, window, cx| {
                            app.close_git_remote_editor(window, cx)
                        })),
                ),
        )
        .child(
            div()
                .mt(px(10.0))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(96.0))
                        .child(Input::new(&name_state).with_size(gpui_component::Size::Small)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&url_state).with_size(gpui_component::Size::Small)),
                )
                .child(
                    Button::new("git-remote-editor-add")
                        .compact()
                        .secondary()
                        .disabled(remote_name.trim().is_empty() || remote_url.trim().is_empty())
                        .text_color(cx.theme().secondary_foreground)
                        .label(labels.add.clone())
                        .on_click(cx.listener(|app, _event, window, cx| {
                            app.add_project_git_remote(window, cx)
                        })),
                ),
        )
}

fn git_commit_panel(
    commit_message: &str,
    commit_message_revision: u64,
    labels: Rc<GitSidebarLabels>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let button_bg = color(theme::ACCENT).opacity(0.70);
    let app_entity = cx.entity();
    let value = commit_message.to_string();
    let input_state = window.use_keyed_state(
        SharedString::from(format!("git-commit-message-{commit_message_revision}")),
        cx,
        |window, cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(3)
                .default_value(value.clone())
                .placeholder(labels.commit_message.clone())
        },
    );
    cx.subscribe_in(&input_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_git_commit_message(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .h(px(162.0))
        .flex_shrink_0()
        .p(px(12.0))
        .border_b_1()
        .border_color(color(theme::BORDER_SOFT))
        .child(
            Input::new(&input_state)
                .with_size(gpui_component::Size::Medium)
                .h(px(86.0)),
        )
        .child(
            div()
                .id("git-sidebar-commit-button")
                .mt(px(12.0))
                .h(px(34.0))
                .rounded(px(8.0))
                .flex()
                .items_center()
                .overflow_hidden()
                .bg(button_bg)
                .text_color(color(0xFFFFFF))
                .cursor_pointer()
                .on_click(cx.listener(|app, _event, window, cx| app.commit_staged_git(window, cx)))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .child(labels.commit.clone()),
                )
                .child(
                    Button::new("git-sidebar-commit-actions")
                        .h(px(34.0))
                        .w(px(44.0))
                        .compact()
                        .primary()
                        .text_color(color(0xFFFFFF))
                        .bg(color(theme::ACCENT).opacity(0.18))
                        .icon(
                            Icon::new(HeroIconName::ChevronDown)
                                .size_3()
                                .text_color(color(0xFFFFFF)),
                        )
                        .dropdown_menu(move |menu, _window, _cx| {
                            let commit_entity = app_entity.clone();
                            let push_entity = app_entity.clone();
                            let sync_entity = app_entity.clone();
                            let load_last_entity = app_entity.clone();
                            let amend_entity = app_entity.clone();
                            let undo_entity = app_entity.clone();
                            menu.item(
                                PopupMenuItem::new(labels.commit.clone())
                                    .icon(HeroIconName::Check)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&commit_entity, |app, cx| {
                                            app.commit_staged_git(window, cx);
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(labels.commit_push.clone())
                                    .icon(HeroIconName::ArrowUp)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&push_entity, |app, cx| {
                                            app.commit_and_push_git(window, cx);
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(labels.commit_sync.clone())
                                    .icon(HeroIconName::ArrowPath)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&sync_entity, |app, cx| {
                                            app.commit_and_sync_git(window, cx);
                                        });
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new(labels.load_last_commit_message.clone())
                                    .icon(HeroIconName::DocumentDuplicate)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&load_last_entity, |app, cx| {
                                            app.load_last_git_commit_message(window, cx);
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(labels.amend_last_commit.clone())
                                    .icon(HeroIconName::ArrowUturnRight)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&amend_entity, |app, cx| {
                                            app.amend_last_git_commit(window, cx);
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(labels.undo_last_commit.clone())
                                    .icon(HeroIconName::ArrowUturnLeft)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&undo_entity, |app, cx| {
                                            app.undo_last_git_commit(window, cx);
                                        });
                                    }),
                            )
                        }),
                ),
        )
}

fn git_files_panel(
    staged: &[GitFileStatus],
    changed: &[GitFileStatus],
    untracked: &[GitFileStatus],
    expanded_sections: &HashSet<String>,
    expanded_dirs: &HashSet<String>,
    tree_children: &HashMap<String, Vec<GitFileStatus>>,
    selected_file: Option<&str>,
    selected_files: &HashSet<String>,
    labels: Rc<GitSidebarLabels>,
    scroll_handle: VirtualListScrollHandle,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let rows = Rc::new(git_status_virtual_rows(
        staged,
        changed,
        untracked,
        expanded_sections,
        expanded_dirs,
        tree_children,
        selected_file,
        selected_files,
        &labels,
    ));
    let item_sizes = Rc::new(
        rows.iter()
            .map(|row| size(px(1.0), row.height()))
            .collect::<Vec<_>>(),
    );
    div()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .relative()
        .overflow_hidden()
        .child(
            v_virtual_list(
                cx.entity().clone(),
                "git-files-list",
                item_sizes,
                move |_app, visible_range: Range<usize>, _window, cx| {
                    visible_range
                        .filter_map(|index| {
                            rows.get(index)
                                .cloned()
                                .map(|row: GitStatusVirtualRow| row.render(cx))
                        })
                        .collect::<Vec<_>>()
                },
            )
            .track_scroll(&scroll_handle)
            .with_sizing_behavior(ListSizingBehavior::Auto),
        )
        .vertical_scrollbar(&scroll_handle)
}

fn git_empty_repository_panel(
    labels: Rc<GitSidebarLabels>,
    running_operation: Option<&GitRunningOperation>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let cloning = running_operation.is_some_and(|operation| operation.label == "clone");
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .p(px(18.0))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .text_center()
                .child(
                    div()
                        .size(px(42.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(color(theme::ORANGE).opacity(0.12))
                        .text_color(color(theme::ORANGE))
                        .child(Icon::new(HeroIconName::Folder).size_5()),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .child(labels.no_repository.clone()),
                )
                .child(
                    div()
                        .mt(px(6.0))
                        .max_w(px(220.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0625))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(if cloning {
                            labels.clone_preparing.clone()
                        } else {
                            labels.no_repository_description.clone()
                        }),
                )
                .when(cloning, |this| {
                    this.child(git_clone_indeterminate_progress())
                })
                .child(div().mt(px(14.0)).flex().items_center().gap(px(8.0)).when(
                    !cloning,
                    |this| {
                        this.child(
                            git_empty_action_button(labels.init_repository.clone(), true).on_click(
                                cx.listener(|app, _event, window, cx| {
                                    app.init_project_git(window, cx)
                                }),
                            ),
                        )
                        .child(
                            git_empty_action_button(labels.clone_repository.clone(), false)
                                .on_click(cx.listener(|app, _event, window, cx| {
                                    app.open_git_clone_dialog(window, cx)
                                })),
                        )
                    },
                )),
        )
}

fn git_clone_indeterminate_progress() -> impl IntoElement {
    div()
        .mt(px(8.0))
        .w_full()
        .h(px(4.0))
        .rounded(px(4.0))
        .overflow_hidden()
        .bg(color(theme::BORDER_SOFT))
        .child(
            div()
                .h_full()
                .w(gpui::relative(0.34))
                .rounded(px(4.0))
                .bg(color(theme::ACCENT))
                .with_animation(
                    "git-clone-progress",
                    Animation::new(Duration::from_millis(980)).repeat(),
                    |bar, delta| bar.ml(gpui::relative(-0.34 + delta * 1.34)),
                ),
        )
}

fn git_empty_action_button(label: String, primary: bool) -> Stateful<Div> {
    div()
        .id(ElementId::Name(SharedString::from(format!(
            "git-empty-action-{label}"
        ))))
        .h(px(24.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .when(primary, |this| {
            this.bg(color(theme::ACCENT))
                .text_color(color(0xFFFFFF))
                .hover(|style| style.bg(color(theme::ACCENT).opacity(0.88)))
        })
        .when(!primary, |this| {
            this.bg(color(theme::BG_PANEL))
                .text_color(color(theme::TEXT))
                .hover(|style| style.bg(color(theme::BG_ROW_HOVER)))
        })
        .child(label)
}

pub(in crate::app) fn git_clone_window_workspace(
    clone_remote_url: &str,
    running_operation: Option<&GitRunningOperation>,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let labels = Rc::new(GitSidebarLabels::load(language));
    let cloning = running_operation.is_some_and(|operation| operation.label == "clone");
    let value = clone_remote_url.to_string();
    let input_state = window.use_keyed_state("git-clone-remote-url", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .placeholder(labels.remote_url.clone())
    });
    input_state.update(cx, |state, cx| {
        if state.value().as_ref() != clone_remote_url {
            state.set_value(clone_remote_url.to_string(), window, cx);
        }
    });
    cx.subscribe_in(
        &input_state,
        window,
        |app, state, event, window, cx| match event {
            InputEvent::Change => {
                app.set_git_clone_remote_url(state.read(cx).value().to_string(), window, cx);
            }
            InputEvent::PressEnter { .. } => app.clone_project_git(window, cx),
            _ => {}
        },
    )
    .detach();

    child_window_shell(labels.clone_repository.clone(), cx)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .p(px(18.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(git_clone_input_label(labels.remote_url.clone()))
                .child(
                    div()
                        .child(
                            Input::new(&input_state)
                                .disabled(cloning)
                                .with_size(gpui_component::Size::Medium),
                        )
                        .when(cloning, |this| {
                            this.child(git_clone_indeterminate_progress())
                        }),
                ),
        )
        .child(dialog_footer_bar(
            div().flex().items_center().gap(px(8.0)).child(
                dialog_primary_button(
                    "git-clone-confirm",
                    labels.confirm.clone(),
                    cx,
                    |app, _event, window, cx| app.clone_project_git(window, cx),
                )
                .loading(cloning)
                .disabled(cloning || clone_remote_url.trim().is_empty()),
            ),
            cx,
        ))
}

fn git_clone_input_label(label: impl Into<String>) -> impl IntoElement {
    div()
        .text_size(rems(0.875))
        .line_height(rems(1.125))
        .text_color(color(theme::TEXT))
        .child(label.into())
}

pub(in crate::app) fn git_credentials_window_workspace(
    app: &CoduxApp,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let labels = Rc::new(GitSidebarLabels::load(language));
    let retrying = app.git_credential_retrying
        || app
            .git_running_operation
            .as_ref()
            .is_some_and(|operation| operation.label == "clone");

    child_window_shell(labels.credentials_title.clone(), cx)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .p(px(16.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .mb(px(14.0))
                        .text_size(rems(0.875))
                        .line_height(rems(1.25))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(labels.credentials_message.clone()),
                )
                .child(git_credentials_input(
                    "username",
                    labels.credential_username.clone(),
                    &app.git_credential_username,
                    false,
                    retrying,
                    window,
                    cx,
                    |app, value, window, cx| app.set_git_credential_username(value, window, cx),
                ))
                .child(git_credentials_input(
                    "password-or-token",
                    labels.credential_password_or_token.clone(),
                    &app.git_credential_password_or_token,
                    true,
                    retrying,
                    window,
                    cx,
                    |app, value, window, cx| {
                        app.set_git_credential_password_or_token(value, window, cx)
                    },
                ))
                .when_some(app.git_credential_error.clone(), |this, error| {
                    this.child(
                        div()
                            .mt(px(8.0))
                            .text_size(rems(0.75))
                            .line_height(rems(1.0))
                            .text_color(color(0xF47C7C))
                            .child(error),
                    )
                }),
        )
        .child(dialog_footer_bar(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    dialog_cancel_button(
                        "git-credentials-cancel",
                        labels.cancel.clone(),
                        cx,
                        |app, _event, window, cx| app.close_git_credentials_dialog(window, cx),
                    )
                    .disabled(retrying),
                )
                .child(
                    dialog_primary_button(
                        "git-credentials-confirm",
                        labels.confirm.clone(),
                        cx,
                        |app, _event, window, cx| app.retry_git_clone_with_credentials(window, cx),
                    )
                    .loading(retrying)
                    .disabled(retrying),
                ),
            cx,
        ))
}

fn git_credentials_input(
    id: &'static str,
    label: String,
    value: &str,
    masked: bool,
    disabled: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let value = value.to_string();
    let state = window.use_keyed_state(SharedString::from(format!("git-credential-{id}")), cx, {
        let value = value.clone();
        move |window, cx| {
            InputState::new(window, cx)
                .default_value(value.clone())
                .masked(masked)
        }
    });
    state.update(cx, |state, cx| {
        if state.value().as_ref() != value.as_str() {
            state.set_value(value.clone(), window, cx);
        }
    });
    cx.subscribe_in(&state, window, move |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            action(app, state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .mb(px(14.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .child(label),
        )
        .child(
            Input::new(&state)
                .disabled(disabled)
                .with_size(gpui_component::Size::Medium),
        )
}

#[derive(Clone)]
enum GitStatusVirtualRow {
    GroupHeader {
        id: &'static str,
        title: String,
        count: usize,
        files: Vec<GitFileStatus>,
        expanded: bool,
        first: bool,
    },
    Spacer {
        height: f32,
    },
    Empty {
        text: String,
    },
    Dir {
        section_id: &'static str,
        name: String,
        path: String,
        expanded: bool,
        depth: usize,
    },
    File {
        file: GitFileStatus,
        active: bool,
        selected_files: HashSet<String>,
        depth: usize,
        labels: Rc<GitFileMenuLabels>,
    },
    Limit {
        count: usize,
        text: String,
    },
}

const GIT_STATUS_GROUP_TOP_PADDING: f32 = 4.0;
const GIT_STATUS_GROUP_BOTTOM_PADDING: f32 = 8.0;

impl GitStatusVirtualRow {
    fn height(&self) -> Pixels {
        match self {
            Self::GroupHeader { .. } => px(40.0),
            Self::Spacer { height } => px(*height),
            Self::Empty { .. } => px(42.0),
            Self::Dir { .. } | Self::File { .. } => px(24.0),
            Self::Limit { .. } => px(32.0),
        }
    }

    fn render(self, cx: &mut Context<CoduxApp>) -> AnyElement {
        match self {
            Self::GroupHeader {
                id,
                title,
                count,
                files,
                expanded,
                first,
            } => git_status_group_header(id, title, count, files, expanded, first, cx)
                .into_any_element(),
            Self::Spacer { height } => div().h(px(height)).into_any_element(),
            Self::Empty { text } => div()
                .px_3()
                .py_3()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT_DIM))
                .child(text)
                .into_any_element(),
            Self::Dir {
                section_id,
                name,
                path,
                expanded,
                depth,
            } => {
                git_status_dir_row(section_id, &name, &path, expanded, depth, cx).into_any_element()
            }
            Self::File {
                file,
                active,
                selected_files,
                depth,
                labels,
            } => {
                let selected_path = active.then(|| file.path.clone());
                git_status_file_row(
                    file,
                    selected_path.as_deref(),
                    &selected_files,
                    depth,
                    labels,
                    cx,
                )
                .into_any_element()
            }
            Self::Limit { count, text } => div()
                .px_3()
                .py_2()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_DIM))
                .child(text.replace("%@", &count.to_string()))
                .into_any_element(),
        }
    }
}

fn git_status_virtual_rows(
    staged: &[GitFileStatus],
    changed: &[GitFileStatus],
    untracked: &[GitFileStatus],
    expanded_sections: &HashSet<String>,
    expanded_dirs: &HashSet<String>,
    tree_children: &HashMap<String, Vec<GitFileStatus>>,
    selected_file: Option<&str>,
    selected_files: &HashSet<String>,
    labels: &GitSidebarLabels,
) -> Vec<GitStatusVirtualRow> {
    let mut rows = Vec::new();
    let file_menu_labels = Rc::new(GitFileMenuLabels::from(labels));
    append_git_status_group_virtual_rows(
        "staged",
        labels.staged.clone(),
        staged,
        expanded_sections,
        expanded_dirs,
        tree_children,
        selected_file,
        selected_files,
        labels.staged_empty.clone(),
        labels.tree_limit.clone(),
        file_menu_labels.clone(),
        rows.is_empty(),
        &mut rows,
    );
    append_git_status_group_virtual_rows(
        "changed",
        labels.changed.clone(),
        changed,
        expanded_sections,
        expanded_dirs,
        tree_children,
        selected_file,
        selected_files,
        labels.changed_empty.clone(),
        labels.tree_limit.clone(),
        file_menu_labels.clone(),
        rows.is_empty(),
        &mut rows,
    );
    append_git_status_group_virtual_rows(
        "untracked",
        labels.untracked.clone(),
        untracked,
        expanded_sections,
        expanded_dirs,
        tree_children,
        selected_file,
        selected_files,
        labels.untracked_empty.clone(),
        labels.tree_limit.clone(),
        file_menu_labels,
        rows.is_empty(),
        &mut rows,
    );
    rows
}

fn append_git_status_group_virtual_rows(
    id: &'static str,
    title: String,
    files: &[GitFileStatus],
    expanded_sections: &HashSet<String>,
    expanded_dirs: &HashSet<String>,
    tree_children: &HashMap<String, Vec<GitFileStatus>>,
    selected_file: Option<&str>,
    selected_files: &HashSet<String>,
    empty_text: String,
    tree_limit: String,
    file_menu_labels: Rc<GitFileMenuLabels>,
    first: bool,
    rows: &mut Vec<GitStatusVirtualRow>,
) {
    let expanded = expanded_sections.contains(id);
    rows.push(GitStatusVirtualRow::GroupHeader {
        id,
        title,
        count: files.len(),
        files: files.to_vec(),
        expanded,
        first,
    });
    if !expanded {
        return;
    }
    rows.push(GitStatusVirtualRow::Spacer {
        height: GIT_STATUS_GROUP_TOP_PADDING,
    });
    if files.is_empty() {
        rows.push(GitStatusVirtualRow::Empty { text: empty_text });
        rows.push(GitStatusVirtualRow::Spacer {
            height: GIT_STATUS_GROUP_BOTTOM_PADDING,
        });
        return;
    }
    let start_len = rows.len();
    append_git_status_virtual_directory_rows(
        id,
        "",
        files,
        0,
        expanded_dirs,
        tree_children,
        selected_file,
        selected_files,
        file_menu_labels,
        rows,
    );
    let appended = rows.len().saturating_sub(start_len);
    if appended >= MAX_GIT_STATUS_TREE_ROWS {
        rows.push(GitStatusVirtualRow::Limit {
            count: appended,
            text: tree_limit,
        });
    }
    rows.push(GitStatusVirtualRow::Spacer {
        height: GIT_STATUS_GROUP_BOTTOM_PADDING,
    });
}

fn append_git_status_virtual_directory_rows(
    section_id: &'static str,
    base_path: &str,
    files: &[GitFileStatus],
    depth: usize,
    expanded_dirs: &HashSet<String>,
    tree_children: &HashMap<String, Vec<GitFileStatus>>,
    selected_file: Option<&str>,
    selected_files: &HashSet<String>,
    file_menu_labels: Rc<GitFileMenuLabels>,
    rows: &mut Vec<GitStatusVirtualRow>,
) {
    if rows.len() >= MAX_GIT_STATUS_TREE_ROWS {
        return;
    }

    let (dirs, direct_files) = collect_immediate_git_status_entries(section_id, base_path, files);

    for (name, dir) in dirs {
        if rows.len() >= MAX_GIT_STATUS_TREE_ROWS {
            return;
        }
        let tree_key = git_status_tree_key(section_id, &dir.path);
        let expanded = expanded_dirs.contains(&tree_key);
        rows.push(GitStatusVirtualRow::Dir {
            section_id,
            name,
            path: dir.path.clone(),
            expanded,
            depth,
        });
        if expanded {
            if let Some(children) = tree_children.get(&tree_key) {
                append_git_status_virtual_directory_rows(
                    section_id,
                    &dir.path,
                    children,
                    depth + 1,
                    expanded_dirs,
                    tree_children,
                    selected_file,
                    selected_files,
                    file_menu_labels.clone(),
                    rows,
                );
            }
        }
    }
    for file in direct_files {
        if rows.len() >= MAX_GIT_STATUS_TREE_ROWS {
            return;
        }
        let active = selected_file
            .map(|path| path == file.path.as_str())
            .unwrap_or(false);
        rows.push(GitStatusVirtualRow::File {
            file,
            active,
            selected_files: selected_files.clone(),
            depth,
            labels: file_menu_labels.clone(),
        });
    }
}

fn git_status_group_header(
    id: &'static str,
    title: String,
    count: usize,
    _files: Vec<GitFileStatus>,
    expanded: bool,
    first: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("git-status-group-{id}")))
        .w_full()
        .min_w_0()
        .h(px(40.0))
        .px_3()
        .flex()
        .items_center()
        .justify_between()
        .border_color(cx.theme().border)
        .when(!first, |this| this.border_t_1())
        .bg(cx.theme().list_head)
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _event, _window, cx| app.toggle_git_status_section(id, cx)),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .min_w_0()
                .gap_2()
                .child(
                    Icon::new(if expanded {
                        HeroIconName::ChevronDown
                    } else {
                        HeroIconName::ChevronRight
                    })
                    .size_3p5()
                    .text_color(color(theme::TEXT_DIM)),
                )
                .child(
                    div()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(cx.theme().muted_foreground)
                        .child(title),
                )
                .child(
                    div()
                        .px_1p5()
                        .h(px(18.0))
                        .min_w(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.0))
                        .bg(cx.theme().secondary)
                        .text_size(rems(0.75))
                        .line_height(rems(0.875))
                        .text_color(cx.theme().muted_foreground)
                        .child(count.to_string()),
                ),
        )
}

struct GitImmediateDir {
    path: String,
    count: usize,
}

const MAX_GIT_STATUS_TREE_ROWS: usize = 600;

fn collect_immediate_git_status_entries(
    section_id: &'static str,
    base_path: &str,
    files: &[GitFileStatus],
) -> (BTreeMap<String, GitImmediateDir>, Vec<GitFileStatus>) {
    let mut dirs = BTreeMap::<String, GitImmediateDir>::new();
    let mut direct_files = Vec::<GitFileStatus>::new();
    for file in files {
        if !git_status_matches_section(section_id, file) {
            continue;
        }
        let Some(relative_path) = relative_git_status_path(base_path, &file.path) else {
            continue;
        };
        let relative_path = relative_path.trim_end_matches('/');
        if relative_path.is_empty() {
            continue;
        }
        if let Some((dir_name, _rest)) = relative_path.split_once('/') {
            let dir_path = join_git_path(base_path, dir_name);
            dirs.entry(dir_name.to_string())
                .and_modify(|dir| dir.count += 1)
                .or_insert(GitImmediateDir {
                    path: dir_path,
                    count: 1,
                });
        } else if file.path.ends_with('/') {
            let dir_path = join_git_path(base_path, relative_path);
            dirs.entry(relative_path.to_string())
                .and_modify(|dir| dir.count += 1)
                .or_insert(GitImmediateDir {
                    path: dir_path,
                    count: 1,
                });
        } else {
            direct_files.push(file.clone());
        }
    }
    (dirs, direct_files)
}

fn git_status_matches_section(section_id: &'static str, file: &GitFileStatus) -> bool {
    match section_id {
        "staged" => is_git_staged_file(file),
        "changed" => is_git_worktree_file(file),
        "untracked" => is_git_untracked_file(file),
        _ => true,
    }
}

fn relative_git_status_path<'a>(base_path: &str, file_path: &'a str) -> Option<&'a str> {
    let base_path = base_path.trim_matches('/');
    if base_path.is_empty() {
        return Some(file_path);
    }
    file_path
        .strip_prefix(base_path)
        .and_then(|path| path.strip_prefix('/'))
}

fn join_git_path(base_path: &str, name: &str) -> String {
    let base_path = base_path.trim_matches('/');
    if base_path.is_empty() {
        name.to_string()
    } else {
        format!("{base_path}/{name}")
    }
}

fn git_status_tree_key(section_id: &str, path: &str) -> String {
    format!("{section_id}:{}", path.trim_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_file(path: &str, index_status: &str, worktree_status: &str) -> GitFileStatus {
        GitFileStatus {
            path: path.to_string(),
            index_status: index_status.to_string(),
            worktree_status: worktree_status.to_string(),
        }
    }

    #[test]
    fn git_tree_collects_only_immediate_rows_for_current_directory() {
        let files = vec![
            git_file("src/main.rs", " ", "M"),
            git_file("src/nested/lib.rs", " ", "M"),
            git_file("README.md", " ", "M"),
            git_file("bulk/", "?", "?"),
        ];

        let (root_dirs, root_files) = collect_immediate_git_status_entries("changed", "", &files);
        assert_eq!(root_dirs.keys().cloned().collect::<Vec<_>>(), vec!["src"]);
        assert_eq!(
            root_files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["README.md"]
        );

        let (src_dirs, src_files) = collect_immediate_git_status_entries("changed", "src", &files);
        assert_eq!(src_dirs.keys().cloned().collect::<Vec<_>>(), vec!["nested"]);
        assert_eq!(
            src_files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/main.rs"]
        );
    }

    #[test]
    fn git_tree_keeps_untracked_directory_as_lazy_child() {
        let files = vec![
            git_file("bulk/", "?", "?"),
            git_file("bulk/nested/a.txt", "?", "?"),
        ];

        let (root_dirs, root_files) = collect_immediate_git_status_entries("untracked", "", &files);
        assert_eq!(root_dirs["bulk"].path, "bulk");
        assert!(root_files.is_empty());

        let (bulk_dirs, bulk_files) =
            collect_immediate_git_status_entries("untracked", "bulk", &files);
        assert_eq!(
            bulk_dirs.keys().cloned().collect::<Vec<_>>(),
            vec!["nested"]
        );
        assert!(bulk_files.is_empty());
    }

    #[test]
    fn review_tree_treats_added_directory_marker_as_directory() {
        let files = vec![
            GitReviewFile {
                path: "plan_test/".to_string(),
                status: "added".to_string(),
                additions: 0,
                deletions: 0,
            },
            GitReviewFile {
                path: "plan_test/readme.md".to_string(),
                status: "added".to_string(),
                additions: 2,
                deletions: 0,
            },
        ];

        let (root_dirs, root_files) = collect_immediate_git_review_entries("", &files);
        assert_eq!(
            root_dirs.keys().cloned().collect::<Vec<_>>(),
            vec!["plan_test"]
        );
        assert!(root_files.is_empty());

        let (_child_dirs, child_files) = collect_immediate_git_review_entries("plan_test", &files);
        assert_eq!(
            child_files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["plan_test/readme.md"]
        );
    }

    #[test]
    fn review_tree_splits_nested_added_directory_marker_by_depth() {
        let files = vec![GitReviewFile {
            path: "assets/art/generated/sliced/characters/skeleton_test/".to_string(),
            status: "added".to_string(),
            additions: 0,
            deletions: 0,
        }];

        let (root_dirs, root_files) = collect_immediate_git_review_entries("", &files);
        assert_eq!(
            root_dirs.keys().cloned().collect::<Vec<_>>(),
            vec!["assets"]
        );
        assert!(root_files.is_empty());

        let (asset_dirs, asset_files) = collect_immediate_git_review_entries("assets", &files);
        assert_eq!(asset_dirs.keys().cloned().collect::<Vec<_>>(), vec!["art"]);
        assert!(asset_files.is_empty());
    }

    #[test]
    fn git_tree_keys_scope_same_directory_by_section() {
        assert_eq!(git_status_tree_key("changed", "src"), "changed:src");
        assert_eq!(git_status_tree_key("untracked", "src"), "untracked:src");
        assert_ne!(
            git_status_tree_key("changed", "src"),
            git_status_tree_key("untracked", "src")
        );
    }
}

fn git_status_dir_row(
    section_id: &'static str,
    name: &str,
    path: &str,
    expanded: bool,
    depth: usize,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let directory_path = path.to_string();
    let directory_section = section_id.to_string();

    div()
        .id(SharedString::from(format!(
            "git-sidebar-dir-{section_id}-{path}"
        )))
        .w_full()
        .min_w_0()
        .h(px(24.0))
        .pl(px(18.0 + depth as f32 * 18.0))
        .pr_3()
        .flex()
        .items_center()
        .text_color(color(theme::TEXT_MUTED))
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(move |app, _event, _window, cx| {
            app.toggle_git_status_dir(directory_section.clone(), directory_path.clone(), cx)
        }))
        .child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .min_w_0()
                .child(
                    Icon::new(if expanded {
                        HeroIconName::ChevronDown
                    } else {
                        HeroIconName::ChevronRight
                    })
                    .size_3(),
                )
                .child(
                    Icon::new(HeroIconName::Folder)
                        .size_4()
                        .ml(px(8.0))
                        .text_color(color(theme::ACCENT)),
                )
                .child(
                    div()
                        .ml(px(8.0))
                        .min_w_0()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .truncate()
                        .child(name.to_string()),
                ),
        )
}

fn git_status_file_row(
    file: GitFileStatus,
    selected_file: Option<&str>,
    selected_files: &HashSet<String>,
    depth: usize,
    labels: Rc<GitFileMenuLabels>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let status = git_file_status_label(&file);
    let status_color = git_file_status_color(&status);
    let can_stage = is_git_worktree_file(&file) || is_git_untracked_file(&file);
    let can_unstage = is_git_staged_file(&file);
    let can_discard = is_git_worktree_file(&file) || is_git_untracked_file(&file);
    let active = selected_file.map(|path| path == file.path).unwrap_or(false)
        || selected_files.contains(&file.path);
    let file_path = file.path.clone();
    let menu_file_path = file.path.clone();
    let file_name = file
        .path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(file.path.as_str())
        .to_string();
    let is_dir_status = file.path.ends_with('/');
    let app_entity = cx.entity();

    div()
        .id(SharedString::from(format!(
            "git-sidebar-file-{}",
            file.path
        )))
        .w_full()
        .min_w_0()
        .h(px(24.0))
        .pl(px(46.0 + depth as f32 * 18.0))
        .pr_3()
        .flex()
        .items_center()
        .justify_between()
        .bg(if active {
            cx.theme().list_hover
        } else {
            cx.theme().transparent
        })
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(move |app, event: &ClickEvent, window, cx| {
            if event.click_count() >= 2 && !is_dir_status {
                app.open_git_diff_window(file_path.clone(), window, cx);
            } else if event.modifiers().shift {
                app.toggle_git_file_selection(file_path.clone(), cx);
            } else {
                app.select_git_file_only(file_path.clone(), cx);
            }
        }))
        .child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .min_w_0()
                .text_color(color(theme::TEXT_MUTED))
                .child(
                    Icon::new(if is_dir_status {
                        HeroIconName::Folder
                    } else {
                        HeroIconName::Document
                    })
                    .size_3p5(),
                )
                .child(
                    div()
                        .ml(px(8.0))
                        .min_w_0()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .truncate()
                        .child(file_name),
                ),
        )
        .child(
            div().ml_2().flex().items_center().gap_1().child(
                div()
                    .min_w(px(18.0))
                    .text_size(rems(0.875))
                    .line_height(rems(1.125))
                    .text_color(color(status_color))
                    .child(status),
            ),
        )
        .context_menu(move |menu, _window, _cx| {
            let stage_entity = app_entity.clone();
            let stage_path = menu_file_path.clone();
            let unstage_entity = app_entity.clone();
            let unstage_path = menu_file_path.clone();
            let discard_entity = app_entity.clone();
            let discard_path = menu_file_path.clone();
            let ignore_entity = app_entity.clone();
            let ignore_path = menu_file_path.clone();
            let diff_entity = app_entity.clone();
            let diff_path = menu_file_path.clone();

            let menu = if can_stage {
                menu.item(
                    git_context_menu_item(labels.stage.clone(), HeroIconName::Plus).on_click(
                        move |_, window, cx| {
                            cx.update_entity(&stage_entity, |app, cx| {
                                app.select_git_file_only(stage_path.clone(), cx);
                                app.stage_git_paths(
                                    app.selected_git_action_paths(&stage_path),
                                    window,
                                    cx,
                                );
                            });
                        },
                    ),
                )
            } else {
                menu
            };
            let menu = if can_unstage {
                menu.item(
                    git_context_menu_item(labels.unstage.clone(), HeroIconName::Minus).on_click(
                        move |_, window, cx| {
                            cx.update_entity(&unstage_entity, |app, cx| {
                                app.select_git_file_only(unstage_path.clone(), cx);
                                app.unstage_git_paths(
                                    app.selected_git_action_paths(&unstage_path),
                                    window,
                                    cx,
                                );
                            });
                        },
                    ),
                )
            } else {
                menu
            };
            let menu = if !is_dir_status {
                menu.item(
                    git_context_menu_item(
                        labels.open_diff.clone(),
                        HeroIconName::ArrowTopRightOnSquare,
                    )
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&diff_entity, |app, cx| {
                            app.open_git_diff_window(diff_path.clone(), window, cx);
                        });
                    }),
                )
            } else {
                menu
            };
            let menu = if can_discard {
                menu.separator().item(
                    git_context_menu_item(
                        labels.discard_changes.clone(),
                        HeroIconName::ArrowUturnLeft,
                    )
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&discard_entity, |app, cx| {
                            app.select_git_file_only(discard_path.clone(), cx);
                            app.discard_git_paths(
                                app.selected_git_action_paths(&discard_path),
                                window,
                                cx,
                            );
                        });
                    }),
                )
            } else {
                menu
            };
            if is_git_untracked_file(&file) || is_dir_status {
                menu.item(
                    git_context_menu_item(labels.add_gitignore.clone(), HeroIconName::XMark)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&ignore_entity, |app, cx| {
                                app.append_project_gitignore_paths(
                                    app.selected_git_action_paths(&ignore_path),
                                    window,
                                    cx,
                                );
                            });
                        }),
                )
            } else {
                menu
            }
        })
        .into_any_element()
}

#[derive(Clone)]
struct GitFileMenuLabels {
    stage: String,
    unstage: String,
    open_diff: String,
    discard_changes: String,
    add_gitignore: String,
}

impl From<&GitSidebarLabels> for GitFileMenuLabels {
    fn from(labels: &GitSidebarLabels) -> Self {
        Self {
            stage: labels.stage.clone(),
            unstage: labels.unstage.clone(),
            open_diff: labels.open_diff.clone(),
            discard_changes: labels.discard_changes.clone(),
            add_gitignore: labels.add_gitignore.clone(),
        }
    }
}

fn git_context_menu_item(label: String, icon: HeroIconName) -> PopupMenuItem {
    PopupMenuItem::element(move |_window, cx| {
        div()
            .flex()
            .items_center()
            .min_w(px(132.0))
            .text_size(rems(0.875))
            .line_height(rems(1.125))
            .text_color(cx.theme().foreground)
            .child(
                Icon::new(icon.clone())
                    .size_3p5()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(div().ml(px(10.0)).child(label.clone()))
    })
}

pub(in crate::app) fn git_diff_window_workspace(
    selected_path: Option<&str>,
    diff: &str,
    error: Option<&str>,
    derived_rows: Option<&GitReviewDerivedRows>,
    code_scroll_handle: ScrollHandle,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let labels = GitSidebarLabels::load(language);
    let file_path = selected_path
        .unwrap_or(labels.no_selected_file.as_str())
        .to_string();

    div()
        .flex()
        .flex_col()
        .size_full()
        .bg(color(theme::BG))
        .child(
            div()
                .h(px(52.0))
                .pr(px(24.0))
                .when(cfg!(target_os = "macos"), |this| this.pl(px(86.0)))
                .when(!cfg!(target_os = "macos"), |this| this.pl(px(24.0)))
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .truncate()
                                .text_color(color(theme::TEXT))
                                .child("Diff"),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .truncate()
                                .text_color(color(theme::TEXT_DIM))
                                .child(file_path),
                        ),
                ),
        )
        .when_some(error.map(str::to_string), |this, error| {
            this.child(
                div()
                    .mx_4()
                    .mt_3()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(color(theme::ORANGE).opacity(0.35))
                    .bg(color(theme::ORANGE).opacity(0.12))
                    .px_3()
                    .py_2()
                    .text_size(rems(0.75))
                    .line_height(rems(1.0))
                    .text_color(color(theme::ORANGE))
                    .child(error),
            )
        })
        .child(git_diff_window_body(
            diff,
            derived_rows,
            code_scroll_handle,
            labels.empty_diff.clone(),
            labels.review_original.clone(),
            labels.diff_current.clone(),
            cx,
        ))
}

fn git_diff_window_body(
    diff: &str,
    derived_rows: Option<&GitReviewDerivedRows>,
    code_scroll_handle: ScrollHandle,
    empty_label: String,
    original_label: String,
    current_label: String,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    if let Some(rows) = derived_rows {
        return div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(git_diff_window_content_panel(
                "git-diff-window-original-code",
                &original_label,
                rows.original.clone(),
                VirtualListScrollHandle::from(code_scroll_handle.clone()),
                cx,
            ))
            .child(git_diff_window_content_panel(
                "git-diff-window-current-code",
                &current_label,
                rows.final_file.clone(),
                VirtualListScrollHandle::from(code_scroll_handle),
                cx,
            ))
            .into_any_element();
    }

    div()
        .flex_1()
        .min_h_0()
        .overflow_y_scrollbar()
        .bg(color(theme::BG_TERMINAL))
        .px_4()
        .py_3()
        .children(if diff.trim().is_empty() {
            vec![
                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(rems(0.875))
                    .line_height(rems(1.125))
                    .text_color(color(theme::TEXT_DIM))
                    .child(empty_label)
                    .into_any_element(),
            ]
        } else {
            diff.lines()
                .map(|line| git_diff_line_row(line).into_any_element())
                .collect::<Vec<_>>()
        })
        .into_any_element()
}

fn git_diff_window_content_panel(
    list_id: &'static str,
    title: &str,
    cells: Rc<Vec<GitReviewAlignedCell>>,
    scroll_handle: VirtualListScrollHandle,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let item_sizes = Rc::new(vec![size(px(1.0), px(18.0)); cells.len()]);
    let list_cells = cells.clone();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .flex_basis(px(0.0))
        .min_w_0()
        .overflow_hidden()
        .border_r_1()
        .border_color(color(theme::BORDER_SOFT))
        .child(
            div()
                .h(px(30.0))
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .bg(color(theme::BG_TERMINAL))
                .p_2()
                .text_size(rems(0.75))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .font_family("SF Mono")
                .child(
                    v_virtual_list(
                        cx.entity().clone(),
                        list_id,
                        item_sizes,
                        move |_app, visible_range: Range<usize>, _window, _cx| {
                            visible_range
                                .filter_map(|index| {
                                    let cell = list_cells.get(index)?;
                                    Some(git_diff_window_code_line(cell.clone()))
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .track_scroll(&scroll_handle)
                    .with_sizing_behavior(ListSizingBehavior::Auto),
                )
                .vertical_scrollbar(&scroll_handle),
        )
}

fn git_diff_window_code_line(cell: GitReviewAlignedCell) -> AnyElement {
    let (line_bg, gutter_bg, marker_color) = match cell.tone {
        Some(GitReviewLineTone::Addition) => (
            Some(color(theme::GREEN).opacity(0.10)),
            color(theme::GREEN).opacity(0.16),
            color(theme::GREEN),
        ),
        Some(GitReviewLineTone::Deletion) => (
            Some(color(0xF87171).opacity(0.11)),
            color(0xF87171).opacity(0.16),
            color(0xF87171),
        ),
        None => (
            None,
            color(theme::BG_PANEL).opacity(0.72),
            color(theme::TEXT_DIM),
        ),
    };
    div()
        .h(px(18.0))
        .flex()
        .w_full()
        .min_w_0()
        .when_some(line_bg, |this, bg| this.bg(bg))
        .child(
            div()
                .w(px(46.0))
                .h_full()
                .flex_none()
                .pr_2()
                .border_r_1()
                .border_color(color(theme::BORDER_SOFT).opacity(0.55))
                .bg(gutter_bg)
                .text_align(gpui::TextAlign::Right)
                .text_color(marker_color)
                .child(
                    cell.line_number
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_x_hidden()
                .pl_2()
                .child(cell.text),
        )
        .into_any_element()
}

fn git_diff_line_row(line: &str) -> impl IntoElement {
    let line_color = if line.starts_with('+') && !line.starts_with("+++") {
        theme::GREEN
    } else if line.starts_with('-') && !line.starts_with("---") {
        0xF87171
    } else if line.starts_with("@@") {
        theme::ACCENT
    } else {
        theme::TEXT_MUTED
    };

    div()
        .min_h(px(18.0))
        .text_size(rems(0.75))
        .line_height(rems(1.125))
        .font_family("SF Mono")
        .text_color(color(line_color))
        .child(line.to_string())
}

fn git_history_panel(
    git: &GitSummary,
    labels: Rc<GitSidebarLabels>,
    scroll_handle: VirtualListScrollHandle,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let commits = Rc::new(git.commits.clone());
    let commit_count = commits.len();
    let menu_labels = Rc::new(GitHistoryMenuLabels::from(labels.as_ref()));
    let item_sizes = Rc::new(vec![size(px(1.0), px(44.0)); commit_count]);
    div()
        .size_full()
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(38.0))
                .flex_shrink_0()
                .px_3()
                .flex()
                .items_center()
                .bg(cx.theme().list_head)
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(cx.theme().muted_foreground)
                .child(labels.history.clone()),
        )
        .child(if git.commits.is_empty() {
            div()
                .flex_1()
                .px_3()
                .py_4()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT_DIM))
                .child(labels.history_empty.clone())
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .py(px(6.0))
                .child(
                    v_virtual_list(
                        cx.entity().clone(),
                        "git-history-list",
                        item_sizes,
                        move |_app, visible_range: Range<usize>, _window, cx| {
                            visible_range
                                .filter_map(|index| {
                                    commits.get(index).cloned().map(|commit| {
                                        git_history_timeline_row(
                                            &commit,
                                            index == 0,
                                            index == 0,
                                            index + 1 >= commit_count,
                                            menu_labels.clone(),
                                            cx,
                                        )
                                        .into_any_element()
                                    })
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .track_scroll(&scroll_handle)
                    .with_sizing_behavior(ListSizingBehavior::Auto),
                )
                .vertical_scrollbar(&scroll_handle)
                .into_any_element()
        })
}

fn git_history_timeline_row(
    commit: &GitCommitSummary,
    active: bool,
    is_first: bool,
    is_last: bool,
    labels: Rc<GitHistoryMenuLabels>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title = commit.title.clone();
    let author = commit.author.clone();
    let relative_time = commit.relative_time.clone();
    let hash = commit.hash.clone();
    let menu_hash = hash.clone();
    let app_entity = cx.entity();
    let context_entity = app_entity.clone();
    let context_hash = menu_hash.clone();
    let tooltip = format!(
        "{}\n{}\n{} · {}",
        commit.hash, commit.title, commit.author, commit.relative_time
    );

    codux_tooltip_container(
        app_entity.clone(),
        SharedString::from(format!("git-history-{}", commit.hash)),
        tooltip,
    )
    .w_full()
    .min_w_0()
    .relative()
    .h(px(44.0))
    .px_3()
    .py(px(4.0))
    .flex()
    .gap_2()
    .hover(|style| style.bg(cx.theme().list_hover))
    .child(
        div()
            .w(px(18.0))
            .h(px(36.0))
            .relative()
            .flex_shrink_0()
            .when(!is_first, |this| {
                this.child(
                    div()
                        .absolute()
                        .left(px(8.5))
                        .top(px(-4.0))
                        .h(px(13.0))
                        .w(px(1.0))
                        .bg(color(0x7A8599).opacity(0.82)),
                )
            })
            .when(!is_last, |this| {
                this.child(
                    div()
                        .absolute()
                        .left(px(8.5))
                        .top(px(21.0))
                        .bottom(px(-4.0))
                        .w(px(1.0))
                        .bg(color(0x7A8599).opacity(0.82)),
                )
            })
            .child(
                div()
                    .absolute()
                    .left(px(2.5))
                    .top(px(12.0))
                    .size(px(12.0))
                    .rounded_full()
                    .border_1()
                    .border_color(color(theme::BG_COLUMN))
                    .bg(color(if active {
                        theme::ACCENT
                    } else {
                        theme::TEXT_DIM
                    })),
            ),
    )
    .child(
        div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(rems(0.875))
                            .line_height(rems(1.125))
                            .text_color(color(theme::TEXT))
                            .truncate()
                            .child(title),
                    )
                    .child(if active {
                        div()
                            .rounded(px(6.0))
                            .px_2()
                            .py(px(2.0))
                            .bg(color(theme::ACCENT).opacity(0.16))
                            .text_size(rems(0.75))
                            .line_height(rems(0.875))
                            .text_color(color(theme::ACCENT))
                            .child("HEAD->main")
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    }),
            )
            .child(
                div()
                    .text_size(rems(0.75))
                    .line_height(rems(1.0))
                    .text_color(color(theme::TEXT_DIM))
                    .truncate()
                    .child(format!("{author} · {relative_time} · {hash}")),
            ),
    )
    .context_menu(move |menu, _window, _cx| {
        let checkout_hash = context_hash.clone();
        let revert_hash = context_hash.clone();
        let restore_hash = context_hash.clone();
        let checkout_entity = context_entity.clone();
        let revert_entity = context_entity.clone();
        let restore_entity = context_entity.clone();
        menu.item(
            PopupMenuItem::new(labels.checkout_commit.clone())
                .icon(HeroIconName::ArrowPathRoundedSquare)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&checkout_entity, |app, cx| {
                        app.checkout_git_commit(checkout_hash.clone(), window, cx);
                    });
                }),
        )
        .item(
            PopupMenuItem::new(labels.revert_commit.clone())
                .icon(HeroIconName::ArrowUturnLeft)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&revert_entity, |app, cx| {
                        app.revert_git_commit(revert_hash.clone(), window, cx);
                    });
                }),
        )
        .item(
            PopupMenuItem::new(labels.restore_commit.clone())
                .icon(HeroIconName::ArrowUturnRight)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&restore_entity, |app, cx| {
                        app.restore_git_commit(restore_hash.clone(), window, cx);
                    });
                }),
        )
    })
}

#[derive(Clone)]
struct GitHistoryMenuLabels {
    checkout_commit: String,
    revert_commit: String,
    restore_commit: String,
}

impl From<&GitSidebarLabels> for GitHistoryMenuLabels {
    fn from(labels: &GitSidebarLabels) -> Self {
        Self {
            checkout_commit: labels.checkout_commit.clone(),
            revert_commit: labels.revert_commit.clone(),
            restore_commit: labels.restore_commit.clone(),
        }
    }
}

fn is_git_staged_file(file: &GitFileStatus) -> bool {
    let index = file.index_status.trim();
    !index.is_empty() && index != "?"
}

fn is_git_worktree_file(file: &GitFileStatus) -> bool {
    !is_git_untracked_file(file) && !file.worktree_status.trim().is_empty()
}

fn is_git_untracked_file(file: &GitFileStatus) -> bool {
    file.worktree_status == "?" && (file.index_status == "?" || file.index_status.trim().is_empty())
}

fn git_file_status_label(file: &GitFileStatus) -> String {
    if is_git_untracked_file(file) {
        "A".to_string()
    } else {
        let status = format!(
            "{}{}",
            file.index_status.trim(),
            file.worktree_status.trim()
        );
        if status.is_empty() {
            "M".to_string()
        } else {
            status
        }
    }
}

fn git_file_status_color(status: &str) -> u32 {
    match status.chars().next().unwrap_or('?') {
        'A' | '?' => theme::GREEN,
        'D' => theme::ACCENT,
        'M' => theme::ORANGE,
        'R' | 'C' | 'T' => theme::ORANGE,
        _ => theme::TEXT_DIM,
    }
}

pub(in crate::app) fn git_review_workspace(
    selected_path: Option<&str>,
    review: &GitReviewSummary,
    content: Option<&GitReviewContentSummary>,
    derived_rows: Option<&GitReviewDerivedRows>,
    labels: Rc<GitSidebarLabels>,
    code_scroll_handle: ScrollHandle,
    cx: &mut Context<workspace_views::ReviewDiffContentView>,
) -> impl IntoElement {
    if !review.is_repository {
        return git_review_empty_workspace(labels.review_no_repository.clone()).into_any_element();
    }
    let Some(selected_path) = selected_path else {
        return git_review_empty_workspace(labels.review_select_file.clone()).into_any_element();
    };
    let Some(content) = content else {
        return git_review_empty_workspace(labels.review_select_file.clone()).into_any_element();
    };
    if let Some(error) = content.error.clone() {
        let message = git_review_error_message(&error, labels.as_ref());
        return git_review_empty_workspace(message).into_any_element();
    }
    let task_branch_mode = review.mode == "taskBranch";
    let original_title = labels.review_original.clone();
    let body = div()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .child(
            div()
                .h(px(44.0))
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .child(
                    div()
                        .min_w_0()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(selected_path.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .child(
                            div()
                                .text_color(color(theme::GREEN))
                                .child(format!("+{}", content.added_lines.len())),
                        )
                        .child(
                            div()
                                .text_color(color(0xF47C7C))
                                .child(format!("-{}", content.deleted_lines.len())),
                        ),
                ),
        )
        .child({
            let derived_rows = derived_rows.cloned().unwrap_or_default();
            div()
                .flex()
                .flex_1()
                .flex_basis(px(0.0))
                .min_h_0()
                .min_w_0()
                .overflow_hidden()
                .child(git_review_content_panel(
                    "git-review-original-code",
                    original_title.as_str(),
                    derived_rows.original.clone(),
                    VirtualListScrollHandle::from(code_scroll_handle.clone()),
                    cx,
                ))
                .child(git_review_content_panel(
                    "git-review-new-code",
                    labels.review_new_file.as_str(),
                    derived_rows.new_file.clone(),
                    VirtualListScrollHandle::from(code_scroll_handle.clone()),
                    cx,
                ))
                .child(git_review_content_panel(
                    "git-review-final-code",
                    labels.review_final_file.as_str(),
                    derived_rows.final_file.clone(),
                    VirtualListScrollHandle::from(code_scroll_handle.clone()),
                    cx,
                ))
                .when(task_branch_mode, |this| {
                    this.child(git_review_content_panel(
                        "git-review-branch-code",
                        labels.review_branch.as_str(),
                        derived_rows.branch.clone().unwrap_or_default(),
                        VirtualListScrollHandle::from(code_scroll_handle.clone()),
                        cx,
                    ))
                })
        });
    body.into_any_element()
}

fn git_review_empty_workspace(message: String) -> impl IntoElement {
    div()
        .flex()
        .flex_1()
        .size_full()
        .min_h_0()
        .items_center()
        .justify_center()
        .text_color(color(theme::TEXT_DIM))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(Icon::new(HeroIconName::DocumentText).size_6())
                .child(
                    div()
                        .text_size(rems(0.8125))
                        .line_height(rems(1.125))
                        .child(message),
                ),
        )
}

fn git_review_error_message(error: &str, labels: &GitSidebarLabels) -> String {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("could not find repository")
        || normalized.contains("not a git repository")
        || normalized.contains("no git repository")
    {
        return labels.review_no_repository.clone();
    }

    error.to_string()
}

#[derive(Clone, Copy)]
enum GitReviewLineTone {
    Addition,
    Deletion,
}

#[derive(Clone, Default)]
pub(in crate::app) struct GitReviewDerivedRows {
    original: Rc<Vec<GitReviewAlignedCell>>,
    new_file: Rc<Vec<GitReviewAlignedCell>>,
    final_file: Rc<Vec<GitReviewAlignedCell>>,
    branch: Option<Rc<Vec<GitReviewAlignedCell>>>,
}

#[derive(Clone, Default)]
struct GitReviewAlignedCell {
    line_number: Option<usize>,
    text: String,
    tone: Option<GitReviewLineTone>,
}

fn git_review_content_panel(
    list_id: &'static str,
    title: &str,
    cells: Rc<Vec<GitReviewAlignedCell>>,
    scroll_handle: VirtualListScrollHandle,
    cx: &mut Context<workspace_views::ReviewDiffContentView>,
) -> impl IntoElement {
    let item_sizes = Rc::new(vec![size(px(1.0), px(18.0)); cells.len()]);
    let list_cells = cells.clone();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .flex_basis(px(0.0))
        .min_w_0()
        .overflow_hidden()
        .border_r_1()
        .border_color(color(theme::BORDER_SOFT))
        .child(
            div()
                .h(px(30.0))
                .px_2()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .bg(color(theme::BG_TERMINAL))
                .p_2()
                .text_size(rems(0.75))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .font_family("SF Mono")
                .child(
                    v_virtual_list(
                        cx.entity().clone(),
                        list_id,
                        item_sizes,
                        move |_view, visible_range: Range<usize>, _window, _cx| {
                            visible_range
                                .filter_map(|index| {
                                    let cell = list_cells.get(index)?;
                                    Some(git_review_code_line(cell.clone()))
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .track_scroll(&scroll_handle)
                    .with_sizing_behavior(ListSizingBehavior::Auto),
                )
                .vertical_scrollbar(&scroll_handle),
        )
}

fn git_review_code_line(cell: GitReviewAlignedCell) -> AnyElement {
    let line_bg = match cell.tone {
        Some(GitReviewLineTone::Addition) => Some(color(theme::GREEN).opacity(0.13)),
        Some(GitReviewLineTone::Deletion) => Some(color(0xF87171).opacity(0.14)),
        None => None,
    };
    div()
        .h(px(18.0))
        .flex()
        .w_full()
        .min_w_0()
        .when_some(line_bg, |this, bg| this.bg(bg))
        .child(
            div()
                .w(px(44.0))
                .flex_none()
                .pr_2()
                .text_align(gpui::TextAlign::Right)
                .text_color(color(theme::TEXT_DIM))
                .child(
                    cell.line_number
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_x_hidden()
                .child(cell.text),
        )
        .into_any_element()
}

pub(in crate::app) fn build_git_review_derived_rows(
    original_content: &str,
    new_content: &str,
    final_content: &str,
    branch_content: Option<&str>,
    deleted_lines: &[usize],
    added_lines: &[usize],
) -> GitReviewDerivedRows {
    let original_lines = split_review_lines(original_content);
    let new_lines = split_review_lines(new_content);
    let final_lines = split_review_lines(final_content);
    let branch_lines = branch_content.map(split_review_lines);
    let deleted = deleted_lines.iter().copied().collect::<HashSet<_>>();
    let added = added_lines.iter().copied().collect::<HashSet<_>>();

    let mut original_cells = Vec::new();
    let mut new_cells = Vec::new();
    let mut final_cells = Vec::new();
    let mut branch_cells = branch_lines.as_ref().map(|_| Vec::new());
    let mut old_line = 1usize;
    let mut new_line = 1usize;

    while original_cells.len() < 600
        && (old_line <= original_lines.len() || new_line <= final_lines.len())
    {
        if deleted.contains(&old_line) || added.contains(&new_line) {
            let mut deleted_block = Vec::new();
            while old_line <= original_lines.len() && deleted.contains(&old_line) {
                deleted_block.push(old_line);
                old_line += 1;
            }

            let mut added_block = Vec::new();
            while new_line <= final_lines.len() && added.contains(&new_line) {
                added_block.push(new_line);
                new_line += 1;
            }

            let block_len = deleted_block.len().max(added_block.len()).max(1);
            for offset in 0..block_len {
                let old_number = deleted_block.get(offset).copied();
                let new_number = added_block.get(offset).copied();
                original_cells.push(review_cell(
                    &original_lines,
                    old_number,
                    Some(GitReviewLineTone::Deletion),
                ));
                new_cells.push(review_cell(
                    &new_lines,
                    new_number,
                    Some(GitReviewLineTone::Addition),
                ));
                final_cells.push(review_cell(
                    &final_lines,
                    new_number,
                    Some(GitReviewLineTone::Addition),
                ));
                if let (Some(lines), Some(cells)) = (&branch_lines, &mut branch_cells) {
                    cells.push(review_cell(
                        lines,
                        new_number,
                        Some(GitReviewLineTone::Addition),
                    ));
                }
            }
        } else {
            original_cells.push(review_cell(&original_lines, Some(old_line), None));
            new_cells.push(review_cell(&new_lines, Some(new_line), None));
            final_cells.push(review_cell(&final_lines, Some(new_line), None));
            if let (Some(lines), Some(cells)) = (&branch_lines, &mut branch_cells) {
                cells.push(review_cell(lines, Some(new_line), None));
            }
            old_line += 1;
            new_line += 1;
        }
    }

    GitReviewDerivedRows {
        original: Rc::new(original_cells),
        new_file: Rc::new(new_cells),
        final_file: Rc::new(final_cells),
        branch: branch_cells.map(Rc::new),
    }
}

fn split_review_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .map(|line| line.chars().take(160).collect::<String>())
        .collect()
}

fn review_cell(
    lines: &[String],
    line_number: Option<usize>,
    tone: Option<GitReviewLineTone>,
) -> GitReviewAlignedCell {
    let text = line_number.and_then(|number| lines.get(number.saturating_sub(1)).cloned());
    GitReviewAlignedCell {
        line_number: if text.is_some() { line_number } else { None },
        text: text.unwrap_or_default(),
        tone,
    }
}

pub(in crate::app) fn git_review_file_list(
    app_entity: gpui::Entity<CoduxApp>,
    review: &GitReviewSummary,
    selected_path: Option<&str>,
    expanded_dirs: &HashSet<String>,
    refreshing: bool,
    labels: Rc<GitSidebarLabels>,
    cx: &mut Context<workspace_views::ReviewFileListView>,
) -> impl IntoElement {
    let expanded_dirs = expanded_dirs.clone();
    div()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .bg(color(theme::BG_PANEL).opacity(0.35))
        .child(
            div()
                .h(px(38.0))
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .text_size(rems(0.75))
                .text_color(color(theme::TEXT_DIM))
                .child(labels.review_changed_files.clone())
                .child(
                    Button::new("git-review-refresh")
                        .compact()
                        .ghost()
                        .loading(refreshing)
                        .icon(Icon::new(HeroIconName::ArrowPath).size_4())
                        .on_click(cx.listener({
                            let app_entity = app_entity.clone();
                            move |_view, _event, _window, cx| {
                                app_entity.update(cx, |app, app_cx| {
                                    app.refresh_git_panel_state_async(app_cx);
                                });
                            }
                        })),
                ),
        )
        .child(if review.files.is_empty() {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .text_size(rems(0.8125))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT_DIM))
                .child(if review.is_repository {
                    labels.review_empty.clone()
                } else {
                    labels.review_no_repository.clone()
                })
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .py_2()
                .children(git_review_directory_rows(
                    &review.files,
                    "",
                    0,
                    selected_path,
                    &expanded_dirs,
                    labels.clone(),
                    app_entity.clone(),
                    cx,
                ))
                .into_any_element()
        })
}

fn git_review_directory_rows(
    files: &[GitReviewFile],
    base_path: &str,
    depth: usize,
    selected_path: Option<&str>,
    expanded_dirs: &HashSet<String>,
    labels: Rc<GitSidebarLabels>,
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<workspace_views::ReviewFileListView>,
) -> Vec<AnyElement> {
    let (dirs, direct_files) = collect_immediate_git_review_entries(base_path, files);
    let mut rows = Vec::new();
    for (name, dir) in dirs {
        if rows.len() >= MAX_GIT_REVIEW_TREE_ROWS {
            rows.push(git_review_tree_limit_row(files.len(), &labels).into_any_element());
            return rows;
        }
        let expanded = expanded_dirs.contains(&git_status_tree_key("review", &dir.path));
        rows.push(
            git_review_dir_row(&name, &dir, expanded, depth, app_entity.clone(), cx)
                .into_any_element(),
        );
        if expanded {
            rows.extend(git_review_directory_rows(
                files,
                &dir.path,
                depth + 1,
                selected_path,
                expanded_dirs,
                labels.clone(),
                app_entity.clone(),
                cx,
            ));
        }
    }
    for file in direct_files {
        if rows.len() >= MAX_GIT_REVIEW_TREE_ROWS {
            rows.push(git_review_tree_limit_row(files.len(), &labels).into_any_element());
            return rows;
        }
        let selected = selected_path == Some(file.path.as_str());
        rows.push(
            git_review_file_row(file, selected, depth, app_entity.clone(), cx).into_any_element(),
        );
    }
    rows
}

const MAX_GIT_REVIEW_TREE_ROWS: usize = 600;

fn git_review_tree_limit_row(total: usize, labels: &GitSidebarLabels) -> impl IntoElement {
    let message = labels
        .review_tree_limit
        .replacen("%@", &MAX_GIT_REVIEW_TREE_ROWS.to_string(), 1)
        .replacen("%@", &total.to_string(), 1);
    div()
        .h(px(30.0))
        .px_3()
        .flex()
        .items_center()
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(color(theme::TEXT_DIM))
        .child(message)
}

fn git_review_file_row(
    file: GitReviewFile,
    selected: bool,
    depth: usize,
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<workspace_views::ReviewFileListView>,
) -> impl IntoElement {
    let path = file.path.clone();
    let badge = git_review_status_badge(&file.status);
    let file_name = file
        .path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(file.path.as_str())
        .to_string();
    Button::new(format!("review-file-{path}"))
        .ghost()
        .w_full()
        .h(px(24.0))
        .px_2()
        .rounded_sm()
        .text_color(if selected {
            color(theme::TEXT)
        } else {
            color(theme::TEXT_MUTED)
        })
        .when(selected, |this| this.bg(color(theme::ACCENT).opacity(0.13)))
        .on_click(cx.listener(move |_view, _event: &ClickEvent, _window, cx| {
            app_entity.update(cx, |app, app_cx| {
                app.load_git_file_diff_async(path.clone(), app_cx);
            });
        }))
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .items_center()
                        .text_color(color(theme::TEXT_MUTED))
                        .child(div().flex_none().w(px(28.0 + depth as f32 * 18.0)))
                        .child(
                            Icon::new(review_file_icon(&file.status))
                                .size_3p5()
                                .flex_none(),
                        )
                        .child(
                            div()
                                .flex_1()
                                .ml(px(8.0))
                                .min_w_0()
                                .max_w_full()
                                .truncate()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .child(file_name),
                        ),
                )
                .child(git_review_stats_cells(
                    None,
                    None,
                    Some((badge.0.to_string(), badge.1, true)),
                )),
        )
}

fn git_review_dir_row(
    name: &str,
    dir: &GitReviewDirSummary,
    expanded: bool,
    depth: usize,
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<workspace_views::ReviewFileListView>,
) -> impl IntoElement {
    let path = dir.path.clone();
    Button::new(format!("review-dir-{path}"))
        .ghost()
        .w_full()
        .h(px(24.0))
        .px_2()
        .rounded_sm()
        .text_color(color(theme::TEXT_MUTED))
        .on_click(cx.listener(move |_view, _event, _window, cx| {
            app_entity.update(cx, |app, app_cx| {
                app.toggle_git_review_dir(path.clone(), app_cx);
            });
        }))
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .items_center()
                        .child(div().flex_none().w(px(depth as f32 * 18.0)))
                        .child(
                            Icon::new(if expanded {
                                HeroIconName::ChevronDown
                            } else {
                                HeroIconName::ChevronRight
                            })
                            .size_3()
                            .flex_none()
                            .text_color(color(theme::TEXT_DIM)),
                        )
                        .child(
                            Icon::new(if expanded {
                                HeroIconName::FolderOpen
                            } else {
                                HeroIconName::Folder
                            })
                            .size_4()
                            .ml(px(8.0))
                            .flex_none()
                            .text_color(color(theme::ACCENT)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .ml(px(8.0))
                                .min_w_0()
                                .max_w_full()
                                .truncate()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .child(name.to_string()),
                        ),
                )
                .child(git_review_stats_cells(
                    None,
                    None,
                    Some((dir.count.to_string(), color(theme::TEXT_DIM), false)),
                )),
        )
}

fn git_review_stats_cells(
    additions: Option<(String, gpui::Hsla, bool)>,
    deletions: Option<(String, gpui::Hsla, bool)>,
    trailing: Option<(String, gpui::Hsla, bool)>,
) -> impl IntoElement {
    div()
        .flex_none()
        .w(if additions.is_some() || deletions.is_some() {
            px(78.0)
        } else {
            px(24.0)
        })
        .flex()
        .items_center()
        .justify_end()
        .gap(px(8.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .when_some(additions, |this, cell| {
            this.child(git_review_stat_cell(cell, px(28.0)))
        })
        .when_some(deletions, |this, cell| {
            this.child(git_review_stat_cell(cell, px(28.0)))
        })
        .when_some(trailing, |this, cell| {
            this.child(git_review_stat_cell(cell, px(18.0)))
        })
}

fn git_review_stat_cell(
    (label, label_color, strong): (String, gpui::Hsla, bool),
    width: Pixels,
) -> impl IntoElement {
    div()
        .w(width)
        .h(px(16.0))
        .overflow_hidden()
        .truncate()
        .text_align(gpui::TextAlign::Right)
        .text_color(label_color)
        .when(strong, |this| this.font_weight(FontWeight::BOLD))
        .child(label)
}

fn git_review_status_badge(status: &str) -> (&'static str, gpui::Hsla) {
    match status {
        "added" => ("A", color(theme::GREEN)),
        "deleted" => ("D", color(theme::ACCENT)),
        "renamed" => ("R", color(theme::ORANGE)),
        "copied" => ("C", color(theme::ACCENT)),
        "typeChanged" => ("T", color(theme::ORANGE)),
        "modified" => ("M", color(theme::ORANGE)),
        _ => ("?", color(theme::TEXT_DIM)),
    }
}

fn review_file_icon(status: &str) -> HeroIconName {
    match status {
        "added" => HeroIconName::DocumentPlus,
        "deleted" => HeroIconName::DocumentMinus,
        "renamed" => HeroIconName::ArrowPath,
        _ => HeroIconName::Document,
    }
}

#[derive(Clone)]
struct GitReviewDirSummary {
    path: String,
    count: usize,
    additions: i64,
    deletions: i64,
}

fn collect_immediate_git_review_entries(
    base_path: &str,
    files: &[GitReviewFile],
) -> (BTreeMap<String, GitReviewDirSummary>, Vec<GitReviewFile>) {
    let mut dirs = BTreeMap::<String, GitReviewDirSummary>::new();
    let mut direct_files = Vec::<GitReviewFile>::new();
    for file in files {
        let Some(relative_path) = relative_git_status_path(base_path, &file.path) else {
            continue;
        };
        let relative_path = relative_path.trim_end_matches('/');
        if relative_path.is_empty() {
            continue;
        }
        if let Some((dir_name, _rest)) = relative_path.split_once('/') {
            let dir_path = join_git_path(base_path, dir_name);
            dirs.entry(dir_name.to_string())
                .and_modify(|dir| {
                    dir.count += 1;
                    dir.additions += file.additions.max(0);
                    dir.deletions += file.deletions.max(0);
                })
                .or_insert(GitReviewDirSummary {
                    path: dir_path,
                    count: 1,
                    additions: file.additions.max(0),
                    deletions: file.deletions.max(0),
                });
        } else if file.path.ends_with('/') {
            let dir_path = join_git_path(base_path, relative_path);
            dirs.entry(relative_path.to_string())
                .and_modify(|dir| {
                    dir.count += 1;
                    dir.additions += file.additions.max(0);
                    dir.deletions += file.deletions.max(0);
                })
                .or_insert(GitReviewDirSummary {
                    path: dir_path,
                    count: 1,
                    additions: file.additions.max(0),
                    deletions: file.deletions.max(0),
                });
        } else {
            direct_files.push(file.clone());
        }
    }
    (dirs, direct_files)
}
