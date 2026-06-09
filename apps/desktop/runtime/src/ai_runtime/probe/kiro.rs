use crate::ai_runtime::{
    probe::paths::{find_kiro_session_path, paths_equivalent},
    snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest},
    state::normalized_string,
};
use serde_json::Value;
use std::{fs, path::Path};

pub(crate) fn probe_kiro_runtime(
    request: &AIRuntimeProbeRequest,
) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let external_session_id = normalized_string(request.external_session_id.as_deref())?;
    let file_path = find_kiro_session_path(&project_path, &external_session_id)?;
    let parsed = parse_kiro_runtime_state(&file_path, Some(&project_path))?;
    Some(AIRuntimeContextSnapshot {
        tool: "kiro".to_string(),
        external_session_id: Some(external_session_id),
        transcript_path: Some(file_path.display().to_string()),
        model: parsed.model,
        assistant_preview: parsed.assistant_preview,
        input_tokens: parsed.input_tokens,
        output_tokens: parsed.output_tokens,
        cached_input_tokens: parsed.cached_input_tokens,
        total_tokens: parsed.total_tokens,
        updated_at: parsed.updated_at.unwrap_or(request.updated_at),
        started_at: parsed.started_at,
        completed_at: parsed.completed_at,
        response_state: parsed.response_state,
        was_interrupted: parsed.was_interrupted,
        has_completed_turn: parsed.has_completed_turn,
        session_origin: parsed.origin,
        source: "probe".to_string(),
        plan: None,
    })
}

#[derive(Debug, Clone)]
struct KiroParsedState {
    model: Option<String>,
    assistant_preview: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    total_tokens: i64,
    updated_at: Option<f64>,
    started_at: Option<f64>,
    completed_at: Option<f64>,
    response_state: Option<String>,
    was_interrupted: bool,
    has_completed_turn: bool,
    origin: String,
}

fn parse_kiro_runtime_state(
    file_path: &Path,
    project_path: Option<&str>,
) -> Option<KiroParsedState> {
    let data = fs::read_to_string(file_path).ok()?;
    let value = serde_json::from_str::<Value>(&data).ok()?;
    let mut model = normalized_string(
        value
            .get("model")
            .and_then(|value| value.as_str())
            .or_else(|| value.get("modelId").and_then(|value| value.as_str())),
    );
    let mut assistant_preview = normalized_string(
        value
            .get("assistantPreview")
            .and_then(|value| value.as_str())
            .or_else(|| value.get("preview").and_then(|value| value.as_str())),
    );
    let input_tokens = json_number(
        value
            .get("inputTokens")
            .or_else(|| value.get("input_tokens")),
    );
    let output_tokens = json_number(
        value
            .get("outputTokens")
            .or_else(|| value.get("output_tokens")),
    );
    let cached_input_tokens = json_number(
        value
            .get("cachedInputTokens")
            .or_else(|| value.get("cached_input_tokens")),
    );
    let total_tokens = json_number(
        value
            .get("totalTokens")
            .or_else(|| value.get("total_tokens")),
    );
    let updated_at = value.get("updatedAt").and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_i64().map(|value| value as f64))
    });
    let started_at = value.get("startedAt").and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_i64().map(|value| value as f64))
    });
    let completed_at = value.get("completedAt").and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_i64().map(|value| value as f64))
    });
    let response_state = normalized_string(
        value
            .get("responseState")
            .and_then(|value| value.as_str())
            .or_else(|| value.get("state").and_then(|value| value.as_str())),
    );
    let was_interrupted = value
        .get("wasInterrupted")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let has_completed_turn = value
        .get("hasCompletedTurn")
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| response_state.as_deref() == Some("idle"));
    if model.is_none() {
        model = value
            .get("session")
            .and_then(|value| value.get("model"))
            .and_then(|value| value.as_str())
            .and_then(|value| normalized_string(Some(value)));
    }
    if assistant_preview.is_none() {
        assistant_preview = value
            .get("messages")
            .and_then(|value| value.as_array())
            .and_then(|messages| messages.iter().rev().find_map(kiro_message_preview));
    }
    let origin = if project_path
        .and_then(|project| {
            value
                .get("projectPath")
                .and_then(|value| value.as_str())
                .or_else(|| value.get("cwd").and_then(|value| value.as_str()))
                .map(|current| paths_equivalent(Some(current), project))
        })
        .unwrap_or(false)
    {
        "fresh".to_string()
    } else {
        "restored".to_string()
    };

    Some(KiroParsedState {
        model,
        assistant_preview,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        total_tokens,
        updated_at,
        started_at,
        completed_at,
        response_state,
        was_interrupted,
        has_completed_turn,
        origin,
    })
}

fn kiro_message_preview(value: &Value) -> Option<String> {
    let role = value
        .get("role")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if role != "assistant" {
        return None;
    }
    if let Some(text) = value.get("content").and_then(|value| value.as_str()) {
        return normalized_string(Some(text));
    }
    value
        .get("parts")
        .and_then(|value| value.as_array())
        .and_then(|parts| {
            parts.iter().find_map(|part| {
                part.get("text")
                    .and_then(|value| value.as_str())
                    .and_then(|text| normalized_string(Some(text)))
            })
        })
}

fn json_number(value: Option<&Value>) -> i64 {
    value.and_then(|value| value.as_i64()).unwrap_or(0)
}
