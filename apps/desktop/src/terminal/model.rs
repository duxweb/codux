struct TerminalModel {
    handle: TerminalStateHandle,
    parser: Processor,
    stdin_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    event_rx: mpsc::Receiver<TerminalUiEvent>,
    session_event_rx: mpsc::Receiver<TerminalUiEvent>,
    events: VecDeque<TerminalInternalEvent>,
    pending_output_bytes: Vec<u8>,
    output_flush_pending: bool,
    snapshot_dirty: bool,
    sync_output_depth: usize,
    sync_output_pending_notify: bool,
    sync_output_scan_tail: Vec<u8>,
    color_scheme_state: TerminalColorSchemeState,
    title: Option<String>,
    bell_count: usize,
    exited: bool,
    focused: bool,
    colors: ColorPalette,
    paste_images_as_paths: bool,
    window_size: WindowSize,
    #[cfg(test)]
    written_bytes: Option<Arc<Mutex<Vec<u8>>>>,
    _reader_task: Task<()>,
}

#[derive(Clone)]
struct TerminalStateHandle {
    term: Arc<Mutex<Term<GpuiEventProxy>>>,
    snapshot: Arc<Mutex<TerminalContent>>,
}

#[derive(Clone, Copy, Debug)]
enum TerminalInternalEvent {
    Resize { cols: usize, rows: usize },
    Scroll { lines: i32 },
}

