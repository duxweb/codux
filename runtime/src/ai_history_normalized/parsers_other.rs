fn parse_kiro_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    let Ok(data) = fs::read_to_string(file_path) else {
        return ParsedHistory::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return ParsedHistory::default();
    };
    let Some(session_id) = kiro_session_id(&value, file_path) else {
        return ParsedHistory::default();
    };
    let Some(project_path) = kiro_project_path(&value) else {
        return ParsedHistory::default();
    };
    if !paths_equivalent(Some(&project_path), &project.path) {
        return ParsedHistory::default();
    }

    let model = kiro_model(&value);
    let session_title = kiro_session_title(&value).or_else(|| Some(project.name.clone()));
    let timestamps = kiro_history_timestamps(&value);
    let last_timestamp = timestamps.last().copied().unwrap_or_else(now_seconds);
    let mut result = ParsedHistory::default();
    let mut last_role = None;
    for timestamp in &timestamps {
        let role = if last_role == Some(HistoryRole::User) {
            HistoryRole::Assistant
        } else {
            HistoryRole::User
        };
        last_role = Some(role);
        result.events.push(HistoryEvent {
            source: "kiro".to_string(),
            session_id: session_id.clone(),
            timestamp: *timestamp,
            role,
        });
    }

    let usage = kiro_usage(&value);
    if usage.total_tokens() > 0 || usage.cached_input_tokens > 0 {
        result.entries.push(HistoryEntry {
            source: "kiro".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id.clone()),
            session_title,
            timestamp: last_timestamp,
            model,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        });
    }
    result
}

fn parse_gemini_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    parse_gemini_like_history_file("gemini", project, file_path)
}

fn parse_agy_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    parse_gemini_like_history_file("agy", project, file_path)
}

fn parse_gemini_like_history_file(
    source: &str,
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    let Ok(data) = fs::read(file_path) else {
        return result;
    };
    let Ok(object) = serde_json::from_slice::<Value>(&data) else {
        return result;
    };
    let Some(session_id) = object
        .get("sessionId")
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
    else {
        return result;
    };
    let messages = object
        .get("messages")
        .or_else(|| object.get("history"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let mut session_title = None;
    let mut session_model = object
        .get("model")
        .and_then(|value| value.as_str())
        .and_then(normalized_string);
    for message in messages {
        let timestamp = message
            .get("timestamp")
            .or_else(|| message.get("createTime"))
            .or_else(|| object.get("createTime"))
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds)
            .unwrap_or_else(now_seconds);
        let message_type = message
            .get("type")
            .or_else(|| message.get("role"))
            .and_then(|value| value.as_str());
        let role = if message_type == Some("user") {
            HistoryRole::User
        } else {
            HistoryRole::Assistant
        };
        result.events.push(HistoryEvent {
            source: source.to_string(),
            session_id: session_id.clone(),
            timestamp,
            role,
        });
        if role == HistoryRole::User && session_title.is_none() {
            session_title = parse_gemini_title(message.get("content"));
        }
        let resolved_model = message
            .get("model")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            .or_else(|| session_model.clone())
            .unwrap_or_else(|| "unknown".to_string());
        session_model = Some(resolved_model.clone());
        let usage = message
            .get("tokens")
            .map(gemini_tokens_usage)
            .or_else(|| message.get("usage").map(gemini_usage_metadata))
            .or_else(|| message.get("usageMetadata").map(gemini_usage_metadata))
            .or_else(|| message.get("token_count").map(gemini_usage_metadata));
        let Some(usage) = usage else {
            continue;
        };
        if usage.total_tokens() <= 0 && usage.cached_input_tokens <= 0 {
            continue;
        }
        result.entries.push(HistoryEntry {
            source: source.to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id.clone()),
            session_title: session_title.clone().or_else(|| Some(project.name.clone())),
            timestamp,
            model: Some(resolved_model),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        });
    }
    result
}

fn parse_opencode_history_file(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    if file_path.extension().and_then(|value| value.to_str()) == Some("db") {
        parse_opencode_database(project, file_path)
    } else {
        parse_opencode_legacy_message_file(project, file_path)
    }
}

fn parse_opencode_database(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    let Ok(conn) = Connection::open(file_path) else {
        return result;
    };
    let Ok(mut statement) = conn.prepare(
        r#"
        SELECT s.id, s.title, m.data
        FROM session s
        JOIN message m ON m.session_id = s.id
        WHERE s.time_archived IS NULL
        ORDER BY m.time_created ASC;
        "#,
    ) else {
        return result;
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) else {
        return result;
    };

    for row in rows.flatten() {
        let (session_id, title, data) = row;
        let Ok(payload) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        let Some(root_path) = payload
            .get("path")
            .and_then(|value| value.get("root"))
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        if !paths_equivalent(Some(root_path), &project.path) {
            continue;
        }
        let Some(timestamp) = payload
            .get("time")
            .and_then(|value| value.get("created"))
            .and_then(value_to_string)
            .and_then(|value| parse_opencode_timestamp(&value))
        else {
            continue;
        };
        let role = if payload.get("role").and_then(|value| value.as_str()) == Some("user") {
            HistoryRole::User
        } else {
            HistoryRole::Assistant
        };
        result.events.push(HistoryEvent {
            source: "opencode".to_string(),
            session_id: session_id.clone(),
            timestamp,
            role,
        });
        let model = payload
            .get("modelID")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            .unwrap_or_else(|| "unknown".to_string());
        let usage = opencode_tokens_usage(payload.get("tokens").unwrap_or(&Value::Null));
        if usage.total_tokens() <= 0 && usage.cached_input_tokens <= 0 {
            continue;
        }
        result.entries.push(HistoryEntry {
            source: "opencode".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id.clone()),
            session_title: title
                .as_deref()
                .and_then(normalized_string)
                .or_else(|| Some(project.name.clone())),
            timestamp,
            model: Some(model),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        });
    }
    result
}

fn parse_opencode_legacy_message_file(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    let Ok(data) = fs::read(file_path) else {
        return result;
    };
    let Ok(payload) = serde_json::from_slice::<Value>(&data) else {
        return result;
    };
    let Some(root_path) = payload
        .get("path")
        .and_then(|value| value.get("root"))
        .and_then(|value| value.as_str())
    else {
        return result;
    };
    if !paths_equivalent(Some(root_path), &project.path) {
        return result;
    }
    let Some(timestamp) = payload
        .get("time")
        .and_then(|value| value.get("created"))
        .and_then(value_to_string)
        .and_then(|value| parse_opencode_timestamp(&value))
    else {
        return result;
    };
    let session_id = file_path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|value| value.to_str())
        .and_then(normalized_string)
        .unwrap_or_else(|| file_path.display().to_string());
    let role = if payload.get("role").and_then(|value| value.as_str()) == Some("user") {
        HistoryRole::User
    } else {
        HistoryRole::Assistant
    };
    result.events.push(HistoryEvent {
        source: "opencode".to_string(),
        session_id: session_id.clone(),
        timestamp,
        role,
    });
    let model = payload
        .get("modelID")
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
        .unwrap_or_else(|| "unknown".to_string());
    let usage = opencode_tokens_usage(payload.get("tokens").unwrap_or(&Value::Null));
    if usage.total_tokens() > 0 || usage.cached_input_tokens > 0 {
        result.entries.push(HistoryEntry {
            source: "opencode".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id),
            session_title: Some(project.name.clone()),
            timestamp,
            model: Some(model),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        });
    }
    result
}
