fn parse_kiro_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    let Some(value) = read_small_json_value(file_path) else {
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
    let usage_amounts = kiro_usage_amounts(&value);
    if !timestamps.is_empty()
        || usage.total_tokens() > 0
        || usage.cached_input_tokens > 0
        || !usage_amounts.is_empty()
    {
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
            usage_amounts,
        });
    }
    result
}

fn parse_agy_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    if file_path.extension().and_then(|value| value.to_str()) != Some("db") {
        return ParsedHistory::default();
    }
    parse_agy_database_history_file(project, file_path)
}

fn parse_agy_database_history_file(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    let Some(conversation) = crate::agy_db::parse_agy_conversation_db(file_path) else {
        return ParsedHistory::default();
    };
    if !paths_equivalent(conversation.project_path.as_deref(), &project.path) {
        return ParsedHistory::default();
    }
    let session_id = conversation
        .conversation_id
        .clone()
        .unwrap_or_else(|| deterministic_uuid(&file_path.display().to_string()));
    let mut result = ParsedHistory::default();
    for event in &conversation.events {
        result.events.push(HistoryEvent {
            source: "agy".to_string(),
            session_id: session_id.clone(),
            timestamp: event.timestamp,
            role: match event.role {
                crate::agy_db::AgyConversationRole::User => HistoryRole::User,
                crate::agy_db::AgyConversationRole::Assistant => HistoryRole::Assistant,
            },
        });
    }
    if conversation.has_token_usage() {
        result.entries.push(HistoryEntry {
            source: "agy".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id),
            session_title: conversation.title.or_else(|| Some(project.name.clone())),
            timestamp: conversation
                .last_seen_at
                .or(conversation.last_model_at)
                .or(conversation.last_user_at)
                .unwrap_or_else(now_seconds),
            model: conversation.model,
            input_tokens: conversation.input_tokens,
            output_tokens: conversation.output_tokens,
            cached_input_tokens: conversation.cached_input_tokens,
            reasoning_output_tokens: conversation.reasoning_output_tokens,
            usage_amounts: Vec::new(),
        });
    }
    result
}

fn parse_kimi_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    let Some(state_path) = kimi_state_for_wire(file_path) else {
        return ParsedHistory::default();
    };
    let state = read_small_json_value(&state_path).unwrap_or(Value::Null);
    let Some(project_path) = kimi_project_path(&state) else {
        return ParsedHistory::default();
    };
    if !paths_equivalent(Some(&project_path), &project.path) {
        return ParsedHistory::default();
    }

    let session_id = kimi_session_id(&state, file_path);
    let mut result = ParsedHistory::default();
    let mut session_title = kimi_session_title(&state);
    let mut session_model = kimi_model(&state);
    let mut last_timestamp = None;
    let mut last_usage: Option<HistoryUsage> = None;

    let _ = for_each_jsonl_line(file_path, 0, |line, _| {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            return true;
        };
        let timestamp = kimi_timestamp(&row).unwrap_or_else(now_seconds);
        last_timestamp = Some(timestamp);
        if let Some(role) = kimi_role(&row) {
            result.events.push(HistoryEvent {
                source: "kimi".to_string(),
                session_id: session_id.clone(),
                timestamp,
                role,
            });
            if role == HistoryRole::User && session_title.is_none() {
                session_title = kimi_text(&row).map(|value| truncate_title(&value));
            }
        }
        if let Some(model) = kimi_model(&row) {
            session_model = Some(model);
        }
        if let Some(usage) = kimi_usage(&row) {
            last_usage = Some(usage);
        }
        true
    });

    let usage = last_usage.or_else(|| kimi_usage(&state));
    if let Some(usage) = usage {
        if usage.total_tokens() > 0 || usage.cached_input_tokens > 0 {
            result.entries.push(HistoryEntry {
                source: "kimi".to_string(),
                session_id: session_id.clone(),
                external_session_id: Some(session_id),
                session_title: session_title.or_else(|| Some(project.name.clone())),
                timestamp: last_timestamp
                    .or_else(|| kimi_timestamp(&state))
                    .unwrap_or_else(now_seconds),
                model: session_model,
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cached_input_tokens: usage.cached_input_tokens,
                reasoning_output_tokens: usage.reasoning_output_tokens,
                usage_amounts: Vec::new(),
            });
        }
    }

    result
}

fn kimi_session_id(state: &Value, file_path: &Path) -> String {
    state
        .get("sessionId")
        .or_else(|| state.get("session_id"))
        .or_else(|| state.get("id"))
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            file_path
                .parent()
                .and_then(|path| path.parent())
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .and_then(normalized_string)
        })
        .unwrap_or_else(|| deterministic_uuid(&file_path.display().to_string()))
}

