use crate::ai_runtime::{
    probe::{
        common::{json_i64, parse_iso8601_seconds},
        paths::{agy_session_paths, gemini_session_paths},
        preview::joined_preview_from_values,
    },
    snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest},
    state::{canonical_tool_name, normalized_string},
};
use serde_json::Value;
use std::{fs, path::Path};

pub(crate) fn probe_gemini_runtime(
    request: &AIRuntimeProbeRequest,
) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let tool = canonical_tool_name(&request.tool).unwrap_or_else(|| "gemini".to_string());
    let preferred_id = normalized_string(request.external_session_id.as_deref());
    let session_paths = if tool == "agy" {
        agy_session_paths(&project_path)
    } else {
        gemini_session_paths(&project_path)
    };
    let states = session_paths
        .into_iter()
        .take(16)
        .filter_map(|path| parse_gemini_runtime_state(&path))
        .collect::<Vec<_>>();
    if states.is_empty() {
        return None;
    }

    let mut preferred_match: Option<GeminiParsedState> = None;
    let mut current_launch_match: Option<GeminiParsedState> = None;
    let mut candidate_match: Option<GeminiParsedState> = None;
    for state in states {
        let is_current_launch = request
            .started_at
            .map(|started| state.started_at >= started)
            .unwrap_or(false);
        if preferred_id.as_deref() == Some(state.external_session_id.as_str()) {
            preferred_match = Some(state.clone());
        }
        if is_current_launch {
            if current_launch_match
                .as_ref()
                .map(|existing| state.updated_at > existing.updated_at)
                .unwrap_or(true)
            {
                current_launch_match = Some(state.clone());
            }
            continue;
        }
        if candidate_match
            .as_ref()
            .map(|existing| state.updated_at > existing.updated_at)
            .unwrap_or(true)
        {
            candidate_match = Some(state);
        }
    }

    let authoritative = preferred_id.is_some();
    let mut state = if authoritative {
        preferred_match?
    } else {
        current_launch_match.or(preferred_match).or_else(|| {
            if request.started_at.is_none() {
                candidate_match
            } else {
                None
            }
        })?
    };
    state.origin = if request
        .started_at
        .map(|started| state.started_at >= started)
        .unwrap_or(false)
    {
        "fresh".to_string()
    } else {
        "restored".to_string()
    };

    let has_completed_turn = state.response_state.as_deref() == Some("idle");
    Some(AIRuntimeContextSnapshot {
        tool,
        external_session_id: Some(state.external_session_id),
        transcript_path: None,
        model: state.model,
        assistant_preview: state.assistant_preview,
        input_tokens: state.input_tokens,
        output_tokens: state.output_tokens,
        cached_input_tokens: state.cached_input_tokens,
        total_tokens: state.total_tokens,
        updated_at: state.updated_at.max(request.updated_at),
        started_at: Some(state.started_at),
        completed_at: state.completed_at,
        response_state: state.response_state,
        was_interrupted: false,
        has_completed_turn,
        session_origin: state.origin,
        source: "probe".to_string(),
        plan: None,
    })
}

#[derive(Clone)]
struct GeminiParsedState {
    external_session_id: String,
    model: Option<String>,
    assistant_preview: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    total_tokens: i64,
    started_at: f64,
    updated_at: f64,
    completed_at: Option<f64>,
    response_state: Option<String>,
    origin: String,
}

fn parse_gemini_runtime_state(file_path: &Path) -> Option<GeminiParsedState> {
    let data = fs::read(file_path).ok()?;
    let object: Value = serde_json::from_slice(&data).ok()?;
    let external_session_id = object
        .get("sessionId")
        .and_then(|value| value.as_str())
        .and_then(|value| normalized_string(Some(value)))?;
    let messages = object
        .get("messages")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let started_at = object
        .get("startTime")
        .and_then(|value| value.as_str())
        .and_then(parse_iso8601_seconds)
        .or_else(|| {
            messages
                .iter()
                .filter_map(|message| {
                    message
                        .get("timestamp")
                        .and_then(|value| value.as_str())
                        .and_then(parse_iso8601_seconds)
                })
                .min_by(|left, right| left.total_cmp(right))
        })
        .unwrap_or(0.0);
    let updated_at = object
        .get("lastUpdated")
        .and_then(|value| value.as_str())
        .and_then(parse_iso8601_seconds)
        .or_else(|| {
            messages
                .iter()
                .filter_map(|message| {
                    message
                        .get("timestamp")
                        .and_then(|value| value.as_str())
                        .and_then(parse_iso8601_seconds)
                })
                .max_by(|left, right| left.total_cmp(right))
        })
        .unwrap_or(started_at);

    let mut model = None;
    let mut input_tokens = 0;
    let mut output_tokens = 0;
    let mut cached_input_tokens = 0;
    let mut total_tokens = 0;
    let mut last_relevant_type: Option<String> = None;
    let mut assistant_preview = None;

    for message in messages {
        if let Some(message_type) = message.get("type").and_then(|value| value.as_str()) {
            if message_type != "warning" {
                last_relevant_type = Some(message_type.to_string());
            }
            if message_type != "gemini" {
                continue;
            }
        }
        if let Some(candidate_model) = message
            .get("model")
            .and_then(|value| value.as_str())
            .and_then(|value| normalized_string(Some(value)))
        {
            model = Some(candidate_model);
        }
        if let Some(preview) = joined_preview_from_values(&[
            message.get("content"),
            message.get("text"),
            message.get("message"),
            message.get("parts"),
        ]) {
            assistant_preview = Some(preview);
        }
        let tokens = message.get("tokens").unwrap_or(&Value::Null);
        let cached = json_i64(tokens.get("cached"));
        let thoughts = json_i64(tokens.get("thoughts"));
        let input = (json_i64(tokens.get("input")) - cached).max(0);
        let output = (json_i64(tokens.get("output")) - thoughts).max(0);
        let total = tokens
            .get("total")
            .and_then(|value| value.as_i64())
            .map(|value| (value - cached).max(0))
            .unwrap_or(input + output + thoughts);
        input_tokens += input;
        output_tokens += output;
        cached_input_tokens += cached.max(0);
        total_tokens += total.max(0);
    }

    let response_state = match last_relevant_type.as_deref() {
        Some("user") => Some("responding".to_string()),
        Some("gemini") | Some("error") | Some("info") => Some("idle".to_string()),
        _ if total_tokens > 0 || model.is_some() => Some("idle".to_string()),
        _ => None,
    };
    let completed_at = (response_state.as_deref() == Some("idle")).then_some(updated_at);
    Some(GeminiParsedState {
        external_session_id,
        model,
        assistant_preview,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        total_tokens,
        started_at,
        updated_at,
        completed_at,
        response_state,
        origin: "unknown".to_string(),
    })
}
