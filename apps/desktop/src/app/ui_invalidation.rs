use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(in crate::app) enum UiRegion {
    Root,
    ProjectColumn,
    TaskColumn,
    WorkspaceChrome,
    WorkspaceBody,
    WorkspaceAssistant,
    AIStatsSidebar,
    SshSidebar,
    FileSidebar,
    GitSidebar,
    StatusBar,
}

impl CoduxApp {
    pub(in crate::app) fn invalidate_ui(
        &mut self,
        cx: &mut Context<Self>,
        regions: impl IntoIterator<Item = UiRegion>,
    ) {
        for region in regions {
            self.invalidate_ui_region(cx, region);
        }
    }

    pub(in crate::app) fn invalidate_ui_region(
        &mut self,
        cx: &mut Context<Self>,
        region: UiRegion,
    ) {
        match region {
            UiRegion::StatusBar => {
                if let Some(view) = &self.status_bar_view {
                    let snapshot = self.status_bar_snapshot();
                    let changed = view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
                    if changed {
                        self.record_ui_performance_event("invalidate", region.label());
                    }
                }
            }
            UiRegion::Root => {
                self.record_ui_performance_event("invalidate", region.label());
                cx.notify();
            }
            UiRegion::ProjectColumn => {
                self.record_ui_performance_event("invalidate", region.label());
                if self.project_column_view.is_some() {
                    let _ = self.project_column_view(cx);
                }
            }
            UiRegion::TaskColumn => {
                self.record_ui_performance_event("invalidate", region.label());
                if self.task_column_view.is_some() {
                    self.update_task_column_child_views(cx);
                }
            }
            UiRegion::WorkspaceChrome => {
                self.record_ui_performance_event("invalidate", region.label());
                if let Some(view) = &self.workspace_toolbar_view {
                    let snapshot = self.workspace_toolbar_snapshot();
                    view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
                }
            }
            UiRegion::WorkspaceBody => {
                self.record_ui_performance_event("invalidate", region.label());
                if self.workspace_view == WorkspaceView::Terminal {
                    self.update_terminal_workspace_view(cx);
                } else if self.workspace_view == WorkspaceView::Files {
                    if !self.update_file_editor_workspace_view(cx) {
                        if let Some(view) = &self.workspace_body_view {
                            view.update(cx, |_view, cx| cx.notify());
                        }
                    }
                } else if self.workspace_view == WorkspaceView::Review {
                    if !self.update_review_workspace_view(cx) {
                        if let Some(view) = &self.workspace_body_view {
                            view.update(cx, |_view, cx| cx.notify());
                        }
                    }
                } else if let Some(view) = &self.workspace_body_view {
                    view.update(cx, |_view, cx| cx.notify());
                }
            }
            UiRegion::WorkspaceAssistant => {
                self.record_ui_performance_event("invalidate", region.label());
                if let Some(view) = &self.workspace_assistant_view {
                    let snapshot = self.workspace_assistant_snapshot();
                    view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
                }
            }
            UiRegion::AIStatsSidebar => {
                self.record_ui_performance_event("invalidate", region.label());
                if self.ai_stats_sidebar_view.is_some() {
                    let _ = self.ai_stats_sidebar_view(cx);
                }
            }
            UiRegion::SshSidebar => {
                self.record_ui_performance_event("invalidate", region.label());
                if self.ssh_sidebar_view.is_some() {
                    let _ = self.ssh_sidebar_view(cx);
                }
            }
            UiRegion::FileSidebar => {
                self.record_ui_performance_event("invalidate", region.label());
                if self.file_sidebar_view.is_some() {
                    let _ = self.file_sidebar_view(cx);
                }
            }
            UiRegion::GitSidebar => {
                self.record_ui_performance_event("invalidate", region.label());
                if self.git_sidebar_view.is_some() {
                    let _ = self.git_sidebar_view(cx);
                }
                if self.git_files_panel_view.is_some() {
                    let _ = self.git_files_panel_view(cx);
                }
                if self.git_history_panel_view.is_some() {
                    let _ = self.git_history_panel_view(cx);
                }
            }
        }
    }

