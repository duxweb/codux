mod buffer_assembler;
mod headless_screen;
mod output_sequence;
mod remote_pty;
mod runtime_model;
mod sequence_guard;
mod terminal_driver;
mod terminal_input;

pub use buffer_assembler::{TerminalBufferAssembler, TerminalBufferAssemblyResult};
pub use headless_screen::{
    HeadlessTerminalScreen, HeadlessTerminalSnapshotRequest, TerminalPtyResponder,
    TerminalScreenCellSnapshot, TerminalScreenColor, TerminalScreenCursorShape,
    TerminalScreenCursorSnapshot, TerminalScreenSnapshot, stack_scrolled_snapshots,
};
pub use output_sequence::{
    TerminalOutputSequenceAction, TerminalOutputSequenceResult, TerminalOutputSequencer,
};
pub use remote_pty::{RemotePtyBaselinePageResult, RemotePtySession, RemotePtySnapshot};
pub use runtime_model::{
    RemoteRuntimeModel, RemoteRuntimePlan, RemoteRuntimeProject, RemoteRuntimeStateSnapshot,
    RemoteRuntimeTerminal, RemoteRuntimeTerminalScope, RemoteRuntimeWorktree,
    RemoteRuntimeWorktreeState, RuntimeModel, RuntimePlan, RuntimeProject, RuntimeStateSnapshot,
    RuntimeTerminal, RuntimeTerminalScope, RuntimeWorktree, RuntimeWorktreeState,
    runtime_scope_key, runtime_scope_parts,
};
pub use sequence_guard::RemoteSequenceGuard;
pub use terminal_driver::{
    TerminalBaselineRequest, TerminalDriver, TerminalEvent, TerminalEventSink,
    TerminalLaunchConfig, TerminalSessionHandle, TerminalSessionSnapshot, TerminalViewportState,
};
pub use terminal_input::{
    TerminalInputMode, TerminalKeyInput, TerminalKeyInputModifiers, TerminalMouseAction,
    TerminalMouseButton, TerminalMouseInput, terminal_insert_input, terminal_insert_input_bytes,
    terminal_is_copy_shortcut, terminal_is_paste_shortcut, terminal_key_input,
    terminal_key_input_bytes, terminal_mouse_input_bytes, terminal_paste_input_bytes,
    terminal_selector_input, terminal_selector_input_bytes, terminal_text_input,
    terminal_text_input_bytes,
};

pub type TerminalSequence = i64;

#[cfg(test)]
mod tests;
