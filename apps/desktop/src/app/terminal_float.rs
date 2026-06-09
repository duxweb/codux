use super::*;

pub(in crate::app) fn terminal_float_window(
    title: String,
    app_entity: gpui::Entity<CoduxApp>,
    project_id: Option<String>,
    tab_view_id: usize,
    pane_index: usize,
    slot: TerminalPaneSlot,
    cx: &mut Context<CoduxApp>,
) -> gpui::Entity<TerminalFloatWindow> {
    cx.new(|_| TerminalFloatWindow {
        title,
        restore: Some(TerminalFloatRestore {
            app_entity,
            project_id,
            tab_view_id,
            pane_index,
            slot,
        }),
    })
}

pub(in crate::app) struct TerminalFloatWindow {
    title: String,
    restore: Option<TerminalFloatRestore>,
}

struct TerminalFloatRestore {
    app_entity: gpui::Entity<CoduxApp>,
    project_id: Option<String>,
    tab_view_id: usize,
    pane_index: usize,
    slot: TerminalPaneSlot,
}

impl TerminalFloatWindow {
    pub(in crate::app) fn restore_to_parent(&mut self, cx: &mut Context<Self>) {
        let Some(restore) = self.restore.take() else {
            return;
        };
        restore.app_entity.update(cx, |app, cx| {
            app.restore_floated_terminal_slot(
                restore.project_id,
                restore.tab_view_id,
                restore.pane_index,
                restore.slot,
                cx,
            )
        });
    }
}

impl Render for TerminalFloatWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        child_window_shell(self.title.clone(), cx).child(
            div()
                .flex_1()
                .min_h_0()
                .bg(color(theme::BG_TERMINAL))
                .child(
                    match self
                        .restore
                        .as_ref()
                        .and_then(|restore| restore.slot.pane.as_ref())
                    {
                        Some(pane) => gpui::AnyView::from(pane.view.clone())
                            .cached(gpui::StyleRefinement::default().flex().size_full())
                            .into_any_element(),
                        None => div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(cx.theme().muted_foreground)
                            .child("Terminal mounting...")
                            .into_any_element(),
                    },
                ),
        )
    }
}
