use crate::{
    ai_runtime::{
        probe::{
            common::{json_i64, parse_iso8601_seconds},
            paths::paths_equivalent,
            preview::joined_preview_from_values,
        },
        snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest},
        state::normalized_string,
    },
    runtime_paths::home_dir,
};
use serde_json::Value;

pub(crate) fn probe_opencode_runtime(
    request: &AIRuntimeProbeRequest,
) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let external_session_id = normalized_string(request.external_session_id.as_deref())?;
    let database_path = home_dir()
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    if !database_path.exists() {
        return None;
    }
    let conn = rusqlite::Connection::open(&database_path).ok()?;
    let mut statement = conn
        .prepare(
            r#"
            SELECT m.data, m.time_created, s.time_updated, COALESCE(s.directory, '')
            FROM session s
            LEFT JOIN message m ON m.session_id = s.id
            WHERE s.id = ?1
              AND s.time_archived IS NULL
            ORDER BY m.time_created DESC;
            "#,
        )
        .ok()?;
    let rows = statement
        .query_map([external_session_id.as_str()], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .ok()?;

    let mut had_row = false;
    let mut latest_model = None;
    let mut input_tokens = 0;
    let mut output_tokens = 0;
    let mut cached_input_tokens = 0;
    let mut total_tokens = 0;
    let mut updated_at = 0.0f64;
    let mut last_user_at = 0.0f64;
    let mut last_completion_at = 0.0f64;
    let mut assistant_preview = None;

    for row in rows.flatten() {
        let (data, message_created_at, session_updated_at, session_directory) = row;
        let payload = data
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok())
            .unwrap_or(Value::Null);
        let root_path = payload
            .get("path")
            .and_then(|value| value.get("root"))
            .and_then(|value| value.as_str())
            .or(session_directory.as_deref());
        if !paths_equivalent(root_path, &project_path) {
            continue;
        }
        had_row = true;
        if latest_model.is_none() {
            latest_model = payload
                .get("modelID")
                .and_then(|value| value.as_str())
                .and_then(|value| normalized_string(Some(value)));
        }
        let tokens = payload.get("tokens").unwrap_or(&Value::Null);
        let cache = tokens.get("cache").unwrap_or(&Value::Null);
        let input = json_i64(tokens.get("input"));
        let output = json_i64(tokens.get("output"));
        let cache_read = json_i64(cache.get("read"));
        let reasoning = json_i64(tokens.get("reasoning"));
        input_tokens += input;
        output_tokens += output;
        cached_input_tokens += cache_read;
        total_tokens += input + output + reasoning;

        let created_at = payload
            .get("time")
            .and_then(|value| value.get("created"))
            .and_then(opencode_value_timestamp)
            .or_else(|| message_created_at.map(|value| value / 1000.0))
            .unwrap_or(0.0);
        let completed_at = payload
            .get("time")
            .and_then(|value| value.get("completed"))
            .and_then(opencode_value_timestamp);
        let role = payload
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let finish_reason = payload
            .get("finish")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if role == "user" {
            last_user_at = last_user_at.max(created_at);
        } else if role == "assistant" {
            if assistant_preview.is_none() {
                assistant_preview = joined_preview_from_values(&[
                    payload.get("content"),
                    payload.get("text"),
                    payload.get("message"),
                    payload.get("parts"),
                ]);
            }
            if is_opencode_final_assistant_finish(finish_reason, completed_at) {
                last_completion_at = last_completion_at.max(completed_at.unwrap_or(created_at));
            }
        }
        updated_at = updated_at.max(created_at);
        updated_at = updated_at.max(completed_at.unwrap_or(0.0));
        updated_at = updated_at.max(session_updated_at.unwrap_or(0.0) / 1000.0);
    }

    if !had_row {
        return None;
    }
    let response_state = if last_user_at > 0.0 {
        if last_user_at > last_completion_at {
            Some("responding".to_string())
        } else {
            Some("idle".to_string())
        }
    } else if total_tokens > 0 {
        Some("idle".to_string())
    } else {
        None
    };
    let has_completed_turn = last_completion_at > 0.0 && last_completion_at >= last_user_at;
    Some(AIRuntimeContextSnapshot {
        tool: "opencode".to_string(),
        external_session_id: Some(external_session_id),
        transcript_path: Some(database_path.display().to_string()),
        model: latest_model,
        assistant_preview,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        total_tokens,
        updated_at: updated_at.max(request.updated_at),
        started_at: (last_user_at > 0.0).then_some(last_user_at),
        completed_at: has_completed_turn.then_some(last_completion_at),
        response_state,
        was_interrupted: false,
        has_completed_turn,
        session_origin: if total_tokens > 0 {
            "restored"
        } else {
            "fresh"
        }
        .to_string(),
        source: "probe".to_string(),
    })
}

fn is_opencode_final_assistant_finish(value: &str, completed_at: Option<f64>) -> bool {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        return completed_at.is_some();
    }
    normalized != "tool-calls"
}

fn opencode_value_timestamp(value: &Value) -> Option<f64> {
    let raw = value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .or_else(|| value.as_f64().map(|value| value.to_string()))?;
    if let Ok(milliseconds) = raw.parse::<f64>() {
        return Some(milliseconds / 1000.0);
    }
    parse_iso8601_seconds(&raw)
}
