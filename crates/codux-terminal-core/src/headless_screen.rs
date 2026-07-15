use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::thread;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config as AlacrittyConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor};
use base64::{Engine as _, engine::general_purpose};
use serde::Serialize;

use crate::TerminalInputMode;

/// Receives reply bytes the VT engine writes back to the PTY in response
/// to queries (DSR/CPR, DECRQM, DA, kitty keyboard, XTVERSION, ...).
/// Invoked on the screen worker thread.
pub type TerminalPtyResponder = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// Transient engine events for the embedder (not screen state). Only live
/// output triggers them; replayed history never does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalScreenEvent {
    /// OSC 52 clipboard store, already base64-decoded by the engine.
    ClipboardStore(String),
    /// BEL / CSI bell.
    Bell,
}

pub type TerminalScreenEventSink = Arc<dyn Fn(TerminalScreenEvent) + Send + Sync>;

const PROCESS_CHUNK_BYTES: usize = 64 * 1024;
const TERMINAL_CELL_WIDTH_PX: u32 = 10;
const TERMINAL_CELL_HEIGHT_PX: u32 = 20;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalScreenSnapshot {
    pub data: String,
    pub cols: usize,
    pub rows: usize,
    pub total_lines: usize,
    pub display_offset: usize,
    /// Rows at the top of the grid that are overscan context (content above
    /// the visible viewport, pre-rendered for smooth scrolling). 0 for
    /// plain viewport snapshots.
    #[serde(default)]
    pub margin_rows: usize,
    /// Rows at the bottom of the grid that are overscan context (content
    /// below the visible viewport, pre-rendered for smooth scrolling). 0
    /// for plain viewport snapshots.
    #[serde(default)]
    pub margin_rows_below: usize,
    pub scroll_pixel_offset: f64,
    pub application_cursor: bool,
    pub input_mode: TerminalInputMode,
    /// Window title set by the shell via OSC 0/2; None until one arrives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Per-viewport-row soft-wrap flags: true means the row continues onto the
    /// next without a hard line break, so copy joins them as one line. Empty
    /// when unknown (for example, when an older peer omits the metadata).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrapped_rows: Vec<bool>,
    /// OSC 133;A prompt-start lines in absolute buffer coordinates (0 =
    /// oldest retained scrollback line), sorted ascending.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompt_marks: Vec<usize>,
    /// Inline images (iTerm2 OSC 1337 File) intersecting the viewport.
    /// Not serialized: the desktop renders in-process; remote viewers do
    /// not receive images yet.
    #[serde(skip)]
    pub images: Vec<TerminalScreenImage>,
    pub cells: Vec<TerminalScreenCellSnapshot>,
    pub cursor: TerminalScreenCursorSnapshot,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TerminalScreenCursorSnapshot {
    pub row: usize,
    pub col: usize,
    pub visible: bool,
    pub shape: TerminalScreenCursorShape,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalScreenCursorShape {
    #[default]
    Block,
    Beam,
    Underline,
    HollowBlock,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalScreenCellSnapshot {
    pub row: i32,
    pub col: usize,
    pub text: String,
    pub width: usize,
    pub fg: TerminalScreenColor,
    pub bg: TerminalScreenColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: TerminalScreenUnderline,
    /// SGR 58 underline color override; None means the text foreground.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline_color: Option<TerminalScreenColor>,
    /// OSC 8 hyperlink URI attached to this cell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    pub inverse: bool,
    pub hidden: bool,
    pub strikeout: bool,
}

/// Underline style carried per cell (SGR 4 and 4:x colon subparams).
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalScreenUnderline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TerminalScreenColor {
    Default,
    Named { name: String },
    Rgb { r: u8, g: u8, b: u8 },
    Indexed { index: u8 },
}

/// An inline image laid over the cell grid. Equality ignores the pixel data
/// (the id is unique per decoded image), keeping snapshot comparison cheap.
#[derive(Clone, Debug)]
pub struct TerminalScreenImage {
    pub id: u64,
    /// Viewport row of the top edge; negative when partly scrolled off.
    pub row: i32,
    pub col: usize,
    pub rows: usize,
    pub cols: usize,
    /// Original encoded bytes (PNG/JPEG/GIF/WebP...).
    pub data: Arc<Vec<u8>>,
}

impl PartialEq for TerminalScreenImage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.row == other.row
            && self.col == other.col
            && self.rows == other.rows
            && self.cols == other.cols
    }
}

/// Double-click selects a word (alacritty semantic search); triple-click a
/// whole logical line (following soft wraps).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalSelectionSpanKind {
    Word,
    Line,
}

/// A resolved selection span in absolute buffer coordinates (line 0 = oldest
/// scrollback), with an exclusive end column — ready to hand to the UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalSelectionSpan {
    pub start_line: i32,
    pub start_col: usize,
    pub end_line: i32,
    pub end_col: usize,
}

pub struct HeadlessTerminalScreen {
    engine: TerminalScreenEngine,
    pending_scroll_pixels: f64,
}

pub struct HeadlessTerminalSnapshotRequest {
    rx: mpsc::Receiver<TerminalScreenSnapshot>,
}

impl HeadlessTerminalSnapshotRequest {
    pub fn snapshot(self) -> TerminalScreenSnapshot {
        self.rx.recv().unwrap_or_default()
    }
}

impl HeadlessTerminalScreen {
    pub fn new(cols: usize, rows: usize, scrollback: usize) -> Self {
        Self::new_with_responder(cols, rows, scrollback, None)
    }

    /// Like [`Self::new`], but installs a responder that receives the VT
    /// engine's query replies (DSR/CPR, DECRQM, DA, ...) for forwarding to
    /// the PTY.
    pub fn new_with_responder(
        cols: usize,
        rows: usize,
        scrollback: usize,
        responder: Option<TerminalPtyResponder>,
    ) -> Self {
        Self {
            engine: TerminalScreenEngine::new(cols, rows, scrollback, responder),
            pending_scroll_pixels: 0.0,
        }
    }

    pub fn set_event_sink(&mut self, sink: TerminalScreenEventSink) {
        self.engine.send(TerminalScreenCommand::SetEventSink(sink));
    }

    pub fn process(&mut self, bytes: &[u8]) {
        // Large writes are split so interleaved worker queries (snapshot,
        // display offset) wait for one chunk instead of one multi-second
        // parse. The VT parser is incremental; arbitrary split points are
        // safe.
        for chunk in bytes.chunks(PROCESS_CHUNK_BYTES) {
            self.engine.process(chunk);
        }
    }

    /// Process recorded output without answering the queries it contains.
    /// Replayed history can carry stale DSR/DA queries from a previous run;
    /// answering those would inject unsolicited reply bytes into the PTY.
    pub fn process_replay(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(PROCESS_CHUNK_BYTES) {
            if !chunk.is_empty() {
                self.engine
                    .send(TerminalScreenCommand::ProcessReplay(chunk.to_vec()));
            }
        }
    }

    /// Replace the active visible grid from an authoritative keyframe without
    /// moving the previous viewport into scrollback.
    pub fn replace_visible_with_keyframe(&mut self, bytes: &[u8]) {
        self.pending_scroll_pixels = 0.0;
        self.engine
            .send(TerminalScreenCommand::ReplaceVisible(bytes.to_vec()));
    }

    pub(crate) fn restore_visible_wrapped_rows(&mut self, wrapped_rows: &[bool]) {
        self.engine.send(TerminalScreenCommand::RestoreWrappedRows(
            wrapped_rows.to_vec(),
        ));
    }

    pub fn replace_with_keyframe(&mut self, bytes: &[u8]) {
        self.clear();
        // Keyframes are recorded output; never answer the queries they
        // contain.
        self.process_replay(bytes);
        self.process_replay(b"\x1b[3J");
        self.scroll_to_bottom();
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.pending_scroll_pixels = 0.0;
        self.engine.resize(cols, rows);
    }

    pub fn set_scrollback(&mut self, scrollback: usize) {
        self.engine.set_scrollback(scrollback);
    }

    pub fn scroll_lines(&mut self, lines: i32) {
        if lines == 0 {
            return;
        }
        self.pending_scroll_pixels = 0.0;
        self.engine.scroll_lines(lines);
    }

    pub fn scroll_pixels(&mut self, pixels: f64, cell_height: f64) {
        if !pixels.is_finite() || pixels == 0.0 || !cell_height.is_finite() || cell_height <= 0.0 {
            return;
        }
        self.pending_scroll_pixels += pixels;
        let requested_lines = (self.pending_scroll_pixels / cell_height).trunc() as i32;
        if requested_lines != 0 {
            let previous_offset = self.engine.display_offset() as i32;
            self.engine.scroll_lines(requested_lines);
            let applied_lines = self.engine.display_offset() as i32 - previous_offset;
            self.pending_scroll_pixels -= applied_lines as f64 * cell_height;
            if applied_lines != requested_lines
                && ((requested_lines > 0 && self.pending_scroll_pixels > 0.0)
                    || (requested_lines < 0 && self.pending_scroll_pixels < 0.0))
            {
                self.pending_scroll_pixels = 0.0;
            }
        }
        if self.engine.display_offset() == 0 && self.pending_scroll_pixels < 0.0 {
            self.pending_scroll_pixels = 0.0;
        }
        if self.pending_scroll_pixels > 0.0 && !self.engine.has_history_above_viewport() {
            self.pending_scroll_pixels = 0.0;
        }
    }

    pub fn settle_pixel_scroll(&mut self) {
        // Pixel scrolling intentionally allows the viewport to stop between
        // terminal rows. Snapping here makes every drag look like a row-based
        // rebound; true bounds are already clamped in `scroll_pixels`.
    }

    pub fn scroll_to_bottom(&mut self) {
        self.pending_scroll_pixels = 0.0;
        self.engine.scroll_to_bottom();
    }

    /// Scroll the viewport to an absolute history offset (0 = bottom).
    /// The delta is computed on the worker against the live engine offset,
    /// so callers never compound errors from a lagging published offset.
    pub fn scroll_to_offset(&mut self, offset: usize) {
        self.pending_scroll_pixels = 0.0;
        self.engine.scroll_to_offset(offset);
    }

    /// Current input mode read from the live engine (blocking worker
    /// round-trip). Use for decisions that must not act on a stale
    /// published snapshot, e.g. bracketed paste.
    pub fn input_mode(&self) -> TerminalInputMode {
        self.engine.input_mode()
    }

    /// Request a snapshot of the viewport as it would appear at `offset`,
    /// without disturbing the current scroll position. Used to pre-render
    /// overscan context for remote scrolling.
    pub fn snapshot_at_offset_request(&self, offset: usize) -> HeadlessTerminalSnapshotRequest {
        let (tx, rx) = mpsc::channel();
        let _ = self
            .engine
            .tx
            .send(TerminalScreenCommand::SnapshotAtOffset { offset, reply: tx });
        HeadlessTerminalSnapshotRequest { rx }
    }

    pub fn display_offset(&self) -> usize {
        self.engine.display_offset()
    }

