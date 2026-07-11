use super::*;

#[derive(Clone, PartialEq)]
pub(in crate::app) struct TerminalWorkspaceSnapshot {
    pub(super) loading: bool,
    pub(super) language: String,
    pub(super) layout_key: String,
    pub(super) top_ratios: Vec<f64>,
    pub(super) top_grid: TerminalTopGrid,
    pub(super) split_tree: Option<TerminalSplitNode>,
    pub(super) main_panes: Vec<TerminalPaneViewSnapshot>,
}

impl TerminalWorkspaceSnapshot {
    fn visible_terminal_views(&self) -> Vec<gpui::Entity<TerminalView>> {
        self.main_panes
            .iter()
            .filter_map(|pane| pane.view.clone())
            .collect()
    }

    fn set_terminal_views_visible<C>(&self, visible: bool, cx: &mut C)
    where
        C: AppContext,
    {
        for view in self.visible_terminal_views() {
            view.update(cx, |view, cx| view.set_render_visible(visible, cx));
        }
    }
}

#[derive(Clone)]
pub(super) struct TerminalPaneViewSnapshot {
    pub(super) terminal_id: Option<String>,
    pub(super) view: Option<gpui::Entity<TerminalView>>,
    pub(super) title: String,
    pub(super) subtitle: Option<String>,
    pub(super) search_open: bool,
}

impl PartialEq for TerminalPaneViewSnapshot {
    fn eq(&self, other: &Self) -> bool {
        if self.terminal_id != other.terminal_id
            || self.title != other.title
            || self.subtitle != other.subtitle
            || self.search_open != other.search_open
        {
            return false;
        }
        match (&self.view, &other.view) {
            (Some(left), Some(right)) => left.entity_id() == right.entity_id(),
            (None, None) => true,
            _ => false,
        }
    }
}

pub(in crate::app) struct TerminalWorkspaceView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: TerminalWorkspaceSnapshot,
    // Real size of the terminal workspace, recorded at prepaint. Used to
    // derive panel sizes from the persisted ratios so the first frame
    // after a layout-key switch matches the actual container.
    container_height: Option<Pixels>,
    container_width: Option<Pixels>,
    pub(super) pane_drop_preview: Option<TerminalPaneDropPreview>,
    pub(super) open_split_menu_pane: Option<usize>,
    split_menu_hover_epoch: u64,
}

impl TerminalWorkspaceView {
    pub(super) fn new(
        app_entity: gpui::Entity<CoduxApp>,
        snapshot: TerminalWorkspaceSnapshot,
    ) -> Self {
        Self {
            app_entity,
            snapshot,
            container_height: None,
            container_width: None,
            pane_drop_preview: None,
            open_split_menu_pane: None,
            split_menu_hover_epoch: 0,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: TerminalWorkspaceSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        let next_visible = snapshot
            .visible_terminal_views()
            .into_iter()
            .map(|view| view.entity_id())
            .collect::<std::collections::HashSet<_>>();
        for view in self.snapshot.visible_terminal_views() {
            if !next_visible.contains(&view.entity_id()) {
                view.update(cx, |view, cx| view.set_render_visible(false, cx));
            }
        }
        if self
            .open_split_menu_pane
            .is_some_and(|index| index >= snapshot.main_panes.len())
        {
            self.open_split_menu_pane = None;
        }
        self.snapshot = snapshot;
        cx.notify();
    }

    pub(in crate::app) fn set_render_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.snapshot.set_terminal_views_visible(visible, cx);
    }

    pub(super) fn set_split_menu_open(
        &mut self,
        pane_index: usize,
        open: bool,
        cx: &mut Context<Self>,
    ) {
        let next = open.then_some(pane_index);
        if self.open_split_menu_pane == next {
            if open {
                self.split_menu_hover_epoch = self.split_menu_hover_epoch.wrapping_add(1);
            }
            return;
        }
        self.open_split_menu_pane = next;
        self.split_menu_hover_epoch = self.split_menu_hover_epoch.wrapping_add(1);
        cx.notify();
    }

    pub(super) fn close_split_menu_after_hover_gap(
        &mut self,
        pane_index: usize,
        cx: &mut Context<Self>,
    ) {
        let epoch = self.split_menu_hover_epoch;
        cx.spawn(async move |view: gpui::WeakEntity<Self>, cx| {
            // Grace long enough to cross the trigger→popover gap without the
            // menu vanishing mid-travel.
            cx.background_executor()
                .timer(Duration::from_millis(260))
                .await;
            let _ = view.update(cx, |view, cx| {
                if view.open_split_menu_pane == Some(pane_index)
                    && view.split_menu_hover_epoch == epoch
                {
                    view.open_split_menu_pane = None;
                    view.split_menu_hover_epoch = view.split_menu_hover_epoch.wrapping_add(1);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for TerminalWorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main = terminal_main_split_area(
            TerminalMainSplitInput {
                app_entity: self.app_entity.clone(),
                language: &self.snapshot.language,
                panes: &self.snapshot.main_panes,
                layout_key: &self.snapshot.layout_key,
                top_ratios: &self.snapshot.top_ratios,
                top_grid: &self.snapshot.top_grid,
                split_tree: &self.snapshot.split_tree,
                container_width: self.container_width,
                container_height: self.container_height,
                pane_drop_preview: self.pane_drop_preview,
                open_split_menu_pane: self.open_split_menu_pane,
            },
            cx,
        );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .on_prepaint({
                let view = cx.entity();
                move |bounds, _, cx| {
                    view.update(cx, |view, cx| {
                        let height = bounds.size.height;
                        let width = bounds.size.width;
                        let changed = view
                            .container_height
                            .is_none_or(|recorded| (recorded - height).abs() > px(1.0))
                            || view
                                .container_width
                                .is_none_or(|recorded| (recorded - width).abs() > px(1.0));
                        if changed {
                            view.container_height = Some(height);
                            view.container_width = Some(width);
                            cx.notify();
                        }
                    });
                }
            })
            .child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .min_w_0()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .overflow_hidden()
                    .child(main),
            )
    }
}
