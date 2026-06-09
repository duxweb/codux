#[cfg(test)]
mod tests {
    use super::*;

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
    fn matches_windows_extended_paths_without_matching_project_children() {
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
        assert_eq!(
            snapshot
                .tool_breakdown
                .iter()
                .any(|item| item.key == "kiro"),
            true
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
        assert_eq!(
            snapshot.sessions[0].last_tool.as_deref(),
            Some("codewhale")
        );
        assert_eq!(
            snapshot.sessions[0].last_model.as_deref(),
            Some("deepseek-v4-pro")
        );
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert!(snapshot
            .tool_breakdown
            .iter()
            .any(|item| item.key == "codewhale" && item.total_tokens == 123));
        assert!(snapshot
            .model_breakdown
            .iter()
            .any(|item| item.key == "deepseek-v4-pro" && item.total_tokens == 123));
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
        assert!(snapshot
            .tool_breakdown
            .iter()
            .any(|item| item.key == "kimi" && item.total_tokens == 55));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn separates_gemini_and_agy_history_roots() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let gemini_chat_dir = root.join(".gemini/tmp/gemini-project/chats");
        let agy_chat_dir = root.join(".gemini/antigravity-cli/tmp/agy-project/chats");
        fs::create_dir_all(&gemini_chat_dir).unwrap();
        fs::create_dir_all(&agy_chat_dir).unwrap();
        fs::write(
            root.join(".gemini/projects.json"),
            serde_json::json!({ "projects": { project_path.clone(): "gemini-project" } })
                .to_string(),
        )
        .unwrap();
        fs::write(
            root.join(".gemini/antigravity-cli/projects.json"),
            serde_json::json!({ "projects": { project_path.clone(): "agy-project" } }).to_string(),
        )
        .unwrap();
        let gemini_file = gemini_chat_dir.join("session-gemini.json");
        let agy_file = agy_chat_dir.join("session-agy.json");
        fs::write(&gemini_file, "{}").unwrap();
        fs::write(&agy_file, "{}").unwrap();

        assert_eq!(gemini_session_paths(&project_path, &root), vec![gemini_file]);
        assert_eq!(agy_session_paths(&project_path, &root), vec![agy_file]);
        let _ = fs::remove_dir_all(root);
    }
}
