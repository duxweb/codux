use super::*;
use crate::app::app_state::{CoduxTooltipPlacement, CoduxTooltipState};
use gpui::{
    AnyElement, Display, Element, GlobalElementId, InspectorElementId, InteractiveElement,
    LayoutId, Position, Stateful, Style, deferred,
};

pub(in crate::app) fn with_codux_tooltip(
    app_entity: gpui::Entity<CoduxApp>,
    id: impl Into<ElementId>,
    element: impl IntoElement,
    text: impl Into<SharedString>,
) -> impl IntoElement {
    codux_tooltip_container(app_entity, id, text).child(element)
}

pub(in crate::app) fn codux_tooltip_container(
    app_entity: gpui::Entity<CoduxApp>,
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
) -> Stateful<gpui::Div> {
    codux_tooltip_container_with_placement(app_entity, id, text, CoduxTooltipPlacement::Auto)
}

pub(in crate::app) fn codux_tooltip_container_with_placement(
    app_entity: gpui::Entity<CoduxApp>,
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    placement: CoduxTooltipPlacement,
) -> Stateful<gpui::Div> {
    let text = text.into();
    let id = id.into();
    let tooltip_id = id.clone();
    let bounds = Rc::new(Cell::new(Bounds::default()));
    let bounds_writer = bounds.clone();
    div()
        .id(id)
        .flex_none()
        .on_prepaint(move |element_bounds, _, _| bounds_writer.set(element_bounds))
        .on_click({
            let app_entity = app_entity.clone();
            move |_event, _window, cx| {
                app_entity.update(cx, |app, cx| app.clear_codux_tooltip(cx));
            }
        })
        .on_hover(move |hovered, _window, cx| {
            app_entity.update(cx, |app, cx| {
                app.set_codux_tooltip(
                    *hovered,
                    tooltip_id.clone(),
                    text.clone(),
                    bounds.get(),
                    placement,
                    cx,
                );
            });
        })
}

impl CoduxApp {
    pub(in crate::app) fn set_codux_tooltip(
        &mut self,
        hovered: bool,
        id: ElementId,
        text: SharedString,
        bounds: Bounds<Pixels>,
        placement: CoduxTooltipPlacement,
        cx: &mut Context<Self>,
    ) {
        if !hovered {
            self.hide_codux_tooltip(&id, cx);
            return;
        }
        if self.tooltip_state.id.as_ref() == Some(&id)
            && self.tooltip_state.text == text
            && self.tooltip_state.bounds == bounds
            && self.tooltip_state.placement == placement
        {
            return;
        }
        self.tooltip_state = CoduxTooltipState {
            id: Some(id),
            text,
            bounds,
            placement,
        };
        cx.notify();
    }

    pub(in crate::app) fn hide_codux_tooltip(&mut self, id: &ElementId, cx: &mut Context<Self>) {
        if self.tooltip_state.id.as_ref() != Some(id) {
            return;
        }
        self.tooltip_state = CoduxTooltipState::default();
        cx.notify();
    }

    pub(in crate::app) fn clear_codux_tooltip(&mut self, cx: &mut Context<Self>) {
        if self.tooltip_state.id.is_none() {
            return;
        }
        self.tooltip_state = CoduxTooltipState::default();
        cx.notify();
    }

    pub(in crate::app) fn codux_tooltip_layer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(_) = self.tooltip_state.id.as_ref() else {
            return div().hidden().into_any_element();
        };

        deferred(
            codux_tooltip_positioner(self.tooltip_state.bounds, self.tooltip_state.placement)
                .child(
                    div()
                        .id("codux-tooltip-layer")
                        .max_w(px(260.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(color(0xFFFFFF).opacity(0.22))
                        .bg(color(0x000000))
                        .px_2()
                        .py_1()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(0xF4F6FA))
                        .whitespace_normal()
                        .on_click(cx.listener(|app, _event, _window, cx| {
                            app.clear_codux_tooltip(cx);
                            cx.stop_propagation();
                        }))
                        .child(self.tooltip_state.text.clone()),
                ),
        )
        .with_priority(2)
        .into_any_element()
    }
}

