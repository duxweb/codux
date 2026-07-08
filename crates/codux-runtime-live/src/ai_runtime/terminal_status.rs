use crate::ai_runtime::snapshot::AIRuntimeStateSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalStatusState {
    Idle,
    Working,
    Waiting,
    Completed,
    Error,
    Warning,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalStatusEvent {
    pub terminal_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_instance_id: Option<String>,
    pub state: TerminalStatusState,
    pub updated_at: f64,
    pub source: String,
}

pub(crate) const TERMINAL_PROGRESS_OSC_SOURCE: &str = "terminal-progress-osc";
pub(crate) const RUNTIME_PROBE_STATUS_SOURCE: &str = "runtime-probe";
// OSC 133 C/D from the staged shell integration; command-level, so the desktop
// must not stale-GC it against AI turn liveness.
pub const TERMINAL_COMMAND_OSC_SOURCE: &str = "terminal-command-osc";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbePhase {
    Idle,
    Busy,
    NeedsInput,
}

fn probe_phase(state: &str) -> ProbePhase {
    match state {
        "responding" | "running" => ProbePhase::Busy,
        "needsInput" | "needs-input" => ProbePhase::NeedsInput,
        _ => ProbePhase::Idle,
    }
}

// CLIs that never emit progress OSC (codex) still need loading dots, so probe
// session phases feed the same status channel. Once a terminal emits real
// progress OSC it owns Working/Completed; the probe then only opens and closes
// needs-input episodes (CLIs don't re-emit OSC around approval prompts).
#[derive(Default)]
pub(crate) struct ProbeStatusFallback {
    phases: HashMap<String, ProbePhase>,
    progress_osc_terminals: HashSet<String>,
}

impl ProbeStatusFallback {
    pub(crate) fn note_status_event(&mut self, event: &TerminalStatusEvent) {
        if event.source == TERMINAL_PROGRESS_OSC_SOURCE {
            self.progress_osc_terminals
                .insert(event.terminal_id.clone());
        }
    }

    pub(crate) fn forget(&mut self, terminal_id: &str) {
        self.phases.remove(terminal_id);
        self.progress_osc_terminals.remove(terminal_id);
    }

    pub(crate) fn sync(
        &mut self,
        snapshot: &AIRuntimeStateSnapshot,
        now: f64,
    ) -> Vec<TerminalStatusEvent> {
        self.sync_entries(
            snapshot.sessions.iter().map(|session| {
                (
                    session.terminal_id.as_str(),
                    session.terminal_instance_id.as_deref(),
                    session.state.as_str(),
                    session.has_completed_turn && !session.was_interrupted,
                )
            }),
            now,
        )
    }

    fn sync_entries<'a>(
        &mut self,
        entries: impl Iterator<Item = (&'a str, Option<&'a str>, &'a str, bool)>,
        now: f64,
    ) -> Vec<TerminalStatusEvent> {
        let mut events = Vec::new();
        let mut live = HashSet::new();
        for (terminal_id, terminal_instance_id, session_state, turn_completed) in entries {
            live.insert(terminal_id.to_string());
            let next = probe_phase(session_state);
            let previous = self
                .phases
                .insert(terminal_id.to_string(), next)
                .unwrap_or(ProbePhase::Idle);
            let osc_owned = self.progress_osc_terminals.contains(terminal_id);
            let Some(state) = probe_transition_status(previous, next, osc_owned, turn_completed)
            else {
                continue;
            };
            events.push(TerminalStatusEvent {
                terminal_id: terminal_id.to_string(),
                terminal_instance_id: terminal_instance_id.map(str::to_string),
                state,
                updated_at: now,
                source: RUNTIME_PROBE_STATUS_SOURCE.to_string(),
            });
        }
        // OSC marks may arrive before the probe session exists, so only drop a
        // mark together with the session it belonged to — never for merely not
        // having a session yet.
        let dead: Vec<String> = self
            .phases
            .keys()
            .filter(|terminal_id| !live.contains(*terminal_id))
            .cloned()
            .collect();
        for terminal_id in dead {
            self.phases.remove(&terminal_id);
            self.progress_osc_terminals.remove(&terminal_id);
        }
        events
    }
}

// Completed is reserved for genuinely finished turns: a silently timed-out or
// interrupted runtime goes back to idle with `turn_completed == false` (see
// store::helpers::mark_timed_out) and must clear the dot, not fake a green one.
fn probe_transition_status(
    previous: ProbePhase,
    next: ProbePhase,
    osc_owned: bool,
    turn_completed: bool,
) -> Option<TerminalStatusState> {
    if previous == next {
        return None;
    }
    let idle_state = if turn_completed {
        TerminalStatusState::Completed
    } else {
        TerminalStatusState::Idle
    };
    match next {
        ProbePhase::NeedsInput => Some(TerminalStatusState::Waiting),
        // Closing a needs-input episode must not depend on the CLI re-emitting
        // OSC, so these two fire even for OSC-owning terminals.
        ProbePhase::Busy if previous == ProbePhase::NeedsInput => {
            Some(TerminalStatusState::Working)
        }
        ProbePhase::Idle if previous == ProbePhase::NeedsInput => Some(idle_state),
        ProbePhase::Busy if !osc_owned => Some(TerminalStatusState::Working),
        ProbePhase::Idle if !osc_owned => Some(idle_state),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn osc_event(terminal_id: &str, source: &str) -> TerminalStatusEvent {
        TerminalStatusEvent {
            terminal_id: terminal_id.to_string(),
            terminal_instance_id: None,
            state: TerminalStatusState::Working,
            updated_at: 1.0,
            source: source.to_string(),
        }
    }

    fn states(events: &[TerminalStatusEvent]) -> Vec<TerminalStatusState> {
        events.iter().map(|event| event.state).collect()
    }

    #[test]
    fn probe_fallback_drives_full_turn_without_osc() {
        let mut fallback = ProbeStatusFallback::default();

        let started =
            fallback.sync_entries([("term-1", Some("i-1"), "responding", false)].into_iter(), 1.0);
        assert_eq!(states(&started), [TerminalStatusState::Working]);
        assert_eq!(started[0].source, RUNTIME_PROBE_STATUS_SOURCE);
        assert_eq!(started[0].terminal_instance_id.as_deref(), Some("i-1"));

        let unchanged =
            fallback.sync_entries([("term-1", Some("i-1"), "responding", false)].into_iter(), 2.0);
        assert!(unchanged.is_empty());

        let waiting =
            fallback.sync_entries([("term-1", Some("i-1"), "needsInput", false)].into_iter(), 3.0);
        assert_eq!(states(&waiting), [TerminalStatusState::Waiting]);

        let resumed =
            fallback.sync_entries([("term-1", Some("i-1"), "responding", false)].into_iter(), 4.0);
        assert_eq!(states(&resumed), [TerminalStatusState::Working]);

        let finished =
            fallback.sync_entries([("term-1", Some("i-1"), "idle", true)].into_iter(), 5.0);
        assert_eq!(states(&finished), [TerminalStatusState::Completed]);
    }

    #[test]
    fn silent_timeout_clears_instead_of_faking_completion() {
        let mut fallback = ProbeStatusFallback::default();
        fallback.sync_entries([("term-1", None, "responding", false)].into_iter(), 1.0);

        // mark_timed_out retires with has_completed_turn=false: clear, no green.
        let retired = fallback.sync_entries([("term-1", None, "idle", false)].into_iter(), 2.0);
        assert_eq!(states(&retired), [TerminalStatusState::Idle]);
    }

    #[test]
    fn interrupted_needs_input_episode_clears_instead_of_completing() {
        let mut fallback = ProbeStatusFallback::default();
        fallback.sync_entries([("term-1", None, "needsInput", false)].into_iter(), 1.0);

        let rejected = fallback.sync_entries([("term-1", None, "idle", false)].into_iter(), 2.0);
        assert_eq!(states(&rejected), [TerminalStatusState::Idle]);
    }

    #[test]
    fn progress_osc_terminal_keeps_probe_out_of_working_and_completed() {
        let mut fallback = ProbeStatusFallback::default();
        fallback.note_status_event(&osc_event("term-1", TERMINAL_PROGRESS_OSC_SOURCE));

        let busy = fallback.sync_entries([("term-1", None, "responding", false)].into_iter(), 1.0);
        assert!(busy.is_empty());

        let idle = fallback.sync_entries([("term-1", None, "idle", true)].into_iter(), 2.0);
        assert!(idle.is_empty());
    }

    #[test]
    fn progress_osc_terminal_still_gets_needs_input_episode() {
        let mut fallback = ProbeStatusFallback::default();
        fallback.note_status_event(&osc_event("term-1", TERMINAL_PROGRESS_OSC_SOURCE));
        fallback.sync_entries([("term-1", None, "responding", false)].into_iter(), 1.0);

        let waiting =
            fallback.sync_entries([("term-1", None, "needsInput", false)].into_iter(), 2.0);
        assert_eq!(states(&waiting), [TerminalStatusState::Waiting]);

        // Episode close self-corrects even though the terminal owns progress OSC.
        let resumed =
            fallback.sync_entries([("term-1", None, "responding", false)].into_iter(), 3.0);
        assert_eq!(states(&resumed), [TerminalStatusState::Working]);
    }

    #[test]
    fn notification_osc_does_not_claim_progress_ownership() {
        let mut fallback = ProbeStatusFallback::default();
        fallback.note_status_event(&osc_event("term-1", "terminal-notification-osc"));

        let busy = fallback.sync_entries([("term-1", None, "responding", false)].into_iter(), 1.0);
        assert_eq!(states(&busy), [TerminalStatusState::Working]);
    }

    #[test]
    fn vanished_sessions_are_forgotten_including_their_osc_mark() {
        let mut fallback = ProbeStatusFallback::default();
        fallback.note_status_event(&osc_event("term-1", TERMINAL_PROGRESS_OSC_SOURCE));
        fallback.sync_entries([("term-1", None, "responding", false)].into_iter(), 1.0);

        let gone = fallback.sync_entries(std::iter::empty(), 2.0);
        assert!(gone.is_empty());
        assert!(fallback.phases.is_empty());
        assert!(fallback.progress_osc_terminals.is_empty());

        // Re-appearing busy counts as a fresh start, not a stale continuation.
        let back = fallback.sync_entries([("term-1", None, "responding", false)].into_iter(), 3.0);
        assert_eq!(states(&back), [TerminalStatusState::Working]);
    }

    #[test]
    fn forget_resets_phase_and_osc_ownership_for_reopened_terminal() {
        let mut fallback = ProbeStatusFallback::default();
        fallback.note_status_event(&osc_event("term-1", TERMINAL_PROGRESS_OSC_SOURCE));
        fallback.sync_entries([("term-1", None, "responding", false)].into_iter(), 1.0);

        fallback.forget("term-1");
        assert!(fallback.phases.is_empty());
        assert!(fallback.progress_osc_terminals.is_empty());

        // Same terminal id reopened: fresh phase, no inherited OSC ownership.
        let back = fallback.sync_entries([("term-1", None, "responding", false)].into_iter(), 2.0);
        assert_eq!(states(&back), [TerminalStatusState::Working]);
    }

    #[test]
    fn early_osc_mark_survives_until_its_session_exists() {
        let mut fallback = ProbeStatusFallback::default();
        fallback.note_status_event(&osc_event("term-osc", TERMINAL_PROGRESS_OSC_SOURCE));
        // Unrelated session churn must not evict the mark of a terminal whose
        // probe session has not appeared yet.
        fallback.sync_entries([("term-other", None, "responding", false)].into_iter(), 1.0);

        let busy = fallback.sync_entries(
            [
                ("term-other", None, "responding", false),
                ("term-osc", None, "responding", false),
            ]
            .into_iter(),
            2.0,
        );
        assert!(busy.is_empty());
    }
}
