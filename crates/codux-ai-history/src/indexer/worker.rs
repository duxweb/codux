use super::events::push_history_event;
use super::state::{
    mark_project_completed, mark_project_failed, mark_project_progress, mark_project_running,
    project_source_fingerprint_unchanged, set_project_source_fingerprint,
};
use super::types::{AIHistoryEvent, AIHistoryIndexerState, AIHistoryJob};
use crate::normalized::{
    AIGlobalHistorySnapshot, AIHistoryProjectRequest, AIHistorySnapshot,
    index_global_history_fresh_at, index_project_history_fresh_at, load_indexed_global_history_at,
    load_indexed_project_history_at, project_history_source_fingerprint,
};
use crate::trace::{runtime_trace, runtime_trace_elapsed};
use crate::usage_store::AIUsageStore;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub fn history_indexer_loop(
    rx: Receiver<AIHistoryJob>,
    events: Arc<Mutex<VecDeque<AIHistoryEvent>>>,
    state: Arc<Mutex<AIHistoryIndexerState>>,
) {
    while let Ok(job) = rx.recv() {
        match job {
            AIHistoryJob::Global {
                projects,
                database_path,
                reply,
            } => {
                runtime_trace(
                    "ai-history",
                    &format!("global index start projects={}", projects.len()),
                );
                let result = run_global_index(database_path, projects);
                if let Ok(snapshot) = &result {
                    push_history_event(
                        &events,
                        AIHistoryEvent::Global {
                            snapshot: snapshot.clone(),
                        },
                    );
                }
                let _ = reply.send(result);
            }
            AIHistoryJob::RefreshProject {
                project,
                database_path,
            } => {
                let _ = refresh_project(&events, Arc::clone(&state), database_path, project);
            }
            AIHistoryJob::RefreshGlobal {
                projects,
                database_path,
            } => {
                runtime_trace(
                    "ai-history",
                    &format!("global refresh start projects={}", projects.len()),
                );
                push_history_event(
                    &events,
                    AIHistoryEvent::Status {
                        scope: "global".to_string(),
                        project_id: None,
                        is_loading: true,
                        detail: "indexing".to_string(),
                    },
                );
                if let Ok(snapshot) = run_global_snapshot(database_path, projects) {
                    push_history_event(&events, AIHistoryEvent::Global { snapshot });
                }
                push_history_event(
                    &events,
                    AIHistoryEvent::Status {
                        scope: "global".to_string(),
                        project_id: None,
                        is_loading: false,
                        detail: "completed".to_string(),
                    },
                );
            }
        }
    }
}

fn refresh_project(
    events: &Arc<Mutex<VecDeque<AIHistoryEvent>>>,
    state: Arc<Mutex<AIHistoryIndexerState>>,
    database_path: PathBuf,
    project: AIHistoryProjectRequest,
) -> Result<super::AIHistoryProjectState, String> {
    runtime_trace(
        "ai-history",
        &format!(
            "project refresh start project={} path={}",
            project.id, project.path
        ),
    );
    if let Ok(next_state) = mark_project_running(&state, &project) {
        push_history_event(events, AIHistoryEvent::ProjectState { state: next_state });
    }
    push_history_event(
        events,
        AIHistoryEvent::Status {
            scope: "project".to_string(),
            project_id: Some(project.id.clone()),
            is_loading: true,
            detail: "indexing".to_string(),
        },
    );
    let result = run_project_index(events, Arc::clone(&state), database_path, project.clone());
    let finished_state = match result {
        Ok(snapshot) => {
            push_history_event(
                events,
                AIHistoryEvent::Project {
                    snapshot: snapshot.clone(),
                },
            );
            mark_project_completed(&state, &project, snapshot)
        }
        Err(error) => mark_project_failed(&state, &project, error),
    };
    if let Ok(next_state) = &finished_state {
        push_history_event(
            events,
            AIHistoryEvent::ProjectState {
                state: next_state.clone(),
            },
        );
    }
    push_history_event(
        events,
        AIHistoryEvent::Status {
            scope: "project".to_string(),
            project_id: Some(project.id),
            is_loading: false,
            detail: "completed".to_string(),
        },
    );
    finished_state
}

fn run_global_snapshot(
    database_path: PathBuf,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<AIGlobalHistorySnapshot, String> {
    load_indexed_global_history_at(database_path, projects)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "AI history is not indexed yet.".to_string())
}

fn run_project_index(
    events: &Arc<Mutex<VecDeque<AIHistoryEvent>>>,
    state: Arc<Mutex<AIHistoryIndexerState>>,
    database_path: PathBuf,
    project: AIHistoryProjectRequest,
) -> Result<AIHistorySnapshot, String> {
    let started_at = Instant::now();
    let project_id = project.id.clone();
    let project_path = project.path.clone();
    let fingerprint = project_history_source_fingerprint(&project);
    if project_source_fingerprint_unchanged(&state, &project_id, &fingerprint)
        && let Ok(Some(snapshot)) =
            load_indexed_project_history_at(database_path.clone(), project.clone())
    {
        runtime_trace_elapsed(
            "ai-history",
            "run_project_index skipped_unchanged",
            started_at,
            &format!(
                "project={} path={} files={} sessions={} total_tokens={}",
                project_id,
                project_path,
                fingerprint.files.len(),
                snapshot.sessions.len(),
                snapshot.project_summary.project_total_tokens
            ),
        );
        return Ok(snapshot);
    }
    let progress_project = project.clone();
    let snapshot = index_project_history_fresh_at(
        project,
        AIUsageStore::at_path(database_path),
        |progress, detail| {
            if let Ok(next_state) =
                mark_project_progress(&state, &progress_project, progress, detail)
            {
                push_history_event(events, AIHistoryEvent::ProjectState { state: next_state });
            }
        },
    );
    set_project_source_fingerprint(&state, &project_id, fingerprint.clone());
    runtime_trace_elapsed(
        "ai-history",
        "run_project_index",
        started_at,
        &format!(
            "project={} path={} files={} sessions={} total_tokens={}",
            project_id,
            project_path,
            fingerprint.files.len(),
            snapshot.sessions.len(),
            snapshot.project_summary.project_total_tokens
        ),
    );
    Ok(snapshot)
}

fn run_global_index(
    database_path: impl AsRef<Path>,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<AIGlobalHistorySnapshot, String> {
    let started_at = Instant::now();
    let project_count = projects.len();
    let snapshot = index_global_history_fresh_at(
        projects,
        AIUsageStore::at_path(database_path.as_ref().to_path_buf()),
    );
    runtime_trace_elapsed(
        "ai-history",
        "run_global_index",
        started_at,
        &format!(
            "projects={} sessions={} total_tokens={}",
            project_count,
            snapshot.sessions.len(),
            snapshot.total_tokens
        ),
    );
    Ok(snapshot)
}
