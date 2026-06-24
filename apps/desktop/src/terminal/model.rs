struct TerminalModel {
    handle: TerminalStateHandle,
    stdin_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    session_event_rx: mpsc::Receiver<TerminalUiEvent>,
    events: VecDeque<TerminalInternalEvent>,
    pending_output_bytes: Vec<u8>,
    output_flush_pending: bool,
    snapshot_dirty: bool,
    snapshot_publish_pending: bool,
    sync_output_depth: usize,
    sync_output_pending_notify: bool,
    sync_output_scan_tail: Vec<u8>,
    restored_bootstrap_active: bool,
    // A remote client (mobile, always dark-themed) owns the viewport; color
    // scheme queries must be answered for the renderer the user is actually
    // looking at, not the desktop theme.
    remote_viewer: bool,
    viewport_generation: u64,
    last_paint_sync: Option<Instant>,
    last_engine_resize_at: Option<Instant>,
    engine_resize_flush_pending: bool,
    color_scheme_state: TerminalColorSchemeState,
    title: Option<String>,
    exited: bool,
    focused: bool,
    colors: ColorPalette,
    paste_images_as_paths: bool,
    window_size: TerminalWindowSize,
    selection: SelectionState,
    #[cfg(test)]
    written_bytes: Option<Arc<Mutex<Vec<u8>>>>,
    _reader_task: Task<()>,
}

#[derive(Clone)]
struct TerminalStateHandle {
    screen: Arc<Mutex<HeadlessTerminalScreen>>,
    snapshot: Arc<Mutex<TerminalContent>>,
    // Last dimensions requested from the screen engine. The published
    // snapshot lags behind the engine while publishes are queued, so resize
    // dedup must not compare against it.
    engine_dims: Arc<Mutex<(usize, usize)>>,
}

#[derive(Clone, Copy, Debug)]
enum TerminalInternalEvent {
    Resize { cols: usize, rows: usize },
    Scroll { lines: i32 },
    ScrollAbsolute { offset: usize },
}

