fn claude_project_log_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    let directory_name = project_path.replace('/', "-").replace('.', "-");
    directory_files(
        &home.join(".claude").join("projects").join(directory_name),
        "jsonl",
    )
}

fn gemini_session_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    gemini_session_paths_for_roots(project_path, &[home.join(".gemini")])
}

fn agy_session_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    agy_session_paths_for_roots(project_path, &[home.join(".gemini").join("antigravity-cli")])
}

fn gemini_session_paths_for_roots(project_path: &str, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root_dir in roots {
        let temp_dir = root_dir.join("tmp");
        let projects_path = root_dir.join("projects.json");
        if let Ok(data) = fs::read(projects_path) {
            if let Ok(root) = serde_json::from_slice::<Value>(&data) {
                if let Some(projects) = root.get("projects").and_then(|value| value.as_object()) {
                    for (stored_path, value) in projects {
                        if paths_equivalent(Some(stored_path), project_path) {
                            if let Some(directory) = value.as_str().and_then(normalized_string) {
                                dirs.push(temp_dir.join(directory));
                            }
                        }
                    }
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&temp_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let marker = path.join(".project_root");
                if let Ok(value) = fs::read_to_string(marker) {
                    if paths_equivalent(Some(value.trim()), project_path) {
                        dirs.push(path);
                    }
                }
            }
        }
    }
    let mut files = Vec::new();
    for dir in dirs {
        files.extend(directory_files(&dir.join("chats"), "json"));
    }
    files.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("session-"))
            .unwrap_or(false)
    });
    files.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    files
}

fn agy_session_paths_for_roots(project_path: &str, roots: &[PathBuf]) -> Vec<PathBuf> {
    gemini_session_paths_for_roots(project_path, roots)
}

fn codex_session_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    let database_path = home.join(".codex").join("state_5.sqlite");
    let from_database = codex_session_paths_from_database(project_path, &database_path);
    if !from_database.is_empty() {
        return from_database;
    }
    recursive_files(&home.join(".codex").join("sessions"), "jsonl")
        .into_iter()
        .filter(|path| codex_rollout_file_belongs_to_project(path, project_path))
        .collect()
}

fn codex_session_paths_from_database(project_path: &str, database_path: &Path) -> Vec<PathBuf> {
    if !database_path.exists() {
        return Vec::new();
    }
    let Ok(conn) = Connection::open(database_path) else {
        return Vec::new();
    };
    let Ok(mut statement) = conn.prepare(
        r#"
        SELECT rollout_path, cwd
        FROM threads
        WHERE rollout_path IS NOT NULL
        ORDER BY updated_at DESC;
        "#,
    ) else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    }) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    let mut seen = HashMap::<String, bool>::new();
    for row in rows.flatten() {
        let (rollout_path, cwd) = row;
        if !paths_equivalent(cwd.as_deref(), project_path) {
            continue;
        }
        if rollout_path.trim().is_empty() || seen.insert(rollout_path.clone(), true).is_some() {
            continue;
        }
        let file_path = PathBuf::from(rollout_path);
        if file_path.exists() {
            files.push(file_path);
        }
    }
    files
}

fn codex_rollout_file_belongs_to_project(file_path: &Path, project_path: &str) -> bool {
    let mut line_count = 0usize;
    let mut matches_project = false;
    let _ = for_each_jsonl_line(file_path, 0, |line, _| {
        line_count += 1;
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            return line_count < 20;
        };
        let row_type = row.get("type").and_then(|value| value.as_str());
        let payload = row.get("payload").unwrap_or(&Value::Null);
        if matches!(row_type, Some("session_meta") | Some("turn_context")) {
            if let Some(cwd) = payload.get("cwd").and_then(|value| value.as_str()) {
                matches_project = paths_equivalent(Some(cwd), project_path);
                return false;
            }
        }
        line_count < 20
    });
    matches_project
}

fn opencode_history_source_paths(home: &Path) -> Vec<PathBuf> {
    let database_path = home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    if database_path.exists() {
        return vec![database_path];
    }
    opencode_legacy_message_paths(home)
}

fn kiro_session_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    let sessions_dir = home.join(".kiro").join("sessions").join("cli");
    let files = directory_files(&sessions_dir, "json");
    let mut matched = files
        .into_iter()
        .filter(|path| kiro_file_belongs_to_project(path, project_path))
        .collect::<Vec<_>>();
    matched.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    matched
}

fn codewhale_session_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    let sessions_dir = home.join(".codewhale").join("sessions");
    let mut matched = directory_files(&sessions_dir, "json")
        .into_iter()
        .filter(|path| codewhale_file_belongs_to_project(path, project_path))
        .collect::<Vec<_>>();
    matched.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    matched
}