impl TerminalModel {
    fn new<W>(
        stdin_writer: W,
        bytes_rx: flume::Receiver<Vec<u8>>,
        session_event_rx: mpsc::Receiver<TerminalUiEvent>,
        config: &TerminalConfig,
        cx: &mut Context<Self>,
    ) -> Self
    where
        W: Write + Send + 'static,
    {
        let (event_tx, event_rx) = mpsc::channel();
        let alacritty_config = AlacrittyConfig {
            scrolling_history: config.scrollback,
            ..Default::default()
        };
        let term = Arc::new(Mutex::new(Term::new(
            alacritty_config,
            &TermSize::new(config.cols, config.rows),
            GpuiEventProxy::new(event_tx),
        )));
        let snapshot = TerminalContent::from_term(&term.lock());
        let reader_task = cx.spawn(async move |this: WeakEntity<Self>, cx| {
            while let Ok(bytes) = bytes_rx.recv_async().await {
                if this
                    .update(cx, |model, cx| model.receive_output(bytes, cx))
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            handle: TerminalStateHandle {
                term,
                snapshot: Arc::new(Mutex::new(snapshot)),
            },
            parser: Processor::new(),
            stdin_writer: Arc::new(Mutex::new(Box::new(stdin_writer) as Box<dyn Write + Send>)),
            event_rx,
            session_event_rx,
            events: VecDeque::new(),
            pending_output_bytes: Vec::new(),
            output_flush_pending: false,
            snapshot_dirty: false,
            sync_output_depth: 0,
            sync_output_pending_notify: false,
            sync_output_scan_tail: Vec::new(),
            color_scheme_state: TerminalColorSchemeState::default(),
            title: None,
            bell_count: 0,
            exited: false,
            focused: false,
            colors: config.colors.clone(),
            paste_images_as_paths: config.paste_images_as_paths,
            window_size: WindowSize {
                num_lines: config.rows as u16,
                num_cols: config.cols as u16,
                cell_width: 1,
                cell_height: 1,
            },
            #[cfg(test)]
            written_bytes: None,
            _reader_task: reader_task,
        }
    }

    fn receive_output(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        if self.output_flush_pending {
            self.pending_output_bytes.extend(bytes);
            return;
        }

        self.output_flush_pending = true;
        self.process_output_bytes(&bytes, cx);
        self.schedule_pending_output_flush(cx);
    }

    fn schedule_pending_output_flush(&mut self, cx: &mut Context<Self>) {
        let timer = cx.background_executor().clone();
        cx.spawn(async move |model: WeakEntity<Self>, cx| {
            timer.timer(TERMINAL_OUTPUT_FRAME_INTERVAL).await;
            let _ = model.update(cx, |model, cx| {
                model.output_flush_pending = false;
                model.flush_output(cx);
            });
        })
        .detach();
    }

    fn flush_output(&mut self, cx: &mut Context<Self>) {
        let bytes = std::mem::take(&mut self.pending_output_bytes);
        if bytes.is_empty() {
            if self.process_pending_events(cx) {
                cx.notify();
            }
            return;
        }

        self.process_output_bytes(&bytes, cx);
    }

    fn process_output_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let before_display_offset = self.handle.display_offset();
        let sync_update = self.update_synchronized_output_state(bytes);
        let color_scheme_update =
            update_terminal_color_scheme_state(bytes, &mut self.color_scheme_state);
        self.respond_to_color_scheme_queries(color_scheme_update.query_count);
        self.process_bytes(&bytes);
        trace_terminal_protocol_bytes(
            bytes,
            sync_update,
            self.sync_output_depth,
            color_scheme_update,
            self.color_scheme_state.updates_enabled,
        );
        self.trace_terminal_state_after_output(bytes.len());
        let event_should_notify = self.process_pending_events(cx);

        if self.sync_output_depth > 0 {
            self.sync_output_pending_notify = true;
            return;
        }

        if sync_update.should_notify || event_should_notify || self.sync_output_pending_notify {
            self.sync_output_pending_notify = false;
        }
        let after_display_offset = self.handle.display_offset();
        if after_display_offset != before_display_offset {
            self.snapshot_dirty = true;
        }
        cx.notify();
    }

    fn update_synchronized_output_state(&mut self, bytes: &[u8]) -> SyncOutputUpdate {
        update_synchronized_output_state(
            bytes,
            &mut self.sync_output_depth,
            &mut self.sync_output_scan_tail,
        )
    }

    fn trace_terminal_state_after_output(&self, bytes_len: usize) {
        if !terminal_trace_enabled() {
            return;
        }
        let content = self.live_snapshot();
        terminal_trace(&format!(
            "state bytes={} sync_depth={} show_cursor={} cursor_hidden={} cursor_row={} cursor_col={} cursor_shape={:?} display_offset={}",
            bytes_len,
            self.sync_output_depth,
            content.mode.contains(TermMode::SHOW_CURSOR),
            content.cursor.shape == CursorShape::Hidden,
            content.cursor.point.line.0,
            content.cursor.point.column.0,
            content.cursor.shape,
            content.display_offset,
        ));
    }

    fn process_pending_events(&mut self, cx: &mut Context<Self>) -> bool {
        let mut should_notify = false;
        while let Ok(event) = self.event_rx.try_recv() {
            self.handle_ui_event(event, cx, &mut should_notify);
        }
        while let Ok(event) = self.session_event_rx.try_recv() {
            self.handle_ui_event(event, cx, &mut should_notify);
        }
        should_notify
    }

    fn handle_ui_event(
        &mut self,
        event: TerminalUiEvent,
        cx: &mut Context<Self>,
        should_notify: &mut bool,
    ) {
        match event {
            TerminalUiEvent::Wakeup => {
                if !self.output_flush_pending {
                    self.output_flush_pending = true;
                    self.schedule_pending_output_flush(cx);
                }
            }
            TerminalUiEvent::PtyWrite(bytes) => self.write_bytes(&bytes),
            TerminalUiEvent::Bell => {
                self.bell_count = self.bell_count.saturating_add(1);
                *should_notify = true;
            }
            TerminalUiEvent::Title(title) => {
                self.title = Some(title);
                *should_notify = true;
            }
            TerminalUiEvent::ClipboardStore(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            TerminalUiEvent::ClipboardLoad => {
                if let Some(text) = terminal_clipboard_paste_text(cx, self.paste_images_as_paths) {
                    self.write_bytes(text.as_bytes());
                }
            }
            TerminalUiEvent::ColorRequest(index, format) => {
                let color = self.color_request(index);
                terminal_trace(&format!(
                    "color_response index={} rgb=#{:02x}{:02x}{:02x}",
                    index, color.r, color.g, color.b
                ));
                self.write_bytes(format(color).as_bytes());
            }
            TerminalUiEvent::TextAreaSizeRequest(format) => {
                self.write_bytes(format(self.window_size).as_bytes());
            }
            TerminalUiEvent::Exit => {
                self.exited = true;
                *should_notify = true;
            }
            TerminalUiEvent::Error(message) => {
                self.title = Some(format!("Terminal error: {message}"));
                *should_notify = true;
            }
            TerminalUiEvent::Viewport { cols, rows } => {
                self.resize(
                    cols as usize,
                    rows as usize,
                    WindowSize {
                        num_lines: rows,
                        num_cols: cols,
                        cell_width: self.window_size.cell_width,
                        cell_height: self.window_size.cell_height,
                    },
                );
                *should_notify = true;
            }
        }
    }

    fn process_bytes(&mut self, bytes: &[u8]) {
        let mut term = self.handle.term.lock();
        self.parser.advance(&mut *term, bytes);
        self.snapshot_dirty = true;
    }

    fn update_colors(&mut self, colors: ColorPalette) {
        let was_dark = self.colors.is_dark();
        let is_dark = colors.is_dark();
        self.colors = colors;
        if self.color_scheme_state.updates_enabled && was_dark != is_dark {
            self.write_color_scheme_report();
        }
    }

    fn update_config(&mut self, colors: ColorPalette, paste_images_as_paths: bool) {
        self.paste_images_as_paths = paste_images_as_paths;
        self.update_colors(colors);
    }

    fn respond_to_color_scheme_queries(&self, query_count: usize) {
        for _ in 0..query_count {
            self.write_color_scheme_report();
        }
    }

    fn write_color_scheme_report(&self) {
        self.write_bytes(terminal_color_scheme_report(&self.colors));
    }

    fn sync(&mut self, cx: &mut Context<Self>) -> TerminalContent {
        self.process_pending_events(cx);
        self.sync_model_events()
    }

    fn sync_model_events(&mut self) -> TerminalContent {
        let mut snapshot_dirty = self.snapshot_dirty;
        while let Some(event) = self.events.pop_front() {
            match event {
                TerminalInternalEvent::Resize { cols, rows } => {
                    snapshot_dirty |= self.handle.resize(cols, rows);
                }
                TerminalInternalEvent::Scroll { lines } => {
                    snapshot_dirty |= self.handle.scroll_display(lines);
                }
            }
        }
        if snapshot_dirty {
            self.handle.publish_snapshot();
            self.snapshot_dirty = false;
        }
        self.handle.snapshot()
    }

    fn prepare_input_viewport(&mut self, cx: &mut Context<Self>) {
        if self.prepare_input_viewport_snapshot() {
            self.handle.publish_snapshot();
            self.snapshot_dirty = false;
            cx.notify();
        }
    }

    #[cfg(test)]
    fn prepare_input_viewport_for_test(&mut self) {
        if self.prepare_input_viewport_snapshot() {
            self.handle.publish_snapshot();
            self.snapshot_dirty = false;
        }
    }

    fn prepare_input_viewport_snapshot(&mut self) -> bool {
        let mut snapshot_dirty = self.snapshot_dirty;
        let events = std::mem::take(&mut self.events);
        for event in events {
            match event {
                TerminalInternalEvent::Resize { cols, rows } => {
                    snapshot_dirty |= self.handle.resize(cols, rows);
                }
                TerminalInternalEvent::Scroll { .. } => {}
            }
        }
        snapshot_dirty | self.handle.scroll_to_bottom()
    }

    #[cfg(test)]
    fn sync_for_test(&mut self) -> TerminalContent {
        self.sync_model_events()
    }

    fn live_snapshot(&self) -> TerminalContent {
        let term = self.handle.term.lock();
        let content = TerminalContent::from_term(&term);
        content
    }

    fn mode(&self) -> TermMode {
        self.handle.mode()
    }

    fn display_offset(&self) -> usize {
        self.handle.display_offset()
    }

    fn snapshot(&self) -> TerminalContent {
        self.handle.snapshot()
    }

    fn current_ime_cursor_bounds(&self, layout: &TerminalLayoutMetrics) -> Option<Bounds<Pixels>> {
        let content = self.handle.snapshot();
        ime_cursor_bounds_from_content(&content, layout)
    }

    fn dimensions(&self) -> (usize, usize) {
        self.handle.dimensions()
    }

    fn color_request(&self, index: usize) -> Rgb {
        if matches!(index, 256 | 257 | 258 | 267 | 268) {
            return self.colors.color_request(index);
        }
        self.handle.term.lock().colors()[index].unwrap_or_else(|| self.colors.color_request(index))
    }

    fn scroll_display(&mut self, lines: i32) -> bool {
        self.events
            .push_back(TerminalInternalEvent::Scroll { lines });
        true
    }

    fn start_selection(&self, point: TerminalSelectionPoint, side: TerminalSide) {
        self.handle.start_selection(point, side);
    }

    fn update_selection(&self, point: TerminalSelectionPoint, side: TerminalSide) {
        self.handle.update_selection(point, side);
    }

    fn clear_selection(&self) {
        self.handle.clear_selection();
    }

    fn selected_text(&self) -> Option<String> {
        self.handle.selected_text()
    }

    fn selection_range(&self) -> Option<SelectionRange> {
        self.handle.selection_range()
    }

    fn resize(&mut self, cols: usize, rows: usize, window_size: WindowSize) {
        self.window_size = window_size;
        if self.dimensions() == (cols, rows) {
            return;
        }
        match self.events.back_mut() {
            Some(TerminalInternalEvent::Resize { cols: c, rows: r }) => {
                *c = cols;
                *r = rows;
            }
            _ => self
                .events
                .push_back(TerminalInternalEvent::Resize { cols, rows }),
        }
    }

    fn write_bytes(&self, bytes: &[u8]) {
        let mut writer = self.stdin_writer.lock();
        let _ = writer.write_all(bytes);
        let _ = writer.flush();
    }

    #[cfg(test)]
    fn written_bytes_for_test(&self) -> Vec<u8> {
        self.written_bytes
            .as_ref()
            .map(|bytes| bytes.lock().clone())
            .unwrap_or_default()
    }

    fn paste_text(&self, text: &str) {
        if self.mode().contains(TermMode::BRACKETED_PASTE) {
            self.write_bytes(b"\x1b[200~");
            self.write_bytes(text.replace("\r\n", "\n").replace('\r', "\n").as_bytes());
            self.write_bytes(b"\x1b[201~");
        } else {
            self.write_bytes(text.as_bytes());
        }
    }

    fn report_focus_change(&self, focused: bool) {
        if !self.mode().contains(TermMode::FOCUS_IN_OUT) {
            return;
        }
        self.write_bytes(if focused { b"\x1b[I" } else { b"\x1b[O" });
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    #[cfg(test)]
    fn new_for_test(cols: usize, rows: usize, scrollback: usize) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        let (_session_event_tx, session_event_rx) = mpsc::channel();
        let written_bytes = Arc::new(Mutex::new(Vec::new()));
        let config = AlacrittyConfig {
            scrolling_history: scrollback,
            ..Default::default()
        };
        let term = Arc::new(Mutex::new(Term::new(
            config,
            &TermSize::new(cols, rows),
            GpuiEventProxy::new(event_tx),
        )));
        let snapshot = TerminalContent::from_term(&term.lock());
        Self {
            handle: TerminalStateHandle {
                term,
                snapshot: Arc::new(Mutex::new(snapshot)),
            },
            parser: Processor::new(),
            stdin_writer: Arc::new(Mutex::new(Box::new(TestTerminalWriter {
                bytes: written_bytes.clone(),
            }) as Box<dyn Write + Send>)),
            event_rx,
            session_event_rx,
            events: VecDeque::new(),
            pending_output_bytes: Vec::new(),
            output_flush_pending: false,
            snapshot_dirty: false,
            sync_output_depth: 0,
            sync_output_pending_notify: false,
            sync_output_scan_tail: Vec::new(),
            color_scheme_state: TerminalColorSchemeState::default(),
            title: None,
            bell_count: 0,
            exited: false,
            focused: false,
            colors: ColorPalette::default(),
            paste_images_as_paths: true,
            window_size: WindowSize {
                num_lines: rows as u16,
                num_cols: cols as u16,
                cell_width: 1,
                cell_height: 1,
            },
            written_bytes: Some(written_bytes),
            _reader_task: Task::ready(()),
        }
    }
}

#[cfg(test)]
struct TestTerminalWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
}

#[cfg(test)]
impl Write for TestTerminalWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes.lock().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl TerminalStateHandle {
    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    fn display_offset(&self) -> usize {
        self.term.lock().grid().display_offset()
    }

