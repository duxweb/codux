use super::state::{
    mark_project_completed, mark_project_progress, mark_project_queued, mark_project_running,
    project_source_fingerprint_unchanged, seed_or_queue_project_state,
    set_project_source_fingerprint,
};
use super::types::AIHistoryIndexerState;
use crate::ai_history_normalized::{
    AIHistoryProjectRequest, AIHistorySnapshot, AIHistorySourceFileFingerprint,
    AIHistorySourceFingerprint, AIProjectUsageSummary,
};
use std::sync::{Arc, Mutex};

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
    assert!(
        !state
            .lock()
            .unwrap()
            .queued_or_running_projects
            .contains(&project.id)
    );
}

#[test]
fn project_source_fingerprint_tracks_changes_per_project() {
    let state = Arc::new(Mutex::new(AIHistoryIndexerState::default()));
    let fingerprint = AIHistorySourceFingerprint {
        files: vec![AIHistorySourceFileFingerprint {
            source: "codex".to_string(),
            path: "/tmp/project/session.jsonl".to_string(),
            modified_millis: 10,
            size: 128,
        }],
    };
    assert!(!project_source_fingerprint_unchanged(
        &state,
        "project-1",
        &fingerprint
    ));
    set_project_source_fingerprint(&state, "project-1", fingerprint.clone());
    assert!(project_source_fingerprint_unchanged(
        &state,
        "project-1",
        &fingerprint
    ));

    let changed = AIHistorySourceFingerprint {
        files: vec![AIHistorySourceFileFingerprint {
            size: 256,
            ..fingerprint.files[0].clone()
        }],
    };
    assert!(!project_source_fingerprint_unchanged(
        &state,
        "project-1",
        &changed
    ));
}

#[test]
fn cache_miss_project_state_is_queued_for_indexing() {
    let state = Arc::new(Mutex::new(AIHistoryIndexerState::default()));
    let project = test_project();

    let (queued, should_enqueue) = seed_or_queue_project_state(&state, &project, None).unwrap();

    assert!(should_enqueue);
    assert!(queued.is_loading);
    assert!(queued.queued);
    assert_eq!(queued.progress, Some(0.0));
    assert!(queued.snapshot.is_none());
    assert!(
        state
            .lock()
            .unwrap()
            .queued_or_running_projects
            .contains(&project.id)
    );
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
