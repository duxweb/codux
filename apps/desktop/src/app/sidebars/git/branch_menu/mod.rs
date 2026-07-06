use super::*;

mod items;

use items::*;

pub(super) fn git_branch_dropdown_menu(
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
    stashes: Vec<GitStashSummary>,
    tags: Vec<String>,
    changed_paths: Vec<String>,
    has_staged: bool,
    language: String,
    app_entity: gpui::Entity<CoduxApp>,
) -> PopupMenu {
    let labels = Rc::new(GitBranchMenuLabels::load(&language));
    let has_remotes = !remotes.is_empty();
    let has_changes = !changed_paths.is_empty();
    let can_use_current_branch_remote = upstream.is_some()
        && current_branch != "HEAD"
        && current_branch != "uninitialized"
        && !current_branch.trim().is_empty();

    // Non-current local branches — candidates for merge / squash / rebase / delete.
    let other_local: Vec<GitBranchSummary> = branches
        .iter()
        .filter(|branch| !branch.is_current)
        .cloned()
        .collect();
    // Deletable remote refs (skip HEAD pointers).
    let remote_refs: Vec<String> = {
        let mut seen = HashSet::new();
        remote_branches
            .iter()
            .filter(|reference| {
                matches!(reference.split_once('/'), Some((remote, branch))
                    if !remote.is_empty() && !branch.is_empty() && branch != "HEAD")
            })
            .filter(|reference| seen.insert((*reference).clone()))
            .cloned()
            .collect()
    };

    // Top level mirrors VS Code's SCM overflow menu: Pull / Push / Checkout To… / Fetch.
    let pull_entity = app_entity.clone();
    let push_entity = app_entity.clone();
    let fetch_entity = app_entity.clone();
    let menu = menu
        .item(
            PopupMenuItem::new(labels.pull.clone())
                .icon(HeroIconName::ArrowDown)
                .disabled(!can_use_current_branch_remote)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&pull_entity, |app, cx| app.pull_project_git(window, cx));
                }),
        )
        .item(
            PopupMenuItem::new(labels.push.clone())
                .icon(HeroIconName::ArrowUp)
                .disabled(!has_remotes)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&push_entity, |app, cx| app.push_project_git(window, cx));
                }),
        )
        .item(checkout_to_item(
            labels.checkout_to.clone(),
            labels.clone(),
            branches.clone(),
            remote_refs.clone(),
            app_entity.clone(),
        ))
        .item(
            PopupMenuItem::new(labels.fetch.clone())
                .icon(HeroIconName::ArrowDownTray)
                .disabled(!has_remotes)
                .on_click(move |_, window, cx| {
                    cx.update_entity(&fetch_entity, |app, cx| app.fetch_project_git(window, cx));
                }),
        );

    // Commit ▸
    let commit_labels = labels.clone();
    let commit_entity = app_entity.clone();
    let menu = menu.separator().submenu_with_icon(
        Some(Icon::new(HeroIconName::CheckCircle)),
        labels.commit_menu.clone(),
        window,
        cx,
        move |menu, _window, _cx| {
            let undo_entity = commit_entity.clone();
            let edit_entity = commit_entity.clone();
            menu.item(
                PopupMenuItem::new(commit_labels.undo_last_commit.clone())
                    .icon(HeroIconName::ArrowUturnLeft)
                    .disabled(!has_commits)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&undo_entity, |app, cx| {
                            app.undo_last_git_commit(window, cx)
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(commit_labels.edit_last_commit_message.clone())
                    .icon(HeroIconName::PencilSquare)
                    .disabled(!has_commits)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&edit_entity, |app, cx| {
                            app.edit_last_git_commit_message(window, cx)
                        });
                    }),
            )
        },
    );

    // Changes ▸
    let changes_labels = labels.clone();
    let changes_entity = app_entity.clone();
    let changes_paths = changed_paths.clone();
    let menu = menu.submenu_with_icon(
        Some(Icon::new(HeroIconName::ListBullet)),
        labels.changes_menu.clone(),
        window,
        cx,
        move |menu, _window, _cx| {
            let stage_entity = changes_entity.clone();
            let stage_paths = changes_paths.clone();
            let unstage_entity = changes_entity.clone();
            let unstage_paths = changes_paths.clone();
            let discard_entity = changes_entity.clone();
            let discard_paths = changes_paths.clone();
            menu.item(
                PopupMenuItem::new(changes_labels.stage_all.clone())
                    .icon(HeroIconName::Check)
                    .disabled(!has_changes)
                    .on_click(move |_, window, cx| {
                        let paths = stage_paths.clone();
                        cx.update_entity(&stage_entity, |app, cx| {
                            app.stage_git_paths(paths, window, cx)
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(changes_labels.unstage_all.clone())
                    .icon(HeroIconName::XMark)
                    .disabled(!has_staged)
                    .on_click(move |_, window, cx| {
                        let paths = unstage_paths.clone();
                        cx.update_entity(&unstage_entity, |app, cx| {
                            app.unstage_git_paths(paths, window, cx)
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(changes_labels.discard_all.clone())
                    .icon(HeroIconName::Trash)
                    .disabled(!has_changes)
                    .on_click(move |_, window, cx| {
                        let paths = discard_paths.clone();
                        cx.update_entity(&discard_entity, |app, cx| {
                            app.discard_all_git_changes(paths, window, cx)
                        });
                    }),
            )
        },
    );

    // Pull, Push ▸
    let pp_labels = labels.clone();
    let pp_entity = app_entity.clone();
    let pp_remotes = remotes.clone();
    let pp_default = default_push_remote.clone();
    let menu = menu.submenu_with_icon(
        Some(Icon::new(HeroIconName::ArrowsUpDown)),
        labels.pull_push_menu.clone(),
        window,
        cx,
        move |menu, _window, _cx| {
            let sync_entity = pp_entity.clone();
            let pull_entity = pp_entity.clone();
            let push_entity = pp_entity.clone();
            let force_entity = pp_entity.clone();
            let fetch_entity = pp_entity.clone();
            let prune_entity = pp_entity.clone();
            let menu = menu
                .item(
                    PopupMenuItem::new(pp_labels.sync.clone())
                        .icon(HeroIconName::ArrowPath)
                        .disabled(!can_use_current_branch_remote)
                        .on_click(move |_, _window, cx| {
                            cx.update_entity(&sync_entity, |app, cx| {
                                app.run_project_git_remote_action("sync", cx)
                            });
                        }),
                )
                .separator()
                .item(
                    PopupMenuItem::new(pp_labels.pull.clone())
                        .icon(HeroIconName::ArrowDown)
                        .disabled(!can_use_current_branch_remote)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&pull_entity, |app, cx| {
                                app.pull_project_git(window, cx)
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new(pp_labels.push.clone())
                        .icon(HeroIconName::ArrowUp)
                        .disabled(!has_remotes)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&push_entity, |app, cx| {
                                app.push_project_git(window, cx)
                            });
                        }),
                );

            // Push To… — searchable picker over remotes.
            let push_to_remotes = pp_remotes.clone();
            let push_to_default = pp_default.clone();
            let push_to_entity = pp_entity.clone();
            let push_to_placeholder = pp_labels.push_to.clone();
            let menu = menu.item(
                PopupMenuItem::new(format!("{}…", pp_labels.push_to))
                    .icon(HeroIconName::ArrowUp)
                    .disabled(!has_remotes)
                    .on_click(move |_, window, cx| {
                        let items =
                            remote_pick_items(&push_to_remotes, push_to_default.as_deref(), false);
                        let entity = push_to_entity.clone();
                        show_quick_pick(
                            push_to_placeholder.clone(),
                            items,
                            move |id, window, cx| {
                                entity.update(cx, |app, cx| {
                                    app.push_project_git_remote(id.to_string(), window, cx);
                                });
                            },
                            window,
                            cx,
                        );
                    }),
            );

            menu.item(
                PopupMenuItem::new(pp_labels.force_push.clone())
                    .icon(HeroIconName::ExclamationTriangle)
                    .disabled(!can_use_current_branch_remote)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&force_entity, |app, cx| {
                            app.force_push_project_git(window, cx)
                        });
                    }),
            )
            .separator()
            .item(
                PopupMenuItem::new(pp_labels.fetch.clone())
                    .icon(HeroIconName::ArrowDownTray)
                    .disabled(!has_remotes)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&fetch_entity, |app, cx| {
                            app.fetch_project_git(window, cx)
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(pp_labels.fetch_prune.clone())
                    .icon(HeroIconName::Scissors)
                    .disabled(!has_remotes)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&prune_entity, |app, cx| {
                            app.fetch_prune_project_git(window, cx)
                        });
                    }),
            )
        },
    );

    // Branch ▸
    let branch_labels = labels.clone();
    let branch_entity = app_entity.clone();
    let branch_all = branches.clone();
    let branch_other = other_local.clone();
    let branch_remote_refs = remote_refs.clone();
    let menu = menu.submenu_with_icon(
        Some(Icon::new(HeroIconName::ArrowPathRoundedSquare)),
        labels.branch_menu.clone(),
        window,
        cx,
        move |menu, _window, _cx| {
            let menu = menu
                .item(checkout_to_item(
                    format!("{}…", branch_labels.switch_branch),
                    branch_labels.clone(),
                    branch_all.clone(),
                    branch_remote_refs.clone(),
                    branch_entity.clone(),
                ))
                .separator();
            let menu = branch_pick_item(
                menu,
                format!("{}…", branch_labels.merge),
                HeroIconName::ArrowUturnRight,
                branch_labels.merge.clone(),
                branch_other.clone(),
                branch_entity.clone(),
                BranchPickAction::Merge,
            );
            let menu = branch_pick_item(
                menu,
                format!("{}…", branch_labels.squash_merge),
                HeroIconName::ArrowPath,
                branch_labels.squash_merge.clone(),
                branch_other.clone(),
                branch_entity.clone(),
                BranchPickAction::Squash,
            );
            let menu = branch_pick_item(
                menu,
                format!("{}…", branch_labels.rebase),
                HeroIconName::ArrowsUpDown,
                branch_labels.rebase.clone(),
                branch_other.clone(),
                branch_entity.clone(),
                BranchPickAction::Rebase,
            );

            // Create Branch… — Quick Input prefilled with a generated name.
            let create_labels = branch_labels.clone();
            let create_entity = branch_entity.clone();
            let menu = menu.separator().item(
                PopupMenuItem::new(format!("{}…", branch_labels.new_branch))
                    .icon(HeroIconName::Plus)
                    .on_click(move |_, window, cx| {
                        let entity = create_entity.clone();
                        show_quick_input(
                            create_labels.new_branch.clone(),
                            create_labels.branch_name_placeholder.clone(),
                            generated_git_branch_name(),
                            false,
                            move |name, window, cx| {
                                entity.update(cx, |app, cx| {
                                    app.create_git_branch(name, window, cx);
                                });
                            },
                            window,
                            cx,
                        );
                    }),
            );

            // Create Branch From… — Quick Input, then base picker (local + remote refs).
            let from_labels = branch_labels.clone();
            let from_entity = branch_entity.clone();
            let from_branches = branch_all.clone();
            let from_remote_refs = branch_remote_refs.clone();
            let menu = menu.item(
                PopupMenuItem::new(format!("{}…", branch_labels.create_from))
                    .icon(HeroIconName::SquaresPlus)
                    .disabled(branch_all.is_empty())
                    .on_click(move |_, window, cx| {
                        let entity = from_entity.clone();
                        let branches = from_branches.clone();
                        let remote_refs = from_remote_refs.clone();
                        let placeholder = from_labels.create_from.clone();
                        show_quick_input(
                            from_labels.create_from.clone(),
                            from_labels.branch_name_placeholder.clone(),
                            generated_git_branch_name(),
                            false,
                            move |name, window, cx| {
                                let mut items: Vec<QuickPickItem> = branches
                                    .iter()
                                    .map(|branch| {
                                        QuickPickItem::new(branch.name.clone(), branch.name.clone())
                                            .icon(Icon::new(HeroIconName::ArrowPathRoundedSquare))
                                    })
                                    .collect();
                                items.extend(remote_refs.iter().map(|reference| {
                                    QuickPickItem::new(reference.clone(), reference.clone())
                                        .icon(Icon::new(HeroIconName::GlobeAlt))
                                }));
                                let entity = entity.clone();
                                let name = name.clone();
                                show_quick_pick(
                                    placeholder.clone(),
                                    items,
                                    move |base, window, cx| {
                                        entity.update(cx, |app, cx| {
                                            app.create_git_branch_from(
                                                name.clone(),
                                                base.to_string(),
                                                window,
                                                cx,
                                            );
                                        });
                                    },
                                    window,
                                    cx,
                                );
                            },
                            window,
                            cx,
                        );
                    }),
            );

            // Rename Branch… — branch picker, then Quick Input prefilled with the old name.
            let rename_labels = branch_labels.clone();
            let rename_entity = branch_entity.clone();
            let rename_branches = branch_all.clone();
            let menu = menu.item(
                PopupMenuItem::new(format!("{}…", branch_labels.rename))
                    .icon(HeroIconName::PencilSquare)
                    .disabled(branch_all.is_empty())
                    .on_click(move |_, window, cx| {
                        let entity = rename_entity.clone();
                        let input_title = rename_labels.rename.clone();
                        let input_placeholder = rename_labels.branch_name_placeholder.clone();
                        let items: Vec<QuickPickItem> = rename_branches
                            .iter()
                            .map(|branch| {
                                QuickPickItem::new(branch.name.clone(), branch.name.clone())
                                    .icon(Icon::new(HeroIconName::PencilSquare))
                            })
                            .collect();
                        show_quick_pick(
                            rename_labels.rename.clone(),
                            items,
                            move |branch, window, cx| {
                                let entity = entity.clone();
                                let branch = branch.to_string();
                                let prefill = branch.clone();
                                show_quick_input(
                                    input_title.clone(),
                                    input_placeholder.clone(),
                                    prefill,
                                    false,
                                    move |new_name, window, cx| {
                                        entity.update(cx, |app, cx| {
                                            app.rename_git_branch(
                                                branch.clone(),
                                                new_name,
                                                window,
                                                cx,
                                            );
                                        });
                                    },
                                    window,
                                    cx,
                                );
                            },
                            window,
                            cx,
                        );
                    }),
            );

            let menu = branch_pick_item(
                menu,
                format!("{}…", branch_labels.delete_local),
                HeroIconName::Trash,
                branch_labels.delete_local.clone(),
                branch_other.clone(),
                branch_entity.clone(),
                BranchPickAction::Delete,
            );

            // Delete Remote Branch… — picker over remote refs.
            let delete_remote_labels = branch_labels.clone();
            let delete_remote_entity = branch_entity.clone();
            let delete_remote_refs = branch_remote_refs.clone();
            menu.item(
                PopupMenuItem::new(format!("{}…", branch_labels.delete_remote))
                    .icon(HeroIconName::Trash)
                    .disabled(branch_remote_refs.is_empty())
                    .on_click(move |_, window, cx| {
                        let entity = delete_remote_entity.clone();
                        let items: Vec<QuickPickItem> = delete_remote_refs
                            .iter()
                            .map(|reference| {
                                QuickPickItem::new(reference.clone(), reference.clone())
                                    .icon(Icon::new(HeroIconName::GlobeAlt))
                            })
                            .collect();
                        show_quick_pick(
                            delete_remote_labels.delete_remote.clone(),
                            items,
                            move |reference, window, cx| {
                                entity.update(cx, |app, cx| {
                                    app.delete_git_remote_branch(reference.to_string(), window, cx);
                                });
                            },
                            window,
                            cx,
                        );
                    }),
            )
        },
    );

    // Remote ▸
    let remote_menu_remotes = remotes.clone();
    let remote_menu_default = default_push_remote.clone();
    let remote_menu_entity = app_entity.clone();
    let remote_menu_labels = labels.clone();
    let menu = menu.submenu_with_icon(
        Some(Icon::new(HeroIconName::GlobeAlt)),
        labels.remotes.clone(),
        window,
        cx,
        move |menu, _window, _cx| {
            // Add Remote… — chained Quick Inputs: name, then URL (VS Code flow).
            let add_entity = remote_menu_entity.clone();
            let add_labels = remote_menu_labels.clone();
            let menu = menu.item(
                PopupMenuItem::new(format!("{}…", remote_menu_labels.add_remote))
                    .icon(HeroIconName::Plus)
                    .on_click(move |_, window, cx| {
                        let entity = add_entity.clone();
                        let labels = add_labels.clone();
                        show_quick_input(
                            labels.add_remote.clone(),
                            labels.remote_name_placeholder.clone(),
                            "origin",
                            false,
                            move |name, window, cx| {
                                let entity = entity.clone();
                                let labels = labels.clone();
                                show_quick_input(
                                    format!("{} — {}", labels.add_remote, name),
                                    labels.remote_url_placeholder.clone(),
                                    "",
                                    false,
                                    move |url, window, cx| {
                                        entity.update(cx, |app, cx| {
                                            app.add_git_remote(name.clone(), url, window, cx);
                                        });
                                    },
                                    window,
                                    cx,
                                );
                            },
                            window,
                            cx,
                        );
                    }),
            );

            // Set default push remote — picker over remotes.
            let set_remotes = remote_menu_remotes.clone();
            let set_default = remote_menu_default.clone();
            let set_entity = remote_menu_entity.clone();
            let set_placeholder = remote_menu_labels.set_default.clone();
            let menu = menu.item(
                PopupMenuItem::new(format!("{}…", remote_menu_labels.set_default))
                    .icon(HeroIconName::Check)
                    .disabled(set_remotes.is_empty())
                    .on_click(move |_, window, cx| {
                        let items = remote_pick_items(&set_remotes, set_default.as_deref(), true);
                        let entity = set_entity.clone();
                        show_quick_pick(
                            set_placeholder.clone(),
                            items,
                            move |id, window, cx| {
                                entity.update(cx, |app, cx| {
                                    app.set_project_default_push_remote(
                                        Some(id.to_string()),
                                        window,
                                        cx,
                                    );
                                });
                            },
                            window,
                            cx,
                        );
                    }),
            );

            // Copy remote URL — picker over remotes (id carries the URL).
            let copy_remotes = remote_menu_remotes.clone();
            let copy_placeholder = remote_menu_labels.copy_url.clone();
            let menu = menu.item(
                PopupMenuItem::new(format!("{}…", remote_menu_labels.copy_url))
                    .icon(HeroIconName::DocumentDuplicate)
                    // Match the picker filter: only non-empty URLs count.
                    .disabled(
                        !copy_remotes
                            .iter()
                            .any(|remote| !remote.url.trim().is_empty()),
                    )
                    .on_click(move |_, window, cx| {
                        let items: Vec<QuickPickItem> = copy_remotes
                            .iter()
                            .filter(|remote| !remote.url.trim().is_empty())
                            .map(|remote| {
                                QuickPickItem::new(remote.url.clone(), remote.name.clone())
                                    .description(remote.url.clone())
                                    .icon(Icon::new(HeroIconName::GlobeAlt))
                            })
                            .collect();
                        show_quick_pick(
                            copy_placeholder.clone(),
                            items,
                            move |id, _window, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(id.to_string()));
                            },
                            window,
                            cx,
                        );
                    }),
            );

            // Remove remote — picker over remotes.
            let remove_remotes = remote_menu_remotes.clone();
            let remove_entity = remote_menu_entity.clone();
            let remove_placeholder = remote_menu_labels.remove_remote.clone();
            menu.item(
                PopupMenuItem::new(format!("{}…", remote_menu_labels.remove_remote))
                    .icon(HeroIconName::Trash)
                    .disabled(remove_remotes.is_empty())
                    .on_click(move |_, window, cx| {
                        let items = remote_pick_items(&remove_remotes, None, false);
                        let entity = remove_entity.clone();
                        show_quick_pick(
                            remove_placeholder.clone(),
                            items,
                            move |id, window, cx| {
                                entity.update(cx, |app, cx| {
                                    app.remove_project_git_remote(id.to_string(), window, cx);
                                });
                            },
                            window,
                            cx,
                        );
                    }),
            )
        },
    );

    // Stash ▸
    let stash_labels = labels.clone();
    let stash_entity = app_entity.clone();
    let stash_list = stashes.clone();
    let has_stashes = !stashes.is_empty();
    let menu = menu.submenu_with_icon(
        Some(Icon::new(HeroIconName::ArchiveBox)),
        labels.stash_menu.clone(),
        window,
        cx,
        move |menu, _window, _cx| {
            let menu = stash_push_item(
                menu,
                format!("{}…", stash_labels.stash_push),
                stash_labels.stash_push.clone(),
                stash_labels.stash_message_placeholder.clone(),
                false,
                has_changes,
                stash_entity.clone(),
            );
            let menu = stash_push_item(
                menu,
                format!("{}…", stash_labels.stash_push_untracked),
                stash_labels.stash_push_untracked.clone(),
                stash_labels.stash_message_placeholder.clone(),
                true,
                has_changes,
                stash_entity.clone(),
            );

            let apply_latest_entity = stash_entity.clone();
            let pop_latest_entity = stash_entity.clone();
            let menu = menu.separator().item(
                PopupMenuItem::new(stash_labels.stash_apply_latest.clone())
                    .icon(HeroIconName::ArrowUpOnSquare)
                    .disabled(!has_stashes)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&apply_latest_entity, |app, cx| {
                            app.apply_git_stash(0, window, cx)
                        });
                    }),
            );
            let menu = stash_pick_item(
                menu,
                format!("{}…", stash_labels.stash_apply),
                HeroIconName::ArrowUpOnSquare,
                stash_labels.stash_apply.clone(),
                stash_list.clone(),
                stash_entity.clone(),
                StashPickAction::Apply,
            );
            let menu = menu.item(
                PopupMenuItem::new(stash_labels.stash_pop_latest.clone())
                    .icon(HeroIconName::ArrowUpTray)
                    .disabled(!has_stashes)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&pop_latest_entity, |app, cx| {
                            app.pop_git_stash(0, window, cx)
                        });
                    }),
            );
            let menu = stash_pick_item(
                menu,
                format!("{}…", stash_labels.stash_pop),
                HeroIconName::ArrowUpTray,
                stash_labels.stash_pop.clone(),
                stash_list.clone(),
                stash_entity.clone(),
                StashPickAction::Pop,
            );

            let menu = menu.separator();
            let menu = stash_pick_item(
                menu,
                format!("{}…", stash_labels.stash_drop),
                HeroIconName::Trash,
                stash_labels.stash_drop.clone(),
                stash_list.clone(),
                stash_entity.clone(),
                StashPickAction::Drop,
            );
            let drop_all_entity = stash_entity.clone();
            menu.item(
                PopupMenuItem::new(stash_labels.stash_drop_all.clone())
                    .icon(HeroIconName::Trash)
                    .disabled(!has_stashes)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&drop_all_entity, |app, cx| {
                            app.drop_all_git_stashes(window, cx)
                        });
                    }),
            )
        },
    );

    // Tags ▸
    let tag_labels = labels.clone();
    let tag_entity = app_entity.clone();
    let tag_list = tags.clone();
    let has_tags = !tags.is_empty();
    let menu = menu.submenu_with_icon(
        Some(Icon::new(HeroIconName::Tag)),
        labels.tags_menu.clone(),
        window,
        cx,
        move |menu, _window, _cx| {
            // Create Tag… — Quick Input for the tag name.
            let create_labels = tag_labels.clone();
            let create_entity = tag_entity.clone();
            let menu = menu.item(
                PopupMenuItem::new(format!("{}…", tag_labels.tag_create))
                    .icon(HeroIconName::Plus)
                    .disabled(!has_commits)
                    .on_click(move |_, window, cx| {
                        let entity = create_entity.clone();
                        show_quick_input(
                            create_labels.tag_create.clone(),
                            create_labels.tag_name_placeholder.clone(),
                            "",
                            false,
                            move |name, window, cx| {
                                entity.update(cx, |app, cx| {
                                    app.create_git_tag(name, window, cx);
                                });
                            },
                            window,
                            cx,
                        );
                    }),
            );

            let menu = tag_pick_item(
                menu,
                format!("{}…", tag_labels.tag_delete),
                tag_labels.tag_delete.clone(),
                tag_list.clone(),
                tag_entity.clone(),
                TagPickAction::Delete,
            );
            let menu = tag_pick_item(
                menu,
                format!("{}…", tag_labels.tag_delete_remote),
                tag_labels.tag_delete_remote.clone(),
                if has_remotes {
                    tag_list.clone()
                } else {
                    Vec::new()
                },
                tag_entity.clone(),
                TagPickAction::DeleteRemote,
            );

            let push_tags_entity = tag_entity.clone();
            menu.item(
                PopupMenuItem::new(tag_labels.tag_push.clone())
                    .icon(HeroIconName::ArrowUp)
                    .disabled(!has_tags || !has_remotes)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&push_tags_entity, |app, cx| {
                            app.push_git_tags(window, cx)
                        });
                    }),
            )
        },
    );

    // Repository actions.
    let reveal_entity = app_entity.clone();
    menu.separator().item(
        PopupMenuItem::new(labels.show_repository.clone())
            .icon(HeroIconName::FolderOpen)
            .on_click(move |_, window, cx| {
                cx.update_entity(&reveal_entity, |app, cx| {
                    app.reveal_selected_project_in_file_manager(window, cx)
                });
            }),
    )
}

