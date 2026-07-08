use super::ai_runtime_status::AgentLifecycleState;
use super::{CoduxApp, ProjectInfo, WorktreeInfo};
use codux_runtime::ai_runtime::{TerminalStatusEvent, TerminalStatusState};
use std::collections::HashSet;
use std::time::{Duration, Instant};

// A crashed/killed CLI never sends its closing OSC; give the runtime probes
// this long to confirm a live turn before garbage-collecting Working/Waiting.
const STALE_ACTIVE_GRACE: Duration = Duration::from_secs(30);

pub(in crate::app) struct PaneAgentLifecycle {
    pub state: AgentLifecycleState,
    updated_at: Instant,
    from_command_osc: bool,
}

impl PaneAgentLifecycle {
    pub(in crate::app) fn new() -> Self {
        Self {
            state: AgentLifecycleState::Idle,
            updated_at: Instant::now(),
            from_command_osc: false,
        }
    }

    pub(in crate::app) fn apply_status(&mut self, state: AgentLifecycleState, from_command_osc: bool) {
        self.updated_at = Instant::now();
        self.from_command_osc = from_command_osc;
        if self.state == state {
            return;
        }
        self.state = state;
    }

    pub(in crate::app) fn dismiss_completed(&mut self) -> bool {
        if self.state != AgentLifecycleState::Completed {
            return false;
        }
        self.state = AgentLifecycleState::Idle;
        self.updated_at = Instant::now();
        true
    }

