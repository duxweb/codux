use crate::ai_runtime::{AIRuntimeBridge, AIRuntimeTerminalBinding};
use anyhow::{Context, Result, anyhow};
use codux_terminal_core::{
    HeadlessTerminalScreen, TerminalDriver as CoreTerminalDriver, TerminalEventSink,
    TerminalLaunchConfig, TerminalPtyResponder, TerminalQueryColors, TerminalScreenSnapshot,
    TerminalSessionHandle as CoreTerminalSessionHandle,
};
pub use codux_terminal_core::{TerminalEvent, TerminalSessionSnapshot, TerminalViewportState};
use codux_terminal_pty::{
    LocalPtyCommandMode, LocalPtyProcessHandle, LocalPtySpawnConfig, spawn_local_pty,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex as StdMutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[cfg(not(windows))]
use std::{
    process::{Command, Stdio},
    sync::OnceLock,
};
use uuid::Uuid;

const INPUT_CAPTURE_LIMIT: usize = 20;
const OUTPUT_CAPTURE_LIMIT: usize = 16 * 1024;
const MIN_HISTORY_BYTES: usize = 128 * 1024;
const MAX_CONFIGURED_HISTORY_BYTES: usize = 8 * 1024 * 1024;
const REMOTE_SCREEN_SCROLLBACK_CAP: usize = 2_000;
const REMOTE_SCREEN_IDLE_SCROLLBACK: usize = 500;
const TERMINAL_VIEWPORT_LEASE_TTL: Duration = Duration::from_secs(20);
const REMOTE_TERMINAL_QUERY_COLORS: TerminalQueryColors = TerminalQueryColors {
    foreground: (0xe6, 0xed, 0xf3),
    background: (0x0d, 0x11, 0x17),
};

mod capture;
mod config;
mod environment;
mod events;
mod manager;
mod osc;
mod platform;
mod session;
#[cfg(test)]
mod tests;
mod watcher;

pub use capture::{TerminalCapturedInput, TerminalInputSnapshot, TerminalOutputSnapshot};
pub use config::{TerminalLaunchContext, TerminalPtyConfig};
pub use environment::terminal_environment;
pub use events::{
    EventSink, ViewportOwnerResolver, terminal_viewport_local_owner, terminal_viewport_remote_owner,
};
pub use manager::{DesktopTerminalSessionHandle, TerminalManager};
pub use platform::default_shell;
pub use session::{TerminalBaselineSnapshot, TerminalPtySession, TerminalPtySessionHandle};

use capture::*;
use config::*;
use events::*;
use osc::*;
use platform::*;
use session::*;
use watcher::*;
