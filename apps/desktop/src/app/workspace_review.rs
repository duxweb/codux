use super::*;
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};
use gpui_component::resizable::{h_resizable, resizable_panel};

#[derive(Clone)]
pub(in crate::app) struct ReviewWorkspaceSnapshot {
    review_title: String,
    review_subtitle: String,
    changed_files_count: String,
    selected_path: Option<String>,
    review: GitReviewSummary,
    expanded_dirs: HashSet<String>,
    refreshing: bool,
    content: Option<GitReviewContentSummary>,
    derived_rows: Option<sidebars::GitReviewDerivedRows>,
    labels: Rc<sidebars::GitSidebarLabels>,
    code_scroll_handle: ScrollHandle,
    fingerprint: u64,
}

impl PartialEq for ReviewWorkspaceSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
    }
}

impl CoduxApp {
    pub(in crate::app) fn review_workspace_snapshot(&mut self) -> ReviewWorkspaceSnapshot {
        let locale = locale_from_language_setting(&self.state.settings.language);
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        let review_title = if self.git_review.mode == "taskBranch" {
            tr("worktree.review.title", "Worktree Review")
        } else {
            tr("worktree.review.audit_title", "Uncommitted Audit")
        };
        let review_subtitle = if self.git_review.mode == "taskBranch" {
            self.git_review
                .base_branch
                .as_ref()
                .map(|base| format!("{} <- {base}", self.state.git.branch))
                .unwrap_or_else(|| self.git_review.diff_stat.clone())
        } else if self.git_review.diff_stat.trim().is_empty() {
            self.state
                .selected_project
                .as_ref()
                .map(|project| project.path.clone())
                .unwrap_or_else(|| tr("worktree.review.audit_working_tree", "Working Tree"))
        } else {
            self.git_review.diff_stat.clone()
        };
        let changed_files_count = tr("worktree.review.changed_files_count_format", "%@ files")
            .replace("%@", &self.git_review.files.len().to_string());
        let selected_path = self.selected_git_file.clone();
        let selected_content_matches = self
            .git_review_content
            .as_ref()
            .is_some_and(|content| selected_path.as_deref() == Some(content.path.as_str()));
        if selected_content_matches {
            self.ensure_git_review_derived_rows();
        }
        let derived_rows = selected_content_matches
            .then_some(())
            .and(self.git_review_derived_rows.as_ref())
            .cloned();
        let content = selected_content_matches
            .then_some(())
            .and(self.git_review_content.as_ref())
            .cloned();
        let labels = Rc::new(sidebars::GitSidebarLabels::load(
            &self.state.settings.language,
        ));

        ReviewWorkspaceSnapshot {
            review_title,
            review_subtitle,
            changed_files_count,
            selected_path,
            review: self.git_review.clone(),
            expanded_dirs: self.git_expanded_dirs.clone(),
            refreshing: self.git_review_refreshing,
            content,
            derived_rows,
            labels,
            code_scroll_handle: self.git_review_code_scroll_handle.clone(),
            fingerprint: self.review_workspace_fingerprint(),
        }
    }

    fn review_workspace_fingerprint(&self) -> u64 {
        let mut expanded_dirs = self.git_expanded_dirs.iter().cloned().collect::<Vec<_>>();
        expanded_dirs.sort();
        super::workspace_views::workspace_view_hash(&[
            super::workspace_views::workspace_view_hash(&(
                self.state.settings.language.clone(),
                self.state.git.branch.clone(),
                self.state
                    .selected_project
                    .as_ref()
                    .map(|project| project.path.clone()),
            )),
            super::workspace_views::workspace_view_hash(&(
                self.git_review.mode.clone(),
                self.git_review.base_branch.clone(),
                self.git_review.diff_stat.clone(),
                self.git_review.is_repository,
                self.git_review.error.clone(),
            )),
            super::workspace_views::workspace_view_hash(
                &self
                    .git_review
                    .files
                    .iter()
                    .map(|file| {
                        (
                            file.path.clone(),
                            file.status.clone(),
                            file.additions,
                            file.deletions,
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
            super::workspace_views::workspace_view_hash(&(
                self.selected_git_file.clone(),
                expanded_dirs,
                self.git_review_refreshing,
            )),
            super::workspace_views::workspace_view_hash(&self.git_review_content.as_ref().map(
                |content| {
                    (
                        content.path.clone(),
                        content.error.clone(),
                        content.added_lines.clone(),
                        content.deleted_lines.clone(),
                        content.head_content.len(),
                        content.base_content.as_ref().map(|value| value.len()),
                        content.index_content.as_ref().map(|value| value.len()),
                        content.worktree_content.len(),
                    )
                },
            )),
        ])
    }
}

#[derive(Clone)]
pub(in crate::app) struct ReviewFileListSnapshot {
    review: GitReviewSummary,
    selected_path: Option<String>,
    expanded_dirs: HashSet<String>,
    refreshing: bool,
    labels: Rc<sidebars::GitSidebarLabels>,
    fingerprint: u64,
}

impl PartialEq for ReviewFileListSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
    }
}

#[derive(Clone)]
pub(in crate::app) struct ReviewDiffContentSnapshot {
    selected_path: Option<String>,
    review: GitReviewSummary,
    content: Option<GitReviewContentSummary>,
    derived_rows: Option<sidebars::GitReviewDerivedRows>,
    labels: Rc<sidebars::GitSidebarLabels>,
    code_scroll_handle: ScrollHandle,
    fingerprint: u64,
}

impl PartialEq for ReviewDiffContentSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
    }
}

