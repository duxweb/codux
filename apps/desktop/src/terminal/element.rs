struct TerminalElement {
    model: Entity<TerminalModel>,
    renderer: TerminalRenderer,
    layout: Arc<Mutex<TerminalLayoutMetrics>>,
    scroll_handle: TerminalScrollHandle,
    session: TerminalSessionBinding,
    focus_handle: FocusHandle,
    terminal_view: WeakEntity<TerminalView>,
    padding: Edges<Pixels>,
    marked_text: Option<String>,
    hover_link: Option<TerminalLink>,
    cursor_visible: bool,
    cursor_focused: bool,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = TerminalPaintState;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::from(&self.focus_handle))
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size = Size::full();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let available_width =
            (bounds.size.width - self.padding.left - self.padding.right).max(px(1.0));
        let available_height =
            (bounds.size.height - self.padding.top - self.padding.bottom).max(px(1.0));
        let available_width: f32 = available_width.into();
        let available_height: f32 = available_height.into();
        let cell_width: f32 = self.renderer.cell_width.into();
        let cell_height: f32 = self.renderer.cell_height.into();
        let cols = terminal_grid_dimension(available_width, cell_width, 20);
        let rows = terminal_grid_dimension(available_height, cell_height, 1);
        self.layout.lock().update(
            bounds,
            self.padding,
            self.renderer.cell_width,
            self.renderer.cell_height,
            cols,
            rows,
        );
        let layout_record = self.session.record_layout(cols as u16, rows as u16);
        let mut local_owner = self.session.local_viewport_owns();
        // Claim only on first layout or when ownership sits elsewhere
        // (e.g. mobile handoff). When we already own the viewport, plain
        // size changes go through the debounced PTY resize below; claiming
        // here would resize the PTY on every frame of a window drag.
        if layout_record.initialized || !local_owner {
            if let Err(error) = self.session.claim_local_viewport() {
                eprintln!("failed to claim terminal viewport: {error}");
            }
            local_owner = self.session.local_viewport_owns();
        }

        let mut window_size = self.layout.lock().window_size();
        let (model_cols, model_rows) = self.model.read(cx).dimensions();
        let next_cols = if local_owner { cols } else { model_cols };
        let next_rows = if local_owner { rows } else { model_rows };
        // Keep the recorded window size consistent with the dims actually
        // requested from the engine: for a non-owner pane the layout dims
        // differ from the engine dims, and dimensions() must not drift.
        window_size.num_cols = next_cols as u16;
        window_size.num_lines = next_rows as u16;
        let resized = self.model.read(cx).dimensions() != (next_cols, next_rows);
        self.model.update(cx, |model, _| {
            model.resize(next_cols, next_rows, window_size)
        });
        if local_owner && resized {
            let scheduled = self.terminal_view.update(cx, |view, cx| {
                view.schedule_pty_resize(next_cols as u16, next_rows as u16, cx);
            });
            if scheduled.is_err()
                && let Err(error) = self.session.resize(next_cols as u16, next_rows as u16)
            {
                eprintln!("failed to resize terminal pty: {error}");
            }
        }

        let snapshot = self
            .model
            .update(cx, |model, cx| model.sync(cx).with_visible_row_shift(rows));
        self.layout
            .lock()
            .set_row_shift(snapshot.visible_row_shift);
        self.scroll_handle
            .update(&snapshot, self.renderer.cell_height.max(px(1.0)));
        trace_terminal_paint_snapshot(&snapshot, self.cursor_visible);
        let selection = self.model.read(cx).selection_range();
        let paint_state = self.renderer.prepare_paint(
            bounds,
            self.padding,
            &snapshot,
            selection,
            self.hover_link.as_ref(),
            self.cursor_visible,
            self.cursor_focused,
            window,
        );
        self.layout
            .lock()
            .record_ime_cursor_bounds(paint_state.ime_cursor_bounds);
        paint_state
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        paint_state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.renderer.paint_prepared(paint_state, window, cx);
        if let Some(marked_text) = self.marked_text.as_deref() {
            self.renderer
                .paint_marked_text(paint_state, marked_text, window, cx);
        }
        window.handle_input(
            &self.focus_handle,
            TerminalInputHandler {
                model: self.model.clone(),
                layout: self.layout.clone(),
                terminal_view: self.terminal_view.clone(),
            },
            cx,
        );
    }
}

