use alacritty_terminal::{
    event::{Event, EventListener, WindowSize},
    grid::Dimensions,
    index::{Column, Line, Point as TerminalPoint},
    term::{
        Config as AlacrittyConfig, Term, TermMode,
        cell::{Cell, Flags},
        color::Colors,
    },
    vte::ansi::{Color, NamedColor, Processor, Rgb},
};
use anyhow::Result;
use codux_runtime::terminal_pty::{
    TerminalEvent, TerminalInputSnapshot, TerminalManager, TerminalOutputSnapshot,
    TerminalPtyConfig, TerminalPtySession, TerminalPtySessionHandle,
};
use gpui::{
    App, AppContext, Bounds, ClipboardItem, Context, Edges, Entity, FocusHandle, Font,
    FontFeatures, FontStyle, FontWeight, Hsla, InputHandler, InteractiveElement, IntoElement,
    KeyDownEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    NavigationDirection, ParentElement, Pixels, Point, Render, ScrollWheelEvent, SharedString,
    Size, Styled, Task, TextAlign, TextRun, UTF16Selection, UnderlineStyle, WeakEntity, Window,
    canvas, div, px, quad, rgb, transparent_black,
};
use parking_lot::Mutex;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::Write,
    ops::Range,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

pub use codux_runtime::terminal_pty::TerminalLaunchContext;

pub struct TerminalPane {
    pub view: Entity<TerminalView>,
    session: Arc<TerminalPtySession>,
}

impl TerminalPane {
    pub fn spawn_with_context_and_config<C>(
        cx: &mut C,
        terminal_manager: Arc<TerminalManager>,
        context: Option<&TerminalLaunchContext>,
        terminal_config: TerminalConfig,
    ) -> Result<Self>
    where
        C: AppContext,
    {
        let mut config =
            context
                .map(TerminalLaunchContext::to_config)
                .unwrap_or(TerminalPtyConfig {
                    ..Default::default()
                });
        config.cols = Some(terminal_config.cols as u16);
        config.rows = Some(terminal_config.rows as u16);
        config.scrollback_lines = Some(terminal_config.scrollback);
        let (session_event_tx, session_event_rx) = mpsc::channel();
        let emit = Arc::new(move |event| match event {
            TerminalEvent::Exit { .. } => {
                let _ = session_event_tx.send(TerminalUiEvent::Exit);
            }
            TerminalEvent::Error { message, .. } => {
                let _ = session_event_tx.send(TerminalUiEvent::Error(message));
            }
            TerminalEvent::Output { .. } => {}
        });
        let terminal_id = config.terminal_id.clone();
        let attach_started_at = Instant::now();
        let (session, output_rx) =
            terminal_manager.attach_or_create_with_context(config, context, emit)?;
        codux_runtime::runtime_trace::runtime_trace(
            "terminal-restore",
            &format!(
                "pty_attach elapsed_ms={} terminal_id={}",
                attach_started_at.elapsed().as_millis(),
                terminal_id.as_deref().unwrap_or("none")
            ),
        );
        let resize_handle = session.clone_handle();
        let writer = TerminalSessionWriter::new(session.clone());
        let view_started_at = Instant::now();
        let view = cx.new(|cx| {
            TerminalView::new(
                writer,
                output_rx,
                session_event_rx,
                resize_handle,
                terminal_config,
                cx,
            )
        });
        codux_runtime::runtime_trace::runtime_trace(
            "terminal-restore",
            &format!(
                "view_create elapsed_ms={} terminal_id={}",
                view_started_at.elapsed().as_millis(),
                terminal_id.as_deref().unwrap_or("none")
            ),
        );

        Ok(Self { view, session })
    }

    pub fn send_text(&self, text: &str) -> Result<()> {
        self.session.write(text.as_bytes())
    }

    pub fn input_snapshot(&self) -> TerminalInputSnapshot {
        self.session.input_snapshot()
    }

    pub fn output_snapshot(&self) -> TerminalOutputSnapshot {
        self.session.output_snapshot()
    }
}

#[derive(Clone)]
struct TerminalSessionWriter {
    session: Arc<TerminalPtySession>,
}

impl TerminalSessionWriter {
    fn new(session: Arc<TerminalPtySession>) -> Self {
        Self { session }
    }
}

impl Write for TerminalSessionWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.session.write(buf).map_err(std::io::Error::other)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct TerminalConfig {
    pub cols: usize,
    pub rows: usize,
    pub font_family: String,
    pub font_size: Pixels,
    pub scrollback: usize,
    pub line_height_multiplier: f32,
    pub padding: Edges<Pixels>,
    pub colors: ColorPalette,
}

pub fn terminal_config() -> TerminalConfig {
    let colors = ColorPalette::builder()
        .background(0x11, 0x14, 0x1A)
        .foreground(0xD6, 0xDA, 0xE2)
        .cursor(0xF3, 0xC9, 0x6B)
        .black(0x1A, 0x1D, 0x24)
        .red(0xF2, 0x72, 0x72)
        .green(0x7D, 0xD8, 0x92)
        .yellow(0xE8, 0xC6, 0x6A)
        .blue(0x7A, 0xB8, 0xFF)
        .magenta(0xD6, 0x8A, 0xFF)
        .cyan(0x66, 0xD9, 0xE8)
        .white(0xD6, 0xDA, 0xE2)
        .bright_black(0x5C, 0x65, 0x73)
        .bright_red(0xFF, 0x9B, 0x9B)
        .bright_green(0xA8, 0xEE, 0xB7)
        .bright_yellow(0xF4, 0xD9, 0x86)
        .bright_blue(0xA6, 0xD0, 0xFF)
        .bright_magenta(0xE6, 0xB3, 0xFF)
        .bright_cyan(0x9E, 0xF0, 0xF5)
        .bright_white(0xFF, 0xFF, 0xFF)
        .build();

    TerminalConfig {
        font_family: default_terminal_font_family().into(),
        font_size: px(14.0),
        cols: 100,
        rows: 32,
        scrollback: 10_000,
        line_height_multiplier: 1.22,
        padding: Edges::all(px(10.0)),
        colors,
    }
}

pub fn terminal_config_with_font_family(font_family: &str) -> TerminalConfig {
    let mut config = terminal_config();
    let font_family = font_family.trim();
    if !font_family.is_empty() {
        config.font_family = font_family.to_string();
    }
    config
}

fn terminal_text_width(text: &str) -> usize {
    text.chars()
        .map(|ch| {
            if ch.is_ascii()
                || matches!(
                    ch as u32,
                    0x0300..=0x036F
                        | 0x1AB0..=0x1AFF
                        | 0x1DC0..=0x1DFF
                        | 0x20D0..=0x20FF
                        | 0xFE20..=0xFE2F
                )
            {
                1
            } else {
                2
            }
        })
        .sum::<usize>()
        .max(1)
}

fn default_terminal_font_family() -> &'static str {
    if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "Liberation Mono"
    }
}

const TERMINAL_SCROLL_FRAME_INTERVAL: Duration = Duration::from_millis(16);

pub struct TerminalView {
    state: TerminalState,
    renderer: TerminalRenderer,
    focus_handle: FocusHandle,
    stdin_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    resize_handle: TerminalPtySessionHandle,
    event_rx: mpsc::Receiver<TerminalUiEvent>,
    session_event_rx: mpsc::Receiver<TerminalUiEvent>,
    config: TerminalConfig,
    layout: Arc<Mutex<TerminalLayoutMetrics>>,
    selection: Arc<Mutex<SelectionState>>,
    marked_text: Option<String>,
    title: Option<String>,
    bell_count: usize,
    exited: bool,
    cursor_visible: bool,
    pending_scroll_lines: i32,
    pending_scroll_pixels: f32,
    scroll_frame_pending: bool,
    selection_autoscroll: Option<SelectionAutoScroll>,
    _reader_task: Task<()>,
    _cursor_blink_task: Task<()>,
}

