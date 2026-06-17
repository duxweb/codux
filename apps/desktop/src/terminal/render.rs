impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.process_events(window, cx);
        if let Some(new_display_offset) = self.scroll_handle.take_future_display_offset() {
            self.model.update(cx, |model, _| {
                model.scroll_to_display_offset(new_display_offset);
            });
        }

        self.renderer.measure_cell(window);
        self.ensure_focus_report_subscriptions(window, cx);
        let terminal_focused = self.focus_handle.is_focused(window);
        let cursor_visible = self.marked_text.is_none()
            && window.is_window_active()
            && (!terminal_focused || self.blink_manager.read(cx).visible());
        let element = TerminalElement {
            model: self.model.clone(),
            renderer: self.renderer.clone(),
            layout: self.layout.clone(),
            scroll_handle: self.scroll_handle.clone(),
            session: self.session.clone(),
            focus_handle: self.focus_handle.clone(),
            terminal_view: cx.weak_entity(),
            padding: self.config.padding,
            marked_text: self.marked_text.clone(),
            hover_link: self.hover_link.clone(),
            cursor_visible,
            cursor_focused: terminal_focused,
        };

        let terminal = div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(self.config.colors.background())
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .drag_over::<ExternalPaths>(move |this, _paths, _window, _cx| this)
            .on_drop(cx.listener(Self::drop_external_paths));
        let remote_viewer = self.model.read(cx).remote_viewer();
        let terminal = terminal.child(element).child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(
                    Scrollbar::new(&self.scroll_handle)
                        .id("terminal-scrollbar")
                        .axis(ScrollbarAxis::Vertical)
                        .scrollbar_show(ScrollbarShow::Scrolling),
                ),
        );
        let terminal = if remote_viewer {
            terminal.child(
                div()
                    .absolute()
                    .right(px(10.0))
                    .bottom(px(10.0))
                    .size(px(26.0))
                    .rounded(px(8.0))
                    .bg(self.config.colors.background().opacity(0.86))
                    .border_1()
                    .border_color(self.config.colors.foreground().opacity(0.18))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(HeroIconName::PhoneArrowUpRight)
                            .size_4()
                            .text_color(self.config.colors.foreground().opacity(0.78)),
                    ),
            )
        } else {
            terminal
        };
        if self.hover_link.is_some() {
            terminal.cursor(CursorStyle::PointingHand)
        } else {
            terminal
        }
    }
}