    fn dimensions(&self) -> (usize, usize) {
        let term = self.term.lock();
        (term.columns(), term.screen_lines())
    }

    fn snapshot(&self) -> TerminalContent {
        self.snapshot.lock().clone()
    }

    fn publish_snapshot(&self) {
        let term = self.term.lock();
        *self.snapshot.lock() = TerminalContent::from_term(&term);
    }

    fn resize(&self, cols: usize, rows: usize) -> bool {
        let mut term = self.term.lock();
        if cols == term.columns() && rows == term.screen_lines() {
            return false;
        }
        term.resize(TermSize::new(cols, rows));
        true
    }

    fn scroll_display(&self, lines: i32) -> bool {
        use alacritty_terminal::grid::Scroll;

        let mut term = self.term.lock();
        let before = term.grid().display_offset();
        let scroll = Scroll::Delta(lines);
        term.scroll_display(scroll);
        let did_scroll = term.grid().display_offset() != before;
        did_scroll
    }

    fn scroll_to_bottom(&self) -> bool {
        use alacritty_terminal::grid::Scroll;

        let mut term = self.term.lock();
        let before = term.grid().display_offset();
        term.scroll_display(Scroll::Bottom);
        term.grid().display_offset() != before
    }

    fn start_selection(&self, point: TerminalSelectionPoint, side: TerminalSide) {
        let mut term = self.term.lock();
        term.selection = Some(AlacrittySelection::new(
            AlacrittySelectionType::Simple,
            TerminalPoint::new(Line(point.line), Column(point.col)),
            side,
        ));
    }