    pub fn clear(&mut self) {
        self.engine.clear();
        self.pending_scroll_pixels = 0.0;
    }

    /// Drop scrollback and screen but keep the cursor row as the new top line,
    /// so the shell prompt survives (iTerm/Zed-style clear).
    pub fn clear_keep_prompt(&mut self) {
        self.engine.clear_keep_prompt();
        self.pending_scroll_pixels = 0.0;
    }

    pub fn snapshot(&self) -> TerminalScreenSnapshot {
        self.engine.snapshot(self.pending_scroll_pixels)
    }

    /// Resolve the word/line selection span around a buffer cell. Blocks on the
    /// screen worker (a rare click action), like [`Self::display_offset`].
    pub fn selection_span(
        &self,
        line: i32,
        col: usize,
        kind: TerminalSelectionSpanKind,
    ) -> Option<TerminalSelectionSpan> {
        self.engine.selection_span(line, col, kind)
    }

    /// Request an asynchronous snapshot. `include_data` controls whether the
    /// ANSI repaint string (`TerminalScreenSnapshot::data`) is generated;
    /// consumers that only read `cells` should pass `false` to skip that
    /// work on the screen worker.
    pub fn snapshot_request(&self, include_data: bool) -> HeadlessTerminalSnapshotRequest {
        self.engine
            .snapshot_request(self.pending_scroll_pixels, include_data)
    }

    /// Build a read-only viewport snapshot for a remote observer. The
    /// requested offset and reported total are clamped to the trailing
    /// `max_lines` window so mobile scrollback cannot expose an unbounded
    /// virtual range, and the live terminal viewport is restored before the
    /// command completes.
    pub fn remote_viewport_snapshot_request(
        &self,
        display_offset: usize,
        overscan_rows: usize,
        max_lines: usize,
    ) -> HeadlessTerminalSnapshotRequest {
        self.engine
            .remote_viewport_snapshot_request(display_offset, overscan_rows, max_lines)
    }
}

#[derive(Clone)]
struct TerminalScreenEngine {
    tx: mpsc::Sender<TerminalScreenCommand>,
}

impl TerminalScreenEngine {
    fn new(
        cols: usize,
        rows: usize,
        scrollback: usize,
        responder: Option<TerminalPtyResponder>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("codux-terminal-screen".to_string())
            .spawn(move || {
                TerminalScreenWorker::new(cols, rows, scrollback, responder).run(rx);
            })
            .expect("failed to spawn terminal screen worker");
        Self { tx }
    }

    fn clear(&mut self) {
        self.send(TerminalScreenCommand::Clear);
    }

    fn clear_keep_prompt(&mut self) {
        self.send(TerminalScreenCommand::ClearKeepPrompt);
    }

    fn send(&self, command: TerminalScreenCommand) {
        let _ = self.tx.send(command);
    }

    fn request<R: Default>(
        &self,
        build: impl FnOnce(mpsc::Sender<R>) -> TerminalScreenCommand,
    ) -> R {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(build(tx)).is_err() {
            return R::default();
        }
        rx.recv().unwrap_or_default()
    }

    fn process(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.send(TerminalScreenCommand::Process(bytes.to_vec()));
        }
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        self.send(TerminalScreenCommand::Resize { cols, rows });
    }

    fn set_scrollback(&mut self, scrollback: usize) {
        self.send(TerminalScreenCommand::SetScrollback(scrollback));
    }

    fn scroll_lines(&mut self, lines: i32) {
        if lines != 0 {
            self.send(TerminalScreenCommand::ScrollLines(lines));
        }
    }

    fn scroll_to_bottom(&mut self) {
        self.send(TerminalScreenCommand::ScrollToBottom);
    }

    fn scroll_to_offset(&mut self, offset: usize) {
        self.send(TerminalScreenCommand::ScrollToOffset(offset));
    }

    fn display_offset(&self) -> usize {
        self.request(TerminalScreenCommand::DisplayOffset)
    }

    fn input_mode(&self) -> TerminalInputMode {
        self.request(TerminalScreenCommand::InputMode)
    }

    fn selection_span(
        &self,
        line: i32,
        col: usize,
        kind: TerminalSelectionSpanKind,
    ) -> Option<TerminalSelectionSpan> {
        self.request(|reply| TerminalScreenCommand::SelectionSpan {
            line,
            col,
            kind,
            reply,
        })
    }

    fn snapshot(&self, scroll_pixel_offset: f64) -> TerminalScreenSnapshot {
        self.request(|reply| TerminalScreenCommand::Snapshot {
            scroll_pixel_offset,
            include_data: true,
            reply,
        })
    }

    fn snapshot_request(
        &self,
        scroll_pixel_offset: f64,
        include_data: bool,
    ) -> HeadlessTerminalSnapshotRequest {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(TerminalScreenCommand::Snapshot {
            scroll_pixel_offset,
            include_data,
            reply: tx,
        });
        HeadlessTerminalSnapshotRequest { rx }
    }

    fn remote_viewport_snapshot_request(
        &self,
        display_offset: usize,
        overscan_rows: usize,
        max_lines: usize,
    ) -> HeadlessTerminalSnapshotRequest {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.send(TerminalScreenCommand::RemoteViewportSnapshot {
            display_offset,
            overscan_rows,
            max_lines,
            reply: tx,
        });
        HeadlessTerminalSnapshotRequest { rx }
    }

    fn has_history_above_viewport(&self) -> bool {
        self.request(TerminalScreenCommand::HasHistoryAboveViewport)
    }
}

enum TerminalScreenCommand {
    Process(Vec<u8>),
    ProcessReplay(Vec<u8>),
    ReplaceVisible(Vec<u8>),
    RestoreWrappedRows(Vec<bool>),
    SetEventSink(TerminalScreenEventSink),
    Resize {
        cols: usize,
        rows: usize,
    },
    ScrollLines(i32),
    SetScrollback(usize),
    ScrollToBottom,
    ScrollToOffset(usize),
    DisplayOffset(mpsc::Sender<usize>),
    InputMode(mpsc::Sender<TerminalInputMode>),
    HasHistoryAboveViewport(mpsc::Sender<bool>),
    Snapshot {
        scroll_pixel_offset: f64,
        include_data: bool,
        reply: mpsc::Sender<TerminalScreenSnapshot>,
    },
    // Atomic peek at another history offset: scrolls, snapshots, restores.
    // Atomicity inside one command keeps concurrent snapshot consumers from
    // observing the temporary position.
    SnapshotAtOffset {
        offset: usize,
        reply: mpsc::Sender<TerminalScreenSnapshot>,
    },
    RemoteViewportSnapshot {
        display_offset: usize,
        overscan_rows: usize,
        max_lines: usize,
        reply: mpsc::Sender<TerminalScreenSnapshot>,
    },
    SelectionSpan {
        line: i32,
        col: usize,
        kind: TerminalSelectionSpanKind,
        reply: mpsc::Sender<Option<TerminalSelectionSpan>>,
    },
    Clear,
    ClearKeepPrompt,
}

/// Fixed grid size handed to the alacritty `Term`; scrollback is configured
/// separately via [`AlacrittyConfig::scrolling_history`].
struct HeadlessTermSize {
    cols: usize,
    rows: usize,
}

impl HeadlessTermSize {
    fn new(cols: usize, rows: usize) -> Self {
        Self { cols, rows }
    }
}

impl Dimensions for HeadlessTermSize {
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

/// Buffers the VT engine's query replies (DSR/CPR, DECRQM, DA, ...) emitted
/// while parsing. They are drained to the PTY responder after each parse
/// completes; `Rc`/`RefCell` are sound because the worker (and the engine it
/// owns) live entirely on the dedicated worker thread.
#[derive(Clone)]
struct HeadlessEventProxy {
    events: Rc<RefCell<Vec<Event>>>,
}

impl EventListener for HeadlessEventProxy {
    fn send_event(&self, event: Event) {
        self.events.borrow_mut().push(event);
    }
}

struct TerminalScreenWorker {
    term: Term<HeadlessEventProxy>,
    config: AlacrittyConfig,
    parser: Processor,
    events: Rc<RefCell<Vec<Event>>>,
    cols: usize,
    rows: usize,
    scrollback: usize,
    responder: Option<TerminalPtyResponder>,
    event_sink: Option<TerminalScreenEventSink>,
    title: Option<String>,
    /// OSC 133;A prompt-start lines (absolute buffer coordinates at record
    /// time); exact until the scrollback cap starts dropping lines.
    prompt_marks: Vec<usize>,
    /// Chunk-boundary carry: bytes held back because they may be the start
    /// of a split intercepted sequence (prompt mark or inline image).
    prompt_mark_carry: Vec<u8>,
    /// Stored inline images anchored to absolute buffer lines.
    images: Vec<StoredInlineImage>,
    /// In-flight OSC 1337 capture spanning feed chunks.
    image_capture: Option<TerminalImageCapture>,
    next_image_id: u64,
}

struct StoredInlineImage {
    id: u64,
    line: usize,
    col: usize,
    rows: usize,
    cols: usize,
    alt_screen: bool,
    data: Arc<Vec<u8>>,
}

struct TerminalImageCapture {
    buffer: Vec<u8>,
    overflowed: bool,
}

impl TerminalScreenWorker {
    fn new(
        cols: usize,
        rows: usize,
        scrollback: usize,
        responder: Option<TerminalPtyResponder>,
    ) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = AlacrittyConfig {
            scrolling_history: scrollback,
            // The engine then answers `CSI ? u` queries and tracks the kitty
            // keyboard flag stack; the key encoder reads the flags back via
            // TerminalInputMode::kitty_flags.
            kitty_keyboard: true,
            ..Default::default()
        };
        let term = Term::new(
            config.clone(),
            &HeadlessTermSize::new(cols, rows),
            HeadlessEventProxy {
                events: events.clone(),
            },
        );
        Self {
            term,
            config,
            parser: Processor::new(),
            events,
            cols,
            rows,
            scrollback,
            responder,
            event_sink: None,
            title: None,
            prompt_marks: Vec::new(),
            prompt_mark_carry: Vec::new(),
            images: Vec::new(),
            image_capture: None,
            next_image_id: 0,
        }
    }

    /// Feed bytes to the parser, then dispatch any query replies the engine
    /// produced. `answer_queries` is false for replayed history so stale
    /// DSR/DA queries in recorded output don't inject unsolicited replies.
    fn feed(&mut self, bytes: &[u8], answer_queries: bool) {
        self.advance_with_intercepts(bytes);
        let events: Vec<Event> = self.events.borrow_mut().drain(..).collect();
        // Titles apply for replayed history too: restored output re-sets the
        // title the shell had established before the restore.
        for event in &events {
            match event {
                Event::Title(title) => self.title = Some(title.clone()),
                Event::ResetTitle => self.title = None,
                _ => {}
            }
        }
        if !answer_queries {
            return;
        }
        for event in events {
            match event {
                Event::PtyWrite(text) => {
                    if let Some(responder) = self.responder.as_ref() {
                        responder(text.as_bytes());
                    }
                }
                Event::TextAreaSizeRequest(format) => {
                    if let Some(responder) = self.responder.as_ref() {
                        responder(format(self.window_size()).as_bytes());
                    }
                }
                // OSC 52 write; loads are ignored (remote clipboard reads are
                // a data leak, matching most terminals' default).
                Event::ClipboardStore(_, text) => {
                    if let Some(sink) = self.event_sink.as_ref() {
                        sink(TerminalScreenEvent::ClipboardStore(text));
                    }
                }
                Event::Bell => {
                    if let Some(sink) = self.event_sink.as_ref() {
                        sink(TerminalScreenEvent::Bell);
                    }
                }
                // OSC color queries are intentionally not answered here; the
                // embedder resolves those from its own theme palette.
                _ => {}
            }
        }
    }

