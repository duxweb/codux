//! Types and functions around terminal state management.

use std::{marker::PhantomData, mem::MaybeUninit};

use crate::{
    alloc::{Allocator, Object},
    error::{Error, Result, from_result},
    ffi, key,
    screen::GridRef,
    style,
};

#[doc(inline)]
pub use ffi::GhosttySizeReportSize as SizeReportSize;

/// Complete terminal emulator state and rendering.
///
/// A terminal instance manages the full emulator state including the screen,
/// scrollback, cursor, styles, modes, and VT stream processing.
///
/// Once a terminal session is up and running, you can configure a key encoder
/// to write keyboard input via [`key::Encoder::set_options_from_terminal`].
///
/// # Effects
///
/// By default, the terminal sequence processing with [`Terminal::vt_write`]
/// only process sequences that directly affect terminal state and ignores
/// sequences that have side effect behavior or require responses. These
/// sequences include things like bell characters, title changes, device
/// attributes queries, and more. To handle these sequences, the user
/// must configure "effects."
///
/// Effects are callbacks that the terminal invokes in response to VT sequences
/// processed during [`Terminal::vt_write`]. They let the embedding application
/// react to terminal-initiated events such as bell characters, title changes,
/// device status report responses, and more.
///
/// Each effect is registered with its corresponding `Terminal::on_<effect>`
/// function, which accepts a closure with access to the terminal state and
/// possibly other parameters. Some examples include [`Terminal::on_bell`]
/// and [`Terminal::on_pty_write`].
///
/// All callbacks are invoked synchronously during [`Terminal::vt_write`].
/// Callbacks must be very careful to not block for too long or perform
/// expensive operations, since they are blocking further IO processing.
///
/// ## Shared state
///
/// **Unlike the C API**, you *cannot* specify arbitrary user data that's
/// shared between all callbacks, mainly because a safe, idiomatic Rust
/// equivalent of this pattern is very difficult to implement and use
/// due to Rust's much stricter safety guarantees. In turn, we use the
/// user data internally for callback dispatch purposes.
///
/// You should instead use idiomatic Rust mechanisms like [`Rc`](std::rc::Rc)s
/// to hold common, mutable state between callbacks (which is perfectly safe,
/// since everything is run on a single thread within a single `vt_write` call),
/// or with some other type with interior mutability.
///
/// ## Example: Registering effects and processing VT data
///
/// ```rust
/// use std::{cell::Cell, rc::Rc};
/// use libghostty_vt::{Terminal, TerminalOptions};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut terminal = Terminal::new(TerminalOptions {
///     cols: 80,
///     rows: 24,
///     max_scrollback: 0,
/// })?;
///
/// // Set up a simple bell counter
/// let bell_count = Rc::new(Cell::new(0usize));
/// terminal
///     .on_pty_write(|_term, data| {
///         println!("Replying {} bytes to the PTY", data.len());
///     })?
///    .on_bell({
///        let bell_count = bell_count.clone();
///        move |_term| {
///            bell_count.update(|v| v + 1);
///            println!("Bell! (count = {})", bell_count.get())
///        }
///     })?
///    .on_title_changed(|term| {
///        // Query the cursor position to confirm the terminal processed the
///        // title change (the title itself is tracked by the embedder via the
///        // OSC parser or its own state).
///        let col = term.cursor_x().unwrap();
///        println!("Title changed! (cursor at col {col})");
///    })?;
///
/// // Feed VT data that triggers effects:
/// // 1. Bell (BEL = 0x07)
/// terminal.vt_write(b"\x07");
/// // 2. Title change (OSC 2 ; <title> ST)
/// terminal.vt_write(b"\x1b]2;Hello Effects\x1b\\");
/// // 3. Device status report (DECRQM for wraparound mode ?7)
/// //    triggers write_pty with the response
/// terminal.vt_write(b"\x1B[?7$p");
/// // 4. Another bell to show the counter increments
/// terminal.vt_write(b"\x07");
///
/// assert_eq!(bell_count.get(), 2);
/// # Ok(())}
/// ```
#[derive(Debug)]
pub struct Terminal<'alloc: 'cb, 'cb> {
    pub(crate) inner: Object<'alloc, ffi::GhosttyTerminal>,
    vtable: Option<Box<VTable<'alloc, 'cb>>>,
}

/// Terminal initialization options.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Terminal width in cells. Must be greater than zero.
    pub cols: u16,
    /// Terminal height in cells. Must be greater than zero.
    pub rows: u16,
    /// Maximum number of lines to keep in scrollback history.
    pub max_scrollback: usize,
}

impl From<Options> for ffi::GhosttyTerminalOptions {
    fn from(value: Options) -> Self {
        Self {
            cols: value.cols,
            rows: value.rows,
            max_scrollback: value.max_scrollback,
        }
    }
}

