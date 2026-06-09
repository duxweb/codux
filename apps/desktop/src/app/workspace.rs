use super::*;

impl CoduxApp {
    pub(in crate::app) fn workspace_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .flex_basis(px(0.0))
            .w_full()
            .h_full()
            .min_w_0()
            .min_h_0()
            .child(match self.workspace_view {
                WorkspaceView::Terminal => self.terminal_workspace_body(cx).into_any_element(),
                WorkspaceView::Files => self.files_workspace_body(window, cx).into_any_element(),
                WorkspaceView::Review => div()
                    .flex_1()
                    .size_full()
                    .bg(color(theme::BG_TERMINAL))
                    .into_any_element(),
            })
    }
}
