use super::*;

pub(super) fn git_history_panel(
    git: &GitSummary,
    labels: Rc<GitSidebarLabels>,
    scroll_handle: VirtualListScrollHandle,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let commits = Rc::new(git.commits.clone());
    let commit_count = commits.len();
    let menu_labels = Rc::new(GitHistoryMenuLabels::from(labels.as_ref()));
    let head_label = SharedString::from(match git.branch.as_str() {
        "" | "HEAD" | "uninitialized" => "HEAD".to_string(),
        branch => format!("HEAD->{branch}"),
    });
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
                // Thin translucent darkening: deeper than the panel, see-through.
                .bg(theme::vibrancy_raised(cx.theme().list_head))
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
                                            head_label.clone(),
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
    head_label: SharedString,
    labels: Rc<GitHistoryMenuLabels>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title = commit.title.clone();
    // Comma-joined tag names from the commit log (see git2_commit_log).
    let tags: Vec<SharedString> = commit
        .decorations
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|name| !name.is_empty())
        .map(|name| SharedString::from(name.to_string()))
        .collect();
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
                        .bg(color(theme::TEXT_MUTED).opacity(0.82)),
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
                        .bg(color(theme::TEXT_MUTED).opacity(0.82)),
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
                            .whitespace_nowrap()
                            .child(head_label.clone())
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .children(tags.iter().cloned().map(|tag| {
                        div()
                            .rounded(px(6.0))
                            .px_2()
                            .py(px(2.0))
                            .bg(color(theme::ORANGE).opacity(0.16))
                            .text_size(rems(0.75))
                            .line_height(rems(0.875))
                            .text_color(color(theme::ORANGE))
                            .whitespace_nowrap()
                            .child(tag)
                    })),
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
