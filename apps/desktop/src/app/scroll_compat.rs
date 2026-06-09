use std::{ops::Range, panic::Location, rc::Rc};

use gpui::{
    AnyElement, App, Context, Div, Element, ElementId, InteractiveElement, IntoElement,
    ListHorizontalSizingBehavior, ListSizingBehavior, ParentElement, Pixels, RenderOnce,
    ScrollHandle, Stateful, StatefulInteractiveElement, StyleRefinement, Styled,
    UniformListScrollHandle, Window, div,
};
use gpui_component::{
    StyledExt,
    scroll::{Scrollbar, ScrollbarAxis, ScrollbarHandle, ScrollbarShow},
};

pub(super) fn codux_uniform_list<V, T, F>(
    id: &'static str,
    rows: Rc<Vec<T>>,
    scroll_handle: UniformListScrollHandle,
    gap: Option<Pixels>,
    cx: &mut Context<V>,
    render: F,
) -> impl IntoElement
where
    V: 'static,
    T: Clone + 'static,
    F: Fn(T, usize, &mut Window, &mut Context<V>) -> AnyElement + 'static,
{
    let count = rows.len();
    let list = gpui::uniform_list(id, count, {
        cx.processor(move |_app, visible_range: Range<usize>, window, cx| {
            visible_range
                .filter_map(|index| {
                    rows.get(index)
                        .cloned()
                        .map(|row| render(row, index, window, cx))
                })
                .collect::<Vec<_>>()
        })
    })
    .size_full()
    .flex_grow()
    .with_sizing_behavior(ListSizingBehavior::Infer)
    .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::FitList)
    .track_scroll(&scroll_handle);
    let list = if let Some(gap) = gap {
        list.gap(gap)
    } else {
        list
    };

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .child(list)
        .vertical_scrollbar(&scroll_handle)
}

pub trait ScrollableElement: InteractiveElement + Styled + ParentElement + Element {
    #[track_caller]
    fn scrollbar<H: ScrollbarHandle + Clone>(
        self,
        scroll_handle: &H,
        axis: impl Into<ScrollbarAxis>,
    ) -> Self {
        self.child(ScrollbarLayer {
            id: "scrollbar_layer".into(),
            axis: axis.into(),
            scroll_handle: Rc::new(scroll_handle.clone()),
        })
    }

    #[track_caller]
    fn vertical_scrollbar<H: ScrollbarHandle + Clone>(self, scroll_handle: &H) -> Self {
        self.scrollbar(scroll_handle, ScrollbarAxis::Vertical)
    }

    #[track_caller]
    fn overflow_y_scrollbar(self) -> Scrollable<Self> {
        Scrollable::new(self)
    }
}

#[derive(IntoElement)]
pub struct Scrollable<E: InteractiveElement + Styled + ParentElement + Element> {
    id: ElementId,
    element: E,
}

impl<E> Scrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element,
{
    #[track_caller]
    fn new(element: E) -> Self {
        Self {
            id: ElementId::CodeLocation(*Location::caller()),
            element,
        }
    }
}

impl<E> Styled for Scrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element,
{
    fn style(&mut self) -> &mut StyleRefinement {
        self.element.style()
    }
}

impl<E> ParentElement for Scrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element,
{
    fn extend(&mut self, elements: impl IntoIterator<Item = gpui::AnyElement>) {
        self.element.extend(elements)
    }
}

impl InteractiveElement for Scrollable<Div> {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.element.interactivity()
    }
}

impl InteractiveElement for Scrollable<Stateful<Div>> {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.element.interactivity()
    }
}

impl<E> RenderOnce for Scrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element + 'static,
{
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scroll_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, _| ScrollHandle::default())
            .read(cx)
            .clone();

        let style = self.element.style().clone();
        *self.element.style() = StyleRefinement::default();

        div()
            .id(self.id)
            .size_full()
            .refine_style(&style)
            .relative()
            .child(
                div()
                    .id("scroll-area")
                    .flex()
                    .size_full()
                    .track_scroll(&scroll_handle)
                    .flex_col()
                    .overflow_y_scroll()
                    .child(self.element.flex_1()),
            )
            .child(render_scrollbar(
                "scrollbar",
                &scroll_handle,
                ScrollbarAxis::Vertical,
                window,
                cx,
            ))
    }
}

impl ScrollableElement for Div {}

impl<E> ScrollableElement for Stateful<E>
where
    E: ParentElement + Styled + Element,
    Self: InteractiveElement,
{
}

#[derive(IntoElement)]
struct ScrollbarLayer<H: ScrollbarHandle + Clone> {
    id: ElementId,
    axis: ScrollbarAxis,
    scroll_handle: Rc<H>,
}

impl<H> RenderOnce for ScrollbarLayer<H>
where
    H: ScrollbarHandle + Clone + 'static,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        render_scrollbar(self.id, self.scroll_handle.as_ref(), self.axis, window, cx)
    }
}

fn render_scrollbar<H: ScrollbarHandle + Clone>(
    id: impl Into<ElementId>,
    scroll_handle: &H,
    axis: ScrollbarAxis,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    if window.is_inspector_picking(cx) {
        return div();
    }

    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .child(
            Scrollbar::new(scroll_handle)
                .id(id)
                .axis(axis)
                .scrollbar_show(ScrollbarShow::Scrolling),
        )
}
