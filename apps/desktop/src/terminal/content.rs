#[derive(Clone)]
struct TerminalContent {
    cells: Vec<TerminalIndexedCell>,
    colors: Colors,
    colors_hash: u64,
    cursor: RenderableCursor,
    cursor_char: char,
    mode: TermMode,
    display_offset: usize,
    columns: usize,
    screen_lines: usize,
    total_lines: usize,
    #[cfg(test)]
    scrolled_to_bottom: bool,
}

impl TerminalContent {
    fn from_term(term: &Term<GpuiEventProxy>) -> Self {
        let content = term.renderable_content();
        let mut cells = Vec::with_capacity(content.display_iter.size_hint().0);
        cells.extend(content.display_iter.map(|indexed| TerminalIndexedCell {
            point: indexed.point,
            cell: indexed.cell.clone(),
        }));
        Self {
            cells,
            colors: *content.colors,
            colors_hash: terminal_colors_hash(content.colors),
            cursor: content.cursor,
            cursor_char: term.grid()[content.cursor.point].c,
            mode: content.mode,
            display_offset: content.display_offset,
            columns: term.columns(),
            screen_lines: term.screen_lines(),
            total_lines: term.grid().total_lines(),
            #[cfg(test)]
            scrolled_to_bottom: content.display_offset == 0,
        }
    }
}

#[derive(Clone)]
struct TerminalIndexedCell {
    point: TerminalPoint,
    cell: Cell,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TerminalLink {
    url: String,
    line: i32,
    range: Range<usize>,
}

fn terminal_link_at_cell(
    content: &TerminalContent,
    point: TerminalCellPoint,
) -> Option<TerminalLink> {
    let line = point.row as i32 - content.display_offset as i32;
    let row_cells: Vec<&TerminalIndexedCell> = content
        .cells
        .iter()
        .filter(|indexed| indexed.point.line.0 == line)
        .collect();
    if row_cells.is_empty() {
        return None;
    }

    if let Some(cell) = row_cells
        .iter()
        .find(|indexed| indexed.point.column.0 == point.col)
        && let Some(hyperlink) = cell.hyperlink()
    {
        let url = hyperlink.uri().to_string();
        if is_openable_terminal_url(&url) {
            let range = terminal_hyperlink_range(&row_cells, point.col, hyperlink.uri());
            return Some(TerminalLink { url, line, range });
        }
    }

    let row_text = terminal_row_text(&row_cells);
    terminal_plain_url_at(&row_text, point.col).map(|(url, range)| TerminalLink {
        url,
        line,
        range,
    })
}

fn terminal_hyperlink_range(
    row_cells: &[&TerminalIndexedCell],
    col: usize,
    uri: &str,
) -> Range<usize> {
    let mut start = col;
    let mut end = col.saturating_add(1);
    for indexed in row_cells {
        if indexed
            .hyperlink()
            .is_some_and(|hyperlink| hyperlink.uri() == uri)
        {
            let cell_col = indexed.point.column.0;
            let width = terminal_cell_width(&indexed.cell);
            start = start.min(cell_col);
            end = end.max(cell_col.saturating_add(width));
        }
    }
    start..end
}

fn terminal_row_text(row_cells: &[&TerminalIndexedCell]) -> Vec<(usize, char)> {
    let mut text: Vec<(usize, char)> = Vec::new();
    for indexed in row_cells {
        let col = indexed.point.column.0;
        if indexed
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            || indexed.c == '\0'
        {
            continue;
        }
        let next_col = text
            .last()
            .map(|(last_col, last_ch)| last_col.saturating_add(terminal_char_width(*last_ch)))
            .unwrap_or(0);
        for spacer_col in next_col..col {
            text.push((spacer_col, ' '));
        }
        text.push((col, indexed.c));
    }
    text
}

fn terminal_plain_url_at(row_text: &[(usize, char)], col: usize) -> Option<(String, Range<usize>)> {
    static STRICT_URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)(?:https?|file)://[^\s"'!*(){}|\\^<>`]*[^\s"':,.!?{}|\\^~\[\]`()<>]"#)
            .expect("valid terminal URL regex")
    });

    let text: String = row_text.iter().map(|(_, ch)| *ch).collect();
    for candidate in STRICT_URL_REGEX.find_iter(&text) {
        let start = candidate.start();
        let end = candidate.end();
        let start_index = text[..start].chars().count();
        let end_index = text[..end].chars().count();
        let Some(start_col) = row_text.get(start_index).map(|(col, _)| *col) else {
            continue;
        };
        let end_col = row_text
            .get(end_index.saturating_sub(1))
            .map(|(col, ch)| col.saturating_add(terminal_char_width(*ch)))
            .unwrap_or(start_col);
        if start_col <= col && col < end_col {
            let url = candidate.as_str().to_string();
            if is_openable_terminal_url(&url) {
                return Some((url, start_col..end_col));
            }
        }
    }
    None
}

