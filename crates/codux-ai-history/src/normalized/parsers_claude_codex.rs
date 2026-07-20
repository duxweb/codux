fn parse_claude_history_file_snapshot(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
    starting_at: i64,
    seed: Option<&AIExternalFileCheckpointPayload>,
) -> JSONLParseSnapshot {
    let mut result = ParsedHistory::default();
    let mut last_processed_offset = starting_at.max(0);
    let mut cwd_confirmed = false;
    let mut cwd_denied = false;
    let mut early_line_count = 0;
    let mut assistant_entry_indexes = HashMap::<String, usize>::new();
    let mut assistant_usage_baselines = HashMap::<String, HistoryUsage>::new();
    let mut payload = seed.cloned().unwrap_or_default();
    if starting_at > 0 || payload.session_key.is_some() {
        cwd_confirmed = true;
    }

    let stop_on_invalid_json = starting_at > 0;
    let _ = for_each_jsonl_line(file_path, starting_at, |line, end_offset| {
        if cwd_denied {
            return false;
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            return !stop_on_invalid_json;
        };
        if !cwd_confirmed
            && let Some(cwd) = row.get("cwd").and_then(|value| value.as_str())
        {
            if paths_equivalent(Some(cwd), &project.path) {
                cwd_confirmed = true;
            } else {
                cwd_denied = true;
                return false;
            }
        }
        if !cwd_confirmed {
            last_processed_offset = end_offset;
            early_line_count += 1;
            return early_line_count < 10;
        }
        let Some(session_id) = row
            .get("sessionId")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
        else {
            last_processed_offset = end_offset;
            return true;
        };
        let source_timestamp = row
            .get("timestamp")
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds);
        let row_type = row.get("type").and_then(|value| value.as_str());
        if let Some(kind) = claude_event_kind(&row) {
            let timestamp = source_timestamp
                .or(payload.last_source_timestamp)
                .unwrap_or(0.0);
            result.events.push(HistoryEvent {
                source: "claude".to_string(),
                session_id: session_id.clone(),
                timestamp,
                kind,
            });
            payload.session_key = Some(session_id.clone());
            payload.external_session_id = Some(session_id.clone());
            payload.last_source_timestamp = Some(timestamp);
            payload.session_title = claude_title(&row)
                .or(payload.session_title.clone())
                .or_else(|| Some(project.name.clone()));
        }
        if row_type != Some("assistant") {
            last_processed_offset = end_offset;
            return true;
        }
        let timestamp = source_timestamp
            .or(payload.last_source_timestamp)
            .unwrap_or(0.0);
        payload.last_source_timestamp = Some(timestamp);
        let message = row.get("message").unwrap_or(&Value::Null);
        let message_id = message
            .get("id")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            .or_else(|| {
                row.get("uuid")
                    .and_then(|value| value.as_str())
                    .and_then(normalized_string)
            });
        let usage = message.get("usage").unwrap_or(&Value::Null);
        let current_usage = HistoryUsage {
            input_tokens: json_i64(usage.get("input_tokens")),
            output_tokens: json_i64(usage.get("output_tokens")),
            cached_input_tokens: json_i64(usage.get("cache_read_input_tokens"))
                + json_i64(usage.get("cache_creation_input_tokens")),
            reasoning_output_tokens: 0,
        };
        let message_id = message_id.unwrap_or_else(|| format!("offset:{end_offset}"));
        let baseline = assistant_usage_baselines
            .entry(message_id.clone())
            .or_insert_with(|| {
                if payload.last_claude_message_id.as_deref() == Some(message_id.as_str()) {
                    HistoryUsage {
                        input_tokens: payload.last_claude_input_tokens,
                        output_tokens: payload.last_claude_output_tokens,
                        cached_input_tokens: payload.last_claude_cached_input_tokens,
                        reasoning_output_tokens: 0,
                    }
                } else {
                    HistoryUsage::default()
                }
            });
        let entry_usage = current_usage.saturating_delta(baseline);
        if !message_id.starts_with("offset:") {
            payload.last_claude_message_id = Some(message_id.clone());
            payload.last_claude_input_tokens = current_usage.input_tokens;
            payload.last_claude_output_tokens = current_usage.output_tokens;
            payload.last_claude_cached_input_tokens = current_usage.cached_input_tokens;
        }
        let model = message
            .get("model")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            .or_else(|| Some("unknown".to_string()));
        payload.last_model = model.clone().or(payload.last_model.clone());
        let entry = HistoryEntry {
            source: "claude".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id),
            session_title: claude_title(&row).or_else(|| Some(project.name.clone())),
            timestamp,
            model,
            input_tokens: entry_usage.input_tokens,
            output_tokens: entry_usage.output_tokens,
            cached_input_tokens: entry_usage.cached_input_tokens,
            reasoning_output_tokens: 0,
            usage_amounts: Vec::new(),
        };
        if let Some(index) = assistant_entry_indexes.get(&message_id).copied() {
            result.entries[index] = entry;
        } else {
            assistant_entry_indexes.insert(message_id, result.entries.len());
            result.entries.push(entry);
        }
        last_processed_offset = end_offset;
        true
    });

    result.entries.retain(|entry| {
        entry.total_tokens() > 0 || entry.cached_input_tokens > 0 || !entry.usage_amounts.is_empty()
    });

    JSONLParseSnapshot {
        result: if cwd_denied {
            ParsedHistory::default()
        } else {
            result
        },
        last_processed_offset,
        payload_json: encode_checkpoint_payload(&payload),
    }
}

