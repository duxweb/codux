use super::{
    MemoryService,
    extraction::ensure_memory_provider_available,
    now_seconds,
    queue::MemoryExtractionStatusSnapshot,
    transcript::{MemoryProjectContext, resolve_transcript_source, session_identifier},
};
use crate::{
    ai_history_normalized::AISessionSummary,
    ai_runtime::{
        probe::paths::paths_equivalent, snapshot::AISessionSnapshot, state::normalized_string,
    },
    project_store::ProjectWorkspaceRecord,
    settings::{AIMemorySettings, AISettings},
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const MAX_AUTOMATIC_EXTRACTION_ENQUEUE: usize = 10;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryExtractionEnqueueResult {
    pub checked_count: i64,
    pub enqueued_count: i64,
    pub status: MemoryExtractionStatusSnapshot,
}

impl MemoryService {
    pub fn enqueue_manual_extraction_candidates(
        &self,
        memory_settings: &AIMemorySettings,
        projects: &[ProjectWorkspaceRecord],
        runtime_sessions: &[AISessionSnapshot],
        history_sessions: &[AISessionSummary],
    ) -> Result<MemoryExtractionEnqueueResult, String> {
        if !memory_settings.enabled {
            return Ok(MemoryExtractionEnqueueResult {
                checked_count: 0,
                enqueued_count: 0,
                status: self.extraction_status_snapshot()?,
            });
        }

        self.ensure_queue_schema()?;
        let mut sessions =
            manual_extraction_candidates(memory_settings, projects, runtime_sessions);
        sessions.extend(manual_extraction_candidates_from_history(
            memory_settings,
            projects,
            history_sessions,
        ));
        let sessions = deduplicate_manual_candidates(sessions);
        self.enqueue_extraction_candidates(projects, &sessions)
    }

    pub fn enqueue_automatic_extraction_candidates(
        &self,
        memory_settings: &AIMemorySettings,
        projects: &[ProjectWorkspaceRecord],
        runtime_sessions: &[AISessionSnapshot],
        history_sessions: &[AISessionSummary],
    ) -> Result<MemoryExtractionEnqueueResult, String> {
        if !memory_settings.enabled || !memory_settings.automatic_extraction_enabled {
            return Ok(MemoryExtractionEnqueueResult {
                checked_count: 0,
                enqueued_count: 0,
                status: self.extraction_status_snapshot()?,
            });
        }

        self.ensure_queue_schema()?;
        let mut sessions =
            automatic_extraction_candidates(memory_settings, projects, runtime_sessions);
        sessions.extend(automatic_extraction_candidates_from_history(
            memory_settings,
            projects,
            history_sessions,
        ));
        let mut sessions = deduplicate_manual_candidates(sessions);
        sessions.truncate(MAX_AUTOMATIC_EXTRACTION_ENQUEUE);
        self.enqueue_extraction_candidates(projects, &sessions)
    }

    pub async fn process_memory_sessions_now(
        &self,
        settings: &AISettings,
        projects: &[ProjectWorkspaceRecord],
        runtime_sessions: &[AISessionSnapshot],
        history_sessions: &[AISessionSummary],
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        if !settings.memory.enabled {
            return self.extraction_status_snapshot();
        }
        ensure_memory_provider_available(settings)?;
        let enqueued = self.enqueue_manual_extraction_candidates(
            &settings.memory,
            projects,
            runtime_sessions,
            history_sessions,
        )?;
        let mut status = self
            .process_memory_extraction_queue(settings, projects)
            .await?;
        status.checked_count = enqueued.checked_count;
        status.enqueued_count = enqueued.enqueued_count;
        Ok(status)
    }

    fn enqueue_extraction_candidates(
        &self,
        projects: &[ProjectWorkspaceRecord],
        sessions: &[AISessionSnapshot],
    ) -> Result<MemoryExtractionEnqueueResult, String> {
        let checked_count = sessions.len() as i64;
        let mut enqueued_count = 0_i64;

        for session in sessions {
            if self.enqueue_session_for_extraction(projects, session)? {
                enqueued_count += 1;
            }
        }

        let mut status = self.extraction_status_snapshot()?;
        status.checked_count = checked_count;
        status.enqueued_count = enqueued_count;
        Ok(MemoryExtractionEnqueueResult {
            checked_count,
            enqueued_count,
            status,
        })
    }

    fn enqueue_session_for_extraction(
        &self,
        projects: &[ProjectWorkspaceRecord],
        session: &AISessionSnapshot,
    ) -> Result<bool, String> {
        if session.state != "idle" || !session.has_completed_turn {
            return Ok(false);
        }
        let Some(project) = memory_project_context(projects, session) else {
            return Ok(false);
        };
        let session_id = session_identifier(session);
        if self.has_active_extraction_for_session(&project.project_id, &session.tool, &session_id)?
        {
            return Ok(false);
        }
        let Some(source) = resolve_transcript_source(session, &project) else {
            return Ok(false);
        };
        self.enqueue_extraction_if_needed(
            &project.project_id,
            &project.workspace_path,
            &session.tool,
            &session_id,
            &source.location,
            &source.fingerprint,
            false,
        )
    }

    fn has_active_extraction_for_session(
        &self,
        project_id: &str,
        tool: &str,
        session_id: &str,
    ) -> Result<bool, String> {
        let conn = self.open_or_create_connection()?;
        let count: i64 = conn
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM memory_extraction_queue
                WHERE project_id = ?1
                  AND tool = ?2
                  AND session_id = ?3
                  AND status IN ('pending', 'running');
                "#,
                rusqlite::params![project_id, tool, session_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(count > 0)
    }
}

fn manual_extraction_candidates(
    memory_settings: &AIMemorySettings,
    projects: &[ProjectWorkspaceRecord],
    sessions: &[AISessionSnapshot],
) -> Vec<AISessionSnapshot> {
    let limit = memory_settings.max_index_sessions.max(1) as usize;
    let mut by_project: HashMap<String, Vec<AISessionSnapshot>> = HashMap::new();

    for session in sessions
        .iter()
        .filter(|session| session.state == "idle" && session.has_completed_turn)
    {
        let Some(project) = memory_project_context(projects, session) else {
            continue;
        };
        if resolve_transcript_source(session, &project).is_none() {
            continue;
        }
        by_project
            .entry(project.project_id)
            .or_default()
            .push(session.clone());
    }

    newest_limited_by_project(by_project, limit)
}

fn manual_extraction_candidates_from_history(
    memory_settings: &AIMemorySettings,
    projects: &[ProjectWorkspaceRecord],
    sessions: &[AISessionSummary],
) -> Vec<AISessionSnapshot> {
    let limit = memory_settings.max_index_sessions.max(1) as usize;
    let mut by_project: HashMap<String, Vec<AISessionSnapshot>> = HashMap::new();

    for summary in sessions.iter().filter(|session| {
        session.total_tokens + session.cached_input_tokens + session.request_count > 0
    }) {
        let Some(project) = memory_project_context_from_history(projects, summary) else {
            continue;
        };
        let Some(snapshot) = historical_session_snapshot(summary, &project) else {
            continue;
        };
        if resolve_transcript_source(&snapshot, &project).is_none() {
            continue;
        }
        by_project
            .entry(project.project_id)
            .or_default()
            .push(snapshot);
    }

    newest_limited_by_project(by_project, limit)
}

fn automatic_extraction_candidates(
    memory_settings: &AIMemorySettings,
    projects: &[ProjectWorkspaceRecord],
    sessions: &[AISessionSnapshot],
) -> Vec<AISessionSnapshot> {
    let idle_delay = f64::from(memory_settings.extraction_idle_delay_seconds.max(0));
    let now = now_seconds();
    manual_extraction_candidates(memory_settings, projects, sessions)
        .into_iter()
        .filter(|session| !session.was_interrupted)
        .filter(|session| idle_delay == 0.0 || now - session.updated_at >= idle_delay)
        .collect()
}

fn automatic_extraction_candidates_from_history(
    memory_settings: &AIMemorySettings,
    projects: &[ProjectWorkspaceRecord],
    sessions: &[AISessionSummary],
) -> Vec<AISessionSnapshot> {
    let idle_delay = f64::from(memory_settings.extraction_idle_delay_seconds.max(0));
    let now = now_seconds();
    manual_extraction_candidates_from_history(memory_settings, projects, sessions)
        .into_iter()
        .filter(|session| idle_delay == 0.0 || now - session.updated_at >= idle_delay)
        .collect()
}

fn newest_limited_by_project(
    mut by_project: HashMap<String, Vec<AISessionSnapshot>>,
    limit: usize,
) -> Vec<AISessionSnapshot> {
    let mut candidates = Vec::new();
    for sessions in by_project.values_mut() {
        sessions.sort_by(|left, right| {
            right
                .updated_at
                .partial_cmp(&left.updated_at)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sessions.truncate(limit);
        candidates.extend(sessions.iter().cloned());
    }
    candidates.sort_by(|left, right| {
        left.updated_at
            .partial_cmp(&right.updated_at)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates
}

fn deduplicate_manual_candidates(sessions: Vec<AISessionSnapshot>) -> Vec<AISessionSnapshot> {
    let mut seen = HashSet::new();
    let mut deduplicated = Vec::new();
    for session in sessions {
        if seen.insert(extraction_session_key(&session)) {
            deduplicated.push(session);
        }
    }
    deduplicated.sort_by(|left, right| {
        left.updated_at
            .partial_cmp(&right.updated_at)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    deduplicated
}

fn memory_project_context(
    projects: &[ProjectWorkspaceRecord],
    session: &AISessionSnapshot,
) -> Option<MemoryProjectContext> {
    projects
        .iter()
        .find(|project| {
            project.id == session.project_id || project.root_project_id == session.project_id
        })
        .or_else(|| {
            session.project_path.as_ref().and_then(|path| {
                projects.iter().find(|project| {
                    paths_equivalent(Some(project.workspace_path.as_str()), path)
                        || paths_equivalent(Some(project.root_project_path.as_str()), path)
                })
            })
        })
        .map(|project| MemoryProjectContext {
            project_id: project.root_project_id.clone(),
            project_name: project.root_project_name.clone(),
            workspace_path: project.workspace_path.clone(),
        })
}

fn memory_project_context_from_history(
    projects: &[ProjectWorkspaceRecord],
    session: &AISessionSummary,
) -> Option<MemoryProjectContext> {
    projects
        .iter()
        .find(|project| {
            project.root_project_id == session.project_id
                || paths_equivalent(Some(project.workspace_path.as_str()), &session.project_path)
                || paths_equivalent(Some(project.root_project_path.as_str()), &session.project_path)
        })
        .map(|project| MemoryProjectContext {
            project_id: project.root_project_id.clone(),
            project_name: project.root_project_name.clone(),
            workspace_path: project.workspace_path.clone(),
        })
}

fn historical_session_snapshot(
    session: &AISessionSummary,
    project: &MemoryProjectContext,
) -> Option<AISessionSnapshot> {
    let tool = normalized_string(session.last_tool.as_deref())?.to_lowercase();
    Some(AISessionSnapshot {
        terminal_id: session.session_id.clone(),
        terminal_instance_id: None,
        project_id: project.project_id.clone(),
        project_name: project.project_name.clone(),
        project_path: Some(session.project_path.clone()),
        session_title: session.session_title.clone(),
        tool,
        ai_session_id: session.external_session_id.clone(),
        model: session.last_model.clone(),
        state: "idle".to_string(),
        status: "idle".to_string(),
        is_running: false,
        input_tokens: session.total_input_tokens,
        output_tokens: session.total_output_tokens,
        cached_input_tokens: session.cached_input_tokens,
        total_tokens: session.total_tokens,
        baseline_total_tokens: session.total_tokens,
        baseline_cached_input_tokens: session.cached_input_tokens,
        baseline_resolved: true,
        started_at: Some(session.first_seen_at),
        updated_at: session.last_seen_at,
        active_turn_started_at: None,
        runtime_turn_started_at: None,
        completed_turn_started_at: None,
        has_completed_turn: true,
        was_interrupted: false,
        transcript_path: None,
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
    })
}

fn extraction_session_key(session: &AISessionSnapshot) -> String {
    [
        session.project_id.clone(),
        session.tool.to_lowercase(),
        session_identifier(session),
    ]
    .join("|")
}

#[cfg(test)]
mod tests;
