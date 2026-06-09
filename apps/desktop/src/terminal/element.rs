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
        let layout_changed = self.session.record_layout(cols as u16, rows as u16);
        if self.cursor_focused && layout_changed {
            if let Err(error) = self.session.claim_local_viewport() {
                eprintln!("failed to claim terminal viewport: {error}");
            }
        }

        let window_size = self.layout.lock().window_size();
        let local_owner = self.session.local_viewport_owns();
        let (model_cols, model_rows) = self.model.read(cx).dimensions();
        let next_cols = if local_owner { cols } else { model_cols };
        let next_rows = if local_owner { rows } else { model_rows };
        let resized = self.model.read(cx).dimensions() != (next_cols, next_rows);
        self.model.update(cx, |model, _| {
            model.resize(next_cols, next_rows, window_size)
        });
        if local_owner
            && resized
            && let Err(error) = self.session.resize(next_cols as u16, next_rows as u16)
        {
            eprintln!("failed to resize terminal pty: {error}");
        }

        let snapshot = self.model.update(cx, |model, cx| model.sync(cx));
        self.scroll_handle
            .update(&snapshot, self.renderer.cell_height.max(px(1.0)));
        trace_terminal_paint_snapshot(&snapshot, self.cursor_visible);
        let selection = self.model.read(cx).selection_range();
        self.renderer.prepare_paint(
            bounds,
            self.padding,
            &snapshot,
            selection,
            self.hover_link.as_ref(),
            self.cursor_visible,
            self.cursor_focused,
            window,
        )
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
                fallback_cursor_bounds: paint_state.ime_cursor_bounds,
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
    text_runs: Vec<TerminalTextRun>,
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
    text_runs: Vec<TerminalTextRun>,
}

impl TerminalPreparedRow {
    fn for_display_row(&self, row: usize) -> Self {
        let mut prepared = self.clone();
        for rect in &mut prepared.background_rects {
            rect.row = row;
        }
        for text_run in &mut prepared.text_runs {
            text_run.row = row;
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

struct TerminalCursorPaint {
    point: TerminalPoint,
    display_row: usize,
    shape: CursorShape,
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
            Bounds {
                origin: Point {
                    x: origin.x + renderer.cell_width * self.start_col as f32,
                    y: origin.y + renderer.cell_height * self.row as f32,
                },
                size: Size {
                    width: renderer.cell_width * self.width_cols as f32,
                    height: renderer.cell_height,
                },
            },
            px(0.0),
            self.color,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
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
        let x = origin.x + renderer.cell_width * self.point.column.0 as f32;
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
            CursorShape::Hidden => {}
            CursorShape::HollowBlock => {
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
            CursorShape::Beam => {
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
            CursorShape::Underline => {
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
            CursorShape::Block => {
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
}

#[derive(Clone, Debug)]
struct TerminalLayoutMetrics {
    bounds: Bounds<Pixels>,
    padding: Edges<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    cols: usize,
    rows: usize,
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

    fn window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: self.rows as u16,
            num_cols: self.cols as u16,
            cell_width: f32::from(self.cell_width).round().max(1.0) as u16,
            cell_height: f32::from(self.cell_height).round().max(1.0) as u16,
        }
    }
}
