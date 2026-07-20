//! Live AI runtime engine shared by the desktop app and the headless agent: the
//! AI session supervisor (`ai_runtime`), its summarized state + remote
//! current-session mapping (`ai_runtime_state`), the enhanced PTY manager that
//! feeds the supervisor (`terminal_pty`), the shared remote-terminal protocol
//! router both hosts dispatch through (`remote_terminal_dispatch`), and the
//! runtime path helpers they need (`runtime_paths`). Kept free of AppKit/objc so
//! a headless host can depend on it without dragging in the desktop's platform
//! layer.

pub mod agent_worktree;
pub mod ai_runtime;
pub mod ai_runtime_state;
pub mod host_metrics;
pub mod remote_terminal_dispatch;
pub mod runtime_paths;
pub mod terminal_pty;
pub mod wrapper_helper;
