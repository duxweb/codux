use super::{
    preview::codex_assistant_preview,
    types::{CodexPayloadFields, CodexTokenInfo, CodexTranscriptRow},
};
use crate::ai_runtime::{
    constants::{CODEX_LIVE_TRANSCRIPT_TAIL_BYTES, CODEX_LIVE_TRANSCRIPT_TAIL_LINES},
    probe::{
        common::parse_iso8601_seconds,
        usage::{UsageTotals, resolve_runtime_usage, usage_totals_from_fields},
    },
    snapshot::{AIPlanItem, AIPlanSnapshot},
    state::normalized_string,
};
use serde_json::Value;
use std::{
    collections::VecDeque,
    fs,
    io::{BufRead, BufReader, Seek},
    path::Path,
};

#[derive(Default)]
pub(super) struct CodexParsedState {
    pub(super) model: Option<String>,
    pub(super) assistant_preview: Option<String>,
    pub(super) input_tokens: Option<i64>,
    pub(super) output_tokens: Option<i64>,
    pub(super) cached_input_tokens: Option<i64>,
    pub(super) total_tokens: Option<i64>,
    pub(super) updated_at: Option<f64>,
    pub(super) started_at: Option<f64>,
    pub(super) completed_at: Option<f64>,
    pub(super) response_state: Option<String>,
    pub(super) was_interrupted: bool,
    pub(super) has_completed_turn: bool,
    pub(super) origin: String,
    pub(super) plan: Option<AIPlanSnapshot>,
    last_user_message_at: Option<f64>,
}

pub(super) fn parse_codex_runtime_state(
    file_path: &Path,
    project_path: Option<&str>,
    fallback_started_at: Option<f64>,
    fallback_updated_at: f64,
) -> Option<CodexParsedState> {
    if let Some(started_at) = fallback_started_at {
        return parse_codex_runtime_state_tail(
            file_path,
            project_path,
            started_at,
            fallback_updated_at,
        )
        .or_else(|| parse_codex_runtime_state_full(file_path, project_path));
    }
    parse_codex_runtime_state_full(file_path, project_path)
}

fn parse_codex_runtime_state_full(
    file_path: &Path,
    project_path: Option<&str>,
) -> Option<CodexParsedState> {
    let file = fs::File::open(file_path).ok()?;
    let reader = BufReader::new(file);
    parse_codex_runtime_lines(
        reader.lines().map_while(Result::ok),
        project_path,
        None,
        None,
    )
}

fn parse_codex_runtime_state_tail(
    file_path: &Path,
    project_path: Option<&str>,
    fallback_started_at: f64,
    fallback_updated_at: f64,
) -> Option<CodexParsedState> {
    let metadata = fs::metadata(file_path).ok()?;
    if metadata.len() <= CODEX_LIVE_TRANSCRIPT_TAIL_BYTES {
        return parse_codex_runtime_state_full(file_path, project_path);
    }
    let mut file = fs::File::open(file_path).ok()?;
    let start = metadata
        .len()
        .saturating_sub(CODEX_LIVE_TRANSCRIPT_TAIL_BYTES);
    file.seek(std::io::SeekFrom::Start(start)).ok()?;
    let mut reader = BufReader::with_capacity(32 * 1024, file);
    if start > 0 {
        let mut partial = String::new();
        reader.read_line(&mut partial).ok()?;
    }
    let lines = read_recent_jsonl_lines(reader, CODEX_LIVE_TRANSCRIPT_TAIL_LINES)?;
    parse_codex_runtime_lines(
        lines.into_iter(),
        project_path,
        Some(fallback_started_at),
        Some(fallback_updated_at),
    )
}

fn read_recent_jsonl_lines<R>(mut reader: R, limit: usize) -> Option<Vec<String>>
where
    R: BufRead,
{
    if limit == 0 {
        return Some(Vec::new());
    }
    let mut lines = VecDeque::with_capacity(limit);
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).ok()?;
        if bytes == 0 {
            break;
        }
        while line.ends_with(['\n', '\r']) {
            line.pop();
        }
        if line.is_empty() {
            continue;
        }
        if lines.len() == limit {
            lines.pop_front();
        }
        lines.push_back(line);
    }
    Some(lines.into_iter().collect())
}

