use super::*;
use gpui_component::input::{Input, InputEvent, InputState};

pub(in crate::app) fn git_section(
    git: &GitSummary,
    expanded_sections: &HashSet<String>,
    expanded_dirs: &HashSet<String>,
    tree_children: &HashMap<String, Vec<GitFileStatus>>,
    selected_file: Option<&str>,
    selected_branch: Option<&str>,
    default_push_remote: Option<&str>,
    clone_remote_url: &str,
    _remote_name: &str,
    _remote_url: &str,
    running_operation: Option<&GitRunningOperation>,
    commit_message: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let branch = if git.branch.trim().is_empty() {
        "HEAD"
    } else {
        git.branch.as_str()
    };

    div()
        .flex()
        .min_h_0()
        .flex_col()
        .child(git_panel_header(
            git,
            branch,
            selected_branch,
            default_push_remote,
            running_operation,
            cx,
        ))
        .child(if git.is_repository {
            git_repository_panel(
                git,
                expanded_sections,
                expanded_dirs,
                tree_children,
                selected_file,
                commit_message,
                window,
                cx,
            )
            .into_any_element()
        } else {
            git_empty_repository_panel(clone_remote_url, window, cx).into_any_element()
        })
}

fn git_panel_header(
    git: &GitSummary,
    branch: &str,
    _selected_branch: Option<&str>,
    default_push_remote: Option<&str>,
    running_operation: Option<&GitRunningOperation>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let branches = git.branches.clone();
    let remote_branches = git.remote_branches.clone();
    let remotes = git.remotes.clone();
    let default_push_remote = default_push_remote.map(str::to_string);
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
            div().flex().items_center().min_w_0().child(
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
                                    .text_size(px(14.0))
                                    .line_height(px(18.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(branch.to_string()),
                            )
                            .child(
                                Icon::new(IconName::ChevronDown)
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
                            app_entity.clone(),
                        )
                    }),
            ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .child(assistant_header_icon_button(
                    "git-sidebar-ai",
                    IconName::Asterisk,
                    cx,
                    |app, _event, window, cx| app.generate_git_commit_message_with_ai(window, cx),
                ))
                .when_some(running_operation, |this, operation| {
                    if operation.cancellable {
                        this.child(assistant_header_icon_button(
                            "git-sidebar-cancel",
                            IconName::CircleX,
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
                                    Icon::new(IconName::LoaderCircle)
                                        .size_3p5()
                                        .text_color(cx.theme().secondary_foreground),
                                ),
                        )
                    }
                })
                .child(assistant_header_icon_button(
                    "git-sidebar-refresh",
                    IconName::Redo2,
                    cx,
                    |app, _event, window, cx| app.reload_project_git(window, cx),
                )),
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
    app_entity: gpui::Entity<CoduxApp>,
) -> PopupMenu {
    if branches.is_empty() && remote_branches.is_empty() && remotes.is_empty() {
        return menu.item(
            PopupMenuItem::new("暂无 Git 分支")
                .icon(IconName::Github)
                .disabled(true),
        );
    }

    let create_entity = app_entity.clone();
    let menu = menu.item(
        PopupMenuItem::new("新建分支")
            .icon(IconName::Plus)
            .on_click(move |_, window, cx| {
                cx.update_entity(&create_entity, |app, cx| {
                    app.create_git_branch(window, cx);
                });
            }),
    );

    let local_branches = branches.clone();
    let local_entity = app_entity.clone();
    let menu = menu.submenu_with_icon(
        Some(Icon::new(IconName::Github)),
        "本地分支",
        window,
        cx,
        move |menu, window, cx| {
            if local_branches.is_empty() {
                return menu.item(
                    PopupMenuItem::new("暂无本地分支")
                        .icon(IconName::Github)
                        .disabled(true),
                );
            }

            local_branches.iter().take(40).fold(menu, |menu, branch| {
                let branch_name = branch.name.clone();
                let is_current = branch.is_current;
                let submenu_entity = local_entity.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(if is_current {
                        IconName::Check
                    } else {
                        IconName::Github
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
                            PopupMenuItem::new("切换分支")
                                .icon(IconName::Check)
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
                            PopupMenuItem::new("合并到当前分支")
                                .icon(IconName::Redo2)
                                .disabled(is_current)
                                .on_click(move |_, window, cx| {
                                    cx.update_entity(&merge_entity, |app, cx| {
                                        app.merge_git_branch(merge_branch.clone(), window, cx);
                                    });
                                }),
                        )
                        .item(
                            PopupMenuItem::new("压缩合并到当前分支")
                                .icon(IconName::Redo)
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
                            PopupMenuItem::new("删除本地分支")
                                .icon(IconName::Delete)
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
    let menu = menu.submenu(
        "合并到当前分支",
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
                    PopupMenuItem::new("暂无可合并分支")
                        .icon(IconName::Redo2)
                        .disabled(true),
                );
            }

            candidates.into_iter().fold(menu, |menu, branch| {
                let branch_name = branch.name.clone();
                let app_entity = merge_entity.clone();
                menu.item(
                    PopupMenuItem::new(branch.name.clone())
                        .icon(IconName::Redo2)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&app_entity, |app, cx| {
                                app.merge_git_branch(branch_name.clone(), window, cx);
                            });
                        }),
                )
            })
        },
    );

    let remote_branch_items = remote_branches.clone();
    let remote_branch_entity = app_entity.clone();
    let menu = menu.submenu("远程分支", window, cx, move |menu, window, cx| {
        let fetch_entity = remote_branch_entity.clone();
        let menu = menu.item(
            PopupMenuItem::new("刷新远程分支")
                .icon(IconName::Redo2)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&fetch_entity, |app, cx| {
                        app.fetch_project_git(window, cx);
                    });
                }),
        );

        if remote_branch_items.is_empty() {
            return menu.separator().item(
                PopupMenuItem::new("暂无远程分支")
                    .icon(IconName::ArrowDown)
                    .disabled(true),
            );
        }

        remote_branch_items
            .iter()
            .take(80)
            .fold(menu.separator(), |menu, remote_branch| {
                let checkout_branch = remote_branch.clone();
                let checkout_entity = remote_branch_entity.clone();
                let push_branch = remote_branch.clone();
                let push_entity = remote_branch_entity.clone();
                menu.submenu(
                    remote_branch.clone(),
                    window,
                    cx,
                    move |menu, _window, _cx| {
                        let checkout_branch = checkout_branch.clone();
                        let checkout_entity = checkout_entity.clone();
                        let push_branch = push_branch.clone();
                        let push_entity = push_entity.clone();

                        menu.item(
                            PopupMenuItem::new("检出为本地分支")
                                .icon(IconName::ArrowDown)
                                .on_click(move |_, window, cx| {
                                    cx.update_entity(&checkout_entity, |app, cx| {
                                        app.checkout_git_remote_branch(
                                            checkout_branch.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .item(
                            PopupMenuItem::new("推送到此分支")
                                .icon(IconName::ArrowUp)
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
    });

    let remote_items = remotes.clone();
    let remote_entity = app_entity.clone();
    let default_remote = default_push_remote.clone();
    let menu = menu.submenu("远程仓库", window, cx, move |menu, window, cx| {
        if remote_items.is_empty() {
            return menu.item(
                PopupMenuItem::new("暂无远程仓库")
                    .icon(IconName::Globe)
                    .disabled(true),
            );
        }

        remote_items.iter().fold(menu, |menu, remote| {
            let is_default = default_remote
                .as_deref()
                .map(|name| name == remote.name)
                .unwrap_or(false);
            let remote_name = remote.name.clone();
            let remote_url = remote.url.clone();
            let set_entity = remote_entity.clone();
            let remove_entity = remote_entity.clone();
            menu.submenu_with_icon(
                Some(Icon::new(if is_default {
                    IconName::Check
                } else {
                    IconName::Globe
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
                        PopupMenuItem::new("设为默认")
                            .icon(IconName::Check)
                            .checked(is_default)
                            .on_click(move |_, window, cx| {
                                cx.update_entity(&set_entity, |app, cx| {
                                    app.set_project_default_push_remote(
                                        Some(set_remote.clone()),
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    )
                    .item(
                        PopupMenuItem::new("复制 URL")
                            .icon(IconName::Copy)
                            .on_click(move |_, _window, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_url.clone()));
                            }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("移除远程仓库")
                            .icon(IconName::Delete)
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
    });

    let clear_default_entity = app_entity.clone();
    let menu = menu.item(
        PopupMenuItem::new("清空默认远程")
            .icon(IconName::Delete)
            .disabled(default_push_remote.is_none())
            .on_click(move |_, window, cx| {
                cx.update_entity(&clear_default_entity, |app, cx| {
                    app.set_project_default_push_remote(None, window, cx);
                });
            }),
    );

    let fetch_entity = app_entity.clone();
    let pull_entity = app_entity.clone();
    let push_entity = app_entity.clone();
    let menu =
        menu.separator()
            .item(
                PopupMenuItem::new("拉取远程状态")
                    .icon(IconName::ArrowDown)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&fetch_entity, |app, cx| {
                            app.fetch_project_git(window, cx);
                        });
                    }),
            )
            .item(
                PopupMenuItem::new("拉取")
                    .icon(IconName::ArrowDown)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&pull_entity, |app, cx| {
                            app.pull_project_git(window, cx);
                        });
                    }),
            )
            .item(PopupMenuItem::new("推送").icon(IconName::ArrowUp).on_click(
                move |_, window, cx| {
                    cx.update_entity(&push_entity, |app, cx| {
                        app.push_project_git(window, cx);
                    });
                },
            ));

    let push_remotes = remotes.clone();
    let push_remote_entity = app_entity.clone();
    let menu = menu.submenu("推送到...", window, cx, move |menu, _window, _cx| {
        if push_remotes.is_empty() {
            return menu.item(
                PopupMenuItem::new("暂无远程仓库")
                    .icon(IconName::Globe)
                    .disabled(true),
            );
        }

        push_remotes.iter().fold(menu, |menu, remote| {
            let remote_name = remote.name.clone();
            let app_entity = push_remote_entity.clone();
            menu.item(
                PopupMenuItem::new(remote.name.clone())
                    .icon(IconName::ArrowUp)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&app_entity, |app, cx| {
                            app.push_project_git_remote(remote_name.clone(), window, cx);
                        });
                    }),
            )
        })
    });

    let force_push_entity = app_entity.clone();
    let undo_entity = app_entity.clone();
    let edit_entity = app_entity.clone();
    let reveal_entity = app_entity.clone();
    menu.separator()
        .item(
            PopupMenuItem::new("强制推送")
                .icon(IconName::TriangleAlert)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&force_push_entity, |app, cx| {
                        app.force_push_project_git(window, cx);
                    });
                }),
        )
        .item(
            PopupMenuItem::new("撤销上次提交")
                .icon(IconName::Undo2)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&undo_entity, |app, cx| {
                        app.undo_last_git_commit(window, cx);
                    });
                }),
        )
        .item(
            PopupMenuItem::new("编辑上次提交信息")
                .icon(IconName::Redo)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&edit_entity, |app, cx| {
                        app.load_last_git_commit_message(window, cx);
                    });
                }),
        )
        .item(
            PopupMenuItem::new("在文件管理器显示仓库")
                .icon(IconName::FolderOpen)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&reveal_entity, |app, cx| {
                        app.reveal_selected_project_in_file_manager(window, cx);
                    });
                }),
        )
}

