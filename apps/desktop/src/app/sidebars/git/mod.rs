use super::*;
use crate::app::quick_input::show_quick_input;
use crate::app::quick_pick::{QuickPickItem, show_quick_pick};
use crate::app::ui_helpers::codux_tooltip_container;
use codux_runtime::git::{GitReviewFile, GitStashSummary};
use gpui::{ClickEvent, Div, ListSizingBehavior, Pixels, Stateful};
use gpui_component::{
    Size,
    input::{Input, InputEvent, InputState},
    progress::Progress,
};
use std::ops::Range;

mod branch_menu;
mod clone_ui;
mod diff_window;
mod history;
mod labels;
mod panels;
mod review;
mod status_format;
mod status_rows;
mod status_tree;

use branch_menu::*;
use clone_ui::git_empty_repository_panel;
pub(in crate::app) use clone_ui::{git_clone_window_workspace, git_credentials_window_workspace};
use diff_window::*;
use history::*;
pub(in crate::app) use labels::{GitSectionInput, GitSidebarLabels, git_section};
use panels::*;
use review::*;
pub(in crate::app) use review::{
    GitReviewDerivedRows, build_git_review_derived_rows, git_review_file_list,
};
pub(in crate::app) use status_format::git_review_workspace;
use status_format::*;
pub(in crate::app) use status_rows::git_diff_window_workspace;
use status_rows::*;
use status_tree::*;
type GitTreeChildrenSnapshot = Vec<(String, Vec<(String, String, String)>)>;
#[derive(Clone, PartialEq)]
pub(in crate::app) struct GitFilesPanelSnapshot {
    language: String,
    branch: String,
    changed_files: Vec<(String, String, String)>,
    expanded_sections: Vec<String>,
    expanded_dirs: Vec<String>,
    tree_children: GitTreeChildrenSnapshot,
    selected_file: Option<String>,
    selected_files: Vec<String>,
}

pub(in crate::app) struct GitFilesPanelView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: GitFilesPanelSnapshot,
}

impl GitFilesPanelView {
    fn set_snapshot(&mut self, snapshot: GitFilesPanelSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for GitFilesPanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = self.app_entity.clone();
        app_entity.update(cx, |app, cx| {
            let labels = Rc::new(GitSidebarLabels::load(&app.state.settings.language));
            let staged = app
                .state
                .git
                .changed_files
                .iter()
                .filter(|file| is_git_staged_file(file))
                .cloned()
                .collect::<Vec<_>>();
            let changed = app
                .state
                .git
                .changed_files
                .iter()
                .filter(|file| is_git_worktree_file(file))
                .cloned()
                .collect::<Vec<_>>();
            let untracked = app
                .state
                .git
                .changed_files
                .iter()
                .filter(|file| is_git_untracked_file(file))
                .cloned()
                .collect::<Vec<_>>();

            git_files_panel(
                GitFilesPanelInput {
                    staged: &staged,
                    changed: &changed,
                    untracked: &untracked,
                    expanded_sections: &app.git_expanded_sections,
                    expanded_dirs: &app.git_expanded_dirs,
                    tree_children: &app.git_tree_children,
                    selected_file: app.selected_git_file.as_deref(),
                    selected_files: &app.selected_git_files,
                    labels,
                    scroll_handle: app.git_files_scroll_handle.clone(),
                },
                cx,
            )
            .into_any_element()
        })
    }
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct GitHistoryPanelSnapshot {
    language: String,
    commits: Vec<(String, String, String, Option<String>, String, String)>,
}

pub(in crate::app) struct GitHistoryPanelView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: GitHistoryPanelSnapshot,
}

impl GitHistoryPanelView {
    fn set_snapshot(&mut self, snapshot: GitHistoryPanelSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for GitHistoryPanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = self.app_entity.clone();
        app_entity.update(cx, |app, cx| {
            let labels = Rc::new(GitSidebarLabels::load(&app.state.settings.language));
            git_history_panel(
                &app.state.git,
                labels,
                app.git_history_scroll_handle.clone(),
                cx,
            )
            .into_any_element()
        })
    }
}

impl CoduxApp {
    pub(in crate::app) fn git_files_panel_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<GitFilesPanelView> {
        let snapshot = self.git_files_panel_snapshot();
        if let Some(view) = self.git_files_panel_view.clone() {
            view.update(cx, |view: &mut GitFilesPanelView, cx| {
                view.set_snapshot(snapshot, cx)
            });
            return view;
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| GitFilesPanelView {
            app_entity,
            snapshot,
        });
        self.git_files_panel_view = Some(view.clone());
        view
    }

    pub(in crate::app) fn git_history_panel_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<GitHistoryPanelView> {
        let snapshot = self.git_history_panel_snapshot();
        if let Some(view) = self.git_history_panel_view.clone() {
            view.update(cx, |view: &mut GitHistoryPanelView, cx| {
                view.set_snapshot(snapshot, cx)
            });
            return view;
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| GitHistoryPanelView {
            app_entity,
            snapshot,
        });
        self.git_history_panel_view = Some(view.clone());
        view
    }

    fn git_files_panel_snapshot(&self) -> GitFilesPanelSnapshot {
        let mut expanded_sections = self
            .git_expanded_sections
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        expanded_sections.sort();
        let mut expanded_dirs = self.git_expanded_dirs.iter().cloned().collect::<Vec<_>>();
        expanded_dirs.sort();
        let mut selected_files = self.selected_git_files.iter().cloned().collect::<Vec<_>>();
        selected_files.sort();
        let mut tree_children = self
            .git_tree_children
            .iter()
            .map(|(path, files)| {
                (
                    path.clone(),
                    files.iter().map(git_file_status_tuple).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        tree_children.sort_by(|left, right| left.0.cmp(&right.0));

        GitFilesPanelSnapshot {
            language: self.state.settings.language.clone(),
            branch: self.state.git.branch.clone(),
            changed_files: self
                .state
                .git
                .changed_files
                .iter()
                .map(git_file_status_tuple)
                .collect(),
            expanded_sections,
            expanded_dirs,
            tree_children,
            selected_file: self.selected_git_file.clone(),
            selected_files,
        }
    }

    fn git_history_panel_snapshot(&self) -> GitHistoryPanelSnapshot {
        GitHistoryPanelSnapshot {
            language: self.state.settings.language.clone(),
            commits: self
                .state
                .git
                .commits
                .iter()
                .map(|commit| {
                    (
                        commit.hash.clone(),
                        commit.title.clone(),
                        commit.relative_time.clone(),
                        commit.decorations.clone(),
                        commit.graph_prefix.clone(),
                        commit.author.clone(),
                    )
                })
                .collect(),
        }
    }
}

fn git_file_status_tuple(file: &GitFileStatus) -> (String, String, String) {
    (
        file.path.clone(),
        file.index_status.clone(),
        file.worktree_status.clone(),
    )
}