struct CoduxTooltipPositioner {
    trigger_bounds: Bounds<Pixels>,
    placement: CoduxTooltipPlacement,
    children: Vec<AnyElement>,
}

struct CoduxTooltipPositionerState {
    child_layout_ids: Vec<LayoutId>,
}

fn codux_tooltip_positioner(
    trigger_bounds: Bounds<Pixels>,
    placement: CoduxTooltipPlacement,
) -> CoduxTooltipPositioner {
    CoduxTooltipPositioner {
        trigger_bounds,
        placement,
        children: Vec::new(),
    }
}

impl ParentElement for CoduxTooltipPositioner {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Element for CoduxTooltipPositioner {
    type RequestLayoutState = CoduxTooltipPositionerState;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let child_layout_ids = self
            .children
            .iter_mut()
            .map(|child| child.request_layout(window, cx))
            .collect::<Vec<_>>();

        let layout_id = window.request_layout(
            Style {
                position: Position::Absolute,
                display: Display::Flex,
                ..Style::default()
            },
            child_layout_ids.iter().copied(),
            cx,
        );

        (layout_id, CoduxTooltipPositionerState { child_layout_ids })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if request_layout.child_layout_ids.is_empty() {
            return;
        }

        let mut child_min = point(Pixels::MAX, Pixels::MAX);
        let mut child_max = point(px(0.0), px(0.0));
        for child_layout_id in &request_layout.child_layout_ids {
            let child_bounds = window.layout_bounds(*child_layout_id);
            child_min = child_min.min(&child_bounds.origin);
            child_max = child_max.max(&child_bounds.bottom_right());
        }

        let tooltip_size: gpui::Size<Pixels> = (child_max - child_min).into();
        let offset = codux_tooltip_position(
            self.trigger_bounds,
            tooltip_size,
            window.viewport_size(),
            window.client_inset().unwrap_or(px(0.0)) + px(8.0),
            self.placement,
        ) - bounds.origin;
        let offset = point(offset.x.round(), offset.y.round());