fn git_repository_panel(
    git: &GitSummary,
    expanded_sections: &HashSet<String>,
    expanded_dirs: &HashSet<String>,
    tree_children: &HashMap<String, Vec<GitFileStatus>>,
    selected_file: Option<&str>,
    commit_message: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let staged = git
        .changed_files
        .iter()
        .filter(|file| is_git_staged_file(file))
        .cloned()
        .collect::<Vec<_>>();
    let changed = git
        .changed_files
        .iter()
        .filter(|file| is_git_worktree_file(file))
        .cloned()
        .collect::<Vec<_>>();
    let untracked = git
        .changed_files
        .iter()
        .filter(|file| is_git_untracked_file(file))
        .cloned()
        .collect::<Vec<_>>();

    div()
        .flex()
        .flex_1()
        .min_h_0()
        .flex_col()
        .child(git_commit_panel(commit_message, window, cx))
        .child(
            v_resizable("git-sidebar-file-history-split")
                .child(
                    resizable_panel()
                        .size_range(px(160.0)..px(900.0))
                        .child(git_files_panel(
                            &staged,
                            &changed,
                            &untracked,
                            expanded_sections,
                            expanded_dirs,
                            tree_children,
                            selected_file,
                            cx,
                        )),
                )
                .child(
                    resizable_panel()
                        .size(px(260.0))
                        .size_range(px(180.0)..px(420.0))
                        .child(git_history_panel(git, cx)),
                ),
        )
}

