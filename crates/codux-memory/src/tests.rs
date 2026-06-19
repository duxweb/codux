use super::*;
use crate::{
    MemoryConfig, MemoryConfig as AppAISettings, MemoryProjectRecord, MemorySessionSnapshot,
    MemorySettings,
    extraction::{
        MemoryExtractionItem, MemoryExtractionResponse, MemoryKind, MemoryScope, MemoryTier,
    },
};
use std::fs;
use uuid::Uuid;

fn temp_support_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("codux-gpui-memory-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn create_memory_db(support_dir: &std::path::Path) {
    let conn = Connection::open(support_dir.join("memory.sqlite3")).unwrap();
    conn.execute_batch(
        r#"
            CREATE TABLE memory_entries (
                id TEXT PRIMARY KEY,
                scope TEXT NOT NULL,
                project_id TEXT,
                tool_id TEXT,
                tier TEXT NOT NULL,
                kind TEXT NOT NULL,
                content TEXT NOT NULL,
                rationale TEXT,
                source_tool TEXT,
                source_session_id TEXT,
                source_fingerprint TEXT,
                normalized_hash TEXT NOT NULL,
                superseded_by TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                merged_summary_id TEXT,
                merged_at REAL,
                archived_at REAL,
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at REAL,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL,
                module_key TEXT
            );
            CREATE TABLE memory_summaries (
                id TEXT PRIMARY KEY,
                scope TEXT NOT NULL,
                project_id TEXT,
                tool_id TEXT,
                content TEXT NOT NULL,
                version INTEGER NOT NULL,
                source_entry_ids TEXT NOT NULL,
                token_estimate INTEGER NOT NULL,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL
            );
            CREATE TABLE memory_extraction_queue (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                tool TEXT NOT NULL,
                session_id TEXT NOT NULL,
                transcript_path TEXT NOT NULL,
                source_fingerprint TEXT UNIQUE,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL,
                error TEXT,
                enqueued_at REAL NOT NULL,
                workspace_path TEXT
            );
            CREATE TABLE memory_project_profiles (
                project_id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                source_fingerprint TEXT,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL
            );
            CREATE TABLE memory_decision_logs (
                id TEXT PRIMARY KEY,
                decision TEXT NOT NULL,
                entry_id TEXT,
                target_entry_id TEXT,
                reason TEXT NOT NULL,
                created_at REAL NOT NULL
            );
            "#,
    )
    .unwrap();

    let entries = [
        (
            "project-active",
            "project",
            Some("project-a"),
            "core",
            "decision",
            "active",
            "project active entry",
            30.0,
        ),
        (
            "project-archived",
            "project",
            Some("project-a"),
            "working",
            "note",
            "archived",
            "project archived entry",
            20.0,
        ),
        (
            "user-active",
            "user",
            None,
            "working",
            "preference",
            "active",
            "user active entry",
            10.0,
        ),
        (
            "other-project",
            "project",
            Some("project-b"),
            "core",
            "decision",
            "active",
            "other project entry",
            40.0,
        ),
    ];
    for (id, scope, project_id, tier, kind, status, content, updated_at) in entries {
        conn.execute(
            r#"
                INSERT INTO memory_entries (
                    id, scope, project_id, tier, kind, content, normalized_hash,
                    status, created_at, updated_at, module_key
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, 'general')
                "#,
            params![
                id, scope, project_id, tier, kind, content, id, status, updated_at
            ],
        )
        .unwrap();
    }
    conn.execute(
            "INSERT INTO memory_extraction_queue VALUES ('q1', 'project-a', 'codex', 's1', '/tmp/t', 'fp1', 'queued', 0, NULL, 1, '/tmp')",
            [],
        )
        .unwrap();
    conn.execute(
        "INSERT INTO memory_project_profiles VALUES ('project-a', 'profile', 'fp', 1, 1)",
        [],
    )
    .unwrap();
    conn.execute(
            "INSERT INTO memory_summaries VALUES ('summary-a', 'project', 'project-a', NULL, 'project summary', 1, '[\"project-active\"]', 10, 1, 50)",
            [],
        )
        .unwrap();
}