    fn window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: self.rows.try_into().unwrap_or(u16::MAX),
            num_cols: self.cols.try_into().unwrap_or(u16::MAX),
            cell_width: TERMINAL_CELL_WIDTH_PX as u16,
            cell_height: TERMINAL_CELL_HEIGHT_PX as u16,
        }
    }

    /// Advance the parser around the sequences alacritty ignores but we
    /// handle ourselves: OSC 133;A prompt marks (cursor sampled where the
    /// prompt is about to print) and OSC 1337 inline images (payload kept
    /// out of the VT parser entirely). A tail that could be a split
    /// sequence start is held back until the next feed.
    fn advance_with_intercepts(&mut self, bytes: &[u8]) {
        const MARK: &[u8] = b"\x1b]133;A";
        const IMAGE: &[u8] = b"\x1b]1337;File=";
        let carried;
        let data: &[u8] = if self.prompt_mark_carry.is_empty() {
            bytes
        } else {
            carried = [std::mem::take(&mut self.prompt_mark_carry), bytes.to_vec()].concat();
            &carried
        };

        let mut start = 0;
        let mut index = 0;
        if self.image_capture.is_some() {
            index = self.continue_image_capture(data, 0);
            start = index;
            if self.image_capture.is_some() {
                return;
            }
        }
        while index < data.len() {
            if data[index..].starts_with(MARK) {
                self.parser.advance(&mut self.term, &data[start..index]);
                self.record_prompt_mark();
                // The marker bytes still go to the parser (it ignores them).
                start = index;
                index += MARK.len();
            } else if data[index..].starts_with(IMAGE) {
                self.parser.advance(&mut self.term, &data[start..index]);
                self.image_capture = Some(TerminalImageCapture {
                    buffer: Vec::new(),
                    overflowed: false,
                });
                index = self.continue_image_capture(data, index + IMAGE.len());
                start = index;
                if self.image_capture.is_some() {
                    return;
                }
            } else {
                index += 1;
            }
        }

        let mut held = data.len();
        for candidate in data.len().saturating_sub(IMAGE.len() - 1).max(start)..data.len() {
            let suffix = &data[candidate..];
            if MARK.starts_with(suffix) || IMAGE.starts_with(suffix) {
                held = candidate;
                break;
            }
        }
        self.parser.advance(&mut self.term, &data[start..held]);
        self.prompt_mark_carry = data[held..].to_vec();
    }

    /// Consume capture bytes until BEL / ST; returns the index just past the
    /// terminator, or `data.len()` when the capture continues into the next
    /// feed. The payload never reaches the VT parser.
    fn continue_image_capture(&mut self, data: &[u8], from: usize) -> usize {
        // Encoded base64 cap (~24MB decoded), far above realistic previews.
        const IMAGE_CAPTURE_MAX: usize = 32 * 1024 * 1024;

        // Terminator split across feeds: buffered trailing ESC + leading '\'.
        if from < data.len()
            && data[from] == b'\\'
            && self
                .image_capture
                .as_mut()
                .is_some_and(|capture| capture.buffer.last() == Some(&0x1b))
        {
            if let Some(capture) = self.image_capture.as_mut() {
                capture.buffer.pop();
            }
            self.finish_image_capture();
            return from + 1;
        }

        let mut index = from;
        while index < data.len() {
            let byte = data[index];
            if byte == 0x07 {
                self.append_image_capture(&data[from..index], IMAGE_CAPTURE_MAX);
                self.finish_image_capture();
                return index + 1;
            }
            if byte == 0x1b && data.get(index + 1) == Some(&b'\\') {
                self.append_image_capture(&data[from..index], IMAGE_CAPTURE_MAX);
                self.finish_image_capture();
                return index + 2;
            }
            index += 1;
        }
        self.append_image_capture(&data[from..], IMAGE_CAPTURE_MAX);
        data.len()
    }

    fn append_image_capture(&mut self, bytes: &[u8], max: usize) {
        let Some(capture) = self.image_capture.as_mut() else {
            return;
        };
        if capture.overflowed || capture.buffer.len() + bytes.len() > max {
            capture.overflowed = true;
            // Keep only a possible trailing ESC for terminator detection.
            capture.buffer.clear();
            if bytes.last() == Some(&0x1b) {
                capture.buffer.push(0x1b);
            }
            return;
        }
        capture.buffer.extend_from_slice(bytes);
    }

    fn finish_image_capture(&mut self) {
        let Some(capture) = self.image_capture.take() else {
            return;
        };
        if capture.overflowed {
            return;
        }
        let buffer = capture.buffer;
        let Some(colon) = buffer.iter().position(|byte| *byte == b':') else {
            return;
        };
        let mut inline = false;
        let mut width_arg = None;
        let mut height_arg = None;
        for pair in String::from_utf8_lossy(&buffer[..colon]).split(';') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            match key {
                "inline" => inline = value == "1",
                "width" => width_arg = Some(value.to_string()),
                "height" => height_arg = Some(value.to_string()),
                _ => {}
            }
        }
        // Non-inline OSC 1337 files are downloads, which we don't accept.
        if !inline {
            return;
        }
        let payload: Vec<u8> = buffer[colon + 1..]
            .iter()
            .copied()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();
        let Ok(data) = general_purpose::STANDARD.decode(&payload) else {
            return;
        };
        let Ok(dimensions) = imagesize::blob_size(&data) else {
            return;
        };
        let (cols, rows) = terminal_image_cell_span(
            dimensions.width,
            dimensions.height,
            width_arg.as_deref(),
            height_arg.as_deref(),
            self.term.columns(),
            self.term.screen_lines(),
        );
        let history = self.term.grid().history_size() as i32;
        let line = (self.term.grid().cursor.point.line.0 + history).max(0) as usize;
        let col = self.term.grid().cursor.point.column.0;
        let id = self.next_image_id;
        self.next_image_id += 1;
        self.images.push(StoredInlineImage {
            id,
            line,
            col,
            rows,
            cols,
            alt_screen: self.term.mode().contains(TermMode::ALT_SCREEN),
            data: Arc::new(data),
        });
        self.enforce_image_budget();
        // Reserve grid rows: the cursor lands on the line below the image,
        // scrolling the buffer as needed (iTerm2 semantics).
        self.parser
            .advance(&mut self.term, "\r\n".repeat(rows).as_bytes());
    }

    // Newest images win; a preview-heavy session can't grow unbounded.
    fn enforce_image_budget(&mut self) {
        const MAX_IMAGES: usize = 32;
        // Conservative: this engine also runs inside the mobile app.
        const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
        while self.images.len() > MAX_IMAGES
            || self
                .images
                .iter()
                .map(|image| image.data.len())
                .sum::<usize>()
                > MAX_TOTAL_BYTES
        {
            self.images.remove(0);
        }
    }

    fn record_prompt_mark(&mut self) {
        let history = self.term.grid().history_size() as i32;
        let line = self.term.grid().cursor.point.line.0 + history;
        if line < 0 {
            return;
        }
        let line = line as usize;
        if self.prompt_marks.last() == Some(&line) {
            return;
        }
        self.prompt_marks.push(line);
        if self.prompt_marks.len() > 500 {
            self.prompt_marks.remove(0);
        }
    }

    fn retained_prompt_marks(&self, total_lines: usize) -> Vec<usize> {
        let mut marks: Vec<usize> = self
            .prompt_marks
            .iter()
            .copied()
            .filter(|line| *line < total_lines)
            .collect();
        marks.sort_unstable();
        marks.dedup();
        marks
    }

    fn run(mut self, rx: mpsc::Receiver<TerminalScreenCommand>) {
        let mut deferred = None;
        loop {
            let command = match deferred.take() {
                Some(command) => command,
                None => match rx.recv() {
                    Ok(command) => command,
                    Err(_) => break,
                },
            };
            match command {
                TerminalScreenCommand::Process(bytes) => self.feed(&bytes, true),
                TerminalScreenCommand::ProcessReplay(bytes) => self.feed(&bytes, false),
                TerminalScreenCommand::ReplaceVisible(bytes) => {
                    let targets_alternate_screen = bytes.starts_with(b"\x1b[?1049h");
                    if self.term.mode().contains(TermMode::ALT_SCREEN) != targets_alternate_screen {
                        self.term.swap_alt();
                    }
                    if let Some(index) = bytes.windows(4).position(|window| window == b"\x1b[2J") {
                        self.feed(&bytes[..index], false);
                        self.term.grid_mut().reset_region(..);
                        self.feed(&bytes[index + 4..], false);
                    } else {
                        self.term.grid_mut().reset_region(..);
                        self.feed(&bytes, false);
                    }
                }
                TerminalScreenCommand::RestoreWrappedRows(wrapped_rows) => {
                    self.restore_visible_wrapped_rows(&wrapped_rows);
                }
                TerminalScreenCommand::SetEventSink(sink) => {
                    self.event_sink = Some(sink);
                }
                TerminalScreenCommand::Resize { mut cols, mut rows } => {
                    // Layout settling queues resizes back-to-back, and every
                    // column change reflows the whole scrollback. Collapse a
                    // consecutive run of resizes into the final one; ordering
                    // relative to other commands is preserved.
                    while let Ok(following) = rx.try_recv() {
                        match following {
                            TerminalScreenCommand::Resize {
                                cols: next_cols,
                                rows: next_rows,
                            } => {
                                cols = next_cols;
                                rows = next_rows;
                            }
                            other => {
                                deferred = Some(other);
                                break;
                            }
                        }
                    }
                    self.resize(cols, rows);
                }
                TerminalScreenCommand::ScrollLines(lines) => self.scroll_lines(lines),
                TerminalScreenCommand::SetScrollback(scrollback) => self.set_scrollback(scrollback),
                TerminalScreenCommand::ScrollToBottom => {
                    self.term.scroll_display(Scroll::Bottom);
                }
                TerminalScreenCommand::ScrollToOffset(offset) => self.scroll_to_offset(offset),
                TerminalScreenCommand::DisplayOffset(reply) => {
                    let _ = reply.send(self.display_offset());
                }
                TerminalScreenCommand::InputMode(reply) => {
                    let _ = reply.send(terminal_input_mode(&self.term));
                }
                TerminalScreenCommand::HasHistoryAboveViewport(reply) => {
                    let _ = reply.send(self.has_history_above_viewport());
                }
                TerminalScreenCommand::Snapshot {
                    scroll_pixel_offset,
                    include_data,
                    reply,
                } => {
                    let _ = reply.send(self.snapshot(scroll_pixel_offset, include_data));
                }
                TerminalScreenCommand::SnapshotAtOffset { offset, reply } => {
                    let saved = self.display_offset();
                    self.scroll_to_offset(offset);
                    let snapshot = self.snapshot(0.0, true);
                    self.scroll_to_offset(saved);
                    let _ = reply.send(snapshot);
                }
                TerminalScreenCommand::RemoteViewportSnapshot {
                    display_offset,
                    overscan_rows,
                    max_lines,
                    reply,
                } => {
                    let saved = self.display_offset();
                    let snapshot =
                        self.remote_viewport_snapshot(display_offset, overscan_rows, max_lines);
                    self.scroll_to_offset(saved);
                    let _ = reply.send(snapshot);
                }
                TerminalScreenCommand::SelectionSpan {
                    line,
                    col,
                    kind,
                    reply,
                } => {
                    let _ = reply.send(self.selection_span(line, col, kind));
                }
                TerminalScreenCommand::Clear => {
                    let event_sink = self.event_sink.take();
                    self = Self::new(
                        self.cols,
                        self.rows,
                        self.scrollback,
                        self.responder.clone(),
                    );
                    self.event_sink = event_sink;
                }
                TerminalScreenCommand::ClearKeepPrompt => self.clear_keep_prompt(),
            }
        }
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if self.cols == cols && self.rows == rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.term.resize(HeadlessTermSize::new(cols, rows));
    }

    fn restore_visible_wrapped_rows(&mut self, wrapped_rows: &[bool]) {
        let cols = self.term.columns();
        let rows = self.term.screen_lines();
        if cols == 0 || rows == 0 {
            return;
        }
        let display_offset = self.term.grid().display_offset() as i32;
        let topmost = self.term.grid().topmost_line();
        let bottommost = self.term.grid().bottommost_line();
        let grid = self.term.grid_mut();
        for row in 0..rows {
            let line = Line(row as i32 - display_offset);
            if line < topmost || line > bottommost {
                continue;
            }
            grid[line][Column(cols - 1)].flags.set(
                Flags::WRAPLINE,
                wrapped_rows.get(row).copied().unwrap_or(false),
            );
        }
    }

    fn set_scrollback(&mut self, scrollback: usize) {
        if self.scrollback == scrollback {
            return;
        }
        self.scrollback = scrollback;
        self.config.scrolling_history = scrollback;
        self.term.set_options(self.config.clone());
    }

    fn scroll_lines(&mut self, lines: i32) {
        if lines == 0 {
            return;
        }
        // Positive `lines` scrolls up into history; `Scroll::Delta` uses the
        // same sign (it adds to the display offset).
        self.term.scroll_display(Scroll::Delta(lines));
    }

    fn scroll_to_offset(&mut self, offset: usize) {
        let delta = offset as i32 - self.display_offset() as i32;
        if delta != 0 {
            self.term.scroll_display(Scroll::Delta(delta));
        }
    }

    // iTerm/Zed-style clear: wipe scrollback and screen, then move the cursor
    // row to the top so the shell prompt stays visible. No-op on the alternate
    // screen (a full-screen TUI owns the whole grid).
    fn clear_keep_prompt(&mut self) {
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return;
        }
        let cursor = self.term.grid().cursor.point;
        let prompt_row: Vec<Cell> = (0..self.cols)
            .map(|col| self.term.grid()[cursor.line][Column(col)].clone())
            .collect();
        let grid = self.term.grid_mut();
        grid.clear_history();
        if cursor.line.0 > 0 {
            grid.reset_region(..cursor.line);
        }
        for (col, cell) in prompt_row.into_iter().enumerate() {
            grid[Line(0)][Column(col)] = cell;
        }
        grid.cursor.point = Point::new(Line(0), cursor.column);
        if self.rows > 1 {
            grid.reset_region(Line(1)..);
        }
        self.term.scroll_display(Scroll::Bottom);
    }

    fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    fn has_history_above_viewport(&self) -> bool {
        let grid = self.term.grid();
        let visible_top = -(grid.display_offset() as i32);
        visible_top > grid.topmost_line().0
    }

    fn remote_viewport_snapshot(
        &mut self,
        display_offset: usize,
        overscan_rows: usize,
        max_lines: usize,
    ) -> TerminalScreenSnapshot {
        let total = self.total_lines().max(self.rows);
        let visible_total = if max_lines == 0 {
            total
        } else {
            total.min(max_lines.max(self.rows))
        };
        let max_offset = visible_total.saturating_sub(self.rows);
        let display_offset = display_offset.min(max_offset);
        let above_offset = display_offset
            .saturating_add(self.rows)
            .saturating_add(overscan_rows)
            .min(max_offset);
        self.scroll_to_offset(above_offset);
        let above = self.snapshot(0.0, true);
        self.scroll_to_offset(display_offset);
        let viewport = self.snapshot(0.0, true);
        let below = if display_offset > 0 && overscan_rows > 0 {
            let below_offset = display_offset.saturating_sub(self.rows + overscan_rows);
            self.scroll_to_offset(below_offset);
            Some(self.snapshot(0.0, true))
        } else {
            None
        };
        let mut snapshot = stack_scrolled_snapshots(&above, &viewport, below.as_ref());
        snapshot.total_lines = visible_total;
        snapshot.display_offset = display_offset;
        snapshot
    }

    fn total_lines(&self) -> usize {
        self.term.grid().total_lines().max(self.rows)
    }

    // Word/line selection via alacritty's own semantic + line search (the
    // canonical implementation, driven by its semantic_escape_chars), so we
    // don't hand-roll word boundaries. Input/output are in absolute buffer
    // coordinates (line 0 = oldest); the end column is exclusive.
    fn selection_span(
        &self,
        line: i32,
        col: usize,
        kind: TerminalSelectionSpanKind,
    ) -> Option<TerminalSelectionSpan> {
        let grid = self.term.grid();
        let cols = grid.columns();
        if cols == 0 {
            return None;
        }
        let history = grid.history_size() as i32;
        let grid_line = Line(line - history);
        if grid_line < grid.topmost_line() || grid_line > grid.bottommost_line() {
            return None;
        }
        let point = Point::new(grid_line, Column(col.min(cols - 1)));
        // Nothing to select on a blank cell (e.g. a double-click past the text).
        if matches!(kind, TerminalSelectionSpanKind::Word)
            && matches!(grid[point.line][point.column].c, ' ' | '\0')
        {
            return None;
        }
        let (start, end) = match kind {
            TerminalSelectionSpanKind::Word => (
                self.term.semantic_search_left(point),
                self.term.semantic_search_right(point),
            ),
            TerminalSelectionSpanKind::Line => (
                self.term.line_search_left(point),
                self.term.line_search_right(point),
            ),
        };
        // Alacritty's end column is inclusive; widen past a trailing wide char.
        let end_width = if grid[end.line][end.column].flags.contains(Flags::WIDE_CHAR) {
            2
        } else {
            1
        };
        Some(TerminalSelectionSpan {
            start_line: start.line.0 + history,
            start_col: start.column.0,
            end_line: end.line.0 + history,
            end_col: end.column.0 + end_width,
        })
    }

    fn snapshot(&mut self, scroll_pixel_offset: f64, include_data: bool) -> TerminalScreenSnapshot {
        let cols = self.term.columns();
        let rows = self.term.screen_lines();
        let total_lines = self.term.grid().total_lines().max(rows);
        let display_offset = self.term.grid().display_offset();
        let input_mode = terminal_input_mode(&self.term);
        let application_cursor = self.term.mode().contains(TermMode::APP_CURSOR);

        // `display_iter` yields the visible viewport; map each term line to a
        // 0..rows viewport row by adding the display offset. Overscan context
        // for smooth remote scrolling is produced separately by
        // `remote_viewport_snapshot` (scroll + stack), not here.
        let content = self.term.renderable_content();
        let cursor_point = content.cursor.point;
        let cursor_shape = content.cursor.shape;
        let cursor_visible =
            content.mode.contains(TermMode::SHOW_CURSOR) && cursor_shape != CursorShape::Hidden;

        let mut cells = Vec::new();
        let mut wrapped_rows = vec![false; rows];
        for indexed in content.display_iter {
            let row = indexed.point.line.0 + display_offset as i32;
            if row < 0 || row as usize >= rows {
                continue;
            }
            let col = indexed.point.column.0;
            if col >= cols {
                continue;
            }
            let cell = indexed.cell;
            // WRAPLINE sits on the last cell of a soft-wrapped row; record it
            // before any spacer/blank skip so copy knows this row has no hard
            // line break (mirrors alacritty's own line_to_string).
            if cell.flags.contains(Flags::WRAPLINE)
                && let Some(slot) = wrapped_rows.get_mut(row as usize)
            {
                *slot = true;
            }
            // Wide-char spacers carry no glyph of their own; the leading cell
            // already reports width 2.
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let mut text = String::new();
            if cell.c != '\0' && !cell.c.is_control() {
                text.push(cell.c);
            }
            if let Some(zerowidth) = cell.zerowidth() {
                for ch in zerowidth {
                    if !ch.is_control() {
                        text.push(*ch);
                    }
                }
            }
            let fg = terminal_screen_color(cell.fg);
            let bg = terminal_screen_color(cell.bg);
            // Skip blank, default-styled cells so the consumer's own theme
            // background shows through (and the snapshot stays compact). An
            // unwritten alacritty cell is a space (`' '`), not `\0`, so trim
            // before testing -- otherwise trailing run-of-spaces leak into the
            // snapshot and into selection/URL reconstruction. Middle spaces are
            // rebuilt from cell-column gaps by consumers; styled/colored spaces
            // (non-default bg or visuals) are kept.
            if text.trim().is_empty()
                && bg == TerminalScreenColor::Default
                && !cell_has_visuals(cell)
            {
                continue;
            }
            cells.push(TerminalScreenCellSnapshot {
                row,
                col,
                text,
                width: if cell.flags.contains(Flags::WIDE_CHAR) {
                    2
                } else {
                    1
                },
                fg,
                bg,
                bold: cell.flags.contains(Flags::BOLD),
                dim: cell.flags.contains(Flags::DIM),
                italic: cell.flags.contains(Flags::ITALIC),
                underline: terminal_screen_underline(cell.flags),
                underline_color: cell
                    .underline_color()
                    .map(terminal_screen_color)
                    .filter(|color| *color != TerminalScreenColor::Default),
                link: cell.hyperlink().map(|link| link.uri().to_string()),
                inverse: cell.flags.contains(Flags::INVERSE),
                hidden: cell.flags.contains(Flags::HIDDEN),
                strikeout: cell.flags.contains(Flags::STRIKEOUT),
            });
        }

        // Hide the cursor when its line has scrolled out of the viewport.
        let cursor_row = cursor_point.line.0 + display_offset as i32;
        let mut cursor = TerminalScreenCursorSnapshot {
            row: 0,
            col: 0,
            visible: false,
            shape: terminal_screen_cursor_shape(cursor_shape),
        };
        if cursor_row >= 0 && (cursor_row as usize) < rows {
            cursor.row = cursor_row as usize;
            cursor.col = cursor_point.column.0.min(cols.saturating_sub(1));
            cursor.visible = cursor_visible;
        }

        let data = if include_data {
            // Prefix the painted cells with the active DEC modes so a fresh
            // viewer that replays this keyframe restores the *state*, not just
            // the glyphs: an alt-screen TUI must re-enter the alternate buffer
            // before the repaint paints into it, and a mouse-tracking app needs
            // its tracking mode back so wheel scrolling is forwarded.
            let mut data = terminal_keyframe_mode_prefix(&input_mode);
            data.push_str(&terminal_snapshot_data(cols, rows, &cells, &cursor));
            data
        } else {
            String::new()
        };

        TerminalScreenSnapshot {
            data,
            cols,
            rows,
            total_lines,
            display_offset,
            margin_rows: 0,
            margin_rows_below: 0,
            scroll_pixel_offset,
            application_cursor,
            input_mode,
            title: self.title.clone(),
            wrapped_rows,
            prompt_marks: self.retained_prompt_marks(total_lines),
            images: self.viewport_images(total_lines, rows, display_offset),
            cells,
            cursor,
        }
    }

    fn viewport_images(
        &mut self,
        total_lines: usize,
        rows: usize,
        display_offset: usize,
    ) -> Vec<TerminalScreenImage> {
        let alt_active = self.term.mode().contains(TermMode::ALT_SCREEN);
        // Alt-screen images die with the alt screen; primary images that
        // scrolled out of the retained buffer are gone for good.
        self.images
            .retain(|image| image.line < total_lines && (!image.alt_screen || alt_active));
        let viewport_top = (total_lines - rows.min(total_lines)) as i64 - display_offset as i64;
        self.images
            .iter()
            .filter(|image| image.alt_screen == alt_active)
            .filter_map(|image| {
                let row = image.line as i64 - viewport_top;
                (row + image.rows as i64 > 0 && row < rows as i64).then(|| TerminalScreenImage {
                    id: image.id,
                    row: row as i32,
                    col: image.col,
                    rows: image.rows,
                    cols: image.cols,
                    data: image.data.clone(),
                })
            })
            .collect()
    }
}