impl<'alloc: 'cb, 'cb> Terminal<'alloc, 'cb> {
    /// Create a new terminal instance.
    pub fn new(opts: Options) -> Result<Self> {
        // SAFETY: A NULL allocator is always valid
        unsafe { Self::new_inner(std::ptr::null(), opts) }
    }

    /// Create a new terminal instance with a custom allocator.
    ///
    /// See the [crate-level documentation](crate#memory-management-and-lifetimes)
    /// regarding custom memory management and lifetimes.
    pub fn new_with_alloc<'ctx: 'alloc, Ctx>(
        alloc: &'alloc Allocator<'ctx, Ctx>,
        opts: Options,
    ) -> Result<Self> {
        // SAFETY: Borrow checking should forbid invalid allocators
        unsafe { Self::new_inner(alloc.to_raw(), opts) }
    }

    unsafe fn new_inner(alloc: *const ffi::GhosttyAllocator, opts: Options) -> Result<Self> {
        let mut raw: ffi::GhosttyTerminal_ptr = std::ptr::null_mut();
        let result = unsafe { ffi::ghostty_terminal_new(alloc, &raw mut raw, opts.into()) };
        from_result(result)?;
        Ok(Self {
            inner: Object::new(raw)?,
            vtable: Some(Box::new(VTable::default())),
        })
    }

    /// Write VT-encoded data to the terminal for processing.
    ///
    /// Feeds raw bytes through the terminal's VT stream parser, updating
    /// terminal state accordingly. By default, sequences that require output
    /// (queries, device status reports) are silently ignored.
    /// Use [`Terminal::on_pty_write`] to install a callback that receives
    /// response data.
    ///
    /// This never fails. Any erroneous input or errors in processing the input
    /// are logged internally but do not cause this function to fail because
    /// this input is assumed to be untrusted and from an external source; so
    /// the primary goal is to keep the terminal state consistent and not allow
    /// malformed input to corrupt or crash.    
    pub fn vt_write(&mut self, data: &[u8]) {
        unsafe { ffi::ghostty_terminal_vt_write(self.inner.as_raw(), data.as_ptr(), data.len()) }
    }

    /// Resize the terminal to the given dimensions.
    ///
    /// Changes the number of columns and rows in the terminal. The primary
    /// screen will reflow content if wraparound mode is enabled; the alternate
    /// screen does not reflow. If the dimensions are unchanged, this is a no-op.
    ///
    /// This also updates the terminal's pixel dimensions (used for image
    /// protocols and size reports), disables synchronized output mode (allowed
    /// by the spec so that resize results are shown immediately), and sends an
    /// in-band size report if mode 2048 is enabled.
    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<()> {
        let result = unsafe {
            ffi::ghostty_terminal_resize(
                self.inner.as_raw(),
                cols,
                rows,
                cell_width_px,
                cell_height_px,
            )
        };
        from_result(result)
    }

    /// Perform a full reset of the terminal (RIS).
    ///
    /// Resets all terminal state back to its initial configuration,
    /// including modes, scrollback, scrolling region, and screen contents.
    /// The terminal dimensions are preserved.
    pub fn reset(&mut self) {
        unsafe { ffi::ghostty_terminal_reset(self.inner.as_raw()) }
    }

    /// Scroll the terminal viewport.
    pub fn scroll_viewport(&mut self, scroll: ScrollViewport) {
        unsafe { ffi::ghostty_terminal_scroll_viewport(self.inner.as_raw(), scroll.into()) }
    }

    /// Resolve a point in the terminal grid to a grid reference.
    ///
    /// Resolves the given point (which can be in active, viewport, screen,
    /// or history coordinates) to a grid reference for that location. Use
    /// [`GridRef::cell`] and [`GridRef::row`] to extract the cell and row.
    ///
    /// Lookups in the active region and viewport are fast. Lookups in the
    /// screen and history may require traversing the full scrollback page
    /// list to resolve the y coordinate, so they can be expensive for large
    /// scrollback buffers.
    ///
    /// This function isn't meant to be used as the core of render loop. It
    /// isn't built to sustain the framerates needed for rendering large
    /// screens. Use the [render state API](crate::render::RenderState) for
    /// that. This API is instead meant for less strictly performance-sensitive
    /// use cases.
    pub fn grid_ref(&self, point: Point) -> Result<GridRef<'_>> {
        let mut grid_ref = ffi::sized!(ffi::GhosttyGridRef);
        let result = unsafe {
            ffi::ghostty_terminal_grid_ref(self.inner.as_raw(), point.into(), &raw mut grid_ref)
        };
        from_result(result)?;
        Ok(GridRef {
            inner: grid_ref,
            _phan: PhantomData,
        })
    }

    /// Get the current value of a terminal mode.
    pub fn mode(&self, mode: Mode) -> Result<bool> {
        let mut value = false;
        let result = unsafe {
            ffi::ghostty_terminal_mode_get(self.inner.as_raw(), mode.into(), &raw mut value)
        };
        from_result(result)?;
        Ok(value)
    }

    /// Set the value of a terminal mode.
    pub fn set_mode(&mut self, mode: Mode, value: bool) -> Result<()> {
        let result =
            unsafe { ffi::ghostty_terminal_mode_set(self.inner.as_raw(), mode.into(), value) };
        from_result(result)
    }

    fn get<T>(&self, tag: ffi::GhosttyTerminalData) -> Result<T> {
        let mut value = MaybeUninit::<T>::zeroed();
        let result = unsafe {
            ffi::ghostty_terminal_get(self.inner.as_raw(), tag, value.as_mut_ptr().cast())
        };
        from_result(result)?;
        // SAFETY: Value should be initialized after successful call.
        Ok(unsafe { value.assume_init() })
    }

    /// Get the terminal width in cells.
    pub fn cols(&self) -> Result<u16> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLS)
    }
    /// Get the terminal height in cells.
    pub fn rows(&self) -> Result<u16> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_ROWS)
    }
    /// Get the cursor column position (inner-indexed).
    pub fn cursor_x(&self) -> Result<u16> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_X)
    }
    /// Get the cursor row position within the active area (inner-indexed).
    pub fn cursor_y(&self) -> Result<u16> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_Y)
    }
    /// Get whether the cursor has a pending wrap (next print will soft-wrap).
    pub fn is_cursor_pending_wrap(&self) -> Result<bool> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP)
    }
    /// Get whether the cursor is visible (DEC mode 25).
    pub fn is_cursor_visible(&self) -> Result<bool> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE)
    }
    /// Get the current SGR style of the cursor.
    ///
    /// This is the style that will be applied to newly printed characters.
    pub fn cursor_style(&self) -> Result<style::Style> {
        self.get::<ffi::GhosttyStyle>(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_STYLE)
            .and_then(std::convert::TryInto::try_into)
    }
    /// Get the current Kitty keyboard protocol flags.
    pub fn kitty_keyboard_flags(&self) -> Result<key::KittyKeyFlags> {
        self.get::<ffi::GhosttyKittyKeyFlags>(
            ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS,
        )
        .map(key::KittyKeyFlags::from_bits_retain)
    }

    /// Get the scrollbar state for the terminal viewport.
    ///
    /// This may be expensive to calculate depending on where the viewport is
    /// (arbitrary pins are expensive). The caller should take care to only call
    /// this as needed and not too frequently.
    pub fn scrollbar(&self) -> Result<ffi::GhosttyTerminalScrollbar> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_SCROLLBAR)
    }
    /// Get the currently active screen.
    pub fn active_screen(&self) -> Result<ffi::GhosttyTerminalScreen> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN)
    }
    /// Get whether any mouse tracking mode is active.
    ///
    /// Returns true if any of the mouse tracking modes (X1inner, normal, button,
    /// or any-event) are enabled.
    pub fn is_mouse_tracking(&self) -> Result<bool> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING)
    }
    /// Get the terminal title as set by escape sequences (e.g. OSC inner/2).
    ///
    /// Returns a borrowed string, valid until the next call to
    /// [`Terminal::vt_write`] or [`Terminal::reset`]. An empty string is
    /// returned when no title has been set.
    pub fn title(&self) -> Result<&str> {
        let str =
            self.get::<ffi::GhosttyString>(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_TITLE)?;
        // SAFETY: We trust libghostty to return a valid borrowed string,
        // while we uphold that no mutation could happen during its lifetime.
        let str = unsafe { std::slice::from_raw_parts(str.ptr, str.len) };
        std::str::from_utf8(str).map_err(|_| Error::InvalidValue)
    }

    /// Get the current working directory as set by escape sequences (e.g. OSC 7).
    ///
    /// Returns a borrowed string, valid until the next call to
    /// [`Terminal::vt_write`] or [`Terminal::reset`]. An empty string is
    /// returned when no title has been set.
    pub fn pwd(&self) -> Result<&str> {
        let str =
            self.get::<ffi::GhosttyString>(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_PWD)?;
        // SAFETY: We trust libghostty to return a valid borrowed string,
        // while we uphold that no mutation could happen during its lifetime.
        let str = unsafe { std::slice::from_raw_parts(str.ptr, str.len) };
        std::str::from_utf8(str).map_err(|_| Error::InvalidValue)
    }
    /// The total number of rows in the active screen including scrollback.
    pub fn total_rows(&self) -> Result<usize> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_TOTAL_ROWS)
    }
    ///  The number of scrollback rows (total rows minus viewport rows).
    pub fn scrollback_rows(&self) -> Result<usize> {
        self.get(ffi::GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_SCROLLBACK_ROWS)
    }
}

