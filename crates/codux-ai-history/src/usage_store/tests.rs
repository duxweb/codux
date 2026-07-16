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
    fn schema_13_upgrade_preserves_existing_usage_events() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let database_path = root.join("ai-usage.sqlite3");
        let conn = Connection::open(&database_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE ai_history_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO ai_history_meta VALUES ('normalized_history_schema_version', '13');
            CREATE TABLE ai_history_file_usage_event (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                event_ordinal INTEGER NOT NULL,
                session_key TEXT NOT NULL,
                occurred_at INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, event_ordinal)
            );
            CREATE TABLE ai_history_project_index_state (
                project_path TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                project_name TEXT NOT NULL,
                indexed_at REAL NOT NULL
            );
            INSERT INTO ai_history_project_index_state
                VALUES ('/old/path', 'project-a', 'Project A', 100);
            INSERT INTO ai_history_file_usage_event
                VALUES ('codex', 'missing.jsonl', '/old/path', 0, 'session', 110, 42);
            "#,
        )
        .unwrap();
        drop(conn);

        let store = AIUsageStore::at_path(database_path);
        let conn = store.connect().unwrap();
        assert_eq!(
            store
                .normalized_tokens_in_intervals(
                    &conn,
                    &[AIUsageInterval {
                        project_path: "/old/path".to_string(),
                        included_at: 100,
                        excluded_at: None,
                    }],
                )
                .unwrap(),
            42
        );
        let event: (String, i64, i64) = conn
            .query_row(
                "SELECT project_id, request_count, active_duration_seconds FROM ai_history_file_usage_event",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(event, ("project-a".to_string(), 0, 0));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn schema_13_upgrade_canonicalizes_preserved_event_paths() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let project_dir = root.join("project");
        fs::create_dir_all(&project_dir).unwrap();
        let database_path = root.join("ai-usage.sqlite3");
        let aliased_path = project_dir.join("..").join("project");
        let conn = Connection::open(&database_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE ai_history_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO ai_history_meta VALUES ('normalized_history_schema_version', '13');
            CREATE TABLE ai_history_file_usage_event (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                event_ordinal INTEGER NOT NULL,
                session_key TEXT NOT NULL,
                occurred_at INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, event_ordinal)
            );
            CREATE TABLE ai_history_project_index_state (
                project_path TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                project_name TEXT NOT NULL,
                indexed_at REAL NOT NULL
            );
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ai_history_project_index_state VALUES (?1, 'project-a', 'Project A', 100);",
            params![aliased_path.to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ai_history_file_usage_event VALUES ('codex', 'session.jsonl', ?1, 0, 'session', 110, 42);",
            params![aliased_path.to_string_lossy()],
        )
        .unwrap();
        drop(conn);

        let store = AIUsageStore::at_path(database_path);
        let conn = store.connect().unwrap();
        let stored_path: String = conn
            .query_row(
                "SELECT project_path FROM ai_history_file_usage_event;",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_path, canonical_project_path(&project_dir.to_string_lossy()));
        assert_eq!(
            store
                .normalized_tokens_in_intervals(
                    &conn,
                    &[AIUsageInterval {
                        project_path: project_dir.to_string_lossy().into_owned(),
                        included_at: 0,
                        excluded_at: None,
                    }],
                )
                .unwrap(),
            42
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn schema_13_upgrade_converts_usage_buckets_to_facts() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let database_path = root.join("ai-usage.sqlite3");
        let conn = Connection::open(&database_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE ai_history_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO ai_history_meta VALUES ('normalized_history_schema_version', '13');
            CREATE TABLE ai_history_file_usage_bucket (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                session_key TEXT NOT NULL,
                model TEXT NOT NULL,
                bucket_start REAL NOT NULL,
                bucket_end REAL NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                cached_input_tokens INTEGER NOT NULL,
                request_count INTEGER NOT NULL,
                active_duration_seconds INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, session_key, model, bucket_start)
            );
            CREATE TABLE ai_history_file_session_link (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                session_key TEXT NOT NULL,
                external_session_id TEXT,
                project_id TEXT NOT NULL,
                project_name TEXT NOT NULL,
                session_title TEXT NOT NULL,
                first_seen_at REAL NOT NULL,
                last_seen_at REAL NOT NULL,
                last_model TEXT,
                active_duration_seconds INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, session_key)
            );
            INSERT INTO ai_history_file_session_link VALUES
                ('codex', 'session.jsonl', '/old/path', 'session', NULL,
                 'project-a', 'Project A', 'Session', 100, 200, 'gpt-5', 60);
            INSERT INTO ai_history_file_usage_bucket VALUES
                ('codex', 'session.jsonl', '/old/path', 'session', 'gpt-5',
                 100, 200, 30, 12, 42, 0, 3, 60);
            "#,
        )
        .unwrap();
        drop(conn);

        let store = AIUsageStore::at_path(database_path);
        let conn = store.connect().unwrap();
        assert_eq!(
            store
                .normalized_tokens_in_intervals(
                    &conn,
                    &[AIUsageInterval {
                        project_path: "/old/path".to_string(),
                        included_at: 0,
                        excluded_at: None,
                    }],
                )
                .unwrap(),
            42
        );
        let sessions = store
            .interval_sessions(
                &conn,
                &[AIUsageInterval {
                    project_path: "/old/path".to_string(),
                    included_at: 0,
                    excluded_at: None,
                }],
            )
            .unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].request_count, 3);
        assert_eq!(sessions[0].active_duration_seconds, 60);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn schema_13_upgrade_enriches_precise_events_from_buckets() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let database_path = root.join("ai-usage.sqlite3");
        let conn = Connection::open(&database_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE ai_history_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO ai_history_meta VALUES ('normalized_history_schema_version', '13');
            CREATE TABLE ai_history_file_usage_event (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                event_ordinal INTEGER NOT NULL,
                session_key TEXT NOT NULL,
                occurred_at INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, event_ordinal)
            );
            CREATE TABLE ai_history_file_usage_bucket (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                session_key TEXT NOT NULL,
                model TEXT NOT NULL,
                bucket_start REAL NOT NULL,
                bucket_end REAL NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                cached_input_tokens INTEGER NOT NULL,
                request_count INTEGER NOT NULL,
                active_duration_seconds INTEGER NOT NULL,
                PRIMARY KEY (source, file_path, project_path, session_key, model, bucket_start)
            );
            CREATE TABLE ai_history_project_index_state (
                project_path TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                project_name TEXT NOT NULL,
                indexed_at REAL NOT NULL
            );
            INSERT INTO ai_history_project_index_state
                VALUES ('/old/path', 'project-a', 'Project A', 100);
            INSERT INTO ai_history_file_usage_event
                VALUES ('codex', 'session.jsonl', '/old/path', 0, 'session', 110, 42);
            INSERT INTO ai_history_file_usage_bucket VALUES
                ('codex', 'session.jsonl', '/old/path', 'session', 'gpt-5',
                 100, 200, 30, 12, 42, 0, 3, 60);
            "#,
        )
        .unwrap();
        drop(conn);

        let store = AIUsageStore::at_path(database_path);
        let conn = store.connect().unwrap();
        let sessions = store
            .interval_sessions(
                &conn,
                &[AIUsageInterval {
                    project_path: "/old/path".to_string(),
                    included_at: 0,
                    excluded_at: None,
                }],
            )
            .unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].total_tokens, 42);
        assert_eq!(sessions[0].request_count, 3);
        assert_eq!(sessions[0].active_duration_seconds, 60);
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
    fn usage_intervals_select_exact_events_within_the_same_bucket() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        insert_usage_event(&conn, "/tmp/project-a", "session", 0, 100, 10);
        insert_usage_event(&conn, "/tmp/project-a", "session", 1, 110, 20);
        insert_usage_event(&conn, "/tmp/project-a", "session", 2, 120, 30);

        let total = store
            .normalized_tokens_in_intervals(
                &conn,
                &[AIUsageInterval {
                    project_path: "/tmp/project-a".to_string(),
                    included_at: 110,
                    excluded_at: Some(120),
                }],
            )
            .unwrap();

        assert_eq!(total, 20);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn overlapping_usage_intervals_do_not_double_count_events() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        insert_usage_event(&conn, "/tmp/project-a", "session", 0, 100, 10);
        insert_usage_event(&conn, "/tmp/project-a", "session", 1, 110, 20);
        insert_usage_event(&conn, "/tmp/project-a", "session", 2, 120, 30);

        let total = store
            .normalized_tokens_in_intervals(
                &conn,
                &[
                    AIUsageInterval {
                        project_path: "/tmp/project-a".to_string(),
                        included_at: 100,
                        excluded_at: Some(120),
                    },
                    AIUsageInterval {
                        project_path: "/tmp/project-a".to_string(),
                        included_at: 110,
                        excluded_at: None,
                    },
                ],
            )
            .unwrap();

        assert_eq!(total, 60);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interval_sessions_clip_activity_and_exclude_outside_facts() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        conn.execute(
            r#"
            INSERT INTO ai_history_file_usage_event (
                source, file_path, project_path, project_id, event_ordinal,
                session_key, occurred_at, total_tokens, request_count,
                active_duration_seconds
            ) VALUES
                ('codex', 'session.jsonl', '/old/path', 'project-a', 0, 'session', 90, 100, 1, 30),
                ('codex', 'session.jsonl', '/old/path', 'project-a', 1, 'session', 110, 20, 1, 0),
                ('codex', 'session.jsonl', '/old/path', 'project-a', 2, 'session', 130, 300, 1, 0)
            "#,
            [],
        )
        .unwrap();

        let sessions = store
            .interval_sessions(
                &conn,
                &[AIUsageInterval {
                    project_path: "/old/path".to_string(),
                    included_at: 100,
                    excluded_at: Some(120),
                }],
            )
            .unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].first_seen_at, 100);
        assert_eq!(sessions[0].request_count, 1);
        assert_eq!(sessions[0].total_tokens, 20);
        assert_eq!(sessions[0].active_duration_seconds, 20);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interval_sessions_keep_same_session_key_separate_across_sources() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        insert_source_usage_event(
            &conn,
            "codex",
            "/tmp/project-a",
            "shared-session",
            0,
            100,
            10,
        );
        insert_source_usage_event(
            &conn,
            "claude",
            "/tmp/project-a",
            "shared-session",
            0,
            110,
            20,
        );

        let sessions = store
            .interval_sessions(
                &conn,
                &[AIUsageInterval {
                    project_path: "/tmp/project-a".to_string(),
                    included_at: 0,
                    excluded_at: None,
                }],
            )
            .unwrap();

        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().any(|session| session.source == "codex"));
        assert!(sessions.iter().any(|session| session.source == "claude"));
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
    fn jsonl_rebuild_can_correct_interval_tokens_downward() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "one\ntwo\n").unwrap();
        let initial_size = fs::metadata(&file_path).unwrap().len() as i64;
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        let interval = AIUsageInterval {
            project_path: project.path.clone(),
            included_at: 0,
            excluded_at: None,
        };

        store
            .load_or_index_jsonl_file(
                &conn,
                "codex",
                &file_path,
                &project,
                |_| JSONLParseSnapshot::default(),
                || JSONLParseSnapshot {
                    result: parsed_history("session", 100.0, 80, 20),
                    last_processed_offset: initial_size,
                    payload_json: None,
                },
            )
            .unwrap();
        assert_eq!(
            store
                .normalized_tokens_in_intervals(&conn, std::slice::from_ref(&interval))
                .unwrap(),
            100
        );

        fs::write(&file_path, "one\n").unwrap();
        let rebuilt_size = fs::metadata(&file_path).unwrap().len() as i64;
        store
            .load_or_index_jsonl_file(
                &conn,
                "codex",
                &file_path,
                &project,
                |_| JSONLParseSnapshot::default(),
                || JSONLParseSnapshot {
                    result: parsed_history("session", 100.0, 8, 2),
                    last_processed_offset: rebuilt_size,
                    payload_json: None,
                },
            )
            .unwrap();

        assert_eq!(
            store
                .normalized_tokens_in_intervals(&conn, &[interval])
                .unwrap(),
            10
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn replacing_file_after_project_id_change_keeps_path_scoped_usage() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "{}\n").unwrap();
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        let first = store
            .load_or_index_file(&conn, "codex", &file_path, &project, || {
                parsed_history("session", 100.0, 80, 20)
            })
            .unwrap();
        let mut replacement = first;
        for event in &mut replacement.usage_events {
            event.project_id = "project-b".to_string();
        }

        store
            .replace_external_summary(&conn, &replacement, None)
            .unwrap();

        assert_eq!(
            store
                .normalized_tokens_in_intervals(
                    &conn,
                    &[AIUsageInterval {
                        project_path: project.path.clone(),
                        included_at: 0,
                        excluded_at: None,
                    }],
                )
                .unwrap(),
            100
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn removing_session_preserves_earned_usage_events() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(&file_path, "{}\n").unwrap();
        let store = AIUsageStore::at_path(root.join("ai-usage.sqlite3"));
        let conn = store.connect().unwrap();
        let project = test_project(&root);
        store
            .load_or_index_file(&conn, "codex", &file_path, &project, || {
                parsed_history("session", 100.0, 80, 20)
            })
            .unwrap();
        let snapshot = store.project_snapshot(&conn, project.clone()).unwrap();

        assert!(
            store
                .remove_project_session(&conn, &project.path, &snapshot.sessions[0].session_id)
                .unwrap()
        );

        assert!(
            store
                .project_snapshot(&conn, project.clone())
                .unwrap()
                .sessions
                .is_empty()
        );
        assert_eq!(
            store
                .normalized_tokens_in_intervals(
                    &conn,
                    &[AIUsageInterval {
                        project_path: project.path,
                        included_at: 0,
                        excluded_at: None,
                    }],
                )
                .unwrap(),
            100
        );
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
        let project_path = project.path.clone();
        let snapshot = store.project_snapshot(&conn, project).unwrap();

        assert_eq!(snapshot.project_summary.project_total_tokens, 20);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(
            store
                .normalized_tokens_in_intervals(
                    &conn,
                    &[AIUsageInterval {
                        project_path,
                        included_at: 0,
                        excluded_at: None,
                    }],
                )
                .unwrap(),
            120
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn global_scope_filters_projects_without_deleting_history() {
        let root = std::env::temp_dir().join(format!("codux-ai-usage-store-{}", Uuid::new_v4()));
        let database_path = root.join("ai-usage.sqlite3");
        let store = AIUsageStore::at_path(database_path.clone());
        let conn = store.connect().unwrap();
        insert_usage_bucket(&conn, "/tmp/project-a", "project", 10_000.0, 100);
        insert_usage_bucket(&conn, "/tmp/project-a-worktree", "worktree", 10_000.0, 200);
        insert_usage_bucket(&conn, "/tmp/removed-project", "removed", 10_000.0, 9_999);
        drop(conn);

        let snapshot = load_indexed_global_history_at(
            database_path,
            vec![AIHistoryProjectRequest {
                id: "project-a".to_string(),
                name: "Project A".to_string(),
                path: "/tmp/project-a".to_string(),
            }],
        )
        .unwrap()
            .unwrap();
        assert_eq!(snapshot.total_tokens, 100);

        let conn = store.connect().unwrap();
        let totals = store.indexed_global_project_totals(&conn).unwrap();
        assert_eq!(totals.len(), 3);
        assert_eq!(
            totals.iter().map(|item| item.total_tokens).sum::<i64>(),
            10_299
        );
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
            sessions: Vec::new(),
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
        insert_source_usage_bucket(
            conn,
            "codex",
            project_path,
            session_key,
            bucket_start,
            total_tokens,
        );
    }

    fn insert_source_usage_bucket(
        conn: &Connection,
        source: &str,
        project_path: &str,
        session_key: &str,
        bucket_start: f64,
        total_tokens: i64,
    ) {
        let file_path = format!("{source}-{session_key}.jsonl");
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
                source,
                file_path,
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
                source,
                file_path,
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

    fn insert_usage_event(
        conn: &Connection,
        project_path: &str,
        session_key: &str,
        event_ordinal: i64,
        occurred_at: i64,
        total_tokens: i64,
    ) {
        insert_source_usage_event(
            conn,
            "codex",
            project_path,
            session_key,
            event_ordinal,
            occurred_at,
            total_tokens,
        );
    }

    fn insert_source_usage_event(
        conn: &Connection,
        source: &str,
        project_path: &str,
        session_key: &str,
        event_ordinal: i64,
        occurred_at: i64,
        total_tokens: i64,
    ) {
        let file_path = format!("{source}-{session_key}.jsonl");
        conn.execute(
            r#"
            INSERT INTO ai_history_file_usage_event (
                source, file_path, project_path, project_id, event_ordinal,
                session_key, occurred_at, total_tokens, request_count,
                active_duration_seconds
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 0)
            "#,
            params![
                source,
                file_path,
                project_path,
                "project-a",
                event_ordinal,
                session_key,
                occurred_at,
                total_tokens
            ],
        )
        .unwrap();
    }

}