fn parse_codex_runtime_lines<I>(
    lines: I,
    project_path: Option<&str>,
    fallback_started_at: Option<f64>,
    fallback_updated_at: Option<f64>,
) -> Option<CodexParsedState>
where
    I: Iterator<Item = String>,
{
    let mut state = CodexParsedState {
        origin: "unknown".to_string(),
        ..Default::default()
    };
    let mut latest_cumulative_usage: Option<UsageTotals> = None;
    let mut usage_at_turn_start: Option<UsageTotals> = None;

    for line in lines {
        let Ok(row) = serde_json::from_str::<CodexTranscriptRow>(&line) else {
            continue;
        };
        let timestamp = row.timestamp.as_deref().and_then(parse_iso8601_seconds);
        if let Some(timestamp) = timestamp {
            state.updated_at = Some(state.updated_at.unwrap_or(timestamp).max(timestamp));
        }
        let row_type = row.row_type.as_deref();
        let payload = row
            .payload
            .and_then(|payload| serde_json::from_str::<CodexPayloadFields>(payload.get()).ok())
            .unwrap_or_default();

        if let Some(preview) = codex_assistant_preview(row_type, &payload) {
            state.assistant_preview = Some(preview);
        }
        if let Some(plan) = codex_update_plan(row_type, &payload, timestamp.or(state.updated_at)) {
            state.plan = Some(plan);
        }

        if row_type == Some("turn_context") {
            if project_path
                .map(|project| payload.cwd.as_deref() == Some(project))
                .unwrap_or(true)
            {
                if let Some(model) = payload
                    .model
                    .as_deref()
                    .and_then(|value| normalized_string(Some(value)))
                {
                    state.model = Some(model);
                }
            }
            continue;
        }

        update_codex_turn_state(
            &mut state,
            &mut latest_cumulative_usage,
            &mut usage_at_turn_start,
            row_type,
            &payload,
            timestamp,
        );
    }

    finish_codex_state(
        state,
        latest_cumulative_usage,
        usage_at_turn_start,
        fallback_started_at,
        fallback_updated_at,
    )
}

fn codex_update_plan(
    row_type: Option<&str>,
    payload: &CodexPayloadFields<'_>,
    updated_at: Option<f64>,
) -> Option<AIPlanSnapshot> {
    if row_type != Some("response_item")
        || payload.payload_type.as_deref() != Some("function_call")
        || payload.name.as_deref() != Some("update_plan")
    {
        return None;
    }
    let arguments = payload.arguments.as_deref()?;
    let value = serde_json::from_str::<Value>(arguments).ok()?;
    let items = value
        .get("plan")
        .and_then(|value| value.as_array())?
        .iter()
        .filter_map(|item| {
            let text = item
                .get("step")
                .and_then(|value| value.as_str())
                .and_then(|value| normalized_string(Some(value)))?;
            let status = item
                .get("status")
                .and_then(|value| value.as_str())
                .map(normalized_plan_status)
                .unwrap_or_else(|| "pending".to_string());
            Some(AIPlanItem {
                text,
                status,
                priority: None,
            })
        })
        .collect::<Vec<_>>();
    (!items.is_empty()).then_some(AIPlanSnapshot {
        source: "codex".to_string(),
        session_id: "update_plan".to_string(),
        updated_at: updated_at.unwrap_or(0.0),
        items,
    })
}

fn normalized_plan_status(value: &str) -> String {
    match value.trim() {
        "completed" | "complete" | "done" => "completed",
        "in_progress" | "in-progress" | "running" | "active" => "in_progress",
        _ => "pending",
    }
    .to_string()
}

fn update_codex_turn_state(
    state: &mut CodexParsedState,
    latest_cumulative_usage: &mut Option<UsageTotals>,
    usage_at_turn_start: &mut Option<UsageTotals>,
    row_type: Option<&str>,
    payload: &CodexPayloadFields<'_>,
    timestamp: Option<f64>,
) {
    let event_type = payload.payload_type.as_deref();
    let is_user_message = (row_type == Some("event_msg") && event_type == Some("user_message"))
        || (row_type == Some("response_item")
            && event_type == Some("message")
            && payload.role.as_deref() == Some("user"));
    if is_user_message {
        let user_message_at = timestamp.or(state.updated_at);
        if user_message_at > state.last_user_message_at {
            state.last_user_message_at = user_message_at;
            if let Some(user_message_at) = user_message_at {
                if state
                    .completed_at
                    .is_some_and(|completed_at| user_message_at > completed_at)
                {
                    state.started_at = Some(user_message_at);
                    *usage_at_turn_start = latest_cumulative_usage.clone();
                    state.completed_at = None;
                    state.was_interrupted = false;
                    state.has_completed_turn = false;
                }
            }
        }
    }
    let is_final_answer = (row_type == Some("event_msg")
        && event_type == Some("agent_message")
        && payload.phase.as_deref() == Some("final_answer"))
        || (row_type == Some("response_item")
            && event_type == Some("message")
            && payload.phase.as_deref() == Some("final_answer"));
    if is_final_answer {
        let completed_at = timestamp.or(state.updated_at);
        if completed_at >= state.completed_at {
            state.completed_at = completed_at;
            state.was_interrupted = false;
            state.has_completed_turn = true;
        }
        return;
    }

    if row_type != Some("event_msg") {
        return;
    }
    match event_type {
        Some("task_started") => {
            state.started_at = payload.started_at.or(timestamp);
            *usage_at_turn_start = latest_cumulative_usage.clone();
            state.was_interrupted = false;
            state.has_completed_turn = false;
        }
        Some("task_complete") => {
            let completed_at = payload.completed_at.or(timestamp);
            if completed_at >= state.completed_at {
                state.completed_at = completed_at;
                state.was_interrupted = false;
                state.has_completed_turn = true;
            }
        }
        Some("turn_aborted") => {
            let completed_at = payload.completed_at.or(timestamp);
            if completed_at >= state.completed_at {
                state.completed_at = completed_at;
                state.was_interrupted = true;
                state.has_completed_turn = false;
            }
        }
        Some("token_count") => {
            update_codex_usage(state, latest_cumulative_usage, usage_at_turn_start, payload);
        }
        _ => {}
    }
}

