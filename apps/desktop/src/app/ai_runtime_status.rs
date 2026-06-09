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
    let selected_id = state.worktrees.selected_worktree_id.as_deref()?;
    state
        .worktrees
        .worktrees
        .iter()
        .find(|worktree| worktree.id == selected_id)
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

pub(in crate::app) fn terminal_layout_storage_key(project_id: &str, worktree_id: &str) -> String {
    codux_runtime::terminal_layout::terminal_layout_storage_key(project_id, worktree_id)
}

pub(in crate::app) fn current_terminal_layout_storage_key(state: &RuntimeState) -> Option<String> {
    let project_id = state.selected_project.as_ref()?.id.as_str();
    let worktree_id = terminal_layout_owner_id(state)?;
    Some(terminal_layout_storage_key(project_id, &worktree_id))
}

pub(in crate::app) fn ai_activity_project_states_changed(
    previous: &[AIRuntimeProjectStateSummary],
    next: &[AIRuntimeProjectStateSummary],
) -> bool {
    previous != next
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

    pub(in crate::app) fn ai_activity_for_project(&self, project: &ProjectInfo) -> AIActivityState {
        let dismissed_at = self
            .dismissed_worktree_ai_completion_at
            .get(&project.id)
            .copied()
            .unwrap_or(0.0);
        ai_activity_for_id_with_dismissed_at(&self.state, &project.id, dismissed_at)
            .unwrap_or(AIActivityState::Idle)
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

fn ai_activity_for_id_with_dismissed_at(
    state: &RuntimeState,
    id: &str,
    dismissed_at: f64,
) -> Option<AIActivityState> {
    let project_state = runtime_project_state(state, id)?;
    let phase =
        resolve_displayed_phase(&project_state.project_phase, &project_state.completed_phase);
    if phase.kind == "completed" && phase.updated_at <= dismissed_at {
        return Some(AIActivityState::Idle);
    }
    Some(phase_to_activity(phase))
}

pub(in crate::app) fn aggregate_project_activity(
    project_activity: AIActivityState,
    project_id: &str,
    worktrees: &[WorktreeInfo],
    worktree_activity: &HashMap<String, AIActivityState>,
) -> AIActivityState {
    let mut activities = vec![project_activity];
    for worktree in worktrees
        .iter()
        .filter(|worktree| worktree.project_id == project_id)
    {
        let activity = worktree_activity
            .get(&worktree.id)
            .copied()
            .unwrap_or(AIActivityState::Idle);
        activities.push(activity);
    }
    if activities.contains(&AIActivityState::Review) {
        return AIActivityState::Review;
    }
    if activities.contains(&AIActivityState::Done) {
        return AIActivityState::Done;
    }
    if activities.contains(&AIActivityState::Running) {
        return AIActivityState::Running;
    }
    AIActivityState::Idle
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
    if project_phase.kind == "needsInput" {
        return project_phase;
    }
    if completed_phase.kind == "completed" {
        return completed_phase;
    }
    if project_phase.kind == "running" {
        return project_phase;
    }
    project_phase
}

fn phase_to_activity(phase: &AIRuntimeProjectPhaseSummary) -> AIActivityState {
    match phase.kind.as_str() {
        "needsInput" => AIActivityState::Review,
        "running" => AIActivityState::Running,
        "completed" => AIActivityState::Done,
        _ => AIActivityState::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codux_runtime::ai_runtime_state::AIRuntimeProjectTotalsSummary;

    fn project_state(project_id: &str, kind: &str) -> AIRuntimeProjectStateSummary {
        AIRuntimeProjectStateSummary {
            project_id: project_id.to_string(),
            project_phase: AIRuntimeProjectPhaseSummary {
                kind: kind.to_string(),
                updated_at: 1.0,
                ..Default::default()
            },
            completed_phase: AIRuntimeProjectPhaseSummary::default(),
            totals: AIRuntimeProjectTotalsSummary {
                project_id: project_id.to_string(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn ai_activity_project_states_changed_tracks_phase_changes() {
        let previous = vec![project_state("project-a", "idle")];
        let next = vec![project_state("project-a", "running")];

        assert!(ai_activity_project_states_changed(&previous, &next));
    }

    #[test]
    fn ai_activity_project_states_changed_ignores_equal_project_states() {
        let previous = vec![project_state("project-a", "running")];
        let next = previous.clone();

        assert!(!ai_activity_project_states_changed(&previous, &next));
    }

    #[test]
    fn resolve_displayed_phase_prioritizes_review_done_running_idle() {
        let running = AIRuntimeProjectPhaseSummary {
            kind: "running".to_string(),
            ..Default::default()
        };
        let completed = AIRuntimeProjectPhaseSummary {
            kind: "completed".to_string(),
            updated_at: 2.0,
            ..Default::default()
        };
        assert_eq!(
            resolve_displayed_phase(&running, &completed).kind,
            "completed"
        );

        let review = AIRuntimeProjectPhaseSummary {
            kind: "needsInput".to_string(),
            ..Default::default()
        };
        assert_eq!(
            resolve_displayed_phase(&review, &completed).kind,
            "needsInput"
        );

        let idle = AIRuntimeProjectPhaseSummary::default();
        assert_eq!(resolve_displayed_phase(&idle, &completed).kind, "completed");
    }

    #[test]
    fn aggregate_project_activity_prioritizes_done_before_running() {
        let worktrees = vec![
            WorktreeInfo {
                id: "worktree-a".to_string(),
                project_id: "project-a".to_string(),
                name: "main".to_string(),
                branch: "main".to_string(),
                path: "/tmp/project-a".to_string(),
                status: "active".to_string(),
                is_default: true,
                exists: true,
                git_summary: Default::default(),
            },
            WorktreeInfo {
                id: "worktree-b".to_string(),
                project_id: "project-a".to_string(),
                name: "feature".to_string(),
                branch: "feature".to_string(),
                path: "/tmp/project-a-feature".to_string(),
                status: "active".to_string(),
                is_default: false,
                exists: true,
                git_summary: Default::default(),
            },
        ];
        let mut worktree_activity = HashMap::new();
        worktree_activity.insert("worktree-a".to_string(), AIActivityState::Running);
        worktree_activity.insert("worktree-b".to_string(), AIActivityState::Done);

        assert_eq!(
            aggregate_project_activity(
                AIActivityState::Idle,
                "project-a",
                &worktrees,
                &worktree_activity,
            ),
            AIActivityState::Done
        );
    }
}
