use super::*;
use std::fs;
use uuid::Uuid;

fn temp_support_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("codux-memory-manual-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn project(path: &str) -> ProjectInfo {
    ProjectInfo {
        id: "project-a".to_string(),
        name: "Project A".to_string(),
        path: path.to_string(),
        exists: true,
        badge: "PA".to_string(),
        badge_symbol: None,
        badge_color_hex: None,
        git_default_push_remote_name: None,
    }
}

fn runtime_session(
    terminal_id: &str,
    ai_session_id: &str,
    transcript_path: &str,
    updated_at: f64,
) -> AISessionSnapshot {
    AISessionSnapshot {
        terminal_id: terminal_id.to_string(),
        terminal_instance_id: None,
        project_id: "project-a".to_string(),
        project_name: "Project A".to_string(),
        project_path: Some(
            std::path::Path::new(transcript_path)
                .parent()
                .unwrap()
                .display()
                .to_string(),
        ),
        session_title: "Task".to_string(),
        tool: "codex".to_string(),
        ai_session_id: Some(ai_session_id.to_string()),
        model: Some("gpt-5".to_string()),
        state: "idle".to_string(),
        status: "idle".to_string(),
        is_running: false,
        input_tokens: 1,
        output_tokens: 2,
        cached_input_tokens: 0,
        total_tokens: 3,
        baseline_total_tokens: 0,
        baseline_cached_input_tokens: 0,
        baseline_resolved: true,
        started_at: Some(updated_at - 10.0),
        updated_at,
        active_turn_started_at: None,
        runtime_turn_started_at: None,
        has_completed_turn: true,
        was_interrupted: false,
        transcript_path: Some(transcript_path.to_string()),
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
    }
}

fn history_session(project_path: &str) -> AISessionSummary {
    AISessionSummary {
        session_id: "history-a".to_string(),
        external_session_id: Some("external-a".to_string()),
        project_id: "project-a".to_string(),
        project_name: "Project A".to_string(),
        project_path: project_path.to_string(),
        session_title: "Historical Task".to_string(),
        first_seen_at: 10.0,
        last_seen_at: 20.0,
        last_tool: Some("Codex".to_string()),
        last_model: Some("gpt-5".to_string()),
        request_count: 1,
        total_input_tokens: 12,
        total_output_tokens: 34,
        total_tokens: 46,
        cached_input_tokens: 5,
        active_duration_seconds: 6,
        today_tokens: 46,
        today_cached_input_tokens: 5,
    }
}

#[test]
fn manual_enqueue_limits_candidates_by_project() {
    let support_dir = temp_support_dir();
    let transcript_dir = support_dir.join("project-a");
    fs::create_dir_all(&transcript_dir).unwrap();
    let older = transcript_dir.join("older.jsonl");
    let newer = transcript_dir.join("newer.jsonl");
    fs::write(&older, "user older\nassistant older\n").unwrap();
    fs::write(&newer, "user newer\nassistant newer\n").unwrap();

    let service = MemoryService::new(support_dir.clone());
    let project = project(&transcript_dir.display().to_string());
    let settings = AIMemorySettings {
        enabled: true,
        max_index_sessions: 1,
        ..Default::default()
    };
    let sessions = vec![
        runtime_session(
            "term-old",
            "session-old",
            &older.display().to_string(),
            10.0,
        ),
        runtime_session(
            "term-new",
            "session-new",
            &newer.display().to_string(),
            20.0,
        ),
    ];

    let result = service
        .enqueue_manual_extraction_candidates(&settings, &[project], &sessions, &[])
        .unwrap();

    assert_eq!(result.checked_count, 1);
    assert_eq!(result.enqueued_count, 1);
    assert_eq!(result.status.pending_count, 1);
    let task = service.next_pending_extraction_task().unwrap().unwrap();
    assert_eq!(task.session_id, "session-new");

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn manual_candidates_deduplicate_by_project_tool_and_session() {
    let transcript_dir = std::env::temp_dir().join(format!("codux-dedup-{}", Uuid::new_v4()));
    fs::create_dir_all(&transcript_dir).unwrap();
    let transcript = transcript_dir.join("session.jsonl");
    fs::write(&transcript, "user\nassistant\n").unwrap();
    let left = runtime_session(
        "term-a",
        "same-session",
        &transcript.display().to_string(),
        1.0,
    );
    let right = runtime_session(
        "term-b",
        "same-session",
        &transcript.display().to_string(),
        2.0,
    );

    let deduplicated = deduplicate_manual_candidates(vec![left, right]);

    assert_eq!(deduplicated.len(), 1);
    assert_eq!(
        deduplicated[0].ai_session_id.as_deref(),
        Some("same-session")
    );

    fs::remove_dir_all(transcript_dir).unwrap();
}

#[test]
fn historical_session_summary_converts_to_runtime_snapshot() {
    let project_path = "/workspace/project-a";
    let project = project(project_path);
    let context =
        memory_project_context_from_history(&[project], &history_session(project_path)).unwrap();
    let snapshot = historical_session_snapshot(&history_session(project_path), &context).unwrap();

    assert_eq!(snapshot.project_id, "project-a");
    assert_eq!(snapshot.tool, "codex");
    assert_eq!(snapshot.ai_session_id.as_deref(), Some("external-a"));
    assert!(snapshot.has_completed_turn);
    assert_eq!(snapshot.total_tokens, 46);
}