fn kimi_session_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    let sessions_dir = home.join(".kimi-code").join("sessions");
    let mut matched = recursive_files(&sessions_dir, "jsonl")
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == "wire.jsonl")
                .unwrap_or(false)
        })
        .filter(|path| kimi_wire_belongs_to_project(path, project_path))
        .collect::<Vec<_>>();
    matched.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    matched
}

fn kimi_wire_belongs_to_project(file_path: &Path, project_path: &str) -> bool {
    kimi_state_for_wire(file_path)
        .and_then(|state_path| fs::read_to_string(state_path).ok())
        .and_then(|data| serde_json::from_str::<Value>(&data).ok())
        .and_then(|value| kimi_project_path(&value))
        .map(|path| paths_equivalent(Some(&path), project_path))
        .unwrap_or(false)
}

fn kimi_state_for_wire(file_path: &Path) -> Option<PathBuf> {
    file_path
        .parent()?
        .parent()?
        .parent()
        .map(|path| path.join("state.json"))
}

fn codewhale_file_belongs_to_project(file_path: &Path, project_path: &str) -> bool {
    let Ok(data) = fs::read_to_string(file_path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return false;
    };
    codewhale_project_path(&value)
        .map(|path| paths_equivalent(Some(&path), project_path))
        .unwrap_or(false)
}

fn kiro_file_belongs_to_project(file_path: &Path, project_path: &str) -> bool {
    let Ok(data) = fs::read_to_string(file_path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return false;
    };
    kiro_project_path(&value)
        .map(|path| paths_equivalent(Some(&path), project_path))
        .unwrap_or(false)
}

fn kiro_session_id(value: &Value, file_path: &Path) -> Option<String> {
    value
        .get("sessionId")
        .and_then(|v| v.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("session")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
        .or_else(|| {
            file_path
                .file_stem()
                .and_then(|name| name.to_str())
                .and_then(normalized_string)
        })
}

fn kiro_project_path(value: &Value) -> Option<String> {
    value
        .get("projectPath")
        .and_then(|v| v.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("project")
                .and_then(|v| v.get("path"))
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
        .or_else(|| {
            value
                .get("cwd")
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
        .or_else(|| {
            value
                .get("workingDirectory")
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
}

fn kiro_model(value: &Value) -> Option<String> {
    value
        .get("model")
        .and_then(|v| v.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("session")
                .and_then(|v| v.get("model"))
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
}

fn kiro_session_title(value: &Value) -> Option<String> {
    value
        .get("title")
        .and_then(|v| v.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("session")
                .and_then(|v| v.get("title"))
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
}

fn kiro_history_timestamps(value: &Value) -> Vec<f64> {
    let mut timestamps = value
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| {
                    message
                        .get("timestamp")
                        .or_else(|| message.get("createdAt"))
                        .and_then(value_to_string)
                        .and_then(|value| parse_iso8601_seconds(&value))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if timestamps.is_empty() {
        if let Some(value) = value
            .get("updatedAt")
            .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|value| value as f64)))
        {
            timestamps.push(value);
        }
    }
    timestamps.sort_by(|left, right| left.total_cmp(right));
    timestamps
}

fn kiro_usage(value: &Value) -> HistoryUsage {
    let usage = value.get("usage").unwrap_or(&Value::Null);
    HistoryUsage {
        input_tokens: json_i64(
            usage
                .get("input")
                .or_else(|| usage.get("input_tokens"))
                .or_else(|| value.get("inputTokens")),
        ),
        output_tokens: json_i64(
            usage
                .get("output")
                .or_else(|| usage.get("output_tokens"))
                .or_else(|| value.get("outputTokens")),
        ),
        cached_input_tokens: json_i64(
            usage
                .get("cache")
                .and_then(|cache| cache.get("read"))
                .or_else(|| usage.get("cached_input_tokens"))
                .or_else(|| value.get("cachedInputTokens")),
        ),
        reasoning_output_tokens: json_i64(
            usage
                .get("reasoning")
                .or_else(|| value.get("reasoningTokens")),
        ),
    }
}

fn opencode_legacy_message_paths(home: &Path) -> Vec<PathBuf> {
    let messages_dir = home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("storage")
        .join("message");
    let Ok(entries) = fs::read_dir(messages_dir) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir()
            || !dir
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("ses_"))
                .unwrap_or(false)
        {
            continue;
        }
        files.extend(directory_files(&dir, "json"));
    }
    files.sort();
    files
}
