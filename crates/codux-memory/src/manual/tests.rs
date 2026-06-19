use super::*;
use crate::MemoryProjectRecord;
use rusqlite::Connection;
use std::fs;
use uuid::Uuid;

fn temp_support_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("codux-memory-manual-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn project(path: &str) -> MemoryProjectRecord {
    MemoryProjectRecord {
        id: "project-a".to_string(),
        root_project_id: "project-a".to_string(),
        root_project_name: "Project A".to_string(),
        root_project_path: path.to_string(),
        workspace_path: path.to_string(),
        git_default_push_remote_name: None,
    }
}

fn runtime_session(
    terminal_id: &str,
    ai_session_id: &str,
    transcript_path: &str,
    updated_at: f64,
) -> MemorySessionSnapshot {
    MemorySessionSnapshot {
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
        completed_turn_started_at: None,
        has_completed_turn: true,
        was_interrupted: false,
        transcript_path: Some(transcript_path.to_string()),
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
        plan: None,
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
    let settings = MemorySettings {
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
fn automatic_enqueue_respects_idle_delay_and_enabled_flag() {
    let support_dir = temp_support_dir();
    let transcript_dir = support_dir.join("project-a");
    fs::create_dir_all(&transcript_dir).unwrap();
    let recent = transcript_dir.join("recent.jsonl");
    let ready = transcript_dir.join("ready.jsonl");
    fs::write(&recent, "user recent\nassistant recent\n").unwrap();
    fs::write(&ready, "user ready\nassistant ready\n").unwrap();

    let service = MemoryService::new(support_dir.clone());
    let project = project(&transcript_dir.display().to_string());
    let settings = MemorySettings {
        enabled: true,
        automatic_extraction_enabled: true,
        extraction_idle_delay_seconds: 60,
        max_index_sessions: 10,
        ..Default::default()
    };
    let now = now_seconds();
    let sessions = vec![
        runtime_session(
            "term-recent",
            "session-recent",
            &recent.display().to_string(),
            now - 10.0,
        ),
        runtime_session(
            "term-ready",
            "session-ready",
            &ready.display().to_string(),
            now - 120.0,
        ),
    ];

    let result = service
        .enqueue_automatic_extraction_candidates(
            &settings,
            std::slice::from_ref(&project),
            &sessions,
            &[],
        )
        .unwrap();

    assert_eq!(result.checked_count, 1);
    assert_eq!(result.enqueued_count, 1);
    assert_eq!(result.status.pending_count, 1);
    let task = service.next_pending_extraction_task().unwrap().unwrap();
    assert_eq!(task.session_id, "session-ready");

    let disabled = MemorySettings {
        automatic_extraction_enabled: false,
        ..settings
    };
    let result = service
        .enqueue_automatic_extraction_candidates(&disabled, &[project], &sessions, &[])
        .unwrap();
    assert_eq!(result.checked_count, 0);
    assert_eq!(result.enqueued_count, 0);
    assert_eq!(result.status.pending_count, 1);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn automatic_enqueue_keeps_older_ready_runtime_candidates() {
    let support_dir = temp_support_dir();
    let transcript_dir = support_dir.join("project-a");
    fs::create_dir_all(&transcript_dir).unwrap();
    let old = transcript_dir.join("old.jsonl");
    let newer = transcript_dir.join("newer.jsonl");
    fs::write(&old, "user old\nassistant old\n").unwrap();
    fs::write(&newer, "user newer\nassistant newer\n").unwrap();

    let service = MemoryService::new(support_dir.clone());
    let project = project(&transcript_dir.display().to_string());
    let settings = MemorySettings {
        enabled: true,
        automatic_extraction_enabled: true,
        extraction_idle_delay_seconds: 300,
        max_index_sessions: 20,
        ..Default::default()
    };
    let now = now_seconds();
    let sessions = vec![
        runtime_session(
            "term-old",
            "session-old",
            &old.display().to_string(),
            now - 7_200.0,
        ),
        runtime_session(
            "term-newer",
            "session-newer",
            &newer.display().to_string(),
            now - 600.0,
        ),
    ];

    let result = service
        .enqueue_automatic_extraction_candidates(&settings, &[project], &sessions, &[])
        .unwrap();

    assert_eq!(result.checked_count, 2);
    assert_eq!(result.enqueued_count, 2);
    assert_eq!(result.status.pending_count, 2);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn automatic_enqueue_skips_session_with_active_pending_task() {
    let support_dir = temp_support_dir();
    let transcript_dir = support_dir.join("project-a");
    fs::create_dir_all(&transcript_dir).unwrap();
    let first = transcript_dir.join("first.jsonl");
    let updated = transcript_dir.join("updated.jsonl");
    fs::write(&first, "user first\nassistant first\n").unwrap();
    fs::write(&updated, "user first\nassistant first\nuser again\n").unwrap();

    let service = MemoryService::new(support_dir.clone());
    let project = project(&transcript_dir.display().to_string());
    let settings = MemorySettings {
        enabled: true,
        automatic_extraction_enabled: true,
        extraction_idle_delay_seconds: 0,
        max_index_sessions: 10,
        ..Default::default()
    };
    let first_session =
        runtime_session("term-a", "same-session", &first.display().to_string(), 10.0);
    let updated_session = runtime_session(
        "term-a",
        "same-session",
        &updated.display().to_string(),
        20.0,
    );

    let first_result = service
        .enqueue_automatic_extraction_candidates(
            &settings,
            std::slice::from_ref(&project),
            &[first_session],
            &[],
        )
        .unwrap();
    assert_eq!(first_result.checked_count, 1);
    assert_eq!(first_result.enqueued_count, 1);
    assert_eq!(first_result.status.pending_count, 1);

    let updated_result = service
        .enqueue_automatic_extraction_candidates(&settings, &[project], &[updated_session], &[])
        .unwrap();
    assert_eq!(updated_result.checked_count, 1);
    assert_eq!(updated_result.enqueued_count, 0);
    assert_eq!(updated_result.status.pending_count, 1);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn automatic_enqueue_uses_configured_candidate_limit_and_stores_only_task_metadata() {
    let support_dir = temp_support_dir();
    let transcript_dir = support_dir.join("project-a");
    fs::create_dir_all(&transcript_dir).unwrap();

    let service = MemoryService::new(support_dir.clone());
    let project = project(&transcript_dir.display().to_string());
    let settings = MemorySettings {
        enabled: true,
        automatic_extraction_enabled: true,
        extraction_idle_delay_seconds: 0,
        max_index_sessions: 40,
        ..Default::default()
    };
    let sessions = (0..12)
        .map(|index| {
            let transcript = transcript_dir.join(format!("session-{index}.jsonl"));
            fs::write(
                &transcript,
                format!("user large content {index}\nassistant large content {index}\n"),
            )
            .unwrap();
            runtime_session(
                &format!("term-{index}"),
                &format!("session-{index}"),
                &transcript.display().to_string(),
                index as f64,
            )
        })
        .collect::<Vec<_>>();

    let result = service
        .enqueue_automatic_extraction_candidates(&settings, &[project], &sessions, &[])
        .unwrap();

    assert_eq!(result.checked_count, 12);
    assert_eq!(result.enqueued_count, 12);
    assert_eq!(result.status.pending_count, 12);

    let conn = Connection::open(support_dir.join("memory.sqlite3")).unwrap();
    let schema = conn
        .prepare("SELECT * FROM memory_extraction_queue LIMIT 1;")
        .unwrap()
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        schema,
        vec![
            "id",
            "project_id",
            "tool",
            "session_id",
            "transcript_path",
            "workspace_path",
            "source_fingerprint",
            "status",
            "attempts",
            "error",
            "enqueued_at"
        ]
    );

    let stored_paths = {
        let mut statement = conn
            .prepare("SELECT transcript_path FROM memory_extraction_queue;")
            .unwrap();
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(stored_paths.len(), 12);
    assert!(stored_paths.iter().all(|path| path.ends_with(".jsonl")));
    assert!(
        stored_paths
            .iter()
            .all(|path| !path.contains("large content"))
    );

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
