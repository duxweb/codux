use super::*;

const TERMINAL_SPLIT_BASE_SIZE: Pixels = px(640.0);
const TERMINAL_SPLIT_BASE_WIDTH: Pixels = px(1200.0);
const TERMINAL_TOP_PANE_MIN_WIDTH: Pixels = px(160.0);
const TERMINAL_TOP_PANE_MIN_HEIGHT: Pixels = px(120.0);
const SPLIT_MENU_DIRECTIONS: [TerminalSplitDirection; 4] = [
    TerminalSplitDirection::Left,
    TerminalSplitDirection::Right,
    TerminalSplitDirection::Up,
    TerminalSplitDirection::Down,
];

fn terminal_layout_key_for_element_id(key: &str) -> String {
    key.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

pub(super) fn terminal_main_split_area(
    app_entity: gpui::Entity<CoduxApp>,
    language: &str,
    panes: Vec<TerminalPaneViewSnapshot>,
    layout_key: &str,
    top_ratios: &[f64],
    top_grid: &TerminalTopGrid,
    split_tree: &Option<TerminalSplitNode>,
    container_width: Option<Pixels>,
    container_height: Option<Pixels>,
    pane_drop_preview: Option<TerminalPaneDropPreview>,
    open_pane_menu: Option<PaneMenu>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    if panes.is_empty() {
        return div()
            .flex_1()
            .size_full()
            .bg(theme::terminal_fill(color(theme::BG_TERMINAL)))
            .into_any_element();
    }

    let pane_count = panes.len();
    let grid = terminal_top_grid_for_panes(top_grid.clone(), top_ratios, pane_count);
    let tree = terminal_split_tree_for_panes(split_tree.clone(), &grid, top_ratios, pane_count)
        .unwrap_or(TerminalSplitNode::Leaf { pane: 0 });
    let total_width = container_width.unwrap_or(TERMINAL_SPLIT_BASE_WIDTH);
    let total_height = container_height.unwrap_or(TERMINAL_SPLIT_BASE_SIZE);
    let overlay = terminal_pane_drag_overlay(
        app_entity.clone(),
        tree.clone(),
        pane_count,
        pane_drop_preview,
        cx,
    );
    let content = terminal_split_node_element(
        app_entity.clone(),
        layout_key,
        language,
        panes,
        &tree,
        Vec::new(),
        TerminalSplitDivider::None,
        pane_count,
        total_width,
        total_height,
        open_pane_menu,
        cx,
    );

    div()
        .relative()
        .group("terminal-pane-drag-target")
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .child(content)
        .child(overlay)
        .into_any_element()
}

fn terminal_pane_drag_overlay(
    app_entity: gpui::Entity<CoduxApp>,
    split_tree: TerminalSplitNode,
    pane_count: usize,
    pane_drop_preview: Option<TerminalPaneDropPreview>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .invisible()
        .bg(color(theme::BG_TERMINAL).opacity(0.12))
        .group_drag_over::<TerminalPaneDrag>("terminal-pane-drag-target", |this| this.visible())
        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
            cx.stop_propagation();
            window.prevent_default();
        })
        .on_drag_move::<TerminalPaneDrag>(cx.listener({
            let split_tree = split_tree.clone();
            move |view, event: &gpui::DragMoveEvent<TerminalPaneDrag>, _window, cx| {
                let Some(pane_index) = terminal_pane_drop_target_at_position(
                    &split_tree,
                    pane_count,
                    event.bounds,
                    event.event.position,
                ) else {
                    if view.pane_drop_preview.take().is_some() {
                        cx.notify();
                    }
                    return;
                };
                let next = Some(TerminalPaneDropPreview { pane_index });
                if view.pane_drop_preview != next {
                    view.pane_drop_preview = next;
                    cx.notify();
                }
            }
        }))
        .on_drop(cx.listener({
            let app_entity = app_entity.clone();
            move |view, drag: &TerminalPaneDrag, window, cx| {
                let from_index = drag.pane_index;
                let preview = view.pane_drop_preview.take();
                let target = preview
                    .map(|preview| preview.pane_index)
                    .unwrap_or(from_index);
                if target != from_index {
                    defer_terminal_workspace_app_update(
                        app_entity.clone(),
                        window,
                        cx,
                        move |app, _window, app_cx| {
                            app.swap_terminal_top_panes(from_index, target, app_cx);
                        },
                    );
                }
                cx.stop_propagation();
                cx.notify();
            }
        }))
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .left_0()
                .when_some(pane_drop_preview, |this, preview| {
                    this.children(terminal_pane_drop_placeholder(
                        &split_tree,
                        pane_count,
                        preview,
                    ))
                }),
        )
        .into_any_element()
}

