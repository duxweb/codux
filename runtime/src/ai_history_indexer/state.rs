use super::{AIHistoryIndexerState, AIHistoryProjectState};
use crate::ai_history_normalized::{
    AIHistoryProjectRequest, AIHistorySnapshot, AIHistorySourceFingerprint,
};
use std::sync::{Arc, Mutex};

pub(super) fn current_project_state(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project: &AIHistoryProjectRequest,
) -> Result<Option<AIHistoryProjectState>, String> {
    let guard = state
        .lock()
        .map_err(|_| "AI history indexer state lock poisoned.".to_string())?;
    Ok(guard.projects.get(&project.id).cloned())
}

pub(super) fn seed_project_state(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project: &AIHistoryProjectRequest,
    snapshot: Option<AIHistorySnapshot>,
) -> Result<AIHistoryProjectState, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "AI history indexer state lock poisoned.".to_string())?;
    let version = next_state_version(&mut guard);
    let next = AIHistoryProjectState {
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        project_path: project.path.clone(),
        snapshot,
        is_loading: false,
        queued: false,
        progress: None,
        detail: "idle".to_string(),
        error: None,
        version,
    };
    guard.projects.insert(project.id.clone(), next.clone());
    Ok(next)
}

pub(super) fn seed_or_queue_project_state(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project: &AIHistoryProjectRequest,
    snapshot: Option<AIHistorySnapshot>,
) -> Result<(AIHistoryProjectState, bool), String> {
    if snapshot.is_some() || project.path.trim().is_empty() {
        seed_project_state(state, project, snapshot).map(|state| (state, false))
    } else {
        mark_project_queued(state, project, None)
    }
}

pub(super) fn mark_project_queued(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project: &AIHistoryProjectRequest,
    cached_snapshot: Option<AIHistorySnapshot>,
) -> Result<(AIHistoryProjectState, bool), String> {
    let mut guard = state
        .lock()
        .map_err(|_| "AI history indexer state lock poisoned.".to_string())?;

    let was_already_queued = guard.queued_or_running_projects.contains(&project.id);
    let previous = guard.projects.get(&project.id).cloned();
    let previous_snapshot = previous.as_ref().and_then(|state| state.snapshot.clone());
    let snapshot = cached_snapshot.or(previous_snapshot);
    let (queued, progress, detail) = match (was_already_queued, previous.as_ref()) {
        (true, Some(state)) => (
            state.queued,
            state.progress.or(Some(0.0)),
            state.detail.clone(),
        ),
        (true, None) => (false, Some(0.0), "indexing".to_string()),
        (false, _) => (true, Some(0.0), "queued".to_string()),
    };
    let version = next_state_version(&mut guard);
    let next = AIHistoryProjectState {
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        project_path: project.path.clone(),
        snapshot,
        is_loading: true,
        queued,
        progress,
        detail,
        error: None,
        version,
    };
    guard.projects.insert(project.id.clone(), next.clone());

    match was_already_queued {
        true => Ok((next, false)),
        false => {
            guard.queued_or_running_projects.insert(project.id.clone());
            Ok((next, true))
        }
    }
}

pub(super) fn mark_project_running(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project: &AIHistoryProjectRequest,
) -> Result<AIHistoryProjectState, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "AI history indexer state lock poisoned.".to_string())?;
    let previous_snapshot = guard
        .projects
        .get(&project.id)
        .and_then(|state| state.snapshot.clone());
    let version = next_state_version(&mut guard);
    let next = AIHistoryProjectState {
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        project_path: project.path.clone(),
        snapshot: previous_snapshot,
        is_loading: true,
        queued: false,
        progress: Some(0.0),
        detail: "indexing".to_string(),
        error: None,
        version,
    };
    guard.projects.insert(project.id.clone(), next.clone());
    Ok(next)
}

pub(super) fn mark_project_completed(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project: &AIHistoryProjectRequest,
    snapshot: AIHistorySnapshot,
) -> Result<AIHistoryProjectState, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "AI history indexer state lock poisoned.".to_string())?;
    guard.queued_or_running_projects.remove(&project.id);
    let version = next_state_version(&mut guard);
    let next = AIHistoryProjectState {
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        project_path: project.path.clone(),
        snapshot: Some(snapshot),
        is_loading: false,
        queued: false,
        progress: Some(1.0),
        detail: "completed".to_string(),
        error: None,
        version,
    };
    guard.projects.insert(project.id.clone(), next.clone());
    Ok(next)
}

pub(super) fn project_source_fingerprint_unchanged(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project_id: &str,
    fingerprint: &AIHistorySourceFingerprint,
) -> bool {
    state
        .lock()
        .ok()
        .and_then(|guard| guard.project_source_fingerprints.get(project_id).cloned())
        .is_some_and(|previous| previous == *fingerprint)
}

pub(super) fn set_project_source_fingerprint(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project_id: &str,
    fingerprint: AIHistorySourceFingerprint,
) {
    if let Ok(mut guard) = state.lock() {
        guard
            .project_source_fingerprints
            .insert(project_id.to_string(), fingerprint);
    }
}

pub(super) fn mark_project_failed(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project: &AIHistoryProjectRequest,
    error: String,
) -> Result<AIHistoryProjectState, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "AI history indexer state lock poisoned.".to_string())?;
    guard.queued_or_running_projects.remove(&project.id);
    let previous_snapshot = guard
        .projects
        .get(&project.id)
        .and_then(|state| state.snapshot.clone());
    let version = next_state_version(&mut guard);
    let next = AIHistoryProjectState {
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        project_path: project.path.clone(),
        snapshot: previous_snapshot,
        is_loading: false,
        queued: false,
        progress: None,
        detail: "failed".to_string(),
        error: Some(error),
        version,
    };
    guard.projects.insert(project.id.clone(), next.clone());
    Ok(next)
}

pub(super) fn mark_project_progress(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project: &AIHistoryProjectRequest,
    progress: f64,
    detail: &'static str,
) -> Result<AIHistoryProjectState, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "AI history indexer state lock poisoned.".to_string())?;
    let previous_snapshot = guard
        .projects
        .get(&project.id)
        .and_then(|state| state.snapshot.clone());
    let version = next_state_version(&mut guard);
    let next = AIHistoryProjectState {
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        project_path: project.path.clone(),
        snapshot: previous_snapshot,
        is_loading: true,
        queued: false,
        progress: Some(progress.clamp(0.0, 1.0)),
        detail: detail.to_string(),
        error: None,
        version,
    };
    guard.projects.insert(project.id.clone(), next.clone());
    Ok(next)
}

fn next_state_version(state: &mut AIHistoryIndexerState) -> u64 {
    state.next_version = state.next_version.saturating_add(1);
    state.next_version
}