fn parse_codex_history_file_snapshot(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
    starting_at: i64,
    seed: Option<&AIExternalFileCheckpointPayload>,
) -> JSONLParseSnapshot {
    let mut result = ParsedHistory::default();
    let mut payload = seed.cloned().unwrap_or_default();
    let mut matched_project = payload.session_key.is_some();
    let mut session_id = payload
        .session_key
        .clone()
        .unwrap_or_else(|| file_path.display().to_string());
    let mut session_identity_resolved = payload.session_key.is_some();
    let mut session_title: Option<String> = payload.session_title.clone();
    let mut model: Option<String> = payload.last_model.clone();
    let mut total_usage_by_session = payload.codex_total_usage_by_session.clone();
    let mut active_task_started_at_by_session = payload
        .codex_active_task_started_at_by_session
        .clone();
    let mut canonical_session_meta_seen = payload.codex_canonical_session_meta_seen;
    let mut is_subagent = payload.codex_is_subagent;
    let mut session_created_at = payload.codex_session_created_at;
    let mut subagent_history_start_ordinal = payload.codex_subagent_history_start_ordinal;
    let mut own_history_started = payload.codex_own_history_started || !is_subagent;
    let mut pending_entries = Vec::new();
    let mut pending_events = Vec::new();
    let mut last_processed_offset = starting_at.max(0);
    let stop_on_invalid_json = starting_at > 0;

    let _ = for_each_jsonl_line(file_path, starting_at, |line, end_offset| {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            return !stop_on_invalid_json;
        };
        let Some(timestamp) = row
            .get("timestamp")
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds)
        else {
            last_processed_offset = end_offset;
            return true;
        };
        let row_type = row.get("type").and_then(|value| value.as_str());
        let payload = row.get("payload").unwrap_or(&Value::Null);
        if row_type == Some("session_meta") && !canonical_session_meta_seen {
            canonical_session_meta_seen = true;
            matched_project = payload
                .get("cwd")
                .and_then(|value| value.as_str())
                .map(|cwd| paths_equivalent(Some(cwd), &project.path))
                .unwrap_or(false);
            is_subagent = codex_session_meta_is_subagent(payload);
            session_created_at = payload
                .get("timestamp")
                .and_then(|value| value.as_str())
                .and_then(parse_iso8601_seconds)
                .or(Some(timestamp));
            subagent_history_start_ordinal = payload
                .get("subagent_history_start_ordinal")
                .and_then(Value::as_u64);
            own_history_started = !is_subagent;
            if matched_project && !session_identity_resolved {
                if let Some(id) = payload
                    .get("id")
                    .and_then(|value| value.as_str())
                    .and_then(normalized_string)
                {
                    session_id = id;
                }
                session_identity_resolved = true;
            }
            if matched_project {
                session_title = payload
                    .get("thread_name")
                    .and_then(|value| value.as_str())
                    .and_then(normalized_string)
                    .or_else(|| {
                        payload
                            .get("title")
                            .and_then(|value| value.as_str())
                            .and_then(normalized_string)
                    })
                    .or(session_title.clone());
            }
        }
        if is_subagent && !own_history_started {
            own_history_started = codex_subagent_row_starts_own_history(
                &row,
                payload,
                session_created_at,
                subagent_history_start_ordinal,
            );
            if !own_history_started {
                last_processed_offset = end_offset;
                return true;
            }
        }
        if row_type == Some("turn_context")
            && payload
                .get("cwd")
                .and_then(|value| value.as_str())
                .map(|cwd| paths_equivalent(Some(cwd), &project.path))
                .unwrap_or(false)
        {
            matched_project = true;
            model = payload
                .get("model")
                .and_then(|value| value.as_str())
                .and_then(normalized_string)
                .or(model.clone());
        }
        if !matched_project {
            last_processed_offset = end_offset;
            return true;
        }
        if row_type == Some("response_item") && session_title.is_none() {
            session_title = codex_response_title(payload);
        }
        if let Some(kind) = codex_event_kind(row_type, payload) {
            match kind {
                HistoryEventKind::ActivityStart => {
                    active_task_started_at_by_session.insert(session_id.clone(), timestamp);
                }
                HistoryEventKind::ActivityEnd => {
                    if let Some(started_at) =
                        active_task_started_at_by_session.remove(&session_id)
                    {
                        pending_events.push(HistoryEvent {
                            source: "codex".to_string(),
                            session_id: session_id.clone(),
                            timestamp: started_at,
                            kind: HistoryEventKind::ActivityStart,
                        });
                    }
                    pending_events.push(HistoryEvent {
                        source: "codex".to_string(),
                        session_id: session_id.clone(),
                        timestamp,
                        kind,
                    });
                }
                HistoryEventKind::Request | HistoryEventKind::Activity => {
                    pending_events.push(HistoryEvent {
                        source: "codex".to_string(),
                        session_id: session_id.clone(),
                        timestamp,
                        kind,
                    });
                }
            }
        }
        if is_subagent
            || row_type != Some("event_msg")
            || payload.get("type").and_then(|value| value.as_str()) != Some("token_count")
        {
            last_processed_offset = end_offset;
            return true;
        }
        let info = payload.get("info").unwrap_or(&Value::Null);
        let resolved_model = info
            .get("model")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            .or_else(|| {
                payload
                    .get("model")
                    .and_then(|value| value.as_str())
                    .and_then(normalized_string)
            })
            .or_else(|| model.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let last_usage = codex_history_usage(info.get("last_token_usage"));
        let total_usage = codex_history_usage(info.get("total_token_usage"));
        let usage = if let Some(total_usage) = total_usage {
            let previous = total_usage_by_session
                .get(&session_id)
                .cloned()
                .unwrap_or_default();
            let reset = total_usage.cumulative_total_tokens()
                < previous.cumulative_total_tokens()
                && last_usage.as_ref() == Some(&total_usage);
            let delta = total_usage.saturating_delta(&previous);
            total_usage_by_session.insert(
                session_id.clone(),
                if reset {
                    total_usage.clone()
                } else {
                    total_usage.componentwise_max(&previous)
                },
            );
            if reset {
                last_usage
            } else if delta.total_tokens() <= 0 && delta.cached_input_tokens <= 0 {
                None
            } else {
                Some(delta)
            }
        } else {
            last_usage
        };
        let Some(usage) = usage else {
            last_processed_offset = end_offset;
            return true;
        };
        if usage.total_tokens() <= 0 && usage.cached_input_tokens <= 0 {
            last_processed_offset = end_offset;
            return true;
        }
        pending_entries.push(HistoryEntry {
            source: "codex".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id.clone()),
            session_title: session_title.clone().or_else(|| Some(project.name.clone())),
            timestamp,
            model: Some(resolved_model.clone()),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
            usage_amounts: Vec::new(),
        });
        model = Some(resolved_model);
        last_processed_offset = end_offset;
        true
    });
    if matched_project {
        if let Some(timestamp) = session_created_at {
            result.sessions.push(HistorySessionMetadata {
                source: "codex".to_string(),
                session_id: session_id.clone(),
                external_session_id: Some(session_id.clone()),
                session_title: session_title.clone().or_else(|| Some(project.name.clone())),
                timestamp,
                model: model.clone(),
            });
        }
        result.events.extend(pending_events);
        result.entries.extend(pending_entries);
    }
    payload.session_key = matched_project
        .then(|| session_id.clone())
        .or(payload.session_key.clone());
    payload.external_session_id = matched_project
        .then(|| session_id.clone())
        .or(payload.external_session_id.clone());
    payload.session_title = session_title.or(payload.session_title.clone());
    payload.last_model = model.or(payload.last_model.clone());
    payload.codex_total_usage_by_session = total_usage_by_session;
    payload.codex_active_task_started_at_by_session = active_task_started_at_by_session;
    payload.codex_canonical_session_meta_seen = canonical_session_meta_seen;
    payload.codex_is_subagent = is_subagent;
    payload.codex_session_created_at = session_created_at;
    payload.codex_subagent_history_start_ordinal = subagent_history_start_ordinal;
    payload.codex_own_history_started = own_history_started;

    JSONLParseSnapshot {
        result,
        last_processed_offset,
        payload_json: encode_checkpoint_payload(&payload),
    }
}

