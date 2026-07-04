use super::ai_runtime_status::{AgentLifecycleInput, AgentLifecycleState};
use super::CoduxApp;
use std::collections::HashSet;
use std::time::{Duration, Instant};

const MIN_WORKING_HOLD: Duration = Duration::from_millis(1500);
const WORKING_IDLE_DEBOUNCE: Duration = Duration::from_millis(8000);
const COMPLETED_DECAY: Duration = Duration::from_millis(3000);
const WORKING_WAITING_LOCK: Duration = Duration::from_millis(500);

pub(in crate::app) struct PaneAgentLifecycle {
    pub state: AgentLifecycleState,
    last_transition_at: Instant,
    last_input_at: Instant,
    working_waiting_lock_until: Option<Instant>,
    locked_reverse_input: Option<AgentLifecycleInput>,
}

impl PaneAgentLifecycle {
    pub(in crate::app) fn new(now: Instant) -> Self {
        Self {
            state: AgentLifecycleState::Idle,
            last_transition_at: now,
            last_input_at: now,
            working_waiting_lock_until: None,
            locked_reverse_input: None,
        }
    }

    pub(in crate::app) fn tick(&mut self, input: Option<AgentLifecycleInput>, now: Instant) {
        if input.is_some() {
            self.last_input_at = now;
        }

        if let Some(inp) = input {
            self.try_apply_input(inp, now);
        }

        self.apply_timer_transitions(now);
    }

    fn try_apply_input(&mut self, input: AgentLifecycleInput, now: Instant) -> bool {
        if self.is_reverse_transition_locked(input, now) {
            return false;
        }

        if self.state == AgentLifecycleState::Working
            && matches!(input, AgentLifecycleInput::Prompt | AgentLifecycleInput::Settle)
            && now.duration_since(self.last_transition_at) < MIN_WORKING_HOLD
        {
            return false;
        }

        let Some(next) = self.state.transition(input) else {
            return false;
        };

        let previous = self.state;
        self.state = next;
        self.last_transition_at = now;
        self.record_working_waiting_lock(previous, next, now);
        true
    }

    fn is_reverse_transition_locked(&self, input: AgentLifecycleInput, now: Instant) -> bool {
        let Some(lock_until) = self.working_waiting_lock_until else {
            return false;
        };
        now < lock_until && self.locked_reverse_input == Some(input)
    }

    fn record_working_waiting_lock(
        &mut self,
        from: AgentLifecycleState,
        to: AgentLifecycleState,
        now: Instant,
    ) {
        match (from, to) {
            (AgentLifecycleState::Working, AgentLifecycleState::Waiting) => {
                self.working_waiting_lock_until = Some(now + WORKING_WAITING_LOCK);
                self.locked_reverse_input = Some(AgentLifecycleInput::Busy);
            }
            (AgentLifecycleState::Waiting, AgentLifecycleState::Working) => {
                self.working_waiting_lock_until = Some(now + WORKING_WAITING_LOCK);
                self.locked_reverse_input = Some(AgentLifecycleInput::Prompt);
            }
            _ => {}
        }
    }

    fn apply_timer_transitions(&mut self, now: Instant) {
        if self.state == AgentLifecycleState::Working
            && now.duration_since(self.last_input_at) >= WORKING_IDLE_DEBOUNCE
        {
            self.state = AgentLifecycleState::Idle;
            self.last_transition_at = now;
            self.working_waiting_lock_until = None;
            self.locked_reverse_input = None;
            return;
        }

        if self.state == AgentLifecycleState::Completed
            && now.duration_since(self.last_transition_at) >= COMPLETED_DECAY
        {
            self.state = AgentLifecycleState::Idle;
            self.last_transition_at = now;
        }
    }
}

impl CoduxApp {
    pub(in crate::app) fn sync_pane_agent_lifecycle(&mut self) {
        let now = Instant::now();
        let sessions = &self.state.ai_runtime_state.sessions;

        for session in sessions {
            let input = AgentLifecycleState::from_session_state(&session.state);
            self.pane_agent_lifecycle
                .entry(session.terminal_id.clone())
                .or_insert_with(|| PaneAgentLifecycle::new(now))
                .tick(input, now);
        }

        let active_terminal_ids: HashSet<&str> =
            sessions.iter().map(|session| session.terminal_id.as_str()).collect();
        self.pane_agent_lifecycle
            .retain(|terminal_id, _| active_terminal_ids.contains(terminal_id.as_str()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_instant() -> Instant {
        Instant::now()
    }

    #[test]
    fn minimum_hold_blocks_prompt_within_window() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);

        let early = base + Duration::from_millis(500);
        lifecycle.tick(Some(AgentLifecycleInput::Prompt), early);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);
    }

