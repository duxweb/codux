#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalized::{JSONLParseSnapshot, load_indexed_global_history_at};
    use chrono::TimeZone;
    use uuid::Uuid;

    #[test]
    fn initializes_normalized_schema() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();

        let version: String = conn
            .query_row(
                "SELECT value FROM ai_history_meta WHERE key = 'normalized_history_schema_version';",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, NORMALIZED_HISTORY_SCHEMA_VERSION);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unchanged_file_reuses_persisted_summary() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "{}\n").unwrap();
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        let mut parse_count = 0;

        let first = store
            .load_or_index_file(&conn, "claude", &file_path, &project, || {
                parse_count += 1;
                parsed_history("s1", 100.0, 100, 50)
            })
            .unwrap();
        let second = store
            .load_or_index_file(&conn, "claude", &file_path, &project, || {
                parse_count += 1;
                ParsedHistory::default()
            })
            .unwrap();

        assert_eq!(parse_count, 1);
        assert_eq!(first.usage_buckets.len(), 1);
        assert_eq!(second.usage_buckets.len(), 1);
        assert_eq!(second.usage_buckets[0].total_tokens, 150);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn persists_half_hour_bucket_and_project_snapshot() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "{}\n").unwrap();
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        let timestamp = chrono::Local
            .with_ymd_and_hms(2026, 5, 17, 10, 42, 0)
            .single()
            .unwrap()
            .timestamp() as f64;

        store
            .load_or_index_file(&conn, "codex", &file_path, &project, || {
                parsed_history("s2", timestamp, 80, 20)
            })
            .unwrap();
        let stored = store
            .stored_external_summary(
                &conn,
                "codex",
                &normalized_path(&file_path),
                &project.path,
                None,
            )
            .unwrap()
            .unwrap();
        let project_path = project.path.clone();
        let snapshot = store.project_snapshot(&conn, project.clone()).unwrap();

        assert_eq!(
            stored.usage_buckets[0].bucket_start,
            half_hour_bucket_start(timestamp)
        );
        assert_eq!(snapshot.project_summary.project_total_tokens, 100);
        assert_eq!(snapshot.today_time_buckets.len(), 48);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert_eq!(snapshot.tool_breakdown[0].key, "codex");
        store
            .save_project_index_state(&conn, &snapshot, &project_path)
            .unwrap();
        assert!(
            store
                .indexed_project_snapshot(&conn, project)
                .unwrap()
                .is_some()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn global_today_tokens_excludes_historical_project_totals() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let today_start = local_day_start_seconds(now_seconds());
        let old_start = today_start - 7_200.0;
        let current_start = today_start + 1_800.0;

        insert_usage_bucket(&conn, "/tmp/project-a", "old-session", old_start, 325_000_000);
        insert_usage_bucket(
            &conn,
            "/tmp/project-a",
            "today-session",
            current_start,
            12_000_000,
        );

        assert_eq!(store.global_today_normalized_tokens(&conn).unwrap(), 12_000_000);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_totals_include_groups_with_zero_token_buckets() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let today_start = local_day_start_seconds(now_seconds());

        insert_usage_bucket(&conn, "/tmp/project-a", "zero-session", today_start, 0);
        insert_usage_bucket(
            &conn,
            "/tmp/project-a",
            "active-session",
            today_start + 1_800.0,
            15_241,
        );

        let totals = store.normalized_project_totals_since(&conn, None).unwrap();
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].project_id, "project-a");
        assert_eq!(totals[0].total_tokens, 15_241);

        let sessions = store.indexed_sessions_since(&conn, None).unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().any(|session| {
            session.session_title == "active-session" && session.total_tokens == 15_241
        }));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn jsonl_append_indexes_only_new_bytes() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "one\n").unwrap();
        let initial_size = fs::metadata(&file_path).unwrap().len() as i64;
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        let mut rebuild_count = 0;
        let mut append_count = 0;

        store
            .load_or_index_jsonl_file(
                &conn,
                "codex",
                &file_path,
                &project,
                |_| {
                    append_count += 1;
                    JSONLParseSnapshot::default()
                },
                || {
                    rebuild_count += 1;
                    JSONLParseSnapshot {
                        result: parsed_history("s1", 100.0, 10, 10),
                        last_processed_offset: initial_size,
                        payload_json: None,
                    }
                },
            )
            .unwrap();
        fs::write(&file_path, "one\ntwo\n").unwrap();
        let updated_size = fs::metadata(&file_path).unwrap().len() as i64;
        let summary = store
            .load_or_index_jsonl_file(
                &conn,
                "codex",
                &file_path,
                &project,
                |checkpoint| {
                    append_count += 1;
                    assert_eq!(checkpoint.unwrap().last_offset, initial_size);
                    JSONLParseSnapshot {
                        result: parsed_history("s1", 200.0, 20, 20),
                        last_processed_offset: updated_size,
                        payload_json: None,
                    }
                },
                || {
                    rebuild_count += 1;
                    JSONLParseSnapshot::default()
                },
            )
            .unwrap();

        assert_eq!(rebuild_count, 1);
        assert_eq!(append_count, 1);
        assert_eq!(
            summary
                .usage_buckets
                .iter()
                .map(|bucket| bucket.total_tokens)
                .sum::<i64>(),
            60
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn jsonl_truncation_promotes_to_rebuild() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "one\ntwo\n").unwrap();
        let initial_size = fs::metadata(&file_path).unwrap().len() as i64;
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        let mut rebuild_count = 0;
        let mut append_count = 0;

        store
            .load_or_index_jsonl_file(
                &conn,
                "claude",
                &file_path,
                &project,
                |_| {
                    append_count += 1;
                    JSONLParseSnapshot::default()
                },
                || {
                    rebuild_count += 1;
                    JSONLParseSnapshot {
                        result: parsed_history("s1", 100.0, 10, 10),
                        last_processed_offset: initial_size,
                        payload_json: None,
                    }
                },
            )
            .unwrap();
        fs::write(&file_path, "one\n").unwrap();
        let truncated_size = fs::metadata(&file_path).unwrap().len() as i64;
        let summary = store
            .load_or_index_jsonl_file(
                &conn,
                "claude",
                &file_path,
                &project,
                |_| {
                    append_count += 1;
                    JSONLParseSnapshot::default()
                },
                || {
                    rebuild_count += 1;
                    JSONLParseSnapshot {
                        result: parsed_history("s1", 300.0, 30, 30),
                        last_processed_offset: truncated_size,
                        payload_json: None,
                    }
                },
            )
            .unwrap();

        assert_eq!(rebuild_count, 2);
        assert_eq!(append_count, 0);
        assert_eq!(summary.usage_buckets[0].total_tokens, 60);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_snapshot_groups_sessions_by_external_session_id() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let first_path = root.join("first.jsonl");
        let second_path = root.join("second.jsonl");
        fs::write(&first_path, "{}\n").unwrap();
        fs::write(&second_path, "{}\n").unwrap();
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);

        store
            .load_or_index_file(&conn, "opencode", &first_path, &project, || {
                parsed_history_with_external("file-session-1", "external-1", 100.0, 10, 10)
            })
            .unwrap();
        store
            .load_or_index_file(&conn, "opencode", &second_path, &project, || {
                parsed_history_with_external("file-session-2", "external-1", 200.0, 20, 20)
            })
            .unwrap();
        for (path, duration) in [(&first_path, 60), (&second_path, 30)] {
            let path = normalized_path(path);
            conn.execute(
                "UPDATE ai_history_file_session_link SET active_duration_seconds = ?1 WHERE file_path = ?2;",
                params![duration, path],
            )
            .unwrap();
            conn.execute(
                "UPDATE ai_history_file_usage_bucket SET active_duration_seconds = ?1 WHERE file_path = ?2;",
                params![duration, path],
            )
            .unwrap();
        }
        let snapshot = store.project_snapshot(&conn, project).unwrap();
        let global_sessions = store.indexed_sessions_since(&conn, None).unwrap();
        let range = store.indexed_global_range_summary(&conn, "all", None).unwrap();

        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(
            snapshot.sessions[0].external_session_id.as_deref(),
            Some("external-1")
        );
        assert_eq!(snapshot.sessions[0].total_tokens, 60);
        assert_eq!(snapshot.sessions[0].active_duration_seconds, 60);
        assert_eq!(global_sessions.len(), 1);
        assert_eq!(global_sessions[0].total_tokens, 60);
        assert_eq!(global_sessions[0].active_duration_seconds, 60);
        assert_eq!(range.session_count, 1);
        assert_eq!(range.active_duration_seconds, 60);
        assert_eq!(range.project_totals[0].active_duration_seconds, 60);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_snapshot_uses_measured_active_duration_only() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "{}\n").unwrap();
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);

        store
            .load_or_index_file(&conn, "claude", &file_path, &project, || {
                parsed_history("s1", 100.0, 10, 10)
            })
            .unwrap();
        let snapshot = store.project_snapshot(&conn, project).unwrap();

        assert_eq!(snapshot.sessions[0].active_duration_seconds, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn range_active_duration_is_clipped_to_the_selected_window() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let start = 10_000.0;
        insert_usage_bucket(&conn, "/tmp/project-a", "session", start, 100);

        let all = store.indexed_global_range_summary(&conn, "all", None).unwrap();
        let clipped = store
            .indexed_global_range_summary(&conn, "range", Some(start + 900.0))
            .unwrap();

        assert_eq!(all.active_duration_seconds, 1_800);
        assert_eq!(clipped.active_duration_seconds, 900);
        assert_eq!(clipped.project_totals[0].active_duration_seconds, 900);
        assert_eq!(clipped.sessions[0].active_duration_seconds, 900);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn removes_deleted_source_files_after_a_successful_scan() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let removed_path = root.join("removed.jsonl");
        let retained_path = root.join("retained.jsonl");
        fs::write(&removed_path, "{}\n").unwrap();
        fs::write(&retained_path, "{}\n").unwrap();
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        store
            .load_or_index_file(&conn, "claude", &removed_path, &project, || {
                parsed_history("removed", 100.0, 100, 0)
            })
            .unwrap();
        store
            .load_or_index_file(&conn, "claude", &retained_path, &project, || {
                parsed_history("retained", 200.0, 20, 0)
            })
            .unwrap();
        fs::remove_file(&removed_path).unwrap();

        store
            .remove_missing_source_files(
                &conn,
                "claude",
                &project.path,
                &[retained_path],
            )
            .unwrap();
        let snapshot = store.project_snapshot(&conn, project).unwrap();

        assert_eq!(snapshot.project_summary.project_total_tokens, 20);
        assert_eq!(snapshot.sessions.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn global_store_retains_only_current_project_and_worktree_paths() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        insert_usage_bucket(&conn, "/tmp/project-a", "project", 10_000.0, 100);
        insert_usage_bucket(&conn, "/tmp/project-a-worktree", "worktree", 10_000.0, 200);
        insert_usage_bucket(&conn, "/tmp/removed-project", "removed", 10_000.0, 9_999);

        store
            .retain_project_paths(
                &conn,
                &["/tmp/project-a".to_string(), "/tmp/project-a-worktree".to_string()],
            )
            .unwrap();
        let totals = store.indexed_global_project_totals(&conn).unwrap();

        assert_eq!(totals.len(), 2);
        assert_eq!(totals.iter().map(|item| item.total_tokens).sum::<i64>(), 300);
        assert!(totals.iter().all(|item| item.project_path != "/tmp/removed-project"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn partial_global_index_is_not_reported_as_complete() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let database_path = root.join("ai-usage.sqlite3");
        let store = AIUsageStore::at_path(database_path.clone());
        let conn = store.connect().unwrap();
        insert_usage_bucket(&conn, "/tmp/project-a", "project-a", 10_000.0, 100);
        conn.execute(
            r#"
            INSERT INTO ai_history_project_index_state (
                project_path, project_id, project_name, indexed_at
            ) VALUES (?1, ?2, ?3, ?4)
            "#,
            params!["/tmp/project-a", "project-a", "Project A", 10_000.0],
        )
        .unwrap();
        drop(conn);

        let snapshot = load_indexed_global_history_at(
            database_path,
            vec![
                AIHistoryProjectRequest {
                    id: "project-a".to_string(),
                    name: "Project A".to_string(),
                    path: "/tmp/project-a".to_string(),
                },
                AIHistoryProjectRequest {
                    id: "project-b".to_string(),
                    name: "Project B".to_string(),
                    path: "/tmp/project-b".to_string(),
                },
            ],
        )
        .unwrap();

        assert!(snapshot.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn empty_project_scope_removes_all_indexed_history() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let database_path = root.join("ai-usage.sqlite3");
        let store = AIUsageStore::at_path(database_path.clone());
        let conn = store.connect().unwrap();
        insert_usage_bucket(&conn, "/tmp/removed-project", "removed", 10_000.0, 100);
        drop(conn);

        let snapshot = load_indexed_global_history_at(database_path, Vec::new())
            .unwrap()
            .unwrap();

        assert_eq!(snapshot.total_tokens, 0);
        assert_eq!(snapshot.project_count, 0);
        assert!(snapshot.sessions.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    fn test_project(root: &Path) -> AIHistoryProjectRequest {
        AIHistoryProjectRequest {
            id: "project-1".to_string(),
            name: "Project".to_string(),
            path: root.to_string_lossy().to_string(),
        }
    }

    fn parsed_history(
        session_id: &str,
        timestamp: f64,
        input_tokens: i64,
        output_tokens: i64,
    ) -> ParsedHistory {
        parsed_history_with_external(
            session_id,
            session_id,
            timestamp,
            input_tokens,
            output_tokens,
        )
    }

    fn parsed_history_with_external(
        session_id: &str,
        external_session_id: &str,
        timestamp: f64,
        input_tokens: i64,
        output_tokens: i64,
    ) -> ParsedHistory {
        ParsedHistory {
            events: vec![HistoryEvent {
                source: "claude".to_string(),
                session_id: session_id.to_string(),
                timestamp,
                kind: HistoryEventKind::Request,
            }],
            entries: vec![HistoryEntry {
                source: "claude".to_string(),
                session_id: session_id.to_string(),
                external_session_id: Some(external_session_id.to_string()),
                session_title: Some("Session".to_string()),
                timestamp: timestamp + 60.0,
                model: Some("model-a".to_string()),
                input_tokens,
                output_tokens,
                cached_input_tokens: 5,
                reasoning_output_tokens: 0,
                usage_amounts: Vec::new(),
            }],
        }
    }

    fn insert_usage_bucket(
        conn: &Connection,
        project_path: &str,
        session_key: &str,
        bucket_start: f64,
        total_tokens: i64,
    ) {
        conn.execute(
            r#"
            INSERT INTO ai_history_file_session_link (
                source, file_path, project_path, session_key, external_session_id, project_id,
                project_name, session_title, first_seen_at, last_seen_at, last_model,
                active_duration_seconds
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(source, file_path, project_path, session_key) DO UPDATE SET
                session_title = excluded.session_title,
                last_seen_at = excluded.last_seen_at,
                last_model = excluded.last_model
            "#,
            params![
                "codex",
                "session.jsonl",
                project_path,
                session_key,
                session_key,
                "project-a",
                "Project A",
                session_key,
                bucket_start,
                bucket_start + 1_800.0,
                "gpt-5",
                1_800
            ],
        )
        .unwrap();
        conn.execute(
            r#"
            INSERT INTO ai_history_file_usage_bucket (
                source, file_path, project_path, session_key, model, bucket_start, bucket_end,
                input_tokens, output_tokens, total_tokens, cached_input_tokens, request_count,
                active_duration_seconds
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                "codex",
                "session.jsonl",
                project_path,
                session_key,
                "gpt-5",
                bucket_start,
                bucket_start + 1_800.0,
                total_tokens / 2,
                total_tokens / 2,
                total_tokens,
                0,
                1,
                1_800
            ],
        )
        .unwrap();
    }
}
