use super::*;

pub(super) fn git_diff_window_body(
    diff: &str,
    derived_rows: Option<&GitReviewDerivedRows>,
    code_scroll_handle: ScrollHandle,
    empty_label: String,
    original_label: String,
    current_label: String,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    if let Some(rows) = derived_rows {
        return div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(git_diff_window_content_panel(
                "git-diff-window-original-code",
                &original_label,
                rows.original.clone(),
                VirtualListScrollHandle::from(code_scroll_handle.clone()),
                cx,
            ))
            .child(git_diff_window_content_panel(
                "git-diff-window-current-code",
                &current_label,
                rows.current.clone(),
                VirtualListScrollHandle::from(code_scroll_handle),
                cx,
            ))
            .into_any_element();
    }

    div()
        .flex_1()
        .min_h_0()
        .overflow_y_scrollbar()
        .bg(color(theme::BG_TERMINAL))
        .px_4()
        .py_3()
        .children(if diff.trim().is_empty() {
            vec![
                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(rems(0.875))
                    .line_height(rems(1.125))
                    .text_color(color(theme::TEXT_DIM))
                    .child(empty_label)
                    .into_any_element(),
            ]
        } else {
            diff.lines()
                .map(|line| git_diff_line_row(line).into_any_element())
                .collect::<Vec<_>>()
        })
        .into_any_element()
}

fn git_diff_window_content_panel(
    list_id: &'static str,
    title: &str,
    cells: Rc<Vec<GitReviewAlignedCell>>,
    scroll_handle: VirtualListScrollHandle,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let item_sizes = Rc::new(vec![size(px(1.0), px(18.0)); cells.len()]);
    let list_cells = cells.clone();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .flex_basis(px(0.0))
        .min_w_0()
        .overflow_hidden()
        .border_r_1()
        .border_color(color(theme::BORDER_SOFT))
        .child(
            div()
                .h(px(30.0))
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .border_b_1()
                .border_color(color(theme::BORDER_SOFT))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .bg(color(theme::BG_TERMINAL))
                .p_2()
                .text_size(rems(0.75))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .font_family("SF Mono")
                .child(
                    v_virtual_list(
                        cx.entity().clone(),
                        list_id,
                        item_sizes,
                        move |_app, visible_range: Range<usize>, _window, _cx| {
                            visible_range
                                .filter_map(|index| {
                                    let cell = list_cells.get(index)?;
                                    Some(git_diff_window_code_line(cell.clone()))
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .track_scroll(&scroll_handle)
                    .with_sizing_behavior(ListSizingBehavior::Auto),
                )
                .vertical_scrollbar(&scroll_handle),
        )
}

fn git_diff_window_code_line(cell: GitReviewAlignedCell) -> AnyElement {
    let (line_bg, gutter_bg, marker_color) = match cell.tone {
        Some(GitReviewLineTone::Addition) => (
            Some(color(theme::GREEN).opacity(0.10)),
            color(theme::GREEN).opacity(0.16),
            color(theme::GREEN),
        ),
        Some(GitReviewLineTone::Deletion) => (
            Some(color(theme::RED).opacity(0.11)),
            color(theme::RED).opacity(0.16),
            color(theme::RED),
        ),
        None => (
            None,
            color(theme::BG_PANEL).opacity(0.72),
            color(theme::TEXT_DIM),
        ),
    };
    div()
        .h(px(18.0))
        .flex()
        .w_full()
        .min_w_0()
        .when_some(line_bg, |this, bg| this.bg(bg))
        .child(
            div()
                .w(px(46.0))
                .h_full()
                .flex_none()
                .pr_2()
                .border_r_1()
                .border_color(color(theme::BORDER_SOFT).opacity(0.55))
                .bg(gutter_bg)
                .text_align(gpui::TextAlign::Right)
                .text_color(marker_color)
                .child(
                    cell.line_number
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_x_hidden()
                .pl_2()
                .child(cell.text),
        )
        .into_any_element()
}

fn git_diff_line_row(line: &str) -> impl IntoElement {
    let line_color = if line.starts_with('+') && !line.starts_with("+++") {
        theme::GREEN
    } else if line.starts_with('-') && !line.starts_with("---") {
        theme::RED
    } else if line.starts_with("@@") {
        theme::ACCENT
    } else {
        theme::TEXT_MUTED
    };

    div()
        .min_h(px(18.0))
        .text_size(rems(0.75))
        .line_height(rems(1.125))
        .font_family("SF Mono")
        .text_color(color(line_color))
        .child(line.to_string())
}
