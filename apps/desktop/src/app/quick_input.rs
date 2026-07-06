//! A centered Quick Input overlay (VS Code style): a title strip and a single
//! text field in a small top dialog. Enter confirms, Escape / click-outside
//! cancels. Used by the git menu for branch names, tag names, stash messages.

use std::rc::Rc;

use gpui::{
    App, AppContext as _, Context, Entity, ParentElement as _, Render, SharedString, Styled as _,
    Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _, WindowExt as _,
    input::{Input, InputState, SelectAll},
    v_flex,
};

type OnConfirm = Rc<dyn Fn(String, &mut Window, &mut App)>;

struct QuickInputView {
    title: SharedString,
    input: Entity<InputState>,
}

impl Render for QuickInputView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        v_flex()
            .w_full()
            .child(
                // Same title strip as the Quick Pick overlay.
                div()
                    .h(px(30.))
                    .px_3()
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .h(px(52.))
                    .px_3()
                    .flex()
                    .items_center()
                    .child(Input::new(&self.input).large().p_0().appearance(false)),
            )
    }
}

/// Show a centered Quick Input overlay. `on_confirm` receives the trimmed
/// text on Enter (empty only when `allow_empty`); the overlay then dismisses.
/// Enter is consumed by the Dialog layer (`ConfirmDialog`), so confirmation is
/// wired through the dialog's `on_ok` — not the input's own Enter event.
/// Requires the window root to render `Root::render_dialog_layer` (see
/// `app_render`).
pub fn show_quick_input(
    title: impl Into<SharedString>,
    placeholder: impl Into<SharedString>,
    initial_value: impl Into<SharedString>,
    allow_empty: bool,
    on_confirm: impl Fn(String, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let title = title.into();
    let placeholder = placeholder.into();
    let initial_value = initial_value.into();
    let on_confirm: OnConfirm = Rc::new(on_confirm);

    let input = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(initial_value)
    });
    let view = cx.new(|_| QuickInputView {
        title,
        input: input.clone(),
    });

    let dialog_view = view.clone();
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let input = input.clone();
        let on_confirm = on_confirm.clone();
        dialog
            .close_button(false)
            .w(px(560.))
            .p_0()
            .gap_0()
            .min_h(px(0.))
            .on_ok(move |_, window, cx| {
                let value = input.read(cx).value().trim().to_string();
                if value.is_empty() && !allow_empty {
                    return false;
                }
                // Close ourselves first: the callback may open a follow-up
                // overlay that must stay on top of the dialog stack.
                window.close_dialog(cx);
                (on_confirm)(value, window, cx);
                false
            })
            .child(dialog_view.clone())
    });

    // Focus the field and preselect the prefill so typing replaces it.
    view.update(cx, |view, cx| {
        view.input.update(cx, |input, cx| input.focus(window, cx));
    });
    window.dispatch_action(Box::new(SelectAll), cx);
}
