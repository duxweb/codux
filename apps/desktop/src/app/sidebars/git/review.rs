use super::*;

pub(super) fn git_review_empty_workspace(message: String) -> impl IntoElement {
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

pub(super) fn git_review_error_message(error: &str, labels: &GitSidebarLabels) -> String {
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
pub(super) enum GitReviewLineTone {
    Addition,
    Deletion,
}

#[derive(Clone, Default)]
pub(in crate::app) struct GitReviewDerivedRows {
    pub(super) original: Rc<Vec<GitReviewAlignedCell>>,
    pub(super) new_file: Rc<Vec<GitReviewAlignedCell>>,
    pub(super) final_file: Rc<Vec<GitReviewAlignedCell>>,
    pub(super) branch: Option<Rc<Vec<GitReviewAlignedCell>>>,
}

#[derive(Clone, Default)]
pub(super) struct GitReviewAlignedCell {
    pub(super) line_number: Option<usize>,
    pub(super) text: String,
    pub(super) tone: Option<GitReviewLineTone>,
}

pub(super) fn git_review_content_panel(
    list_id: &'static str,
    title: &str,
    cells: Rc<Vec<GitReviewAlignedCell>>,
    scroll_handle: VirtualListScrollHandle,
    cx: &mut Context<workspace_views::ReviewDiffContentView>,
) -> impl IntoElement {
    // Declare each row's intrinsic width from the longest line so the virtual
    // list reports a wide content size and offers horizontal scrolling; without
    // this the list assumes ~0 width and clips long lines instead.
    let content_width = review_content_width(&cells);
    let item_sizes = Rc::new(vec![size(content_width, px(18.0)); cells.len()]);
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
                                    Some(git_review_code_line(cell.clone(), content_width))
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

/// Estimated pixel width of the widest line, used to size the virtual list's
/// content so it can scroll horizontally. SF Mono at ~12px advances ~7.2px per
/// glyph; a small buffer avoids clipping the last characters.
fn review_content_width(cells: &[GitReviewAlignedCell]) -> Pixels {
    let max_chars = cells
        .iter()
        .map(|cell| cell.text.chars().count())
        .max()
        .unwrap_or(0);
    px(60.0 + max_chars as f32 * 7.5)
}

fn git_review_code_line(cell: GitReviewAlignedCell, content_width: Pixels) -> AnyElement {
    let line_bg = match cell.tone {
        Some(GitReviewLineTone::Addition) => Some(color(theme::GREEN).opacity(0.13)),
        Some(GitReviewLineTone::Deletion) => Some(color(theme::RED).opacity(0.14)),
        None => None,
    };
    div()
        .h(px(18.0))
        .flex()
        // Fixed to the longest-line width (≥ viewport via the list) so row
        // backgrounds span the full scrollable width and lines stay aligned
        // across the three columns when scrolled horizontally.
        .w(content_width)
        .min_w(gpui::relative(1.0))
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
            // Natural width, single line — long content overflows and is reached
            // via the list's horizontal scroll instead of being clipped.
            div().flex_none().whitespace_nowrap().child(cell.text),
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
            let tree_context = GitReviewTreeContext {
                selected_path,
                expanded_dirs: &expanded_dirs,
                labels,
                app_entity,
            };
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .py_2()
                .children(git_review_directory_rows(
                    &review.files,
                    "",
                    0,
                    &tree_context,
                    cx,
                ))
                .into_any_element()
        })
}

struct GitReviewTreeContext<'a> {
    selected_path: Option<&'a str>,
    expanded_dirs: &'a HashSet<String>,
    labels: Rc<GitSidebarLabels>,
    app_entity: gpui::Entity<CoduxApp>,
}

fn git_review_directory_rows(
    files: &[GitReviewFile],
    base_path: &str,
    depth: usize,
    tree_context: &GitReviewTreeContext<'_>,
    cx: &mut Context<workspace_views::ReviewFileListView>,
) -> Vec<AnyElement> {
    let (dirs, direct_files) = collect_immediate_git_review_entries(base_path, files);
    let mut rows = Vec::new();
    for (name, dir) in dirs {
        if rows.len() >= MAX_GIT_REVIEW_TREE_ROWS {
            rows.push(
                git_review_tree_limit_row(files.len(), &tree_context.labels).into_any_element(),
            );
            return rows;
        }
        let expanded = tree_context
            .expanded_dirs
            .contains(&git_status_tree_key("review", &dir.path));
        rows.push(
            git_review_dir_row(
                &name,
                &dir,
                expanded,
                depth,
                tree_context.app_entity.clone(),
                cx,
            )
            .into_any_element(),
        );
        if expanded {
            rows.extend(git_review_directory_rows(
                files,
                &dir.path,
                depth + 1,
                tree_context,
                cx,
            ));
        }
    }
    for file in direct_files {
        if rows.len() >= MAX_GIT_REVIEW_TREE_ROWS {
            rows.push(
                git_review_tree_limit_row(files.len(), &tree_context.labels).into_any_element(),
            );
            return rows;
        }
        let selected = tree_context.selected_path == Some(file.path.as_str());
        rows.push(
            git_review_file_row(file, selected, depth, tree_context.app_entity.clone(), cx)
                .into_any_element(),
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
pub(super) struct GitReviewDirSummary {
    pub(super) path: String,
    pub(super) count: usize,
    pub(super) additions: i64,
    pub(super) deletions: i64,
}

pub(super) fn collect_immediate_git_review_entries(
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
