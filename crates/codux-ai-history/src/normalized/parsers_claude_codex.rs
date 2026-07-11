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
    let mut seen_assistant_ids = HashMap::<String, bool>::new();
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
        let timestamp = row
            .get("timestamp")
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds)
            .unwrap_or_else(now_seconds);
        let row_type = row.get("type").and_then(|value| value.as_str());
        if let Some(role) = claude_role(row_type) {
            result.events.push(HistoryEvent {
                source: "claude".to_string(),
                session_id: session_id.clone(),
                timestamp,
                role,
            });
            payload.session_key = Some(session_id.clone());
            payload.external_session_id = Some(session_id.clone());
            payload.session_title = claude_title(&row)
                .or(payload.session_title.clone())
                .or_else(|| Some(project.name.clone()));
        }
        if row_type != Some("assistant") {
            last_processed_offset = end_offset;
            return true;
        }
        if let Some(uuid) = row
            .get("uuid")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            && seen_assistant_ids.insert(uuid, true).is_some()
        {
            last_processed_offset = end_offset;
            return true;
        }
        let message = row.get("message").unwrap_or(&Value::Null);
        let usage = message.get("usage").unwrap_or(&Value::Null);
        let input_tokens = json_i64(usage.get("input_tokens"));
        let output_tokens = json_i64(usage.get("output_tokens"));
        // Claude reports cache writes (cache_creation_input_tokens) and cache
        // reads (cache_read_input_tokens) as separate categories from
        // input_tokens. Both are cached input -- count both, matching the live
        // runtime probe (ai_runtime/probe/claude.rs). Dropping cache-creation
        // here undercounts Claude usage.
        let cached_input_tokens = json_i64(usage.get("cache_read_input_tokens"))
            + json_i64(usage.get("cache_creation_input_tokens"));
        let total_tokens = input_tokens + output_tokens + cached_input_tokens;
        if total_tokens <= 0 {
            last_processed_offset = end_offset;
            return true;
        }
        let model = message
            .get("model")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            .or_else(|| Some("unknown".to_string()));
        payload.last_model = model.clone().or(payload.last_model.clone());
        result.entries.push(HistoryEntry {
            source: "claude".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id),
            session_title: claude_title(&row).or_else(|| Some(project.name.clone())),
            timestamp,
            model,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            reasoning_output_tokens: 0,
            usage_amounts: Vec::new(),
        });
        last_processed_offset = end_offset;
        true
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
    let mut session_title: Option<String> = payload.session_title.clone();
    let mut model: Option<String> = payload.last_model.clone();
    let mut total_by_model = payload.model_total_tokens_by_name.clone();
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
        if row_type == Some("session_meta")
            && payload
                .get("cwd")
                .and_then(|value| value.as_str())
                .map(|cwd| paths_equivalent(Some(cwd), &project.path))
                .unwrap_or(false)
        {
            matched_project = true;
            if let Some(id) = payload
                .get("id")
                .and_then(|value| value.as_str())
                .and_then(normalized_string)
            {
                session_id = id;
            }
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
        pending_events.push(HistoryEvent {
            source: "codex".to_string(),
            session_id: session_id.clone(),
            timestamp,
            role: codex_role(row_type),
        });
        if row_type != Some("event_msg")
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
            // codex's total_token_usage is a session-global cumulative counter
            // shared across models, so the baseline must be tracked per session,
            // NOT per model. Keying it by model meant a mid-session model switch
            // saw a 0 baseline and re-attributed the entire accumulated total as
            // one delta (the ~100M single-request inflation).
            let previous = total_by_model.get(&session_id).copied().unwrap_or_else(|| {
                // Migrate a pre-fix per-model checkpoint: the cumulative is
                // monotonic, so the highest recorded value is the last
                // cumulative seen -- using it avoids a one-time re-inflation on
                // the first parse after this fix.
                total_by_model.values().copied().max().unwrap_or(0)
            });
            let current = total_usage.total_tokens();
            let delta = (current - previous).max(0);
            total_by_model.clear();
            total_by_model.insert(session_id.clone(), previous.max(current));
            if delta <= 0 {
                None
            } else if last_usage.as_ref().map(|usage| usage.total_tokens()) == Some(delta) {
                last_usage
            } else {
                Some(total_usage.delta(delta))
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
    payload.model_total_tokens_by_name = total_by_model;

    JSONLParseSnapshot {
        result,
        last_processed_offset,
        payload_json: encode_checkpoint_payload(&payload),
    }
}