/// DEC-mode prefix for a keyframe so a viewer that replays it restores the
/// active modes alongside the painted cells. Emitted before the repaint: the
/// alt-screen enter must precede the paint so it lands in the alternate buffer,
/// and the mouse / cursor-key modes make the viewer forward wheel and arrow
/// input instead of scrolling its local history.
fn terminal_keyframe_mode_prefix(mode: &TerminalInputMode) -> String {
    let mut out = if mode.alternate_screen {
        "\x1b[?1049h".to_string()
    } else {
        "\x1b[?1049l".to_string()
    };
    if mode.application_cursor {
        out.push_str("\x1b[?1h");
    }
    if mode.bracketed_paste {
        out.push_str("\x1b[?2004h");
    }
    if mode.focus_in_out {
        out.push_str("\x1b[?1004h");
    }
    // Pick the highest-granularity mouse tracking that is active; the coarser
    // modes are subsumed by it on the receiving terminal.
    if mode.mouse_motion {
        out.push_str("\x1b[?1003h");
    } else if mode.mouse_drag {
        out.push_str("\x1b[?1002h");
    } else if mode.mouse_tracking {
        out.push_str("\x1b[?1000h");
    }
    if mode.utf8_mouse {
        out.push_str("\x1b[?1005h");
    }
    if mode.sgr_mouse {
        out.push_str("\x1b[?1006h");
    }
    if mode.alternate_scroll {
        out.push_str("\x1b[?1007h");
    }
    // Kitty keyboard flags (CSI = flags ; 1 u sets them outright).
    if mode.kitty_flags != 0 {
        out.push_str(&format!("\x1b[={};1u", mode.kitty_flags));
    }
    out
}

