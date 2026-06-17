use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::thread;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config as AlacrittyConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor};
use serde::Serialize;

use crate::TerminalInputMode;

/// Receives reply bytes the VT engine writes back to the PTY in response
/// to queries (DSR/CPR, DECRQM, DA, kitty keyboard, XTVERSION, ...).
/// Invoked on the screen worker thread.
pub type TerminalPtyResponder = Arc<dyn Fn(&[u8]) + Send + Sync>;

const PROCESS_CHUNK_BYTES: usize = 64 * 1024;
const GHOSTTY_CELL_WIDTH_PX: u32 = 10;
const GHOSTTY_CELL_HEIGHT_PX: u32 = 20;

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
    pub underline: bool,
    pub inverse: bool,
    pub hidden: bool,
    pub strikeout: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TerminalScreenColor {
    Default,
    Named { name: String },
    Rgb { r: u8, g: u8, b: u8 },
    Indexed { index: u8 },
}

pub struct HeadlessTerminalScreen {
    engine: GhosttyTerminalScreenEngine,
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
            engine: GhosttyTerminalScreenEngine::new(cols, rows, scrollback, responder),
            pending_scroll_pixels: 0.0,
        }
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
                    .send(GhosttyScreenCommand::ProcessReplay(chunk.to_vec()));
            }
        }
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
            .send(GhosttyScreenCommand::SnapshotAtOffset { offset, reply: tx });
        HeadlessTerminalSnapshotRequest { rx }
    }

    pub fn display_offset(&self) -> usize {
        self.engine.display_offset()
    }

    pub fn clear(&mut self) {
        self.engine.clear();
        self.pending_scroll_pixels = 0.0;
    }

    pub fn snapshot(&self) -> TerminalScreenSnapshot {
        self.engine.snapshot(self.pending_scroll_pixels)
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
struct GhosttyTerminalScreenEngine {
    tx: mpsc::Sender<GhosttyScreenCommand>,
}

impl GhosttyTerminalScreenEngine {
    fn new(
        cols: usize,
        rows: usize,
        scrollback: usize,
        responder: Option<TerminalPtyResponder>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("codux-ghostty-screen".to_string())
            .spawn(move || {
                GhosttyScreenWorker::new(cols, rows, scrollback, responder).run(rx);
            })
            .expect("failed to spawn ghostty screen worker");
        Self { tx }
    }

    fn clear(&mut self) {
        self.send(GhosttyScreenCommand::Clear);
    }

    fn send(&self, command: GhosttyScreenCommand) {
        let _ = self.tx.send(command);
    }

    fn request<R: Default>(
        &self,
        build: impl FnOnce(mpsc::Sender<R>) -> GhosttyScreenCommand,
    ) -> R {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(build(tx)).is_err() {
            return R::default();
        }
        rx.recv().unwrap_or_default()
    }

    fn process(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.send(GhosttyScreenCommand::Process(bytes.to_vec()));
        }
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        self.send(GhosttyScreenCommand::Resize { cols, rows });
    }

    fn scroll_lines(&mut self, lines: i32) {
        if lines != 0 {
            self.send(GhosttyScreenCommand::ScrollLines(lines));
        }
    }

    fn scroll_to_bottom(&mut self) {
        self.send(GhosttyScreenCommand::ScrollToBottom);
    }

    fn scroll_to_offset(&mut self, offset: usize) {
        self.send(GhosttyScreenCommand::ScrollToOffset(offset));
    }

    fn display_offset(&self) -> usize {
        self.request(GhosttyScreenCommand::DisplayOffset)
    }

    fn input_mode(&self) -> TerminalInputMode {
        self.request(GhosttyScreenCommand::InputMode)
    }

    fn snapshot(&self, scroll_pixel_offset: f64) -> TerminalScreenSnapshot {
        self.request(|reply| GhosttyScreenCommand::Snapshot {
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
        let _ = self.tx.send(GhosttyScreenCommand::Snapshot {
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
        let _ = self.tx.send(GhosttyScreenCommand::RemoteViewportSnapshot {
            display_offset,
            overscan_rows,
            max_lines,
            reply: tx,
        });
        HeadlessTerminalSnapshotRequest { rx }
    }

    fn has_history_above_viewport(&self) -> bool {
        self.request(GhosttyScreenCommand::HasHistoryAboveViewport)
    }
}

enum GhosttyScreenCommand {
    Process(Vec<u8>),
    ProcessReplay(Vec<u8>),
    Resize {
        cols: usize,
        rows: usize,
    },
    ScrollLines(i32),
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
    Clear,
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

struct GhosttyScreenWorker {
    term: Term<HeadlessEventProxy>,
    parser: Processor,
    events: Rc<RefCell<Vec<Event>>>,
    cols: usize,
    rows: usize,
    scrollback: usize,
    responder: Option<TerminalPtyResponder>,
}

impl GhosttyScreenWorker {
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
            ..Default::default()
        };
        let term = Term::new(
            config,
            &HeadlessTermSize::new(cols, rows),
            HeadlessEventProxy {
                events: events.clone(),
            },
        );
        Self {
            term,
            parser: Processor::new(),
            events,
            cols,
            rows,
            scrollback,
            responder,
        }
    }

    /// Feed bytes to the parser, then dispatch any query replies the engine
    /// produced. `answer_queries` is false for replayed history so stale
    /// DSR/DA queries in recorded output don't inject unsolicited replies.
    fn feed(&mut self, bytes: &[u8], answer_queries: bool) {
        self.parser.advance(&mut self.term, bytes);
        let events: Vec<Event> = self.events.borrow_mut().drain(..).collect();
        let Some(responder) = self.responder.as_ref() else {
            return;
        };
        if !answer_queries {
            return;
        }
        for event in events {
            match event {
                Event::PtyWrite(text) => responder(text.as_bytes()),
                Event::TextAreaSizeRequest(format) => {
                    responder(format(self.window_size()).as_bytes())
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
            cell_width: GHOSTTY_CELL_WIDTH_PX as u16,
            cell_height: GHOSTTY_CELL_HEIGHT_PX as u16,
        }
    }

    fn run(mut self, rx: mpsc::Receiver<GhosttyScreenCommand>) {
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
                GhosttyScreenCommand::Process(bytes) => self.feed(&bytes, true),
                GhosttyScreenCommand::ProcessReplay(bytes) => self.feed(&bytes, false),
                GhosttyScreenCommand::Resize { mut cols, mut rows } => {
                    // Layout settling queues resizes back-to-back, and every
                    // column change reflows the whole scrollback. Collapse a
                    // consecutive run of resizes into the final one; ordering
                    // relative to other commands is preserved.
                    while let Ok(following) = rx.try_recv() {
                        match following {
                            GhosttyScreenCommand::Resize {
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
                GhosttyScreenCommand::ScrollLines(lines) => self.scroll_lines(lines),
                GhosttyScreenCommand::ScrollToBottom => {
                    self.term.scroll_display(Scroll::Bottom);
                }
                GhosttyScreenCommand::ScrollToOffset(offset) => self.scroll_to_offset(offset),
                GhosttyScreenCommand::DisplayOffset(reply) => {
                    let _ = reply.send(self.display_offset());
                }
                GhosttyScreenCommand::InputMode(reply) => {
                    let _ = reply.send(terminal_input_mode(&self.term));
                }
                GhosttyScreenCommand::HasHistoryAboveViewport(reply) => {
                    let _ = reply.send(self.has_history_above_viewport());
                }
                GhosttyScreenCommand::Snapshot {
                    scroll_pixel_offset,
                    include_data,
                    reply,
                } => {
                    let _ = reply.send(self.snapshot(scroll_pixel_offset, include_data));
                }
                GhosttyScreenCommand::SnapshotAtOffset { offset, reply } => {
                    let saved = self.display_offset();
                    self.scroll_to_offset(offset);
                    let snapshot = self.snapshot(0.0, true);
                    self.scroll_to_offset(saved);
                    let _ = reply.send(snapshot);
                }
                GhosttyScreenCommand::RemoteViewportSnapshot {
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
                GhosttyScreenCommand::Clear => {
                    self = Self::new(
                        self.cols,
                        self.rows,
                        self.scrollback,
                        self.responder.clone(),
                    );
                }
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
            // background shows through (and the snapshot stays compact).
            if text.is_empty() && bg == TerminalScreenColor::Default && !cell_has_visuals(cell) {
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
                underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
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
            cells,
            cursor,
        }
    }
}

/// DEC-mode prefix for a keyframe so a viewer that replays it restores the
/// active modes alongside the painted cells. Emitted before the repaint: the
/// alt-screen enter must precede the paint so it lands in the alternate buffer,
/// and the mouse / cursor-key modes make the viewer forward wheel and arrow
/// input instead of scrolling its local history.
fn terminal_keyframe_mode_prefix(mode: &TerminalInputMode) -> String {
    let mut out = String::new();
    if mode.alternate_screen {
        out.push_str("\x1b[?1049h");
    }
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
    out
}

fn terminal_input_mode(term: &Term<HeadlessEventProxy>) -> TerminalInputMode {
    let mode = term.mode();
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
    underline: bool,
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
            underline: false,
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
        output.push_str("\x1b[0m");
    }
    if cursor.visible {
        output.push_str(&format!("\x1b[{};{}H", cursor.row + 1, cursor.col + 1));
        output.push_str("\x1b[?25h");
    }
    output
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
    if style.underline {
        codes.push("4".to_string());
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
    let data = terminal_snapshot_data(viewport.cols, rows, &cells, &cursor);
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
