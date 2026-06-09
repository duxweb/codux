use alacritty_terminal::{
    event::{Event, EventListener, WindowSize},
    grid::Dimensions,
    index::{Column, Line, Point as TerminalPoint, Side as TerminalSide},
    selection::{Selection as AlacrittySelection, SelectionType as AlacrittySelectionType},
    term::{
        Config as AlacrittyConfig, RenderableCursor, Term, TermMode,
        cell::{Cell, Flags},
        color::Colors,
    },
    vte::ansi::{Color, CursorShape, NamedColor, Processor, Rgb},
};
use anyhow::Result;
use codux_runtime::terminal_pty::{
    TerminalEvent, TerminalInputSnapshot, TerminalManager, TerminalOutputSnapshot,
    TerminalPtyConfig, TerminalPtySession, terminal_viewport_local_owner,
};
use gpui::{
    App, AppContext, Bounds, ClipboardEntry, ClipboardItem, Context, CursorStyle, Edges, Element,
    ElementId, Entity, FocusHandle, Font, FontFeatures, FontStyle, FontWeight, GlobalElementId,
    Hsla, ImageFormat, InputHandler, InspectorElementId, InteractiveElement, IntoElement,
    KeyDownEvent, Keystroke, LayoutId, Modifiers, ModifiersChangedEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, NavigationDirection, ParentElement, Pixels,
    Point, Render, ScrollWheelEvent, SharedString, Size, Style, Styled, Subscription, Task,
    TextAlign, TextRun, TouchPhase, UTF16Selection, UnderlineStyle, WeakEntity, Window, div, px,
    quad, rgb, transparent_black,
};
use gpui_component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarHandle, ScrollbarShow};
use parking_lot::Mutex;
use regex::Regex;
use std::{
    cell::{Cell as StdCell, RefCell},
    collections::{HashMap, VecDeque, hash_map::DefaultHasher},
    env, fs,
    hash::{Hash, Hasher},
    io::Write,
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, LazyLock, OnceLock, mpsc},
    time::{Duration, Instant},
};

pub use codux_runtime::terminal_pty::TerminalLaunchContext;

#[derive(Clone)]
pub struct TerminalPane {
    pub view: Entity<TerminalView>,
    session: TerminalSessionBinding,
}