fn terminal_input_mode(term: &Term<HeadlessEventProxy>) -> TerminalInputMode {
    let mode = term.mode();
    let mut kitty_flags = 0u8;
    for (flag, bit) in [
        (TermMode::DISAMBIGUATE_ESC_CODES, 1u8),
        (TermMode::REPORT_EVENT_TYPES, 2),
        (TermMode::REPORT_ALTERNATE_KEYS, 4),
        (TermMode::REPORT_ALL_KEYS_AS_ESC, 8),
        (TermMode::REPORT_ASSOCIATED_TEXT, 16),
    ] {
        if mode.contains(flag) {
            kitty_flags |= bit;
        }
    }
    TerminalInputMode {
        application_cursor: mode.contains(TermMode::APP_CURSOR),
        alternate_screen: mode.contains(TermMode::ALT_SCREEN),
        alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
        bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        focus_in_out: mode.contains(TermMode::FOCUS_IN_OUT),
        mouse_tracking: mode.intersects(TermMode::MOUSE_MODE),
        mouse_motion: mode.contains(TermMode::MOUSE_MOTION),
        mouse_drag: mode.contains(TermMode::MOUSE_DRAG),
        sgr_mouse: mode.contains(TermMode::SGR_MOUSE),
        utf8_mouse: mode.contains(TermMode::UTF8_MOUSE),
        kitty_flags,
    }
}

/// Whether a blank cell still carries visible styling, so it must be retained
/// in the snapshot even with no glyph and a default background.
fn cell_has_visuals(cell: &Cell) -> bool {
    cell.flags.intersects(
        Flags::BOLD
            | Flags::ITALIC
            | Flags::DIM
            | Flags::INVERSE
            | Flags::HIDDEN
            | Flags::STRIKEOUT
            | Flags::ALL_UNDERLINES,
    ) || terminal_screen_color(cell.fg) != TerminalScreenColor::Default
}

/// Map an alacritty cell color to the engine-neutral snapshot color. Named ANSI
/// colors (0–15) become palette indices so the consumer's theme resolves them;
/// the semantic foreground/background/cursor names collapse to `Default`.
fn terminal_screen_color(color: Color) -> TerminalScreenColor {
    match color {
        Color::Named(named) => match named {
            NamedColor::Foreground
            | NamedColor::Background
            | NamedColor::Cursor
            | NamedColor::DimForeground
            | NamedColor::BrightForeground => TerminalScreenColor::Default,
            other => {
                let index = other as usize;
                if index < 16 {
                    TerminalScreenColor::Indexed { index: index as u8 }
                } else {
                    TerminalScreenColor::Default
                }
            }
        },
        Color::Spec(rgb) => TerminalScreenColor::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
        Color::Indexed(index) => TerminalScreenColor::Indexed { index },
    }
}

/// Map an image's pixel size plus optional iTerm2 width/height args ("N"
/// cells, "Npx", "N%", "auto") to a cell-grid span. Sizing uses the nominal
/// cell (real metrics live in the renderer, which aspect-fits inside the
/// reserved box).
fn terminal_image_cell_span(
    px_width: usize,
    px_height: usize,
    width_arg: Option<&str>,
    height_arg: Option<&str>,
    grid_cols: usize,
    grid_rows: usize,
) -> (usize, usize) {
    const CELL_W: f64 = TERMINAL_CELL_WIDTH_PX as f64;
    const CELL_H: f64 = TERMINAL_CELL_HEIGHT_PX as f64;
    const MAX_ROWS: usize = 500;
    let px_width = px_width.max(1) as f64;
    let px_height = px_height.max(1) as f64;

    let parse = |arg: Option<&str>, grid: usize, cell: f64| -> Option<f64> {
        let arg = arg?;
        if arg == "auto" {
            return None;
        }
        if let Some(percent) = arg.strip_suffix('%') {
            return Some(grid as f64 * percent.parse::<f64>().ok()? / 100.0);
        }
        if let Some(pixels) = arg.strip_suffix("px") {
            return Some(pixels.parse::<f64>().ok()? / cell);
        }
        arg.parse::<f64>().ok()
    };

    let explicit_cols = parse(width_arg, grid_cols, CELL_W);
    let explicit_rows = parse(height_arg, grid_rows, CELL_H);
    let (cols, rows) = match (explicit_cols, explicit_rows) {
        (Some(cols), Some(rows)) => (cols, rows),
        (Some(cols), None) => (cols, px_height * (cols * CELL_W / px_width) / CELL_H),
        (None, Some(rows)) => (px_width * (rows * CELL_H / px_height) / CELL_W, rows),
        (None, None) => {
            let mut cols = px_width / CELL_W;
            let mut rows = px_height / CELL_H;
            if cols > grid_cols as f64 {
                rows *= grid_cols as f64 / cols;
                cols = grid_cols as f64;
            }
            (cols, rows)
        }
    };
    (
        (cols.ceil() as usize).clamp(1, grid_cols.max(1)),
        (rows.ceil() as usize).clamp(1, MAX_ROWS),
    )
}

fn terminal_screen_underline(flags: Flags) -> TerminalScreenUnderline {
    if flags.contains(Flags::UNDERCURL) {
        TerminalScreenUnderline::Curly
    } else if flags.contains(Flags::DOUBLE_UNDERLINE) {
        TerminalScreenUnderline::Double
    } else if flags.contains(Flags::DOTTED_UNDERLINE) {
        TerminalScreenUnderline::Dotted
    } else if flags.contains(Flags::DASHED_UNDERLINE) {
        TerminalScreenUnderline::Dashed
    } else if flags.contains(Flags::UNDERLINE) {
        TerminalScreenUnderline::Single
    } else {
        TerminalScreenUnderline::None
    }
}

fn terminal_screen_cursor_shape(shape: CursorShape) -> TerminalScreenCursorShape {
    match shape {
        CursorShape::Block => TerminalScreenCursorShape::Block,
        CursorShape::Beam => TerminalScreenCursorShape::Beam,
        CursorShape::Underline => TerminalScreenCursorShape::Underline,
        CursorShape::HollowBlock => TerminalScreenCursorShape::HollowBlock,
        CursorShape::Hidden => TerminalScreenCursorShape::Block,
    }
}

#[derive(Clone, PartialEq, Eq)]
struct SnapshotCellStyle {
    fg: TerminalScreenColor,
    bg: TerminalScreenColor,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: TerminalScreenUnderline,
    underline_color: Option<TerminalScreenColor>,
    link: Option<String>,
    inverse: bool,
    hidden: bool,
    strikeout: bool,
}

impl Default for SnapshotCellStyle {
    fn default() -> Self {
        Self {
            fg: TerminalScreenColor::Default,
            bg: TerminalScreenColor::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: TerminalScreenUnderline::None,
            underline_color: None,
            link: None,
            inverse: false,
            hidden: false,
            strikeout: false,
        }
    }
}

impl From<&TerminalScreenCellSnapshot> for SnapshotCellStyle {
    fn from(cell: &TerminalScreenCellSnapshot) -> Self {
        Self {
            fg: cell.fg.clone(),
            bg: cell.bg.clone(),
            bold: cell.bold,
            dim: cell.dim,
            italic: cell.italic,
            underline: cell.underline,
            underline_color: cell.underline_color.clone(),
            link: cell.link.clone(),
            inverse: cell.inverse,
            hidden: cell.hidden,
            strikeout: cell.strikeout,
        }
    }
}

