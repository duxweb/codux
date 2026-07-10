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
    marked_text: Option<TerminalMarkedText>,
    hover_link: Option<TerminalLink>,
    suppressed_text_input: Option<TerminalSuppressedTextInput>,
    scroll_input: TerminalScrollInputState,
    selection_frame_pending: bool,
    pending_pty_resize: Option<(u16, u16)>,
    pty_resize_flush_pending: bool,
    last_pty_resize_at: Option<Instant>,
    focus_in_subscription: Option<Subscription>,
    focus_out_subscription: Option<Subscription>,
    focus_observer: Option<Arc<dyn Fn(&mut Window, &mut Context<TerminalView>)>>,
    title_observer: Option<Arc<dyn Fn(Option<String>, &mut Context<TerminalView>)>>,
    search_observer: Option<Arc<dyn Fn(bool, &mut Context<TerminalView>)>>,
    osc_title: Option<String>,
    search_open: bool,
    search_input: Option<Entity<InputState>>,
    search_matches: Vec<SelectionRange>,
    search_match_index: usize,
    _search_input_subscription: Option<Subscription>,
    link_opener: Option<Arc<dyn Fn(String, &mut Window, &mut Context<TerminalView>)>>,
    selection_autoscroll: Option<SelectionAutoScroll>,
    // Right-click went to the app as a mouse report; the context menu must stay closed.
    context_menu_suppressed: bool,
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

struct TerminalSuppressedTextInput {
    text: String,
    expires_at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalMarkedText {
    text: String,
    selected_range_utf16: Range<usize>,
}

impl TerminalMarkedText {
    fn new(text: String, selected_range_utf16: Option<Range<usize>>) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let len = text.encode_utf16().count();
        let selected_range_utf16 = clamp_utf16_range(selected_range_utf16.unwrap_or(len..len), len);
        Some(Self {
            text,
            selected_range_utf16,
        })
    }

    fn marked_range_utf16(&self) -> Range<usize> {
        0..self.text.encode_utf16().count()
    }
}

const TERMINAL_TEXT_INPUT_SUPPRESS_WINDOW: Duration = Duration::from_millis(350);

