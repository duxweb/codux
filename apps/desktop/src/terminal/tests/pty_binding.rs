use super::super::*;

#[derive(Default)]
struct HostedTestController {
    actions: std::sync::Mutex<Vec<String>>,
    terminals: Vec<String>,
}

impl HostedTestController {
    fn with_terminals(terminals: &[&str]) -> Self {
        Self {
            actions: std::sync::Mutex::new(Vec::new()),
            terminals: terminals.iter().map(|value| value.to_string()).collect(),
        }
    }

    fn record(&self, action: impl Into<String>) {
        self.actions.lock().unwrap().push(action.into());
    }
}

impl RuntimeTerminalController for HostedTestController {
    fn open_terminal(&self, config: &TerminalPtyConfig) -> Result<String, String> {
        let session_id = config
            .terminal_id
            .clone()
            .ok_or_else(|| "terminal id missing".to_string())?;
        self.record(format!("open:{session_id}"));
        Ok(session_id)
    }

    fn list_terminals(&self) -> Result<serde_json::Value, String> {
        self.record("list");
        Ok(serde_json::json!({
            "terminals": self
                .terminals
                .iter()
                .map(|id| serde_json::json!({ "id": id }))
                .collect::<Vec<_>>()
        }))
    }

    fn terminal_input(&self, _session_id: &str, _bytes: &[u8]) -> bool {
        true
    }

    fn terminal_resize(&self, _session_id: &str, _cols: u16, _rows: u16) -> bool {
        true
    }

    fn close_terminal(&self, _session_id: &str) -> Result<(), String> {
        Ok(())
    }

    fn close_terminal_fire(&self, _session_id: &str) -> bool {
        true
    }

    fn register_terminal_output(
        &self,
        session_id: &str,
        _forwarder: codux_runtime::runtime_terminal::RuntimeTerminalOutputForwarder,
    ) {
        self.record(format!("output:{session_id}"));
    }

    fn unregister_terminal_output(&self, _session_id: &str) {}

    fn register_terminal_events(
        &self,
        session_id: &str,
        _forwarder: codux_runtime::runtime_terminal::RuntimeTerminalEventForwarder,
    ) {
        self.record(format!("events:{session_id}"));
    }
}

#[test]
fn pending_terminal_binding_matches_requested_config_before_attach() {
    let config = terminal_pty_config_with_view(
        TerminalPtyConfig {
            cwd: Some("/tmp/project".to_string()),
            project_id: Some("project-1".to_string()),
            terminal_id: Some("terminal-1".to_string()),
            session_key: Some("gpui:project-1:terminal-1".to_string()),
            ..Default::default()
        },
        &terminal_config(),
    );

    let (binding, _initial_layout_rx) = TerminalSessionBinding::pending(config.clone());

    assert!(binding.matches_pty_config(&config));

    let mut different_terminal = config;
    different_terminal.terminal_id = Some("terminal-2".to_string());
    assert!(!binding.matches_pty_config(&different_terminal));
}

#[test]
fn hosted_restore_recreates_missing_session_after_registering_forwarders() {
    let controller = HostedTestController::default();
    let config = TerminalPtyConfig {
        terminal_id: Some("terminal-1".to_string()),
        ..Default::default()
    };
    let (output_tx, _) = flume::unbounded();
    let (event_tx, _) = flume::unbounded();
    let (wake_tx, _) = flume::unbounded();

    let session_id = restore_hosted_session(
        &controller,
        "terminal-1",
        &config,
        &output_tx,
        &event_tx,
        &wake_tx,
    )
    .unwrap();

    assert_eq!(session_id, "terminal-1");
    assert_eq!(
        *controller.actions.lock().unwrap(),
        [
            "list",
            "output:terminal-1",
            "events:terminal-1",
            "open:terminal-1"
        ]
    );
}

#[test]
fn hosted_restore_reuses_existing_session_without_reopening() {
    let controller = HostedTestController::with_terminals(&["terminal-1"]);
    let config = TerminalPtyConfig {
        terminal_id: Some("terminal-1".to_string()),
        ..Default::default()
    };
    let (output_tx, _) = flume::unbounded();
    let (event_tx, _) = flume::unbounded();
    let (wake_tx, _) = flume::unbounded();

    let session_id = restore_hosted_session(
        &controller,
        "terminal-1",
        &config,
        &output_tx,
        &event_tx,
        &wake_tx,
    )
    .unwrap();

    assert_eq!(session_id, "terminal-1");
    assert_eq!(*controller.actions.lock().unwrap(), ["list"]);
}

#[test]
fn reconnect_event_clears_terminal_failure_state() {
    let mut model = TerminalModel::new_for_test(80, 24, 100);

    assert!(model.apply_ui_event(TerminalUiEvent::Exit));
    assert!(model.apply_ui_event(TerminalUiEvent::Error("runtime exited".to_string())));
    assert!(model.exited);
    assert!(model.title.is_some());

    assert!(model.apply_ui_event(TerminalUiEvent::Reconnected));
    assert!(!model.exited);
    assert!(model.title.is_none());
}
