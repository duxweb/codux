use super::helpers::{deterministic_uuid, history_group_key, local_today_start_seconds};
use super::*;
use crate::ai_history_normalized::{AITimeBucket, AIUsageBreakdownItem};
use crate::ai_runtime_state::{AIRuntimeSessionSummary, AIRuntimeStateSummary};
use rusqlite::params;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn rename_and_remove_project_sessions_preserve_summary_totals() {
    let support_dir = temp_support_dir("rename-remove");
    create_test_history_db(&support_dir);

    let service = AIHistoryService::new(support_dir.clone());
    let project_path = "/tmp/codux-gpui";

    let renamed = service
        .rename_project_session(project_path, "session-1", "Renamed session")
        .expect("rename raw session id");
    assert_eq!(renamed.session_count, 2);
    assert_eq!(renamed.project_total_tokens, 150);
    assert_eq!(renamed.sessions[0].title, "Renamed session");

    let grouped_id =
        deterministic_uuid(&history_group_key("codex", "session-2", Some("external-2")));
    let removed = service
        .remove_project_session(project_path, &grouped_id)
        .expect("remove grouped session id");
    assert_eq!(removed.session_count, 1);
    assert_eq!(removed.project_total_tokens, 100);
    assert_eq!(removed.sessions[0].session_key, "session-1");

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn project_session_detail_groups_external_session_files() {
    let support_dir = temp_support_dir("detail");
    create_test_history_db(&support_dir);

    let service = AIHistoryService::new(support_dir.clone());
    let project_path = "/tmp/codux-gpui";
    let grouped_id =
        deterministic_uuid(&history_group_key("codex", "session-2", Some("external-2")));
    let detail = service
        .project_session_detail(project_path, &grouped_id)
        .expect("session detail");

    assert_eq!(detail.title, "Grouped session");
    assert_eq!(detail.source, "codex");
    assert_eq!(detail.external_session_id.as_deref(), Some("external-2"));
    assert_eq!(detail.total_tokens, 50);
    assert_eq!(detail.cached_input_tokens, 5);
    assert_eq!(detail.request_count, 1);
    assert_eq!(detail.files.len(), 1);
    assert_eq!(detail.files[0].file_path, "src/main.rs");

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn project_summary_includes_tauri_ai_panel_aggregates() {
    let support_dir = temp_support_dir("panel-aggregates");
    create_test_history_db(&support_dir);

    let summary = AIHistoryService::new(support_dir.clone()).project_summary("/tmp/codux-gpui");

    assert_eq!(summary.session_count, 2);
    assert!(!summary.heatmap.is_empty());
    assert_eq!(summary.today_time_buckets.len(), 48);
    assert_eq!(summary.tool_breakdown[0].key, "codex");
    assert_eq!(summary.tool_breakdown[0].total_tokens, 150);
    assert_eq!(summary.model_breakdown[0].key, "gpt-5");
    assert_eq!(summary.model_breakdown[0].total_tokens, 150);

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn global_summary_aggregates_projects_and_recent_sessions() {
    let support_dir = temp_support_dir("global");
    create_test_history_db(&support_dir);
    let conn = Connection::open(support_dir.join("ai-usage.sqlite3")).expect("open sqlite");
    conn.execute(
        "INSERT INTO ai_history_project_index_state (project_path, indexed_at) VALUES (?1, ?2)",
        params!["/tmp/other", 2000.0],
    )
    .expect("other index state");
    insert_session(
        &conn,
        "/tmp/other",
        "claude-code",
        "src/lib.rs",
        "other-session",
        Some("claude-external"),
        "Other project",
        2500.0,
        25,
    );

    let summary = AIHistoryService::new(support_dir.clone()).global_summary();

    assert_eq!(summary.indexed_project_count, 2);
    assert_eq!(summary.session_count, 3);
    assert_eq!(summary.total_tokens, 175);
    assert_eq!(summary.cached_input_tokens, 17);
    assert_eq!(summary.today_total_tokens, 175);
    assert_eq!(summary.today_cached_input_tokens, 17);
    assert_eq!(summary.project_totals[0].project_path, "/tmp/codux-gpui");
    assert_eq!(summary.project_totals[0].session_count, 2);
    assert_eq!(summary.recent_sessions[0].title, "Other project");

    fs::remove_dir_all(support_dir).ok();
}

#[test]
fn stats_view_owns_display_token_mode_and_project_filtering() {
    let today_start = crate::ai_history_normalized::local_day_start_seconds(2_000.0);
    let history = AIHistorySummary {
        project_total_tokens: 100,
        project_cached_input_tokens: 40,
        today_total_tokens: 30,
        today_cached_input_tokens: 10,
        today_time_buckets: vec![AITimeBucket {
            start: today_start,
            end: today_start + 1800.0,
            total_tokens: 30,
            cached_input_tokens: 10,
            request_count: 2,
        }],
        tool_breakdown: vec![AIUsageBreakdownItem {
            key: "codex".to_string(),
            total_tokens: 100,
            cached_input_tokens: 40,
            request_count: 2,
        }],
        model_breakdown: vec![AIUsageBreakdownItem {
            key: "gpt-5".to_string(),
            total_tokens: 80,
            cached_input_tokens: 20,
            request_count: 1,
        }],
        ..Default::default()
    };
    let runtime = AIRuntimeStateSummary {
        sessions: vec![
            AIRuntimeSessionSummary {
                project_id: "project-a".to_string(),
                tool: "codex".to_string(),
                model: Some("gpt-5".to_string()),
                total_tokens: 5,
                cached_input_tokens: 3,
                ..runtime_session("term-a")
            },
            AIRuntimeSessionSummary {
                project_id: "project-b".to_string(),
                tool: "claude".to_string(),
                total_tokens: 50,
                cached_input_tokens: 50,
                ..runtime_session("term-b")
            },
        ],
        ..Default::default()
    };

    let normalized = stats_view(&history, &runtime, Some("project-a"), "normalized", 2_000.0);
    assert_eq!(normalized.project_total_tokens, 100);
    assert_eq!(normalized.today_total_tokens, 30);
    assert_eq!(normalized.today_buckets[0].value, 30);
    assert_eq!(normalized.current_sessions.len(), 1);
    assert_eq!(normalized.current_sessions[0].total_tokens, 5);
    assert_eq!(normalized.tool_rows[0].value, 100);

    let with_cache = stats_view(
        &history,
        &runtime,
        Some("project-a"),
        "includingCache",
        2_000.0,
    );
    assert_eq!(with_cache.project_total_tokens, 140);
    assert_eq!(with_cache.today_total_tokens, 40);
    assert_eq!(with_cache.today_buckets[0].value, 40);
    assert_eq!(with_cache.current_sessions[0].total_tokens, 8);
    assert_eq!(with_cache.tool_rows[0].value, 140);
    assert_eq!(with_cache.model_rows[0].value, 100);
}

#[test]
fn stats_view_filters_current_sessions_by_selected_worktree_scope() {
    let runtime = AIRuntimeStateSummary {
        sessions: vec![
            AIRuntimeSessionSummary {
                project_id: "worktree-a".to_string(),
                tool: "codex".to_string(),
                total_tokens: 10,
                ..runtime_session("term-a")
            },
            AIRuntimeSessionSummary {
                project_id: "worktree-b".to_string(),
                tool: "codewhale".to_string(),
                total_tokens: 20,
                ..runtime_session("term-b")
            },
        ],
        ..Default::default()
    };

    let stats = stats_view(
        &AIHistorySummary::default(),
        &runtime,
        Some("worktree-b"),
        "normalized",
        2_000.0,
    );

    assert_eq!(stats.current_sessions.len(), 1);
    assert_eq!(stats.current_sessions[0].tool, "codewhale");
    assert_eq!(stats.current_sessions[0].total_tokens, 20);
}

fn temp_support_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("codux-gpui-ai-history-{label}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp support dir");
    dir
}

fn runtime_session(terminal_id: &str) -> AIRuntimeSessionSummary {
    AIRuntimeSessionSummary {
        terminal_id: terminal_id.to_string(),
        project_id: String::new(),
        project_path: None,
        tool: String::new(),
        ai_session_id: None,
        model: None,
        state: "running".to_string(),
        project_name: "Project".to_string(),
        session_title: "Session".to_string(),
        started_at: None,
        updated_at: 2_000.0,
        event_count: 1,
        has_completed_turn: false,
        was_interrupted: false,
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
        total_tokens: 0,
        cached_input_tokens: 0,
        raw_total_tokens: 0,
        raw_cached_input_tokens: 0,
        baseline_total_tokens: 0,
        baseline_cached_input_tokens: 0,
        source: "test".to_string(),
    }
}

fn create_test_history_db(support_dir: &std::path::Path) {
    let conn = Connection::open(support_dir.join("ai-usage.sqlite3")).expect("open sqlite");
    conn.execute_batch(
        r#"
        CREATE TABLE ai_history_project_index_state (
            project_path TEXT PRIMARY KEY,
            indexed_at REAL NOT NULL
        );
        CREATE TABLE ai_history_file_session_link (
            project_path TEXT NOT NULL,
            source TEXT NOT NULL,
            file_path TEXT NOT NULL,
            session_key TEXT NOT NULL,
            external_session_id TEXT,
            session_title TEXT NOT NULL,
            first_seen_at REAL NOT NULL,
            last_model TEXT,
            last_seen_at REAL NOT NULL,
            active_duration_seconds INTEGER NOT NULL
        );
        CREATE TABLE ai_history_file_usage_bucket (
            project_path TEXT NOT NULL,
            source TEXT NOT NULL,
            file_path TEXT NOT NULL,
            session_key TEXT NOT NULL,
            bucket_start REAL NOT NULL,
            total_tokens INTEGER NOT NULL,
            cached_input_tokens INTEGER NOT NULL,
            request_count INTEGER NOT NULL
        );
        "#,
    )
    .expect("schema");

    conn.execute(
        "INSERT INTO ai_history_project_index_state (project_path, indexed_at) VALUES (?1, ?2)",
        params!["/tmp/codux-gpui", 1000.0],
    )
    .expect("index state");
    insert_session(
        &conn,
        "/tmp/codux-gpui",
        "codex",
        "README.md",
        "session-1",
        None,
        "Original session",
        2000.0,
        100,
    );
    insert_session(
        &conn,
        "/tmp/codux-gpui",
        "codex",
        "src/main.rs",
        "session-2",
        Some("external-2"),
        "Grouped session",
        1500.0,
        50,
    );
}

#[allow(clippy::too_many_arguments)]
fn insert_session(
    conn: &Connection,
    project_path: &str,
    source: &str,
    file_path: &str,
    session_key: &str,
    external_session_id: Option<&str>,
    title: &str,
    last_seen_at: f64,
    total_tokens: i64,
) {
    conn.execute(
        r#"
        INSERT INTO ai_history_file_session_link
            (project_path, source, file_path, session_key, external_session_id, session_title, first_seen_at, last_model, last_seen_at, active_duration_seconds)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            project_path,
            source,
            file_path,
            session_key,
            external_session_id,
            title,
            last_seen_at - 30.0,
            "gpt-5",
            last_seen_at,
            30
        ],
    )
    .expect("session link");
    conn.execute(
        r#"
        INSERT INTO ai_history_file_usage_bucket
            (project_path, source, file_path, session_key, bucket_start, total_tokens, cached_input_tokens, request_count)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            project_path,
            source,
            file_path,
            session_key,
            local_today_start_seconds(),
            total_tokens,
            total_tokens / 10,
            1
        ],
    )
    .expect("usage bucket");
}