#[derive(Clone)]
struct SnapshotScreenCell {
    text: String,
    width: usize,
    style: SnapshotCellStyle,
}

fn terminal_snapshot_data(
    cols: usize,
    rows: usize,
    cells: &[TerminalScreenCellSnapshot],
    cursor: &TerminalScreenCursorSnapshot,
) -> String {
    let mut rows_cells = vec![vec![None; cols]; rows];
    for cell in cells {
        if cell.row < 0 || cell.row as usize >= rows || cell.col >= cols {
            continue;
        }
        rows_cells[cell.row as usize][cell.col] = Some(SnapshotScreenCell {
            text: cell.text.clone(),
            width: cell.width,
            style: SnapshotCellStyle::from(cell),
        });
    }

    let mut output = String::new();
    output.push_str("\x1b[?25l\x1b[0m\x1b[H\x1b[2J");
    let mut current_style = SnapshotCellStyle::default();
    for (row_index, row_cells) in rows_cells.iter().enumerate() {
        let Some(last_col) = row_cells.iter().rposition(|cell| {
            cell.as_ref()
                .map(|cell| {
                    !cell.text.trim().is_empty() || cell.style != SnapshotCellStyle::default()
                })
                .unwrap_or(false)
        }) else {
            continue;
        };
        output.push_str(&format!("\x1b[{};1H", row_index + 1));
        let mut col = 0;
        while col <= last_col {
            match &row_cells[col] {
                Some(cell) => {
                    if cell.style != current_style {
                        if cell.style.link != current_style.link {
                            output.push_str(&snapshot_link_osc(cell.style.link.as_deref()));
                        }
                        output.push_str(&snapshot_style_sgr(cell.style.clone()));
                        current_style = cell.style.clone();
                    }
                    if cell.text.is_empty() {
                        // Background-only cells (BCE-erased panel bands)
                        // must still advance the cursor and paint their
                        // background on the receiving screen.
                        for _ in 0..cell.width.max(1) {
                            output.push(' ');
                        }
                    } else {
                        output.push_str(&terminal_snapshot_text(&cell.text));
                    }
                    col += cell.width;
                }
                None => {
                    // Gap cells have no recorded style; reset to default so
                    // the space does not paint a lingering band background.
                    if current_style != SnapshotCellStyle::default() {
                        if current_style.link.is_some() {
                            output.push_str(&snapshot_link_osc(None));
                        }
                        output.push_str("\x1b[0m");
                        current_style = SnapshotCellStyle::default();
                    }
                    output.push(' ');
                    col += 1;
                }
            }
        }
    }
    if current_style != SnapshotCellStyle::default() {
        if current_style.link.is_some() {
            output.push_str(&snapshot_link_osc(None));
        }
        output.push_str("\x1b[0m");
    }
    if cursor.visible {
        output.push_str(&format!("\x1b[{};{}H", cursor.row + 1, cursor.col + 1));
        output.push_str("\x1b[?25h");
    }
    output
}

/// OSC 8 open/close for keyframe re-encoding; alacritty regenerates the link
/// id, so only the URI needs carrying.
fn snapshot_link_osc(uri: Option<&str>) -> String {
    format!("\x1b]8;;{}\x1b\\", uri.unwrap_or(""))
}