#[test]
fn summary_includes_active_and_archived_recent_entries() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let summary = MemoryService::new(support_dir.clone()).summary(Some("project-a"));

    assert!(summary.available);
    // active_entries is now project-scoped (project-a + user) and consistent
    // with core/working: project-active + user-active = 2.
    assert_eq!(summary.active_entries, 2);
    assert_eq!(summary.core_entries, 1);
    assert_eq!(summary.working_entries, 1);
    assert_eq!(summary.archived_entries, 1);
    assert_eq!(summary.queued_extractions, 1);
    assert!(summary.project_profile_present);
    // Injection recall is active-only and ranked core-first.
    assert_eq!(summary.recent_entries[0].id, "project-active");
    assert_eq!(summary.recent_entries[0].status, "active");
    assert!(
        summary
            .recent_entries
            .iter()
            .all(|entry| entry.status == "active"),
        "archived entries must not be injected into the launch context"
    );
    assert!(
        !summary
            .recent_entries
            .iter()
            .any(|entry| entry.id == "project-archived")
    );
    assert!(
        !summary
            .recent_entries
            .iter()
            .any(|entry| entry.id == "other-project")
    );

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn archive_and_restore_memory_entry() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());

    let archived = service
        .set_entry_status(Some("project-a"), "project-active", "archived")
        .unwrap();
    assert_eq!(archived.archived_entries, 2);
    // Archived entries are excluded from injection recall.
    assert!(
        !archived
            .recent_entries
            .iter()
            .any(|entry| entry.id == "project-active")
    );

    let restored = service
        .set_entry_status(Some("project-a"), "project-active", "active")
        .unwrap();
    assert_eq!(restored.archived_entries, 1);
    assert!(
        restored
            .recent_entries
            .iter()
            .any(|entry| entry.id == "project-active" && entry.status == "active")
    );

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn prepare_launch_artifacts_writes_memory_context_files() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());

    let artifacts = service
        .prepare_launch_artifacts_for_project(
            &std::env::temp_dir(),
            "project-a",
            "Project A",
            "/workspace/project-a",
        )
        .unwrap();
    let index = fs::read_to_string(&artifacts.index_file).unwrap();
    let prompt = fs::read_to_string(&artifacts.prompt_file).unwrap();
    let recent = fs::read_to_string(artifacts.workspace_root.join("memory-recent.md")).unwrap();

    assert!(index.contains("Project: Project A"));
    assert!(index.contains("project active entry"));
    assert_eq!(index, prompt);
    assert!(recent.contains("user active entry"));

    fs::remove_dir_all(support_dir).unwrap();
    let _ = fs::remove_dir_all(artifacts.workspace_root);
}

#[test]
fn launch_request_respects_cross_project_user_memory_setting() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());
    let mut settings = AppAISettings::default();
    settings.memory.allow_cross_project_user_recall = false;

    let artifacts = service
        .prepare_launch_artifacts(&std::env::temp_dir(), MemoryLaunchRequest {
            project_id: "project-a".to_string(),
            project_name: "Project A".to_string(),
            workspace_path: Some("/workspace/project-a".to_string()),
            settings,
            extra_context: None,
        })
        .unwrap();

    let index = fs::read_to_string(&artifacts.index_file).unwrap();
    assert!(index.contains("project active entry"));
    assert!(!index.contains("user active entry"));

    fs::remove_dir_all(support_dir).unwrap();
    let _ = fs::remove_dir_all(artifacts.workspace_root);
}

