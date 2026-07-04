#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
struct TerminalPoint {
    line: i32,
    column: usize,
}

/// Per-line content hashes, computed once when the content is built (i.e. once
/// per published snapshot, not per frame) so the renderer can key its row cache
/// without re-hashing every visible row on every frame. Derived purely from
/// `cells`, so it is excluded from content identity (see `PartialEq`).
#[derive(Clone, Default)]
struct PrecomputedRowHashes(HashMap<i32, u64>);

impl PartialEq for PrecomputedRowHashes {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl PrecomputedRowHashes {
    fn from_cells(cells: &[TerminalIndexedCell]) -> Self {
        let mut hashers: HashMap<i32, DefaultHasher> = HashMap::new();
        for indexed in cells {
            terminal_cell_hash(
                &indexed.cell,
                hashers.entry(indexed.point.line).or_default(),
            );
        }
        Self(
            hashers
                .into_iter()
                .map(|(line, hasher)| (line, hasher.finish()))
                .collect(),
        )
    }

    fn get(&self, line: i32) -> Option<u64> {
        self.0.get(&line).copied()
    }
}

#[derive(Clone, PartialEq)]
struct TerminalContent {
    cells: Vec<TerminalIndexedCell>,
    row_hashes: PrecomputedRowHashes,
    cursor: TerminalScreenCursorSnapshot,
    display_offset: usize,
    viewport_start_line: i32,
    columns: usize,
    screen_lines: usize,
    total_lines: usize,
    visible_rows: usize,
    visible_row_shift: usize,
    input_mode: TerminalInputMode,
    title: Option<String>,
    #[cfg(test)]
    scrolled_to_bottom: bool,
}

impl TerminalContent {
    fn from_screen_snapshot(snapshot: TerminalScreenSnapshot) -> Self {
        let total_lines = snapshot.total_lines.max(snapshot.rows);
        let viewport_start_line = total_lines
            .saturating_sub(snapshot.display_offset)
            .saturating_sub(snapshot.rows) as i32;
        let cells = snapshot
            .cells
            .into_iter()
            .map(|cell| TerminalIndexedCell {
                point: TerminalPoint {
                    line: viewport_start_line + cell.row,
                    column: cell.col,
                },
                cell,
            })
            .collect::<Vec<_>>();
        let row_hashes = PrecomputedRowHashes::from_cells(&cells);
        Self {
            cells,
            row_hashes,
            cursor: snapshot.cursor,
            display_offset: snapshot.display_offset,
            viewport_start_line,
            columns: snapshot.cols,
            screen_lines: snapshot.rows,
            total_lines,
            visible_rows: snapshot.rows,
            visible_row_shift: 0,
            input_mode: snapshot.input_mode,
            title: snapshot.title,
            #[cfg(test)]
            scrolled_to_bottom: snapshot.display_offset == 0,
        }
    }

    fn with_visible_row_shift(mut self, visible_rows: usize) -> Self {
        self.visible_rows = visible_rows.min(self.screen_lines);
        self.visible_row_shift = self.screen_lines.saturating_sub(self.visible_rows);
        self
    }

    fn visible_rows(&self) -> usize {
        self.visible_rows
    }

    fn display_row_for_line(&self, line: i32) -> Option<usize> {
        let row = line - self.viewport_start_line - self.visible_row_shift as i32;
        if row < 0 || row as usize >= self.visible_rows {
            return None;
        }
        Some(row as usize)
    }

    fn line_for_display_row(&self, row: usize) -> i32 {
        self.viewport_start_line + row as i32 + self.visible_row_shift as i32
    }

    fn line_in_snapshot(&self, line: i32) -> bool {
        let start = self.viewport_start_line;
        let end = self.last_snapshot_line().unwrap_or(start);
        start <= line && line <= end
    }

    fn last_snapshot_line(&self) -> Option<i32> {
        self.screen_lines
            .checked_sub(1)
            .map(|row| self.viewport_start_line + row as i32)
    }

    fn display_cursor(&self) -> DisplayCursor {
        DisplayCursor {
            row: self.cursor.row as i32,
            col: self.cursor.col,
        }
        .shifted(self.visible_row_shift)
    }
}

#[derive(Clone, PartialEq)]
struct TerminalIndexedCell {
    point: TerminalPoint,
    cell: TerminalScreenCellSnapshot,
}

impl TerminalIndexedCell {
    fn col(&self) -> usize {
        self.point.column
    }

    fn line(&self) -> i32 {
        self.point.line
    }

    fn text(&self) -> &str {
        &self.cell.text
    }

    fn is_spacer_or_empty(&self) -> bool {
        self.cell.hidden || self.cell.text.is_empty()
    }
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
    let line = content.line_for_display_row(point.row);
    let row_cells: Vec<&TerminalIndexedCell> = content
        .cells
        .iter()
        .filter(|indexed| indexed.line() == line)
        .collect();
    if row_cells.is_empty() {
        return None;
    }

    let row_text = terminal_row_text(&row_cells);
    terminal_plain_url_at(&row_text, point.col).map(|(url, range)| TerminalLink {
        url,
        line,
        range,
    })
}

fn terminal_row_text(row_cells: &[&TerminalIndexedCell]) -> Vec<(usize, char)> {
    let mut text: Vec<(usize, char)> = Vec::new();
    for indexed in row_cells {
        let col = indexed.col();
        if indexed.is_spacer_or_empty() {
            continue;
        }
        let next_col = text
            .last()
            .map(|(last_col, last_ch)| last_col.saturating_add(terminal_char_width(*last_ch)))
            .unwrap_or(0);
        for spacer_col in next_col..col {
            text.push((spacer_col, ' '));
        }
        for (offset, ch) in indexed.text().chars().enumerate() {
            text.push((col + offset, ch));
        }
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

fn terminal_char_width(ch: char) -> usize {
    if ch.is_ascii() { 1 } else { 2 }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DisplayCursor {
    row: i32,
    col: usize,
}

impl DisplayCursor {
    fn shifted(self, row_shift: usize) -> Self {
        Self {
            row: self.row - row_shift as i32,
            col: self.col,
        }
    }
}

fn terminal_cell_hash(cell: &TerminalScreenCellSnapshot, hasher: &mut DefaultHasher) {
    cell.col.hash(hasher);
    cell.text.hash(hasher);
    cell.width.hash(hasher);
    terminal_screen_color_hash(&cell.fg, hasher);
    terminal_screen_color_hash(&cell.bg, hasher);
    cell.bold.hash(hasher);
    cell.dim.hash(hasher);
    cell.italic.hash(hasher);
    cell.underline.hash(hasher);
    cell.inverse.hash(hasher);
    cell.hidden.hash(hasher);
    cell.strikeout.hash(hasher);
}

fn terminal_screen_color_hash(color: &TerminalScreenColor, hasher: &mut DefaultHasher) {
    match color {
        TerminalScreenColor::Default => 0u8.hash(hasher),
        TerminalScreenColor::Named { name } => {
            1u8.hash(hasher);
            name.hash(hasher);
        }
        TerminalScreenColor::Rgb { r, g, b } => {
            2u8.hash(hasher);
            r.hash(hasher);
            g.hash(hasher);
            b.hash(hasher);
        }
        TerminalScreenColor::Indexed { index } => {
            3u8.hash(hasher);
            index.hash(hasher);
        }
    }
}

fn terminal_row_hash(cells: &[TerminalIndexedCell]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for indexed in cells {
        terminal_cell_hash(&indexed.cell, &mut hasher);
    }
    hasher.finish()
}
