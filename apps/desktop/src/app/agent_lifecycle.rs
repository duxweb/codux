use super::ai_runtime_status::{AgentLifecycleInput, AgentLifecycleState};
use super::{CoduxApp, WorktreeInfo};
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

fn pane_lifecycle_sync_entry_changed(
    existed_before: bool,
    state_before_tick: AgentLifecycleState,
    state_after_tick: AgentLifecycleState,
) -> bool {
    if existed_before {
        state_before_tick != state_after_tick
    } else {
        state_after_tick != AgentLifecycleState::Idle
    }
}

fn pane_lifecycle_prune_changed(state: AgentLifecycleState) -> bool {
    state != AgentLifecycleState::Idle
}

pub(in crate::app) fn aggregate_agent_lifecycle(
    states: impl Iterator<Item = AgentLifecycleState>,
) -> Option<AgentLifecycleState> {
    let mut has_waiting = false;
    let mut has_completed = false;
    for state in states {
        match state {
            AgentLifecycleState::Working => return Some(AgentLifecycleState::Working),
            AgentLifecycleState::Waiting => has_waiting = true,
            AgentLifecycleState::Completed => has_completed = true,
            AgentLifecycleState::Idle => {}
        }
    }
    if has_waiting {
        Some(AgentLifecycleState::Waiting)
    } else if has_completed {
        Some(AgentLifecycleState::Completed)
    } else {
        None
    }
}

impl CoduxApp {
    pub(in crate::app) fn worktree_agent_lifecycle(
        &self,
        worktree: &WorktreeInfo,
    ) -> Option<AgentLifecycleState> {
        aggregate_agent_lifecycle(
            self.state
                .ai_runtime_state
                .sessions
                .iter()
                .filter(|session| {
                    session.project_id == worktree.id
                        || (worktree.is_default && session.project_id == worktree.project_id)
                })
                .filter_map(|session| {
                    self.pane_agent_lifecycle
                        .get(&session.terminal_id)
                        .map(|lifecycle| lifecycle.state)
                }),
        )
    }

    pub(in crate::app) fn sync_pane_agent_lifecycle(&mut self) -> bool {
        let now = Instant::now();
        let sessions = &self.state.ai_runtime_state.sessions;
        let mut changed = false;

        for session in sessions {
            let input = AgentLifecycleState::from_session_state(&session.state);
            let terminal_id = session.terminal_id.clone();
            let existed_before = self.pane_agent_lifecycle.contains_key(&terminal_id);
            let entry = self
                .pane_agent_lifecycle
                .entry(terminal_id)
                .or_insert_with(|| PaneAgentLifecycle::new(now));
            let state_before_tick = entry.state;
            entry.tick(input, now);
            if pane_lifecycle_sync_entry_changed(existed_before, state_before_tick, entry.state) {
                codux_runtime::runtime_trace::runtime_trace(
                    "agent-lifecycle",
                    &format!(
                        "terminal={} {:?}->{:?} session_state={}",
                        session.terminal_id, state_before_tick, entry.state, session.state
                    ),
                );
                changed = true;
            }
        }

        let active_terminal_ids: HashSet<&str> =
            sessions.iter().map(|session| session.terminal_id.as_str()).collect();
        self.pane_agent_lifecycle.retain(|terminal_id, entry| {
            if active_terminal_ids.contains(terminal_id.as_str()) {
                true
            } else {
                if pane_lifecycle_prune_changed(entry.state) {
                    codux_runtime::runtime_trace::runtime_trace(
                        "agent-lifecycle",
                        &format!("terminal={terminal_id} pruned was={:?}", entry.state),
                    );
                    changed = true;
                }
                false
            }
        });
        changed
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
        lifecycle.tick(AgentLifecycleState::from_session_state("running"), t0);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);

        let t1 = t0 + Duration::from_millis(2000);
        lifecycle.tick(AgentLifecycleState::from_session_state("needs-input"), t1);
        assert_eq!(lifecycle.state, AgentLifecycleState::Waiting);

        let t2 = t1 + Duration::from_millis(600);
        lifecycle.tick(AgentLifecycleState::from_session_state("running"), t2);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);

        let t3 = t2 + Duration::from_millis(2000);
        lifecycle.tick(AgentLifecycleState::from_session_state("idle"), t3);
        assert_eq!(lifecycle.state, AgentLifecycleState::Completed);

        let t4 = t3 + Duration::from_millis(3000);
        lifecycle.tick(AgentLifecycleState::from_session_state("idle"), t4);
        assert_eq!(lifecycle.state, AgentLifecycleState::Idle);
    }

    #[test]
    fn sync_entry_changed_on_session_state_transition() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        let before = lifecycle.state;
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);
        assert!(pane_lifecycle_sync_entry_changed(
            false,
            before,
            lifecycle.state,
        ));
    }

    #[test]
    fn sync_entry_unchanged_on_steady_state() {
        let base = base_instant();
        let mut lifecycle = PaneAgentLifecycle::new(base);
        lifecycle.tick(Some(AgentLifecycleInput::Busy), base);
        let before = lifecycle.state;
        lifecycle.tick(AgentLifecycleState::from_session_state("responding"), base);
        assert!(!pane_lifecycle_sync_entry_changed(
            true,
            before,
            lifecycle.state,
        ));
    }

    #[test]
    fn sync_prune_changed_for_non_idle() {
        assert!(pane_lifecycle_prune_changed(AgentLifecycleState::Working));
        assert!(!pane_lifecycle_prune_changed(AgentLifecycleState::Idle));
    }

    #[test]
    fn aggregate_agent_lifecycle_prefers_working() {
        let states = [
            AgentLifecycleState::Completed,
            AgentLifecycleState::Waiting,
            AgentLifecycleState::Working,
        ];
        assert_eq!(
            aggregate_agent_lifecycle(states.into_iter()),
            Some(AgentLifecycleState::Working)
        );
    }

    #[test]
    fn aggregate_agent_lifecycle_prefers_waiting_over_completed() {
        let states = [
            AgentLifecycleState::Completed,
            AgentLifecycleState::Waiting,
            AgentLifecycleState::Idle,
        ];
        assert_eq!(
            aggregate_agent_lifecycle(states.into_iter()),
            Some(AgentLifecycleState::Waiting)
        );
    }

    #[test]
    fn aggregate_agent_lifecycle_returns_completed_when_only_completed() {
        let states = [AgentLifecycleState::Completed, AgentLifecycleState::Idle];
        assert_eq!(
            aggregate_agent_lifecycle(states.into_iter()),
            Some(AgentLifecycleState::Completed)
        );
    }

    #[test]
    fn aggregate_agent_lifecycle_returns_none_for_empty_or_idle_only() {
        assert_eq!(aggregate_agent_lifecycle([].into_iter()), None);
        assert_eq!(
            aggregate_agent_lifecycle([AgentLifecycleState::Idle].into_iter()),
            None
        );
    }
}
