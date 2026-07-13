use super::*;
use crate::app::ui_helpers::codux_tooltip_container;

impl CoduxApp {
    /// Overlay descriptor for the terminal when the selected project's remote
    /// host link is not usable: `(icon, tint, message)`. `None` for a local
    /// project or a healthy connected link.
    fn selected_project_terminal_link_overlay(&self) -> Option<(HeroIconName, u32, String)> {
        let host = self
            .state
            .selected_project
            .as_ref()
            .and_then(|project| project.remote_device_id())?;
        match self.remote_link_states.get(host).copied() {
            Some(codux_runtime::remote::ControllerLinkState::Disconnected) => Some((
                HeroIconName::LinkSlash,
                theme::RED,
                "远程主机已离线 · 正在自动重连…".to_string(),
            )),
            Some(codux_runtime::remote::ControllerLinkState::Connecting) => Some((
                HeroIconName::Link,
                theme::ORANGE,
                "正在连接远程主机…".to_string(),
            )),
            _ => None,
        }
    }

    pub(in crate::app) fn terminal_workspace_body(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .min_w_0()
                    .min_h_0()
                    .w_full()
                    .child(self.terminal_main_split_area(cx)),
            )
    }

    fn terminal_main_split_area(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(self.terminal_panes(cx))
    }

    pub(in crate::app) fn terminal_panes(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(active) = self.main_terminal() else {
            return div().flex_1().size_full();
        };
        let pane_count = active.panes.len();
        let link_overlay = self.selected_project_terminal_link_overlay();
        div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .when_some(link_overlay, |this, overlay| {
                this.child(terminal_link_overlay(overlay))
            })
            .children(active.panes.iter().enumerate().map(|(index, slot)| {
                let close_id = SharedString::from(format!("terminal-pane-close-{index}"));
                let float_id = SharedString::from(format!("terminal-pane-float-{index}"));
                let add_id = SharedString::from(format!("terminal-pane-add-{index}"));
                div()
                    .relative()
                    .group("terminal-pane")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .border_l_1()
                    .border_color(color(if index == 0 {
                        theme::BG_TERMINAL
                    } else {
                        theme::BORDER_SOFT
                    }))
                    .child(
                        div().flex_1().min_w_0().child(match &slot.pane {
                            Some(pane) => gpui::AnyView::from(pane.view.clone())
                                .cached(gpui::StyleRefinement::default().flex().size_full())
                                .into_any_element(),
                            None => div()
                                .id(SharedString::from(format!("terminal-pane-mount-{index}")))
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .bg(theme::terminal_fill(color(theme::BG_TERMINAL)))
                                .text_color(color(theme::TEXT_DIM))
                                .on_click(cx.listener(move |app, _event, window, cx| {
                                    app.select_terminal_pane(index, window, cx);
                                }))
                                .child("Click to open terminal")
                                .into_any_element(),
                        }),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_2()
                            .right_2()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(terminal_pane_control_button(
                                float_id,
                                HeroIconName::ArrowTopRightOnSquare,
                                "浮窗",
                                pane_count > 1,
                                cx,
                                move |app, _event, window, cx| {
                                    app.float_terminal_pane(index, window, cx)
                                },
                            ))
                            .child(terminal_pane_control_button(
                                add_id,
                                HeroIconName::Plus,
                                "新建分屏",
                                true,
                                cx,
                                |app, _event, window, cx| app.split_terminal(window, cx),
                            ))
                            .child(terminal_pane_control_button(
                                close_id,
                                HeroIconName::XMark,
                                "关闭分屏",
                                pane_count > 1,
                                cx,
                                move |app, _event, window, cx| {
                                    app.close_terminal_pane(index, window, cx)
                                },
                            )),
                    )
                    .into_any_element()
            }))
    }
}

/// A centered banner over the terminal area when the remote host link is down
/// or reconnecting, so a frozen remote shell reads as "offline, recovering"
/// instead of an unexplained blank pane.
fn terminal_link_overlay(overlay: (HeroIconName, u32, String)) -> impl IntoElement {
    let (icon, tint, message) = overlay;
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .flex()
        .items_center()
        .justify_center()
        .py_2()
        .gap_2()
        .bg(color(theme::BG_HEADER))
        .border_b_1()
        .border_color(color(theme::BORDER_SOFT))
        .child(Icon::new(icon).size_4().text_color(color(tint)))
        .child(
            div()
                .text_size(rems(0.8125))
                .text_color(color(theme::TEXT))
                .child(message),
        )
}

fn terminal_pane_control_button(
    id: SharedString,
    icon: HeroIconName,
    tooltip: &'static str,
    enabled: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let text_color = if enabled {
        cx.theme().secondary_foreground
    } else {
        color(theme::TEXT_DIM)
    };
    let button = codux_tooltip_container(cx.entity(), id, tooltip)
        .size(px(28.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_sm()
        .text_color(text_color)
        .child(Icon::new(icon).size_3p5().text_color(text_color));

    if enabled {
        button
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().secondary_hover))
            .on_click(cx.listener(move |app, event, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                on_click(app, event, window, cx);
            }))
            .into_any_element()
    } else {
        button.opacity(0.45).into_any_element()
    }
}