fn terminal_pane_drop_placeholder(
    split_tree: &TerminalSplitNode,
    pane_count: usize,
    preview: TerminalPaneDropPreview,
) -> Vec<AnyElement> {
    let rect = terminal_pane_rect(split_tree, pane_count, preview.pane_index);
    vec![
        div()
            .absolute()
            .left(relative(rect.left))
            .top(relative(rect.top))
            .w(relative(rect.width))
            .h(relative(rect.height))
            .p_2()
            .child(
                div()
                    .size_full()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(color(theme::ACCENT).opacity(0.70))
                    .bg(color(theme::ACCENT).opacity(0.20)),
            )
            .into_any_element(),
    ]
}

#[derive(Clone, Copy)]
pub(in crate::app) struct TerminalPaneRect {
    pub(in crate::app) left: f32,
    pub(in crate::app) top: f32,
    pub(in crate::app) width: f32,
    pub(in crate::app) height: f32,
}

pub(in crate::app) fn terminal_pane_rect(
    split_tree: &TerminalSplitNode,
    pane_count: usize,
    pane_index: usize,
) -> TerminalPaneRect {
    if let Some(rect) = terminal_pane_rect_in_node(
        split_tree,
        pane_count,
        pane_index,
        TerminalPaneRect {
            left: 0.0,
            top: 0.0,
            width: 1.0,
            height: 1.0,
        },
    ) {
        return rect;
    }
    TerminalPaneRect {
        left: 0.0,
        top: 0.0,
        width: 1.0,
        height: 1.0,
    }
}

fn terminal_pane_rect_in_node(
    node: &TerminalSplitNode,
    pane_count: usize,
    pane_index: usize,
    rect: TerminalPaneRect,
) -> Option<TerminalPaneRect> {
    match node {
        TerminalSplitNode::Leaf { pane } => {
            (*pane == pane_index && *pane < pane_count).then_some(rect)
        }
        TerminalSplitNode::Split {
            axis,
            ratios,
            children,
        } => {
            let ratios = codux_runtime::terminal_layout::normalize_split_ratios(
                ratios.clone(),
                children.len(),
            );
            let mut offset = 0.0_f32;
            for (child, ratio) in children.iter().zip(ratios) {
                let ratio = ratio as f32;
                let child_rect = match axis {
                    SplitAxis::Horizontal => TerminalPaneRect {
                        left: rect.left + offset,
                        top: rect.top,
                        width: rect.width * ratio,
                        height: rect.height,
                    },
                    SplitAxis::Vertical => TerminalPaneRect {
                        left: rect.left,
                        top: rect.top + offset,
                        width: rect.width,
                        height: rect.height * ratio,
                    },
                };
                if let Some(rect) =
                    terminal_pane_rect_in_node(child, pane_count, pane_index, child_rect)
                {
                    return Some(rect);
                }
                offset += match axis {
                    SplitAxis::Horizontal => rect.width * ratio,
                    SplitAxis::Vertical => rect.height * ratio,
                };
            }
            None
        }
    }
}

pub(in crate::app) fn terminal_pane_drop_target_at_position(
    split_tree: &TerminalSplitNode,
    pane_count: usize,
    bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
) -> Option<usize> {
    if pane_count == 0 || bounds.size.width <= px(0.0) || bounds.size.height <= px(0.0) {
        return None;
    }
    let x = ((position.x - bounds.left()) / bounds.size.width).clamp(0.0, 0.999_999);
    let y = ((position.y - bounds.top()) / bounds.size.height).clamp(0.0, 0.999_999);
    terminal_pane_drop_target_in_node(
        split_tree,
        pane_count,
        x,
        y,
        TerminalPaneRect {
            left: 0.0,
            top: 0.0,
            width: 1.0,
            height: 1.0,
        },
    )
}

fn terminal_pane_drop_target_in_node(
    node: &TerminalSplitNode,
    pane_count: usize,
    x: f32,
    y: f32,
    rect: TerminalPaneRect,
) -> Option<usize> {
    match node {
        TerminalSplitNode::Leaf { pane } if *pane < pane_count => Some(*pane),
        TerminalSplitNode::Leaf { .. } => None,
        TerminalSplitNode::Split {
            axis,
            ratios,
            children,
        } => {
            let ratios = codux_runtime::terminal_layout::normalize_split_ratios(
                ratios.clone(),
                children.len(),
            );
            let mut offset = 0.0_f32;
            for (index, (child, ratio)) in children.iter().zip(ratios).enumerate() {
                let ratio = ratio as f32;
                let child_rect = match axis {
                    SplitAxis::Horizontal => TerminalPaneRect {
                        left: rect.left + offset,
                        top: rect.top,
                        width: rect.width * ratio,
                        height: rect.height,
                    },
                    SplitAxis::Vertical => TerminalPaneRect {
                        left: rect.left,
                        top: rect.top + offset,
                        width: rect.width,
                        height: rect.height * ratio,
                    },
                };
                let inside = x >= child_rect.left
                    && x <= child_rect.left + child_rect.width
                    && y >= child_rect.top
                    && y <= child_rect.top + child_rect.height;
                if inside || index + 1 == children.len() {
                    return terminal_pane_drop_target_in_node(child, pane_count, x, y, child_rect);
                }
                offset += match axis {
                    SplitAxis::Horizontal => rect.width * ratio,
                    SplitAxis::Vertical => rect.height * ratio,
                };
            }
            None
        }
    }
}

