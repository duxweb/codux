use super::helpers::{deterministic_uuid, history_group_key, local_today_start_seconds};
use super::*;
use rusqlite::params;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn restore_command_uses_interactive_session_flags() {
    let mut session = AISessionSummary {
        id: "local-id".to_string(),
        session_key: "session key".to_string(),
        external_session_id: Some("external-1".to_string()),
        title: "Task".to_string(),
        source: "codex".to_string(),
        project_name: None,
        project_path: None,
        last_model: None,
        last_seen_at: 0.0,
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        cached_input_tokens: 0,
        request_count: 0,
        active_duration_seconds: 0,
        usage_amounts: Vec::new(),
    };

    assert_eq!(session_restore_command(&session), "codex resume external-1");

    session.source = "opencode".to_string();
    session.external_session_id = Some("ses_0f1e6192effe3vkDd6vSiCMDrF".to_string());
    assert_eq!(
        session_restore_command(&session),
        "opencode --session ses_0f1e6192effe3vkDd6vSiCMDrF"
    );

    session.source = "mimo".to_string();
    session.external_session_id = None;
    assert_eq!(
        session_restore_command(&session),
        "mimo --session 'session key'"
    );

    session.source = "kiro".to_string();
    session.external_session_id = Some("session-1".to_string());
    assert_eq!(
        session_restore_command(&session),
        "kiro-cli --resume-id session-1"
    );
}

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
fn session_dispatch_preserves_operation_errors() {
    let support_dir = temp_support_dir("dispatch-errors");
    create_test_history_db(&support_dir);
    let service = AIHistoryService::new(support_dir.clone());

    assert_eq!(
        session_op_result(
            &service,
            "/tmp/codux-gpui",
            &serde_json::json!({ "op": "detail", "sessionId": "missing" }),
        ),
        Err("Session not found.".to_string())
    );
    assert_eq!(
        session_op_result(
            &service,
            "/tmp/codux-gpui",
            &serde_json::json!({ "op": "unsupported" }),
        ),
        Err("Unsupported AI session operation: unsupported".to_string())
    );

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
        TestSessionInput {
            project_path: "/tmp/other",
            source: "claude-code",
            file_path: "src/lib.rs",
            session_key: "other-session",
            external_session_id: Some("claude-external"),
            title: "Other project",
            last_seen_at: 2500.0,
            total_tokens: 25,
        },
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

fn temp_support_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("codux-gpui-ai-history-{label}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp support dir");
    dir
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
        TestSessionInput {
            project_path: "/tmp/codux-gpui",
            source: "codex",
            file_path: "README.md",
            session_key: "session-1",
            external_session_id: None,
            title: "Original session",
            last_seen_at: 2000.0,
            total_tokens: 100,
        },
    );
    insert_session(
        &conn,
        TestSessionInput {
            project_path: "/tmp/codux-gpui",
            source: "codex",
            file_path: "src/main.rs",
            session_key: "session-2",
            external_session_id: Some("external-2"),
            title: "Grouped session",
            last_seen_at: 1500.0,
            total_tokens: 50,
        },
    );
}

struct TestSessionInput<'a> {
    project_path: &'a str,
    source: &'a str,
    file_path: &'a str,
    session_key: &'a str,
    external_session_id: Option<&'a str>,
    title: &'a str,
    last_seen_at: f64,
    total_tokens: i64,
}

fn insert_session(conn: &Connection, input: TestSessionInput<'_>) {
    conn.execute(
        r#"
        INSERT INTO ai_history_file_session_link
            (project_path, source, file_path, session_key, external_session_id, session_title, first_seen_at, last_model, last_seen_at, active_duration_seconds)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            input.project_path,
            input.source,
            input.file_path,
            input.session_key,
            input.external_session_id,
            input.title,
            input.last_seen_at - 30.0,
            "gpt-5",
            input.last_seen_at,
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
            input.project_path,
            input.source,
            input.file_path,
            input.session_key,
            local_today_start_seconds(),
            input.total_tokens,
            input.total_tokens / 10,
            1
        ],
    )
    .expect("usage bucket");
}
