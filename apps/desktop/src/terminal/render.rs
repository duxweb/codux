impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.set_render_visible(true, cx);
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
            .bg(crate::theme::terminal_fill(self.config.colors.background()))
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
        // Whether a REMOTE device currently owns this session. Read the live
        // viewport lease (not model.remote_viewer): that cache only updates on an
        // owner-CHANGE event, so switching to a session a remote already owns
        // (no change → no event) would leave it stale and we'd wrongly render the
        // live terminal instead of the "taken over" placeholder.
        let remote_viewer = !self.session.local_viewport_owns();
        let terminal = if remote_viewer {
            // Handed off to a remote device (phone/pad). We deliberately do NOT
            // mirror its (differently-sized) grid into this pane -- that is the
            // resize mess. Show a placeholder with actions instead: "Take over"
            // reclaims ownership (reflowing the PTY back to this desktop's grid);
            // "Preview" (next step) will open a read-only child window at the
            // remote's own 1:1 size that never sends a resize, so it cannot
            // disturb the remote owner.
            let fg = self.config.colors.foreground();
            let bg = self.config.colors.background();
            let accent = cx.theme().primary;
            let accent_fg = cx.theme().primary_foreground;
            let lang = self.config.language.clone();
            // Infer the controlling device type from its grid aspect ratio
            // (portrait → phone, landscape → tablet). A friendly device NAME
            // needs a device_id→name pipe from the host; for now the type stands
            // in for it.
            let (model_cols, model_rows) = self.model.read(cx).dimensions();
            let portrait = model_rows >= model_cols;
            let device_icon = if portrait {
                HeroIconName::DevicePhoneMobile
            } else {
                HeroIconName::DeviceTablet
            };
            let device_label = self.session.viewport_owner_label().unwrap_or_else(|| {
                if portrait {
                    codux_runtime::i18n::translate(&lang, "terminal.handoff.phone", "Phone")
                } else {
                    codux_runtime::i18n::translate(&lang, "terminal.handoff.tablet", "Tablet")
                }
            });
            terminal.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .bg(bg.opacity(0.98))
                    .child(
                        div()
                            .size(px(72.0))
                            .rounded(px(36.0))
                            .bg(fg.opacity(0.04))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(device_icon)
                                    .size_8()
                                    .text_color(fg.opacity(0.5)),
                            ),
                    )
                    .child(div().h(px(18.0)))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(fg.opacity(0.88))
                            .child(device_label.to_string()),
                    )
                    .child(div().h(px(4.0)))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(fg.opacity(0.45))
                            .child(codux_runtime::i18n::translate(
                                &lang,
                                "terminal.handoff.inUse",
                                "is using this terminal",
                            )),
                    )
                    .child(div().h(px(24.0)))
                    .child(
                        div()
                            .id("terminal-take-over")
                            .px(px(16.0))
                            .py(px(6.0))
                            .rounded(px(999.0))
                            .bg(accent)
                            .cursor_pointer()
                            .hover(|style| style.opacity(0.88))
                            .on_click(cx.listener(|view, _event, _window, cx| {
                                if let Err(error) = view.session.restore_local_viewport() {
                                    eprintln!("failed to reclaim terminal viewport: {error}");
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(accent_fg)
                                    .child(codux_runtime::i18n::translate(
                                        &lang,
                                        "terminal.handoff.takeOver",
                                        "Take over",
                                    )),
                            ),
                    ),
            )
        } else {
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
            if self.search_open {
                terminal.child(self.render_search_bar(cx))
            } else {
                terminal
            }
        };
        if self.hover_link.is_some() {
            terminal.cursor(CursorStyle::PointingHand)
        } else {
            terminal
        }
    }
}
