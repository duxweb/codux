use super::*;

pub(super) fn git_status_dir_row(
    section_id: &'static str,
    name: &str,
    path: &str,
    expanded: bool,
    depth: usize,
    labels: Rc<GitFileMenuLabels>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let directory_path = path.to_string();
    let directory_section = section_id.to_string();
    // Mirror the file-row capabilities, but resolve them from the section the
    // directory lives in rather than per-file status: a folder under "Staged"
    // can only be unstaged, one under "Changes"/"Untracked" can be staged or
    // discarded, and only an untracked folder can be added to .gitignore. The
    // batched git path operations recurse through a directory pathspec, so a
    // single folder path covers every changed file beneath it.
    let can_stage = matches!(section_id, "changed" | "untracked");
    let can_unstage = section_id == "staged";
    let can_discard = matches!(section_id, "changed" | "untracked");
    let can_ignore = section_id == "untracked";
    let menu_path = path.to_string();
    // .gitignore entries are directory-precise with a trailing slash, matching
    // how an untracked-directory marker is ignored from the file row.
    let ignore_path = format!("{}/", path.trim_end_matches('/'));
    let app_entity = cx.entity();

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
        .context_menu(move |menu, _window, _cx| {
            let stage_entity = app_entity.clone();
            let stage_path = menu_path.clone();
            let unstage_entity = app_entity.clone();
            let unstage_path = menu_path.clone();
            let discard_entity = app_entity.clone();
            let discard_path = menu_path.clone();
            let ignore_entity = app_entity.clone();
            let ignore_path = ignore_path.clone();

            let menu = if can_stage {
                menu.item(
                    git_context_menu_item(labels.stage.clone(), HeroIconName::Plus).on_click(
                        move |_, window, cx| {
                            cx.update_entity(&stage_entity, |app, cx| {
                                app.stage_git_paths(vec![stage_path.clone()], window, cx);
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
                                app.unstage_git_paths(vec![unstage_path.clone()], window, cx);
                            });
                        },
                    ),
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
                            app.discard_git_paths(vec![discard_path.clone()], window, cx);
                        });
                    }),
                )
            } else {
                menu
            };
            if can_ignore {
                menu.item(
                    git_context_menu_item(labels.add_gitignore.clone(), HeroIconName::XMark)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&ignore_entity, |app, cx| {
                                app.append_project_gitignore_paths(
                                    vec![ignore_path.clone()],
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
}

pub(super) fn git_status_file_row(
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
pub(super) struct GitFileMenuLabels {
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
                Icon::new(icon)
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