impl TerminalModel {
    fn new<W>(
        stdin_writer: W,
        bytes_rx: flume::Receiver<Vec<u8>>,
        session_event_rx: mpsc::Receiver<TerminalUiEvent>,
        config: &TerminalConfig,
        restored_output: Option<TerminalOutputSnapshot>,
        cx: &mut Context<Self>,
    ) -> Self
    where
        W: Write + Send + 'static,
    {
        let stdin_writer: Arc<Mutex<Box<dyn Write + Send>>> =
            Arc::new(Mutex::new(Box::new(stdin_writer)));
        // The engine answers VT queries (DSR/CPR, DECRQM, DA, ...) itself;
        // replies are forwarded straight to the PTY from the worker thread.
        let responder: codux_terminal_core::TerminalPtyResponder = {
            let writer = stdin_writer.clone();
            Arc::new(move |bytes: &[u8]| {
                let mut writer = writer.lock();
                let _ = writer.write_all(bytes);
                let _ = writer.flush();
            })
        };
        let screen = Arc::new(Mutex::new(HeadlessTerminalScreen::new_with_responder(
            config.cols,
            config.rows,
            config.scrollback,
            Some(responder),
        )));
        let restored_bootstrap_active = restored_output
            .as_ref()
            .is_some_and(|restored_output| !restored_output.tail.is_empty());
        if let Some(restored_output) = restored_output.as_ref()
            && !restored_output.tail.is_empty()
        {
            screen.lock().process_replay(restored_output.tail.as_bytes());
            codux_runtime::runtime_trace::runtime_trace(
                "terminal-restore",
                &format!(
                    "restored_bootstrap tail_bytes={} total_bytes={}",
                    restored_output.tail.len(),
                    restored_output.bytes
                ),
            );
        }
        // Seed the published content with correct dimensions only; blocking
        // here for a real snapshot would stall the UI thread behind the
        // restored-tail processing for every pane on a project switch. The
        // first paint publishes the real content asynchronously.
        let snapshot = TerminalContent::from_screen_snapshot(TerminalScreenSnapshot {
            cols: config.cols,
            rows: config.rows,
            total_lines: config.rows,
            ..TerminalScreenSnapshot::default()
        });
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
        // Publish the real engine content (restored tail) right away: the
        // seeded published content is dimensions-only, and if any later
        // publish path stalls (no output, deferred sync state) the pane
        // must not sit on a blank seed.
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let _ = this.update(cx, |model, cx| model.schedule_snapshot_publish(cx));
        })
        .detach();

        Self {
            handle: TerminalStateHandle {
                screen,
                snapshot: Arc::new(Mutex::new(snapshot)),
                engine_dims: Arc::new(Mutex::new((config.cols, config.rows))),
            },
            stdin_writer,
            session_event_rx,
            events: VecDeque::new(),
            pending_output_bytes: Vec::new(),
            output_flush_pending: false,
            // The seeded published content is dimensions-only; the first
            // paint publishes the real screen.
            snapshot_dirty: true,
            snapshot_publish_pending: false,
            sync_output_depth: 0,
            sync_output_pending_notify: false,
            sync_output_scan_tail: Vec::new(),
            restored_bootstrap_active,
            remote_viewer: false,
            viewport_generation: 0,
            last_paint_sync: None,
            last_engine_resize_at: None,
            engine_resize_flush_pending: false,
            color_scheme_state: TerminalColorSchemeState::default(),
            title: None,
            exited: false,
            focused: false,
            colors: config.colors.clone(),
            paste_images_as_paths: config.paste_images_as_paths,
            window_size: TerminalWindowSize {
                num_lines: config.rows as u16,
                num_cols: config.cols as u16,
                cell_width: 1,
                cell_height: 1,
            },
            selection: SelectionState::default(),
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
        // Append to the pending buffer and drain at most one capped chunk now;
        // a large burst (session restore) keeps the remainder for the next
        // frame instead of processing everything synchronously.
        self.pending_output_bytes.extend(bytes);
        self.drain_pending_output(cx);
        self.schedule_pending_output_flush(cx);
    }

    /// Process up to [`TERMINAL_OUTPUT_MAX_BYTES_PER_FLUSH`] of the pending
    /// output. Returns true if bytes remained unprocessed (caller should keep
    /// the flush armed so the rest drains on the next frame).
    fn drain_pending_output(&mut self, cx: &mut Context<Self>) -> bool {
        if self.pending_output_bytes.is_empty() {
            return false;
        }
        if self.pending_output_bytes.len() <= TERMINAL_OUTPUT_MAX_BYTES_PER_FLUSH {
            let bytes = std::mem::take(&mut self.pending_output_bytes);
            self.process_output_bytes(&bytes, cx);
            return false;
        }
        // Split off a capped chunk; the VT parser is stateful across calls, so
        // any byte boundary is safe to resume from.
        let rest = self
            .pending_output_bytes
            .split_off(TERMINAL_OUTPUT_MAX_BYTES_PER_FLUSH);
        let chunk = std::mem::replace(&mut self.pending_output_bytes, rest);
        self.process_output_bytes(&chunk, cx);
        true
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
        if self.pending_output_bytes.is_empty() {
            if self.process_pending_events(cx) || self.snapshot_dirty {
                self.request_snapshot_publish(cx);
                cx.notify();
            }
            return;
        }

        let has_backlog = self.drain_pending_output(cx);
        if has_backlog {
            // More than one capped chunk is queued (e.g. a session restore);
            // keep the flush armed so the rest drains on subsequent frames
            // without blocking this one.
            self.output_flush_pending = true;
            self.schedule_pending_output_flush(cx);
        }
    }

    fn process_output_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        // Apply any pending viewport resize before parsing this output. A mobile
        // handoff resizes the PTY and the running TUI repaints for the new grid,
        // but the viewport event and the output bytes arrive on separate
        // channels. alacritty does not reflow the alt screen, so a repaint
        // parsed while our screen is still the old size lands misplaced (the
        // intermittently garbled TUI input box). Draining the viewport event and
        // flushing the resize first keeps our grid in lockstep with the PTY
        // before the bytes are parsed.
        let pending_event_notify = self.process_pending_events(cx);
        self.apply_model_events();
        self.replace_restored_bootstrap_before_output(bytes.len());
        let sync_update = self.update_synchronized_output_state(bytes);
        let color_scheme_update =
            update_terminal_color_scheme_state(bytes, &mut self.color_scheme_state);
        self.respond_to_color_scheme_queries(color_scheme_update.query_count);
        self.respond_to_osc_color_queries(&color_scheme_update);
        self.process_bytes(bytes);
        trace_terminal_protocol_bytes(
            bytes,
            sync_update,
            self.sync_output_depth,
            color_scheme_update,
            self.color_scheme_state.updates_enabled,
        );
        self.trace_terminal_state_after_output(bytes.len());
        let event_should_notify = pending_event_notify | self.process_pending_events(cx);

        if self.sync_output_depth > 0 {
            self.sync_output_pending_notify = true;
            return;
        }

        let should_publish =
            sync_update.should_notify || event_should_notify || self.sync_output_pending_notify;
        self.sync_output_pending_notify = false;
        if should_publish || self.snapshot_dirty {
            self.request_snapshot_publish(cx);
        }
    }

    #[cfg(test)]
    fn process_output_bytes_for_test(&mut self, bytes: &[u8]) {
        self.replace_restored_bootstrap_before_output(bytes.len());
        self.process_bytes(bytes);
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
            "state bytes={} sync_depth={} cursor_visible={} cursor_row={} cursor_col={} cursor_shape={:?} display_offset={}",
            bytes_len,
            self.sync_output_depth,
            content.cursor.visible,
            content.cursor.row,
            content.cursor.col,
            content.cursor.shape,
            content.display_offset,
        ));
    }

    fn process_pending_events(&mut self, _cx: &mut Context<Self>) -> bool {
        let mut should_notify = false;
        while let Ok(event) = self.session_event_rx.try_recv() {
            if self.apply_ui_event(event) {
                should_notify = true;
            }
        }
        should_notify
    }

    fn apply_ui_event(&mut self, event: TerminalUiEvent) -> bool {
        match event {
            TerminalUiEvent::Wakeup => !self.output_flush_pending,
            TerminalUiEvent::Viewport {
                remote_owner,
                generation,
                cols,
                rows,
            } => {
                let scheme_changed = self.remote_viewer != remote_owner
                    && self.effective_scheme_is_dark() != scheme_is_dark(remote_owner, &self.colors);
                self.remote_viewer = remote_owner;
                self.viewport_generation = generation;
                // While a remote client owns the viewport, the PTY is sized to
                // its grid and the running TUI repaints for that size. Adopt the
                // remote grid for our own screen so the TUI's repaint renders at
                // the size it was drawn for, instead of being misplaced in our
                // larger desktop grid (input box pushed mid-screen, the rows
                // below it blank). The desktop prepaint keeps our screen at this
                // size while the remote owns (it is no longer the local owner);
                // on release it reclaims and resizes back to the desktop grid.
                if remote_owner && cols > 0 && rows > 0 {
                    let (cols, rows) = (cols as usize, rows as usize);
                    if self.dimensions() != (cols, rows) {
                        self.window_size.num_cols = cols as u16;
                        self.window_size.num_lines = rows as u16;
                        self.events
                            .push_back(TerminalInternalEvent::Resize { cols, rows });
                    }
                }
                if scheme_changed && self.color_scheme_state.updates_enabled {
                    // Let the TUI re-adapt to the renderer that now owns
                    // the viewport (mobile is always dark).
                    self.write_color_scheme_report();
                }
                true
            }
            TerminalUiEvent::Exit => {
                self.exited = true;
                true
            }
            TerminalUiEvent::Error(message) => {
                self.title = Some(format!("Terminal error: {message}"));
                true
            }
        }
    }

    fn process_bytes(&mut self, bytes: &[u8]) {
        let before_selection = self.selection_range().and_then(|range| {
            let text = self.handle.selected_text_for_range(range);
            (!text.is_empty()).then_some((range, text))
        });
        self.handle.screen.lock().process(bytes);
        if let Some((range, text)) = before_selection {
            let content = self.live_snapshot();
            if selected_text_from_content(&content, range) != text
                && let Some(next_range) = find_selection_text_range(&content, &text)
            {
                self.selection.set_range(next_range);
            }
        }
        self.snapshot_dirty = true;
    }

    fn replace_restored_bootstrap_before_output(&mut self, bytes_len: usize) {
        if !self.restored_bootstrap_active {
            return;
        }
        let cols = self.window_size.num_cols as usize;
        let rows = self.window_size.num_lines as usize;
        self.handle.clear_screen();
        self.restored_bootstrap_active = false;
        self.snapshot_dirty = true;
        codux_runtime::runtime_trace::runtime_trace(
            "terminal-restore",
            &format!(
                "restored_bootstrap_replace first_bytes={} cols={} rows={}",
                bytes_len, cols, rows
            ),
        );
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

    fn respond_to_osc_color_queries(&self, update: &TerminalColorSchemeUpdate) {
        // While a remote (always dark) client owns the viewport, report its
        // colors: the TUI derives panel backgrounds from these replies and
        // must match the renderer the user is looking at.
        for _ in 0..update.osc_foreground_queries {
            let report = if self.remote_viewer {
                terminal_osc_rgb_report(10, REMOTE_VIEWER_FOREGROUND)
            } else {
                terminal_osc_color_report(10, self.colors.foreground())
            };
            self.write_bytes(&report);
        }
        for _ in 0..update.osc_background_queries {
            let report = if self.remote_viewer {
                terminal_osc_rgb_report(11, REMOTE_VIEWER_BACKGROUND)
            } else {
                terminal_osc_color_report(11, self.colors.background())
            };
            self.write_bytes(&report);
        }
    }

    fn write_color_scheme_report(&self) {
        self.write_bytes(terminal_color_scheme_report_for(
            self.effective_scheme_is_dark(),
        ));
    }

    fn effective_scheme_is_dark(&self) -> bool {
        scheme_is_dark(self.remote_viewer, &self.colors)
    }

    fn remote_viewer(&self) -> bool {
        self.remote_viewer
    }

    fn viewport_generation(&self) -> u64 {
        self.viewport_generation
    }

    fn sync(&mut self, cx: &mut Context<Self>) -> TerminalContent {
        self.last_paint_sync = Some(Instant::now());
        self.process_pending_events(cx);
        self.apply_model_events();
        self.schedule_deferred_resize_flush(cx);
        self.schedule_snapshot_publish(cx);
        self.handle.snapshot()
    }

    // Output-driven publish requests go through here: snapshots are only
    // computed for terminals that are actually being painted. For hidden
    // panes (background worktrees, occluded windows) the snapshot stays
    // dirty and we just notify; when the view paints again its prepaint
    // sync() schedules the publish.
    fn request_snapshot_publish(&mut self, cx: &mut Context<Self>) {
        let painted_recently = self
            .last_paint_sync
            .is_some_and(|at| at.elapsed() < TERMINAL_PAINT_RECENCY);
        if painted_recently {
            self.schedule_snapshot_publish(cx);
        } else {
            cx.notify();
        }
    }

    fn apply_model_events(&mut self) -> bool {
        let mut snapshot_dirty = self.snapshot_dirty;
        let mut deferred_resize = None;
        while let Some(event) = self.events.pop_front() {
            match event {
                TerminalInternalEvent::Resize { cols, rows } => {
                    if self.engine_resize_throttled(cols, rows) {
                        deferred_resize = Some(event);
                    } else {
                        let applied = self.handle.resize(cols, rows);
                        if applied {
                            self.last_engine_resize_at = Some(Instant::now());
                        }
                        snapshot_dirty |= applied;
                    }
                }
                TerminalInternalEvent::Scroll { lines } => {
                    snapshot_dirty |= self.handle.scroll_display(lines);
                }
                TerminalInternalEvent::ScrollAbsolute { offset } => {
                    snapshot_dirty |= self.handle.scroll_to_offset(offset);
                }
            }
        }
        if let Some(event) = deferred_resize {
            self.events.push_back(event);
        }
        self.snapshot_dirty = snapshot_dirty;
        snapshot_dirty
    }

    // Column changes rewrap the whole scrollback; split/window drags emit
    // one per frame, so engine resizes are rate-limited. The deferred event
    // stays queued and is flushed by schedule_deferred_resize_flush.
    fn engine_resize_throttled(&self, cols: usize, rows: usize) -> bool {
        if *self.handle.engine_dims.lock() == (cols, rows) {
            return false;
        }
        self.last_engine_resize_at
            .is_some_and(|at| at.elapsed() < TERMINAL_ENGINE_RESIZE_THROTTLE)
    }

    fn schedule_deferred_resize_flush(&mut self, cx: &mut Context<Self>) {
        if self.engine_resize_flush_pending {
            return;
        }
        let has_resize = self
            .events
            .iter()
            .any(|event| matches!(event, TerminalInternalEvent::Resize { .. }));
        if !has_resize {
            return;
        }
        self.engine_resize_flush_pending = true;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |model: WeakEntity<Self>, cx| {
            timer.timer(TERMINAL_ENGINE_RESIZE_THROTTLE).await;
            let _ = model.update(cx, |model, cx| {
                model.engine_resize_flush_pending = false;
                if model.apply_model_events() {
                    model.schedule_snapshot_publish(cx);
                    cx.notify();
                }
                // Still throttled (another resize landed meanwhile): re-arm.
                model.schedule_deferred_resize_flush(cx);
            });
        })
        .detach();
    }

    #[cfg(test)]
    fn publish_snapshot_now(&mut self) -> TerminalContent {
        if self.apply_model_events() {
            self.handle.publish_snapshot();
            self.snapshot_dirty = false;
        }
        self.handle.snapshot()
    }

    fn schedule_snapshot_publish(&mut self, cx: &mut Context<Self>) {
        if !self.snapshot_dirty || self.snapshot_publish_pending || self.snapshot_publish_deferred()
        {
            return;
        }
        let request = self.handle.snapshot_request();
        self.snapshot_dirty = false;
        self.snapshot_publish_pending = true;
        let started_at = Instant::now();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let snapshot = codux_runtime::async_runtime::spawn_blocking(move || request.snapshot())
                .await
                .ok();
            let _ = this.update(cx, |model, cx| {
                model.snapshot_publish_pending = false;
                let mut content_changed = false;
                if let Some(snapshot) = snapshot {
                    let elapsed = started_at.elapsed();
                    content_changed = model.publish_completed_snapshot(snapshot, elapsed);
                }
                if model.snapshot_dirty {
                    model.request_snapshot_publish(cx);
                }
                if content_changed {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn publish_completed_snapshot(
        &mut self,
        snapshot: TerminalScreenSnapshot,
        elapsed: Duration,
    ) -> bool {
        if !self.events.is_empty() {
            self.apply_model_events();
        }
        let publish_deferred = self.snapshot_publish_deferred();
        let dirty_queued = self.snapshot_dirty;
        if self.should_skip_completed_snapshot(&snapshot, dirty_queued, publish_deferred) {
            self.snapshot_dirty = true;
            trace_snapshot_publish_result(
                "snapshot_publish_skip_stale",
                elapsed,
                snapshot.cols,
                snapshot.rows,
                snapshot.cells.len(),
                dirty_queued,
            );
            return false;
        }

        let (cols, rows, cells, content_changed) = self.handle.publish_screen_snapshot(snapshot);
        trace_snapshot_publish_result(
            "snapshot_publish_slow",
            elapsed,
            cols,
            rows,
            cells,
            dirty_queued,
        );
        content_changed
    }

    fn snapshot_publish_deferred(&self) -> bool {
        self.output_flush_pending
            || !self.pending_output_bytes.is_empty()
            || self.sync_output_depth > 0
    }

    fn should_skip_completed_snapshot(
        &self,
        snapshot: &TerminalScreenSnapshot,
        dirty_queued: bool,
        publish_deferred: bool,
    ) -> bool {
        if self.sync_output_depth > 0 {
            return true;
        }

        // Compare against the dims last requested from the engine, not the
        // window's target dims: while an engine resize is throttled during a
        // drag, snapshots at the current engine size are the freshest
        // content available and must keep publishing.
        let engine_dims = *self.handle.engine_dims.lock();
        if dirty_queued && (snapshot.cols, snapshot.rows) != engine_dims {
            return true;
        }

        !self.handle.snapshot.lock().cells.is_empty()
            && snapshot.cells.is_empty()
            && (dirty_queued || publish_deferred)
    }

    fn prepare_input_viewport(&mut self, cx: &mut Context<Self>) {
        if self.prepare_input_viewport_snapshot() {
            self.schedule_snapshot_publish(cx);
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
                    // Input preparation applies resizes immediately (typing
                    // implies the drag burst is over); stamp the throttle so
                    // a following frame doesn't double-resize.
                    let applied = self.handle.resize(cols, rows);
                    if applied {
                        self.last_engine_resize_at = Some(Instant::now());
                    }
                    snapshot_dirty |= applied;
                }
                TerminalInternalEvent::Scroll { .. }
                | TerminalInternalEvent::ScrollAbsolute { .. } => {}
            }
        }
        snapshot_dirty |= self.handle.scroll_to_bottom();
        // scroll_to_bottom judges "already at bottom" from the published
        // offset, which lags the engine while a publish is in flight; the
        // in-flight snapshot may capture a scrolled viewport, so republish
        // to be safe.
        snapshot_dirty |= self.snapshot_publish_pending;
        self.snapshot_dirty = snapshot_dirty;
        snapshot_dirty
    }

    #[cfg(test)]
    fn sync_for_test(&mut self) -> TerminalContent {
        self.publish_snapshot_now()
    }

    fn live_snapshot(&self) -> TerminalContent {
        TerminalContent::from_screen_snapshot(self.handle.screen.lock().snapshot())
    }

    fn mode(&self) -> TerminalInputMode {
        self.handle.live_input_mode()
    }

    fn snapshot(&self) -> TerminalContent {
        self.handle.snapshot()
    }

    fn current_ime_cursor_bounds(&self, layout: &TerminalLayoutMetrics) -> Option<Bounds<Pixels>> {
        let content = self.handle.snapshot();
        ime_cursor_bounds_from_content(&content, layout)
    }

    fn dimensions(&self) -> (usize, usize) {
        (
            self.window_size.num_cols as usize,
            self.window_size.num_lines as usize,
        )
    }

    fn scroll_display(&mut self, lines: i32) -> bool {
        self.events
            .push_back(TerminalInternalEvent::Scroll { lines });
        true
    }

    fn apply_pending_scroll_for_selection(&mut self) -> (bool, TerminalContent) {
        let before = self.handle.live_display_offset();
        if self.apply_model_events() {
            self.snapshot_dirty = true;
        }
        let content = self.live_snapshot();
        (content.display_offset != before, content)
    }

    // Absolute scrollbar targets: the delta is resolved on the engine
    // worker against the live offset, so a lagging published offset can't
    // compound the same distance across drag frames.
    fn scroll_to_display_offset(&mut self, offset: usize) -> bool {
        match self.events.back_mut() {
            Some(TerminalInternalEvent::ScrollAbsolute { offset: o }) => *o = offset,
            _ => self
                .events
                .push_back(TerminalInternalEvent::ScrollAbsolute { offset }),
        }
        true
    }

    fn start_selection(&mut self, point: TerminalSelectionPoint) {
        self.selection.start(point);
    }

    fn update_selection(&mut self, point: TerminalSelectionPoint) {
        if self.selection.dragging || self.selection.anchor.is_some() {
            self.selection.update(point);
        }
    }

    fn clear_selection(&mut self) {
        self.selection.clear();
    }

    fn selected_text(&self) -> Option<String> {
        self.selection_range()
            .map(|range| self.handle.selected_text_for_range(range))
    }

    fn selection_range(&self) -> Option<SelectionRange> {
        self.selection.range()
    }

    fn resize(&mut self, cols: usize, rows: usize, window_size: TerminalWindowSize) {
        let current = self.dimensions();
        self.window_size = window_size;
        if current == (cols, rows) {
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
        // Read bracketed-paste from the live engine: the published snapshot
        // lags it, and an unbracketed multi-line paste into a shell executes
        // every line. Pastes are rare user actions, so the blocking worker
        // round-trip is acceptable.
        self.write_bytes(&codux_terminal_core::terminal_paste_input_bytes(
            text,
            self.handle.live_input_mode().bracketed_paste,
        ));
    }

    fn report_focus_change(&self, focused: bool) {
        if !self.mode().focus_in_out {
            return;
        }
        self.write_bytes(if focused { b"\x1b[I" } else { b"\x1b[O" });
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    #[cfg(test)]
    fn new_for_test(cols: usize, rows: usize, scrollback: usize) -> Self {
        let (_session_event_tx, session_event_rx) = mpsc::channel();
        let written_bytes = Arc::new(Mutex::new(Vec::new()));
        let screen = Arc::new(Mutex::new(HeadlessTerminalScreen::new(
            cols, rows, scrollback,
        )));
        let snapshot = TerminalContent::from_screen_snapshot(screen.lock().snapshot());
        Self {
            handle: TerminalStateHandle {
                screen,
                snapshot: Arc::new(Mutex::new(snapshot)),
                engine_dims: Arc::new(Mutex::new((cols, rows))),
            },
            stdin_writer: Arc::new(Mutex::new(Box::new(TestTerminalWriter {
                bytes: written_bytes.clone(),
            }) as Box<dyn Write + Send>)),
            session_event_rx,
            events: VecDeque::new(),
            pending_output_bytes: Vec::new(),
            output_flush_pending: false,
            snapshot_dirty: false,
            snapshot_publish_pending: false,
            sync_output_depth: 0,
            sync_output_pending_notify: false,
            sync_output_scan_tail: Vec::new(),
            restored_bootstrap_active: false,
            remote_viewer: false,
            viewport_generation: 0,
            last_paint_sync: None,
            last_engine_resize_at: None,
            engine_resize_flush_pending: false,
            color_scheme_state: TerminalColorSchemeState::default(),
            title: None,
            exited: false,
            focused: false,
            colors: ColorPalette::default(),
            paste_images_as_paths: true,
            window_size: TerminalWindowSize {
                num_lines: rows as u16,
                num_cols: cols as u16,
                cell_width: 1,
                cell_height: 1,
            },
            selection: SelectionState::default(),
            written_bytes: Some(written_bytes),
            _reader_task: Task::ready(()),
        }
    }

    #[cfg(test)]
    fn new_for_test_with_restored_output(
        cols: usize,
        rows: usize,
        scrollback: usize,
        restored_output: TerminalOutputSnapshot,
    ) -> Self {
        let mut model = Self::new_for_test(cols, rows, scrollback);
        if !restored_output.tail.is_empty() {
            model
                .handle
                .screen
                .lock()
                .process_replay(restored_output.tail.as_bytes());
            model.handle.publish_snapshot();
            model.restored_bootstrap_active = true;
        }
        model
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
    fn display_offset(&self) -> usize {
        self.snapshot.lock().display_offset
    }

    #[cfg(test)]
    fn input_mode(&self) -> TerminalInputMode {
        self.snapshot.lock().input_mode
    }

    fn live_input_mode(&self) -> TerminalInputMode {
        self.screen.lock().input_mode()
    }

    fn live_display_offset(&self) -> usize {
        self.screen.lock().display_offset()
    }

    fn snapshot(&self) -> TerminalContent {
        self.snapshot.lock().clone()
    }

    #[cfg(test)]
    fn publish_snapshot(&self) {
        let snapshot = self.screen.lock().snapshot();
        *self.snapshot.lock() = TerminalContent::from_screen_snapshot(snapshot);
    }

    fn snapshot_request(&self) -> HeadlessTerminalSnapshotRequest {
        // The desktop renderer only consumes cells; skip the ANSI repaint
        // string the worker would otherwise build per snapshot.
        self.screen.lock().snapshot_request(false)
    }

    // Returns (cols, rows, cell count, content changed). Skipping the
    // repaint when the content is unchanged keeps idle-but-noisy terminals
    // from re-shaping the whole viewport every output batch.
    fn publish_screen_snapshot(&self, snapshot: TerminalScreenSnapshot) -> (usize, usize, usize, bool) {
        let content = TerminalContent::from_screen_snapshot(snapshot);
        let stats = (content.columns, content.screen_lines, content.cells.len());
        let mut published = self.snapshot.lock();
        let content_changed = *published != content;
        *published = content;
        (stats.0, stats.1, stats.2, content_changed)
    }

    fn resize(&self, cols: usize, rows: usize) -> bool {
        let mut engine_dims = self.engine_dims.lock();
        if *engine_dims == (cols, rows) {
            return false;
        }
        *engine_dims = (cols, rows);
        self.screen.lock().resize(cols, rows);
        true
    }

    fn clear_screen(&self) {
        self.screen.lock().clear();
    }

    fn scroll_display(&self, lines: i32) -> bool {
        if lines == 0 {
            return false;
        }
        let mut screen = self.screen.lock();
        let before = screen.display_offset();
        screen.scroll_lines(lines);
        before != screen.display_offset()
    }

    fn scroll_to_offset(&self, offset: usize) -> bool {
        let mut screen = self.screen.lock();
        let before = screen.display_offset();
        screen.scroll_to_offset(offset);
        before != screen.display_offset()
    }

    fn scroll_to_bottom(&self) -> bool {
        let before = self.display_offset();
        self.screen.lock().scroll_to_bottom();
        before != 0
    }

    fn selected_text_for_range(&self, range: SelectionRange) -> String {
        let content = self.snapshot();
        if content_covers_selection_range(&content, range) {
            selected_text_from_content(&content, range)
        } else {
            selected_text_from_screen_range(&self.screen, &content, range)
        }
    }
}

fn trace_snapshot_publish_result(
    label: &str,
    elapsed: Duration,
    cols: usize,
    rows: usize,
    cells: usize,
    dirty_queued: bool,
) {
    if elapsed < TERMINAL_SNAPSHOT_PUBLISH_SLOW && !terminal_trace_enabled() {
        return;
    }
    codux_runtime::runtime_trace::runtime_trace(
        "terminal-render",
        &format!(
            "{label} elapsed_ms={} cols={} rows={} cells={} dirty_queued={}",
            elapsed.as_millis(),
            cols,
            rows,
            cells,
            dirty_queued
        ),
    );
}

fn selected_text_from_content(content: &TerminalContent, range: SelectionRange) -> String {
    let mut lines = Vec::new();
    for line in range.start.line..=range.end.line {
        let start_col = if line == range.start.line {
            range.start.col
        } else {
            0
        };
        let end_col = if line == range.end.line {
            range.end.col
        } else {
            content.columns
        };
        lines.push(selected_line_text(content, line, start_col, end_col));
    }
    lines.join("\n")
}

fn selected_text_from_screen_range(
    screen: &Arc<Mutex<HeadlessTerminalScreen>>,
    fallback_content: &TerminalContent,
    range: SelectionRange,
) -> String {
    let mut lines = Vec::new();
    let mut line = range.start.line;
    let mut content = fallback_content.clone();
    while line <= range.end.line {
        if !content.line_in_snapshot(line) {
            let offset = display_offset_for_line(
                line,
                content.total_lines,
                content.screen_lines,
            );
            let request = { screen.lock().snapshot_at_offset_request(offset) };
            content = TerminalContent::from_screen_snapshot(request.snapshot());
        }
        if !content.line_in_snapshot(line) {
            lines.push(String::new());
            line = line.saturating_add(1);
            continue;
        }
        let chunk_end = content
            .last_snapshot_line()
            .unwrap_or(line)
            .min(range.end.line);
        for selected_line in line..=chunk_end {
            let start_col = if selected_line == range.start.line {
                range.start.col
            } else {
                0
            };
            let end_col = if selected_line == range.end.line {
                range.end.col
            } else {
                content.columns
            };
            lines.push(selected_line_text(
                &content,
                selected_line,
                start_col,
                end_col,
            ));
        }
        line = chunk_end.saturating_add(1);
    }
    lines.join("\n")
}

fn content_covers_selection_range(content: &TerminalContent, range: SelectionRange) -> bool {
    content.line_in_snapshot(range.start.line) && content.line_in_snapshot(range.end.line)
}

fn display_offset_for_line(line: i32, total_lines: usize, rows: usize) -> usize {
    let line = usize::try_from(line).unwrap_or(0);
    total_lines
        .saturating_sub(rows)
        .saturating_sub(line)
}

fn selection_point_from_cell(
    point: TerminalCellPoint,
    content: &TerminalContent,
) -> TerminalSelectionPoint {
    TerminalSelectionPoint {
        line: content.line_for_display_row(point.row),
        col: point.col,
    }
}

fn selected_line_text(
    content: &TerminalContent,
    line: i32,
    start_col: usize,
    end_col: usize,
) -> String {
    let row_cells = content
        .cells
        .iter()
        .filter(|cell| cell.line() == line)
        .collect::<Vec<_>>();
    let row_text = terminal_row_text(&row_cells);
    row_text
        .into_iter()
        .filter(|(col, _)| *col >= start_col && *col < end_col)
        .map(|(_, ch)| ch)
        .collect()
}

fn find_selection_text_range(content: &TerminalContent, selected_text: &str) -> Option<SelectionRange> {
    let mut lines = selected_text.split('\n');
    let first_line = lines.next()?;
    if lines.next().is_some() || first_line.is_empty() {
        return None;
    }

    for line in content.line_for_display_row(0)
        ..=content.line_for_display_row(content.visible_rows().saturating_sub(1))
    {
        let row_cells = content
            .cells
            .iter()
            .filter(|cell| cell.line() == line)
            .collect::<Vec<_>>();
        let row_text = terminal_row_text(&row_cells);
        let chars = row_text.iter().map(|(_, ch)| *ch).collect::<String>();
        if let Some(byte_start) = chars.find(first_line) {
            let start_char = chars[..byte_start].chars().count();
            let end_char = start_char + first_line.chars().count();
            let start_col = row_text.get(start_char).map(|(col, _)| *col)?;
            let end_col = row_text
                .get(end_char.saturating_sub(1))
                .map(|(col, ch)| col.saturating_add(terminal_char_width(*ch)))?;
            return Some(SelectionRange {
                start: TerminalSelectionPoint {
                    line,
                    col: start_col,
                },
                end: TerminalSelectionPoint { line, col: end_col },
            });
        }
    }
    None
}