impl Drop for Terminal<'_, '_> {
    fn drop(&mut self) {
        unsafe { ffi::ghostty_terminal_free(self.inner.as_raw()) }
    }
}

/// A point in the terminal grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point {
    /// Active area where the cursor can move.
    Active(PointCoordinate),
    /// Visible viewport (changes when scrolled).
    Viewport(PointCoordinate),
    /// Full screen including scrollback.
    Screen(PointCoordinate),
    /// Scrollback history only (before active area).
    History(PointCoordinate),
}

impl From<Point> for ffi::GhosttyPoint {
    fn from(value: Point) -> Self {
        match value {
            Point::Active(coord) => Self {
                tag: ffi::GhosttyPointTag_GHOSTTY_POINT_TAG_ACTIVE,
                value: ffi::GhosttyPointValue {
                    coordinate: coord.into(),
                },
            },
            Point::Viewport(coord) => Self {
                tag: ffi::GhosttyPointTag_GHOSTTY_POINT_TAG_VIEWPORT,
                value: ffi::GhosttyPointValue {
                    coordinate: coord.into(),
                },
            },
            Point::Screen(coord) => Self {
                tag: ffi::GhosttyPointTag_GHOSTTY_POINT_TAG_SCREEN,
                value: ffi::GhosttyPointValue {
                    coordinate: coord.into(),
                },
            },
            Point::History(coord) => Self {
                tag: ffi::GhosttyPointTag_GHOSTTY_POINT_TAG_HISTORY,
                value: ffi::GhosttyPointValue {
                    coordinate: coord.into(),
                },
            },
        }
    }
}

