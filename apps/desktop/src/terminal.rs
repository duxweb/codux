use crate::heroicons::HeroIconName;
use anyhow::Result;
use codux_runtime::remote::RemoteController;
use codux_runtime::terminal_pty::{
    TerminalEvent, TerminalInputSnapshot, TerminalManager, TerminalOutputSnapshot,
    TerminalPtyConfig, TerminalPtySession, terminal_viewport_local_owner,
};
use codux_terminal_core::{
    HeadlessTerminalScreen, HeadlessTerminalSnapshotRequest, TerminalInputMode,
    TerminalScreenCellSnapshot, TerminalScreenColor, TerminalScreenCursorShape,
    TerminalScreenCursorSnapshot, TerminalScreenSnapshot,
};
use gpui::{
    App, AppContext, Bounds, ClipboardEntry, ClipboardItem, Context, CursorStyle, Edges, Element,
    ElementId, Entity, ExternalPaths, FocusHandle, Font, FontFeatures, FontStyle, FontWeight,
    GlobalElementId, Hsla, ImageFormat, InputHandler, InspectorElementId, InteractiveElement,
    IntoElement, KeyDownEvent, Keystroke, LayoutId, Modifiers, ModifiersChangedEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, NavigationDirection, ParentElement, Pixels,
    Point, Render, ScrollWheelEvent, SharedString, Size, Style, Styled, Subscription, Task,
    TextAlign, TextRun, TouchPhase, UTF16Selection, UnderlineStyle, WeakEntity, Window, div, px,
    quad, rgb, transparent_black,
};
use gpui_component::Icon;
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

include!("terminal/pane.rs");
include!("terminal/config.rs");
include!("terminal/view.rs");
include!("terminal/protocol.rs");
include!("terminal/render.rs");
include!("terminal/model.rs");
include!("terminal/content.rs");
include!("terminal/grid_version.rs");
include!("terminal/element.rs");
include!("terminal/input.rs");
include!("terminal/events.rs");
include!("terminal/keys.rs");
include!("terminal/mouse.rs");
include!("terminal/renderer.rs");
include!("terminal/palette.rs");
include!("terminal/tests.rs");
