use super::*;
use crate::ai_runtime::{TerminalStatusEvent, TerminalStatusState};

pub(super) fn attach_ai_runtime_terminal_output_watcher(
    session: &TerminalPtySession,
    ai_runtime: Arc<AIRuntimeBridge>,
) {
    let binding = session.ai_runtime_binding();
    let watcher = Arc::new(parking_lot::Mutex::new(
        AIRuntimeTerminalOutputWatcher::new(binding, ai_runtime),
    ));
    session.subscribe_events(Arc::new(move |event| {
        watcher.lock().handle_terminal_event(&event);
        true
    }));
}

pub(super) struct AIRuntimeTerminalOutputWatcher {
    binding: AIRuntimeTerminalBinding,
    ai_runtime: Arc<AIRuntimeBridge>,
    parser: TerminalProgressOscParser,
    last_activity_at: f64,
    last_screen_signal_at: f64,
}

/// Throttle output heartbeats so a chatty turn does not lock the state store on
/// every byte; one refresh per second is ample against the 90s staleness sweep.
const OUTPUT_ACTIVITY_THROTTLE_SECONDS: f64 = 1.0;
const SCREEN_SIGNAL_THROTTLE_SECONDS: f64 = 0.25;

impl AIRuntimeTerminalOutputWatcher {
    pub(super) fn new(binding: AIRuntimeTerminalBinding, ai_runtime: Arc<AIRuntimeBridge>) -> Self {
        Self {
            binding,
            ai_runtime,
            parser: TerminalProgressOscParser::default(),
            last_activity_at: 0.0,
            last_screen_signal_at: 0.0,
        }
    }

    pub(super) fn handle_terminal_event(&mut self, event: &TerminalEvent) {
        let TerminalEvent::Output {
            session_id, bytes, ..
        } = event
        else {
            return;
        };
        if session_id != &self.binding.terminal_id {
            return;
        }
        let now = now_seconds();
        if now - self.last_activity_at >= OUTPUT_ACTIVITY_THROTTLE_SECONDS {
            self.last_activity_at = now;
            // Keeps an in-flight AI turn alive; it never fabricates terminal
            // status, which is driven by OSC events below.
            self.ai_runtime
                .note_output_activity(&self.binding.terminal_id, now);
        }
        if now - self.last_screen_signal_at >= SCREEN_SIGNAL_THROTTLE_SECONDS {
            self.last_screen_signal_at = now;
            self.ai_runtime
                .refresh_screen_signal(&self.binding.terminal_id);
        }
        for progress in self.parser.push(bytes) {
            match progress {
                TerminalOscEvent::Progress(state) => self.submit_terminal_status(
                    terminal_status_state_for_progress(state),
                    crate::ai_runtime::terminal_status::TERMINAL_PROGRESS_OSC_SOURCE,
                ),
                TerminalOscEvent::Notification(TerminalNotificationKind::ApprovalRequested)
                | TerminalOscEvent::Notification(TerminalNotificationKind::PlanModePrompt) => {
                    self.submit_terminal_status(
                        TerminalStatusState::Waiting,
                        "terminal-notification-osc",
                    );
                }
                TerminalOscEvent::Command(state) => self.submit_terminal_status(
                    match state {
                        TerminalCommandOscState::Started => TerminalStatusState::Working,
                        // D clears rather than completing: per-command green dots
                        // for every `ls` would be pure noise.
                        TerminalCommandOscState::Finished => TerminalStatusState::Idle,
                    },
                    crate::ai_runtime::terminal_status::TERMINAL_COMMAND_OSC_SOURCE,
                ),
            }
        }
    }

    fn submit_terminal_status(&self, state: TerminalStatusState, source: &str) {
        let status = TerminalStatusEvent {
            terminal_id: self.binding.terminal_id.clone(),
            terminal_instance_id: self.binding.terminal_instance_id.clone(),
            state,
            updated_at: now_seconds(),
            source: source.to_string(),
        };
        if let Err(error) = self.ai_runtime.submit_terminal_status(status) {
            crate::ai_runtime::runtime_log_line(
                "terminal-ai-runtime",
                &format!(
                    "submit terminal status failed terminal={} source={} error={}",
                    self.binding.terminal_id, source, error
                ),
            );
        }
    }
}

fn terminal_status_state_for_progress(state: TerminalProgressOscState) -> TerminalStatusState {
    match state {
        TerminalProgressOscState::Completed => TerminalStatusState::Completed,
        TerminalProgressOscState::Working => TerminalStatusState::Working,
        TerminalProgressOscState::Error => TerminalStatusState::Error,
        TerminalProgressOscState::Warning => TerminalStatusState::Warning,
    }
}