/// A coordinate in the terminal grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointCoordinate {
    /// Column (0-indexed).
    x: u16,
    /// Row (0-indexed). May exceed page size for screen/history tags.
    y: u32,
}
impl PointCoordinate {
    /// Create a point coordinate.
    #[must_use]
    pub const fn new(x: u16, y: u32) -> Self {
        Self { x, y }
    }
}
impl From<PointCoordinate> for ffi::GhosttyPointCoordinate {
    fn from(value: PointCoordinate) -> Self {
        let PointCoordinate { x, y } = value;
        Self { x, y }
    }
}
impl From<ffi::GhosttyPointCoordinate> for PointCoordinate {
    fn from(value: ffi::GhosttyPointCoordinate) -> Self {
        let ffi::GhosttyPointCoordinate { x, y } = value;
        Self { x, y }
    }
}

/// Scroll viewport behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollViewport {
    /// Scroll to the top of the scrollback.
    Top,
    /// Scroll to the bottom (active area).
    Bottom,
    /// Scroll by a delta amount (up is negative).
    Delta(isize),
}
impl From<ScrollViewport> for ffi::GhosttyTerminalScrollViewport {
    fn from(value: ScrollViewport) -> Self {
        match value {
            ScrollViewport::Top => Self {
                tag: ffi::GhosttyTerminalScrollViewportTag_GHOSTTY_SCROLL_VIEWPORT_TOP,
                value: ffi::GhosttyTerminalScrollViewportValue::default(),
            },
            ScrollViewport::Bottom => Self {
                tag: ffi::GhosttyTerminalScrollViewportTag_GHOSTTY_SCROLL_VIEWPORT_BOTTOM,
                value: ffi::GhosttyTerminalScrollViewportValue::default(),
            },
            ScrollViewport::Delta(delta) => Self {
                tag: ffi::GhosttyTerminalScrollViewportTag_GHOSTTY_SCROLL_VIEWPORT_DELTA,
                value: {
                    let mut v = ffi::GhosttyTerminalScrollViewportValue::default();
                    v.delta = delta;
                    v
                },
            },
        }
    }
}

/// A terminal mode consisting of its value and its kind (DEC/ANSI).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Mode(pub ffi::GhosttyMode);

impl Mode {
    #![expect(missing_docs, reason = "no upstream documentation provided")]
    const ANSI_BIT: u16 = 1 << 15;

    /// Create a new mode from its numeric value and its kind.
    #[must_use]
    pub const fn new(v: u16, kind: ModeKind) -> Self {
        match kind {
            ModeKind::Ansi => Self(v | Self::ANSI_BIT),
            ModeKind::Dec => Self(v),
        }
    }

    /// The numeric value of the mode.
    #[must_use]
    pub fn value(self) -> u16 {
        (self.0) & 0x7fff
    }

    /// The kind of the mode (DEC/ANSI).
    #[must_use]
    pub fn kind(self) -> ModeKind {
        if (self.0) & Self::ANSI_BIT > 0 {
            ModeKind::Ansi
        } else {
            ModeKind::Dec
        }
    }

    pub const KAM: Self = Self::new(2, ModeKind::Ansi);
    pub const INSERT: Self = Self::new(4, ModeKind::Ansi);
    pub const SRM: Self = Self::new(12, ModeKind::Ansi);
    pub const LINEFEED: Self = Self::new(20, ModeKind::Ansi);

