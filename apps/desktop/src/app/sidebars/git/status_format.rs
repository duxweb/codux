use super::*;

pub(super) fn is_git_staged_file(file: &GitFileStatus) -> bool {
    let index = file.index_status.trim();
    !index.is_empty() && index != "?"
}

pub(super) fn is_git_worktree_file(file: &GitFileStatus) -> bool {
    !is_git_untracked_file(file) && !file.worktree_status.trim().is_empty()
}

pub(super) fn is_git_untracked_file(file: &GitFileStatus) -> bool {
    file.worktree_status == "?" && (file.index_status == "?" || file.index_status.trim().is_empty())
}

pub(super) fn git_file_status_label(file: &GitFileStatus) -> String {
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

pub(super) fn git_file_status_color(status: &str) -> u32 {
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
                                .text_color(color(theme::RED))
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
                    "git-review-current-code",
                    labels.diff_current.as_str(),
                    derived_rows.current.clone(),
                    VirtualListScrollHandle::from(code_scroll_handle),
                    cx,
                ))
        });
    body.into_any_element()
}
