use crate::terminal_pty::TerminalPtyConfig;
use codux_terminal_core::TerminalEvent;
use serde_json::Value;

pub type RuntimeTerminalOutputForwarder = Box<dyn Fn(Vec<u8>) + Send + Sync>;
pub type RuntimeTerminalEventForwarder = Box<dyn Fn(TerminalEvent) + Send + Sync>;

pub trait RuntimeTerminalController: Send + Sync {
    fn open_terminal(&self, config: &TerminalPtyConfig) -> Result<String, String>;
    fn list_terminals(&self) -> Result<Value, String>;
    fn terminal_input(&self, session_id: &str, bytes: &[u8]) -> bool;
    fn terminal_resize(&self, session_id: &str, cols: u16, rows: u16) -> bool;
    fn close_terminal(&self, session_id: &str) -> Result<(), String>;
    fn close_terminal_fire(&self, session_id: &str) -> bool;
    fn register_terminal_output(&self, session_id: &str, forwarder: RuntimeTerminalOutputForwarder);
    fn unregister_terminal_output(&self, session_id: &str);
    fn register_terminal_events(
        &self,
        _session_id: &str,
        _forwarder: RuntimeTerminalEventForwarder,
    ) {
    }
    fn unregister_terminal_events(&self, _session_id: &str) {}
}

impl RuntimeTerminalController for crate::remote::RemoteController {
    fn open_terminal(&self, config: &TerminalPtyConfig) -> Result<String, String> {
        crate::remote::RemoteController::open_terminal(self, config)
    }

    fn list_terminals(&self) -> Result<Value, String> {
        crate::remote::RemoteController::list_terminals(self)
    }

    fn terminal_input(&self, session_id: &str, bytes: &[u8]) -> bool {
        crate::remote::RemoteController::terminal_input(
            self,
            session_id,
            &String::from_utf8_lossy(bytes),
        )
    }

    fn terminal_resize(&self, session_id: &str, cols: u16, rows: u16) -> bool {
        crate::remote::RemoteController::terminal_resize(self, session_id, cols, rows)
    }

    fn close_terminal(&self, session_id: &str) -> Result<(), String> {
        crate::remote::RemoteController::close_terminal(self, session_id).map(|_| ())
    }

    fn close_terminal_fire(&self, session_id: &str) -> bool {
        crate::remote::RemoteController::close_terminal_fire(self, session_id)
    }

    fn register_terminal_output(
        &self,
        session_id: &str,
        forwarder: RuntimeTerminalOutputForwarder,
    ) {
        crate::remote::RemoteController::register_terminal_output(self, session_id, forwarder);
    }

    fn unregister_terminal_output(&self, session_id: &str) {
        crate::remote::RemoteController::unregister_terminal_output(self, session_id);
    }
}