fn kimi_project_path(value: &Value) -> Option<String> {
    value
        .get("cwd")
        .or_else(|| value.get("projectPath"))
        .or_else(|| value.get("project_path"))
        .or_else(|| value.get("workingDirectory"))
        .or_else(|| value.get("working_directory"))
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("project")
                .and_then(|project| {
                    project
                        .get("path")
                        .or_else(|| project.get("root"))
                        .or_else(|| project.get("cwd"))
                })
                .and_then(|value| value.as_str())
                .and_then(normalized_string)
        })
}

fn kimi_model(value: &Value) -> Option<String> {
    value
        .get("model")
        .or_else(|| value.get("modelName"))
        .or_else(|| value.get("model_name"))
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
        .or_else(|| value.get("message").and_then(kimi_model))
}

fn kimi_session_title(value: &Value) -> Option<String> {
    value
        .get("title")
        .or_else(|| value.get("summary"))
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
        .map(|value| truncate_title(&value))
}

fn kimi_timestamp(value: &Value) -> Option<f64> {
    value
        .get("timestamp")
        .or_else(|| value.get("createdAt"))
        .or_else(|| value.get("created_at"))
        .or_else(|| value.get("time"))
        .and_then(value_to_string)
        .and_then(|value| {
            parse_iso8601_seconds(&value).or_else(|| {
                value.parse::<f64>().ok().map(|number| {
                    if number > 10_000_000_000.0 {
                        number / 1000.0
                    } else {
                        number
                    }
                })
            })
        })
}

fn kimi_role(value: &Value) -> Option<HistoryRole> {
    let role = value
        .get("role")
        .or_else(|| value.get("message").and_then(|message| message.get("role")))
        .or_else(|| value.get("type"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if role.contains("user") || role == "human" {
        Some(HistoryRole::User)
    } else if role.contains("assistant") || role.contains("agent") || role.contains("model") {
        Some(HistoryRole::Assistant)
    } else {
        None
    }
}

fn kimi_usage(value: &Value) -> Option<HistoryUsage> {
    let usage = value
        .get("usage")
        .or_else(|| value.get("tokenUsage"))
        .or_else(|| value.get("token_usage"))
        .or_else(|| value.get("tokens"))
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("usage"))
        })?;
    let cached = json_i64(
        usage
            .get("cached_input_tokens")
            .or_else(|| usage.get("cacheReadInputTokens"))
            .or_else(|| usage.get("cachedTokens")),
    );
    let reasoning = json_i64(
        usage
            .get("reasoning_output_tokens")
            .or_else(|| usage.get("reasoningTokens")),
    );
    let mut input = json_i64(
        usage
            .get("input_tokens")
            .or_else(|| usage.get("prompt_tokens"))
            .or_else(|| usage.get("promptTokens"))
            .or_else(|| usage.get("inputTokens")),
    );
    let mut output = json_i64(
        usage
            .get("output_tokens")
            .or_else(|| usage.get("completion_tokens"))
            .or_else(|| usage.get("completionTokens"))
            .or_else(|| usage.get("outputTokens")),
    );
    let total = json_i64(
        usage
            .get("total_tokens")
            .or_else(|| usage.get("totalTokens"))
            .or_else(|| usage.get("total")),
    );
    if input == 0 && output == 0 && total > 0 {
        input = total;
    }
    input = (input - cached).max(0);
    output = (output - reasoning).max(0);
    let resolved = HistoryUsage {
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached.max(0),
        reasoning_output_tokens: reasoning.max(0),
    };
    (resolved.total_tokens() > 0 || resolved.cached_input_tokens > 0).then_some(resolved)
}