/// "Checkout To…" — searchable picker over local + remote branches, with a
/// leading "create new branch" row (VS Code semantics).

#[derive(Clone)]
struct GitBranchMenuLabels {
    pull: String,
    push: String,
    checkout_to: String,
    switch_branch: String,
    fetch: String,
    fetch_prune: String,
    sync: String,
    push_to: String,
    force_push: String,
    commit_menu: String,
    undo_last_commit: String,
    edit_last_commit_message: String,
    changes_menu: String,
    stage_all: String,
    unstage_all: String,
    discard_all: String,
    pull_push_menu: String,
    branch_menu: String,
    stash_menu: String,
    tags_menu: String,
    remotes: String,
    merge: String,
    squash_merge: String,
    rebase: String,
    new_branch: String,
    create_from: String,
    rename: String,
    delete_local: String,
    delete_remote: String,
    branch_name_placeholder: String,
    remote_name_placeholder: String,
    remote_url_placeholder: String,
    add_remote: String,
    set_default: String,
    copy_url: String,
    remove_remote: String,
    stash_push: String,
    stash_push_untracked: String,
    stash_apply_latest: String,
    stash_apply: String,
    stash_pop_latest: String,
    stash_pop: String,
    stash_drop: String,
    stash_drop_all: String,
    stash_message_placeholder: String,
    tag_create: String,
    tag_delete: String,
    tag_delete_remote: String,
    tag_push: String,
    tag_name_placeholder: String,
    show_repository: String,
}

