use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(in crate::app) enum UiRegion {
    Root,
    ProjectColumn,
    TaskColumn,
    WorkspaceColumn,
    WorkspaceChrome,
    WorkspaceBody,
    WorkspaceAssistant,
    FileSidebar,
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
        self.record_ui_invalidation(region);
        match region {
            UiRegion::Root => cx.notify(),
            UiRegion::ProjectColumn => {
                if let Some(view) = &self.project_column_view {
                    view.update(cx, |_view, cx| cx.notify());
                }
            }
            UiRegion::TaskColumn => {
                if let Some(view) = &self.task_column_view {
                    view.update(cx, |_view, cx| cx.notify());
                }
            }
            UiRegion::WorkspaceColumn => {
                if let Some(view) = &self.workspace_column_view {
                    view.update(cx, |_view, cx| cx.notify());
                }
            }
            UiRegion::WorkspaceChrome => {
                if let Some(view) = &self.workspace_toolbar_view {
                    view.update(cx, |_view, cx| cx.notify());
                }
            }
            UiRegion::WorkspaceBody => {
                if let Some(view) = &self.workspace_body_view {
                    view.update(cx, |_view, cx| cx.notify());
                }
            }
            UiRegion::WorkspaceAssistant => {
                if let Some(view) = &self.workspace_assistant_view {
                    view.update(cx, |_view, cx| cx.notify());
                }
            }
            UiRegion::FileSidebar => {
                if let Some(view) = &self.file_sidebar_view {
                    view.update(cx, |_view, cx| cx.notify());
                }
            }
            UiRegion::StatusBar => {
                if let Some(view) = &self.status_bar_view {
                    view.update(cx, |_view, cx| cx.notify());
                }
            }
        }
    }

    pub(in crate::app) fn invalidate_workspace(&mut self, cx: &mut Context<Self>) {
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceColumn,
                UiRegion::WorkspaceBody,
                UiRegion::WorkspaceAssistant,
            ],
        );
    }

    pub(in crate::app) fn invalidate_workspace_body(&mut self, cx: &mut Context<Self>) {
        self.invalidate_ui_region(cx, UiRegion::WorkspaceBody);
    }

    pub(in crate::app) fn invalidate_workspace_chrome(&mut self, cx: &mut Context<Self>) {
        self.invalidate_ui(
            cx,
            [
                UiRegion::WorkspaceColumn,
                UiRegion::WorkspaceChrome,
                UiRegion::WorkspaceAssistant,
            ],
        );
    }

    pub(in crate::app) fn invalidate_status_bar(&mut self, cx: &mut Context<Self>) {
        self.invalidate_ui_region(cx, UiRegion::StatusBar);
    }

    pub(in crate::app) fn invalidate_task_column(&mut self, cx: &mut Context<Self>) {
        self.invalidate_ui_region(cx, UiRegion::TaskColumn);
    }

    fn record_ui_invalidation(&mut self, region: UiRegion) {
        if !self.state.settings.developer_hud {
            return;
        }
        let label = region.label();
        *self.ui_invalidation_counts.entry(label).or_insert(0) += 1;
        let now = app_now_seconds();
        if now - self.ui_invalidation_last_report_at < 5.0 {
            return;
        }
        self.ui_invalidation_last_report_at = now;
        let mut counts = self
            .ui_invalidation_counts
            .iter()
            .map(|(region, count)| format!("{region}={count}"))
            .collect::<Vec<_>>();
        counts.sort();
        self.runtime_trace("performance-ui", &format!("invalidations {}", counts.join(" ")));
        self.ui_invalidation_counts.clear();
    }
}

impl UiRegion {
    fn label(self) -> &'static str {
        match self {
            UiRegion::Root => "root",
            UiRegion::ProjectColumn => "project_column",
            UiRegion::TaskColumn => "task_column",
            UiRegion::WorkspaceColumn => "workspace_column",
            UiRegion::WorkspaceChrome => "workspace_chrome",
            UiRegion::WorkspaceBody => "workspace_body",
            UiRegion::WorkspaceAssistant => "workspace_assistant",
            UiRegion::FileSidebar => "file_sidebar",
            UiRegion::StatusBar => "status_bar",
        }
    }
}
