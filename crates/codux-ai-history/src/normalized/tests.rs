#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_oversized_single_json_history_files() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("large.json");
        let file = fs::File::create(&file_path).unwrap();
        file.set_len(MAX_SINGLE_JSON_HISTORY_BYTES + 1).unwrap();

        assert!(read_small_json_value(&file_path).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn aggregates_claude_history() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = "/tmp/project-a";
        let log_dir = root.join(".claude/projects/-tmp-project-a");
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(
            log_dir.join("session.jsonl"),
            r#"{"type":"user","sessionId":"s1","cwd":"/tmp/project-a","timestamp":"2026-05-17T00:00:00Z","message":{"content":"hello"}}
{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-a","timestamp":"2026-05-17T00:01:00Z","uuid":"a1","message":{"model":"claude-sonnet","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10}}}
"#,
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path.to_string(),
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 150);
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 10);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].request_count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn claude_missing_timestamps_do_not_follow_index_time() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        fs::write(
            &file_path,
            r#"{"type":"user","sessionId":"s1","cwd":"/tmp/project-a","message":{"content":"hello"}}
{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-a","uuid":"a1","message":{"model":"claude-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
"#,
        )
        .unwrap();
        let project = AIHistoryProjectRequest {
            id: "project-1".to_string(),
            name: "Project".to_string(),
            path: "/tmp/project-a".to_string(),
        };

        let first = parse_claude_history_file_snapshot(&project, &file_path, 0, None);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let second = parse_claude_history_file_snapshot(&project, &file_path, 0, None);

        assert_eq!(first.result.events[0].timestamp, second.result.events[0].timestamp);
        assert_eq!(first.result.entries[0].timestamp, second.result.entries[0].timestamp);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn history_without_store_is_stable_when_source_mtime_changes() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = "/tmp/project-a";
        let log_dir = root.join(".claude/projects/-tmp-project-a");
        fs::create_dir_all(&log_dir).unwrap();
        let file_path = log_dir.join("session.jsonl");
        let contents = r#"{"type":"user","sessionId":"s1","cwd":"/tmp/project-a","message":{"content":"hello"}}
{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-a","uuid":"a1","message":{"model":"claude-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
"#;
        fs::write(&file_path, contents).unwrap();
        let project = AIHistoryProjectRequest {
            id: "project-1".to_string(),
            name: "Project".to_string(),
            path: project_path.to_string(),
        };

        let mut first =
            load_project_history_without_store(project.clone(), &root, &mut |_, _| {});
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&file_path, contents).unwrap();
        let mut second = load_project_history_without_store(project, &root, &mut |_, _| {});

        first.indexed_at = 0.0;
        second.indexed_at = 0.0;
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(second).unwrap()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_uses_state_database_before_recursive_scan() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex");
        fs::create_dir_all(codex_dir.join("sessions")).unwrap();
        let rollout_path = codex_dir.join("sessions").join("rollout.jsonl");
        fs::write(
            &rollout_path,
            format!(
                r#"{{"timestamp":"2026-05-17T00:00:00Z","type":"session_meta","payload":{{"cwd":"{}","id":"s1"}}}}"#,
                project_path
            ),
        )
        .unwrap();
        let database_path = codex_dir.join("state_5.sqlite");
        let conn = Connection::open(&database_path).unwrap();
        conn.execute(
            "CREATE TABLE threads (rollout_path TEXT, cwd TEXT, updated_at REAL);",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (rollout_path, cwd, updated_at) VALUES (?1, ?2, 2);",
            rusqlite::params![
                rollout_path.to_string_lossy().to_string(),
                project_path.clone()
            ],
        )
        .unwrap();

        let files = codex_session_paths(&project_path, &root);

        assert_eq!(files, vec![rollout_path]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_history_title_ignores_injected_launch_context() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex/sessions");
        fs::create_dir_all(&codex_dir).unwrap();
        let rollout_path = codex_dir.join("rollout.jsonl");
        let user_prompt = r#"# AGENTS.md instructions

<collaboration_mode>
runtime launch context
</collaboration_mode>
<environment_context>
  <cwd>/tmp/project-a</cwd>
</environment_context>
修复项目排序拖动崩溃"#;
        fs::write(
            &rollout_path,
            format!(
                "{}\n{}\n{}\n",
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:00Z",
                    "type": "session_meta",
                    "payload": { "cwd": project_path, "id": "s1" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:01Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [{ "type": "input_text", "text": user_prompt }]
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:01:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "last_token_usage": {
                                "input_tokens": 10,
                                "output_tokens": 5,
                                "cached_input_tokens": 0,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 15
                            }
                        }
                    }
                }),
            ),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].session_title, "修复项目排序拖动崩溃");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_history_title_skips_memory_only_injected_prompt() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex/sessions");
        fs::create_dir_all(&codex_dir).unwrap();
        let rollout_path = codex_dir.join("rollout.jsonl");
        fs::write(
            &rollout_path,
            format!(
                "{}\n{}\n{}\n",
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:00Z",
                    "type": "session_meta",
                    "payload": { "cwd": project_path, "id": "s1" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:01Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [{
                            "type": "input_text",
                            "text": "# Codux Memory\n\nProject: Demo\n## Global Prompt\nUse memory."
                        }]
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:01:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "last_token_usage": {
                                "input_tokens": 10,
                                "output_tokens": 5,
                                "cached_input_tokens": 0,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 15
                            }
                        }
                    }
                }),
            ),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].session_title, "Project");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_history_title_skips_fork_handoff_prompt() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex/sessions");
        fs::create_dir_all(&codex_dir).unwrap();
        let rollout_path = codex_dir.join("rollout.jsonl");
        fs::write(
            &rollout_path,
            format!(
                "{}\n{}\n{}\n{}\n",
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:00Z",
                    "type": "session_meta",
                    "payload": { "cwd": project_path, "id": "s1" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:01Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [{
                            "type": "input_text",
                            "text": "# Continue Cleaned AI Session\n\nYou are continuing an AI coding session in Codux."
                        }]
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:02Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [{
                            "type": "input_text",
                            "text": "继续修复会话标题过滤"
                        }]
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:01:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "last_token_usage": {
                                "input_tokens": 10,
                                "output_tokens": 5,
                                "cached_input_tokens": 0,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 15
                            }
                        }
                    }
                }),
            ),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].session_title, "继续修复会话标题过滤");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn claude_counts_cache_creation_and_read_tokens() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = "/tmp/project-cache";
        let log_dir = root.join(".claude/projects/-tmp-project-cache");
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(
            log_dir.join("session.jsonl"),
            r#"{"type":"user","sessionId":"s1","cwd":"/tmp/project-cache","timestamp":"2026-05-17T00:00:00Z","message":{"content":"hi"}}
{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-cache","timestamp":"2026-05-17T00:01:00Z","uuid":"a1","message":{"model":"claude-sonnet","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10,"cache_creation_input_tokens":40}}}
"#,
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path.to_string(),
            },
            &root,
            &mut |_, _| {},
        );

        // Both cache reads (10) and cache writes/creation (40) are cached input.
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 50);
        // project_total_tokens excludes cached: input(100) + output(50).
        assert_eq!(snapshot.project_summary.project_total_tokens, 150);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn claude_counts_only_the_final_snapshot_for_each_message() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = "/tmp/project-snapshots";
        let log_dir = root.join(".claude/projects/-tmp-project-snapshots");
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(
            log_dir.join("session.jsonl"),
            r#"{"type":"user","sessionId":"s1","cwd":"/tmp/project-snapshots","timestamp":"2026-05-17T00:00:00Z","message":{"content":"hello"}}
{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-snapshots","timestamp":"2026-05-17T00:01:00Z","uuid":"row-1","message":{"id":"msg-1","model":"claude-sonnet","usage":{"input_tokens":100,"output_tokens":10,"cache_read_input_tokens":5}}}
{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-snapshots","timestamp":"2026-05-17T00:01:01Z","uuid":"row-2","message":{"id":"msg-1","model":"claude-sonnet","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":5}}}
"#,
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path.to_string(),
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 150);
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 5);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn claude_checkpoint_continues_the_last_message_snapshot() {
        use std::io::Write as _;

        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("session.jsonl");
        let project = AIHistoryProjectRequest {
            id: "project-1".to_string(),
            name: "Project".to_string(),
            path: "/tmp/project-checkpoint".to_string(),
        };
        fs::write(
            &file_path,
            r#"{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-checkpoint","timestamp":"2026-05-17T00:01:00Z","uuid":"row-1","message":{"id":"msg-1","model":"claude-sonnet","usage":{"input_tokens":100,"output_tokens":10,"cache_read_input_tokens":5}}}
"#,
        )
        .unwrap();
        let initial_size = fs::metadata(&file_path).unwrap().len() as i64;
        let first = parse_claude_history_file_snapshot(&project, &file_path, 0, None);
        let seed = decode_checkpoint_payload(first.payload_json.as_deref()).unwrap();

        let mut file = fs::OpenOptions::new().append(true).open(&file_path).unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({
                "type": "assistant",
                "sessionId": "s1",
                "cwd": "/tmp/project-checkpoint",
                "timestamp": "2026-05-17T00:01:01Z",
                "uuid": "row-2",
                "message": {
                    "id": "msg-1",
                    "model": "claude-sonnet",
                    "usage": {
                        "input_tokens": 100,
                        "output_tokens": 50,
                        "cache_read_input_tokens": 5
                    }
                }
            })
        )
        .unwrap();
        let second = parse_claude_history_file_snapshot(
            &project,
            &file_path,
            initial_size,
            Some(&seed),
        );

        assert_eq!(first.result.entries.len(), 1);
        assert_eq!(first.result.entries[0].input_tokens, 100);
        assert_eq!(first.result.entries[0].output_tokens, 10);
        assert_eq!(second.result.entries.len(), 1);
        assert_eq!(second.result.entries[0].input_tokens, 0);
        assert_eq!(second.result.entries[0].output_tokens, 40);
        assert_eq!(second.result.entries[0].cached_input_tokens, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn claude_request_count_excludes_tool_results_and_synthetic_rows() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = "/tmp/project-requests";
        let log_dir = root.join(".claude/projects/-tmp-project-requests");
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(
            log_dir.join("session.jsonl"),
            r#"{"type":"user","sessionId":"s1","cwd":"/tmp/project-requests","timestamp":"2026-05-17T00:00:00Z","message":{"content":"real request"}}
{"type":"user","sessionId":"s1","cwd":"/tmp/project-requests","timestamp":"2026-05-17T00:00:10Z","message":{"content":[{"type":"tool_result","tool_use_id":"tool-1","content":"done"}]}}
{"type":"user","sessionId":"s1","cwd":"/tmp/project-requests","timestamp":"2026-05-17T00:00:20Z","isMeta":true,"message":{"content":"synthetic context"}}
{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-requests","timestamp":"2026-05-17T00:01:00Z","uuid":"row-1","message":{"id":"msg-1","model":"claude-sonnet","usage":{"input_tokens":10,"output_tokens":5}}}
"#,
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path.to_string(),
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.sessions[0].request_count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn claude_active_duration_starts_at_each_request_and_excludes_idle_gaps() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = "/tmp/project-duration";
        let log_dir = root.join(".claude/projects/-tmp-project-duration");
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(
            log_dir.join("session.jsonl"),
            r#"{"type":"user","sessionId":"s1","cwd":"/tmp/project-duration","timestamp":"2026-05-17T00:00:00Z","message":{"content":"first"}}
{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-duration","timestamp":"2026-05-17T00:01:00Z","uuid":"a1","message":{"id":"m1","model":"claude-sonnet","usage":{"input_tokens":10,"output_tokens":5}}}
{"type":"user","sessionId":"s1","cwd":"/tmp/project-duration","timestamp":"2026-05-20T00:00:00Z","message":{"content":"second"}}
{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-duration","timestamp":"2026-05-20T00:02:00Z","uuid":"a2","message":{"id":"m2","model":"claude-sonnet","usage":{"input_tokens":12,"output_tokens":6}}}
"#,
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path.to_string(),
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.sessions[0].request_count, 2);
        assert_eq!(snapshot.sessions[0].active_duration_seconds, 180);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_model_switch_does_not_reinflate_cumulative_tokens() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex/sessions");
        fs::create_dir_all(&codex_dir).unwrap();
        let rollout_path = codex_dir.join("rollout.jsonl");
        fs::write(
            &rollout_path,
            format!(
                "{}\n{}\n{}\n",
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:00Z",
                    "type": "session_meta",
                    "payload": { "cwd": project_path, "id": "s1" }
                }),
                // First model: the session-global cumulative input reaches 1000.
                serde_json::json!({
                    "timestamp": "2026-05-17T00:01:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "model": "gpt-5.5",
                            "total_token_usage": { "input_tokens": 1000, "output_tokens": 0 }
                        }
                    }
                }),
                // Model SWITCH: the SAME session cumulative grows by 100 to 1100.
                // The pre-fix per-model baseline saw 0 for gpt-5.4 and
                // re-attributed the whole 1100 here (the ~100M inflation).
                serde_json::json!({
                    "timestamp": "2026-05-17T00:02:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "model": "gpt-5.4",
                            "total_token_usage": { "input_tokens": 1100, "output_tokens": 0 }
                        }
                    }
                }),
            ),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        // Cumulative grew 1000 -> 1100, so the session used 1100 input total.
        // The per-model-baseline bug would report 1000 + 1100 = 2100.
        assert_eq!(snapshot.project_summary.project_total_tokens, 1100);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_cumulative_usage_differences_each_field_exactly() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex/sessions");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(
            codex_dir.join("rollout.jsonl"),
            [
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:00Z",
                    "type": "session_meta",
                    "payload": { "cwd": project_path, "id": "s1" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:01:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": { "total_token_usage": {
                            "input_tokens": 100,
                            "cached_input_tokens": 40,
                            "cache_read_input_tokens": 35,
                            "output_tokens": 20,
                            "reasoning_output_tokens": 5
                        }}
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:02:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": { "total_token_usage": {
                            "input_tokens": 180,
                            "cached_input_tokens": 70,
                            "cache_read_input_tokens": 65,
                            "output_tokens": 35,
                            "reasoning_output_tokens": 8
                        }}
                    }
                }),
            ]
            .into_iter()
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 145);
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 70);
        assert_eq!(snapshot.sessions[0].total_input_tokens, 110);
        assert_eq!(snapshot.sessions[0].total_output_tokens, 35);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_active_duration_uses_explicit_task_boundaries() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex/sessions");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(
            codex_dir.join("rollout.jsonl"),
            [
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:00Z",
                    "type": "session_meta",
                    "payload": { "cwd": project_path, "id": "s1" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:01Z",
                    "type": "turn_context",
                    "payload": { "cwd": project_path, "model": "gpt-5" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:10Z",
                    "type": "event_msg",
                    "payload": { "type": "task_started" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:01:10Z",
                    "type": "event_msg",
                    "payload": { "type": "task_complete" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-20T00:00:01Z",
                    "type": "turn_context",
                    "payload": { "cwd": project_path, "model": "gpt-5" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-20T00:00:10Z",
                    "type": "event_msg",
                    "payload": { "type": "task_started" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-20T00:01:10Z",
                    "type": "event_msg",
                    "payload": { "type": "task_complete" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-20T00:01:11Z",
                    "type": "event_msg",
                    "payload": { "type": "token_count", "info": {
                        "last_token_usage": { "input_tokens": 10, "output_tokens": 5 }
                    }}
                }),
            ]
            .into_iter()
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.sessions[0].request_count, 2);
        assert_eq!(snapshot.sessions[0].active_duration_seconds, 120);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_subagent_remains_an_independent_session() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex/sessions");
        fs::create_dir_all(&codex_dir).unwrap();

        for (file_name, session_id, parent_thread_id) in [
            ("parent.jsonl", "parent-session", None),
            ("subagent.jsonl", "subagent-session", Some("parent-session")),
        ] {
            let mut session_meta = serde_json::json!({
                "timestamp": if parent_thread_id.is_some() {
                    "2026-05-17T00:10:00Z"
                } else {
                    "2026-05-17T00:00:00Z"
                },
                "type": "session_meta",
                "payload": {
                    "cwd": project_path,
                    "id": session_id,
                    "timestamp": if parent_thread_id.is_some() {
                        "2026-05-17T00:10:00Z"
                    } else {
                        "2026-05-17T00:00:00Z"
                    },
                    "source": "cli"
                }
            });
            if let Some(parent_thread_id) = parent_thread_id {
                session_meta["payload"]["parent_thread_id"] =
                    serde_json::json!(parent_thread_id);
                session_meta["payload"]["source"] = serde_json::json!({
                    "subagent": {
                        "thread_spawn": {
                            "parent_thread_id": parent_thread_id,
                            "depth": 1
                        }
                    }
                });
            }
            let mut rows = vec![session_meta];
            if let Some(parent_thread_id) = parent_thread_id {
                rows.push(serde_json::json!({
                    "timestamp": "2026-05-17T00:00:00Z",
                    "type": "session_meta",
                    "payload": {
                        "cwd": project_path,
                        "id": parent_thread_id,
                        "source": "cli"
                    }
                }));
            }
            rows.extend([
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:01Z",
                    "type": "turn_context",
                    "payload": { "cwd": project_path, "model": "gpt-5" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:01:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": { "total_token_usage": {
                            "input_tokens": 100,
                            "output_tokens": 10
                        }}
                    }
                }),
            ]);
            if parent_thread_id.is_some() {
                rows.extend([
                    serde_json::json!({
                        "timestamp": "2026-05-17T00:10:01Z",
                        "type": "event_msg",
                        "payload": {
                            "type": "task_started",
                            "started_at": 1_778_976_601_i64,
                            "turn_id": "child-turn"
                        }
                    }),
                    serde_json::json!({
                        "timestamp": "2026-05-17T00:10:02Z",
                        "type": "turn_context",
                        "payload": {
                            "cwd": project_path,
                            "model": "gpt-5",
                            "turn_id": "child-turn"
                        }
                    }),
                    serde_json::json!({
                        "timestamp": "2026-05-17T00:11:01Z",
                        "type": "event_msg",
                        "payload": {
                            "type": "task_complete",
                            "turn_id": "child-turn"
                        }
                    }),
                ]);
            }
            fs::write(
                codex_dir.join(file_name),
                rows
                .into_iter()
                .map(|row| row.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            )
            .unwrap();
        }

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        let session_ids = snapshot
            .sessions
            .iter()
            .filter_map(|session| session.external_session_id.as_deref())
            .collect::<HashSet<_>>();
        assert_eq!(snapshot.sessions.len(), 2);
        assert_eq!(session_ids, HashSet::from(["parent-session", "subagent-session"]));
        assert_eq!(snapshot.project_summary.project_total_tokens, 110);
        assert_eq!(
            snapshot
                .sessions
                .iter()
                .map(|session| session.request_count)
                .sum::<i64>(),
            2
        );
        let subagent = snapshot
            .sessions
            .iter()
            .find(|session| session.external_session_id.as_deref() == Some("subagent-session"))
            .unwrap();
        assert_eq!(subagent.total_tokens, 0);
        assert_eq!(subagent.active_duration_seconds, 60);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_subagent_checkpoint_keeps_shared_usage_excluded() {
        use std::io::Write as _;

        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("subagent.jsonl");
        let project = AIHistoryProjectRequest {
            id: "project-1".to_string(),
            name: "Project".to_string(),
            path: "/tmp/project-subagent-checkpoint".to_string(),
        };
        fs::write(
            &file_path,
            [
                serde_json::json!({
                    "timestamp": "2026-05-17T00:10:00Z",
                    "type": "session_meta",
                    "payload": {
                        "cwd": project.path,
                        "id": "subagent-session",
                        "timestamp": "2026-05-17T00:10:00Z",
                        "thread_source": "subagent",
                        "parent_thread_id": "parent-session"
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:10:00Z",
                    "type": "session_meta",
                    "payload": {
                        "cwd": project.path,
                        "id": "parent-session",
                        "timestamp": "2026-05-17T00:00:00Z",
                        "source": "cli"
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:10:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "task_started",
                        "started_at": 1_778_976_000_i64,
                        "turn_id": "copied-parent-turn"
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:10:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": { "total_token_usage": {
                            "input_tokens": 100,
                            "output_tokens": 10
                        }}
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:10:01Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "task_started",
                        "started_at": 1_778_976_601_i64,
                        "turn_id": "child-turn"
                    }
                }),
            ]
            .into_iter()
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        )
        .unwrap();
        let initial_size = fs::metadata(&file_path).unwrap().len() as i64;
        let first = parse_codex_history_file_snapshot(&project, &file_path, 0, None);
        let seed = decode_checkpoint_payload(first.payload_json.as_deref()).unwrap();

        assert!(first.result.entries.is_empty());
        assert_eq!(seed.session_key.as_deref(), Some("subagent-session"));
        assert!(seed.codex_is_subagent);
        assert!(seed.codex_own_history_started);

        let mut file = fs::OpenOptions::new().append(true).open(&file_path).unwrap();
        writeln!(
            file,
            "\n{}\n{}",
            serde_json::json!({
                "timestamp": "2026-05-17T00:11:01Z",
                "type": "event_msg",
                "payload": { "type": "task_complete", "turn_id": "child-turn" }
            }),
            serde_json::json!({
                "timestamp": "2026-05-17T00:11:02Z",
                "type": "event_msg",
                "payload": {
                    "type": "token_count",
                    "info": { "total_token_usage": {
                        "input_tokens": 140,
                        "output_tokens": 20
                    }}
                }
            })
        )
        .unwrap();

        let second = parse_codex_history_file_snapshot(
            &project,
            &file_path,
            initial_size,
            Some(&seed),
        );
        let durations = active_duration_by_history_key(&second.result.events);

        assert!(second.result.entries.is_empty());
        assert_eq!(durations["codex:subagent-session"].total_seconds, 60);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_subagent_uses_uuid_v7_turn_boundary_from_real_rollout_shape() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("subagent.jsonl");
        let project = AIHistoryProjectRequest {
            id: "project-1".to_string(),
            name: "Project".to_string(),
            path: "/tmp/project-subagent-boundary".to_string(),
        };
        fs::write(
            &file_path,
            [
                serde_json::json!({
                    "timestamp": "2026-07-15T22:07:07.281Z",
                    "type": "session_meta",
                    "payload": {
                        "cwd": project.path,
                        "id": "019f67d1-b59f-7863-bb2a-c214943ae97f",
                        "timestamp": "2026-07-15T22:07:04.632Z",
                        "parent_thread_id": "019f4b35-162f-7bb2-8968-b3f087081d8a",
                        "thread_source": "subagent",
                        "source": {"subagent": {"thread_spawn": {
                            "parent_thread_id": "019f4b35-162f-7bb2-8968-b3f087081d8a"
                        }}}
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-07-15T22:07:07.281Z",
                    "type": "session_meta",
                    "payload": {
                        "cwd": project.path,
                        "id": "019f4b35-162f-7bb2-8968-b3f087081d8a",
                        "timestamp": "2026-07-10T08:46:38.160Z",
                        "source": "cli"
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-07-15T22:07:07.281Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "task_started",
                        "turn_id": "019f4b3b-138b-77b2-bb4b-79d292f86d69"
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-07-15T22:07:07.282Z",
                    "type": "event_msg",
                    "payload": {"type": "token_count", "info": {"total_token_usage": {
                        "input_tokens": 100,
                        "output_tokens": 10
                    }}}
                }),
                serde_json::json!({
                    "timestamp": "2026-07-15T22:07:07.283Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "task_started",
                        "turn_id": "019f67d1-c0cc-7c63-8d0e-8fafb170e9f6"
                    }
                }),
                serde_json::json!({
                    "timestamp": "2026-07-15T22:08:07.283Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "task_complete",
                        "turn_id": "019f67d1-c0cc-7c63-8d0e-8fafb170e9f6"
                    }
                }),
            ]
            .into_iter()
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        )
        .unwrap();

        let snapshot = parse_codex_history_file_snapshot(&project, &file_path, 0, None);

        assert_eq!(snapshot.result.sessions.len(), 1);
        assert!(snapshot.result.entries.is_empty());
        assert_eq!(snapshot.result.events.len(), 2);
        let active = active_duration_by_history_key(&snapshot.result.events);
        assert_eq!(
            active["codex:019f67d1-b59f-7863-bb2a-c214943ae97f"].total_seconds,
            60
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_checkpoint_preserves_an_open_task_boundary() {
        use std::io::Write as _;

        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("rollout.jsonl");
        let project = AIHistoryProjectRequest {
            id: "project-1".to_string(),
            name: "Project".to_string(),
            path: "/tmp/project-checkpoint".to_string(),
        };
        fs::write(
            &file_path,
            r#"{"timestamp":"2026-05-17T00:00:00Z","type":"session_meta","payload":{"cwd":"/tmp/project-checkpoint","id":"s1"}}
{"timestamp":"2026-05-17T00:00:10Z","type":"event_msg","payload":{"type":"task_started"}}
"#,
        )
        .unwrap();
        let initial_size = fs::metadata(&file_path).unwrap().len() as i64;
        let first = parse_codex_history_file_snapshot(&project, &file_path, 0, None);
        let seed = decode_checkpoint_payload(first.payload_json.as_deref()).unwrap();
        let mut file = fs::OpenOptions::new().append(true).open(&file_path).unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({
                "timestamp": "2026-05-17T00:02:10Z",
                "type": "event_msg",
                "payload": { "type": "task_complete" }
            })
        )
        .unwrap();

        let second = parse_codex_history_file_snapshot(
            &project,
            &file_path,
            initial_size,
            Some(&seed),
        );
        let durations = active_duration_by_history_key(&second.result.events);

        assert_eq!(durations["codex:s1"].total_seconds, 120);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_session_metadata_is_not_counted_as_a_request() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex/sessions");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(
            codex_dir.join("rollout.jsonl"),
            format!(
                "{}\n{}\n{}\n",
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:00Z",
                    "type": "session_meta",
                    "payload": { "cwd": project_path, "id": "s1" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:00:01Z",
                    "type": "turn_context",
                    "payload": { "cwd": project_path, "model": "gpt-5" }
                }),
                serde_json::json!({
                    "timestamp": "2026-05-17T00:01:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": { "last_token_usage": { "input_tokens": 10, "output_tokens": 5 } }
                    }
                }),
            ),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.sessions[0].request_count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_repeated_cumulative_snapshot_is_not_counted_twice() {
        let snapshot = codex_cumulative_snapshot(&[100, 100], &[None, Some(100)]);
        assert_eq!(snapshot.project_summary.project_total_tokens, 100);
    }

    #[test]
    fn codex_cumulative_reset_starts_a_new_baseline() {
        let snapshot = codex_cumulative_snapshot(
            &[1_000, 200, 250],
            &[None, Some(200), Some(50)],
        );
        assert_eq!(snapshot.project_summary.project_total_tokens, 1_250);
    }

    fn codex_cumulative_snapshot(
        cumulative_input_tokens: &[i64],
        last_input_tokens: &[Option<i64>],
    ) -> AIHistorySnapshot {
        assert_eq!(cumulative_input_tokens.len(), last_input_tokens.len());
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex/sessions");
        fs::create_dir_all(&codex_dir).unwrap();
        let mut rows = vec![serde_json::json!({
            "timestamp": "2026-05-17T00:00:00Z",
            "type": "session_meta",
            "payload": { "cwd": project_path, "id": "s1" }
        })];
        for (index, (cumulative, last)) in cumulative_input_tokens
            .iter()
            .zip(last_input_tokens)
            .enumerate()
        {
            let mut info = serde_json::json!({
                "model": "gpt-5",
                "total_token_usage": {
                    "input_tokens": cumulative,
                    "output_tokens": 0
                }
            });
            if let Some(last) = last {
                info["last_token_usage"] = serde_json::json!({
                    "input_tokens": last,
                    "output_tokens": 0
                });
            }
            rows.push(serde_json::json!({
                "timestamp": format!("2026-05-17T00:{:02}:00Z", index + 1),
                "type": "event_msg",
                "payload": { "type": "token_count", "info": info }
            }));
        }
        fs::write(
            codex_dir.join("rollout.jsonl"),
            rows.into_iter()
                .map(|row| row.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );
        let _ = fs::remove_dir_all(root);
        snapshot
    }

    #[test]
    fn matches_windows_extended_paths_without_matching_project_children() {
        #[cfg(windows)]
        assert!(paths_equivalent(
            Some(r"\\?\F:\codux-tauri"),
            r"F:\codux-tauri"
        ));
        assert!(!paths_equivalent(
            Some(r"F:\codux-tauri-other"),
            r"F:\codux-tauri"
        ));
        assert!(!paths_equivalent(
            Some(r"F:\codux-tauri\.codux\worktrees\task-a"),
            r"F:\codux-tauri"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn project_matching_preserves_posix_backslashes() {
        assert!(!paths_equivalent(
            Some(r"/repo/project\name"),
            "/repo/project/name"
        ));
    }

    #[test]
    fn indexes_opencode_sqlite_history() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let db_dir = root.join(".local/share/opencode");
        fs::create_dir_all(&db_dir).unwrap();
        let database_path = db_dir.join("opencode.db");
        let conn = Connection::open(&database_path).unwrap();
        conn.execute(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, time_archived REAL);",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE message (session_id TEXT, data TEXT, time_created REAL);",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title, time_archived) VALUES ('ses_1', 'OpenCode Session', NULL);",
            [],
        )
        .unwrap();
        let user_payload = serde_json::json!({
            "role": "user",
            "time": { "created": "2026-05-17T00:00:00Z" },
            "path": { "root": project_path },
            "modelID": "model-a"
        });
        let assistant_payload = serde_json::json!({
            "role": "assistant",
            "time": { "created": "2026-05-17T00:01:00Z" },
            "path": { "root": project_path },
            "modelID": "model-a",
            "tokens": {
                "input": 10,
                "output": 5,
                "reasoning": 2,
                "cache": { "read": 3 }
            }
        });
        conn.execute(
            "INSERT INTO message (session_id, data, time_created) VALUES ('ses_1', ?1, 1);",
            [user_payload.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (session_id, data, time_created) VALUES ('ses_1', ?1, 2);",
            [assistant_payload.to_string()],
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 17);
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 3);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].last_tool.as_deref(), Some("opencode"));
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert_eq!(snapshot.tool_breakdown[0].key, "opencode");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn indexes_current_opencode_sqlite_history() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let db_dir = root.join(".local/share/opencode");
        fs::create_dir_all(&db_dir).unwrap();
        let database_path = db_dir.join("opencode.db");
        let conn = Connection::open(&database_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                title TEXT NOT NULL,
                directory TEXT NOT NULL,
                path TEXT,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL,
                time_archived INTEGER,
                model TEXT,
                tokens_input INTEGER DEFAULT 0 NOT NULL,
                tokens_output INTEGER DEFAULT 0 NOT NULL,
                tokens_reasoning INTEGER DEFAULT 0 NOT NULL,
                tokens_cache_read INTEGER DEFAULT 0 NOT NULL
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE part (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (
                id, project_id, title, directory, path, time_created, time_updated,
                time_archived, model, tokens_input, tokens_output, tokens_reasoning,
                tokens_cache_read
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11, ?12);",
            rusqlite::params![
                "ses_current",
                "proj_current",
                "OpenCode Current",
                project_path,
                "",
                1_700_000_010_000i64,
                1_700_000_013_000i64,
                r#"{"id":"gpt-5.4","providerID":"rightcode","variant":"high"}"#,
                120i64,
                12i64,
                3i64,
                8704i64,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, time_updated, data)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            rusqlite::params![
                "msg_user",
                "ses_current",
                1_700_000_010_500i64,
                1_700_000_010_500i64,
                r#"{"role":"user","time":{"created":1700000010500}}"#,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, time_updated, data)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            rusqlite::params![
                "msg_assistant",
                "ses_current",
                1_700_000_011_000i64,
                1_700_000_013_000i64,
                r#"{"role":"assistant","modelID":"gpt-5.4","time":{"created":1700000011000,"completed":1700000013000},"finish":"stop"}"#,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6);",
            rusqlite::params![
                "part_text",
                "msg_assistant",
                "ses_current",
                1_700_000_012_000i64,
                1_700_000_012_500i64,
                r#"{"type":"text","text":"done"}"#,
            ],
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 135);
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 8704);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].last_tool.as_deref(), Some("opencode"));
        assert_eq!(
            snapshot.sessions[0].last_model.as_deref(),
            Some("gpt-5.4")
        );
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert!(
            snapshot
                .tool_breakdown
                .iter()
                .any(|item| item.key == "opencode" && item.total_tokens == 135)
        );
        assert!(
            snapshot
                .model_breakdown
                .iter()
                .any(|item| item.key == "gpt-5.4" && item.total_tokens == 135)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_kiro_history_json() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let session_dir = root.join(".kiro/sessions/cli");
        fs::create_dir_all(&session_dir).unwrap();
        let file_path = session_dir.join("session-abc.json");
        fs::write(
            &file_path,
            serde_json::json!({
                "sessionId": "session-abc",
                "projectPath": project_path,
                "model": "kiro-1",
                "title": "Kiro Session",
                "updatedAt": 1000,
                "messages": [
                    { "role": "user", "timestamp": "2026-05-17T00:00:00Z" },
                    { "role": "assistant", "timestamp": "2026-05-17T00:01:00Z", "content": "hello from kiro" }
                ],
                "usage": { "input_tokens": 12, "output_tokens": 8, "cache": { "read": 4 } }
            })
            .to_string(),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].last_tool.as_deref(), Some("kiro"));
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert!(
            snapshot
                .tool_breakdown
                .iter()
                .any(|item| item.key == "kiro")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_kiro_210_credit_usage_without_tokens() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let session_dir = root.join(".kiro/sessions/cli");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(
            session_dir.join("14d0cee2-bca4-45ab-a085-7613abbb692c.json"),
            serde_json::json!({
                "session_id": "14d0cee2-bca4-45ab-a085-7613abbb692c",
                "cwd": project_path,
                "title": "hi",
                "created_at": "2026-06-28T13:40:41.491371Z",
                "updated_at": "2026-06-28T13:41:48.600845Z",
                "session_state": {
                    "rts_model_state": {
                        "model_info": { "model_id": "auto" }
                    },
                    "conversation_metadata": {
                        "user_turn_metadatas": [{
                            "result": {
                                "Ok": {
                                    "role": "assistant",
                                    "content": [{ "kind": "text", "data": "Hi! How can I help you?" }],
                                    "meta": { "timestamp": 1782654108 }
                                }
                            },
                            "end_timestamp": "2026-06-28T13:41:48.600069Z",
                            "input_token_count": 0,
                            "output_token_count": 0,
                            "metering_usage": [{
                                "value": 0.03090351028192372,
                                "unit": "credit",
                                "unitPlural": "credits"
                            }]
                        }]
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].total_tokens, 0);
        assert_eq!(snapshot.sessions[0].last_model.as_deref(), Some("auto"));
        assert_eq!(snapshot.sessions[0].usage_amounts[0].unit, "credit");
        assert!((snapshot.sessions[0].usage_amounts[0].value - 0.03090351028192372).abs() < 0.0001);
        assert!(
            snapshot
                .tool_breakdown
                .iter()
                .any(|item| item.key == "kiro" && item.usage_amounts[0].unit == "credit")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_codewhale_history_json() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let session_dir = root.join(".codewhale/sessions");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(
            session_dir.join("session-abc.json"),
            serde_json::json!({
                "schema_version": 1,
                "metadata": {
                    "id": "session-abc",
                    "title": "CodeWhale Session",
                    "created_at": "2026-05-17T00:00:00Z",
                    "updated_at": "2026-05-17T00:01:00Z",
                    "total_tokens": 123,
                    "model": "deepseek-v4-pro",
                    "workspace": project_path,
                    "mode": "agent"
                },
                "messages": [
                    { "role": "user", "content": [{ "type": "text", "text": "hello" }] },
                    { "role": "assistant", "content": [{ "type": "text", "text": "hello from codewhale" }] }
                ]
            })
            .to_string(),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 123);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].last_tool.as_deref(), Some("codewhale"));
        assert_eq!(
            snapshot.sessions[0].last_model.as_deref(),
            Some("deepseek-v4-pro")
        );
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert!(
            snapshot
                .tool_breakdown
                .iter()
                .any(|item| item.key == "codewhale" && item.total_tokens == 123)
        );
        assert!(
            snapshot
                .model_breakdown
                .iter()
                .any(|item| item.key == "deepseek-v4-pro" && item.total_tokens == 123)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_kimi_history_wire_jsonl() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let session_dir = root.join(".kimi-code/sessions/project-key/session-abc");
        let agent_dir = session_dir.join("agents/main");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::write(
            session_dir.join("state.json"),
            serde_json::json!({
                "sessionId": "session-abc",
                "title": "Kimi Session",
                "cwd": project_path,
                "model": "kimi-k2",
                "createdAt": "2026-05-17T00:00:00Z"
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            agent_dir.join("wire.jsonl"),
            [
                serde_json::json!({
                    "role": "user",
                    "content": "hello kimi",
                    "timestamp": "2026-05-17T00:00:01Z"
                })
                .to_string(),
                serde_json::json!({
                    "role": "assistant",
                    "content": "hello from kimi",
                    "timestamp": "2026-05-17T00:00:02Z",
                    "model": "kimi-k2",
                    "usage": {
                        "input_tokens": 40,
                        "output_tokens": 20,
                        "cached_input_tokens": 5,
                        "reasoning_output_tokens": 3
                    }
                })
                .to_string(),
            ]
            .join("\n"),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 55);
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 5);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].last_tool.as_deref(), Some("kimi"));
        assert_eq!(snapshot.sessions[0].last_model.as_deref(), Some("kimi-k2"));
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert!(
            snapshot
                .tool_breakdown
                .iter()
                .any(|item| item.key == "kimi" && item.total_tokens == 55)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_current_kimi_index_and_incremental_turn_usage() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let other_project_path = root.join("project-b").to_string_lossy().to_string();
        let share_dir = root.join(".kimi-code");
        let session_dir = share_dir.join("sessions/project-key/session-current");
        let other_session_dir = share_dir.join("sessions/other-key/session-other");
        let agent_dir = session_dir.join("agents/main");
        let other_agent_dir = other_session_dir.join("agents/main");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&other_agent_dir).unwrap();
        fs::write(
            session_dir.join("state.json"),
            serde_json::json!({ "title": "Current Kimi Session" }).to_string(),
        )
        .unwrap();
        fs::write(
            agent_dir.join("wire.jsonl"),
            [
                serde_json::json!({
                    "type": "config.update",
                    "modelAlias": "kimi-code/k3",
                    "thinkingEffort": "on",
                    "time": 1_784_382_046_042_i64
                })
                .to_string(),
                serde_json::json!({
                    "type": "turn.prompt",
                    "input": [{ "type": "text", "text": "hello current kimi" }],
                    "time": 1_784_382_046_992_i64
                })
                .to_string(),
                serde_json::json!({
                    "type": "usage.record",
                    "model": "kimi-code/k3",
                    "usage": {
                        "inputOther": 10,
                        "output": 5,
                        "inputCacheRead": 20,
                        "inputCacheCreation": 3
                    },
                    "usageScope": "turn",
                    "time": 1_784_382_081_647_i64
                })
                .to_string(),
                serde_json::json!({
                    "type": "usage.record",
                    "model": "kimi-code/k3",
                    "usage": {
                        "inputOther": 4,
                        "output": 2,
                        "inputCacheRead": 30,
                        "inputCacheCreation": 1
                    },
                    "usageScope": "turn",
                    "time": 1_784_382_082_647_i64
                })
                .to_string(),
            ]
            .join("\n"),
        )
        .unwrap();
        fs::write(other_agent_dir.join("wire.jsonl"), "{}\n").unwrap();
        fs::write(
            share_dir.join("session_index.jsonl"),
            [
                serde_json::json!({
                    "sessionId": "session-current",
                    "sessionDir": session_dir.display().to_string(),
                    "workDir": project_path
                })
                .to_string(),
                serde_json::json!({
                    "sessionId": "session-other",
                    "sessionDir": other_session_dir.display().to_string(),
                    "workDir": other_project_path
                })
                .to_string(),
            ]
            .join("\n"),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 21);
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 54);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].external_session_id.as_deref(), Some("session-current"));
        assert_eq!(snapshot.sessions[0].last_model.as_deref(), Some("kimi-code/k3"));
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert_eq!(snapshot.sessions[0].total_input_tokens, 14);
        assert_eq!(snapshot.sessions[0].total_output_tokens, 7);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn agy_history_uses_antigravity_conversation_db_only() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let conversations_dir = root.join(".gemini/antigravity-cli/conversations");
        fs::create_dir_all(&conversations_dir).unwrap();
        fs::write(conversations_dir.join("not-a-conversation.json"), "{}").unwrap();

        assert!(agy_session_paths(&project_path, &root).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn aggregates_omp_incremental_usage_cost_and_current_title() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let sessions = root.join(".omp/agent/sessions/-project-a");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(
            sessions.join("session.jsonl"),
            [
                serde_json::json!({
                    "type": "title",
                    "v": 1,
                    "title": "Current OMP title",
                    "updatedAt": "2026-07-19T01:04:00Z",
                    "pad": ""
                }),
                serde_json::json!({
                    "type": "session",
                    "version": 3,
                    "id": "omp-session-1",
                    "timestamp": "2026-07-19T01:00:00Z",
                    "cwd": project_path,
                    "title": "Stale title"
                }),
                serde_json::json!({
                    "type": "message",
                    "timestamp": "2026-07-19T01:00:01Z",
                    "message": { "role": "user" }
                }),
                serde_json::json!({
                    "type": "message",
                    "timestamp": "2026-07-19T01:00:02Z",
                    "message": {
                        "role": "assistant",
                        "provider": "anthropic",
                        "model": "claude-sonnet-4-5",
                        "usage": {
                            "input": 3,
                            "output": 191,
                            "cacheRead": 5,
                            "cacheWrite": 1684,
                            "cost": { "total": 0.009189 }
                        }
                    }
                }),
                serde_json::json!({
                    "type": "message",
                    "timestamp": "2026-07-19T01:00:03Z",
                    "message": {
                        "role": "assistant",
                        "provider": "anthropic",
                        "model": "claude-sonnet-4-5",
                        "usage": {
                            "input": 7,
                            "output": 11,
                            "cacheRead": 13,
                            "cacheWrite": 17,
                            "cost": { "total": 0.01 }
                        }
                    }
                }),
            ]
            .into_iter()
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 212);
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 1_719);
        assert_eq!(snapshot.sessions.len(), 1);
        let session = &snapshot.sessions[0];
        assert_eq!(session.external_session_id.as_deref(), Some("omp-session-1"));
        assert_eq!(session.session_title, "Current OMP title");
        assert_eq!(session.last_tool.as_deref(), Some("omp"));
        assert_eq!(session.last_model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(session.request_count, 1);
        assert_eq!(session.total_input_tokens, 10);
        assert_eq!(session.total_output_tokens, 202);
        assert_eq!(session.cached_input_tokens, 1_719);
        assert_eq!(session.usage_amounts[0].unit, "USD");
        assert!((session.usage_amounts[0].value - 0.019189).abs() < 0.000_000_1);
        assert!(
            snapshot
                .tool_breakdown
                .iter()
                .any(|item| item.key == "omp" && item.total_tokens == 212)
        );
        assert!(
            snapshot
                .model_breakdown
                .iter()
                .any(|item| item.key == "claude-sonnet-4-5" && item.total_tokens == 212)
        );
        let _ = fs::remove_dir_all(root);
    }
}
