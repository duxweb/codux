#[derive(Clone)]
struct TerminalRenderer {
    font_family: String,
    font_size: Pixels,
    line_height_multiplier: f32,
    fonts: TerminalFonts,
    cell_width: Pixels,
    cell_height: Pixels,
    palette: ColorPalette,
    measured_key: Option<TerminalCellMeasurementKey>,
    cache: Arc<Mutex<TerminalRenderCache>>,
}

struct TerminalPaintRequest<'a> {
    bounds: Bounds<Pixels>,
    padding: Edges<Pixels>,
    content: &'a TerminalContent,
    selection: Option<SelectionRange>,
    hover_link: Option<&'a TerminalLink>,
    cursor_visible: bool,
    cursor_focused: bool,
}

struct TerminalSelectionPaint {
    line: i32,
    row: usize,
    origin: Point<Pixels>,
    columns: usize,
    content_right: Pixels,
    selection: SelectionRange,
}

// Bundled with the app (runtime-assets/fonts/nerd-font-symbols); covers the
// Nerd Font PUA icons (starship, eza …) that regular families lack.
const TERMINAL_SYMBOL_FONT_FAMILY: &str = "Symbols Nerd Font Mono";

// Nerd Font glyphs live in the BMP private use area and supplementary PUA-A.
fn terminal_cell_is_private_use(text: &str) -> bool {
    terminal_cell_codepoint(text)
        .is_some_and(|codepoint| matches!(codepoint, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD))
}

#[derive(Clone)]
struct TerminalFonts {
    normal: Font,
    bold: Font,
    italic: Font,
    bold_italic: Font,
    symbol: Font,
}

impl TerminalFonts {
    fn new(font_family: &str) -> Self {
        let family: SharedString = font_family.to_string().into();
        let features = FontFeatures::disable_ligatures();
        let font = |weight, style| Font {
            family: family.clone(),
            features: features.clone(),
            fallbacks: None,
            weight,
            style,
        };
        Self {
            normal: font(FontWeight::NORMAL, FontStyle::Normal),
            bold: font(FontWeight::SEMIBOLD, FontStyle::Normal),
            italic: font(FontWeight::NORMAL, FontStyle::Italic),
            bold_italic: font(FontWeight::SEMIBOLD, FontStyle::Italic),
            symbol: Font {
                family: TERMINAL_SYMBOL_FONT_FAMILY.into(),
                features: FontFeatures::disable_ligatures(),
                fallbacks: None,
                weight: FontWeight::NORMAL,
                style: FontStyle::Normal,
            },
        }
    }