    pub(in crate::app) fn invalidate_workspace(&mut self, cx: &mut Context<Self>) {
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceBody,
                UiRegion::WorkspaceAssistant,
            ],
        );
    }

    pub(in crate::app) fn invalidate_project_context(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        self.invalidate_ui(
            cx,
            [
                UiRegion::ProjectColumn,
                UiRegion::TaskColumn,
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceBody,
                UiRegion::WorkspaceAssistant,
                UiRegion::AIStatsSidebar,
                UiRegion::FileSidebar,
                UiRegion::GitSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(in crate::app) fn invalidate_worktree_context(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        self.invalidate_ui(
            cx,
            [
                UiRegion::TaskColumn,
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceBody,
                UiRegion::WorkspaceAssistant,
                UiRegion::FileSidebar,
                UiRegion::GitSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(in crate::app) fn invalidate_status_bar(&mut self, cx: &mut Context<Self>) {
        self.invalidate_ui_region(cx, UiRegion::StatusBar);
    }

    pub(in crate::app) fn invalidate_task_column(&mut self, cx: &mut Context<Self>) {
        self.invalidate_ui_region(cx, UiRegion::TaskColumn);
    }

    pub(in crate::app) fn invalidate_project_shell(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        self.invalidate_ui(
            cx,
            [
                UiRegion::ProjectColumn,
                UiRegion::TaskColumn,
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceBody,
                UiRegion::WorkspaceAssistant,
                UiRegion::AIStatsSidebar,
                UiRegion::SshSidebar,
                UiRegion::FileSidebar,
                UiRegion::GitSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(in crate::app) fn invalidate_terminal_workspace(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        self.invalidate_ui(cx, [UiRegion::WorkspaceBody, UiRegion::StatusBar]);
    }

    pub(in crate::app) fn invalidate_file_panel(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        self.invalidate_ui(
            cx,
            [
                UiRegion::FileSidebar,
                UiRegion::WorkspaceBody,
                UiRegion::WorkspaceAssistant,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(in crate::app) fn invalidate_git_panel(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceAssistant,
                UiRegion::GitSidebar,
                UiRegion::WorkspaceBody,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(in crate::app) fn invalidate_memory_panel(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceAssistant,
                UiRegion::AIStatsSidebar,
                UiRegion::WorkspaceChrome,
                UiRegion::TaskColumn,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(in crate::app) fn invalidate_remote_panel(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceAssistant,
                UiRegion::SshSidebar,
                UiRegion::TaskColumn,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(in crate::app) fn invalidate_project_management(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            self.invalidate_ui_region(cx, UiRegion::Root);
            return;
        }
        self.invalidate_ui(
            cx,
            [
                UiRegion::ProjectColumn,
                UiRegion::TaskColumn,
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceAssistant,
                UiRegion::AIStatsSidebar,
                UiRegion::SshSidebar,
                UiRegion::GitSidebar,
                UiRegion::StatusBar,
            ],
        );
    }

    pub(in crate::app) fn record_ui_state_clear(&mut self, state: &'static str) {
        self.record_ui_performance_event("state_clear", state);
    }

    pub(in crate::app) fn record_ui_performance_dynamic_event(&mut self, kind: &str, name: &str) {
        if !self.state.settings.developer_hud {
            return;
        }
        self.record_ui_performance_label(format!("{kind}.{name}"));
    }

    fn record_ui_performance_event(&mut self, kind: &'static str, name: &'static str) {
        if !self.state.settings.developer_hud {
            return;
        }
        self.record_ui_performance_label(format!("{kind}.{name}"));
    }

    fn record_ui_performance_label(&mut self, label: String) {
        let now = app_now_seconds();
        *self.ui_performance_counts.entry(label).or_insert(0) += 1;
        if now - self.ui_performance_last_report_at < 5.0 {
            return;
        }
        self.ui_performance_last_report_at = now;
        let mut counts = self
            .ui_performance_counts
            .iter()
            .map(|(region, count)| format!("{region}={count}"))
            .collect::<Vec<_>>();
        counts.sort();
        self.runtime_trace("performance-ui", &format!("events {}", counts.join(" ")));
        self.ui_performance_counts.clear();
    }
}

impl UiRegion {
    fn label(self) -> &'static str {
        match self {
            UiRegion::Root => "root",
            UiRegion::ProjectColumn => "project_column",
            UiRegion::TaskColumn => "task_column",
            UiRegion::WorkspaceChrome => "workspace_chrome",
            UiRegion::WorkspaceBody => "workspace_body",
            UiRegion::WorkspaceAssistant => "workspace_assistant",
            UiRegion::AIStatsSidebar => "ai_stats_sidebar",
            UiRegion::SshSidebar => "ssh_sidebar",
            UiRegion::FileSidebar => "file_sidebar",
            UiRegion::GitSidebar => "git_sidebar",
            UiRegion::StatusBar => "status_bar",
        }
    }
}