fn terminal_split_node_element(
    app_entity: gpui::Entity<CoduxApp>,
    layout_key: &str,
    language: &str,
    panes: Vec<TerminalPaneViewSnapshot>,
    node: &TerminalSplitNode,
    path: Vec<usize>,
    divider: TerminalSplitDivider,
    pane_count: usize,
    total_width: Pixels,
    total_height: Pixels,
    open_pane_menu: Option<PaneMenu>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    let content = match node {
        TerminalSplitNode::Leaf { pane } => {
            let slot = panes.get(*pane).cloned();
            slot.map(|slot| {
                terminal_pane(
                    app_entity,
                    *pane,
                    language,
                    pane_count,
                    slot,
                    open_pane_menu,
                    cx,
                )
            })
            .unwrap_or_else(|| div().size_full().into_any_element())
        }
        TerminalSplitNode::Split {
            axis,
            ratios,
            children,
        } => {
            let split_id = SharedString::from(format!(
                "workspace-terminal-split-tree-{}-{}-{}",
                terminal_layout_key_for_element_id(layout_key),
                pane_count,
                path.iter()
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>()
                    .join("-")
            ));
            let resize_app_entity = app_entity.clone();
            let resize_layout_key = layout_key.to_string();
            let resize_path = path.clone();
            let ratios = codux_runtime::terminal_layout::normalize_split_ratios(
                ratios.clone(),
                children.len(),
            );
            let render_child = move |(index, child): (usize, &TerminalSplitNode)| {
                let mut child_path = path.clone();
                child_path.push(index);
                let divider = if index == 0 {
                    TerminalSplitDivider::None
                } else {
                    match axis {
                        SplitAxis::Horizontal => TerminalSplitDivider::Left,
                        SplitAxis::Vertical => TerminalSplitDivider::Top,
                    }
                };
                let ratio = ratios
                    .get(index)
                    .copied()
                    .unwrap_or(1.0 / children.len().max(1) as f64);
                let child_element = terminal_split_node_element(
                    app_entity.clone(),
                    layout_key,
                    language,
                    panes.clone(),
                    child,
                    child_path,
                    divider,
                    pane_count,
                    total_width,
                    total_height,
                    open_pane_menu,
                    cx,
                );
                match axis {
                    SplitAxis::Horizontal => resizable_panel()
                        .size(px((total_width.as_f32() as f64 * ratio) as f32))
                        .size_range(TERMINAL_TOP_PANE_MIN_WIDTH..Pixels::MAX)
                        .child(child_element),
                    SplitAxis::Vertical => resizable_panel()
                        .size(px((total_height.as_f32() as f64 * ratio) as f32))
                        .size_range(TERMINAL_TOP_PANE_MIN_HEIGHT..Pixels::MAX)
                        .child(child_element),
                }
            };
            match axis {
                SplitAxis::Horizontal => h_resizable(split_id)
                    .on_resize({
                        let resize_app_entity = resize_app_entity.clone();
                        let resize_layout_key = resize_layout_key.clone();
                        let resize_path = resize_path.clone();
                        move |state: &gpui::Entity<ResizableState>, window, cx| {
                            let sizes = state.read(cx).sizes().clone();
                            let Some(ratios) = terminal_top_ratios_from_sizes(&sizes) else {
                                return;
                            };
                            window.defer(cx, {
                                let app_entity = resize_app_entity.clone();
                                let layout_key = resize_layout_key.clone();
                                let path = resize_path.clone();
                                move |_window, cx| {
                                    let _ = app_entity.update(cx, |app, cx| {
                                        app.update_terminal_split_ratios(
                                            layout_key, path, ratios, cx,
                                        );
                                    });
                                }
                            });
                        }
                    })
                    .children(children.iter().enumerate().map(render_child))
                    .into_any_element(),
                SplitAxis::Vertical => v_resizable(split_id)
                    .on_resize(move |state: &gpui::Entity<ResizableState>, window, cx| {
                        let sizes = state.read(cx).sizes().clone();
                        let Some(ratios) = terminal_top_ratios_from_sizes(&sizes) else {
                            return;
                        };
                        window.defer(cx, {
                            let app_entity = resize_app_entity.clone();
                            let layout_key = resize_layout_key.clone();
                            let path = resize_path.clone();
                            move |_window, cx| {
                                let _ = app_entity.update(cx, |app, cx| {
                                    app.update_terminal_split_ratios(layout_key, path, ratios, cx);
                                });
                            }
                        });
                    })
                    .children(children.iter().enumerate().map(render_child))
                    .into_any_element(),
            }
        }
    };
    terminal_split_divider(content, divider)
}

