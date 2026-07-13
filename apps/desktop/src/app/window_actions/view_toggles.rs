use super::*;

impl CoduxApp {
    pub(in crate::app) fn toggle_project_column(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_column_collapsed = !self.project_column_collapsed;
        self.invalidate_project_shell(cx);
    }

    pub(in crate::app) fn toggle_task_column(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.task_column_collapsed = !self.task_column_collapsed;
        self.invalidate_project_shell(cx);
    }

    pub(in crate::app) fn close_active_workspace_item(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected_project_missing = self
            .state
            .selected_project
            .as_ref()
            .is_some_and(|project| self.runtime_service.project_root_missing(&project.id));
        if selected_project_missing {
            self.close_selected_project(window, cx);
            return;
        }

        match self.workspace_view {
            WorkspaceView::Terminal => {
                // In split mode, if the file editor holds focus, Cmd+W closes the
                // active file tab (and the split collapses once the last one is
                // gone) instead of closing the terminal.
                let close_split_file = self.workspace_split == Some(WorkspaceSplitKind::FileEditor)
                    && self.active_file_editor_tab.is_some()
                    && self.active_file_editor_split_focused(window, cx);
                if close_split_file {
                    if let Some(relative_path) = self.active_file_editor_tab.clone() {
                        self.close_file_editor_tab(relative_path, window, cx);
                        self.status_message = "file tab closed".to_string();
                    }
                } else {
                    self.confirm_or_close_active_terminal_target(window, cx);
                }
            }
            WorkspaceView::Files => {
                if let Some(relative_path) = self.active_file_editor_tab.clone() {
                    self.close_file_editor_tab(relative_path, window, cx);
                    self.status_message = "file tab closed".to_string();
                } else if self.selected_file_entry.take().is_some() {
                    self.file_preview = "select a file to preview it".to_string();
                    self.file_editable = false;
                    self.file_dirty = false;
                    self.status_message = "file preview closed".to_string();
                } else {
                    self.status_message = "no active file preview".to_string();
                }
                self.invalidate_file_panel(cx);
            }
            WorkspaceView::Review => {
                self.status_message = "no active review item to close".to_string();
                self.invalidate_git_panel(cx);
            }
            WorkspaceView::Stats => {
                self.set_workspace_view(WorkspaceView::Terminal, window, cx);
            }
        }
    }

    pub(in crate::app) fn set_workspace_view(
        &mut self,
        view: WorkspaceView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_view = view;
        match view {
            WorkspaceView::Files => {
                self.assistant_panel = Some(AssistantPanel::FileManager);
                self.refresh_files_panel_state_async(cx);
            }
            WorkspaceView::Review => {
                self.assistant_panel = None;
                self.refresh_git_panel_state_async(cx);
                self.ensure_selected_git_review_file_loaded_async(cx);
            }
            WorkspaceView::Stats => {
                self.assistant_panel = None;
                self.refresh_stats_workspace_data(false, cx);
            }
            WorkspaceView::Terminal => {
                self.activate_first_terminal();
                if let Err(error) = self.ensure_active_terminal_mounted(cx) {
                    self.status_message = format!("failed to focus terminal: {error}");
                } else {
                    self.focus_active_terminal(window, cx);
                }
            }
        }
        self.invalidate_workspace(cx);
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn show_stats_workspace_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view == WorkspaceView::Stats {
            self.set_workspace_view(WorkspaceView::Terminal, window, cx);
        } else {
            self.set_workspace_view(WorkspaceView::Stats, window, cx);
        }
    }

    pub(in crate::app) fn refresh_stats_workspace_data(
        &mut self,
        show_progress: bool,
        cx: &mut Context<Self>,
    ) {
        self.refresh_ai_global_history_summary(cx);
        if self.state.selected_project.is_some() {
            self.start_ai_history_refresh(show_progress, cx);
        } else {
            self.invalidate_ui(cx, [UiRegion::WorkspaceBody, UiRegion::StatusBar]);
        }
    }

    pub(in crate::app) fn set_stats_time_range(
        &mut self,
        range: StatsTimeRange,
        cx: &mut Context<Self>,
    ) {
        if self.stats_time_range == range {
            return;
        }
        self.stats_time_range = range;
        self.invalidate_ui(cx, [UiRegion::WorkspaceBody]);
    }

    pub(in crate::app) fn set_settings_pane(&mut self, pane: SettingsPane, cx: &mut Context<Self>) {
        self.active_settings_pane = pane;
        if pane == SettingsPane::Wsl {
            self.load_wsl_distribution_catalog(cx);
        }
        self.invalidate_ui_region(cx, UiRegion::Root);
    }

    pub(in crate::app) fn toggle_assistant_panel(
        &mut self,
        panel: AssistantPanel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.assistant_panel = if self.assistant_panel == Some(panel) {
            None
        } else {
            Some(panel)
        };
        if self.assistant_panel == Some(panel) {
            self.refresh_assistant_panel_state(panel, cx);
        }
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceAssistant,
                UiRegion::AIStatsSidebar,
                UiRegion::ServerInfoSidebar,
                UiRegion::SshSidebar,
                UiRegion::DbSidebar,
                UiRegion::FileSidebar,
                UiRegion::GitSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(in crate::app) fn refresh_assistant_panel_state(
        &mut self,
        panel: AssistantPanel,
        cx: &mut Context<Self>,
    ) {
        match panel {
            AssistantPanel::AIStats => {
                self.refresh_ai_stats_panel_async(cx);
            }
            AssistantPanel::ServerInfo => {
                self.refresh_server_info_panel(cx);
            }
            AssistantPanel::Ssh => {
                self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
                self.normalize_selected_ssh_profile();
            }
            AssistantPanel::DB => {
                self.reload_selected_project_db();
                self.normalize_selected_db_profile();
            }
            AssistantPanel::FileManager => {
                self.refresh_files_panel_state_async(cx);
            }
            AssistantPanel::Git => {
                self.refresh_git_panel_state_async(cx);
            }
        }
    }
}
