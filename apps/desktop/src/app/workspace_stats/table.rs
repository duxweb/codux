use super::format::*;
use super::*;

#[derive(Clone)]
pub(in crate::app) struct StatsProjectTableDelegate {
    rows: Vec<StatsProjectRow>,
    language: String,
    layout_width: f32,
    columns: Vec<Column>,
}

impl StatsProjectTableDelegate {
    pub(in crate::app) fn new(rows: Vec<StatsProjectRow>, language: String) -> Self {
        let columns = stats_project_table_columns(&language, STATS_TABLE_BASE_WIDTH);
        Self {
            rows,
            language,
            layout_width: STATS_TABLE_BASE_WIDTH,
            columns,
        }
    }

    pub(in crate::app) fn set_rows(&mut self, rows: Vec<StatsProjectRow>, language: String) {
        self.rows = rows;
        if self.language != language {
            self.columns = stats_project_table_columns(&language, self.layout_width);
            self.language = language;
        }
    }

    pub(in crate::app) fn set_layout_width(&mut self, layout_width: f32, language: String) -> bool {
        let layout_width = layout_width.max(STATS_TABLE_BASE_WIDTH);
        if self.language == language && (self.layout_width - layout_width).abs() < 1.0 {
            return false;
        }
        self.language = language;
        self.layout_width = layout_width;
        self.columns = stats_project_table_columns(&self.language, self.layout_width);
        true
    }

    fn sort_rows(&mut self, col_ix: usize, sort: ColumnSort) {
        if matches!(sort, ColumnSort::Default) {
            self.rows.sort_by(|left, right| {
                right
                    .total_tokens
                    .cmp(&left.total_tokens)
                    .then_with(|| left.project.cmp(&right.project))
            });
            return;
        }
        let descending = matches!(sort, ColumnSort::Descending);
        self.rows.sort_by(|left, right| {
            let ordering = match col_ix {
                0 => left.project.cmp(&right.project),
                1 => left.total_tokens.cmp(&right.total_tokens),
                2 => left.no_cache_tokens.cmp(&right.no_cache_tokens),
                3 => left.input_tokens.cmp(&right.input_tokens),
                4 => left.output_tokens.cmp(&right.output_tokens),
                5 => left.cached_input_tokens.cmp(&right.cached_input_tokens),
                6 => left.request_count.cmp(&right.request_count),
                7 => left
                    .active_duration_seconds
                    .cmp(&right.active_duration_seconds),
                _ => left.project.cmp(&right.project),
            };
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
}

impl TableDelegate for StatsProjectTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let column = self.column(col_ix, cx);
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .text_size(rems(0.8125))
            .line_height(rems(1.125))
            .text_color(color(theme::TEXT_MUTED))
            .child(column.name)
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(("stats-project-row", row_ix))
            .bg(if row_ix.is_multiple_of(2) {
                cx.theme().secondary.opacity(0.12)
            } else {
                cx.theme().transparent
            })
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(row) = self.rows.get(row_ix) else {
            return div().into_any_element();
        };
        let text = match col_ix {
            0 => row.project.clone(),
            1 => compact_number(row.total_tokens),
            2 => compact_number(row.no_cache_tokens),
            3 => compact_number(row.input_tokens),
            4 => compact_number(row.output_tokens),
            5 => compact_number(row.cached_input_tokens),
            6 => compact_number(row.request_count),
            7 => format_duration_short(row.active_duration_seconds),
            _ => String::new(),
        };
        let cell = div()
            .size_full()
            .flex()
            .items_center()
            .px_3()
            .text_size(rems(0.875))
            .line_height(rems(1.125))
            .text_color(if col_ix == 0 {
                color(theme::TEXT)
            } else {
                color(theme::TEXT_MUTED)
            });
        if col_ix == 0 {
            cell.child(div().truncate().child(text)).into_any_element()
        } else {
            cell.justify_end()
                .child(div().text_align(gpui::TextAlign::Right).child(text))
                .into_any_element()
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        self.sort_rows(col_ix, sort);
        cx.notify();
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(rems(0.8125))
            .text_color(cx.theme().muted_foreground)
            .child(stats_text(
                &self.language,
                "stats.projects.empty",
                "No project stats yet",
            ))
            .into_any_element()
    }
}

pub(super) fn stats_project_table_columns(language: &str, layout_width: f32) -> Vec<Column> {
    let project_width = 414.0 + (layout_width - STATS_TABLE_BASE_WIDTH).max(0.0) * 0.34;
    let metric_ratio = ((layout_width - project_width) / (STATS_TABLE_BASE_WIDTH - 414.0)).max(1.0);
    vec![
        Column::new(
            "project",
            stats_text(language, "stats.table.project", "Project"),
        )
        .width(px(project_width))
        .min_width(px(220.0))
        .sortable(),
        Column::new(
            "total",
            stats_text(language, "stats.table.total", "Total Tokens"),
        )
        .width(px(126.0 * metric_ratio))
        .text_right()
        .sortable(),
        Column::new(
            "no_cache",
            stats_text(language, "stats.table.no_cache", "No-Cache Tokens"),
        )
        .width(px(132.0 * metric_ratio))
        .text_right()
        .sortable(),
        Column::new("input", stats_text(language, "stats.table.input", "Input"))
            .width(px(104.0 * metric_ratio))
            .text_right()
            .sortable(),
        Column::new(
            "output",
            stats_text(language, "stats.table.output", "Output"),
        )
        .width(px(104.0 * metric_ratio))
        .text_right()
        .sortable(),
        Column::new("cache", stats_text(language, "stats.table.cache", "Cache"))
            .width(px(104.0 * metric_ratio))
            .text_right()
            .sortable(),
        Column::new(
            "requests",
            stats_text(language, "stats.table.requests", "Requests"),
        )
        .width(px(106.0 * metric_ratio))
        .text_right()
        .sortable(),
        Column::new(
            "duration",
            stats_text(language, "stats.table.duration", "Runtime"),
        )
        .width(px(110.0 * metric_ratio))
        .text_right()
        .sortable(),
    ]
}