impl TerminalPane {
    pub fn spawn_with_pty_config<C>(
        cx: &mut C,
        terminal_manager: Arc<TerminalManager>,
        pty_config: TerminalPtyConfig,
        terminal_config: TerminalConfig,
    ) -> Result<Self>
    where
        C: AppContext,
    {
        let config = terminal_pty_config_with_view(pty_config, &terminal_config);
        let (session_event_tx, session_event_rx) = mpsc::channel();
        let emit = Arc::new(move |event| match event {
            TerminalEvent::Exit { .. } => {
                let _ = session_event_tx.send(TerminalUiEvent::Exit);
            }
            TerminalEvent::Error { message, .. } => {
                let _ = session_event_tx.send(TerminalUiEvent::Error(message));
            }
            TerminalEvent::Output { .. } => {}
            TerminalEvent::Viewport { cols, rows, .. } => {
                let _ = session_event_tx.send(TerminalUiEvent::Viewport { cols, rows });
            }
        });
        let terminal_id = config.terminal_id.clone();
        let attach_started_at = Instant::now();
        let (session, output_rx) =
            terminal_manager.attach_or_create_with_context(config, None, emit)?;
        codux_runtime::runtime_trace::runtime_trace(
            "terminal-restore",
            &format!(
                "pty_attach elapsed_ms={} terminal_id={}",
                attach_started_at.elapsed().as_millis(),
                terminal_id.as_deref().unwrap_or("none")
            ),
        );
        let session = TerminalSessionBinding::attached(session);
        let writer = TerminalSessionWriter::new(session.clone());
        let view_started_at = Instant::now();
        let view = cx.new(|cx| {
            TerminalView::new(
                writer,
                output_rx,
                session_event_rx,
                session.clone(),
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

    pub fn pending_with_pty_config<C>(
        cx: &mut C,
        pty_config: TerminalPtyConfig,
        terminal_config: TerminalConfig,
    ) -> (Self, PendingTerminalAttach)
    where
        C: AppContext,
    {
        let config = terminal_pty_config_with_view(pty_config, &terminal_config);
        let terminal_id = config.terminal_id.clone();
        let (session_event_tx, session_event_rx) = mpsc::channel();
        let (output_tx, output_rx) = flume::unbounded();
        let (session, initial_layout_rx) = TerminalSessionBinding::pending();
        let writer = TerminalSessionWriter::new(session.clone());
        let view_started_at = Instant::now();
        let view = cx.new(|cx| {
            TerminalView::new(
                writer,
                output_rx,
                session_event_rx,
                session.clone(),
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
        (
            Self {
                view,
                session: session.clone(),
            },
            PendingTerminalAttach {
                session,
                output_tx,
                session_event_tx,
                terminal_id,
                initial_layout_rx,
            },
        )
    }

    pub fn attach_pending_session(
        terminal_manager: Arc<TerminalManager>,
        pty_config: TerminalPtyConfig,
        terminal_config: TerminalConfig,
        pending: PendingTerminalAttach,
    ) -> Result<String> {
        let initial_layout = pending.wait_for_initial_layout();
        let config = terminal_pty_config_with_view(pty_config, &terminal_config);
        let terminal_id = config.terminal_id.clone();
        let session_event_tx = pending.session_event_tx.clone();
        let emit = Arc::new(move |event| match event {
            TerminalEvent::Exit { .. } => {
                let _ = session_event_tx.send(TerminalUiEvent::Exit);
            }
            TerminalEvent::Error { message, .. } => {
                let _ = session_event_tx.send(TerminalUiEvent::Error(message));
            }
            TerminalEvent::Output { .. } => {}
            TerminalEvent::Viewport { cols, rows, .. } => {
                let _ = session_event_tx.send(TerminalUiEvent::Viewport { cols, rows });
            }
        });
        let attach_started_at = Instant::now();
        let (session, output_rx) =
            terminal_manager.attach_or_create_with_context(config, None, emit)?;
        codux_runtime::runtime_trace::runtime_trace(
            "terminal-restore",
            &format!(
                "pty_attach elapsed_ms={} terminal_id={}",
                attach_started_at.elapsed().as_millis(),
                terminal_id.as_deref().unwrap_or("none")
            ),
        );
        let attached_id = session.id().to_string();
        pending.session.attach(session)?;
        match initial_layout {
            Some((cols, rows)) => codux_runtime::runtime_trace::runtime_trace(
                "terminal-restore",
                &format!(
                    "initial_layout_ready terminal_id={} cols={} rows={}",
                    terminal_id.as_deref().unwrap_or("none"),
                    cols,
                    rows
                ),
            ),
            None => codux_runtime::runtime_trace::runtime_trace(
                "terminal-restore",
                &format!(
                    "initial_layout_timeout terminal_id={}",
                    terminal_id.as_deref().unwrap_or("none")
                ),
            ),
        }
        let output_tx = pending.output_tx;
        codux_runtime::async_runtime::spawn(async move {
            while let Ok(bytes) = output_rx.recv_async().await {
                if output_tx.send_async(bytes).await.is_err() {
                    break;
                }
            }
        });
        Ok(attached_id)
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

pub struct PendingTerminalAttach {
    session: TerminalSessionBinding,
    output_tx: flume::Sender<Vec<u8>>,
    session_event_tx: mpsc::Sender<TerminalUiEvent>,
    terminal_id: Option<String>,
    initial_layout_rx: mpsc::Receiver<(u16, u16)>,
}

impl PendingTerminalAttach {
    pub fn terminal_id(&self) -> Option<&str> {
        self.terminal_id.as_deref()
    }

    fn wait_for_initial_layout(&self) -> Option<(u16, u16)> {
        self.initial_layout_rx
            .recv_timeout(TERMINAL_INITIAL_LAYOUT_WAIT)
            .ok()
    }
}

pub fn terminal_pty_config_with_view(
    mut config: TerminalPtyConfig,
    terminal_config: &TerminalConfig,
) -> TerminalPtyConfig {
    config.cols = Some(terminal_config.cols as u16);
    config.rows = Some(terminal_config.rows as u16);
    config.scrollback_lines = Some(terminal_config.scrollback);
    config
}

#[derive(Clone)]
struct TerminalSessionWriter {
    session: TerminalSessionBinding,
}

impl TerminalSessionWriter {
    fn new(session: TerminalSessionBinding) -> Self {
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

#[derive(Clone)]
struct TerminalSessionBinding {
    inner: Arc<Mutex<TerminalSessionBindingInner>>,
}

struct TerminalSessionBindingInner {
    session: Option<Arc<TerminalPtySession>>,
    pending_writes: VecDeque<Vec<u8>>,
    pending_write_bytes: usize,
    last_resize: Option<(u16, u16)>,
    initial_layout_tx: Option<mpsc::Sender<(u16, u16)>>,
}

impl TerminalSessionBinding {
    fn pending() -> (Self, mpsc::Receiver<(u16, u16)>) {
        let (initial_layout_tx, initial_layout_rx) = mpsc::channel();
        (
            Self {
                inner: Arc::new(Mutex::new(TerminalSessionBindingInner {
                    session: None,
                    pending_writes: VecDeque::new(),
                    pending_write_bytes: 0,
                    last_resize: None,
                    initial_layout_tx: Some(initial_layout_tx),
                })),
            },
            initial_layout_rx,
        )
    }

    fn attached(session: Arc<TerminalPtySession>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TerminalSessionBindingInner {
                session: Some(session),
                pending_writes: VecDeque::new(),
                pending_write_bytes: 0,
                last_resize: None,
                initial_layout_tx: None,
            })),
        }
    }

    fn attach(&self, session: Arc<TerminalPtySession>) -> Result<()> {
        let (pending_writes, last_resize) = {
            let mut inner = self.inner.lock();
            inner.session = Some(session.clone());
            inner.pending_write_bytes = 0;
            (std::mem::take(&mut inner.pending_writes), inner.last_resize)
        };
        if let Some((cols, rows)) = last_resize {
            session
                .clone_handle()
                .resize_viewport(terminal_viewport_local_owner(), cols, rows)?;
        }
        for bytes in pending_writes {
            session.write(&bytes)?;
        }
        Ok(())
    }

    fn write(&self, bytes: &[u8]) -> Result<()> {
        if let Some(session) = self.inner.lock().session.clone() {
            return session.write(bytes);
        }
        const MAX_PENDING_WRITE_BYTES: usize = 64 * 1024;
        let mut inner = self.inner.lock();
        if inner.pending_write_bytes + bytes.len() > MAX_PENDING_WRITE_BYTES {
            return Ok(());
        }
        inner.pending_write_bytes += bytes.len();
        inner.pending_writes.push_back(bytes.to_vec());
        Ok(())
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let (session, initial_layout_tx) = {
            let mut inner = self.inner.lock();
            inner.last_resize = Some((cols, rows));
            (inner.session.clone(), inner.initial_layout_tx.take())
        };
        if let Some(tx) = initial_layout_tx {
            let _ = tx.send((cols, rows));
        }
        if let Some(session) = session {
            session
                .clone_handle()
                .resize_viewport(terminal_viewport_local_owner(), cols, rows)?;
        }
        Ok(())
    }

    fn claim_local_viewport(&self) -> Result<()> {
        if let Some(session) = self.inner.lock().session.clone() {
            session
                .clone_handle()
                .claim_viewport(terminal_viewport_local_owner())?;
        }
        Ok(())
    }

    fn local_viewport_owns(&self) -> bool {
        self.inner
            .lock()
            .session
            .as_ref()
            .map(|session| session.viewport_state().owner == terminal_viewport_local_owner())
            .unwrap_or(true)
    }

    fn record_layout(&self, cols: u16, rows: u16) -> bool {
        let initial_layout_tx = {
            let mut inner = self.inner.lock();
            let changed = inner.last_resize != Some((cols, rows));
            inner.last_resize = Some((cols, rows));
            (inner.initial_layout_tx.take(), changed)
        };
        if let Some(tx) = initial_layout_tx.0 {
            let _ = tx.send((cols, rows));
        }
        initial_layout_tx.1
    }

    fn input_snapshot(&self) -> TerminalInputSnapshot {
        self.inner
            .lock()
            .session
            .as_ref()
            .map(|session| session.input_snapshot())
            .unwrap_or_default()
    }

    fn output_snapshot(&self) -> TerminalOutputSnapshot {
        self.inner
            .lock()
            .session
            .as_ref()
            .map(|session| session.output_snapshot())
            .unwrap_or_default()
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
    pub paste_images_as_paths: bool,
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
        line_height_multiplier: DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
        padding: Edges::all(px(10.0)),
        colors,
        paste_images_as_paths: true,
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

fn terminal_grid_dimension(available: f32, cell: f32, minimum: usize) -> usize {
    if !available.is_finite() || !cell.is_finite() || cell <= 0.0 {
        return minimum;
    }
    (available / cell).next_up().floor().max(minimum as f32) as usize
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

const DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER: f32 = 1.45;
const TERMINAL_SCROLL_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const TERMINAL_OUTPUT_FRAME_INTERVAL: Duration = Duration::from_millis(4);
const TERMINAL_INITIAL_LAYOUT_WAIT: Duration = Duration::from_millis(120);
const TERMINAL_ROW_CACHE_LIMIT: usize = 4096;
static TERMINAL_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();

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
            viewport_lines: content.screen_lines,
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
        cx: &mut Context<Self>,
    ) -> Self
    where
        W: Write + Send + 'static,
    {
        let model =
            cx.new(|cx| TerminalModel::new(stdin_writer, bytes_rx, session_event_rx, &config, cx));
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
            focus_in_subscription: None,
            focus_out_subscription: None,
            focus_observer: None,
            selection_autoscroll: None,
            _observe_model: observe_model,
            _observe_blink_manager: observe_blink_manager,
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
            if let Some(text) = self.selected_text(cx) {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
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
        if event.button == MouseButton::Left
            && event.modifiers.secondary()
            && let Some(link) = point.and_then(|point| self.link_at_cell(point, cx))
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
            if let Some(point) = point {
                let selection_point = self.selection_point_from_cell(point, cx);
                self.selection.lock().extend(selection_point);
                self.model.update(cx, |model, _| {
                    model.update_selection(selection_point, TerminalSide::Right)
                });
            }
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
                    MouseReportKind::Press,
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
                if let Some(point) = point {
                    let selection_point = self.selection_point_from_cell(point, cx);
                    self.selection.lock().start(selection_point);
                    self.model.update(cx, |model, _| {
                        model.start_selection(selection_point, TerminalSide::Left)
                    });
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
        if let Some((point, _)) = self.layout.lock().drag_cell_at(event.position) {
            if self.should_report_mouse(event.modifiers.shift, cx) {
                self.send_mouse_report(
                    Some(event.button),
                    point,
                    MouseReportKind::Release,
                    event.modifiers,
                    cx,
                );
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if selection_dragging {
                let selection_point = self.selection_point_from_cell(point, cx);
                self.selection.lock().finish(selection_point);
                self.model.update(cx, |model, _| {
                    model.update_selection(selection_point, TerminalSide::Right)
                });
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
                MouseReportKind::Move,
                event.modifiers,
                cx,
            );
            cx.stop_propagation();
            return;
        }
        if event.dragging() && self.selection.lock().dragging {
            let Some((point, scroll_lines)) = self.layout.lock().drag_cell_at(event.position)
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
                self.model.update(cx, |model, _| {
                    model.update_selection(selection_point, TerminalSide::Right)
                });
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
            if let Some(point) = self.layout.lock().cell_at(event.position)
                && self.should_report_mouse(event.modifiers.shift, cx)
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

        let did_scroll = self
            .model
            .update(cx, |model, _| model.scroll_display(lines));
        if did_scroll
            && let Some(autoscroll) = self.selection_autoscroll
            && self.selection.lock().dragging
        {
            let point = self.selection_point_from_cell(autoscroll.edge_cell, cx);
            let _ = self.selection.lock().update(point);
            self.model.update(cx, |model, _| {
                model.update_selection(point, TerminalSide::Right)
            });
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
        self.model
            .update(cx, |model, cx| model.process_pending_events(cx));
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
                    if let Err(error) = view.session.claim_local_viewport() {
                        eprintln!("failed to claim terminal viewport: {error}");
                    }
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

    fn terminal_clipboard_paste_text(&self, cx: &mut App) -> Option<String> {
        terminal_clipboard_paste_text(cx, self.config.paste_images_as_paths)
    }

    fn clear_pending_view_scroll(&mut self) {
        self.scroll_input.prepare_for_keyboard_input();
        self.selection_autoscroll = None;
    }

    fn should_report_mouse(&self, shift_pressed: bool, cx: &App) -> bool {
        !shift_pressed && self.model.read(cx).mode().intersects(TermMode::MOUSE_MODE)
    }

    fn send_mouse_report(
        &self,
        button: Option<MouseButton>,
        point: TerminalCellPoint,
        kind: MouseReportKind,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        let mode = self.model.read(cx).mode();
        let Some(sequence) = mouse_report_sequence(button, point, kind, modifiers, mode) else {
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
            .layout
            .lock()
            .cell_at(position)
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

    fn selected_text(&self, cx: &App) -> Option<String> {
        let text = self.model.read(cx).selected_text()?;
        (!text.is_empty()).then_some(text)
    }

    fn selection_point_from_cell(
        &self,
        point: TerminalCellPoint,
        cx: &App,
    ) -> TerminalSelectionPoint {
        TerminalSelectionPoint {
            line: point.row as i32 - self.model.read(cx).display_offset() as i32,
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

struct TerminalBlinkManager {
    blink_interval: Duration,
    blink_epoch: usize,
    blinking_paused: bool,
    visible: bool,
    enabled: bool,
}

impl TerminalBlinkManager {
    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            blink_interval: Duration::from_millis(500),
            blink_epoch: 0,
            blinking_paused: false,
            visible: true,
            enabled: false,
        }
    }

    fn next_blink_epoch(&mut self) -> usize {
        self.blink_epoch += 1;
        self.blink_epoch
    }

    fn pause_blinking(&mut self, cx: &mut Context<Self>) {
        self.show_cursor(cx);
        self.blinking_paused = true;
        let epoch = self.next_blink_epoch();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = this.update(cx, |this, cx| this.resume_blinking(epoch, cx));
        })
        .detach();
    }

    fn resume_blinking(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if epoch == self.blink_epoch {
            self.blinking_paused = false;
            self.blink_cursors(epoch, cx);
        }
    }

    fn blink_cursors(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if epoch != self.blink_epoch || !self.enabled || self.blinking_paused {
            return;
        }
        self.visible = !self.visible;
        cx.notify();

        let epoch = self.next_blink_epoch();
        let interval = self.blink_interval;
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(interval).await;
            let _ = this.update(cx, |this, cx| this.blink_cursors(epoch, cx));
        })
        .detach();
    }

    fn show_cursor(&mut self, cx: &mut Context<Self>) {
        if !self.visible {
            self.visible = true;
            cx.notify();
        }
    }

    fn enable(&mut self, cx: &mut Context<Self>) {
        if self.enabled {
            return;
        }
        self.enabled = true;
        self.visible = false;
        self.blink_cursors(self.blink_epoch, cx);
    }

    fn disable(&mut self, cx: &mut Context<Self>) {
        self.enabled = false;
        self.show_cursor(cx);
    }

    fn visible(&self) -> bool {
        self.visible
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SyncOutputUpdate {
    entered_from_idle: bool,
    exited_to_idle: bool,
    should_notify: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalColorSchemeUpdate {
    enabled: bool,
    disabled: bool,
    query_count: usize,
}

#[derive(Debug, Default)]
struct TerminalColorSchemeState {
    updates_enabled: bool,
    scan_tail: Vec<u8>,
}

fn update_synchronized_output_state(
    bytes: &[u8],
    depth: &mut usize,
    scan_tail: &mut Vec<u8>,
) -> SyncOutputUpdate {
    const START: &[u8] = b"\x1b[?2026h";
    const END: &[u8] = b"\x1b[?2026l";
    const MAX_PATTERN_LEN: usize = START.len();

    let mut update = SyncOutputUpdate::default();
    let mut scan = Vec::with_capacity(scan_tail.len() + bytes.len());
    scan.extend_from_slice(scan_tail);
    scan.extend_from_slice(bytes);

    let mut index = 0;
    while index < scan.len() {
        if scan[index..].starts_with(START) {
            if *depth == 0 {
                update.entered_from_idle = true;
            }
            *depth = depth.saturating_add(1);
            index += START.len();
            continue;
        }
        if scan[index..].starts_with(END) {
            let was_syncing = *depth > 0;
            *depth = depth.saturating_sub(1);
            if was_syncing {
                update.should_notify = true;
                if *depth == 0 {
                    update.exited_to_idle = true;
                }
            }
            index += END.len();
            continue;
        }
        index += 1;
    }

    let tail_len = scan.len().min(MAX_PATTERN_LEN.saturating_sub(1));
    scan_tail.clear();
    scan_tail.extend_from_slice(&scan[scan.len().saturating_sub(tail_len)..]);

    update
}

fn update_terminal_color_scheme_state(
    bytes: &[u8],
    state: &mut TerminalColorSchemeState,
) -> TerminalColorSchemeUpdate {
    const ENABLE: &[u8] = b"\x1b[?2031h";
    const DISABLE: &[u8] = b"\x1b[?2031l";
    const QUERY: &[u8] = b"\x1b[?996n";
    const MAX_PATTERN_LEN: usize = ENABLE.len();

    let mut update = TerminalColorSchemeUpdate::default();
    let old_tail_len = state.scan_tail.len();
    let mut scan = Vec::with_capacity(state.scan_tail.len() + bytes.len());
    scan.extend_from_slice(&state.scan_tail);
    scan.extend_from_slice(bytes);

    let mut index = 0;
    while index < scan.len() {
        if scan[index..].starts_with(ENABLE) {
            if index + ENABLE.len() > old_tail_len {
                state.updates_enabled = true;
                update.enabled = true;
            }
            index += ENABLE.len();
            continue;
        }
        if scan[index..].starts_with(DISABLE) {
            if index + DISABLE.len() > old_tail_len {
                state.updates_enabled = false;
                update.disabled = true;
            }
            index += DISABLE.len();
            continue;
        }
        if scan[index..].starts_with(QUERY) {
            if index + QUERY.len() > old_tail_len {
                update.query_count += 1;
            }
            index += QUERY.len();
            continue;
        }
        index += 1;
    }

    let tail_len = scan.len().min(MAX_PATTERN_LEN.saturating_sub(1));
    state.scan_tail.clear();
    state
        .scan_tail
        .extend_from_slice(&scan[scan.len().saturating_sub(tail_len)..]);

    update
}

fn terminal_color_scheme_report(colors: &ColorPalette) -> &'static [u8] {
    if colors.is_dark() {
        b"\x1b[?997;1n"
    } else {
        b"\x1b[?997;2n"
    }
}

fn terminal_trace_enabled() -> bool {
    *TERMINAL_TRACE_ENABLED.get_or_init(|| {
        env::var("CODUX_TERMINAL_TRACE")
            .map(|value| {
                let value = value.trim();
                !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
            })
            .unwrap_or(false)
    })
}

fn terminal_trace(message: &str) {
    if terminal_trace_enabled() {
        codux_runtime::runtime_trace::runtime_trace("terminal-pty", message);
    }
}

fn trace_terminal_protocol_bytes(
    bytes: &[u8],
    sync_update: SyncOutputUpdate,
    sync_depth: usize,
    color_scheme_update: TerminalColorSchemeUpdate,
    color_scheme_updates_enabled: bool,
) {
    if !terminal_trace_enabled() {
        return;
    }
    let flags = terminal_protocol_flags(bytes);

    if sync_update != SyncOutputUpdate::default()
        || flags.show_cursor
        || flags.hide_cursor
        || flags.osc_10_request
        || flags.osc_11_request
        || color_scheme_update != TerminalColorSchemeUpdate::default()
    {
        terminal_trace(&format!(
            "protocol bytes={} sync_depth={} sync_enter={} sync_exit={} notify={} show_cursor={} hide_cursor={} osc10_request={} osc11_request={} color_scheme_enabled={} color_scheme_enable={} color_scheme_disable={} color_scheme_queries={}",
            bytes.len(),
            sync_depth,
            sync_update.entered_from_idle,
            sync_update.exited_to_idle,
            sync_update.should_notify,
            flags.show_cursor,
            flags.hide_cursor,
            flags.osc_10_request,
            flags.osc_11_request,
            color_scheme_updates_enabled,
            color_scheme_update.enabled,
            color_scheme_update.disabled,
            color_scheme_update.query_count,
        ));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalProtocolFlags {
    show_cursor: bool,
    hide_cursor: bool,
    osc_10_request: bool,
    osc_11_request: bool,
}

fn terminal_protocol_flags(bytes: &[u8]) -> TerminalProtocolFlags {
    TerminalProtocolFlags {
        show_cursor: bytes
            .windows(b"\x1b[?25h".len())
            .any(|part| part == b"\x1b[?25h"),
        hide_cursor: bytes
            .windows(b"\x1b[?25l".len())
            .any(|part| part == b"\x1b[?25l"),
        osc_10_request: bytes
            .windows(b"\x1b]10;?".len())
            .any(|part| part == b"\x1b]10;?"),
        osc_11_request: bytes
            .windows(b"\x1b]11;?".len())
            .any(|part| part == b"\x1b]11;?"),
    }
}

fn trace_terminal_paint_snapshot(content: &TerminalContent, cursor_visible: bool) {
    if !terminal_trace_enabled() {
        return;
    }
    terminal_trace(&format!(
        "paint cursor_visible={} show_cursor={} cursor_hidden={} cursor_row={} cursor_col={} cursor_shape={:?} display_offset={} cells={} cols={} rows={}",
        cursor_visible,
        content.mode.contains(TermMode::SHOW_CURSOR),
        content.cursor.shape == CursorShape::Hidden,
        content.cursor.point.line.0,
        content.cursor.point.column.0,
        content.cursor.shape,
        content.display_offset,
        content.cells.len(),
        content.columns,
        content.screen_lines,
    ));
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.process_events(window, cx);
        if let Some(new_display_offset) = self.scroll_handle.take_future_display_offset() {
            self.model.update(cx, |model, _| {
                let current = model.display_offset() as i32;
                let target = new_display_offset as i32;
                model.scroll_display(target.saturating_sub(current));
            });
        }

        self.renderer.measure_cell(window);
        self.ensure_focus_report_subscriptions(window, cx);
        let terminal_focused = self.focus_handle.is_focused(window);
        let cursor_visible = self.marked_text.is_none()
            && window.is_window_active()
            && (!terminal_focused || self.blink_manager.read(cx).visible());
        let element = TerminalElement {
            model: self.model.clone(),
            renderer: self.renderer.clone(),
            layout: self.layout.clone(),
            scroll_handle: self.scroll_handle.clone(),
            session: self.session.clone(),
            focus_handle: self.focus_handle.clone(),
            terminal_view: cx.weak_entity(),
            padding: self.config.padding,
            marked_text: self.marked_text.clone(),
            hover_link: self.hover_link.clone(),
            cursor_visible,
            cursor_focused: terminal_focused,
        };

        let terminal = div()
            .size_full()
            .relative()
            .overflow_hidden()
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
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .on_scroll_wheel(cx.listener(Self::on_scroll));
        let terminal = terminal.child(element).child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(
                    Scrollbar::new(&self.scroll_handle)
                        .id("terminal-scrollbar")
                        .axis(ScrollbarAxis::Vertical)
                        .scrollbar_show(ScrollbarShow::Scrolling),
                ),
        );
        if self.hover_link.is_some() {
            terminal.cursor(CursorStyle::PointingHand)
        } else {
            terminal
        }
    }
}

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

struct TerminalElement {
    model: Entity<TerminalModel>,
    renderer: TerminalRenderer,
    layout: Arc<Mutex<TerminalLayoutMetrics>>,
    scroll_handle: TerminalScrollHandle,
    session: TerminalSessionBinding,
    focus_handle: FocusHandle,
    terminal_view: WeakEntity<TerminalView>,
    padding: Edges<Pixels>,
    marked_text: Option<String>,
    hover_link: Option<TerminalLink>,
    cursor_visible: bool,
    cursor_focused: bool,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = TerminalPaintState;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::from(&self.focus_handle))
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size = Size::full();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let available_width =
            (bounds.size.width - self.padding.left - self.padding.right).max(px(1.0));
        let available_height =
            (bounds.size.height - self.padding.top - self.padding.bottom).max(px(1.0));
        let available_width: f32 = available_width.into();
        let available_height: f32 = available_height.into();
        let cell_width: f32 = self.renderer.cell_width.into();
        let cell_height: f32 = self.renderer.cell_height.into();
        let cols = terminal_grid_dimension(available_width, cell_width, 20);
        let rows = terminal_grid_dimension(available_height, cell_height, 1);
        self.layout.lock().update(
            bounds,
            self.padding,
            self.renderer.cell_width,
            self.renderer.cell_height,
            cols,
            rows,
        );
        let layout_changed = self.session.record_layout(cols as u16, rows as u16);
        if self.cursor_focused && layout_changed {
            if let Err(error) = self.session.claim_local_viewport() {
                eprintln!("failed to claim terminal viewport: {error}");
            }
        }

        let window_size = self.layout.lock().window_size();
        let local_owner = self.session.local_viewport_owns();
        let (model_cols, model_rows) = self.model.read(cx).dimensions();
        let next_cols = if local_owner { cols } else { model_cols };
        let next_rows = if local_owner { rows } else { model_rows };
        let resized = self.model.read(cx).dimensions() != (next_cols, next_rows);
        self.model.update(cx, |model, _| {
            model.resize(next_cols, next_rows, window_size)
        });
        if local_owner
            && resized
            && let Err(error) = self.session.resize(next_cols as u16, next_rows as u16)
        {
            eprintln!("failed to resize terminal pty: {error}");
        }

        let snapshot = self.model.update(cx, |model, cx| model.sync(cx));
        self.scroll_handle
            .update(&snapshot, self.renderer.cell_height.max(px(1.0)));
        trace_terminal_paint_snapshot(&snapshot, self.cursor_visible);
        let selection = self.model.read(cx).selection_range();
        self.renderer.prepare_paint(
            bounds,
            self.padding,
            &snapshot,
            selection,
            self.hover_link.as_ref(),
            self.cursor_visible,
            self.cursor_focused,
            window,
        )
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        paint_state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.renderer.paint_prepared(paint_state, window, cx);
        if let Some(marked_text) = self.marked_text.as_deref() {
            self.renderer
                .paint_marked_text(paint_state, marked_text, window, cx);
        }
        window.handle_input(
            &self.focus_handle,
            TerminalInputHandler {
                model: self.model.clone(),
                layout: self.layout.clone(),
                terminal_view: self.terminal_view.clone(),
                fallback_cursor_bounds: paint_state.ime_cursor_bounds,
            },
            cx,
        );
    }
}

struct TerminalPaintState {
    bounds: Bounds<Pixels>,
    origin: Point<Pixels>,
    background: Hsla,
    background_rects: Vec<TerminalBackgroundRect>,
    text_runs: Vec<TerminalTextRun>,
    cursor: Option<TerminalCursorPaint>,
    marked_text_cursor: Option<TerminalPoint>,
    ime_cursor_bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct TerminalRowCacheKey {
    row_hash: u64,
    font_key: TerminalRendererCacheKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct TerminalRendererCacheKey {
    font_size_bits: u32,
    cell_width_bits: u32,
    cell_height_bits: u32,
}

#[derive(Clone, Default)]
struct TerminalRenderCache {
    rows: HashMap<TerminalRowCacheKey, TerminalPreparedRow>,
}

#[derive(Clone)]
struct TerminalPreparedRow {
    background_rects: Vec<TerminalBackgroundRect>,
    text_runs: Vec<TerminalTextRun>,
}

impl TerminalPreparedRow {
    fn for_display_row(&self, row: usize) -> Self {
        let mut prepared = self.clone();
        for rect in &mut prepared.background_rects {
            rect.row = row;
        }
        for text_run in &mut prepared.text_runs {
            text_run.row = row;
        }
        prepared
    }
}

#[derive(Clone)]
struct TerminalBackgroundRect {
    row: usize,
    start_col: usize,
    width_cols: usize,
    color: Hsla,
}

struct TerminalCursorPaint {
    point: TerminalPoint,
    display_row: usize,
    shape: CursorShape,
    color: Hsla,
    width: Pixels,
    text_run: Option<TerminalTextRun>,
}

impl TerminalBackgroundRect {
    fn paint(&self, renderer: &TerminalRenderer, origin: Point<Pixels>, window: &mut Window) {
        if self.width_cols == 0 {
            return;
        }
        window.paint_quad(quad(
            Bounds {
                origin: Point {
                    x: origin.x + renderer.cell_width * self.start_col as f32,
                    y: origin.y + renderer.cell_height * self.row as f32,
                },
                size: Size {
                    width: renderer.cell_width * self.width_cols as f32,
                    height: renderer.cell_height,
                },
            },
            px(0.0),
            self.color,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
    }
}

impl TerminalCursorPaint {
    fn paint(
        &self,
        renderer: &TerminalRenderer,
        origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let x = origin.x + renderer.cell_width * self.point.column.0 as f32;
        let y = origin.y + renderer.cell_height * self.display_row as f32;
        let bounds = Bounds {
            origin: Point {
                x: px(f32::from(x).floor()),
                y: px(f32::from(y).floor()),
            },
            size: Size {
                width: px(f32::from(self.width).round().max(1.0)),
                height: px(f32::from(renderer.cell_height).round().max(1.0)),
            },
        };

        match self.shape {
            CursorShape::Hidden => {}
            CursorShape::HollowBlock => {
                let border_width = px(1.0);
                window.paint_quad(quad(
                    bounds,
                    px(0.0),
                    transparent_black(),
                    Edges::all(border_width),
                    self.color,
                    Default::default(),
                ));
            }
            CursorShape::Beam => {
                self.paint_filled(
                    Bounds {
                        origin: bounds.origin,
                        size: Size {
                            width: px(2.0),
                            height: bounds.size.height,
                        },
                    },
                    window,
                );
            }
            CursorShape::Underline => {
                self.paint_filled(
                    Bounds {
                        origin: Point {
                            x: bounds.origin.x,
                            y: bounds.origin.y + bounds.size.height - px(2.0),
                        },
                        size: Size {
                            width: bounds.size.width,
                            height: px(2.0),
                        },
                    },
                    window,
                );
            }
            CursorShape::Block => {
                self.paint_filled(bounds, window);
                if let Some(text_run) = &self.text_run {
                    text_run.paint(renderer, origin, window, cx);
                }
            }
        }
    }

    fn paint_filled(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        window.paint_quad(quad(
            bounds,
            px(0.0),
            self.color,
            Edges::<Pixels>::default(),
            transparent_black(),
            Default::default(),
        ));
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

    fn update(&mut self, point: TerminalSelectionPoint) -> bool {
        if self.anchor.is_some() {
            if self.head == Some(point) && self.dragging {
                return false;
            }
            self.head = Some(point);
            self.dragging = true;
            return true;
        }
        false
    }

    fn extend(&mut self, point: TerminalSelectionPoint) {
        if self.anchor.is_none() {
            self.anchor = self.head.or(Some(point));
        }
        self.head = Some(point);
        self.dragging = true;
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
    Viewport { cols: u16, rows: u16 },
    ClipboardStore(String),
    ClipboardLoad,
    PtyWrite(Vec<u8>),
    ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Sync + Send + 'static>),
    TextAreaSizeRequest(Arc<dyn Fn(WindowSize) -> String + Sync + Send + 'static>),
    Exit,
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
    Platform,
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
        ) {
            (false, false, false, false) => Self::None,
            (true, false, false, false) => Self::Alt,
            (false, true, false, false) => Self::Ctrl,
            (false, false, true, false) => Self::Shift,
            (false, false, false, true) => Self::Platform,
            (false, true, true, false) => Self::CtrlShift,
            _ => Self::Other,
        }
    }

    fn any(&self) -> bool {
        !matches!(self, Self::None)
    }
}

fn keystroke_to_bytes(keystroke: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    if keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && let Some(sequence) = control_key_char_sequence(keystroke)
    {
        return Some(sequence);
    }

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
        ("back", TerminalKeyModifiers::Alt) => Some("\x1b\x7f"),
        ("delete", TerminalKeyModifiers::Alt) => Some("\x1bd"),
        ("backspace", TerminalKeyModifiers::Platform) => Some("\x15"),
        ("back", TerminalKeyModifiers::Platform) => Some("\x15"),
        ("delete", TerminalKeyModifiers::Platform) => Some("\x0b"),
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
        ("right", TerminalKeyModifiers::Alt) => Some("\x1bf"),
        ("left", TerminalKeyModifiers::Alt) => Some("\x1bb"),
        ("right", TerminalKeyModifiers::Platform) => Some("\x05"),
        ("left", TerminalKeyModifiers::Platform) => Some("\x01"),
        ("end", TerminalKeyModifiers::Platform) => Some("\x05"),
        ("home", TerminalKeyModifiers::Platform) => Some("\x01"),
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

fn control_key_char_sequence(keystroke: &Keystroke) -> Option<Vec<u8>> {
    let key_char = keystroke.key_char.as_deref()?;
    let mut chars = key_char.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    if ch.is_control() {
        return Some(vec![ch as u8]);
    }
    ctrl_sequence(&ch.to_string()).map(|sequence| sequence.as_bytes().to_vec())
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

fn terminal_clipboard_paste_text(cx: &mut App, paste_images_as_paths: bool) -> Option<String> {
    let item = cx.read_from_clipboard()?;
    let text = item
        .text()
        .filter(|text| !paste_images_as_paths || !clipboard_text_looks_like_image_payload(text));
    if text.is_some() {
        return text;
    }
    if !paste_images_as_paths {
        return None;
    }
    item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) if !image.bytes.is_empty() => {
            write_terminal_clipboard_image(image.format, &image.bytes)
                .ok()
                .map(|path| terminal_path_input(&path))
        }
        _ => None,
    })
}

fn clipboard_text_looks_like_image_payload(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("data:image/")
        || trimmed.starts_with("<img ")
        || trimmed.starts_with("<img\n")
        || trimmed.starts_with("<img\t")
}

fn write_terminal_clipboard_image(format: ImageFormat, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let directory = codux_runtime::runtime_paths::runtime_temp_dir().join("clipboard-images");
    fs::create_dir_all(&directory)?;
    let file_name = format!(
        "terminal-paste-{}-{}.{}",
        std::process::id(),
        terminal_clipboard_image_timestamp(),
        terminal_clipboard_image_extension(format)
    );
    let path = next_available_terminal_clipboard_path(&directory, &file_name);
    fs::write(&path, bytes)?;
    Ok(path)
}

fn next_available_terminal_clipboard_path(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let source = Path::new(file_name);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(file_name);
    let extension = source.extension().and_then(|value| value.to_str());
    for index in 1.. {
        let next_name = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        let next = directory.join(next_name);
        if !next.exists() {
            return next;
        }
    }
    candidate
}

fn terminal_clipboard_image_timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn terminal_clipboard_image_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
        ImageFormat::Ico => "ico",
        ImageFormat::Pnm => "pnm",
    }
}

fn terminal_path_input(path: &Path) -> String {
    shell_quote_path(&path.to_string_lossy())
}

fn shell_quote_path(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '='))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
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

fn ansi_named_color(index: usize) -> NamedColor {
    match index {
        0 => NamedColor::Black,
        1 => NamedColor::Red,
        2 => NamedColor::Green,
        3 => NamedColor::Yellow,
        4 => NamedColor::Blue,
        5 => NamedColor::Magenta,
        6 => NamedColor::Cyan,
        7 => NamedColor::White,
        8 => NamedColor::BrightBlack,
        9 => NamedColor::BrightRed,
        10 => NamedColor::BrightGreen,
        11 => NamedColor::BrightYellow,
        12 => NamedColor::BrightBlue,
        13 => NamedColor::BrightMagenta,
        14 => NamedColor::BrightCyan,
        15 => NamedColor::BrightWhite,
        _ => NamedColor::White,
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

    fn is_dark(&self) -> bool {
        relative_luminance(hsla_to_rgb(self.background))
            < relative_luminance(hsla_to_rgb(self.foreground))
    }

    fn color_request(&self, index: usize) -> Rgb {
        match index {
            0..=255 => hsla_to_rgb(self.extended_colors[index]),
            256 => hsla_to_rgb(self.foreground),
            257 => hsla_to_rgb(self.background),
            258 => hsla_to_rgb(self.cursor),
            259 => hsla_to_rgb(dim_color(self.ansi_colors[0])),
            260 => hsla_to_rgb(dim_color(self.ansi_colors[1])),
            261 => hsla_to_rgb(dim_color(self.ansi_colors[2])),
            262 => hsla_to_rgb(dim_color(self.ansi_colors[3])),
            263 => hsla_to_rgb(dim_color(self.ansi_colors[4])),
            264 => hsla_to_rgb(dim_color(self.ansi_colors[5])),
            265 => hsla_to_rgb(dim_color(self.ansi_colors[6])),
            266 => hsla_to_rgb(dim_color(self.ansi_colors[7])),
            267 => hsla_to_rgb(brighten_color(self.foreground)),
            268 => hsla_to_rgb(dim_color(self.foreground)),
            _ => hsla_to_rgb(self.foreground),
        }
    }

    fn resolve(&self, color: Color, colors: &Colors) -> Hsla {
        match color {
            Color::Named(named) => {
                match named {
                    NamedColor::Foreground => return self.foreground,
                    NamedColor::Background => return self.background,
                    NamedColor::Cursor => return self.cursor,
                    NamedColor::DimForeground => return dim_color(self.foreground),
                    NamedColor::BrightForeground => return brighten_color(self.foreground),
                    _ => {}
                }
                if let Some(rgb) = colors[named] {
                    return rgb_to_hsla(rgb);
                }
                let index = named as usize;
                if index < 16 {
                    self.ansi_colors[index]
                } else {
                    match named {
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

fn hsla_to_rgb(color: Hsla) -> Rgb {
    let rgba = color.to_rgb();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb {
        r: channel(rgba.r),
        g: channel(rgba.g),
        b: channel(rgba.b),
    }
}

fn relative_luminance(rgb: Rgb) -> f32 {
    let channel = |value: u8| {
        let value = value as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb.r) + 0.7152 * channel(rgb.g) + 0.0722 * channel(rgb.b)
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
        modified_key_with_function(key, shift, alt, control, platform, false)
    }

    fn modified_key_with_function(
        key: &str,
        shift: bool,
        alt: bool,
        control: bool,
        platform: bool,
        function: bool,
    ) -> Keystroke {
        Keystroke {
            key: key.to_string(),
            key_char: None,
            modifiers: Modifiers {
                shift,
                alt,
                control,
                platform,
                function,
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

    fn modified_key_with_char(
        key: &str,
        key_char: &str,
        shift: bool,
        alt: bool,
        control: bool,
        platform: bool,
    ) -> Keystroke {
        let mut keystroke = modified_key(key, shift, alt, control, platform);
        keystroke.key_char = Some(key_char.to_string());
        keystroke
    }

    fn bytes(keystroke: Keystroke, mode: TermMode) -> Vec<u8> {
        keystroke_to_bytes(&keystroke, mode).expect("keystroke should map to terminal bytes")
    }

    #[test]
    fn maps_plain_text_and_basic_control_keys() {
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
    fn plain_character_without_text_input_is_not_lowercased() {
        assert!(keystroke_to_bytes(&keystroke("a"), TermMode::NONE).is_none());
    }

    #[test]
    fn printable_key_chars_are_committed_by_text_input() {
        assert!(keystroke_to_bytes(&key_char("a", "a"), TermMode::NONE).is_none());
        assert!(keystroke_to_bytes(&key_char("a", "A"), TermMode::NONE).is_none());
        assert!(keystroke_to_bytes(&key_char("semicolon", ";"), TermMode::NONE).is_none());
    }

    #[test]
    fn maps_terminal_interrupt_shortcut_to_etx() {
        assert_eq!(
            bytes(modified_key("c", false, false, true, false), TermMode::NONE),
            b"\x03"
        );
        assert_eq!(
            bytes(
                modified_key_with_char("c", "c", false, false, true, false),
                TermMode::NONE
            ),
            b"\x03"
        );
        assert_eq!(
            bytes(
                modified_key_with_char("c", "\x03", false, false, true, false),
                TermMode::NONE
            ),
            b"\x03"
        );
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
    fn tracks_synchronized_output_across_chunks() {
        let mut depth = 0;
        let mut tail = Vec::new();

        assert_eq!(
            update_synchronized_output_state(b"\x1b[?202", &mut depth, &mut tail),
            SyncOutputUpdate::default()
        );
        assert_eq!(depth, 0);

        assert_eq!(
            update_synchronized_output_state(b"6hpartial frame", &mut depth, &mut tail),
            SyncOutputUpdate {
                entered_from_idle: true,
                exited_to_idle: false,
                should_notify: false,
            }
        );
        assert_eq!(depth, 1);

        assert_eq!(
            update_synchronized_output_state(b"done\x1b[?2026l", &mut depth, &mut tail),
            SyncOutputUpdate {
                entered_from_idle: false,
                exited_to_idle: true,
                should_notify: true,
            }
        );
        assert_eq!(depth, 0);
    }

    #[test]
    fn reports_notify_when_synchronized_output_ends() {
        let mut depth = 0;
        let mut tail = Vec::new();

        assert_eq!(
            update_synchronized_output_state(b"\x1b[?2026hframe\x1b[?2026l", &mut depth, &mut tail),
            SyncOutputUpdate {
                entered_from_idle: true,
                exited_to_idle: true,
                should_notify: true,
            }
        );
        assert_eq!(depth, 0);
    }

    #[test]
    fn tracks_nested_synchronized_output() {
        let mut depth = 0;
        let mut tail = Vec::new();

        assert_eq!(
            update_synchronized_output_state(
                b"\x1b[?2026houter\x1b[?2026hinner",
                &mut depth,
                &mut tail,
            ),
            SyncOutputUpdate {
                entered_from_idle: true,
                exited_to_idle: false,
                should_notify: false,
            }
        );
        assert_eq!(depth, 2);

        assert_eq!(
            update_synchronized_output_state(b"\x1b[?2026l", &mut depth, &mut tail),
            SyncOutputUpdate {
                entered_from_idle: false,
                exited_to_idle: false,
                should_notify: true,
            }
        );
        assert_eq!(depth, 1);

        assert_eq!(
            update_synchronized_output_state(b"\x1b[?2026l", &mut depth, &mut tail),
            SyncOutputUpdate {
                entered_from_idle: false,
                exited_to_idle: true,
                should_notify: true,
            }
        );
        assert_eq!(depth, 0);
    }

    #[test]
    fn processed_output_updates_live_terminal_before_paint_snapshot() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"before");

        state.process_bytes(b"\r\x1b[2Kduring");
        let paint_snapshot = state.handle.snapshot();
        let paint_text: String = paint_snapshot
            .cells
            .iter()
            .filter(|cell| cell.point.line.0 == 0)
            .map(|cell| cell.c)
            .collect();
        assert!(!paint_text.contains("during"));

        let live = state.live_snapshot();

        let live_text: String = live
            .cells
            .iter()
            .filter(|cell| cell.point.line.0 == 0)
            .map(|cell| cell.c)
            .collect();

        assert!(live_text.contains("during"));

        state.handle.publish_snapshot();
        let published = state.handle.snapshot();
        let published_text: String = published
            .cells
            .iter()
            .filter(|cell| cell.point.line.0 == 0)
            .map(|cell| cell.c)
            .collect();

        assert!(published_text.contains("during"));
    }

    #[test]
    fn protocol_flags_detect_cursor_and_color_requests() {
        assert_eq!(
            terminal_protocol_flags(b"\x1b[?25lhello\x1b[?25h\x1b]10;?\x07\x1b]11;?\x07"),
            TerminalProtocolFlags {
                show_cursor: true,
                hide_cursor: true,
                osc_10_request: true,
                osc_11_request: true,
            }
        );
    }

    #[test]
    fn color_scheme_protocol_tracks_subscription_and_queries_across_chunks() {
        let mut state = TerminalColorSchemeState::default();

        assert_eq!(
            update_terminal_color_scheme_state(b"\x1b[?203", &mut state),
            TerminalColorSchemeUpdate::default()
        );
        assert!(!state.updates_enabled);

        assert_eq!(
            update_terminal_color_scheme_state(b"1h\x1b[?996n", &mut state),
            TerminalColorSchemeUpdate {
                enabled: true,
                disabled: false,
                query_count: 1,
            }
        );
        assert!(state.updates_enabled);

        assert_eq!(
            update_terminal_color_scheme_state(b"\x1b[?2031l", &mut state),
            TerminalColorSchemeUpdate {
                enabled: false,
                disabled: true,
                query_count: 0,
            }
        );
        assert!(!state.updates_enabled);
    }

    #[test]
    fn color_scheme_report_matches_xterm_codes() {
        assert_eq!(
            terminal_color_scheme_report(&ColorPalette::default()),
            b"\x1b[?997;1n"
        );

        let light = ColorPalette::builder()
            .background(0xee, 0xee, 0xee)
            .foreground(0x11, 0x11, 0x11)
            .build();
        assert_eq!(terminal_color_scheme_report(&light), b"\x1b[?997;2n");
    }

    #[test]
    fn color_scheme_queries_write_current_scheme() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.colors = ColorPalette::builder()
            .background(0xee, 0xee, 0xee)
            .foreground(0x11, 0x11, 0x11)
            .build();

        state.respond_to_color_scheme_queries(2);

        assert_eq!(state.written_bytes_for_test(), b"\x1b[?997;2n\x1b[?997;2n");
    }

    #[test]
    fn color_scheme_update_reports_theme_change_when_subscribed() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.color_scheme_state.updates_enabled = true;

        state.update_colors(
            ColorPalette::builder()
                .background(0xee, 0xee, 0xee)
                .foreground(0x11, 0x11, 0x11)
                .build(),
        );
        assert_eq!(state.written_bytes_for_test(), b"\x1b[?997;2n");

        state.update_colors(
            ColorPalette::builder()
                .background(0xdd, 0xdd, 0xdd)
                .foreground(0x22, 0x22, 0x22)
                .build(),
        );
        assert_eq!(state.written_bytes_for_test(), b"\x1b[?997;2n");

        state.update_colors(ColorPalette::default());
        assert_eq!(state.written_bytes_for_test(), b"\x1b[?997;2n\x1b[?997;1n");
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
    fn maps_macos_terminal_navigation_shortcuts() {
        assert_eq!(
            bytes(
                modified_key("left", false, true, false, false),
                TermMode::NONE
            ),
            b"\x1bb"
        );
        assert_eq!(
            bytes(
                modified_key("right", false, true, false, false),
                TermMode::NONE
            ),
            b"\x1bf"
        );
        assert_eq!(
            bytes(
                modified_key("left", false, false, false, true),
                TermMode::NONE
            ),
            b"\x01"
        );
        assert_eq!(
            bytes(
                modified_key("right", false, false, false, true),
                TermMode::NONE
            ),
            b"\x05"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("left", false, false, false, true, true),
                TermMode::NONE
            ),
            b"\x01"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("right", false, false, false, true, true),
                TermMode::NONE
            ),
            b"\x05"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("left", false, true, false, false, true),
                TermMode::NONE
            ),
            b"\x1bb"
        );
        assert_eq!(
            bytes(
                modified_key_with_function("right", false, true, false, false, true),
                TermMode::NONE
            ),
            b"\x1bf"
        );
        assert_eq!(
            bytes(
                modified_key("home", false, false, false, true),
                TermMode::NONE
            ),
            b"\x01"
        );
        assert_eq!(
            bytes(
                modified_key("end", false, false, false, true),
                TermMode::NONE
            ),
            b"\x05"
        );
        assert_eq!(
            bytes(
                modified_key("delete", false, true, false, false),
                TermMode::NONE
            ),
            b"\x1bd"
        );
        assert_eq!(
            bytes(
                modified_key("backspace", false, false, false, true),
                TermMode::NONE
            ),
            b"\x15"
        );
        assert_eq!(
            bytes(
                modified_key("back", false, false, false, true),
                TermMode::NONE
            ),
            b"\x15"
        );
        assert_eq!(
            bytes(
                modified_key("delete", false, false, false, true),
                TermMode::NONE
            ),
            b"\x0b"
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
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"hello\r\nworld");

        assert_eq!(
            state.handle.selected_text_for_range(SelectionRange {
                start: TerminalSelectionPoint { line: 0, col: 0 },
                end: TerminalSelectionPoint { line: 1, col: 5 },
            }),
            "hello\nworld"
        );
    }

    #[test]
    fn keeps_utf8_cjk_output_in_terminal_grid() {
        let mut state = TerminalModel::new_for_test(20, 4, 100);
        state.process_bytes("中文恢复记录".as_bytes());

        assert_eq!(
            state.handle.selected_text_for_range(SelectionRange {
                start: TerminalSelectionPoint { line: 0, col: 0 },
                end: TerminalSelectionPoint { line: 0, col: 11 },
            }),
            "中文恢复记录"
        );
    }

    #[test]
    fn alacritty_selection_tracks_output_scrollback_rotation() {
        let mut state = TerminalModel::new_for_test(10, 3, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree");
        state.start_selection(
            TerminalSelectionPoint { line: 1, col: 0 },
            TerminalSide::Left,
        );
        state.update_selection(
            TerminalSelectionPoint { line: 1, col: 3 },
            TerminalSide::Right,
        );
        assert_eq!(state.selected_text(), Some("two".to_string()));

        state.process_bytes(b"\r\nfour");

        assert_eq!(state.selected_text(), Some("two".to_string()));
        assert_eq!(
            state.selection_range(),
            Some(SelectionRange {
                start: TerminalSelectionPoint { line: 0, col: 0 },
                end: TerminalSelectionPoint { line: 0, col: 3 },
            })
        );
    }

    #[test]
    fn updates_render_snapshot_after_output() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"hello");

        let pending_snapshot = state.handle.snapshot();
        let pending_text: String = pending_snapshot
            .cells
            .iter()
            .filter(|cell| cell.point.line.0 == 0)
            .map(|cell| cell.c)
            .collect();
        assert!(!pending_text.contains("hello"));

        state.handle.publish_snapshot();

        let snapshot = state.handle.snapshot();
        let text: String = snapshot
            .cells
            .iter()
            .filter(|cell| cell.point.line.0 == 0)
            .map(|cell| cell.c)
            .collect();

        assert!(text.contains("hello"));
        assert_eq!(snapshot.columns, 10);
        assert_eq!(snapshot.screen_lines, 4);
    }

    #[test]
    fn updates_render_snapshot_after_resize() {
        let state = TerminalModel::new_for_test(10, 4, 100);
        let handle = state.handle.clone();
        assert!(handle.resize(20, 8));
        handle.publish_snapshot();

        let snapshot = handle.snapshot();
        assert_eq!(snapshot.columns, 20);
        assert_eq!(snapshot.screen_lines, 8);
        assert!(!handle.resize(20, 8));
    }

    #[test]
    fn display_cursor_tracks_scroll_offset_like_zed() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
        state.handle.publish_snapshot();
        assert!(state.scroll_display(2));
        let snapshot = state.sync_for_test();

        let display_cursor = DisplayCursor::from(snapshot.cursor.point, snapshot.display_offset);

        assert_eq!(snapshot.display_offset, 2);
        assert_eq!(
            display_cursor,
            DisplayCursor {
                row: snapshot.cursor.point.line.0 + 2,
                col: snapshot.cursor.point.column.0,
            }
        );
    }

    #[test]
    fn scroll_to_bottom_restores_input_viewport() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
        state.handle.publish_snapshot();
        assert!(state.scroll_display(2));
        let scrolled = state.sync_for_test();
        assert_eq!(scrolled.display_offset, 2);
        assert!(!scrolled.scrolled_to_bottom);

        state.prepare_input_viewport_for_test();
        let bottom = state.live_snapshot();

        assert_eq!(bottom.display_offset, 0);
        assert!(bottom.scrolled_to_bottom);
    }

    #[test]
    fn input_viewport_preparation_discards_pending_history_scroll() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
        state.handle.publish_snapshot();
        assert!(state.scroll_display(2));
        assert_eq!(state.sync_for_test().display_offset, 2);

        assert!(state.scroll_display(1));
        state.prepare_input_viewport_for_test();
        let snapshot = state.sync_for_test();

        assert_eq!(snapshot.display_offset, 0);
        assert!(snapshot.scrolled_to_bottom);
    }

    #[test]
    fn input_viewport_preparation_keeps_bottom_stable_across_drift_events() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight");
        state.handle.publish_snapshot();

        for lines in [2, -1, 3, 1] {
            assert!(state.scroll_display(lines));
        }
        state.prepare_input_viewport_for_test();
        let snapshot = state.sync_for_test();

        assert_eq!(snapshot.display_offset, 0);
        assert!(snapshot.scrolled_to_bottom);
    }

    #[test]
    fn keyboard_input_suppresses_residual_precise_scroll_from_same_gesture() {
        let mut state = TerminalScrollInputState {
            pending_lines: 3,
            pending_pixels: 12.0,
            frame_pending: false,
            suppress_residual_precise_scroll: false,
        };
        state.prepare_for_keyboard_input();

        assert_eq!(state.pending_lines, 0);
        assert_eq!(state.pending_pixels, 0.0);
        assert!(state.should_suppress_residual_scroll(&ScrollWheelEvent {
            delta: gpui::ScrollDelta::Pixels(Point {
                x: px(0.0),
                y: px(8.0),
            }),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        }));
        assert_eq!(state.pending_pixels, 0.0);
    }

    #[test]
    fn new_scroll_gesture_after_keyboard_input_is_not_suppressed() {
        let mut state = TerminalScrollInputState::default();
        state.prepare_for_keyboard_input();

        assert!(!state.should_suppress_residual_scroll(&ScrollWheelEvent {
            delta: gpui::ScrollDelta::Pixels(Point {
                x: px(0.0),
                y: px(8.0),
            }),
            touch_phase: TouchPhase::Started,
            ..Default::default()
        }));
        assert!(!state.should_suppress_residual_scroll(&ScrollWheelEvent {
            delta: gpui::ScrollDelta::Pixels(Point {
                x: px(0.0),
                y: px(8.0),
            }),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        }));
    }

    #[test]
    fn keyboard_input_does_not_suppress_line_wheel_scroll() {
        let mut state = TerminalScrollInputState::default();
        state.prepare_for_keyboard_input();

        assert!(!state.should_suppress_residual_scroll(&ScrollWheelEvent {
            delta: gpui::ScrollDelta::Lines(Point { x: 0.0, y: 1.0 }),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        }));
    }

    #[test]
    fn shift_click_extends_existing_terminal_selection_anchor() {
        let mut selection = SelectionState::default();
        selection.start(TerminalSelectionPoint { line: 2, col: 4 });
        selection.finish(TerminalSelectionPoint { line: 2, col: 8 });
        selection.extend(TerminalSelectionPoint { line: 4, col: 3 });

        assert_eq!(
            selection.anchor,
            Some(TerminalSelectionPoint { line: 2, col: 4 })
        );
        assert_eq!(
            selection.head,
            Some(TerminalSelectionPoint { line: 4, col: 3 })
        );
        assert!(selection.dragging);
    }

    #[test]
    fn ime_cursor_bounds_follow_current_viewport_after_history_scroll() {
        let mut layout = TerminalLayoutMetrics::default();
        layout.update(
            Bounds {
                origin: Point {
                    x: px(10.0),
                    y: px(20.0),
                },
                size: Size {
                    width: px(100.0),
                    height: px(80.0),
                },
            },
            Edges {
                top: px(2.0),
                right: px(3.0),
                bottom: px(4.0),
                left: px(5.0),
            },
            px(10.0),
            px(20.0),
            10,
            4,
        );

        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.process_bytes(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
        state.handle.publish_snapshot();

        assert!(state.scroll_display(2));
        let scrolled = state.sync_for_test();
        assert_eq!(scrolled.display_offset, 2);
        assert!(state.current_ime_cursor_bounds(&layout).is_none());

        state.prepare_input_viewport_for_test();
        let bottom = state.sync_for_test();
        let bounds = state.current_ime_cursor_bounds(&layout).unwrap();
        let row = DisplayCursor::from(bottom.cursor.point, bottom.display_offset).row;

        assert_eq!(bottom.display_offset, 0);
        assert!(bottom.scrolled_to_bottom);
        assert!(row >= 0);
        assert_eq!(
            bounds.origin.x,
            px(15.0) + px(10.0) * bottom.cursor.point.column.0 as f32
        );
        assert_eq!(bounds.origin.y, px(22.0) + px(20.0) * row as f32);
        assert_eq!(bounds.size.width, px(10.0));
        assert_eq!(bounds.size.height, px(20.0));
    }

    #[test]
    fn ime_bounds_for_range_offsets_from_current_cursor_cell() {
        let mut layout = TerminalLayoutMetrics::default();
        layout.update(
            Bounds {
                origin: Point {
                    x: px(10.0),
                    y: px(20.0),
                },
                size: Size {
                    width: px(100.0),
                    height: px(80.0),
                },
            },
            Edges {
                top: px(2.0),
                right: px(0.0),
                bottom: px(0.0),
                left: px(5.0),
            },
            px(10.0),
            px(20.0),
            10,
            4,
        );
        let cursor = Bounds {
            origin: Point {
                x: px(25.0),
                y: px(42.0),
            },
            size: Size {
                width: px(10.0),
                height: px(20.0),
            },
        };

        let bounds = ime_bounds_for_range(Some(cursor), &layout, 2..4).unwrap();

        assert_eq!(bounds.origin.x, px(45.0));
        assert_eq!(bounds.origin.y, px(42.0));
    }

    #[test]
    fn pending_session_reports_initial_layout_once() {
        let (binding, rx) = TerminalSessionBinding::pending();

        binding.record_layout(120, 36);

        assert_eq!(
            rx.recv_timeout(Duration::from_millis(10)).unwrap(),
            (120, 36)
        );
        binding.record_layout(121, 37);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn terminal_grid_dimension_tolerates_float_precision() {
        for cell in [8.1_f32, 9.7, 14.1, 20.1] {
            for count in 20..=200 {
                let available = count as f32 * cell;
                assert_eq!(terminal_grid_dimension(available, cell, 20), count);
            }
        }
        assert_eq!(terminal_grid_dimension(1.0, 8.0, 20), 20);
        assert_eq!(terminal_grid_dimension(1.0, 18.0, 1), 1);
    }

    #[test]
    fn detects_plain_terminal_urls_at_cell() {
        let mut state = TerminalModel::new_for_test(80, 4, 100);
        state.process_bytes(b"open https://example.com/path?x=1.\r\n");
        state.handle.publish_snapshot();
        let snapshot = state.handle.snapshot();

        let link = terminal_link_at_cell(&snapshot, TerminalCellPoint { row: 0, col: 12 })
            .expect("url under cursor");

        assert_eq!(link.url, "https://example.com/path?x=1");
        assert_eq!(link.line, 0);
        assert_eq!(link.range, 5..33);
        assert!(terminal_link_at_cell(&snapshot, TerminalCellPoint { row: 0, col: 2 }).is_none());
    }

    #[test]
    fn plain_url_detection_uses_terminal_columns() {
        let row_text = vec![
            (0, '中'),
            (2, ' '),
            (3, 'h'),
            (4, 't'),
            (5, 't'),
            (6, 'p'),
            (7, 's'),
            (8, ':'),
            (9, '/'),
            (10, '/'),
            (11, 'e'),
            (12, 'x'),
            (13, '.'),
            (14, 'c'),
            (15, 'o'),
            (16, 'm'),
            (17, ')'),
        ];

        let (url, range) = terminal_plain_url_at(&row_text, 12).expect("url under cursor");

        assert_eq!(url, "https://ex.com");
        assert_eq!(range, 3..17);
    }

    #[test]
    fn plain_url_detection_matches_xterm_style_boundaries() {
        let row_text: Vec<(usize, char)> =
            "(HTTPS://example.com/a?q=1),".chars().enumerate().collect();

        let (url, range) = terminal_plain_url_at(&row_text, 4).expect("url under cursor");

        assert_eq!(url, "HTTPS://example.com/a?q=1");
        assert_eq!(range, 1..26);
        assert!(terminal_plain_url_at(&row_text, 0).is_none());
        assert!(terminal_plain_url_at(&row_text, 26).is_none());
    }

    #[test]
    fn plain_url_detection_supports_file_urls() {
        let row_text: Vec<(usize, char)> = "open file:///tmp/codux-log.txt."
            .chars()
            .enumerate()
            .collect();

        let (url, range) = terminal_plain_url_at(&row_text, 12).expect("file url under cursor");

        assert_eq!(url, "file:///tmp/codux-log.txt");
        assert_eq!(range, 5..30);
    }

    #[test]
    fn inverse_cells_swap_foreground_and_background_colors() {
        let renderer = TerminalRenderer::new(
            default_terminal_font_family().to_string(),
            px(14.0),
            DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
            ColorPalette::default(),
        );
        let colors = Colors::default();
        let mut cell = Cell::default();
        cell.fg = Color::Named(NamedColor::Foreground);
        cell.bg = Color::Named(NamedColor::Background);

        let normal = renderer.cell_render_colors(&cell, &colors);
        cell.flags.insert(Flags::INVERSE);
        let inverse = renderer.cell_render_colors(&cell, &colors);

        assert_eq!(inverse.0, normal.1);
        assert_eq!(inverse.1, normal.0);
    }

    #[test]
    fn default_terminal_line_height_matches_renderer_cell_height() {
        let config = terminal_config();
        assert_eq!(
            config.line_height_multiplier,
            DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER
        );

        let renderer = TerminalRenderer::new(
            config.font_family,
            config.font_size,
            config.line_height_multiplier,
            config.colors,
        );
        assert_eq!(
            renderer.cell_height,
            config.font_size * DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER
        );
        assert!(config.paste_images_as_paths);
    }

    #[test]
    fn terminal_clipboard_image_payload_detection_filters_data_and_html() {
        assert!(clipboard_text_looks_like_image_payload(
            "data:image/png;base64,abc"
        ));
        assert!(clipboard_text_looks_like_image_payload(
            "<img src=\"data:image/png;base64,abc\">"
        ));
        assert!(!clipboard_text_looks_like_image_payload("/tmp/image.png"));
    }

    #[test]
    fn terminal_path_input_quotes_spaces() {
        assert_eq!(
            terminal_path_input(Path::new("/tmp/codux image.png")),
            "'/tmp/codux image.png'"
        );
        assert_eq!(
            terminal_path_input(Path::new("/tmp/codux-image.png")),
            "/tmp/codux-image.png"
        );
        assert_eq!(terminal_clipboard_image_extension(ImageFormat::Jpeg), "jpg");
    }

    #[test]
    fn bold_ansi_foreground_uses_bright_color() {
        let renderer = TerminalRenderer::new(
            default_terminal_font_family().to_string(),
            px(14.0),
            DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
            ColorPalette::default(),
        );
        let colors = Colors::default();
        let mut cell = Cell::default();
        cell.fg = Color::Named(NamedColor::Blue);
        cell.bg = Color::Named(NamedColor::Background);
        cell.flags.insert(Flags::BOLD);

        let (fg, _) = renderer.cell_render_colors(&cell, &colors);
        assert_eq!(
            fg,
            renderer
                .palette
                .resolve(Color::Named(NamedColor::BrightBlue), &colors)
        );
    }

    #[test]
    fn default_named_colors_ignore_stale_dynamic_terminal_colors() {
        let palette = ColorPalette::builder()
            .background(0xee, 0xee, 0xee)
            .foreground(0x11, 0x11, 0x11)
            .cursor(0x22, 0x22, 0x22)
            .build();
        let mut colors = Colors::default();
        colors[NamedColor::Background] = Some(Rgb {
            r: 0x11,
            g: 0x14,
            b: 0x1a,
        });
        colors[NamedColor::Foreground] = Some(Rgb {
            r: 0xd6,
            g: 0xda,
            b: 0xe2,
        });

        assert_eq!(
            palette.resolve(Color::Named(NamedColor::Background), &colors),
            palette.background
        );
        assert_eq!(
            palette.resolve(Color::Named(NamedColor::Foreground), &colors),
            palette.foreground
        );
    }

    #[test]
    fn color_requests_use_current_palette_for_default_colors() {
        let mut state = TerminalModel::new_for_test(10, 4, 100);
        state.colors = ColorPalette::builder()
            .background(0xee, 0xee, 0xee)
            .foreground(0x11, 0x11, 0x11)
            .cursor(0x22, 0x22, 0x22)
            .build();

        assert_eq!(
            state.color_request(NamedColor::Background as usize),
            Rgb {
                r: 0xee,
                g: 0xee,
                b: 0xee
            }
        );
    }

    #[test]
    fn inverse_bold_only_brightens_final_foreground() {
        let renderer = TerminalRenderer::new(
            default_terminal_font_family().to_string(),
            px(14.0),
            DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
            ColorPalette::default(),
        );
        let colors = Colors::default();
        let mut cell = Cell::default();
        cell.fg = Color::Named(NamedColor::Blue);
        cell.bg = Color::Named(NamedColor::Red);
        cell.flags.insert(Flags::BOLD | Flags::INVERSE);

        let (fg, bg) = renderer.cell_render_colors(&cell, &colors);
        assert_eq!(
            fg,
            renderer
                .palette
                .resolve(Color::Named(NamedColor::BrightRed), &colors)
        );
        assert_eq!(
            bg,
            renderer
                .palette
                .resolve(Color::Named(NamedColor::Blue), &colors)
        );
    }

    #[test]
    fn color_requests_use_configured_palette() {
        let palette = ColorPalette::builder()
            .background(0x28, 0x2A, 0x36)
            .foreground(0xF8, 0xF8, 0xF2)
            .cursor(0xF8, 0xF8, 0xF2)
            .selection(0x44, 0x47, 0x5A)
            .black(0x21, 0x22, 0x2C)
            .bright_black(0x62, 0x72, 0xA4)
            .build();

        assert_eq!(
            palette.color_request(NamedColor::Background as usize),
            Rgb {
                r: 0x28,
                g: 0x2A,
                b: 0x36
            }
        );
        assert_eq!(
            palette.color_request(NamedColor::Foreground as usize),
            Rgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            }
        );
        assert_eq!(
            palette.color_request(NamedColor::Black as usize),
            Rgb {
                r: 0x21,
                g: 0x22,
                b: 0x2C
            }
        );
        assert_eq!(
            palette.color_request(NamedColor::BrightBlack as usize),
            Rgb {
                r: 0x62,
                g: 0x72,
                b: 0xA4
            }
        );
        assert_eq!(
            palette.color_request(NamedColor::BrightForeground as usize),
            hsla_to_rgb(brighten_color(rgb_to_hsla(Rgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            })))
        );
        assert_eq!(
            palette.color_request(NamedColor::DimForeground as usize),
            hsla_to_rgb(dim_color(rgb_to_hsla(Rgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            })))
        );
        assert_eq!(
            palette.color_request(999),
            Rgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            }
        );
    }
}