    pub const DECCKM: Self = Self::new(1, ModeKind::Dec);
    pub const _132_COLUMN: Self = Self::new(3, ModeKind::Dec);
    pub const SLOW_SCROLL: Self = Self::new(4, ModeKind::Dec);
    pub const REVERSE_COLORS: Self = Self::new(5, ModeKind::Dec);
    pub const ORIGIN: Self = Self::new(6, ModeKind::Dec);
    pub const WRAPAROUND: Self = Self::new(7, ModeKind::Dec);
    pub const AUTOREPEAT: Self = Self::new(8, ModeKind::Dec);
    pub const X10_MOUSE: Self = Self::new(9, ModeKind::Dec);
    pub const CURSOR_BLINKING: Self = Self::new(12, ModeKind::Dec);
    pub const CURSOR_VISIBLE: Self = Self::new(25, ModeKind::Dec);
    pub const ENABLE_MODE3: Self = Self::new(40, ModeKind::Dec);
    pub const REVERSE_WRAP: Self = Self::new(45, ModeKind::Dec);
    pub const ALT_SCREEN_LEGACY: Self = Self::new(47, ModeKind::Dec);
    pub const KEYPAD_KEYS: Self = Self::new(66, ModeKind::Dec);
    pub const LEFT_RIGHT_MARGIN: Self = Self::new(69, ModeKind::Dec);
    pub const NORMAL_MOUSE: Self = Self::new(1000, ModeKind::Dec);
    pub const BUTTON_MOUSE: Self = Self::new(1002, ModeKind::Dec);
    pub const ANY_MOUSE: Self = Self::new(1003, ModeKind::Dec);
    pub const FOCUS_EVENT: Self = Self::new(1004, ModeKind::Dec);
    pub const UTF8_MOUSE: Self = Self::new(1005, ModeKind::Dec);
    pub const SGR_MOUSE: Self = Self::new(1006, ModeKind::Dec);
    pub const ALT_SCROLL: Self = Self::new(1007, ModeKind::Dec);
    pub const URXVT_MOUSE: Self = Self::new(1015, ModeKind::Dec);
    pub const SGR_PIXELS_MOUSE: Self = Self::new(1016, ModeKind::Dec);
    pub const NUMLOCK_KEYPAD: Self = Self::new(1035, ModeKind::Dec);
    pub const ALT_ESC_PREFIX: Self = Self::new(1036, ModeKind::Dec);
    pub const ALT_SENDS_ESC: Self = Self::new(1039, ModeKind::Dec);
    pub const REVERSE_WRAP_EXT: Self = Self::new(1045, ModeKind::Dec);
    pub const ALT_SCREEN: Self = Self::new(1047, ModeKind::Dec);
    pub const SAVE_CURSOR: Self = Self::new(1048, ModeKind::Dec);
    pub const ALT_SCREEN_SAVE: Self = Self::new(1049, ModeKind::Dec);
    pub const BRACKETED_PASTE: Self = Self::new(2004, ModeKind::Dec);
    pub const SYNC_OUTPUT: Self = Self::new(2026, ModeKind::Dec);
    pub const GRAPHEME_CLUSTER: Self = Self::new(2027, ModeKind::Dec);
    pub const COLOR_SCHEME_REPORT: Self = Self::new(2031, ModeKind::Dec);
    pub const IN_BAND_RESIZE: Self = Self::new(2048, ModeKind::Dec);
}

/// The kind of a terminal mode.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModeKind {
    /// DEC terminal mode.
    Dec,
    /// ANSI terminal mode.
    Ansi,
}

impl From<Mode> for ffi::GhosttyMode {
    fn from(value: Mode) -> Self {
        value.0
    }
}

/// Device attributes response data for all three DA levels.
/// Filled by the [`Terminal::on_device_attributes`] callback in response
/// to CSI c, CSI > c, or CSI = c queries. The terminal uses whichever
/// sub-struct matches the request type.
#[derive(Debug, Clone, Copy)]
pub struct DeviceAttributes {
    /// Primary device attributes (DA1).
    pub primary: PrimaryDeviceAttributes,
    /// Secondary device attributes (DA2).
    pub secondary: SecondaryDeviceAttributes,
    /// Tertiary device attributes (DA3).
    pub tertiary: TertiaryDeviceAttributes,
}

impl From<DeviceAttributes> for ffi::GhosttyDeviceAttributes {
    fn from(value: DeviceAttributes) -> Self {
        Self {
            primary: value.primary.into(),
            secondary: value.secondary.into(),
            tertiary: value.tertiary.into(),
        }
    }
}

/// Primary device attributes (DA1) response data.
///
/// Returned as part of [`DeviceAttributes`] in response to a CSI c query.
#[derive(Debug, Clone, Copy)]
pub struct PrimaryDeviceAttributes(ffi::GhosttyDeviceAttributesPrimary);

impl PrimaryDeviceAttributes {
    /// Construct primary device attributes from a conformance level
    /// and an array of device attribute features.
    ///
    /// # Panics
    ///
    /// **Panics** when more than 64 features are given.
    #[must_use]
    pub fn new<const N: usize>(
        conformance_level: ConformanceLevel,
        features: [DeviceAttributeFeature; N],
    ) -> Self {
        assert!(N <= 64);

        let mut f = [0u16; 64];
        f[..N].copy_from_slice(features.map(|f| f.0).as_slice());

        Self(ffi::GhosttyDeviceAttributesPrimary {
            conformance_level: conformance_level.0,
            features: f,
            num_features: N,
        })
    }
}

impl From<PrimaryDeviceAttributes> for ffi::GhosttyDeviceAttributesPrimary {
    fn from(value: PrimaryDeviceAttributes) -> Self {
        value.0
    }
}

/// The level of conformance to the behavior of a specific or a family of
/// physical terminal models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConformanceLevel(pub u16);

