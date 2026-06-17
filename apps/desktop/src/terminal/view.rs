pub struct TerminalView {
    model: Entity<TerminalModel>,
    renderer: TerminalRenderer,
    blink_manager: Entity<TerminalBlinkManager>,
    focus_handle: FocusHandle,
    session: TerminalSessionBinding,
    config: TerminalConfig,
    layout: Arc<Mutex<TerminalLayoutMetrics>>,
    selection: Arc<Mutex<SelectionState>>,
    scroll_handle: TerminalScrollHandle,
    marked_text: Option<String>,
    hover_link: Option<TerminalLink>,
    scroll_input: TerminalScrollInputState,
    selection_frame_pending: bool,
    pending_pty_resize: Option<(u16, u16)>,
    pty_resize_flush_pending: bool,
    last_pty_resize_at: Option<Instant>,
    focus_in_subscription: Option<Subscription>,
    focus_out_subscription: Option<Subscription>,
    focus_observer: Option<Arc<dyn Fn(&mut Window, &mut Context<TerminalView>)>>,
    selection_autoscroll: Option<SelectionAutoScroll>,
    _observe_model: Subscription,
    _observe_blink_manager: Subscription,
}

#[derive(Default)]
struct TerminalScrollInputState {
    pending_lines: i32,
    pending_pixels: f32,
    frame_pending: bool,
    suppress_residual_precise_scroll: bool,
}

#[derive(Debug, Default)]
struct TerminalScrollHandleState {
    line_height: Pixels,
    total_lines: usize,
    viewport_lines: usize,
    display_offset: usize,
}

#[derive(Clone, Debug, Default)]
struct TerminalScrollHandle {
    state: Rc<RefCell<TerminalScrollHandleState>>,
    future_display_offset: Rc<StdCell<Option<usize>>>,
}

impl TerminalScrollHandle {
    fn update(&self, content: &TerminalContent, line_height: Pixels) {
        *self.state.borrow_mut() = TerminalScrollHandleState {
            line_height: line_height.max(px(1.0)),
            total_lines: content.total_lines.max(content.screen_lines),
            viewport_lines: content.visible_rows(),
            display_offset: content.display_offset,
        };
    }

    fn take_future_display_offset(&self) -> Option<usize> {
        self.future_display_offset.take()
    }
}

impl ScrollbarHandle for TerminalScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        let state = self.state.borrow();
        let max_offset = state.total_lines.saturating_sub(state.viewport_lines);
        let scroll_offset = max_offset.saturating_sub(state.display_offset);
        Point::new(px(0.0), -(scroll_offset as f32 * state.line_height))
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        let state = self.state.borrow();
        let max_offset = state.total_lines.saturating_sub(state.viewport_lines);
        let offset_delta = (offset.y / state.line_height).round() as i32;
        let display_offset = (max_offset as i32 + offset_delta).clamp(0, max_offset as i32);
        self.future_display_offset
            .set(Some(display_offset as usize));
    }

    fn content_size(&self) -> Size<Pixels> {
        let state = self.state.borrow();
        Size {
            width: px(0.0),
            height: state.total_lines as f32 * state.line_height,
        }
    }
}

impl TerminalScrollInputState {
    fn prepare_for_keyboard_input(&mut self) {
        self.pending_lines = 0;
        self.pending_pixels = 0.0;
        self.suppress_residual_precise_scroll = true;
    }

    fn should_suppress_residual_scroll(&mut self, event: &ScrollWheelEvent) -> bool {
        match event.touch_phase {
            TouchPhase::Started => {
                self.suppress_residual_precise_scroll = false;
                false
            }
            TouchPhase::Ended => {
                self.suppress_residual_precise_scroll = false;
                false
            }
            TouchPhase::Moved => {
                if self.suppress_residual_precise_scroll && event.delta.precise() {
                    self.pending_pixels = 0.0;
                    true
                } else {
                    false
                }
            }
        }
    }
}

