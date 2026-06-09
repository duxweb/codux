mod cache;
mod events;
mod state;
#[cfg(test)]
mod tests;
mod types;
mod worker;

use crate::ai_history_normalized::{
    AIGlobalHistorySnapshot, AIHistoryProjectRequest, project_history_source_fingerprint,
    remove_indexed_history_session_at, rename_indexed_history_session_at,
};
use crate::runtime_trace::{runtime_trace, runtime_trace_elapsed};
use cache::{indexed_global_snapshot, indexed_project_snapshot, receive_reply};
use events::push_history_event;
use state::*;
use std::collections::VecDeque;
use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
pub use types::{AIHistoryEvent, AIHistoryProjectState};
use types::{AIHistoryIndexerState, AIHistoryJob};
use worker::history_indexer_loop;

#[derive(Clone)]
pub struct AIHistoryIndexer {
    tx: SyncSender<AIHistoryJob>,
    state: Arc<Mutex<AIHistoryIndexerState>>,
    events: Arc<Mutex<VecDeque<AIHistoryEvent>>>,
    database_path: PathBuf,
}

impl fmt::Debug for AIHistoryIndexer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let project_count = self
            .state
            .lock()
            .map(|state| state.projects.len())
            .unwrap_or_default();
        let queued_count = self
            .state
            .lock()
            .map(|state| state.queued_or_running_projects.len())
            .unwrap_or_default();
        let event_count = self
            .events
            .lock()
            .map(|events| events.len())
            .unwrap_or_default();
        f.debug_struct("AIHistoryIndexer")
            .field("project_count", &project_count)
            .field("queued_count", &queued_count)
            .field("event_count", &event_count)
            .finish()
    }
}

impl AIHistoryIndexer {
    pub fn new() -> Self {
        Self::with_database_path(crate::runtime_paths::app_support_dir().join("ai-usage.sqlite3"))
    }

    pub fn with_database_path(database_path: PathBuf) -> Self {
        let (tx, rx) = sync_channel(16);
        let state = Arc::new(Mutex::new(AIHistoryIndexerState::default()));
        let events = Arc::new(Mutex::new(VecDeque::new()));
        runtime_trace(
            "ai-history",
            &format!(
                "indexer started queue_capacity=16 database={}",
                database_path.display()
            ),
        );
        let worker_state = Arc::clone(&state);
        let worker_events = Arc::clone(&events);
        thread::Builder::new()
            .name("codux-ai-history-indexer".to_string())
            .spawn(move || history_indexer_loop(rx, worker_events, worker_state))
            .expect("failed to spawn AI history indexer worker");
        Self {
            tx,
            state,
            events,
            database_path,
        }
    }

    pub fn active_project_count(&self) -> usize {
        self.state
            .lock()
            .map(|state| state.queued_or_running_projects.len())
            .unwrap_or_default()
    }

    pub fn project_summary(
        &self,
        project: AIHistoryProjectRequest,
    ) -> Result<AIHistoryProjectState, String> {
        self.project_state(project)
    }

    pub fn refresh_project(&self, project: AIHistoryProjectRequest) -> Result<(), String> {
        let cached_snapshot = indexed_project_snapshot(&self.database_path, project.clone())?;
        let fingerprint = project_history_source_fingerprint(&project);
        if cached_snapshot.is_some()
            && project_source_fingerprint_unchanged(&self.state, &project.id, &fingerprint)
        {
            let project_state = seed_project_state(&self.state, &project, cached_snapshot)?;
            push_history_event(
                &self.events,
                AIHistoryEvent::ProjectState {
                    state: project_state,
                },
            );
            return Ok(());
        }
        let (project_state, should_enqueue) =
            mark_project_queued(&self.state, &project, cached_snapshot)?;
        push_history_event(
            &self.events,
            AIHistoryEvent::ProjectState {
                state: project_state,
            },
        );

        if should_enqueue
            && self
                .tx
                .send(AIHistoryJob::RefreshProject {
                    project,
                    database_path: self.database_path.clone(),
                })
                .is_err()
        {
            return Err("AI history indexer stopped.".to_string());
        }

        Ok(())
    }

    pub fn project_state(
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
        let cached_snapshot = indexed_project_snapshot(&self.database_path, project.clone())?;
        let (project_state, should_enqueue) =
            seed_or_queue_project_state(&self.state, &project, cached_snapshot)?;
        if should_enqueue {
            push_history_event(
                &self.events,
                AIHistoryEvent::ProjectState {
                    state: project_state.clone(),
                },
            );
            if self
                .tx
                .send(AIHistoryJob::RefreshProject {
                    project: project.clone(),
                    database_path: self.database_path.clone(),
                })
                .is_err()
            {
                return Err("AI history indexer stopped.".to_string());
            }
            runtime_trace_elapsed(
                "ai-history",
                "project_state cache_miss_queued",
                started_at,
                &format!("project={}", project.id),
            );
            return Ok(project_state);
        }
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

    pub fn global_summary(
        &self,
        projects: Vec<AIHistoryProjectRequest>,
    ) -> Result<AIGlobalHistorySnapshot, String> {
        if let Some(snapshot) = indexed_global_snapshot(&self.database_path, projects.clone())? {
            return Ok(snapshot);
        }

        let (reply, result) = sync_channel(1);
        self.tx
            .send(AIHistoryJob::Global {
                projects,
                database_path: self.database_path.clone(),
                reply,
            })
            .map_err(|_| "AI history indexer stopped.".to_string())?;
        receive_reply(result)
    }

    pub fn global_state(
        &self,
        projects: Vec<AIHistoryProjectRequest>,
    ) -> Result<Option<AIGlobalHistorySnapshot>, String> {
        indexed_global_snapshot(&self.database_path, projects)
    }

    pub fn refresh_global(&self, projects: Vec<AIHistoryProjectRequest>) -> Result<(), String> {
        self.tx
            .send(AIHistoryJob::RefreshGlobal {
                projects,
                database_path: self.database_path.clone(),
            })
            .map_err(|_| "AI history indexer stopped.".to_string())
    }

    pub fn rename_session(
        &self,
        project: AIHistoryProjectRequest,
        session_id: String,
        title: String,
    ) -> Result<AIHistoryProjectState, String> {
        let snapshot = rename_indexed_history_session_at(
            self.database_path.clone(),
            project.clone(),
            session_id,
            title,
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Matching session record was not found.".to_string())?;
        let next_state = mark_project_completed(&self.state, &project, snapshot)?;
        push_history_event(
            &self.events,
            AIHistoryEvent::ProjectState {
                state: next_state.clone(),
            },
        );
        Ok(next_state)
    }

    pub fn remove_session(
        &self,
        project: AIHistoryProjectRequest,
        session_id: String,
    ) -> Result<AIHistoryProjectState, String> {
        let snapshot = remove_indexed_history_session_at(
            self.database_path.clone(),
            project.clone(),
            session_id,
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Matching session record was not found.".to_string())?;
        let next_state = mark_project_completed(&self.state, &project, snapshot)?;
        push_history_event(
            &self.events,
            AIHistoryEvent::ProjectState {
                state: next_state.clone(),
            },
        );
        Ok(next_state)
    }

    pub fn drain_events(&self) -> Vec<AIHistoryEvent> {
        self.events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }
}

impl Default for AIHistoryIndexer {
    fn default() -> Self {
        Self::new()
    }
}