fn terminal_split_divider(child: AnyElement, divider: TerminalSplitDivider) -> AnyElement {
    let element = div()
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .child(child);
    match divider {
        TerminalSplitDivider::None => element,
        TerminalSplitDivider::Left => element.border_l_1().border_color(color(theme::BORDER_SOFT)),
        TerminalSplitDivider::Top => element.border_t_1().border_color(color(theme::BORDER_SOFT)),
    }
    .into_any_element()
}

fn terminal_top_ratios_from_sizes(sizes: &[Pixels]) -> Option<Vec<f64>> {
    if sizes.len() < 2 {
        return None;
    }
    let total = sizes.iter().map(|size| size.as_f32() as f64).sum::<f64>();
    if total <= 1.0 {
        return None;
    }
    Some(
        sizes
            .iter()
            .map(|size| size.as_f32() as f64 / total)
            .collect(),
    )
}

fn terminal_pane(
    app_entity: gpui::Entity<CoduxApp>,
    index: usize,
    language: &str,
    pane_count: usize,
    slot: TerminalPaneViewSnapshot,
    open_pane_menu: Option<PaneMenu>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    let close_id = SharedString::from(format!("terminal-pane-close-{index}"));
    let float_id = SharedString::from(format!("terminal-pane-float-{index}"));
    let collapse_id = SharedString::from(format!("terminal-pane-collapse-{index}"));
    let add_id = SharedString::from(format!("terminal-pane-add-{index}"));
    let chat_id = SharedString::from(format!("terminal-pane-chat-{index}"));
    let session_drop_entity = app_entity.clone();
    let pane_view = slot.view.clone();
    let drop_terminal_id = slot.terminal_id.clone();
    // Chat panes share the split chrome; only the hosted content differs.
    let is_chat = slot.is_chat;
    let chat_view = slot.chat_view.clone();
    let chat_pane_id = slot.terminal_id.clone();
    let mount_entity = app_entity.clone();
    // The search bar floats over the same top-right corner as the controls.
    let search_open = slot.search_open;

    // Flat pane: hairline divider against the previous column, controls float
    // over the terminal's top-right corner and appear on hover.
    div()
        .id(SharedString::from(format!("terminal-pane-{index}")))
        .relative()
        .group("terminal-pane")
        .flex()
        .flex_col()
        .flex_1()
        .flex_basis(px(0.0))
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .bg(theme::terminal_fill(color(theme::BG_TERMINAL)))
        .child(
            div()
                .flex_1()
                .flex_basis(px(0.0))
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(if is_chat {
                    match chat_view {
                        Some(view) => gpui::AnyView::from(view).into_any_element(),
                        // View not created yet (e.g. restored layout): mount on
                        // click, mirroring the terminal click-to-open pattern.
                        None => div()
                            .id(SharedString::from(format!("terminal-pane-chat-mount-{index}")))
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .bg(theme::terminal_fill(color(theme::BG_TERMINAL)))
                            .text_color(color(theme::TEXT_DIM))
                            .on_click(cx.listener(move |_view, _event, window, cx| {
                                let chat_pane_id = chat_pane_id.clone();
                                defer_terminal_workspace_app_update(
                                    mount_entity.clone(),
                                    window,
                                    cx,
                                    move |app, window, app_cx| {
                                        if let Some(chat_pane_id) = chat_pane_id.as_deref() {
                                            app.ensure_chat_view(chat_pane_id, window, app_cx);
                                            app.update_terminal_workspace_view(app_cx);
                                        }
                                    },
                                );
                            }))
                            .child(workspace_i18n(
                                language,
                                "terminal.chat.start",
                                "Click to start AI chat",
                            ))
                            .into_any_element(),
                    }
                } else {
                    match pane_view {
                        Some(view) => gpui::AnyView::from(view).into_any_element(),
                        None => div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(theme::terminal_fill(color(theme::BG_TERMINAL)))
                            .text_color(color(theme::TEXT_DIM))
                            .child(workspace_i18n(
                                language,
                                "terminal.detached.mounting",
                                "Mounting terminal...",
                            ))
                            .into_any_element(),
                    }
                }),
        )
        .when(!search_open, |pane| {
            pane.child(
                div()
                    .absolute()
                    .top(px(6.0))
                    .right(px(8.0))
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded(px(6.0))
                    .p(px(2.0))
                    .bg(theme::elevate(color(theme::BG_TERMINAL), 0.08).opacity(0.92))
                    .opacity(0.0)
                    .group_hover("terminal-pane", |style| style.opacity(1.0))
                    // The popover overlay lives outside the group, so hovering the
                    // menu would fade the controls out — pin them while it's open.
                    .when(
                        open_pane_menu.is_some_and(|menu| menu.pane_index == index),
                        |style| style.opacity(1.0),
                    )
                    .child(terminal_pane_drag_handle(app_entity.clone(), index, cx))
                    .child(terminal_pane_control_button(
                        app_entity.clone(),
                        float_id,
                        HeroIconName::ArrowTopRightOnSquare,
                        SharedString::from(workspace_i18n(
                            language,
                            "terminal.detach",
                            "Open in Separate Window",
                        )),
                        pane_count > 1 && !is_chat,
                        cx,
                        move |app, window, cx| app.float_terminal_pane(index, window, cx),
                    ))
                    .child(terminal_pane_control_button(
                        app_entity.clone(),
                        collapse_id,
                        HeroIconName::ChevronDown,
                        SharedString::from(workspace_i18n(
                            language,
                            "terminal.collapse",
                            "Collapse to Sidebar",
                        )),
                        pane_count > 1 && !is_chat,
                        cx,
                        move |app, window, cx| app.collapse_terminal_pane(index, window, cx),
                    ))
                    .child(terminal_pane_split_button(
                        app_entity.clone(),
                        add_id,
                        index,
                        open_pane_menu,
                        cx,
                    ))
                    .when(!is_chat, |controls| {
                        controls.child(terminal_pane_chat_button(
                            app_entity.clone(),
                            chat_id,
                            index,
                            language,
                            open_pane_menu,
                            cx,
                        ))
                    })
                    .child(terminal_pane_control_button(
                        app_entity,
                        close_id,
                        HeroIconName::XMark,
                        SharedString::from(if is_chat {
                            workspace_i18n(language, "terminal.chat.close", "Close Chat Split")
                        } else {
                            workspace_i18n(language, "terminal.split.close", "Close Split")
                        }),
                        pane_count > 1 || is_chat,
                        cx,
                        move |app, window, cx| app.close_terminal_pane(index, window, cx),
                    )),
            )
        })
        .when(!is_chat, |pane| {
            pane.child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .left_0()
                    .invisible()
                    .p_2()
                    .group_drag_over::<TaskSessionDrag>("terminal-pane", |this| this.visible())
                    .on_drop(
                        cx.listener(move |_view, drag: &TaskSessionDrag, window, cx| {
                            let session_id = drag.session_id.clone();
                            let terminal_id = drop_terminal_id.clone();
                            defer_terminal_workspace_app_update(
                                session_drop_entity.clone(),
                                window,
                                cx,
                                move |app, window, app_cx| {
                                    app.paste_ai_session_restore_to_main_pane(
                                        terminal_id.as_deref(),
                                        &session_id,
                                        window,
                                        app_cx,
                                    );
                                },
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        div()
                            .size_full()
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(color(theme::ACCENT).opacity(0.70))
                            .bg(color(theme::ACCENT).opacity(0.12)),
                    ),
            )
        })
        .into_any_element()
}

fn terminal_pane_drag_handle(
    app_entity: gpui::Entity<CoduxApp>,
    pane_index: usize,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    let drag_icon = div()
        .id(SharedString::from(format!(
            "terminal-pane-drag-source-{pane_index}"
        )))
        .size(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .child(
            Icon::new(HeroIconName::ArrowsPointingOut)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_drag(TerminalPaneDrag { pane_index }, move |drag, _, _, cx| {
            cx.stop_propagation();
            cx.new(|_| TerminalPaneDrag {
                pane_index: drag.pane_index,
            })
        });

    div()
        .id(SharedString::from(format!(
            "terminal-pane-drag-handle-{pane_index}"
        )))
        .size(px(28.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .text_color(cx.theme().secondary_foreground)
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
            cx.stop_propagation();
            window.prevent_default();
        })
        .child(drag_icon)
        .map(|this| {
            codux_tooltip_container(
                app_entity,
                SharedString::from(format!("terminal-pane-drag-tooltip-{pane_index}")),
                "拖动分屏",
            )
            .child(this)
        })
        .into_any_element()
}

fn terminal_pane_split_button(
    app_entity: gpui::Entity<CoduxApp>,
    id: SharedString,
    pane_index: usize,
    open_pane_menu: Option<PaneMenu>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    let is_open = open_pane_menu
        .is_some_and(|menu| menu.pane_index == pane_index && menu.kind == PaneMenuKind::Split);
    let view = cx.entity();
    let content_id = SharedString::from(format!("{id}-menu-content"));
    let button = Button::new(SharedString::from(format!("{id}-default")))
        .with_size(Size::Size(px(22.0)))
        .rounded(px(3.0))
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(cx.theme().secondary_foreground)
                .hover(cx.theme().secondary_hover)
                .active(cx.theme().secondary_hover),
        )
        .icon(
            Icon::new(HeroIconName::Plus)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .on_hover(pane_menu_hover_listener(view.clone(), pane_index, PaneMenuKind::Split))
        .on_click({
            let app_entity = app_entity.clone();
            let view = view.clone();
            move |_, window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.open_pane_menu = None;
                    cx.notify();
                });
                cx.update_entity(&app_entity, |app, cx| {
                    app.split_terminal_direction(
                        TerminalSplitDirection::Right,
                        TerminalSplitScope::Inner,
                        pane_index,
                        window,
                        cx,
                    );
                });
            }
        });

    Popover::new(id)
        .anchor(Anchor::TopRight)
        .appearance(false)
        .overlay_closable(false)
        .open(is_open)
        .trigger(button)
        .content(move |_, _window, cx| {
            // Icon-only grid: row 1 = split inside the current pane (dashed
            // frame), row 2 = split the whole layout (solid frame, edge slice).
            let row = |scope: TerminalSplitScope,
                       app_entity: &gpui::Entity<CoduxApp>,
                       view: &gpui::Entity<TerminalWorkspaceView>| {
                div()
                    .flex()
                    .gap_1()
                    .children(SPLIT_MENU_DIRECTIONS.into_iter().map(|direction| {
                        terminal_split_direction_menu_button(
                            app_entity.clone(),
                            view.clone(),
                            pane_index,
                            direction,
                            scope,
                        )
                    }))
            };
            div()
                .id(content_id.clone())
                .flex()
                .flex_col()
                .gap_1()
                .rounded_lg()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().popover)
                .shadow_lg()
                .p_1()
                .on_hover(pane_menu_hover_listener(view.clone(), pane_index, PaneMenuKind::Split))
                .child(row(TerminalSplitScope::Inner, &app_entity, &view))
                .child(row(TerminalSplitScope::Root, &app_entity, &view))
        })
        .into_any_element()
}