const TERMINAL_SEARCH_MAX_MATCHES: usize = 1000;

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
        session_event_rx: flume::Receiver<TerminalUiEvent>,
        session_event_wake_rx: flume::Receiver<()>,
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
                session_event_wake_rx,
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
        let observe_model = cx.observe(&model, |view: &mut Self, model, cx| {
            let title = model.read(cx).osc_title();
            if title != view.osc_title {
                view.osc_title = title.clone();
                // The observer runs while this view is under its update lease;
                // it must hand the VALUE to the app and never trigger a
                // read-back of this view (entity re-entrancy panic).
                if let Some(observer) = view.title_observer.clone() {
                    observer(title, cx);
                }
            }
            cx.notify();
        });
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
            suppressed_text_input: None,
            scroll_input: TerminalScrollInputState::default(),
            selection_frame_pending: false,
            pending_pty_resize: None,
            pty_resize_flush_pending: false,
            last_pty_resize_at: None,
            focus_in_subscription: None,
            focus_out_subscription: None,
            focus_observer: None,
            title_observer: None,
            search_observer: None,
            osc_title: None,
            search_open: false,
            search_input: None,
            search_matches: Vec::new(),
            search_match_index: 0,
            _search_input_subscription: None,
            link_opener: None,
            selection_autoscroll: None,
            context_menu_suppressed: false,
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

    pub fn set_render_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.model
            .update(cx, |model, cx| model.set_render_visible(visible, cx));
        self.blink_manager
            .update(cx, |manager, cx| manager.set_render_visible(visible, cx));
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

    pub fn set_link_opener<F>(&mut self, opener: F)
    where
        F: Fn(String, &mut Window, &mut Context<TerminalView>) + 'static,
    {
        self.link_opener = Some(Arc::new(opener));
    }

    pub fn set_title_observer<F>(&mut self, observer: F)
    where
        F: Fn(Option<String>, &mut Context<TerminalView>) + 'static,
    {
        self.title_observer = Some(Arc::new(observer));
    }

    pub fn set_search_observer<F>(&mut self, observer: F)
    where
        F: Fn(bool, &mut Context<TerminalView>) + 'static,
    {
        self.search_observer = Some(Arc::new(observer));
    }

    pub fn search_is_open(&self) -> bool {
        self.search_open
    }

    /// Shell-set window title (OSC 0/2); None when the shell never set one.
    pub fn osc_title(&self) -> Option<&str> {
        self.osc_title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
    }

    fn search_bar_icon_button(
        &self,
        id: &'static str,
        icon: HeroIconName,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .flex_none()
            .size(px(20.0))
            .rounded(px(4.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().secondary))
            .on_click(cx.listener(move |view, _, window, cx| on_click(view, window, cx)))
            .child(
                Icon::new(icon)
                    .size_3()
                    .text_color(cx.theme().muted_foreground),
            )
    }

    fn render_search_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(input) = self.search_input.clone() else {
            return div().into_any_element();
        };
        let count = self.search_matches.len();
        let counter = if count == 0 {
            "0/0".to_string()
        } else {
            format!("{}/{}", self.search_match_index + 1, count)
        };
        div()
            .id("terminal-search-bar")
            .absolute()
            .top(px(8.0))
            .right(px(16.0))
            .occlude()
            .flex()
            .items_center()
            .gap_1()
            .px(px(6.0))
            .py(px(4.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_md()
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key.eq_ignore_ascii_case("escape") {
                    view.close_search(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                Input::new(&input)
                    .appearance(false)
                    .with_size(ComponentSize::Small)
                    .w(px(180.0)),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(cx.theme().muted_foreground)
                    .child(counter),
            )
            .child(self.search_bar_icon_button(
                "terminal-search-prev",
                HeroIconName::ChevronUp,
                |view, _window, cx| view.step_search(-1, cx),
                cx,
            ))
            .child(self.search_bar_icon_button(
                "terminal-search-next",
                HeroIconName::ChevronDown,
                |view, _window, cx| view.step_search(1, cx),
                cx,
            ))
            .child(self.search_bar_icon_button(
                "terminal-search-close",
                HeroIconName::XMark,
                |view, window, cx| view.close_search(window, cx),
                cx,
            ))
            .into_any_element()
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
            if let Some(text) = self.terminal_clipboard_paste_text(cx) {
                self.suppress_text_input_echo(&text);
                let view = cx.entity();
                window.defer(cx, move |_window, cx| {
                    let _ = view.update(cx, |terminal, cx| {
                        terminal.paste_text(&text, cx);
                    });
                });
            }
            cx.stop_propagation();
            return;
        }

        if self.handle_app_terminal_keystroke(&event.keystroke, window, cx) {
            cx.stop_propagation();
        }
    }

    /// Window-level shortcuts (select-all) plus the PTY keystroke path. Find
    /// is not handled here: the global cmd-f binding consumes the keystroke
    /// and routes through the EditorSearch action into open_search.
    pub fn handle_app_terminal_keystroke(
        &mut self,
        keystroke: &Keystroke,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if is_select_all_keystroke(keystroke) {
            self.select_all(cx);
            return true;
        }
        // cmd+up/down: jump between OSC 133 prompt marks (Ghostty parity);
        // falls through to the PTY when the shell emits no marks.
        if let Some(direction) = prompt_jump_direction(keystroke)
            && self
                .model
                .update(cx, |model, cx| model.jump_to_prompt(direction, cx))
        {
            return true;
        }
        self.handle_terminal_keystroke(keystroke, cx)
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        let range = self.model.update(cx, |model, _| model.select_all());
        self.selection.lock().set_range(range);
        cx.notify();
    }

    fn clear_screen(&mut self, cx: &mut Context<Self>) {
        self.selection.lock().clear();
        self.selection_autoscroll = None;
        self.model.update(cx, |model, _| model.clear_screen());
        cx.notify();
    }

    pub fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_open = self.search_open;
        self.search_open = true;
        if !was_open {
            if let Some(observer) = self.search_observer.clone() {
                observer(true, cx);
            }
        }
        let input = match self.search_input.clone() {
            Some(input) => input,
            None => {
                let placeholder = codux_runtime::i18n::translate(
                    &self.config.language,
                    "terminal.search.placeholder",
                    "Search",
                );
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
                self._search_input_subscription =
                    Some(cx.subscribe_in(&input, window, Self::on_search_input_event));
                self.search_input = Some(input.clone());
                input
            }
        };
        input.update(cx, |state, cx| state.focus(window, cx));
        // Reopening with a previous query: refresh its matches against the
        // current buffer so Enter cycling works right away.
        let query = input.read(cx).value().to_string();
        if !query.trim().is_empty() {
            self.run_search(query, cx);
        }
        cx.notify();
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.search_open {
            return;
        }
        self.search_open = false;
        if let Some(observer) = self.search_observer.clone() {
            observer(false, cx);
        }
        self.search_matches.clear();
        self.search_match_index = 0;
        self.selection.lock().clear();
        self.model.update(cx, |model, _| model.clear_selection());
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub fn search_contains_focused(&self, window: &Window, cx: &App) -> bool {
        self.search_open
            && self
                .search_input
                .as_ref()
                .is_some_and(|input| input.read(cx).focus_handle(cx).contains_focused(window, cx))
    }

    fn on_search_input_event(
        &mut self,
        input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                let query = input.read(cx).value().to_string();
                self.run_search(query, cx);
                cx.notify();
            }
            InputEvent::PressEnter { shift, .. } => {
                if self.search_matches.is_empty() {
                    let query = input.read(cx).value().to_string();
                    self.run_search(query, cx);
                } else if *shift {
                    self.step_search(-1, cx);
                } else {
                    self.step_search(1, cx);
                }
                cx.notify();
            }
            _ => {}
        }
        let _ = window;
    }

    fn run_search(&mut self, query: String, cx: &mut Context<Self>) {
        self.search_match_index = 0;
        if query.trim().is_empty() {
            self.search_matches.clear();
            self.selection.lock().clear();
            self.model.update(cx, |model, _| model.clear_selection());
            return;
        }
        self.search_matches = self
            .model
            .read(cx)
            .search_buffer(&query, TERMINAL_SEARCH_MAX_MATCHES);
        if self.search_matches.is_empty() {
            self.selection.lock().clear();
            self.model.update(cx, |model, _| model.clear_selection());
        } else {
            self.jump_to_search_match(0, cx);
        }
    }

    fn step_search(&mut self, delta: i32, cx: &mut Context<Self>) {
        let count = self.search_matches.len();
        if count == 0 {
            return;
        }
        let index = (self.search_match_index as i32 + delta).rem_euclid(count as i32) as usize;
        self.jump_to_search_match(index, cx);
        cx.notify();
    }

    fn jump_to_search_match(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(range) = self.search_matches.get(index).copied() else {
            return;
        };
        self.search_match_index = index;
        self.selection.lock().set_range(range);
        self.model.update(cx, |model, _| {
            model.selection.set_range(range);
            model.scroll_line_to_center(range.start.line);
        });
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
        self.prepare_local_viewport_for_input(cx);
        self.blink_manager
            .update(cx, TerminalBlinkManager::pause_blinking);
        self.clear_pending_view_scroll();
        self.model.update(cx, |model, cx| {
            model.prepare_input_viewport(cx);
            model.write_bytes(&bytes);
        });
        true
    }

    fn suppress_text_input_echo(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.suppressed_text_input = Some(TerminalSuppressedTextInput {
            text: text.to_string(),
            expires_at: Instant::now() + TERMINAL_TEXT_INPUT_SUPPRESS_WINDOW,
        });
    }

    fn take_suppressed_text_input_echo(&mut self, text: &str) -> bool {
        let Some(suppressed) = self.suppressed_text_input.as_ref() else {
            return false;
        };
        if Instant::now() > suppressed.expires_at {
            self.suppressed_text_input = None;
            return false;
        }
        if suppressed.text == text {
            self.suppressed_text_input = None;
            return true;
        }
        false
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
            if let Some(opener) = self.link_opener.clone() {
                opener(link.url.clone(), window, cx);
            } else if let Err(error) = codux_runtime::app_commands::app_open_url(link.url.clone()) {
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

        if event.button == MouseButton::Right {
            // The context menu element opens on this same event; keep it closed
            // when the click is forwarded to a mouse-reporting app instead.
            self.context_menu_suppressed = self.should_report_mouse(event.modifiers.shift, cx);
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
                    match event.click_count {
                        // Triple-click: whole logical line (follows soft wraps).
                        count if count >= 3 => {
                            if let Some(range) = self
                                .model
                                .update(cx, |model, _| model.select_line_at(selection_point))
                            {
                                self.selection.lock().set_range(range);
                            }
                        }
                        // Double-click: the word under the cursor; a separator
                        // cell falls back to a plain caret.
                        2 => {
                            let range = self
                                .model
                                .update(cx, |model, _| model.select_word_at(selection_point));
                            match range {
                                Some(range) => self.selection.lock().set_range(range),
                                None => {
                                    self.selection.lock().start(selection_point);
                                    self.model
                                        .update(cx, |model, _| model.start_selection(selection_point));
                                }
                            }
                        }
                        _ => {
                            self.selection.lock().start(selection_point);
                            self.model
                                .update(cx, |model, _| model.start_selection(selection_point));
                        }
                    }
                } else {
                    self.selection.lock().clear();
                    self.model.update(cx, |model, _| model.clear_selection());
                }
                self.selection_autoscroll = None;
            }
            MouseButton::Middle => {
                if let Some(text) = self.terminal_clipboard_paste_text(cx) {
                    self.suppress_text_input_echo(&text);
                    self.paste_text(&text, cx);
                }
            }
            // Right-click opens the context menu (handled by the wrapping element).
            MouseButton::Right => {}
            MouseButton::Navigate(_) => {}
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
                let sequence = alternate_scroll_sequence(
                    lines > 0,
                    self.model.read(cx).mode().application_cursor,
                );
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
            if let Err(error) = self.session.refresh_local_viewport_if_current_owner() {
                eprintln!("failed to restore desktop terminal viewport: {error}");
            }
        }
    }

    fn write_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.prepare_local_viewport_for_input(cx);
        self.model.update(cx, |model, _| model.write_bytes(bytes));
    }

    fn prepare_local_viewport_for_input(&mut self, cx: &mut Context<Self>) {
        let reclaimed = !self.session.local_viewport_owns();
        if let Err(error) = self.session.restore_local_viewport() {
            eprintln!("failed to reclaim terminal viewport on input: {error}");
        }
        if reclaimed {
            cx.notify();
        }
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
        self.prepare_local_viewport_for_input(cx);
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

    fn set_marked_text(
        &mut self,
        text: String,
        selected_range_utf16: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        let Some(marked_text) = TerminalMarkedText::new(text, selected_range_utf16) else {
            self.clear_marked_text(cx);
            return;
        };
        if terminal_trace_enabled() {
            terminal_trace(&format!(
                "ime_marked_text len_utf16={} selected={:?}",
                marked_text.marked_range_utf16().end,
                marked_text.selected_range_utf16
            ));
        }
        if self.marked_text.as_ref() != Some(&marked_text) {
            self.marked_text = Some(marked_text);
            cx.notify();
        }
    }

    fn clear_marked_text(&mut self, cx: &mut Context<Self>) {
        if self.marked_text.take().is_some() {
            cx.notify();
        }
    }

    fn marked_text_range(&self) -> Option<Range<usize>> {
        self.marked_text
            .as_ref()
            .map(TerminalMarkedText::marked_range_utf16)
    }

    fn marked_text_selection_range(&self) -> Option<Range<usize>> {
        self.marked_text
            .as_ref()
            .map(|marked_text| marked_text.selected_range_utf16.clone())
    }

    fn marked_text_for_range(&self, range_utf16: Range<usize>) -> Option<String> {
        let marked_text = self.marked_text.as_ref()?;
        let len = marked_text.text.encode_utf16().count();
        let range = clamp_utf16_range(range_utf16, len);
        Some(utf16_substring(&marked_text.text, range))
    }
}

fn should_send_alternate_scroll(mode: TerminalInputMode, shift_pressed: bool) -> bool {
    !shift_pressed && mode.alternate_screen && mode.alternate_scroll
}

// Alternate-scroll (wheel over an alt-screen pager) emits cursor-key sequences,
// and xterm honors DECCKM: SS3 (ESC O) in application-cursor mode, CSI (ESC [)
// in normal mode. We used to always send SS3, so a normal-mode app (Claude's
// pager) ignored the wheel while a mouse-tracking app (codex) scrolled fine.
fn alternate_scroll_sequence(scroll_up: bool, application_cursor: bool) -> &'static [u8] {
    match (scroll_up, application_cursor) {
        (true, true) => b"\x1bOA",
        (true, false) => b"\x1b[A",
        (false, true) => b"\x1bOB",
        (false, false) => b"\x1b[B",
    }
}
