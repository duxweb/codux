#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_history_normalized::JSONLParseSnapshot;
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
        let snapshot = store.project_snapshot(&conn, project).unwrap();

        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(
            snapshot.sessions[0].external_session_id.as_deref(),
            Some("external-1")
        );
        assert_eq!(snapshot.sessions[0].total_tokens, 60);
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
                role: HistoryRole::User,
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
                input_tokens, output_tokens, total_tokens, cached_input_tokens, request_count
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
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
                1
            ],
        )
        .unwrap();
    }
}
