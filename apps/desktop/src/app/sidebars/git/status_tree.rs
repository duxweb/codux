use super::*;

#[derive(Clone)]
pub(super) enum GitStatusVirtualRow {
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
        labels: Rc<GitFileMenuLabels>,
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
    pub(super) fn height(&self) -> Pixels {
        match self {
            Self::GroupHeader { .. } => px(40.0),
            Self::Spacer { height } => px(*height),
            Self::Empty { .. } => px(42.0),
            Self::Dir { .. } | Self::File { .. } => px(24.0),
            Self::Limit { .. } => px(32.0),
        }
    }

    pub(super) fn render(self, cx: &mut Context<CoduxApp>) -> AnyElement {
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
                labels,
            } => git_status_dir_row(section_id, &name, &path, expanded, depth, labels, cx)
                .into_any_element(),
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

pub(super) struct GitStatusRowsInput<'a> {
    pub(super) staged: &'a [GitFileStatus],
    pub(super) changed: &'a [GitFileStatus],
    pub(super) untracked: &'a [GitFileStatus],
    pub(super) expanded_sections: &'a HashSet<String>,
    pub(super) expanded_dirs: &'a HashSet<String>,
    pub(super) tree_children: &'a HashMap<String, Vec<GitFileStatus>>,
    pub(super) selected_file: Option<&'a str>,
    pub(super) selected_files: &'a HashSet<String>,
    pub(super) labels: &'a GitSidebarLabels,
}

pub(super) fn git_status_virtual_rows(input: GitStatusRowsInput<'_>) -> Vec<GitStatusVirtualRow> {
    let GitStatusRowsInput {
        staged,
        changed,
        untracked,
        expanded_sections,
        expanded_dirs,
        tree_children,
        selected_file,
        selected_files,
        labels,
    } = input;
    let mut rows = Vec::new();
    let file_menu_labels = Rc::new(GitFileMenuLabels::from(labels));
    let tree_context = GitStatusTreeContext {
        expanded_dirs,
        tree_children,
        selected_file,
        selected_files,
        file_menu_labels,
    };
    append_git_status_group_virtual_rows(
        GitStatusGroupInput {
            id: "staged",
            title: labels.staged.clone(),
            files: staged,
            expanded_sections,
            empty_text: labels.staged_empty.clone(),
            tree_limit: labels.tree_limit.clone(),
            first: rows.is_empty(),
        },
        &tree_context,
        &mut rows,
    );
    append_git_status_group_virtual_rows(
        GitStatusGroupInput {
            id: "changed",
            title: labels.changed.clone(),
            files: changed,
            expanded_sections,
            empty_text: labels.changed_empty.clone(),
            tree_limit: labels.tree_limit.clone(),
            first: rows.is_empty(),
        },
        &tree_context,
        &mut rows,
    );
    append_git_status_group_virtual_rows(
        GitStatusGroupInput {
            id: "untracked",
            title: labels.untracked.clone(),
            files: untracked,
            expanded_sections,
            empty_text: labels.untracked_empty.clone(),
            tree_limit: labels.tree_limit.clone(),
            first: rows.is_empty(),
        },
        &tree_context,
        &mut rows,
    );
    rows
}

struct GitStatusTreeContext<'a> {
    expanded_dirs: &'a HashSet<String>,
    tree_children: &'a HashMap<String, Vec<GitFileStatus>>,
    selected_file: Option<&'a str>,
    selected_files: &'a HashSet<String>,
    file_menu_labels: Rc<GitFileMenuLabels>,
}

struct GitStatusGroupInput<'a> {
    id: &'static str,
    title: String,
    files: &'a [GitFileStatus],
    expanded_sections: &'a HashSet<String>,
    empty_text: String,
    tree_limit: String,
    first: bool,
}

fn append_git_status_group_virtual_rows(
    input: GitStatusGroupInput<'_>,
    tree_context: &GitStatusTreeContext<'_>,
    rows: &mut Vec<GitStatusVirtualRow>,
) {
    let GitStatusGroupInput {
        id,
        title,
        files,
        expanded_sections,
        empty_text,
        tree_limit,
        first,
    } = input;
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
    append_git_status_virtual_directory_rows(id, "", files, 0, tree_context, rows);
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
    tree_context: &GitStatusTreeContext<'_>,
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
        let expanded = tree_context.expanded_dirs.contains(&tree_key);
        rows.push(GitStatusVirtualRow::Dir {
            section_id,
            name,
            path: dir.path.clone(),
            expanded,
            depth,
            labels: tree_context.file_menu_labels.clone(),
        });
        if expanded && let Some(children) = tree_context.tree_children.get(&tree_key) {
            append_git_status_virtual_directory_rows(
                section_id,
                &dir.path,
                children,
                depth + 1,
                tree_context,
                rows,
            );
        }
    }
    for file in direct_files {
        if rows.len() >= MAX_GIT_STATUS_TREE_ROWS {
            return;
        }
        let active = tree_context
            .selected_file
            .map(|path| path == file.path.as_str())
            .unwrap_or(false);
        rows.push(GitStatusVirtualRow::File {
            file,
            active,
            selected_files: tree_context.selected_files.clone(),
            depth,
            labels: tree_context.file_menu_labels.clone(),
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
        // Thin translucent darkening: deeper than the panel, still see-through.
        .bg(theme::vibrancy_raised(cx.theme().list_head))
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
                        .rounded(px(6.0))
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

pub(super) fn relative_git_status_path<'a>(base_path: &str, file_path: &'a str) -> Option<&'a str> {
    let base_path = base_path.trim_matches('/');
    if base_path.is_empty() {
        return Some(file_path);
    }
    file_path
        .strip_prefix(base_path)
        .and_then(|path| path.strip_prefix('/'))
}

pub(super) fn join_git_path(base_path: &str, name: &str) -> String {
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