    fn update_selection(&self, point: TerminalSelectionPoint, side: TerminalSide) {
        let mut term = self.term.lock();
        let point = TerminalPoint::new(Line(point.line), Column(point.col));
        if let Some(selection) = &mut term.selection {
            selection.update(point, side);
        } else {
            term.selection = Some(AlacrittySelection::new(
                AlacrittySelectionType::Simple,
                point,
                side.opposite(),
            ));
            if let Some(selection) = &mut term.selection {
                selection.update(point, side);
            }
        }
    }

    fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    fn selected_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    fn selection_range(&self) -> Option<SelectionRange> {
        let term = self.term.lock();
        let range = term.selection.as_ref()?.to_range(&term)?;
        Some(SelectionRange {
            start: TerminalSelectionPoint {
                line: range.start.line.0,
                col: range.start.column.0,
            },
            end: TerminalSelectionPoint {
                line: range.end.line.0,
                col: range.end.column.0,
            },
        })
    }

    #[cfg(test)]
    fn selected_text_for_range(&self, selection: SelectionRange) -> String {
        let term = self.term.lock();
        let grid = term.grid();
        let start = selection.start;
        let end = selection.end;
        let mut text = String::new();

        for term_line in start.line..=end.line {
            let start_col = if term_line == start.line {
                start.col
            } else {
                0
            };
            let end_col = if term_line == end.line {
                end.col
            } else {
                grid.columns()
            };
            let mut line_text = String::new();
            for col in start_col..end_col.min(grid.columns()) {
                let cell = &grid[TerminalPoint::new(Line(term_line), Column(col))];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                if cell.c != '\0' {
                    line_text.push(cell.c);
                    for c in cell.zerowidth().into_iter().flatten() {
                        line_text.push(*c);
                    }
                }
            }
            if term_line != end.line {
                text.push_str(line_text.trim_end());
                text.push('\n');
            } else {
                text.push_str(line_text.trim_end());
            }
        }

        text
    }
}