fn terminal_snapshot_text(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

fn snapshot_style_sgr(style: SnapshotCellStyle) -> String {
    let mut codes = vec!["0".to_string()];
    if style.bold {
        codes.push("1".to_string());
    }
    if style.dim {
        codes.push("2".to_string());
    }
    if style.italic {
        codes.push("3".to_string());
    }
    match style.underline {
        TerminalScreenUnderline::None => {}
        TerminalScreenUnderline::Single => codes.push("4".to_string()),
        TerminalScreenUnderline::Double => codes.push("4:2".to_string()),
        TerminalScreenUnderline::Curly => codes.push("4:3".to_string()),
        TerminalScreenUnderline::Dotted => codes.push("4:4".to_string()),
        TerminalScreenUnderline::Dashed => codes.push("4:5".to_string()),
    }
    if style.inverse {
        codes.push("7".to_string());
    }
    if style.hidden {
        codes.push("8".to_string());
    }
    if style.strikeout {
        codes.push("9".to_string());
    }
    snapshot_color_sgr(&style.fg, false, &mut codes);
    snapshot_color_sgr(&style.bg, true, &mut codes);
    // SGR 58: underline color override (colors reset to default via the
    // leading 0, so it only needs emitting when set).
    match &style.underline_color {
        Some(TerminalScreenColor::Rgb { r, g, b }) => {
            codes.push("58".to_string());
            codes.push("2".to_string());
            codes.push(r.to_string());
            codes.push(g.to_string());
            codes.push(b.to_string());
        }
        Some(TerminalScreenColor::Indexed { index }) => {
            codes.push("58".to_string());
            codes.push("5".to_string());
            codes.push(index.to_string());
        }
        _ => {}
    }
    format!("\x1b[{}m", codes.join(";"))
}

fn snapshot_color_sgr(color: &TerminalScreenColor, background: bool, codes: &mut Vec<String>) {
    match color {
        TerminalScreenColor::Default | TerminalScreenColor::Named { .. } => {
            codes.push(if background { "49" } else { "39" }.to_string());
        }
        TerminalScreenColor::Rgb { r, g, b } => {
            codes.push(if background { "48" } else { "38" }.to_string());
            codes.push("2".to_string());
            codes.push(r.to_string());
            codes.push(g.to_string());
            codes.push(b.to_string());
        }
        TerminalScreenColor::Indexed { index } => {
            codes.push(if background { "48" } else { "38" }.to_string());
            codes.push("5".to_string());
            codes.push(index.to_string());
        }
    }
}

/// Stack overscan snapshots (content above and below the viewport) around
/// the viewport snapshot, producing one taller snapshot whose top
/// `margin_rows` rows and bottom `margin_rows_below` rows are pre-rendered
/// context for smooth remote scrolling. The below-peek, when present, is a
/// snapshot taken at `viewport.display_offset.saturating_sub(viewport.rows)`.
pub fn stack_scrolled_snapshots(
    above: &TerminalScreenSnapshot,
    viewport: &TerminalScreenSnapshot,
    below: Option<&TerminalScreenSnapshot>,
) -> TerminalScreenSnapshot {
    // Rows of `above` that sit strictly above the viewport top. When the
    // above-peek was clamped at the top of history, only the non-overlapping
    // prefix is context.
    let margin = above
        .display_offset
        .saturating_sub(viewport.display_offset)
        .min(above.rows);
    // Rows of `below` that sit strictly below the viewport bottom; 0 when
    // the viewport is already at the live bottom.
    let margin_below = below
        .map(|below| {
            viewport
                .display_offset
                .saturating_sub(below.display_offset)
                .min(below.rows)
        })
        .unwrap_or(0);
    if margin == 0 && margin_below == 0 {
        return viewport.clone();
    }
    let rows = margin + viewport.rows + margin_below;
    let mut cells = Vec::with_capacity(
        above.cells.len() + viewport.cells.len() + below.map_or(0, |below| below.cells.len()),
    );
    let above_skip = above.rows - margin;
    for cell in &above.cells {
        let row = cell.row - above_skip as i32;
        if row < 0 {
            continue;
        }
        let mut cell = cell.clone();
        cell.row = row;
        cells.push(cell);
    }
    for cell in &viewport.cells {
        let mut cell = cell.clone();
        cell.row += margin as i32;
        cells.push(cell);
    }
    if margin_below > 0 {
        // The unique below rows are the below-peek's last `margin_below`
        // rows; everything before them overlaps the viewport.
        let below = below.expect("margin_below > 0 implies below snapshot");
        let below_skip = below.rows - margin_below;
        let base = (margin + viewport.rows) as i32;
        for cell in &below.cells {
            let row = cell.row - below_skip as i32;
            if row < 0 {
                continue;
            }
            let mut cell = cell.clone();
            cell.row = base + row;
            cells.push(cell);
        }
    }
    let mut cursor = viewport.cursor.clone();
    cursor.row += margin;
    let mut data = terminal_keyframe_mode_prefix(&viewport.input_mode);
    data.push_str(&terminal_snapshot_data(
        viewport.cols,
        rows,
        &cells,
        &cursor,
    ));
    let wrapped_rows = if above.wrapped_rows.len() >= above.rows
        && viewport.wrapped_rows.len() >= viewport.rows
        && below.is_none_or(|below| below.wrapped_rows.len() >= below.rows)
    {
        let mut wrapped_rows = above.wrapped_rows[above_skip..above.rows].to_vec();
        wrapped_rows.extend_from_slice(&viewport.wrapped_rows[..viewport.rows]);
        if margin_below > 0 {
            let below = below.expect("margin_below > 0 implies below snapshot");
            wrapped_rows.extend_from_slice(&below.wrapped_rows[below.rows - margin_below..]);
        }
        wrapped_rows
    } else {
        Vec::new()
    };
    TerminalScreenSnapshot {
        data,
        cols: viewport.cols,
        rows,
        total_lines: viewport.total_lines,
        display_offset: viewport.display_offset,
        margin_rows: margin,
        margin_rows_below: margin_below,
        scroll_pixel_offset: viewport.scroll_pixel_offset,
        application_cursor: viewport.application_cursor,
        input_mode: viewport.input_mode,
        title: viewport.title.clone(),
        wrapped_rows,
        prompt_marks: viewport.prompt_marks.clone(),
        images: Vec::new(),
        cells,
        cursor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraws_current_screen_after_clear_and_cursor_moves() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"old line\n\x1b[2J\x1b[Htop\x1b[3;5Hbottom");

        let snapshot = screen.snapshot();

        assert_eq!(snapshot.cols, 20);
        assert_eq!(snapshot.rows, 4);
        assert!(snapshot.data.contains("top"));
        assert!(snapshot.data.contains("bottom"));
        assert!(!snapshot.data.contains("old line"));
        assert!(snapshot.cells.iter().any(|cell| cell.text == "t"));
    }

    #[test]
    fn captures_osc_title_into_snapshot() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        assert_eq!(screen.snapshot().title, None);

        screen.process(b"\x1b]2;dartvm\x07hello");
        assert_eq!(screen.snapshot().title.as_deref(), Some("dartvm"));

        // Replayed history applies titles too (restore path).
        screen.process_replay(b"\x1b]0;zsh\x07");
        assert_eq!(screen.snapshot().title.as_deref(), Some("zsh"));
    }

    #[test]
    fn extended_underlines_carry_style_and_color_through_keyframe() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"\x1b[4:3m\x1b[58;2;255;0;10mcurly\x1b[0m \x1b[4:2mdd\x1b[0m \x1b[4:5;58;5;9mda\x1b[0m");
        let snap = screen.snapshot();

        let cell = |ch: &str| {
            snap.cells
                .iter()
                .find(|cell| cell.text == ch)
                .unwrap_or_else(|| panic!("cell {ch:?} missing"))
        };
        assert_eq!(cell("c").underline, TerminalScreenUnderline::Curly);
        assert_eq!(
            cell("c").underline_color,
            Some(TerminalScreenColor::Rgb {
                r: 255,
                g: 0,
                b: 10
            })
        );
        assert_eq!(cell("d").underline, TerminalScreenUnderline::Double);
        assert_eq!(cell("d").underline_color, None);
        assert_eq!(cell("a").underline, TerminalScreenUnderline::Dashed);
        assert_eq!(
            cell("a").underline_color,
            Some(TerminalScreenColor::Indexed { index: 9 })
        );

        // The keyframe re-encodes 4:x / 58 so a replaying viewer restores them.
        let mut dst = HeadlessTerminalScreen::new(20, 4, 100);
        dst.process_replay(snap.data.as_bytes());
        let dst_snap = dst.snapshot();
        let dst_cell = dst_snap
            .cells
            .iter()
            .find(|cell| cell.text == "c")
            .expect("replayed cell");
        assert_eq!(dst_cell.underline, TerminalScreenUnderline::Curly);
        assert_eq!(
            dst_cell.underline_color,
            Some(TerminalScreenColor::Rgb {
                r: 255,
                g: 0,
                b: 10
            })
        );
    }

    #[test]
    fn osc8_hyperlinks_attach_to_cells_and_survive_keyframe() {
        let mut screen = HeadlessTerminalScreen::new(40, 4, 100);
        screen.process(b"\x1b]8;;https://example.com\x1b\\docs\x1b]8;;\x1b\\ plain");
        let snap = screen.snapshot();
        let linked = snap.cells.iter().find(|cell| cell.text == "d").unwrap();
        assert_eq!(linked.link.as_deref(), Some("https://example.com"));
        let plain = snap.cells.iter().find(|cell| cell.text == "p").unwrap();
        assert_eq!(plain.link, None);

        let mut dst = HeadlessTerminalScreen::new(40, 4, 100);
        dst.process_replay(snap.data.as_bytes());
        let dst_snap = dst.snapshot();
        let linked = dst_snap.cells.iter().find(|cell| cell.text == "d").unwrap();
        assert_eq!(linked.link.as_deref(), Some("https://example.com"));
        let plain = dst_snap.cells.iter().find(|cell| cell.text == "p").unwrap();
        assert_eq!(plain.link, None);
    }

    #[test]
    fn engine_events_reach_sink_only_for_live_output() {
        let stored = Arc::new(std::sync::Mutex::new(Vec::<TerminalScreenEvent>::new()));
        let sink_stored = stored.clone();
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.set_event_sink(Arc::new(move |event: TerminalScreenEvent| {
            sink_stored.lock().unwrap().push(event);
        }));
        screen.process(b"\x1b]52;c;aGVsbG8=\x07"); // OSC 52 "hello"
        screen.process(b"ding\x07"); // BEL
        screen.process_replay(b"\x1b]52;c;aWdub3Jl\x07\x07"); // replayed history: ignored
        let _ = screen.snapshot(); // worker barrier
        assert_eq!(
            *stored.lock().unwrap(),
            vec![
                TerminalScreenEvent::ClipboardStore("hello".to_string()),
                TerminalScreenEvent::Bell,
            ]
        );
    }

    #[test]
    fn kitty_keyboard_flags_surface_in_input_mode_and_keyframe() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        assert_eq!(screen.snapshot().input_mode.kitty_flags, 0);

        screen.process(b"\x1b[>1u"); // push disambiguate
        let snap = screen.snapshot();
        assert_eq!(snap.input_mode.kitty_flags, 1);
        assert!(
            snap.data.contains("\x1b[=1;1u"),
            "keyframe should restore kitty flags, got: {:?}",
            &snap.data[..snap.data.len().min(80)]
        );

        screen.process(b"\x1b[<u"); // pop
        assert_eq!(screen.snapshot().input_mode.kitty_flags, 0);
    }

    #[test]
    fn osc133_prompt_marks_record_absolute_lines_across_chunks() {
        let mut screen = HeadlessTerminalScreen::new(20, 6, 100);
        screen.process(b"\x1b]133;A\x07$ first\r\nout\r\n");
        // Marker split across process calls must still be detected once.
        screen.process(b"\x1b]13");
        screen.process(b"3;A\x07$ second\r\n");
        let snap = screen.snapshot();
        assert_eq!(snap.prompt_marks, vec![0, 2]);

        // Scrolled history keeps marks in absolute coordinates.
        for _ in 0..10 {
            screen.process(b"line\r\n");
        }
        screen.process(b"\x1b]133;A\x07$ third");
        let snap = screen.snapshot();
        assert_eq!(snap.prompt_marks, vec![0, 2, 13]);
        assert!(snap.data.contains("third"));
    }

    #[test]
    fn osc1337_inline_images_anchor_to_grid_and_reserve_rows() {
        // 1x1 transparent PNG.
        const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let mut screen = HeadlessTerminalScreen::new(20, 6, 100);
        screen.process(b"before\r\n");
        let sequence = format!("\x1b]1337;File=name=dGVzdA==;inline=1:{PNG_BASE64}\x07");
        // Split the sequence across feeds to exercise the capture carry.
        let (head, tail) = sequence.as_bytes().split_at(30);
        screen.process(head);
        screen.process(tail);
        screen.process(b"after");

        let snap = screen.snapshot();
        assert_eq!(snap.images.len(), 1);
        let image = &snap.images[0];
        assert_eq!(image.row, 1, "image anchors at the emitting cursor row");
        assert_eq!((image.cols, image.rows), (1, 1), "1x1 px fits one cell");
        assert!(!image.data.is_empty());
        // The reserved row moved the cursor below the image.
        let after = snap.cells.iter().find(|cell| cell.text == "a");
        assert_eq!(after.map(|cell| cell.row), Some(2));
        // The base64 payload never reached the grid.
        assert!(!snap.data.contains("iVBOR"));
    }

    #[test]
    fn alt_screen_keyframe_reconstructs_into_fresh_screen() {
        // The desktop re-attach path (subscribe_output) replays a session's
        // screen keyframe to rebuild an alt-screen TUI. Verify the alacritty
        // keyframe round-trips: enter alt, paint, snapshot -> replay into a
        // fresh screen -> alt mode + content restored.
        let mut src = HeadlessTerminalScreen::new(20, 6, 100);
        src.process(b"\x1b[?1049h\x1b[H\x1b[2JCONVERSATION\r\nmore lines\x1b[6;1H> input box");
        let src_snap = src.snapshot();
        assert!(
            src_snap.input_mode.alternate_screen,
            "source should be in alt screen"
        );
        assert!(
            src_snap.data.contains("CONVERSATION"),
            "source keyframe should hold content"
        );

        let mut dst = HeadlessTerminalScreen::new(20, 6, 100);
        dst.process_replay(src_snap.data.as_bytes());
        let dst_snap = dst.snapshot();
        assert!(
            dst_snap.input_mode.alternate_screen,
            "reconstructed screen should be in alt screen"
        );
        assert!(
            dst_snap.data.contains("CONVERSATION"),
            "alt-screen content should survive keyframe reconstruction, got: {:?}",
            dst_snap.data
        );
        assert!(dst_snap.data.contains("input box"));
    }

    #[test]
    fn alt_screen_content_survives_resize_roundtrip() {
        // Mobile claims (small grid) then desktop reclaims (large grid). The
        // alt-screen content must still be in the snapshot after the round-trip
        // (alacritty does not reflow the alt screen, but must not drop it).
        let mut screen = HeadlessTerminalScreen::new(40, 10, 100);
        screen.process(b"\x1b[?1049h\x1b[H\x1b[2J");
        screen.process(b"alpha\r\nbravo\r\ncharlie\x1b[10;1H> prompt");
        screen.resize(24, 20); // mobile-ish (narrow/tall)
        screen.resize(80, 10); // desktop reclaim
        let snap = screen.snapshot();
        assert!(snap.input_mode.alternate_screen);
        assert!(
            snap.data.contains("prompt"),
            "prompt line should survive resize round-trip, got: {:?}",
            snap.data
        );
    }

    #[test]
    fn resize_bursts_settle_on_final_dimensions() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"ready");
        for cols in [30usize, 40, 50, 60, 25] {
            screen.resize(cols, 10);
        }

        let snapshot = screen.snapshot();

        assert_eq!(snapshot.cols, 25);
        assert_eq!(snapshot.rows, 10);
        assert!(snapshot.data.contains("ready"));
    }

    #[test]
    fn keeps_resize_state() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.resize(30, 10);
        screen.process(b"ready");

        let snapshot = screen.snapshot();

        assert_eq!(snapshot.cols, 30);
        assert_eq!(snapshot.rows, 10);
        assert!(snapshot.data.contains("ready"));
    }

    #[test]
    fn preserves_wide_text_without_split_cells() {
        let mut screen = HeadlessTerminalScreen::new(40, 4, 100);
        screen.process("第 2003行 测 试 文 本".as_bytes());

        let snapshot = screen.snapshot();

        assert!(snapshot.data.contains("第 2003行 测 试 文 本"));
        assert!(
            snapshot
                .cells
                .iter()
                .any(|cell| cell.text == "第" && cell.width == 2)
        );
    }

    #[test]
    fn plain_cells_keep_default_colors_for_app_theme_resolution() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"theme");

        let snapshot = screen.snapshot();
        let cell = snapshot
            .cells
            .iter()
            .find(|cell| cell.text == "t")
            .expect("plain cell");

        assert_eq!(cell.fg, TerminalScreenColor::Default);
        assert_eq!(cell.bg, TerminalScreenColor::Default);
    }

    #[test]
    fn sgr_colors_remain_semantic_until_ui_palette_resolution() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"\x1b[31mred\x1b[0m \x1b[48;5;4mblue-bg");

        let snapshot = screen.snapshot();
        let red = snapshot
            .cells
            .iter()
            .find(|cell| cell.text == "r")
            .expect("red cell");
        let blue_bg = snapshot
            .cells
            .iter()
            .find(|cell| cell.text == "b")
            .expect("blue bg cell");

        assert_eq!(red.fg, TerminalScreenColor::Indexed { index: 1 });
        assert_eq!(red.bg, TerminalScreenColor::Default);
        assert_eq!(blue_bg.bg, TerminalScreenColor::Indexed { index: 4 });
    }

    #[test]
    fn scrolls_viewport_through_history() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix");
        assert_eq!(screen.snapshot().display_offset, 0);

        screen.scroll_lines(2);
        let scrolled = screen.snapshot();
        assert!(scrolled.display_offset > 0);
        assert!(scrolled.data.contains("two") || scrolled.data.contains("three"));
        assert!(scrolled.total_lines >= scrolled.rows + scrolled.display_offset);
        assert!(
            scrolled
                .cells
                .iter()
                .any(|cell| cell.row == 0 && !cell.text.trim().is_empty())
        );
        assert!(scrolled.cells.iter().all(|cell| cell.row >= 0));
        assert!(
            scrolled
                .cells
                .iter()
                .all(|cell| (cell.row as usize) < scrolled.rows)
        );

        screen.scroll_to_bottom();
        assert_eq!(screen.snapshot().display_offset, 0);
    }

    #[test]
    fn scroll_to_offset_targets_absolute_history_position() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight");

        screen.scroll_to_offset(3);
        assert_eq!(screen.snapshot().display_offset, 3);

        // Re-targeting the same offset is a no-op; a new target is exact
        // regardless of the current position.
        screen.scroll_to_offset(3);
        assert_eq!(screen.snapshot().display_offset, 3);
        screen.scroll_to_offset(1);
        assert_eq!(screen.snapshot().display_offset, 1);
        screen.scroll_to_offset(0);
        assert_eq!(screen.snapshot().display_offset, 0);
    }

    #[test]
    fn clear_keep_prompt_moves_cursor_row_to_top_and_drops_history() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nprompt$ ");

        screen.clear_keep_prompt();
        let snapshot = screen.snapshot();

        assert_eq!(snapshot.display_offset, 0);
        assert_eq!(snapshot.total_lines, 4, "scrollback must be dropped");
        assert_eq!(snapshot.cursor.row, 0);
        assert_eq!(snapshot.cursor.col, "prompt$ ".len());
        let top_row: String = snapshot
            .cells
            .iter()
            .filter(|cell| cell.row == 0)
            .map(|cell| cell.text.as_str())
            .collect();
        assert!(
            top_row.starts_with("prompt$"),
            "prompt row must survive at the top: {top_row:?}"
        );
        assert!(
            snapshot
                .cells
                .iter()
                .all(|cell| cell.row == 0 || cell.text.trim().is_empty()),
            "rows below the prompt must be blank"
        );
    }

    #[test]
    fn pty_responder_answers_cursor_position_and_mode_queries() {
        let replies = Arc::new(parking_lot_free_buffer::Buffer::default());
        let responder: TerminalPtyResponder = {
            let replies = replies.clone();
            Arc::new(move |bytes: &[u8]| replies.push(bytes))
        };
        let mut screen = HeadlessTerminalScreen::new_with_responder(20, 4, 100, Some(responder));

        // This exercises the real worker path: the Ghostty terminal installs
        // callbacks during construction, is then moved into the worker, and
        // later dispatches PTY replies from vt_write.
        // CPR (CSI 6n), DECRQM for bracketed paste, DA1 (CSI c).
        screen.process(b"hi\x1b[6n\x1b[?2004$p\x1b[c");
        // Synchronize on the worker queue before reading replies.
        let _ = screen.snapshot();

        let replies = replies.take();
        let text = String::from_utf8_lossy(&replies);
        assert!(text.contains("\x1b[1;3R"), "missing CPR reply: {text:?}");
        assert!(
            text.contains("\x1b[?2004;2$y"),
            "missing DECRQM reply: {text:?}"
        );
        assert!(text.contains("\x1b[?6c"), "missing DA1 reply: {text:?}");
    }

    #[test]
    fn replayed_history_does_not_answer_stale_queries() {
        let replies = Arc::new(parking_lot_free_buffer::Buffer::default());
        let responder: TerminalPtyResponder = {
            let replies = replies.clone();
            Arc::new(move |bytes: &[u8]| replies.push(bytes))
        };
        let mut screen = HeadlessTerminalScreen::new_with_responder(20, 4, 100, Some(responder));

        // Recorded output containing a stale CPR query must not reply...
        screen.process_replay(b"restored\x1b[6n");
        let _ = screen.snapshot();
        assert!(replies.take().is_empty());

        // ...while live queries afterwards still do.
        screen.process(b"\x1b[6n");
        let _ = screen.snapshot();
        assert!(!replies.take().is_empty());
    }

    #[test]
    fn live_input_mode_reflects_engine_state_immediately() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        assert!(!screen.input_mode().bracketed_paste);
        screen.process(b"\x1b[?2004h");
        assert!(screen.input_mode().bracketed_paste);
    }

    mod parking_lot_free_buffer {
        use std::sync::Mutex;

        #[derive(Default)]
        pub struct Buffer {
            bytes: Mutex<Vec<u8>>,
        }

        impl Buffer {
            pub fn push(&self, bytes: &[u8]) {
                self.bytes.lock().unwrap().extend_from_slice(bytes);
            }

            pub fn take(&self) -> Vec<u8> {
                std::mem::take(&mut self.bytes.lock().unwrap())
            }
        }
    }

    #[test]
    fn snapshot_request_preserves_command_order_without_blocking_caller() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"before");
        screen.process(b"\r\x1b[2Kafter");

        let request = screen.snapshot_request(true);
        screen.process(b"\r\x1b[2Klater");

        let requested = request.snapshot();
        let current = screen.snapshot();

        assert!(requested.data.contains("after"));
        assert!(!requested.data.contains("later"));
        assert!(current.data.contains("later"));
    }

    #[test]
    fn keyframe_replaces_previous_screen_and_scrollback() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"old one\r\nold two\r\nold three\r\nold four\r\nold five");

        screen.replace_with_keyframe(b"\x1b[2J\x1b[Hnew one\r\n\x1b[3;1Hnew input");

        let current = screen.snapshot();
        assert!(current.data.contains("new one"));
        assert!(current.data.contains("new input"));
        assert!(!current.data.contains("old one"));
        assert_eq!(current.display_offset, 0);

        screen.scroll_lines(8);
        let scrolled = screen.snapshot();
        assert_eq!(scrolled.display_offset, 0);
        assert!(!scrolled.data.contains("old one"));
    }

    #[test]
    fn visible_keyframe_does_not_add_rows_to_scrollback() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix");
        let before = screen.snapshot().total_lines;

        screen.replace_visible_with_keyframe(b"\x1b[?1049l\x1b[H\x1b[2Jcurrent");
        let after = screen.snapshot();

        assert_eq!(after.total_lines, before);
        assert!(after.data.contains("current"));
    }

    #[test]
    fn visible_keyframe_switches_between_primary_and_alternate_buffers() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"primary");

        screen.replace_visible_with_keyframe(b"\x1b[?1049h\x1b[H\x1b[2Jalternate");
        let alternate = screen.snapshot();
        assert!(alternate.input_mode.alternate_screen);
        assert!(alternate.data.contains("alternate"));

        screen.replace_visible_with_keyframe(b"\x1b[?1049l\x1b[H\x1b[2Jprimary fresh");
        let primary = screen.snapshot();
        assert!(!primary.input_mode.alternate_screen);
        assert!(primary.data.contains("primary fresh"));
        assert!(!primary.data.contains("alternate"));
    }

    #[test]
    fn hides_cursor_when_current_input_row_is_outside_viewport() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven");
        let bottom = screen.snapshot();
        assert!(bottom.cursor.visible);

        screen.scroll_lines(2);
        let scrolled = screen.snapshot();

        assert_eq!(scrolled.display_offset, 2);
        assert!(!scrolled.cursor.visible);
    }

    #[test]
    fn pixel_scroll_keeps_fractional_offset_without_synthetic_rows() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix");

        screen.scroll_pixels(7.0, 10.0);
        let partial = screen.snapshot();
        assert_eq!(partial.display_offset, 0);
        assert_eq!(partial.scroll_pixel_offset, 7.0);

        screen.scroll_pixels(6.0, 10.0);
        let scrolled = screen.snapshot();
        assert!(scrolled.display_offset > 0);
        assert_eq!(scrolled.scroll_pixel_offset, 3.0);
        assert!(
            scrolled
                .cells
                .iter()
                .all(|cell| cell.row >= 0 && (cell.row as usize) < scrolled.rows)
        );

        screen.settle_pixel_scroll();
        assert_eq!(screen.snapshot().scroll_pixel_offset, 3.0);
    }

    #[test]
    fn remote_viewport_snapshot_is_read_only_and_line_limited() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        screen.process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight");
        screen.scroll_lines(2);
        let before = screen.snapshot().display_offset;
        assert!(before > 0);

        let snapshot = screen.remote_viewport_snapshot_request(0, 2, 6).snapshot();

        assert_eq!(screen.snapshot().display_offset, before);
        assert_eq!(snapshot.total_lines, 6);
        assert_eq!(snapshot.display_offset, 0);
        assert!(snapshot.rows >= 4);
        assert!(snapshot.margin_rows > 0);
    }

    #[test]
    fn remote_viewport_snapshot_preserves_wrapped_rows_across_overscan() {
        let mut screen = HeadlessTerminalScreen::new(5, 3, 100);
        screen.process(b"abcdef\r\none\r\ntwo\r\nthree\r\nfour");

        let snapshot = screen.remote_viewport_snapshot_request(0, 2, 0).snapshot();

        assert_eq!(snapshot.wrapped_rows.len(), snapshot.rows);
        assert!(snapshot.wrapped_rows.iter().any(|wrapped| *wrapped));
    }

    #[test]
    fn set_scrollback_shrinks_history() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        for index in 0..20 {
            screen.process(format!("line-{index}\r\n").as_bytes());
        }
        assert!(
            screen
                .remote_viewport_snapshot_request(0, 0, 0)
                .snapshot()
                .total_lines
                > 8
        );

        screen.set_scrollback(2);

        let snapshot = screen.remote_viewport_snapshot_request(0, 0, 0).snapshot();
        assert_eq!(snapshot.total_lines, 6);
    }

    #[test]
    fn regenerated_snapshot_keeps_default_background_after_styled_band() {
        let mut screen = HeadlessTerminalScreen::new(20, 4, 100);
        // Row 0: a BCE-erased panel band (background-only cells) with wide
        // CJK text. Row 1: default-style wide text after a 3-column gap.
        screen.process("\x1b[H\x1b[48;5;17m\x1b[2K中文面板\x1b[0m\r\n\x1b[3C下一行".as_bytes());
        let snapshot = screen.snapshot();

        // Replay the snapshot data into a fresh screen, as the mobile remote
        // terminal does, and verify the band background does not leak into
        // gap cells on the following row.
        let mut regenerated = HeadlessTerminalScreen::new(20, 4, 100);
        regenerated.process(snapshot.data.as_bytes());
        let regenerated = regenerated.snapshot();

        assert!(
            regenerated
                .cells
                .iter()
                .any(|cell| cell.row == 0 && cell.bg == TerminalScreenColor::Indexed { index: 17 }),
            "band row should keep its background"
        );
        for cell in regenerated.cells.iter().filter(|cell| cell.row == 1) {
            assert_eq!(
                cell.bg,
                TerminalScreenColor::Default,
                "row 1 col {} unexpectedly inherited the band background",
                cell.col
            );
        }
    }

    #[test]
    fn keeps_requested_small_viewport_size() {
        let mut screen = HeadlessTerminalScreen::new(5, 3, 100);
        screen.process(b"small");

        let snapshot = screen.snapshot();

        assert_eq!(snapshot.cols, 5);
        assert_eq!(snapshot.rows, 3);
    }
}