impl TerminalView {
    fn new<W>(
        stdin_writer: W,
        bytes_rx: flume::Receiver<Vec<u8>>,
        session_event_rx: mpsc::Receiver<TerminalUiEvent>,
        session: TerminalSessionBinding,
        config: TerminalConfig,
        restored_output: Option<TerminalOutputSnapshot>,
        cx: &mut Context<Self>,
    ) -> Self
    where
        W: Write + Send + 'static,
    {
        let model = cx.new(|cx| {
            TerminalModel::new(
                stdin_writer,
                bytes_rx,
                session_event_rx,
                &config,
                restored_output,
                cx,
            )
        });
        let blink_manager = cx.new(TerminalBlinkManager::new);
        let renderer = TerminalRenderer::new(
            config.font_family.clone(),
            config.font_size,
            config.line_height_multiplier,
            config.colors.clone(),
        );
        let focus_handle = cx.focus_handle();
        let observe_model = cx.observe(&model, |_, _, cx| cx.notify());
        let observe_blink_manager = cx.observe(&blink_manager, |_, _, cx| cx.notify());

        Self {
            model,
            renderer,
            blink_manager,
            focus_handle,
            session,
            config,
            layout: Arc::new(Mutex::new(TerminalLayoutMetrics::default())),
            selection: Arc::new(Mutex::new(SelectionState::default())),
            scroll_handle: TerminalScrollHandle::default(),
            marked_text: None,
            hover_link: None,
            scroll_input: TerminalScrollInputState::default(),
            selection_frame_pending: false,
            pending_pty_resize: None,
            pty_resize_flush_pending: false,
            last_pty_resize_at: None,
            focus_in_subscription: None,
            focus_out_subscription: None,
            focus_observer: None,
            selection_autoscroll: None,
            _observe_model: observe_model,
            _observe_blink_manager: observe_blink_manager,
        }
    }

