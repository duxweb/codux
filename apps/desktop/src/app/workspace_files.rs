use super::*;

impl CoduxApp {
    pub(in crate::app) fn files_workspace_body(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let snapshot = self.file_editor_workspace_snapshot();
        let app_entity = cx.entity();
        div()
            .flex()
            .flex_1()
            .bg(color(theme::BG_TERMINAL))
            .child(cx.new(|_| file_editor::FileEditorWorkspaceView::new(app_entity, snapshot)))
    }
}
