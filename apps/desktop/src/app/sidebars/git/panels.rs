use super::*;

pub(super) fn git_panel_header(
    git: &GitSummary,
    branch: &str,
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
    let stashes = git.stashes.clone();
    let tags = git.tags.clone();
    let changed_paths: Vec<String> = git
        .changed_files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    let has_staged = git.staged > 0;
    let app_entity = cx.entity();

    div()
        .h(px(44.0))
        .px_3()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_between()
        // Thin translucent darkening: deeper than the panel, still see-through.
        .bg(theme::vibrancy_raised(color(theme::BG_HEADER)))
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
                                    GitBranchMenuInput {
                                        branches: branches.clone(),
                                        remote_branches: remote_branches.clone(),
                                        remotes: remotes.clone(),
                                        default_push_remote: default_push_remote.clone(),
                                        current_branch: current_branch.clone(),
                                        upstream: upstream.clone(),
                                        has_commits,
                                        stashes: stashes.clone(),
                                        tags: tags.clone(),
                                        changed_paths: changed_paths.clone(),
                                        has_staged,
                                        language: language.clone(),
                                        app_entity: app_entity.clone(),
                                    },
                                    window,
                                    cx,
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

pub(super) fn git_repository_panel(
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

pub(super) fn git_commit_panel(
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

pub(super) struct GitFilesPanelInput<'a> {
    pub(super) staged: &'a [GitFileStatus],
    pub(super) changed: &'a [GitFileStatus],
    pub(super) untracked: &'a [GitFileStatus],
    pub(super) expanded_sections: &'a HashSet<String>,
    pub(super) expanded_dirs: &'a HashSet<String>,
    pub(super) tree_children: &'a HashMap<String, Vec<GitFileStatus>>,
    pub(super) selected_file: Option<&'a str>,
    pub(super) selected_files: &'a HashSet<String>,
    pub(super) labels: Rc<GitSidebarLabels>,
    pub(super) scroll_handle: VirtualListScrollHandle,
}

pub(super) fn git_files_panel(
    input: GitFilesPanelInput<'_>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let GitFilesPanelInput {
        staged,
        changed,
        untracked,
        expanded_sections,
        expanded_dirs,
        tree_children,
        selected_file,
        selected_files,
        labels,
        scroll_handle,
    } = input;
    let rows = Rc::new(git_status_virtual_rows(GitStatusRowsInput {
        staged,
        changed,
        untracked,
        expanded_sections,
        expanded_dirs,
        tree_children,
        selected_file,
        selected_files,
        labels: &labels,
    }));
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
