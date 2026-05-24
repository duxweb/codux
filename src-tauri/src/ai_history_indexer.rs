use crate::ai_history::{
    index_global_history_fresh, index_project_history_fresh_with_progress,
    load_indexed_global_history, load_indexed_project_history, remove_indexed_history_session,
    rename_indexed_history_session, AIGlobalHistorySnapshot, AIHistoryProjectRequest,
    AIHistorySnapshot,
};
use crate::runtime_trace::{runtime_trace, runtime_trace_elapsed};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{sync_channel, Receiver as StdReceiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::async_runtime::{channel, Receiver, Sender};
use tauri::{AppHandle, Emitter};

pub struct AIHistoryIndexer {
    tx: Sender<AIHistoryJob>,
    state: Arc<Mutex<AIHistoryIndexerState>>,
    app: AppHandle,
}

enum AIHistoryJob {
    Global {
        projects: Vec<AIHistoryProjectRequest>,
        reply: SyncSender<Result<AIGlobalHistorySnapshot, String>>,
    },
    RefreshProject {
        project: AIHistoryProjectRequest,
    },
    RefreshGlobal {
        projects: Vec<AIHistoryProjectRequest>,
    },
}

#[derive(Default)]
struct AIHistoryIndexerState {
    projects: HashMap<String, AIHistoryProjectState>,
    queued_or_running_projects: HashSet<String>,
    next_version: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryProjectState {
    project_id: String,
    project_name: String,
    project_path: String,
    snapshot: Option<AIHistorySnapshot>,
    is_loading: bool,
    queued: bool,
    progress: Option<f64>,
    detail: String,
    error: Option<String>,
    version: u64,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AIHistoryEvent {
    Project {
        snapshot: AIHistorySnapshot,
    },
    ProjectState {
        state: AIHistoryProjectState,
    },
    Global {
        snapshot: AIGlobalHistorySnapshot,
    },
    Status {
        scope: String,
        project_id: Option<String>,
        is_loading: bool,
        detail: String,
    },
}

impl AIHistoryIndexer {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = channel(16);
        let state = Arc::new(Mutex::new(AIHistoryIndexerState::default()));
        runtime_trace("ai-history", "indexer started queue_capacity=16");
        tauri::async_runtime::spawn(history_indexer_loop(rx, app.clone(), Arc::clone(&state)));
        Self { tx, state, app }
    }

    pub async fn project_summary(
        &self,
        project: AIHistoryProjectRequest,
    ) -> Result<AIHistoryProjectState, String> {
        let cached_snapshot = indexed_project_snapshot(project.clone()).await?;
        let (project_state, should_enqueue) =
            mark_project_queued(&self.state, &project, cached_snapshot)?;
        emit_history_event(
            &self.app,
            AIHistoryEvent::ProjectState {
                state: project_state.clone(),
            },
        );

        if should_enqueue {
            if self
                .tx
                .send(AIHistoryJob::RefreshProject {
                    project: project.clone(),
                })
                .await
                .is_err()
            {
                let failed_state = mark_project_failed(
                    &self.state,
                    &project,
                    "AI history indexer stopped.".to_string(),
                )?;
                emit_history_event(
                    &self.app,
                    AIHistoryEvent::ProjectState {
                        state: failed_state,
                    },
                );
                return Err("AI history indexer stopped.".to_string());
            }
        }

        Ok(project_state)
    }

    pub async fn refresh_project(&self, project: AIHistoryProjectRequest) -> Result<(), String> {
        let cached_snapshot = indexed_project_snapshot(project.clone()).await?;
        let (project_state, should_enqueue) =
            mark_project_queued(&self.state, &project, cached_snapshot)?;
        emit_history_event(
            &self.app,
            AIHistoryEvent::ProjectState {
                state: project_state.clone(),
            },
        );

        if should_enqueue
            && self
                .tx
                .send(AIHistoryJob::RefreshProject { project })
                .await
                .is_err()
        {
            return Err("AI history indexer stopped.".to_string());
        }

        Ok(())
    }

    pub async fn project_state(
        &self,
        project: AIHistoryProjectRequest,
    ) -> Result<AIHistoryProjectState, String> {
        let started_at = Instant::now();
        if let Some(state) = current_project_state(&self.state, &project)? {
            runtime_trace_elapsed(
                "ai-history",
                "project_state memory_cache",
                started_at,
                &format!(
                    "project={} sessions={}",
                    project.id,
                    state
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.sessions.len())
                        .unwrap_or(0)
                ),
            );
            return Ok(state);
        }
        let cached_snapshot = indexed_project_snapshot(project.clone()).await?;
        let project_state = seed_project_state(&self.state, &project, cached_snapshot)?;
        runtime_trace_elapsed(
            "ai-history",
            "project_state sqlite_cache",
            started_at,
            &format!(
                "project={} sessions={}",
                project.id,
                project_state
                    .snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.sessions.len())
                    .unwrap_or(0)
            ),
        );
        Ok(project_state)
    }

    pub async fn global_summary(
        &self,
        projects: Vec<AIHistoryProjectRequest>,
    ) -> Result<AIGlobalHistorySnapshot, String> {
        if let Some(snapshot) = indexed_global_snapshot(projects.clone()).await? {
            return Ok(snapshot);
        }

        let (reply, result) = sync_channel(1);
        self.tx
            .send(AIHistoryJob::Global { projects, reply })
            .await
            .map_err(|_| "AI history indexer stopped.".to_string())?;
        receive_reply(result).await
    }

    pub async fn global_state(
        &self,
        projects: Vec<AIHistoryProjectRequest>,
    ) -> Result<Option<AIGlobalHistorySnapshot>, String> {
        indexed_global_snapshot(projects).await
    }

    pub async fn refresh_global(
        &self,
        projects: Vec<AIHistoryProjectRequest>,
    ) -> Result<(), String> {
        self.tx
            .send(AIHistoryJob::RefreshGlobal { projects })
            .await
            .map_err(|_| "AI history indexer stopped.".to_string())
    }

    pub async fn rename_session(
        &self,
        project: AIHistoryProjectRequest,
        session_id: String,
        title: String,
    ) -> Result<AIHistoryProjectState, String> {
        let snapshot = tauri::async_runtime::spawn_blocking({
            let project = project.clone();
            move || rename_indexed_history_session(project, session_id, title)
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Matching session record was not found.".to_string())?;
        let next_state = mark_project_completed(&self.state, &project, snapshot)?;
        emit_history_event(
            &self.app,
            AIHistoryEvent::ProjectState {
                state: next_state.clone(),
            },
        );
        Ok(next_state)
    }

    pub async fn remove_session(
        &self,
        project: AIHistoryProjectRequest,
        session_id: String,
    ) -> Result<AIHistoryProjectState, String> {
        let snapshot = tauri::async_runtime::spawn_blocking({
            let project = project.clone();
            move || remove_indexed_history_session(project, session_id)
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Matching session record was not found.".to_string())?;
        let next_state = mark_project_completed(&self.state, &project, snapshot)?;
        emit_history_event(
            &self.app,
            AIHistoryEvent::ProjectState {
                state: next_state.clone(),
            },
        );
        Ok(next_state)
    }
}

async fn history_indexer_loop(
    mut rx: Receiver<AIHistoryJob>,
    app: AppHandle,
    state: Arc<Mutex<AIHistoryIndexerState>>,
) {
    while let Some(job) = rx.recv().await {
        match job {
            AIHistoryJob::Global { projects, reply } => {
                runtime_trace(
                    "ai-history",
                    &format!("global index start projects={}", projects.len()),
                );
                let result = run_global_index(projects).await;
                if let Ok(snapshot) = &result {
                    emit_history_event(
                        &app,
                        AIHistoryEvent::Global {
                            snapshot: snapshot.clone(),
                        },
                    );
                }
                let _ = reply.send(result);
            }
            AIHistoryJob::RefreshProject { project } => {
                runtime_trace(
                    "ai-history",
                    &format!(
                        "project refresh start project={} path={}",
                        project.id, project.path
                    ),
                );
                if let Ok(next_state) = mark_project_running(&state, &project) {
                    emit_history_event(&app, AIHistoryEvent::ProjectState { state: next_state });
                }
                emit_history_event(
                    &app,
                    AIHistoryEvent::Status {
                        scope: "project".to_string(),
                        project_id: Some(project.id.clone()),
                        is_loading: true,
                        detail: "indexing".to_string(),
                    },
                );
                let result =
                    run_project_index(app.clone(), Arc::clone(&state), project.clone()).await;
                let finished_state = match result {
                    Ok(snapshot) => {
                        emit_history_event(
                            &app,
                            AIHistoryEvent::Project {
                                snapshot: snapshot.clone(),
                            },
                        );
                        mark_project_completed(&state, &project, snapshot)
                    }
                    Err(error) => mark_project_failed(&state, &project, error),
                };
                if let Ok(next_state) = finished_state {
                    emit_history_event(&app, AIHistoryEvent::ProjectState { state: next_state });
                }
                emit_history_event(
                    &app,
                    AIHistoryEvent::Status {
                        scope: "project".to_string(),
                        project_id: Some(project.id),
                        is_loading: false,
                        detail: "completed".to_string(),
                    },
                );
            }
            AIHistoryJob::RefreshGlobal { projects } => {
                runtime_trace(
                    "ai-history",
                    &format!("global refresh start projects={}", projects.len()),
                );
                emit_history_event(
                    &app,
                    AIHistoryEvent::Status {
                        scope: "global".to_string(),
                        project_id: None,
                        is_loading: true,
                        detail: "indexing".to_string(),
                    },
                );
                if let Ok(snapshot) = run_global_index(projects).await {
                    emit_history_event(&app, AIHistoryEvent::Global { snapshot });
                }
                emit_history_event(
                    &app,
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

fn current_project_state(
    state: &Arc<Mutex<AIHistoryIndexerState>>,
    project: &AIHistoryProjectRequest,
) -> Result<Option<AIHistoryProjectState>, String> {
    let guard = state
        .lock()
        .map_err(|_| "AI history indexer state lock poisoned.".to_string())?;
    Ok(guard.projects.get(&project.id).cloned())
}

fn seed_project_state(
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

fn mark_project_queued(
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

fn mark_project_running(
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

fn mark_project_completed(
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

fn mark_project_failed(
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

async fn indexed_project_snapshot(
    project: AIHistoryProjectRequest,
) -> Result<Option<AIHistorySnapshot>, String> {
    let started_at = Instant::now();
    let project_id = project.id.clone();
    tauri::async_runtime::spawn_blocking(move || load_indexed_project_history(project))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
        .map(|snapshot| {
            runtime_trace_elapsed(
                "ai-history",
                "load_project_cache",
                started_at,
                &format!(
                    "project={} hit={} sessions={}",
                    project_id,
                    snapshot.is_some(),
                    snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.sessions.len())
                        .unwrap_or(0)
                ),
            );
            snapshot
        })
}

async fn indexed_global_snapshot(
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<Option<AIGlobalHistorySnapshot>, String> {
    let started_at = Instant::now();
    let project_count = projects.len();
    tauri::async_runtime::spawn_blocking(move || load_indexed_global_history(projects))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
        .map(|snapshot| {
            runtime_trace_elapsed(
                "ai-history",
                "load_global_cache",
                started_at,
                &format!(
                    "projects={} hit={} sessions={}",
                    project_count,
                    snapshot.is_some(),
                    snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.sessions.len())
                        .unwrap_or(0)
                ),
            );
            snapshot
        })
}

fn mark_project_progress(
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

async fn run_project_index(
    app: AppHandle,
    state: Arc<Mutex<AIHistoryIndexerState>>,
    project: AIHistoryProjectRequest,
) -> Result<AIHistorySnapshot, String> {
    let started_at = Instant::now();
    let project_id = project.id.clone();
    let project_path = project.path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let progress_project = project.clone();
        index_project_history_fresh_with_progress(project, |progress, detail| {
            if let Ok(next_state) =
                mark_project_progress(&state, &progress_project, progress, detail)
            {
                emit_history_event(&app, AIHistoryEvent::ProjectState { state: next_state });
            }
        })
    })
    .await
    .map_err(|error| error.to_string())
    .map(|snapshot| {
        runtime_trace_elapsed(
            "ai-history",
            "run_project_index",
            started_at,
            &format!(
                "project={} path={} sessions={} total_tokens={}",
                project_id,
                project_path,
                snapshot.sessions.len(),
                snapshot.project_summary.project_total_tokens
            ),
        );
        snapshot
    })
}

async fn run_global_index(
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<AIGlobalHistorySnapshot, String> {
    let started_at = Instant::now();
    let project_count = projects.len();
    tauri::async_runtime::spawn_blocking(move || index_global_history_fresh(projects))
        .await
        .map_err(|error| error.to_string())
        .map(|snapshot| {
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
            snapshot
        })
}

fn emit_history_event(app: &AppHandle, event: AIHistoryEvent) {
    let _ = app.emit("ai-history:event", event);
}

async fn receive_reply<T: Send + 'static>(rx: StdReceiver<Result<T, String>>) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv()
            .map_err(|_| "AI history indexer reply dropped.".to_string())
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_history::AIProjectUsageSummary;

    #[test]
    fn project_state_tracks_queue_progress_and_completion() {
        let state = Arc::new(Mutex::new(AIHistoryIndexerState::default()));
        let project = test_project();

        let (queued, should_enqueue) = mark_project_queued(&state, &project, None).unwrap();
        assert!(should_enqueue);
        assert!(queued.is_loading);
        assert!(queued.queued);
        assert_eq!(queued.detail, "queued");
        assert_eq!(queued.progress, Some(0.0));
        assert_eq!(queued.version, 1);

        let (duplicate, should_enqueue_duplicate) =
            mark_project_queued(&state, &project, None).unwrap();
        assert!(!should_enqueue_duplicate);
        assert!(duplicate.is_loading);
        assert!(duplicate.queued);
        assert_eq!(duplicate.detail, "queued");
        assert_eq!(duplicate.progress, Some(0.0));
        assert!(duplicate.version > queued.version);

        let running = mark_project_running(&state, &project).unwrap();
        assert!(running.is_loading);
        assert!(!running.queued);
        assert_eq!(running.detail, "indexing");

        let progressed = mark_project_progress(&state, &project, 0.58, "readingSources").unwrap();
        assert!(progressed.is_loading);
        assert_eq!(progressed.progress, Some(0.58));
        assert_eq!(progressed.detail, "readingSources");

        let completed = mark_project_completed(&state, &project, test_snapshot()).unwrap();
        assert!(!completed.is_loading);
        assert!(!completed.queued);
        assert_eq!(completed.progress, Some(1.0));
        assert_eq!(completed.detail, "completed");
        assert!(completed.snapshot.is_some());
        assert!(!state
            .lock()
            .unwrap()
            .queued_or_running_projects
            .contains(&project.id));
    }

    fn test_project() -> AIHistoryProjectRequest {
        AIHistoryProjectRequest {
            id: "project-1".to_string(),
            name: "Project".to_string(),
            path: "/tmp/project".to_string(),
        }
    }

    fn test_snapshot() -> AIHistorySnapshot {
        AIHistorySnapshot {
            project_id: "project-1".to_string(),
            project_name: "Project".to_string(),
            project_summary: AIProjectUsageSummary {
                project_id: "project-1".to_string(),
                project_name: "Project".to_string(),
                current_session_tokens: 0,
                current_session_cached_input_tokens: 0,
                project_total_tokens: 10,
                project_cached_input_tokens: 0,
                today_total_tokens: 10,
                today_cached_input_tokens: 0,
                current_tool: None,
                current_model: None,
                current_session_updated_at: None,
            },
            sessions: Vec::new(),
            heatmap: Vec::new(),
            today_time_buckets: Vec::new(),
            tool_breakdown: Vec::new(),
            model_breakdown: Vec::new(),
            indexed_at: 1.0,
        }
    }
}