impl TerminalView {
    fn new<W>(
        stdin_writer: W,
        bytes_rx: flume::Receiver<Vec<u8>>,
        session_event_rx: mpsc::Receiver<TerminalUiEvent>,
        resize_handle: TerminalPtySessionHandle,
        config: TerminalConfig,
        cx: &mut Context<Self>,
    ) -> Self
    where
        W: Write + Send + 'static,
    {
        let (event_tx, event_rx) = mpsc::channel();
        let state = TerminalState::new(
            config.cols,
            config.rows,
            config.scrollback,
            GpuiEventProxy::new(event_tx.clone()),
        );
        let renderer = TerminalRenderer::new(
            config.font_family.clone(),
            config.font_size,
            config.line_height_multiplier,
            config.colors.clone(),
        );
        let focus_handle = cx.focus_handle();
        let stdin_writer = Arc::new(Mutex::new(Box::new(stdin_writer) as Box<dyn Write + Send>));

        let reader_task = cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            while let Ok(bytes) = bytes_rx.recv_async().await {
                if this
                    .update(cx, |view, cx| {
                        view.cursor_visible = true;
                        view.state.process_bytes(&bytes);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let blink_timer = cx.background_executor().clone();
        let cursor_blink_task = cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                blink_timer.timer(Duration::from_millis(500)).await;
                if this
                    .update(cx, |view, cx| {
                        view.cursor_visible = !view.cursor_visible;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            state,
            renderer,
            focus_handle,
            stdin_writer,
            resize_handle,
            event_rx,
            session_event_rx,
            config,
            layout: Arc::new(Mutex::new(TerminalLayoutMetrics::default())),
            selection: Arc::new(Mutex::new(SelectionState::default())),
            marked_text: None,
            title: None,
            bell_count: 0,
            exited: false,
            cursor_visible: true,
            pending_scroll_lines: 0,
            pending_scroll_pixels: 0.0,
            scroll_frame_pending: false,
            selection_autoscroll: None,
            _reader_task: reader_task,
            _cursor_blink_task: cursor_blink_task,
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub fn config(&self) -> &TerminalConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: TerminalConfig, cx: &mut Context<Self>) {
        self.renderer.font_family = config.font_family.clone();
        self.renderer.font_size = config.font_size;
        self.renderer.line_height_multiplier = config.line_height_multiplier;
        self.renderer.palette = config.colors.clone();
        self.config = config;
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        if is_copy_keystroke(&event.keystroke) {
            if let Some(text) = self.selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }

        if is_paste_keystroke(&event.keystroke) {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.paste_text(&text);
                cx.stop_propagation();
                return;
            }
        }

        if let Some(bytes) = keystroke_to_bytes(&event.keystroke, self.state.mode()) {
            self.write_bytes(&bytes);
            cx.stop_propagation();
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let point = self.layout.lock().cell_at(event.position);
        if self.should_report_mouse(event.modifiers.shift) {
            if let Some(point) = point {
                self.send_mouse_report(
                    Some(event.button),
                    point,
                    MouseReportKind::Press,
                    event.modifiers,
                );
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }

        match event.button {
            MouseButton::Left => {
                if let Some(point) = point {
                    self.selection
                        .lock()
                        .start(self.selection_point_from_cell(point));
                } else {
                    self.selection.lock().clear();
                }
                self.selection_autoscroll = None;
            }
            MouseButton::Middle => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.paste_text(&text);
                }
            }
            MouseButton::Right | MouseButton::Navigate(_) => {}
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some((point, _)) = self.layout.lock().drag_cell_at(event.position) {
            if self.should_report_mouse(event.modifiers.shift) {
                self.send_mouse_report(
                    Some(event.button),
                    point,
                    MouseReportKind::Release,
                    event.modifiers,
                );
                cx.stop_propagation();
                cx.notify();
                return;
            }
            self.selection
                .lock()
                .finish(self.selection_point_from_cell(point));
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
        if self.should_report_mouse(event.modifiers.shift) {
            let Some(point) = self.layout.lock().cell_at(event.position) else {
                return;
            };
            self.send_mouse_report(
                event.pressed_button,
                point,
                MouseReportKind::Move,
                event.modifiers,
            );
            cx.stop_propagation();
            return;
        }
        if event.dragging() {
            let Some((point, scroll_lines)) = self.layout.lock().drag_cell_at(event.position)
            else {
                return;
            };
            self.selection
                .lock()
                .update(self.selection_point_from_cell(point));
            self.selection_autoscroll = (scroll_lines != 0).then_some(SelectionAutoScroll {
                edge_cell: point,
                lines: scroll_lines,
            });
            if scroll_lines != 0 {
                self.queue_display_scroll(scroll_lines, cx);
            }
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn on_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pixels: f32 = event.delta.pixel_delta(px(20.0)).y.into();
        self.pending_scroll_pixels += pixels;
        let lines = (self.pending_scroll_pixels / 20.0) as i32;
        if lines != 0 {
            self.pending_scroll_pixels -= lines as f32 * 20.0;
            if let Some(point) = self.layout.lock().cell_at(event.position)
                && self.should_report_mouse(event.modifiers.shift)
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
                        MouseReportKind::Wheel,
                        event.modifiers,
                    );
                }
            } else if should_send_alternate_scroll(self.state.mode(), event.modifiers.shift) {
                let sequence = if lines > 0 { b"\x1bOA" } else { b"\x1bOB" };
                for _ in 0..lines.unsigned_abs().min(80) {
                    self.write_bytes(sequence);
                }
            } else {
                self.queue_display_scroll(lines, cx);
            }
            cx.stop_propagation();
        }
    }

    fn queue_display_scroll(&mut self, lines: i32, cx: &mut Context<Self>) {
        self.pending_scroll_lines = self.pending_scroll_lines.saturating_add(lines);
        self.schedule_scroll_flush(cx);
    }

    fn schedule_scroll_flush(&mut self, cx: &mut Context<Self>) {
        if self.scroll_frame_pending {
            return;
        }

        self.scroll_frame_pending = true;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |terminal: WeakEntity<Self>, cx| {
            timer.timer(TERMINAL_SCROLL_FRAME_INTERVAL).await;
            let _ = terminal.update(cx, |terminal, cx| {
                if let Some(flush) = terminal.flush_pending_scroll() {
                    if let Some(lines) = flush.next_lines {
                        terminal.pending_scroll_lines =
                            terminal.pending_scroll_lines.saturating_add(lines);
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

    fn flush_pending_scroll(&mut self) -> Option<ScrollFlushResult> {
        self.scroll_frame_pending = false;
        let lines = std::mem::take(&mut self.pending_scroll_lines);
        if lines == 0 {
            return None;
        }

        let did_scroll = self.state.scroll_display(lines);
        if did_scroll
            && let Some(autoscroll) = self.selection_autoscroll
            && self.selection.lock().dragging
        {
            let point = self.selection_point_from_cell(autoscroll.edge_cell);
            self.selection.lock().update(point);
            return Some(ScrollFlushResult {
                did_scroll,
                next_lines: Some(autoscroll.lines),
            });
        }

        Some(ScrollFlushResult {
            did_scroll,
            next_lines: None,
        })
    }

    fn process_events(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let mut should_notify = false;
        while let Ok(event) = self.event_rx.try_recv() {
            self.handle_ui_event(event, cx, &mut should_notify);
        }
        while let Ok(event) = self.session_event_rx.try_recv() {
            self.handle_ui_event(event, cx, &mut should_notify);
        }
        if should_notify {
            cx.notify();
        }
    }

    fn handle_ui_event(
        &mut self,
        event: TerminalUiEvent,
        cx: &mut Context<Self>,
        should_notify: &mut bool,
    ) {
        match event {
            TerminalUiEvent::Wakeup => *should_notify = true,
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
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.write_bytes(text.as_bytes());
                }
            }
            TerminalUiEvent::ColorRequest(index, format) => {
                let color = self.state.term.lock().colors()[index]
                    .unwrap_or_else(|| terminal_color_request_default(index));
                self.write_bytes(format(color).as_bytes());
            }
            TerminalUiEvent::TextAreaSizeRequest(format) => {
                let layout = self.layout.lock().clone();
                self.write_bytes(format(layout.window_size()).as_bytes());
            }
            TerminalUiEvent::Exit => {
                self.exited = true;
                *should_notify = true;
            }
            TerminalUiEvent::Error(message) => {
                self.title = Some(format!("Terminal error: {message}"));
                *should_notify = true;
            }
        }
    }

    fn write_bytes(&self, bytes: &[u8]) {
        let mut writer = self.stdin_writer.lock();
        let _ = writer.write_all(bytes);
        let _ = writer.flush();
    }

    fn paste_text(&self, text: &str) {
        if self.state.mode().contains(TermMode::BRACKETED_PASTE) {
            self.write_bytes(b"\x1b[200~");
            self.write_bytes(text.replace("\r\n", "\n").replace('\r', "\n").as_bytes());
            self.write_bytes(b"\x1b[201~");
        } else {
            self.write_bytes(text.as_bytes());
        }
    }

    fn should_report_mouse(&self, shift_pressed: bool) -> bool {
        !shift_pressed && self.state.mode().intersects(TermMode::MOUSE_MODE)
    }

    fn send_mouse_report(
        &self,
        button: Option<MouseButton>,
        point: TerminalCellPoint,
        kind: MouseReportKind,
        modifiers: Modifiers,
    ) {
        let mode = self.state.mode();
        let Some(sequence) = mouse_report_sequence(button, point, kind, modifiers, mode) else {
            return;
        };
        self.write_bytes(&sequence);
    }

    fn selected_text(&self) -> Option<String> {
        let selection = self.selection.lock().range()?;
        let text = self.state.selected_text(selection);
        (!text.is_empty()).then_some(text)
    }

    fn selection_point_from_cell(&self, point: TerminalCellPoint) -> TerminalSelectionPoint {
        TerminalSelectionPoint {
            line: point.row as i32 - self.state.display_offset() as i32,
            col: point.col,
        }
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

fn should_send_alternate_scroll(mode: TermMode, shift_pressed: bool) -> bool {
    !shift_pressed && mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.process_events(window, cx);

        self.renderer.measure_cell(window);
        let term = self.state.term.clone();
        let renderer = self.renderer.clone();
        let layout = self.layout.clone();
        let selection = self.selection.clone();
        let focus_handle = self.focus_handle.clone();
        let stdin_writer = self.stdin_writer.clone();
        let terminal_view = cx.weak_entity();
        let marked_text = self.marked_text.clone();
        let padding = self.config.padding;
        let cursor_visible = !self.focus_handle.contains_focused(window, cx) || self.cursor_visible;
        let resize_handle = self.resize_handle.clone();

        div()
            .size_full()
            .bg(self.config.colors.background())
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .child(
                canvas(
                    move |bounds, _window, _cx| bounds,
                    move |bounds, _, window, cx| {
                        let available_width =
                            (bounds.size.width - padding.left - padding.right).max(px(1.0));
                        let available_height =
                            (bounds.size.height - padding.top - padding.bottom).max(px(1.0));
                        let available_width: f32 = available_width.into();
                        let available_height: f32 = available_height.into();
                        let cell_width: f32 = renderer.cell_width.into();
                        let cell_height: f32 = renderer.cell_height.into();
                        let cols = ((available_width / cell_width) as usize).max(20);
                        let rows = ((available_height / cell_height) as usize).max(8);
                        layout.lock().update(
                            bounds,
                            padding,
                            renderer.cell_width,
                            renderer.cell_height,
                            cols,
                            rows,
                        );

                        let mut term = term.lock();
                        if cols != term.columns() || rows != term.screen_lines() {
                            term.resize(TermSize::new(cols, rows));
                            if let Err(error) = resize_handle.resize(cols as u16, rows as u16) {
                                eprintln!("failed to resize terminal pty: {error}");
                            }
                        }
                        let selection = selection.lock().range();
                        renderer.paint(
                            bounds,
                            padding,
                            &term,
                            selection,
                            cursor_visible,
                            window,
                            cx,
                        );
                        if let Some(marked_text) = marked_text.as_deref() {
                            renderer.paint_marked_text(
                                bounds,
                                padding,
                                &term,
                                marked_text,
                                window,
                                cx,
                            );
                        }
                        window.handle_input(
                            &focus_handle,
                            TerminalInputHandler {
                                stdin_writer: stdin_writer.clone(),
                                layout: layout.clone(),
                                terminal_view: terminal_view.clone(),
                            },
                            cx,
                        );
                    },
                )
                .size_full(),
            )
    }
}

struct TerminalState {
    term: Arc<Mutex<Term<GpuiEventProxy>>>,
    parser: Processor,
}

impl TerminalState {
    fn new(cols: usize, rows: usize, scrollback: usize, event_proxy: GpuiEventProxy) -> Self {
        let config = AlacrittyConfig {
            scrolling_history: scrollback,
            ..Default::default()
        };
        Self {
            term: Arc::new(Mutex::new(Term::new(
                config,
                &TermSize::new(cols, rows),
                event_proxy,
            ))),
            parser: Processor::new(),
        }
    }

    fn process_bytes(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut *self.term.lock(), bytes);
    }

    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    fn display_offset(&self) -> usize {
        self.term.lock().grid().display_offset()
    }

    fn scroll_display(&self, lines: i32) -> bool {
        use alacritty_terminal::grid::Scroll;

        let mut term = self.term.lock();
        let before = term.grid().display_offset();
        let scroll = Scroll::Delta(lines);
        term.scroll_display(scroll);
        term.grid().display_offset() != before
    }

    fn selected_text(&self, selection: SelectionRange) -> String {
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
struct TerminalCellPoint {
    row: usize,
    col: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
struct TerminalSelectionPoint {
    line: i32,
    col: usize,
}

#[derive(Clone, Copy, Debug)]
struct SelectionAutoScroll {
    edge_cell: TerminalCellPoint,
    lines: i32,
}

struct ScrollFlushResult {
    did_scroll: bool,
    next_lines: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SelectionRange {
    start: TerminalSelectionPoint,
    end: TerminalSelectionPoint,
}

#[derive(Clone, Debug, Default)]
struct SelectionState {
    anchor: Option<TerminalSelectionPoint>,
    head: Option<TerminalSelectionPoint>,
    dragging: bool,
}

impl SelectionState {
    fn start(&mut self, point: TerminalSelectionPoint) {
        self.anchor = Some(point);
        self.head = Some(point);
        self.dragging = true;
    }

    fn update(&mut self, point: TerminalSelectionPoint) {
        if self.anchor.is_some() {
            self.head = Some(point);
            self.dragging = true;
        }
    }

    fn finish(&mut self, point: TerminalSelectionPoint) {
        if self.anchor.is_some() {
            self.head = Some(point);
        }
        self.dragging = false;
    }

    fn clear(&mut self) {
        self.anchor = None;
        self.head = None;
        self.dragging = false;
    }

    fn range(&self) -> Option<SelectionRange> {
        let anchor = self.anchor?;
        let head = self.head?;
        if anchor == head {
            return None;
        }
        let (start, end) = if anchor <= head {
            (anchor, head)
        } else {
            (head, anchor)
        };
        Some(SelectionRange { start, end })
    }
}

#[derive(Clone, Debug)]
struct TerminalLayoutMetrics {
    bounds: Bounds<Pixels>,
    padding: Edges<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    cols: usize,
    rows: usize,
}

impl Default for TerminalLayoutMetrics {
    fn default() -> Self {
        Self {
            bounds: Bounds {
                origin: Point {
                    x: px(0.0),
                    y: px(0.0),
                },
                size: Size {
                    width: px(0.0),
                    height: px(0.0),
                },
            },
            padding: Edges::all(px(0.0)),
            cell_width: px(1.0),
            cell_height: px(1.0),
            cols: 0,
            rows: 0,
        }
    }
}

impl TerminalLayoutMetrics {
    fn update(
        &mut self,
        bounds: Bounds<Pixels>,
        padding: Edges<Pixels>,
        cell_width: Pixels,
        cell_height: Pixels,
        cols: usize,
        rows: usize,
    ) {
        self.bounds = bounds;
        self.padding = padding;
        self.cell_width = cell_width.max(px(1.0));
        self.cell_height = cell_height.max(px(1.0));
        self.cols = cols;
        self.rows = rows;
    }

    fn cell_at(&self, position: Point<Pixels>) -> Option<TerminalCellPoint> {
        if self.cols == 0 || self.rows == 0 {
            return None;
        }

        let origin = Point {
            x: self.bounds.origin.x + self.padding.left,
            y: self.bounds.origin.y + self.padding.top,
        };
        let relative_x = position.x - origin.x;
        let relative_y = position.y - origin.y;
        let width = self.cell_width * self.cols as f32;
        let height = self.cell_height * self.rows as f32;
        if relative_x < px(0.0)
            || relative_y < px(0.0)
            || relative_x >= width
            || relative_y >= height
        {
            return None;
        }

        Some(TerminalCellPoint {
            row: ((relative_y / self.cell_height) as usize).min(self.rows.saturating_sub(1)),
            col: ((relative_x / self.cell_width) as usize).min(self.cols.saturating_sub(1)),
        })
    }

    fn drag_cell_at(&self, position: Point<Pixels>) -> Option<(TerminalCellPoint, i32)> {
        if self.cols == 0 || self.rows == 0 {
            return None;
        }

        let origin = Point {
            x: self.bounds.origin.x + self.padding.left,
            y: self.bounds.origin.y + self.padding.top,
        };
        let relative_x = position.x - origin.x;
        let relative_y = position.y - origin.y;
        let width = self.cell_width * self.cols as f32;
        let height = self.cell_height * self.rows as f32;
        if relative_x < px(0.0) || relative_x >= width {
            return None;
        }

        let col = ((relative_x / self.cell_width) as usize).min(self.cols.saturating_sub(1));
        if relative_y < px(0.0) {
            let lines = ((-relative_y / self.cell_height) as i32 + 1).clamp(1, 8);
            return Some((TerminalCellPoint { row: 0, col }, lines));
        }
        if relative_y >= height {
            let lines = (((relative_y - height) / self.cell_height) as i32 + 1).clamp(1, 8);
            return Some((
                TerminalCellPoint {
                    row: self.rows.saturating_sub(1),
                    col,
                },
                -lines,
            ));
        }

        Some((
            TerminalCellPoint {
                row: ((relative_y / self.cell_height) as usize).min(self.rows.saturating_sub(1)),
                col,
            },
            0,
        ))
    }

    fn input_bounds(&self) -> Bounds<Pixels> {
        Bounds {
            origin: Point {
                x: self.bounds.origin.x + self.padding.left,
                y: self.bounds.origin.y + self.padding.top,
            },
            size: Size {
                width: self.cell_width,
                height: self.cell_height,
            },
        }
    }

    fn window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: self.rows as u16,
            num_cols: self.cols as u16,
            cell_width: f32::from(self.cell_width).round().max(1.0) as u16,
            cell_height: f32::from(self.cell_height).round().max(1.0) as u16,
        }
    }
}

#[derive(Clone)]
struct TerminalInputHandler {
    stdin_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    layout: Arc<Mutex<TerminalLayoutMetrics>>,
    terminal_view: WeakEntity<TerminalView>,
}

impl TerminalInputHandler {
    fn send_filtered_input(&self, text: &str) {
        if text.is_empty() {
            return;
        }

        let mut writer = self.stdin_writer.lock();
        for c in text
            .chars()
            .filter(|c| !('\u{F700}'..='\u{F8FF}').contains(c))
        {
            match c {
                '\u{8}' => {
                    let _ = writer.write_all(&[0x7f]);
                }
                '\n' | '\r' => {
                    let _ = writer.write_all(b"\r");
                }
                _ => {
                    let mut buffer = [0; 4];
                    let _ = writer.write_all(c.encode_utf8(&mut buffer).as_bytes());
                }
            }
        }
        let _ = writer.flush();
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
        _window: &mut Window,
        cx: &mut App,
    ) {
        let _ = self
            .terminal_view
            .update(cx, |view, cx| view.clear_marked_text(cx));
        self.send_filtered_input(text);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let _ = self.terminal_view.update(cx, |view, cx| {
            view.set_marked_text(new_text.to_string(), cx)
        });
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut App) {
        let _ = self
            .terminal_view
            .update(cx, |view, cx| view.clear_marked_text(cx));
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        Some(self.layout.lock().input_bounds())
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

struct TermSize {
    cols: usize,
    rows: usize,
}

impl TermSize {
    fn new(cols: usize, rows: usize) -> Self {
        Self { cols, rows }
    }
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

#[derive(Clone)]
enum TerminalUiEvent {
    Wakeup,
    Bell,
    Title(String),
    Error(String),
    ClipboardStore(String),
    ClipboardLoad,
    PtyWrite(Vec<u8>),
    ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Sync + Send + 'static>),
    TextAreaSizeRequest(Arc<dyn Fn(WindowSize) -> String + Sync + Send + 'static>),
    Exit,
}

fn terminal_color_request_default(index: usize) -> Rgb {
    const ANSI: [Rgb; 16] = [
        Rgb {
            r: 0x1A,
            g: 0x1D,
            b: 0x24,
        },
        Rgb {
            r: 0xF2,
            g: 0x72,
            b: 0x72,
        },
        Rgb {
            r: 0x7D,
            g: 0xD8,
            b: 0x92,
        },
        Rgb {
            r: 0xE8,
            g: 0xC6,
            b: 0x6A,
        },
        Rgb {
            r: 0x7A,
            g: 0xB8,
            b: 0xFF,
        },
        Rgb {
            r: 0xD6,
            g: 0x8A,
            b: 0xFF,
        },
        Rgb {
            r: 0x66,
            g: 0xD9,
            b: 0xE8,
        },
        Rgb {
            r: 0xD6,
            g: 0xDA,
            b: 0xE2,
        },
        Rgb {
            r: 0x5C,
            g: 0x65,
            b: 0x73,
        },
        Rgb {
            r: 0xFF,
            g: 0x9B,
            b: 0x9B,
        },
        Rgb {
            r: 0xA8,
            g: 0xEE,
            b: 0xB7,
        },
        Rgb {
            r: 0xF4,
            g: 0xD9,
            b: 0x86,
        },
        Rgb {
            r: 0xA6,
            g: 0xD0,
            b: 0xFF,
        },
        Rgb {
            r: 0xE6,
            g: 0xB3,
            b: 0xFF,
        },
        Rgb {
            r: 0x9E,
            g: 0xF0,
            b: 0xF5,
        },
        Rgb {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        },
    ];

    match index {
        0..=15 => ANSI[index],
        256 | 267 => Rgb {
            r: 0xD6,
            g: 0xDA,
            b: 0xE2,
        },
        257 | 268 => Rgb {
            r: 0x11,
            g: 0x14,
            b: 0x1A,
        },
        258 => Rgb {
            r: 0xF3,
            g: 0xC9,
            b: 0x6B,
        },
        _ => Rgb {
            r: 0xD6,
            g: 0xDA,
            b: 0xE2,
        },
    }
}

#[derive(Clone)]
struct GpuiEventProxy {
    tx: mpsc::Sender<TerminalUiEvent>,
}

impl GpuiEventProxy {
    fn new(tx: mpsc::Sender<TerminalUiEvent>) -> Self {
        Self { tx }
    }

    fn send(&self, event: TerminalUiEvent) {
        let _ = self.tx.send(event);
    }
}

impl EventListener for GpuiEventProxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => self.send(TerminalUiEvent::Wakeup),
            Event::Bell => self.send(TerminalUiEvent::Bell),
            Event::Title(title) => self.send(TerminalUiEvent::Title(title)),
            Event::ClipboardStore(_, text) => self.send(TerminalUiEvent::ClipboardStore(text)),
            Event::ClipboardLoad(_, _) => self.send(TerminalUiEvent::ClipboardLoad),
            Event::PtyWrite(text) => self.send(TerminalUiEvent::PtyWrite(text.into_bytes())),
            Event::ColorRequest(index, format) => {
                self.send(TerminalUiEvent::ColorRequest(index, format))
            }
            Event::TextAreaSizeRequest(format) => {
                self.send(TerminalUiEvent::TextAreaSizeRequest(format))
            }
            Event::Exit | Event::ChildExit(_) => self.send(TerminalUiEvent::Exit),
            Event::ResetTitle => self.send(TerminalUiEvent::Title(String::new())),
            Event::MouseCursorDirty | Event::CursorBlinkingChange => {}
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum TerminalKeyModifiers {
    None,
    Alt,
    Ctrl,
    Shift,
    CtrlShift,
    Other,
}

impl TerminalKeyModifiers {
    fn new(keystroke: &Keystroke) -> Self {
        match (
            keystroke.modifiers.alt,
            keystroke.modifiers.control,
            keystroke.modifiers.shift,
            keystroke.modifiers.platform,
            keystroke.modifiers.function,
        ) {
            (false, false, false, false, false) => Self::None,
            (true, false, false, false, false) => Self::Alt,
            (false, true, false, false, false) => Self::Ctrl,
            (false, false, true, false, false) => Self::Shift,
            (false, true, true, false, false) => Self::CtrlShift,
            _ => Self::Other,
        }
    }

    fn any(&self) -> bool {
        !matches!(self, Self::None)
    }
}

fn keystroke_to_bytes(keystroke: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    let modifiers = TerminalKeyModifiers::new(keystroke);
    let key = normalize_terminal_key(&keystroke.key);
    let manual = match (key.as_str(), &modifiers) {
        ("tab", TerminalKeyModifiers::None) => Some("\x09"),
        ("escape", TerminalKeyModifiers::None) => Some("\x1b"),
        ("enter", TerminalKeyModifiers::None) => Some("\x0d"),
        ("enter", TerminalKeyModifiers::Shift) => Some("\x0a"),
        ("enter", TerminalKeyModifiers::Alt) => Some("\x1b\x0d"),
        ("backspace", TerminalKeyModifiers::None) | ("back", TerminalKeyModifiers::None) => {
            Some("\x7f")
        }
        ("tab", TerminalKeyModifiers::Shift) => Some("\x1b[Z"),
        ("backspace", TerminalKeyModifiers::Ctrl) => Some("\x08"),
        ("backspace", TerminalKeyModifiers::Alt) => Some("\x1b\x7f"),
        ("backspace", TerminalKeyModifiers::Shift) => Some("\x7f"),
        ("space", TerminalKeyModifiers::Ctrl) => Some("\x00"),
        ("home", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => {
            Some("\x1bOH")
        }
        ("home", TerminalKeyModifiers::None) => Some("\x1b[H"),
        ("end", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => {
            Some("\x1bOF")
        }
        ("end", TerminalKeyModifiers::None) => Some("\x1b[F"),
        ("up", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => Some("\x1bOA"),
        ("up", TerminalKeyModifiers::None) => Some("\x1b[A"),
        ("down", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => {
            Some("\x1bOB")
        }
        ("down", TerminalKeyModifiers::None) => Some("\x1b[B"),
        ("right", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => {
            Some("\x1bOC")
        }
        ("right", TerminalKeyModifiers::None) => Some("\x1b[C"),
        ("left", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => {
            Some("\x1bOD")
        }
        ("left", TerminalKeyModifiers::None) => Some("\x1b[D"),
        ("insert", TerminalKeyModifiers::None) => Some("\x1b[2~"),
        ("delete", TerminalKeyModifiers::None) => Some("\x1b[3~"),
        ("pageup", TerminalKeyModifiers::None) => Some("\x1b[5~"),
        ("pagedown", TerminalKeyModifiers::None) => Some("\x1b[6~"),
        ("f1", TerminalKeyModifiers::None) => Some("\x1bOP"),
        ("f2", TerminalKeyModifiers::None) => Some("\x1bOQ"),
        ("f3", TerminalKeyModifiers::None) => Some("\x1bOR"),
        ("f4", TerminalKeyModifiers::None) => Some("\x1bOS"),
        ("f5", TerminalKeyModifiers::None) => Some("\x1b[15~"),
        ("f6", TerminalKeyModifiers::None) => Some("\x1b[17~"),
        ("f7", TerminalKeyModifiers::None) => Some("\x1b[18~"),
        ("f8", TerminalKeyModifiers::None) => Some("\x1b[19~"),
        ("f9", TerminalKeyModifiers::None) => Some("\x1b[20~"),
        ("f10", TerminalKeyModifiers::None) => Some("\x1b[21~"),
        ("f11", TerminalKeyModifiers::None) => Some("\x1b[23~"),
        ("f12", TerminalKeyModifiers::None) => Some("\x1b[24~"),
        ("f13", TerminalKeyModifiers::None) => Some("\x1b[25~"),
        ("f14", TerminalKeyModifiers::None) => Some("\x1b[26~"),
        ("f15", TerminalKeyModifiers::None) => Some("\x1b[28~"),
        ("f16", TerminalKeyModifiers::None) => Some("\x1b[29~"),
        ("f17", TerminalKeyModifiers::None) => Some("\x1b[31~"),
        ("f18", TerminalKeyModifiers::None) => Some("\x1b[32~"),
        ("f19", TerminalKeyModifiers::None) => Some("\x1b[33~"),
        ("f20", TerminalKeyModifiers::None) => Some("\x1b[34~"),
        (key, TerminalKeyModifiers::Ctrl | TerminalKeyModifiers::CtrlShift) => ctrl_sequence(key),
        _ => None,
    };
    if let Some(sequence) = manual {
        return Some(sequence.as_bytes().to_vec());
    }

    if modifiers.any() {
        let modifier_code = terminal_modifier_code(keystroke);
        let modified = match key.as_str() {
            "up" => Some(format!("\x1b[1;{modifier_code}A")),
            "down" => Some(format!("\x1b[1;{modifier_code}B")),
            "right" => Some(format!("\x1b[1;{modifier_code}C")),
            "left" => Some(format!("\x1b[1;{modifier_code}D")),
            "f1" => Some(format!("\x1b[1;{modifier_code}P")),
            "f2" => Some(format!("\x1b[1;{modifier_code}Q")),
            "f3" => Some(format!("\x1b[1;{modifier_code}R")),
            "f4" => Some(format!("\x1b[1;{modifier_code}S")),
            "f5" => Some(format!("\x1b[15;{modifier_code}~")),
            "f6" => Some(format!("\x1b[17;{modifier_code}~")),
            "f7" => Some(format!("\x1b[18;{modifier_code}~")),
            "f8" => Some(format!("\x1b[19;{modifier_code}~")),
            "f9" => Some(format!("\x1b[20;{modifier_code}~")),
            "f10" => Some(format!("\x1b[21;{modifier_code}~")),
            "f11" => Some(format!("\x1b[23;{modifier_code}~")),
            "f12" => Some(format!("\x1b[24;{modifier_code}~")),
            "f13" => Some(format!("\x1b[25;{modifier_code}~")),
            "f14" => Some(format!("\x1b[26;{modifier_code}~")),
            "f15" => Some(format!("\x1b[28;{modifier_code}~")),
            "f16" => Some(format!("\x1b[29;{modifier_code}~")),
            "f17" => Some(format!("\x1b[31;{modifier_code}~")),
            "f18" => Some(format!("\x1b[32;{modifier_code}~")),
            "f19" => Some(format!("\x1b[33;{modifier_code}~")),
            "f20" => Some(format!("\x1b[34;{modifier_code}~")),
            "insert" => Some(format!("\x1b[2;{modifier_code}~")),
            "delete" => Some(format!("\x1b[3;{modifier_code}~")),
            "pageup" => Some(format!("\x1b[5;{modifier_code}~")),
            "pagedown" => Some(format!("\x1b[6;{modifier_code}~")),
            "end" => Some(format!("\x1b[1;{modifier_code}F")),
            "home" => Some(format!("\x1b[1;{modifier_code}H")),
            _ => None,
        };
        if let Some(sequence) = modified {
            return Some(sequence.into_bytes());
        }
    }

    if keystroke.modifiers.alt
        && !keystroke.modifiers.control
        && !keystroke.modifiers.platform
        && key.is_ascii()
        && key.chars().count() == 1
    {
        let mut key = key;
        if keystroke.modifiers.shift {
            key = key.to_ascii_uppercase();
        }
        return Some(format!("\x1b{key}").into_bytes());
    }

    if !keystroke.modifiers.control && !keystroke.modifiers.alt && !keystroke.modifiers.platform {
        if let Some(key_char) = &keystroke.key_char {
            return Some(key_char.as_bytes().to_vec());
        }
        if key.chars().count() == 1 {
            return Some(key.as_bytes().to_vec());
        }
    }

    None
}

fn normalize_terminal_key(key: &str) -> String {
    let normalized = key.to_ascii_lowercase();
    match normalized.as_str() {
        "return" | "kp_enter" | "numpadenter" | "numpad_enter" => "enter",
        "esc" => "escape",
        "backtab" | "iso_left_tab" => "tab",
        "del" => "delete",
        "pgup" | "page_up" => "pageup",
        "pgdn" | "page_down" => "pagedown",
        "arrowup" | "arrow_up" | "up_arrow" => "up",
        "arrowdown" | "arrow_down" | "down_arrow" => "down",
        "arrowleft" | "arrow_left" | "left_arrow" => "left",
        "arrowright" | "arrow_right" | "right_arrow" => "right",
        _ => normalized.as_str(),
    }
    .to_string()
}

fn ctrl_sequence(key: &str) -> Option<&'static str> {
    match key {
        "a" | "A" => Some("\x01"),
        "b" | "B" => Some("\x02"),
        "c" | "C" => Some("\x03"),
        "d" | "D" => Some("\x04"),
        "e" | "E" => Some("\x05"),
        "f" | "F" => Some("\x06"),
        "g" | "G" => Some("\x07"),
        "h" | "H" => Some("\x08"),
        "i" | "I" => Some("\x09"),
        "j" | "J" => Some("\x0a"),
        "k" | "K" => Some("\x0b"),
        "l" | "L" => Some("\x0c"),
        "m" | "M" => Some("\x0d"),
        "n" | "N" => Some("\x0e"),
        "o" | "O" => Some("\x0f"),
        "p" | "P" => Some("\x10"),
        "q" | "Q" => Some("\x11"),
        "r" | "R" => Some("\x12"),
        "s" | "S" => Some("\x13"),
        "t" | "T" => Some("\x14"),
        "u" | "U" => Some("\x15"),
        "v" | "V" => Some("\x16"),
        "w" | "W" => Some("\x17"),
        "x" | "X" => Some("\x18"),
        "y" | "Y" => Some("\x19"),
        "z" | "Z" => Some("\x1a"),
        "@" => Some("\x00"),
        "[" => Some("\x1b"),
        "\\" => Some("\x1c"),
        "]" => Some("\x1d"),
        "^" => Some("\x1e"),
        "_" => Some("\x1f"),
        "?" => Some("\x7f"),
        _ => None,
    }
}

fn terminal_modifier_code(keystroke: &Keystroke) -> u32 {
    let mut code = 0;
    if keystroke.modifiers.shift {
        code |= 1;
    }
    if keystroke.modifiers.alt {
        code |= 1 << 1;
    }
    if keystroke.modifiers.control {
        code |= 1 << 2;
    }
    code + 1
}

fn is_copy_keystroke(keystroke: &Keystroke) -> bool {
    normalize_terminal_key(&keystroke.key) == "c"
        && keystroke.modifiers.platform
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
}

fn is_paste_keystroke(keystroke: &Keystroke) -> bool {
    normalize_terminal_key(&keystroke.key) == "v"
        && keystroke.modifiers.platform
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseReportKind {
    Press,
    Release,
    Move,
    Wheel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalMouseButton {
    Left = 0,
    Middle = 1,
    Right = 2,
    LeftMove = 32,
    MiddleMove = 33,
    RightMove = 34,
    NoneMove = 35,
    ScrollUp = 64,
    ScrollDown = 65,
}

fn mouse_report_sequence(
    button: Option<MouseButton>,
    point: TerminalCellPoint,
    kind: MouseReportKind,
    modifiers: Modifiers,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if !mode.intersects(TermMode::MOUSE_MODE) {
        return None;
    }

    let (button, pressed) = match kind {
        MouseReportKind::Press => (mouse_button(button?)?, true),
        MouseReportKind::Release => (mouse_button(button?)?, false),
        MouseReportKind::Move => {
            if !mode.intersects(TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG) {
                return None;
            }
            let button = mouse_move_button(button)?;
            if mode.contains(TermMode::MOUSE_DRAG)
                && matches!(button, TerminalMouseButton::NoneMove)
            {
                return None;
            }
            (button, true)
        }
        MouseReportKind::Wheel => (mouse_wheel_button(button?)?, true),
    };

    let mut code = button as u8;
    if modifiers.shift {
        code += 4;
    }
    if modifiers.alt {
        code += 8;
    }
    if modifiers.control {
        code += 16;
    }

    if mode.contains(TermMode::SGR_MOUSE) {
        let suffix = if pressed { 'M' } else { 'm' };
        return Some(
            format!(
                "\x1b[<{};{};{}{}",
                code,
                point.col + 1,
                point.row + 1,
                suffix
            )
            .into_bytes(),
        );
    }

    normal_mouse_report(
        point,
        if pressed {
            code
        } else {
            3 + (code - button as u8)
        },
        mode,
    )
}

fn mouse_button(button: MouseButton) -> Option<TerminalMouseButton> {
    match button {
        MouseButton::Left => Some(TerminalMouseButton::Left),
        MouseButton::Middle => Some(TerminalMouseButton::Middle),
        MouseButton::Right => Some(TerminalMouseButton::Right),
        MouseButton::Navigate(_) => None,
    }
}

fn mouse_move_button(button: Option<MouseButton>) -> Option<TerminalMouseButton> {
    match button {
        Some(MouseButton::Left) => Some(TerminalMouseButton::LeftMove),
        Some(MouseButton::Middle) => Some(TerminalMouseButton::MiddleMove),
        Some(MouseButton::Right) => Some(TerminalMouseButton::RightMove),
        Some(MouseButton::Navigate(_)) => None,
        None => Some(TerminalMouseButton::NoneMove),
    }
}

fn mouse_wheel_button(button: MouseButton) -> Option<TerminalMouseButton> {
    match button {
        MouseButton::Navigate(NavigationDirection::Back) => Some(TerminalMouseButton::ScrollUp),
        MouseButton::Navigate(NavigationDirection::Forward) => {
            Some(TerminalMouseButton::ScrollDown)
        }
        _ => None,
    }
}

fn normal_mouse_report(
    point: TerminalCellPoint,
    button_code: u8,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let utf8 = mode.contains(TermMode::UTF8_MOUSE);
    let max_point = if utf8 { 2015 } else { 223 };
    if point.row >= max_point || point.col >= max_point {
        return None;
    }

    let mut message = vec![b'\x1b', b'[', b'M', 32 + button_code];
    append_mouse_position(&mut message, point.col, utf8);
    append_mouse_position(&mut message, point.row, utf8);
    Some(message)
}

fn append_mouse_position(message: &mut Vec<u8>, position: usize, utf8: bool) {
    let encoded = 32 + 1 + position;
    if utf8 && position >= 95 {
        message.push((0xC0 + encoded / 64) as u8);
        message.push((0x80 + (encoded & 63)) as u8);
    } else {
        message.push(encoded as u8);
    }
}

#[derive(Clone)]
struct TerminalRenderer {
    font_family: String,
    font_size: Pixels,
    line_height_multiplier: f32,
    cell_width: Pixels,
    cell_height: Pixels,
    palette: ColorPalette,
    measured_key: Option<TerminalCellMeasurementKey>,
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
            font_family,
            font_size,
            line_height_multiplier,
            cell_width: font_size * 0.6,
            cell_height: font_size * 1.4,
            palette,
            measured_key: None,
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
        let font = self.font(FontWeight::NORMAL, FontStyle::Normal);
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        self.cell_width = text_system
            .advance(font_id, self.font_size, 'm')
            .map(|size| size.width)
            .unwrap_or(self.font_size * 0.6);
        self.cell_height = self.font_size * self.line_height_multiplier;
        self.measured_key = Some(key);
    }

    fn font(&self, weight: FontWeight, style: FontStyle) -> Font {
        Font {
            family: self.font_family.clone().into(),
            features: FontFeatures::disable_ligatures(),
            fallbacks: None,
            weight,
            style,
        }
    }

    fn paint(
        &self,
        bounds: Bounds<Pixels>,
        padding: Edges<Pixels>,
        term: &Term<GpuiEventProxy>,
        selection: Option<SelectionRange>,
        cursor_visible: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let grid = term.grid();
        let colors = term.colors();
        let default_bg = self
            .palette
            .resolve(Color::Named(NamedColor::Background), colors);

        window.paint_quad(quad(
            bounds,
            px(0.0),
            default_bg,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));

        let origin = Point {
            x: bounds.origin.x + padding.left,
            y: bounds.origin.y + padding.top,
        };
        let content_right = bounds.origin.x + bounds.size.width - padding.right;
        let display_offset = grid.display_offset() as i32;

        for row in 0..grid.screen_lines() {
            let line = Line(row as i32 - display_offset);
            self.paint_row_backgrounds(
                line,
                row,
                grid.columns(),
                grid,
                colors,
                origin,
                default_bg,
                selection,
                content_right,
                window,
            );
            self.paint_row_text(line, row, grid.columns(), grid, colors, origin, window, cx);
        }

        if display_offset == 0 && cursor_visible && term.mode().contains(TermMode::SHOW_CURSOR) {
            self.paint_cursor(grid.cursor.point, colors, origin, window);
        }
    }

    fn paint_row_backgrounds(
        &self,
        line: Line,
        row: usize,
        columns: usize,
        grid: &alacritty_terminal::Grid<Cell>,
        colors: &Colors,
        origin: Point<Pixels>,
        default_bg: Hsla,
        selection: Option<SelectionRange>,
        content_right: Pixels,
        window: &mut Window,
    ) {
        let mut start_col = 0;
        let mut current = self
            .palette
            .resolve(Color::Named(NamedColor::Background), colors);

        for col in 0..=columns {
            let bg = if col < columns {
                let cell = &grid[TerminalPoint::new(line, Column(col))];
                self.palette.resolve(cell.bg, colors)
            } else {
                Hsla::default()
            };
            if col == 0 {
                current = bg;
            }
            if col == columns || bg != current {
                self.paint_cell_background(
                    row,
                    start_col,
                    col - start_col,
                    current,
                    default_bg,
                    origin,
                    window,
                );
                start_col = col;
                current = bg;
            }
        }

        if let Some(selection) = selection {
            self.paint_selection(line, row, columns, origin, content_right, selection, window);
        }
    }

    fn paint_row_text(
        &self,
        line: Line,
        row: usize,
        columns: usize,
        grid: &alacritty_terminal::Grid<Cell>,
        colors: &Colors,
        origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut text_runs = Vec::new();
        let mut current_run: Option<TerminalTextRun> = None;
        let mut pending_spaces = 0usize;
        for col in 0..columns {
            let cell = &grid[TerminalPoint::new(line, Column(col))];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) || cell.c == '\0' {
                pending_spaces = 0;
                continue;
            }
            if cell.c == ' ' {
                if current_run.is_some() {
                    pending_spaces += 1;
                }
                continue;
            }

            let fg = self.palette.resolve(cell.fg, colors);
            let font = self.font(
                if cell.flags.contains(Flags::BOLD) {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                },
                if cell.flags.contains(Flags::ITALIC) {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
            );
            let text = cell.c.to_string();
            let run = TextRun {
                len: text.len(),
                font,
                color: fg,
                background_color: None,
                underline: cell
                    .flags
                    .contains(Flags::UNDERLINE)
                    .then_some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(fg),
                        wavy: false,
                    }),
                strikethrough: None,
            };
            let cell_width = if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
            if current_run
                .as_ref()
                .is_some_and(|current| current.can_append(col, cell_width, pending_spaces, &run))
            {
                if let Some(current) = current_run.as_mut() {
                    current.append_spaces(pending_spaces);
                    current.append(cell.c, cell_width);
                }
            } else {
                if let Some(current) = current_run.take() {
                    text_runs.push(current);
                }
                current_run = Some(TerminalTextRun::new(col, cell.c, cell_width, run));
            }
            pending_spaces = 0;
        }

        if let Some(current) = current_run {
            text_runs.push(current);
        }

        for text_run in &text_runs {
            text_run.paint(self, row, origin, window, cx);
        }
    }

    fn paint_cell_background(
        &self,
        row: usize,
        start_col: usize,
        width_cols: usize,
        color: Hsla,
        default_bg: Hsla,
        origin: Point<Pixels>,
        window: &mut Window,
    ) {
        if width_cols == 0 || color == default_bg {
            return;
        }

        let x = origin.x + self.cell_width * start_col as f32;
        let y = origin.y + self.cell_height * row as f32;
        window.paint_quad(quad(
            Bounds {
                origin: Point { x, y },
                size: Size {
                    width: self.cell_width * width_cols as f32,
                    height: self.cell_height,
                },
            },
            px(0.0),
            color,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
    }

    fn paint_selection(
        &self,
        line: Line,
        row: usize,
        columns: usize,
        origin: Point<Pixels>,
        content_right: Pixels,
        selection: SelectionRange,
        window: &mut Window,
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

        let x = origin.x + self.cell_width * start_col as f32;
        let width = if end_col >= columns {
            content_right - x
        } else {
            self.cell_width * end_col.saturating_sub(start_col) as f32
        };
        if width <= px(0.0) {
            return;
        }

        window.paint_quad(quad(
            Bounds {
                origin: Point {
                    x,
                    y: origin.y + self.cell_height * row as f32,
                },
                size: Size {
                    width,
                    height: self.cell_height,
                },
            },
            px(0.0),
            self.palette.selection,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
    }

    fn paint_cursor(
        &self,
        cursor: TerminalPoint,
        colors: &Colors,
        origin: Point<Pixels>,
        window: &mut Window,
    ) {
        let cursor_color = self
            .palette
            .resolve(Color::Named(NamedColor::Cursor), colors);
        let x = origin.x + self.cell_width * cursor.column.0 as f32;
        let y = origin.y + self.cell_height * cursor.line.0 as f32;
        window.paint_quad(quad(
            Bounds {
                origin: Point {
                    x: px(f32::from(x).floor()),
                    y: px(f32::from(y).floor()),
                },
                size: Size {
                    width: px(f32::from(self.cell_width).round().max(1.0)),
                    height: px(f32::from(self.cell_height).round().max(1.0)),
                },
            },
            px(0.0),
            cursor_color,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
    }

    fn paint_marked_text(
        &self,
        bounds: Bounds<Pixels>,
        padding: Edges<Pixels>,
        term: &Term<GpuiEventProxy>,
        marked_text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        if marked_text.is_empty() || term.grid().display_offset() != 0 {
            return;
        }

        let colors = term.colors();
        let cursor = term.grid().cursor.point;
        let origin = Point {
            x: bounds.origin.x + padding.left + self.cell_width * cursor.column.0 as f32,
            y: bounds.origin.y + padding.top + self.cell_height * cursor.line.0 as f32,
        };
        let fg = self
            .palette
            .resolve(Color::Named(NamedColor::Foreground), colors);
        let bg = self
            .palette
            .resolve(Color::Named(NamedColor::Background), colors);
        let run = TextRun {
            len: marked_text.len(),
            font: self.font(FontWeight::NORMAL, FontStyle::Normal),
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
    start_col: usize,
    width_cols: usize,
    text: String,
    style: TextRun,
    text_hash: u64,
}

impl TerminalTextRun {
    fn new(start_col: usize, c: char, width_cols: usize, style: TextRun) -> Self {
        let mut hasher = DefaultHasher::new();
        c.hash(&mut hasher);
        Self {
            start_col,
            width_cols,
            text: c.to_string(),
            style,
            text_hash: hasher.finish(),
        }
    }

    fn can_append(
        &self,
        col: usize,
        width_cols: usize,
        pending_spaces: usize,
        style: &TextRun,
    ) -> bool {
        self.start_col + self.width_cols + pending_spaces == col
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
        row: usize,
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
                y: origin.y + renderer.cell_height * row as f32,
            },
            renderer.cell_height,
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

#[derive(Debug, Clone)]
pub struct ColorPalette {
    ansi_colors: [Hsla; 16],
    extended_colors: [Hsla; 256],
    foreground: Hsla,
    background: Hsla,
    cursor: Hsla,
    selection: Hsla,
}

impl Default for ColorPalette {
    fn default() -> Self {
        let ansi_colors = [
            rgb_to_hsla(Rgb {
                r: 0x00,
                g: 0x00,
                b: 0x00,
            }),
            rgb_to_hsla(Rgb {
                r: 0xcc,
                g: 0x00,
                b: 0x00,
            }),
            rgb_to_hsla(Rgb {
                r: 0x4e,
                g: 0x9a,
                b: 0x06,
            }),
            rgb_to_hsla(Rgb {
                r: 0xc4,
                g: 0xa0,
                b: 0x00,
            }),
            rgb_to_hsla(Rgb {
                r: 0x34,
                g: 0x65,
                b: 0xa4,
            }),
            rgb_to_hsla(Rgb {
                r: 0x75,
                g: 0x50,
                b: 0x7b,
            }),
            rgb_to_hsla(Rgb {
                r: 0x06,
                g: 0x98,
                b: 0x9a,
            }),
            rgb_to_hsla(Rgb {
                r: 0xd3,
                g: 0xd7,
                b: 0xcf,
            }),
            rgb_to_hsla(Rgb {
                r: 0x55,
                g: 0x57,
                b: 0x53,
            }),
            rgb_to_hsla(Rgb {
                r: 0xef,
                g: 0x29,
                b: 0x29,
            }),
            rgb_to_hsla(Rgb {
                r: 0x8a,
                g: 0xe2,
                b: 0x34,
            }),
            rgb_to_hsla(Rgb {
                r: 0xfc,
                g: 0xe9,
                b: 0x4f,
            }),
            rgb_to_hsla(Rgb {
                r: 0x72,
                g: 0x9f,
                b: 0xcf,
            }),
            rgb_to_hsla(Rgb {
                r: 0xad,
                g: 0x7f,
                b: 0xa8,
            }),
            rgb_to_hsla(Rgb {
                r: 0x34,
                g: 0xe2,
                b: 0xe2,
            }),
            rgb_to_hsla(Rgb {
                r: 0xee,
                g: 0xee,
                b: 0xec,
            }),
        ];
        let mut extended_colors = [Hsla::default(); 256];
        extended_colors[0..16].copy_from_slice(&ansi_colors);
        let mut idx = 16;
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    extended_colors[idx] = rgb_to_hsla(Rgb {
                        r: if r == 0 { 0 } else { 55 + r * 40 },
                        g: if g == 0 { 0 } else { 55 + g * 40 },
                        b: if b == 0 { 0 } else { 55 + b * 40 },
                    });
                    idx += 1;
                }
            }
        }
        for i in 0..24 {
            let gray = (8 + i * 10) as u8;
            extended_colors[232 + i] = rgb_to_hsla(Rgb {
                r: gray,
                g: gray,
                b: gray,
            });
        }

        Self {
            ansi_colors,
            extended_colors,
            foreground: rgb_to_hsla(Rgb {
                r: 0xd6,
                g: 0xda,
                b: 0xe2,
            }),
            background: rgb_to_hsla(Rgb {
                r: 0x11,
                g: 0x14,
                b: 0x1a,
            }),
            cursor: rgb_to_hsla(Rgb {
                r: 0xf3,
                g: 0xc9,
                b: 0x6b,
            }),
            selection: rgb_to_hsla(Rgb {
                r: 0x26,
                g: 0x4f,
                b: 0x78,
            }),
        }
    }
}

impl ColorPalette {
    pub fn builder() -> ColorPaletteBuilder {
        ColorPaletteBuilder::new()
    }

    fn background(&self) -> Hsla {
        self.background
    }

    fn resolve(&self, color: Color, colors: &Colors) -> Hsla {
        match color {
            Color::Named(named) => {
                if let Some(rgb) = colors[named] {
                    return rgb_to_hsla(rgb);
                }
                let index = named as usize;
                if index < 16 {
                    self.ansi_colors[index]
                } else {
                    match named {
                        NamedColor::Foreground => self.foreground,
                        NamedColor::Background => self.background,
                        NamedColor::Cursor => self.cursor,
                        NamedColor::DimForeground => dim_color(self.foreground),
                        NamedColor::BrightForeground => brighten_color(self.foreground),
                        NamedColor::DimBlack => dim_color(self.ansi_colors[0]),
                        NamedColor::DimRed => dim_color(self.ansi_colors[1]),
                        NamedColor::DimGreen => dim_color(self.ansi_colors[2]),
                        NamedColor::DimYellow => dim_color(self.ansi_colors[3]),
                        NamedColor::DimBlue => dim_color(self.ansi_colors[4]),
                        NamedColor::DimMagenta => dim_color(self.ansi_colors[5]),
                        NamedColor::DimCyan => dim_color(self.ansi_colors[6]),
                        NamedColor::DimWhite => dim_color(self.ansi_colors[7]),
                        _ => self.foreground,
                    }
                }
            }
            Color::Spec(rgb) => rgb_to_hsla(rgb),
            Color::Indexed(index) => self.extended_colors[index as usize],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ColorPaletteBuilder {
    palette: ColorPalette,
}

impl ColorPaletteBuilder {
    fn new() -> Self {
        Self {
            palette: ColorPalette::default(),
        }
    }

    pub fn background(mut self, r: u8, g: u8, b: u8) -> Self {
        self.palette.background = rgb_to_hsla(Rgb { r, g, b });
        self
    }

    pub fn foreground(mut self, r: u8, g: u8, b: u8) -> Self {
        self.palette.foreground = rgb_to_hsla(Rgb { r, g, b });
        self
    }

    pub fn cursor(mut self, r: u8, g: u8, b: u8) -> Self {
        self.palette.cursor = rgb_to_hsla(Rgb { r, g, b });
        self
    }

    pub fn selection(mut self, r: u8, g: u8, b: u8) -> Self {
        self.palette.selection = rgb_to_hsla(Rgb { r, g, b });
        self
    }

    pub fn black(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(0, r, g, b)
    }
    pub fn red(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(1, r, g, b)
    }
    pub fn green(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(2, r, g, b)
    }
    pub fn yellow(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(3, r, g, b)
    }
    pub fn blue(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(4, r, g, b)
    }
    pub fn magenta(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(5, r, g, b)
    }
    pub fn cyan(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(6, r, g, b)
    }
    pub fn white(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(7, r, g, b)
    }
    pub fn bright_black(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(8, r, g, b)
    }
    pub fn bright_red(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(9, r, g, b)
    }
    pub fn bright_green(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(10, r, g, b)
    }
    pub fn bright_yellow(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(11, r, g, b)
    }
    pub fn bright_blue(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(12, r, g, b)
    }
    pub fn bright_magenta(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(13, r, g, b)
    }
    pub fn bright_cyan(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(14, r, g, b)
    }
    pub fn bright_white(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(15, r, g, b)
    }

    fn ansi(mut self, index: usize, r: u8, g: u8, b: u8) -> Self {
        let color = rgb_to_hsla(Rgb { r, g, b });
        self.palette.ansi_colors[index] = color;
        self.palette.extended_colors[index] = color;
        self
    }

    pub fn build(self) -> ColorPalette {
        self.palette
    }
}

fn rgb_to_hsla(rgb: Rgb) -> Hsla {
    gpui_rgb(rgb.r, rgb.g, rgb.b)
}

fn gpui_rgb(r: u8, g: u8, b: u8) -> Hsla {
    rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into()
}

fn dim_color(mut color: Hsla) -> Hsla {
    color.l *= 0.7;
    color
}

fn brighten_color(mut color: Hsla) -> Hsla {
    color.l = (color.l * 1.2).min(1.0);
    color
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keystroke(key: &str) -> Keystroke {
        Keystroke {
            key: key.to_string(),
            key_char: None,
            modifiers: Modifiers::default(),
        }
    }

    fn modified_key(key: &str, shift: bool, alt: bool, control: bool, platform: bool) -> Keystroke {
        Keystroke {
            key: key.to_string(),
            key_char: None,
            modifiers: Modifiers {
                shift,
                alt,
                control,
                platform,
                function: false,
            },
        }
    }

    fn key_char(key: &str, key_char: &str) -> Keystroke {
        Keystroke {
            key: key.to_string(),
            key_char: Some(key_char.to_string()),
            modifiers: Modifiers::default(),
        }
    }

    fn bytes(keystroke: Keystroke, mode: TermMode) -> Vec<u8> {
        keystroke_to_bytes(&keystroke, mode).expect("keystroke should map to terminal bytes")
    }

    #[test]
    fn maps_plain_text_and_basic_control_keys() {
        assert_eq!(bytes(key_char("a", "a"), TermMode::NONE), b"a");
        assert_eq!(bytes(key_char("semicolon", ";"), TermMode::NONE), b";");
        assert_eq!(bytes(keystroke("enter"), TermMode::NONE), b"\r");
        assert_eq!(bytes(keystroke("Return"), TermMode::NONE), b"\r");
        assert_eq!(bytes(keystroke("kp_enter"), TermMode::NONE), b"\r");
        assert_eq!(bytes(keystroke("tab"), TermMode::NONE), b"\t");
        assert_eq!(bytes(keystroke("Tab"), TermMode::NONE), b"\t");
        assert_eq!(bytes(keystroke("escape"), TermMode::NONE), b"\x1b");
        assert_eq!(bytes(keystroke("Esc"), TermMode::NONE), b"\x1b");
        assert_eq!(bytes(keystroke("backspace"), TermMode::NONE), b"\x7f");
    }

    #[test]
    fn maps_copy_and_paste_shortcuts_as_ui_commands() {
        assert!(is_copy_keystroke(&modified_key(
            "C", false, false, false, true
        )));
        assert!(is_paste_keystroke(&modified_key(
            "V", false, false, false, true
        )));
        assert!(!is_copy_keystroke(&modified_key(
            "c", false, false, true, false
        )));
        assert!(!is_paste_keystroke(&modified_key(
            "v", false, false, true, false
        )));
    }

    #[test]
    fn shift_scroll_keeps_terminal_history_available_in_alternate_screen() {
        let mode = TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL;
        assert!(should_send_alternate_scroll(mode, false));
        assert!(!should_send_alternate_scroll(mode, true));
    }

    #[test]
    fn maps_app_cursor_mode() {
        assert_eq!(bytes(keystroke("up"), TermMode::NONE), b"\x1b[A");
        assert_eq!(bytes(keystroke("down"), TermMode::NONE), b"\x1b[B");
        assert_eq!(bytes(keystroke("right"), TermMode::NONE), b"\x1b[C");
        assert_eq!(bytes(keystroke("left"), TermMode::NONE), b"\x1b[D");
        assert_eq!(bytes(keystroke("arrow_up"), TermMode::NONE), b"\x1b[A");
        assert_eq!(bytes(keystroke("down_arrow"), TermMode::NONE), b"\x1b[B");
        assert_eq!(bytes(keystroke("home"), TermMode::NONE), b"\x1b[H");
        assert_eq!(bytes(keystroke("end"), TermMode::NONE), b"\x1b[F");

        assert_eq!(bytes(keystroke("up"), TermMode::APP_CURSOR), b"\x1bOA");
        assert_eq!(bytes(keystroke("down"), TermMode::APP_CURSOR), b"\x1bOB");
        assert_eq!(bytes(keystroke("right"), TermMode::APP_CURSOR), b"\x1bOC");
        assert_eq!(bytes(keystroke("left"), TermMode::APP_CURSOR), b"\x1bOD");
        assert_eq!(bytes(keystroke("home"), TermMode::APP_CURSOR), b"\x1bOH");
        assert_eq!(bytes(keystroke("end"), TermMode::APP_CURSOR), b"\x1bOF");
    }

    #[test]
    fn maps_modified_navigation_and_function_keys() {
        assert_eq!(
            bytes(
                modified_key("up", true, false, false, false),
                TermMode::NONE
            ),
            b"\x1b[1;2A"
        );
        assert_eq!(
            bytes(
                modified_key("left", false, true, true, false),
                TermMode::NONE
            ),
            b"\x1b[1;7D"
        );
        assert_eq!(
            bytes(
                modified_key("home", true, false, false, false),
                TermMode::NONE
            ),
            b"\x1b[1;2H"
        );
        assert_eq!(bytes(keystroke("f12"), TermMode::NONE), b"\x1b[24~");
        assert_eq!(bytes(keystroke("f20"), TermMode::NONE), b"\x1b[34~");
        assert_eq!(
            bytes(
                modified_key("f5", false, false, true, false),
                TermMode::NONE
            ),
            b"\x1b[15;5~"
        );
        assert_eq!(
            bytes(
                modified_key("delete", true, false, false, false),
                TermMode::NONE
            ),
            b"\x1b[3;2~"
        );
    }

    #[test]
    fn maps_ctrl_alt_and_shift_enter_sequences() {
        assert_eq!(
            bytes(modified_key("a", false, false, true, false), TermMode::NONE),
            b"\x01"
        );
        assert_eq!(
            bytes(modified_key("C", true, false, true, false), TermMode::NONE),
            b"\x03"
        );
        assert_eq!(
            bytes(modified_key("[", false, false, true, false), TermMode::NONE),
            b"\x1b"
        );
        assert_eq!(
            bytes(
                modified_key("enter", true, false, false, false),
                TermMode::NONE
            ),
            b"\n"
        );
        assert_eq!(
            bytes(
                modified_key("Tab", true, false, false, false),
                TermMode::NONE
            ),
            b"\x1b[Z"
        );
        assert_eq!(
            bytes(
                modified_key("BackTab", true, false, false, false),
                TermMode::NONE
            ),
            b"\x1b[Z"
        );
        assert_eq!(
            bytes(
                modified_key("enter", false, true, false, false),
                TermMode::NONE
            ),
            b"\x1b\r"
        );
        assert_eq!(
            bytes(modified_key("x", false, true, false, false), TermMode::NONE),
            b"\x1bx"
        );
    }

    #[test]
    fn maps_mouse_reports() {
        let point = TerminalCellPoint { row: 1, col: 2 };
        assert_eq!(
            mouse_report_sequence(
                Some(MouseButton::Left),
                point,
                MouseReportKind::Press,
                Modifiers::default(),
                TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE
            )
            .unwrap(),
            b"\x1b[<0;3;2M"
        );
        assert_eq!(
            mouse_report_sequence(
                Some(MouseButton::Left),
                point,
                MouseReportKind::Release,
                Modifiers::default(),
                TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE
            )
            .unwrap(),
            b"\x1b[<0;3;2m"
        );
        assert_eq!(
            mouse_report_sequence(
                Some(MouseButton::Left),
                point,
                MouseReportKind::Move,
                Modifiers {
                    shift: true,
                    alt: true,
                    control: true,
                    platform: false,
                    function: false,
                },
                TermMode::MOUSE_DRAG | TermMode::SGR_MOUSE
            )
            .unwrap(),
            b"\x1b[<60;3;2M"
        );
        assert_eq!(
            mouse_report_sequence(
                Some(MouseButton::Navigate(NavigationDirection::Back)),
                point,
                MouseReportKind::Wheel,
                Modifiers::default(),
                TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE
            )
            .unwrap(),
            b"\x1b[<64;3;2M"
        );
    }

    #[test]
    fn maps_normal_and_utf8_mouse_reports() {
        let point = TerminalCellPoint { row: 1, col: 2 };
        assert_eq!(
            mouse_report_sequence(
                Some(MouseButton::Left),
                point,
                MouseReportKind::Press,
                Modifiers::default(),
                TermMode::MOUSE_MODE
            )
            .unwrap(),
            vec![b'\x1b', b'[', b'M', 32, 35, 34]
        );

        let utf8_point = TerminalCellPoint { row: 100, col: 100 };
        let report = mouse_report_sequence(
            Some(MouseButton::Left),
            utf8_point,
            MouseReportKind::Press,
            Modifiers::default(),
            TermMode::MOUSE_REPORT_CLICK | TermMode::UTF8_MOUSE,
        )
        .unwrap();
        assert_eq!(&report[..4], &[b'\x1b', b'[', b'M', 32]);
        assert!(report.len() > 6);
    }

    #[test]
    fn selects_text_from_terminal_grid() {
        let mut state = TerminalState::new(10, 4, 100, GpuiEventProxy::new(mpsc::channel().0));
        state.process_bytes(b"hello\r\nworld");

        assert_eq!(
            state.selected_text(SelectionRange {
                start: TerminalSelectionPoint { line: 0, col: 0 },
                end: TerminalSelectionPoint { line: 1, col: 5 },
            }),
            "hello\nworld"
        );
    }

    #[test]
    fn keeps_utf8_cjk_output_in_terminal_grid() {
        let mut state = TerminalState::new(20, 4, 100, GpuiEventProxy::new(mpsc::channel().0));
        state.process_bytes("中文恢复记录".as_bytes());

        assert_eq!(
            state.selected_text(SelectionRange {
                start: TerminalSelectionPoint { line: 0, col: 0 },
                end: TerminalSelectionPoint { line: 0, col: 11 },
            }),
            "中文恢复记录"
        );
    }
}