fn is_openable_terminal_url(url: &str) -> bool {
    url::Url::parse(url)
        .map(|url| matches!(url.scheme(), "http" | "https" | "file"))
        .unwrap_or(false)
}

fn terminal_cell_width(cell: &Cell) -> usize {
    if cell.flags.contains(Flags::WIDE_CHAR) {
        2
    } else {
        1
    }
}

fn terminal_char_width(ch: char) -> usize {
    if ch.is_ascii() { 1 } else { 2 }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DisplayCursor {
    row: i32,
    col: usize,
}

impl DisplayCursor {
    fn from(cursor_point: TerminalPoint, display_offset: usize) -> Self {
        Self {
            row: cursor_point.line.0 + display_offset as i32,
            col: cursor_point.column.0,
        }
    }
}

fn terminal_colors_hash(colors: &Colors) -> u64 {
    let mut hasher = DefaultHasher::new();
    for index in 0..alacritty_terminal::term::color::COUNT {
        terminal_optional_rgb_hash(colors[index], &mut hasher);
    }
    hasher.finish()
}

fn terminal_optional_rgb_hash(rgb: Option<Rgb>, hasher: &mut DefaultHasher) {
    match rgb {
        Some(rgb) => {
            1u8.hash(hasher);
            rgb.r.hash(hasher);
            rgb.g.hash(hasher);
            rgb.b.hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn terminal_color_hash(color: Color, hasher: &mut DefaultHasher) {
    match color {
        Color::Named(named) => {
            0u8.hash(hasher);
            (named as usize).hash(hasher);
        }
        Color::Spec(rgb) => {
            1u8.hash(hasher);
            rgb.r.hash(hasher);
            rgb.g.hash(hasher);
            rgb.b.hash(hasher);
        }
        Color::Indexed(index) => {
            2u8.hash(hasher);
            index.hash(hasher);
        }
    }
}

fn terminal_optional_color_hash(color: Option<Color>, hasher: &mut DefaultHasher) {
    match color {
        Some(color) => {
            1u8.hash(hasher);
            terminal_color_hash(color, hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn terminal_cell_hash(cell: &Cell, hasher: &mut DefaultHasher) {
    cell.c.hash(hasher);
    terminal_color_hash(cell.fg, hasher);
    terminal_color_hash(cell.bg, hasher);
    cell.flags.hash(hasher);
    if let Some(zerowidth) = cell.zerowidth() {
        zerowidth.hash(hasher);
    }
    terminal_optional_color_hash(cell.underline_color(), hasher);
    if let Some(hyperlink) = cell.hyperlink() {
        hyperlink.id().hash(hasher);
        hyperlink.uri().hash(hasher);
    }
}

fn terminal_row_hash(cells: &[TerminalIndexedCell], colors_hash: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    colors_hash.hash(&mut hasher);
    for indexed in cells {
        indexed.point.column.0.hash(&mut hasher);
        terminal_cell_hash(&indexed.cell, &mut hasher);
    }
    hasher.finish()
}

impl std::ops::Deref for TerminalIndexedCell {
    type Target = Cell;

    fn deref(&self) -> &Self::Target {
        &self.cell
    }
}
