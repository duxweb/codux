fn parse_codewhale_history_file(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    let Ok(data) = fs::read_to_string(file_path) else {
        return ParsedHistory::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return ParsedHistory::default();
    };
    let Some(project_path) = codewhale_project_path(&value) else {
        return ParsedHistory::default();
    };
    if !paths_equivalent(Some(&project_path), &project.path) {
        return ParsedHistory::default();
    }
    let Some(session_id) = codewhale_session_id(&value, file_path) else {
        return ParsedHistory::default();
    };

    let metadata = value.get("metadata").unwrap_or(&Value::Null);
    let model = codewhale_model(&value);
    let session_title = codewhale_session_title(&value).or_else(|| Some(project.name.clone()));
    let created_at = metadata
        .get("created_at")
        .or_else(|| metadata.get("createdAt"))
        .and_then(value_to_string)
        .and_then(|value| parse_iso8601_seconds(&value));
    let updated_at = metadata
        .get("updated_at")
        .or_else(|| metadata.get("updatedAt"))
        .and_then(value_to_string)
        .and_then(|value| parse_iso8601_seconds(&value));

    let mut result = ParsedHistory::default();
    let messages = value
        .get("messages")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    for (index, message) in messages.iter().enumerate() {
        let timestamp = message
            .get("timestamp")
            .or_else(|| message.get("created_at"))
            .or_else(|| message.get("createdAt"))
            .and_then(value_to_string)
            .and_then(|value| parse_iso8601_seconds(&value))
            .or_else(|| {
                if index == 0 {
                    created_at
                } else {
                    updated_at.or(created_at)
                }
            })
            .unwrap_or_else(now_seconds);
        result.events.push(HistoryEvent {
            source: "codewhale".to_string(),
            session_id: session_id.clone(),
            timestamp,
            role: codewhale_role(message),
        });
    }

    let total_tokens = json_i64(
        metadata
            .get("total_tokens")
            .or_else(|| metadata.get("totalTokens"))
            .or_else(|| value.get("total_tokens"))
            .or_else(|| value.get("totalTokens")),
    );
    if total_tokens > 0 {
        result.entries.push(HistoryEntry {
            source: "codewhale".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id),
            session_title,
            timestamp: updated_at.or(created_at).unwrap_or_else(now_seconds),
            model,
            input_tokens: total_tokens,
            output_tokens: 0,
            cached_input_tokens: 0,
            reasoning_output_tokens: 0,
        });
    }
    result
}

fn codewhale_session_id(value: &Value, file_path: &Path) -> Option<String> {
    value
        .get("metadata")
        .and_then(|metadata| metadata.get("id"))
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("id")
                .and_then(|value| value.as_str())
                .and_then(normalized_string)
        })
        .or_else(|| {
            file_path
                .file_stem()
                .and_then(|name| name.to_str())
                .and_then(normalized_string)
        })
}

fn codewhale_project_path(value: &Value) -> Option<String> {
    value
        .get("metadata")
        .and_then(|metadata| metadata.get("workspace"))
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("workspace")
                .and_then(|value| value.as_str())
                .and_then(normalized_string)
        })
        .or_else(|| {
            value
                .get("cwd")
                .and_then(|value| value.as_str())
                .and_then(normalized_string)
        })
}

fn codewhale_model(value: &Value) -> Option<String> {
    value
        .get("metadata")
        .and_then(|metadata| metadata.get("model"))
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("model")
                .and_then(|value| value.as_str())
                .and_then(normalized_string)
        })
}

fn codewhale_session_title(value: &Value) -> Option<String> {
    value
        .get("metadata")
        .and_then(|metadata| metadata.get("title"))
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("title")
                .and_then(|value| value.as_str())
                .and_then(normalized_string)
        })
        .map(|value| truncate_title(&value))
}

fn codewhale_role(message: &Value) -> HistoryRole {
    if message.get("role").and_then(|value| value.as_str()) == Some("user") {
        HistoryRole::User
    } else {
        HistoryRole::Assistant
    }
}