fn git_commit_panel(
    commit_message: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let button_bg = color(theme::ACCENT).opacity(0.70);
    let app_entity = cx.entity();
    let value = commit_message.to_string();
    let input_state = window.use_keyed_state("git-commit-message", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .placeholder("填写提交说明")
    });
    input_state.update(cx, |state, cx| {
        if state.value().as_ref() != commit_message {
            state.set_value(commit_message.to_string(), window, cx);
        }
    });
    cx.subscribe_in(&input_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_git_commit_message(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .h(px(158.0))
        .flex_shrink_0()
        .p(px(12.0))
        .border_b_1()
        .border_color(color(theme::BORDER_SOFT))
        .child(
            div()
                .h(px(90.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(color(0xFFFFFF).opacity(0.06))
                .bg(color(0xFFFFFF).opacity(0.06))
                .p(px(14.0))
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(color(theme::TEXT_DIM))
                .child(Input::new(&input_state).with_size(gpui_component::Size::Medium)),
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
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("提交"),
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
                            Icon::new(IconName::ChevronDown)
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
                            menu.item(PopupMenuItem::new("提交").icon(IconName::Check).on_click(
                                move |_, window, cx| {
                                    cx.update_entity(&commit_entity, |app, cx| {
                                        app.commit_staged_git(window, cx);
                                    });
                                },
                            ))
                            .item(
                                PopupMenuItem::new("提交并推送")
                                    .icon(IconName::ArrowUp)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&push_entity, |app, cx| {
                                            app.commit_and_push_git(window, cx);
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("提交并同步")
                                    .icon(IconName::Redo2)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&sync_entity, |app, cx| {
                                            app.commit_and_sync_git(window, cx);
                                        });
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("载入上次提交说明")
                                    .icon(IconName::Copy)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&load_last_entity, |app, cx| {
                                            app.load_last_git_commit_message(window, cx);
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("修改上次提交")
                                    .icon(IconName::Redo2)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&amend_entity, |app, cx| {
                                            app.amend_last_git_commit(window, cx);
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("撤销上次提交")
                                    .icon(IconName::Undo2)
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
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .overflow_y_scrollbar()
        .child(git_status_group(
            "staged",
            "已暂存",
            staged.len(),
            staged,
            expanded_sections.contains("staged"),
            expanded_dirs,
            tree_children,
            selected_file,
            "暂无暂存文件",
            cx,
        ))
        .child(git_status_group(
            "changed",
            "更改",
            changed.len(),
            changed,
            expanded_sections.contains("changed"),
            expanded_dirs,
            tree_children,
            selected_file,
            "没有工作区更改",
            cx,
        ))
        .child(git_status_group(
            "untracked",
            "未跟踪",
            untracked.len(),
            untracked,
            expanded_sections.contains("untracked"),
            expanded_dirs,
            tree_children,
            selected_file,
            "暂无未跟踪文件",
            cx,
        ))
}

fn git_empty_repository_panel(
    clone_remote_url: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let value = clone_remote_url.to_string();
    let input_state = window.use_keyed_state("git-clone-remote-url", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .placeholder("远程仓库 URL")
    });
    input_state.update(cx, |state, cx| {
        if state.value().as_ref() != clone_remote_url {
            state.set_value(clone_remote_url.to_string(), window, cx);
        }
    });
    cx.subscribe_in(&input_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_git_clone_remote_url(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .p(px(28.0))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .text_center()
                .child(
                    div()
                        .size(px(84.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(color(theme::ORANGE).opacity(0.12))
                        .text_color(color(theme::ORANGE))
                        .child(Icon::new(IconName::Folder).size_8()),
                )
                .child(
                    div()
                        .mt(px(18.0))
                        .text_size(px(18.0))
                        .line_height(px(24.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .child("暂无仓库"),
                )
                .child(
                    div()
                        .mt(px(10.0))
                        .max_w(px(280.0))
                        .text_size(px(14.0))
                        .line_height(px(22.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child("初始化仓库或克隆远程仓库后，就可以在这里查看提交、差异和分支。"),
                )
                .child(
                    div()
                        .mt(px(22.0))
                        .w_full()
                        .max_w(px(300.0))
                        .child(Input::new(&input_state).with_size(gpui_component::Size::Medium)),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("git-init-repo")
                                .primary()
                                .text_color(color(0xFFFFFF))
                                .on_click(cx.listener(|app, _event, window, cx| {
                                    app.init_project_git(window, cx)
                                }))
                                .child("初始化仓库"),
                        )
                        .child(
                            Button::new("git-clone-repo")
                                .secondary()
                                .text_color(cx.theme().secondary_foreground)
                                .on_click(cx.listener(|app, _event, window, cx| {
                                    app.clone_project_git(window, cx)
                                }))
                                .child("克隆远程仓库"),
                        ),
                ),
        )
}

fn git_status_group(
    id: &'static str,
    title: &'static str,
    count: usize,
    files: &[GitFileStatus],
    expanded: bool,
    expanded_dirs: &HashSet<String>,
    tree_children: &HashMap<String, Vec<GitFileStatus>>,
    selected_file: Option<&str>,
    empty_text: &'static str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let action_paths = files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let rows = if expanded {
        if files.is_empty() {
            vec![
                div()
                    .px_3()
                    .py_3()
                    .text_size(px(14.0))
                    .line_height(px(18.0))
                    .text_color(color(theme::TEXT_DIM))
                    .child(empty_text)
                    .into_any_element(),
            ]
        } else {
            git_status_tree_rows(id, files, expanded_dirs, tree_children, selected_file, cx)
        }
    } else {
        Vec::new()
    };

    div()
        .flex()
        .flex_col()
        .child(
            div()
                .id(SharedString::from(format!("git-status-group-{id}")))
                .h(px(40.0))
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .border_t_1()
                .border_color(color(theme::BORDER_SOFT))
                .bg(color(0xFFFFFF).opacity(0.02))
                .cursor_pointer()
                .on_click(cx.listener(move |app, _event, _window, cx| {
                    app.toggle_git_status_section(id, cx)
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .min_w_0()
                        .gap_2()
                        .child(
                            Icon::new(if expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .size_3p5()
                            .text_color(color(theme::TEXT_DIM)),
                        )
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color(theme::TEXT_MUTED))
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
                                .bg(color(0xFFFFFF).opacity(0.07))
                                .text_size(px(12.0))
                                .line_height(px(14.0))
                                .text_color(color(theme::TEXT_DIM))
                                .child(count.to_string()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_color(color(theme::TEXT_DIM))
                        .children(git_status_group_actions(id, action_paths, cx)),
                ),
        )
        .children(rows)
}

fn git_status_group_actions(
    section_id: &'static str,
    paths: Vec<String>,
    cx: &mut Context<CoduxApp>,
) -> Vec<AnyElement> {
    match section_id {
        "staged" => vec![
            git_status_group_action_button("unstage", IconName::Minus, paths, cx)
                .into_any_element(),
        ],
        "changed" => vec![
            git_status_group_action_button("stage", IconName::Plus, paths.clone(), cx)
                .into_any_element(),
            git_status_group_action_button("discard", IconName::Undo, paths, cx).into_any_element(),
        ],
        "untracked" => vec![
            git_status_group_action_button("stage", IconName::Plus, paths.clone(), cx)
                .into_any_element(),
            git_status_group_action_button("ignore", IconName::Close, paths, cx).into_any_element(),
        ],
        _ => Vec::new(),
    }
}

fn git_status_group_action_button(
    action: &'static str,
    icon: IconName,
    paths: Vec<String>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    Button::new(SharedString::from(format!("git-group-action-{action}")))
        .compact()
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .icon(
            Icon::new(icon)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .on_click(cx.listener(move |app, _event, window, cx| {
            cx.stop_propagation();
            window.prevent_default();
            match action {
                "stage" => app.stage_git_paths(paths.clone(), window, cx),
                "unstage" => app.unstage_git_paths(paths.clone(), window, cx),
                "discard" => app.discard_git_paths(paths.clone(), window, cx),
                "ignore" => app.append_project_gitignore_paths(paths.clone(), window, cx),
                _ => {}
            }
        }))
}

struct GitImmediateDir {
    path: String,
    count: usize,
}

const MAX_GIT_STATUS_TREE_ROWS: usize = 600;

fn git_status_tree_rows(
    section: &'static str,
    files: &[GitFileStatus],
    expanded_dirs: &HashSet<String>,
    tree_children: &HashMap<String, Vec<GitFileStatus>>,
    selected_file: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> Vec<AnyElement> {
    let mut rows = Vec::new();
    append_git_status_directory_rows(
        section,
        "",
        files,
        0,
        expanded_dirs,
        tree_children,
        selected_file,
        &mut rows,
        cx,
    );
    if rows.len() >= MAX_GIT_STATUS_TREE_ROWS {
        rows.push(
            div()
                .px_3()
                .py_2()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_DIM))
                .child(format!("已显示前 {} 项，继续展开目录查看子级", rows.len()))
                .into_any_element(),
        );
    }
    rows
}

fn append_git_status_directory_rows(
    section_id: &'static str,
    base_path: &str,
    files: &[GitFileStatus],
    depth: usize,
    expanded_dirs: &HashSet<String>,
    tree_children: &HashMap<String, Vec<GitFileStatus>>,
    selected_file: Option<&str>,
    rows: &mut Vec<AnyElement>,
    cx: &mut Context<CoduxApp>,
) {
    if rows.len() >= MAX_GIT_STATUS_TREE_ROWS {
        return;
    }

    let (dirs, direct_files) = collect_immediate_git_status_entries(section_id, base_path, files);

    for (name, dir) in dirs {
        if rows.len() >= MAX_GIT_STATUS_TREE_ROWS {
            return;
        }
        let expanded = expanded_dirs.contains(&dir.path);
        rows.push(
            git_status_dir_row(&name, &dir.path, dir.count, expanded, depth, cx).into_any_element(),
        );
        if expanded {
            if let Some(children) = tree_children.get(&dir.path) {
                append_git_status_directory_rows(
                    section_id,
                    &dir.path,
                    children,
                    depth + 1,
                    expanded_dirs,
                    tree_children,
                    selected_file,
                    rows,
                    cx,
                );
            }
        }
    }
    for file in direct_files {
        if rows.len() >= MAX_GIT_STATUS_TREE_ROWS {
            return;
        }
        rows.push(git_status_file_row(file, selected_file, depth, cx).into_any_element());
    }
}

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
}

fn git_status_dir_row(
    name: &str,
    path: &str,
    count: usize,
    expanded: bool,
    depth: usize,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let directory_path = path.to_string();
    let ignore_path = format!("{}/", path.trim_end_matches('/'));

    div()
        .id(SharedString::from(format!("git-sidebar-dir-{path}")))
        .h(px(28.0))
        .pl(px(18.0 + depth as f32 * 22.0))
        .pr_3()
        .flex()
        .items_center()
        .justify_between()
        .text_color(color(theme::TEXT_MUTED))
        .cursor_pointer()
        .hover(|style| style.bg(color(0xFFFFFF).opacity(0.05)))
        .on_click(cx.listener(move |app, _event, _window, cx| {
            app.toggle_git_status_dir(directory_path.clone(), cx)
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .child(
                    Icon::new(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size_3(),
                )
                .child(
                    Icon::new(IconName::Folder)
                        .size_4()
                        .text_color(color(theme::ACCENT)),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .truncate()
                        .child(name.to_string()),
                ),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_DIM))
                .flex()
                .items_center()
                .gap_1()
                .child(git_file_action_button(
                    "ignore",
                    IconName::Close,
                    ignore_path,
                    cx,
                ))
                .child(count.to_string()),
        )
}

fn git_status_file_row(
    file: GitFileStatus,
    selected_file: Option<&str>,
    depth: usize,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let status = git_file_status_label(&file);
    let can_stage = is_git_worktree_file(&file) || is_git_untracked_file(&file);
    let can_unstage = is_git_staged_file(&file);
    let can_discard = is_git_worktree_file(&file) || is_git_untracked_file(&file);
    let active = selected_file.map(|path| path == file.path).unwrap_or(false);
    let file_path = file.path.clone();
    let diff_file_path = file.path.clone();
    let file_name = file
        .path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(file.path.as_str())
        .to_string();
    let is_dir_status = file.path.ends_with('/');
    let mut actions = Vec::new();
    if can_stage {
        actions.push(
            git_file_action_button("stage", IconName::Plus, file.path.clone(), cx)
                .into_any_element(),
        );
    }
    if can_unstage {
        actions.push(
            git_file_action_button("unstage", IconName::Minus, file.path.clone(), cx)
                .into_any_element(),
        );
    }
    if can_discard {
        actions.push(
            git_file_action_button("discard", IconName::Undo, file.path.clone(), cx)
                .into_any_element(),
        );
    }
    if is_git_untracked_file(&file) || is_dir_status {
        actions.push(
            git_file_action_button("ignore", IconName::Close, file.path.clone(), cx)
                .into_any_element(),
        );
    }
    if !is_dir_status {
        actions.push(
            git_file_action_button("diff", IconName::ExternalLink, diff_file_path, cx)
                .into_any_element(),
        );
    }

    div()
        .id(SharedString::from(format!(
            "git-sidebar-file-{}",
            file.path
        )))
        .h(px(28.0))
        .pl(px(46.0 + depth as f32 * 22.0))
        .pr_3()
        .flex()
        .items_center()
        .justify_between()
        .bg(if active {
            color(0xFFFFFF).opacity(0.06)
        } else {
            color(0xFFFFFF).opacity(0.0)
        })
        .cursor_pointer()
        .hover(|style| style.bg(color(0xFFFFFF).opacity(0.05)))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_git_file(file_path.clone(), window, cx)
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .text_color(color(theme::TEXT_MUTED))
                .child(
                    Icon::new(if is_dir_status {
                        IconName::Folder
                    } else {
                        IconName::File
                    })
                    .size_3p5(),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .truncate()
                        .child(file_name),
                ),
        )
        .child(
            div()
                .ml_2()
                .flex()
                .items_center()
                .gap_1()
                .children(actions)
                .child(
                    div()
                        .min_w(px(18.0))
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(if status == "U" {
                            theme::GREEN
                        } else {
                            theme::TEXT_DIM
                        }))
                        .child(status),
                ),
        )
}

fn git_file_action_button(
    action: &'static str,
    icon: IconName,
    file_path: String,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let id_path = file_path.replace('/', "-");
    Button::new(SharedString::from(format!(
        "git-file-action-{action}-{id_path}"
    )))
    .compact()
    .ghost()
    .text_color(cx.theme().secondary_foreground)
    .icon(
        Icon::new(icon)
            .size_3p5()
            .text_color(cx.theme().secondary_foreground),
    )
    .on_click(cx.listener(move |app, _event, window, cx| {
        cx.stop_propagation();
        window.prevent_default();
        match action {
            "stage" => {
                app.select_git_file(file_path.clone(), window, cx);
                app.stage_selected_git_file(window, cx);
            }
            "unstage" => {
                app.select_git_file(file_path.clone(), window, cx);
                app.unstage_selected_git_file(window, cx);
            }
            "discard" => {
                app.select_git_file(file_path.clone(), window, cx);
                app.discard_selected_git_file(window, cx);
            }
            "ignore" => app.append_project_gitignore_path(file_path.clone(), window, cx),
            "diff" => app.open_git_diff_window(file_path.clone(), window, cx),
            _ => {}
        }
    }))
}

pub(in crate::app) fn git_diff_window_workspace(
    selected_path: Option<&str>,
    diff: &str,
    error: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let file_path = selected_path.unwrap_or("未选择文件").to_string();
    let open_path = file_path.clone();

    div()
        .flex()
        .flex_col()
        .size_full()
        .bg(color(theme::BG))
        .child(
            div()
                .h(px(52.0))
                .px_4()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .text_color(color(theme::TEXT))
                                .child("Diff"),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .truncate()
                                .text_color(color(theme::TEXT_DIM))
                                .child(file_path),
                        ),
                )
                .child(
                    Button::new("git-diff-window-open-file")
                        .secondary()
                        .compact()
                        .text_color(cx.theme().secondary_foreground)
                        .disabled(selected_path.is_none())
                        .on_click(cx.listener(move |app, _event, _window, cx| {
                            app.open_git_diff_window_file(open_path.clone(), cx);
                        }))
                        .child(
                            div()
                                .h(px(22.0))
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .child(Icon::new(IconName::ExternalLink).size_3())
                                .child("打开文件"),
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
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(color(theme::ORANGE))
                    .child(error),
            )
        })
        .child(
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
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(color(theme::TEXT_DIM))
                            .child("没有可显示的 Diff")
                            .into_any_element(),
                    ]
                } else {
                    diff.lines()
                        .map(|line| git_diff_line_row(line).into_any_element())
                        .collect::<Vec<_>>()
                }),
        )
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
        .text_size(px(12.0))
        .line_height(px(18.0))
        .font_family("SF Mono")
        .text_color(color(line_color))
        .child(line.to_string())
}

fn git_history_panel(git: &GitSummary, cx: &mut Context<CoduxApp>) -> impl IntoElement {
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
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .bg(color(0xFFFFFF).opacity(0.02))
                .text_size(px(14.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_DIM))
                .child("Git 历史"),
        )
        .child(if git.commits.is_empty() {
            div()
                .flex_1()
                .px_3()
                .py_4()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(color(theme::TEXT_DIM))
                .child("暂无提交记录")
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .py_1()
                .children(
                    git.commits
                        .iter()
                        .take(24)
                        .enumerate()
                        .map(|(index, commit)| {
                            git_history_timeline_row(commit, index == 0, cx).into_any_element()
                        }),
                )
                .into_any_element()
        })
}

fn git_history_timeline_row(
    commit: &GitCommitSummary,
    active: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title = commit.title.clone();
    let author = commit.author.clone();
    let relative_time = commit.relative_time.clone();
    let hash = commit.hash.clone();
    let menu_hash = hash.clone();
    let app_entity = cx.entity();
    let tooltip = format!(
        "{}\n{}\n{} · {}",
        commit.hash, commit.title, commit.author, commit.relative_time
    );

    div()
        .id(SharedString::from(format!("git-history-{}", commit.hash)))
        .relative()
        .min_h(px(60.0))
        .px_3()
        .py_1()
        .flex()
        .gap_2()
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .hover(|style| style.bg(color(0xFFFFFF).opacity(0.04)))
        .child(
            div()
                .w(px(18.0))
                .h_full()
                .relative()
                .flex_shrink_0()
                .child(
                    div()
                        .absolute()
                        .left(px(8.0))
                        .top(px(0.0))
                        .bottom(px(0.0))
                        .w(px(1.0))
                        .bg(color(theme::ACCENT).opacity(0.46)),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(2.0))
                        .top(px(13.0))
                        .size(px(13.0))
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
                .gap(px(4.0))
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
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::SEMIBOLD)
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
                                .text_size(px(12.0))
                                .line_height(px(14.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color(theme::ACCENT))
                                .child("HEAD->main")
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        }),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_DIM))
                        .truncate()
                        .child(format!("{author} · {relative_time} · {hash}")),
                ),
        )
        .child(
            Button::new(SharedString::from(format!(
                "git-history-actions-{menu_hash}"
            )))
            .compact()
            .ghost()
            .tooltip("提交操作")
            .text_color(cx.theme().secondary_foreground)
            .icon(
                Icon::new(IconName::Ellipsis)
                    .size_3p5()
                    .text_color(cx.theme().secondary_foreground),
            )
            .dropdown_menu(move |menu, _window, _cx| {
                let checkout_hash = menu_hash.clone();
                let revert_hash = menu_hash.clone();
                let restore_hash = menu_hash.clone();
                let checkout_entity = app_entity.clone();
                let revert_entity = app_entity.clone();
                let restore_entity = app_entity.clone();
                menu.item(
                    PopupMenuItem::new("检出此提交")
                        .icon(IconName::Github)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&checkout_entity, |app, cx| {
                                app.checkout_git_commit(checkout_hash.clone(), window, cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new("回滚此提交")
                        .icon(IconName::Undo2)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&revert_entity, |app, cx| {
                                app.revert_git_commit(revert_hash.clone(), window, cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new("恢复到此提交")
                        .icon(IconName::Redo2)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&restore_entity, |app, cx| {
                                app.restore_git_commit(restore_hash.clone(), window, cx);
                            });
                        }),
                )
            }),
        )
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
        "U".to_string()
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

pub(in crate::app) fn git_workspace_section(
    git: &GitSummary,
    selected_file: Option<&str>,
    selected_branch: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let status_rows = vec![
        format!(
            "repository: {}",
            if git.is_repository { "yes" } else { "no" }
        ),
        format!("branch: {}", git.branch),
        format!("upstream: {}", git.upstream.as_deref().unwrap_or("none")),
        format!("ahead / behind: {} / {}", git.ahead, git.behind),
        format!(
            "staged / unstaged / untracked: {} / {} / {}",
            git.staged, git.unstaged, git.untracked
        ),
    ];
    let commit_rows = if git.commits.is_empty() {
        vec!["no recent commits".to_string()]
    } else {
        git.commits
            .iter()
            .take(8)
            .map(|commit| {
                format!(
                    "{} {} · {} · {}",
                    commit.hash, commit.title, commit.author, commit.relative_time
                )
            })
            .collect()
    };

    div()
        .flex()
        .flex_col()
        .child(section("Repository", status_rows))
        .child(git_changed_files_section(
            &git.changed_files,
            selected_file,
            cx,
        ))
        .child(section("Recent Commits", commit_rows))
        .child(git_branches_section(&git.branches, selected_branch, cx))
        .child(section(
            "Remotes",
            git.remotes
                .iter()
                .take(6)
                .map(|remote| format!("{} {}", remote.name, remote.url))
                .collect(),
        ))
}

fn git_branches_section(
    branches: &[GitBranchSummary],
    selected_branch: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let rows = if branches.is_empty() {
        vec![
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(color(theme::TEXT_DIM))
                .child("no local branches")
                .into_any_element(),
        ]
    } else {
        branches
            .iter()
            .take(20)
            .cloned()
            .map(|branch| git_branch_row(branch, selected_branch, cx).into_any_element())
            .collect()
    };
    div()
        .flex()
        .flex_col()
        .mx_3()
        .mt_3()
        .rounded_sm()
        .border_1()
        .border_color(color(theme::BORDER))
        .bg(color(theme::BG_ELEVATED))
        .child(
            div()
                .h(px(30.0))
                .px_2()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_MUTED))
                .child("Branches"),
        )
        .children(rows)
}

fn git_branch_row(
    branch: GitBranchSummary,
    selected_branch: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let active = selected_branch
        .map(|name| name == branch.name)
        .unwrap_or(branch.is_current);
    let branch_name = branch.name.clone();
    div()
        .id(SharedString::from(format!("git-branch-{}", branch.name)))
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .px_2()
        .py_1()
        .border_t_1()
        .border_color(color(theme::BORDER_SOFT))
        .bg(color(if active {
            theme::BG_PANEL
        } else {
            theme::BG_ELEVATED
        }))
        .cursor_pointer()
        .hover(|style| style.bg(color(theme::BORDER_SOFT)))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_git_branch(branch_name.clone(), window, cx)
        }))
        .child(
            div()
                .text_xs()
                .text_color(color(theme::TEXT))
                .truncate()
                .child(branch.name),
        )
        .child(
            div()
                .text_xs()
                .text_color(color(if branch.is_current {
                    theme::ACCENT
                } else {
                    theme::TEXT_DIM
                }))
                .child(if branch.is_current { "current" } else { "" }),
        )
}