impl ConformanceLevel {
    #![allow(clippy::cast_possible_truncation, reason = "bindgen ain't perfect")]
    #![expect(missing_docs, reason = "self-explanatory")]
    pub const VT100: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT100 as u16);
    pub const VT101: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT101 as u16);
    pub const VT102: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT102 as u16);
    pub const VT125: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT125 as u16);
    pub const VT131: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT131 as u16);
    pub const VT132: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT132 as u16);
    pub const VT220: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT220 as u16);
    pub const VT240: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT240 as u16);
    pub const VT320: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT320 as u16);
    pub const VT340: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT340 as u16);
    pub const VT420: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT420 as u16);
    pub const VT510: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT510 as u16);
    pub const VT520: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT520 as u16);
    pub const VT525: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_VT525 as u16);
    /// Equivalent to a VT2xx terminal.
    pub const LEVEL_2: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_LEVEL_2 as u16);
    /// Equivalent to a VT3xx terminal.
    pub const LEVEL_3: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_LEVEL_3 as u16);
    /// Equivalent to a VT4xx terminal.
    pub const LEVEL_4: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_LEVEL_4 as u16);
    /// Equivalent to a VT5xx terminal.
    pub const LEVEL_5: Self = Self(ffi::GHOSTTY_DA_CONFORMANCE_LEVEL_5 as u16);
}

/// A feature that a terminal can report to support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceAttributeFeature(pub u16);

impl DeviceAttributeFeature {
    #![expect(clippy::cast_possible_truncation, reason = "bindgen ain't perfect")]
    #![expect(missing_docs, reason = "no upstream documentation provided")]
    pub const COLUMNS_132: Self = Self(ffi::GHOSTTY_DA_FEATURE_COLUMNS_132 as u16);
    pub const PRINTER: Self = Self(ffi::GHOSTTY_DA_FEATURE_PRINTER as u16);
    pub const REGIS: Self = Self(ffi::GHOSTTY_DA_FEATURE_REGIS as u16);
    pub const SIXEL: Self = Self(ffi::GHOSTTY_DA_FEATURE_SIXEL as u16);
    pub const SELECTIVE_ERASE: Self = Self(ffi::GHOSTTY_DA_FEATURE_SELECTIVE_ERASE as u16);
    pub const USER_DEFINED_KEYS: Self = Self(ffi::GHOSTTY_DA_FEATURE_USER_DEFINED_KEYS as u16);
    pub const NATIONAL_REPLACEMENT: Self =
        Self(ffi::GHOSTTY_DA_FEATURE_NATIONAL_REPLACEMENT as u16);
    pub const TECHNICAL_CHARACTERS: Self =
        Self(ffi::GHOSTTY_DA_FEATURE_TECHNICAL_CHARACTERS as u16);
    pub const LOCATOR: Self = Self(ffi::GHOSTTY_DA_FEATURE_LOCATOR as u16);
    pub const TERMINAL_STATE: Self = Self(ffi::GHOSTTY_DA_FEATURE_TERMINAL_STATE as u16);
    pub const WINDOWING: Self = Self(ffi::GHOSTTY_DA_FEATURE_WINDOWING as u16);
    pub const HORIZONTAL_SCROLLING: Self =
        Self(ffi::GHOSTTY_DA_FEATURE_HORIZONTAL_SCROLLING as u16);
    pub const ANSI_COLOR: Self = Self(ffi::GHOSTTY_DA_FEATURE_ANSI_COLOR as u16);
    pub const RECTANGULAR_EDITING: Self = Self(ffi::GHOSTTY_DA_FEATURE_RECTANGULAR_EDITING as u16);
    pub const ANSI_TEXT_LOCATOR: Self = Self(ffi::GHOSTTY_DA_FEATURE_ANSI_TEXT_LOCATOR as u16);
    pub const CLIPBOARD: Self = Self(ffi::GHOSTTY_DA_FEATURE_CLIPBOARD as u16);
}

/// Secondary device attributes (DA2) response data.
///
/// Returned as part of [`DeviceAttributes`] in response to a CSI > c query.
/// Response format: CSI > Pp ; Pv ; Pc c
#[derive(Debug, Copy, Clone)]
pub struct SecondaryDeviceAttributes {
    /// Terminal type identifier (Pp).
    pub device_type: DeviceType,
    /// Firmware/patch version number (Pv).
    pub firmware_version: u16,
    /// ROM cartridge registration number (Pc). Always 0 for emulators.
    pub rom_cartridge: u16,
}

impl From<SecondaryDeviceAttributes> for ffi::GhosttyDeviceAttributesSecondary {
    fn from(value: SecondaryDeviceAttributes) -> Self {
        Self {
            device_type: value.device_type.0,
            firmware_version: value.firmware_version,
            rom_cartridge: value.rom_cartridge,
        }
    }
}

/// The type of terminal device being emulated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceType(pub u16);

impl DeviceType {
    #![expect(clippy::cast_possible_truncation, reason = "bindgen ain't perfect")]
    #![expect(missing_docs, reason = "self-explanatory")]
    pub const VT100: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT100 as u16);
    pub const VT220: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT220 as u16);
    pub const VT240: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT240 as u16);
    pub const VT330: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT330 as u16);
    pub const VT340: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT340 as u16);
    pub const VT320: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT320 as u16);
    pub const VT382: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT382 as u16);
    pub const VT420: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT420 as u16);
    pub const VT510: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT510 as u16);
    pub const VT520: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT520 as u16);
    pub const VT525: Self = Self(ffi::GHOSTTY_DA_DEVICE_TYPE_VT525 as u16);
}

