use super::*;

mod ai;
mod files;
mod git;
mod ssh;

use ai::ai_stats_sidebar;
pub(in crate::app) use ai::memory_manager_window_workspace;
pub(in crate::app) use files::{FileTreeRow, file_section};
pub(in crate::app) use files::{clipboard_external_paths, file_tree_rows};
pub(in crate::app) use git::git_section;
pub(in crate::app) use ssh::ssh_section;

pub(in crate::app) use files::{
    current_directory_suffix, file_directory_option, parent_relative_directory,
};
pub(in crate::app) use git::{
    GitFilesPanelView, GitHistoryPanelView, GitReviewAlignedRows, GitSidebarLabels,
    build_git_review_aligned_rows, git_clone_window_workspace, git_credentials_window_workspace,
    git_diff_window_workspace, git_review_file_list, git_review_workspace,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AssistantPanel {
    AIStats,
    SSH,
    FileManager,
    Git,
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct AIStatsSidebarSnapshot {
    selected_project_id: Option<String>,
    statistics_mode: String,
    language: String,
    global_fingerprint: u64,
    history_fingerprint: u64,
    runtime_fingerprint: u64,
}

pub(in crate::app) struct AIStatsSidebarView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: AIStatsSidebarSnapshot,
}

impl AIStatsSidebarView {
    fn set_snapshot(&mut self, snapshot: AIStatsSidebarSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for AIStatsSidebarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = self.app_entity.clone();
        app_entity.update(cx, |app, cx| {
            ai_stats_sidebar(
                &app.state.ai_global_history,
                &app.state.ai_history,
                app.state
                    .selected_project
                    .as_ref()
                    .map(|project| project.id.as_str()),
                &app.state.settings.statistics_mode,
                &app.state.ai_runtime_state,
                &app.state.settings.language,
                cx,
            )
            .into_any_element()
        })
    }
}

impl CoduxApp {
    pub(in crate::app) fn ai_stats_sidebar_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<AIStatsSidebarView> {
        let snapshot = self.ai_stats_sidebar_snapshot();
        if let Some(view) = &self.ai_stats_sidebar_view {
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| AIStatsSidebarView {
            app_entity,
            snapshot,
        });
        self.ai_stats_sidebar_view = Some(view.clone());
        view
    }

    fn ai_stats_sidebar_snapshot(&self) -> AIStatsSidebarSnapshot {
        AIStatsSidebarSnapshot {
            selected_project_id: self
                .state
                .selected_project
                .as_ref()
                .map(|project| project.id.clone()),
            statistics_mode: self.state.settings.statistics_mode.clone(),
            language: self.state.settings.language.clone(),
            global_fingerprint: ai_global_history_fingerprint(&self.state.ai_global_history),
            history_fingerprint: ai_history_fingerprint(&self.state.ai_history),
            runtime_fingerprint: ai_runtime_state_fingerprint(&self.state.ai_runtime_state),
        }
    }
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct SshSidebarSnapshot {
    profiles_fingerprint: u64,
    selected_profile_id: Option<String>,
    language: String,
}

pub(in crate::app) struct SshSidebarView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: SshSidebarSnapshot,
}

impl SshSidebarView {
    fn set_snapshot(&mut self, snapshot: SshSidebarSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for SshSidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = self.app_entity.clone();
        app_entity.update(cx, |app, cx| {
            ssh_section(
                &app.state.ssh,
                app.selected_ssh_profile_id.as_deref(),
                app.ssh_scroll_handle.clone(),
                &app.state.settings.language,
                window,
                cx,
            )
            .into_any_element()
        })
    }
}

impl CoduxApp {
    pub(in crate::app) fn ssh_sidebar_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<SshSidebarView> {
        let snapshot = self.ssh_sidebar_snapshot();
        if let Some(view) = &self.ssh_sidebar_view {
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| SshSidebarView {
            app_entity,
            snapshot,
        });
        self.ssh_sidebar_view = Some(view.clone());
        view
    }

    fn ssh_sidebar_snapshot(&self) -> SshSidebarSnapshot {
        SshSidebarSnapshot {
            profiles_fingerprint: ssh_fingerprint(&self.state.ssh),
            selected_profile_id: self.selected_ssh_profile_id.clone(),
            language: self.state.settings.language.clone(),
        }
    }
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct GitSidebarSnapshot {
    git_fingerprint: u64,
    interaction_fingerprint: u64,
}

pub(in crate::app) struct GitSidebarView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: GitSidebarSnapshot,
}

impl GitSidebarView {
    fn set_snapshot(&mut self, snapshot: GitSidebarSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for GitSidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = self.app_entity.clone();
        app_entity.update(cx, |app, cx| {
            let files_panel_view = app.git_files_panel_view(cx);
            let history_panel_view = app.git_history_panel_view(cx);
            git_section(
                &app.state.git,
                app.selected_git_branch.as_deref(),
                app.state
                    .selected_project
                    .as_ref()
                    .and_then(|project| project.git_default_push_remote_name.as_deref()),
                &app.git_clone_remote_url,
                &app.state.settings.language,
                app.git_remote_editor_open,
                &app.git_remote_name,
                &app.git_remote_url,
                app.git_running_operation.as_ref(),
                &app.git_commit_message,
                app.git_commit_message_revision,
                files_panel_view,
                history_panel_view,
                window,
                cx,
            )
            .into_any_element()
        })
    }
}

impl CoduxApp {
    pub(in crate::app) fn git_sidebar_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<GitSidebarView> {
        let snapshot = self.git_sidebar_snapshot();
        if let Some(view) = &self.git_sidebar_view {
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| GitSidebarView {
            app_entity,
            snapshot,
        });
        self.git_sidebar_view = Some(view.clone());
        view
    }

    fn git_sidebar_snapshot(&self) -> GitSidebarSnapshot {
        GitSidebarSnapshot {
            git_fingerprint: git_fingerprint(&self.state.git),
            interaction_fingerprint: git_interaction_fingerprint(self),
        }
    }
}

fn hash_sidebar_value<T: std::hash::Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(value, &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

fn combine_sidebar_hashes(parts: &[u64]) -> u64 {
    hash_sidebar_value(parts)
}

fn f64_bits(value: f64) -> u64 {
    value.to_bits()
}

fn option_f64_bits(value: Option<f64>) -> Option<u64> {
    value.map(f64_bits)
}

fn ai_session_fingerprints(
    sessions: &[AISessionSummary],
) -> Vec<(String, String, u64, i64, i64, i64)> {
    sessions
        .iter()
        .map(|session| {
            (
                session.id.clone(),
                session.title.clone(),
                f64_bits(session.last_seen_at),
                session.total_tokens,
                session.cached_input_tokens,
                session.request_count,
            )
        })
        .collect()
}

fn ai_global_history_fingerprint(global: &AIGlobalHistorySummary) -> u64 {
    hash_sidebar_value(&(
        global.indexed_project_count,
        global.session_count,
        global.total_tokens,
        global.cached_input_tokens,
        global.today_total_tokens,
        global.today_cached_input_tokens,
        global
            .project_totals
            .iter()
            .map(|project| {
                (
                    project.project_path.clone(),
                    project.project_name.clone(),
                    project.session_count,
                    project.total_tokens,
                    project.cached_input_tokens,
                    project.today_total_tokens,
                )
            })
            .collect::<Vec<_>>(),
        ai_session_fingerprints(&global.recent_sessions),
        global.error.clone(),
    ))
}

fn ai_history_fingerprint(history: &AIHistorySummary) -> u64 {
    combine_sidebar_hashes(&[
        hash_sidebar_value(&(
            history.indexed,
            option_f64_bits(history.indexed_at),
            history.is_loading,
            history.queued,
            option_f64_bits(history.progress),
            history.detail.clone(),
        )),
        hash_sidebar_value(&(
            history.project_total_tokens,
            history.project_cached_input_tokens,
            history.today_total_tokens,
            history.today_cached_input_tokens,
            history.session_count,
        )),
        hash_sidebar_value(&ai_session_fingerprints(&history.sessions)),
        hash_sidebar_value(
            &history
                .heatmap
                .iter()
                .map(|day| {
                    (
                        f64_bits(day.day),
                        day.total_tokens,
                        day.cached_input_tokens,
                        day.request_count,
                    )
                })
                .collect::<Vec<_>>(),
        ),
        hash_sidebar_value(
            &history
                .today_time_buckets
                .iter()
                .map(|bucket| {
                    (
                        f64_bits(bucket.start),
                        f64_bits(bucket.end),
                        bucket.total_tokens,
                        bucket.cached_input_tokens,
                        bucket.request_count,
                    )
                })
                .collect::<Vec<_>>(),
        ),
        hash_sidebar_value(&usage_breakdown_fingerprints(&history.tool_breakdown)),
        hash_sidebar_value(&usage_breakdown_fingerprints(&history.model_breakdown)),
        hash_sidebar_value(&history.error),
    ])
}

fn usage_breakdown_fingerprints(
    items: &[codux_runtime::ai_history_normalized::AIUsageBreakdownItem],
) -> Vec<(String, i64, i64, i64)> {
    items
        .iter()
        .map(|item| {
            (
                item.key.clone(),
                item.total_tokens,
                item.cached_input_tokens,
                item.request_count,
            )
        })
        .collect()
}

fn ai_runtime_state_fingerprint(
    runtime_state: &codux_runtime::ai_runtime_state::AIRuntimeStateSummary,
) -> u64 {
    hash_sidebar_value(&(
        runtime_state.path.clone(),
        f64_bits(runtime_state.updated_at),
        runtime_state.running_count,
        runtime_state.needs_input_count,
        runtime_state.completed_count,
        runtime_state.session_count,
        runtime_state.global_total_tokens,
        runtime_state.global_cached_input_tokens,
        runtime_state
            .project_totals
            .iter()
            .map(|project| {
                (
                    project.project_id.clone(),
                    project.total_tokens,
                    project.cached_input_tokens,
                    project.running,
                    project.needs_input,
                    project.completed,
                )
            })
            .collect::<Vec<_>>(),
        runtime_state
            .sessions
            .iter()
            .map(|session| {
                (
                    session.terminal_id.clone(),
                    session.project_id.clone(),
                    session.state.clone(),
                    f64_bits(session.updated_at),
                    session.event_count,
                    session.total_tokens,
                    session.cached_input_tokens,
                    session.has_completed_turn,
                    session.was_interrupted,
                )
            })
            .collect::<Vec<_>>(),
        runtime_state.error.clone(),
    ))
}

fn ssh_fingerprint(ssh: &SSHSummary) -> u64 {
    hash_sidebar_value(&(
        ssh.wrapper_available,
        ssh.profiles
            .iter()
            .map(|profile| {
                (
                    profile.id.clone(),
                    profile.name.clone(),
                    profile.endpoint.clone(),
                    profile.credential_kind.clone(),
                    profile.updated_at,
                )
            })
            .collect::<Vec<_>>(),
        ssh.error.clone(),
    ))
}

fn git_fingerprint(git: &GitSummary) -> u64 {
    combine_sidebar_hashes(&[
        hash_sidebar_value(&(
            git.branch.clone(),
            git.upstream.clone(),
            git.ahead,
            git.behind,
            git.head_pushed,
            git.staged,
            git.unstaged,
            git.untracked,
            git.is_repository,
            git.error.clone(),
        )),
        hash_sidebar_value(
            &git.changed_files
                .iter()
                .map(git_file_status_fingerprint)
                .collect::<Vec<_>>(),
        ),
        hash_sidebar_value(
            &git.branches
                .iter()
                .map(|branch| (branch.name.clone(), branch.is_current))
                .collect::<Vec<_>>(),
        ),
        hash_sidebar_value(&git.remote_branches),
        hash_sidebar_value(
            &git.remotes
                .iter()
                .map(|remote| (remote.name.clone(), remote.url.clone()))
                .collect::<Vec<_>>(),
        ),
        hash_sidebar_value(
            &git.commits
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
                .collect::<Vec<_>>(),
        ),
    ])
}

fn git_file_status_fingerprint(status: &GitFileStatus) -> (String, String, String) {
    (
        status.path.clone(),
        status.index_status.clone(),
        status.worktree_status.clone(),
    )
}

fn sorted_strings(values: &HashSet<String>) -> Vec<String> {
    let mut values = values.iter().cloned().collect::<Vec<_>>();
    values.sort();
    values
}

fn git_interaction_fingerprint(app: &CoduxApp) -> u64 {
    let mut tree_children = app
        .git_tree_children
        .iter()
        .map(|(path, entries)| {
            (
                path.clone(),
                entries
                    .iter()
                    .map(git_file_status_fingerprint)
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    tree_children.sort_by(|left, right| left.0.cmp(&right.0));

    combine_sidebar_hashes(&[
        hash_sidebar_value(&sorted_strings(&app.git_expanded_sections)),
        hash_sidebar_value(&sorted_strings(&app.git_expanded_dirs)),
        hash_sidebar_value(&tree_children),
        hash_sidebar_value(&(
            app.selected_git_file.clone(),
            sorted_strings(&app.selected_git_files),
            app.selected_git_branch.clone(),
            app.state
                .selected_project
                .as_ref()
                .and_then(|project| project.git_default_push_remote_name.clone()),
            app.git_clone_remote_url.clone(),
            app.state.settings.language.clone(),
            app.git_remote_editor_open,
        )),
        hash_sidebar_value(&(
            app.git_remote_name.clone(),
            app.git_remote_url.clone(),
            app.git_running_operation
                .as_ref()
                .map(|operation| (operation.label.clone(), operation.cancellable)),
            app.git_commit_message.clone(),
            app.git_commit_message_revision,
        )),
    ])
}

impl CoduxApp {
    pub(in crate::app) fn file_sidebar_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<FileSidebarView> {
        let snapshot = self.file_sidebar_snapshot();

        if let Some(view) = &self.file_sidebar_view {
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }

        let app_entity = cx.entity();
        let scroll_handle = self.file_tree_scroll_handle.clone();
        let view = cx.new(|cx| FileSidebarView {
            app_entity: app_entity.clone(),
            focus_handle: cx.focus_handle(),
            snapshot,
            scroll_handle,
        });
        self.file_sidebar_view = Some(view.clone());
        view
    }

    fn file_sidebar_snapshot(&self) -> FileSidebarSnapshot {
        let files_fingerprint = files_fingerprint(&self.state.files);
        let tree_fingerprint = file_tree_children_fingerprint(&self.file_tree_children);
        let selection_fingerprint = hash_sidebar_value(&(
            self.selected_file_entry.clone(),
            sorted_strings(&self.selected_file_entries),
        ));
        let draft_fingerprint = hash_sidebar_value(&(
            file_name_draft_kind_key(self.file_name_draft_kind),
            self.file_name_draft_target.clone(),
            self.file_name_draft_value.clone(),
            self.file_name_draft_select_all,
        ));
        let rows = Rc::new(file_tree_rows(
            &self.state.files,
            &self.file_tree_children,
            &self.file_tree_expanded_dirs,
            self.selected_file_entry.as_deref(),
            &self.selected_file_entries,
            self.file_name_draft_kind,
            self.file_name_draft_target.as_deref(),
            &self.file_name_draft_value,
            0,
        ));

        FileSidebarSnapshot {
            project_name: self
                .state
                .selected_project
                .as_ref()
                .map(|project| project.name.clone())
                .unwrap_or_else(|| "Project".to_string()),
            files_empty: self.state.files.is_empty(),
            rows,
            language: self.state.settings.language.clone(),
            refreshing: self.file_panel_refreshing,
            draft_kind: self.file_name_draft_kind,
            draft_value: self.file_name_draft_value.clone(),
            draft_select_all: self.file_name_draft_select_all,
            fingerprint: combine_sidebar_hashes(&[
                files_fingerprint,
                tree_fingerprint,
                hash_sidebar_value(&sorted_strings(&self.file_tree_expanded_dirs)),
                hash_sidebar_value(&self.file_directory),
                selection_fingerprint,
                draft_fingerprint,
                hash_sidebar_value(&self.file_panel_refreshing),
                hash_sidebar_value(&self.state.settings.language),
            ]),
        }
    }
}

#[derive(Clone)]
pub(in crate::app) struct FileSidebarSnapshot {
    project_name: String,
    files_empty: bool,
    rows: Rc<Vec<FileTreeRow>>,
    language: String,
    refreshing: bool,
    draft_kind: Option<FileNameDraftKind>,
    draft_value: String,
    draft_select_all: bool,
    fingerprint: u64,
}

impl PartialEq for FileSidebarSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
    }
}

pub(in crate::app) struct FileSidebarView {
    app_entity: gpui::Entity<CoduxApp>,
    focus_handle: FocusHandle,
    snapshot: FileSidebarSnapshot,
    scroll_handle: UniformListScrollHandle,
}

impl FileSidebarView {
    fn set_snapshot(&mut self, snapshot: FileSidebarSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }

    fn defer_app_update(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
    ) {
        let app_entity = self.app_entity.clone();
        window.defer(cx, move |window, cx| {
            app_entity.update(cx, |app, cx| update(app, window, cx));
        });
    }
}

impl Render for FileSidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot.clone();
        file_section(
            self.app_entity.clone(),
            self.focus_handle.clone(),
            &snapshot.project_name,
            snapshot.files_empty,
            snapshot.draft_kind,
            &snapshot.draft_value,
            snapshot.draft_select_all,
            snapshot.rows.clone(),
            self.scroll_handle.clone(),
            &snapshot.language,
            snapshot.refreshing,
            window,
            cx,
        )
        .into_any_element()
    }
}

fn files_fingerprint(files: &[FileEntry]) -> u64 {
    hash_sidebar_value(&files.iter().map(file_entry_fingerprint).collect::<Vec<_>>())
}

fn file_tree_children_fingerprint(tree_children: &HashMap<String, Vec<FileEntry>>) -> u64 {
    let mut children = tree_children
        .iter()
        .map(|(path, entries)| {
            (
                path.clone(),
                entries
                    .iter()
                    .map(file_entry_fingerprint)
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    children.sort_by(|left, right| left.0.cmp(&right.0));
    hash_sidebar_value(&children)
}

fn file_entry_fingerprint(entry: &FileEntry) -> (String, String, u64) {
    let kind = match entry.kind {
        FileKind::Directory => "directory",
        FileKind::File => "file",
    };
    (entry.relative_path.clone(), kind.to_string(), entry.size)
}

fn file_name_draft_kind_key(kind: Option<FileNameDraftKind>) -> &'static str {
    match kind {
        Some(FileNameDraftKind::CreateFile) => "create_file",
        Some(FileNameDraftKind::CreateDirectory) => "create_directory",
        Some(FileNameDraftKind::Rename) => "rename",
        None => "none",
    }
}

fn assistant_panel_header(
    title: impl Into<SharedString>,
    icon: HeroIconName,
    action: impl IntoElement,
) -> impl IntoElement {
    let title = title.into();
    div()
        .h(px(44.0))
        .px_3()
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(color(theme::BORDER_SOFT))
        .bg(color(theme::BG_HEADER))
        .child(
            div()
                .flex()
                .items_center()
                .child(
                    Icon::new(icon)
                        .size_4()
                        .text_color(color(theme::TEXT_MUTED)),
                )
                .child(
                    div()
                        .ml(px(8.0))
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .text_color(color(theme::TEXT))
                        .child(title),
                ),
        )
        .child(action)
}

fn ai_stats_surface(cx: &mut Context<CoduxApp>) -> gpui::Hsla {
    cx.theme().secondary
}

fn ai_stats_track_surface(cx: &mut Context<CoduxApp>) -> gpui::Hsla {
    cx.theme().secondary_hover
}
