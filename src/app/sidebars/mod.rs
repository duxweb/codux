use super::*;

mod ai;
mod files;
mod git;
mod ssh;

use ai::ai_stats_sidebar;
pub(in crate::app) use ai::memory_manager_window_workspace;
pub(in crate::app) use files::file_section;
pub(in crate::app) use git::git_section;
pub(in crate::app) use ssh::ssh_section;

pub(in crate::app) use files::{
    current_directory_suffix, file_directory_option, file_preview_workspace,
    parent_relative_directory,
};
pub(in crate::app) use git::{
    git_diff_window_workspace, git_review_workspace, git_workspace_section,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AssistantPanel {
    AIStats,
    SSH,
    FileManager,
    Git,
}

impl CoduxApp {
    pub(super) fn assistant_column(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(panel) = self.assistant_panel else {
            return div().into_any_element();
        };
        if self.state.selected_project.is_none() {
            return div().into_any_element();
        }

        div()
            .flex()
            .flex_col()
            .w(px(318.0))
            .h_full()
            .bg(color(theme::BG_COLUMN))
            .border_l_1()
            .border_color(color(theme::BORDER_SOFT))
            .child(match panel {
                AssistantPanel::AIStats => div()
                    .flex()
                    .min_h_0()
                    .flex_col()
                    .child(ai_stats_sidebar(
                        &self.state.ai_global_history,
                        &self.state.ai_history,
                        self.state
                            .selected_project
                            .as_ref()
                            .map(|project| project.id.as_str()),
                        &self.state.settings.statistics_mode,
                        &self.state.memory,
                        &self.state.memory_manager,
                        self.memory_manager_tab,
                        &self.state.runtime_events,
                        &self.state.ai_runtime_state,
                        &self.state.runtime_activity,
                        &self.runtime_ingress,
                        self.state.ai_session_detail.as_ref(),
                        self.selected_ai_session_id.as_deref(),
                        self.selected_memory_entry_id.as_deref(),
                        self.selected_memory_summary_id.as_deref(),
                        self.memory_processing,
                        self.selected_runtime_session(),
                        window,
                        cx,
                    ))
                    .into_any_element(),
                AssistantPanel::SSH => div()
                    .flex()
                    .min_h_0()
                    .flex_col()
                    .child(ssh_section(
                        &self.state.ssh,
                        self.selected_ssh_profile_id.as_deref(),
                        cx,
                    ))
                    .into_any_element(),
                AssistantPanel::FileManager => div()
                    .flex()
                    .min_h_0()
                    .flex_col()
                    .child(file_section(
                        self.state
                            .selected_project
                            .as_ref()
                            .map(|project| project.name.as_str())
                            .unwrap_or("Project"),
                        &self.state.files,
                        &self.file_tree_children,
                        &self.file_tree_expanded_dirs,
                        &self.file_directory,
                        self.selected_file_entry.as_deref(),
                        self.file_name_draft_kind,
                        &self.file_name_draft_value,
                        window,
                        cx,
                    ))
                    .into_any_element(),
                AssistantPanel::Git => div()
                    .flex()
                    .min_h_0()
                    .flex_col()
                    .child(git_section(
                        &self.state.git,
                        &self.git_expanded_sections,
                        &self.git_expanded_dirs,
                        &self.git_tree_children,
                        self.selected_git_file.as_deref(),
                        self.selected_git_branch.as_deref(),
                        self.state
                            .selected_project
                            .as_ref()
                            .and_then(|project| project.git_default_push_remote_name.as_deref()),
                        &self.git_clone_remote_url,
                        self.git_remote_editor_open,
                        &self.git_remote_name,
                        &self.git_remote_url,
                        self.git_running_operation.as_ref(),
                        &self.git_commit_message,
                        window,
                        cx,
                    ))
                    .into_any_element(),
            })
            .into_any_element()
    }
}

fn assistant_panel_header(
    title: impl Into<SharedString>,
    icon: IconName,
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .child(title),
                ),
        )
        .child(action)
}

fn ai_stats_surface(cx: &mut Context<CoduxApp>) -> gpui::Hsla {
    if cx.theme().is_dark() {
        color(0xFFFFFF).opacity(0.06)
    } else {
        color(0x000000).opacity(0.045)
    }
}

fn ai_stats_track_surface(cx: &mut Context<CoduxApp>) -> gpui::Hsla {
    if cx.theme().is_dark() {
        color(0xFFFFFF).opacity(0.10)
    } else {
        color(0x000000).opacity(0.07)
    }
}