    fn is_stale_active(&self, has_live_turn: bool, now: Instant) -> bool {
        // Command-level (OSC 133) busy has no AI turn to cross-check; its
        // lifecycle ends with the shell's D mark or the terminal pruning.
        if self.from_command_osc {
            return false;
        }
        matches!(
            self.state,
            AgentLifecycleState::Working | AgentLifecycleState::Waiting
        ) && !has_live_turn
            && now.duration_since(self.updated_at) >= STALE_ACTIVE_GRACE
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
            AgentLifecycleState::Error => return Some(AgentLifecycleState::Error),
            AgentLifecycleState::Waiting => has_waiting = true,
            AgentLifecycleState::Warning => has_waiting = true,
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
    pub(in crate::app) fn apply_terminal_status_events(
        &mut self,
        events: &[codux_runtime::ai_runtime::AIRuntimeSupervisorEvent],
    ) -> bool {
        let mut changed = false;
        for event in events {
            let codux_runtime::ai_runtime::AIRuntimeSupervisorEvent::TerminalStatus { status } =
                event
            else {
                continue;
            };
            if self.apply_terminal_status_event(status) {
                changed = true;
            }
        }
        changed
    }

    // Status pushed by a viewed remote host; same apply path as local events,
    // and the shared prune keeps only terminals the current view still shows.
    pub(in crate::app) fn apply_remote_terminal_status_payloads(
        &mut self,
        payloads: &[serde_json::Value],
    ) -> bool {
        let mut changed = false;
        for payload in payloads {
            let Ok(status) = serde_json::from_value::<TerminalStatusEvent>(payload.clone()) else {
                continue;
            };
            if self.apply_terminal_status_event(&status) {
                changed = true;
            }
        }
        changed
    }

    fn apply_terminal_status_event(&mut self, status: &TerminalStatusEvent) -> bool {
        let Some(next) = agent_lifecycle_state_for_terminal_status(status.state) else {
            return self.clear_pane_agent_lifecycle(&status.terminal_id);
        };
        let terminal_id = status.terminal_id.clone();
        let existed_before = self.pane_agent_lifecycle.contains_key(&terminal_id);
        let entry = self
            .pane_agent_lifecycle
            .entry(terminal_id.clone())
            .or_insert_with(PaneAgentLifecycle::new);
        let previous = entry.state;
        entry.apply_status(
            next,
            status.source == codux_runtime::ai_runtime::TERMINAL_COMMAND_OSC_SOURCE,
        );
        let changed = pane_lifecycle_sync_entry_changed(existed_before, previous, entry.state);
        if changed {
            codux_runtime::runtime_trace::runtime_trace(
                "agent-lifecycle",
                &format!(
                    "terminal={} {:?}->{:?} status_source={}",
                    status.terminal_id, previous, entry.state, status.source
                ),
            );
        }
        changed
    }

    fn clear_pane_agent_lifecycle(&mut self, terminal_id: &str) -> bool {
        self.pane_agent_lifecycle
            .remove(terminal_id)
            .map(|entry| pane_lifecycle_prune_changed(entry.state))
            .unwrap_or(false)
    }

    pub(in crate::app) fn dismiss_pane_agent_lifecycle_completion(
        &mut self,
        terminal_id: &str,
    ) -> bool {
        let Some(entry) = self.pane_agent_lifecycle.get_mut(terminal_id) else {
            return false;
        };
        let changed = entry.dismiss_completed();
        if changed {
            codux_runtime::runtime_trace::runtime_trace(
                "agent-lifecycle",
                &format!("terminal={terminal_id} Completed->Idle dismissed"),
            );
        }
        changed
    }

    pub(in crate::app) fn dismiss_worktree_pane_agent_lifecycle_completion(
        &mut self,
        worktree_id: &str,
    ) -> bool {
        let terminal_ids = self.terminal_ids_for_worktree_id(worktree_id);
        let mut changed = false;
        for terminal_id in terminal_ids {
            changed |= self.dismiss_pane_agent_lifecycle_completion(&terminal_id);
        }
        changed
    }

    pub(in crate::app) fn worktree_agent_lifecycle(
        &self,
        worktree: &WorktreeInfo,
    ) -> Option<AgentLifecycleState> {
        self.aggregate_terminal_lifecycle(self.terminal_ids_for_worktree(worktree))
    }

    pub(in crate::app) fn project_agent_lifecycle(
        &self,
        project: &ProjectInfo,
    ) -> Option<AgentLifecycleState> {
        self.aggregate_terminal_lifecycle(self.terminal_ids_for_project(project))
    }

    pub(in crate::app) fn any_pane_agent_working(&self) -> bool {
        self.pane_agent_lifecycle
            .values()
            .any(|lifecycle| lifecycle.state == AgentLifecycleState::Working)
    }

    // Force the dot-bearing views to repaint so their ping re-reads its phase;
    // bypasses the snapshot-diff skip since only the pulse phase moves. GPUI
    // replays the (unchanged) terminal, so this is just the small dot views.
    pub(in crate::app) fn pulse_agent_dots(&self, cx: &mut gpui::Context<Self>) {
        if let Some(view) = &self.task_worktree_list_view {
            view.update(cx, |_, cx| cx.notify());
        }
        if let Some(view) = &self.task_terminal_list_view {
            view.update(cx, |_, cx| cx.notify());
        }
        if let Some(view) = &self.project_column_view {
            view.update(cx, |_, cx| cx.notify());
        }
    }

    pub(in crate::app) fn sync_pane_agent_lifecycle(&mut self) -> bool {
        let mut changed = false;
        // OSC status is one-shot, so entries must survive layout switches: keep
        // every live host terminal, not just the visible layout, or background
        // projects lose their dots the tick after switching away.
        let mut retained_terminal_ids = self.active_terminal_lifecycle_ids();
        for session in &self.state.terminal_runtime.sessions {
            let terminal_id = session.terminal_id.trim();
            if !terminal_id.is_empty() {
                retained_terminal_ids.insert(terminal_id.to_string());
            }
        }
        let live_turn_terminal_ids: HashSet<&str> = self
            .state
            .ai_runtime_state
            .sessions
            .iter()
            .filter(|session| {
                matches!(
                    session.state.as_str(),
                    "running" | "responding" | "needs-input" | "needsInput"
                )
            })
            .map(|session| session.terminal_id.as_str())
            .collect();
        let now = Instant::now();
        self.pane_agent_lifecycle.retain(|terminal_id, entry| {
            if !retained_terminal_ids.contains(terminal_id) {
                if pane_lifecycle_prune_changed(entry.state) {
                    codux_runtime::runtime_trace::runtime_trace(
                        "agent-lifecycle",
                        &format!("terminal={terminal_id} pruned was={:?}", entry.state),
                    );
                    changed = true;
                }
                return false;
            }
            if entry.is_stale_active(live_turn_terminal_ids.contains(terminal_id.as_str()), now) {
                codux_runtime::runtime_trace::runtime_trace(
                    "agent-lifecycle",
                    &format!("terminal={terminal_id} stale {:?} cleared", entry.state),
                );
                changed = true;
                return false;
            }
            true
        });
        changed
    }

    fn active_terminal_lifecycle_ids(&self) -> HashSet<String> {
        let mut ids = HashSet::new();
        if let Some(tab) = self.main_terminal() {
            for (index, slot) in tab.panes.iter().enumerate() {
                if let Some(id) = Self::terminal_slot_terminal_id(tab, index, slot) {
                    ids.insert(id);
                }
            }
        }
        for slot in &self.collapsed_terminal_panes {
            if let Some(id) = slot.terminal_id.as_ref().filter(|id| !id.trim().is_empty()) {
                ids.insert(id.clone());
            }
        }
        ids
    }

    fn aggregate_terminal_lifecycle(
        &self,
        terminal_ids: impl IntoIterator<Item = String>,
    ) -> Option<AgentLifecycleState> {
        aggregate_agent_lifecycle(terminal_ids.into_iter().filter_map(|terminal_id| {
            self.pane_agent_lifecycle
                .get(&terminal_id)
                .map(|lifecycle| lifecycle.state)
        }))
    }

    fn terminal_ids_for_project(&self, project: &ProjectInfo) -> Vec<String> {
        let mut ids = Vec::new();
        for session in &self.state.terminal_runtime.sessions {
            if session.project_id == project.id {
                push_unique_terminal_id(&mut ids, &session.terminal_id);
            }
        }
        for worktree in self
            .state
            .worktrees
            .worktrees
            .iter()
            .filter(|worktree| worktree.project_id == project.id)
        {
            for terminal_id in self.terminal_ids_for_worktree(worktree) {
                push_unique_terminal_id(&mut ids, &terminal_id);
            }
        }
        ids
    }

    fn terminal_ids_for_worktree_id(&self, worktree_id: &str) -> Vec<String> {
        self.state
            .worktrees
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .map(|worktree| self.terminal_ids_for_worktree(worktree))
            .unwrap_or_default()
    }

    fn terminal_ids_for_worktree(&self, worktree: &WorktreeInfo) -> Vec<String> {
        let mut ids = Vec::new();
        for session in &self.state.terminal_runtime.sessions {
            if terminal_runtime_session_belongs_to_worktree(session.project_id.as_str(), worktree) {
                push_unique_terminal_id(&mut ids, &session.terminal_id);
            }
        }
        if super::ai_runtime_status::terminal_layout_owner_id(&self.state).as_deref()
            == Some(worktree.id.as_str())
        {
            for terminal_id in self.active_terminal_lifecycle_ids() {
                push_unique_terminal_id(&mut ids, &terminal_id);
            }
        }
        ids
    }
}

fn terminal_runtime_session_belongs_to_worktree(project_id: &str, worktree: &WorktreeInfo) -> bool {
    project_id == worktree.id || (worktree.is_default && project_id == worktree.project_id)
}

fn push_unique_terminal_id(ids: &mut Vec<String>, terminal_id: &str) {
    let terminal_id = terminal_id.trim();
    if terminal_id.is_empty() || ids.iter().any(|id| id == terminal_id) {
        return;
    }
    ids.push(terminal_id.to_string());
}

fn agent_lifecycle_state_for_terminal_status(
    state: TerminalStatusState,
) -> Option<AgentLifecycleState> {
    match state {
        TerminalStatusState::Idle => None,
        TerminalStatusState::Working => Some(AgentLifecycleState::Working),
        TerminalStatusState::Waiting => Some(AgentLifecycleState::Waiting),
        TerminalStatusState::Completed => Some(AgentLifecycleState::Completed),
        TerminalStatusState::Error => Some(AgentLifecycleState::Error),
        TerminalStatusState::Warning => Some(AgentLifecycleState::Warning),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_lifecycle_applies_status_directly() {
        let mut lifecycle = PaneAgentLifecycle::new();

        lifecycle.apply_status(AgentLifecycleState::Working, false);
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);

        lifecycle.apply_status(AgentLifecycleState::Completed, false);
        assert_eq!(lifecycle.state, AgentLifecycleState::Completed);
    }

    #[test]
    fn completed_status_stays_until_dismissed() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status(AgentLifecycleState::Completed, false);

        assert!(lifecycle.dismiss_completed());
        assert_eq!(lifecycle.state, AgentLifecycleState::Idle);
    }

