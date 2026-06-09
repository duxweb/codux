use super::*;
use crate::app::ui_helpers::codux_tooltip_container;
use gpui_component::resizable::{resizable_panel, v_resizable};

const TERMINAL_BOTTOM_TAB_BAR_HEIGHT: Pixels = px(40.0);
const TERMINAL_BOTTOM_PANEL_MIN_SIZE: Pixels = px(128.0);

impl CoduxApp {
    pub(in crate::app) fn terminal_workspace_body(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let has_bottom_tabs = self.bottom_terminals().next().is_some();
        if !has_bottom_tabs {
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .flex_basis(px(0.0))
                .min_w_0()
                .min_h_0()
                .w_full()
                .h_full()
                .bg(color(theme::BG_TERMINAL))
                .child(
                    div()
                        .flex_1()
                        .flex_basis(px(0.0))
                        .min_w_0()
                        .min_h_0()
                        .w_full()
                        .child(self.terminal_main_split_area(cx)),
                )
                .child(
                    div()
                        .h(TERMINAL_BOTTOM_TAB_BAR_HEIGHT)
                        .child(self.terminal_bottom_tabs_area(cx)),
                );
        }

        div()
            .flex()
            .flex_col()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .bg(color(theme::BG_TERMINAL))
            .child(
                v_resizable("workspace-terminal-split")
                    .child(
                        resizable_panel()
                            .size(px(420.0))
                            .size_range(px(220.0)..px(900.0))
                            .child(self.terminal_main_split_area(cx)),
                    )
                    .child(
                        resizable_panel()
                            .size(px(220.0))
                            .size_range(TERMINAL_BOTTOM_PANEL_MIN_SIZE..px(520.0))
                            .child(self.terminal_bottom_tabs_area(cx)),
                    ),
            )
    }

    fn terminal_main_split_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(self.terminal_panes(cx))
    }

    fn terminal_bottom_tabs_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_bottom_terminal();
        let has_bottom_tabs = active.is_some();

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .h(TERMINAL_BOTTOM_TAB_BAR_HEIGHT)
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .when(!has_bottom_tabs, |this| {
                                this.child(
                                    div()
                                        .px_2()
                                        .text_size(rems(0.75))
                                        .line_height(rems(1.0))
                                        .text_color(cx.theme().secondary_foreground)
                                        .child("终端"),
                                )
                            })
                            .children(self.bottom_terminals().map(|terminal| {
                                terminal_bottom_tab_button(
                                    terminal.id,
                                    terminal.label.clone(),
                                    terminal.id == self.active_terminal_id,
                                    cx,
                                )
                                .into_any_element()
                            })),
                    )
                    .child(terminal_bottom_add_button(cx)),
            )
            .when_some(active, |this, tab| {
                this.child(
                    div()
                        .flex_1()
                        .flex_basis(px(0.0))
                        .min_h_0()
                        .overflow_hidden()
                        .child(terminal_bottom_content(tab)),
                )
            })
    }

    pub(in crate::app) fn terminal_panes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(active) = self.main_terminal() else {
            return div().flex_1().size_full().bg(color(theme::BG_TERMINAL));
        };
        let pane_count = active.panes.len();

        div().flex().flex_1().min_w_0().overflow_hidden().children(
            active.panes.iter().enumerate().map(|(index, slot)| {
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
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(color(theme::TEXT_DIM))
                                .child("Terminal mounting...")
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
            }),
        )
    }
}

fn terminal_bottom_content(tab: &TerminalTab) -> impl IntoElement {
    div().size_full().min_h_0().overflow_hidden().child(
        match tab.panes.first().and_then(|slot| slot.pane.as_ref()) {
            Some(pane) => gpui::AnyView::from(pane.view.clone())
                .cached(gpui::StyleRefinement::default().flex().size_full())
                .into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(color(theme::TEXT_DIM))
                .child("Terminal mounting...")
                .into_any_element(),
        },
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

fn terminal_bottom_tab_button(
    terminal_id: usize,
    label: String,
    active: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!(
            "terminal-bottom-tab-{terminal_id}"
        )))
        .h(px(32.0))
        .px_3()
        .relative()
        .flex()
        .items_center()
        .gap_2()
        .rounded_md()
        .cursor_pointer()
        .text_color(if active {
            cx.theme().foreground
        } else {
            cx.theme().secondary_foreground
        })
        .bg(if active {
            cx.theme().secondary_hover
        } else {
            cx.theme().transparent
        })
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_terminal_tab(terminal_id, window, cx)
        }))
        .child(
            div()
                .text_size(rems(0.75))
                .line_height(rems(0.875))
                .child(label),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "terminal-bottom-tab-close-{terminal_id}"
                )))
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .text_color(cx.theme().secondary_foreground)
                .hover(|style| style.bg(cx.theme().secondary_hover))
                .on_click(cx.listener(move |app, _event, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                    app.close_terminal_tab(terminal_id, window, cx)
                }))
                .child(Icon::new(HeroIconName::XMark).size_3()),
        )
}

fn terminal_bottom_add_button(cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .id("terminal-bottom-tab-add")
        .size(px(26.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .text_color(cx.theme().secondary_foreground)
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_click(cx.listener(|app, _event, window, cx| app.add_terminal_tab(window, cx)))
        .child(Icon::new(HeroIconName::Plus).size_3p5())
}
