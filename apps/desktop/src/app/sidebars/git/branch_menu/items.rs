use super::*;

pub(super) fn checkout_to_item(
    label: String,
    labels: Rc<GitBranchMenuLabels>,
    branches: Vec<GitBranchSummary>,
    remote_refs: Vec<String>,
    app_entity: gpui::Entity<CoduxApp>,
) -> PopupMenuItem {
    const CREATE_ID: &str = "\u{0}create-branch";
    PopupMenuItem::new(label)
        .icon(HeroIconName::ArrowPathRoundedSquare)
        .on_click(move |_, window, cx| {
            let mut items = vec![
                QuickPickItem::new(CREATE_ID, labels.new_branch.clone())
                    .icon(Icon::new(HeroIconName::Plus)),
            ];
            for branch in &branches {
                let icon = if branch.is_current {
                    HeroIconName::Check
                } else {
                    HeroIconName::ArrowPathRoundedSquare
                };
                items.push(
                    QuickPickItem::new(branch.name.clone(), branch.name.clone())
                        .icon(Icon::new(icon)),
                );
            }
            let mut remote_lookup = HashSet::new();
            for reference in &remote_refs {
                if remote_lookup.insert(reference.clone()) {
                    items.push(
                        QuickPickItem::new(reference.clone(), reference.clone())
                            .icon(Icon::new(HeroIconName::GlobeAlt)),
                    );
                }
            }
            let entity = app_entity.clone();
            let labels = labels.clone();
            show_quick_pick(
                labels.checkout_to.clone(),
                items,
                move |id, window, cx| {
                    if id.as_ref() == CREATE_ID {
                        let entity = entity.clone();
                        show_quick_input(
                            labels.new_branch.clone(),
                            labels.branch_name_placeholder.clone(),
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
                        return;
                    }
                    let is_remote = remote_lookup.contains(id.as_ref());
                    entity.update(cx, |app, cx| {
                        if is_remote {
                            app.checkout_git_remote_branch(id.to_string(), window, cx);
                        } else {
                            app.select_git_branch(id.to_string(), window, cx);
                            app.checkout_selected_git_branch(window, cx);
                        }
                    });
                },
                window,
                cx,
            );
        })
}

#[derive(Clone, Copy)]
pub(super) enum BranchPickAction {
    Merge,
    Squash,
    Rebase,
    Delete,
}

/// A menu item that opens a searchable Quick Pick over `candidates` and applies
/// `action` to the chosen branch. Disabled when there are no candidates.
pub(super) fn branch_pick_item(
    menu: PopupMenu,
    label: String,
    icon: HeroIconName,
    placeholder: String,
    candidates: Vec<GitBranchSummary>,
    app_entity: gpui::Entity<CoduxApp>,
    action: BranchPickAction,
) -> PopupMenu {
    let enabled = !candidates.is_empty();
    menu.item(
        PopupMenuItem::new(label)
            .icon(icon)
            .disabled(!enabled)
            .on_click(move |_, window, cx| {
                let items: Vec<QuickPickItem> = candidates
                    .iter()
                    .map(|branch| {
                        QuickPickItem::new(branch.name.clone(), branch.name.clone())
                            .icon(Icon::new(HeroIconName::ArrowPathRoundedSquare))
                    })
                    .collect();
                let entity = app_entity.clone();
                show_quick_pick(
                    placeholder.clone(),
                    items,
                    move |id, window, cx| {
                        entity.update(cx, |app, cx| match action {
                            BranchPickAction::Merge => {
                                app.merge_git_branch(id.to_string(), window, cx);
                            }
                            BranchPickAction::Squash => {
                                app.squash_merge_git_branch(id.to_string(), window, cx);
                            }
                            BranchPickAction::Rebase => {
                                app.rebase_git_branch(id.to_string(), window, cx);
                            }
                            BranchPickAction::Delete => {
                                app.select_git_branch(id.to_string(), window, cx);
                                app.delete_selected_git_branch(window, cx);
                            }
                        });
                    },
                    window,
                    cx,
                );
            }),
    )
}

#[derive(Clone, Copy)]
pub(super) enum StashPickAction {
    Apply,
    Pop,
    Drop,
}

/// A menu item that opens a Quick Pick over the stash list. Disabled when empty.
pub(super) fn stash_pick_item(
    menu: PopupMenu,
    label: String,
    icon: HeroIconName,
    placeholder: String,
    stashes: Vec<GitStashSummary>,
    app_entity: gpui::Entity<CoduxApp>,
    action: StashPickAction,
) -> PopupMenu {
    let enabled = !stashes.is_empty();
    menu.item(
        PopupMenuItem::new(label)
            .icon(icon)
            .disabled(!enabled)
            .on_click(move |_, window, cx| {
                let items: Vec<QuickPickItem> = stashes
                    .iter()
                    .map(|stash| {
                        QuickPickItem::new(
                            stash.index.to_string(),
                            format!("stash@{{{}}}", stash.index),
                        )
                        .description(stash.message.clone())
                        .icon(Icon::new(HeroIconName::ArchiveBox))
                    })
                    .collect();
                let entity = app_entity.clone();
                let stashes = stashes.clone();
                show_quick_pick(
                    placeholder.clone(),
                    items,
                    move |id, window, cx| {
                        let Ok(index) = id.as_ref().parse::<usize>() else {
                            return;
                        };
                        let stash_label = stashes
                            .iter()
                            .find(|stash| stash.index == index)
                            .map(|stash| format!("stash@{{{}}}: {}", stash.index, stash.message))
                            .unwrap_or_else(|| format!("stash@{{{index}}}"));
                        entity.update(cx, |app, cx| match action {
                            StashPickAction::Apply => app.apply_git_stash(index, window, cx),
                            StashPickAction::Pop => app.pop_git_stash(index, window, cx),
                            StashPickAction::Drop => {
                                app.drop_git_stash(index, stash_label, window, cx)
                            }
                        });
                    },
                    window,
                    cx,
                );
            }),
    )
}

/// A "Stash…" menu item that asks for an optional message via Quick Input.
pub(super) fn stash_push_item(
    menu: PopupMenu,
    label: String,
    title: String,
    placeholder: String,
    include_untracked: bool,
    has_changes: bool,
    app_entity: gpui::Entity<CoduxApp>,
) -> PopupMenu {
    menu.item(
        PopupMenuItem::new(label)
            .icon(HeroIconName::ArchiveBoxArrowDown)
            .disabled(!has_changes)
            .on_click(move |_, window, cx| {
                let entity = app_entity.clone();
                show_quick_input(
                    title.clone(),
                    placeholder.clone(),
                    "",
                    true,
                    move |message, window, cx| {
                        let message = (!message.is_empty()).then_some(message);
                        entity.update(cx, |app, cx| {
                            app.stash_git(message, include_untracked, window, cx);
                        });
                    },
                    window,
                    cx,
                );
            }),
    )
}

#[derive(Clone, Copy)]
pub(super) enum TagPickAction {
    Delete,
    DeleteRemote,
}

/// A menu item that opens a Quick Pick over the tag list. Disabled when empty.
pub(super) fn tag_pick_item(
    menu: PopupMenu,
    label: String,
    placeholder: String,
    tags: Vec<String>,
    app_entity: gpui::Entity<CoduxApp>,
    action: TagPickAction,
) -> PopupMenu {
    let enabled = !tags.is_empty();
    menu.item(
        PopupMenuItem::new(label)
            .icon(HeroIconName::Trash)
            .disabled(!enabled)
            .on_click(move |_, window, cx| {
                let items: Vec<QuickPickItem> = tags
                    .iter()
                    .map(|tag| {
                        QuickPickItem::new(tag.clone(), tag.clone())
                            .icon(Icon::new(HeroIconName::Tag))
                    })
                    .collect();
                let entity = app_entity.clone();
                show_quick_pick(
                    placeholder.clone(),
                    items,
                    move |id, window, cx| {
                        entity.update(cx, |app, cx| match action {
                            TagPickAction::Delete => app.delete_git_tag(id.to_string(), window, cx),
                            TagPickAction::DeleteRemote => {
                                app.delete_git_remote_tag(id.to_string(), window, cx)
                            }
                        });
                    },
                    window,
                    cx,
                );
            }),
    )
}

/// Build Quick Pick items from remotes, using the remote name as the id. When
/// `mark_default` is set the current default push remote gets a check icon.
pub(super) fn remote_pick_items(
    remotes: &[GitRemoteSummary],
    default_remote: Option<&str>,
    mark_default: bool,
) -> Vec<QuickPickItem> {
    remotes
        .iter()
        .map(|remote| {
            let is_default = default_remote == Some(remote.name.as_str());
            let icon = if mark_default && is_default {
                HeroIconName::Check
            } else {
                HeroIconName::GlobeAlt
            };
            let mut item =
                QuickPickItem::new(remote.name.clone(), remote.name.clone()).icon(Icon::new(icon));
            if !remote.url.trim().is_empty() {
                item = item.description(remote.url.clone());
            }
            item
        })
        .collect()
}