fn pane_menu_hover_listener(
    view: gpui::Entity<TerminalWorkspaceView>,
    pane_index: usize,
    kind: PaneMenuKind,
) -> impl Fn(&bool, &mut Window, &mut gpui::App) + 'static {
    move |hovered, _window, cx| {
        let _ = view.update(cx, |view, cx| {
            if *hovered {
                view.set_pane_menu_open(pane_index, kind, true, cx);
            } else {
                view.close_pane_menu_after_hover_gap(pane_index, kind, cx);
            }
        });
    }
}

fn terminal_split_direction_menu_button(
    app_entity: gpui::Entity<CoduxApp>,
    view: gpui::Entity<TerminalWorkspaceView>,
    pane_index: usize,
    direction: TerminalSplitDirection,
    scope: TerminalSplitScope,
) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "terminal-pane-split-{pane_index}-{scope:?}-{direction:?}"
        )))
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .hover(|style| style.bg(color(theme::ACCENT).opacity(0.12)))
        .child(terminal_split_direction_icon(direction, scope))
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.open_pane_menu = None;
                cx.notify();
            });
            cx.update_entity(&app_entity, |app, cx| {
                app.split_terminal_direction(direction, scope, pane_index, window, cx);
            });
        })
        .into_any_element()
}

/// The "✦" chat button mirrors the "+" split button: hover opens the same
/// direction grid; picking a direction steps the popover to an agent list
/// (codex first) and the chosen agent's chat pane lands at that position.
fn terminal_pane_chat_button(
    app_entity: gpui::Entity<CoduxApp>,
    id: SharedString,
    pane_index: usize,
    language: &str,
    open_pane_menu: Option<PaneMenu>,
    cx: &mut Context<TerminalWorkspaceView>,
) -> AnyElement {
    let menu = open_pane_menu
        .filter(|menu| menu.pane_index == pane_index && menu.kind == PaneMenuKind::Chat);
    let is_open = menu.is_some();
    let chat_pick = menu.and_then(|menu| menu.chat_pick);
    let view = cx.entity();
    let content_id = SharedString::from(format!("{id}-menu-content"));
    let language = language.to_string();
    let button = Button::new(SharedString::from(format!("{id}-default")))
        .with_size(Size::Size(px(22.0)))
        .rounded(px(3.0))
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(cx.theme().secondary_foreground)
                .hover(cx.theme().secondary_hover)
                .active(cx.theme().secondary_hover),
        )
        .icon(
            Icon::new(HeroIconName::Sparkles)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .on_hover(pane_menu_hover_listener(
            view.clone(),
            pane_index,
            PaneMenuKind::Chat,
        ))
        .on_click({
            // Click = the default placement (split right, inner) but the agent
            // still has to be chosen, so jump straight to the picker step.
            let view = view.clone();
            move |_, _window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.set_chat_menu_pick(
                        pane_index,
                        (TerminalSplitDirection::Right, TerminalSplitScope::Inner),
                        cx,
                    );
                });
            }
        });

    Popover::new(id)
        .anchor(Anchor::TopRight)
        .appearance(false)
        .overlay_closable(false)
        .open(is_open)
        .trigger(button)
        .content(move |_, _window, cx| {
            let container = div()
                .id(content_id.clone())
                .flex()
                .flex_col()
                .gap_1()
                .rounded_lg()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().popover)
                .shadow_lg()
                .p_1()
                .on_hover(pane_menu_hover_listener(
                    view.clone(),
                    pane_index,
                    PaneMenuKind::Chat,
                ));
            if let Some((direction, scope)) = chat_pick {
                // Step 2: which agent runs the new chat pane.
                container
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .text_size(rems(0.6875))
                            .text_color(cx.theme().secondary_foreground)
                            .child(workspace_i18n(
                                &language,
                                "terminal.chat.agent.title",
                                "New AI chat",
                            )),
                    )
                    .child(chat_agent_menu_row(
                        app_entity.clone(),
                        view.clone(),
                        pane_index,
                        direction,
                        scope,
                        "Codex",
                        true,
                        &language,
                    ))
                    .child(chat_agent_menu_row(
                        app_entity.clone(),
                        view.clone(),
                        pane_index,
                        direction,
                        scope,
                        "Claude Code",
                        false,
                        &language,
                    ))
                    .child(chat_agent_menu_row(
                        app_entity.clone(),
                        view.clone(),
                        pane_index,
                        direction,
                        scope,
                        "OpenCode",
                        false,
                        &language,
                    ))
            } else {
                // Step 1: same direction grid as the terminal split button.
                let row = |scope: TerminalSplitScope, view: &gpui::Entity<TerminalWorkspaceView>| {
                    div()
                        .flex()
                        .gap_1()
                        .children(SPLIT_MENU_DIRECTIONS.into_iter().map(|direction| {
                            chat_split_direction_menu_button(
                                view.clone(),
                                pane_index,
                                direction,
                                scope,
                            )
                        }))
                };
                container
                    .child(row(TerminalSplitScope::Inner, &view))
                    .child(row(TerminalSplitScope::Root, &view))
            }
        })
        .into_any_element()
}

