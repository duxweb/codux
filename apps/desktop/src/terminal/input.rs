#[derive(Clone)]
struct TerminalInputHandler {
    model: Entity<TerminalModel>,
    layout: Arc<Mutex<TerminalLayoutMetrics>>,
    terminal_view: WeakEntity<TerminalView>,
}

impl TerminalInputHandler {
    fn send_filtered_input(&self, text: &str, window: &mut Window, cx: &mut App) {
        // The IME text channel carries committed text only. A navigation key
        // (arrows/home/end) is sometimes mis-delivered here as its escape
        // sequence -- as caret notation ("^[OA") or a real ESC -- in addition
        // to the keystroke path that already encodes and sends it. Writing the
        // caret-notation form verbatim makes the shell echo a literal "^[OA"
        // even though the keystroke path already recalled history. Drop it: the
        // keystroke path owns real keys. This mirrors the marked-text guard in
        // `terminal_input_marked_text`.
        if terminal_text_input_should_drop(text) {
            return;
        }
        if self
            .terminal_view
            .update(cx, |view, _| view.take_suppressed_text_input_echo(text))
            .unwrap_or(false)
        {
            return;
        }
        let bytes = codux_terminal_core::terminal_text_input_bytes(text);
        if bytes.is_empty() {
            return;
        }
        self.model.update(cx, |model, cx| {
            model.prepare_input_viewport(cx);
            model.write_bytes(&bytes);
        });
        window.invalidate_character_coordinates();
    }
}

impl InputHandler for TerminalInputHandler {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.terminal_view
            .read_with(cx, |view, _| view.marked_text_range())
            .ok()
            .flatten()
    }

    fn text_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        None
    }

    fn replace_text_in_range(
        &mut self,
        _replacement_range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        let _ = self.terminal_view.update(cx, |view, cx| {
            view.blink_manager
                .update(cx, TerminalBlinkManager::pause_blinking);
            view.clear_pending_view_scroll();
            view.clear_marked_text(cx);
        });
        self.send_filtered_input(text, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.model
            .update(cx, |model, cx| model.prepare_input_viewport(cx));
        let _ = self.terminal_view.update(cx, |view, cx| {
            view.clear_pending_view_scroll();
            view.set_marked_text(terminal_input_marked_text(new_text), cx)
        });
        window.invalidate_character_coordinates();
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut App) {
        let _ = self
            .terminal_view
            .update(cx, |view, cx| view.clear_marked_text(cx));
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.lock();
        let cursor_bounds = self
            .model
            .read(cx)
            .current_ime_cursor_bounds(&layout)
            .or_else(|| layout.last_ime_cursor_bounds())
            .or_else(|| layout.first_cell_ime_bounds());
        ime_bounds_for_range(cursor_bounds, &layout, range_utf16)
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        Some(0)
    }

    fn accepts_text_input(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
    }

    fn prefers_ime_for_printable_keys(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
    }
}

fn terminal_input_marked_text(text: &str) -> String {
    if terminal_marked_text_looks_like_escape_sequence(text) {
        return String::new();
    }
    text.chars()
        .filter(|ch| {
            !ch.is_control()
                && !('\u{F700}'..='\u{F8FF}').contains(ch)
                && !('\u{2400}'..='\u{2426}').contains(ch)
        })
        .collect()
}

fn terminal_text_input_should_drop(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    if terminal_marked_text_looks_like_escape_sequence(text) {
        return true;
    }
    if text.starts_with("\u{1b}[200~") && text.ends_with("\u{1b}[201~") {
        return true;
    }
    text.chars()
        .all(|ch| ch.is_control() || ('\u{F700}'..='\u{F8FF}').contains(&ch))
}

fn terminal_marked_text_looks_like_escape_sequence(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with('\u{1b}')
        || trimmed.starts_with("^[")
        || trimmed.starts_with("␛")
        || trimmed.starts_with("\\e")
        || trimmed.starts_with("\\x1b")
}

fn ime_bounds_for_range(
    cursor_bounds: Option<Bounds<Pixels>>,
    layout: &TerminalLayoutMetrics,
    range_utf16: Range<usize>,
) -> Option<Bounds<Pixels>> {
    let mut bounds = cursor_bounds?;
    bounds.origin.x += layout.cell_width * range_utf16.start as f32;
    Some(bounds)
}

fn ime_cursor_bounds_from_content(
    content: &TerminalContent,
    layout: &TerminalLayoutMetrics,
) -> Option<Bounds<Pixels>> {
    if content.screen_lines == 0 || content.columns == 0 || layout.rows == 0 || layout.cols == 0 {
        return None;
    }
    if !content.cursor.visible {
        return None;
    }
    let display_cursor = content.display_cursor();
    // The published snapshot never carries a visible_row_shift (it is only
    // applied to the prepaint copy); the layout records the shift the
    // renderer painted with, so apply it here to line up with the screen.
    let display_cursor = display_cursor.shifted(layout.row_shift);
    if display_cursor.row < 0
        || display_cursor.row as usize >= content.visible_rows()
        || display_cursor.col >= content.columns
    {
        return None;
    }
    let row = display_cursor.row as usize;
    if row >= layout.rows {
        return None;
    }
    let origin = Point {
        x: layout.bounds.origin.x + layout.padding.left,
        y: layout.bounds.origin.y + layout.padding.top,
    };
    Some(Bounds {
        origin: Point {
            x: origin.x + layout.cell_width * display_cursor.col as f32,
            y: origin.y + layout.cell_height * row as f32,
        },
        size: Size {
            width: layout.cell_width,
            height: layout.cell_height,
        },
    })
}
