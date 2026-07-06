//! A centered, searchable Quick Pick overlay (VS Code style), built on
//! gpui-component's `List` + `Dialog`. Used by the git menu to pick a branch or
//! remote without deep cascading submenus.

use std::rc::Rc;

use gpui::{
    App, AppContext as _, Context, ParentElement as _, SharedString, Styled as _, Task, Window,
    div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IndexPath, Sizable as _, WindowExt as _, h_flex,
    list::{List, ListDelegate, ListItem, ListState},
};

/// Compact VS Code-like row height.
const ROW_HEIGHT: f32 = 30.0;
/// Search input strip height (Large input + bottom border).
const SEARCH_HEIGHT: f32 = 45.0;
/// Inset around the rows so the selected pill doesn't touch the edges.
const LIST_PADDING: f32 = 8.0;
/// List area grows with content up to this many rows, then scrolls.
const MAX_VISIBLE_ROWS: usize = 12;

/// One selectable row in a [`show_quick_pick`] overlay.
#[derive(Clone)]
pub struct QuickPickItem {
    pub id: SharedString,
    pub icon: Option<Icon>,
    pub label: SharedString,
    pub description: Option<SharedString>,
}

impl QuickPickItem {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon: None,
            label: label.into(),
            description: None,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Dim secondary text rendered after the label (VS Code "description").
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

type OnConfirm = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

struct QuickPickDelegate {
    all: Vec<QuickPickItem>,
    filtered: Vec<QuickPickItem>,
    selected: Option<IndexPath>,
    on_confirm: OnConfirm,
}

impl ListDelegate for QuickPickDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.filtered.len()
    }

    fn perform_search(
        &mut self,
        query: &str,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        let needle = query.trim().to_lowercase();
        self.filtered = if needle.is_empty() {
            self.all.clone()
        } else {
            self.all
                .iter()
                .filter(|item| item.label.to_lowercase().contains(&needle))
                .cloned()
                .collect()
        };
        self.selected = Some(IndexPath::default());
        Task::ready(())
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let item = self.filtered.get(ix.row)?;
        Some(
            ListItem::new(ix)
                .py_0()
                .h(px(ROW_HEIGHT))
                .rounded(px(6.))
                .text_sm()
                .child(
                    h_flex()
                        .gap_2()
                        .min_w_0()
                        .when_some(item.icon.clone(), |this, icon| {
                            this.child(icon.size_4().text_color(cx.theme().muted_foreground))
                        })
                        .child(div().truncate().child(item.label.clone()))
                        .when_some(item.description.clone(), |this, description| {
                            this.child(
                                div()
                                    .text_xs()
                                    .truncate()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(description),
                            )
                        }),
                ),
        )
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl gpui::IntoElement {
        // A single quiet row instead of the default oversized inbox icon.
        h_flex()
            .h(px(ROW_HEIGHT))
            .justify_center()
            .text_sm()
            .text_color(cx.theme().muted_foreground.opacity(0.6))
            .child("· · ·")
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix;
        cx.notify();
    }

    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        let Some(ix) = self.selected else { return };
        let Some(item) = self.filtered.get(ix.row) else {
            return;
        };
        let id = item.id.clone();
        // Close first: the callback may open a follow-up overlay (chained picker
        // or Quick Input) that must stay on top of the dialog stack.
        window.close_dialog(cx);
        (self.on_confirm.clone())(id, window, cx);
    }

    // Escape is handled by the hosting Dialog (List::Cancel re-propagates to it).
}

/// Show a centered, searchable Quick Pick overlay with a title strip.
/// `on_confirm` receives the chosen item's `id`; the overlay dismisses on
/// Enter/click (before the callback, so chained overlays stack correctly),
/// Escape, or click-outside. Requires the window root to render
/// `Root::render_dialog_layer` (see `app_render`).
pub fn show_quick_pick(
    title: impl Into<SharedString>,
    items: Vec<QuickPickItem>,
    on_confirm: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    // The title doubles as the search placeholder (no separate title strip).
    let placeholder = title.into();
    let on_confirm: OnConfirm = Rc::new(on_confirm);
    let state = cx.new(|cx| {
        ListState::new(
            QuickPickDelegate {
                filtered: items.clone(),
                all: items,
                selected: Some(IndexPath::default()),
                on_confirm,
            },
            window,
            cx,
        )
        .searchable(true)
    });

    let list = state.clone();
    window.open_dialog(cx, move |dialog, _window, cx| {
        // Height tracks the filtered result count, VS Code style.
        let count = list.read(cx).delegate().filtered.len();
        let rows = count.clamp(1, MAX_VISIBLE_ROWS);
        // The empty row renders outside the padded rows area.
        let inset = if count == 0 { 0.0 } else { LIST_PADDING * 2.0 };
        let list_height = px(SEARCH_HEIGHT + inset + rows as f32 * ROW_HEIGHT);
        dialog
            .close_button(false)
            .w(px(560.))
            .p_0()
            .gap_0()
            .min_h(px(0.))
            .child(
                div().h(list_height).w_full().child(
                    List::new(&list)
                        .large()
                        .search_placeholder(placeholder.clone())
                        .p(px(LIST_PADDING)),
                ),
            )
    });

    // `open_dialog` focuses the dialog handle; move focus into the search input
    // so typing + Up/Down + Enter work immediately.
    state.update(cx, |list, cx| list.focus(window, cx));
}