/// Direction tile in the chat menu: steps to the agent picker instead of
/// splitting immediately.
fn chat_split_direction_menu_button(
    view: gpui::Entity<TerminalWorkspaceView>,
    pane_index: usize,
    direction: TerminalSplitDirection,
    scope: TerminalSplitScope,
) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "terminal-pane-chat-split-{pane_index}-{scope:?}-{direction:?}"
        )))
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .hover(|style| style.bg(color(theme::ACCENT).opacity(0.12)))
        .child(terminal_split_direction_icon(direction, scope))
        .on_click(move |_, _window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.set_chat_menu_pick(pane_index, (direction, scope), cx);
            });
        })
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn chat_agent_menu_row(
    app_entity: gpui::Entity<CoduxApp>,
    view: gpui::Entity<TerminalWorkspaceView>,
    pane_index: usize,
    direction: TerminalSplitDirection,
    scope: TerminalSplitScope,
    label: &'static str,
    enabled: bool,
    language: &str,
) -> AnyElement {
    let row = div()
        .id(SharedString::from(format!(
            "terminal-pane-chat-agent-{pane_index}-{label}"
        )))
        .min_w(px(132.0))
        .px_2()
        .py(px(5.0))
        .flex()
        .items_center()
        .gap_2()
        .rounded_md()
        .child(
            Icon::new(HeroIconName::Sparkles)
                .size_3()
                .text_color(if enabled {
                    color(theme::ACCENT)
                } else {
                    color(theme::TEXT_DIM)
                }),
        )
        .child(
            div()
                .flex_1()
                .text_size(rems(0.75))
                .text_color(if enabled {
                    color(theme::TEXT)
                } else {
                    color(theme::TEXT_DIM)
                })
                .child(label),
        );
    if enabled {
        row.cursor_pointer()
            .hover(|style| style.bg(color(theme::ACCENT).opacity(0.12)))
            .on_click(move |_, window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.open_pane_menu = None;
                    cx.notify();
                });
                cx.update_entity(&app_entity, |app, cx| {
                    app.open_chat_split_direction(direction, scope, pane_index, window, cx);
                });
            })
            .into_any_element()
    } else {
        row.opacity(0.55)
            .child(
                div()
                    .text_size(rems(0.625))
                    .text_color(color(theme::TEXT_DIM))
                    .child(workspace_i18n(
                        language,
                        "terminal.chat.agent.soon",
                        "Coming soon",
                    )),
            )
            .into_any_element()
    }
}