impl GitBranchMenuLabels {
    fn load(language: &str) -> Self {
        let locale = locale_from_language_setting(language);
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        Self {
            pull: tr("git.remote.pull", "Pull"),
            push: tr("git.remote.push", "Push"),
            checkout_to: tr("git.menu.checkout_to", "Checkout To..."),
            switch_branch: tr("git.branch.switch", "Switch Branch"),
            fetch: tr("git.remote.fetch", "Fetch"),
            fetch_prune: tr("git.remote.fetch_prune", "Fetch (Prune)"),
            sync: tr("git.remote.sync", "Sync"),
            push_to: tr("git.remote.push_to", "Push To..."),
            force_push: tr("git.remote.force_push", "Force Push"),
            commit_menu: tr("git.commit.action", "Commit"),
            undo_last_commit: tr("git.history.undo_last_commit", "Undo Last Commit"),
            edit_last_commit_message: tr(
                "git.history.edit_last_commit_message",
                "Edit Last Commit Message",
            ),
            changes_menu: tr("git.files.changes", "Changes"),
            stage_all: tr("git.files.stage_all", "Stage All Changes"),
            unstage_all: tr("git.files.unstage_all", "Unstage All Changes"),
            discard_all: tr("git.files.discard_all", "Discard All Changes"),
            pull_push_menu: tr("git.menu.pull_push", "Pull, Push"),
            branch_menu: tr("git.menu.branch", "Branch"),
            stash_menu: tr("git.menu.stash", "Stash"),
            tags_menu: tr("git.menu.tags", "Tags"),
            remotes: tr("git.remote.remotes", "Remotes"),
            merge: tr("git.branch.merge.title", "Merge Branch"),
            squash_merge: tr("git.branch.squash_merge", "Squash Merge Branch"),
            rebase: tr("git.branch.rebase", "Rebase Branch"),
            new_branch: tr("git.branch.create_and_switch", "New Branch"),
            create_from: tr("git.branch.create_from", "Create Branch From"),
            rename: tr("git.branch.rename", "Rename Branch"),
            delete_local: tr("git.branch.delete_local", "Delete Local Branch"),
            delete_remote: tr("git.branch.delete_remote", "Delete Remote Branch"),
            branch_name_placeholder: tr("git.branch.new.message", "Enter a new branch name."),
            remote_name_placeholder: tr(
                "git.remote.add.name_message",
                "Enter the remote name first, such as origin or upstream.",
            ),
            remote_url_placeholder: tr("git.remote.add.url_message", "Enter the remote URL"),
            add_remote: tr("git.remote.add", "Add Remote"),
            set_default: tr("git.remote.set_default", "Set as Default"),
            copy_url: tr("git.remote.copy_url", "Copy URL"),
            remove_remote: tr("git.remote.remove", "Remove Remote"),
            stash_push: tr("git.stash.push", "Stash"),
            stash_push_untracked: tr("git.stash.push_untracked", "Stash (Include Untracked)"),
            stash_apply_latest: tr("git.stash.apply_latest", "Apply Latest Stash"),
            stash_apply: tr("git.stash.apply", "Apply Stash"),
            stash_pop_latest: tr("git.stash.pop_latest", "Pop Latest Stash"),
            stash_pop: tr("git.stash.pop", "Pop Stash"),
            stash_drop: tr("git.stash.drop", "Drop Stash"),
            stash_drop_all: tr("git.stash.drop_all", "Drop All Stashes"),
            stash_message_placeholder: tr(
                "git.stash.message.placeholder",
                "Stash message (optional)",
            ),
            tag_create: tr("git.tag.create", "Create Tag"),
            tag_delete: tr("git.tag.delete", "Delete Tag"),
            tag_delete_remote: tr("git.tag.delete_remote", "Delete Remote Tag"),
            tag_push: tr("git.tag.push", "Push Tags"),
            tag_name_placeholder: tr("git.tag.name.placeholder", "Tag name"),
            show_repository: tr(
                "git.repository.show_in_finder",
                "Show Repository in File Manager",
            ),
        }
    }
}