fn kimi_text(value: &Value) -> Option<String> {
    value
        .get("content")
        .or_else(|| value.get("text"))
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("content"))
        })
        .and_then(|content| {
            content.as_str().map(str::to_string).or_else(|| {
                content.as_array().map(|items| {
                    items
                        .iter()
                        .filter_map(|item| {
                            item.as_str().map(str::to_string).or_else(|| {
                                item.get("text")
                                    .and_then(|value| value.as_str())
                                    .map(str::to_string)
                            })
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            })
        })
        .and_then(|value| normalized_string(&value))
}

pub fn parse_opencode_history_file(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    parse_opencode_like_history_file("opencode", project, file_path)
}

pub fn parse_mimo_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    parse_opencode_like_history_file("mimo", project, file_path)
}

fn parse_opencode_like_history_file(
    source: &str,
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    if file_path.extension().and_then(|value| value.to_str()) == Some("db") {
        parse_opencode_database(source, project, file_path)
    } else {
        parse_opencode_legacy_message_file(source, project, file_path)
    }
}

fn parse_opencode_database(
    source: &str,
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    parse_opencode_current_database(source, project, file_path)
        .or_else(|| parse_opencode_legacy_database(source, project, file_path))
        .unwrap_or_default()
}

fn parse_opencode_current_database(
    source: &str,
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> Option<ParsedHistory> {
    let mut result = ParsedHistory::default();
    let Ok(conn) = Connection::open(file_path) else {
        return Some(result);
    };
    let Ok(mut statement) = conn.prepare(
        r#"
        SELECT id, title, directory, COALESCE(path, ''), time_created, time_updated,
               tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, model
        FROM session
        WHERE time_archived IS NULL
        ORDER BY time_updated ASC, id ASC;
        "#,
    ) else {
        return None;
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, i64>(9)?,
            row.get::<_, Option<String>>(10)?,
        ))
    }) else {
        return Some(result);
    };

    for row in rows.flatten() {
        let (
            session_id,
            title,
            directory,
            sub_path,
            session_created_at,
            session_updated_at,
            input_tokens,
            output_tokens,
            reasoning_output_tokens,
            cached_input_tokens,
            model,
        ) = row;
        if !opencode_session_matches_project(&directory, &sub_path, &project.path) {
            continue;
        }

        let parsed_messages = parse_opencode_current_messages(source, &conn, &session_id);
        result.events.extend(parsed_messages.events);

        let timestamp = parsed_messages
            .last_seen_at
            .max(opencode_epoch_value_seconds(session_updated_at))
            .max(opencode_epoch_value_seconds(session_created_at));
        let model = parsed_messages
            .last_model
            .or_else(|| model.as_deref().and_then(opencode_model_from_value));
        if input_tokens + output_tokens + reasoning_output_tokens > 0 || cached_input_tokens > 0 {
            result.entries.push(HistoryEntry {
                source: source.to_string(),
                session_id: session_id.clone(),
                external_session_id: Some(session_id),
                session_title: title
                    .as_deref()
                    .and_then(normalized_string)
                    .or_else(|| Some(project.name.clone())),
                timestamp,
                model,
                input_tokens: input_tokens.max(0),
                output_tokens: output_tokens.max(0),
                cached_input_tokens: cached_input_tokens.max(0),
                reasoning_output_tokens: reasoning_output_tokens.max(0),
                usage_amounts: Vec::new(),
            });
        }
    }
    Some(result)
}

#[derive(Default)]
struct OpenCodeCurrentMessages {
    events: Vec<HistoryEvent>,
    last_seen_at: f64,
    last_model: Option<String>,
}

fn parse_opencode_current_messages(
    source: &str,
    conn: &Connection,
    session_id: &str,
) -> OpenCodeCurrentMessages {
    let mut parsed = OpenCodeCurrentMessages::default();
    if parse_opencode_session_message_events(source, conn, session_id, &mut parsed) {
        return parsed;
    }
    parse_opencode_message_events(source, conn, session_id, &mut parsed);
    parsed
}

fn parse_opencode_session_message_events(
    source: &str,
    conn: &Connection,
    session_id: &str,
    parsed: &mut OpenCodeCurrentMessages,
) -> bool {
    let Ok(mut statement) = conn.prepare(
        r#"
        SELECT type, data, time_created, time_updated
        FROM session_message
        WHERE session_id = ?1
        ORDER BY seq ASC;
        "#,
    ) else {
        return false;
    };
    let Ok(rows) = statement.query_map([session_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    }) else {
        return false;
    };

    let mut had_row = false;
    for row in rows.flatten() {
        let (message_type, data, created_at_raw, updated_at_raw) = row;
        had_row = true;
        let data = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
        let created_at = opencode_epoch_value_seconds(created_at_raw);
        let updated_at = opencode_epoch_value_seconds(updated_at_raw);
        parsed.last_seen_at = parsed.last_seen_at.max(created_at).max(updated_at);
        if let Some(model) = opencode_message_model(&data) {
            parsed.last_model = Some(model);
        }
        let role = match message_type.as_str() {
            "user" => Some(HistoryRole::User),
            "assistant" => Some(HistoryRole::Assistant),
            _ => None,
        };
        if let Some(role) = role {
            parsed.events.push(HistoryEvent {
                source: source.to_string(),
                session_id: session_id.to_string(),
                timestamp: created_at,
                role,
            });
        }
    }
    had_row
}