    #[test]
    fn dismiss_completed_ignores_non_completed_state() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status(AgentLifecycleState::Working, false);

        assert!(!lifecycle.dismiss_completed());
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);
    }

    #[test]
    fn completed_status_is_overwritten_by_following_loading() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status(AgentLifecycleState::Completed, false);
        lifecycle.apply_status(AgentLifecycleState::Working, false);

        assert_eq!(lifecycle.state, AgentLifecycleState::Working);
    }

    #[test]
    fn sync_prune_changed_for_non_idle() {
        assert!(pane_lifecycle_prune_changed(AgentLifecycleState::Working));
        assert!(!pane_lifecycle_prune_changed(AgentLifecycleState::Idle));
    }

    #[test]
    fn stale_working_is_collected_only_without_live_turn_after_grace() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status(AgentLifecycleState::Working, false);
        let after_grace = lifecycle.updated_at + STALE_ACTIVE_GRACE;

        assert!(!lifecycle.is_stale_active(false, lifecycle.updated_at));
        assert!(!lifecycle.is_stale_active(true, after_grace));
        assert!(lifecycle.is_stale_active(false, after_grace));
    }

    #[test]
    fn command_osc_working_is_exempt_from_stale_collection() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status(AgentLifecycleState::Working, true);

        assert!(!lifecycle.is_stale_active(false, lifecycle.updated_at + STALE_ACTIVE_GRACE));

        // A later AI-sourced event re-enters the normal stale rules.
        lifecycle.apply_status(AgentLifecycleState::Working, false);
        assert!(lifecycle.is_stale_active(false, lifecycle.updated_at + STALE_ACTIVE_GRACE));
    }

    #[test]
    fn stale_collection_skips_terminal_states() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status(AgentLifecycleState::Completed, false);
        let after_grace = lifecycle.updated_at + STALE_ACTIVE_GRACE;

        assert!(!lifecycle.is_stale_active(false, after_grace));

        lifecycle.apply_status(AgentLifecycleState::Waiting, false);
        assert!(lifecycle.is_stale_active(false, lifecycle.updated_at + STALE_ACTIVE_GRACE));
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

    #[test]
    fn default_worktree_accepts_root_project_terminal_runtime_sessions() {
        let worktree = WorktreeInfo {
            id: "project-a".to_string(),
            project_id: "project-a".to_string(),
            name: "main".to_string(),
            branch: "main".to_string(),
            path: "/tmp/project-a".to_string(),
            status: "active".to_string(),
            is_default: true,
            exists: true,
            git_summary: Default::default(),
        };

        assert!(terminal_runtime_session_belongs_to_worktree(
            "project-a",
            &worktree
        ));
    }

    #[test]
    fn linked_worktree_accepts_own_terminal_runtime_sessions() {
        let worktree = WorktreeInfo {
            id: "worktree-a".to_string(),
            project_id: "project-a".to_string(),
            name: "feature".to_string(),
            branch: "feature".to_string(),
            path: "/tmp/project-a-feature".to_string(),
            status: "active".to_string(),
            is_default: false,
            exists: true,
            git_summary: Default::default(),
        };

        assert!(terminal_runtime_session_belongs_to_worktree(
            "worktree-a",
            &worktree
        ));
        assert!(!terminal_runtime_session_belongs_to_worktree(
            "project-a",
            &worktree
        ));
    }
}