    // PTY resizes are debounced: a live window drag yields a new grid size
    // nearly every frame, and each SIGWINCH makes the running TUI repaint
    // its whole screen. The first resize after a quiet period goes out
    // immediately; bursts settle on the trailing edge with the final size.
    fn schedule_pty_resize(&mut self, cols: u16, rows: u16, cx: &mut Context<Self>) {
        self.pending_pty_resize = Some((cols, rows));
        if self.pty_resize_flush_pending {
            return;
        }
        let quiet = self
            .last_pty_resize_at
            .is_none_or(|at| at.elapsed() >= TERMINAL_PTY_RESIZE_DEBOUNCE);
        if quiet {
            self.flush_pending_pty_resize();
            return;
        }
        self.pty_resize_flush_pending = true;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |view: WeakEntity<Self>, cx| {
            timer.timer(TERMINAL_PTY_RESIZE_DEBOUNCE).await;
            let _ = view.update(cx, |view, _| {
                view.pty_resize_flush_pending = false;
                view.flush_pending_pty_resize();
            });
        })
        .detach();
    }

    fn flush_pending_pty_resize(&mut self) {
        let Some((cols, rows)) = self.pending_pty_resize.take() else {
            return;
        };
        self.last_pty_resize_at = Some(Instant::now());
        if let Err(error) = self.session.resize(cols, rows) {
            eprintln!("failed to resize terminal pty: {error}");
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    pub fn config(&self) -> &TerminalConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: TerminalConfig, cx: &mut Context<Self>) {
        self.renderer.font_family = config.font_family.clone();
        self.renderer.font_size = config.font_size;
        self.renderer.line_height_multiplier = config.line_height_multiplier;
        self.renderer.palette = config.colors.clone();
        self.renderer.fonts = TerminalFonts::new(&config.font_family);
        self.renderer.clear_cache();
        self.model.update(cx, |model, _| {
            model.update_config(config.colors.clone(), config.paste_images_as_paths)
        });
        self.config = config;
        cx.notify();
    }

    pub fn set_focus_observer<F>(&mut self, observer: F)
    where
        F: Fn(&mut Window, &mut Context<TerminalView>) + 'static,
    {
        self.focus_observer = Some(Arc::new(observer));
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if is_copy_keystroke(&event.keystroke) {
            if self.copy_selected_text(cx) {
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }

        if is_paste_keystroke(&event.keystroke) {
            let view = cx.entity();
            window.defer(cx, move |_window, cx| {
                let _ = view.update(cx, |terminal, cx| {
                    if let Some(text) = terminal.terminal_clipboard_paste_text(cx) {
                        terminal.paste_text(&text, cx);
                    }
                });
            });
            cx.stop_propagation();
            return;
        }

        if self.handle_terminal_keystroke(&event.keystroke, cx) {
            cx.stop_propagation();
        }
    }

    pub fn handle_terminal_keystroke(
        &mut self,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let mode = self.model.read(cx).mode();
        let Some(bytes) = keystroke_to_bytes(keystroke, mode) else {
            return false;
        };
        self.blink_manager
            .update(cx, TerminalBlinkManager::pause_blinking);
        self.clear_pending_view_scroll();
        self.model.update(cx, |model, cx| {
            model.prepare_input_viewport(cx);
            model.write_bytes(&bytes);
        });
        true
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let point = self.layout.lock().cell_at(event.position);
        let model_point = self.layout.lock().model_cell_at(event.position);
        if event.button == MouseButton::Left
            && event.modifiers.secondary()
            && let Some(link) = model_point.and_then(|point| self.link_at_cell(point, cx))
        {
            if let Err(error) = codux_runtime::app_commands::app_open_url(link.url.clone()) {
                eprintln!("failed to open terminal link {}: {error}", link.url);
            }
            self.hover_link = Some(link);
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if event.button == MouseButton::Left && event.modifiers.shift {
            if let Some(point) = model_point {
                let selection_point = self.selection_point_from_cell(point, cx);
                self.selection.lock().extend(selection_point);
                self.model
                    .update(cx, |model, _| model.update_selection(selection_point));
            }
            self.selection_autoscroll = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if event.button == MouseButton::Right && self.copy_selected_text(cx) {
            self.selection.lock().clear();
            self.model.update(cx, |model, _| model.clear_selection());
            self.selection_autoscroll = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if self.should_report_mouse(event.modifiers.shift, cx) {
            if let Some(point) = point {
                self.send_mouse_report(
                    Some(event.button),
                    point,
                    TerminalMouseUiEvent::Press,
                    event.modifiers,
                    cx,
                );
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }

        match event.button {
            MouseButton::Left => {
                if let Some(point) = model_point {
                    let selection_point = self.selection_point_from_cell(point, cx);
                    self.selection.lock().start(selection_point);
                    self.model
                        .update(cx, |model, _| model.start_selection(selection_point));
                } else {
                    self.selection.lock().clear();
                    self.model.update(cx, |model, _| model.clear_selection());
                }
                self.selection_autoscroll = None;
            }
            MouseButton::Middle => {
                if let Some(text) = self.terminal_clipboard_paste_text(cx) {
                    self.paste_text(&text, cx);
                }
            }
            MouseButton::Right | MouseButton::Navigate(_) => {}
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let selection_dragging = self.selection.lock().dragging;
        let drag_cell = self.layout.lock().drag_cell_at(event.position);
        if let Some((point, _)) = drag_cell {
            if self.should_report_mouse(event.modifiers.shift, cx) {
                self.send_mouse_report(
                    Some(event.button),
                    point,
                    TerminalMouseUiEvent::Release,
                    event.modifiers,
                    cx,
                );
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if selection_dragging {
                let point = self
                    .layout
                    .lock()
                    .model_cell_at(event.position)
                    .unwrap_or(point);
                let selection_point = self.selection_point_from_cell(point, cx);
                self.selection.lock().finish(selection_point);
                self.model
                    .update(cx, |model, _| model.update_selection(selection_point));
            }
        } else {
            self.selection.lock().dragging = false;
        }
        self.selection_autoscroll = None;
        cx.stop_propagation();
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            self.update_hover_link(event.position, event.modifiers, cx);
        } else if self.hover_link.take().is_some() {
            cx.notify();
        }

        if self.should_report_mouse(event.modifiers.shift, cx) {
            let Some(point) = self.layout.lock().cell_at(event.position) else {
                return;
            };
            self.send_mouse_report(
                event.pressed_button,
                point,
                TerminalMouseUiEvent::Move,
                event.modifiers,
                cx,
            );
            cx.stop_propagation();
            return;
        }
        if event.dragging() && self.selection.lock().dragging {
            let Some((point, scroll_lines)) = self.layout.lock().model_drag_cell_at(event.position)
            else {
                return;
            };
            let selection_point = self.selection_point_from_cell(point, cx);
            let selection_changed = self.selection.lock().update(selection_point);
            self.selection_autoscroll = (scroll_lines != 0).then_some(SelectionAutoScroll {
                edge_cell: point,
                lines: scroll_lines,
            });
            if !selection_changed && scroll_lines == 0 {
                cx.stop_propagation();
                return;
            }
            if selection_changed {
                self.model
                    .update(cx, |model, _| model.update_selection(selection_point));
            }
            if scroll_lines != 0 {
                self.queue_display_scroll(scroll_lines, cx);
            }
            cx.stop_propagation();
            self.schedule_selection_frame(cx);
        }
    }

    fn on_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_hover_link(window.mouse_position(), event.modifiers, cx);
    }

    fn on_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.scroll_input.should_suppress_residual_scroll(event) {
            cx.stop_propagation();
            return;
        }

        let pixels: f32 = event.delta.pixel_delta(px(20.0)).y.into();
        self.scroll_input.pending_pixels += pixels;
        let lines = (self.scroll_input.pending_pixels / 20.0) as i32;
        if lines != 0 {
            self.scroll_input.pending_pixels -= lines as f32 * 20.0;
            let point = self.layout.lock().cell_at(event.position);
            if let Some(point) =
                point.filter(|_| self.should_report_mouse(event.modifiers.shift, cx))
            {
                let button = if lines > 0 {
                    MouseButton::Navigate(NavigationDirection::Back)
                } else {
                    MouseButton::Navigate(NavigationDirection::Forward)
                };
                for _ in 0..lines.unsigned_abs().min(80) {
                    self.send_mouse_report(
                        Some(button),
                        point,
                        TerminalMouseUiEvent::Wheel,
                        event.modifiers,
                        cx,
                    );
                }
            } else if should_send_alternate_scroll(
                self.model.read(cx).mode(),
                event.modifiers.shift,
            ) {
                let sequence = if lines > 0 { b"\x1bOA" } else { b"\x1bOB" };
                for _ in 0..lines.unsigned_abs().min(80) {
                    self.write_bytes(sequence, cx);
                }
            } else {
                self.queue_display_scroll(lines, cx);
            }
            cx.stop_propagation();
        }
    }

    fn queue_display_scroll(&mut self, lines: i32, cx: &mut Context<Self>) {
        self.scroll_input.pending_lines = self.scroll_input.pending_lines.saturating_add(lines);
        self.schedule_scroll_flush(cx);
    }

    fn schedule_selection_frame(&mut self, cx: &mut Context<Self>) {
        if self.selection_frame_pending {
            return;
        }

        self.selection_frame_pending = true;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |terminal: WeakEntity<Self>, cx| {
            timer.timer(TERMINAL_SCROLL_FRAME_INTERVAL).await;
            let _ = terminal.update(cx, |terminal, cx| {
                terminal.selection_frame_pending = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn schedule_scroll_flush(&mut self, cx: &mut Context<Self>) {
        if self.scroll_input.frame_pending {
            return;
        }

        self.scroll_input.frame_pending = true;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |terminal: WeakEntity<Self>, cx| {
            timer.timer(TERMINAL_SCROLL_FRAME_INTERVAL).await;
            let _ = terminal.update(cx, |terminal, cx| {
                if let Some(flush) = terminal.flush_pending_scroll(cx) {
                    if let Some(lines) = flush.next_lines {
                        terminal.scroll_input.pending_lines =
                            terminal.scroll_input.pending_lines.saturating_add(lines);
                        terminal.schedule_scroll_flush(cx);
                    }
                    if flush.did_scroll {
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn flush_pending_scroll(&mut self, cx: &mut Context<Self>) -> Option<ScrollFlushResult> {
        self.scroll_input.frame_pending = false;
        let lines = std::mem::take(&mut self.scroll_input.pending_lines);
        if lines == 0 {
            return None;
        }

        let queued_scroll = self
            .model
            .update(cx, |model, _| model.scroll_display(lines));
        if queued_scroll
            && let Some(autoscroll) = self.selection_autoscroll
            && self.selection.lock().dragging
        {
            let (did_scroll, content) = self
                .model
                .update(cx, |model, _| model.apply_pending_scroll_for_selection());
            if !did_scroll {
                return Some(ScrollFlushResult {
                    did_scroll: false,
                    next_lines: None,
                });
            }
            let point = selection_point_from_cell(autoscroll.edge_cell, &content);
            let _ = self.selection.lock().update(point);
            self.model
                .update(cx, |model, _| model.update_selection(point));
            return Some(ScrollFlushResult {
                did_scroll,
                next_lines: Some(autoscroll.lines),
            });
        }

        Some(ScrollFlushResult {
            did_scroll: queued_scroll,
            next_lines: None,
        })
    }

    fn process_events(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let previous_generation = {
            let model = self.model.read(cx);
            model.viewport_generation()
        };
        self.model
            .update(cx, |model, cx| model.process_pending_events(cx));
        let (is_remote, generation) = {
            let model = self.model.read(cx);
            (model.remote_viewer(), model.viewport_generation())
        };
        if !is_remote && generation != previous_generation {
            if let Err(error) = self.session.force_local_viewport_if_current_owner() {
                eprintln!("failed to restore desktop terminal viewport: {error}");
            }
        }
    }

    fn write_bytes(&self, bytes: &[u8], cx: &mut Context<Self>) {
        self.model.update(cx, |model, _| model.write_bytes(bytes));
    }

    fn report_focus_change(&self, focused: bool, cx: &mut Context<Self>) {
        self.model
            .update(cx, |model, _| model.report_focus_change(focused));
    }

    fn ensure_focus_report_subscriptions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_in_subscription.is_none() {
            let focus_handle = self.focus_handle.clone();
            self.focus_in_subscription =
                Some(cx.on_focus(&focus_handle, window, |view, window, cx| {
                    view.model.update(cx, |model, _| model.set_focused(true));
                    view.blink_manager.update(cx, TerminalBlinkManager::enable);
                    view.report_focus_change(true, cx);
                    if let Some(observer) = view.focus_observer.clone() {
                        cx.defer_in(window, move |_, window, cx| {
                            observer(window, cx);
                        });
                    }
                    cx.notify();
                }));
        }
        if self.focus_out_subscription.is_none() {
            let focus_handle = self.focus_handle.clone();
            self.focus_out_subscription =
                Some(cx.on_focus_out(&focus_handle, window, |view, _, _, cx| {
                    view.model.update(cx, |model, _| model.set_focused(false));
                    view.blink_manager.update(cx, TerminalBlinkManager::disable);
                    view.report_focus_change(false, cx);
                    cx.notify();
                }));
        }
    }

    fn paste_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.blink_manager
            .update(cx, TerminalBlinkManager::pause_blinking);
        self.clear_pending_view_scroll();
        self.model.update(cx, |model, cx| {
            model.prepare_input_viewport(cx);
            model.paste_text(text);
        });
    }

    fn drop_external_paths(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = terminal_paths_input(paths.paths()) else {
            return;
        };
        self.focus_handle.focus(window, cx);
        self.paste_text(&text, cx);
    }

    fn terminal_clipboard_paste_text(&self, cx: &mut App) -> Option<String> {
        terminal_clipboard_paste_text(cx, self.config.paste_images_as_paths)
    }

    fn clear_pending_view_scroll(&mut self) {
        self.scroll_input.prepare_for_keyboard_input();
        self.selection_autoscroll = None;
    }

    fn should_report_mouse(&self, shift_pressed: bool, cx: &App) -> bool {
        !shift_pressed && self.model.read(cx).mode().mouse_tracking
    }

    fn send_mouse_report(
        &mut self,
        button: Option<MouseButton>,
        point: TerminalCellPoint,
        kind: TerminalMouseUiEvent,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        let mode = self.model.read(cx).mode();
        let Some(sequence) = terminal_mouse_ui_event_bytes(button, point, kind, modifiers, mode)
        else {
            return;
        };
        self.write_bytes(&sequence, cx);
    }

    fn update_hover_link(
        &mut self,
        position: Point<Pixels>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        let next = self
            .model_cell_at(position)
            .and_then(|point| self.link_at_cell(point, cx));
        if modifiers.secondary() && self.hover_link != next {
            self.hover_link = next;
            cx.notify();
        } else if !modifiers.secondary() && self.hover_link.take().is_some() {
            cx.notify();
        }
    }

    fn link_at_cell(&self, point: TerminalCellPoint, cx: &App) -> Option<TerminalLink> {
        let content = self.model.read(cx).snapshot();
        terminal_link_at_cell(&content, point)
    }

    fn model_cell_at(&self, position: Point<Pixels>) -> Option<TerminalCellPoint> {
        self.layout.lock().model_cell_at(position)
    }

    fn selected_text(&self, cx: &App) -> Option<String> {
        let text = self.model.read(cx).selected_text()?;
        (!text.is_empty()).then_some(text)
    }

    fn selection_point_from_cell(
        &self,
        point: TerminalCellPoint,
        cx: &App,
    ) -> TerminalSelectionPoint {
        let content = self.model.read(cx).snapshot();
        selection_point_from_cell(point, &content)
    }

    fn copy_selected_text(&self, cx: &mut App) -> bool {
        let Some(text) = self.selected_text(cx) else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    fn set_marked_text(&mut self, text: String, cx: &mut Context<Self>) {
        if text.is_empty() {
            self.clear_marked_text(cx);
            return;
        }
        self.marked_text = Some(text);
        cx.notify();
    }

    fn clear_marked_text(&mut self, cx: &mut Context<Self>) {
        if self.marked_text.take().is_some() {
            cx.notify();
        }
    }

    fn marked_text_range(&self) -> Option<Range<usize>> {
        self.marked_text
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }
}

fn should_send_alternate_scroll(mode: TerminalInputMode, shift_pressed: bool) -> bool {
    !shift_pressed && mode.alternate_screen && mode.alternate_scroll
}