fn git_changed_files_section(
    files: &[GitFileStatus],
    selected_file: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let rows = if files.is_empty() {
        vec![
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(color(theme::TEXT_DIM))
                .child("no changed files")
                .into_any_element(),
        ]
    } else {
        files
            .iter()
            .take(24)
            .cloned()
            .map(|file| git_changed_file_row(file, selected_file, cx).into_any_element())
            .collect()
    };
    div()
        .flex()
        .flex_col()
        .mx_3()
        .mt_3()
        .rounded_sm()
        .border_1()
        .border_color(color(theme::BORDER))
        .bg(color(theme::BG_ELEVATED))
        .child(
            div()
                .h(px(30.0))
                .px_2()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_MUTED))
                .child("Changed Files"),
        )
        .children(rows)
}

fn git_changed_file_row(
    file: GitFileStatus,
    selected_file: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let active = selected_file.map(|path| path == file.path).unwrap_or(false);
    let file_path = file.path.clone();
    div()
        .id(SharedString::from(format!("git-file-{}", file.path)))
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .px_2()
        .py_1()
        .border_t_1()
        .border_color(color(theme::BORDER_SOFT))
        .bg(color(if active {
            theme::BG_PANEL
        } else {
            theme::BG_ELEVATED
        }))
        .cursor_pointer()
        .hover(|style| style.bg(color(theme::BORDER_SOFT)))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_git_file(file_path.clone(), window, cx)
        }))
        .child(
            div()
                .text_xs()
                .text_color(color(theme::TEXT))
                .truncate()
                .child(file.path),
        )
        .child(
            div()
                .text_xs()
                .text_color(color(if active {
                    theme::ACCENT
                } else {
                    theme::TEXT_DIM
                }))
                .child(format!("{}{}", file.index_status, file.worktree_status)),
        )
}