fn update_codex_usage(
    state: &mut CodexParsedState,
    latest_cumulative_usage: &mut Option<UsageTotals>,
    usage_at_turn_start: &Option<UsageTotals>,
    payload: &CodexPayloadFields<'_>,
) {
    let info = payload
        .info
        .and_then(|info| serde_json::from_str::<CodexTokenInfo>(info.get()).ok());
    let total_usage = info
        .as_ref()
        .and_then(|info| info.total_token_usage.as_ref())
        .and_then(usage_totals_from_fields);
    let last_usage = info
        .as_ref()
        .and_then(|info| info.last_token_usage.as_ref())
        .and_then(usage_totals_from_fields);
    if let Some(total_usage) = total_usage.clone() {
        *latest_cumulative_usage = Some(total_usage);
    }
    let resolved = resolve_runtime_usage(
        total_usage,
        usage_at_turn_start
            .clone()
            .or_else(|| latest_cumulative_usage.clone()),
        last_usage,
    );
    if let Some(resolved) = resolved {
        state.input_tokens = Some(resolved.input_tokens);
        state.output_tokens = Some(resolved.output_tokens);
        state.cached_input_tokens = Some(resolved.cached_input_tokens);
        state.total_tokens = Some(resolved.total_tokens);
    }
}

fn finish_codex_state(
    mut state: CodexParsedState,
    latest_cumulative_usage: Option<UsageTotals>,
    usage_at_turn_start: Option<UsageTotals>,
    fallback_started_at: Option<f64>,
    fallback_updated_at: Option<f64>,
) -> Option<CodexParsedState> {
    if state.started_at.is_none() {
        state.started_at = fallback_started_at;
    }
    if let Some(fallback_updated_at) = fallback_updated_at {
        state.updated_at = Some(
            state
                .updated_at
                .unwrap_or(fallback_updated_at)
                .max(fallback_updated_at),
        );
    }
    state.response_state = match (state.started_at, state.completed_at) {
        (Some(started_at), Some(completed_at)) if completed_at >= started_at => {
            Some("idle".to_string())
        }
        (None, Some(_)) => Some("idle".to_string()),
        (Some(_), _) => Some("responding".to_string()),
        _ => None,
    };
    let final_usage = match state.response_state.as_deref() {
        Some("idle") => latest_cumulative_usage,
        _ => None,
    };
    if let Some(final_usage) = final_usage {
        state.input_tokens = Some(final_usage.input_tokens);
        state.output_tokens = Some(final_usage.output_tokens);
        state.cached_input_tokens = Some(final_usage.cached_input_tokens);
        state.total_tokens = Some(final_usage.total_tokens);
    }
    if state.response_state.as_deref() == Some("responding") {
        let historical_total = usage_at_turn_start
            .as_ref()
            .map(|usage| usage.total_tokens + usage.cached_input_tokens)
            .unwrap_or(0);
        state.origin = if historical_total > 0 {
            "restored"
        } else {
            "fresh"
        }
        .to_string();
    }
    Some(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_latest_update_plan_from_response_item() {
        let arguments = serde_json::json!({
            "plan": [
                {"step": "Read runtime probes", "status": "completed"},
                {"step": "Wire pet bubble", "status": "in_progress"},
                {"step": "Run tests", "status": "pending"}
            ]
        })
        .to_string();
        let line = serde_json::json!({
            "timestamp": "2026-06-09T10:00:00Z",
            "type": "response_item",
            "payload": {
                "type": "function_call",
                "name": "update_plan",
                "arguments": arguments
            }
        })
        .to_string();

        let state = parse_codex_runtime_lines(vec![line].into_iter(), None, Some(1.0), Some(2.0))
            .expect("codex state");
        let plan = state.plan.expect("plan");

        assert_eq!(plan.source, "codex");
        assert_eq!(plan.items.len(), 3);
        assert_eq!(plan.items[0].text, "Read runtime probes");
        assert_eq!(plan.items[0].status, "completed");
        assert_eq!(plan.items[1].status, "in_progress");
        assert_eq!(plan.items[2].status, "pending");
    }
}