fn parse_opencode_message_events(
    source: &str,
    conn: &Connection,
    session_id: &str,
    parsed: &mut OpenCodeCurrentMessages,
) {
    let Ok(mut statement) = conn.prepare(
        r#"
        SELECT id, data, time_created, time_updated
        FROM message
        WHERE session_id = ?1
        ORDER BY time_created ASC, id ASC;
        "#,
    ) else {
        return;
    };
    let Ok(rows) = statement.query_map([session_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    }) else {
        return;
    };

    for row in rows.flatten() {
        let (message_id, data, created_at_raw, updated_at_raw) = row;
        let data = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
        let created_at = opencode_epoch_value_seconds(created_at_raw);
        let updated_at = opencode_epoch_value_seconds(updated_at_raw);
        parsed.last_seen_at = parsed.last_seen_at.max(created_at).max(updated_at);
        if let Some(model) = opencode_message_model(&data) {
            parsed.last_model = Some(model);
        }
        let role = match data.get("role").and_then(|value| value.as_str()) {
            Some("user") => Some(HistoryRole::User),
            Some("assistant") => Some(HistoryRole::Assistant),
            _ => None,
        };
        if let Some(role) = role {
            parsed.events.push(HistoryEvent {
                source: source.to_string(),
                session_id: session_id.to_string(),
                timestamp: created_at,
                role,
            });
        }
        parse_opencode_part_models(conn, &message_id, parsed);
    }
}

fn parse_opencode_part_models(
    conn: &Connection,
    message_id: &str,
    parsed: &mut OpenCodeCurrentMessages,
) {
    let Ok(mut statement) = conn.prepare(
        r#"
        SELECT data, time_created, time_updated
        FROM part
        WHERE message_id = ?1
        ORDER BY time_created ASC, id ASC;
        "#,
    ) else {
        return;
    };
    let Ok(rows) = statement.query_map([message_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    }) else {
        return;
    };
    for row in rows.flatten() {
        let (data, created_at_raw, updated_at_raw) = row;
        let data = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
        parsed.last_seen_at = parsed
            .last_seen_at
            .max(opencode_epoch_value_seconds(created_at_raw))
            .max(opencode_epoch_value_seconds(updated_at_raw));
        if let Some(model) = opencode_message_model(&data) {
            parsed.last_model = Some(model);
        }
    }
}

fn parse_opencode_legacy_database(
    source: &str,
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> Option<ParsedHistory> {
    let mut result = ParsedHistory::default();
    let Ok(conn) = Connection::open(file_path) else {
        return Some(result);
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
        return None;
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) else {
        return Some(result);
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
            source: source.to_string(),
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
            source: source.to_string(),
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
            usage_amounts: Vec::new(),
        });
    }
    Some(result)
}

fn parse_opencode_legacy_message_file(
    source: &str,
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    let Some(payload) = read_small_json_value(file_path) else {
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
        source: source.to_string(),
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
            source: source.to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id),
            session_title: Some(project.name.clone()),
            timestamp,
            model: Some(model),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
            usage_amounts: Vec::new(),
        });
    }
    result
}

fn opencode_session_matches_project(directory: &str, sub_path: &str, project_path: &str) -> bool {
    if paths_equivalent(Some(directory), project_path) {
        return true;
    }
    let sub_path = sub_path.trim_matches('/');
    if sub_path.is_empty() {
        return false;
    }
    let candidate = Path::new(directory).join(sub_path);
    candidate
        .to_str()
        .map(|path| paths_equivalent(Some(path), project_path))
        .unwrap_or(false)
}

fn opencode_epoch_value_seconds(value: i64) -> f64 {
    let value = value.max(0);
    if value >= 10_000_000_000 {
        value as f64 / 1000.0
    } else {
        value as f64
    }
}

fn opencode_model_from_value(value: &str) -> Option<String> {
    normalized_string(value).and_then(|value| {
        serde_json::from_str::<Value>(&value)
            .ok()
            .and_then(|root| {
                root.get("id")
                    .and_then(|value| value.as_str())
                    .or_else(|| root.get("modelID").and_then(|value| value.as_str()))
                    .or_else(|| root.get("model").and_then(|value| value.as_str()))
                    .and_then(normalized_string)
            })
            .or(Some(value))
    })
}

fn opencode_message_model(data: &Value) -> Option<String> {
    data.get("model")
        .and_then(|value| value.as_str())
        .and_then(opencode_model_from_value)
        .or_else(|| {
            data.get("model")
                .and_then(|value| (!value.is_null()).then(|| value.to_string()))
                .and_then(|value| opencode_model_from_value(&value))
        })
        .or_else(|| {
            data.get("modelID")
                .and_then(|value| value.as_str())
                .and_then(opencode_model_from_value)
        })
        .or_else(|| {
            data.get("model_id")
                .and_then(|value| value.as_str())
                .and_then(opencode_model_from_value)
        })
}
