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

#[derive(Clone)]
struct TerminalFonts {
    normal: Font,
    bold: Font,
    italic: Font,
    bold_italic: Font,
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
        bounds: Bounds<Pixels>,
        padding: Edges<Pixels>,
        content: &TerminalContent,
        selection: Option<SelectionRange>,
        hover_link: Option<&TerminalLink>,
        cursor_visible: bool,
        cursor_focused: bool,
        window: &mut Window,
    ) -> TerminalPaintState {
        let colors = &content.colors;
        let default_bg = self
            .palette
            .resolve(Color::Named(NamedColor::Background), colors);
        let origin = Point {
            x: bounds.origin.x + padding.left,
            y: bounds.origin.y + padding.top,
        };
        let content_right = bounds.origin.x + bounds.size.width - padding.right;
        let display_offset = content.display_offset as i32;
        let visible_rows = self.visible_row_range(bounds, padding, content, window);

        let mut background_rects = Vec::new();
        let mut text_runs = Vec::new();
        let mut cursor_cell = None;
        let cursor_row = content.cursor.point.line.0;
        let cursor_col = content.cursor.point.column.0;
        let cache_key = self.cache_key();
        let mut index = 0usize;
        while index < content.cells.len() {
            let line = content.cells[index].point.line;
            let row = line.0 + display_offset;
            let start = index;
            while index < content.cells.len() && content.cells[index].point.line == line {
                index += 1;
            }
            if row < 0 {
                continue;
            }
            let row = row as usize;
            if row < visible_rows.start || row >= visible_rows.end {
                continue;
            }
            let cells = &content.cells[start..index];
            let prepared = self.prepare_cached_row(
                row,
                cells,
                colors,
                content.colors_hash,
                default_bg,
                cache_key,
            );
            if let Some(hover_link) = hover_link
                && hover_link.line == line.0
            {
                self.prepare_row_text(
                    row,
                    cells,
                    colors,
                    &mut text_runs,
                    Some(hover_link.range.clone()),
                );
                background_rects.extend(prepared.background_rects);
            } else {
                background_rects.extend(prepared.background_rects);
                text_runs.extend(prepared.text_runs);
            }
            if cursor_row + display_offset == row as i32 {
                cursor_cell = cells
                    .iter()
                    .find(|indexed| indexed.point.column.0 == cursor_col)
                    .map(|indexed| &indexed.cell);
            }
        }

        if let Some(selection) = selection {
            for row in visible_rows.clone() {
                let line = Line(row as i32 - display_offset);
                self.prepare_selection(
                    line,
                    row,
                    origin,
                    content.columns,
                    content_right,
                    selection,
                    &mut background_rects,
                );
            }
        }

        let display_cursor = DisplayCursor::from(content.cursor.point, content.display_offset);
        let cursor_on_visible_row = display_cursor.row >= 0
            && (display_cursor.row as usize) < content.screen_lines
            && display_cursor.col < content.columns
            && (visible_rows.start..visible_rows.end).contains(&(display_cursor.row as usize));
        let cursor = (cursor_visible
            && content.mode.contains(TermMode::SHOW_CURSOR)
            && content.cursor.shape != CursorShape::Hidden
            && cursor_on_visible_row)
            .then(|| {
                let shape = if cursor_focused {
                    content.cursor.shape
                } else {
                    CursorShape::HollowBlock
                };
                let row = display_cursor.row as usize;
                let col = display_cursor.col;
                let cursor_width = self.cursor_width(cursor_cell, default_bg, window);
                let text_run = cursor_cell
                    .filter(|cell| {
                        cursor_focused
                            && content.cursor.shape == CursorShape::Block
                            && content.cursor_char != '\0'
                            && !cell.flags.contains(Flags::WIDE_CHAR_SPACER)
                    })
                    .map(|cell| {
                        let font = self.font(
                            cell.flags.contains(Flags::BOLD),
                            cell.flags.contains(Flags::ITALIC),
                        );
                        TerminalTextRun::new(
                            row,
                            col,
                            content.cursor_char,
                            if cell.flags.contains(Flags::WIDE_CHAR) {
                                2
                            } else {
                                1
                            },
                            TextRun {
                                len: cell.c.len_utf8(),
                                font,
                                color: default_bg,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            },
                        )
                    });

                TerminalCursorPaint {
                    point: content.cursor.point,
                    display_row: row,
                    shape,
                    color: self
                        .palette
                        .resolve(Color::Named(NamedColor::Cursor), colors),
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

        TerminalPaintState {
            bounds,
            origin,
            background: default_bg,
            background_rects,
            text_runs,
            cursor,
            marked_text_cursor: cursor_on_visible_row.then_some(TerminalPoint::new(
                Line(display_cursor.row),
                Column(display_cursor.col),
            )),
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
        if content.screen_lines == 0 {
            return 0..0;
        }
        let content_bounds = Bounds {
            origin: Point {
                x: bounds.origin.x + padding.left,
                y: bounds.origin.y + padding.top,
            },
            size: Size {
                width: self.cell_width * content.columns.max(1) as f32,
                height: self.cell_height * content.screen_lines as f32,
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
        let start = start.min(content.screen_lines);
        let end = start.saturating_add(count).min(content.screen_lines);
        start..end
    }

    fn cursor_width(
        &self,
        cursor_cell: Option<&Cell>,
        default_bg: Hsla,
        window: &mut Window,
    ) -> Pixels {
        let Some(cell) = cursor_cell else {
            return self.cell_width;
        };
        if cell.c == '\0' || cell.c.is_whitespace() || cell.flags.contains(Flags::WIDE_CHAR_SPACER)
        {
            return self.cell_width;
        }

        let font = self.font(
            cell.flags.contains(Flags::BOLD),
            cell.flags.contains(Flags::ITALIC),
        );
        let text = cell.c.to_string();
        let shaped = window.text_system().shape_line(
            SharedString::from(text),
            self.font_size,
            &[TextRun {
                len: cell.c.len_utf8(),
                font,
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
        colors: &Colors,
        colors_hash: u64,
        default_bg: Hsla,
        font_key: TerminalRendererCacheKey,
    ) -> TerminalPreparedRow {
        let row_hash = terminal_row_hash(cells, colors_hash);
        let key = TerminalRowCacheKey { row_hash, font_key };
        if let Some(prepared) = self.cache.lock().rows.get(&key).cloned() {
            return prepared.for_display_row(row);
        }

        let mut background_rects = Vec::new();
        let mut text_runs = Vec::new();
        self.prepare_row_backgrounds(0, cells, colors, default_bg, &mut background_rects);
        self.prepare_row_text(0, cells, colors, &mut text_runs, None);
        let prepared = TerminalPreparedRow {
            background_rects,
            text_runs,
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
        colors: &Colors,
        default_bg: Hsla,
        background_rects: &mut Vec<TerminalBackgroundRect>,
    ) {
        let mut current: Option<TerminalBackgroundRect> = None;
        for indexed in cells {
            let col = indexed.point.column.0;
            let bg = self.cell_render_colors(&indexed.cell, colors).1;
            let width_cols = if indexed.cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
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
        colors: &Colors,
        text_runs: &mut Vec<TerminalTextRun>,
        underline_range: Option<Range<usize>>,
    ) {
        let mut current_run: Option<TerminalTextRun> = None;
        let mut pending_spaces = 0usize;
        let mut next_col = 0usize;
        for indexed in cells {
            let col = indexed.point.column.0;
            let cell = &indexed.cell;
            if col > next_col {
                pending_spaces = 0;
            }
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) || cell.c == '\0' {
                pending_spaces = 0;
                next_col = col.saturating_add(1);
                continue;
            }
            if cell.c == ' ' {
                if current_run.is_some() {
                    pending_spaces += 1;
                }
                next_col = col.saturating_add(1);
                continue;
            }

            let (fg, _) = self.cell_render_colors(cell, colors);
            let font = self.font(
                cell.flags.contains(Flags::BOLD),
                cell.flags.contains(Flags::ITALIC),
            );
            let text = cell.c.to_string();
            let link_underline = underline_range
                .as_ref()
                .is_some_and(|range| range.contains(&col));
            let underline = cell.flags.contains(Flags::UNDERLINE) || link_underline;
            let run = TextRun {
                len: text.len(),
                font,
                color: fg,
                background_color: None,
                underline: underline.then_some(UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(fg),
                    wavy: link_underline,
                }),
                strikethrough: None,
            };
            let cell_width = if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
            if current_run.as_ref().is_some_and(|current| {
                current.can_append(row, col, cell_width, pending_spaces, &run)
            }) {
                if let Some(current) = current_run.as_mut() {
                    current.append_spaces(pending_spaces);
                    current.append(cell.c, cell_width);
                }
            } else {
                if let Some(current) = current_run.take() {
                    text_runs.push(current);
                }
                current_run = Some(TerminalTextRun::new(row, col, cell.c, cell_width, run));
            }
            pending_spaces = 0;
            next_col = col.saturating_add(cell_width);
        }

        if let Some(current) = current_run {
            text_runs.push(current);
        }
    }

    fn cell_render_colors(&self, cell: &Cell, colors: &Colors) -> (Hsla, Hsla) {
        let mut fg = cell.fg;
        let mut bg = cell.bg;
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if cell.flags.contains(Flags::BOLD)
            && let Color::Named(named) = fg
        {
            let index = named as usize;
            if index < 8 {
                fg = Color::Named(ansi_named_color(index + 8));
            }
        }
        (
            self.palette.resolve(fg, colors),
            self.palette.resolve(bg, colors),
        )
    }

    fn prepare_selection(
        &self,
        line: Line,
        row: usize,
        origin: Point<Pixels>,
        columns: usize,
        content_right: Pixels,
        selection: SelectionRange,
        background_rects: &mut Vec<TerminalBackgroundRect>,
    ) {
        if line.0 < selection.start.line || line.0 > selection.end.line {
            return;
        }

        let start_col = if line.0 == selection.start.line {
            selection.start.col
        } else {
            0
        };
        let end_col = if line.0 == selection.end.line {
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
        window.paint_quad(quad(
            state.bounds,
            px(0.0),
            state.background,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));

        for rect in &state.background_rects {
            rect.paint(self, state.origin, window);
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
            x: state.origin.x + self.cell_width * cursor.column.0 as f32,
            y: state.origin.y + self.cell_height * cursor.line.0 as f32,
        };
        let fg = self.palette.foreground;
        let bg = self.palette.background;
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

#[derive(Clone)]
struct TerminalTextRun {
    row: usize,
    start_col: usize,
    width_cols: usize,
    text: String,
    style: TextRun,
    text_hash: u64,
}

impl TerminalTextRun {
    fn new(row: usize, start_col: usize, c: char, width_cols: usize, style: TextRun) -> Self {
        let mut hasher = DefaultHasher::new();
        c.hash(&mut hasher);
        Self {
            row,
            start_col,
            width_cols,
            text: c.to_string(),
            style,
            text_hash: hasher.finish(),
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
        for _ in 0..count {
            self.append(' ', 1);
        }
    }

    fn append(&mut self, c: char, width_cols: usize) {
        let mut hasher = DefaultHasher::new();
        self.text_hash.hash(&mut hasher);
        c.hash(&mut hasher);
        self.text_hash = hasher.finish();
        self.text.push(c);
        self.width_cols += width_cols;
        self.style.len += c.len_utf8();
    }

    fn paint(
        &self,
        renderer: &TerminalRenderer,
        origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
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
    }
}