    fn get(&self, bold: bool, italic: bool) -> Font {
        match (bold, italic) {
            (true, true) => self.bold_italic.clone(),
            (true, false) => self.bold.clone(),
            (false, true) => self.italic.clone(),
            (false, false) => self.normal.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TerminalCellMeasurementKey {
    font_family: String,
    font_size_bits: u32,
    line_height_bits: u32,
}

impl TerminalCellMeasurementKey {
    fn new(font_family: &str, font_size: Pixels, line_height_multiplier: f32) -> Self {
        Self {
            font_family: font_family.to_string(),
            font_size_bits: f32::from(font_size).to_bits(),
            line_height_bits: line_height_multiplier.to_bits(),
        }
    }
}

impl TerminalRenderer {
    fn new(
        font_family: String,
        font_size: Pixels,
        line_height_multiplier: f32,
        palette: ColorPalette,
    ) -> Self {
        Self {
            fonts: TerminalFonts::new(&font_family),
            font_family,
            font_size,
            line_height_multiplier,
            cell_width: font_size * 0.6,
            cell_height: font_size * line_height_multiplier,
            palette,
            measured_key: None,
            cache: Arc::new(Mutex::new(TerminalRenderCache::default())),
        }
    }

    fn clear_cache(&self) {
        self.cache.lock().rows.clear();
    }

    fn cache_key(&self) -> TerminalRendererCacheKey {
        TerminalRendererCacheKey {
            font_size_bits: f32::from(self.font_size).to_bits(),
            cell_width_bits: f32::from(self.cell_width).to_bits(),
            cell_height_bits: f32::from(self.cell_height).to_bits(),
        }
    }

    fn measure_cell(&mut self, window: &mut Window) {
        let key = TerminalCellMeasurementKey::new(
            &self.font_family,
            self.font_size,
            self.line_height_multiplier,
        );
        if self.measured_key.as_ref() == Some(&key) {
            return;
        }
        let font = self.font(false, false);
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        self.cell_width = text_system
            .advance(font_id, self.font_size, 'm')
            .map(|size| size.width)
            .unwrap_or(self.font_size * 0.6);
        self.cell_height = self.font_size * self.line_height_multiplier;
        self.measured_key = Some(key);
        self.clear_cache();
    }

    fn font(&self, bold: bool, italic: bool) -> Font {
        self.fonts.get(bold, italic)
    }

    fn prepare_paint(
        &self,
        request: TerminalPaintRequest<'_>,
        window: &mut Window,
    ) -> TerminalPaintState {
        let TerminalPaintRequest {
            bounds,
            padding,
            content,
            selection,
            hover_link,
            cursor_visible,
            cursor_focused,
        } = request;
        let default_bg = self.palette.background();
        let origin = Point {
            x: bounds.origin.x + padding.left,
            y: bounds.origin.y + padding.top,
        };
        let content_right = bounds.origin.x + bounds.size.width - padding.right;
        let visible_rows = self.visible_row_range(bounds, padding, content, window);

        let mut background_rects = Vec::new();
        let mut graphics = Vec::new();
        let mut text_runs = Vec::new();
        let mut lines = Vec::new();
        let mut cursor_cell = None;
        let cache_key = self.cache_key();
        let mut index = 0usize;
        while index < content.cells.len() {
            let line = content.cells[index].point.line;
            let start = index;
            while index < content.cells.len() && content.cells[index].point.line == line {
                index += 1;
            }
            let Some(row) = content.display_row_for_line(line) else {
                continue;
            };
            if row < visible_rows.start || row >= visible_rows.end {
                continue;
            }
            let cells = &content.cells[start..index];
            // Use the per-line hash computed once when the content was built,
            // instead of re-hashing the row on every frame.
            let row_hash = content
                .row_hashes
                .get(line)
                .unwrap_or_else(|| terminal_row_hash(cells));
            let prepared = self.prepare_cached_row(row, cells, default_bg, cache_key, row_hash);
            if let Some(hover_link) = hover_link
                && hover_link.line == line
            {
                let mut hover_graphics = Vec::new();
                self.prepare_row_text(
                    row,
                    cells,
                    &mut text_runs,
                    &mut hover_graphics,
                    Some(hover_link.range.clone()),
                );
                background_rects.extend(prepared.background_rects);
                graphics.extend(hover_graphics);
            } else {
                background_rects.extend(prepared.background_rects);
                graphics.extend(prepared.graphics);
                text_runs.extend(prepared.text_runs);
                lines.extend(prepared.lines);
            }
            let cursor_line = content.viewport_start_line + content.cursor.row as i32;
            if content.display_row_for_line(cursor_line) == Some(row) {
                cursor_cell = cells
                    .iter()
                    .find(|indexed| indexed.point.column == content.cursor.col)
                    .map(|indexed| &indexed.cell);
            }
        }

        if let Some(selection) = selection {
            for row in visible_rows.clone() {
                let line = content.line_for_display_row(row);
                self.prepare_selection(
                    TerminalSelectionPaint {
                        line,
                        row,
                        origin,
                        columns: content.columns,
                        content_right,
                        selection,
                    },
                    &mut background_rects,
                );
            }
        }

        let display_cursor = content.display_cursor();
        let cursor_on_visible_row = display_cursor.row >= 0
            && (display_cursor.row as usize) < content.visible_rows()
            && display_cursor.col < content.columns
            && (visible_rows.start..visible_rows.end).contains(&(display_cursor.row as usize));
        let cursor = (cursor_visible && content.cursor.visible && cursor_on_visible_row).then(|| {
            let shape = if cursor_focused {
                content.cursor.shape
            } else {
                TerminalScreenCursorShape::HollowBlock
            };
            let row = display_cursor.row as usize;
            let col = display_cursor.col;
            let cursor_width = self.cursor_width(cursor_cell, default_bg, window);
            let text_run = cursor_cell
                .filter(|cell| {
                    cursor_focused
                        && content.cursor.shape == TerminalScreenCursorShape::Block
                        && !cell.hidden
                        && !cell.text.is_empty()
                })
                .map(|cell| {
                    let text = cell.text.clone();
                    TerminalTextRun::from_text(
                        row,
                        col,
                        text.clone(),
                        cell.width.max(1),
                        TextRun {
                            len: text.len(),
                            font: self.font(cell.bold, cell.italic),
                            color: default_bg,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        },
                    )
                });

            TerminalCursorPaint {
                point: TerminalPoint {
                    line: content.viewport_start_line + content.cursor.row as i32,
                    column: content.cursor.col,
                },
                display_row: row,
                shape,
                color: self.palette.cursor(),
                width: cursor_width,
                text_run,
            }
        });
        let ime_cursor_bounds = cursor_on_visible_row.then(|| {
            let width = self.cursor_width(cursor_cell, default_bg, window);
            let x = origin.x + self.cell_width * display_cursor.col as f32;
            let y = origin.y + self.cell_height * display_cursor.row as f32;
            Bounds {
                origin: Point { x, y },
                size: Size {
                    width,
                    height: self.cell_height,
                },
            }
        });

        // Inline images anchored like cells; the top may sit above the
        // visible range (negative display row), clipped at paint time.
        let base_line = content.line_for_display_row(0);
        let images = content
            .images
            .iter()
            .filter_map(|placement| {
                let row = i64::from(placement.line) - i64::from(base_line);
                let intersects = row + placement.image.rows as i64 > visible_rows.start as i64
                    && row < visible_rows.end as i64;
                intersects.then(|| TerminalImagePaint {
                    row: row as i32,
                    col: placement.image.col,
                    rows: placement.image.rows,
                    cols: placement.image.cols,
                    image: placement.image.clone(),
                })
            })
            .collect();

        TerminalPaintState {
            bounds,
            origin,
            background: default_bg,
            background_rects,
            images,
            graphics,
            text_runs,
            lines,
            cursor,
            marked_text_cursor: cursor_on_visible_row.then_some(TerminalPoint {
                line: display_cursor.row,
                column: display_cursor.col,
            }),
            ime_cursor_bounds,
        }
    }

    fn visible_row_range(
        &self,
        bounds: Bounds<Pixels>,
        padding: Edges<Pixels>,
        content: &TerminalContent,
        window: &mut Window,
    ) -> Range<usize> {
        if content.visible_rows() == 0 {
            return 0..0;
        }
        let content_bounds = Bounds {
            origin: Point {
                x: bounds.origin.x + padding.left,
                y: bounds.origin.y + padding.top,
            },
            size: Size {
                width: self.cell_width * content.columns.max(1) as f32,
                height: self.cell_height * content.visible_rows() as f32,
            },
        };
        let intersection = window.content_mask().bounds.intersect(&content_bounds);
        if intersection.size.width <= px(0.0) || intersection.size.height <= px(0.0) {
            return 0..0;
        }

        let cell_height = f32::from(self.cell_height).max(1.0);
        let top_delta = f32::from((intersection.origin.y - content_bounds.origin.y).max(px(0.0)));
        let start = (top_delta / cell_height).floor().max(0.0) as usize;
        let count = (f32::from(intersection.size.height) / cell_height)
            .ceil()
            .max(1.0) as usize
            + 1;
        let start = start.min(content.visible_rows());
        let end = start.saturating_add(count).min(content.visible_rows());
        start..end
    }

    fn cursor_width(
        &self,
        cursor_cell: Option<&TerminalScreenCellSnapshot>,
        default_bg: Hsla,
        window: &mut Window,
    ) -> Pixels {
        let Some(cell) = cursor_cell else {
            return self.cell_width;
        };
        if cell.hidden || cell.text.trim().is_empty() {
            return self.cell_width;
        }

        let shaped = window.text_system().shape_line(
            SharedString::from(cell.text.clone()),
            self.font_size,
            &[TextRun {
                len: cell.text.len(),
                font: self.font(cell.bold, cell.italic),
                color: default_bg,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );

        shaped.width.max(self.cell_width)
    }

    fn prepare_cached_row(
        &self,
        row: usize,
        cells: &[TerminalIndexedCell],
        default_bg: Hsla,
        font_key: TerminalRendererCacheKey,
        row_hash: u64,
    ) -> TerminalPreparedRow {
        let key = TerminalRowCacheKey { row_hash, font_key };
        if let Some(prepared) = self.cache.lock().rows.get(&key).cloned() {
            return prepared.for_display_row(row);
        }

        let mut background_rects = Vec::new();
        let mut graphics = Vec::new();
        let mut text_runs = Vec::new();
        self.prepare_row_backgrounds(0, cells, default_bg, &mut background_rects);
        self.prepare_row_text(0, cells, &mut text_runs, &mut graphics, None);
        // Combine maximal contiguous runs of simple (1:1 char-to-cell, ASCII)
        // spans into single shaped lines, so the row is shaped/painted once
        // instead of once per span; any remaining wide/multi-codepoint/
        // non-ASCII spans stay in text_runs and are painted per-span. One such
        // span no longer blocks the rest of the row from combining.
        let (lines, text_runs) = self.combine_row_runs(text_runs);
        let prepared = TerminalPreparedRow {
            background_rects,
            graphics,
            text_runs,
            lines,
        };
        let mut cache = self.cache.lock();
        if cache.rows.len() > TERMINAL_ROW_CACHE_LIMIT {
            cache.rows.clear();
        }
        cache.rows.insert(key, prepared.clone());
        prepared.for_display_row(row)
    }

    fn prepare_row_backgrounds(
        &self,
        row: usize,
        cells: &[TerminalIndexedCell],
        default_bg: Hsla,
        background_rects: &mut Vec<TerminalBackgroundRect>,
    ) {
        let mut current: Option<TerminalBackgroundRect> = None;
        for indexed in cells {
            let col = indexed.point.column;
            let bg = self.cell_colors(&indexed.cell).1;
            let width_cols = indexed.cell.width.max(1);
            if bg == default_bg {
                if let Some(rect) = current.take() {
                    background_rects.push(rect);
                }
                continue;
            }
            match current.as_mut() {
                Some(rect)
                    if rect.color == bg
                        && rect.start_col.saturating_add(rect.width_cols) == col =>
                {
                    rect.width_cols += width_cols;
                }
                Some(_) => {
                    if let Some(rect) = current.replace(TerminalBackgroundRect {
                        row,
                        start_col: col,
                        width_cols,
                        color: bg,
                    }) {
                        background_rects.push(rect);
                    }
                }
                None => {
                    current = Some(TerminalBackgroundRect {
                        row,
                        start_col: col,
                        width_cols,
                        color: bg,
                    });
                }
            }
        }
        if let Some(rect) = current {
            background_rects.push(rect);
        }
    }

    fn prepare_row_text(
        &self,
        row: usize,
        cells: &[TerminalIndexedCell],
        text_runs: &mut Vec<TerminalTextRun>,
        graphics: &mut Vec<TerminalGraphicCell>,
        underline_range: Option<Range<usize>>,
    ) {
        let mut current_run: Option<TerminalTextRun> = None;
        let mut pending_spaces = 0usize;
        let mut next_col = 0usize;
        for indexed in cells {
            let col = indexed.point.column;
            let cell = &indexed.cell;
            let cell_width = cell.width.max(1);
            if col > next_col {
                pending_spaces = 0;
            }
            if cell.hidden || cell.text.is_empty() {
                pending_spaces = 0;
                next_col = col.saturating_add(cell_width);
                continue;
            }
            if let Some(graphic) = terminal_cell_codepoint(&cell.text).and_then(terminal_builtin_graphic) {
                if let Some(current) = current_run.take() {
                    text_runs.push(current);
                }
                pending_spaces = 0;
                let (fg, _) = self.cell_render_colors(cell);
                graphics.push(TerminalGraphicCell {
                    row,
                    col,
                    width_cols: cell_width,
                    color: fg,
                    graphic,
                });
                next_col = col.saturating_add(cell_width);
                continue;
            }
            if cell.text == " " {
                // Decorated underlines are painted per cell, so an underlined
                // space still gets its segment even outside a text run.
                if let Some(graphic) = terminal_underline_graphic(cell.underline) {
                    graphics.push(TerminalGraphicCell {
                        row,
                        col,
                        width_cols: cell_width,
                        color: self.cell_underline_color(cell),
                        graphic,
                    });
                }
                if current_run.is_some() {
                    pending_spaces += 1;
                }
                next_col = col.saturating_add(cell_width);
                continue;
            }

            let (fg, _) = self.cell_render_colors(cell);
            let text = cell.text.clone();
            let symbol = terminal_cell_is_private_use(&text);
            let link_underline = underline_range
                .as_ref()
                .is_some_and(|range| range.contains(&col));
            let underline_color = self.cell_underline_color(cell);
            if let Some(graphic) = terminal_underline_graphic(cell.underline) {
                graphics.push(TerminalGraphicCell {
                    row,
                    col,
                    width_cols: cell_width,
                    color: underline_color,
                    graphic,
                });
            }
            // Single/curly draw through gpui's run underline; double/dotted/
            // dashed went to the decoration channel above.
            let native_underline = link_underline
                || matches!(
                    cell.underline,
                    TerminalScreenUnderline::Single | TerminalScreenUnderline::Curly
                );
            let run = TextRun {
                len: text.len(),
                font: if symbol {
                    self.fonts.symbol.clone()
                } else {
                    self.font(cell.bold, cell.italic)
                },
                color: fg,
                background_color: None,
                underline: native_underline.then_some(UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(if link_underline { fg } else { underline_color }),
                    wavy: link_underline || cell.underline == TerminalScreenUnderline::Curly,
                }),
                strikethrough: cell.strikeout.then_some(gpui::StrikethroughStyle {
                    thickness: px(1.0),
                    color: Some(fg),
                }),
            };
            if symbol {
                // Symbol glyph advances differ from the cell grid: keep every
                // icon a standalone span so it stays anchored to its column.
                if let Some(current) = current_run.take() {
                    text_runs.push(current);
                }
                text_runs.push(TerminalTextRun::from_text(row, col, text, cell_width, run));
            } else if current_run.as_ref().is_some_and(|current| {
                current.can_append(row, col, cell_width, pending_spaces, &run)
            }) {
                if let Some(current) = current_run.as_mut() {
                    current.append_spaces(pending_spaces);
                    current.append_text(&text, cell_width);
                }
            } else {
                if let Some(current) = current_run.take() {
                    text_runs.push(current);
                }
                current_run = Some(TerminalTextRun::from_text(row, col, text, cell_width, run));
            }
            pending_spaces = 0;
            next_col = col.saturating_add(cell_width);
        }

        if let Some(current) = current_run {
            text_runs.push(current);
        }
    }

    fn cell_render_colors(&self, cell: &TerminalScreenCellSnapshot) -> (Hsla, Hsla) {
        let (mut fg, bg) = self.cell_colors(cell);
        if !cell.text.chars().all(char::is_whitespace)
            && !terminal_cell_is_private_use(&cell.text)
            && terminal_cell_codepoint(&cell.text)
                .and_then(terminal_builtin_graphic)
                .is_none()
        {
            fg = ensure_contrast(fg, bg, MIN_TERMINAL_TEXT_CONTRAST);
        }
        (fg, bg)
    }

    fn cell_colors(&self, cell: &TerminalScreenCellSnapshot) -> (Hsla, Hsla) {
        let mut fg = self.palette.resolve_fg(&cell.fg, false, cell.dim);
        let mut bg = self.palette.resolve_bg(&cell.bg);
        let bold_source = if cell.inverse { &cell.bg } else { &cell.fg };
        if cell.inverse {
            std::mem::swap(&mut fg, &mut bg);
        }
        if cell.bold
            && let TerminalScreenColor::Indexed { index } = bold_source
            && *index < 8
        {
            fg = self
                .palette
                .resolve_fg(&TerminalScreenColor::Indexed { index: *index + 8 }, false, false);
        }
        (fg, bg)
    }

    /// Decode an inline image to the GPU-ready BGRA form once, keyed by the
    /// engine's image id.
    fn render_image(&self, image: &TerminalScreenImage) -> Option<Arc<RenderImage>> {
        let mut cache = self.cache.lock();
        if let Some(render) = cache.images.get(&image.id) {
            return Some(render.clone());
        }
        let mut decoded = image::load_from_memory(&image.data).ok()?.into_rgba8();
        for pixel in decoded.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let render = Arc::new(RenderImage::new(vec![image::Frame::new(decoded)]));
        cache.images.insert(image.id, render.clone());
        Some(render)
    }

    /// SGR 58 override when present, else the rendered text foreground.
    fn cell_underline_color(&self, cell: &TerminalScreenCellSnapshot) -> Hsla {
        match &cell.underline_color {
            Some(color) => self.palette.resolve_fg(color, false, cell.dim),
            None => self.cell_render_colors(cell).0,
        }
    }

    fn prepare_selection(
        &self,
        request: TerminalSelectionPaint,
        background_rects: &mut Vec<TerminalBackgroundRect>,
    ) {
        let TerminalSelectionPaint {
            line,
            row,
            origin,
            columns,
            content_right,
            selection,
        } = request;
        if line < selection.start.line || line > selection.end.line {
            return;
        }

        let start_col = if line == selection.start.line {
            selection.start.col
        } else {
            0
        };
        let end_col = if line == selection.end.line {
            selection.end.col
        } else {
            columns
        };
        if start_col >= end_col || start_col >= columns {
            return;
        }

        let width_cols = if end_col >= columns {
            let x = origin.x + self.cell_width * start_col as f32;
            if content_right <= x {
                return;
            }
            columns.saturating_sub(start_col).max(1)
        } else {
            end_col.saturating_sub(start_col)
        };
        background_rects.push(TerminalBackgroundRect {
            row,
            start_col,
            width_cols,
            color: self.palette.selection,
        });
    }

    fn paint_prepared(&self, state: &TerminalPaintState, window: &mut Window, cx: &mut App) {
        // In transparent style, skip the opaque base fill so the workspace
        // body's translucent backing (and the window blur) shows behind the
        // terminal; styled cells still paint their own opaque backgrounds.
        let base_background = if crate::theme::window_is_solid() {
            state.background
        } else {
            transparent_black()
        };
        window.paint_quad(quad(
            state.bounds,
            px(0.0),
            base_background,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));

        for rect in &state.background_rects {
            rect.paint(self, state.origin, window);
        }
        if !state.images.is_empty() {
            // Clip: an image scrolled half off the top must not bleed above
            // the terminal area.
            window.with_content_mask(
                Some(ContentMask {
                    bounds: state.bounds,
                }),
                |window| {
                    for image in &state.images {
                        image.paint(self, state.origin, window);
                    }
                },
            );
        }
        for graphic in &state.graphics {
            graphic.paint(self, state.origin, window);
        }
        for line in &state.lines {
            line.paint(self, state.origin, window, cx);
        }
        for text_run in &state.text_runs {
            text_run.paint(self, state.origin, window, cx);
        }
        if let Some(cursor) = &state.cursor {
            cursor.paint(self, state.origin, window, cx);
        }
    }

    fn paint_marked_text(
        &self,
        state: &TerminalPaintState,
        marked_text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(cursor) = state.marked_text_cursor else {
            return;
        };
        if marked_text.is_empty() {
            return;
        }
        let origin = Point {
            x: state.origin.x + self.cell_width * cursor.column as f32,
            y: state.origin.y + self.cell_height * cursor.line as f32,
        };
        let fg = self.palette.foreground();
        let bg = self.palette.background();
        let run = TextRun {
            len: marked_text.len(),
            font: self.font(false, false),
            color: fg,
            background_color: None,
            underline: Some(UnderlineStyle {
                thickness: px(1.0),
                color: Some(fg),
                wavy: false,
            }),
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(marked_text.to_string()),
            self.font_size,
            &[run],
            None,
        );
        window.paint_quad(quad(
            Bounds {
                origin,
                size: Size {
                    width: self.cell_width * terminal_text_width(marked_text) as f32,
                    height: self.cell_height,
                },
            },
            px(0.0),
            bg,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
        let _ = shaped.paint(origin, self.cell_height, TextAlign::Left, None, window, cx);
    }
}

/// One originating terminal cell's byte range within a `TerminalTextRun`'s
/// `text`. A cell's text can hold more than one codepoint (a base character
/// plus combining/zero-width marks attached by the terminal model), so cells
/// must be shaped and painted as a whole unit at `col` rather than split by
/// Rust `char` — splitting would displace combining marks into the next
/// column instead of stacking them on the base glyph. `hash` is computed once
/// when the cell's text is appended so paint can hit the shape cache without
/// re-hashing every frame.
#[derive(Clone, Copy)]
struct TerminalTextSegment {
    col: usize,
    byte_start: usize,
    byte_len: usize,
    hash: u64,
}

#[derive(Clone)]
struct TerminalTextRun {
    row: usize,
    start_col: usize,
    width_cols: usize,
    text: String,
    style: TextRun,
    text_hash: u64,
    segments: Vec<TerminalTextSegment>,
}

impl TerminalTextRun {
    fn from_text(
        row: usize,
        start_col: usize,
        text: String,
        width_cols: usize,
        style: TextRun,
    ) -> Self {
        let text_hash = terminal_text_run_hash(&text);
        let segments = vec![TerminalTextSegment {
            col: start_col,
            byte_start: 0,
            byte_len: text.len(),
            hash: text_hash,
        }];
        Self {
            row,
            start_col,
            width_cols,
            text,
            style,
            text_hash,
            segments,
        }
    }

    fn can_append(
        &self,
        row: usize,
        col: usize,
        width_cols: usize,
        pending_spaces: usize,
        style: &TextRun,
    ) -> bool {
        self.row == row
            && self.start_col + self.width_cols + pending_spaces == col
            && width_cols == 1
            && self.width_cols == self.text.chars().count()
            && self.style.font == style.font
            && self.style.color == style.color
            && self.style.background_color == style.background_color
            && self.style.underline == style.underline
            && self.style.strikethrough == style.strikethrough
    }

    fn append_spaces(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        self.text.extend(std::iter::repeat_n(' ', count));
        self.width_cols += count;
        self.style.len += count;
        self.text_hash = terminal_text_run_hash(&self.text);
    }

    fn append_text(&mut self, text: &str, width_cols: usize) {
        let byte_start = self.text.len();
        let col = self.start_col + self.width_cols;
        let hash = terminal_text_run_hash(text);
        self.text.push_str(text);
        self.segments.push(TerminalTextSegment {
            col,
            byte_start,
            byte_len: text.len(),
            hash,
        });
        self.width_cols += width_cols;
        self.style.len += text.len();
        self.text_hash = terminal_text_run_hash(&self.text);
    }

    fn paint(
        &self,
        renderer: &TerminalRenderer,
        origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.text.is_ascii() {
            let run = TextRun {
                len: self.text.len(),
                ..self.style.clone()
            };
            let text = self.text.as_str();
            let shaped = window.text_system().shape_line_by_hash(
                self.text_hash,
                text.len(),
                renderer.font_size,
                &[run],
                None,
                || SharedString::from(text.to_string()),
            );
            let _ = shaped.paint(
                Point {
                    x: origin.x + renderer.cell_width * self.start_col as f32,
                    y: origin.y + renderer.cell_height * self.row as f32,
                },
                renderer.cell_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
            return;
        }
        // Non-ASCII text may trigger font fallback with advances that differ
        // from cell_width, so each originating cell is shaped and positioned
        // individually rather than as one natural-advance line. Painting by
        // `segments` (one per cell) rather than by `char` keeps a cell's base
        // character and any combining marks together as a single shaped unit.
        for segment in &self.segments {
            let chunk = &self.text[segment.byte_start..segment.byte_start + segment.byte_len];
            let mut run = self.style.clone();
            run.len = chunk.len();
            let shaped = window.text_system().shape_line_by_hash(
                segment.hash,
                chunk.len(),
                renderer.font_size,
                &[run],
                None,
                || SharedString::from(chunk.to_string()),
            );
            let _ = shaped.paint(
                Point {
                    x: origin.x + renderer.cell_width * segment.col as f32,
                    y: origin.y + renderer.cell_height * self.row as f32,
                },
                renderer.cell_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
    }
}

fn terminal_text_run_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// A maximal run of a row's "simple" spans (single-codepoint, width-1,
/// ASCII glyphs) painted as a single shaped line carrying multiple style
/// runs, so GPUI shapes and paints them once instead of once per style span.
/// A row can contain several of these interleaved with per-span spans (wide
/// CJK or multi-codepoint cells) that must keep the per-span path to hold
/// grid alignment — one such span no longer prevents the rest of the row
/// from combining.
#[derive(Clone)]
struct TerminalRowLine {
    row: usize,
    start_col: usize,
    text: String,
    runs: Vec<TextRun>,
    layout_hash: u64,
}

impl TerminalRowLine {
    fn paint(
        &self,
        renderer: &TerminalRenderer,
        origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.text.is_empty() || self.runs.is_empty() {
            return;
        }
        let shaped = window.text_system().shape_line_by_hash(
            self.layout_hash,
            self.text.len(),
            renderer.font_size,
            &self.runs,
            None,
            || SharedString::from(self.text.clone()),
        );
        let _ = shaped.paint(
            Point {
                x: origin.x + renderer.cell_width * self.start_col as f32,
                y: origin.y + renderer.cell_height * self.row as f32,
            },
            renderer.cell_height,
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

fn terminal_row_line_layout_hash(text: &str, runs: &[TextRun]) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    for run in runs {
        // Glyph layout depends on text + font (family/weight/style) + run
        // length; colour, underline and strikethrough are applied at paint time
        // and do not affect advances, so they are excluded for better reuse.
        run.len.hash(&mut hasher);
        run.font.family.hash(&mut hasher);
        run.font.weight.0.to_bits().hash(&mut hasher);
        matches!(run.font.style, FontStyle::Italic).hash(&mut hasher);
    }
    hasher.finish()
}

impl TerminalRenderer {
    /// Combine a row's already-built per-span runs into as few shaped lines as
    /// possible: maximal contiguous groups of 1:1 char-to-cell ASCII spans are
    /// merged into a `TerminalRowLine` each (so those spans are shaped/painted
    /// once instead of once per span), while any wide (CJK), multi-codepoint,
    /// or non-ASCII span is returned unmodified in the leftover run list to
    /// keep the per-span path — without forcing the rest of the row's simple
    /// spans to give up combining too.
    fn combine_row_runs(
        &self,
        runs: Vec<TerminalTextRun>,
    ) -> (Vec<TerminalRowLine>, Vec<TerminalTextRun>) {
        combine_terminal_row_runs(runs, self.font(false, false), self.palette.foreground())
    }
}

fn combine_terminal_row_runs(
    runs: Vec<TerminalTextRun>,
    gap_font: Font,
    gap_color: Hsla,
) -> (Vec<TerminalRowLine>, Vec<TerminalTextRun>) {
    let mut lines = Vec::new();
    let mut leftover = Vec::new();
    let mut group: Vec<TerminalTextRun> = Vec::new();

    for run in runs {
        // Only 1:1 char-to-cell ASCII spans are safe to re-flow as one shaped
        // line by natural glyph advances; wide (CJK) or multi-codepoint cells
        // keep their own grid columns, non-ASCII text may trigger font
        // fallback with advances that differ from cell_width, and symbol-font
        // glyph advances don't match the primary font.
        if run.text.chars().count() == run.width_cols
            && run.text.is_ascii()
            && run.style.font.family.as_ref() != TERMINAL_SYMBOL_FONT_FAMILY
        {
            group.push(run);
            continue;
        }
        flush_combinable_group(&mut group, &gap_font, gap_color, &mut lines, &mut leftover);
        leftover.push(run);
    }
    flush_combinable_group(&mut group, &gap_font, gap_color, &mut lines, &mut leftover);

    (lines, leftover)
}

/// Turns a maximal group of combinable spans into one `TerminalRowLine`
/// (dropping the group), or moves it to `leftover` unchanged when it's too
/// small to be worth combining (or, defensively, if building the line fails).
fn flush_combinable_group(
    group: &mut Vec<TerminalTextRun>,
    gap_font: &Font,
    gap_color: Hsla,
    lines: &mut Vec<TerminalRowLine>,
    leftover: &mut Vec<TerminalTextRun>,
) {
    if group.len() < 2 {
        leftover.append(group);
        return;
    }
    match build_terminal_row_line(group, gap_font, gap_color) {
        Some(line) => {
            lines.push(line);
            group.clear();
        }
        None => leftover.append(group),
    }
}

fn build_terminal_row_line(
    runs: &[TerminalTextRun],
    gap_font: &Font,
    gap_color: Hsla,
) -> Option<TerminalRowLine> {
    let first = runs.first()?;
    let row = first.row;
    let start_col = first.start_col;
    let mut text = String::new();
    let mut text_runs: Vec<TextRun> = Vec::new();
    let mut next_col = start_col;
    for run in runs {
        if run.start_col > next_col {
            let gap = run.start_col - next_col;
            text.extend(std::iter::repeat_n(' ', gap));
            text_runs.push(TextRun {
                len: gap,
                font: gap_font.clone(),
                color: gap_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
        text.push_str(&run.text);
        let mut style = run.style.clone();
        style.len = run.text.len();
        text_runs.push(style);
        next_col = run.start_col + run.width_cols;
    }
    if text.is_empty() {
        return None;
    }
    debug_assert_eq!(
        text_runs.iter().map(|run| run.len).sum::<usize>(),
        text.len(),
        "combined run lengths must cover the line text exactly"
    );
    let layout_hash = terminal_row_line_layout_hash(&text, &text_runs);
    Some(TerminalRowLine {
        row,
        start_col,
        text,
        runs: text_runs,
        layout_hash,
    })
}