/// Tertiary device attributes (DA3) response data.
///
/// Returned as part of [`DeviceAttributes`] in response to a CSI = c query.
/// Response format: DCS ! | D...D ST (DECRPTUI).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TertiaryDeviceAttributes {
    /// Unit ID encoded as 8 uppercase hex digits in the response.
    pub unit_id: u32,
}

impl From<TertiaryDeviceAttributes> for ffi::GhosttyDeviceAttributesTertiary {
    fn from(value: TertiaryDeviceAttributes) -> Self {
        Self {
            unit_id: value.unit_id,
        }
    }
}

//---------------------------------------
// Callbacks
//---------------------------------------

/// You might be wondering just what the heck this is.
///
/// Truth to be told, you don't need to understand how it works
/// in order to use it. It does a bunch of voodoo behind the scenes
/// that make sure all the invariants of the C API are upheld, while
/// providing a convenient API for Rust users.
///
/// Each handler is defined in this following format:
/// ```ignore
/// pub fn on_foobar(
///     &mut self,
///     // The corresponding GhosttyTerminalOption
///     tag = GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_FOOBAR,
///
///     // The name of the original function type in C,
///     // along with the extra C parameters and the expected C return type
///     from = GhosttyTerminalFoobarFn(foo: *const u8, bar: usize) -> bool,
///
///     // The name of mapped Rust function type,
///     // along with the Rust parameters and return type.
///     //
///     // `<'t>` is used to tie the return value to the lifetime of the
///     // terminal. The name is arbitrary - any lifetime marker will do.
///     to = <'t>FoobarFn(&'t [u8]) -> bool,
/// ) |term, func| {
///     // `term` is the terminal and `func` is the Rust callback.
///     // Both names are arbitrary.
///
///     // Convert the raw parameters into Rust types.
///     // This is just to illustrate how.
///     let slice = unsafe { std::slice::from_raw_parts(foo, bar) };
///
///     // Call into user logic and return.
///     func(&terminal, slice)
/// }
/// ```
macro_rules! handlers {
    {
        $(
            $(#[$fmeta:meta])*
            $vis:vis fn $name:ident(
                &mut self,
                tag = $tag:ident,
                from = $rawfnty:ident( $($rfname:ident: $rfty:ty),*$(,)? ) $(-> $rawrty:ty)?,
                $(#[$tmeta:meta])*
                to = $(<$lf:lifetime>)? $fnty:ident( $($fty:ty),*$(,)? ) $(-> $rty:ty)?,
            ) |$t:ident, $func:ident| $block:block
        )*
    } => {
        impl<'alloc, 'cb> $crate::terminal::Terminal<'alloc, 'cb> {$(
            $(#[$fmeta])*
            $vis fn $name(&mut self, f: impl $fnty<'alloc, 'cb>) -> $crate::error::Result<&mut Self> {
                unsafe extern "C" fn callback(
                    t: *mut $crate::ffi::GhosttyTerminal,
                    ud: *mut std::ffi::c_void,
                    $($rfname: $rfty),*
                ) $(-> $rawrty)? {
                    // SAFETY: We own the vtable, so it should never become invalid.
                    let vtable = unsafe { &mut *ud.cast::<VTable<'_, '_>>() };

                    let obj = $crate::alloc::Object::new(t).expect("received null terminal ptr in callback - this is a bug!");
                    let $t = $crate::terminal::Terminal::<'_, '_> {
                        inner: obj,
                        vtable: None,
                    };
                    let $func = vtable.$name.as_deref_mut()
                        .expect("no handler set but callback is still called - this is a bug!");
                    let ret = $block;

                    // IMPORTANT: Do NOT let the destructor run.
                    ::core::mem::forget($t);
                    ret
                }

                let userdata = {
                    let vtable = self
                        .vtable
                        .as_mut()
                        .expect("missing terminal callback vtable - this is a bug!");
                    vtable.$name = Some(::std::boxed::Box::new(f));
                    ::std::ptr::from_ref::<VTable<'alloc, 'cb>>(vtable.as_ref())
                        .cast::<::std::ffi::c_void>()
                };

                let result = unsafe {
                    $crate::ffi::ghostty_terminal_set(
                        self.inner.as_raw(),
                        $crate::ffi::GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_USERDATA,
                        userdata
                    )
                };
                $crate::error::from_result(result)?;

                // The callback must be coerced into a function *pointer*
                // and not a function *item* (which is a ZST whose address is meaningless).
                // :)
                let callback_ptr: unsafe extern "C" fn(
                    *mut $crate::ffi::GhosttyTerminal,
                    *mut ::std::ffi::c_void,
                    $($rfty),*
                ) $(-> $rawrty)? = callback;

                let result = unsafe {
                    $crate::ffi::ghostty_terminal_set(
                        self.inner.as_raw(),
                        $crate::ffi::$tag,
                        callback_ptr as *const ::std::ffi::c_void
                    )
                };
                $crate::error::from_result(result)?;
                Ok(self)
            }
        )*}
        $(
            #[doc = concat!(
                "Callback type for [`Terminal::",
                stringify!($name),
                "`](Terminal::",
                stringify!($name),
                ").\n"
            )]
            $(#[$tmeta])*
            pub trait $fnty<'alloc, 'cb>:
                $(for<$lf>)? FnMut(
                    &$($lf)? $crate::terminal::Terminal<'alloc, 'cb>,
                    $($fty),*
                ) $(-> $rty)? + 'cb {}

            impl<'alloc, 'cb, F> $fnty<'alloc, 'cb> for F
            where
                F: $(for<$lf>)? FnMut(
                    &$($lf)? $crate::terminal::Terminal<'alloc, 'cb>,
                    $($fty),*
                ) $(-> $rty)? + 'cb
            {}
        )*

        struct VTable<'alloc, 'cb> {
            $($name: Option<::std::boxed::Box<dyn $fnty<'alloc, 'cb>>>),*
        }

        impl ::core::fmt::Debug for VTable<'_, '_> {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                f.write_str("VTable {..}")
            }
        }

        impl ::core::default::Default for VTable<'_, '_> {
            fn default() -> Self {
                Self {
                    $($name: None),*
                }
            }
        }
    };
}

handlers! {
    /// Call the given function when the terminal needs to write data back
    /// to the pty (e.g. in response to a DECRQM query or device status report).
    pub fn on_pty_write(
        &mut self,
        tag = GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_WRITE_PTY,
        from = GhosttyTerminalWritePtyFn(ptr: *const u8, len: usize),
        to = <'t>PtyWriteFn(&'t [u8]),
    ) |term, func| {
        // SAFETY: We trust libghostty to return valid memory given we
        // uphold all lifetime invariants (e.g. no `vt_write` calls
        // during this callback, which is guaranteed via the mutable reference).
        let data = unsafe { std::slice::from_raw_parts(ptr, len) };
        func(&term, data);
    }

    /// Call the given function when the terminal receives
    /// a BEL character (0x07).
    pub fn on_bell(
        &mut self,
        tag = GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_BELL,
        from = GhosttyTerminalBellFn(),
        to = BellFn(),
    ) |term, func| {
        func(&term);
    }

    /// Call the given function when the terminal receives
    /// an ENQ character (0x05).
    pub fn on_enquiry(
        &mut self,
        tag = GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_ENQUIRY,
        from = GhosttyTerminalEnquiryFn() -> ffi::GhosttyString,
        to = <'t>EnquiryFn() -> Option<&'t str>,
    ) |term, func| {
        func(&term).unwrap_or("").into()
    }

    /// Call the given function when the terminal receives an XTVERSION
    /// query (CSI > q), and respond with the resulting version string
    /// (e.g. "myterm 1.0").
    pub fn on_xtversion(
        &mut self,
        tag = GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_XTVERSION,
        from = GhosttyTerminalXtversionFn() -> ffi::GhosttyString,
        to = <'t>XtversionFn() -> Option<&'t str>,
    ) |term, func| {
        func(&term).unwrap_or("").into()
    }

    /// Call the given function when the terminal title changes
    /// via escape sequences (e.g. OSC 0 or OSC 2).
    ///
    /// The new title can be queried from the terminal after
    /// the callback returns.
    pub fn on_title_changed(
        &mut self,
        tag = GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_TITLE_CHANGED,
        from = GhosttyTerminalTitleChangedFn(),
        to = TitleChanged(),
    ) |term, func| {
        func(&term);
    }

    /// Call the given function in response to XTWINOPS size queries
    /// (CSI 14/16/18 t).
    pub fn on_size(
        &mut self,
        tag = GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_SIZE,
        from = GhosttyTerminalSizeFn(out: *mut ffi::GhosttySizeReportSize) -> bool,
        to = SizeFn() -> Option<ffi::GhosttySizeReportSize>,
    ) |term, func| {
        if let Some(size) = func(&term) {
            // SAFETY: Out pointer is assumed to be valid.
            unsafe { *out = size };
            true
        } else {
            false
        }
    }

    /// Call the given function in response to a color scheme
    /// device status report query (CSI ? 996 n).
    ///
    /// Return `Some` to report the current color scheme,
    /// or return `None` to silently ignore.
    pub fn on_color_scheme(
        &mut self,
        tag = GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_COLOR_SCHEME,
        from = GhosttyTerminalColorSchemeFn(out: *mut ffi::GhosttyColorScheme) -> bool,
        to = ColorSchemeFn() -> Option<ffi::GhosttyColorScheme>,
    ) |term, func| {
        if let Some(size) = func(&term) {
            // SAFETY: Out pointer is assumed to be valid.
            unsafe { *out = size };
            true
        } else {
            false
        }
    }

    /// Call the given function in response to a device attributes query
    /// (CSI c, CSI > c, or CSI = c).
    ///
    /// Return `Some` with the response data,
    /// or return `None` to silently ignore.
    pub fn on_device_attributes(
        &mut self,
        tag = GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_DEVICE_ATTRIBUTES,
        from = GhosttyTerminalDeviceAttributesFn(out: *mut ffi::GhosttyDeviceAttributes) -> bool,
        to = DeviceAttributesFn() -> Option<DeviceAttributes>,
    ) |term, func| {
        if let Some(size) = func(&term) {
            // SAFETY: Out pointer is assumed to be valid.
            unsafe { *out = size.into() };
            true
        } else {
            false
        }
    }
}
