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
    parser: TerminalOscParser,
    last_activity_at: f64,
    last_screen_signal_at: f64,
    // Titles repeat every spinner frame; only signal TRANSITIONS become status.
    last_title_signal: Option<TerminalTitleAgentSignal>,
    last_title_submit_at: f64,
}

/// Throttle output heartbeats so a chatty turn does not lock the state store on
/// every byte; one refresh per second is ample against the 90s staleness sweep.
const OUTPUT_ACTIVITY_THROTTLE_SECONDS: f64 = 1.0;
const SCREEN_SIGNAL_THROTTLE_SECONDS: f64 = 0.25;
// Spinner/blink frames keep flowing while a turn is active; re-asserting the
// deduped state occasionally keeps the desktop's 30s stale GC away without
// leaning on the runtime probes.
const TITLE_ACTIVE_KEEPALIVE_SECONDS: f64 = 5.0;

impl AIRuntimeTerminalOutputWatcher {
    pub(super) fn new(binding: AIRuntimeTerminalBinding, ai_runtime: Arc<AIRuntimeBridge>) -> Self {
        Self {
            binding,
            ai_runtime,
            parser: TerminalOscParser::default(),
            last_activity_at: 0.0,
            last_screen_signal_at: 0.0,
            last_title_signal: None,
            last_title_submit_at: 0.0,
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
                        crate::ai_runtime::terminal_status::TERMINAL_NOTIFICATION_OSC_SOURCE,
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
                TerminalOscEvent::Title(signal) => self.handle_title_signal(signal),
            }
        }
    }

    // codex paints turn state into its OSC 0 title (braille spinner while
    // responding, "Action Required" while blocked); transitions map onto the
    // status channel. A plain title after a spinner is a finished turn; after
    // an Action Required prefix it is a dismissed prompt, which only clears.
    fn handle_title_signal(&mut self, signal: TerminalTitleAgentSignal) {
        let now = now_seconds();
        let previous = self.last_title_signal.replace(signal);
        if previous == Some(signal) {
            let active = matches!(
                signal,
                TerminalTitleAgentSignal::Working | TerminalTitleAgentSignal::Waiting
            );
            if active && now - self.last_title_submit_at >= TITLE_ACTIVE_KEEPALIVE_SECONDS {
                self.last_title_submit_at = now;
                self.submit_terminal_status(
                    match signal {
                        TerminalTitleAgentSignal::Working => TerminalStatusState::Working,
                        _ => TerminalStatusState::Waiting,
                    },
                    crate::ai_runtime::terminal_status::TERMINAL_TITLE_OSC_SOURCE,
                );
            }
            return;
        }
        let state = match signal {
            TerminalTitleAgentSignal::Working => TerminalStatusState::Working,
            TerminalTitleAgentSignal::Waiting => TerminalStatusState::Waiting,
            TerminalTitleAgentSignal::Plain => match previous {
                Some(TerminalTitleAgentSignal::Working) => TerminalStatusState::Completed,
                Some(TerminalTitleAgentSignal::Waiting) => TerminalStatusState::Idle,
                _ => return,
            },
        };
        self.last_title_submit_at = now;
        self.submit_terminal_status(
            state,
            crate::ai_runtime::terminal_status::TERMINAL_TITLE_OSC_SOURCE,
        );
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
