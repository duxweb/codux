use super::*;

pub(in crate::app) fn child_window_shell<T>(
    title: impl Into<SharedString>,
    cx: &mut Context<T>,
) -> gpui::Div {
    let title = title.into();
    let title_row = div()
        .h(px(48.0))
        .flex_shrink_0()
        .pr(px(12.0))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().title_bar)
        .when(cfg!(target_os = "macos"), |this| this.pl(px(54.0)))
        .when(!cfg!(target_os = "macos"), |this| this.pl(px(18.0)))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .window_control_area(WindowControlArea::Drag)
                .truncate()
                .text_size(rems(0.875))
                .line_height(rems(0.875))
                .child(title),
        )
        .when(!cfg!(target_os = "macos"), |this| {
            this.child(
                Button::new("child-window-close")
                    .compact()
                    .ghost()
                    .h(px(28.0))
                    .w(px(28.0))
                    .text_color(cx.theme().muted_foreground)
                    .window_control_area(WindowControlArea::Close)
                    .on_click(|_, window, _| window.remove_window())
                    .child(Icon::new(HeroIconName::XMark).size_3()),
            )
        });

    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(cx.theme().background)
        .text_color(cx.theme().foreground)
        .child(title_row)
}