    #[test]
    fn minimum_hold_blocks_settle_within_window() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);

        let early = base + Duration::from_millis(500);
        lifecycle.tick(Some(AgentLifecycleInput::Settle), early);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);
    }

    #[test]
    fn minimum_hold_allows_prompt_after_window() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);

        let later = base + Duration::from_millis(2000);
        lifecycle.tick(Some(AgentLifecycleInput::Prompt), later);
        assert_eq!(lifecycle.state, AgentLifecycleState::Waiting);
    }

    #[test]
    fn working_idle_debounce_does_not_trigger_before_threshold() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);

        let silent = base + Duration::from_millis(5000);
        lifecycle.tick(None, silent);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);
    }

    #[test]
    fn working_idle_debounce_triggers_after_threshold() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);

        let silent = base + Duration::from_millis(8000);
        lifecycle.tick(None, silent);
        assert_eq!(lifecycle.state, AgentLifecycleState::Idle);
    }

    #[test]
    fn completed_decay_returns_to_idle() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);
        let after_hold = base + Duration::from_millis(2000);
        lifecycle.tick(Some(AgentLifecycleInput::Settle), after_hold);
        assert_eq!(lifecycle.state, AgentLifecycleState::Completed);

        let decayed = after_hold + Duration::from_millis(3000);
        lifecycle.tick(Some(AgentLifecycleInput::Settle), decayed);
        assert_eq!(lifecycle.state, AgentLifecycleState::Idle);
    }

    #[test]
    fn completed_decay_interrupted_by_busy() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);
        let after_hold = base + Duration::from_millis(2000);
        lifecycle.tick(Some(AgentLifecycleInput::Settle), after_hold);
        assert_eq!(lifecycle.state, AgentLifecycleState::Completed);

        let busy_again = after_hold + Duration::from_millis(1000);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), busy_again);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);
    }

    #[test]
    fn transition_lock_suppresses_reverse_within_window() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);
        let after_hold = base + Duration::from_millis(2000);
        lifecycle.tick(Some(AgentLifecycleInput::Prompt), after_hold);
        assert_eq!(lifecycle.state, AgentLifecycleState::Waiting);

        let too_soon = after_hold + Duration::from_millis(200);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), too_soon);
        assert_eq!(lifecycle.state, AgentLifecycleState::Waiting);
    }

    #[test]
    fn transition_lock_allows_reverse_after_window() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);
        let after_hold = base + Duration::from_millis(2000);
        lifecycle.tick(Some(AgentLifecycleInput::Prompt), after_hold);
        assert_eq!(lifecycle.state, AgentLifecycleState::Waiting);

        let after_lock = after_hold + Duration::from_millis(600);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), after_lock);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);
    }

    #[test]
    fn session_state_sequence_reaches_expected_final_state() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);

        let t0 = base;
        lifecycle.tick(AgentLifecycleState::from_session_state("responding"), t0);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);

        let t1 = t0 + Duration::from_millis(2000);
        lifecycle.tick(AgentLifecycleState::from_session_state("needsInput"), t1);
        assert_eq!(lifecycle.state, AgentLifecycleState::Waiting);

        let t2 = t1 + Duration::from_millis(600);
        lifecycle.tick(AgentLifecycleState::from_session_state("responding"), t2);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);

        let t3 = t2 + Duration::from_millis(2000);
        lifecycle.tick(AgentLifecycleState::from_session_state("idle"), t3);
        assert_eq!(lifecycle.state, AgentLifecycleState::Completed);

        let t4 = t3 + Duration::from_millis(3000);
        lifecycle.tick(AgentLifecycleState::from_session_state("idle"), t4);
        assert_eq!(lifecycle.state, AgentLifecycleState::Idle);
    }
}
