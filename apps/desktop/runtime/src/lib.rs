pub mod ai_history;
// The AI usage-history engine lives in the shared `codux-ai-history` crate so
// the headless agent can host the same stats with full parity. Re-export it
// under the historical module paths the desktop code already uses.
pub use codux_ai_history::indexer as ai_history_indexer;
pub use codux_ai_history::normalized as ai_history_normalized;
pub use codux_ai_history::usage_store as ai_usage_store;
// The live AI runtime engine (supervisor, runtime state, enhanced PTY manager,
// runtime paths) lives in the AppKit-free `codux-runtime-live` crate so the
// headless agent runs the same engine without the desktop platform layer.
// Re-exported under the historical module paths the desktop code already uses.
pub use codux_runtime_live::{ai_runtime, ai_runtime_state, runtime_paths, terminal_pty};
// Cross-platform path primitives (drive-list sentinel, Windows path detection,
// separator-aware joins, `\\?\` stripping) live in the shared core crate so the
// host's file.list and the desktop UI breadcrumb agree on path handling.
// Re-exported as `codux_runtime::path`.
pub use codux_runtime_core::path;
pub use codux_runtime_core::project;
pub mod app_commands;
pub mod app_icon;
pub mod app_info;
pub mod app_milestones;
pub mod async_runtime;
pub mod background_queue;
pub mod config;
pub mod db;
pub mod desktop_pet;
pub mod dialog;
pub mod dock_badge;
pub mod file_editor_layout;
pub mod files;
pub mod host_browser;
// The git engine (GitService + git_* commands + GitWatchManager + the shared
// `wire` dispatch) lives in the AppKit-free `codux-git` crate so the headless
// agent runs the exact same git logic. Re-exported as `crate::git`.
pub use codux_git as git;
pub mod i18n;
pub mod llm;
pub mod memory;
pub mod notification;
pub mod performance;
pub mod persistent_cache;
pub mod pet;
pub mod power;
pub mod project_activity;
pub mod project_open;
pub mod project_store;
pub mod remote;
pub use codux_protocol::{
    RemoteHostCpuMetrics, RemoteHostDiskMetrics, RemoteHostMemoryMetrics, RemoteHostMetrics,
    RemoteHostNetworkMetrics, RemoteHostProcessMetrics, RemoteHostSystemMetrics,
};
pub mod runtime_activity;
pub mod runtime_bridge;
pub mod runtime_cache;
pub mod runtime_event;
pub mod runtime_state;
pub mod runtime_terminal;
pub mod runtime_trace;
pub mod settings;
pub mod ssh;
pub mod system_fonts;
pub mod system_limits;
pub mod terminal_layout;
pub mod terminal_runtime;
pub mod tool_permissions;
pub mod update;
pub mod worktree;
pub use codux_runtime_live::wrapper_helper;
pub mod wsl;