        window.with_element_offset(offset, |window| {
            for child in &mut self.children {
                child.prepaint(window, cx);
            }
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for child in &mut self.children {
            child.paint(window, cx);
        }
    }
}

impl IntoElement for CoduxTooltipPositioner {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn codux_tooltip_position(
    trigger_bounds: Bounds<Pixels>,
    tooltip_size: gpui::Size<Pixels>,
    viewport_size: gpui::Size<Pixels>,
    margin: Pixels,
    placement: CoduxTooltipPlacement,
) -> gpui::Point<Pixels> {
    let gap = px(8.0);
    if placement == CoduxTooltipPlacement::Right {
        let right_limit = (viewport_size.width - tooltip_size.width - margin).max(margin);
        let bottom_limit = (viewport_size.height - tooltip_size.height - margin).max(margin);
        let x = (trigger_bounds.right() + gap).max(margin).min(right_limit);
        let centered_y = trigger_bounds.center().y - tooltip_size.height / 2.0;
        let y = centered_y.max(margin).min(bottom_limit);
        return point(x, y);
    }

    let centered_x = trigger_bounds.center().x - tooltip_size.width / 2.0;
    let below_y = trigger_bounds.bottom() + gap;
    let above_y = trigger_bounds.top() - tooltip_size.height - gap;
    let bottom_limit = (viewport_size.height - margin).max(margin);

    let y = if below_y + tooltip_size.height <= bottom_limit {
        below_y
    } else if above_y >= margin {
        above_y
    } else {
        (viewport_size.height - tooltip_size.height - margin).max(margin)
    };

    let right_limit = (viewport_size.width - tooltip_size.width - margin).max(margin);
    let x = centered_x.max(margin).min(right_limit);
    point(x, y)
}

pub(in crate::app) fn column_header(
    content: impl IntoElement,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .h(px(52.0))
        .w_full()
        .px(px(10.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().title_bar)
        .child(content)
}

pub(in crate::app) fn titlebar_drag_area(
    id: impl Into<ElementId>,
    element: gpui::Div,
) -> impl IntoElement {
    element
        .id(id)
        .window_control_area(WindowControlArea::Drag)
        .when(!cfg!(target_os = "windows"), |this| {
            this.on_click(|event, window, _| {
                if event.click_count() == 2 {
                    if cfg!(target_os = "macos") {
                        window.titlebar_double_click();
                    } else {
                        window.zoom_window();
                    }
                }
            })
        })
}

pub(in crate::app) fn header_icon_button(
    id: &'static str,
    icon: HeroIconName,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .icon(Icon::new(icon).text_color(cx.theme().secondary_foreground))
        .on_click(cx.listener(on_click))
}

pub(in crate::app) fn header_icon_button_loading(
    id: &'static str,
    icon: HeroIconName,
    loading: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .ghost()
        .loading(loading)
        .disabled(loading)
        .text_color(cx.theme().secondary_foreground)
        .icon(Icon::new(icon).text_color(cx.theme().secondary_foreground))
        .on_click(cx.listener(on_click))
}

pub(in crate::app) fn assistant_header_icon_button(
    id: &'static str,
    icon: HeroIconName,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .compact()
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .icon(
            Icon::new(icon)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .on_click(cx.listener(on_click))
}

pub(in crate::app) fn centered_empty_state(
    icon: HeroIconName,
    message: impl Into<String>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .size_full()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .text_color(cx.theme().muted_foreground)
        .child(
            Icon::new(icon)
                .size_5()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .child(message.into()),
        )
}

#[cfg(test)]
pub(in crate::app) fn restored_terminal_preview_lines(output_tail: &str) -> Vec<String> {
    output_tail
        .lines()
        .filter(|line| !line.trim().is_empty())
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|line| line.chars().take(96).collect::<String>())
        .collect()
}

pub(in crate::app) fn empty_label(value: &str) -> String {
    if value.trim().is_empty() {
        "none".to_string()
    } else {
        value.to_string()
    }
}

/// Shared text label for dialog footer buttons. Keeps every sub-window button at
/// the same size and line-height instead of each module rolling its own.
pub(in crate::app) fn dialog_button_label(label: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_size(rems(0.875))
        .line_height(rems(1.125))
        .child(label.into())
}

/// Primary action button used in dialog footers (save / update / confirm).
pub(in crate::app) fn dialog_primary_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> Button {
    Button::new(id)
        .primary()
        .text_color(cx.theme().primary_foreground)
        .child(dialog_button_label(label))
        .on_click(cx.listener(on_click))
}

/// Secondary action button used in dialog footers (test / retry / neutral).
pub(in crate::app) fn dialog_secondary_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> Button {
    Button::new(id)
        .secondary()
        .text_color(cx.theme().secondary_foreground)
        .child(dialog_button_label(label))
        .on_click(cx.listener(on_click))
}

/// Cancel / dismiss button used in dialog footers. Ghost styled so it reads as
/// the lower-emphasis action next to a primary button.
pub(in crate::app) fn dialog_cancel_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> Button {
    Button::new(id)
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .child(dialog_button_label(label))
        .on_click(cx.listener(on_click))
}

/// Standard bottom action bar shared by every sub-window dialog: fixed height,
/// top divider, right-aligned actions with consistent spacing.
pub(in crate::app) fn dialog_footer_bar(
    children: impl IntoElement,
    cx: &mut Context<CoduxApp>,
) -> gpui::Div {
    div()
        .h(px(56.0))
        .flex_shrink_0()
        .border_t_1()
        .border_color(cx.theme().border)
        .px(px(16.0))
        .flex()
        .items_center()
        .justify_end()
        .gap(px(8.0))
        .child(children)
}