/// Split glyphs: INNER = dashed frame (the current pane) cut in half, the new
/// half filled; ROOT = solid frame (the whole layout) with a new slice pushed
/// in from that edge.
fn terminal_split_direction_icon(
    direction: TerminalSplitDirection,
    scope: TerminalSplitScope,
) -> AnyElement {
    // Frames must stay legible on the dark popover: border rides on text-level
    // grey (dashed gaps read darker than solid, so no extra dimming), the new
    // slice is near-solid accent, the remaining area a faint grey wash.
    let frame_line = color(theme::TEXT_DIM).opacity(0.9);
    let active = color(theme::ACCENT).opacity(0.95);
    let inactive = color(theme::TEXT_DIM).opacity(0.16);
    let horizontal = matches!(
        direction,
        TerminalSplitDirection::Left | TerminalSplitDirection::Right
    );
    let before = matches!(
        direction,
        TerminalSplitDirection::Left | TerminalSplitDirection::Up
    );

    let frame = div()
        .size(px(22.0))
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(frame_line)
        .flex()
        .map(|frame| if horizontal { frame } else { frame.flex_col() })
        .map(|frame| match scope {
            TerminalSplitScope::Inner => frame.border_dashed(),
            TerminalSplitScope::Root => frame,
        });

    let (new_cell, old_cell) = match scope {
        // Inner: the pane splits 50/50, new half filled.
        TerminalSplitScope::Inner => (
            div()
                .flex_1()
                .rounded(px(1.0))
                .bg(active)
                .into_any_element(),
            div()
                .flex_1()
                .rounded(px(1.0))
                .bg(inactive)
                .into_any_element(),
        ),
        // Root: a narrow full-length slice lands at the edge of the layout.
        TerminalSplitScope::Root => (
            div()
                .map(|cell| {
                    if horizontal {
                        cell.w(px(5.0)).h_full()
                    } else {
                        cell.h(px(5.0)).w_full()
                    }
                })
                .flex_none()
                .rounded(px(1.0))
                .bg(active)
                .into_any_element(),
            div()
                .flex_1()
                .rounded(px(1.0))
                .bg(inactive)
                .into_any_element(),
        ),
    };

    if before {
        frame.child(new_cell).child(old_cell)
    } else {
        frame.child(old_cell).child(new_cell)
    }
    .into_any_element()
}

