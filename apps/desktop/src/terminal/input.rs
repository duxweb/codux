#[derive(Clone)]
struct TerminalInputHandler {
    model: Entity<TerminalModel>,
    layout: Arc<Mutex<TerminalLayoutMetrics>>,
    terminal_view: WeakEntity<TerminalView>,
    fallback_cursor_bounds: Option<Bounds<Pixels>>,
}

impl TerminalInputHandler {
    fn send_filtered_input(&self, text: &str, window: &mut Window, cx: &mut App) {
        if text.is_empty() {
            return;
        }

        let mut bytes = Vec::new();
        for c in text
            .chars()
            .filter(|c| !('\u{F700}'..='\u{F8FF}').contains(c))
        {
            match c {
                '\u{8}' => {
                    bytes.push(0x7f);
                }
                '\n' | '\r' => {
                    bytes.push(b'\r');
                }
                _ => {
                    let mut buffer = [0; 4];
                    bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
                }
            }
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
            view.set_marked_text(new_text.to_string(), cx)
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
            .or(self.fallback_cursor_bounds);
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
    let display_cursor = DisplayCursor::from(content.cursor.point, content.display_offset);
    if display_cursor.row < 0
        || display_cursor.row as usize >= content.screen_lines
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