fn codex_session_meta_is_subagent(payload: &Value) -> bool {
    payload
        .get("parent_thread_id")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
        || payload.get("thread_source").and_then(Value::as_str) == Some("subagent")
        || payload
            .get("source")
            .and_then(|source| source.get("subagent"))
            .is_some()
}

fn codex_subagent_row_starts_own_history(
    row: &Value,
    payload: &Value,
    session_created_at: Option<f64>,
    history_start_ordinal: Option<u64>,
) -> bool {
    if let Some(history_start_ordinal) = history_start_ordinal {
        return row
            .get("ordinal")
            .and_then(Value::as_u64)
            .is_some_and(|ordinal| ordinal >= history_start_ordinal);
    }
    if row.get("type").and_then(Value::as_str) != Some("event_msg")
        || payload.get("type").and_then(Value::as_str) != Some("task_started")
    {
        return false;
    }
    let Some(session_created_at) = session_created_at else {
        return false;
    };
    payload
        .get("started_at")
        .and_then(|value| value.as_f64().or_else(|| value.as_i64().map(|value| value as f64)))
        .is_some_and(|started_at| started_at >= session_created_at.floor())
        || payload
            .get("turn_id")
            .and_then(Value::as_str)
            .and_then(uuid_v7_unix_seconds)
            .is_some_and(|started_at| started_at >= session_created_at)
}

fn uuid_v7_unix_seconds(value: &str) -> Option<f64> {
    let uuid = Uuid::parse_str(value).ok()?;
    let bytes = uuid.as_bytes();
    if bytes[6] >> 4 != 7 {
        return None;
    }
    let millis = bytes[..6]
        .iter()
        .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte));
    Some(millis as f64 / 1_000.0)
}
