use super::*;
use codux_runtime::ai_runtime_state::{AIRuntimeProjectPhaseSummary, AIRuntimeProjectStateSummary};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum AIActivityState {
    Idle,
    Running,
    Review,
    Done,
}

impl AIActivityState {
    pub(in crate::app) fn is_active(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

pub(in crate::app) fn selected_worktree_info(state: &RuntimeState) -> Option<WorktreeInfo> {
    let selected_id = state.worktrees.selected_worktree_id.as_deref();
    selected_id
        .and_then(|id| {
            state
                .worktrees
                .worktrees
                .iter()
                .find(|worktree| worktree.id == id)
        })
        .or_else(|| state.worktrees.worktrees.first())
        .cloned()
}

pub(in crate::app) fn terminal_layout_owner_id(state: &RuntimeState) -> Option<String> {
    selected_worktree_info(state)
        .map(|worktree| worktree.id)
        .or_else(|| {
            state
                .selected_project
                .as_ref()
                .map(|project| project.id.clone())
        })
}

impl CoduxApp {
    pub(in crate::app) fn ai_activity_for_worktree(
        &self,
        worktree: &WorktreeInfo,
    ) -> AIActivityState {
        let dismissed_at = self
            .dismissed_worktree_ai_completion_at
            .get(&worktree.id)
            .copied()
            .unwrap_or(0.0)
            .max(
                self.dismissed_worktree_ai_completion_at
                    .get(&worktree.project_id)
                    .copied()
                    .unwrap_or(0.0),
            );
        ai_activity_for_worktree_with_dismissed_at(&self.state, worktree, dismissed_at)
    }
}

pub(in crate::app) fn ai_activity_for_worktree_with_dismissed_at(
    state: &RuntimeState,
    worktree: &WorktreeInfo,
    dismissed_at: f64,
) -> AIActivityState {
    let Some(project_state) = runtime_project_state_for_worktree(state, worktree) else {
        return AIActivityState::Idle;
    };
    let phase =
        resolve_displayed_phase(&project_state.project_phase, &project_state.completed_phase);
    if phase.kind == "completed" && phase.updated_at <= dismissed_at {
        return AIActivityState::Idle;
    }
    phase_to_activity(phase)
}

pub(in crate::app) fn ai_activity_for_project_with_worktree_activity(
    state: &RuntimeState,
    project: &ProjectInfo,
    worktree_activity: &HashMap<String, AIActivityState>,
) -> AIActivityState {
    aggregate_project_activity(state, project, worktree_activity).unwrap_or(AIActivityState::Idle)
}

fn ai_activity_for_id(state: &RuntimeState, id: &str) -> Option<AIActivityState> {
    let project_state = runtime_project_state(state, id)?;
    Some(phase_to_activity(resolve_displayed_phase(
        &project_state.project_phase,
        &project_state.completed_phase,
    )))
}

fn aggregate_project_activity(
    state: &RuntimeState,
    project: &ProjectInfo,
    worktree_activity: &HashMap<String, AIActivityState>,
) -> Option<AIActivityState> {
    let mut activities =
        vec![ai_activity_for_id(state, &project.id).unwrap_or(AIActivityState::Idle)];
    for worktree in state
        .worktrees
        .worktrees
        .iter()
        .filter(|worktree| worktree.project_id == project.id)
    {
        let activity = worktree_activity
            .get(&worktree.id)
            .copied()
            .or_else(|| ai_activity_for_id(state, &worktree.id))
            .unwrap_or(AIActivityState::Idle);
        activities.push(activity);
    }
    if activities.contains(&AIActivityState::Review) {
        return Some(AIActivityState::Review);
    }
    if activities.contains(&AIActivityState::Running) {
        return Some(AIActivityState::Running);
    }
    if activities.contains(&AIActivityState::Done) {
        return Some(AIActivityState::Done);
    }
    Some(AIActivityState::Idle)
}

fn runtime_project_state<'a>(
    state: &'a RuntimeState,
    id: &str,
) -> Option<&'a AIRuntimeProjectStateSummary> {
    state
        .ai_runtime_state
        .project_states
        .iter()
        .find(|project| project.project_id == id)
}

fn runtime_project_state_for_worktree<'a>(
    state: &'a RuntimeState,
    worktree: &WorktreeInfo,
) -> Option<&'a AIRuntimeProjectStateSummary> {
    runtime_project_state(state, &worktree.id).or_else(|| {
        if worktree.is_default {
            runtime_project_state(state, &worktree.project_id)
        } else {
            None
        }
    })
}

fn resolve_displayed_phase<'a>(
    project_phase: &'a AIRuntimeProjectPhaseSummary,
    completed_phase: &'a AIRuntimeProjectPhaseSummary,
) -> &'a AIRuntimeProjectPhaseSummary {
    if is_idle_phase(project_phase) {
        completed_phase
    } else {
        project_phase
    }
}

fn phase_to_activity(phase: &AIRuntimeProjectPhaseSummary) -> AIActivityState {
    match phase.kind.as_str() {
        "needsInput" => AIActivityState::Review,
        "running" => AIActivityState::Running,
        "completed" => AIActivityState::Done,
        _ => AIActivityState::Idle,
    }
}

fn is_idle_phase(phase: &AIRuntimeProjectPhaseSummary) -> bool {
    phase.kind.is_empty() || phase.kind == "idle"
}
