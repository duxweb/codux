use super::ai_runtime_status::AgentLifecycleState;
use super::{CoduxApp, ProjectInfo, WorktreeInfo};
use crate::terminal::TerminalPane;
use codux_runtime::ai_runtime::{TerminalStatusEvent, TerminalStatusState};

pub(in crate::app) struct PaneAgentLifecycle {
    pub state: AgentLifecycleState,
    project_id: Option<String>,
    worktree_id: Option<String>,
    terminal_instance_id: Option<String>,
    updated_at: f64,
    remote_device_id: Option<String>,
}

impl PaneAgentLifecycle {
    pub(in crate::app) fn new() -> Self {
        Self {
            state: AgentLifecycleState::Idle,
            project_id: None,
            worktree_id: None,
            terminal_instance_id: None,
            updated_at: 0.0,
            remote_device_id: None,
        }
    }

    fn apply_status_event(
        &mut self,
        state: AgentLifecycleState,
        project_id: Option<String>,
        worktree_id: Option<String>,
        terminal_instance_id: Option<String>,
        updated_at: f64,
        remote_device_id: Option<String>,
    ) {
        self.project_id = project_id;
        self.worktree_id = worktree_id;
        self.terminal_instance_id = terminal_instance_id;
        self.updated_at = updated_at;
        self.remote_device_id = remote_device_id;
        if self.state == state {
            return;
        }
        self.state = state;
    }

    fn accepts_status_event(&self, terminal_instance_id: Option<&str>, updated_at: f64) -> bool {
        if self.terminal_instance_id.as_deref() == terminal_instance_id {
            return self.updated_at <= 0.0 || (updated_at > 0.0 && updated_at >= self.updated_at);
        }
        updated_at > self.updated_at
    }

    pub(in crate::app) fn dismiss_completed(&mut self) -> bool {
        if self.state != AgentLifecycleState::Completed {
            return false;
        }
        self.state = AgentLifecycleState::Idle;
        true
    }
}