#[test]
fn launch_request_includes_global_prompt_even_when_memory_injection_is_disabled() {
    let support_dir = temp_support_dir();
    let service = MemoryService::new(support_dir.clone());
    let mut settings = crate::MemoryConfig::default();
    settings.global_prompt = "Always prefer small runtime modules.".to_string();
    settings.memory.enabled = false;

    let artifacts = service
        .prepare_launch_artifacts(&std::env::temp_dir(), MemoryLaunchRequest {
            project_id: "project-launch".to_string(),
            project_name: "Launch Project".to_string(),
            workspace_path: Some("/workspace/launch".to_string()),
            settings,
            extra_context: Some("Extra launch note.".to_string()),
        })
        .unwrap();

    let index = fs::read_to_string(&artifacts.index_file).unwrap();
    assert!(index.contains("Always prefer small runtime modules."));
    assert!(index.contains("Extra launch note."));

    fs::remove_dir_all(support_dir).unwrap();
    let _ = fs::remove_dir_all(artifacts.workspace_root);
}

#[test]
fn extraction_prompt_context_respects_cross_project_user_memory_setting() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());
    let mut settings = MemorySettings::default();
    settings.allow_cross_project_user_recall = false;
    settings.max_injected_project_working_memories = 5;

    let (user_summary, user_memories, project_memories) = service
        .extraction_prompt_context(&settings, "project-a", "project active entry")
        .unwrap();

    assert!(user_summary.is_none());
    assert!(user_memories.is_empty());
    assert!(
        project_memories
            .iter()
            .any(|entry| entry.content == "project active entry")
    );

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn project_scope_prevents_archiving_other_project_entries() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());

    let error = service
        .set_entry_status(Some("project-a"), "other-project", "archived")
        .unwrap_err();

    assert_eq!(error, "Memory entry not found.");
    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn manager_snapshot_includes_targets_profile_entries_and_summaries() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());
    let projects = vec![MemoryProjectInfo {
        id: "project-a".to_string(),
        name: "Project A".to_string(),
        path: "/workspace/project-a".to_string(),
    }];

    let active = service.manager_snapshot(&projects, "project", Some("project-a"), "active", 50);
    assert!(active.available);
    assert_eq!(active.selected_target_title, "Project A");
    assert_eq!(active.current_overview.active_entry_count, 1);
    assert_eq!(active.current_overview.profile_count, 1);
    assert!(active.project_profile.is_some());
    assert!(
        active
            .entries
            .iter()
            .any(|entry| entry.id == "project-active")
    );
    assert!(
        active
            .target_rows
            .iter()
            .any(|row| row.project_id.as_deref() == Some("project-a") && row.is_open_project)
    );

    let summaries =
        service.manager_snapshot(&projects, "project", Some("project-a"), "summary", 50);
    assert!(
        summaries
            .summaries
            .iter()
            .any(|summary| summary.id == "summary-a")
    );

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn delete_entry_summary_and_project_profile_are_scoped() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());

    let error = service
        .delete_entry(Some("project-a"), "other-project")
        .unwrap_err();
    assert_eq!(error, "Memory entry not found.");

    let summary = service
        .delete_entry(Some("project-a"), "project-active")
        .unwrap();
    assert!(
        !summary
            .recent_entries
            .iter()
            .any(|entry| entry.id == "project-active")
    );
    let summary = service
        .delete_summary(Some("project-a"), "summary-a")
        .unwrap();
    assert_eq!(summary.summaries, 0);
    let summary = service.delete_project_profile("project-a").unwrap();
    assert!(!summary.project_profile_present);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn update_summary_preserves_sources_and_versions_content() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());
    let source_id = Uuid::new_v4().to_string();
    let conn = Connection::open(support_dir.join("memory.sqlite3")).unwrap();
    conn.execute(
        "UPDATE memory_summaries SET source_entry_ids = ?1 WHERE id = 'summary-a'",
        params![serde_json::to_string(&vec![source_id.clone()]).unwrap()],
    )
    .unwrap();

    let updated = service
        .update_summary(MemorySummaryUpdateRequest {
            summary_id: "summary-a".to_string(),
            content: "updated project summary".to_string(),
            max_versions: Some(5),
        })
        .unwrap();

    assert_eq!(updated.id, "summary-a");
    assert_eq!(updated.content, "updated project summary");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.source_entry_ids, vec![source_id]);
    let version_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_summary_versions WHERE summary_id = 'summary-a'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version_count, 1);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn migrate_project_memory_requires_overwrite_for_existing_target() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());

    let blocked = service
        .migrate_project_memory(MemoryProjectMigrationRequest {
            from_project_id: "project-a".to_string(),
            to_project_id: "project-b".to_string(),
            overwrite: false,
        })
        .unwrap_err();
    assert_eq!(blocked, "target project already has memory");

    service
        .migrate_project_memory(MemoryProjectMigrationRequest {
            from_project_id: "project-a".to_string(),
            to_project_id: "project-b".to_string(),
            overwrite: true,
        })
        .unwrap();

    let conn = Connection::open(support_dir.join("memory.sqlite3")).unwrap();
    let source_entries: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_entries WHERE project_id = 'project-a'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let target_entries: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_entries WHERE project_id = 'project-b'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let target_profiles: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_project_profiles WHERE project_id = 'project-b'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source_entries, 0);
    assert_eq!(target_entries, 2);
    assert_eq!(target_profiles, 1);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn enqueue_completed_session_creates_pending_memory_task() {
    let support_dir = temp_support_dir();
    let transcript_dir = std::env::temp_dir().join(format!("codux-transcript-{}", Uuid::new_v4()));
    fs::create_dir_all(&transcript_dir).unwrap();
    let transcript = transcript_dir.join("session.jsonl");
    fs::write(&transcript, "user\nassistant\n").unwrap();
    let service = MemoryService::new(support_dir.clone());
    let settings = MemorySettings {
        enabled: true,
        automatic_extraction_enabled: true,
        extraction_idle_delay_seconds: 0,
        ..Default::default()
    };
    let project = MemoryProjectRecord {
        id: "project-a".to_string(),
        root_project_id: "project-a".to_string(),
        root_project_name: "Project A".to_string(),
        root_project_path: transcript_dir.display().to_string(),
        workspace_path: transcript_dir.display().to_string(),
        git_default_push_remote_name: None,
    };
    let session = MemorySessionSnapshot {
        terminal_id: "term-a".to_string(),
        terminal_instance_id: None,
        project_id: "project-a".to_string(),
        project_name: "Project A".to_string(),
        project_path: Some(transcript_dir.display().to_string()),
        session_title: "Task".to_string(),
        tool: "codex".to_string(),
        ai_session_id: Some("session-a".to_string()),
        model: None,
        state: "idle".to_string(),
        status: "idle".to_string(),
        is_running: false,
        input_tokens: 1,
        output_tokens: 1,
        cached_input_tokens: 0,
        total_tokens: 2,
        baseline_total_tokens: 0,
        baseline_cached_input_tokens: 0,
        baseline_resolved: true,
        started_at: Some(1.0),
        updated_at: now_seconds() - 10.0,
        active_turn_started_at: None,
        runtime_turn_started_at: None,
        completed_turn_started_at: None,
        has_completed_turn: true,
        was_interrupted: false,
        transcript_path: Some(transcript.display().to_string()),
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
        plan: None,
    };

    let result = service
        .enqueue_completed_session_if_ready(&settings, std::slice::from_ref(&project), &session)
        .unwrap();
    assert!(result.enqueued);
    assert_eq!(result.summary.queued_extractions, 1);

    let duplicate = service
        .enqueue_completed_session_if_ready(&settings, std::slice::from_ref(&project), &session)
        .unwrap();
    assert!(!duplicate.enqueued);

    fs::remove_dir_all(support_dir).unwrap();
    fs::remove_dir_all(transcript_dir).unwrap();
}