struct TerminalPaintState {
    bounds: Bounds<Pixels>,
    origin: Point<Pixels>,
    background: Hsla,
    background_rects: Vec<TerminalBackgroundRect>,
    graphics: Vec<TerminalGraphicCell>,
    text_runs: Vec<TerminalTextRun>,
    lines: Vec<TerminalRowLine>,
    cursor: Option<TerminalCursorPaint>,
    marked_text_cursor: Option<TerminalPoint>,
    ime_cursor_bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct TerminalRowCacheKey {
    row_hash: u64,
    font_key: TerminalRendererCacheKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct TerminalRendererCacheKey {
    font_size_bits: u32,
    cell_width_bits: u32,
    cell_height_bits: u32,
}

#[derive(Clone, Default)]
struct TerminalRenderCache {
    rows: HashMap<TerminalRowCacheKey, TerminalPreparedRow>,
}

#[derive(Clone)]
struct TerminalPreparedRow {
    background_rects: Vec<TerminalBackgroundRect>,
    graphics: Vec<TerminalGraphicCell>,
    text_runs: Vec<TerminalTextRun>,
    line: Option<TerminalRowLine>,
}

impl TerminalPreparedRow {
    fn for_display_row(&self, row: usize) -> Self {
        let mut prepared = self.clone();
        for rect in &mut prepared.background_rects {
            rect.row = row;
        }
        for graphic in &mut prepared.graphics {
            graphic.row = row;
        }
        for text_run in &mut prepared.text_runs {
            text_run.row = row;
        }
        if let Some(line) = &mut prepared.line {
            line.row = row;
        }
        prepared
    }
}

#[derive(Clone)]
struct TerminalBackgroundRect {
    row: usize,
    start_col: usize,
    width_cols: usize,
    color: Hsla,
}

#[derive(Clone, Copy)]
struct TerminalGraphicCell {
    row: usize,
    col: usize,
    width_cols: usize,
    color: Hsla,
    graphic: TerminalCellGraphic,
}

struct TerminalCursorPaint {
    point: TerminalPoint,
    display_row: usize,
    shape: TerminalScreenCursorShape,
    color: Hsla,
    width: Pixels,
    text_run: Option<TerminalTextRun>,
}

impl TerminalBackgroundRect {
    fn paint(&self, renderer: &TerminalRenderer, origin: Point<Pixels>, window: &mut Window) {
        if self.width_cols == 0 {
            return;
        }
        window.paint_quad(quad(
            terminal_cell_bounds(renderer, origin, self.row, self.start_col, self.width_cols),
            px(0.0),
            self.color,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
    }
}

impl TerminalGraphicCell {
    fn paint(&self, renderer: &TerminalRenderer, origin: Point<Pixels>, window: &mut Window) {
        let bounds = terminal_cell_bounds(renderer, origin, self.row, self.col, self.width_cols);
        match self.graphic {
            TerminalCellGraphic::Block(graphic) => {
                self.paint_block(graphic, bounds, window);
            }
            TerminalCellGraphic::Box(graphic) => {
                self.paint_box(graphic, bounds, window);
            }
        }
    }

    fn paint_block(
        &self,
        graphic: TerminalBlockGraphic,
        bounds: Bounds<Pixels>,
        window: &mut Window,
    ) {
        match graphic {
            TerminalBlockGraphic::Full => self.paint_filled(bounds, window),
            TerminalBlockGraphic::Upper(ratio) => {
                self.paint_fraction(bounds, 0.0, 0.0, 1.0, ratio, window);
            }
            TerminalBlockGraphic::Lower(ratio) => {
                self.paint_fraction(bounds, 0.0, 1.0 - ratio, 1.0, ratio, window);
            }
            TerminalBlockGraphic::Left(ratio) => {
                self.paint_fraction(bounds, 0.0, 0.0, ratio, 1.0, window);
            }
            TerminalBlockGraphic::Right(ratio) => {
                self.paint_fraction(bounds, 1.0 - ratio, 0.0, ratio, 1.0, window);
            }
            TerminalBlockGraphic::Quadrants {
                upper_left,
                upper_right,
                lower_left,
                lower_right,
            } => {
                if upper_left {
                    self.paint_fraction(bounds, 0.0, 0.0, 0.5, 0.5, window);
                }
                if upper_right {
                    self.paint_fraction(bounds, 0.5, 0.0, 0.5, 0.5, window);
                }
                if lower_left {
                    self.paint_fraction(bounds, 0.0, 0.5, 0.5, 0.5, window);
                }
                if lower_right {
                    self.paint_fraction(bounds, 0.5, 0.5, 0.5, 0.5, window);
                }
            }
        }
    }

    fn paint_box(
        &self,
        graphic: TerminalBoxGraphic,
        bounds: Bounds<Pixels>,
        window: &mut Window,
    ) {
        if graphic.double {
            self.paint_double_box(graphic, bounds, window);
            return;
        }

        let thickness = match graphic.weight {
            TerminalBoxWeight::Light => 1.0,
            TerminalBoxWeight::Heavy => 2.0,
        };
        let x = f32::from(bounds.origin.x);
        let y = f32::from(bounds.origin.y);
        let right = x + f32::from(bounds.size.width);
        let bottom = y + f32::from(bounds.size.height);
        let center_x = (x + right) * 0.5;
        let center_y = (y + bottom) * 0.5;
        let half = thickness * 0.5;

        if graphic.left {
            self.paint_rect(x, center_y - half, center_x + half, center_y + half, window);
        }
        if graphic.right {
            self.paint_rect(center_x - half, center_y - half, right, center_y + half, window);
        }
        if graphic.up {
            self.paint_rect(center_x - half, y, center_x + half, center_y + half, window);
        }
        if graphic.down {
            self.paint_rect(center_x - half, center_y - half, center_x + half, bottom, window);
        }
    }

    fn paint_double_box(
        &self,
        graphic: TerminalBoxGraphic,
        bounds: Bounds<Pixels>,
        window: &mut Window,
    ) {
        let x = f32::from(bounds.origin.x);
        let y = f32::from(bounds.origin.y);
        let right = x + f32::from(bounds.size.width);
        let bottom = y + f32::from(bounds.size.height);
        let center_x = (x + right) * 0.5;
        let center_y = (y + bottom) * 0.5;
        let gap = 1.5;

        for offset in [-gap, gap] {
            if graphic.left {
                self.paint_rect(x, center_y + offset, center_x, center_y + offset + 1.0, window);
            }
            if graphic.right {
                self.paint_rect(center_x, center_y + offset, right, center_y + offset + 1.0, window);
            }
            if graphic.up {
                self.paint_rect(center_x + offset, y, center_x + offset + 1.0, center_y, window);
            }
            if graphic.down {
                self.paint_rect(center_x + offset, center_y, center_x + offset + 1.0, bottom, window);
            }
        }
    }

    fn paint_fraction(
        &self,
        bounds: Bounds<Pixels>,
        x_ratio: f32,
        y_ratio: f32,
        width_ratio: f32,
        height_ratio: f32,
        window: &mut Window,
    ) {
        let x = f32::from(bounds.origin.x);
        let y = f32::from(bounds.origin.y);
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        self.paint_rect(
            x + width * x_ratio,
            y + height * y_ratio,
            x + width * (x_ratio + width_ratio),
            y + height * (y_ratio + height_ratio),
            window,
        );
    }

    fn paint_rect(&self, x: f32, y: f32, right: f32, bottom: f32, window: &mut Window) {
        self.paint_filled(snapped_bounds(x, y, right, bottom), window);
    }

    fn paint_filled(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        if bounds.size.width <= px(0.0) || bounds.size.height <= px(0.0) {
            return;
        }
        window.paint_quad(quad(
            bounds,
            px(0.0),
            self.color,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
    }
}

fn terminal_cell_bounds(
    renderer: &TerminalRenderer,
    origin: Point<Pixels>,
    row: usize,
    col: usize,
    width_cols: usize,
) -> Bounds<Pixels> {
    let x = origin.x + renderer.cell_width * col as f32;
    let y = origin.y + renderer.cell_height * row as f32;
    let right = x + renderer.cell_width * width_cols.max(1) as f32;
    let bottom = y + renderer.cell_height;
    snapped_bounds(
        f32::from(x),
        f32::from(y),
        f32::from(right),
        f32::from(bottom),
    )
}

fn snapped_bounds(x: f32, y: f32, right: f32, bottom: f32) -> Bounds<Pixels> {
    let x = x.floor();
    let y = y.floor();
    let right = right.ceil();
    let bottom = bottom.ceil();
    Bounds {
        origin: Point { x: px(x), y: px(y) },
        size: Size {
            width: px((right - x).max(1.0)),
            height: px((bottom - y).max(1.0)),
        },
    }
}

impl TerminalCursorPaint {
    fn paint(
        &self,
        renderer: &TerminalRenderer,
        origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let x = origin.x + renderer.cell_width * self.point.column as f32;
        let y = origin.y + renderer.cell_height * self.display_row as f32;
        let bounds = Bounds {
            origin: Point {
                x: px(f32::from(x).floor()),
                y: px(f32::from(y).floor()),
            },
            size: Size {
                width: px(f32::from(self.width).round().max(1.0)),
                height: px(f32::from(renderer.cell_height).round().max(1.0)),
            },
        };

        match self.shape {
            TerminalScreenCursorShape::HollowBlock => {
                let border_width = px(1.0);
                window.paint_quad(quad(
                    bounds,
                    px(0.0),
                    transparent_black(),
                    Edges::all(border_width),
                    self.color,
                    Default::default(),
                ));
            }
            TerminalScreenCursorShape::Beam => {
                self.paint_filled(
                    Bounds {
                        origin: bounds.origin,
                        size: Size {
                            width: px(2.0),
                            height: bounds.size.height,
                        },
                    },
                    window,
                );
            }
            TerminalScreenCursorShape::Underline => {
                self.paint_filled(
                    Bounds {
                        origin: Point {
                            x: bounds.origin.x,
                            y: bounds.origin.y + bounds.size.height - px(2.0),
                        },
                        size: Size {
                            width: bounds.size.width,
                            height: px(2.0),
                        },
                    },
                    window,
                );
            }
            TerminalScreenCursorShape::Block => {
                self.paint_filled(bounds, window);
                if let Some(text_run) = &self.text_run {
                    text_run.paint(renderer, origin, window, cx);
                }
            }
        }
    }

    fn paint_filled(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        window.paint_quad(quad(
            bounds,
            px(0.0),
            self.color,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
struct TerminalCellPoint {
    row: usize,
    col: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
struct TerminalSelectionPoint {
    line: i32,
    col: usize,
}

#[derive(Clone, Copy, Debug)]
struct SelectionAutoScroll {
    edge_cell: TerminalCellPoint,
    lines: i32,
}

struct ScrollFlushResult {
    did_scroll: bool,
    next_lines: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SelectionRange {
    start: TerminalSelectionPoint,
    end: TerminalSelectionPoint,
}

#[derive(Clone, Debug, Default)]
struct SelectionState {
    anchor: Option<TerminalSelectionPoint>,
    head: Option<TerminalSelectionPoint>,
    dragging: bool,
}

impl SelectionState {
    fn start(&mut self, point: TerminalSelectionPoint) {
        self.anchor = Some(point);
        self.head = Some(point);
        self.dragging = true;
    }

    fn update(&mut self, point: TerminalSelectionPoint) -> bool {
        if self.anchor.is_some() {
            if self.head == Some(point) && self.dragging {
                return false;
            }
            self.head = Some(point);
            self.dragging = true;
            return true;
        }
        false
    }

    fn extend(&mut self, point: TerminalSelectionPoint) {
        if self.anchor.is_none() {
            self.anchor = self.head.or(Some(point));
        }
        self.head = Some(point);
        self.dragging = true;
    }

    fn finish(&mut self, point: TerminalSelectionPoint) {
        if self.anchor.is_some() {
            self.head = Some(point);
        }
        self.dragging = false;
    }

    fn clear(&mut self) {
        self.anchor = None;
        self.head = None;
        self.dragging = false;
    }

    fn set_range(&mut self, range: SelectionRange) {
        self.anchor = Some(range.start);
        self.head = Some(range.end);
    }

    fn range(&self) -> Option<SelectionRange> {
        let anchor = self.anchor?;
        let head = self.head?;
        let (start, end) = if anchor <= head {
            (anchor, head)
        } else {
            (head, anchor)
        };
        (start != end).then_some(SelectionRange { start, end })
    }
}

#[derive(Clone, Debug)]
struct TerminalLayoutMetrics {
    bounds: Bounds<Pixels>,
    padding: Edges<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    cols: usize,
    rows: usize,
    row_shift: usize,
    last_ime_cursor_bounds: Option<Bounds<Pixels>>,
}

impl Default for TerminalLayoutMetrics {
    fn default() -> Self {
        Self {
            bounds: Bounds {
                origin: Point {
                    x: px(0.0),
                    y: px(0.0),
                },
                size: Size {
                    width: px(0.0),
                    height: px(0.0),
                },
            },
            padding: Edges::all(px(0.0)),
            cell_width: px(1.0),
            cell_height: px(1.0),
            cols: 0,
            rows: 0,
            row_shift: 0,
            last_ime_cursor_bounds: None,
        }
    }
}

impl TerminalLayoutMetrics {
    fn update(
        &mut self,
        bounds: Bounds<Pixels>,
        padding: Edges<Pixels>,
        cell_width: Pixels,
        cell_height: Pixels,
        cols: usize,
        rows: usize,
    ) {
        self.bounds = bounds;
        self.padding = padding;
        self.cell_width = cell_width.max(px(1.0));
        self.cell_height = cell_height.max(px(1.0));
        self.cols = cols;
        self.rows = rows;
    }

    fn set_row_shift(&mut self, row_shift: usize) {
        self.row_shift = row_shift;
    }

    fn record_ime_cursor_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        if let Some(bounds) = bounds {
            self.last_ime_cursor_bounds = Some(bounds);
        }
    }

    fn last_ime_cursor_bounds(&self) -> Option<Bounds<Pixels>> {
        self.last_ime_cursor_bounds
            .filter(|bounds| self.contains_ime_bounds(*bounds))
    }

    fn first_cell_ime_bounds(&self) -> Option<Bounds<Pixels>> {
        if self.bounds.size.width <= px(0.0) || self.bounds.size.height <= px(0.0) {
            return None;
        }
        Some(Bounds {
            origin: Point {
                x: self.bounds.origin.x + self.padding.left,
                y: self.bounds.origin.y + self.padding.top,
            },
            size: Size {
                width: self.cell_width.max(px(1.0)),
                height: self.cell_height.max(px(1.0)),
            },
        })
    }

    fn contains_ime_bounds(&self, bounds: Bounds<Pixels>) -> bool {
        let left = self.bounds.origin.x;
        let top = self.bounds.origin.y;
        let right = self.bounds.origin.x + self.bounds.size.width;
        let bottom = self.bounds.origin.y + self.bounds.size.height;
        bounds.origin.x >= left
            && bounds.origin.y >= top
            && bounds.origin.x < right
            && bounds.origin.y < bottom
    }

    fn model_row(&self, row: usize) -> usize {
        row.saturating_add(self.row_shift)
    }

    fn model_cell_at(&self, position: Point<Pixels>) -> Option<TerminalCellPoint> {
        self.cell_at(position).map(|point| TerminalCellPoint {
            row: self.model_row(point.row),
            col: point.col,
        })
    }

    fn model_drag_cell_at(&self, position: Point<Pixels>) -> Option<(TerminalCellPoint, i32)> {
        self.drag_cell_at(position).map(|(point, lines)| {
            (
                TerminalCellPoint {
                    row: self.model_row(point.row),
                    col: point.col,
                },
                lines,
            )
        })
    }

    fn cell_at(&self, position: Point<Pixels>) -> Option<TerminalCellPoint> {
        if self.cols == 0 || self.rows == 0 {
            return None;
        }

        let origin = Point {
            x: self.bounds.origin.x + self.padding.left,
            y: self.bounds.origin.y + self.padding.top,
        };
        let relative_x = position.x - origin.x;
        let relative_y = position.y - origin.y;
        let width = self.cell_width * self.cols as f32;
        let height = self.cell_height * self.rows as f32;
        if relative_x < px(0.0)
            || relative_y < px(0.0)
            || relative_x >= width
            || relative_y >= height
        {
            return None;
        }

        Some(TerminalCellPoint {
            row: ((relative_y / self.cell_height) as usize).min(self.rows.saturating_sub(1)),
            col: ((relative_x / self.cell_width) as usize).min(self.cols.saturating_sub(1)),
        })
    }

    fn drag_cell_at(&self, position: Point<Pixels>) -> Option<(TerminalCellPoint, i32)> {
        if self.cols == 0 || self.rows == 0 {
            return None;
        }

        let origin = Point {
            x: self.bounds.origin.x + self.padding.left,
            y: self.bounds.origin.y + self.padding.top,
        };
        let relative_x = position.x - origin.x;
        let relative_y = position.y - origin.y;
        let width = self.cell_width * self.cols as f32;
        let height = self.cell_height * self.rows as f32;
        if relative_x < px(0.0) || relative_x >= width {
            return None;
        }

        let col = ((relative_x / self.cell_width) as usize).min(self.cols.saturating_sub(1));
        if relative_y < px(0.0) {
            let lines = ((-relative_y / self.cell_height) as i32 + 1).clamp(1, 8);
            return Some((TerminalCellPoint { row: 0, col }, lines));
        }
        if relative_y >= height {
            let lines = (((relative_y - height) / self.cell_height) as i32 + 1).clamp(1, 8);
            return Some((
                TerminalCellPoint {
                    row: self.rows.saturating_sub(1),
                    col,
                },
                -lines,
            ));
        }

        Some((
            TerminalCellPoint {
                row: ((relative_y / self.cell_height) as usize).min(self.rows.saturating_sub(1)),
                col,
            },
            0,
        ))
    }

    fn window_size(&self) -> TerminalWindowSize {
        TerminalWindowSize {
            num_lines: self.rows as u16,
            num_cols: self.cols as u16,
            cell_width: f32::from(self.cell_width).round().max(1.0) as u16,
            cell_height: f32::from(self.cell_height).round().max(1.0) as u16,
        }
    }
}