fn pane_lifecycle_entry_changed(
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

fn pane_lifecycle_removal_changed(state: AgentLifecycleState) -> bool {
    state != AgentLifecycleState::Idle
}

pub(in crate::app) fn aggregate_agent_lifecycle(
    states: impl Iterator<Item = AgentLifecycleState>,
) -> Option<AgentLifecycleState> {
    let mut has_error = false;
    let mut has_working = false;
    let mut has_waiting = false;
    let mut has_completed = false;
    for state in states {
        match state {
            AgentLifecycleState::Error => has_error = true,
            AgentLifecycleState::Working => has_working = true,
            AgentLifecycleState::Waiting => has_waiting = true,
            AgentLifecycleState::Warning => has_waiting = true,
            AgentLifecycleState::Completed => has_completed = true,
            AgentLifecycleState::Idle => {}
        }
    }
    if has_error {
        Some(AgentLifecycleState::Error)
    } else if has_working {
        Some(AgentLifecycleState::Working)
    } else if has_waiting {
        Some(AgentLifecycleState::Waiting)
    } else if has_completed {
        Some(AgentLifecycleState::Completed)
    } else {
        None
    }
}

impl CoduxApp {
    pub(in crate::app) fn any_pane_agent_working(&self) -> bool {
        self.pane_agent_lifecycle
            .values()
            .any(|lifecycle| lifecycle.state == AgentLifecycleState::Working)
    }

    pub(in crate::app) fn ensure_agent_pulse(&mut self, cx: &mut gpui::Context<Self>) {
        if self.agent_pulse_active
            || !self.any_pane_agent_working()
            || super::agent_display::reduce_motion_enabled()
        {
            return;
        }
        self.agent_pulse_active = true;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                timer.timer(std::time::Duration::from_millis(33)).await;
                let keep_running = this
                    .update(cx, |app, cx| {
                        if app.window_mode != super::AppWindowMode::Main
                            || !app.any_pane_agent_working()
                        {
                            app.agent_pulse_active = false;
                            return false;
                        }
                        if let Some(view) = &app.task_worktree_list_view {
                            view.update(cx, |_, cx| cx.notify());
                        }
                        if let Some(view) = &app.task_terminal_list_view {
                            view.update(cx, |_, cx| cx.notify());
                        }
                        if let Some(view) = &app.project_column_view {
                            view.update(cx, |_, cx| cx.notify());
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::app) fn drain_and_apply_terminal_lifecycle_events(
        &mut self,
        events: &[codux_runtime::ai_runtime::AIRuntimeSupervisorEvent],
    ) -> bool {
        let disconnected_devices = self.runtime_service.drain_disconnected_remote_devices();
        let remote_statuses = self.runtime_service.drain_remote_terminal_status();
        let mut changed = self.clear_remote_agent_lifecycle_for_devices(&disconnected_devices);
        changed |= self.apply_terminal_status_events(events);
        changed |= self.apply_remote_terminal_status_payloads(&remote_statuses);
        changed
    }

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
            if self.apply_terminal_status_event(status, None) {
                changed = true;
            }
        }
        changed
    }

    // Status pushed by a viewed remote host; same apply path as local events.
    pub(in crate::app) fn apply_remote_terminal_status_payloads(
        &mut self,
        payloads: &[(String, serde_json::Value)],
    ) -> bool {
        let mut changed = false;
        for (device_id, payload) in payloads {
            let Ok(status) = serde_json::from_value::<TerminalStatusEvent>(payload.clone()) else {
                continue;
            };
            if self.apply_terminal_status_event(&status, Some(device_id)) {
                changed = true;
            }
        }
        changed
    }

    fn apply_terminal_status_event(
        &mut self,
        status: &TerminalStatusEvent,
        remote_device_id: Option<&str>,
    ) -> bool {
        let terminal_instance_id = normalized_status_id(status.terminal_instance_id.as_deref());
        let current_instance_id = self
            .terminal_pane_registry
            .get(&status.terminal_id)
            .and_then(TerminalPane::terminal_instance_id);
        if !status_matches_terminal_instance(
            terminal_instance_id.as_deref(),
            current_instance_id.as_deref(),
        ) {
            return false;
        }
        if self
            .pane_agent_lifecycle
            .get(&status.terminal_id)
            .is_some_and(|existing| {
                !existing.accepts_status_event(terminal_instance_id.as_deref(), status.updated_at)
            })
        {
            return false;
        }
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
        entry.apply_status_event(
            next,
            normalized_status_id(status.project_id.as_deref()),
            normalized_status_id(status.worktree_id.as_deref()),
            terminal_instance_id,
            status.updated_at,
            remote_device_id.map(str::to_string),
        );
        let changed = pane_lifecycle_entry_changed(existed_before, previous, entry.state);
        if changed {
            codux_runtime::runtime_trace::runtime_trace(
                "agent-lifecycle",
                &format!(
                    "terminal={} {:?}->{:?} status_source={} project={:?} worktree={:?}",
                    status.terminal_id,
                    previous,
                    entry.state,
                    status.source,
                    entry.project_id,
                    entry.worktree_id
                ),
            );
        }
        changed
    }

    pub(in crate::app) fn clear_remote_agent_lifecycle_for_devices(
        &mut self,
        device_ids: &[String],
    ) -> bool {
        if device_ids.is_empty() {
            return false;
        }
        let terminal_ids = self
            .pane_agent_lifecycle
            .iter()
            .filter(|&(_terminal_id, lifecycle)| {
                lifecycle
                    .remote_device_id
                    .as_deref()
                    .is_some_and(|device_id| {
                        device_ids.iter().any(|candidate| candidate == device_id)
                    })
            })
            .map(|(terminal_id, _lifecycle)| terminal_id.clone())
            .collect::<Vec<_>>();
        let mut changed = false;
        for terminal_id in terminal_ids {
            changed |= self.clear_pane_agent_lifecycle(&terminal_id);
        }
        changed
    }

    pub(in crate::app) fn clear_pane_agent_lifecycle(&mut self, terminal_id: &str) -> bool {
        self.pane_agent_lifecycle
            .remove(terminal_id)
            .map(|entry| pane_lifecycle_removal_changed(entry.state))
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
        aggregate_agent_lifecycle(
            self.pane_agent_lifecycle
                .values()
                .filter(|lifecycle| lifecycle_belongs_to_worktree(lifecycle, worktree))
                .map(|lifecycle| lifecycle.state),
        )
    }

    pub(in crate::app) fn project_agent_lifecycle(
        &self,
        project: &ProjectInfo,
    ) -> Option<AgentLifecycleState> {
        aggregate_agent_lifecycle(
            self.pane_agent_lifecycle
                .values()
                .filter(|lifecycle| lifecycle.project_id.as_deref() == Some(project.id.as_str()))
                .map(|lifecycle| lifecycle.state),
        )
    }

    fn terminal_ids_for_worktree_id(&self, worktree_id: &str) -> Vec<String> {
        self.state
            .worktrees
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .map(|worktree| {
                self.pane_agent_lifecycle
                    .iter()
                    .filter(|(_, lifecycle)| lifecycle_belongs_to_worktree(lifecycle, worktree))
                    .map(|(terminal_id, _)| terminal_id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn lifecycle_belongs_to_worktree(lifecycle: &PaneAgentLifecycle, worktree: &WorktreeInfo) -> bool {
    lifecycle.worktree_id.as_deref() == Some(worktree.id.as_str())
}

fn normalized_status_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn status_matches_terminal_instance(
    event_instance_id: Option<&str>,
    current_instance_id: Option<&str>,
) -> bool {
    current_instance_id.is_none() || event_instance_id == current_instance_id
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

    fn lifecycle(
        state: AgentLifecycleState,
        project_id: Option<&str>,
        worktree_id: Option<&str>,
    ) -> PaneAgentLifecycle {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status_event(
            state,
            project_id.map(str::to_string),
            worktree_id.map(str::to_string),
            None,
            0.0,
            None,
        );
        lifecycle
    }

    #[test]
    fn pane_lifecycle_applies_status_directly() {
        let mut lifecycle = PaneAgentLifecycle::new();

        lifecycle.apply_status_event(
            AgentLifecycleState::Working,
            Some("project-a".to_string()),
            Some("worktree-a".to_string()),
            None,
            0.0,
            None,
        );
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);
        assert_eq!(lifecycle.project_id.as_deref(), Some("project-a"));
        assert_eq!(lifecycle.worktree_id.as_deref(), Some("worktree-a"));

        lifecycle.apply_status_event(
            AgentLifecycleState::Completed,
            Some("project-a".to_string()),
            Some("worktree-a".to_string()),
            None,
            0.0,
            None,
        );
        assert_eq!(lifecycle.state, AgentLifecycleState::Completed);
    }

    #[test]
    fn completed_status_stays_until_dismissed() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status_event(AgentLifecycleState::Completed, None, None, None, 0.0, None);

        assert!(lifecycle.dismiss_completed());
        assert_eq!(lifecycle.state, AgentLifecycleState::Idle);
    }

    #[test]
    fn dismiss_completed_ignores_non_completed_state() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status_event(AgentLifecycleState::Working, None, None, None, 0.0, None);

        assert!(!lifecycle.dismiss_completed());
        assert_eq!(lifecycle.state, AgentLifecycleState::Working);
    }

    #[test]
    fn completed_status_is_overwritten_by_following_loading() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status_event(AgentLifecycleState::Completed, None, None, None, 0.0, None);
        lifecycle.apply_status_event(AgentLifecycleState::Working, None, None, None, 0.0, None);

        assert_eq!(lifecycle.state, AgentLifecycleState::Working);
    }

    #[test]
    fn removal_changed_for_non_idle() {
        assert!(pane_lifecycle_removal_changed(AgentLifecycleState::Working));
        assert!(!pane_lifecycle_removal_changed(AgentLifecycleState::Idle));
    }

    #[test]
    fn lifecycle_rejects_older_event_for_same_terminal_instance() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status_event(
            AgentLifecycleState::Working,
            Some("project-a".to_string()),
            Some("worktree-a".to_string()),
            Some("instance-2".to_string()),
            20.0,
            None,
        );

        assert!(!lifecycle.accepts_status_event(Some("instance-2"), 19.0));
        assert!(lifecycle.accepts_status_event(Some("instance-2"), 20.0));
        assert!(lifecycle.accepts_status_event(Some("instance-2"), 21.0));
    }

    #[test]
    fn lifecycle_rejects_old_terminal_instance_event() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status_event(
            AgentLifecycleState::Working,
            Some("project-a".to_string()),
            Some("worktree-a".to_string()),
            Some("instance-2".to_string()),
            20.0,
            None,
        );

        assert!(!lifecycle.accepts_status_event(Some("instance-1"), 20.0));
        assert!(!lifecycle.accepts_status_event(Some("instance-1"), 19.0));
        assert!(lifecycle.accepts_status_event(Some("instance-3"), 21.0));
    }

    #[test]
    fn status_must_match_mounted_terminal_instance() {
        assert!(status_matches_terminal_instance(
            Some("instance-2"),
            Some("instance-2")
        ));
        assert!(!status_matches_terminal_instance(
            Some("instance-1"),
            Some("instance-2")
        ));
        assert!(!status_matches_terminal_instance(None, Some("instance-2")));
        assert!(status_matches_terminal_instance(Some("instance-1"), None));
    }

    #[test]
    fn remote_status_records_its_source_device() {
        let mut lifecycle = PaneAgentLifecycle::new();
        lifecycle.apply_status_event(
            AgentLifecycleState::Working,
            None,
            None,
            Some("instance-1".to_string()),
            10.0,
            Some("device-1".to_string()),
        );

        assert_eq!(lifecycle.remote_device_id.as_deref(), Some("device-1"));
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
    fn aggregate_agent_lifecycle_prefers_error_regardless_of_order() {
        for states in [
            [AgentLifecycleState::Working, AgentLifecycleState::Error],
            [AgentLifecycleState::Error, AgentLifecycleState::Working],
        ] {
            assert_eq!(
                aggregate_agent_lifecycle(states.into_iter()),
                Some(AgentLifecycleState::Error)
            );
        }
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
    fn worktree_requires_matching_worktree_lifecycle() {
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
        let linked_lifecycle = lifecycle(
            AgentLifecycleState::Working,
            Some("project-a"),
            Some("worktree-a"),
        );
        let other_lifecycle = lifecycle(
            AgentLifecycleState::Working,
            Some("project-a"),
            Some("project-a"),
        );
        let unscoped_lifecycle = lifecycle(AgentLifecycleState::Working, Some("project-a"), None);

        assert!(lifecycle_belongs_to_worktree(&linked_lifecycle, &worktree));
        assert!(!lifecycle_belongs_to_worktree(&other_lifecycle, &worktree));
        assert!(!lifecycle_belongs_to_worktree(
            &unscoped_lifecycle,
            &worktree
        ));
    }

    #[test]
    fn project_lifecycle_aggregates_terminal_entries_from_all_worktrees() {
        let project_id = "project-a";
        let entries = [
            lifecycle(
                AgentLifecycleState::Completed,
                Some(project_id),
                Some(project_id),
            ),
            lifecycle(
                AgentLifecycleState::Working,
                Some(project_id),
                Some("worktree-a"),
            ),
            lifecycle(
                AgentLifecycleState::Waiting,
                Some("project-b"),
                Some("project-b"),
            ),
        ];

        let lifecycle = aggregate_agent_lifecycle(
            entries
                .iter()
                .filter(|entry| entry.project_id.as_deref() == Some(project_id))
                .map(|entry| entry.state),
        );

        assert_eq!(lifecycle, Some(AgentLifecycleState::Working));
    }

    #[test]
    fn worktree_lifecycle_aggregates_only_its_terminal_entries() {
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
        let entries = [
            lifecycle(
                AgentLifecycleState::Working,
                Some("project-a"),
                Some("project-a"),
            ),
            lifecycle(
                AgentLifecycleState::Completed,
                Some("project-a"),
                Some("worktree-a"),
            ),
        ];

        let lifecycle = aggregate_agent_lifecycle(
            entries
                .iter()
                .filter(|entry| lifecycle_belongs_to_worktree(entry, &worktree))
                .map(|entry| entry.state),
        );

        assert_eq!(lifecycle, Some(AgentLifecycleState::Completed));
    }
}