#[test]
fn extraction_queue_status_and_task_lifecycle() {
    let support_dir = temp_support_dir();
    let service = MemoryService::new(support_dir.clone());

    let idle = service.extraction_status_snapshot().unwrap();
    assert_eq!(idle.status, MemoryExtractionStatus::Idle);

    assert!(
        service
            .enqueue_extraction_if_needed(
                "project-a",
                "/workspace/project-a",
                "codex",
                "session-a",
                "/tmp/session-a.jsonl",
                "fingerprint-a",
                false,
            )
            .unwrap()
    );
    let queued = service.extraction_status_snapshot().unwrap();
    assert_eq!(queued.status, MemoryExtractionStatus::Queued);
    assert_eq!(queued.pending_count, 1);
    assert!(service.has_pending_extraction_task().unwrap());

    let task = service.next_pending_extraction_task().unwrap().unwrap();
    assert_eq!(task.project_id, "project-a");
    service.mark_extraction_task_running(&task.id).unwrap();
    let running = service.extraction_status_snapshot().unwrap();
    assert_eq!(running.status, MemoryExtractionStatus::Processing);

    service
        .mark_extraction_task_failed(&task.id, "provider unavailable")
        .unwrap();
    let failed = service.extraction_status_snapshot().unwrap();
    assert_eq!(failed.status, MemoryExtractionStatus::Failed);
    assert_eq!(failed.last_error.as_deref(), Some("provider unavailable"));

    assert!(
        service
            .enqueue_extraction_if_needed(
                "project-a",
                "/workspace/project-a",
                "codex",
                "session-a",
                "/tmp/session-a.jsonl",
                "fingerprint-a",
                true,
            )
            .unwrap()
    );
    let task = service.next_pending_extraction_task().unwrap().unwrap();
    service.mark_extraction_task_done(&task.id).unwrap();
    assert!(!service.has_pending_extraction_task().unwrap());

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn manager_snapshot_lists_failed_extractions_and_retry_requeues_task() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());
    let conn = Connection::open(support_dir.join("memory.sqlite3")).unwrap();
    conn.execute(
        r#"
        INSERT INTO memory_extraction_queue (
            id, project_id, tool, session_id, transcript_path, source_fingerprint,
            status, attempts, error, enqueued_at, workspace_path
        )
        VALUES (
            'failed-task-a', 'project-a', 'claude', 'session-failed',
            '/tmp/session-failed.jsonl', 'failed-fingerprint-a',
            'failed', 1, 'provider returned malformed memory JSON', 99, '/workspace/project-a'
        );
        "#,
        [],
    )
    .unwrap();
    let projects = vec![MemoryProjectInfo {
        id: "project-a".to_string(),
        name: "Project A".to_string(),
        path: "/workspace/project-a".to_string(),
    }];

    let failed = service.manager_snapshot(&projects, "project", Some("project-a"), "failed", 50);
    assert_eq!(failed.failed_extractions.len(), 1);
    assert_eq!(failed.failed_extractions[0].id, "failed-task-a");
    assert_eq!(
        failed.failed_extractions[0].error.as_deref(),
        Some("provider returned malformed memory JSON")
    );

    let status = service
        .retry_failed_extraction_task("failed-task-a")
        .unwrap();
    assert_eq!(status.status, MemoryExtractionStatus::Queued);
    assert_eq!(status.pending_count, 1);
    assert_eq!(status.last_error, None);

    let retried = service
        .next_pending_extraction_task()
        .unwrap()
        .expect("retried failed task should be pending");
    assert_eq!(retried.id, "failed-task-a");
    assert_eq!(retried.status, "pending");
    assert_eq!(retried.error, None);

    let failed = service.manager_snapshot(&projects, "project", Some("project-a"), "failed", 50);
    assert!(failed.failed_extractions.is_empty());

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn manager_snapshot_lists_active_extraction_queue() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());
    let conn = Connection::open(support_dir.join("memory.sqlite3")).unwrap();
    conn.execute(
        r#"
        INSERT INTO memory_extraction_queue (
            id, project_id, tool, session_id, transcript_path, source_fingerprint,
            status, attempts, error, enqueued_at, workspace_path
        )
        VALUES
          ('pending-task-a', 'project-a', 'codex', 'session-pending',
           '/tmp/session-pending.jsonl', 'pending-fingerprint-a',
           'pending', 0, NULL, 100, '/workspace/project-a'),
          ('running-task-a', 'project-a', 'claude', 'session-running',
           '/tmp/session-running.jsonl', 'running-fingerprint-a',
           'running', 1, NULL, 101, '/workspace/project-a'),
          ('failed-task-a', 'project-a', 'claude', 'session-failed',
           '/tmp/session-failed.jsonl', 'failed-fingerprint-a',
           'failed', 1, 'provider returned malformed memory JSON', 102, '/workspace/project-a');
        "#,
        [],
    )
    .unwrap();
    let projects = vec![MemoryProjectInfo {
        id: "project-a".to_string(),
        name: "Project A".to_string(),
        path: "/workspace/project-a".to_string(),
    }];

    let queue = service.manager_snapshot(&projects, "project", Some("project-a"), "queue", 50);
    assert_eq!(queue.queued_extractions.len(), 3);
    assert_eq!(queue.queued_extractions[0].id, "running-task-a");
    assert!(
        queue
            .queued_extractions
            .iter()
            .any(|task| task.id == "pending-task-a")
    );
    assert!(
        !queue
            .queued_extractions
            .iter()
            .any(|task| task.id == "failed-task-a")
    );
    assert!(queue.failed_extractions.is_empty());

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn clears_individual_extraction_tasks_by_allowed_status() {
    let support_dir = temp_support_dir();
    create_memory_db(&support_dir);
    let service = MemoryService::new(support_dir.clone());
    let conn = Connection::open(support_dir.join("memory.sqlite3")).unwrap();
    conn.execute(
        r#"
        INSERT INTO memory_extraction_queue (
            id, project_id, tool, session_id, transcript_path, source_fingerprint,
            status, attempts, error, enqueued_at, workspace_path
        )
        VALUES
          ('pending-task-a', 'project-a', 'codex', 'session-pending',
           '/tmp/session-pending.jsonl', 'pending-fingerprint-a',
           'pending', 0, NULL, 100, '/workspace/project-a'),
          ('running-task-a', 'project-a', 'claude', 'session-running',
           '/tmp/session-running.jsonl', 'running-fingerprint-a',
           'running', 1, NULL, 101, '/workspace/project-a'),
          ('failed-task-a', 'project-a', 'claude', 'session-failed',
           '/tmp/session-failed.jsonl', 'failed-fingerprint-a',
           'failed', 1, 'provider returned malformed memory JSON', 102, '/workspace/project-a');
        "#,
        [],
    )
    .unwrap();

    service
        .clear_extraction_task("pending-task-a", &["queued", "pending"])
        .unwrap();

    let running_error = service
        .clear_extraction_task("running-task-a", &["queued", "pending"])
        .unwrap_err();
    assert_eq!(running_error, "Memory extraction task not found.");

    service
        .clear_extraction_task("failed-task-a", &["failed"])
        .unwrap();

    let pending_status: String = conn
        .query_row(
            "SELECT status FROM memory_extraction_queue WHERE id = 'pending-task-a';",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let running_status: String = conn
        .query_row(
            "SELECT status FROM memory_extraction_queue WHERE id = 'running-task-a';",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let failed_status: String = conn
        .query_row(
            "SELECT status FROM memory_extraction_queue WHERE id = 'failed-task-a';",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pending_status, "cleared");
    assert_eq!(running_status, "running");
    assert_eq!(failed_status, "cleared");

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn resolves_extraction_task_transcript_from_file() {
    let support_dir = temp_support_dir();
    let transcript_dir = std::env::temp_dir().join(format!("codux-transcript-{}", Uuid::new_v4()));
    fs::create_dir_all(&transcript_dir).unwrap();
    let transcript = transcript_dir.join("session.jsonl");
    fs::write(
        &transcript,
        "stdout: noisy\nuser asked for runtime migration\n",
    )
    .unwrap();
    let service = MemoryService::new(support_dir.clone());
    let project = MemoryProjectRecord {
        id: "project-a".to_string(),
        root_project_id: "project-a".to_string(),
        root_project_name: "Project A".to_string(),
        root_project_path: transcript_dir.display().to_string(),
        workspace_path: transcript_dir.display().to_string(),
        git_default_push_remote_name: None,
    };
    let task = MemoryExtractionTask {
        id: "task-a".to_string(),
        project_id: "project-a".to_string(),
        tool: "codex".to_string(),
        session_id: "session-a".to_string(),
        transcript_path: transcript.display().to_string(),
        workspace_path: Some(transcript_dir.display().to_string()),
        source_fingerprint: "fingerprint-a".to_string(),
        status: "pending".to_string(),
        attempts: 0,
        error: None,
        enqueued_at: 1.0,
    };

    let text = service
        .resolve_extraction_task_transcript(&[project], &task)
        .unwrap();
    assert!(text.contains("user asked for runtime migration"));

    fs::remove_dir_all(support_dir).unwrap();
    fs::remove_dir_all(transcript_dir).unwrap();
}

#[test]
fn resolves_extraction_transcript_for_memory_uses_settings_boundary() {
    let support_dir = temp_support_dir();
    let transcript_dir = std::env::temp_dir().join(format!("codux-transcript-{}", Uuid::new_v4()));
    fs::create_dir_all(&transcript_dir).unwrap();
    let transcript = transcript_dir.join("session.jsonl");
    let mut lines = Vec::new();
    for index in 0..30 {
        lines.push(format!("stdout: noisy output line {index}"));
    }
    lines.push("user: old durable fact should be outside boundary".to_string());
    for index in 0..20 {
        lines.push(format!("stderr: build noise {index}"));
    }
    lines.push("user: keep recent memory boundary decision".to_string());
    lines.push("assistant: implemented memory transcript boundary".to_string());
    fs::write(&transcript, lines.join("\n")).unwrap();
    let project = MemoryProjectRecord {
        id: "project-a".to_string(),
        root_project_id: "project-a".to_string(),
        root_project_name: "Project A".to_string(),
        root_project_path: transcript_dir.display().to_string(),
        workspace_path: transcript_dir.display().to_string(),
        git_default_push_remote_name: None,
    };
    let task = MemoryExtractionTask {
        id: "task-a".to_string(),
        project_id: "project-a".to_string(),
        tool: "codex".to_string(),
        session_id: "session-a".to_string(),
        transcript_path: transcript.display().to_string(),
        workspace_path: Some(transcript_dir.display().to_string()),
        source_fingerprint: "fingerprint-a".to_string(),
        status: "pending".to_string(),
        attempts: 0,
        error: None,
        enqueued_at: 1.0,
    };
    let context = crate::transcript::memory_project_context_for_task(&[project], &task)
        .expect("project context");
    let text = crate::transcript::resolve_transcript_for_task_with_settings(
        &task,
        &context,
        &MemorySettings {
            max_extraction_transcript_lines: 8,
            max_extraction_transcript_tokens: 2000,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(text.contains("keep recent memory boundary decision"));
    assert!(text.contains("implemented memory transcript boundary"));
    assert!(!text.contains("old durable fact should be outside boundary"));
    assert!(text.contains("omitted"));

    fs::remove_dir_all(support_dir).unwrap();
    fs::remove_dir_all(transcript_dir).unwrap();
}

#[test]
fn apply_extraction_response_writes_memory_and_summary() {
    let support_dir = temp_support_dir();
    let service = MemoryService::new(support_dir.clone());
    service.ensure_queue_schema().unwrap();
    let task = MemoryExtractionTask {
        id: "task-a".to_string(),
        project_id: "project-a".to_string(),
        tool: "codex".to_string(),
        session_id: "session-a".to_string(),
        transcript_path: "/tmp/session-a.jsonl".to_string(),
        workspace_path: Some("/workspace/project-a".to_string()),
        source_fingerprint: "fingerprint-a".to_string(),
        status: "pending".to_string(),
        attempts: 0,
        error: None,
        enqueued_at: 1.0,
    };
    let settings = MemorySettings {
        max_active_working_entries: 10,
        max_summary_versions: 3,
        ..Default::default()
    };

    service
            .apply_extraction_response(
                MemoryExtractionResponse {
                    user_summary: Some("User prefers small maintainable modules.".to_string()),
                    working_add: vec![MemoryExtractionItem {
                        scope: Some(MemoryScope::Project),
                        module_key: Some("runtime".to_string()),
                        tier: Some(MemoryTier::Working),
                        kind: MemoryKind::Convention,
                        content: "Runtime migration should keep backend logic in small domain modules instead of one large file.".to_string(),
                        rationale: Some("Maintains readability during the GPUI migration.".to_string()),
                        merge_with: Vec::new(),
                        replace: None,
                        archive: Vec::new(),
                        skip_reason: None,
                    }],
                    working_archive: Vec::new(),
                    merged_entry_ids: Vec::new(),
                    project_profile_refresh_recommended: false,
                },
                &task,
                &settings,
            )
            .unwrap();

    let conn = Connection::open(support_dir.join("memory.sqlite3")).unwrap();
    let entry_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_entries WHERE project_id = 'project-a' AND status = 'active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
    let decision_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_decision_logs", [], |row| {
            row.get(0)
        })
        .unwrap();
    let summary_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_summaries WHERE scope = 'user'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(entry_count, 1);
    assert_eq!(decision_count, 1);
    assert_eq!(summary_count, 1);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn extraction_response_recommendation_refreshes_project_profile() {
    let support_dir = temp_support_dir();
    let project_dir = temp_support_dir();
    fs::write(
        project_dir.join("Cargo.toml"),
        "[package]\nname = \"codux-profile-refresh\"\n",
    )
    .unwrap();
    let service = MemoryService::new(support_dir.clone());
    service.ensure_queue_schema().unwrap();
    let task = MemoryExtractionTask {
        id: "task-profile".to_string(),
        project_id: "project-profile".to_string(),
        tool: "codex".to_string(),
        session_id: "session-profile".to_string(),
        transcript_path: "/tmp/session-profile.jsonl".to_string(),
        workspace_path: Some(project_dir.display().to_string()),
        source_fingerprint: "fingerprint-profile".to_string(),
        status: "pending".to_string(),
        attempts: 0,
        error: None,
        enqueued_at: 1.0,
    };
    let project = MemoryProjectInfo {
        id: "project-profile".to_string(),
        name: "Profile Project".to_string(),
        path: project_dir.display().to_string(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    runtime
        .block_on(service.apply_extraction_response_with_profile_refresh(
            MemoryExtractionResponse {
                project_profile_refresh_recommended: true,
                ..Default::default()
            },
            &task,
            &MemoryConfig {
                memory: MemorySettings {
                    enabled: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &project,
        ))
        .unwrap();

    let conn = Connection::open(support_dir.join("memory.sqlite3")).unwrap();
    let content: String = conn
        .query_row(
            "SELECT content FROM memory_project_profiles WHERE project_id = 'project-profile'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(content.contains("Native/runtime: Rust"));

    fs::remove_dir_all(support_dir).unwrap();
    fs::remove_dir_all(project_dir).unwrap();
}

#[test]
fn process_next_memory_extraction_task_returns_idle_without_pending_work() {
    let support_dir = temp_support_dir();
    let service = MemoryService::new(support_dir.clone());
    service.ensure_queue_schema().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    let status = runtime
        .block_on(service.process_next_memory_extraction_task(
            &crate::MemoryConfig {
                memory: MemorySettings {
                    enabled: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &[],
            "en",
        ))
        .unwrap();
    assert_eq!(status.status, MemoryExtractionStatus::Idle);

    fs::remove_dir_all(support_dir).unwrap();
}