impl ReviewWorkspaceSnapshot {
    pub(in crate::app) fn file_list_snapshot(&self) -> ReviewFileListSnapshot {
        let mut expanded_dirs = self.expanded_dirs.iter().cloned().collect::<Vec<_>>();
        expanded_dirs.sort();
        ReviewFileListSnapshot {
            review: self.review.clone(),
            selected_path: self.selected_path.clone(),
            expanded_dirs: self.expanded_dirs.clone(),
            refreshing: self.refreshing,
            labels: self.labels.clone(),
            fingerprint: super::workspace_views::workspace_view_hash(&(
                self.review_file_fingerprint(),
                self.selected_path.clone(),
                expanded_dirs,
                self.refreshing,
            )),
        }
    }

    pub(in crate::app) fn diff_content_snapshot(&self) -> ReviewDiffContentSnapshot {
        ReviewDiffContentSnapshot {
            selected_path: self.selected_path.clone(),
            review: self.review.clone(),
            content: self.content.clone(),
            derived_rows: self.derived_rows.clone(),
            labels: self.labels.clone(),
            code_scroll_handle: self.code_scroll_handle.clone(),
            fingerprint: super::workspace_views::workspace_view_hash(&(
                self.review.mode.clone(),
                self.review.is_repository,
                self.selected_path.clone(),
                review_content_fingerprint(self.content.as_ref()),
            )),
        }
    }

    fn review_file_fingerprint(&self) -> u64 {
        super::workspace_views::workspace_view_hash(
            &self
                .review
                .files
                .iter()
                .map(|file| {
                    (
                        file.path.clone(),
                        file.status.clone(),
                        file.additions,
                        file.deletions,
                    )
                })
                .collect::<Vec<_>>(),
        )
    }
}

pub(in crate::app) fn review_workspace_body(
    snapshot: ReviewWorkspaceSnapshot,
    file_list_view: gpui::Entity<workspace_views::ReviewFileListView>,
    diff_content_view: gpui::Entity<workspace_views::ReviewDiffContentView>,
    cx: &mut Context<workspace_views::ReviewWorkspaceView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .bg(color(theme::BG_TERMINAL))
        .child(review_workspace_header(&snapshot, cx))
        .child(
            h_resizable("git-review-workspace-split")
                .child(
                    resizable_panel()
                        .size(px(360.0))
                        .size_range(px(260.0)..px(520.0))
                        .child(gpui::AnyView::from(file_list_view)),
                )
                .child(
                    resizable_panel().size_range(px(520.0)..px(1600.0)).child(
                        div()
                            .size_full()
                            .min_w_0()
                            .child(gpui::AnyView::from(diff_content_view)),
                    ),
                ),
        )
}

fn review_workspace_header(
    snapshot: &ReviewWorkspaceSnapshot,
    cx: &mut Context<workspace_views::ReviewWorkspaceView>,
) -> impl IntoElement {
    div()
        .h(px(56.0))
        .px_5()
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().title_bar)
        .child(
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .size(px(32.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(color(theme::ACCENT).opacity(0.14))
                        .text_color(color(theme::ACCENT))
                        .child(Icon::new(HeroIconName::CodeBracket).size_4()),
                )
                .child(
                    div()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color(theme::TEXT))
                                .child(snapshot.review_title.clone()),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_DIM))
                                .truncate()
                                .child(snapshot.review_subtitle.clone()),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .child(snapshot.changed_files_count.clone()),
        )
}

pub(in crate::app) fn review_file_list(
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: ReviewFileListSnapshot,
    cx: &mut Context<workspace_views::ReviewFileListView>,
) -> AnyElement {
    git_review_file_list(
        app_entity,
        &snapshot.review,
        snapshot.selected_path.as_deref(),
        &snapshot.expanded_dirs,
        snapshot.refreshing,
        snapshot.labels,
        cx,
    )
    .into_any_element()
}

pub(in crate::app) fn review_diff_content(
    snapshot: ReviewDiffContentSnapshot,
    cx: &mut Context<workspace_views::ReviewDiffContentView>,
) -> AnyElement {
    git_review_workspace(
        snapshot.selected_path.as_deref(),
        &snapshot.review,
        snapshot.content.as_ref(),
        snapshot.derived_rows.as_ref(),
        snapshot.labels,
        snapshot.code_scroll_handle,
        cx,
    )
    .into_any_element()
}

fn review_content_fingerprint(content: Option<&GitReviewContentSummary>) -> Option<u64> {
    content.map(|content| {
        super::workspace_views::workspace_view_hash(&(
            content.path.clone(),
            content.error.clone(),
            content.added_lines.clone(),
            content.deleted_lines.clone(),
            content.head_content.len(),
            content.base_content.as_ref().map(|value| value.len()),
            content.index_content.as_ref().map(|value| value.len()),
            content.worktree_content.len(),
        ))
    })
}