fn terminal_pane_control_button(
    app_entity: gpui::Entity<CoduxApp>,
    id: SharedString,
    icon: HeroIconName,
    tooltip: SharedString,
    enabled: bool,
    cx: &mut Context<TerminalWorkspaceView>,
    on_click: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let text_color = if enabled {
        cx.theme().secondary_foreground
    } else {
        color(theme::TEXT_DIM)
    };
    let inner = div()
        .size(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .child(Icon::new(icon).size_3p5().text_color(text_color));
    let inner = if enabled {
        inner.hover(|style| style.bg(cx.theme().secondary_hover))
    } else {
        inner
    };
    let button = codux_tooltip_container(app_entity.clone(), id, tooltip)
        .size(px(28.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .text_color(text_color)
        .child(inner);

    if enabled {
        let on_click = std::rc::Rc::new(on_click);
        button
            .cursor_pointer()
            .on_click(cx.listener(move |_view, _event, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                let on_click = on_click.clone();
                defer_terminal_workspace_app_update(
                    app_entity.clone(),
                    window,
                    cx,
                    move |app, window, app_cx| on_click(app, window, app_cx),
                );
            }))
            .into_any_element()
    } else {
        button.opacity(0.45).into_any_element()
    }
}

fn defer_terminal_workspace_app_update(
    app_entity: gpui::Entity<CoduxApp>,
    window: &mut Window,
    cx: &mut Context<TerminalWorkspaceView>,
    update: impl FnOnce(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) {
    defer_codux_app_update(app_entity, window, cx, update);
}