pub(in crate::app) fn git_diff_workspace(diff: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .mx_3()
        .mt_3()
        .rounded_sm()
        .border_1()
        .border_color(color(theme::BORDER))
        .bg(color(theme::BG_TERMINAL))
        .child(
            div()
                .h(px(30.0))
                .px_2()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_MUTED))
                .child("Diff Preview"),
        )
        .child(
            div()
                .p_2()
                .text_xs()
                .text_color(color(theme::TEXT))
                .children(diff.lines().take(40).map(|line| {
                    div()
                        .child(line.chars().take(110).collect::<String>())
                        .into_any_element()
                })),
        )
}

pub(in crate::app) fn git_review_workspace(
    selected_path: Option<&str>,
    diff: &str,
    content: Option<&GitReviewContentSummary>,
) -> impl IntoElement {
    let selected_path = selected_path.unwrap_or("未选择文件");
    div()
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
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(selected_path.to_string()),
                )
                .when_some(content, |this, content| {
                    this.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(color(theme::TEXT_DIM))
                            .child(format!("+{}", content.added_lines.len()))
                            .child(format!("-{}", content.deleted_lines.len())),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(git_review_content_panel(
                    "Base",
                    content.and_then(|item| item.base_content.as_deref()),
                ))
                .child(git_review_content_panel(
                    "Worktree",
                    content.map(|item| item.worktree_content.as_str()),
                )),
        )
        .child(
            div()
                .h(px(190.0))
                .flex_shrink_0()
                .border_t_1()
                .border_color(color(theme::BORDER_SOFT))
                .overflow_y_scrollbar()
                .child(git_diff_workspace(diff)),
        )
}

fn git_review_content_panel(title: &'static str, content: Option<&str>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .border_r_1()
        .border_color(color(theme::BORDER_SOFT))
        .child(
            div()
                .h(px(30.0))
                .px_2()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_MUTED))
                .child(title),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .bg(color(theme::BG_TERMINAL))
                .p_2()
                .text_size(px(12.0))
                .line_height(px(18.0))
                .text_color(color(theme::TEXT))
                .children(content.unwrap_or("").lines().take(160).map(|line| {
                    div()
                        .child(line.chars().take(130).collect::<String>())
                        .into_any_element()
                })),
        )
}
